// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only queries over `inbox_materialization_plan_result_snapshot` and its
//! child tables, plus `acquisition_site_resolution_revision`.
//!
//! Used by `app_core_inbox::session_materialization::apply` to build the
//! per-session work list from an approved plan's result snapshot.

use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

// ── Row types ────────────────────────────────────────────────────────────────

/// Row from `inbox_materialization_plan_result_snapshot`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlanResultSnapshotRow {
    pub row_id: i64,
    pub public_id: String,
    pub plan_row_id: i64,
    pub plan_revision: i64,
    pub config_revision_row_id: i64,
    pub input_evidence_revision: i64,
    pub proposed_session_count: i64,
    pub frame_count: i64,
    pub blocked_frame_count: i64,
    pub canonical_digest: String,
    pub created_at: String,
}

/// One row from `inbox_plan_result_proposed_session`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProposedSessionRow {
    pub row_id: i64,
    pub snapshot_row_id: i64,
    pub proposed_session_key: String,
    pub kind: String,
    /// `acquisition_site_resolution_revision.row_id` pinned by this partition.
    pub site_resolution_revision_row_id: i64,
    pub identity_digest: String,
    pub ordinal: i64,
    pub frame_count: i64,
}

/// Frame membership row from `inbox_plan_result_proposed_session_frame`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProposedSessionFrameRow {
    pub frame_row_id: i64,
    pub ordinal: i64,
}

/// Resolved site information from `acquisition_site_resolution_revision`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SiteResolutionRevisionRow {
    pub row_id: i64,
    pub state: String,
    pub selected_site_row_id: Option<i64>,
    /// IANA timezone name, present when a site was selected.
    pub timezone_name: Option<String>,
    /// Canonical UTC exposure instant (UTC RFC 3339), if known.
    pub canonical_exposure_at_utc: Option<String>,
    /// Derived local noon-to-noon observing-night date (`YYYY-MM-DD`).
    pub observing_night_date: Option<String>,
}

// ── Queries ──────────────────────────────────────────────────────────────────

/// Look up the `inbox_ingestion_operation` subtype row and return its linked
/// `inbox_materialization_plan_result_snapshot`.
///
/// `operation_row_id` is the `session_materialization_operation.row_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no ingestion-operation subtype row exists.
pub async fn get_plan_snapshot_for_operation(
    pool: &SqlitePool,
    operation_row_id: i64,
) -> DbResult<PlanResultSnapshotRow> {
    sqlx::query_as::<_, PlanResultSnapshotRow>(
        "SELECT s.row_id, s.public_id, s.plan_row_id, s.plan_revision,
                s.config_revision_row_id, s.input_evidence_revision,
                s.proposed_session_count, s.frame_count, s.blocked_frame_count,
                s.canonical_digest, s.created_at
         FROM inbox_ingestion_operation iio
         INNER JOIN inbox_materialization_plan_result_snapshot s
             ON s.row_id = iio.inbox_plan_result_snapshot_row_id
         WHERE iio.operation_row_id = ?",
    )
    .bind(operation_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("plan snapshot for operation {operation_row_id}")))
}

/// Return proposed sessions for a plan result snapshot, ordered by ordinal.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_proposed_sessions(
    pool: &SqlitePool,
    snapshot_row_id: i64,
) -> DbResult<Vec<ProposedSessionRow>> {
    sqlx::query_as::<_, ProposedSessionRow>(
        "SELECT row_id, snapshot_row_id, proposed_session_key, kind,
                site_resolution_revision_row_id, identity_digest, ordinal, frame_count
         FROM inbox_plan_result_proposed_session
         WHERE snapshot_row_id = ?
         ORDER BY ordinal ASC",
    )
    .bind(snapshot_row_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::from)
}

/// Return frame row IDs for one proposed session, ordered by ordinal.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_proposed_session_frames(
    pool: &SqlitePool,
    proposed_session_row_id: i64,
) -> DbResult<Vec<ProposedSessionFrameRow>> {
    sqlx::query_as::<_, ProposedSessionFrameRow>(
        "SELECT frame_row_id, ordinal
         FROM inbox_plan_result_proposed_session_frame
         WHERE proposed_session_row_id = ?
         ORDER BY ordinal ASC",
    )
    .bind(proposed_session_row_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::from)
}

/// Return site-resolution evidence from `acquisition_site_resolution_revision`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the revision row does not exist.
pub async fn get_site_resolution_revision(
    pool: &SqlitePool,
    revision_row_id: i64,
) -> DbResult<SiteResolutionRevisionRow> {
    sqlx::query_as::<_, SiteResolutionRevisionRow>(
        "SELECT row_id, state, selected_site_row_id, timezone_name,
                canonical_exposure_at_utc, observing_night_date
         FROM acquisition_site_resolution_revision
         WHERE row_id = ?",
    )
    .bind(revision_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("site resolution revision {revision_row_id}")))
}
