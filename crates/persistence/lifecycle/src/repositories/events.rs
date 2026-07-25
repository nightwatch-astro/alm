// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Durable event-bus table repository (migration `0003_events.sql`).
//!
//! Backs `audit::bus::EventBus` (publish/replay) and, in future, the log
//! panel readers (`log_stream::recent_entries` / `export_entries`,
//! `commands::log`) — hence the retention-gap and time-range read shapes
//! below, not just the two `EventBus::replay` variants.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::DbResult;

/// One row from `events`, as read back (no `source` column — no reader needs
/// it today; writers pass `source` explicitly to `insert_event`).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct EventRow {
    pub event_id: i64,
    pub topic: String,
    pub emitted_at: String,
    pub payload: String,
}

/// Append a durable event row. Returns the assigned `event_id`.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn insert_event(
    pool: &SqlitePool,
    topic: &str,
    source: &str,
    emitted_at: &str,
    payload: &str,
) -> DbResult<i64> {
    let result =
        sqlx::query("INSERT INTO events (topic, source, emitted_at, payload) VALUES (?, ?, ?, ?)")
            .bind(topic)
            .bind(source)
            .bind(emitted_at)
            .bind(payload)
            .execute(pool)
            .await?;
    Ok(result.last_insert_rowid())
}

/// Append a durable event row on an existing connection (for use inside a
/// transaction). Returns the assigned `event_id`.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn insert_event_conn(
    conn: &mut SqliteConnection,
    topic: &str,
    source: &str,
    emitted_at: &str,
    payload: &str,
) -> DbResult<i64> {
    let result =
        sqlx::query("INSERT INTO events (topic, source, emitted_at, payload) VALUES (?, ?, ?, ?)")
            .bind(topic)
            .bind(source)
            .bind(emitted_at)
            .bind(payload)
            .execute(conn)
            .await?;
    Ok(result.last_insert_rowid())
}

/// List all events with `event_id > since_id`, oldest first.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_since(pool: &SqlitePool, since_id: i64) -> DbResult<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT event_id, topic, emitted_at, payload \
         FROM events WHERE event_id > ? ORDER BY event_id ASC",
    )
    .bind(since_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List events on a single topic with `event_id > since_id`, oldest first.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_since_by_topic(
    pool: &SqlitePool,
    since_id: i64,
    topic: &str,
) -> DbResult<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT event_id, topic, emitted_at, payload \
         FROM events WHERE event_id > ? AND topic = ? ORDER BY event_id ASC",
    )
    .bind(since_id)
    .bind(topic)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Page size for bounded replay loops in durable-event subscribers.
pub const REPLAY_PAGE_SIZE: i64 = 500;

/// Like [`list_since_by_topic`] but limited to `limit` rows (ascending by
/// event_id). Use in subscriber replay loops to bound memory per iteration.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_since_by_topic_paged(
    pool: &SqlitePool,
    since_id: i64,
    topic: &str,
    limit: i64,
) -> DbResult<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT event_id, topic, emitted_at, payload \
         FROM events WHERE event_id > ? AND topic = ? ORDER BY event_id ASC LIMIT ?",
    )
    .bind(since_id)
    .bind(topic)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Most-recent `limit` events with `event_id > since_id`, newest first
/// (caller reverses for oldest-first display — mirrors the former
/// `log_stream::recent_entries` query shape).
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_recent_since(
    pool: &SqlitePool,
    since_id: i64,
    limit: i64,
) -> DbResult<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT event_id, topic, emitted_at, payload \
         FROM events WHERE event_id > ? ORDER BY event_id DESC LIMIT ?",
    )
    .bind(since_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List events whose `emitted_at` falls in `[since, until)` (either bound
/// optional), oldest first — mirrors the former `log_stream::export_entries`
/// query shape.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_by_emitted_at_range(
    pool: &SqlitePool,
    since: Option<&str>,
    until: Option<&str>,
) -> DbResult<Vec<EventRow>> {
    let rows = match (since, until) {
        (Some(s), Some(u)) => {
            sqlx::query_as::<_, EventRow>(
                "SELECT event_id, topic, emitted_at, payload \
                 FROM events WHERE emitted_at >= ? AND emitted_at < ? ORDER BY event_id ASC",
            )
            .bind(s)
            .bind(u)
            .fetch_all(pool)
            .await?
        }
        (Some(s), None) => {
            sqlx::query_as::<_, EventRow>(
                "SELECT event_id, topic, emitted_at, payload \
                 FROM events WHERE emitted_at >= ? ORDER BY event_id ASC",
            )
            .bind(s)
            .fetch_all(pool)
            .await?
        }
        (None, Some(u)) => {
            sqlx::query_as::<_, EventRow>(
                "SELECT event_id, topic, emitted_at, payload \
                 FROM events WHERE emitted_at < ? ORDER BY event_id ASC",
            )
            .bind(u)
            .fetch_all(pool)
            .await?
        }
        (None, None) => {
            sqlx::query_as::<_, EventRow>(
                "SELECT event_id, topic, emitted_at, payload FROM events ORDER BY event_id ASC",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}

/// Largest assigned `event_id`, or `0` if the table is empty. Used to seed a
/// live forwarder's cursor so only events emitted after subscribe are sent.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn max_event_id(pool: &SqlitePool) -> DbResult<i64> {
    let (max_id,): (i64,) =
        sqlx::query_as("SELECT COALESCE(MAX(event_id), 0) FROM events").fetch_one(pool).await?;
    Ok(max_id)
}

/// Smallest retained `event_id`, or `None` if the table is empty. Used to
/// detect a retention/eviction gap between a caller's cursor and the oldest
/// row still on disk.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn min_event_id(pool: &SqlitePool) -> DbResult<Option<i64>> {
    let min_id: Option<i64> =
        sqlx::query_scalar("SELECT MIN(event_id) FROM events").fetch_one(pool).await?;
    Ok(min_id)
}

/// Total number of rows in the `events` table.  Used for monitoring and to
/// decide whether a prune pass is warranted.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
#[allow(dead_code)] // retention pruner will call this; no production caller yet
pub(crate) async fn count_events(pool: &SqlitePool) -> DbResult<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events").fetch_one(pool).await?;
    Ok(count)
}

/// Delete events older than `older_than_iso` (exclusive upper-bound on
/// `emitted_at`).
///
/// # Replay-watermark safety
///
/// All in-process event-bus subscribers that replay from the durable table
/// start their cursor at 0 on first lag, then advance it as they process
/// rows.  There is currently no persistent cursor registry, so the pruner
/// cannot know the exact per-subscriber watermark.  The caller is expected
/// to pass an `older_than_iso` that is conservatively far enough in the past
/// that no live subscriber cursor can still be behind it:
///
/// - The default retention window (90 days) exceeds any realistic lag window
///   on a desktop application, so in practice every subscriber has already
///   processed events older than the cutoff.
/// - Hooks are unconditionally idempotent (research.md §6.1), so replaying
///   a pruned range a second time after a prune is safe — the worst outcome
///   is a no-op re-dispatch.
/// - kyo7.100 will add cursor advancement on live events; once that lands the
///   replay window on a healthy app will shrink further, making the 90-day
///   floor even more conservative.
///
/// Returns the number of rows deleted.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn prune_events_older_than(pool: &SqlitePool, older_than_iso: &str) -> DbResult<u64> {
    let result = sqlx::query("DELETE FROM events WHERE emitted_at < ?")
        .bind(older_than_iso)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use super::{
        count_events, insert_event, list_by_emitted_at_range, list_recent_since, list_since,
        list_since_by_topic, list_since_by_topic_paged, max_event_id, min_event_id,
        prune_events_older_than,
    };

    async fn setup() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("in-memory pool");
        sqlx::query(
            "CREATE TABLE events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL,\
             source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL,\
             payload TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("create events table");
        pool
    }

    #[tokio::test]
    async fn insert_and_list_since_round_trips() {
        let pool = setup().await;
        let id1 = insert_event(&pool, "t.a", "system", "2026-01-01T00:00:00Z", "{}")
            .await
            .expect("insert 1");
        insert_event(&pool, "t.b", "system", "2026-01-01T00:00:01Z", "{}").await.expect("insert 2");

        let all = list_since(&pool, 0).await.expect("list_since");
        assert_eq!(all.len(), 2);

        let after_first = list_since(&pool, id1).await.expect("list_since cursor");
        assert_eq!(after_first.len(), 1);
        assert_eq!(after_first[0].topic, "t.b");
    }

    #[tokio::test]
    async fn list_since_by_topic_filters() {
        let pool = setup().await;
        insert_event(&pool, "t.a", "system", "2026-01-01T00:00:00Z", "{}").await.unwrap();
        insert_event(&pool, "t.b", "system", "2026-01-01T00:00:01Z", "{}").await.unwrap();

        let filtered = list_since_by_topic(&pool, 0, "t.b").await.expect("list_since_by_topic");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].topic, "t.b");
    }

    #[tokio::test]
    async fn list_recent_since_orders_newest_first() {
        let pool = setup().await;
        for i in 0..3 {
            insert_event(&pool, "t.a", "system", &format!("2026-01-01T00:00:0{i}Z"), "{}")
                .await
                .unwrap();
        }

        let recent = list_recent_since(&pool, 0, 2).await.expect("list_recent_since");
        assert_eq!(recent.len(), 2);
        assert!(recent[0].event_id > recent[1].event_id);
    }

    #[tokio::test]
    async fn list_by_emitted_at_range_bounds() {
        let pool = setup().await;
        insert_event(&pool, "t.a", "system", "2026-01-01T00:00:00Z", "{}").await.unwrap();
        insert_event(&pool, "t.a", "system", "2026-01-02T00:00:00Z", "{}").await.unwrap();
        insert_event(&pool, "t.a", "system", "2026-01-03T00:00:00Z", "{}").await.unwrap();

        let all = list_by_emitted_at_range(&pool, None, None).await.expect("no bounds");
        assert_eq!(all.len(), 3);

        let lower = list_by_emitted_at_range(&pool, Some("2026-01-02T00:00:00Z"), None)
            .await
            .expect("lower bound");
        assert_eq!(lower.len(), 2);

        let upper = list_by_emitted_at_range(&pool, None, Some("2026-01-02T00:00:00Z"))
            .await
            .expect("upper bound");
        assert_eq!(upper.len(), 1);

        let both = list_by_emitted_at_range(
            &pool,
            Some("2026-01-02T00:00:00Z"),
            Some("2026-01-03T00:00:00Z"),
        )
        .await
        .expect("both bounds");
        assert_eq!(both.len(), 1);
    }

    #[tokio::test]
    async fn max_event_id_empty_is_zero() {
        let pool = setup().await;
        assert_eq!(max_event_id(&pool).await.expect("max_event_id"), 0);

        let id1 = insert_event(&pool, "t.a", "system", "2026-01-01T00:00:00Z", "{}").await.unwrap();
        insert_event(&pool, "t.b", "system", "2026-01-01T00:00:01Z", "{}").await.unwrap();
        assert_eq!(max_event_id(&pool).await.expect("max_event_id"), id1 + 1);
    }

    #[tokio::test]
    async fn min_event_id_empty_is_none() {
        let pool = setup().await;
        assert_eq!(min_event_id(&pool).await.expect("min_event_id"), None);

        insert_event(&pool, "t.a", "system", "2026-01-01T00:00:00Z", "{}").await.unwrap();
        assert_eq!(min_event_id(&pool).await.expect("min_event_id"), Some(1));
    }

    #[tokio::test]
    async fn count_events_empty_is_zero() {
        let pool = setup().await;
        assert_eq!(count_events(&pool).await.expect("count"), 0);
    }

    #[tokio::test]
    async fn count_events_after_inserts() {
        let pool = setup().await;
        for i in 0..5 {
            insert_event(&pool, "t.a", "system", &format!("2026-01-0{}T00:00:00Z", i + 1), "{}")
                .await
                .unwrap();
        }
        assert_eq!(count_events(&pool).await.expect("count"), 5);
    }

    #[tokio::test]
    async fn prune_removes_older_rows_and_keeps_newer() {
        let pool = setup().await;
        // Insert three events on separate days.
        insert_event(&pool, "t.a", "system", "2026-01-01T00:00:00Z", "{}").await.unwrap();
        insert_event(&pool, "t.a", "system", "2026-01-02T00:00:00Z", "{}").await.unwrap();
        insert_event(&pool, "t.a", "system", "2026-01-03T00:00:00Z", "{}").await.unwrap();

        // Prune rows older than 2026-01-03 (exclusive upper bound).
        let deleted = prune_events_older_than(&pool, "2026-01-03T00:00:00Z").await.expect("prune");
        assert_eq!(deleted, 2, "two rows are older than the cutoff");

        let remaining = list_since(&pool, 0).await.expect("list_since");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].emitted_at, "2026-01-03T00:00:00Z");
    }

    #[tokio::test]
    async fn prune_all_rows_leaves_empty_table() {
        let pool = setup().await;
        insert_event(&pool, "t.a", "system", "2026-01-01T00:00:00Z", "{}").await.unwrap();
        insert_event(&pool, "t.a", "system", "2026-01-02T00:00:00Z", "{}").await.unwrap();

        let deleted =
            prune_events_older_than(&pool, "2099-01-01T00:00:00Z").await.expect("prune all");
        assert_eq!(deleted, 2);
        assert_eq!(count_events(&pool).await.expect("count"), 0);
    }

    #[tokio::test]
    async fn prune_empty_table_is_noop() {
        let pool = setup().await;
        let deleted =
            prune_events_older_than(&pool, "2026-01-01T00:00:00Z").await.expect("prune empty");
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn list_since_by_topic_paged_respects_limit() {
        let pool = setup().await;
        for i in 0..5 {
            insert_event(&pool, "t.a", "system", &format!("2026-01-01T00:00:0{i}Z"), "{}")
                .await
                .unwrap();
        }
        let page = list_since_by_topic_paged(&pool, 0, "t.a", 3).await.expect("paged");
        assert_eq!(page.len(), 3, "limit=3 must return at most 3 rows");
        assert!(page[0].event_id < page[2].event_id, "ascending order");
    }
}
