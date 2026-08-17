// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for the `source_availability_rollup` table.
//!
//! The rollup is a rebuildable projection over indexed file records. It is not
//! part of the immutable session membership; it changes whenever the filesystem
//! scanner observes new or missing frames. The `observed_at` timestamp records
//! the projection instant used for candidate-list watermarking.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

// ── Row projections ────────────────────────────────────────────────────────────

/// One `source_availability_rollup` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SourceAvailabilityRollupRow {
    pub session_row_id: i64,
    pub indexed_frame_count: i64,
    pub available_frame_count: i64,
    pub readable_frame_count: i64,
    pub source_byte_count: i64,
    pub observed_at: String,
}

// ── Writes ────────────────────────────────────────────────────────────────────

/// Upsert the availability rollup for one session.
///
/// Overwrites the existing row if present. Callers invoke this from the
/// filesystem scan completion path.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn upsert_source_availability(
    conn: &mut SqliteConnection,
    session_row_id: i64,
    indexed_frame_count: i64,
    available_frame_count: i64,
    readable_frame_count: i64,
    source_byte_count: i64,
    observed_at: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO source_availability_rollup
             (session_row_id, indexed_frame_count, available_frame_count,
              readable_frame_count, source_byte_count, observed_at)
         VALUES (?,?,?,?,?,?)
         ON CONFLICT(session_row_id) DO UPDATE SET
             indexed_frame_count   = excluded.indexed_frame_count,
             available_frame_count = excluded.available_frame_count,
             readable_frame_count  = excluded.readable_frame_count,
             source_byte_count     = excluded.source_byte_count,
             observed_at           = excluded.observed_at",
    )
    .bind(session_row_id)
    .bind(indexed_frame_count)
    .bind(available_frame_count)
    .bind(readable_frame_count)
    .bind(source_byte_count)
    .bind(observed_at)
    .execute(conn)
    .await?;
    Ok(())
}

// ── Reads ─────────────────────────────────────────────────────────────────────

/// Fetch the availability rollup for one session.
///
/// Returns `None` when no rollup has been written yet for the session.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn get_source_availability(
    pool: &SqlitePool,
    session_row_id: i64,
) -> DbResult<Option<SourceAvailabilityRollupRow>> {
    sqlx::query_as::<_, SourceAvailabilityRollupRow>(
        "SELECT session_row_id, indexed_frame_count, available_frame_count,
                readable_frame_count, source_byte_count, observed_at
         FROM source_availability_rollup
         WHERE session_row_id = ?",
    )
    .bind(session_row_id)
    .fetch_optional(pool)
    .await
    .map_err(DbError::from)
}

/// List availability rollups for multiple sessions by `session_row_id`.
///
/// Returned in the same order as the input slice. Sessions with no rollup row
/// are absent from the result — callers should treat absence as
/// `indexed_frame_count = 0`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_source_availability_for_sessions(
    pool: &SqlitePool,
    session_row_ids: &[i64],
) -> DbResult<Vec<SourceAvailabilityRollupRow>> {
    if session_row_ids.is_empty() {
        return Ok(Vec::new());
    }
    // Use QueryBuilder to avoid the 'static lifetime requirement on query_as.
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT session_row_id, indexed_frame_count, available_frame_count, \
         readable_frame_count, source_byte_count, observed_at \
         FROM source_availability_rollup \
         WHERE session_row_id IN (",
    );
    let mut separated = builder.separated(", ");
    for id in session_row_ids {
        separated.push_bind(*id);
    }
    separated.push_unseparated(") ORDER BY session_row_id ASC");
    builder
        .build_query_as::<SourceAvailabilityRollupRow>()
        .fetch_all(pool)
        .await
        .map_err(DbError::from)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_db_with_session() -> (sqlx::SqlitePool, i64) {
        let db = persistence_core::Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        let pool = db.pool().clone();
        let ts = "2026-07-22T00:00:00.000000Z";

        // Minimal FK chain for a single session row
        sqlx::query(
            "INSERT INTO spec062_actor VALUES (1,'00000000-0000-7000-c000-000000000001',?)",
        )
        .bind(ts)
        .execute(&pool)
        .await
        .expect("actor");
        sqlx::query("INSERT INTO spec062_config_revision VALUES (1,'00000000-0000-7000-c000-000000000002',1,'cfg-digest',?)").bind(ts).execute(&pool).await.expect("config");
        sqlx::query("INSERT INTO repository_change(command_row_id,created_at) VALUES (NULL,?)")
            .bind(ts)
            .execute(&pool)
            .await
            .expect("repo_change");
        sqlx::query("INSERT INTO command_execution (row_id,public_id,actor_row_id,operation,canonical_payload_digest,state,response_json,created_at,finished_at) VALUES (1,'00000000-0000-7000-c000-000000000003',1,'inbox.materialization.apply','pd','applied','{}',?,?)").bind(ts).bind(ts).execute(&pool).await.expect("command");
        sqlx::query("INSERT INTO session_materialization_operation (row_id,public_id,kind,command_row_id,config_revision_row_id,state,created_sequence,created_at) VALUES (1,'00000000-0000-7000-c000-000000000004','inbox_ingestion',1,1,'ready',1,?)").bind(ts).execute(&pool).await.expect("operation");
        sqlx::query("INSERT INTO session (row_id,public_id,materialization_operation_row_id,kind,ordinal_in_operation,identity_digest,observing_night_date,night_derivation,created_sequence,created_at) VALUES (1,'ses-pub-001',1,'dark',0,'dark-id-001','2026-01-15','reviewed_local_fallback',1,?)").bind(ts).execute(&pool).await.expect("session");

        (pool, 1i64)
    }

    #[tokio::test]
    async fn upsert_and_get_availability() {
        let (pool, session_row_id) = setup_db_with_session().await;

        let mut conn = pool.acquire().await.expect("conn");
        upsert_source_availability(
            &mut conn,
            session_row_id,
            120, // indexed
            100, // available
            95,  // readable
            1_234_567,
            "2026-07-22T01:00:00.000000Z",
        )
        .await
        .expect("upsert");

        let row = get_source_availability(&pool, session_row_id).await.expect("get").expect("Some");
        assert_eq!(row.indexed_frame_count, 120);
        assert_eq!(row.available_frame_count, 100);
        assert_eq!(row.readable_frame_count, 95);
        assert_eq!(row.source_byte_count, 1_234_567);
    }

    #[tokio::test]
    async fn upsert_overwrites_existing() {
        let (pool, session_row_id) = setup_db_with_session().await;

        let mut conn = pool.acquire().await.expect("conn");
        upsert_source_availability(
            &mut conn,
            session_row_id,
            100,
            100,
            100,
            1_000,
            "2026-07-22T01:00:00.000000Z",
        )
        .await
        .expect("first upsert");
        upsert_source_availability(
            &mut conn,
            session_row_id,
            50,
            40,
            40,
            500,
            "2026-07-22T02:00:00.000000Z",
        )
        .await
        .expect("second upsert");

        let row = get_source_availability(&pool, session_row_id).await.expect("get").expect("Some");
        assert_eq!(row.indexed_frame_count, 50);
        assert_eq!(row.observed_at, "2026-07-22T02:00:00.000000Z");
    }

    #[tokio::test]
    async fn get_returns_none_when_absent() {
        let (pool, _) = setup_db_with_session().await;
        let row = get_source_availability(&pool, 999).await.expect("query");
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn list_for_empty_slice_returns_empty() {
        let (pool, _) = setup_db_with_session().await;
        let rows = list_source_availability_for_sessions(&pool, &[]).await.expect("list");
        assert!(rows.is_empty());
    }
}
