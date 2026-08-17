// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared in-process test fixtures for persistence_topology integration tests.
//!
//! Available under `cfg(test)` within this crate and via the `test-fixture`
//! feature for external consumers.
//!
//! The helper functions insert the minimum rows required to satisfy FK
//! constraints for the spec 062 topology tables. Each helper takes the owning
//! connection pool, not a `&mut SqliteConnection`, so callers can coordinate
//! multi-step seeds outside a transaction.

use sqlx::SqlitePool;

use persistence_core::Database;

/// Provision an isolated in-memory database with all migrations applied.
///
/// # Panics
///
/// Panics if the in-memory pool or migrations fail.
pub async fn setup_db() -> Database {
    persistence_core::test_support::setup_db().await
}

/// Insert a minimal `spec062_actor` row.
///
/// # Panics
///
/// Panics on SQL failure.
pub async fn insert_actor(pool: &SqlitePool, public_id: &str) -> i64 {
    sqlx::query(
        "INSERT OR IGNORE INTO spec062_actor (public_id, created_at)
         VALUES (?, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .execute(pool)
    .await
    .expect("insert_actor failed");

    let (row_id,): (i64,) = sqlx::query_as("SELECT row_id FROM spec062_actor WHERE public_id = ?")
        .bind(public_id)
        .fetch_one(pool)
        .await
        .expect("insert_actor select failed");
    row_id
}

/// Insert a minimal `spec062_config_revision` row.
///
/// # Panics
pub async fn insert_config_revision(
    pool: &SqlitePool,
    public_id: &str,
    revision_number: i64,
) -> i64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    public_id.hash(&mut hasher);
    revision_number.hash(&mut hasher);
    let digest = format!("config-digest-{:x}", hasher.finish());

    sqlx::query(
        "INSERT OR IGNORE INTO spec062_config_revision
             (public_id, revision_number, canonical_digest, created_at)
         VALUES (?, ?, ?, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .bind(revision_number)
    .bind(&digest)
    .execute(pool)
    .await
    .expect("insert_config_revision failed");

    let (row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM spec062_config_revision WHERE public_id = ?")
            .bind(public_id)
            .fetch_one(pool)
            .await
            .expect("insert_config_revision select failed");
    row_id
}

/// Insert a `repository_change` sequence row and return its sequence number.
///
/// # Panics
pub async fn insert_sequence(pool: &SqlitePool) -> i64 {
    sqlx::query(
        "INSERT INTO repository_change (command_row_id, created_at)
         VALUES (NULL, '2026-07-22T00:00:00.000000Z')",
    )
    .execute(pool)
    .await
    .expect("insert_sequence failed");

    let (seq,): (i64,) = sqlx::query_as("SELECT MAX(sequence) FROM repository_change")
        .fetch_one(pool)
        .await
        .expect("insert_sequence max failed");
    seq
}

/// Insert a minimal `command_execution` row and return its row_id.
///
/// # Panics
pub async fn insert_command(pool: &SqlitePool, public_id: &str, actor_row_id: i64) -> i64 {
    sqlx::query(
        "INSERT INTO command_execution
             (public_id, actor_row_id, operation, canonical_payload_digest, state,
              created_at)
         VALUES (?, ?, 'test.op', 'payload-digest', 'received',
                 '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .bind(actor_row_id)
    .execute(pool)
    .await
    .expect("insert_command failed");

    let (row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM command_execution WHERE public_id = ?")
            .bind(public_id)
            .fetch_one(pool)
            .await
            .expect("insert_command select failed");
    row_id
}

/// Insert a `session_materialization_operation` row and return its row_id.
///
/// # Panics
pub async fn insert_materialization_operation(
    pool: &SqlitePool,
    public_id: &str,
    command_row_id: i64,
    config_revision_row_id: i64,
    created_sequence: i64,
) -> i64 {
    sqlx::query(
        "INSERT INTO session_materialization_operation
             (public_id, kind, command_row_id, config_revision_row_id, state,
              created_sequence, created_at)
         VALUES (?, 'inbox_ingestion', ?, ?, 'ready', ?, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .bind(command_row_id)
    .bind(config_revision_row_id)
    .bind(created_sequence)
    .execute(pool)
    .await
    .expect("insert_materialization_operation failed");

    let (row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM session_materialization_operation WHERE public_id = ?")
            .bind(public_id)
            .fetch_one(pool)
            .await
            .expect("insert_materialization_operation select failed");
    row_id
}

/// Insert a minimal `spec062_target` row and return its row_id.
///
/// # Panics
pub async fn insert_spec062_target(pool: &SqlitePool, public_id: &str) -> i64 {
    sqlx::query(
        "INSERT OR IGNORE INTO spec062_target (public_id, created_at)
         VALUES (?, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .execute(pool)
    .await
    .expect("insert_spec062_target failed");

    let (row_id,): (i64,) = sqlx::query_as("SELECT row_id FROM spec062_target WHERE public_id = ?")
        .bind(public_id)
        .fetch_one(pool)
        .await
        .expect("insert_spec062_target select failed");
    row_id
}

/// Insert a minimal `spec062_file_identity` row and return its row_id.
///
/// # Panics
pub async fn insert_file_identity(pool: &SqlitePool, public_id: &str) -> i64 {
    sqlx::query(
        "INSERT OR IGNORE INTO spec062_file_identity (public_id, created_at)
         VALUES (?, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .execute(pool)
    .await
    .expect("insert_file_identity failed");

    let (row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM spec062_file_identity WHERE public_id = ?")
            .bind(public_id)
            .fetch_one(pool)
            .await
            .expect("insert_file_identity select failed");
    row_id
}

/// Insert a `frame_record` row and return its row_id.
///
/// # Panics
pub async fn insert_frame_record(
    pool: &SqlitePool,
    public_id: &str,
    file_row_id: i64,
    created_sequence: i64,
) -> i64 {
    sqlx::query(
        "INSERT INTO frame_record
             (public_id, file_row_id, byte_size, captured_metadata_digest,
              created_sequence, created_at)
         VALUES (?, ?, 4096, 'frame-meta-digest', ?, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .bind(file_row_id)
    .bind(created_sequence)
    .execute(pool)
    .await
    .expect("insert_frame_record failed");

    let (row_id,): (i64,) = sqlx::query_as("SELECT row_id FROM frame_record WHERE public_id = ?")
        .bind(public_id)
        .fetch_one(pool)
        .await
        .expect("insert_frame_record select failed");
    row_id
}

/// Insert a light `session` row with a singleton `session_frame` membership.
///
/// Returns `(session_row_id, frame_row_id)`.
///
/// This is the minimum footprint that passes commit-time invariant 1 (at least
/// one frame and exactly one representative).
///
/// # Panics
pub async fn insert_light_session(
    pool: &SqlitePool,
    session_public_id: &str,
    frame_public_id: &str,
    materialization_op_row_id: i64,
    canonical_target_row_id: i64,
    created_sequence: i64,
    ordinal_in_operation: i64,
) -> (i64, i64) {
    let identity_digest = format!("session-identity-{session_public_id}");

    sqlx::query(
        "INSERT INTO session
             (public_id, materialization_operation_row_id, kind, ordinal_in_operation,
              identity_digest, observing_night_date, night_derivation,
              canonical_target_row_id, created_sequence, created_at)
         VALUES (?, ?, 'light', ?, ?, '2026-07-21', 'reviewed_local_fallback',
                 ?, ?, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(session_public_id)
    .bind(materialization_op_row_id)
    .bind(ordinal_in_operation)
    .bind(&identity_digest)
    .bind(canonical_target_row_id)
    .bind(created_sequence)
    .execute(pool)
    .await
    .expect("insert_light_session failed");

    let (session_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM session WHERE public_id = ?")
            .bind(session_public_id)
            .fetch_one(pool)
            .await
            .expect("insert_light_session select failed");

    // Insert frame file identity and frame record.
    let file_row_id = insert_file_identity(pool, &format!("file-{frame_public_id}")).await;
    let frame_row_id =
        insert_frame_record(pool, frame_public_id, file_row_id, created_sequence).await;

    // Insert session_frame with is_representative = 1.
    sqlx::query(
        "INSERT INTO session_frame
             (session_row_id, frame_row_id, materialization_operation_row_id,
              ordinal, is_representative, created_sequence)
         VALUES (?, ?, ?, 0, 1, ?)",
    )
    .bind(session_row_id)
    .bind(frame_row_id)
    .bind(materialization_op_row_id)
    .bind(created_sequence)
    .execute(pool)
    .await
    .expect("insert session_frame failed");

    (session_row_id, frame_row_id)
}

/// Insert a minimal pending `relation_proposal` row and return its row_id.
///
/// # Panics
pub async fn insert_pending_proposal(
    pool: &SqlitePool,
    public_id: &str,
    kind: &str,
    config_revision_row_id: i64,
    created_sequence: i64,
) -> i64 {
    let basis = format!("basis-{public_id}");
    let evidence = format!("evidence-{public_id}");

    sqlx::query(
        "INSERT INTO relation_proposal
             (public_id, proposal_revision, kind, basis_digest, evidence_digest,
              config_revision_row_id, state, created_sequence, created_at)
         VALUES (?, 1, ?, ?, ?, ?, 'pending', ?, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .bind(kind)
    .bind(&basis)
    .bind(&evidence)
    .bind(config_revision_row_id)
    .bind(created_sequence)
    .execute(pool)
    .await
    .expect("insert_pending_proposal failed");

    sqlx::query(
        "INSERT INTO relation_proposal_visibility_history
             (proposal_row_id, proposal_revision, state, visible_sequence)
         SELECT row_id, 1, 'pending', ? FROM relation_proposal WHERE public_id = ?",
    )
    .bind(created_sequence)
    .bind(public_id)
    .execute(pool)
    .await
    .expect("insert_proposal_visibility_history failed");

    let (row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM relation_proposal WHERE public_id = ?")
            .bind(public_id)
            .fetch_one(pool)
            .await
            .expect("insert_pending_proposal select failed");
    row_id
}

/// Insert a minimal `reclassification_plan_revision` in `applied` state for tests.
///
/// Inserts stub `session_equipment_resolution` and `session_metadata_resolution`
/// rows to satisfy NOT NULL FKs. The `source_session_row_id` is the predecessor
/// session. Returns `(row_id, public_id)`.
///
/// # Panics
pub async fn insert_applied_reclassification_plan_revision(
    pool: &SqlitePool,
    source_session_row_id: i64,
    actor_row_id: i64,
    config_revision_row_id: i64,
    command_row_id: i64,
    created_sequence: i64,
) -> (i64, String) {
    let plan_pub_id = format!("reclass-plan-{source_session_row_id}-{created_sequence}");
    let plan_rev_pub_id = format!("reclass-plan-rev-{source_session_row_id}-{created_sequence}");
    let ts = "2026-07-22T00:00:00.000000Z";

    // Stub equipment_resolution row.
    let equip_pub_id = format!("equip-res-{source_session_row_id}-{created_sequence}");
    sqlx::query(
        "INSERT OR IGNORE INTO session_equipment_resolution
             (public_id, session_row_id, revision_number, comparison_severity,
              assignment_mode, config_revision_row_id, actor_row_id,
              created_sequence, created_at)
         VALUES (?, ?, 1, 'unknown', 'automatic', ?, ?, ?, ?)",
    )
    .bind(&equip_pub_id)
    .bind(source_session_row_id)
    .bind(config_revision_row_id)
    .bind(actor_row_id)
    .bind(created_sequence)
    .bind(ts)
    .execute(pool)
    .await
    .expect("insert_stub_equipment_resolution failed");

    let (equip_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM session_equipment_resolution WHERE public_id = ?")
            .bind(&equip_pub_id)
            .fetch_one(pool)
            .await
            .expect("select stub equipment_resolution failed");

    // Stub metadata_resolution row.
    let meta_pub_id = format!("meta-res-{source_session_row_id}-{created_sequence}");
    sqlx::query(
        "INSERT OR IGNORE INTO session_metadata_resolution
             (public_id, session_row_id, revision_number, state,
              actor_row_id, command_row_id, created_sequence, created_at)
         VALUES (?, ?, 1, 'accepted', ?, ?, ?, ?)",
    )
    .bind(&meta_pub_id)
    .bind(source_session_row_id)
    .bind(actor_row_id)
    .bind(command_row_id)
    .bind(created_sequence)
    .bind(ts)
    .execute(pool)
    .await
    .expect("insert_stub_metadata_resolution failed");

    let (meta_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM session_metadata_resolution WHERE public_id = ?")
            .bind(&meta_pub_id)
            .fetch_one(pool)
            .await
            .expect("select stub metadata_resolution failed");

    // reclassification_plan stable row.
    sqlx::query(
        "INSERT OR IGNORE INTO reclassification_plan (public_id, head_generation, created_at)
         VALUES (?, 0, ?)",
    )
    .bind(&plan_pub_id)
    .bind(ts)
    .execute(pool)
    .await
    .expect("insert_reclassification_plan failed");

    let (plan_row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM reclassification_plan WHERE public_id = ?")
            .bind(&plan_pub_id)
            .fetch_one(pool)
            .await
            .expect("select reclassification_plan failed");

    // reclassification_plan_revision row.
    let basis = format!("basis-{source_session_row_id}");
    sqlx::query(
        "INSERT OR IGNORE INTO reclassification_plan_revision
             (public_id, plan_row_id, revision_number, state,
              source_session_row_id, metadata_resolution_row_id,
              equipment_resolution_row_id, basis_digest,
              actor_row_id, command_row_id, created_sequence, created_at, decided_at)
         VALUES (?, ?, 1, 'applied', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&plan_rev_pub_id)
    .bind(plan_row_id)
    .bind(source_session_row_id)
    .bind(meta_row_id)
    .bind(equip_row_id)
    .bind(&basis)
    .bind(actor_row_id)
    .bind(command_row_id)
    .bind(created_sequence)
    .bind(ts)
    .bind(ts)
    .execute(pool)
    .await
    .expect("insert_reclassification_plan_revision failed");

    let (row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM reclassification_plan_revision WHERE public_id = ?")
            .bind(&plan_rev_pub_id)
            .fetch_one(pool)
            .await
            .expect("select reclassification_plan_revision failed");
    (row_id, plan_rev_pub_id)
}

/// Insert a minimal `spec062_project` row and return its `row_id`.
///
/// The project starts with no membership head (NULL).
///
/// # Panics
pub async fn insert_spec062_project(pool: &SqlitePool, public_id: &str) -> i64 {
    sqlx::query(
        "INSERT OR IGNORE INTO spec062_project
             (public_id, membership_head_generation, materialization_head_generation,
              current_manifest_generation, created_at)
         VALUES (?, 0, 0, 0, '2026-07-22T00:00:00.000000Z')",
    )
    .bind(public_id)
    .execute(pool)
    .await
    .expect("insert_spec062_project failed");

    let (row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM spec062_project WHERE public_id = ?")
            .bind(public_id)
            .fetch_one(pool)
            .await
            .expect("insert_spec062_project select failed");
    row_id
}
