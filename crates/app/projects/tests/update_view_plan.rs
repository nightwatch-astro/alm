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
    approve_update_view, discard_update_view, plan_update_view, query_update_view,
    ApproveUpdateViewRequest, DiscardUpdateViewRequest, PlanUpdateViewRequest,
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
