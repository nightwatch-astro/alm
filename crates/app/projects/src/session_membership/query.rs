// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only queries: `project.related_session.list`, `project.view_state.query`,
//! and `project.view_state.pin.list` (spec 062 contracts).

use sqlx::SqlitePool;

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use persistence_topology::repositories::project_membership as repo;

use super::project_db_err;

// ── DTOs ─────────────────────────────────────────────────────────────────────

/// `RelatedSession` contract DTO (spec 062 contracts/projects-related-sessions-update-view.md).
#[derive(Debug, Clone)]
pub struct RelatedSession {
    pub project_id: String,
    pub session_id: String,
    /// `"panel_sibling"` | `"session_replacement"`
    pub relation_kind: String,
    /// Panel group UUID for sibling relations; None for replacement.
    pub panel_group_id: Option<String>,
    /// Panel head revision UUID or reclassification plan revision UUID used as `evidenceId`.
    pub evidence_id: String,
    pub first_available_at: String,
    pub already_pinned: bool,
}

/// `ProjectSessionPin` contract DTO.
#[derive(Debug, Clone)]
pub struct ProjectSessionPin {
    pub project_id: String,
    pub session_id: String,
    pub pin_revision: i64,
    pub pinned_at: String,
    pub source: String,
    pub replaces_session_id: Option<String>,
}

/// `ProjectViewState` contract DTO.
#[derive(Debug, Clone)]
pub struct ProjectViewState {
    pub project_id: String,
    /// Current `membership_head_generation` (used as `projectRevision`).
    pub project_revision: i64,
    /// Current pinned-session count.
    pub pinned_session_count: u64,
    /// True when the membership head contains a session absent from the
    /// materialization head snapshot's exact materialized-session set.
    pub stale: bool,
    /// Count of sessions in the membership head not in the materialization snapshot.
    pub unmaterialized_session_count: u64,
}

// ── Cursors ───────────────────────────────────────────────────────────────────

/// Cursor for keyset pagination ordered by `(firstAvailableAt DESC, sessionId ASC)`.
pub struct RelatedSessionCursor {
    pub first_available_at: String,
    pub session_id: String,
}

/// Cursor for pin list pagination ordered by `sessionId ASC`.
pub struct PinListCursor {
    pub session_id: String,
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// List related sessions available to a project (informational; does not change pins).
///
/// # Errors
///
/// - `project.not_found` when the `spec062_project` row is absent.
pub async fn list_related_sessions(
    pool: &SqlitePool,
    project_id: &str,
    include_pinned: bool,
    cursor: Option<&RelatedSessionCursor>,
    page_size: i64,
) -> Result<Vec<RelatedSession>, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;

    let project = repo::get_spec062_project(&mut conn, project_id).await.map_err(project_db_err)?;

    let rows = repo::list_related_sessions(
        &mut conn,
        project.row_id,
        project.membership_head_revision_row_id,
        include_pinned,
        cursor.map(|c| c.first_available_at.as_str()),
        cursor.map(|c| c.session_id.as_str()),
        page_size,
    )
    .await
    .map_err(app_core_errors::db_err)?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let evidence_id = r
                .evidence_revision_public_id
                .or(r.reclassification_revision_public_id)
                .unwrap_or_default();
            RelatedSession {
                project_id: project_id.to_owned(),
                session_id: r.session_public_id,
                relation_kind: r.relation_kind,
                panel_group_id: r.panel_group_public_id,
                evidence_id,
                first_available_at: r.first_available_at,
                already_pinned: r.already_pinned,
            }
        })
        .collect())
}

/// Query the current `ProjectViewState` for a spec-062 project.
///
/// # Errors
///
/// - `project.not_found` when the `spec062_project` row is absent.
pub async fn view_state_query(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<ProjectViewState, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;

    let project = repo::get_spec062_project(&mut conn, project_id).await.map_err(project_db_err)?;

    let pinned_count = match project.membership_head_revision_row_id {
        Some(rev_id) => {
            repo::count_pinned_sessions(&mut conn, rev_id).await.map_err(app_core_errors::db_err)?
        }
        None => 0,
    };

    let (unmaterialized_count, stale) = repo::unmaterialized_session_count(
        &mut conn,
        project.row_id,
        project.membership_head_revision_row_id,
    )
    .await
    .map_err(app_core_errors::db_err)?;

    Ok(ProjectViewState {
        project_id: project_id.to_owned(),
        project_revision: project.membership_head_generation,
        pinned_session_count: u64::try_from(pinned_count).unwrap_or(0),
        stale,
        unmaterialized_session_count: u64::try_from(unmaterialized_count).unwrap_or(0),
    })
}

/// List pins in the current membership head of a spec-062 project.
///
/// Sorted by `sessionId ASC` per the contract total order.
///
/// # Errors
///
/// - `project.not_found` when the `spec062_project` row is absent.
/// - `project.membership_conflict` when `project_revision` is stale.
pub async fn list_session_pins(
    pool: &SqlitePool,
    project_id: &str,
    project_revision: i64,
    cursor: Option<&PinListCursor>,
    page_size: i64,
) -> Result<Vec<ProjectSessionPin>, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;

    let project = repo::get_spec062_project(&mut conn, project_id).await.map_err(project_db_err)?;

    if project.membership_head_generation != project_revision {
        return Err(ContractError::new(
            ErrorCode::ProjectMembershipConflict,
            format!(
                "project {} revision {} is stale (current: {})",
                project_id, project_revision, project.membership_head_generation
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let Some(head_rev_id) = project.membership_head_revision_row_id else {
        return Ok(Vec::new());
    };

    let rows = repo::list_pins_paged(
        &mut conn,
        head_rev_id,
        cursor.map(|c| c.session_id.as_str()),
        page_size,
    )
    .await
    .map_err(app_core_errors::db_err)?;

    Ok(rows
        .into_iter()
        .map(|(_row_id, session_id, pin_revision, pinned_at, source, replaces)| ProjectSessionPin {
            project_id: project_id.to_owned(),
            session_id,
            pin_revision,
            pinned_at,
            source,
            replaces_session_id: replaces,
        })
        .collect())
}
