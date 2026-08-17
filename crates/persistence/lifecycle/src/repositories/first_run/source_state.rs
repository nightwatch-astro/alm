// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-source organization state / path / active-flag get-set.

use domain_core::first_run::{OrganizationState, SourceKind};
use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

use super::{organization_state_to_str, str_to_organization_state, str_to_source_kind};

/// Read a source's organization state by its source/root id (spec 041, T029).
///
/// Returns `None` when no source row matches `source_id`. `inbox`-kind sources
/// are always stored as `unorganized` (enforced on write), so the value read
/// back here is authoritative for the per-file move-vs-catalogue decision.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_source_organization_state(
    pool: &SqlitePool,
    source_id: &str,
) -> DbResult<Option<OrganizationState>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT organization_state FROM registered_sources WHERE id = ?")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(s,)| str_to_organization_state(&s)))
}

/// Look up the absolute filesystem `path` of a `registered_sources` row by id.
///
/// Inbox plans store `from_root_id`/`to_root_id` as `registered_sources` ids
/// (the gen-3 source model). The plan executor resolves those ids to an
/// absolute root path so its path gate can anchor the plan's relative
/// source/destination paths. The legacy `library_root` table is not populated
/// by first-run registration, so the executor must consult `registered_sources`
/// to resolve a root that was added through the setup wizard.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_source_path(pool: &SqlitePool, source_id: &str) -> DbResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT path FROM registered_sources WHERE id = ?")
        .bind(source_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(p,)| p))
}

/// Look up a registered source's kind + current path by id (P6a — `roots.remap`
/// preview and `roots.remap.apply` both need the kind-and-path pair to report
/// `original_path` and to resolve sample relative paths).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_source_kind_and_path(
    pool: &SqlitePool,
    source_id: &str,
) -> DbResult<Option<(SourceKind, String)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT kind, path FROM registered_sources WHERE id = ?")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(kind, path)| (str_to_source_kind(&kind), path)))
}

/// Set a source's organization state by id (spec 041, T030 persistence half).
///
/// Enforces the invariant that `inbox`-kind sources are always `unorganized`:
/// attempting to set an inbox source to `organized` returns
/// `persistence_core::DbError::CasFailed` with the `source.invalid_organization_state` marker
/// in the message (the app/core use-case maps this to the contract error code).
///
/// # Errors
///
/// - [`DbError::NotFound`] when no source row matches `source_id`.
/// - `persistence_core::DbError::CasFailed` when attempting to set an inbox source to organized.
/// - `persistence_core::DbError::Database` on query failure.
pub async fn set_source_organization_state(
    pool: &SqlitePool,
    source_id: &str,
    state: OrganizationState,
) -> DbResult<()> {
    // Load the source kind first so we can enforce inbox⇒unorganized.
    let kind_row: Option<(String,)> =
        sqlx::query_as("SELECT kind FROM registered_sources WHERE id = ?")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;
    let Some((kind,)) = kind_row else {
        return Err(DbError::NotFound(format!("registered_source not found: {source_id}")));
    };

    if kind == "inbox" && matches!(state, OrganizationState::Organized) {
        return Err(DbError::CasFailed(
            "source.invalid_organization_state: inbox sources must be unorganized".to_owned(),
        ));
    }

    let state_str = organization_state_to_str(state);
    sqlx::query("UPDATE registered_sources SET organization_state = ? WHERE id = ?")
        .bind(state_str)
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update a registered source's stored path by id (P6a — `roots.remap.apply`).
///
/// This is a metadata-only update: no files are moved, copied, or touched on
/// disk (Constitution §I — the filesystem is user-owned; the app only
/// re-points its own record of where the root now lives).
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn set_source_path(pool: &SqlitePool, source_id: &str, new_path: &str) -> DbResult<()> {
    let result = sqlx::query("UPDATE registered_sources SET path = ? WHERE id = ?")
        .bind(new_path)
        .bind(source_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("registered_source not found: {source_id}")));
    }

    Ok(())
}

/// Read the `active` flag for every registered source, keyed by source id
/// (P6b — `roots.list` merges this into each `LibraryRoot.active`, mirroring
/// how `lastScanned` is merged from `inbox_source_groups`).
///
/// Sources with no matching row simply do not appear in the map; callers
/// should default to `true` (active) for any id absent from it.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_active_flags(
    pool: &SqlitePool,
) -> DbResult<std::collections::HashMap<String, bool>> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT id, active FROM registered_sources").fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(id, active)| (id, active != 0)).collect())
}

/// Set a registered source's `active` flag by id (P6b — `sources.set_active`).
///
/// Disabling a root excludes it from scan/ingest surfaces but does not touch
/// its history: `file_record`, `plan_items`, `inbox_items`, and session rows
/// referencing it are left completely untouched (constitution §I — a
/// visibility flag, not a deletion).
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn set_source_active(pool: &SqlitePool, source_id: &str, active: bool) -> DbResult<()> {
    let result = sqlx::query("UPDATE registered_sources SET active = ? WHERE id = ?")
        .bind(i64::from(active))
        .bind(source_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("registered_source not found: {source_id}")));
    }

    Ok(())
}
