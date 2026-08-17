// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Transaction control helpers for materialization write paths.
//!
//! The app layer drives transaction boundaries (which connection to use, when
//! to commit or roll back) but must not contain raw `sqlx::query` calls. These
//! thin wrappers move the SQL text into the persistence crate boundary.

use sqlx::pool::PoolConnection;
use sqlx::Sqlite;

use persistence_core::DbResult;

/// Begin an `IMMEDIATE` write transaction on an acquired connection.
///
/// All materialization write operations use `BEGIN IMMEDIATE` to serialize
/// against concurrent writers while allowing concurrent readers.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn begin_immediate(conn: &mut PoolConnection<Sqlite>) -> DbResult<()> {
    sqlx::query("BEGIN IMMEDIATE").execute(&mut **conn).await?;
    Ok(())
}

/// Commit the current transaction.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn commit(conn: &mut PoolConnection<Sqlite>) -> DbResult<()> {
    sqlx::query("COMMIT").execute(&mut **conn).await?;
    Ok(())
}

/// Roll back the current transaction. Errors are intentionally swallowed
/// because rollback is called only after a prior failure; the original
/// error is the one that matters.
pub async fn rollback(conn: &mut PoolConnection<Sqlite>) {
    let _ = sqlx::query("ROLLBACK").execute(&mut **conn).await;
}

/// Enable foreign-key enforcement on a connection.
///
/// SQLite does not enable foreign keys by default. Write connections that
/// insert or update rows with FK constraints must call this before beginning
/// a transaction.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn enable_foreign_keys(conn: &mut PoolConnection<Sqlite>) -> DbResult<()> {
    sqlx::query("PRAGMA foreign_keys = ON").execute(&mut **conn).await?;
    Ok(())
}
