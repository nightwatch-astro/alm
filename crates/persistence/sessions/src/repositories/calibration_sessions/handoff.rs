// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for the calibration external-handoff aggregate and its child tables:
//! `calibration_handoff`, `calibration_handoff_snapshot`,
//! `calibration_handoff_requirement`, `calibration_handoff_snapshot_requirement`,
//! `calibration_handoff_candidate_evidence`, `calibration_handoff_candidate_warning`,
//! `calibration_handoff_selection`, `calibration_handoff_snapshot_selection`,
//! `calibration_handoff_review`, `calibration_handoff_review_warning`,
//! `calibration_handoff_frame`, and `calibration_handoff_operation`.
//!
//! CAS semantics: the handoff head uses `(head_snapshot_row_id,
//! head_generation)`. Snapshot succession is the only mutation that advances
//! `head_generation`. All other rows are immutable once inserted.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

// ── Row projections ────────────────────────────────────────────────────────────

/// `calibration_handoff` aggregate row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HandoffRow {
    pub row_id: i64,
    pub public_id: String,
    pub project_row_id: i64,
    pub external_processor: String,
    pub head_snapshot_row_id: Option<i64>,
    pub head_generation: i64,
    pub created_at: String,
}

/// `calibration_handoff_snapshot` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HandoffSnapshotRow {
    pub row_id: i64,
    pub public_id: String,
    pub handoff_row_id: i64,
    pub predecessor_snapshot_row_id: Option<i64>,
    pub evaluation_at: String,
    pub matching_settings_revision_row_id: i64,
    pub basis_digest: String,
    pub requirement_count: i64,
    pub selection_count: i64,
    pub frame_count: i64,
    pub source_byte_count: i64,
    pub actor_row_id: i64,
    pub command_row_id: i64,
    pub created_sequence: i64,
    pub created_at: String,
}

/// `calibration_handoff_requirement` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HandoffRequirementRow {
    pub row_id: i64,
    pub public_id: String,
    pub handoff_row_id: i64,
    pub kind: String,
    pub camera_row_id: Option<i64>,
    pub family_row_id: Option<i64>,
    pub recipe_revision: i64,
    pub evidence_digest: String,
    pub required_field_state: String,
}

/// `calibration_handoff_candidate_evidence` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HandoffCandidateEvidenceRow {
    pub row_id: i64,
    pub public_id: String,
    pub handoff_row_id: i64,
    pub snapshot_row_id: i64,
    pub requirement_row_id: i64,
    pub session_row_id: i64,
    pub recipe_compatible: i64,
    pub recipe_complete: i64,
    pub age_days: i64,
    pub age_severity: String,
    pub thermal_state: String,
    pub available_frame_count: i64,
    pub readable_frame_count: i64,
    pub automatic_eligible: i64,
    pub evidence_digest: String,
    pub observed_at: String,
}

/// `calibration_handoff_selection` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HandoffSelectionRow {
    pub row_id: i64,
    pub public_id: String,
    pub handoff_row_id: i64,
    pub requirement_row_id: i64,
    pub session_row_id: i64,
    pub candidate_evidence_row_id: i64,
    pub source: String,
    pub selected_at: String,
    pub created_sequence: i64,
}

/// `calibration_handoff_operation` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HandoffOperationRow {
    pub row_id: i64,
    pub public_id: String,
    pub handoff_row_id: i64,
    pub command_row_id: i64,
    pub state: String,
    pub state_version: i64,
    pub lease_owner: Option<String>,
    pub lease_generation: i64,
    pub frame_progress: i64,
    pub byte_progress: i64,
    pub terminal_snapshot_row_id: Option<i64>,
    pub created_at: String,
}

/// `calibration_handoff_frame` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HandoffFrameRow {
    pub selection_row_id: i64,
    pub frame_row_id: i64,
    pub session_membership_ordinal: i64,
    pub file_row_id: i64,
    pub source_root_row_id: i64,
    pub canonical_relative_path: String,
    pub stable_file_identity: String,
    pub byte_size: i64,
    pub sha256_fingerprint: String,
    pub no_follow_verified: i64,
    pub verified_at: String,
}

// ── Insert parameters ─────────────────────────────────────────────────────────

/// Parameters for inserting a `calibration_handoff` aggregate row.
pub struct InsertHandoff<'a> {
    pub public_id: &'a str,
    pub project_row_id: i64,
    pub external_processor: &'a str,
    pub created_at: &'a str,
}

/// Parameters for inserting one `calibration_handoff_snapshot` row.
pub struct InsertHandoffSnapshot<'a> {
    pub public_id: &'a str,
    pub handoff_row_id: i64,
    pub predecessor_snapshot_row_id: Option<i64>,
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
}

/// Parameters for inserting a `calibration_handoff_requirement` row.
pub struct InsertHandoffRequirement<'a> {
    pub public_id: &'a str,
    pub handoff_row_id: i64,
    pub kind: &'a str,
    pub camera_row_id: Option<i64>,
    pub family_row_id: Option<i64>,
    pub recipe_revision: i64,
    pub evidence_digest: &'a str,
    pub required_field_state: &'a str,
}

/// Parameters for inserting a `calibration_handoff_candidate_evidence` row.
pub struct InsertHandoffCandidateEvidence<'a> {
    pub public_id: &'a str,
    pub handoff_row_id: i64,
    pub snapshot_row_id: i64,
    pub requirement_row_id: i64,
    pub session_row_id: i64,
    pub recipe_compatible: bool,
    pub recipe_complete: bool,
    pub age_days: i64,
    pub age_severity: &'a str,
    pub thermal_state: &'a str,
    pub available_frame_count: i64,
    pub readable_frame_count: i64,
    pub automatic_eligible: bool,
    pub evidence_digest: &'a str,
    pub observed_at: &'a str,
    pub warning_codes: &'a [&'a str],
}

/// Parameters for inserting a `calibration_handoff_selection` row.
pub struct InsertHandoffSelection<'a> {
    pub public_id: &'a str,
    pub handoff_row_id: i64,
    pub requirement_row_id: i64,
    pub session_row_id: i64,
    pub candidate_evidence_row_id: i64,
    pub source: &'a str,
    pub selected_at: &'a str,
    pub created_sequence: i64,
}

/// Parameters for inserting a `calibration_handoff_frame` row.
pub struct InsertHandoffFrame<'a> {
    pub selection_row_id: i64,
    pub frame_row_id: i64,
    pub session_membership_ordinal: i64,
    pub file_row_id: i64,
    pub source_root_row_id: i64,
    pub canonical_relative_path: &'a str,
    pub stable_file_identity: &'a str,
    pub byte_size: i64,
    pub sha256_fingerprint: &'a str,
    pub verified_at: &'a str,
}

/// Parameters for inserting a `calibration_handoff_operation` row.
pub struct InsertHandoffOperation<'a> {
    pub public_id: &'a str,
    pub handoff_row_id: i64,
    pub command_row_id: i64,
    pub created_at: &'a str,
}

// ── Writes ────────────────────────────────────────────────────────────────────

/// Insert the `calibration_handoff` aggregate row.
///
/// The head snapshot is initially null. Callers set it atomically with the
/// first snapshot insert via [`advance_handoff_head`].
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_handoff(
    conn: &mut SqliteConnection,
    params: &InsertHandoff<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO calibration_handoff
             (public_id, project_row_id, external_processor,
              head_snapshot_row_id, head_generation, created_at)
         VALUES (?,?,?,NULL,0,?)",
    )
    .bind(params.public_id)
    .bind(params.project_row_id)
    .bind(params.external_processor)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert one `calibration_handoff_snapshot` row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_handoff_snapshot(
    conn: &mut SqliteConnection,
    params: &InsertHandoffSnapshot<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO calibration_handoff_snapshot (
            public_id, handoff_row_id, predecessor_snapshot_row_id,
            evaluation_at, matching_settings_revision_row_id,
            basis_digest, requirement_count, selection_count,
            frame_count, source_byte_count,
            actor_row_id, command_row_id,
            created_sequence, created_at
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.handoff_row_id)
    .bind(params.predecessor_snapshot_row_id)
    .bind(params.evaluation_at)
    .bind(params.matching_settings_revision_row_id)
    .bind(params.basis_digest)
    .bind(params.requirement_count)
    .bind(params.selection_count)
    .bind(params.frame_count)
    .bind(params.source_byte_count)
    .bind(params.actor_row_id)
    .bind(params.command_row_id)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Advance the `calibration_handoff` head by CAS on `head_generation`.
///
/// Must be called inside a `BEGIN IMMEDIATE` transaction after the successor
/// snapshot row is inserted. The CAS prevents concurrent reviewed additions
/// from creating an inconsistent head chain.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` when `expected_generation` does not match.
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn advance_handoff_head(
    conn: &mut SqliteConnection,
    handoff_row_id: i64,
    new_snapshot_row_id: i64,
    expected_generation: i64,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE calibration_handoff
         SET head_snapshot_row_id = ?,
             head_generation = head_generation + 1
         WHERE row_id = ?
           AND head_generation = ?",
    )
    .bind(new_snapshot_row_id)
    .bind(handoff_row_id)
    .bind(expected_generation)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "handoff {handoff_row_id} head CAS failed (expected_generation={expected_generation})"
        )));
    }
    Ok(())
}

/// Insert one `calibration_handoff_requirement` row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_handoff_requirement(
    conn: &mut SqliteConnection,
    params: &InsertHandoffRequirement<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO calibration_handoff_requirement
             (public_id, handoff_row_id, kind, camera_row_id, family_row_id,
              recipe_revision, evidence_digest, required_field_state)
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.handoff_row_id)
    .bind(params.kind)
    .bind(params.camera_row_id)
    .bind(params.family_row_id)
    .bind(params.recipe_revision)
    .bind(params.evidence_digest)
    .bind(params.required_field_state)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert the snapshot–requirement mapping row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_snapshot_requirement_mapping(
    conn: &mut SqliteConnection,
    snapshot_row_id: i64,
    requirement_row_id: i64,
    handoff_row_id: i64,
    ordinal: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO calibration_handoff_snapshot_requirement
             (snapshot_row_id, requirement_row_id, handoff_row_id, ordinal)
         VALUES (?,?,?,?)",
    )
    .bind(snapshot_row_id)
    .bind(requirement_row_id)
    .bind(handoff_row_id)
    .bind(ordinal)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert one `calibration_handoff_candidate_evidence` row plus its ordered
/// `calibration_handoff_candidate_warning` children.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_handoff_candidate_evidence(
    conn: &mut SqliteConnection,
    params: &InsertHandoffCandidateEvidence<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO calibration_handoff_candidate_evidence (
            public_id, handoff_row_id, snapshot_row_id,
            requirement_row_id, session_row_id,
            recipe_compatible, recipe_complete,
            age_days, age_severity, thermal_state,
            available_frame_count, readable_frame_count,
            automatic_eligible, evidence_digest, observed_at
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.handoff_row_id)
    .bind(params.snapshot_row_id)
    .bind(params.requirement_row_id)
    .bind(params.session_row_id)
    .bind(i64::from(params.recipe_compatible))
    .bind(i64::from(params.recipe_complete))
    .bind(params.age_days)
    .bind(params.age_severity)
    .bind(params.thermal_state)
    .bind(params.available_frame_count)
    .bind(params.readable_frame_count)
    .bind(i64::from(params.automatic_eligible))
    .bind(params.evidence_digest)
    .bind(params.observed_at)
    .execute(&mut *conn)
    .await?;
    let evidence_row_id = result.last_insert_rowid();

    for (ordinal, code) in params.warning_codes.iter().enumerate() {
        sqlx::query(
            "INSERT INTO calibration_handoff_candidate_warning
                 (candidate_evidence_row_id, warning_code, ordinal)
             VALUES (?,?,?)",
        )
        .bind(evidence_row_id)
        .bind(*code)
        .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
        .execute(&mut *conn)
        .await?;
    }

    Ok(evidence_row_id)
}

/// Insert one `calibration_handoff_selection` row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_handoff_selection(
    conn: &mut SqliteConnection,
    params: &InsertHandoffSelection<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO calibration_handoff_selection
             (public_id, handoff_row_id, requirement_row_id, session_row_id,
              candidate_evidence_row_id, source, selected_at, created_sequence)
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.handoff_row_id)
    .bind(params.requirement_row_id)
    .bind(params.session_row_id)
    .bind(params.candidate_evidence_row_id)
    .bind(params.source)
    .bind(params.selected_at)
    .bind(params.created_sequence)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert the snapshot–selection mapping row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_snapshot_selection_mapping(
    conn: &mut SqliteConnection,
    snapshot_row_id: i64,
    selection_row_id: i64,
    handoff_row_id: i64,
    ordinal: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO calibration_handoff_snapshot_selection
             (snapshot_row_id, selection_row_id, handoff_row_id, ordinal)
         VALUES (?,?,?,?)",
    )
    .bind(snapshot_row_id)
    .bind(selection_row_id)
    .bind(handoff_row_id)
    .bind(ordinal)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert one `calibration_handoff_frame` row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_handoff_frame(
    conn: &mut SqliteConnection,
    params: &InsertHandoffFrame<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO calibration_handoff_frame (
            selection_row_id, frame_row_id, session_membership_ordinal,
            file_row_id, source_root_row_id,
            canonical_relative_path, stable_file_identity,
            byte_size, sha256_fingerprint,
            no_follow_verified, verified_at
         ) VALUES (?,?,?,?,?,?,?,?,?,1,?)",
    )
    .bind(params.selection_row_id)
    .bind(params.frame_row_id)
    .bind(params.session_membership_ordinal)
    .bind(params.file_row_id)
    .bind(params.source_root_row_id)
    .bind(params.canonical_relative_path)
    .bind(params.stable_file_identity)
    .bind(params.byte_size)
    .bind(params.sha256_fingerprint)
    .bind(params.verified_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert a `calibration_handoff_operation` row in `ready` state.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_handoff_operation(
    conn: &mut SqliteConnection,
    params: &InsertHandoffOperation<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO calibration_handoff_operation
             (public_id, handoff_row_id, command_row_id, state,
              state_version, lease_generation,
              frame_progress, byte_progress, created_at)
         VALUES (?,?,?,'ready',0,0,0,0,?)",
    )
    .bind(params.public_id)
    .bind(params.handoff_row_id)
    .bind(params.command_row_id)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Transition a `calibration_handoff_operation` state by CAS on
/// `state_version`.
///
/// `new_state` must be one of `verifying`, `cancelling`, `cancelled`,
/// `applied`, or `failed`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` when `expected_state_version` does not match.
pub async fn transition_handoff_operation_state(
    conn: &mut SqliteConnection,
    operation_row_id: i64,
    from_state: &str,
    to_state: &str,
    expected_state_version: i64,
    terminal_snapshot_row_id: Option<i64>,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE calibration_handoff_operation
         SET state = ?,
             state_version = state_version + 1,
             terminal_snapshot_row_id = COALESCE(?, terminal_snapshot_row_id)
         WHERE row_id = ?
           AND state = ?
           AND state_version = ?",
    )
    .bind(to_state)
    .bind(terminal_snapshot_row_id)
    .bind(operation_row_id)
    .bind(from_state)
    .bind(expected_state_version)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "handoff operation {operation_row_id} state CAS failed \
             ({from_state}→{to_state}, expected_version={expected_state_version})"
        )));
    }
    Ok(())
}

/// Update frame/byte progress counters on an operation.
///
/// Non-CAS: progress is coalesced and race-safe by monotone intent
/// (callers only write higher values).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn update_handoff_operation_progress(
    conn: &mut SqliteConnection,
    operation_row_id: i64,
    frame_progress: i64,
    byte_progress: i64,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE calibration_handoff_operation
         SET frame_progress = ?, byte_progress = ?
         WHERE row_id = ?",
    )
    .bind(frame_progress)
    .bind(byte_progress)
    .bind(operation_row_id)
    .execute(conn)
    .await?;
    Ok(())
}

// ── Reads ─────────────────────────────────────────────────────────────────────

/// Fetch a `calibration_handoff` by `public_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the handoff does not exist.
pub async fn get_handoff_by_public_id(pool: &SqlitePool, public_id: &str) -> DbResult<HandoffRow> {
    sqlx::query_as::<_, HandoffRow>(
        "SELECT row_id, public_id, project_row_id, external_processor,
                head_snapshot_row_id, head_generation, created_at
         FROM calibration_handoff
         WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("calibration_handoff {public_id}")))
}

/// Fetch a `calibration_handoff_snapshot` by `public_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the snapshot does not exist.
pub async fn get_snapshot_by_public_id(
    pool: &SqlitePool,
    public_id: &str,
) -> DbResult<HandoffSnapshotRow> {
    sqlx::query_as::<_, HandoffSnapshotRow>(
        "SELECT row_id, public_id, handoff_row_id, predecessor_snapshot_row_id,
                evaluation_at, matching_settings_revision_row_id,
                basis_digest, requirement_count, selection_count,
                frame_count, source_byte_count,
                actor_row_id, command_row_id, created_sequence, created_at
         FROM calibration_handoff_snapshot
         WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("calibration_handoff_snapshot {public_id}")))
}

/// Fetch a `calibration_handoff_operation` by `public_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the operation does not exist.
pub async fn get_operation_by_public_id(
    pool: &SqlitePool,
    public_id: &str,
) -> DbResult<HandoffOperationRow> {
    sqlx::query_as::<_, HandoffOperationRow>(
        "SELECT row_id, public_id, handoff_row_id, command_row_id,
                state, state_version, lease_owner, lease_generation,
                frame_progress, byte_progress,
                terminal_snapshot_row_id, created_at
         FROM calibration_handoff_operation
         WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("calibration_handoff_operation {public_id}")))
}

/// List requirements belonging to a snapshot in ordinal order.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_snapshot_requirements(
    pool: &SqlitePool,
    snapshot_row_id: i64,
) -> DbResult<Vec<HandoffRequirementRow>> {
    sqlx::query_as::<_, HandoffRequirementRow>(
        "SELECT r.row_id, r.public_id, r.handoff_row_id, r.kind,
                r.camera_row_id, r.family_row_id,
                r.recipe_revision, r.evidence_digest, r.required_field_state
         FROM calibration_handoff_requirement r
         INNER JOIN calibration_handoff_snapshot_requirement sr
             ON sr.requirement_row_id = r.row_id
            AND sr.snapshot_row_id = ?
         ORDER BY sr.ordinal ASC",
    )
    .bind(snapshot_row_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::from)
}

/// List selections belonging to a snapshot in ordinal order.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_snapshot_selections(
    pool: &SqlitePool,
    snapshot_row_id: i64,
) -> DbResult<Vec<HandoffSelectionRow>> {
    sqlx::query_as::<_, HandoffSelectionRow>(
        "SELECT sel.row_id, sel.public_id, sel.handoff_row_id,
                sel.requirement_row_id, sel.session_row_id,
                sel.candidate_evidence_row_id, sel.source,
                sel.selected_at, sel.created_sequence
         FROM calibration_handoff_selection sel
         INNER JOIN calibration_handoff_snapshot_selection ss
             ON ss.selection_row_id = sel.row_id
            AND ss.snapshot_row_id = ?
         ORDER BY ss.ordinal ASC",
    )
    .bind(snapshot_row_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::from)
}

/// List frames for a selection in session-membership-ordinal order.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_selection_frames(
    pool: &SqlitePool,
    selection_row_id: i64,
) -> DbResult<Vec<HandoffFrameRow>> {
    sqlx::query_as::<_, HandoffFrameRow>(
        "SELECT selection_row_id, frame_row_id, session_membership_ordinal,
                file_row_id, source_root_row_id,
                canonical_relative_path, stable_file_identity,
                byte_size, sha256_fingerprint, no_follow_verified, verified_at
         FROM calibration_handoff_frame
         WHERE selection_row_id = ?
         ORDER BY session_membership_ordinal ASC",
    )
    .bind(selection_row_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::from)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_db() -> sqlx::SqlitePool {
        let db = persistence_core::Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db.pool().clone()
    }

    /// Seed the minimal FK chain needed for handoff tests:
    /// actor, config, repo_change, command, operation, session, calibration session,
    /// spec062_project (for handoff FK), matching_settings_revision, source_root,
    /// file_identity, frame_record, spec062_calibration_session.
    async fn seed_handoff_prerequisites(pool: &sqlx::SqlitePool) {
        let ts = "2026-07-22T00:00:00.000000Z";
        sqlx::query(
            "INSERT INTO spec062_actor VALUES (1,'00000000-0000-7000-d000-000000000001',?)",
        )
        .bind(ts)
        .execute(pool)
        .await
        .expect("actor");
        sqlx::query("INSERT INTO spec062_config_revision VALUES (1,'00000000-0000-7000-d000-000000000002',1,'cfg-digest',?)").bind(ts).execute(pool).await.expect("config");
        sqlx::query("INSERT INTO repository_change(command_row_id,created_at) VALUES (NULL,?)")
            .bind(ts)
            .execute(pool)
            .await
            .expect("repo_change");
        sqlx::query("INSERT INTO command_execution (row_id,public_id,actor_row_id,operation,canonical_payload_digest,state,response_json,created_at,finished_at) VALUES (1,'00000000-0000-7000-d000-000000000003',1,'calibration.handoff.create','pd','applied','{}',?,?)").bind(ts).bind(ts).execute(pool).await.expect("command");
        sqlx::query("INSERT INTO session_materialization_operation (row_id,public_id,kind,command_row_id,config_revision_row_id,state,created_sequence,created_at) VALUES (1,'00000000-0000-7000-d000-000000000004','inbox_ingestion',1,1,'ready',1,?)").bind(ts).execute(pool).await.expect("operation");

        // spec062_project
        sqlx::query(
            "INSERT INTO spec062_project (row_id,public_id,created_at) VALUES (1,'proj-pub-001',?)",
        )
        .bind(ts)
        .execute(pool)
        .await
        .expect("project");

        // matching_settings_revision (minimal: all constraints require reasonable defaults)
        sqlx::query(
            "INSERT INTO matching_settings_revision (
                row_id,public_id,revision_number,
                same_session_coverage_min_ppm,same_session_centre_max_ppm,same_session_rotation_max_udeg,
                sibling_coverage_min_ppm,sibling_centre_max_ppm,sibling_rotation_max_udeg,
                mosaic_overlap_min_ppm,mosaic_overlap_max_ppm,
                dark_thermal_moderate_millic,dark_thermal_severe_millic,
                flat_orientation_normal_udeg,flat_orientation_red_udeg,flat_red_age_days,
                canonical_digest,actor_row_id,command_row_id,created_sequence,created_at
             ) VALUES (1,'msr-pub-001',1,950000,20000,1000000,900000,50000,5000000,50000,400000,500,2000,2000000,5000000,7,'msr-digest',1,1,1,?)"
        ).bind(ts).execute(pool).await.expect("matching_settings_revision");
        sqlx::query("INSERT INTO matching_settings_head (singleton,head_revision_row_id,head_generation) VALUES (1,1,0)").execute(pool).await.expect("settings_head");

        // spec062_source_root
        sqlx::query("INSERT INTO spec062_source_root (row_id,public_id,created_at) VALUES (1,'sr-pub-001',?)").bind(ts).execute(pool).await.expect("source_root");

        // file_identity + frame_record + session + spec062_calibration_session
        sqlx::query("INSERT INTO spec062_file_identity VALUES (1,'fi-pub-001',NULL,?)")
            .bind(ts)
            .execute(pool)
            .await
            .expect("file_identity");
        sqlx::query("INSERT INTO frame_record (row_id,public_id,file_row_id,byte_size,captured_metadata_digest,created_sequence,created_at) VALUES (1,'frame-pub-001',1,4096,'fmd-001',1,?)").bind(ts).execute(pool).await.expect("frame_record");
        sqlx::query("INSERT INTO session (row_id,public_id,materialization_operation_row_id,kind,ordinal_in_operation,identity_digest,observing_night_date,night_derivation,created_sequence,created_at) VALUES (1,'ses-pub-001',1,'dark',0,'dark-id-001','2026-01-15','reviewed_local_fallback',1,?)").bind(ts).execute(pool).await.expect("session");
        sqlx::query("INSERT INTO spec062_calibration_session (session_row_id,kind,family_row_id,assignment_state,age_anchor_at_utc,cooling_setpoint_state,representative_sensor_temperature_state,created_sequence,created_at) VALUES (1,'dark',NULL,'blocked_unknown_temperature','2026-01-15T22:00:00.000000Z','absent','absent',1,?)").bind(ts).execute(pool).await.expect("calibration_session");
    }

    #[tokio::test]
    async fn insert_handoff_and_snapshot_with_head_advance() {
        let pool = setup_db().await;
        seed_handoff_prerequisites(&pool).await;
        let ts = "2026-07-22T00:00:00.000000Z";

        let mut conn = pool.acquire().await.expect("conn");
        let handoff_row_id = insert_handoff(
            &mut conn,
            &InsertHandoff {
                public_id: "ho-pub-001",
                project_row_id: 1,
                external_processor: "pixinsight_wbpp",
                created_at: ts,
            },
        )
        .await
        .expect("insert handoff");

        let snapshot_row_id = insert_handoff_snapshot(
            &mut conn,
            &InsertHandoffSnapshot {
                public_id: "hs-pub-001",
                handoff_row_id,
                predecessor_snapshot_row_id: None,
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
            },
        )
        .await
        .expect("insert snapshot");

        advance_handoff_head(&mut conn, handoff_row_id, snapshot_row_id, 0)
            .await
            .expect("advance head");

        let handoff = get_handoff_by_public_id(&pool, "ho-pub-001").await.expect("get handoff");
        assert_eq!(handoff.head_snapshot_row_id, Some(snapshot_row_id));
        assert_eq!(handoff.head_generation, 1);

        let snap = get_snapshot_by_public_id(&pool, "hs-pub-001").await.expect("get snapshot");
        assert_eq!(snap.handoff_row_id, handoff_row_id);
        assert_eq!(snap.basis_digest, "basis-001");
    }

    #[tokio::test]
    async fn advance_head_cas_fails_on_stale_generation() {
        let pool = setup_db().await;
        seed_handoff_prerequisites(&pool).await;
        let ts = "2026-07-22T00:00:00.000000Z";

        let mut conn = pool.acquire().await.expect("conn");
        let handoff_row_id = insert_handoff(
            &mut conn,
            &InsertHandoff {
                public_id: "ho-pub-002",
                project_row_id: 1,
                external_processor: "siril",
                created_at: ts,
            },
        )
        .await
        .expect("handoff");
        let snap_id = insert_handoff_snapshot(
            &mut conn,
            &InsertHandoffSnapshot {
                public_id: "hs-pub-002",
                handoff_row_id,
                predecessor_snapshot_row_id: None,
                evaluation_at: ts,
                matching_settings_revision_row_id: 1,
                basis_digest: "b1",
                requirement_count: 0,
                selection_count: 0,
                frame_count: 0,
                source_byte_count: 0,
                actor_row_id: 1,
                command_row_id: 1,
                created_sequence: 1,
                created_at: ts,
            },
        )
        .await
        .expect("snap");

        // First advance succeeds
        advance_handoff_head(&mut conn, handoff_row_id, snap_id, 0).await.expect("first advance");
        // Second advance with stale generation fails
        let err = advance_handoff_head(&mut conn, handoff_row_id, snap_id, 0).await;
        assert!(matches!(err, Err(DbError::CasFailed(_))));
    }

    #[tokio::test]
    async fn handoff_requirement_and_candidate_evidence_roundtrip() {
        let pool = setup_db().await;
        seed_handoff_prerequisites(&pool).await;
        let ts = "2026-07-22T00:00:00.000000Z";

        let mut conn = pool.acquire().await.expect("conn");
        let handoff_row_id = insert_handoff(
            &mut conn,
            &InsertHandoff {
                public_id: "ho-pub-003",
                project_row_id: 1,
                external_processor: "pixinsight_wbpp",
                created_at: ts,
            },
        )
        .await
        .expect("handoff");
        let snap_id = insert_handoff_snapshot(
            &mut conn,
            &InsertHandoffSnapshot {
                public_id: "hs-pub-003",
                handoff_row_id,
                predecessor_snapshot_row_id: None,
                evaluation_at: ts,
                matching_settings_revision_row_id: 1,
                basis_digest: "b3",
                requirement_count: 1,
                selection_count: 0,
                frame_count: 0,
                source_byte_count: 0,
                actor_row_id: 1,
                command_row_id: 1,
                created_sequence: 1,
                created_at: ts,
            },
        )
        .await
        .expect("snap");
        advance_handoff_head(&mut conn, handoff_row_id, snap_id, 0).await.expect("head");

        let req_id = insert_handoff_requirement(
            &mut conn,
            &InsertHandoffRequirement {
                public_id: "req-pub-001",
                handoff_row_id,
                kind: "dark",
                camera_row_id: None,
                family_row_id: None,
                recipe_revision: 1,
                evidence_digest: "ev-digest-001",
                required_field_state: "complete",
            },
        )
        .await
        .expect("requirement");

        insert_snapshot_requirement_mapping(&mut conn, snap_id, req_id, handoff_row_id, 0)
            .await
            .expect("mapping");

        let ev_id = insert_handoff_candidate_evidence(
            &mut conn,
            &InsertHandoffCandidateEvidence {
                public_id: "cev-pub-001",
                handoff_row_id,
                snapshot_row_id: snap_id,
                requirement_row_id: req_id,
                session_row_id: 1,
                recipe_compatible: true,
                recipe_complete: true,
                age_days: 50,
                age_severity: "normal",
                thermal_state: "normal",
                available_frame_count: 100,
                readable_frame_count: 100,
                automatic_eligible: true,
                evidence_digest: "ev-001",
                observed_at: ts,
                warning_codes: &[],
            },
        )
        .await
        .expect("candidate evidence");
        assert!(ev_id > 0);

        let reqs = list_snapshot_requirements(&pool, snap_id).await.expect("list reqs");
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].kind, "dark");
    }

    #[tokio::test]
    async fn handoff_operation_state_transition() {
        let pool = setup_db().await;
        seed_handoff_prerequisites(&pool).await;
        let ts = "2026-07-22T00:00:00.000000Z";

        let mut conn = pool.acquire().await.expect("conn");
        let handoff_row_id = insert_handoff(
            &mut conn,
            &InsertHandoff {
                public_id: "ho-pub-004",
                project_row_id: 1,
                external_processor: "siril",
                created_at: ts,
            },
        )
        .await
        .expect("handoff");
        let op_id = insert_handoff_operation(
            &mut conn,
            &InsertHandoffOperation {
                public_id: "op-pub-001",
                handoff_row_id,
                command_row_id: 1,
                created_at: ts,
            },
        )
        .await
        .expect("operation");

        // ready → verifying
        transition_handoff_operation_state(&mut conn, op_id, "ready", "verifying", 0, None)
            .await
            .expect("to verifying");

        let op = get_operation_by_public_id(&pool, "op-pub-001").await.expect("get op");
        assert_eq!(op.state, "verifying");
        assert_eq!(op.state_version, 1);
    }
}
