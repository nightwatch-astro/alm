// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-SQLite integration tests for Inbox session materialization
//! (Spec 062 US1).
//!
//! Covers:
//! 1. Apply with two frames in one proposed session → one immutable session +
//!    frame memberships + singleton panel group committed atomically.
//! 2. Apply with two proposed sessions (distinct identity digests) → two
//!    sessions, two panel groups.
//! 3. Idempotent retry: a second apply with the same operation returns the
//!    existing result without duplicating rows.
//! 4. Cancel before commit: no session rows are written; operation reaches
//!    `cancelled`.
//! 5. A mid-transaction failure releases the write lock, so later writes still
//!    work.
//!
//! These tests use the real in-memory SQLite database via
//! `persistence_core::test_support::setup_db`. Case 5 is the exception and opens
//! a file-backed database; its doc comment says why.

mod support;

use support::*;

// ── 1. Single proposed session → one immutable session ───────────────────

#[tokio::test]
async fn apply_creates_one_session_and_panel_group() {
    let db = setup_db().await;
    let pool = db.pool();

    let (op_row_id, state_version) = seed_minimal_apply_context(pool, "target-a", 1, 2).await;

    let progress =
        app_core_inbox::session_materialization::progress::MaterializationProgress::new(1, 2);
    let result = app_core_inbox::session_materialization::apply::run_apply(
        pool,
        app_core_inbox::session_materialization::apply::ApplyParams {
            operation_row_id: op_row_id,
            operation_state_version: state_version,
            approved_plan_digest: "test-digest",
            actor_public_id: "actor-001",
            canonical_target_public_id: Some("target-a"),
            progress,
        },
    )
    .await;

    assert!(result.is_ok(), "apply failed: {result:?}");

    // Operation must be `applied`
    let op: (String,) =
        sqlx::query_as("SELECT state FROM session_materialization_operation WHERE row_id = ?")
            .bind(op_row_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(op.0, "applied");

    // Exactly one session
    let session_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM session WHERE materialization_operation_row_id = ?")
            .bind(op_row_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(session_count.0, 1, "must create exactly one session");

    // Exactly two frame memberships
    let frame_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM session_frame WHERE materialization_operation_row_id = ?",
    )
    .bind(op_row_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(frame_count.0, 2, "must create exactly two frame memberships");

    // Exactly one panel group
    let group_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM panel_group WHERE status = 'active'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(group_count.0, 1, "must create exactly one singleton panel group");

    // The panel group has an accepted head revision
    let head_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM panel_group WHERE head_revision_row_id IS NOT NULL")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(head_count.0, 1, "panel group must have an accepted head revision");

    // Result snapshot must exist and counts must match
    let snap: (i64, i64, i64) = sqlx::query_as(
        "SELECT session_count, membership_count, singleton_group_count
         FROM session_materialization_result_snapshot
         WHERE operation_row_id = ?",
    )
    .bind(op_row_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(snap.0, 1, "snapshot session_count must be 1");
    assert_eq!(snap.1, 2, "snapshot membership_count must be 2");
    assert_eq!(snap.2, 1, "snapshot singleton_group_count must be 1");
}

// ── 2. Two proposed sessions with different identity digests ────────────

#[tokio::test]
async fn apply_creates_two_sessions_for_two_partitions() {
    let db = setup_db().await;
    let pool = db.pool();

    let (op_row_id, state_version) = seed_two_session_apply_context(pool, "target-b").await;

    let progress =
        app_core_inbox::session_materialization::progress::MaterializationProgress::new(2, 2);
    let result = app_core_inbox::session_materialization::apply::run_apply(
        pool,
        app_core_inbox::session_materialization::apply::ApplyParams {
            operation_row_id: op_row_id,
            operation_state_version: state_version,
            approved_plan_digest: "test-digest-2",
            actor_public_id: "actor-002",
            canonical_target_public_id: Some("target-b"),
            progress,
        },
    )
    .await;

    assert!(result.is_ok(), "apply failed: {result:?}");

    let session_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM session WHERE materialization_operation_row_id = ?")
            .bind(op_row_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(session_count.0, 2, "must create two sessions");

    let group_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM panel_group WHERE status = 'active'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(group_count.0, 2, "each light session gets its own singleton panel group");

    // Frame memberships: one per session
    let frame_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM session_frame WHERE materialization_operation_row_id = ?",
    )
    .bind(op_row_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(frame_count.0, 2, "two frame memberships total");
}

// ── 3. Idempotency: operation already applied → zero duplicate rows ───────

#[tokio::test]
async fn apply_is_idempotent_via_ledger_replay() {
    let db = setup_db().await;
    let pool = db.pool();

    let (op_row_id, state_version) = seed_minimal_apply_context(pool, "target-c", 3, 4).await;

    let run = |op_row_id: i64, sv: i64| {
        let pool = pool.clone();
        async move {
            let progress =
                app_core_inbox::session_materialization::progress::MaterializationProgress::new(
                    1, 1,
                );
            app_core_inbox::session_materialization::apply::run_apply(
                &pool,
                app_core_inbox::session_materialization::apply::ApplyParams {
                    operation_row_id: op_row_id,
                    operation_state_version: sv,
                    approved_plan_digest: "idem-digest",
                    actor_public_id: "actor-003",
                    canonical_target_public_id: Some("target-c"),
                    progress,
                },
            )
            .await
        }
    };

    // First apply must succeed
    run(op_row_id, state_version).await.expect("first apply");

    // The operation is now `applied`; a second apply attempt must fail with
    // a CAS error (state no longer `ready`), not create duplicate rows.
    let second = run(op_row_id, state_version).await;
    assert!(second.is_err(), "second apply must fail (CAS on ready→applying)");

    // No duplicate sessions
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM session WHERE materialization_operation_row_id = ?")
            .bind(op_row_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1, "no duplicate sessions after idempotent retry");
}

// ── 5. No-append invariant: two operations with the same identity digest ──
//
// FR-004: a later ingestion creates a NEW session even when all metadata
// matches an accepted session. The schema permits the same identity_digest
// across operations; the apply loop never upserts or appends.

#[tokio::test]
async fn two_operations_with_same_identity_digest_create_two_distinct_sessions() {
    let db = setup_db().await;
    let pool = db.pool();

    // Seed and apply the first operation (one light session, frame row_id=20).
    let (op1_row_id, sv1) = seed_second_operation_context(pool, "target-e", 1).await;
    let progress1 =
        app_core_inbox::session_materialization::progress::MaterializationProgress::new(1, 1);
    app_core_inbox::session_materialization::apply::run_apply(
        pool,
        app_core_inbox::session_materialization::apply::ApplyParams {
            operation_row_id: op1_row_id,
            operation_state_version: sv1,
            approved_plan_digest: "digest-op1",
            actor_public_id: "actor-e1",
            canonical_target_public_id: Some("target-e"),
            progress: progress1,
        },
    )
    .await
    .expect("first operation apply");

    // Seed a second, independent operation with the same identity_digest but a
    // different frame (frame row_id=21). Different frame avoids the
    // UNIQUE(materialization_operation_row_id, frame_row_id) constraint while
    // keeping the session identity identical — exactly the "same metadata, later
    // night" scenario from US1 acceptance scenario 2.
    let (op2_row_id, sv2) = seed_second_operation_context(pool, "target-e", 2).await;
    let progress2 =
        app_core_inbox::session_materialization::progress::MaterializationProgress::new(1, 1);
    app_core_inbox::session_materialization::apply::run_apply(
        pool,
        app_core_inbox::session_materialization::apply::ApplyParams {
            operation_row_id: op2_row_id,
            operation_state_version: sv2,
            approved_plan_digest: "digest-op2",
            actor_public_id: "actor-e2",
            canonical_target_public_id: Some("target-e"),
            progress: progress2,
        },
    )
    .await
    .expect("second operation apply");

    // Both operations must be applied.
    let applied_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM session_materialization_operation WHERE state = 'applied'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(applied_count.0, 2, "both operations must reach applied state");

    // Two distinct session rows — one per operation, never merged or appended.
    let total_sessions: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM session").fetch_one(pool).await.unwrap();
    assert_eq!(total_sessions.0, 2, "each approved operation creates a distinct session");

    // The two sessions have different row_ids and different operation FK values.
    let ops: Vec<(i64,)> =
        sqlx::query_as("SELECT DISTINCT materialization_operation_row_id FROM session")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(ops.len(), 2, "each session must be owned by a different operation");

    // No session was ever modified (append-only: frame membership count stays 1 each).
    let frame_counts: Vec<(i64,)> =
        sqlx::query_as("SELECT COUNT(*) FROM session_frame GROUP BY session_row_id ORDER BY 1")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(
        frame_counts,
        vec![(1,), (1,)],
        "each session has exactly one frame, never appended"
    );
}

// ── 4. Cancel before commit leaves no session rows ────────────────────────

#[tokio::test]
async fn cancel_before_commit_produces_no_sessions() {
    let db = setup_db().await;
    let pool = db.pool();

    let (op_row_id, state_version) = seed_minimal_apply_context(pool, "target-d", 5, 6).await;

    // Pre-set cancel before calling run_apply so the loop never writes a session
    let progress =
        app_core_inbox::session_materialization::progress::MaterializationProgress::new(1, 1);
    progress.request_cancel();

    let result = app_core_inbox::session_materialization::apply::run_apply(
        pool,
        app_core_inbox::session_materialization::apply::ApplyParams {
            operation_row_id: op_row_id,
            operation_state_version: state_version,
            approved_plan_digest: "cancel-digest",
            actor_public_id: "actor-004",
            canonical_target_public_id: Some("target-d"),
            progress,
        },
    )
    .await;

    assert!(result.is_ok(), "cancel must complete without error: {result:?}");

    let state: (String,) =
        sqlx::query_as("SELECT state FROM session_materialization_operation WHERE row_id = ?")
            .bind(op_row_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(state.0, "cancelled", "operation must reach cancelled state");

    let session_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM session WHERE materialization_operation_row_id = ?")
            .bind(op_row_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(session_count.0, 0, "cancelled apply must write zero sessions");
}

// ── 5. A failure mid-transaction leaves the database writable ────────────

/// Fail an apply after it has written rows inside its per-session transaction,
/// then write again on the same pool.
///
/// A light session without a canonical target is rejected after `insert_session`,
/// the visibility row, and every frame membership have already been written, so
/// the error leaves an open `BEGIN IMMEDIATE` behind. SQLite does not roll that
/// back when the connection returns to the pool, and this pool sets no
/// `after_release` reset, so the transaction stays open on an idle connection.
/// Both assertions below catch that, because it surfaces two ways: the pool hands
/// the poisoned connection back and `BEGIN IMMEDIATE` fails with `cannot start a
/// transaction within a transaction`, or it hands back a different connection and
/// the write fails with `database is locked` once the busy timeout expires.
///
/// The row-count assertion alone does not cover this. An abandoned transaction
/// is never committed, so its rows are invisible either way.
///
/// This test is the one case in this file that does not use `setup_db`. That
/// helper opens a shared-cache in-memory database, where the same failure does
/// not reproduce: with the rollback deleted, every test here still passes. A
/// file-backed database is what production uses, and it does reproduce.
#[tokio::test]
async fn a_failed_apply_leaves_the_database_writable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("apply.db").display());
    let db = persistence_core::Database::connect(&url).await.expect("open a file-backed database");
    db.migrate().await.expect("migrate");
    let pool = db.pool();

    let (op_row_id, state_version) = seed_minimal_apply_context(pool, "target-e", 9, 5).await;

    let progress =
        app_core_inbox::session_materialization::progress::MaterializationProgress::new(1, 2);
    let result = app_core_inbox::session_materialization::apply::run_apply(
        pool,
        app_core_inbox::session_materialization::apply::ApplyParams {
            operation_row_id: op_row_id,
            operation_state_version: state_version,
            approved_plan_digest: "test-digest",
            actor_public_id: "actor-001",
            // The proposed session is a light session, so the apply loop demands
            // a canonical target and fails without one.
            canonical_target_public_id: None,
            progress,
        },
    )
    .await;

    assert!(result.is_err(), "a light session without a canonical target must fail");

    let session_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM session WHERE materialization_operation_row_id = ?")
            .bind(op_row_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(session_count.0, 0, "a failed apply must leave no session rows");

    let mut conn = pool.acquire().await.expect("acquire a second connection");
    let begin = sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await;
    assert!(begin.is_ok(), "the failed apply must release the write lock, not hold it: {begin:?}");

    // Writing a row proves the lock is actually free. `BEGIN IMMEDIATE` alone is
    // not enough of an assertion under every SQLite locking mode.
    let write =
        sqlx::query("INSERT INTO repository_change(command_row_id, created_at) VALUES (NULL, ?)")
            .bind("2026-07-22T00:00:01.000000Z")
            .execute(&mut *conn)
            .await;
    assert!(write.is_ok(), "a write must succeed after a failed apply: {write:?}");

    sqlx::query("ROLLBACK").execute(&mut *conn).await.expect("release the test's own lock");
}
