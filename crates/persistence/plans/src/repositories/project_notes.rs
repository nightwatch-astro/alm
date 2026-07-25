// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for the `project_notes` table (spec 024).
//!
//! Operates on the table from migration 0028.
//!
//! One note per project. The `content` column holds the full UTF-8 markdown
//! body. The app layer enforces the 16 384-byte cap before calling here.
//!
//! Constitution V: SQLite row is the durable record; on-disk markdown file is
//! the projection written by the notes adapter.

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::DbResult;

// ── Helpers ───────────────────────────────────────────────────────────────────

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat row from the `project_notes` table.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ProjectNoteRow {
    pub id: String,
    pub project_id: String,
    pub updated_at: String,
    pub content: String,
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

/// Upsert the note for a project.
///
/// If no note row exists for `project_id`, one is created.
/// Returns the new `updated_at` timestamp.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn upsert_note(
    pool: &SqlitePool,
    id: &str,
    project_id: &str,
    content: &str,
) -> DbResult<String> {
    let updated_at = Timestamp::now_iso();
    sqlx::query(
        "\
        INSERT INTO project_notes (id, project_id, updated_at, content)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(project_id) DO UPDATE SET
            updated_at = excluded.updated_at,
            content    = excluded.content
        ",
    )
    .bind(id)
    .bind(project_id)
    .bind(&updated_at)
    .bind(content)
    .execute(pool)
    .await?;
    Ok(updated_at)
}

/// Get the note for a project.
///
/// Returns `Ok(None)` when no note row exists yet (project has never had notes
/// saved; callers treat this as empty content).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_note(pool: &SqlitePool, project_id: &str) -> DbResult<Option<ProjectNoteRow>> {
    let row: Option<ProjectNoteRow> = sqlx::query_as(
        "\
        SELECT id, project_id, updated_at, content
        FROM project_notes
        WHERE project_id = ?
        ",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Get only the content string for a project's notes (lightweight read).
///
/// Returns `None` when no note has been saved yet.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_note_content(pool: &SqlitePool, project_id: &str) -> DbResult<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT content FROM project_notes WHERE project_id = ?")
            .bind(project_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(c,)| c))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup() -> SqlitePool {
        let db = persistence_core::test_support::setup_db().await;
        db.pool().clone()
    }

    use persistence_core::test_support::insert_project;

    #[tokio::test]
    async fn upsert_note_creates_new_row() {
        let pool = setup().await;
        insert_project(&pool, "proj-n1").await;

        let ts = upsert_note(&pool, "note-001", "proj-n1", "Hello world").await.unwrap();
        assert!(!ts.is_empty());

        let row = get_note(&pool, "proj-n1").await.unwrap().unwrap();
        assert_eq!(row.content, "Hello world");
        assert_eq!(row.project_id, "proj-n1");
    }

    #[tokio::test]
    async fn upsert_note_updates_existing_row() {
        let pool = setup().await;
        insert_project(&pool, "proj-n2").await;

        upsert_note(&pool, "note-002", "proj-n2", "First").await.unwrap();
        upsert_note(&pool, "note-003", "proj-n2", "Second").await.unwrap();

        let row = get_note(&pool, "proj-n2").await.unwrap().unwrap();
        assert_eq!(row.content, "Second");
    }

    #[tokio::test]
    async fn get_note_returns_none_when_absent() {
        let pool = setup().await;
        insert_project(&pool, "proj-n3").await;

        let row = get_note(&pool, "proj-n3").await.unwrap();
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn get_note_content_returns_content_string() {
        let pool = setup().await;
        insert_project(&pool, "proj-n4").await;
        upsert_note(&pool, "note-004", "proj-n4", "Test content").await.unwrap();

        let content = get_note_content(&pool, "proj-n4").await.unwrap();
        assert_eq!(content.as_deref(), Some("Test content"));
    }
}
