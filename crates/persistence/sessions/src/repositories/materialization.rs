// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for `session_materialization_operation` and
//! `session_materialization_result_snapshot`.
//!
//! Operations transition through `ready → applying → applied` (or `cancelled` /
//! `failed`). Each state transition uses CAS on `state_version`. The terminal
//! apply transaction inserts the result snapshot, updates the operation state,
//! and records audit/outbox rows atomically.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

/// One `session_materialization_operation` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MaterializationOperationRow {
    pub row_id: i64,
    pub public_id: String,
    pub kind: String,
    pub command_row_id: i64,
    pub config_revision_row_id: i64,
    pub state: String,
    pub state_version: i64,
    pub result_snapshot_row_id: Option<i64>,
    pub session_count: Option<i64>,
    pub membership_count: Option<i64>,
    pub singleton_group_count: Option<i64>,
    pub blocked_frame_count: Option<i64>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub failure_code: Option<String>,
    pub created_sequence: i64,
    pub created_at: String,
}

/// One `session_materialization_result_snapshot` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MaterializationResultSnapshotRow {
    pub row_id: i64,
    pub public_id: String,
    pub operation_row_id: i64,
    pub session_count: i64,
    pub membership_count: i64,
    pub singleton_group_count: i64,
    pub blocked_frame_count: i64,
    pub canonical_digest: String,
    pub created_sequence: i64,
    pub created_at: String,
}

/// Parameters for inserting a new `session_materialization_operation`.
pub struct InsertMaterializationOperation<'a> {
    pub public_id: &'a str,
    pub kind: &'a str,
    pub command_row_id: i64,
    pub config_revision_row_id: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Parameters for inserting a `session_materialization_result_snapshot`.
pub struct InsertMaterializationResultSnapshot<'a> {
    pub public_id: &'a str,
    pub operation_row_id: i64,
    pub session_count: i64,
    pub membership_count: i64,
    pub singleton_group_count: i64,
    pub blocked_frame_count: i64,
    pub canonical_digest: &'a str,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Insert a new `session_materialization_operation` in `ready` state.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_materialization_operation(
    conn: &mut SqliteConnection,
    params: &InsertMaterializationOperation<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO session_materialization_operation (
            public_id, kind, command_row_id, config_revision_row_id, state,
            created_sequence, created_at
         ) VALUES (?,?,?,?,'ready',?,?)",
    )
    .bind(params.public_id)
    .bind(params.kind)
    .bind(params.command_row_id)
    .bind(params.config_revision_row_id)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Transition an operation from `ready` to `applying` using CAS on
/// `state_version`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` on version mismatch, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn transition_operation_to_applying(
    conn: &mut SqliteConnection,
    operation_row_id: i64,
    expected_state_version: i64,
    started_at: &str,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE session_materialization_operation
         SET state = 'applying',
             state_version = state_version + 1,
             started_at = ?
         WHERE row_id = ?
           AND state = 'ready'
           AND state_version = ?",
    )
    .bind(started_at)
    .bind(operation_row_id)
    .bind(expected_state_version)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "operation {operation_row_id} state_version CAS failed (ready → applying)"
        )));
    }
    Ok(())
}

/// Payload for [`transition_operation_to_applied`].
pub struct ApplyOperationResult<'a> {
    pub operation_row_id: i64,
    pub expected_state_version: i64,
    pub result_snapshot_row_id: i64,
    pub session_count: i64,
    pub membership_count: i64,
    pub singleton_group_count: i64,
    pub blocked_frame_count: i64,
    pub finished_at: &'a str,
}

/// Mark an operation as `applied` and link it to its result snapshot.
///
/// The result snapshot must be inserted first. This update sets `state`,
/// increments `state_version`, and stamps `finished_at`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` on version mismatch, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn transition_operation_to_applied(
    conn: &mut SqliteConnection,
    params: &ApplyOperationResult<'_>,
) -> DbResult<()> {
    let operation_row_id = params.operation_row_id;
    let result = sqlx::query(
        "UPDATE session_materialization_operation
         SET state = 'applied',
             state_version = state_version + 1,
             result_snapshot_row_id = ?,
             session_count = ?,
             membership_count = ?,
             singleton_group_count = ?,
             blocked_frame_count = ?,
             finished_at = ?
         WHERE row_id = ?
           AND state = 'applying'
           AND state_version = ?",
    )
    .bind(params.result_snapshot_row_id)
    .bind(params.session_count)
    .bind(params.membership_count)
    .bind(params.singleton_group_count)
    .bind(params.blocked_frame_count)
    .bind(params.finished_at)
    .bind(params.operation_row_id)
    .bind(params.expected_state_version)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "operation {operation_row_id} state_version CAS failed (applying → applied)"
        )));
    }
    Ok(())
}

/// Mark an operation as `failed`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` on version mismatch, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn transition_operation_to_failed(
    conn: &mut SqliteConnection,
    operation_row_id: i64,
    expected_state_version: i64,
    failure_code: &str,
    finished_at: &str,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE session_materialization_operation
         SET state = 'failed',
             state_version = state_version + 1,
             failure_code = ?,
             finished_at = ?
         WHERE row_id = ?
           AND state = 'applying'
           AND state_version = ?",
    )
    .bind(failure_code)
    .bind(finished_at)
    .bind(operation_row_id)
    .bind(expected_state_version)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "operation {operation_row_id} state_version CAS failed (applying → failed)"
        )));
    }
    Ok(())
}

/// Insert one immutable `session_materialization_result_snapshot` row.
///
/// Called inside the terminal apply transaction, before
/// [`transition_operation_to_applied`].
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_result_snapshot(
    conn: &mut SqliteConnection,
    params: &InsertMaterializationResultSnapshot<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO session_materialization_result_snapshot (
            public_id, operation_row_id,
            session_count, membership_count,
            singleton_group_count, blocked_frame_count,
            canonical_digest, created_sequence, created_at
         ) VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.operation_row_id)
    .bind(params.session_count)
    .bind(params.membership_count)
    .bind(params.singleton_group_count)
    .bind(params.blocked_frame_count)
    .bind(params.canonical_digest)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Fetch a `session_materialization_operation` by its `public_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_operation_by_public_id(
    pool: &SqlitePool,
    public_id: &str,
) -> DbResult<MaterializationOperationRow> {
    sqlx::query_as::<_, MaterializationOperationRow>(
        "SELECT row_id, public_id, kind, command_row_id, config_revision_row_id,
                state, state_version, result_snapshot_row_id,
                session_count, membership_count, singleton_group_count, blocked_frame_count,
                started_at, finished_at, failure_code, created_sequence, created_at
         FROM session_materialization_operation
         WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("operation {public_id}")))
}

/// Fetch the result snapshot for an operation by its `public_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no snapshot exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_result_snapshot_by_operation_public_id(
    pool: &SqlitePool,
    operation_public_id: &str,
) -> DbResult<MaterializationResultSnapshotRow> {
    sqlx::query_as::<_, MaterializationResultSnapshotRow>(
        "SELECT rs.row_id, rs.public_id, rs.operation_row_id,
                rs.session_count, rs.membership_count,
                rs.singleton_group_count, rs.blocked_frame_count,
                rs.canonical_digest, rs.created_sequence, rs.created_at
         FROM session_materialization_result_snapshot rs
         INNER JOIN session_materialization_operation op
             ON op.row_id = rs.operation_row_id
         WHERE op.public_id = ?",
    )
    .bind(operation_public_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        DbError::NotFound(format!("result snapshot for operation {operation_public_id}"))
    })
}

/// Transition an operation from `applying` or `cancelling` to `cancelled`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` on version mismatch, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn transition_operation_to_cancelled(
    conn: &mut SqliteConnection,
    operation_row_id: i64,
    expected_state_version: i64,
    finished_at: &str,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE session_materialization_operation
         SET state = 'cancelled',
             state_version = state_version + 1,
             finished_at = ?
         WHERE row_id = ?
           AND state IN ('applying','cancelling')
           AND state_version = ?",
    )
    .bind(finished_at)
    .bind(operation_row_id)
    .bind(expected_state_version)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!("operation {operation_row_id} cancel CAS failed")));
    }
    Ok(())
}

/// Fetch a `session_materialization_operation.public_id` by `row_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_operation_public_id_by_row_id(pool: &SqlitePool, row_id: i64) -> DbResult<String> {
    let row: (String,) =
        sqlx::query_as("SELECT public_id FROM session_materialization_operation WHERE row_id = ?")
            .bind(row_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| DbError::NotFound(format!("operation row_id {row_id}")))?;
    Ok(row.0)
}
