// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(
    clippy::similar_names,
    clippy::explicit_auto_deref,
    clippy::too_many_lines,
    clippy::items_after_statements
)]

//! Lifecycle and related-session tests for spec 062 project session-pin use cases.
//!
//! Covered:
//! 1. Explicit add pins the exact session; view becomes stale.
//! 2. Adding an already-pinned session is refused.
//! 3. Completed/archived lifecycle refuses addition.
//! 4. Concurrent add on the wrong expected revision is refused.
//! 5. Related-session derivation from panel siblings (US3 acceptance scenario 1).
//! 6. Atomic one-to-many replacement (US5 FR-058).
//! 7. Replacement is refused when `replacement_session_ids` do not match authorized set.
//! 8. `view_state_query` returns correct staleness.
//! 9. `list_session_pins` returns pins in `session_id` order.
//!
//! All tests use an in-memory `SQLite` database via `persistence_topology::test_support`.

use sqlx::Acquire;
use uuid::Uuid;

use app_core_projects::session_membership::{
    add_session_pin, list_related_sessions, list_session_pins, replace_session_pin,
    view_state_query, AddSessionPinRequest, ReplaceSessionPinRequest,
};
use contracts_core::error_code::ErrorCode;
use persistence_core::Database;
use persistence_sessions::repositories::supersession::{insert_supersession, InsertSupersession};
use persistence_topology::repositories::panels::{self, InsertSingletonPanel};
use persistence_topology::test_support as support;

fn uid() -> String {
    Uuid::new_v4().to_string()
}

const TS: &str = "2026-07-22T00:00:00.000000Z";

/// Provision a database and seed one actor, config, materialization op, and target.
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

/// Seed a light session with a singleton panel group. Returns
/// `(session_row_id, session_public_id, group_row_id, group_public_id, rev_row_id, rev_public_id)`.
async fn seed_session_with_panel(
    db: &Database,
    actor_id: i64,
    cfg_id: i64,
    op_id: i64,
    target_id: i64,
    ordinal: i64,
) -> (i64, String, i64, String, i64, String) {
    let pool = db.pool();
    let session_pub_id = uid();
    let frame_pub_id = uid();
    let seq = support::insert_sequence(pool).await;

    let (session_row_id, _) = support::insert_light_session(
        pool,
        &session_pub_id,
        &frame_pub_id,
        op_id,
        target_id,
        seq,
        ordinal,
    )
    .await;

    // Insert session visibility (required by session list queries).
    sqlx::query(
        "INSERT INTO session_visibility_history (session_row_id, visible_sequence, reason_code)
         VALUES (?, ?, 'materialization_applied')",
    )
    .bind(session_row_id)
    .bind(seq)
    .execute(pool)
    .await
    .unwrap();

    let group_pub_id = uid();
    let rev_pub_id = uid();
    let group_seq = support::insert_sequence(pool).await;

    let mut conn = pool.acquire().await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    let (group_row_id, rev_row_id) = panels::insert_singleton_panel_group(
        &mut tx,
        &InsertSingletonPanel {
            group_public_id: &group_pub_id,
            revision_public_id: &rev_pub_id,
            session_row_id,
            canonical_target_row_id: target_id,
            config_revision_row_id: cfg_id,
            actor_row_id: actor_id,
            created_sequence: group_seq,
            created_at: TS,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    (session_row_id, session_pub_id, group_row_id, group_pub_id, rev_row_id, rev_pub_id)
}

// ── Test 1: explicit add pins session; view not stale (no prior snapshot) ───

#[tokio::test]
async fn explicit_add_pins_session_and_no_staleness_without_snapshot() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    let (_, session_pub_id, _, _, _, _) =
        seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, 0).await;

    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;

    let resp = add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &session_pub_id,
            expected_project_revision: 0,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect("add_session_pin should succeed");

    assert_eq!(resp.new_project_revision, 1);
    assert!(!resp.view_stale, "no snapshot → not stale");

    // Verify pin exists in DB.
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM project_membership_revision_session prs
         INNER JOIN project_membership_revision pmr ON pmr.row_id = prs.revision_row_id
         INNER JOIN spec062_project p ON p.row_id = pmr.project_row_id
         WHERE p.public_id = ?",
    )
    .bind(&project_pub_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "exactly one pin after first add");
}

// ── Test 2: adding already-pinned session is refused ─────────────────────────

#[tokio::test]
async fn add_already_pinned_session_is_refused() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    let (_, session_pub_id, _, _, _, _) =
        seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, 0).await;

    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;

    // First add succeeds.
    add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &session_pub_id,
            expected_project_revision: 0,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect("first add should succeed");

    // Second add with same session is refused.
    let err = add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &session_pub_id,
            expected_project_revision: 1,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect_err("duplicate add should be refused");

    assert_eq!(err.code, ErrorCode::ProjectSessionAlreadyPinned, "wrong error code: {err:?}");
}

// ── Test 3: completed / archived lifecycle refuses addition ──────────────────

#[tokio::test]
async fn completed_lifecycle_refuses_session_add() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    let (_, session_pub_id, _, _, _, _) =
        seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, 0).await;

    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;

    // Seed a legacy `projects` row so we can set lifecycle = completed.
    sqlx::query(
        "INSERT INTO projects (id, name, tool, lifecycle, path, created_at, updated_at)
         VALUES (?, 'Test Project', 'PixInsight', 'completed', '/test', ?, ?)",
    )
    .bind(&project_pub_id)
    .bind(TS)
    .bind(TS)
    .execute(pool)
    .await
    .unwrap();

    let err = add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &session_pub_id,
            expected_project_revision: 0,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect_err("completed lifecycle should refuse");

    assert_eq!(
        err.code,
        ErrorCode::ProjectLifecycleDisallowsSessionAdd,
        "wrong error code: {err:?}"
    );
}

// ── Test 4: wrong expected_project_revision is refused ───────────────────────

#[tokio::test]
async fn wrong_expected_revision_is_refused() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    let (_, session_pub_id, _, _, _, _) =
        seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, 0).await;

    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;

    // Revision 42 does not exist on a fresh project (current is 0).
    let err = add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &session_pub_id,
            expected_project_revision: 42,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect_err("wrong revision should be refused");

    assert_eq!(err.code, ErrorCode::ProjectMembershipConflict, "wrong error code: {err:?}");
}

// ── Test 5: related-session derivation from panel siblings ───────────────────

#[tokio::test]
async fn panel_sibling_appears_in_related_sessions() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    // Session A: pinned in project.
    let (session_a_row_id, session_a_pub_id, group_a_row_id, _, rev_a_row_id, _) =
        seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, 0).await;

    // Session B: sibling — add it to session A's panel group head.
    let session_b_pub_id = uid();
    let frame_b_pub_id = uid();
    let seq_b = support::insert_sequence(pool).await;
    let (session_b_row_id, _) = support::insert_light_session(
        pool,
        &session_b_pub_id,
        &frame_b_pub_id,
        op_id,
        target_id,
        seq_b,
        1,
    )
    .await;
    sqlx::query(
        "INSERT INTO session_visibility_history (session_row_id, visible_sequence, reason_code)
         VALUES (?, ?, 'materialization_applied')",
    )
    .bind(session_b_row_id)
    .bind(seq_b)
    .execute(pool)
    .await
    .unwrap();

    // Append session B to the panel group revision (new revision).
    let rev_b_pub_id = uid();
    let seq_r = support::insert_sequence(pool).await;
    let proposal_row_id =
        support::insert_pending_proposal(pool, &uid(), "panel_add", cfg_id, seq_r).await;
    {
        let mut conn = pool.acquire().await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        panels::append_panel_revision(
            &mut tx,
            &panels::AppendPanelRevision {
                revision_public_id: &rev_b_pub_id,
                panel_group_row_id: group_a_row_id,
                parent_revision_row_id: rev_a_row_id,
                current_revision_number: 1,
                representative_session_row_id: session_a_row_id,
                members: &[(session_a_row_id, 0), (session_b_row_id, 1)],
                proposal_row_id,
                config_revision_row_id: cfg_id,
                actor_row_id: actor_id,
                reason_code: "sibling_added",
                created_sequence: seq_r,
                created_at: TS,
                expected_head_generation: 0,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    // Create project and pin session A.
    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;
    add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &session_a_pub_id,
            expected_project_revision: 0,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect("pin session A");

    // List related sessions — session B should appear as panel_sibling.
    let related = list_related_sessions(pool, &project_pub_id, false, None, 50)
        .await
        .expect("list_related_sessions should succeed");

    assert!(
        related
            .iter()
            .any(|r| r.session_id == session_b_pub_id && r.relation_kind == "panel_sibling"),
        "session B should be a panel sibling; got: {related:?}"
    );

    // Session A (already pinned) must NOT appear by default.
    assert!(
        !related.iter().any(|r| r.session_id == session_a_pub_id),
        "pinned session A should not appear when include_pinned=false"
    );
}

// ── Test 6: atomic one-to-many replacement ───────────────────────────────────

#[tokio::test]
async fn atomic_replacement_swaps_predecessor_for_replacements() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    // Predecessor session.
    let (predecessor_row_id, predecessor_pub_id, _, _, _, _) =
        seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, 0).await;

    // Replacement sessions — two of them.
    let repl_a_pub_id = uid();
    let repl_b_pub_id = uid();
    let seq_r = support::insert_sequence(pool).await;

    let (repl_a_row_id, _) =
        support::insert_light_session(pool, &repl_a_pub_id, &uid(), op_id, target_id, seq_r, 1)
            .await;
    let (repl_b_row_id, _) =
        support::insert_light_session(pool, &repl_b_pub_id, &uid(), op_id, target_id, seq_r, 2)
            .await;

    // Seed a reclassification plan revision in 'applied' state using the test helper.
    let cmd_id_r = support::insert_command(pool, &uid(), actor_id).await;
    let seq_rr = support::insert_sequence(pool).await;
    let (plan_rev_row_id, reclass_plan_rev_pub_id) =
        support::insert_applied_reclassification_plan_revision(
            pool,
            predecessor_row_id,
            actor_id,
            cfg_id,
            cmd_id_r,
            seq_rr,
        )
        .await;

    // Seed session_supersession rows linking predecessor → replacements.
    let mut conn = pool.acquire().await.unwrap();
    for (ordinal, repl_row_id) in [(0i64, repl_a_row_id), (1, repl_b_row_id)] {
        let seq_ss = support::insert_sequence(pool).await;
        insert_supersession(
            &mut *conn,
            &InsertSupersession {
                predecessor_session_row_id: predecessor_row_id,
                replacement_session_row_id: repl_row_id,
                kind: "light",
                applied_plan_revision_row_id: plan_rev_row_id,
                ordinal,
                created_sequence: seq_ss,
                created_at: TS,
            },
        )
        .await
        .unwrap();
    }

    // Project: pin the predecessor.
    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;

    add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &predecessor_pub_id,
            expected_project_revision: 0,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect("pin predecessor");

    // Replace predecessor with both replacements.
    let resp = replace_session_pin(
        pool,
        &ReplaceSessionPinRequest {
            project_id: &project_pub_id,
            predecessor_session_id: &predecessor_pub_id,
            replacement_session_ids: &[repl_a_pub_id.as_str(), repl_b_pub_id.as_str()],
            applied_reclassification_plan_revision_id: &reclass_plan_rev_pub_id,
            expected_project_revision: 1,
            actor_id: &actor_pub_id,
        },
    )
    .await
    .expect("replace_session_pin should succeed");

    assert_eq!(resp.new_project_revision, 2);

    // Verify: current head has replacements, not predecessor.
    let pins =
        list_session_pins(pool, &project_pub_id, 2, None, 50).await.expect("list_session_pins");

    let pin_ids: Vec<&str> = pins.iter().map(|p| p.session_id.as_str()).collect();
    assert!(pin_ids.contains(&repl_a_pub_id.as_str()), "replacement A should be pinned");
    assert!(pin_ids.contains(&repl_b_pub_id.as_str()), "replacement B should be pinned");
    assert!(
        !pin_ids.contains(&predecessor_pub_id.as_str()),
        "predecessor should not be pinned after replacement"
    );

    // Source field on replacement pins.
    for pin in &pins {
        if pin.session_id == repl_a_pub_id || pin.session_id == repl_b_pub_id {
            assert_eq!(pin.source, "explicit_replacement", "pin source mismatch");
            assert_eq!(
                pin.replaces_session_id.as_deref(),
                Some(predecessor_pub_id.as_str()),
                "replaces_session_id mismatch"
            );
        }
    }
}

// ── Test 7: replacement refused when IDs do not match authorized set ─────────

#[tokio::test]
async fn replace_refuses_mismatched_replacement_set() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    let (predecessor_row_id, predecessor_pub_id, _, _, _, _) =
        seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, 0).await;

    let repl_pub_id = uid();
    let seq_r = support::insert_sequence(pool).await;
    let (repl_row_id, _) =
        support::insert_light_session(pool, &repl_pub_id, &uid(), op_id, target_id, seq_r, 1).await;

    // Plan revision authorizes only `repl_pub_id`.
    let cmd_id_r = support::insert_command(pool, &uid(), actor_id).await;
    let seq_rr = support::insert_sequence(pool).await;
    let (plan_rev_row_id, reclass_plan_rev_pub_id) =
        support::insert_applied_reclassification_plan_revision(
            pool,
            predecessor_row_id,
            actor_id,
            cfg_id,
            cmd_id_r,
            seq_rr,
        )
        .await;

    let mut conn = pool.acquire().await.unwrap();
    let seq_ss = support::insert_sequence(pool).await;
    insert_supersession(
        &mut *conn,
        &InsertSupersession {
            predecessor_session_row_id: predecessor_row_id,
            replacement_session_row_id: repl_row_id,
            kind: "light",
            applied_plan_revision_row_id: plan_rev_row_id,
            ordinal: 0,
            created_sequence: seq_ss,
            created_at: TS,
        },
    )
    .await
    .unwrap();

    // Project: pin predecessor.
    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;
    add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &predecessor_pub_id,
            expected_project_revision: 0,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect("pin predecessor");

    // Try to replace with a different (unauthorized) session.
    let wrong_session_pub_id = uid();
    let (_, _) = support::insert_light_session(
        pool,
        &wrong_session_pub_id,
        &uid(),
        op_id,
        target_id,
        seq_r,
        3,
    )
    .await;

    let err = replace_session_pin(
        pool,
        &ReplaceSessionPinRequest {
            project_id: &project_pub_id,
            predecessor_session_id: &predecessor_pub_id,
            replacement_session_ids: &[wrong_session_pub_id.as_str()],
            applied_reclassification_plan_revision_id: &reclass_plan_rev_pub_id,
            expected_project_revision: 1,
            actor_id: &actor_pub_id,
        },
    )
    .await
    .expect_err("mismatched replacement set should be refused");

    assert_eq!(
        err.code,
        ErrorCode::ProjectReclassificationRevisionInvalid,
        "wrong error code: {err:?}"
    );
}

// ── Test 8: view_state_query returns correct staleness ───────────────────────

#[tokio::test]
async fn view_state_query_returns_pinned_count_and_no_staleness_without_snapshot() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    let (_, session_pub_id, _, _, _, _) =
        seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, 0).await;

    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;

    let before =
        view_state_query(pool, &project_pub_id).await.expect("view_state_query before add");
    assert_eq!(before.pinned_session_count, 0);
    assert!(!before.stale);

    add_session_pin(
        pool,
        &AddSessionPinRequest {
            project_id: &project_pub_id,
            session_id: &session_pub_id,
            expected_project_revision: 0,
            actor_id: &actor_pub_id,
            related_session_evidence_id: None,
        },
    )
    .await
    .expect("add pin");

    let after = view_state_query(pool, &project_pub_id).await.expect("view_state_query after add");
    assert_eq!(after.pinned_session_count, 1);
    assert_eq!(after.project_revision, 1);
    // No materialization snapshot → not stale (no view to be stale against).
    assert!(!after.stale);
    // One session is pinned but never materialized (no snapshot yet).
    assert_eq!(after.unmaterialized_session_count, 1);
}

// ── Test 9: list_session_pins returns pins in session-id order ───────────────

#[tokio::test]
async fn list_session_pins_returns_pins_in_session_id_order() {
    let db = support::setup_db().await;
    let pool = db.pool();
    let (actor_id, cfg_id, op_id, target_id) = seed_basics(&db).await;
    let actor_pub_id =
        sqlx::query_scalar::<_, String>("SELECT public_id FROM spec062_actor WHERE row_id = ?")
            .bind(actor_id)
            .fetch_one(pool)
            .await
            .unwrap();

    // Add three sessions to the project.
    let mut session_ids: Vec<String> = Vec::new();
    for i in 0..3_i64 {
        let (_, pub_id, _, _, _, _) =
            seed_session_with_panel(&db, actor_id, cfg_id, op_id, target_id, i).await;
        session_ids.push(pub_id);
    }

    let project_pub_id = uid();
    support::insert_spec062_project(pool, &project_pub_id).await;

    // Add them — each add requires the current revision.
    // Adding in sorted-id order to get a known final state;
    // the pin list test only needs them all pinned.
    let mut current_rev = 0i64;
    for sid in &session_ids {
        add_session_pin(
            pool,
            &AddSessionPinRequest {
                project_id: &project_pub_id,
                session_id: sid,
                expected_project_revision: current_rev,
                actor_id: &actor_pub_id,
                related_session_evidence_id: None,
            },
        )
        .await
        .expect("add pin");
        current_rev += 1;
    }

    let pins = list_session_pins(pool, &project_pub_id, current_rev, None, 50)
        .await
        .expect("list_session_pins");

    let returned_ids: Vec<&str> = pins.iter().map(|p| p.session_id.as_str()).collect();
    let mut expected = session_ids.clone();
    expected.sort_unstable();
    let expected_ids: Vec<&str> = expected.iter().map(String::as_str).collect();
    assert_eq!(returned_ids, expected_ids, "pins must be ordered by session_id ASC");
}
