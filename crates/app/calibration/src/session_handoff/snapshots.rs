// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Snapshot creation and lease-generation fencing for external-handoff.
//!
//! This module owns the CAS logic for advancing the handoff head from one
//! snapshot to its successor. The fencing token is `lease_generation` on
//! the operation row; the head-advance CAS uses `head_generation` on the
//! handoff aggregate.
//!
//! Verification (frame identity checks, byte hashing) runs outside this
//! module and outside the writer transaction. This module only handles the
//! final atomic commit path.

use persistence_core::DbResult;
use persistence_sessions::repositories::calibration_sessions::handoff::{
    advance_handoff_head, insert_handoff, insert_handoff_operation, insert_handoff_snapshot,
    insert_snapshot_requirement_mapping, insert_snapshot_selection_mapping, InsertHandoff,
    InsertHandoffOperation, InsertHandoffSnapshot,
};
use sqlx::SqliteConnection;

// ── Lease generation ──────────────────────────────────────────────────────────

/// Fencing parameters for one handoff operation.
///
/// The `lease_generation` is incremented each time a resume claims the
/// operation (FR-099). The snapshot's `basis_fingerprint` must match the
/// source snapshot to detect stale predecessor chains.
#[derive(Debug, Clone)]
pub struct LeaseFencing {
    pub operation_public_id: String,
    pub operation_row_id: i64,
    pub lease_generation: i64,
}

// ── Initial creation ─────────────────────────────────────────────────────────

/// Parameters for creating the initial handoff aggregate + first snapshot.
pub struct CreateHandoffParams<'a> {
    pub handoff_public_id: &'a str,
    pub snapshot_public_id: &'a str,
    pub project_row_id: i64,
    pub external_processor: &'a str,
    pub evaluation_at: &'a str,
    pub matching_settings_revision_row_id: i64,
    pub basis_digest: &'a str,
    pub requirement_count: i64,
    pub selection_count: i64,
    pub frame_count: i64,
    pub source_byte_count: i64,
    pub actor_row_id: i64,
    pub command_row_id: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
    /// Ordered `(requirement_row_id, snapshot_ordinal)` pairs to link.
    pub requirement_ordinals: &'a [(i64, i64)],
    /// Ordered `(selection_row_id, snapshot_ordinal)` pairs to link.
    pub selection_ordinals: &'a [(i64, i64)],
}

/// Insert the initial handoff aggregate, first snapshot, and advance the head.
///
/// All three operations occur inside the caller's `BEGIN IMMEDIATE`
/// transaction. The head CAS uses `expected_generation = 0` (the initial
/// value after INSERT).
///
/// Returns `(handoff_row_id, snapshot_row_id)`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
/// Returns `persistence_core::DbError::CasFailed` if the head advance fails (cannot occur
/// on initial creation, but included for type consistency).
pub async fn create_initial_handoff(
    conn: &mut SqliteConnection,
    params: &CreateHandoffParams<'_>,
) -> DbResult<(i64, i64)> {
    let handoff_row_id = insert_handoff(
        conn,
        &InsertHandoff {
            public_id: params.handoff_public_id,
            project_row_id: params.project_row_id,
            external_processor: params.external_processor,
            created_at: params.created_at,
        },
    )
    .await?;

    let snapshot_row_id = insert_handoff_snapshot(
        conn,
        &InsertHandoffSnapshot {
            public_id: params.snapshot_public_id,
            handoff_row_id,
            predecessor_snapshot_row_id: None,
            evaluation_at: params.evaluation_at,
            matching_settings_revision_row_id: params.matching_settings_revision_row_id,
            basis_digest: params.basis_digest,
            requirement_count: params.requirement_count,
            selection_count: params.selection_count,
            frame_count: params.frame_count,
            source_byte_count: params.source_byte_count,
            actor_row_id: params.actor_row_id,
            command_row_id: params.command_row_id,
            created_sequence: params.created_sequence,
            created_at: params.created_at,
        },
    )
    .await?;

    for (req_row_id, ordinal) in params.requirement_ordinals {
        insert_snapshot_requirement_mapping(
            conn,
            snapshot_row_id,
            *req_row_id,
            handoff_row_id,
            *ordinal,
        )
        .await?;
    }
    for (sel_row_id, ordinal) in params.selection_ordinals {
        insert_snapshot_selection_mapping(
            conn,
            snapshot_row_id,
            *sel_row_id,
            handoff_row_id,
            *ordinal,
        )
        .await?;
    }

    // CAS head advance: generation starts at 0 after INSERT.
    advance_handoff_head(conn, handoff_row_id, snapshot_row_id, 0).await?;

    Ok((handoff_row_id, snapshot_row_id))
}

// ── Successor snapshot (reviewed addition) ────────────────────────────────────

/// Parameters for creating a successor snapshot after a reviewed selection.
pub struct AddReviewedSelectionParams<'a> {
    pub successor_snapshot_public_id: &'a str,
    pub handoff_row_id: i64,
    pub predecessor_snapshot_row_id: i64,
    /// Expected `head_generation` — must match for the CAS to succeed.
    pub expected_head_generation: i64,
    pub evaluation_at: &'a str,
    pub matching_settings_revision_row_id: i64,
    pub basis_digest: &'a str,
    pub requirement_count: i64,
    /// Total selection count including the new reviewed selection.
    pub selection_count: i64,
    pub frame_count: i64,
    pub source_byte_count: i64,
    pub actor_row_id: i64,
    pub command_row_id: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
    /// All requirements from the predecessor snapshot (re-mapped to successor).
    pub requirement_ordinals: &'a [(i64, i64)],
    /// All selections from the predecessor snapshot plus the new one.
    pub selection_ordinals: &'a [(i64, i64)],
}

/// Insert a successor snapshot and advance the handoff head by CAS.
///
/// Must run inside a `BEGIN IMMEDIATE` transaction that has already
/// re-validated the source snapshot and head generation.
///
/// Returns the successor `snapshot_row_id`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` when `expected_head_generation` does not
/// match the current head generation.
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn add_reviewed_selection_snapshot(
    conn: &mut SqliteConnection,
    params: &AddReviewedSelectionParams<'_>,
) -> DbResult<i64> {
    let snapshot_row_id = insert_handoff_snapshot(
        conn,
        &InsertHandoffSnapshot {
            public_id: params.successor_snapshot_public_id,
            handoff_row_id: params.handoff_row_id,
            predecessor_snapshot_row_id: Some(params.predecessor_snapshot_row_id),
            evaluation_at: params.evaluation_at,
            matching_settings_revision_row_id: params.matching_settings_revision_row_id,
            basis_digest: params.basis_digest,
            requirement_count: params.requirement_count,
            selection_count: params.selection_count,
            frame_count: params.frame_count,
            source_byte_count: params.source_byte_count,
            actor_row_id: params.actor_row_id,
            command_row_id: params.command_row_id,
            created_sequence: params.created_sequence,
            created_at: params.created_at,
        },
    )
    .await?;

    for (req_row_id, ordinal) in params.requirement_ordinals {
        insert_snapshot_requirement_mapping(
            conn,
            snapshot_row_id,
            *req_row_id,
            params.handoff_row_id,
            *ordinal,
        )
        .await?;
    }
    for (sel_row_id, ordinal) in params.selection_ordinals {
        insert_snapshot_selection_mapping(
            conn,
            snapshot_row_id,
            *sel_row_id,
            params.handoff_row_id,
            *ordinal,
        )
        .await?;
    }

    advance_handoff_head(
        conn,
        params.handoff_row_id,
        snapshot_row_id,
        params.expected_head_generation,
    )
    .await?;

    Ok(snapshot_row_id)
}

// ── Operation insertion ───────────────────────────────────────────────────────

/// Insert an operation row for a new handoff creation or reviewed-addition.
///
/// Returns the `operation_row_id`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_operation_for_handoff(
    conn: &mut SqliteConnection,
    public_id: &str,
    handoff_row_id: i64,
    command_row_id: i64,
    created_at: &str,
) -> DbResult<i64> {
    insert_handoff_operation(
        conn,
        &InsertHandoffOperation { public_id, handoff_row_id, command_row_id, created_at },
    )
    .await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::DbError;
    use persistence_sessions::repositories::calibration_sessions::handoff::{
        get_handoff_by_public_id, get_snapshot_by_public_id,
    };

    async fn setup_db() -> sqlx::SqlitePool {
        let db = persistence_core::Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db.pool().clone()
    }

    async fn seed_snapshot_prerequisites(pool: &sqlx::SqlitePool) {
        let ts = "2026-07-22T00:00:00.000000Z";
        sqlx::query(
            "INSERT INTO spec062_actor VALUES (1,'00000000-0000-7000-e000-000000000001',?)",
        )
        .bind(ts)
        .execute(pool)
        .await
        .expect("actor");
        sqlx::query("INSERT INTO spec062_config_revision VALUES (1,'00000000-0000-7000-e000-000000000002',1,'cfg-digest',?)").bind(ts).execute(pool).await.expect("config");
        sqlx::query("INSERT INTO repository_change(command_row_id,created_at) VALUES (NULL,?)")
            .bind(ts)
            .execute(pool)
            .await
            .expect("repo_change");
        sqlx::query("INSERT INTO command_execution (row_id,public_id,actor_row_id,operation,canonical_payload_digest,state,response_json,created_at,finished_at) VALUES (1,'00000000-0000-7000-e000-000000000003',1,'calibration.handoff.create','pd','applied','{}',?,?)").bind(ts).bind(ts).execute(pool).await.expect("command");
        sqlx::query(
            "INSERT INTO spec062_project (row_id,public_id,created_at) VALUES (1,'proj-pub-001',?)",
        )
        .bind(ts)
        .execute(pool)
        .await
        .expect("project");
        sqlx::query("INSERT INTO matching_settings_revision (row_id,public_id,revision_number,same_session_coverage_min_ppm,same_session_centre_max_ppm,same_session_rotation_max_udeg,sibling_coverage_min_ppm,sibling_centre_max_ppm,sibling_rotation_max_udeg,mosaic_overlap_min_ppm,mosaic_overlap_max_ppm,dark_thermal_moderate_millic,dark_thermal_severe_millic,flat_orientation_normal_udeg,flat_orientation_red_udeg,flat_red_age_days,canonical_digest,actor_row_id,command_row_id,created_sequence,created_at) VALUES (1,'msr-001',1,950000,20000,1000000,900000,50000,5000000,50000,400000,500,2000,2000000,5000000,7,'msr-digest',1,1,1,?)").bind(ts).execute(pool).await.expect("settings");
        sqlx::query("INSERT INTO matching_settings_head (singleton,head_revision_row_id,head_generation) VALUES (1,1,0)").execute(pool).await.expect("settings head");
        sqlx::query("INSERT INTO repository_change(command_row_id,created_at) VALUES (NULL,?)")
            .bind(ts)
            .execute(pool)
            .await
            .expect("repo_change_2");
    }

    #[tokio::test]
    async fn create_initial_handoff_sets_head() {
        let pool = setup_db().await;
        seed_snapshot_prerequisites(&pool).await;
        let ts = "2026-07-22T00:00:00.000000Z";

        let mut conn = pool.acquire().await.expect("conn");
        let (handoff_row_id, snapshot_row_id) = create_initial_handoff(
            &mut conn,
            &CreateHandoffParams {
                handoff_public_id: "ho-snap-001",
                snapshot_public_id: "hs-snap-001",
                project_row_id: 1,
                external_processor: "pixinsight_wbpp",
                evaluation_at: ts,
                matching_settings_revision_row_id: 1,
                basis_digest: "basis-001",
                requirement_count: 0,
                selection_count: 0,
                frame_count: 0,
                source_byte_count: 0,
                actor_row_id: 1,
                command_row_id: 1,
                created_sequence: 1,
                created_at: ts,
                requirement_ordinals: &[],
                selection_ordinals: &[],
            },
        )
        .await
        .expect("create initial handoff");

        let handoff = get_handoff_by_public_id(&pool, "ho-snap-001").await.expect("get handoff");
        assert_eq!(handoff.row_id, handoff_row_id);
        assert_eq!(handoff.head_snapshot_row_id, Some(snapshot_row_id));
        assert_eq!(handoff.head_generation, 1);
    }

    #[tokio::test]
    async fn successor_snapshot_advances_head() {
        let pool = setup_db().await;
        seed_snapshot_prerequisites(&pool).await;
        let ts = "2026-07-22T00:00:00.000000Z";

        let mut conn = pool.acquire().await.expect("conn");
        let (handoff_row_id, first_snapshot_id) = create_initial_handoff(
            &mut conn,
            &CreateHandoffParams {
                handoff_public_id: "ho-snap-002",
                snapshot_public_id: "hs-snap-002",
                project_row_id: 1,
                external_processor: "siril",
                evaluation_at: ts,
                matching_settings_revision_row_id: 1,
                basis_digest: "basis-002",
                requirement_count: 0,
                selection_count: 0,
                frame_count: 0,
                source_byte_count: 0,
                actor_row_id: 1,
                command_row_id: 1,
                created_sequence: 1,
                created_at: ts,
                requirement_ordinals: &[],
                selection_ordinals: &[],
            },
        )
        .await
        .expect("initial");

        // Need a second command for the reviewed addition
        sqlx::query("INSERT INTO command_execution (row_id,public_id,actor_row_id,operation,canonical_payload_digest,state,response_json,created_at,finished_at) VALUES (2,'00000000-0000-7000-e000-000000000004',1,'calibration.handoff.reviewed_add','pd','applied','{}',?,?)").bind(ts).bind(ts).execute(&pool).await.expect("command2");

        let successor_id = add_reviewed_selection_snapshot(
            &mut conn,
            &AddReviewedSelectionParams {
                successor_snapshot_public_id: "hs-snap-002b",
                handoff_row_id,
                predecessor_snapshot_row_id: first_snapshot_id,
                expected_head_generation: 1,
                evaluation_at: ts,
                matching_settings_revision_row_id: 1,
                basis_digest: "basis-002b",
                requirement_count: 0,
                selection_count: 1,
                frame_count: 0,
                source_byte_count: 0,
                actor_row_id: 1,
                command_row_id: 2,
                created_sequence: 2,
                created_at: ts,
                requirement_ordinals: &[],
                selection_ordinals: &[],
            },
        )
        .await
        .expect("successor");

        let handoff = get_handoff_by_public_id(&pool, "ho-snap-002").await.expect("get handoff");
        assert_eq!(handoff.head_snapshot_row_id, Some(successor_id));
        assert_eq!(handoff.head_generation, 2);

        // Verify predecessor link
        let snap = get_snapshot_by_public_id(&pool, "hs-snap-002b").await.expect("snap");
        assert_eq!(snap.predecessor_snapshot_row_id, Some(first_snapshot_id));
    }

    #[tokio::test]
    async fn successor_fails_with_stale_generation() {
        let pool = setup_db().await;
        seed_snapshot_prerequisites(&pool).await;
        let ts = "2026-07-22T00:00:00.000000Z";

        let mut conn = pool.acquire().await.expect("conn");
        let (handoff_row_id, first_snapshot_id) = create_initial_handoff(
            &mut conn,
            &CreateHandoffParams {
                handoff_public_id: "ho-snap-003",
                snapshot_public_id: "hs-snap-003",
                project_row_id: 1,
                external_processor: "pixinsight_wbpp",
                evaluation_at: ts,
                matching_settings_revision_row_id: 1,
                basis_digest: "basis-003",
                requirement_count: 0,
                selection_count: 0,
                frame_count: 0,
                source_byte_count: 0,
                actor_row_id: 1,
                command_row_id: 1,
                created_sequence: 1,
                created_at: ts,
                requirement_ordinals: &[],
                selection_ordinals: &[],
            },
        )
        .await
        .expect("initial");

        // Attempt successor with wrong generation (0 instead of 1)
        let err = add_reviewed_selection_snapshot(
            &mut conn,
            &AddReviewedSelectionParams {
                successor_snapshot_public_id: "hs-snap-003b",
                handoff_row_id,
                predecessor_snapshot_row_id: first_snapshot_id,
                expected_head_generation: 0, // stale
                evaluation_at: ts,
                matching_settings_revision_row_id: 1,
                basis_digest: "basis-003b",
                requirement_count: 0,
                selection_count: 0,
                frame_count: 0,
                source_byte_count: 0,
                actor_row_id: 1,
                command_row_id: 1,
                created_sequence: 1,
                created_at: ts,
                requirement_ordinals: &[],
                selection_ordinals: &[],
            },
        )
        .await;
        assert!(matches!(err, Err(DbError::CasFailed(_))));
    }
}
