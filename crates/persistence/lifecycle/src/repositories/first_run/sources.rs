// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Register/list/find/remove a `registered_sources` row.

use domain_core::first_run::{
    BatchItem, BatchStatus, ItemStatus, OrganizationState, RegisterSourceBatchRequest,
    RegisterSourceBatchResponse, RegisterSourceRequest, RegisterSourceResponse, SourceKind,
};
use domain_core::ids::Timestamp;
use sqlx::SqlitePool;
use uuid::Uuid;

use persistence_core::{DbError, DbResult};

use super::{
    organization_state_to_str, resolve_created_via, scan_depth_to_str, source_kind_to_str,
    str_to_organization_state, str_to_source_kind,
};

/// Find all registered sources that share the given path (any kind).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_sources_by_path(
    pool: &SqlitePool,
    path: &str,
) -> DbResult<Vec<RegisterSourceResponse>> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, kind, path, created_at, organization_state \
         FROM registered_sources WHERE path = ?",
    )
    .bind(path)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, kind, path, created_at, org_state)| RegisterSourceResponse {
            source_id: id,
            kind: str_to_source_kind(&kind),
            path,
            created_at,
            organization_state: str_to_organization_state(&org_state),
        })
        .collect())
}

/// Register a single source directory.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violation (e.g. duplicate
/// kind+path).
pub async fn register_source(
    pool: &SqlitePool,
    req: &RegisterSourceRequest,
) -> DbResult<RegisterSourceResponse> {
    let id = Uuid::new_v4().to_string();
    let kind_str = source_kind_to_str(req.kind);
    let scan_depth_str = scan_depth_to_str(req.scan_depth);
    let created_at = Timestamp::now_iso();
    let created_via = resolve_created_via(pool).await?;

    // Enforce inbox⇒unorganized invariant on write (spec 041, T029). Inbox
    // sources are always relocated on confirm, never catalogued in place.
    let effective_org_state = if matches!(req.kind, SourceKind::Inbox) {
        OrganizationState::Unorganized
    } else {
        req.organization_state
    };
    let org_state_str = organization_state_to_str(effective_org_state);
    sqlx::query(
        "INSERT INTO registered_sources \
         (id, kind, path, kind_subtype, scan_depth, created_at, created_via, organization_state) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(kind_str)
    .bind(&req.path)
    .bind(&req.kind_subtype)
    .bind(scan_depth_str)
    .bind(&created_at)
    .bind(created_via)
    .bind(org_state_str)
    .execute(pool)
    .await?;

    Ok(RegisterSourceResponse {
        source_id: id,
        kind: req.kind,
        path: req.path.clone(),
        created_at,
        organization_state: effective_org_state,
    })
}

/// Register multiple sources in a single transaction with partial-success
/// semantics.
///
/// Each item is attempted independently; failures do not roll back
/// successful inserts within the batch.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` only for catastrophic connection failures.
/// Per-item errors are captured in the response.
pub async fn register_source_batch(
    pool: &SqlitePool,
    req: &RegisterSourceBatchRequest,
) -> DbResult<RegisterSourceBatchResponse> {
    let created_via = resolve_created_via(pool).await?;
    let created_at = Timestamp::now_iso();

    let mut items: Vec<BatchItem> = Vec::with_capacity(req.sources.len());
    let mut success_count = 0usize;
    let mut failure_count = 0usize;

    let mut tx = pool.begin().await?;

    for (index, source) in req.sources.iter().enumerate() {
        let id = Uuid::new_v4().to_string();
        let kind_str = source_kind_to_str(source.kind);
        let scan_depth_str = scan_depth_to_str(source.scan_depth);
        let org_state_str = organization_state_to_str(source.organization_state);

        let result = sqlx::query(
            "INSERT INTO registered_sources \
             (id, kind, path, kind_subtype, scan_depth, created_at, created_via, organization_state) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(kind_str)
        .bind(&source.path)
        .bind(&source.kind_subtype)
        .bind(scan_depth_str)
        .bind(&created_at)
        .bind(created_via)
        .bind(org_state_str)
        .execute(&mut *tx)
        .await;

        match result {
            Ok(_) => {
                success_count += 1;
                items.push(BatchItem {
                    index,
                    status: ItemStatus::Success,
                    source_id: Some(id),
                    error: None,
                    error_detail: None,
                });
            }
            Err(e) => {
                failure_count += 1;
                items.push(BatchItem {
                    index,
                    status: ItemStatus::Failure,
                    source_id: None,
                    error: Some(e.to_string()),
                    error_detail: None,
                });
            }
        }
    }

    tx.commit().await?;

    let status = if failure_count == 0 {
        BatchStatus::Success
    } else if success_count == 0 {
        BatchStatus::Failure
    } else {
        BatchStatus::Partial
    };

    Ok(RegisterSourceBatchResponse { status, items })
}

/// List all registered sources.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_sources(pool: &SqlitePool) -> DbResult<Vec<RegisterSourceResponse>> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, kind, path, created_at, organization_state \
         FROM registered_sources ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, kind, path, created_at, org_state)| RegisterSourceResponse {
            source_id: id,
            kind: str_to_source_kind(&kind),
            path,
            created_at,
            organization_state: str_to_organization_state(&org_state),
        })
        .collect())
}

/// Remove a registered source by ID.
///
/// Also deletes any `inbox_items` whose `root_id` references this source so
/// that no orphaned rows remain after removal (H1 — no FK cascade in schema).
/// Callers MUST check [`super::count_root_dependents`] first (P6b, decision D8) —
/// this function does not itself guard against dependents; it is also used by
/// the pre-existing (dependents-free by construction) removal paths.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn remove_source(pool: &SqlitePool, id: &str) -> DbResult<()> {
    // Clean up inbox items that belong to this source before removing it.
    sqlx::query("DELETE FROM inbox_items WHERE root_id = ?").bind(id).execute(pool).await?;

    let result =
        sqlx::query("DELETE FROM registered_sources WHERE id = ?").bind(id).execute(pool).await?;

    if result.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("registered source {id} not found")));
    }

    Ok(())
}
