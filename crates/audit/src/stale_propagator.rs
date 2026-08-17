// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Event-bus driven stale-propagation subscriber (spec 002 T046).
//!
//! Subscribes to `lifecycle.transition.applied` events and recomputes
//! dependent staleness. The full propagation graph (research.md §6) requires
//! per-entity dependent indexes that aren't all wired yet, so this module
//! ships the spawn-and-loop skeleton plus a tested handler hook. Adding a
//! new dependent kind means dropping a closure into the registered hooks.
//!
//! Idempotence rule (research.md §6.1): subscribers MUST be idempotent on
//! `(audit_id, subscriber_id)`. The hook contract takes the audit id so the
//! handler can deduplicate against its own ledger.

use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::broadcast::error::RecvError;

use crate::bus::EventBus;
use crate::event_bus::{EventEnvelope, TOPIC_LIFECYCLE_TRANSITION_APPLIED};

/// Hook signature for a downstream propagator.
///
/// Receives the full envelope plus an `audit_id` string for dedup tracking.
/// Returning `Err(...)` is logged but does not unsubscribe — propagators
/// are best-effort; the durable bus is the source of truth on restart.
pub type PropagatorFn =
    Arc<dyn Fn(&EventEnvelope<serde_json::Value>) -> Result<(), String> + Send + Sync + 'static>;

/// Configurable propagator that fans events out to registered hooks.
#[derive(Default, Clone)]
pub struct StalePropagator {
    hooks: Vec<PropagatorFn>,
}

impl StalePropagator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new propagation hook. Returns `self` for builder chaining.
    #[must_use]
    pub fn with_hook(mut self, hook: PropagatorFn) -> Self {
        self.hooks.push(hook);
        self
    }

    /// Invoke every hook for the given envelope, swallowing per-hook errors.
    /// Errors are returned as a `Vec<String>` so callers can log them.
    #[must_use]
    pub fn dispatch(&self, env: &EventEnvelope<serde_json::Value>) -> Vec<String> {
        self.hooks.iter().filter_map(|h| (h)(env).err()).collect()
    }

    /// Spawn the subscriber loop on the current tokio runtime.
    ///
    /// Filters to `lifecycle.transition.applied` only; other topics pass
    /// through unhandled. On broadcast lag, replays missed events from the
    /// durable events table using a monotonic cursor (GF-18).
    #[must_use]
    pub fn spawn(self, bus: &EventBus) -> tokio::task::JoinHandle<()> {
        let mut rx = bus.subscribe();
        let bus = bus.clone();
        tokio::spawn(async move {
            // Cursor starts at 0, so the FIRST lag replays from the beginning.
            // Every lag after that resumes from the high-water mark the previous
            // replay left behind, so only one replay can ever walk history, and
            // the pruner bounds how far back even that reaches.
            //
            // Hooks are idempotent (research.md §6.1) so re-dispatching
            // historical events is safe; the events table is authoritative for
            // recovery.
            let mut cursor: i64 = 0;

            loop {
                match rx.recv().await {
                    Ok(env) => {
                        if env.topic == TOPIC_LIFECYCLE_TRANSITION_APPLIED {
                            // The live path deliberately leaves `cursor` alone.
                            // Advancing it here to `max_event_id` would read the
                            // table-wide max rather than this event's row: any event
                            // inserted between the broadcast and that query gets
                            // jumped, so a later `Lagged` replay starts *after* it
                            // and it is lost rather than late. The envelope carries
                            // no `event_id`, so this branch cannot know its own row
                            // to advance to. Only the replay below moves the cursor,
                            // and it does so per row.
                            let _errors = self.dispatch(&env);
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(
                            missed = n,
                            cursor,
                            "stale_propagator: lagged — replaying from durable events"
                        );
                        cursor = self.replay_since(&bus, cursor).await;
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        })
    }

    /// Replay lifecycle-transition events from the durable table since `cursor`,
    /// dispatching each through registered hooks. Pages in chunks of
    /// [`REPLAY_PAGE_SIZE`] to bound memory on large event tables. Returns the
    /// new cursor (highest processed `event_id`, or the incoming cursor on error).
    #[expect(
        clippy::cognitive_complexity,
        reason = "paged replay loop over stored events with per-row cursor advance"
    )]
    async fn replay_since(&self, bus: &EventBus, mut cursor: i64) -> i64 {
        use persistence_lifecycle::repositories::events::{
            list_since_by_topic_paged, REPLAY_PAGE_SIZE,
        };
        loop {
            let rows = list_since_by_topic_paged(
                bus.pool(),
                cursor,
                TOPIC_LIFECYCLE_TRANSITION_APPLIED,
                REPLAY_PAGE_SIZE,
            )
            .await;

            let rows = match rows {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(error = %e, "stale_propagator: replay query failed");
                    break;
                }
            };

            if rows.is_empty() {
                break;
            }

            for row in &rows {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&row.payload) {
                    let env = EventEnvelope::new(
                        TOPIC_LIFECYCLE_TRANSITION_APPLIED,
                        crate::event_bus::Source::Restore,
                        payload,
                    );
                    let _errors = self.dispatch(&env);
                }
                cursor = row.event_id;
            }
            tracing::info!(replayed = rows.len(), cursor, "stale_propagator: replay page");
            if rows.len() < usize::try_from(REPLAY_PAGE_SIZE).unwrap_or(usize::MAX) {
                break; // last page
            }
        }
        cursor
    }
}

/// FR-003 (#713): marks a project's dependent projections/prepared sources
/// stale when the project's own lifecycle transitions (research.md §6:
/// `ProjectManifest depends_on Project`). Narrow slice of the dependency
/// graph — only the `project_id` FK dependents already modeled in the schema
/// (`processing_artifact.project_id`, `prepared_source_view.project_id`);
/// session-level dependents (`PreparedSourceView depends_on AcquisitionSession[]`)
/// would need a further join and are out of scope for this minimal fix.
///
/// No-ops when the envelope carries no `projectId` (unresolvable at the
/// publish site — see `LifecycleTransitionApplied::project_id`).
#[must_use]
pub fn resolve_project_dependents_hook(pool: SqlitePool) -> PropagatorFn {
    Arc::new(move |env| {
        let Some(project_id) = env.payload.get("projectId").and_then(|v| v.as_str()) else {
            return Ok(());
        };
        let project_id = project_id.to_owned();
        let pool = pool.clone();
        // Hooks are synchronous (best-effort, at-least-once per the module
        // docs); spawn the actual DB write rather than blocking the
        // dispatch loop that drives every other registered hook.
        tokio::spawn(async move {
            // DB-boundary: the actual UPDATE statements live in
            // `persistence_lifecycle::repositories::lifecycle` (check-db-boundary.sh
            // forbids raw SQL outside crates/persistence/db).
            if let Err(err) =
                persistence_lifecycle::repositories::lifecycle::mark_project_dependents_stale(
                    &pool,
                    &project_id,
                )
                .await
            {
                tracing::warn!(
                    project_id,
                    error = %err,
                    "stale-dependent propagation failed"
                );
            }
        });
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::{LifecycleTransitionApplied, Source};
    use domain_core::ids::Timestamp;
    use domain_core::lifecycle::data_asset::EntityType;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn test_bus() -> EventBus {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL, source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL, payload TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        EventBus::with_pool(pool)
    }

    #[tokio::test]
    async fn hooks_fire_on_matching_topic() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let propagator = StalePropagator::new().with_hook(Arc::new(move |_env| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));

        let bus = test_bus().await;
        let handle = propagator.spawn(&bus);

        bus.publish(
            TOPIC_LIFECYCLE_TRANSITION_APPLIED,
            Source::User,
            LifecycleTransitionApplied {
                entity_type: EntityType::Project,
                entity_id: "00000000-0000-0000-0000-000000000000".to_owned(),
                from_state: "ready".to_owned(),
                to_state: "processing".to_owned(),
                actor: "user".to_owned(),
                at: Timestamp::now_utc(),
                project_id: Some("00000000-0000-0000-0000-000000000000".to_owned()),
            },
        )
        .await
        .unwrap();

        // Let the spawned task observe the event.
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
        handle.abort();
    }

    /// Real migrated DB — `resolve_project_dependents_hook` needs the actual
    /// `processing_artifact`/`prepared_source_view` tables (migration 0002).
    async fn migrated_test_bus() -> (persistence_core::Database, EventBus) {
        let db = persistence_core::Database::in_memory().await.expect("in-memory db");
        db.migrate().await.expect("migrate");
        let bus = EventBus::with_pool(db.pool().clone());
        (db, bus)
    }

    /// Seeds two projects (`project_id` / `other_project_id`) each with a
    /// `processing_artifact` row, plus one `prepared_source_view` row for
    /// `project_id` only — the fixture `resolve_project_dependents_hook`
    /// tests scope their assertions against.
    async fn seed_two_project_dependents(
        pool: &sqlx::SqlitePool,
        project_id: &str,
        other_project_id: &str,
        now: &str,
    ) {
        sqlx::query(
            "INSERT INTO target (id, primary_designation, created_at) VALUES ('t1', 'M31', ?)",
        )
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        for pid in [project_id, other_project_id] {
            sqlx::query(
                "INSERT INTO project (id, name, target_id, session_ids, created_at) \
                 VALUES (?, 'p', 't1', '[]', ?)",
            )
            .bind(pid)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
             VALUES ('root1', 'r', '/tmp', 'local', 'active', ?)",
        )
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record \
             (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES ('fr1', 'root1', 'a.fits', 1, ?, 'observed', ?, ?)",
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO processing_artifact \
             (id, project_id, file_record_id, kind, staleness, created_at) \
             VALUES ('art-mine', ?, 'fr1', 'manifest', 'current', ?)",
        )
        .bind(project_id)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO processing_artifact \
             (id, project_id, file_record_id, kind, staleness, created_at) \
             VALUES ('art-other', ?, 'fr1', 'manifest', 'current', ?)",
        )
        .bind(other_project_id)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO prepared_source_view (id, project_id, state, created_at) \
             VALUES ('psv-mine', ?, 'ready', ?)",
        )
        .bind(project_id)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn resolve_project_dependents_hook_marks_project_scoped_rows_stale() {
        let (db, bus) = migrated_test_bus().await;
        let project_id = "11111111-1111-1111-1111-111111111111";
        let other_project_id = "22222222-2222-2222-2222-222222222222";
        let now = "2026-07-19T00:00:00Z";
        seed_two_project_dependents(db.pool(), project_id, other_project_id, now).await;

        let propagator =
            StalePropagator::new().with_hook(resolve_project_dependents_hook(db.pool().clone()));
        let handle = propagator.spawn(&bus);

        bus.publish(
            TOPIC_LIFECYCLE_TRANSITION_APPLIED,
            Source::User,
            LifecycleTransitionApplied {
                entity_type: EntityType::Project,
                entity_id: project_id.to_owned(),
                from_state: "ready".to_owned(),
                to_state: "processing".to_owned(),
                actor: "user".to_owned(),
                at: Timestamp::now_utc(),
                project_id: Some(project_id.to_owned()),
            },
        )
        .await
        .unwrap();

        // The hook's own DB write is a spawned task; give it time to land.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        handle.abort();

        let mine: String =
            sqlx::query_scalar("SELECT staleness FROM processing_artifact WHERE id = 'art-mine'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(mine, "stale", "this project's artifact must flip to stale");

        let other: String =
            sqlx::query_scalar("SELECT staleness FROM processing_artifact WHERE id = 'art-other'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(other, "current", "a different project's artifact must be untouched");

        let psv: String =
            sqlx::query_scalar("SELECT state FROM prepared_source_view WHERE id = 'psv-mine'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(psv, "stale", "this project's prepared source view must flip to stale");
    }

    /// Verify that on Lagged the propagator replays from the durable events table.
    ///
    /// Strategy: pre-seed the durable table directly (no broadcast) so cursor=0
    /// at lag time, then trigger a lag via `broadcast_only` (synchronous — no
    /// yield between the two calls so the subscriber cannot run in between).
    /// Using a different topic for the heartbeat keeps the counter clean.
    #[tokio::test]
    async fn lagged_replays_from_durable_events() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let propagator = StalePropagator::new().with_hook(Arc::new(move |_env| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));

        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL, source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL, payload TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let bus = EventBus::new(pool.clone(), 1);

        let handle = propagator.spawn(&bus);

        // Let the subscriber start and begin waiting on rx.recv().
        tokio::task::yield_now().await;

        // Pre-seed 4 durable rows (not via bus.publish, so no broadcast).
        // cursor=0 at this point — no live events processed.
        for _ in 0..4 {
            persistence_lifecycle::repositories::events::insert_event(
                &pool,
                TOPIC_LIFECYCLE_TRANSITION_APPLIED,
                "system",
                "2026-01-01T00:00:00Z",
                "{}",
            )
            .await
            .unwrap();
        }

        // Trigger a lag deterministically: broadcast_only is synchronous, so the
        // subscriber cannot run between the two calls.  The channel (capacity=1)
        // overflows and the receiver gets Lagged(1).  A different topic keeps the
        // dispatch counter clean.
        let _ = bus.broadcast_only("tick.heartbeat");
        let _ = bus.broadcast_only("tick.heartbeat");

        // Deadline-yield loop: replay from cursor=0 must dispatch all 4 rows.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            tokio::task::yield_now().await;
            if counter.load(Ordering::SeqCst) >= 4 {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
        }
        handle.abort();

        assert!(
            counter.load(Ordering::SeqCst) >= 4,
            "expected at least 4 dispatches (replayed), got {}",
            counter.load(Ordering::SeqCst)
        );
    }

    /// A `Lagged` replay must never skip a durable event that the live path
    /// never delivered.
    ///
    /// This is the regression test for the cursor-jump defect (astro-plan-hyk0).
    /// The live branch used to advance the cursor with `max_event_id`, the
    /// TABLE-WIDE max, rather than the row it had just handled. Any event that
    /// reached the table without reaching this subscriber therefore sat *below*
    /// the new cursor and was never replayed — lost, not merely late.
    ///
    /// Strategy — deterministic, no race required. The lost row is inserted
    /// *before* the broadcast one, so it always carries the lower `event_id`:
    ///   1. Insert one durable row directly, with NO broadcast. The subscriber
    ///      cannot see it live; only a replay can ever deliver it.
    ///   2. Publish a second event normally, so the subscriber handles it live.
    ///      The buggy code read `max_event_id` here and jumped the cursor past
    ///      BOTH rows, stranding the one from step 1.
    ///   3. Trigger a lag with two synchronous `broadcast_only` calls (no yield
    ///      between them, so the subscriber cannot run in between → `Lagged`).
    ///   4. The replay must dispatch both durable rows: 1 live + 2 replayed = 3.
    ///
    /// Under the defect this totals 1 — the replay pages from a cursor already
    /// past the end, finds nothing, and the step-1 event is gone. Re-dispatching
    /// the live event is the accepted cost: hooks are idempotent
    /// (research.md §6.1), and re-doing work beats silently dropping it.
    #[tokio::test]
    async fn lag_replay_delivers_an_event_the_live_path_never_saw() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let propagator = StalePropagator::new().with_hook(Arc::new(move |_env| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));

        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL, source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL, payload TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let bus = EventBus::new(pool.clone(), 1);
        let handle = propagator.spawn(&bus);

        // Step 1: a durable row with NO broadcast. Inserted first, so it holds
        // the LOWEST event_id — the row a jumped cursor strands behind it.
        persistence_lifecycle::repositories::events::insert_event(
            &pool,
            TOPIC_LIFECYCLE_TRANSITION_APPLIED,
            "system",
            "2026-01-01T00:00:00Z",
            "{}",
        )
        .await
        .unwrap();

        // Step 2: publish an event via the bus (writes durable row + broadcasts).
        bus.publish(
            TOPIC_LIFECYCLE_TRANSITION_APPLIED,
            Source::User,
            LifecycleTransitionApplied {
                entity_type: EntityType::Project,
                entity_id: "test".to_owned(),
                from_state: "ready".to_owned(),
                to_state: "processing".to_owned(),
                actor: "user".to_owned(),
                at: Timestamp::now_utc(),
                project_id: Some("test".to_owned()),
            },
        )
        .await
        .unwrap();

        // Wait until the subscriber has handled the broadcast event live.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            tokio::task::yield_now().await;
            if counter.load(Ordering::SeqCst) >= 1 {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "live event not processed in time");
        }

        // Step 3: trigger Lagged deterministically (no yield between calls).
        let _ = bus.broadcast_only("tick.heartbeat");
        let _ = bus.broadcast_only("tick.heartbeat");

        // Step 4: wait for the replay to dispatch both durable rows.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            tokio::task::yield_now().await;
            if counter.load(Ordering::SeqCst) >= 3 {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
        }
        handle.abort();

        // 1 live dispatch + 2 replayed durable rows. The load-bearing part is
        // that the replay reached the step-1 row at all: under the defect the
        // cursor was already past it and the total stalls at 1.
        let total = counter.load(Ordering::SeqCst);
        assert_eq!(
            total, 3,
            "the lag replay must deliver the durable event the live path never saw \
             (1 live + 2 replayed); got {total}"
        );
    }

    /// The cursor is a high-water mark that ratchets: a SECOND lag must replay
    /// only what arrived since the first one, not the whole table again.
    ///
    /// This is the property that makes dropping the live-path advance
    /// affordable — only the first lag can ever walk history. Without this test
    /// the evidence sits entirely on one side of the invariant: the test above
    /// proves the cursor never runs ahead of what was delivered, and this one
    /// proves it does not stay behind either.
    /// Insert `n` durable lifecycle rows with no broadcast, so only a replay
    /// can ever deliver them.
    async fn seed_durable_rows(pool: &sqlx::SqlitePool, n: usize) {
        for _ in 0..n {
            persistence_lifecycle::repositories::events::insert_event(
                pool,
                TOPIC_LIFECYCLE_TRANSITION_APPLIED,
                "system",
                "2026-01-01T00:00:00Z",
                "{}",
            )
            .await
            .unwrap();
        }
    }

    /// Yield until the dispatch counter reaches `want`, or the deadline passes.
    /// Returning on timeout rather than panicking keeps the failure attributable
    /// to the caller's own assertion.
    async fn wait_for_dispatches(counter: &AtomicUsize, want: usize) {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        while counter.load(Ordering::SeqCst) < want && tokio::time::Instant::now() < deadline {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn a_second_lag_replays_only_what_arrived_since_the_first() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let propagator = StalePropagator::new().with_hook(Arc::new(move |_env| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));

        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL, source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL, payload TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let bus = EventBus::new(pool.clone(), 1);
        let handle = propagator.spawn(&bus);

        // First lag: 2 rows waiting, so the replay walks both.
        seed_durable_rows(&pool, 2).await;
        let _ = bus.broadcast_only("tick.heartbeat");
        let _ = bus.broadcast_only("tick.heartbeat");
        wait_for_dispatches(&counter, 2).await;
        assert_eq!(counter.load(Ordering::SeqCst), 2, "first lag must replay both seeded rows");

        // Second lag: 1 new row. Had the cursor not ratcheted, this replay would
        // walk all 3 rows and the total would reach 5 rather than 3.
        seed_durable_rows(&pool, 1).await;
        let _ = bus.broadcast_only("tick.heartbeat");
        let _ = bus.broadcast_only("tick.heartbeat");
        wait_for_dispatches(&counter, 3).await;
        handle.abort();

        let total = counter.load(Ordering::SeqCst);
        assert_eq!(
            total, 3,
            "the second lag must replay only the row added since the first (2 + 1), \
             not the whole table again; got {total}"
        );
    }

    #[tokio::test]
    async fn resolve_project_dependents_hook_is_a_noop_without_project_id() {
        let (db, bus) = migrated_test_bus().await;
        let propagator =
            StalePropagator::new().with_hook(resolve_project_dependents_hook(db.pool().clone()));
        let handle = propagator.spawn(&bus);

        // No project_id resolvable (e.g. a FileRecord transition) — must not
        // panic or error the dispatch loop.
        bus.publish(
            TOPIC_LIFECYCLE_TRANSITION_APPLIED,
            Source::System,
            LifecycleTransitionApplied {
                entity_type: EntityType::FileRecord,
                entity_id: "fr-unrelated".to_owned(),
                from_state: "observed".to_owned(),
                to_state: "changed".to_owned(),
                actor: "system".to_owned(),
                at: Timestamp::now_utc(),
                project_id: None,
            },
        )
        .await
        .unwrap();

        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        handle.abort();
    }
}
