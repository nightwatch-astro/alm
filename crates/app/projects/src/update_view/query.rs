// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(
    clippy::missing_errors_doc,
    clippy::explicit_auto_deref,
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    dead_code
)]

//! Read-only queries for Update View plans, items, conflicts, and overlay
//! mappings (spec 062 US3/US5 FR-055–FR-059, FR-093–FR-100).

use sqlx::SqlitePool;

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use persistence_plans::repositories::update_view as repo;
use persistence_sessions::repositories::tx;

// ── DTOs ─────────────────────────────────────────────────────────────────────

/// Map DB `draft` → contract `open`.
pub fn contract_state(db_state: &str) -> &'static str {
    match db_state {
        "draft" => "open",
        "approved" => "approved",
        "applying" => "applying",
        "stopped" => "stopped",
        "applied" => "applied",
        "failed" => "failed",
        "discarded" => "discarded",
        "stale" => "stale",
        _ => "unknown",
    }
}

/// `UpdateViewPlan` contract DTO.
#[derive(Debug, Clone)]
pub struct UpdateViewPlan {
    pub plan_id: String,
    pub state: String,
    pub project_id: String,
    pub base_snapshot_id: Option<String>,
    pub target_membership_revision_id: String,
    pub plan_digest: String,
    pub session_count: i64,
    pub item_count: i64,
    pub source_frame_count: i64,
    pub source_byte_count: i64,
    pub conflict_count: i64,
    pub remaining_session_count: i64,
    pub next_session_id: Option<String>,
    pub created_at: String,
    pub created_by: String,
}

/// Paged result for pinned/added session lists.
#[derive(Debug, Clone)]
pub struct PinnedSessionPage {
    pub sessions: Vec<PlanSessionItem>,
    pub next_ordinal: Option<i64>,
}

/// Paged result for added-session list.
pub type AddedSessionPage = PinnedSessionPage;

#[derive(Debug, Clone)]
pub struct PlanSessionItem {
    pub session_id: String,
    pub ordinal: i64,
}

/// Paged plan items.
#[derive(Debug, Clone)]
pub struct ItemPage {
    pub items: Vec<PlanItemDto>,
    pub next_ordinal: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct PlanItemDto {
    pub item_id: String,
    pub session_id: String,
    pub destination_relative_path: String,
    pub collision_state: String,
    pub approved_fingerprint: String,
    pub ordinal: i64,
}

/// Paged conflict list.
#[derive(Debug, Clone)]
pub struct ConflictPage {
    pub conflicts: Vec<ConflictDto>,
    pub next_ordinal: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ConflictDto {
    pub item_id: String,
    pub destination_relative_path: String,
    pub ordinal: i64,
}

/// Paged overlay-mapping list.
#[derive(Debug, Clone)]
pub struct OverlayMappingPage {
    pub mappings: Vec<OverlayMappingDto>,
    pub next_ordinal: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct OverlayMappingDto {
    pub ordinal: i64,
    pub predecessor_entry_id: String,
    pub replacement_entry_id: Option<String>,
    pub exclusion_reason_code: Option<String>,
}

/// `UpdateViewOperationProgress` DTO.
#[derive(Debug, Clone)]
pub struct OperationProgress {
    pub operation_id: String,
    pub plan_id: String,
    pub state: String,
    pub completed_items: u64,
    pub total_items: u64,
}

const PAGE_LIMIT: i64 = 100;

// ── Helpers ───────────────────────────────────────────────────────────────────

pub(super) fn plan_not_found(plan_id: &str) -> ContractError {
    ContractError::new(
        ErrorCode::ProjectUpdateViewPlanNotFound,
        format!("update view plan {plan_id} not found"),
        ErrorSeverity::Blocking,
        false,
    )
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// `project.update_view.query` — fetch `UpdateViewPlan` DTO.
pub async fn query_update_view(
    pool: &SqlitePool,
    plan_id: &str,
) -> Result<UpdateViewPlan, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;

    let plan = repo::get_plan_by_public_id(&mut *conn, plan_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => plan_not_found(plan_id),
        other => app_core_errors::db_err(other),
    })?;

    let conflict_count =
        repo::count_conflicts(&mut *conn, plan.row_id).await.map_err(app_core_errors::db_err)?;

    let base_snapshot_id = match plan.base_snapshot_row_id {
        Some(rid) => {
            Some(repo::snapshot_public_id(&mut *conn, rid).await.map_err(app_core_errors::db_err)?)
        }
        None => None,
    };
    let target_membership_revision_id =
        repo::revision_public_id(&mut *conn, plan.target_membership_revision_row_id)
            .await
            .map_err(app_core_errors::db_err)?;
    let actor_pub = repo::actor_public_id(&mut *conn, plan.actor_row_id)
        .await
        .map_err(app_core_errors::db_err)?;
    let project_pub = repo::project_public_id(&mut *conn, plan.project_row_id)
        .await
        .map_err(app_core_errors::db_err)?;
    let next_session_id = match plan.next_session_row_id {
        Some(rid) => {
            Some(repo::session_public_id(&mut *conn, rid).await.map_err(app_core_errors::db_err)?)
        }
        None => None,
    };

    Ok(UpdateViewPlan {
        plan_id: plan.public_id,
        state: contract_state(&plan.state).to_owned(),
        project_id: project_pub,
        base_snapshot_id,
        target_membership_revision_id,
        plan_digest: plan.content_digest,
        session_count: plan.session_count,
        item_count: plan.item_count,
        source_frame_count: plan.source_frame_count,
        source_byte_count: plan.source_byte_count,
        conflict_count,
        remaining_session_count: plan.remaining_session_count,
        next_session_id,
        created_at: plan.created_at,
        created_by: actor_pub,
    })
}

/// `project.update_view.pinned_session.list`.
pub async fn list_update_view_pinned_sessions(
    pool: &SqlitePool,
    plan_id: &str,
    after_ordinal: Option<i64>,
) -> Result<PinnedSessionPage, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    let plan = repo::get_plan_by_public_id(&mut *conn, plan_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => plan_not_found(plan_id),
        other => app_core_errors::db_err(other),
    })?;

    let rows = repo::list_plan_sessions(&mut *conn, plan.row_id, after_ordinal, PAGE_LIMIT + 1)
        .await
        .map_err(app_core_errors::db_err)?;

    let has_more = rows.len() as i64 > PAGE_LIMIT;
    let next_ordinal =
        if has_more { rows.get(PAGE_LIMIT as usize - 1).map(|r| r.ordinal) } else { None };
    let sessions = rows
        .into_iter()
        .take(PAGE_LIMIT as usize)
        .map(|r| PlanSessionItem { session_id: r.session_public_id, ordinal: r.ordinal })
        .collect();
    Ok(PinnedSessionPage { sessions, next_ordinal })
}

/// `project.update_view.added_session.list`.
pub async fn list_update_view_added_sessions(
    pool: &SqlitePool,
    plan_id: &str,
    after_ordinal: Option<i64>,
) -> Result<AddedSessionPage, ContractError> {
    list_update_view_pinned_sessions(pool, plan_id, after_ordinal).await
}

/// `project.update_view.item.list`.
pub async fn list_update_view_items(
    pool: &SqlitePool,
    plan_id: &str,
    after_ordinal: Option<i64>,
) -> Result<ItemPage, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    let plan = repo::get_plan_by_public_id(&mut *conn, plan_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => plan_not_found(plan_id),
        other => app_core_errors::db_err(other),
    })?;

    let rows = repo::list_plan_entries(&mut *conn, plan.row_id, after_ordinal, PAGE_LIMIT + 1)
        .await
        .map_err(app_core_errors::db_err)?;

    let has_more = rows.len() as i64 > PAGE_LIMIT;
    let next_ordinal =
        if has_more { rows.get(PAGE_LIMIT as usize - 1).map(|r| r.ordinal) } else { None };

    let mut items = Vec::with_capacity(PAGE_LIMIT as usize);
    for r in rows.into_iter().take(PAGE_LIMIT as usize) {
        let session_pub = repo::session_public_id(&mut *conn, r.session_row_id)
            .await
            .map_err(app_core_errors::db_err)?;
        items.push(PlanItemDto {
            item_id: r.public_id,
            session_id: session_pub,
            destination_relative_path: r.relative_path,
            collision_state: r.collision_state,
            approved_fingerprint: r.approved_fingerprint,
            ordinal: r.ordinal,
        });
    }
    Ok(ItemPage { items, next_ordinal })
}

/// `project.update_view.conflict.list`.
pub async fn list_update_view_conflicts(
    pool: &SqlitePool,
    plan_id: &str,
    after_ordinal: Option<i64>,
) -> Result<ConflictPage, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    let plan = repo::get_plan_by_public_id(&mut *conn, plan_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => plan_not_found(plan_id),
        other => app_core_errors::db_err(other),
    })?;

    let rows = repo::list_conflict_entries(&mut *conn, plan.row_id, after_ordinal, PAGE_LIMIT + 1)
        .await
        .map_err(app_core_errors::db_err)?;

    let has_more = rows.len() as i64 > PAGE_LIMIT;
    let next_ordinal =
        if has_more { rows.get(PAGE_LIMIT as usize - 1).map(|r| r.ordinal) } else { None };
    let conflicts = rows
        .into_iter()
        .take(PAGE_LIMIT as usize)
        .map(|r| ConflictDto {
            item_id: r.public_id,
            destination_relative_path: r.relative_path,
            ordinal: r.ordinal,
        })
        .collect();
    Ok(ConflictPage { conflicts, next_ordinal })
}

/// `project.update_view.overlay_mapping.list`.
pub async fn list_update_view_overlay_mappings(
    pool: &SqlitePool,
    plan_id: &str,
    after_ordinal: Option<i64>,
) -> Result<OverlayMappingPage, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    let plan = repo::get_plan_by_public_id(&mut *conn, plan_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => plan_not_found(plan_id),
        other => app_core_errors::db_err(other),
    })?;

    let rows = repo::list_overlay_mappings(&mut *conn, plan.row_id, after_ordinal, PAGE_LIMIT + 1)
        .await
        .map_err(app_core_errors::db_err)?;

    let has_more = rows.len() as i64 > PAGE_LIMIT;
    let next_ordinal =
        if has_more { rows.get(PAGE_LIMIT as usize - 1).map(|r| r.ordinal) } else { None };
    let mappings = rows
        .into_iter()
        .take(PAGE_LIMIT as usize)
        .map(|r| OverlayMappingDto {
            ordinal: r.ordinal,
            predecessor_entry_id: r.predecessor_entry_public_id,
            replacement_entry_id: r.replacement_plan_entry_public_id,
            exclusion_reason_code: r.exclusion_reason_code,
        })
        .collect();
    Ok(OverlayMappingPage { mappings, next_ordinal })
}
