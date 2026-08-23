#![allow(clippy::doc_markdown)]
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for plan apply, filesystem execution, and audit records
//! (spec 017 cleanup/archive plans #17, spec 025 filesystem plan apply #18,
//! audit record #22).
//!
//! These tests exercise the full Layer-1 path:
//!   seed plan → approve → apply → executor runs real FS → audit events written.
//!
//! The harness uses an in-memory SQLite database (via `support::setup()`).
//! All filesystem operations use `tempfile::tempdir()` — never real user paths.

mod support;

use fs_executor::failure::FailureCode;
use persistence_plans::repositories::plans as plans_repo;
use uuid::Uuid;

// ── Seeding helpers ───────────────────────────────────────────────────────────

/// Insert a draft plan, add `item_count` move-action items pointing at real
/// tempdir paths, then transition plan to `approved` with a known token.
///
/// Returns `(plan_id, approval_token, tempdir)`.  Caller must hold `_dir`
/// alive for the duration of the test so the tempfile paths remain valid.
async fn seed_approved_plan_with_real_files(
    pool: &sqlx::SqlitePool,
    plan_id: &str,
    src_path: &std::path::Path,
    dst_path: &std::path::Path,
) {
    // Insert plan row (draft state).
    plans_repo::insert_plan(
        pool,
        &plans_repo::InsertPlan {
            id: plan_id,
            title: "Integration Test Plan",
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

    // Insert one move item pointing at the real tempdir paths.
    // approved_mtime/approved_size_bytes are NULL → CAS check skipped (permissive).
    plans_repo::insert_plan_item(
        pool,
        &plans_repo::InsertPlanItem {
            id: &format!("{plan_id}-item-0"),
            plan_id,
            item_index: 1,
            name: "file.fits",
            action: "move",
            from_root_id: None,
            from_relative_path: src_path.to_str().expect("utf-8 src path"),
            to_root_id: None,
            to_relative_path: dst_path.to_str().expect("utf-8 dst path"),
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

    // Advance state: draft → ready_for_review → approved.
    plans_repo::update_plan_state(pool, plan_id, "ready_for_review")
        .await
        .expect("update to ready_for_review");
    plans_repo::set_approved(pool, plan_id, "2026-06-19T00:00:00Z", "tok-test-fixed")
        .await
        .expect("set_approved");
}

async fn persisted_item_failure(
    pool: &sqlx::SqlitePool,
    plan_id: &str,
) -> (String, Option<String>) {
    sqlx::query_as(
        "SELECT new_state, failure_code FROM plan_apply_events \
         WHERE plan_id = ? AND item_id IS NOT NULL AND failure_code IS NOT NULL",
    )
    .bind(plan_id)
    .fetch_one(pool)
    .await
    .expect("query persisted item failure")
}

// ── Test 1: plan content round-trip ──────────────────────────────────────────

/// A freshly-seeded approved plan has its items readable via `get_plan`
/// and reports the correct state and item counts.
#[tokio::test]
async fn approved_plan_content_round_trip() {
    let (db, _repo, _bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("source.fits");
    let dst = dir.path().join("archive/source.fits");
    std::fs::write(&src, b"fits-content").expect("write src");

    seed_approved_plan_with_real_files(db.pool(), &plan_id, &src, &dst).await;

    // Verify via the use-case layer (not raw SQL).
    let detail =
        app_core::plans::get_plan(db.pool(), &plan_id).await.expect("get_plan should succeed");

    assert_eq!(detail.id, plan_id);
    assert_eq!(
        detail.state,
        contracts_core::plans::PlanState::Approved,
        "plan should be in approved state"
    );
    assert_eq!(detail.items_total, 1, "expected 1 item");
    assert_eq!(detail.items.len(), 1);
    assert_eq!(detail.items[0].name, "file.fits");
    assert_eq!(detail.items[0].action, contracts_core::plans::PlanItemAction::Move);
}

// ── Test 2: apply produces real FS side effect + audit events ────────────────

/// Applying an approved plan with a move item:
///   (a) moves the source file to the destination on the real filesystem, and
///   (b) writes `plan_apply_events` rows including a plan-level `approved→applying`
///       start event and an item `pending→succeeded` event.
#[tokio::test]
async fn apply_plan_moves_file_and_writes_audit_events() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("capture.fits");
    let dst = dir.path().join("processed/capture.fits");
    std::fs::write(&src, b"raw-light-frame").expect("write src");

    seed_approved_plan_with_real_files(db.pool(), &plan_id, &src, &dst).await;

    // Apply the plan.
    let resp = app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should succeed");
    assert_eq!(resp.plan_id, plan_id);
    assert_eq!(resp.new_state, "applying");
    assert!(!resp.run_id.is_empty(), "run_id should be non-empty");

    support::wait_plan_terminal(db.pool(), &plan_id).await;

    // (a) FS side effect: source gone, destination present.
    assert!(!src.exists(), "source file should have been moved away");
    assert!(dst.exists(), "destination file should exist after move");
    assert_eq!(std::fs::read(&dst).expect("read dst"), b"raw-light-frame");

    // (b) Audit: at least a plan-level start event should be recorded.
    let (event_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM plan_apply_events WHERE plan_id = ?")
            .bind(&plan_id)
            .fetch_one(db.pool())
            .await
            .expect("query plan_apply_events");

    assert!(
        event_count >= 1,
        "expected at least 1 plan_apply_event for plan {plan_id}, found {event_count}"
    );

    // Verify the plan-level start event: approved → applying.
    let (start_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM plan_apply_events \
         WHERE plan_id = ? AND item_id IS NULL AND prior_state = 'approved' AND new_state = 'applying'",
    )
    .bind(&plan_id)
    .fetch_one(db.pool())
    .await
    .expect("query start event");

    assert_eq!(
        start_count, 1,
        "expected 1 plan-level start event (approved→applying) for plan {plan_id}"
    );

    // Verify that an item-level succeeded event was written.
    let (item_succeeded_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM plan_apply_events \
         WHERE plan_id = ? AND item_id IS NOT NULL AND new_state = 'succeeded'",
    )
    .bind(&plan_id)
    .fetch_one(db.pool())
    .await
    .expect("query item succeeded event");

    assert_eq!(item_succeeded_count, 1, "expected 1 item-level succeeded event for plan {plan_id}");

    // Plan should have reached terminal state 'applied'.
    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan row");
    assert_eq!(
        plan_row.state, "applied",
        "plan should reach terminal state 'applied' after all items succeed"
    );
}

// ── Test: spec 017 C5 archive lifecycle closure ──────────────────────────────

/// Seed a `completed` project and an approved `origin = archive` plan whose one
/// item moves a real tempfile, with the project id carried in `origin_path`.
async fn seed_approved_archive_plan(
    pool: &sqlx::SqlitePool,
    plan_id: &str,
    project_id: &str,
    src_path: &std::path::Path,
    dst_path: &std::path::Path,
) {
    plans_repo::insert_plan(
        pool,
        &plans_repo::InsertPlan {
            id: plan_id,
            title: "Archive Test Plan",
            origin: "archive",
            origin_path: Some(project_id),
            plan_type: "archive",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .expect("insert archive plan");

    plans_repo::insert_plan_item(
        pool,
        &plans_repo::InsertPlanItem {
            id: &format!("{plan_id}-item-0"),
            plan_id,
            item_index: 1,
            name: "capture.fits",
            action: "move",
            from_root_id: None,
            from_relative_path: src_path.to_str().expect("utf-8 src"),
            to_root_id: None,
            to_relative_path: dst_path.to_str().expect("utf-8 dst"),
            reason: "archive",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: Some(project_id),
            category: Some("intermediate"),
        },
    )
    .await
    .expect("insert archive item");

    plans_repo::update_plan_state(pool, plan_id, "ready_for_review").await.expect("ready");
    plans_repo::set_approved(pool, plan_id, "2026-06-19T00:00:00Z", "tok-test-fixed")
        .await
        .expect("approve");
}

/// C5: applying an `origin = archive` plan to a clean `applied` terminal drives
/// the owning project into `archived` (the legitimate requires-plan closure) and
/// records `archived_via_plan_id`.
#[tokio::test]
async fn archive_plan_apply_drives_project_to_archived() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let project_id = Uuid::new_v4().to_string();
    support::insert_project(db.pool(), &project_id, "t", "completed").await;

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("capture.fits");
    let dst = dir.path().join("archive/capture.fits");
    std::fs::write(&src, b"raw-light-frame").expect("write src");

    seed_approved_archive_plan(db.pool(), &plan_id, &project_id, &src, &dst).await;

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should succeed");

    support::wait_plan_terminal(db.pool(), &plan_id).await;

    // Plan reached 'applied'.
    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan_row.state, "applied");

    // Project driven to 'archived' with the plan link recorded.
    let archived = persistence_plans::repositories::projects::list_archived_projects(db.pool())
        .await
        .expect("list_archived_projects");
    assert_eq!(archived.len(), 1, "the completed project must now be archived");
    assert_eq!(archived[0].id, project_id);
    assert_eq!(archived[0].archived_via_plan_id.as_deref(), Some(plan_id.as_str()));

    // A lifecycle audit row for the project → archived transition was written.
    let (transition_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log_entry \
         WHERE entity_id = ? AND to_state = 'archived' AND outcome = 'applied'",
    )
    .bind(&project_id)
    .fetch_one(db.pool())
    .await
    .expect("query lifecycle audit");
    assert_eq!(transition_count, 1, "archive closure must write a lifecycle audit row");
}

/// A non-archive plan apply must NOT touch project lifecycle (guard the origin
/// gate — the closure is archive-only).
#[tokio::test]
async fn non_archive_plan_apply_leaves_project_lifecycle_untouched() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let project_id = Uuid::new_v4().to_string();
    support::insert_project(db.pool(), &project_id, "t", "completed").await;

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("capture.fits");
    let dst = dir.path().join("processed/capture.fits");
    std::fs::write(&src, b"raw").expect("write src");

    // A plain cleanup plan (origin != archive) that happens to reference the
    // project in origin_path must not archive it.
    plans_repo::insert_plan(
        db.pool(),
        &plans_repo::InsertPlan {
            id: &plan_id,
            title: "Cleanup",
            origin: "cleanup",
            origin_path: Some(&project_id),
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .expect("insert plan");
    plans_repo::insert_plan_item(
        db.pool(),
        &plans_repo::InsertPlanItem {
            id: &format!("{plan_id}-item-0"),
            plan_id: &plan_id,
            item_index: 1,
            name: "capture.fits",
            action: "move",
            from_root_id: None,
            from_relative_path: src.to_str().unwrap(),
            to_root_id: None,
            to_relative_path: dst.to_str().unwrap(),
            reason: "cleanup",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        },
    )
    .await
    .expect("insert item");
    plans_repo::update_plan_state(db.pool(), &plan_id, "ready_for_review").await.unwrap();
    plans_repo::set_approved(db.pool(), &plan_id, "2026-06-19T00:00:00Z", "tok-test-fixed")
        .await
        .unwrap();

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let project = persistence_plans::repositories::projects::get_project(db.pool(), &project_id)
        .await
        .unwrap();
    assert_eq!(project.lifecycle, "completed", "non-archive apply must not archive the project");
}

// ── Test 3: no-overwrite safety guarantee ────────────────────────────────────

/// Attempting to move a file to a destination that already exists must
/// produce a `failed` item event (ConflictDestinationExists), leaving the
/// source file intact and the plan in a terminal failed/partially_applied state.
///
/// This verifies Constitution §II: "never overwrite silently".
#[tokio::test]
async fn apply_plan_refuses_to_overwrite_existing_destination() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("source.fits");
    let dst = dir.path().join("destination.fits");

    // Both source and destination already exist — triggers ConflictDestinationExists.
    std::fs::write(&src, b"source-content").expect("write src");
    std::fs::write(&dst, b"existing-content").expect("write dst");

    seed_approved_plan_with_real_files(db.pool(), &plan_id, &src, &dst).await;

    let resp = app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should succeed (apply start, not item execution)");
    assert_eq!(resp.new_state, "applying");

    support::wait_plan_terminal(db.pool(), &plan_id).await;

    // Source must be untouched — no silent overwrite.
    assert!(src.exists(), "source file must remain when destination already exists");
    assert_eq!(
        std::fs::read(&dst).expect("read dst"),
        b"existing-content",
        "destination must not be overwritten"
    );

    let (item_state, failure_code) = persisted_item_failure(db.pool(), &plan_id).await;
    assert_eq!(item_state, "failed");
    assert_eq!(failure_code.as_deref(), Some(FailureCode::ConflictDestinationExists.as_str()));

    // Plan terminal state must be 'failed' (0 successes, 1 failure).
    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan row");
    assert_eq!(plan_row.state, "failed", "plan should reach 'failed' when the only item conflicts");
}

#[tokio::test]
async fn apply_plan_persists_source_missing_failure_code() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("missing.fits");
    let dst = dir.path().join("processed/missing.fits");

    seed_approved_plan_with_real_files(db.pool(), &plan_id, &src, &dst).await;

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should start");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let (item_state, failure_code) = persisted_item_failure(db.pool(), &plan_id).await;
    assert_eq!(item_state, "failed");
    assert_eq!(failure_code.as_deref(), Some(FailureCode::SourceMissing.as_str()));
}

#[tokio::test]
async fn apply_plan_persists_protected_source_failure_code() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("protected.fits");
    let dst = dir.path().join("processed/protected.fits");
    std::fs::write(&src, b"protected-content").expect("write src");

    seed_approved_plan_with_real_files(db.pool(), &plan_id, &src, &dst).await;
    sqlx::query("UPDATE plan_items SET protection = 'protected' WHERE plan_id = ?")
        .bind(&plan_id)
        .execute(db.pool())
        .await
        .expect("protect plan item");

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should start");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let (item_state, failure_code) = persisted_item_failure(db.pool(), &plan_id).await;
    assert_eq!(item_state, "failed");
    assert_eq!(failure_code.as_deref(), Some(FailureCode::ProtectedSource.as_str()));
}

#[tokio::test]
async fn apply_plan_persists_destructive_unconfirmed_failure_code() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("delete.fits");
    let dst = dir.path().join("unused.fits");
    std::fs::write(&src, b"delete-content").expect("write src");

    seed_approved_plan_with_real_files(db.pool(), &plan_id, &src, &dst).await;
    sqlx::query("UPDATE plan_items SET action = 'delete' WHERE plan_id = ?")
        .bind(&plan_id)
        .execute(db.pool())
        .await
        .expect("make plan item destructive");

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should start");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let (item_state, failure_code) = persisted_item_failure(db.pool(), &plan_id).await;
    assert_eq!(item_state, "refused");
    assert_eq!(failure_code.as_deref(), Some(FailureCode::DestructiveUnconfirmed.as_str()));
}

#[tokio::test]
async fn apply_plan_persists_item_stale_failure_code() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("stale.fits");
    let dst = dir.path().join("processed/stale.fits");
    std::fs::write(&src, b"changed-content").expect("write src");

    seed_approved_plan_with_real_files(db.pool(), &plan_id, &src, &dst).await;
    sqlx::query("UPDATE plan_items SET approved_size_bytes = 1 WHERE plan_id = ?")
        .bind(&plan_id)
        .execute(db.pool())
        .await
        .expect("set stale size snapshot");

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan should start");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    let (item_state, failure_code) = persisted_item_failure(db.pool(), &plan_id).await;
    assert_eq!(item_state, "stale");
    assert_eq!(failure_code.as_deref(), Some(FailureCode::ItemStale.as_str()));
}

// ── Test 3b: root_id resolved via registered_sources (gen-3) ─────────────────

/// Regression for the inbox apply path: plan items reference a root by
/// `from_root_id`/`to_root_id` and carry **relative** source/destination paths.
/// The executor must resolve that root id to an absolute path so the relative
/// paths anchor correctly. Roots added through the setup wizard live in
/// `registered_sources` (the gen-3 source model) and are NOT mirrored into the
/// legacy `library_root` table; before the fix, the resolver only consulted
/// `library_root`, returned `None`, fell back to bare relative paths, and every
/// move failed with `source.missing (os error 2)` — so no files ever moved.
#[tokio::test]
async fn apply_resolves_root_id_from_registered_sources() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let root_id = "reg-src-move-1";

    // Real source root (as a wizard-added source would be) with a file inside.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(root.join("capture.fits"), b"raw-light-frame").expect("write src");

    // Register the root ONLY in registered_sources (library_root stays empty),
    // exactly as first-run registration does.
    sqlx::query(
        "INSERT INTO registered_sources \
         (id, kind, path, scan_depth, created_at, created_via, organization_state) \
         VALUES (?, 'light_frames', ?, 'recursive', '2026-06-20T00:00:00Z', 'first_run', 'unorganized')",
    )
    .bind(root_id)
    .bind(root.to_str().expect("utf-8 root"))
    .execute(db.pool())
    .await
    .expect("insert registered_sources");

    // Plan move item: root_id set, paths RELATIVE to the root.
    plans_repo::insert_plan(
        db.pool(),
        &plans_repo::InsertPlan {
            id: &plan_id,
            title: "Inbox move (registered_sources root)",
            origin: "inbox",
            origin_path: None,
            plan_type: "split",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .expect("insert_plan");
    plans_repo::insert_plan_item(
        db.pool(),
        &plans_repo::InsertPlanItem {
            id: &format!("{plan_id}-item-0"),
            plan_id: &plan_id,
            item_index: 1,
            name: "capture.fits",
            action: "move",
            from_root_id: Some(root_id),
            from_relative_path: "capture.fits",
            to_root_id: Some(root_id),
            to_relative_path: "M45/L/2026-01-20/light/capture.fits",
            reason: "regression test",
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
    plans_repo::update_plan_state(db.pool(), &plan_id, "ready_for_review")
        .await
        .expect("ready_for_review");
    plans_repo::set_approved(db.pool(), &plan_id, "2026-06-20T00:00:00Z", "tok-test-fixed")
        .await
        .expect("set_approved");

    app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "tok-test-fixed", None)
        .await
        .expect("apply_plan");
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    // The file must have moved to the root-anchored destination.
    assert!(!root.join("capture.fits").exists(), "source should be moved away");
    assert!(
        root.join("M45/L/2026-01-20/light/capture.fits").exists(),
        "destination (anchored to the registered_sources root) should exist"
    );

    let plan_row = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan row");
    assert_eq!(plan_row.state, "applied", "plan should reach 'applied' after the move succeeds");
}

// ── Test 4: approve + apply round-trip via use-case layer ────────────────────

/// Verifies the full review pipeline via public use-case functions:
///   insert draft → approve_plan → apply_plan → audit events.
///
/// This mirrors the UI-driven happy-path without touching repo internals
/// (except for the initial draft insert which has no public use-case yet).
#[tokio::test]
async fn full_review_to_apply_audit_round_trip() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("light.fits");
    let dst = dir.path().join("sorted/light.fits");
    std::fs::write(&src, b"light-frame").expect("write src");

    // 1. Insert draft plan + item via repo (no public use-case for plan generation yet).
    plans_repo::insert_plan(
        db.pool(),
        &plans_repo::InsertPlan {
            id: &plan_id,
            title: "Full Round-Trip Plan",
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

    plans_repo::insert_plan_item(
        db.pool(),
        &plans_repo::InsertPlanItem {
            id: &format!("{plan_id}-item-0"),
            plan_id: &plan_id,
            item_index: 1,
            name: "light.fits",
            action: "move",
            from_root_id: None,
            from_relative_path: src.to_str().expect("utf-8"),
            to_root_id: None,
            to_relative_path: dst.to_str().expect("utf-8"),
            reason: "round-trip test",
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

    // 2. Transition to ready_for_review (required pre-condition for approve_plan).
    plans_repo::update_plan_state(db.pool(), &plan_id, "ready_for_review")
        .await
        .expect("update_plan_state");

    // 3. Approve via use-case layer.
    let approve_resp = app_core::plans::approve_plan(db.pool(), &bus, &plan_id, "tester")
        .await
        .expect("approve_plan");
    assert_eq!(approve_resp.new_state, "approved");
    assert!(!approve_resp.approval_token.is_empty());

    // 4. Apply via use-case layer.
    let apply_resp = app_core::plan_apply::apply_plan(
        db.pool(),
        &bus,
        &plan_id,
        &approve_resp.approval_token,
        None,
    )
    .await
    .expect("apply_plan");
    assert_eq!(apply_resp.new_state, "applying");

    // Wait for background executor task.
    support::wait_plan_terminal(db.pool(), &plan_id).await;

    // 5. FS side effect.
    assert!(!src.exists(), "source should be moved");
    assert!(dst.exists(), "destination should exist");

    // 6. Audit: plan-level terminal event written.
    let (terminal_event_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM plan_apply_events \
         WHERE plan_id = ? AND item_id IS NULL AND new_state = 'applied'",
    )
    .bind(&plan_id)
    .fetch_one(db.pool())
    .await
    .expect("query terminal event");

    assert_eq!(
        terminal_event_count, 1,
        "expected 1 plan-level terminal event (new_state='applied') for plan {plan_id}"
    );

    // 7. get_apply_status reflects completed state.
    let status = app_core::plan_apply::get_apply_status(db.pool(), &plan_id)
        .await
        .expect("get_apply_status");
    assert_eq!(status.plan_state, "applied");
    assert_eq!(status.items_applied, 1);
    assert_eq!(status.items_failed, 0);
}

// ── Test: channel-free apply (spec 037 archive apply, no Channel) ─────

/// `apply_plan` with `event_sink: None` — the route `plans.apply.direct` takes
/// (spec 037 Journeys 6/7) — applies an archive plan exactly like the
/// Channel-carrying route: same FS move, same durable audit trail, same
/// terminal counts via `get_apply_status`, and the same spec 017 C5 archive
/// lifecycle closure. The approval token comes from an explicit `approve_plan`
/// call by the caller, never from the apply path itself.
#[tokio::test]
async fn channel_free_apply_moves_file_after_explicit_approval() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();
    let project_id = Uuid::new_v4().to_string();
    support::insert_project(db.pool(), &project_id, "t", "completed").await;

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("capture.fits");
    let dst = dir.path().join("archive/capture.fits");
    std::fs::write(&src, b"raw-light-frame").expect("write src");

    // Seed a `ready_for_review` archive plan — deliberately NOT pre-approved,
    // so the explicit approve step below is the only source of the token.
    plans_repo::insert_plan(
        db.pool(),
        &plans_repo::InsertPlan {
            id: &plan_id,
            title: "Channel-Free Archive Test Plan",
            origin: "archive",
            origin_path: Some(&project_id),
            plan_type: "archive",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .expect("insert archive plan");

    plans_repo::insert_plan_item(
        db.pool(),
        &plans_repo::InsertPlanItem {
            id: &format!("{plan_id}-item-0"),
            plan_id: &plan_id,
            item_index: 1,
            name: "capture.fits",
            action: "move",
            from_root_id: None,
            from_relative_path: src.to_str().expect("utf-8 src"),
            to_root_id: None,
            to_relative_path: dst.to_str().expect("utf-8 dst"),
            reason: "archive",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: Some(&project_id),
            category: Some("intermediate"),
        },
    )
    .await
    .expect("insert archive item");

    plans_repo::update_plan_state(db.pool(), &plan_id, "ready_for_review")
        .await
        .expect("ready_for_review");

    let approval_token = app_core::plans::approve_plan(db.pool(), &bus, &plan_id, "user")
        .await
        .expect("approve_plan")
        .approval_token;

    let resp = app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, &approval_token, None)
        .await
        .expect("channel-free apply of an approved plan");
    assert_eq!(resp.plan_id, plan_id);
    assert_eq!(resp.new_state, "applying");
    assert!(!resp.run_id.is_empty(), "run_id should be non-empty");

    support::wait_plan_terminal(db.pool(), &plan_id).await;

    // FS side effect: real move to the archive destination (never a silent
    // overwrite — constitution §II).
    assert!(!src.exists(), "source should have been moved");
    assert!(dst.exists(), "destination should exist in the archive subtree");
    assert_eq!(std::fs::read(&dst).expect("read dst"), b"raw-light-frame");

    // Audit: plan-level start + terminal events recorded per attempted action.
    let (start_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM plan_apply_events \
         WHERE plan_id = ? AND item_id IS NULL AND prior_state = 'approved' AND new_state = 'applying'",
    )
    .bind(&plan_id)
    .fetch_one(db.pool())
    .await
    .expect("query start event");
    assert_eq!(start_count, 1, "expected 1 plan-level start event");

    let (terminal_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM plan_apply_events \
         WHERE plan_id = ? AND item_id IS NULL AND new_state = 'applied'",
    )
    .bind(&plan_id)
    .fetch_one(db.pool())
    .await
    .expect("query terminal event");
    assert_eq!(terminal_count, 1, "expected 1 plan-level terminal event");

    // Terminal summary via the same poll path the E2E journeys use.
    let status = app_core::plan_apply::get_apply_status(db.pool(), &plan_id)
        .await
        .expect("get_apply_status");
    assert_eq!(status.plan_state, "applied");
    assert_eq!(status.items_applied, 1);
    assert_eq!(status.items_failed, 0);
    assert_eq!(status.items_skipped, 0);
    assert_eq!(status.items_cancelled, 0);

    // C5 lifecycle closure still fires through the channel-free path: an
    // `origin = archive` plan reaching `applied` drives the project to
    // `archived`.
    let archived = persistence_plans::repositories::projects::list_archived_projects(db.pool())
        .await
        .expect("list_archived_projects");
    assert_eq!(archived.len(), 1, "the completed project must now be archived");
    assert_eq!(archived[0].id, project_id);
    assert_eq!(archived[0].archived_via_plan_id.as_deref(), Some(plan_id.as_str()));
}

/// An absent approval token is rejected and mutates nothing. The channel-free
/// apply path once minted its own token here, which made the gate unfailable.
#[tokio::test]
async fn channel_free_apply_rejects_absent_token() {
    let (db, _repo, bus) = support::setup().await;
    let plan_id = Uuid::new_v4().to_string();

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("source.fits");
    let dst = dir.path().join("processed/source.fits");
    std::fs::write(&src, b"fits-content").expect("write src");

    seed_approved_plan_with_real_files(db.pool(), &plan_id, &src, &dst).await;

    let err = app_core::plan_apply::apply_plan(db.pool(), &bus, &plan_id, "", None)
        .await
        .expect_err("an absent approval token must not apply a plan");
    assert_eq!(err.code, contracts_core::error_code::ErrorCode::PlanApprovalStale);

    assert!(src.exists(), "source must not move without an approval token");
    assert!(!dst.exists(), "destination must not be created");
    let plan = plans_repo::get_plan(db.pool(), &plan_id, false).await.expect("get_plan");
    assert_eq!(plan.state, "approved", "plan state must be untouched");
}

/// A token minted for a *different* plan is rejected: the gate compares
/// against the token stored on the plan being applied.
#[tokio::test]
async fn channel_free_apply_rejects_foreign_token() {
    let (db, _repo, bus) = support::setup().await;
    let target_id = Uuid::new_v4().to_string();
    let other_id = Uuid::new_v4().to_string();

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("target.fits");
    let dst = dir.path().join("processed/target.fits");
    std::fs::write(&src, b"fits-content").expect("write src");
    let other_src = dir.path().join("other.fits");
    let other_dst = dir.path().join("processed/other.fits");
    std::fs::write(&other_src, b"fits-content").expect("write other src");

    seed_approved_plan_with_real_files(db.pool(), &target_id, &src, &dst).await;
    seed_approved_plan_with_real_files(db.pool(), &other_id, &other_src, &other_dst).await;
    // Re-approve the decoy through the real transition so its token is a
    // genuine `approve_plan` product rather than the fixture's fixed string.
    plans_repo::update_plan_state(db.pool(), &other_id, "ready_for_review")
        .await
        .expect("ready_for_review");
    let foreign_token = app_core::plans::approve_plan(db.pool(), &bus, &other_id, "user")
        .await
        .expect("approve_plan")
        .approval_token;

    let err = app_core::plan_apply::apply_plan(db.pool(), &bus, &target_id, &foreign_token, None)
        .await
        .expect_err("a token minted for another plan must not apply this plan");
    assert_eq!(err.code, contracts_core::error_code::ErrorCode::PlanApprovalStale);

    assert!(src.exists(), "source must not move on a foreign token");
    assert!(!dst.exists(), "destination must not be created");
}
