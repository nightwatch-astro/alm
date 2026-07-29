// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for `session_metadata_resolution`,
//! `session_metadata_resolution_frame`, and
//! `session_metadata_resolution_head`.
//!
//! Each accepted revision is immutable. The head row advances by CAS on
//! `head_generation`. `metadataResolutionRevision` in the contract corresponds
//! to the `revision_number` of the accepted head revision.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

/// One metadata resolution revision row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MetadataResolutionRow {
    pub row_id: i64,
    pub public_id: String,
    pub session_row_id: i64,
    pub revision_number: i64,
    pub predecessor_resolution_row_id: Option<i64>,
    pub state: String,
    pub actor_row_id: i64,
    pub command_row_id: i64,
    pub created_sequence: i64,
    pub created_at: String,
}

/// One `session_metadata_resolution_frame` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MetadataResolutionFrameRow {
    pub resolution_row_id: i64,
    pub frame_row_id: i64,
    pub evidence_row_id: i64,
    pub ordinal: i64,
}

/// Head pointer row for metadata resolution.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MetadataResolutionHeadRow {
    pub session_row_id: i64,
    pub head_resolution_row_id: i64,
    pub head_generation: i64,
}

/// Parameters for inserting one metadata resolution revision.
pub struct InsertMetadataResolution<'a> {
    pub public_id: &'a str,
    pub session_row_id: i64,
    pub revision_number: i64,
    pub predecessor_resolution_row_id: Option<i64>,
    pub state: &'a str,
    pub actor_row_id: i64,
    pub command_row_id: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Parameters for inserting one `session_metadata_resolution_frame` pin.
pub struct InsertMetadataResolutionFrame {
    pub resolution_row_id: i64,
    pub frame_row_id: i64,
    pub evidence_row_id: i64,
    pub ordinal: i64,
}

/// Insert one immutable `session_metadata_resolution` revision.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_metadata_resolution(
    conn: &mut SqliteConnection,
    params: &InsertMetadataResolution<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO session_metadata_resolution (
            public_id, session_row_id, revision_number,
            predecessor_resolution_row_id,
            state, actor_row_id, command_row_id,
            created_sequence, created_at
         ) VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.session_row_id)
    .bind(params.revision_number)
    .bind(params.predecessor_resolution_row_id)
    .bind(params.state)
    .bind(params.actor_row_id)
    .bind(params.command_row_id)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert one `session_metadata_resolution_frame` evidence pin.
///
/// Must be called in the same transaction as the resolution row it belongs to.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_metadata_resolution_frame(
    conn: &mut SqliteConnection,
    params: &InsertMetadataResolutionFrame,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_metadata_resolution_frame
         (resolution_row_id, frame_row_id, evidence_row_id, ordinal)
         VALUES (?,?,?,?)",
    )
    .bind(params.resolution_row_id)
    .bind(params.frame_row_id)
    .bind(params.evidence_row_id)
    .bind(params.ordinal)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert the initial head pointer for a session's metadata resolution.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_metadata_resolution_head(
    conn: &mut SqliteConnection,
    session_row_id: i64,
    head_resolution_row_id: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_metadata_resolution_head
         (session_row_id, head_resolution_row_id, head_generation)
         VALUES (?, ?, 0)",
    )
    .bind(session_row_id)
    .bind(head_resolution_row_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Advance the metadata resolution head using CAS.
///
/// Returns `persistence_core::DbError::CasFailed` when the current generation or head revision
/// does not match the expected values.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` on optimistic-lock failure, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn advance_metadata_resolution_head(
    conn: &mut SqliteConnection,
    session_row_id: i64,
    expected_head_row_id: i64,
    expected_generation: i64,
    new_head_row_id: i64,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE session_metadata_resolution_head
         SET head_resolution_row_id = ?,
             head_generation = head_generation + 1
         WHERE session_row_id = ?
           AND head_resolution_row_id = ?
           AND head_generation = ?",
    )
    .bind(new_head_row_id)
    .bind(session_row_id)
    .bind(expected_head_row_id)
    .bind(expected_generation)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "metadata resolution head CAS failed for session {session_row_id}"
        )));
    }
    Ok(())
}

/// Fetch the current metadata resolution head for a session.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no head row exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_metadata_resolution_head(
    pool: &SqlitePool,
    session_row_id: i64,
) -> DbResult<MetadataResolutionHeadRow> {
    sqlx::query_as::<_, MetadataResolutionHeadRow>(
        "SELECT session_row_id, head_resolution_row_id, head_generation
         FROM session_metadata_resolution_head
         WHERE session_row_id = ?",
    )
    .bind(session_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        DbError::NotFound(format!("metadata resolution head for session {session_row_id}"))
    })
}

/// Fetch the accepted metadata resolution revision for a session.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no head or revision exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_accepted_metadata_resolution(
    pool: &SqlitePool,
    session_row_id: i64,
) -> DbResult<MetadataResolutionRow> {
    sqlx::query_as::<_, MetadataResolutionRow>(
        "SELECT mr.row_id, mr.public_id, mr.session_row_id, mr.revision_number,
                mr.predecessor_resolution_row_id,
                mr.state, mr.actor_row_id, mr.command_row_id,
                mr.created_sequence, mr.created_at
         FROM session_metadata_resolution mr
         INNER JOIN session_metadata_resolution_head mrh
             ON mrh.head_resolution_row_id = mr.row_id
         WHERE mrh.session_row_id = ?",
    )
    .bind(session_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("metadata resolution for session {session_row_id}")))
}

/// Return all pinned frame evidence rows for a metadata resolution revision.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_metadata_resolution_frames(
    pool: &SqlitePool,
    resolution_row_id: i64,
) -> DbResult<Vec<MetadataResolutionFrameRow>> {
    let rows = sqlx::query_as::<_, MetadataResolutionFrameRow>(
        "SELECT resolution_row_id, frame_row_id, evidence_row_id, ordinal
         FROM session_metadata_resolution_frame
         WHERE resolution_row_id = ?
         ORDER BY ordinal ASC",
    )
    .bind(resolution_row_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
