// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Layer-1 integration test for the `source_missing` project auto-block
//! (spec 009 US4, FR-020/FR-021).
//!
//! Real tempdir root, real `file_record` rows, real `project.source.add`, real
//! `app_core::frame_inventory::run_reconcile`. This file deliberately contains
//! no reference to the `project_health` block emitter: every pre-existing test
//! of the `source_missing` condition invoked that emitter directly, which is
//! how the condition shipped with zero production callers. Driving the
//! reconcile entry point is the only assertion that proves the trigger is
//! wired.

mod support;

use contracts_core::inventory_frame::{InventoryReconcileRunRequest, ReconcileReason};
use contracts_core::projects_v2::ProjectSourceAddRequest;

// ── Seed helpers ──────────────────────────────────────────────────────────────

async fn insert_root(pool: &sqlx::SqlitePool, id: &str, path: &str) {
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
         VALUES (?, ?, ?, 'local', 'active', datetime('now'))",
    )
    .bind(id)
    .bind(id)
    .bind(path)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert library_root failed: {e}"));
}

async fn insert_frame_record(pool: &sqlx::SqlitePool, id: &str, root_id: &str, rel: &str) {
    sqlx::query(
        "INSERT INTO file_record \
         (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
         VALUES (?, ?, ?, 2048, 't0', 'classified', 't0', 't0')",
    )
    .bind(id)
    .bind(root_id)
    .bind(rel)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert file_record failed: {e}"));
}

async fn insert_acquisition_session(pool: &sqlx::SqlitePool, id: &str, frame_ids: &[&str]) {
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
         VALUES (?, ?, ?, '2026-07-01T00:00:00Z')",
    )
    .bind(id)
    .bind(format!("M31|Ha|2026-07-01|{id}"))
    .bind(serde_json::to_string(frame_ids).unwrap())
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert acquisition_session failed: {e}"));
}

async fn project_lifecycle(pool: &sqlx::SqlitePool, id: &str) -> (String, Option<String>) {
    sqlx::query_as("SELECT lifecycle, blocked_reason_kind FROM projects WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Count of `actor='system'` audit rows recording a `* → blocked` transition
/// for this project (FR-021).
async fn auto_block_audit_count(pool: &sqlx::SqlitePool, project_id: &str) -> i64 {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log_entry \
         WHERE entity_type = 'project' AND entity_id = ? \
           AND to_state = 'blocked' AND actor = 'system'",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .unwrap();
    count
}

async fn source_missing_retry_count(pool: &sqlx::SqlitePool, root_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM operation_states
         WHERE id = ? AND status = 'pending'",
    )
    .bind(format!("project-source-missing-health:{root_id}"))
    .fetch_one(pool)
    .await
    .unwrap()
}

#[expect(
    clippy::cognitive_complexity,
    reason = "parameterised lifecycle fixture: setup, apply, and assertion phases in sequence"
)]
async fn run_lifecycle_case(initial_lifecycle: &str, should_block: bool) {
    let dir = tempfile::tempdir().unwrap();
    let kept = dir.path().join("light_001.fits");
    let doomed = dir.path().join("light_002.fits");
    std::fs::write(&kept, vec![0u8; 2048]).unwrap();
    std::fs::write(&doomed, vec![0u8; 2048]).unwrap();

    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();

    let root_id = format!("root-{initial_lifecycle}");
    let session_id = format!("sess-{initial_lifecycle}");
    let project_id = format!("proj-{initial_lifecycle}");
    let kept_id = format!("frame-kept-{initial_lifecycle}");
    let doomed_id = format!("frame-doomed-{initial_lifecycle}");

    insert_root(pool, &root_id, dir.path().to_str().unwrap()).await;
    insert_frame_record(pool, &kept_id, &root_id, "light_001.fits").await;
    insert_frame_record(pool, &doomed_id, &root_id, "light_002.fits").await;
    insert_acquisition_session(pool, &session_id, &[&kept_id, &doomed_id]).await;
    support::insert_project(pool, &project_id, "target-sm", "setup_incomplete").await;

    app_core::projects::project_setup::add_source(
        pool,
        &bus,
        &ProjectSourceAddRequest {
            request_id: format!("req-{initial_lifecycle}"),
            project_id: project_id.clone(),
            inventory_session_id: session_id,
        },
    )
    .await
    .expect("add_source");

    let (lifecycle, _) = project_lifecycle(pool, &project_id).await;
    assert_eq!(lifecycle, "ready", "add_source must auto-ready the project first");

    sqlx::query("UPDATE projects SET lifecycle = ? WHERE id = ?")
        .bind(initial_lifecycle)
        .bind(&project_id)
        .execute(pool)
        .await
        .expect("set lifecycle under test");

    std::fs::remove_file(&doomed).unwrap();
    let req = InventoryReconcileRunRequest { root_id, reason: ReconcileReason::OnDemand };
    let resp = app_core::frame_inventory::run_reconcile(pool, &bus, &req).await.unwrap();
    assert_eq!(resp.newly_missing, 1);

    let (lifecycle, reason_kind) = project_lifecycle(pool, &project_id).await;
    let expected_lifecycle = if should_block { "blocked" } else { initial_lifecycle };
    assert_eq!(lifecycle, expected_lifecycle, "unexpected result from {initial_lifecycle}");
    assert_eq!(
        reason_kind.as_deref(),
        should_block.then_some("source_missing"),
        "unexpected block reason from {initial_lifecycle}"
    );
    assert_eq!(
        auto_block_audit_count(pool, &project_id).await,
        i64::from(should_block),
        "unexpected audit count from {initial_lifecycle}"
    );

    app_core::frame_inventory::run_reconcile(pool, &bus, &req).await.unwrap();
    assert_eq!(
        auto_block_audit_count(pool, &project_id).await,
        i64::from(should_block),
        "repeat reconcile changed the audit count from {initial_lifecycle}"
    );
}

/// The production reconcile path follows the canonical project lifecycle
/// matrix: the four allowed states auto-block, while terminal or already
/// blocked states remain unchanged.
#[tokio::test]
async fn missing_source_frame_uses_canonical_block_transition_matrix() {
    for lifecycle in ["setup_incomplete", "ready", "prepared", "processing"] {
        run_lifecycle_case(lifecycle, true).await;
    }
    for lifecycle in ["completed", "archived", "blocked"] {
        run_lifecycle_case(lifecycle, false).await;
    }
}

/// A post-commit health-check failure does not turn a successful reconcile
/// into an error, and the same root retries the check even though the frame is
/// no longer newly missing on its next pass.
#[tokio::test]
async fn failed_health_check_retries_after_committed_reconcile() {
    let dir = tempfile::tempdir().unwrap();
    let doomed = dir.path().join("light_001.fits");
    std::fs::write(&doomed, vec![0u8; 2048]).unwrap();

    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();
    let root_id = "root-retry";
    let session_id = "sess-retry";
    let project_id = "proj-retry";
    let frame_id = "frame-retry";

    insert_root(pool, root_id, dir.path().to_str().unwrap()).await;
    insert_frame_record(pool, frame_id, root_id, "light_001.fits").await;
    insert_acquisition_session(pool, session_id, &[frame_id]).await;
    support::insert_project(pool, project_id, "target-retry", "setup_incomplete").await;
    app_core::projects::project_setup::add_source(
        pool,
        &bus,
        &ProjectSourceAddRequest {
            request_id: "req-retry".to_owned(),
            project_id: project_id.to_owned(),
            inventory_session_id: session_id.to_owned(),
        },
    )
    .await
    .expect("add_source");

    sqlx::query("UPDATE acquisition_session SET frame_ids = '{' WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await
        .expect("make health query fail");
    std::fs::remove_file(&doomed).unwrap();

    let req = InventoryReconcileRunRequest {
        root_id: root_id.to_owned(),
        reason: ReconcileReason::OnDemand,
    };
    let first = app_core::frame_inventory::run_reconcile(pool, &bus, &req)
        .await
        .expect("committed reconcile must remain successful");
    assert_eq!(first.newly_missing, 1);
    assert_eq!(project_lifecycle(pool, project_id).await.0, "ready");
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM operation_states
         WHERE id = 'project-source-missing-health:root-retry' AND status = 'pending'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(pending, 1, "failed health check must leave a durable retry marker");

    sqlx::query("ALTER TABLE library_root RENAME TO library_root_reconcile_error")
        .execute(pool)
        .await
        .expect("make intervening reconcile fail");
    app_core::frame_inventory::run_reconcile(pool, &bus, &req)
        .await
        .expect_err("intervening reconcile must fail");
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM operation_states
         WHERE id = 'project-source-missing-health:root-retry' AND status = 'pending'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(pending, 1, "reconcile error must preserve a pre-existing retry marker");
    sqlx::query("ALTER TABLE library_root_reconcile_error RENAME TO library_root")
        .execute(pool)
        .await
        .expect("restore library root table");

    sqlx::query("UPDATE acquisition_session SET frame_ids = ? WHERE id = ?")
        .bind(serde_json::to_string(&[frame_id]).unwrap())
        .bind(session_id)
        .execute(pool)
        .await
        .expect("repair health query input");

    let retry = app_core::frame_inventory::run_reconcile(pool, &bus, &req).await.unwrap();
    assert_eq!(retry.newly_missing, 0, "retry precondition must be non-new missing state");
    let (lifecycle, reason_kind) = project_lifecycle(pool, project_id).await;
    assert_eq!(lifecycle, "blocked");
    assert_eq!(reason_kind.as_deref(), Some("source_missing"));
    assert_eq!(auto_block_audit_count(pool, project_id).await, 1);
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM operation_states
         WHERE id = 'project-source-missing-health:root-retry'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(pending, 0, "successful retry must clear its durable marker");
}

/// The project mutation and required audit row commit atomically. A failed
/// audit insert rolls the lifecycle update back and does not start debounce,
/// so the durable next-run retry can still apply the transition.
#[tokio::test]
async fn failed_block_audit_rolls_back_and_retry_applies() {
    let dir = tempfile::tempdir().unwrap();
    let doomed = dir.path().join("light_001.fits");
    std::fs::write(&doomed, vec![0u8; 2048]).unwrap();

    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();
    let root_id = "root-audit-retry";
    let session_id = "sess-audit-retry";
    let project_id = "proj-audit-retry";
    let frame_id = "frame-audit-retry";

    insert_root(pool, root_id, dir.path().to_str().unwrap()).await;
    insert_frame_record(pool, frame_id, root_id, "light_001.fits").await;
    insert_acquisition_session(pool, session_id, &[frame_id]).await;
    support::insert_project(pool, project_id, "target-audit-retry", "setup_incomplete").await;
    app_core::projects::project_setup::add_source(
        pool,
        &bus,
        &ProjectSourceAddRequest {
            request_id: "req-audit-retry".to_owned(),
            project_id: project_id.to_owned(),
            inventory_session_id: session_id.to_owned(),
        },
    )
    .await
    .expect("add_source");
    sqlx::query("UPDATE projects SET lifecycle = 'processing' WHERE id = ?")
        .bind(project_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_project_block_audit BEFORE INSERT ON audit_log_entry
         WHEN NEW.entity_type = 'project' AND NEW.to_state = 'blocked'
         BEGIN SELECT RAISE(ABORT, 'forced block audit failure'); END",
    )
    .execute(pool)
    .await
    .unwrap();

    std::fs::remove_file(&doomed).unwrap();
    let req = InventoryReconcileRunRequest {
        root_id: root_id.to_owned(),
        reason: ReconcileReason::OnDemand,
    };
    let first = app_core::frame_inventory::run_reconcile(pool, &bus, &req)
        .await
        .expect("post-commit health failure must not fail reconcile");
    assert_eq!(first.newly_missing, 1);
    assert_eq!(project_lifecycle(pool, project_id).await.0, "processing");
    assert_eq!(auto_block_audit_count(pool, project_id).await, 0);

    sqlx::query("DROP TRIGGER fail_project_block_audit").execute(pool).await.unwrap();
    let retry = app_core::frame_inventory::run_reconcile(pool, &bus, &req).await.unwrap();
    assert_eq!(retry.newly_missing, 0);
    assert_eq!(project_lifecycle(pool, project_id).await.0, "blocked");
    assert_eq!(auto_block_audit_count(pool, project_id).await, 1);
}

/// The mutation boundary rejects both a stale processing selection and a
/// direct forbidden completed-to-blocked request.
#[tokio::test]
async fn auto_block_mutation_rejects_completed_state() {
    let (db, _repo, _bus) = support::setup().await;
    let pool = db.pool();
    let project_id = "proj-completed-cas";
    support::insert_project(pool, project_id, "target-completed-cas", "completed").await;

    let stale = persistence_plans::repositories::projects::apply_project_auto_block(
        pool,
        project_id,
        "processing",
        "source_missing",
        "Source missing: sess-completed-cas",
        "auto block: source_missing",
    )
    .await
    .unwrap();
    assert_eq!(
        stale,
        persistence_plans::repositories::projects::ProjectAutoBlockOutcome::CasLost {
            current_lifecycle: Some("completed".to_owned()),
            still_blockable: false,
        },
        "completed CAS winner must not request a retry"
    );

    let direct = persistence_plans::repositories::projects::apply_project_auto_block(
        pool,
        project_id,
        "completed",
        "source_missing",
        "Source missing: sess-completed-cas",
        "auto block: source_missing",
    )
    .await
    .unwrap();
    assert_eq!(
        direct,
        persistence_plans::repositories::projects::ProjectAutoBlockOutcome::Rejected
    );

    assert_eq!(project_lifecycle(pool, project_id).await.0, "completed");
    assert_eq!(auto_block_audit_count(pool, project_id).await, 0);
}

#[expect(
    clippy::cognitive_complexity,
    reason = "end-to-end fixture: setup, apply, and assertion phases in sequence"
)]
async fn run_production_allowed_cas_loss_case() {
    let dir = tempfile::tempdir().unwrap();
    let doomed = dir.path().join("light_001.fits");
    std::fs::write(&doomed, vec![0u8; 2048]).unwrap();

    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();
    let root_id = "root-cas-race";
    let session_id = "sess-cas-race";
    let project_id = "proj-cas-race";
    let frame_id = "frame-cas-race";

    insert_root(pool, root_id, dir.path().to_str().unwrap()).await;
    insert_frame_record(pool, frame_id, root_id, "light_001.fits").await;
    insert_acquisition_session(pool, session_id, &[frame_id]).await;
    support::insert_project(pool, project_id, "target-cas", "setup_incomplete").await;
    app_core::projects::project_setup::add_source(
        pool,
        &bus,
        &ProjectSourceAddRequest {
            request_id: "req-cas-race".to_owned(),
            project_id: project_id.to_owned(),
            inventory_session_id: session_id.to_owned(),
        },
    )
    .await
    .expect("add_source");

    sqlx::query(
        "CREATE TRIGGER force_auto_block_cas_loss
         BEFORE UPDATE OF lifecycle ON projects
         WHEN OLD.id = 'proj-cas-race' AND OLD.lifecycle = 'ready' AND NEW.lifecycle = 'blocked'
         BEGIN
           UPDATE projects SET lifecycle = 'prepared' WHERE id = OLD.id;
           SELECT RAISE(IGNORE);
         END",
    )
    .execute(pool)
    .await
    .unwrap();

    std::fs::remove_file(&doomed).unwrap();
    let req = InventoryReconcileRunRequest {
        root_id: root_id.to_owned(),
        reason: ReconcileReason::OnDemand,
    };
    let first = app_core::frame_inventory::run_reconcile(pool, &bus, &req).await.unwrap();
    assert_eq!(first.newly_missing, 1);
    assert_eq!(project_lifecycle(pool, project_id).await.0, "prepared");
    assert_eq!(auto_block_audit_count(pool, project_id).await, 0);
    assert_eq!(
        source_missing_retry_count(pool, root_id).await,
        1,
        "blockable CAS winner must preserve the retry marker"
    );

    sqlx::query("DROP TRIGGER force_auto_block_cas_loss").execute(pool).await.unwrap();
    let second = app_core::frame_inventory::run_reconcile(pool, &bus, &req).await.unwrap();
    assert_eq!(second.newly_missing, 0);
    assert_eq!(project_lifecycle(pool, project_id).await.0, "blocked");
    assert_eq!(auto_block_audit_count(pool, project_id).await, 1);
    assert_eq!(source_missing_retry_count(pool, root_id).await, 0);
}

/// Losing `ready` to another blockable state preserves the durable retry, so
/// the next zero-newly-missing reconcile can apply the automatic block.
#[tokio::test]
async fn allowed_to_allowed_cas_loss_retries_on_production_path() {
    run_production_allowed_cas_loss_case().await;
}
