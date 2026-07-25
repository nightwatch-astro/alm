// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::doc_markdown, clippy::needless_borrows_for_generic_args, clippy::too_many_lines)]

//! Integration tests for Update View use cases (spec 062 US3/US5).
//!
//! Covered:
//! 1. `plan_update_view` generates a plan with correct session/item counts.
//! 2. `plan_update_view` returns `update_view_no_additions` when all sessions materialized.
//! 3. `approve_update_view` transitions plan to `approved` state.
//! 4. `approve_update_view` refuses on digest mismatch.
//! 5. `discard_update_view` removes an open plan.
//! 6. `discard_update_view` refuses an approved plan.
//! 7. `query_update_view` returns correct DTO after plan generation.
//! 8. `query_update_view` returns typed error for unknown plan ID.

use uuid::Uuid;

use app_core_projects::update_view::{
    apply_update_view, approve_update_view, cancel_update_view, discard_update_view,
    plan_update_view, query_update_view, run_apply_loop, ApplyUpdateViewRequest,
    ApproveUpdateViewRequest, CancelUpdateViewRequest, DiscardUpdateViewRequest, InstallItem,
    InstallerCallbacks, PlanUpdateViewRequest,
};
use contracts_core::error_code::ErrorCode;
use persistence_core::Database;
use persistence_topology::test_support as support;

fn uid() -> String {
    Uuid::new_v4().to_string()
}

const TS: &str = "2026-07-22T00:00:00.000000Z";

// ── Seed helpers ──────────────────────────────────────────────────────────────

async fn seed_basics(db: &Database) -> (i64, i64, i64, i64) {
    let pool = db.pool();
    let seq = support::insert_sequence(pool).await;
    let actor_id = support::insert_actor(pool, &uid()).await;
    let cfg_id = support::insert_config_revision(pool, &uid(), 1).await;
    let cmd_id = support::insert_command(pool, &uid(), actor_id).await;
    let op_id = support::insert_materialization_operation(pool, &uid(), cmd_id, cfg_id, seq).await;
    let target_id = support::insert_spec062_target(pool, &uid()).await;
    (actor_id, cfg_id, op_id, target_id)
}

async fn seed_project(db: &Database) -> (i64, String) {
    let pool = db.pool();
    let pub_id = uid();
    let proj_row_id = support::insert_spec062_project(pool, &pub_id).await;

    // Insert legacy rows so lifecycle queries succeed (table names vary by migration era).
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO project (id, name, state, lifecycle, created_at)
         VALUES (?, 'Test Project', 'active', 'ready', ?)",
    )
    .bind(&pub_id)
    .bind(TS)
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO projects (id, name, state, lifecycle, created_at)
         VALUES (?, 'Test Project', 'active', 'ready', ?)",
    )
    .bind(&pub_id)
    .bind(TS)
    .execute(pool)
    .await;

    sqlx::query(
        "INSERT OR IGNORE INTO spec062_destination_root (public_id, project_row_id, created_at)
         VALUES (?, ?, ?)",
    )
    .bind(&uid())
    .bind(proj_row_id)
    .bind(TS)
    .execute(pool)
    .await
    .expect("insert destination_root");

    (proj_row_id, pub_id)
}

async fn pin_session(db: &Database, project_pub: &str, session_row_id: i64) {
    let pool = db.pool();
    let (proj_row_id, mem_gen): (i64, i64) = sqlx::query_as(
        "SELECT row_id, membership_head_generation FROM spec062_project WHERE public_id = ?",
    )
    .bind(project_pub)
    .fetch_one(pool)
    .await
    .unwrap();

    let seq = support::insert_sequence(pool).await;
    let actor_id = support::insert_actor(pool, &uid()).await;
    let rev_pub = uid();
    let next_rev = mem_gen + 1;

    sqlx::query(
        "INSERT INTO project_membership_revision
             (public_id, project_row_id, revision_number, actor_row_id, created_sequence, created_at)
         VALUES (?,?,?,?,?,?)",
    )
    .bind(&rev_pub)
    .bind(proj_row_id)
    .bind(next_rev)
    .bind(actor_id)
    .bind(seq)
    .bind(TS)
    .execute(pool)
    .await
    .unwrap();

    let (rev_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM project_membership_revision WHERE public_id = ?")
            .bind(&rev_pub)
            .fetch_one(pool)
            .await
            .unwrap();

    sqlx::query(
        "INSERT INTO project_membership_revision_session
             (revision_row_id, session_row_id, pin_revision, source, pinned_by_actor_row_id, pinned_at)
         VALUES (?,?,1,'explicit_add',?,?)",
    )
    .bind(rev_row_id)
    .bind(session_row_id)
    .bind(actor_id)
    .bind(TS)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE spec062_project
         SET membership_head_revision_row_id = ?, membership_head_generation = ?
         WHERE row_id = ?",
    )
    .bind(rev_row_id)
    .bind(next_rev)
    .bind(proj_row_id)
    .execute(pool)
    .await
    .unwrap();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn plan_generates_with_one_session() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let resp = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan should succeed");

    assert_eq!(resp.plan.state, "open");
    assert_eq!(resp.plan.session_count, 1);
    assert_eq!(resp.plan.item_count, 1);
    assert!(!resp.plan.plan_digest.is_empty());
}

#[tokio::test]
async fn plan_no_additions_returns_error() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (proj_row_id, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    // Insert a fake applied plan + snapshot covering the session.
    let plan_pub = uid();
    let plan_seq = support::insert_sequence(pool).await;
    let actor_row_id = support::insert_actor(pool, &uid()).await;
    let (mem_rev_row_id,): (i64,) = sqlx::query_as(
        "SELECT membership_head_revision_row_id FROM spec062_project WHERE row_id = ?",
    )
    .bind(proj_row_id)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO materialization_update_plan
             (public_id, project_row_id, target_membership_revision_row_id,
              state, content_digest, session_count, item_count,
              source_frame_count, source_byte_count, remaining_session_count,
              actor_row_id, created_sequence, created_at)
         VALUES (?,?,?,'applied','dummy',1,1,1,0,0,?,?,?)",
    )
    .bind(&plan_pub)
    .bind(proj_row_id)
    .bind(mem_rev_row_id)
    .bind(actor_row_id)
    .bind(plan_seq)
    .bind(TS)
    .execute(pool)
    .await
    .unwrap();

    let (plan_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM materialization_update_plan WHERE public_id = ?")
            .bind(&plan_pub)
            .fetch_one(pool)
            .await
            .unwrap();

    let snap_pub = uid();
    let snap_seq = support::insert_sequence(pool).await;
    sqlx::query(
        "INSERT INTO project_materialization_snapshot
             (public_id, project_row_id, membership_revision_row_id,
              applied_plan_row_id, entry_count, session_count, created_sequence, created_at)
         VALUES (?,?,?,?,0,1,?,?)",
    )
    .bind(&snap_pub)
    .bind(proj_row_id)
    .bind(mem_rev_row_id)
    .bind(plan_row_id)
    .bind(snap_seq)
    .bind(TS)
    .execute(pool)
    .await
    .unwrap();

    let (snap_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM project_materialization_snapshot WHERE public_id = ?")
            .bind(&snap_pub)
            .fetch_one(pool)
            .await
            .unwrap();

    sqlx::query(
        "INSERT INTO project_materialization_snapshot_session (snapshot_row_id, session_row_id, ordinal)
         VALUES (?,?,0)",
    )
    .bind(snap_row_id)
    .bind(session_row_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE spec062_project
         SET materialization_head_snapshot_row_id = ?, materialization_head_generation = 1
         WHERE row_id = ?",
    )
    .bind(snap_row_id)
    .bind(proj_row_id)
    .execute(pool)
    .await
    .unwrap();

    let err = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect_err("should fail with no_additions");

    assert_eq!(err.code, ErrorCode::ProjectUpdateViewNoAdditions);
}

#[tokio::test]
async fn approve_transitions_to_approved() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    let resp = approve_update_view(
        pool,
        &ApproveUpdateViewRequest {
            plan_id: &plan.plan_id,
            approval_digest: &plan.plan_digest,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("approve");

    assert_eq!(resp.plan.state, "approved");
}

#[tokio::test]
async fn approve_refuses_on_digest_mismatch() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    let err = approve_update_view(
        pool,
        &ApproveUpdateViewRequest {
            plan_id: &plan.plan_id,
            approval_digest: "sha256:wrong",
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect_err("should refuse on digest mismatch");

    assert_eq!(err.code, ErrorCode::ProjectUpdateViewPlanDigestMismatch);
}

#[tokio::test]
async fn discard_removes_open_plan() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    discard_update_view(
        pool,
        &DiscardUpdateViewRequest { plan_id: &plan.plan_id, actor_id: &uid(), command_id: &uid() },
    )
    .await
    .expect("discard");

    let state: (String,) =
        sqlx::query_as("SELECT state FROM materialization_update_plan WHERE public_id = ?")
            .bind(&plan.plan_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(state.0, "discarded");
}

#[tokio::test]
async fn discard_refuses_approved_plan() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    approve_update_view(
        pool,
        &ApproveUpdateViewRequest {
            plan_id: &plan.plan_id,
            approval_digest: &plan.plan_digest,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("approve");

    let err = discard_update_view(
        pool,
        &DiscardUpdateViewRequest { plan_id: &plan.plan_id, actor_id: &uid(), command_id: &uid() },
    )
    .await
    .expect_err("should refuse discard of approved plan");

    assert_eq!(err.code, ErrorCode::ProjectUpdateViewPlanNotOpen);
}

#[tokio::test]
async fn query_returns_correct_dto() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    let dto = query_update_view(pool, &plan.plan_id).await.expect("query");

    assert_eq!(dto.plan_id, plan.plan_id);
    assert_eq!(dto.state, "open");
    assert_eq!(dto.session_count, 1);
    assert_eq!(dto.project_id, project_pub);
}

#[tokio::test]
async fn plan_not_found_returns_typed_error() {
    let db = support::setup_db().await;
    let pool = db.pool();

    let err = query_update_view(pool, "non-existent-plan-id").await.expect_err("should fail");
    assert_eq!(err.code, ErrorCode::ProjectUpdateViewPlanNotFound);
}

// ── Additional tests (review round 1 items 3–4) ───────────────────────────────

/// `cancel_update_view` sets an applying plan to stopped.
#[tokio::test]
async fn cancel_sets_applying_to_stopped() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    approve_update_view(
        pool,
        &ApproveUpdateViewRequest {
            plan_id: &plan.plan_id,
            approval_digest: &plan.plan_digest,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("approve");

    apply_update_view(
        pool,
        &ApplyUpdateViewRequest {
            plan_id: &plan.plan_id,
            approval_digest: &plan.plan_digest,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("apply");

    cancel_update_view(
        pool,
        &CancelUpdateViewRequest { plan_id: &plan.plan_id, actor_id: &uid(), command_id: &uid() },
    )
    .await
    .expect("cancel");

    let (state,): (String,) =
        sqlx::query_as("SELECT state FROM materialization_update_plan WHERE public_id = ?")
            .bind(&plan.plan_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(state, "stopped");
}

/// Cancelling a non-applying plan returns typed error.
#[tokio::test]
async fn cancel_refuses_non_applying_plan() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    // Plan is still open (draft), not applying.
    let err = cancel_update_view(
        pool,
        &CancelUpdateViewRequest { plan_id: &plan.plan_id, actor_id: &uid(), command_id: &uid() },
    )
    .await
    .expect_err("should refuse");

    assert_eq!(err.code, ErrorCode::ProjectUpdateViewOperationNotCancellable);
}

// ── Fake InstallerCallbacks for testing run_apply_loop ─────────────────────────

/// A fake `InstallerCallbacks` that records calls and simulates success.
struct FakeCallbacks {
    journal_entry_counter: std::sync::Arc<std::sync::Mutex<i64>>,
}

impl FakeCallbacks {
    fn new() -> Self {
        Self { journal_entry_counter: std::sync::Arc::new(std::sync::Mutex::new(0)) }
    }
}

impl InstallerCallbacks for FakeCallbacks {
    fn on_intent_prepared<'a>(
        &'a self,
        _pool: &'a sqlx::SqlitePool,
        _item: &'a InstallItem,
        _ownership_token: &'a str,
        _command_id: &'a str,
        _lease_owner: &'a str,
        _lease_generation: i64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<(), contracts_core::ContractError>> + Send + 'a,
        >,
    > {
        Box::pin(async move { Ok(()) })
    }

    fn on_installed<'a>(
        &'a self,
        _pool: &'a sqlx::SqlitePool,
        _item_row_id: i64,
        _lease_owner: &'a str,
        _lease_generation: i64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<(), contracts_core::ContractError>> + Send + 'a,
        >,
    > {
        Box::pin(async move { Ok(()) })
    }

    fn on_journaled<'a>(
        &'a self,
        _pool: &'a sqlx::SqlitePool,
        _item: &'a InstallItem,
        _content_fingerprint: &'a str,
        _operation_command_id: &'a str,
        _lease_owner: &'a str,
        _lease_generation: i64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<i64, contracts_core::ContractError>>
                + Send
                + 'a,
        >,
    > {
        let counter = self.journal_entry_counter.clone();
        Box::pin(async move {
            let mut c = counter.lock().unwrap();
            *c += 1;
            Ok(*c)
        })
    }
}

/// `run_apply_loop` with real files: creates a source file, runs apply, and
/// verifies the destination is created with matching content.
#[tokio::test]
async fn run_apply_loop_installs_file_to_destination() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    approve_update_view(
        pool,
        &ApproveUpdateViewRequest {
            plan_id: &plan.plan_id,
            approval_digest: &plan.plan_digest,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("approve");

    apply_update_view(
        pool,
        &ApplyUpdateViewRequest {
            plan_id: &plan.plan_id,
            approval_digest: &plan.plan_digest,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("apply transitions to applying");

    // Set up real source + destination directories.
    let source_dir = tempfile::TempDir::new().unwrap();
    let dest_dir = tempfile::TempDir::new().unwrap();
    let source_file = source_dir.path().join("frame.fits");
    std::fs::write(&source_file, b"test frame bytes").unwrap();

    let source_abs = camino::Utf8PathBuf::from_path_buf(source_file).expect("utf8 source path");
    let dest_root_abs =
        camino::Utf8PathBuf::from_path_buf(dest_dir.path().to_owned()).expect("utf8 dest root");

    // path_resolver returns the same source + dest root for all items.
    let path_resolver =
        |_frame_row_id: i64, _dest_root_row_id: i64| (source_abs.clone(), dest_root_abs.clone());

    let callbacks = FakeCallbacks::new();
    let counter = callbacks.journal_entry_counter.clone();

    run_apply_loop(pool, &plan.plan_id, &uid(), "test-lease", 0, &callbacks, &path_resolver)
        .await
        .expect("apply loop should succeed");

    // One item was journaled.
    assert_eq!(*counter.lock().unwrap(), 1, "expected one journaled item");

    // Plan should be applied.
    let (state,): (String,) =
        sqlx::query_as("SELECT state FROM materialization_update_plan WHERE public_id = ?")
            .bind(&plan.plan_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(state, "applied");
}

/// `run_apply_loop` refuses when the plan is not in `applying` state.
///
/// Recovery idempotency property: `run_apply_loop` checks the plan state before
/// touching the filesystem. A plan in `applied` or `stopped` state must not
/// trigger any install.
#[tokio::test]
async fn run_apply_loop_refuses_non_applying_plan() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    let seq = support::insert_sequence(pool).await;
    let (session_row_id, _) =
        support::insert_light_session(pool, &uid(), &uid(), op_id, target_id, seq, 0).await;
    pin_session(&db, &project_pub, session_row_id).await;

    let plan = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect("plan")
    .plan;

    // Plan is open (draft) — run_apply_loop requires applying state.
    let callbacks = FakeCallbacks::new();
    let source_dir = tempfile::TempDir::new().unwrap();
    let dest_dir = tempfile::TempDir::new().unwrap();
    let source_abs = camino::Utf8PathBuf::from_path_buf(source_dir.path().to_owned()).unwrap();
    let dest_abs = camino::Utf8PathBuf::from_path_buf(dest_dir.path().to_owned()).unwrap();
    let path_resolver = |_: i64, _: i64| (source_abs.clone(), dest_abs.clone());

    let err = run_apply_loop(pool, &plan.plan_id, &uid(), "lease", 0, &callbacks, &path_resolver)
        .await
        .expect_err("non-applying plan must be refused");

    assert_eq!(err.code, ErrorCode::ProjectUpdateViewPlanNotApproved);
    // No callbacks were invoked because the plan state check fails before the loop.
    assert_eq!(*callbacks.journal_entry_counter.lock().unwrap(), 0);
}

/// Plan generation respects the byte ceiling: a session whose byte sum exceeds
/// `MAX_SOURCE_BYTES` (16 TiB) as the first candidate returns the typed error.
///
/// Inserts the frame record directly with an oversized byte_size because
/// frame_record is append-only (UPDATE rejected by trigger).
#[tokio::test]
async fn plan_generation_refuses_session_exceeding_byte_ceiling() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (_actor_id, _cfg_id, op_id, target_id) = seed_basics(&db).await;
    let (_, project_pub) = seed_project(&db).await;

    // Insert session + frame with byte_size exceeding MAX_SOURCE_BYTES (16 TiB + 1).
    let over_ceiling: i64 = 17_592_186_044_417;
    let seq = support::insert_sequence(pool).await;
    let session_pub = uid();
    let frame_pub = uid();
    let file_identity_pub = uid();

    // Insert file identity row (required FK for frame_record).
    sqlx::query("INSERT INTO spec062_file_identity (public_id, created_at) VALUES (?, ?)")
        .bind(&file_identity_pub)
        .bind(TS)
        .execute(pool)
        .await
        .unwrap();

    let (file_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM spec062_file_identity WHERE public_id = ?")
            .bind(&file_identity_pub)
            .fetch_one(pool)
            .await
            .unwrap();

    // Insert session.
    let identity_digest = format!("large-session-{session_pub}");
    sqlx::query(
        "INSERT INTO session
             (public_id, materialization_operation_row_id, kind, ordinal_in_operation,
              identity_digest, observing_night_date, night_derivation,
              canonical_target_row_id, created_sequence, created_at)
         VALUES (?, ?, 'light', 0, ?, '2026-07-21', 'reviewed_local_fallback', ?, ?, ?)",
    )
    .bind(&session_pub)
    .bind(op_id)
    .bind(&identity_digest)
    .bind(target_id)
    .bind(seq)
    .bind(TS)
    .execute(pool)
    .await
    .unwrap();

    let (session_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM session WHERE public_id = ?")
            .bind(&session_pub)
            .fetch_one(pool)
            .await
            .unwrap();

    // Insert frame record with over-ceiling byte_size.
    sqlx::query(
        "INSERT INTO frame_record
             (public_id, file_row_id, byte_size, captured_metadata_digest, created_sequence, created_at)
         VALUES (?, ?, ?, 'large-frame-digest', ?, ?)",
    )
    .bind(&frame_pub)
    .bind(file_row_id)
    .bind(over_ceiling)
    .bind(seq)
    .bind(TS)
    .execute(pool)
    .await
    .unwrap();

    let (frame_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM frame_record WHERE public_id = ?")
            .bind(&frame_pub)
            .fetch_one(pool)
            .await
            .unwrap();

    // Insert session_frame membership.
    sqlx::query(
        "INSERT INTO session_frame
             (session_row_id, frame_row_id, materialization_operation_row_id,
              ordinal, is_representative, created_sequence)
         VALUES (?, ?, ?, 0, 1, ?)",
    )
    .bind(session_row_id)
    .bind(frame_row_id)
    .bind(op_id)
    .bind(seq)
    .execute(pool)
    .await
    .unwrap();

    pin_session(&db, &project_pub, session_row_id).await;

    let err = plan_update_view(
        pool,
        &PlanUpdateViewRequest {
            project_id: &project_pub,
            expected_project_revision: 1,
            actor_id: &uid(),
            command_id: &uid(),
        },
    )
    .await
    .expect_err("should refuse oversized session");

    assert_eq!(err.code, ErrorCode::ProjectUpdateViewSessionTooLarge);
}
