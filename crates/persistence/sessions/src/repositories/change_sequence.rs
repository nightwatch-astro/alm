// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `repository_change` sequence management.
//!
//! Every domain write that consumers must observe inserts a `repository_change`
//! row. The sequence is used as a watermark for list pagination. Reads inside
//! a `BEGIN IMMEDIATE` transaction see the current writer's view of the table,
//! avoiding races against concurrent inserters on separate pool connections.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::DbResult;

/// Read the current `repository_change` sequence on an open connection.
///
/// Must be called inside a `BEGIN IMMEDIATE` transaction to avoid racing a
/// concurrent writer on a separate pool connection. Returns the current
/// maximum sequence value, or 0 when the table is empty.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn current_sequence_on_conn(conn: &mut SqliteConnection) -> DbResult<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(sequence), 0) FROM repository_change")
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

/// Insert one `repository_change` row and return its sequence number.
///
/// `command_row_id` is nullable; pass `None` for sequence rows created by the
/// materialization apply loop (which owns its own command via the ledger).
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn insert_repository_change(
    conn: &mut SqliteConnection,
    command_row_id: Option<i64>,
    created_at: &str,
) -> DbResult<i64> {
    let result =
        sqlx::query("INSERT INTO repository_change(command_row_id, created_at) VALUES (?, ?)")
            .bind(command_row_id)
            .bind(created_at)
            .execute(&mut *conn)
            .await?;
    Ok(result.last_insert_rowid())
}

/// Read the current `repository_change` sequence from the pool.
///
/// Safe for read-only paths (pagination watermarks, list queries) where no
/// write transaction is held. For writes, prefer [`current_sequence_on_conn`]
/// inside `BEGIN IMMEDIATE` instead.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn current_sequence(pool: &SqlitePool) -> DbResult<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(sequence), 0) FROM repository_change")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}
