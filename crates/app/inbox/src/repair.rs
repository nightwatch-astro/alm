// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Periodic repair sweep for inbox items stuck in `plan_open` (spec 005
//! T-PlanRepair, Ref: R-PlanOpen).
//!
//! [`crate::plan_listener`] is the primary path: it reacts to
//! `plan.applying.completed` / `plan.discarded` on the event bus. This module
//! is the safety net for the cases the bus cannot cover — the app was not
//! running when the plan closed, or the broadcast receiver lagged and dropped
//! the event. It re-derives the same transition from committed database state
//! instead of from an event.
#![allow(clippy::doc_markdown)]

use audit::bus::EventBus;
use contracts_core::lifecycle::PlanState;
use persistence_inbox::repositories::inbox as inbox_repo;
use sqlx::SqlitePool;
use targeting_resolver::simbad::ResolveCache;

/// Transition every inbox item whose linked plan has reached a terminal state,
/// deleting the now-stale `inbox_plan_links` row.
///
/// An `applied` plan runs the full applied path via
/// `crate::plan_listener::complete_applied_plan` — calibration-master
/// registration and light-frame ingest included. Resolving without them would
/// delete the link row, which is the only record that the work is outstanding,
/// so the master would never be registered and the frames never ingested.
///
/// Idempotent: the link row is the work queue, and each successful transition
/// removes its own row, so a second run over unchanged data is a no-op.
///
/// A failing orphan is logged and skipped rather than aborting the sweep — one
/// unrepairable row must not strand the rest. Its link survives, so the next
/// sweep retries it.
///
/// # Errors
/// Returns the error string if the orphan query itself fails.
pub async fn run_repair(
    pool: &SqlitePool,
    bus: &EventBus,
    resolve_cache: &ResolveCache,
) -> Result<usize, String> {
    let orphans = inbox_repo::find_orphaned_plan_links(pool)
        .await
        .map_err(|e| format!("find_orphaned_plan_links: {e}"))?;

    let mut repaired = 0;
    for (inbox_item_id, plan_id, plan_state) in orphans {
        // Strict-parse: quarantine rows whose `plan_state` is not a recognised
        // PlanState variant. A corrupt state string cannot be acted on safely —
        // skip and log so the link row survives for manual inspection.
        let Some(typed_state) = PlanState::parse_str(&plan_state) else {
            tracing::error!(
                %inbox_item_id,
                %plan_id,
                %plan_state,
                "inbox repair: quarantining orphan — unrecognised plan state"
            );
            continue;
        };

        // Same mapping as the listener: only `applied` resolves; every other
        // terminal state goes back to unconfirmed (derived from the row's own
        // frame_type, spec 058 SC-003).
        let outcome = if typed_state == PlanState::Applied {
            crate::plan_listener::complete_applied_plan(pool, bus, resolve_cache, &plan_id).await
        } else {
            crate::plan_listener::transition_via_plan_id(pool, &plan_id, None).await
        };

        match outcome {
            Ok(()) => repaired += 1,
            Err(e) => tracing::warn!(
                %inbox_item_id,
                %plan_id,
                %plan_state,
                "inbox repair: transition failed: {e}"
            ),
        }
    }

    if repaired > 0 {
        tracing::info!(repaired, "inbox repair: swept orphaned plan links");
    }
    Ok(repaired)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_listener::tests::{setup_master_item_plan, test_db};
    use persistence_core::Database;
    use persistence_inbox::repositories::inbox::InsertInboxItem;
    use persistence_plans::repositories::plans;

    fn make_bus(db: &Database) -> EventBus {
        EventBus::with_pool(db.pool().clone())
    }

    /// `run_repair` with throwaway event-bus/resolver handles, for the cases
    /// that assert inbox state rather than resolver behaviour.
    async fn sweep(db: &Database) -> usize {
        run_repair(db.pool(), &make_bus(db), &ResolveCache::in_memory().unwrap()).await.unwrap()
    }

    /// Seed a `plan_open` inbox item linked to a plan left in `plan_state`.
    /// `frame_type = None` models the spec-058 SC-003 row that must fall back
    /// to `pending_classification` instead of `classified`.
    async fn seed(
        db: &Database,
        item_id: &str,
        plan_id: &str,
        plan_state: Option<&str>,
        frame_type: Option<&str>,
    ) {
        let pool = db.pool();
        inbox_repo::insert_inbox_item(
            pool,
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: item_id,
                file_count: 1,
                content_signature: Some("sig"),
                lane: "fits",
            },
        )
        .await
        .unwrap();

        if let Some(ft) = frame_type {
            sqlx::query("UPDATE inbox_items SET frame_type = ? WHERE id = ?")
                .bind(ft)
                .bind(item_id)
                .execute(pool)
                .await
                .unwrap();
        }

        inbox_repo::update_inbox_item_state(pool, item_id, "plan_open").await.unwrap();

        plans::insert_plan(
            pool,
            &plans::InsertPlan {
                id: plan_id,
                title: "Test plan",
                origin: "inbox",
                origin_path: None,
                plan_type: "split",
                destructive_destination: "archive",
                parent_plan_id: None,
                total_bytes_required: 0,
            },
        )
        .await
        .unwrap();

        if let Some(state) = plan_state {
            plans::update_plan_state(pool, plan_id, state).await.unwrap();
        }

        inbox_repo::insert_plan_link(pool, item_id, plan_id).await.unwrap();
    }

    async fn state_of(db: &Database, item_id: &str) -> String {
        inbox_repo::get_inbox_item(db.pool(), item_id).await.unwrap().state
    }

    async fn link_exists(db: &Database, item_id: &str) -> bool {
        inbox_repo::get_plan_link(db.pool(), item_id).await.unwrap().is_some()
    }

    #[tokio::test]
    async fn applied_plan_repairs_item_to_resolved() {
        let db = test_db().await;
        seed(&db, "item-applied", "plan-applied", Some("applied"), Some("light")).await;

        assert_eq!(sweep(&db).await, 1);

        assert_eq!(state_of(&db, "item-applied").await, "resolved");
        assert!(!link_exists(&db, "item-applied").await);
    }

    #[tokio::test]
    async fn non_applied_terminals_return_to_unconfirmed_and_drop_the_link() {
        let db = test_db().await;
        for terminal in ["partially_applied", "failed", "cancelled", "discarded"] {
            seed(
                &db,
                &format!("item-{terminal}"),
                &format!("plan-{terminal}"),
                Some(terminal),
                Some("light"),
            )
            .await;
        }

        assert_eq!(sweep(&db).await, 4);

        for terminal in ["partially_applied", "failed", "cancelled", "discarded"] {
            let item_id = format!("item-{terminal}");
            assert_eq!(state_of(&db, &item_id).await, "classified", "{terminal}");
            assert!(!link_exists(&db, &item_id).await, "{terminal}");
        }
    }

    #[tokio::test]
    async fn item_without_frame_type_returns_to_pending_classification() {
        let db = test_db().await;
        seed(&db, "item-noft", "plan-noft", Some("failed"), None).await;

        sweep(&db).await;

        assert_eq!(state_of(&db, "item-noft").await, "pending_classification");
        assert!(!link_exists(&db, "item-noft").await);
    }

    #[tokio::test]
    async fn second_run_is_a_no_op() {
        let db = test_db().await;
        seed(&db, "item-idem", "plan-idem", Some("applied"), Some("light")).await;

        assert_eq!(sweep(&db).await, 1);
        assert_eq!(sweep(&db).await, 0);
        assert_eq!(state_of(&db, "item-idem").await, "resolved");
    }

    #[tokio::test]
    async fn open_plan_is_left_untouched() {
        let db = test_db().await;
        // No state update: `plans.state` keeps its `draft` default.
        seed(&db, "item-open", "plan-open", None, Some("light")).await;

        assert_eq!(sweep(&db).await, 0);

        assert_eq!(state_of(&db, "item-open").await, "plan_open");
        assert!(link_exists(&db, "item-open").await);
    }

    // ── Applied-path side effects on the sweep ────────────────────────────────

    async fn calibration_sessions_for(db: &Database, item_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM calibration_session WHERE source_inbox_item_id = ?",
        )
        .bind(item_id)
        .fetch_one(db.pool())
        .await
        .unwrap()
    }

    /// The sweep must run the same applied-path side effects as the listener.
    /// Resolving the item alone deletes `inbox_plan_links`, which is the sweep's
    /// only work queue — the master would then never be registered, with nothing
    /// left to retry from. This is exactly the crash/restart case the sweep
    /// exists to cover.
    #[tokio::test]
    async fn applied_orphan_swept_after_a_missed_event_registers_the_master() {
        let db = test_db().await;
        let tmp = tempfile::tempdir().unwrap();
        setup_master_item_plan(&db, tmp.path(), "sweep-master", "sweep-master-plan", 2048).await;
        plans::update_plan_state(db.pool(), "sweep-master-plan", "applied").await.unwrap();

        assert_eq!(sweep(&db).await, 1);

        assert_eq!(
            calibration_sessions_for(&db, "sweep-master").await,
            1,
            "the sweep must register the calibration master, not just mark the item resolved"
        );
        assert_eq!(state_of(&db, "sweep-master").await, "resolved");
        assert!(!link_exists(&db, "sweep-master").await);
    }

    #[tokio::test]
    async fn concurrent_listener_and_sweep_register_one_master() {
        let db = test_db().await;
        let tmp = tempfile::tempdir().unwrap();
        setup_master_item_plan(&db, tmp.path(), "race-master", "race-master-plan", 2048).await;
        plans::update_plan_state(db.pool(), "race-master-plan", "applied").await.unwrap();

        let bus = make_bus(&db);
        let resolve_cache = ResolveCache::in_memory().unwrap();
        let start = std::sync::Arc::new(tokio::sync::Barrier::new(3));

        let listener = async {
            start.wait().await;
            crate::plan_listener::complete_applied_plan(
                db.pool(),
                &bus,
                &resolve_cache,
                "race-master-plan",
            )
            .await
        };
        let sweep = async {
            start.wait().await;
            run_repair(db.pool(), &bus, &resolve_cache).await
        };
        let (_, (listener_result, sweep_result)) =
            tokio::join!(start.wait(), async { tokio::join!(listener, sweep) });

        listener_result.unwrap();
        sweep_result.unwrap();
        assert_eq!(calibration_sessions_for(&db, "race-master").await, 1);
        let fingerprints = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM calibration_fingerprint cf
             JOIN calibration_session cs ON cs.id = cf.id
             WHERE cs.source_inbox_item_id = ?",
        )
        .bind("race-master")
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(fingerprints, 1);
    }

    /// Ordering invariant: the link row outlives a failed side effect, so the
    /// next sweep retries. Here the applied destination names a root that was
    /// never registered, so `calibration_session.root_id` cannot satisfy its
    /// `library_root` foreign key and master registration fails.
    #[tokio::test]
    async fn failed_side_effect_keeps_the_link_for_the_next_sweep() {
        let db = test_db().await;
        sqlx::query("PRAGMA foreign_keys = ON").execute(db.pool()).await.unwrap();
        let tmp = tempfile::tempdir().unwrap();
        setup_master_item_plan(&db, tmp.path(), "sweep-fail", "sweep-fail-plan", 2048).await;
        sqlx::query(
            "UPDATE plan_items SET to_root_id = 'ghost-root', from_root_id = 'ghost-root'
             WHERE plan_id = 'sweep-fail-plan'",
        )
        .execute(db.pool())
        .await
        .unwrap();
        plans::update_plan_state(db.pool(), "sweep-fail-plan", "applied").await.unwrap();

        assert_eq!(sweep(&db).await, 0, "a failed orphan must not count as repaired");

        assert_eq!(calibration_sessions_for(&db, "sweep-fail").await, 0);
        assert!(
            link_exists(&db, "sweep-fail").await,
            "the link is the only work queue — deleting it would strand the master permanently"
        );
        assert_eq!(state_of(&db, "sweep-fail").await, "plan_open");
    }
}
