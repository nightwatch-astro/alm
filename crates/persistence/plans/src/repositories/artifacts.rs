// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for `processing_artifacts` + `classification_overrides`
//! tables (spec 012 T006).
//!
//! Operates on the tables from migration 0025.
//! Constitution V: SQLite is the durable record; observed files remain on
//! the user's disk untouched.
//!
//! ## Key invariants
//! - `(project_id, path)` is UNIQUE; detected file updates are in-place (A8).
//! - Manual override rows in `classification_overrides` are sticky; they are
//!   deleted only by an explicit `clear_override` call (A6).
//! - `tool_launch_id` is set by the attribution pass and updated by re-attribution
//!   (T022/T022b). `tool_launches.completed_at` is set here when the run completes
//!   (T022c).

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::DbResult;

// ── Helpers ───────────────────────────────────────────────────────────────────

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat row from the `processing_artifacts` table.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ArtifactRow {
    pub id: String,
    pub project_id: String,
    pub tool_launch_id: Option<String>,
    pub path: String,
    pub kind: String,
    pub tool: String,
    pub detected_at: String,
    pub last_seen_at: String,
    pub state: String,
    pub classification_confidence: f64,
    pub classification_source: String,
    pub size_bytes: i64,
    pub file_mtime: String,
    pub content_hash: Option<String>,
}

/// Data needed to insert a new `processing_artifacts` row.
#[derive(Clone, Debug)]
pub struct InsertArtifact<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub tool_launch_id: Option<&'a str>,
    pub path: &'a str,
    pub kind: &'a str,
    pub tool: &'a str,
    pub detected_at: &'a str,
    pub state: &'a str,
    pub classification_confidence: f64,
    pub classification_source: &'a str,
    pub size_bytes: i64,
    pub file_mtime: &'a str,
    pub content_hash: Option<&'a str>,
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

/// Insert a new `processing_artifacts` row, returning `None` when the row was
/// silently skipped by `INSERT OR IGNORE` (any constraint violation — UNIQUE,
/// CHECK, or NOT NULL — causes SQLite to ignore the row rather than error).
///
/// # Returns
/// - `Ok(Some(id))` — row was inserted.
/// - `Ok(None)` — constraint violation silenced the insert (no row written).
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on any DB failure that is not a
/// constraint violation (e.g. I/O errors, schema mismatches).
pub async fn insert_artifact_if_absent(
    pool: &SqlitePool,
    data: InsertArtifact<'_>,
) -> DbResult<Option<String>> {
    let now = data.detected_at.to_owned();
    let result = sqlx::query(
        "\
        INSERT OR IGNORE INTO processing_artifacts
            (id, project_id, tool_launch_id, path, kind, tool,
             detected_at, last_seen_at, state,
             classification_confidence, classification_source,
             size_bytes, file_mtime, content_hash)
        VALUES (?,?,?,?,?,?, ?,?,?, ?,?, ?,?,?)
        ",
    )
    .bind(data.id)
    .bind(data.project_id)
    .bind(data.tool_launch_id)
    .bind(data.path)
    .bind(data.kind)
    .bind(data.tool)
    .bind(&now)
    .bind(&now)
    .bind(data.state)
    .bind(data.classification_confidence)
    .bind(data.classification_source)
    .bind(data.size_bytes)
    .bind(data.file_mtime)
    .bind(data.content_hash)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        Ok(None)
    } else {
        Ok(Some(data.id.to_owned()))
    }
}

/// Lookup an artifact by `(project_id, path)`.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn get_artifact_by_path(
    pool: &SqlitePool,
    project_id: &str,
    path: &str,
) -> DbResult<Option<ArtifactRow>> {
    let row = sqlx::query_as::<_, ArtifactRow>(
        "SELECT * FROM processing_artifacts WHERE project_id = ? AND path = ?",
    )
    .bind(project_id)
    .bind(path)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// List all artifact rows for a project, ordered by `detected_at DESC`.
///
/// `include_states` filters by state; if empty, returns all states.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn list_artifacts_for_project(
    pool: &SqlitePool,
    project_id: &str,
    include_states: &[&str],
) -> DbResult<Vec<ArtifactRow>> {
    if include_states.is_empty() {
        // Return all states.
        let rows = sqlx::query_as::<_, ArtifactRow>(
            "SELECT * FROM processing_artifacts WHERE project_id = ? ORDER BY detected_at DESC",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        return Ok(rows);
    }

    // SQLite does not support array binding; use a fixed-max approach instead.
    // The state values are validated by the DB CHECK constraint; the caller
    // supplies only known-good values from the state enum.
    // We support up to 3 states (present, missing, user_resolved_missing).
    let rows = sqlx::query_as::<_, ArtifactRow>(
        r"SELECT * FROM processing_artifacts
           WHERE project_id = ?
             AND state IN (
               SELECT value FROM json_each(?)
             )
           ORDER BY detected_at DESC",
    )
    .bind(project_id)
    .bind(serde_json::to_string(include_states).unwrap_or_else(|_| "[]".to_owned()))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Update `last_seen_at` for a `present` artifact (reconcile seen pass).
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn touch_artifact(pool: &SqlitePool, artifact_id: &str) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query("UPDATE processing_artifacts SET last_seen_at = ? WHERE id = ?")
        .bind(&now)
        .bind(artifact_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Transition an artifact to `missing` state.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn mark_artifact_missing(pool: &SqlitePool, artifact_id: &str) -> DbResult<()> {
    sqlx::query("UPDATE processing_artifacts SET state = 'missing' WHERE id = ?")
        .bind(artifact_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Transition an artifact from `missing` back to `present` and refresh size/hash.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn mark_artifact_recovered(
    pool: &SqlitePool,
    artifact_id: &str,
    size_bytes: i64,
    content_hash: Option<&str>,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "\
        UPDATE processing_artifacts
           SET state = 'present', last_seen_at = ?, size_bytes = ?, content_hash = ?
         WHERE id = ?
        ",
    )
    .bind(&now)
    .bind(size_bytes)
    .bind(content_hash)
    .bind(artifact_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a `missing` artifact as user-resolved.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn mark_artifact_user_resolved(pool: &SqlitePool, artifact_id: &str) -> DbResult<()> {
    sqlx::query(
        "UPDATE processing_artifacts SET state = 'user_resolved_missing' WHERE id = ? AND state = 'missing'"
    )
    .bind(artifact_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update classification on an artifact (auto re-classification after override cleared, A6).
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn update_classification(
    pool: &SqlitePool,
    artifact_id: &str,
    kind: &str,
    confidence: f64,
    source: &str,
) -> DbResult<()> {
    sqlx::query(
        "\
        UPDATE processing_artifacts
           SET kind = ?, classification_confidence = ?, classification_source = ?
         WHERE id = ?
        ",
    )
    .bind(kind)
    .bind(confidence)
    .bind(source)
    .bind(artifact_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Minimal `(id, project_id, path)` view of an artifact row, used by the
/// spec 012 WP-012-A one-time re-attribution fix-up to find rows whose
/// `project_id` was mistakenly set to a library-root id.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ArtifactIdentityRow {
    pub id: String,
    pub project_id: String,
    pub path: String,
}

/// List every artifact's `(id, project_id, path)` (WP-012-A re-attribution
/// fix-up input; small enough table that a full scan is fine).
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn list_all_artifact_identities(pool: &SqlitePool) -> DbResult<Vec<ArtifactIdentityRow>> {
    let rows = sqlx::query_as::<_, ArtifactIdentityRow>(
        "SELECT id, project_id, path FROM processing_artifacts",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Update only the `project_id` of an artifact row (WP-012-A re-attribution
/// fix-up). Leaves every other field untouched.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn set_project_id(
    pool: &SqlitePool,
    artifact_id: &str,
    project_id: &str,
) -> DbResult<()> {
    sqlx::query("UPDATE processing_artifacts SET project_id = ? WHERE id = ?")
        .bind(project_id)
        .bind(artifact_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update `tool_launch_id` for attribution (T022/T022b).
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn set_tool_launch_id(
    pool: &SqlitePool,
    artifact_id: &str,
    tool_launch_id: &str,
) -> DbResult<()> {
    sqlx::query("UPDATE processing_artifacts SET tool_launch_id = ? WHERE id = ?")
        .bind(tool_launch_id)
        .bind(artifact_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update the in-place rerun fields (A8): `content_hash`, `size_bytes`, `last_seen_at`.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn update_artifact_inplace(
    pool: &SqlitePool,
    artifact_id: &str,
    size_bytes: i64,
    content_hash: Option<&str>,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "\
        UPDATE processing_artifacts
           SET size_bytes = ?, content_hash = ?, last_seen_at = ?
         WHERE id = ?
        ",
    )
    .bind(size_bytes)
    .bind(content_hash)
    .bind(&now)
    .bind(artifact_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── classification_overrides ──────────────────────────────────────────────────

/// Flat row from `classification_overrides`.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct OverrideRow {
    pub artifact_id: String,
    pub kind: String,
    pub created_at: String,
    pub reason: Option<String>,
}

/// Insert or replace a manual classification override (T014).
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn upsert_override(
    pool: &SqlitePool,
    artifact_id: &str,
    kind: &str,
    reason: Option<&str>,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "\
        INSERT INTO classification_overrides (artifact_id, kind, created_at, reason)
        VALUES (?,?,?,?)
        ON CONFLICT(artifact_id) DO UPDATE SET kind=excluded.kind, created_at=excluded.created_at, reason=excluded.reason
        ",
    )
    .bind(artifact_id)
    .bind(kind)
    .bind(&now)
    .bind(reason)
    .execute(pool)
    .await?;

    // Also update the main row so it reflects manual_override immediately.
    update_classification(pool, artifact_id, kind, 1.0, "manual_override").await?;
    Ok(())
}

/// Delete the manual override and return the deleted row if any (A6 clear path).
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn clear_override(pool: &SqlitePool, artifact_id: &str) -> DbResult<Option<OverrideRow>> {
    let prior = sqlx::query_as::<_, OverrideRow>(
        "SELECT * FROM classification_overrides WHERE artifact_id = ?",
    )
    .bind(artifact_id)
    .fetch_optional(pool)
    .await?;

    if prior.is_some() {
        sqlx::query("DELETE FROM classification_overrides WHERE artifact_id = ?")
            .bind(artifact_id)
            .execute(pool)
            .await?;
    }
    Ok(prior)
}

// ── tool_launches completion (T022c) ──────────────────────────────────────────

/// Set `tool_launches.completed_at` when the attribution pass determines a run
/// is terminal (T022c). Only updates rows where `completed_at` is currently NULL.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn complete_tool_launch(
    pool: &SqlitePool,
    tool_launch_id: &str,
    completed_at: &str,
) -> DbResult<bool> {
    let result = sqlx::query(
        "UPDATE tool_launches SET completed_at = ? WHERE id = ? AND completed_at IS NULL",
    )
    .bind(completed_at)
    .bind(tool_launch_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// List artifact ids attributed to a given tool launch.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on query failure.
pub async fn list_artifact_ids_for_launch(
    pool: &SqlitePool,
    tool_launch_id: &str,
) -> DbResult<Vec<String>> {
    #[derive(sqlx::FromRow)]
    struct IdRow {
        id: String,
    }
    let rows =
        sqlx::query_as::<_, IdRow>("SELECT id FROM processing_artifacts WHERE tool_launch_id = ?")
            .bind(tool_launch_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;

    fn art(id: &str, project: &str, path: &str, kind: &str) -> InsertArtifact<'static> {
        InsertArtifact {
            id: Box::leak(id.to_owned().into_boxed_str()),
            project_id: Box::leak(project.to_owned().into_boxed_str()),
            tool_launch_id: None,
            path: Box::leak(path.to_owned().into_boxed_str()),
            kind: Box::leak(kind.to_owned().into_boxed_str()),
            tool: "pixinsight",
            detected_at: "2026-06-01T10:00:00Z",
            state: "present",
            classification_confidence: 0.95,
            classification_source: "rule",
            size_bytes: 1024,
            file_mtime: "2026-06-01T09:55:00Z",
            content_hash: None,
        }
    }

    #[tokio::test]
    async fn insert_and_lookup_by_path() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        insert_artifact_if_absent(pool, art("art-1", "proj-1", "output/MasterDark.xisf", "master"))
            .await
            .unwrap();

        let row = get_artifact_by_path(pool, "proj-1", "output/MasterDark.xisf")
            .await
            .unwrap()
            .expect("row should exist");
        assert_eq!(row.id, "art-1");
        assert_eq!(row.kind, "master");
        assert_eq!(row.state, "present");
    }

    #[tokio::test]
    async fn list_filters_by_state() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        insert_artifact_if_absent(pool, art("a1", "p1", "out/a.xisf", "intermediate"))
            .await
            .unwrap();
        insert_artifact_if_absent(pool, art("a2", "p1", "out/b.xisf", "master")).await.unwrap();

        // Transition a2 to missing.
        mark_artifact_missing(pool, "a2").await.unwrap();

        let present = list_artifacts_for_project(pool, "p1", &["present"]).await.unwrap();
        assert_eq!(present.len(), 1);
        assert_eq!(present[0].id, "a1");

        let all = list_artifacts_for_project(pool, "p1", &[]).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn override_upsert_and_clear() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        insert_artifact_if_absent(pool, art("a1", "p1", "out/img.xisf", "intermediate"))
            .await
            .unwrap();
        upsert_override(pool, "a1", "final", Some("manual inspection")).await.unwrap();

        // Main row updated by upsert_override.
        let row = get_artifact_by_path(pool, "p1", "out/img.xisf").await.unwrap().unwrap();
        assert_eq!(row.kind, "final");
        assert_eq!(row.classification_source, "manual_override");
        assert!((row.classification_confidence - 1.0_f64).abs() < f64::EPSILON);

        // Clear the override — clear_override returns the deleted row.
        let cleared = clear_override(pool, "a1").await.unwrap();
        assert!(cleared.is_some());
        assert_eq!(cleared.unwrap().kind, "final");
    }

    #[tokio::test]
    async fn mark_missing_and_recovered() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        insert_artifact_if_absent(pool, art("a1", "p1", "out/img.xisf", "intermediate"))
            .await
            .unwrap();
        mark_artifact_missing(pool, "a1").await.unwrap();

        let row = get_artifact_by_path(pool, "p1", "out/img.xisf").await.unwrap().unwrap();
        assert_eq!(row.state, "missing");

        mark_artifact_recovered(pool, "a1", 2048, Some("abc123")).await.unwrap();
        let row2 = get_artifact_by_path(pool, "p1", "out/img.xisf").await.unwrap().unwrap();
        assert_eq!(row2.state, "present");
        assert_eq!(row2.size_bytes, 2048);
    }

    #[tokio::test]
    async fn complete_tool_launch_sets_completed_at() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        // Insert a tool_launch row (migration 0024 table).
        sqlx::query(
            "INSERT INTO tool_launches (id, project_id, tool_id, launched_at, outcome, audit_id)
             VALUES ('tl-1','proj-1','pixinsight','2026-06-01T08:00:00Z','spawned','audit-1')",
        )
        .execute(pool)
        .await
        .unwrap();

        let updated = complete_tool_launch(pool, "tl-1", "2026-06-01T12:00:00Z").await.unwrap();
        assert!(updated);

        // Second call should be idempotent (returns false — already set).
        let updated2 = complete_tool_launch(pool, "tl-1", "2026-06-01T13:00:00Z").await.unwrap();
        assert!(!updated2);
    }
}
