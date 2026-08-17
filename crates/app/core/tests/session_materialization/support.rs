// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared test-fixture helpers for session-materialization integration tests.
//!
//! Seeds the minimal FK chain required by `session_materialization_operation`
//! and its related tables.

#![allow(dead_code)]

use persistence_core::Database;
use sqlx::SqlitePool;

/// Provision an isolated in-memory database.
pub async fn setup_db() -> Database {
    persistence_core::test_support::setup_db().await
}

// ── Seed helpers ────────────────────────────────────────────────────────

/// Insert the baseline rows required by most fixture builders.
///
/// Inserts: actor, config_revision, repository_change, command_execution.
/// Returns `(actor_row_id, config_revision_row_id, command_row_id)`.
async fn seed_base(pool: &SqlitePool, command_public_id: &str) -> (i64, i64, i64) {
    sqlx::query("INSERT INTO spec062_actor VALUES (1, 'actor-sys', '2026-07-22T00:00:00.000000Z')")
        .execute(pool)
        .await
        .expect("seed actor");

    sqlx::query(
        "INSERT INTO spec062_config_revision VALUES \
         (1, 'cfg-rev-001', 1, 'cfg-digest', '2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed config revision");

    sqlx::query(
        "INSERT INTO repository_change(command_row_id, created_at) \
         VALUES (NULL, '2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed repository change");

    sqlx::query(
        "INSERT INTO command_execution \
         (row_id, public_id, actor_row_id, operation, canonical_payload_digest, state, \
          response_json, created_at, finished_at) \
         VALUES (1, ?, 1, 'inbox.materialization.apply', 'pd', 'applied', '{}', \
                 '2026-07-22T00:00:00.000000Z', '2026-07-22T00:00:01.000000Z')",
    )
    .bind(command_public_id)
    .execute(pool)
    .await
    .expect("seed command");

    (1, 1, 1)
}

/// Insert a site, site resolution, and an approved revision.
/// Returns `site_resolution_revision_row_id`.
async fn seed_site_resolution(pool: &SqlitePool, night_date: &str) -> i64 {
    sqlx::query(
        "INSERT INTO acquisition_site \
         (row_id, public_id, label, timezone_name, timezone_state, created_sequence, created_at) \
         VALUES (1, 'site-001', 'Test Site', 'UTC', 'confirmed', 1, '2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed site");

    sqlx::query(
        "INSERT INTO acquisition_site_resolution \
         (row_id, public_id, created_at) \
         VALUES (1, 'res-001', '2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed site resolution");

    let night = night_date.to_owned();
    sqlx::query(
        "INSERT INTO acquisition_site_resolution_revision \
         (row_id, public_id, resolution_row_id, revision_number, state, \
          selected_site_row_id, timezone_name, canonical_exposure_at_utc, \
          observing_night_date, canonical_digest, actor_row_id, command_row_id, \
          created_sequence, created_at) \
         VALUES (1, 'rev-001', 1, 1, 'resolved', 1, 'UTC', \
                 '2026-07-01T22:00:00Z', ?, 'cd-1', 1, 1, 1, \
                 '2026-07-22T00:00:00.000000Z')",
    )
    .bind(&night)
    .execute(pool)
    .await
    .expect("seed site resolution revision");

    // Set the head on the resolution aggregate
    sqlx::query(
        "UPDATE acquisition_site_resolution SET head_revision_row_id = 1, head_generation = 1 \
         WHERE row_id = 1",
    )
    .execute(pool)
    .await
    .expect("update site resolution head");

    1
}

/// Insert a `spec062_target` and `spec062_inbox_materialization_plan` stub.
async fn seed_plan_stub(pool: &SqlitePool, target_public_id: &str) {
    sqlx::query("INSERT INTO spec062_target VALUES (1, ?, '2026-07-22T00:00:00.000000Z')")
        .bind(target_public_id)
        .execute(pool)
        .await
        .expect("seed spec062_target");

    sqlx::query(
        "INSERT INTO spec062_inbox_materialization_plan \
         VALUES (1, 'plan-001', '2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed inbox_materialization_plan");
}

/// Insert `frame_record` rows. Frame `row_id` values start at `frame_id_start`.
async fn seed_frames(pool: &SqlitePool, frame_id_start: i64, count: usize) {
    for i in 0..i64::try_from(count).unwrap() {
        let row_id = frame_id_start + i;
        sqlx::query(
            "INSERT INTO spec062_file_identity VALUES (?, ?, NULL, '2026-07-22T00:00:00.000000Z')",
        )
        .bind(row_id)
        .bind(format!("file-{row_id:012}"))
        .execute(pool)
        .await
        .expect("seed file identity");

        sqlx::query(
            "INSERT INTO frame_record \
             (row_id, public_id, file_row_id, byte_size, captured_metadata_digest, \
              created_sequence, created_at) \
             VALUES (?, ?, ?, 4096, ?, 1, '2026-07-22T00:00:00.000000Z')",
        )
        .bind(row_id)
        .bind(format!("frame-{row_id:012}"))
        .bind(row_id)
        .bind(format!("meta-{row_id}"))
        .execute(pool)
        .await
        .expect("seed frame record");
    }
}

/// Insert an `inbox_materialization_plan_result_snapshot` with one proposed
/// session containing `frame_count` frames (starting at `frame_id_start`).
async fn seed_plan_snapshot_single_session(
    pool: &SqlitePool,
    kind: &str,
    identity_digest: &str,
    frame_id_start: i64,
    frame_count: usize,
    site_revision_row_id: i64,
) -> i64 {
    sqlx::query(
        "INSERT INTO inbox_materialization_plan_result_snapshot \
         (row_id, public_id, plan_row_id, plan_revision, config_revision_row_id, \
          input_evidence_revision, proposed_session_count, frame_count, blocked_frame_count, \
          canonical_digest, created_sequence, created_at) \
         VALUES (1, 'snap-001', 1, 1, 1, 1, 1, ?, 0, 'snap-digest', 1, \
                 '2026-07-22T00:00:00.000000Z')",
    )
    .bind(i64::try_from(frame_count).unwrap())
    .execute(pool)
    .await
    .expect("seed plan snapshot");

    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session \
         (row_id, snapshot_row_id, proposed_session_key, kind, \
          site_resolution_revision_row_id, identity_digest, ordinal, frame_count) \
         VALUES (1, 1, 'session-key-0', ?, ?, ?, 0, ?)",
    )
    .bind(kind)
    .bind(site_revision_row_id)
    .bind(identity_digest)
    .bind(i64::try_from(frame_count).unwrap())
    .execute(pool)
    .await
    .expect("seed proposed session");

    for i in 0..i64::try_from(frame_count).unwrap() {
        sqlx::query(
            "INSERT INTO inbox_plan_result_proposed_session_frame \
             (proposed_session_row_id, frame_row_id, ordinal) \
             VALUES (1, ?, ?)",
        )
        .bind(frame_id_start + i)
        .bind(i)
        .execute(pool)
        .await
        .expect("seed proposed session frame");
    }

    1 // snapshot row_id
}

/// Insert an `inbox_ingestion_operation` row linking the operation to the
/// plan result snapshot.
async fn seed_inbox_ingestion_operation(
    pool: &SqlitePool,
    operation_row_id: i64,
    snapshot_row_id: i64,
) {
    sqlx::query(
        "INSERT INTO inbox_ingestion_operation \
         (operation_row_id, inbox_plan_result_snapshot_row_id, approved_plan_digest, \
          approved_by_actor_row_id, approved_at) \
         VALUES (?, ?, 'test-digest', 1, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(operation_row_id)
    .bind(snapshot_row_id)
    .execute(pool)
    .await
    .expect("seed inbox_ingestion_operation");
}

/// Seed a minimal complete context for a single-session apply.
///
/// `target_public_id` is the `spec062_target.public_id`. Frame row IDs are
/// `frame_id_start` and `frame_id_start + 1` (two frames).
///
/// Returns `(operation_row_id, state_version)`.
pub async fn seed_minimal_apply_context(
    pool: &SqlitePool,
    target_public_id: &str,
    frame_id_start: i64,
    op_public_id_suffix: u64,
) -> (i64, i64) {
    let cmd_public_id = format!("cmd-{op_public_id_suffix:04}");
    seed_base(pool, &cmd_public_id).await;
    let site_rev_row_id = seed_site_resolution(pool, "2026-07-01").await;
    seed_plan_stub(pool, target_public_id).await;
    seed_frames(pool, frame_id_start, 2).await;
    let snapshot_row_id = seed_plan_snapshot_single_session(
        pool,
        "light",
        "digest-a",
        frame_id_start,
        2,
        site_rev_row_id,
    )
    .await;

    // Insert the materialization operation in `ready` state
    let op_public_id = format!("op-{op_public_id_suffix:04}");
    sqlx::query(
        "INSERT INTO session_materialization_operation \
         (row_id, public_id, kind, command_row_id, config_revision_row_id, state, \
          created_sequence, created_at) \
         VALUES (1, ?, 'inbox_ingestion', 1, 1, 'ready', 1, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(&op_public_id)
    .execute(pool)
    .await
    .expect("seed operation");

    seed_inbox_ingestion_operation(pool, 1, snapshot_row_id).await;

    (1, 0)
}

/// Seed context for two distinct light sessions, each with one frame.
/// Returns `(operation_row_id, state_version)`.
pub async fn seed_two_session_apply_context(
    pool: &SqlitePool,
    target_public_id: &str,
) -> (i64, i64) {
    seed_base(pool, "cmd-two").await;
    let site_rev_row_id = seed_site_resolution(pool, "2026-07-02").await;
    seed_plan_stub(pool, target_public_id).await;
    // Two frames with distinct row IDs
    seed_frames(pool, 10, 2).await;

    // Plan snapshot with two proposed sessions
    sqlx::query(
        "INSERT INTO inbox_materialization_plan_result_snapshot \
         (row_id, public_id, plan_row_id, plan_revision, config_revision_row_id, \
          input_evidence_revision, proposed_session_count, frame_count, blocked_frame_count, \
          canonical_digest, created_sequence, created_at) \
         VALUES (1, 'snap-two', 1, 1, 1, 1, 2, 2, 0, 'snap-digest-two', 1, \
                 '2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed snapshot");

    // Proposed session 1
    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session \
         (row_id, snapshot_row_id, proposed_session_key, kind, \
          site_resolution_revision_row_id, identity_digest, ordinal, frame_count) \
         VALUES (1, 1, 'key-1', 'light', ?, 'digest-1', 0, 1)",
    )
    .bind(site_rev_row_id)
    .execute(pool)
    .await
    .expect("seed proposed session 1");

    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session_frame \
         (proposed_session_row_id, frame_row_id, ordinal) VALUES (1, 10, 0)",
    )
    .execute(pool)
    .await
    .expect("seed frame 1");

    // Proposed session 2
    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session \
         (row_id, snapshot_row_id, proposed_session_key, kind, \
          site_resolution_revision_row_id, identity_digest, ordinal, frame_count) \
         VALUES (2, 1, 'key-2', 'light', ?, 'digest-2', 1, 1)",
    )
    .bind(site_rev_row_id)
    .execute(pool)
    .await
    .expect("seed proposed session 2");

    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session_frame \
         (proposed_session_row_id, frame_row_id, ordinal) VALUES (2, 11, 0)",
    )
    .execute(pool)
    .await
    .expect("seed frame 2");

    // Operation
    sqlx::query(
        "INSERT INTO session_materialization_operation \
         (row_id, public_id, kind, command_row_id, config_revision_row_id, state, \
          created_sequence, created_at) \
         VALUES (1, 'op-two', 'inbox_ingestion', 1, 1, 'ready', 1, \
                 '2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed operation");

    seed_inbox_ingestion_operation(pool, 1, 1).await;

    (1, 0)
}

/// Seed the first operation for the no-append invariant test (FR-004).
///
/// Seeds: base rows, site resolution, plan stub, frame 20, snapshot 1,
/// proposed session with `identity_digest = "shared-digest"`, operation 1.
/// Returns `(operation_row_id=1, state_version=0)`.
pub async fn seed_second_operation_context(
    pool: &SqlitePool,
    target_public_id: &str,
    pass: u8,
) -> (i64, i64) {
    if pass == 1 {
        seed_first_no_append_operation(pool, target_public_id).await
    } else {
        seed_second_no_append_operation(pool).await
    }
}

/// Seeds the first no-append operation (pass 1).
async fn seed_first_no_append_operation(pool: &SqlitePool, target_public_id: &str) -> (i64, i64) {
    seed_base(pool, "cmd-e1").await;
    seed_site_resolution(pool, "2026-07-03").await;
    seed_plan_stub(pool, target_public_id).await;
    seed_frames(pool, 20, 1).await;

    sqlx::query(
        "INSERT INTO inbox_materialization_plan_result_snapshot \
         (row_id, public_id, plan_row_id, plan_revision, config_revision_row_id, \
          input_evidence_revision, proposed_session_count, frame_count, blocked_frame_count, \
          canonical_digest, created_sequence, created_at) \
         VALUES (1,'snap-e1',1,1,1,1,1,1,0,'snap-dg-e1',1,'2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed snapshot op1");

    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session \
         (row_id,snapshot_row_id,proposed_session_key,kind,\
          site_resolution_revision_row_id,identity_digest,ordinal,frame_count) \
         VALUES (1,1,'key-e1','light',1,'shared-digest',0,1)",
    )
    .execute(pool)
    .await
    .expect("seed proposed session op1");

    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session_frame \
         (proposed_session_row_id,frame_row_id,ordinal) VALUES (1,20,0)",
    )
    .execute(pool)
    .await
    .expect("seed frame op1");

    sqlx::query(
        "INSERT INTO session_materialization_operation \
         (row_id,public_id,kind,command_row_id,config_revision_row_id,state,\
          created_sequence,created_at) \
         VALUES (1,'op-e1','inbox_ingestion',1,1,'ready',1,'2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed operation op1");

    seed_inbox_ingestion_operation(pool, 1, 1).await;
    (1, 0)
}

/// Seeds the second no-append operation (pass 2).
///
/// Frame 21 has the same session metadata as frame 20 (`shared-digest`) but is
/// a different physical file — exercises FR-004 (later ingestion → new session).
async fn seed_second_no_append_operation(pool: &SqlitePool) -> (i64, i64) {
    seed_frames(pool, 21, 1).await;

    sqlx::query(
        "INSERT INTO command_execution \
         (row_id,public_id,actor_row_id,operation,canonical_payload_digest,state,\
          response_json,created_at,finished_at) \
         VALUES (2,'cmd-e2',1,'inbox.materialization.apply','pd2','applied','{}',\
                 '2026-07-22T00:00:00.000000Z','2026-07-22T00:00:01.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed command op2");

    sqlx::query(
        "INSERT INTO inbox_materialization_plan_result_snapshot \
         (row_id,public_id,plan_row_id,plan_revision,config_revision_row_id,\
          input_evidence_revision,proposed_session_count,frame_count,blocked_frame_count,\
          canonical_digest,created_sequence,created_at) \
         VALUES (2,'snap-e2',1,2,1,2,1,1,0,'snap-dg-e2',1,'2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed snapshot op2");

    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session \
         (row_id,snapshot_row_id,proposed_session_key,kind,\
          site_resolution_revision_row_id,identity_digest,ordinal,frame_count) \
         VALUES (2,2,'key-e2','light',1,'shared-digest',0,1)",
    )
    .execute(pool)
    .await
    .expect("seed proposed session op2");

    sqlx::query(
        "INSERT INTO inbox_plan_result_proposed_session_frame \
         (proposed_session_row_id,frame_row_id,ordinal) VALUES (2,21,0)",
    )
    .execute(pool)
    .await
    .expect("seed frame op2");

    sqlx::query(
        "INSERT INTO session_materialization_operation \
         (row_id,public_id,kind,command_row_id,config_revision_row_id,state,\
          created_sequence,created_at) \
         VALUES (2,'op-e2','inbox_ingestion',2,1,'ready',1,'2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed operation op2");

    sqlx::query(
        "INSERT INTO inbox_ingestion_operation \
         (operation_row_id,inbox_plan_result_snapshot_row_id,approved_plan_digest,\
          approved_by_actor_row_id,approved_at) \
         VALUES (2,2,'test-digest',1,'2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("seed inbox_ingestion_operation op2");

    (2, 0)
}
