// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for prepared source views (spec 026).
//!
//! Operates on the `prepared_source_views` and `prepared_source_view_items`
//! tables from migration 0029.
//!
//! Records are never hard-deleted (A4, GRILL 2026-05-22).
//! State `removed` is a terminal state that preserves full item membership
//! so that regeneration remains available indefinitely.

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

// ── Helpers ───────────────────────────────────────────────────────────────────

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat row from `prepared_source_views`.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PreparedSourceViewRow {
    pub id: String,
    pub project_id: String,
    pub kind: String,
    pub state: String,
    pub created_at: String,
    pub removed_at: Option<String>,
}

/// Flat row from `prepared_source_view_items`.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PreparedSourceViewItemRow {
    pub id: String,
    pub view_id: String,
    pub inventory_item_id: String,
    pub view_relative_path: String,
    pub materialization: String,
    pub last_observed_state: String,
}

// ── Insert types ──────────────────────────────────────────────────────────────

/// Data required to insert a new view record.
#[derive(Clone, Debug)]
pub struct InsertPreparedSourceView<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub kind: &'a str,
}

/// Data required to insert a view item record.
#[derive(Clone, Debug)]
pub struct InsertPreparedSourceViewItem<'a> {
    pub id: &'a str,
    pub view_id: &'a str,
    pub inventory_item_id: &'a str,
    pub view_relative_path: &'a str,
    pub materialization: &'a str,
}

// ── View CRUD ─────────────────────────────────────────────────────────────────

/// Insert a new view record in `current` state.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn insert_view(pool: &SqlitePool, data: &InsertPreparedSourceView<'_>) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO prepared_source_views
             (id, project_id, kind, state, created_at)
         VALUES (?, ?, ?, 'current', ?)",
    )
    .bind(data.id)
    .bind(data.project_id)
    .bind(data.kind)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(())
}

/// Insert a single view item.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn insert_view_item(
    pool: &SqlitePool,
    item: &InsertPreparedSourceViewItem<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO prepared_source_view_items
             (id, view_id, inventory_item_id, view_relative_path,
              materialization, last_observed_state)
         VALUES (?, ?, ?, ?, ?, 'present')",
    )
    .bind(item.id)
    .bind(item.view_id)
    .bind(item.inventory_item_id)
    .bind(item.view_relative_path)
    .bind(item.materialization)
    .execute(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(())
}

/// Fetch a view row by id.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] when the row does not exist.
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_view(pool: &SqlitePool, view_id: &str) -> DbResult<PreparedSourceViewRow> {
    sqlx::query_as::<_, PreparedSourceViewRow>(
        "SELECT id, project_id, kind, state, created_at, removed_at
         FROM prepared_source_views WHERE id = ?",
    )
    .bind(view_id)
    .fetch_optional(pool)
    .await
    .map_err(DbError::Database)?
    .ok_or_else(|| DbError::NotFound(format!("prepared_source_view {view_id}")))
}

/// Fetch all item rows belonging to a view.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_view_items(
    pool: &SqlitePool,
    view_id: &str,
) -> DbResult<Vec<PreparedSourceViewItemRow>> {
    sqlx::query_as::<_, PreparedSourceViewItemRow>(
        "SELECT id, view_id, inventory_item_id, view_relative_path,
                materialization, last_observed_state
         FROM prepared_source_view_items
         WHERE view_id = ?
         ORDER BY view_relative_path",
    )
    .bind(view_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::Database)
}

/// Fetch all views for a project.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_views_for_project(
    pool: &SqlitePool,
    project_id: &str,
) -> DbResult<Vec<PreparedSourceViewRow>> {
    sqlx::query_as::<_, PreparedSourceViewRow>(
        "SELECT id, project_id, kind, state, created_at, removed_at
         FROM prepared_source_views
         WHERE project_id = ?
         ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::Database)
}

/// Update view state.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn update_view_state(pool: &SqlitePool, view_id: &str, new_state: &str) -> DbResult<()> {
    sqlx::query("UPDATE prepared_source_views SET state = ? WHERE id = ?")
        .bind(new_state)
        .bind(view_id)
        .execute(pool)
        .await
        .map_err(DbError::Database)?;
    Ok(())
}

/// Mark a view as removed and record the timestamp.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn mark_view_removed(pool: &SqlitePool, view_id: &str) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "UPDATE prepared_source_views
         SET state = 'removed', removed_at = ?
         WHERE id = ?",
    )
    .bind(&now)
    .bind(view_id)
    .execute(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(())
}

/// Update a single item's `last_observed_state`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn update_item_observed_state(
    pool: &SqlitePool,
    item_id: &str,
    observed_state: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE prepared_source_view_items
         SET last_observed_state = ?
         WHERE id = ?",
    )
    .bind(observed_state)
    .bind(item_id)
    .execute(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::setup_db;

    #[tokio::test]
    async fn insert_and_get_view_roundtrip() {
        let db = setup_db().await;
        // Need a project row first.
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, created_at, updated_at)
             VALUES ('proj-1', 'Test', 'PixInsight', 'ready', 'projects/test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        insert_view(
            db.pool(),
            &InsertPreparedSourceView { id: "view-1", project_id: "proj-1", kind: "symlink" },
        )
        .await
        .unwrap();

        let row = get_view(db.pool(), "view-1").await.unwrap();
        assert_eq!(row.id, "view-1");
        assert_eq!(row.state, "current");
        assert_eq!(row.kind, "symlink");
        assert!(row.removed_at.is_none());
    }

    #[tokio::test]
    async fn mark_removed_sets_state_and_timestamp() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, created_at, updated_at)
             VALUES ('proj-2', 'Test2', 'PixInsight', 'ready', 'projects/test2', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        insert_view(
            db.pool(),
            &InsertPreparedSourceView { id: "view-2", project_id: "proj-2", kind: "copy" },
        )
        .await
        .unwrap();

        mark_view_removed(db.pool(), "view-2").await.unwrap();

        let row = get_view(db.pool(), "view-2").await.unwrap();
        assert_eq!(row.state, "removed");
        assert!(row.removed_at.is_some());
    }

    #[tokio::test]
    async fn insert_items_and_list() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, created_at, updated_at)
             VALUES ('proj-3', 'Test3', 'PixInsight', 'ready', 'projects/test3', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        insert_view(
            db.pool(),
            &InsertPreparedSourceView { id: "view-3", project_id: "proj-3", kind: "symlink" },
        )
        .await
        .unwrap();

        insert_view_item(
            db.pool(),
            &InsertPreparedSourceViewItem {
                id: "item-1",
                view_id: "view-3",
                inventory_item_id: "inv-1",
                view_relative_path: "Sources/M31_L_001.fit",
                materialization: "symlink",
            },
        )
        .await
        .unwrap();

        insert_view_item(
            db.pool(),
            &InsertPreparedSourceViewItem {
                id: "item-2",
                view_id: "view-3",
                inventory_item_id: "inv-2",
                view_relative_path: "Sources/M31_L_002.fit",
                materialization: "symlink",
            },
        )
        .await
        .unwrap();

        let items = list_view_items(db.pool(), "view-3").await.unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.last_observed_state == "present"));
    }

    #[tokio::test]
    async fn get_view_not_found() {
        let db = setup_db().await;
        let err = get_view(db.pool(), "nonexistent").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_views_for_project_returns_all() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, created_at, updated_at)
             VALUES ('proj-4', 'Test4', 'PixInsight', 'ready', 'projects/test4', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        insert_view(
            db.pool(),
            &InsertPreparedSourceView { id: "v-a", project_id: "proj-4", kind: "symlink" },
        )
        .await
        .unwrap();
        insert_view(
            db.pool(),
            &InsertPreparedSourceView { id: "v-b", project_id: "proj-4", kind: "copy" },
        )
        .await
        .unwrap();

        let views = list_views_for_project(db.pool(), "proj-4").await.unwrap();
        assert_eq!(views.len(), 2);
    }

    /// #745 (spec 049 CL-2): re-running `Database::migrate()` must NOT
    /// force-flip a view whose items carry more than one recorded kind —
    /// that used to be `reconcile_kind_diverged_views`'s job on every app
    /// start, auto-corrupting real cross-drive projects (whose drive-scope
    /// resolution legitimately produces per-item kind diversity) into the
    /// dead-end `kind_diverged` state on every reopen.
    #[tokio::test]
    async fn migrate_does_not_flip_mixed_kind_views_to_kind_diverged() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, created_at, updated_at)
             VALUES ('proj-5', 'Test5', 'PixInsight', 'ready', 'projects/test5', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        // A cross-drive project's view: kind='symlink' (dominant), one item
        // resolved to 'copy' (drive-scope fallback) — VALID per CL-2.
        insert_view(
            db.pool(),
            &InsertPreparedSourceView { id: "v-mixed", project_id: "proj-5", kind: "symlink" },
        )
        .await
        .unwrap();
        insert_view_item(
            db.pool(),
            &InsertPreparedSourceViewItem {
                id: "item-mixed",
                view_id: "v-mixed",
                inventory_item_id: "inv-mixed",
                view_relative_path: "Sources/mixed.fit",
                materialization: "copy",
            },
        )
        .await
        .unwrap();

        // Re-running migrate() (as every app start does) must leave it alone.
        db.migrate().await.unwrap();

        assert_eq!(get_view(db.pool(), "v-mixed").await.unwrap().state, "current");
    }
}
