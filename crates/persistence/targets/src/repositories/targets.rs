// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository query functions for the spec-023 target history + notes surface.
//!
//! Provides:
//! - [`list_sessions_for_target`] — acquisition sessions linked to a canonical
//!   target (migration 0046 `acquisition_session.canonical_target_id`).
//! - [`list_projects_for_target`] — projects linked to a canonical target
//!   (migration 0033 `projects.canonical_target_id`).
//! - [`get_target_notes`] — read the `canonical_target.notes` column
//!   (migration 0048).
//! - [`set_target_notes`] — upsert (UPDATE) `canonical_target.notes`.
//!
//! Constitution §I: read/write SQLite metadata only; no filesystem mutations.
//! Constitution §V: SQLite is the durable record.

use sqlx::SqlitePool;

use persistence_core::DbResult;

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat row returned by [`list_sessions_for_target`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TargetSessionRow {
    pub id: String,
    pub session_key: String,
    pub created_at: String,
    pub frame_count: i64,
}

/// Flat row returned by [`list_projects_for_target`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TargetProjectRow {
    pub id: String,
    pub name: String,
    pub lifecycle: String,
}

// ── Query functions ───────────────────────────────────────────────────────────

/// List acquisition sessions whose `canonical_target_id` matches `target_id`,
/// ordered by `created_at DESC` (newest first).
///
/// `json_array_length(frame_ids)` gives a zero-allocation frame count.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_sessions_for_target(
    pool: &SqlitePool,
    target_id: &str,
) -> DbResult<Vec<TargetSessionRow>> {
    let rows = sqlx::query_as::<_, TargetSessionRow>(
        "SELECT id,
                session_key,
                created_at,
                json_array_length(frame_ids) AS frame_count
         FROM acquisition_session
         WHERE canonical_target_id = ?
         ORDER BY created_at DESC",
    )
    .bind(target_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List projects whose `canonical_target_id` matches `target_id`,
/// ordered by `name ASC`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_projects_for_target(
    pool: &SqlitePool,
    target_id: &str,
) -> DbResult<Vec<TargetProjectRow>> {
    let rows = sqlx::query_as::<_, TargetProjectRow>(
        "SELECT id, name, lifecycle
         FROM projects
         WHERE canonical_target_id = ?
         ORDER BY name ASC",
    )
    .bind(target_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Read the `notes` column for a canonical target.
///
/// Returns `Ok(Some(notes))` when notes are set, `Ok(None)` when the column is
/// NULL or the target does not exist.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_target_notes(pool: &SqlitePool, target_id: &str) -> DbResult<Option<String>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT notes FROM canonical_target WHERE id = ?")
            .bind(target_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(n,)| n))
}

/// Write `notes` to `canonical_target.notes`.  Stores NULL when `notes` is
/// empty or whitespace-only (matches the nullable convention used elsewhere).
///
/// Returns `true` when the row was found and updated, `false` when no row
/// matched `target_id`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn set_target_notes(
    pool: &SqlitePool,
    target_id: &str,
    notes: Option<&str>,
) -> DbResult<bool> {
    let result = sqlx::query("UPDATE canonical_target SET notes = ? WHERE id = ?")
        .bind(notes)
        .bind(target_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::{insert_target, setup_db};

    // Helper: insert an acquisition_session linked to a canonical_target.
    async fn insert_linked_session(
        pool: &SqlitePool,
        session_id: &str,
        target_id: &str,
        created_at: &str,
    ) {
        sqlx::query(
            r#"INSERT INTO acquisition_session
               (id, session_key, frame_ids, created_at, canonical_target_id)
               VALUES (?, '{"target":"T","filter":"Ha","binning":"1","gain":"0","date":"2026-01-01"}',
                       '[1,2,3]', ?, ?)"#,
        )
        .bind(session_id)
        .bind(created_at)
        .bind(target_id)
        .execute(pool)
        .await
        .expect("insert_linked_session failed");
    }

    // Helper: insert a project linked to a canonical_target.
    // `project_id` is used as part of the path to satisfy the UNIQUE constraint.
    async fn insert_linked_project(
        pool: &SqlitePool,
        project_id: &str,
        name: &str,
        target_id: &str,
    ) {
        sqlx::query(
            "INSERT INTO projects
             (id, name, tool, lifecycle, path, canonical_target_id, channel_drift, created_at, updated_at)
             VALUES (?, ?, 'PixInsight', 'ready', ?, ?, 0,
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(project_id)
        .bind(name)
        .bind(format!("projects/{project_id}"))
        .bind(target_id)
        .execute(pool)
        .await
        .expect("insert_linked_project failed");
    }

    // ── list_sessions_for_target ──────────────────────────────────────────────

    #[tokio::test]
    async fn sessions_list_returns_linked_sessions_only() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-001").await;
        insert_target(db.pool(), "t-002").await;

        insert_linked_session(db.pool(), "s-001", "t-001", "2026-01-02T00:00:00Z").await;
        insert_linked_session(db.pool(), "s-002", "t-001", "2026-01-01T00:00:00Z").await;
        insert_linked_session(db.pool(), "s-003", "t-002", "2026-01-03T00:00:00Z").await;

        let rows = list_sessions_for_target(db.pool(), "t-001").await.unwrap();
        assert_eq!(rows.len(), 2, "only t-001 sessions should be returned");
        assert_eq!(rows[0].id, "s-001", "newest first");
        assert_eq!(rows[1].id, "s-002");
        assert_eq!(rows[0].frame_count, 3, "frame_count from json_array_length");
    }

    #[tokio::test]
    async fn sessions_list_empty_when_no_sessions_linked() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-010").await;
        let rows = list_sessions_for_target(db.pool(), "t-010").await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn sessions_list_empty_for_unknown_target() {
        let db = setup_db().await;
        let rows = list_sessions_for_target(db.pool(), "00000000-0000-0000-0000-000000000000")
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    // ── list_projects_for_target ──────────────────────────────────────────────

    #[tokio::test]
    async fn projects_list_returns_linked_projects_only() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-100").await;
        insert_target(db.pool(), "t-101").await;

        insert_linked_project(db.pool(), "p-001", "Bravo", "t-100").await;
        insert_linked_project(db.pool(), "p-002", "Alpha", "t-100").await;
        insert_linked_project(db.pool(), "p-003", "Gamma", "t-101").await;

        let rows = list_projects_for_target(db.pool(), "t-100").await.unwrap();
        assert_eq!(rows.len(), 2, "only t-100 projects");
        assert_eq!(rows[0].name, "Alpha", "ordered by name asc");
        assert_eq!(rows[1].name, "Bravo");
        assert_eq!(rows[0].lifecycle, "ready");
    }

    #[tokio::test]
    async fn projects_list_empty_when_no_projects_linked() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-110").await;
        let rows = list_projects_for_target(db.pool(), "t-110").await.unwrap();
        assert!(rows.is_empty());
    }

    // ── get_target_notes / set_target_notes ───────────────────────────────────

    #[tokio::test]
    async fn notes_get_returns_none_when_not_set() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-200").await;
        let notes = get_target_notes(db.pool(), "t-200").await.unwrap();
        assert!(notes.is_none());
    }

    #[tokio::test]
    async fn notes_get_returns_none_for_unknown_target() {
        let db = setup_db().await;
        let notes =
            get_target_notes(db.pool(), "00000000-0000-0000-0000-000000000099").await.unwrap();
        assert!(notes.is_none());
    }

    #[tokio::test]
    async fn notes_roundtrip_set_and_get() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-300").await;

        let updated =
            set_target_notes(db.pool(), "t-300", Some("My observing note.")).await.unwrap();
        assert!(updated, "should have found and updated the row");

        let notes = get_target_notes(db.pool(), "t-300").await.unwrap();
        assert_eq!(notes.as_deref(), Some("My observing note."));
    }

    #[tokio::test]
    async fn notes_set_null_clears_note() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-301").await;

        set_target_notes(db.pool(), "t-301", Some("Initial note.")).await.unwrap();
        set_target_notes(db.pool(), "t-301", None).await.unwrap();

        let notes = get_target_notes(db.pool(), "t-301").await.unwrap();
        assert!(notes.is_none(), "note should be cleared to NULL");
    }

    #[tokio::test]
    async fn notes_set_returns_false_for_unknown_target() {
        let db = setup_db().await;
        let updated =
            set_target_notes(db.pool(), "00000000-0000-0000-0000-000000000099", Some("x"))
                .await
                .unwrap();
        assert!(!updated, "no row matched → false");
    }
}
