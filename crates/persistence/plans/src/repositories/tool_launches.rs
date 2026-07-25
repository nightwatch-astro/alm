// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for the `tool_launches` table (spec 011 T003).
//!
//! Operates on the `tool_launches` table from migration 0024.
//! Constitution V: SQLite is the durable record; `completed_at` is reserved
//! for spec 012 and is always `NULL` in v1 writes.

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::DbResult;

// ── Helpers ───────────────────────────────────────────────────────────────────

// ── Row type ──────────────────────────────────────────────────────────────────

/// Flat row from the `tool_launches` table.
#[derive(Clone, Debug)]
pub struct ToolLaunchRow {
    pub id: String,
    pub project_id: String,
    pub tool_id: String,
    pub launched_at: String,
    pub pid: Option<i64>,
    pub working_dir: Option<String>,
    pub args_hash: Option<String>,
    pub outcome: String,
    pub completed_at: Option<String>,
    pub audit_id: String,
}

// ── Insert helper ─────────────────────────────────────────────────────────────

/// Data required to insert a new `tool_launches` row.
#[derive(Clone, Debug)]
pub struct InsertToolLaunch<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub tool_id: &'a str,
    pub pid: Option<u32>,
    pub working_dir: Option<&'a str>,
    pub args_hash: Option<&'a str>,
    pub outcome: &'a str,
    pub audit_id: &'a str,
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

/// Insert a new `tool_launches` row.  Returns the row `id`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn insert_tool_launch(
    pool: &SqlitePool,
    data: &InsertToolLaunch<'_>,
) -> DbResult<String> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO tool_launches \
         (id, project_id, tool_id, launched_at, pid, working_dir, args_hash, outcome, audit_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(data.id)
    .bind(data.project_id)
    .bind(data.tool_id)
    .bind(&now)
    .bind(data.pid.map(i64::from))
    .bind(data.working_dir)
    .bind(data.args_hash)
    .bind(data.outcome)
    .bind(data.audit_id)
    .execute(pool)
    .await?;
    Ok(data.id.to_owned())
}

/// Fetch the most recent `tool_launches` row for a `(project_id, tool_id)` pair.
///
/// Used by the re-launch guard (spec 011 T012).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
/// Internal FromRow helper to avoid type-complexity lint on tuple results.
#[derive(sqlx::FromRow)]
struct ToolLaunchRawRow {
    id: String,
    project_id: String,
    tool_id: String,
    launched_at: String,
    pid: Option<i64>,
    working_dir: Option<String>,
    args_hash: Option<String>,
    outcome: String,
    completed_at: Option<String>,
    audit_id: String,
}

impl From<ToolLaunchRawRow> for ToolLaunchRow {
    fn from(r: ToolLaunchRawRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            tool_id: r.tool_id,
            launched_at: r.launched_at,
            pid: r.pid,
            working_dir: r.working_dir,
            args_hash: r.args_hash,
            outcome: r.outcome,
            completed_at: r.completed_at,
            audit_id: r.audit_id,
        }
    }
}

/// Fetch the most recent `tool_launches` row for a `(project_id, tool_id)` pair.
///
/// Used by the re-launch guard (spec 011 T012).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_latest_launch(
    pool: &SqlitePool,
    project_id: &str,
    tool_id: &str,
) -> DbResult<Option<ToolLaunchRow>> {
    let row: Option<ToolLaunchRawRow> = sqlx::query_as(
        "SELECT id, project_id, tool_id, launched_at, pid, working_dir, args_hash, outcome, completed_at, audit_id \
         FROM tool_launches \
         WHERE project_id = ? AND tool_id = ? \
         ORDER BY launched_at DESC \
         LIMIT 1",
    )
    .bind(project_id)
    .bind(tool_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(ToolLaunchRow::from))
}

/// List all `tool_launches` rows for a project, newest first.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_launches_for_project(
    pool: &SqlitePool,
    project_id: &str,
) -> DbResult<Vec<ToolLaunchRow>> {
    let rows: Vec<ToolLaunchRawRow> = sqlx::query_as(
        "SELECT id, project_id, tool_id, launched_at, pid, working_dir, args_hash, outcome, completed_at, audit_id \
         FROM tool_launches \
         WHERE project_id = ? \
         ORDER BY launched_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(ToolLaunchRow::from).collect())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;

    fn test_launch<'a>(id: &'a str, project_id: &'a str, tool_id: &'a str) -> InsertToolLaunch<'a> {
        InsertToolLaunch {
            id,
            project_id,
            tool_id,
            pid: Some(1234),
            working_dir: Some("/mnt/library/project"),
            args_hash: Some("abcdef1234567890"),
            outcome: "spawned",
            audit_id: "audit-1",
        }
    }

    #[tokio::test]
    async fn insert_and_get_latest() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        // Insert a project row first to satisfy FK (no explicit FK in SQLite by default, but be safe).
        let launch_id = "00000000-0000-0000-0000-000000000001";
        let project_id = "00000000-0000-0000-0000-000000000002";

        insert_tool_launch(pool, &test_launch(launch_id, project_id, "pixinsight")).await.unwrap();

        let row = get_latest_launch(pool, project_id, "pixinsight").await.unwrap().unwrap();
        assert_eq!(row.id, launch_id);
        assert_eq!(row.outcome, "spawned");
        assert_eq!(row.pid, Some(1234));
    }

    #[tokio::test]
    async fn get_latest_returns_none_when_no_launches() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let result = get_latest_launch(db.pool(), "no-such-project", "pixinsight").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_launches_for_project_ordered_newest_first() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();
        let project_id = "00000000-0000-0000-0000-000000000003";

        insert_tool_launch(pool, &test_launch("launch-1", project_id, "pixinsight")).await.unwrap();
        // Small delay to ensure launched_at ordering
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        insert_tool_launch(pool, &test_launch("launch-2", project_id, "siril")).await.unwrap();

        let rows = list_launches_for_project(pool, project_id).await.unwrap();
        assert_eq!(rows.len(), 2);
        // Newest first
        assert_eq!(rows[0].id, "launch-2");
        assert_eq!(rows[1].id, "launch-1");
    }
}
