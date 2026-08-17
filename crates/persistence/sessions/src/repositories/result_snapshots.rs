// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for `session_materialization_result_*` child rows.
//!
//! All three insert functions are called inside the terminal `BEGIN IMMEDIATE`
//! transaction of `inbox.materialization.apply`. The result snapshot row
//! itself is inserted by
//! [`super::materialization::insert_result_snapshot`]; these functions
//! populate its ordered child collections.

use sqlx::SqliteConnection;

use persistence_core::DbResult;

/// Insert one `session_materialization_result_session` row.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on constraint violations or SQL errors.
pub async fn insert_result_session(
    conn: &mut SqliteConnection,
    snapshot_row_id: i64,
    session_row_id: i64,
    ordinal: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_materialization_result_session
         (snapshot_row_id, session_row_id, ordinal) VALUES (?,?,?)",
    )
    .bind(snapshot_row_id)
    .bind(session_row_id)
    .bind(ordinal)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert one `session_materialization_result_frame` row.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on constraint violations or SQL errors.
pub async fn insert_result_frame(
    conn: &mut SqliteConnection,
    snapshot_row_id: i64,
    session_row_id: i64,
    frame_row_id: i64,
    ordinal: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_materialization_result_frame
         (snapshot_row_id, session_row_id, frame_row_id, ordinal) VALUES (?,?,?,?)",
    )
    .bind(snapshot_row_id)
    .bind(session_row_id)
    .bind(frame_row_id)
    .bind(ordinal)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert one `session_materialization_result_panel_group` row.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on constraint violations or SQL errors.
pub async fn insert_result_panel_group(
    conn: &mut SqliteConnection,
    snapshot_row_id: i64,
    session_row_id: i64,
    panel_group_row_id: i64,
    initial_panel_revision_row_id: i64,
    ordinal: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_materialization_result_panel_group
         (snapshot_row_id, session_row_id, panel_group_row_id,
          initial_panel_revision_row_id, ordinal)
         VALUES (?,?,?,?,?)",
    )
    .bind(snapshot_row_id)
    .bind(session_row_id)
    .bind(panel_group_row_id)
    .bind(initial_panel_revision_row_id)
    .bind(ordinal)
    .execute(conn)
    .await?;
    Ok(())
}
