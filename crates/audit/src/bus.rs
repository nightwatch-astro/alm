// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Hybrid event bus: live broadcast via tokio + durable SQLite events table.
//!
//! T010b: durable side wired in this module. Publishes to both the SQLite
//! `events` table (durable, survives restart) and a tokio broadcast channel
//! (in-process, non-durable). Replay reads from the `events` table with a
//! monotonic `event_id` cursor.
//!
//! Durable reads/writes delegate to `persistence_lifecycle::repositories::events`
//! (db-boundary-zero) rather than issuing raw SQL here.

use audit_types::{AuditLogEntry, EventPublisher, Source};
use domain_core::ids::AuditId;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::event_bus::EventEnvelope;

/// Capacity of the broadcast channel.  Lagging receivers are dropped with
/// `RecvError::Lagged`; they must re-subscribe and query the durable table.
const DEFAULT_BUS_CAPACITY: usize = 256;

/// Error type for event bus operations.
#[derive(Debug, thiserror::Error)]
pub enum BusError {
    #[error("serialisation error: {0}")]
    Serialise(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Database(#[from] persistence_core::DbError),
}

/// Column value for `events.source`.
fn source_str(source: Source) -> &'static str {
    match source {
        Source::User => "user",
        Source::Restore => "restore",
        Source::System => "system",
    }
}

/// Hybrid live + durable event bus.
///
/// Clone to share across tasks — clones share the same underlying channel and pool.
#[derive(Clone, Debug)]
pub struct EventBus {
    sender: broadcast::Sender<EventEnvelope<serde_json::Value>>,
    pool: SqlitePool,
}

impl EventBus {
    /// Construct with a SQLite pool (durable side) and channel capacity (live side).
    #[must_use]
    pub fn new(pool: SqlitePool, channel_capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(channel_capacity);
        Self { sender, pool }
    }

    /// Convenience constructor with default channel capacity.
    #[must_use]
    pub fn with_pool(pool: SqlitePool) -> Self {
        Self::new(pool, DEFAULT_BUS_CAPACITY)
    }

    /// Publish a typed payload on the given topic.
    ///
    /// 1. Serialises the payload to `serde_json::Value`.
    /// 2. Inserts a durable row into the `events` table.
    /// 3. Broadcasts the envelope to live subscribers.
    ///
    /// Returns the number of active live receivers that received the event.
    /// A return value of `0` is not an error — it just means no live listeners.
    ///
    /// # Errors
    /// Returns `BusError::Serialise` if payload serialisation fails.
    /// Returns `BusError::Database` if the durable write fails.
    pub async fn publish<P: Serialize>(
        &self,
        topic: &str,
        source: Source,
        payload: P,
    ) -> Result<usize, BusError> {
        let value = serde_json::to_value(&payload)?;
        let envelope = EventEnvelope::new(topic, source, value.clone());

        // 1. Write durable row.
        let source_str = source_str(source);
        let emitted_at = envelope
            .emitted_at
            .as_offset_date_time()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
        let payload_str = serde_json::to_string(&value)?;

        persistence_lifecycle::repositories::events::insert_event(
            &self.pool,
            topic,
            source_str,
            &emitted_at,
            &payload_str,
        )
        .await?;

        // 2. Broadcast to live subscribers.
        // `send` errors only when there are NO receivers at all (which is fine).
        Ok(self.sender.send(envelope).unwrap_or(0))
    }

    /// Publish many already-serialised events, writing every durable row in one
    /// transaction before broadcasting the envelopes.
    ///
    /// Same observable outcome as calling [`Self::publish`] per event (one
    /// `events` row and one live envelope each), at one commit for the batch
    /// instead of one per event. Only for Tier-2 (re-derivable) events: on a
    /// commit failure the whole batch is lost, and no envelope is broadcast.
    ///
    /// # Errors
    /// Returns `BusError::Database` if the transaction or any insert fails.
    pub async fn publish_batch(
        &self,
        events: &[(String, Source, serde_json::Value)],
    ) -> Result<(), BusError> {
        if events.is_empty() {
            return Ok(());
        }
        let envelopes: Vec<EventEnvelope<serde_json::Value>> = events
            .iter()
            .map(|(topic, source, payload)| EventEnvelope::new(topic, *source, payload.clone()))
            .collect();

        let mut owned: Vec<(String, String)> = Vec::with_capacity(envelopes.len());
        for envelope in &envelopes {
            let emitted_at = envelope
                .emitted_at
                .as_offset_date_time()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
            owned.push((emitted_at, serde_json::to_string(&envelope.payload)?));
        }
        let rows: Vec<(&str, &str, &str, &str)> = envelopes
            .iter()
            .zip(&owned)
            .map(|(envelope, (emitted_at, payload))| {
                (
                    envelope.topic.as_str(),
                    source_str(envelope.source),
                    emitted_at.as_str(),
                    payload.as_str(),
                )
            })
            .collect();

        let mut tx = self.pool.begin().await.map_err(persistence_core::DbError::Database)?;
        persistence_lifecycle::repositories::events::insert_events_conn(&mut tx, &rows).await?;
        tx.commit().await.map_err(persistence_core::DbError::Database)?;

        for envelope in envelopes {
            let _ = self.sender.send(envelope);
        }
        Ok(())
    }

    /// T121 (spec 030 FR-131, Q15/#647): single write-through path for
    /// audit-worthy mutations. Writes the durable `audit_log_entry` row
    /// first, then emits `event_payload` on `topic` for the live UI, and
    /// returns the durable `entry.audit_id` — never a bus-only id (the
    /// FR-131 violation this closes: settings/protection/equipment/source
    /// mutations previously returned an id pointing at a bus event no audit
    /// read could resolve).
    ///
    /// Build `entry` with `AuditLogEntry::new(..)` and, as needed,
    /// `.with_reason_code(..)` (refused/failed outcomes) and
    /// `.with_payload(json!({"before": .., "after": ..}))` for
    /// non-lifecycle before→after pairs (data-model.md "Audit Entry —
    /// Generalized Mutation Record"). Typical call site:
    ///
    /// ```ignore
    /// let entry = AuditLogEntry::new(
    ///     EntityType::Settings, entity_id, "settings.update", "user",
    ///     Outcome::Applied, Severity::Workflow, request_id,
    /// )
    /// .with_payload(json!({"key": key, "before": prior_value, "after": new_value}));
    /// let audit_id = bus.write_audit(entry, TOPIC_SETTINGS_CHANGED, Source::User, event_payload).await?;
    /// ```
    ///
    /// # Failure semantics (constitution §II)
    /// The durable `audit_log_entry` insert is load-bearing: an insert
    /// failure returns `Err` and the caller's command MUST fail. The bus
    /// emit (including its own durable `events`-table row) is best-effort:
    /// a publish failure is logged and swallowed — the command still
    /// succeeds, because `audit_log_entry` is the authoritative audit
    /// record and `events` is non-authoritative transient diagnostics
    /// (spec §8.3 "Store roles").
    ///
    /// # Errors
    /// Returns `BusError::Database` only if the durable `audit_log_entry`
    /// insert fails.
    pub async fn write_audit<P: Serialize>(
        &self,
        entry: AuditLogEntry,
        topic: &str,
        source: Source,
        event_payload: P,
    ) -> Result<AuditId, BusError> {
        persistence_lifecycle::repositories::audit::insert_audit_entry(&self.pool, &entry)
            .await
            .map_err(BusError::Database)?;

        if let Err(err) = self.publish(topic, source, event_payload).await {
            tracing::warn!(
                audit_id = %entry.audit_id.as_uuid(),
                topic,
                error = %err,
                "audit bus emit failed after durable write; audit_log_entry row is authoritative"
            );
        }

        Ok(entry.audit_id)
    }

    /// Broadcast a signal to live subscribers **without** writing a durable
    /// events-table row.
    ///
    /// Used by the group-commit flush path: durable rows are already committed
    /// inside the flush transaction; this call wakes the log forwarder (which
    /// re-queries the DB) without double-writing. Returns the number of active
    /// receivers that received the signal; `0` is not an error.
    #[must_use]
    pub fn broadcast_only(&self, topic: &str) -> usize {
        let envelope = EventEnvelope::new(topic, Source::System, serde_json::Value::Null);
        self.sender.send(envelope).unwrap_or(0)
    }

    /// Subscribe to all live events on the bus.
    ///
    /// Receiver is non-blocking (async). Missed events due to capacity overflow
    /// come back as `RecvError::Lagged`.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope<serde_json::Value>> {
        self.sender.subscribe()
    }

    /// Access the underlying pool for cursor queries during lag recovery.
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Replay events from the durable `events` table.
    ///
    /// - `topic_filter`: if `Some`, only events with that topic are returned.
    /// - `since`: if `Some`, only events with `event_id > since` are returned (cursor).
    ///
    /// All replayed events have `source` overridden to `Source::Restore` per
    /// spec 002 R-Source-1 (replay = restore semantics).
    ///
    /// # Errors
    /// Returns `BusError::Database` if the query fails.
    pub async fn replay(
        &self,
        topic_filter: Option<&str>,
        since: Option<i64>,
    ) -> Result<Vec<EventEnvelope<serde_json::Value>>, BusError> {
        let since_id = since.unwrap_or(0);

        let rows = if let Some(topic) = topic_filter {
            persistence_lifecycle::repositories::events::list_since_by_topic(
                &self.pool, since_id, topic,
            )
            .await?
        } else {
            persistence_lifecycle::repositories::events::list_since(&self.pool, since_id).await?
        };

        let mut envelopes = Vec::with_capacity(rows.len());
        for row in rows {
            let payload: serde_json::Value = serde_json::from_str(&row.payload)?;
            // Restore semantics: always emit with Source::Restore (R-Source-1).
            let envelope = EventEnvelope::new(&row.topic, Source::Restore, payload);
            envelopes.push(envelope);
        }

        Ok(envelopes)
    }
}

/// Lets `persistence_db` repositories (e.g. `lifecycle::SqliteLifecycleRepository`)
/// publish through this bus without depending on the `audit` crate (which
/// depends on `persistence_db`, so the reverse edge would cycle).
#[async_trait::async_trait]
impl EventPublisher for EventBus {
    async fn publish(&self, topic: &str, source: Source, payload: serde_json::Value) {
        let _ = Self::publish(self, topic, source, payload).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::Source;

    async fn make_test_bus() -> EventBus {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.expect("in-memory pool");
        // Apply the events table migration manually.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL,\
             source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL,\
             payload TEXT NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .expect("create events table");
        EventBus::with_pool(pool)
    }

    #[tokio::test]
    async fn publish_and_receive_event() {
        let bus = make_test_bus().await;
        let mut rx = bus.subscribe();

        bus.publish("test.topic", Source::System, serde_json::json!({"ok": true}))
            .await
            .expect("publish");

        let envelope = rx.try_recv().expect("should receive");
        assert_eq!(envelope.topic, "test.topic");
        assert_eq!(envelope.payload["ok"], true);
    }

    #[tokio::test]
    async fn publish_to_no_receivers_is_not_an_error() {
        let bus = make_test_bus().await;
        let count = bus
            .publish("test.topic", Source::System, serde_json::json!({}))
            .await
            .expect("publish");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn publish_three_events_durable_and_live() {
        let bus = make_test_bus().await;
        let mut rx = bus.subscribe();

        // Publish 3 events.
        for i in 0..3u32 {
            bus.publish("test.topic", Source::User, serde_json::json!({"i": i}))
                .await
                .expect("publish");
        }

        // Verify 3 received on broadcast channel.
        for i in 0..3u32 {
            let envelope = rx.try_recv().expect("should receive");
            assert_eq!(envelope.payload["i"], i);
        }

        // Verify 3 durable rows in the events table.
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM events WHERE topic = 'test.topic'")
                .fetch_one(&bus.pool)
                .await
                .expect("count");
        assert_eq!(count.0, 3);
    }

    #[tokio::test]
    async fn replay_returns_restore_source() {
        let bus = make_test_bus().await;

        // Publish 3 events with Source::User.
        for i in 0..3u32 {
            bus.publish("replay.topic", Source::User, serde_json::json!({"i": i}))
                .await
                .expect("publish");
        }

        // Replay without cursor.
        let replayed = bus.replay(Some("replay.topic"), None).await.expect("replay");
        assert_eq!(replayed.len(), 3);

        // All replayed events must have Source::Restore (R-Source-1).
        for envelope in &replayed {
            assert_eq!(envelope.source, Source::Restore, "replay must use Restore source");
        }
    }

    /// Real migrated DB (not the hand-rolled `events`-only fixture above) —
    /// `write_audit` needs the actual `audit_log_entry` table (migration
    /// 0063 adds `reason_code`).
    async fn make_migrated_bus() -> (persistence_core::Database, EventBus) {
        let db = persistence_core::Database::in_memory().await.expect("in-memory db");
        db.migrate().await.expect("migrate");
        let bus = EventBus::with_pool(db.pool().clone());
        (db, bus)
    }

    #[tokio::test]
    async fn write_audit_writes_durable_row_and_returns_its_audit_id() {
        use audit_types::{Outcome, Severity};
        use domain_core::ids::EntityId;
        use domain_core::lifecycle::data_asset::EntityType;

        let (db, bus) = make_migrated_bus().await;
        let entity_id = EntityId::new();
        let entry = AuditLogEntry::new(
            EntityType::Settings,
            entity_id,
            "settings.update",
            "user",
            Outcome::Refused,
            Severity::Workflow,
            EntityId::new(),
        )
        .with_reason_code("invalid_value")
        .with_payload(serde_json::json!({"key": "pattern", "before": "a", "after": "b"}));
        let expected_audit_id = entry.audit_id;

        let audit_id = bus
            .write_audit(entry, "settings.changed", Source::User, serde_json::json!({"ok": true}))
            .await
            .expect("write_audit");

        assert_eq!(audit_id, expected_audit_id);

        let row: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT outcome, reason_code, payload FROM audit_log_entry WHERE audit_id = ?",
        )
        .bind(audit_id.as_uuid().to_string())
        .fetch_one(db.pool())
        .await
        .expect("durable row must exist");
        assert_eq!(row.0, "refused");
        assert_eq!(row.1.as_deref(), Some("invalid_value"));
        assert!(row.2.expect("payload present").contains("\"before\":\"a\""));
    }

    #[tokio::test]
    async fn write_audit_propagates_error_when_durable_insert_fails() {
        use audit_types::{Outcome, Severity};
        use domain_core::ids::EntityId;
        use domain_core::lifecycle::data_asset::EntityType;

        let (db, bus) = make_migrated_bus().await;
        // Simulate a durable-write failure: drop the load-bearing table.
        sqlx::query("DROP TABLE audit_log_entry").execute(db.pool()).await.expect("drop table");

        let entry = AuditLogEntry::new(
            EntityType::Protection,
            EntityId::new(),
            "protection.source.set",
            "user",
            Outcome::Applied,
            Severity::Workflow,
            EntityId::new(),
        );

        let result = bus
            .write_audit(entry, "protection.source.set", Source::User, serde_json::json!({}))
            .await;
        assert!(result.is_err(), "insert failure must propagate — the durable row is load-bearing");
    }

    #[tokio::test]
    async fn write_audit_succeeds_when_bus_emit_fails_but_durable_row_is_written() {
        use audit_types::{Outcome, Severity};
        use domain_core::ids::EntityId;
        use domain_core::lifecycle::data_asset::EntityType;

        let (db, bus) = make_migrated_bus().await;
        // Simulate a bus-emit failure (the events table's own durable write
        // fails) while audit_log_entry stays intact.
        sqlx::query("DROP TABLE events").execute(db.pool()).await.expect("drop events table");

        let entry = AuditLogEntry::new(
            EntityType::Equipment,
            EntityId::new(),
            "equipment.optical_train.create",
            "user",
            Outcome::Applied,
            Severity::Workflow,
            EntityId::new(),
        );
        let expected_audit_id = entry.audit_id;

        let audit_id = bus
            .write_audit(entry, "equipment.changed", Source::User, serde_json::json!({}))
            .await
            .expect("bus-emit failure must not fail the command");
        assert_eq!(audit_id, expected_audit_id);

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM audit_log_entry WHERE audit_id = ?")
                .bind(audit_id.as_uuid().to_string())
                .fetch_one(db.pool())
                .await
                .expect("count");
        assert_eq!(count.0, 1, "durable row must exist despite the bus-emit failure");
    }

    #[tokio::test]
    async fn replay_cursor_since() {
        let bus = make_test_bus().await;

        // Publish 3 events and note their event_ids via count.
        for i in 0..3u32 {
            bus.publish("cursor.topic", Source::System, serde_json::json!({"i": i}))
                .await
                .expect("publish");
        }

        // Get the event_id of the first two rows.
        let first_id: (i64,) =
            sqlx::query_as("SELECT MIN(event_id) FROM events WHERE topic = 'cursor.topic'")
                .fetch_one(&bus.pool)
                .await
                .expect("min id");

        // Replay since first_id — should return only the 2 events after it.
        let replayed = bus.replay(Some("cursor.topic"), Some(first_id.0)).await.expect("replay");
        assert_eq!(replayed.len(), 2, "should return 2 events after cursor");
    }
}
