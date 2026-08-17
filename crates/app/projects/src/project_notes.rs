// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Application use cases for project notes (spec 024).
//!
//! ## Entry points
//!
//! - [`update_note`]        — replace the project's notes body (max 16 384 bytes).
//! - [`get_note_content`]   — retrieve the current notes body for a project.
//! - [`sync_notes_to_disk`] — sync notes from DB to disk (projection write).
//!
//! ## Architecture
//!
//! Notes content is stored in the `project_notes` DB table (durable record,
//! Constitution V). An on-disk projection at `<project_root>/notes/project-notes.md`
//! is written via [`project_structure::notes::RealNotesAdapter`].
//!
//! The `project.read_only` error fires only when `lifecycle == "archived"`
//! (R-NotesEdit, ratified 2026-05-22).
//!
//! ## Audit events
//!
//! `note.update` is emitted on every successful save.

use std::path::Path;

use audit::bus::EventBus;
use audit::event_bus::Source;
use sqlx::SqlitePool;

use contracts_core::manifests::{
    ManifestOpError, ProjectNoteUpdateRequest, ProjectNoteUpdateResult,
};
use domain_core::project::validate::is_read_only;
use persistence_plans::repositories::project_notes::{get_note, upsert_note};
use persistence_plans::repositories::projects::get_project;
use project_structure::notes::{NotesFileAdapter, RealNotesAdapter};

// ── Constants ─────────────────────────────────────────────────────────────────

const MAX_NOTE_BYTES: usize = 16_384;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn new_uuid() -> String {
    domain_core::ids::new_id()
}

fn op_error(code: &str, message: &str) -> ManifestOpError {
    ManifestOpError { code: code.to_owned(), message: message.to_owned(), details: None }
}

// ── update_note ───────────────────────────────────────────────────────────────

/// Replace the project's notes body.
///
/// - Content is capped at 16 384 UTF-8 bytes (A5).
/// - Returns `project.read_only` when lifecycle is `"archived"` (R-NotesEdit).
/// - Writes the DB row and emits a `note.update` audit event on success.
/// - On-disk file is written as a best-effort projection via the adapter.
///
/// # Errors
/// Returns [`ManifestOpError`] with codes:
/// - `"project.not_found"` — project does not exist.
/// - `"project.read_only"` — project lifecycle is `"archived"`.
/// - `"note.content_too_large"` — content exceeds 16 384 bytes.
/// - `"internal"` — database or file I/O failure.
pub async fn update_note(
    pool: &SqlitePool,
    bus: &EventBus,
    req: ProjectNoteUpdateRequest,
    project_root: Option<&Path>,
) -> Result<ProjectNoteUpdateResult, ManifestOpError> {
    update_note_with_adapter(pool, bus, req, project_root, &RealNotesAdapter).await
}

/// Internal implementation accepting an injected adapter for testability.
///
/// # Errors
/// Same error codes as [`update_note`]: `"project.not_found"`, `"project.read_only"`,
/// `"note.content_too_large"`, `"internal"`.
pub async fn update_note_with_adapter(
    pool: &SqlitePool,
    bus: &EventBus,
    req: ProjectNoteUpdateRequest,
    project_root: Option<&Path>,
    adapter: &dyn NotesFileAdapter,
) -> Result<ProjectNoteUpdateResult, ManifestOpError> {
    // ── Content size guard (A5) ───────────────────────────────────────────────
    if req.content.len() > MAX_NOTE_BYTES {
        return Err(op_error(
            "note.content_too_large",
            &format!(
                "Note body exceeds the 16 384-byte limit ({} bytes supplied).",
                req.content.len()
            ),
        ));
    }

    // ── Fetch project to verify existence and lifecycle ───────────────────────
    let project = get_project(pool, &req.project_id).await.map_err(|e| {
        if matches!(e, persistence_core::DbError::NotFound(_)) {
            op_error("project.not_found", &format!("project {} not found", req.project_id))
        } else {
            op_error("internal", &format!("DB error: {e}"))
        }
    })?;

    // ── Lifecycle guard (R-NotesEdit) ─────────────────────────────────────────
    if is_read_only(&project.lifecycle) {
        return Err(op_error(
            "project.read_only",
            "Notes cannot be edited on an archived project.",
        ));
    }

    // ── Upsert the DB row ─────────────────────────────────────────────────────
    let note_id = match get_note(pool, &req.project_id).await {
        Ok(Some(existing)) => existing.id,
        _ => new_uuid(),
    };

    let updated_at = upsert_note(pool, &note_id, &req.project_id, &req.content)
        .await
        .map_err(|e| op_error("internal", &format!("DB error: {e}")))?;

    // ── Write on-disk projection (best-effort) ────────────────────────────────
    if let Some(root) = project_root {
        if let Err(e) = adapter.write(root, &req.content).await {
            // Non-fatal: DB is the durable record. Log but don't fail.
            tracing::warn!(project_id = req.project_id, error = %e, "project_notes: disk write failed");
        }
    }

    // ── Audit event ───────────────────────────────────────────────────────────
    let _ = bus
        .publish(
            "note.update",
            Source::User,
            serde_json::json!({
                "projectId": req.project_id,
                "updatedAt": updated_at,
            }),
        )
        .await;

    Ok(ProjectNoteUpdateResult { project_id: req.project_id, updated_at })
}

// ── get_note_content ──────────────────────────────────────────────────────────

/// Retrieve the current notes content for a project.
///
/// Returns `None` when no note has been saved yet.
///
/// # Errors
/// Returns a string on database failure.
pub async fn get_note_content(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Option<String>, String> {
    persistence_plans::repositories::project_notes::get_note_content(pool, project_id)
        .await
        .map_err(|e| e.to_string())
}

// ── sync_notes_to_disk ────────────────────────────────────────────────────────

/// Sync project notes from the database to disk.
///
/// Reads the note for `project_id` from the database and writes it to
/// `notes_dir/project-notes.md`. Creates the directory if it doesn't exist.
///
/// Returns the number of files written (0 or 1).
///
/// # Errors
/// Returns an error string if the DB query fails, the notes directory cannot
/// be created, or a file write fails.
pub async fn sync_notes_to_disk(
    pool: &SqlitePool,
    project_id: &str,
    notes_dir: &Path,
) -> Result<u32, String> {
    tokio::fs::create_dir_all(notes_dir)
        .await
        .map_err(|e| format!("failed to create notes directory {}: {e}", notes_dir.display()))?;

    let content =
        persistence_plans::repositories::project_notes::get_note_content(pool, project_id)
            .await
            .map_err(|e| format!("DB error reading notes for {project_id}: {e}"))?;

    match content {
        None => {
            tracing::debug!(project_id, "sync_notes_to_disk: no notes for project");
            Ok(0)
        }
        Some(body) => {
            let target = notes_dir.join(project_structure::notes::NOTES_FILENAME);
            tokio::fs::write(&target, body.as_bytes())
                .await
                .map_err(|e| format!("write notes {}: {e}", target.display()))?;
            Ok(1)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../../crates/persistence/core/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_project(pool: &SqlitePool, id: &str, lifecycle: &str) {
        sqlx::query(
            "INSERT INTO projects \
             (id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at) \
             VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind("Test")
        .bind("PixInsight")
        .bind(lifecycle)
        .bind("projects/test")
        .bind::<Option<String>>(None)
        .bind(false)
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();
    }

    fn make_bus(pool: &SqlitePool) -> EventBus {
        EventBus::with_pool(pool.clone())
    }

    #[tokio::test]
    async fn update_note_persists_and_returns_updated_at() {
        let pool = setup().await;
        insert_project(&pool, "proj-n1", "ready").await;
        let bus = make_bus(&pool);

        let result = update_note(
            &pool,
            &bus,
            ProjectNoteUpdateRequest {
                project_id: "proj-n1".to_owned(),
                content: "My notes content".to_owned(),
            },
            None,
        )
        .await
        .unwrap();

        assert!(!result.updated_at.is_empty());
        assert_eq!(result.project_id, "proj-n1");

        let content = get_note_content(&pool, "proj-n1").await.unwrap();
        assert_eq!(content.as_deref(), Some("My notes content"));
    }

    #[tokio::test]
    async fn update_note_empty_string_stores_empty() {
        let pool = setup().await;
        insert_project(&pool, "proj-n2", "ready").await;
        let bus = make_bus(&pool);

        update_note(
            &pool,
            &bus,
            ProjectNoteUpdateRequest {
                project_id: "proj-n2".to_owned(),
                content: "Initial".to_owned(),
            },
            None,
        )
        .await
        .unwrap();

        update_note(
            &pool,
            &bus,
            ProjectNoteUpdateRequest { project_id: "proj-n2".to_owned(), content: String::new() },
            None,
        )
        .await
        .unwrap();

        let content = get_note_content(&pool, "proj-n2").await.unwrap();
        // Empty string is stored (not null).
        assert_eq!(content.as_deref(), Some(""));
    }

    #[tokio::test]
    async fn update_note_too_long_returns_error() {
        let pool = setup().await;
        insert_project(&pool, "proj-n3", "ready").await;
        let bus = make_bus(&pool);

        let big_content = "x".repeat(MAX_NOTE_BYTES + 1);
        let err = update_note(
            &pool,
            &bus,
            ProjectNoteUpdateRequest { project_id: "proj-n3".to_owned(), content: big_content },
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "note.content_too_large");

        // Nothing should be written.
        let content = get_note_content(&pool, "proj-n3").await.unwrap();
        assert!(content.is_none());
    }

    #[tokio::test]
    async fn update_note_not_found_returns_error() {
        let pool = setup().await;
        let bus = make_bus(&pool);
        let err = update_note(
            &pool,
            &bus,
            ProjectNoteUpdateRequest {
                project_id: "no-such-project".to_owned(),
                content: "Hello".to_owned(),
            },
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "project.not_found");
    }

    #[tokio::test]
    async fn update_note_archived_returns_read_only() {
        let pool = setup().await;
        insert_project(&pool, "proj-arch", "archived").await;
        let bus = make_bus(&pool);

        let err = update_note(
            &pool,
            &bus,
            ProjectNoteUpdateRequest {
                project_id: "proj-arch".to_owned(),
                content: "Should be blocked".to_owned(),
            },
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "project.read_only");
    }

    #[tokio::test]
    async fn update_note_completed_lifecycle_succeeds() {
        let pool = setup().await;
        insert_project(&pool, "proj-done", "completed").await;
        let bus = make_bus(&pool);

        let result = update_note(
            &pool,
            &bus,
            ProjectNoteUpdateRequest {
                project_id: "proj-done".to_owned(),
                content: "Completed project notes".to_owned(),
            },
            None,
        )
        .await;
        assert!(result.is_ok(), "completed lifecycle should allow note edits");
    }

    #[tokio::test]
    async fn sync_notes_to_disk_writes_file() {
        let pool = setup().await;
        insert_project(&pool, "proj-sync", "ready").await;
        let bus = make_bus(&pool);

        update_note(
            &pool,
            &bus,
            ProjectNoteUpdateRequest {
                project_id: "proj-sync".to_owned(),
                content: "Sync test".to_owned(),
            },
            None,
        )
        .await
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let count = sync_notes_to_disk(&pool, "proj-sync", dir.path()).await.unwrap();
        assert_eq!(count, 1);
        let content = tokio::fs::read_to_string(dir.path().join("project-notes.md")).await.unwrap();
        assert_eq!(content, "Sync test");
    }

    #[tokio::test]
    async fn sync_notes_to_disk_returns_zero_when_no_notes() {
        let pool = setup().await;
        insert_project(&pool, "proj-nosync", "ready").await;

        let dir = tempfile::tempdir().unwrap();
        let count = sync_notes_to_disk(&pool, "proj-nosync", dir.path()).await.unwrap();
        assert_eq!(count, 0);
    }
}
