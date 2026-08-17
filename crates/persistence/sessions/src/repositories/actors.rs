// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `spec062_actor` and `spec062_target` upsert helpers.
//!
//! These tables hold stable UUIDs for the Spec 062 domain actor and target
//! identities, distinct from the legacy `target` and `actors` tables.
//! Both inserts are idempotent via `ON CONFLICT DO NOTHING`.

use sqlx::SqliteConnection;

use persistence_core::DbResult;

/// Upsert a `spec062_actor` row and return its `row_id`.
///
/// Inserts if absent; returns the existing `row_id` otherwise.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn ensure_spec062_actor(
    conn: &mut SqliteConnection,
    public_id: &str,
    created_at: &str,
) -> DbResult<i64> {
    sqlx::query(
        "INSERT INTO spec062_actor(public_id, created_at) VALUES (?, ?)
         ON CONFLICT(public_id) DO NOTHING",
    )
    .bind(public_id)
    .bind(created_at)
    .execute(&mut *conn)
    .await?;
    let row: (i64,) = sqlx::query_as("SELECT row_id FROM spec062_actor WHERE public_id = ?")
        .bind(public_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

/// Upsert a `spec062_target` row and return its `row_id`.
///
/// Inserts if absent; returns the existing `row_id` otherwise.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn ensure_spec062_target(
    conn: &mut SqliteConnection,
    public_id: &str,
    created_at: &str,
) -> DbResult<i64> {
    sqlx::query(
        "INSERT INTO spec062_target(public_id, created_at) VALUES (?, ?)
         ON CONFLICT(public_id) DO NOTHING",
    )
    .bind(public_id)
    .bind(created_at)
    .execute(&mut *conn)
    .await?;
    let row: (i64,) = sqlx::query_as("SELECT row_id FROM spec062_target WHERE public_id = ?")
        .bind(public_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

/// Look up a `spec062_target.row_id` by public UUID.
///
/// Returns `None` when the target has no `spec062_target` row yet.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on SQL errors.
pub async fn lookup_spec062_target_row_id(
    conn: &mut SqliteConnection,
    public_id: &str,
) -> DbResult<Option<i64>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT row_id FROM spec062_target WHERE public_id = ?")
            .bind(public_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.map(|r| r.0))
}
