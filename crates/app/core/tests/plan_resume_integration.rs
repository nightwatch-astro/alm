#![allow(clippy::doc_markdown)]
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for `resume_plan` (spec 025 R-Pause-1, issue #575).
//!
//! Prior to this fix, `resume_plan` flipped `paused -> applying` and emitted
//! `plan.applying.resumed` unconditionally, without re-validating the pause
//! condition and without restarting the executor — the run then sat in
//! `applying` forever with nothing running. These tests exercise the full
//! Layer-1 path (real SQLite + real tempdir files + real executor) end to
//! end: refusal while each of the three R-Pause-1 conditions persists, and a
//! genuine resume that finishes the plan's remaining `pending` items.
//!
//! `volume.unavailable`/`disk.full` cannot be triggered through ordinary
//! tempdir operations (they require a raw OS `ENODEV`/`ENXIO`/`StorageFull`
//! error — see `crates/fs/executor/src/failure.rs::classify_io_error` and
//! the note in `plan_apply_lifecycle_integration.rs`), so those tests seed
//! the paused DB state directly via the same repository calls the executor
//! itself would have made, then exercise `resume_plan`'s re-validation
//! against a real (but deliberately unavailable/undersized) probe target.

mod support;

use persistence_plans::repositories::plan_apply as apply_repo;
use persistence_plans::repositories::plans as plans_repo;
use uuid::Uuid;

/// Fetch plan_apply_events rows for a plan as `(item_id, prior_state, new_state)`.
async fn list_events_raw(
    pool: &sqlx::SqlitePool,
    plan_id: &str,
) -> Vec<(Option<String>, String, String)> {
    sqlx::query_as::<_, (Option<String>, String, String)>(
        "SELECT item_id, prior_state, new_state FROM plan_apply_events WHERE plan_id = ? ORDER BY at ASC",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await
    .expect("list_events_raw")
}

/// Seed one item into `failed` state without going through the executor.
/// Replaces the deleted `item_start_applying` + `item_failed` pair —
/// `batch_flush_item_states` goes pending → terminal in one step,
/// matching the group-commit path the real executor now uses.
async fn seed_item_failed(
    pool: &sqlx::SqlitePool,
    item_id: &str,
    plan_id: &str,
    failure_reason: &str,
) {
    let mut conn = pool.acquire().await.expect("acquire connection");
    apply_repo::batch_flush_item_states(
        &mut conn,
        plan_id,
        &[apply_repo::BatchItemState {
            item_id,
            new_state: "failed",
            failure_reason: Some(failure_reason),
            is_stale: false,
        }],
        0,
        1,
        0,
    )
    .await
    .expect("seed_item_failed");
}

/// Register a `registered_sources` root row pointing at a real directory, so
/// `resolve_root_path`/the T023a root-map machinery has something real to
/// resolve (required for the volume/disk probes, which target the plan
/// item's root path).
async fn register_root(pool: &sqlx::SqlitePool, path: &std::path::Path) -> String {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO registered_sources \
         (id, kind, path, scan_depth, created_at, created_via, organization_state) \
         VALUES (?, 'light_frames', ?, 'recursive', '2026-01-01T00:00:00Z', 'first_run', 'organized')",
    )
    .bind(&id)
    .bind(path.to_str().expect("utf-8 root path"))
    .execute(pool)
    .await
    .expect("insert registered_sources row");
    id
}

/// Insert a draft plan with `item_count` move items, each relative to
/// `root_id`, then advance to `approved`. Mirrors
/// `plan_apply_lifecycle_integration::seed_multi_item_approved_plan` but
/// rooted (T023a) rather than legacy-absolute, since the volume/disk probes
/// resolve items via their registered root.
async fn seed_rooted_approved_plan(
    pool: &sqlx::SqlitePool,
    plan_id: &str,
    root_id: &str,
    dir: &std::path::Path,
    item_count: usize,
) -> Vec<(std::path::PathBuf, std::path::PathBuf)> {
    plans_repo::insert_plan(
        pool,
        &plans_repo::InsertPlan {
            id: plan_id,
            title: "Resume Integration Test Plan",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .expect("insert_plan");

    let mut paths = Vec::with_capacity(item_count);
    for i in 0..item_count {
        let src = dir.join(format!("source-{i}.fits"));
        let dst_rel = format!("processed/dest-{i}.fits");
        std::fs::write(&src, format!("content-{i}")).expect("write src");

        plans_repo::insert_plan_item(
            pool,
            &plans_repo::InsertPlanItem {
                id: &format!("{plan_id}-item-{i}"),
                plan_id,
                item_index: i64::try_from(i + 1).expect("item index fits i64"),
                name: &format!("file-{i}.fits"),
                action: "move",
                from_root_id: Some(root_id),
                from_relative_path: &format!("source-{i}.fits"),
                to_root_id: Some(root_id),
                to_relative_path: &dst_rel,
                reason: "integration test",
                protection: "normal",
                linked_entity: None,
                provenance_json: None,
                archive_path: None,
                source_id: None,
                category: None,
            },
        )
        .await
        .expect("insert_plan_item");

        paths.push((src, dir.join(&dst_rel)));
    }

    plans_repo::update_plan_state(pool, plan_id, "ready_for_review")
        .await
        .expect("update to ready_for_review");
    plans_repo::set_approved(pool, plan_id, "2026-07-13T00:00:00Z", "tok-test-fixed")
        .await
        .expect("set_approved");

    paths
}

// ── item.still.stale ──────────────────────────────────────────────────────────

/// Resume must refuse (not flip state, not spawn the executor) while a
/// paused run's stale item's CAS mismatch is still present.
#[tokio::test]
async fn resume_refused_while_item_still_stale() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    let paths = seed_rooted_approved_plan(db.pool(), &plan_id, &root_id, dir.path(), 2).await;
    let (src0, _) = &paths[0];
    let (src1, dst1) = &paths[1];

    // Force item 0 stale: recorded approval-time size does not match reality.
    let real_size = i64::try_from(std::fs::metadata(src0).expect("stat src0").len()).unwrap();
    plans_repo::update_item_fs_snapshot(
        db.pool(),
        &format!("{plan_id}-item-0"),
        None,
        Some(real_size + 1),
    )
    .await
    .expect("update_item_fs_snapshot");

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should start");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "paused", "run must have paused on the stale item");

    let run_row = apply_repo::get_active_run(db.pool(), &plan_id)
        .await
        .expect("get_active_run")
        .expect("a paused run must still have its run row");
    let events_before = list_events_raw(db.pool(), &plan_id).await;

    // The condition is NOT resolved (snapshot still mismatched) — resume must refuse.
    let err = app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, &run_row.id)
        .await
        .expect_err("resume must be refused while the item is still stale");
    assert_eq!(err.code, contracts_core::error_code::ErrorCode::ItemStillStale);

    // State untouched: still paused, no new audit events, item 1 never ran.
    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "paused", "a refused resume must not flip state");
    let events_after = list_events_raw(db.pool(), &plan_id).await;
    assert_eq!(
        events_before.len(),
        events_after.len(),
        "a refused resume must not append any audit events or run the executor"
    );
    assert!(src1.exists(), "item 1 must not have been touched by a refused resume");
    assert!(!dst1.exists(), "item 1's destination must not exist");
}

/// Once the stale item's approval-time snapshot is corrected to match
/// reality, resume must succeed, restart the executor over the plan's
/// remaining `pending` items, and reach a terminal state. The item that
/// triggered the pause stays `failed` (terminal) for this run — only item 1
/// (still `pending`) is re-driven.
#[tokio::test]
async fn resume_succeeds_after_stale_item_resolved_and_drains_remaining_pending() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    let paths = seed_rooted_approved_plan(db.pool(), &plan_id, &root_id, dir.path(), 2).await;
    let (src0, _dst0) = &paths[0];
    let (src1, dst1) = &paths[1];

    let real_size = i64::try_from(std::fs::metadata(src0).expect("stat src0").len()).unwrap();
    plans_repo::update_item_fs_snapshot(
        db.pool(),
        &format!("{plan_id}-item-0"),
        None,
        Some(real_size + 1),
    )
    .await
    .expect("update_item_fs_snapshot");

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should start");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "paused");
    let run_row = apply_repo::get_active_run(db.pool(), &plan_id)
        .await
        .expect("get_active_run")
        .expect("paused run row");

    // Resolve the condition: correct the recorded snapshot to match reality.
    plans_repo::update_item_fs_snapshot(
        db.pool(),
        &format!("{plan_id}-item-0"),
        None,
        Some(real_size),
    )
    .await
    .expect("update_item_fs_snapshot to resolve staleness");

    let resp = app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, &run_row.id)
        .await
        .expect("resume must succeed once the stale condition is resolved");
    assert_eq!(resp.plan_id, plan_id);

    support::wait_plan_terminal(db.pool(), &plan_id).await;

    assert!(!src1.exists(), "item 1's source should have been moved by the resumed run");
    assert!(dst1.exists(), "item 1's destination should exist after the resumed run");

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(
        plan_row.state, "partially_applied",
        "item 0 stays failed (terminal, not retried) while item 1 succeeds"
    );
    assert_eq!(plan_row.items_applied, 1, "item 1 succeeded");
    assert_eq!(plan_row.items_failed, 1, "item 0 stays failed from the original pause");

    // Audit trail carries the resume transition.
    let events = list_events_raw(db.pool(), &plan_id).await;
    assert!(
        events.iter().any(|e| e.1 == "paused" && e.2 == "applying"),
        "audit trail must record the paused -> applying resume transition"
    );
    assert!(
        events.iter().any(|e| e.1 == "applying" && e.2 == "partially_applied"),
        "audit trail must record the final terminal transition"
    );
}

// ── volume.still.unavailable ──────────────────────────────────────────────────

/// Directly seed a paused run whose pause reason is `volume.unavailable`
/// (bypassing the real executor, since a portable test cannot force a raw
/// `ENODEV`/`ENXIO`), pointed at a root directory that has been removed —
/// simulating a still-disconnected drive. Resume must refuse.
#[tokio::test]
async fn resume_refused_while_volume_still_unavailable() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    let paths = seed_rooted_approved_plan(db.pool(), &plan_id, &root_id, dir.path(), 2).await;
    let (_src1, dst1) = &paths[1];
    let item0_id = format!("{plan_id}-item-0");
    let run_id = Uuid::new_v4().to_string();

    // Simulate the executor having already attempted + paused on item 0.
    apply_repo::cas_approved_to_applying(db.pool(), &plan_id, &run_id, "tok-test-fixed", 2, 2)
        .await
        .expect("cas_approved_to_applying");
    seed_item_failed(
        db.pool(),
        &item0_id,
        &plan_id,
        "volume.unavailable: move failed: simulated disconnect",
    )
    .await;
    apply_repo::pause_run(db.pool(), &plan_id, &run_id, "volume.unavailable", 0, 1, 0, 0, 1)
        .await
        .expect("pause_run");

    // The registered root's directory is now gone — the volume is still
    // "unavailable" from the probe's perspective.
    std::fs::remove_dir_all(dir.path()).expect("simulate volume disconnect");

    let err = app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, &run_id)
        .await
        .expect_err("resume must be refused while the volume is still unavailable");
    assert_eq!(err.code, contracts_core::error_code::ErrorCode::VolumeStillUnavailable);

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "paused", "a refused resume must not flip state");
    assert!(!dst1.exists(), "item 1 must not have been touched by a refused resume");
}

/// Once the volume is reachable again (the directory is recreated), resume
/// must succeed and drain the remaining pending item.
#[tokio::test]
async fn resume_succeeds_after_volume_available_again() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    let paths = seed_rooted_approved_plan(db.pool(), &plan_id, &root_id, dir.path(), 2).await;
    let (_src1, dst1) = &paths[1];
    let item0_id = format!("{plan_id}-item-0");
    let run_id = Uuid::new_v4().to_string();

    apply_repo::cas_approved_to_applying(db.pool(), &plan_id, &run_id, "tok-test-fixed", 2, 2)
        .await
        .expect("cas_approved_to_applying");
    seed_item_failed(
        db.pool(),
        &item0_id,
        &plan_id,
        "volume.unavailable: move failed: simulated disconnect",
    )
    .await;
    apply_repo::pause_run(db.pool(), &plan_id, &run_id, "volume.unavailable", 0, 1, 0, 0, 1)
        .await
        .expect("pause_run");

    // Volume reconnected: the root directory exists again (never actually
    // removed here, but re-assert to make the "resolved" precondition explicit).
    assert!(dir.path().exists());

    let resp = app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, &run_id)
        .await
        .expect("resume must succeed once the volume is reachable again");
    assert_eq!(resp.plan_id, plan_id);

    support::wait_plan_terminal(db.pool(), &plan_id).await;

    assert!(dst1.exists(), "item 1's destination should exist after the resumed run");
    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "partially_applied");
    assert_eq!(plan_row.items_applied, 1);
    assert_eq!(plan_row.items_failed, 1, "item 0 stays failed from the original pause");
}

// ── disk.still.full ────────────────────────────────────────────────────────────

/// Directly seed a paused run whose pause reason is `disk.full`, with the
/// paused item's recorded size set absurdly high so the re-validation probe
/// deterministically finds insufficient free space on any real volume.
/// Resume must refuse.
#[tokio::test]
async fn resume_refused_while_disk_still_full() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    let paths = seed_rooted_approved_plan(db.pool(), &plan_id, &root_id, dir.path(), 2).await;
    let (_src1, dst1) = &paths[1];
    let item0_id = format!("{plan_id}-item-0");
    let run_id = Uuid::new_v4().to_string();

    // No real volume has an exabyte free — deterministic without faking a full disk.
    plans_repo::update_item_fs_snapshot(db.pool(), &item0_id, None, Some(i64::MAX / 2))
        .await
        .expect("update_item_fs_snapshot with an absurd required size");

    apply_repo::cas_approved_to_applying(db.pool(), &plan_id, &run_id, "tok-test-fixed", 2, 2)
        .await
        .expect("cas_approved_to_applying");
    seed_item_failed(
        db.pool(),
        &item0_id,
        &plan_id,
        "disk.full: move failed: simulated storage-full",
    )
    .await;
    apply_repo::pause_run(db.pool(), &plan_id, &run_id, "disk.full", 0, 1, 0, 0, 1)
        .await
        .expect("pause_run");

    let err = app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, &run_id)
        .await
        .expect_err("resume must be refused while the destination volume is still full");
    assert_eq!(err.code, contracts_core::error_code::ErrorCode::DiskStillFull);

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "paused", "a refused resume must not flip state");
    assert!(!dst1.exists(), "item 1 must not have been touched by a refused resume");
}

/// Once the destination volume has enough free space (a small, satisfiable
/// required size), resume must succeed and drain the remaining pending item.
#[tokio::test]
async fn resume_succeeds_after_disk_space_available_again() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    let paths = seed_rooted_approved_plan(db.pool(), &plan_id, &root_id, dir.path(), 2).await;
    let (_src1, dst1) = &paths[1];
    let item0_id = format!("{plan_id}-item-0");
    let run_id = Uuid::new_v4().to_string();

    // A tiny required size — any real tempdir volume satisfies this.
    plans_repo::update_item_fs_snapshot(db.pool(), &item0_id, None, Some(1))
        .await
        .expect("update_item_fs_snapshot with a small satisfiable required size");

    apply_repo::cas_approved_to_applying(db.pool(), &plan_id, &run_id, "tok-test-fixed", 2, 2)
        .await
        .expect("cas_approved_to_applying");
    seed_item_failed(
        db.pool(),
        &item0_id,
        &plan_id,
        "disk.full: move failed: simulated storage-full",
    )
    .await;
    apply_repo::pause_run(db.pool(), &plan_id, &run_id, "disk.full", 0, 1, 0, 0, 1)
        .await
        .expect("pause_run");

    let resp = app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, &run_id)
        .await
        .expect("resume must succeed once there is enough free space again");
    assert_eq!(resp.plan_id, plan_id);

    support::wait_plan_terminal(db.pool(), &plan_id).await;

    assert!(dst1.exists(), "item 1's destination should exist after the resumed run");
    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "partially_applied");
    assert_eq!(plan_row.items_applied, 1);
    assert_eq!(plan_row.items_failed, 1, "item 0 stays failed from the original pause");
}

// ── cancellation during a resumed run ────────────────────────────────────────

/// `cancel_plan` must be able to signal a run that was restarted by
/// `resume_plan` — the resumed run re-registers a *fresh* `ActiveRun`
/// (cancel token / skip set / retry queue do not carry over a pause
/// boundary), so this proves `cancel_plan`'s registry lookup finds that new
/// entry rather than a stale/missing one from the original (already-dropped)
/// run.
///
/// Cancellation is checked once *between* items, never mid-item (run.rs),
/// so exactly how many of the many pending items below finish before the
/// signal lands is a genuine scheduling race — asserting on the aggregate
/// outcome (run reaches `cancelled`, and it does so before every one of 200
/// items completed) is what's deterministic, not which specific item was
/// last. A real per-item filesystem move (via `spawn_blocking`, run.rs:501)
/// is slow enough relative to `cancel_plan`'s single DB read that this
/// reliably lands mid-run rather than after the whole (tiny, in-memory) run
/// already finished.
#[tokio::test]
async fn cancel_signals_the_executor_restarted_by_resume() {
    const PENDING_ITEMS: usize = 200;

    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    let paths =
        seed_rooted_approved_plan(db.pool(), &plan_id, &root_id, dir.path(), PENDING_ITEMS + 1)
            .await;
    let item0_id = format!("{plan_id}-item-0");
    let run_id = Uuid::new_v4().to_string();

    apply_repo::cas_approved_to_applying(
        db.pool(),
        &plan_id,
        &run_id,
        "tok-test-fixed",
        i64::try_from(PENDING_ITEMS + 1).unwrap(),
        i64::try_from(PENDING_ITEMS + 1).unwrap(),
    )
    .await
    .expect("cas_approved_to_applying");
    seed_item_failed(
        db.pool(),
        &item0_id,
        &plan_id,
        "volume.unavailable: move failed: simulated disconnect",
    )
    .await;
    apply_repo::pause_run(
        db.pool(),
        &plan_id,
        &run_id,
        "volume.unavailable",
        0,
        1,
        0,
        0,
        i64::try_from(PENDING_ITEMS).unwrap(),
    )
    .await
    .expect("pause_run");

    app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, &run_id)
        .await
        .expect("resume must succeed (volume reachable again)");

    // No `.await` yield point between resume and cancel — gives the
    // restarted executor task the least possible head start.
    app_core::plan_apply::cancel_plan(db.pool(), &bus, &plan_id)
        .await
        .expect("cancel must find the resumed run's freshly re-registered ActiveRun");

    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(
        plan_row.state, "cancelled",
        "the resumed run must reach 'cancelled', proving cancel_plan signalled it"
    );
    assert!(
        plan_row.items_cancelled > 0,
        "cancellation must have interrupted at least one of the {PENDING_ITEMS} pending items \
         (items_cancelled = {}); a run that completed before the signal landed would falsify \
         this test, not confirm it",
        plan_row.items_cancelled
    );
    assert_eq!(
        plan_row.items_applied + plan_row.items_cancelled,
        i64::try_from(PENDING_ITEMS).unwrap(),
        "every previously-pending item must be accounted for as either applied or cancelled"
    );
    assert_eq!(plan_row.items_failed, 1, "item 0 stays failed from the original pause");

    // None of the paths that ended up cancelled were touched.
    let items = plans_repo::list_plan_items(db.pool(), &plan_id).await.expect("list_plan_items");
    for (i, (src, dst)) in paths.iter().enumerate().skip(1) {
        let item = items.iter().find(|it| it.id == format!("{plan_id}-item-{i}")).unwrap();
        if item.item_state == "cancelled" {
            assert!(src.exists(), "item {i} was cancelled but its source is missing");
            assert!(!dst.exists(), "item {i} was cancelled but its destination exists");
        }
    }
}

// ── run.not_paused / run.not_found — resume guards unchanged by this fix ─────

/// Resume on a plan that is not paused must still be rejected (guard
/// unchanged by this fix; re-validation only applies once a resume is
/// otherwise eligible).
#[tokio::test]
async fn resume_refused_when_plan_not_paused() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    seed_rooted_approved_plan(db.pool(), &plan_id, &root_id, dir.path(), 1).await;

    let err = app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, "no-such-run")
        .await
        .expect_err("an approved (not paused) plan must refuse resume");
    assert_eq!(err.code, contracts_core::error_code::ErrorCode::RunNotPaused);
}

// ── resume + retry item-set agreement (review fix) ────────────────────────────

/// Review fix regression: `resume_plan` previously filtered `executor_items`
/// to `item_state == "pending"` only, excluding the item that caused the
/// original pause (left `failed`, terminal, once resumed). A `retry_plan_item`
/// call against that pre-pause-failed item — perfectly legal per its own
/// state-machine guards (`plan` is `applying`, item is `failed`) — flipped
/// its DB row `failed -> applying` and queued it, but the executor's
/// `item_by_id` lookup (built only from the filtered, pending-only list) had
/// no entry for it: the retry was silently dropped, leaving the item stuck
/// `applying` forever with no terminal audit record and `items_total`
/// permanently under-accounted.
///
/// Exercises the exact real sequence end to end (real SQLite + real tempdir
/// files + the real spawned executor, no seeding shortcuts): pause on a
/// stale item, resolve the staleness, resume, then retry the pre-pause-failed
/// item while the resumed run is still draining its other pending items.
///
/// `fs_executor::run::pacing` parks the resumed forward pass at its first item
/// boundary, and this test files the retry only after observing that arrival, so
/// "the run is still draining" is an ordering it asserts rather than a duration
/// it waits out. Item count alone does not establish it: `retry_plan_item` is
/// refused once the run closes its retry queue, and 30 small same-directory
/// moves against a warm pool can finish inside this test's three awaits
/// (`astro-plan-ytx7`).
#[tokio::test]
async fn resume_then_retry_of_pre_pause_failed_item_reaches_terminal_state() {
    const OTHER_PENDING_ITEMS: usize = 30;

    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let root_id = register_root(db.pool(), dir.path()).await;

    let paths = seed_rooted_approved_plan(
        db.pool(),
        &plan_id,
        &root_id,
        dir.path(),
        OTHER_PENDING_ITEMS + 1,
    )
    .await;
    let (src0, dst0) = &paths[0];
    let item0_id = format!("{plan_id}-item-0");

    // Force item 0 stale, same as the other pause tests above.
    let real_size = i64::try_from(std::fs::metadata(src0).expect("stat src0").len()).unwrap();
    plans_repo::update_item_fs_snapshot(db.pool(), &item0_id, None, Some(real_size + 1))
        .await
        .expect("update_item_fs_snapshot");

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should start");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "paused", "run must have paused on the stale item");
    let run_row = apply_repo::get_active_run(db.pool(), &plan_id)
        .await
        .expect("get_active_run")
        .expect("a paused run must still have its run row");

    // Resolve the condition so resume succeeds.
    plans_repo::update_item_fs_snapshot(db.pool(), &item0_id, None, Some(real_size))
        .await
        .expect("update_item_fs_snapshot to resolve staleness");

    // Scoped to this plan, so a test running concurrently in the same binary
    // cannot consume this gate's arrival or release permits. Installed after the
    // first apply has already reached a terminal state, so only the resumed run
    // — the one that has to still be draining when the retry arrives — is
    // gated. Dropping the gate releases every remaining boundary, including on
    // panic unwind, so a failed assertion below cannot strand the spawned
    // executor.
    let gate = fs_executor::run::pacing::ItemGate::install(&plan_id);

    app_core::plan_apply::resume_plan(db.pool(), &bus, &plan_id, &run_row.id)
        .await
        .expect("resume must succeed once the stale condition is resolved");

    // The resumed pass has parked at an item boundary: its retry queue is open
    // and it has not yet closed the run.
    gate.wait_for_arrival().await;
    let gated = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(gated.state, "applying", "the gated run must still be mid-apply");

    // Retry item 0, still `failed` from the pre-pause attempt. `retry_plan_item`
    // writes the item row directly (not through the progress buffer), so the
    // transition is observable here.
    app_core::plan_apply::retry_plan_item(db.pool(), &plan_id, &item0_id)
        .await
        .expect("retry must be accepted: plan is applying, item is failed, run is active");
    let queued = plans_repo::list_plan_items(db.pool(), &plan_id).await.expect("list_plan_items");
    let queued0 = queued.iter().find(|i| i.id == item0_id).expect("item 0 row");
    assert_eq!(
        queued0.item_state, "applying",
        "the accepted retry must flip item 0 out of `failed` before the run drains it"
    );
    assert!(src0.exists(), "the queued retry must not have executed while the run is gated");

    // Release exactly one item. The loop drains the retry queue after every
    // item, so the next arrival is strictly after item 0 was re-executed —
    // making "the retry ran mid-run, not after the queue closed" an assertion
    // rather than an inference.
    gate.release(1);
    gate.wait_for_arrival().await;
    assert!(!src0.exists(), "the mid-run drain must have re-executed item 0's move");
    assert!(dst0.exists(), "item 0's destination must exist after the mid-run drain");

    // Release the rest of the run.
    drop(gate);

    // Wait on ITEM 0 specifically, not on the plan. The plan can already hold a
    // terminal state left over from the pre-retry attempt, in which case
    // `wait_plan_terminal` returns instantly and every assertion below races the
    // restarted executor (astro-plan-9lkf). Item 0 reaching `succeeded` is the
    // completion signal for the work this test actually triggered.
    support::wait_item_state(db.pool(), &item0_id, "succeeded").await;
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    // The retried item actually re-executed for real (not just a DB flip):
    // its source file really moved.
    assert!(!src0.exists(), "item 0's source should have been moved by the retried re-execution");
    assert!(dst0.exists(), "item 0's destination should exist after the retry");

    let items = plans_repo::list_plan_items(db.pool(), &plan_id).await.expect("list_plan_items");
    let item0 = items.iter().find(|i| i.id == item0_id).expect("item 0 row");
    assert_eq!(
        item0.item_state, "succeeded",
        "retried item must reach a real terminal state, not stay stuck 'applying'"
    );

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "applied", "every item (including the retried one) succeeded");
    assert_eq!(
        plan_row.items_applied,
        i64::try_from(OTHER_PENDING_ITEMS + 1).unwrap(),
        "items_total must not be under-accounted — the retried item counts too"
    );
    assert_eq!(plan_row.items_failed, 0, "the retry cleared item 0's failed count");

    // A real terminal audit record landed for the retry (applying -> succeeded),
    // not silence.
    let events = list_events_raw(db.pool(), &plan_id).await;
    assert!(
        events.iter().any(|e| e.0.as_deref() == Some(item0_id.as_str())
            && e.1 == "applying"
            && e.2 == "succeeded"),
        "the retry must append a real terminal audit event for item 0, not leave it silent"
    );
}
