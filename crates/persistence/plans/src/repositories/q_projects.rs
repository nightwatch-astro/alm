// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Free-fn repository queries for `crates/app/projects` (db-boundary-zero
//! drain). These don't fit the existing `projects` / `prepared_source_views`
//! repositories' query shapes exactly, so they live here rather than being
//! folded into either.

use sqlx::SqlitePool;

use persistence_core::DbResult;

/// `(relative_path, state)` for a `file_record` row (spec 049 generation:
/// resolves a session's frame ids to their current path + presence state).
/// Returns `None` when no row exists.
///
/// # Errors
/// Returns [`crate::DbError::Database`] on query failure.
pub async fn get_file_record_path_and_state(
    pool: &SqlitePool,
    id: &str,
) -> DbResult<Option<(String, String)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT relative_path, state FROM file_record WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

/// `acquisition_session` fields needed to plan a session's light-frame
/// destinations (spec 049 generation): source root, night/target key, and
/// the frame id set.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AcquisitionSessionViewRow {
    pub root_id: String,
    pub session_key: String,
    pub frame_ids: String,
}

/// Look up an `acquisition_session` row by id. Returns `None` when no row
/// exists (an unresolved project-linked source, spec 049 FR-019).
///
/// # Errors
/// Returns [`crate::DbError::Database`] on query failure.
pub async fn get_acquisition_session_view(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Option<AcquisitionSessionViewRow>> {
    let row = sqlx::query_as::<_, AcquisitionSessionViewRow>(
        "SELECT root_id, session_key, frame_ids FROM acquisition_session WHERE id = ?",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// `(calibration_type, master_id)` pairs assigned to a session (spec 049
/// generation's best-effort calibration match; unordered — callers group by
/// type into a set).
///
/// # Errors
/// Returns [`crate::DbError::Database`] on query failure.
pub async fn list_calibration_assignment_types(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT calibration_type, master_id FROM calibration_assignment WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// `(root_id, frame_ids)` for a `calibration_session` row (spec 049
/// generation: resolves a matched master's source root + frame id set).
/// Returns `None` when no row exists.
///
/// # Errors
/// Returns [`crate::DbError::Database`] on query failure.
pub async fn get_calibration_session_view(
    pool: &SqlitePool,
    master_id: &str,
) -> DbResult<Option<(String, String)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT root_id, frame_ids FROM calibration_session WHERE id = ?")
            .bind(master_id)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}
