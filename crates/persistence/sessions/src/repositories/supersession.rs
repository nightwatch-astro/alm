// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for `session_supersession`.
//!
//! A supersession links a predecessor session to one or more replacement
//! sessions of the same kind, inserted as part of a `metadata_reclassification`
//! materialization operation. The schema rejects cycles via an application-layer
//! recursive CTE that the caller must execute before commit.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

/// One supersession edge row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SupersessionRow {
    pub predecessor_session_row_id: i64,
    pub replacement_session_row_id: i64,
    pub kind: String,
    pub applied_plan_revision_row_id: i64,
    pub ordinal: i64,
    pub created_sequence: i64,
    pub created_at: String,
}

/// Parameters for inserting one `session_supersession` row.
pub struct InsertSupersession<'a> {
    pub predecessor_session_row_id: i64,
    pub replacement_session_row_id: i64,
    /// Must equal the `kind` of both referenced sessions.
    pub kind: &'a str,
    pub applied_plan_revision_row_id: i64,
    pub ordinal: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Insert one `session_supersession` row.
///
/// The caller is responsible for:
/// - running the cycle-detection CTE before calling this under `BEGIN IMMEDIATE`;
/// - verifying that predecessor and replacement share the same `kind`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_supersession(
    conn: &mut SqliteConnection,
    params: &InsertSupersession<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_supersession (
            predecessor_session_row_id, replacement_session_row_id, kind,
            applied_plan_revision_row_id, ordinal, created_sequence, created_at
         ) VALUES (?,?,?,?,?,?,?)",
    )
    .bind(params.predecessor_session_row_id)
    .bind(params.replacement_session_row_id)
    .bind(params.kind)
    .bind(params.applied_plan_revision_row_id)
    .bind(params.ordinal)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Return all successors of a predecessor session, in ordinal order.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_supersession_successors(
    pool: &SqlitePool,
    predecessor_session_row_id: i64,
) -> DbResult<Vec<SupersessionRow>> {
    let rows = sqlx::query_as::<_, SupersessionRow>(
        "SELECT predecessor_session_row_id, replacement_session_row_id, kind,
                applied_plan_revision_row_id, ordinal, created_sequence, created_at
         FROM session_supersession
         WHERE predecessor_session_row_id = ?
         ORDER BY ordinal ASC",
    )
    .bind(predecessor_session_row_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Return all predecessors of a replacement session, in ordinal order.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_supersession_predecessors(
    pool: &SqlitePool,
    replacement_session_row_id: i64,
) -> DbResult<Vec<SupersessionRow>> {
    let rows = sqlx::query_as::<_, SupersessionRow>(
        "SELECT predecessor_session_row_id, replacement_session_row_id, kind,
                applied_plan_revision_row_id, ordinal, created_sequence, created_at
         FROM session_supersession
         WHERE replacement_session_row_id = ?
         ORDER BY ordinal ASC",
    )
    .bind(replacement_session_row_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Return `true` if a session has no supersession edge starting from it.
///
/// A session is "current" when this returns `true`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn is_session_current(pool: &SqlitePool, session_row_id: i64) -> DbResult<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM session_supersession
         WHERE predecessor_session_row_id = ?",
    )
    .bind(session_row_id)
    .fetch_one(pool)
    .await?;
    Ok(count.0 == 0)
}

/// Run the cycle-detection CTE for a proposed supersession and return an error
/// if a path from `replacement_row_id` back to `predecessor_row_id` already
/// exists.
///
/// The caller holds `BEGIN IMMEDIATE` and must call this before
/// [`insert_supersession`].
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` when a cycle is detected, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn assert_no_supersession_cycle(
    conn: &mut SqliteConnection,
    predecessor_row_id: i64,
    replacement_row_id: i64,
) -> DbResult<()> {
    // Walk forward from replacement_row_id through existing supersession edges.
    // If predecessor_row_id is reachable, inserting the proposed edge would create a cycle.
    let reached: (i64,) = sqlx::query_as(
        "WITH RECURSIVE reachable(session_row_id) AS (
            SELECT ?1
            UNION
            SELECT ss.replacement_session_row_id
            FROM session_supersession ss
            INNER JOIN reachable r ON ss.predecessor_session_row_id = r.session_row_id
         )
         SELECT COUNT(*) FROM reachable WHERE session_row_id = ?2",
    )
    .bind(replacement_row_id)
    .bind(predecessor_row_id)
    .fetch_one(conn)
    .await?;
    if reached.0 > 0 {
        return Err(DbError::CasFailed(format!(
            "supersession cycle: session {predecessor_row_id} is reachable from replacement {replacement_row_id}"
        )));
    }
    Ok(())
}
