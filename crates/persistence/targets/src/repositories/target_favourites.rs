// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository query functions for the spec-051 target favourites surface.
//!
//! Replaces the `localStorage`-only favourites stub in
//! `apps/desktop/src/features/targets/useFavourites.ts` with durable,
//! database-backed state (migration `0061` `target_favourite`). A target is
//! favourited **iff** a row exists for its id — there is no boolean column.
//!
//! Constitution §I: read/write SQLite metadata only; no filesystem mutations.
//! Constitution §V: SQLite is the durable record.

use sqlx::SqlitePool;

use persistence_core::DbResult;

/// Whether a `canonical_target` row exists for `target_id`. Used by the
/// `targets.favourites.add` use case to distinguish `target.not_found` from a
/// genuine database error before inserting.
pub use super::q_targets_mgmt::target_exists;

/// Read back the stored `favourited_at` for `target_id`, or `None` if the
/// target is not currently favourited.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_favourited_at(pool: &SqlitePool, target_id: &str) -> DbResult<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT favourited_at FROM target_favourite WHERE target_id = ?")
            .bind(target_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(at,)| at))
}

/// List the ids of every currently-favourited canonical target, ordered by
/// `favourited_at DESC` (most recently favourited first).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_favourites(pool: &SqlitePool) -> DbResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT target_id FROM target_favourite ORDER BY favourited_at DESC, target_id ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Count every currently-favourited canonical target. Used by
/// `status.summary` (issue #574): the sidebar's "my targets" badge must equal
/// what the Targets page's "My Targets" filter shows, which is wired to this
/// same table, so count and visible list can never diverge.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn count_favourites(pool: &SqlitePool) -> DbResult<i64> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM target_favourite").fetch_one(pool).await?;
    Ok(count)
}

/// Favourite `target_id`. Upsert-safe: favouriting an already-favourited
/// target is a no-op (the original `favourited_at` is preserved, not bumped).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure (including a foreign
/// key violation if `target_id` does not reference an existing
/// `canonical_target` row, when foreign keys are enabled).
pub async fn add_favourite(
    pool: &SqlitePool,
    target_id: &str,
    favourited_at: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO target_favourite (target_id, favourited_at) VALUES (?, ?) \
         ON CONFLICT(target_id) DO NOTHING",
    )
    .bind(target_id)
    .bind(favourited_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Unfavourite `target_id`. No-op (not an error) if the target was never
/// favourited.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn remove_favourite(pool: &SqlitePool, target_id: &str) -> DbResult<()> {
    sqlx::query("DELETE FROM target_favourite WHERE target_id = ?")
        .bind(target_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::{insert_target, setup_db};

    #[tokio::test]
    async fn add_then_list_returns_favourited_target() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-001").await;

        add_favourite(db.pool(), "t-001", "2026-07-05T00:00:00Z").await.unwrap();

        let ids = list_favourites(db.pool()).await.unwrap();
        assert_eq!(ids, vec!["t-001".to_owned()]);
    }

    #[tokio::test]
    async fn add_twice_is_idempotent_single_row() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-002").await;

        add_favourite(db.pool(), "t-002", "2026-07-05T00:00:00Z").await.unwrap();
        add_favourite(db.pool(), "t-002", "2026-07-06T00:00:00Z").await.unwrap();

        let ids = list_favourites(db.pool()).await.unwrap();
        assert_eq!(ids.len(), 1, "second add must not create a duplicate row");

        // Original favourited_at is preserved, not bumped, on a repeat add.
        let row: (String,) =
            sqlx::query_as("SELECT favourited_at FROM target_favourite WHERE target_id = ?")
                .bind("t-002")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(row.0, "2026-07-05T00:00:00Z");
    }

    #[tokio::test]
    async fn remove_deletes_the_row() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-003").await;
        add_favourite(db.pool(), "t-003", "2026-07-05T00:00:00Z").await.unwrap();

        remove_favourite(db.pool(), "t-003").await.unwrap();

        let ids = list_favourites(db.pool()).await.unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn remove_of_never_favourited_id_is_a_noop() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-004").await;

        // Never favourited; removing must not error.
        remove_favourite(db.pool(), "t-004").await.unwrap();

        let ids = list_favourites(db.pool()).await.unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn list_is_empty_when_nothing_favourited() {
        let db = setup_db().await;
        let ids = list_favourites(db.pool()).await.unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn count_favourites_is_zero_when_nothing_favourited() {
        let db = setup_db().await;
        assert_eq!(count_favourites(db.pool()).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn count_favourites_reflects_add_and_remove() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-009").await;
        insert_target(db.pool(), "t-010").await;

        add_favourite(db.pool(), "t-009", "2026-07-05T00:00:00Z").await.unwrap();
        add_favourite(db.pool(), "t-010", "2026-07-05T00:00:00Z").await.unwrap();
        assert_eq!(count_favourites(db.pool()).await.unwrap(), 2);

        remove_favourite(db.pool(), "t-009").await.unwrap();
        assert_eq!(count_favourites(db.pool()).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn target_exists_true_for_known_target() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-006").await;
        assert!(target_exists(db.pool(), "t-006").await.unwrap());
    }

    #[tokio::test]
    async fn target_exists_false_for_unknown_target() {
        let db = setup_db().await;
        assert!(!target_exists(db.pool(), "missing").await.unwrap());
    }

    #[tokio::test]
    async fn get_favourited_at_returns_none_when_not_favourited() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-007").await;
        assert!(get_favourited_at(db.pool(), "t-007").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_favourited_at_returns_stored_timestamp() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-008").await;
        add_favourite(db.pool(), "t-008", "2026-07-05T00:00:00Z").await.unwrap();
        assert_eq!(
            get_favourited_at(db.pool(), "t-008").await.unwrap().as_deref(),
            Some("2026-07-05T00:00:00Z")
        );
    }

    #[tokio::test]
    async fn cascade_delete_of_canonical_target_drops_favourite() {
        let db = setup_db().await;
        insert_target(db.pool(), "t-005").await;
        add_favourite(db.pool(), "t-005", "2026-07-05T00:00:00Z").await.unwrap();

        // Enable FK enforcement explicitly for this connection (SQLite is
        // off by default per-connection unless the pool sets it; matches the
        // ON DELETE CASCADE invariant documented in data-model.md §E1).
        sqlx::query("PRAGMA foreign_keys = ON").execute(db.pool()).await.unwrap();

        sqlx::query("DELETE FROM canonical_target WHERE id = ?")
            .bind("t-005")
            .execute(db.pool())
            .await
            .unwrap();

        let ids = list_favourites(db.pool()).await.unwrap();
        assert!(ids.is_empty(), "cascade delete must remove the favourite row");
    }
}
