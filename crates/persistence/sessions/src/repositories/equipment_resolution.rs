// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for `session_equipment_resolution` and its head pointer.
//!
//! Each accepted revision is immutable. The head row advances by CAS on
//! `head_generation`. Writers must hold `BEGIN IMMEDIATE` and check
//! `changes() = 1` after the CAS update.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

/// One equipment resolution revision row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EquipmentResolutionRow {
    pub row_id: i64,
    pub public_id: String,
    pub session_row_id: i64,
    pub revision_number: i64,
    pub predecessor_resolution_row_id: Option<i64>,
    pub camera_row_id: Option<i64>,
    pub optical_profile_row_id: Option<i64>,
    pub camera_alias_evidence_row_id: Option<i64>,
    pub optical_alias_evidence_row_id: Option<i64>,
    pub focal_length_reported_um: Option<i64>,
    pub focal_length_calculated_um: Option<i64>,
    pub comparison_severity: String,
    pub assignment_mode: String,
    pub accepted_proposal_row_id: Option<i64>,
    pub config_revision_row_id: i64,
    pub actor_row_id: i64,
    pub created_sequence: i64,
    pub created_at: String,
}

/// Head pointer row for equipment resolution.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EquipmentResolutionHeadRow {
    pub session_row_id: i64,
    pub head_resolution_row_id: i64,
    pub head_generation: i64,
}

/// Parameters for inserting one equipment resolution revision.
pub struct InsertEquipmentResolution<'a> {
    pub public_id: &'a str,
    pub session_row_id: i64,
    pub revision_number: i64,
    pub predecessor_resolution_row_id: Option<i64>,
    pub camera_row_id: Option<i64>,
    pub optical_profile_row_id: Option<i64>,
    pub camera_alias_evidence_row_id: Option<i64>,
    pub optical_alias_evidence_row_id: Option<i64>,
    pub focal_length_reported_um: Option<i64>,
    pub focal_length_calculated_um: Option<i64>,
    pub comparison_severity: &'a str,
    pub assignment_mode: &'a str,
    pub accepted_proposal_row_id: Option<i64>,
    pub config_revision_row_id: i64,
    pub actor_row_id: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Insert one immutable `session_equipment_resolution` revision and return its
/// `row_id`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_equipment_resolution(
    conn: &mut SqliteConnection,
    params: &InsertEquipmentResolution<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO session_equipment_resolution (
            public_id, session_row_id, revision_number,
            predecessor_resolution_row_id,
            camera_row_id, optical_profile_row_id,
            camera_alias_evidence_row_id, optical_alias_evidence_row_id,
            focal_length_reported_um, focal_length_calculated_um,
            comparison_severity, assignment_mode,
            accepted_proposal_row_id, config_revision_row_id, actor_row_id,
            created_sequence, created_at
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.session_row_id)
    .bind(params.revision_number)
    .bind(params.predecessor_resolution_row_id)
    .bind(params.camera_row_id)
    .bind(params.optical_profile_row_id)
    .bind(params.camera_alias_evidence_row_id)
    .bind(params.optical_alias_evidence_row_id)
    .bind(params.focal_length_reported_um)
    .bind(params.focal_length_calculated_um)
    .bind(params.comparison_severity)
    .bind(params.assignment_mode)
    .bind(params.accepted_proposal_row_id)
    .bind(params.config_revision_row_id)
    .bind(params.actor_row_id)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert the initial head pointer for a session's equipment resolution.
///
/// Called once when the first revision is accepted. Subsequent updates use
/// [`advance_equipment_resolution_head`].
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_equipment_resolution_head(
    conn: &mut SqliteConnection,
    session_row_id: i64,
    head_resolution_row_id: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_equipment_resolution_head
         (session_row_id, head_resolution_row_id, head_generation)
         VALUES (?, ?, 0)",
    )
    .bind(session_row_id)
    .bind(head_resolution_row_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Advance the equipment resolution head using CAS.
///
/// Returns `Ok(())` when exactly one row was updated. Returns
/// `persistence_core::DbError::CasFailed` when the current generation or head revision does not
/// match the expected values (indicating a concurrent update).
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` on optimistic-lock failure, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn advance_equipment_resolution_head(
    conn: &mut SqliteConnection,
    session_row_id: i64,
    expected_head_row_id: i64,
    expected_generation: i64,
    new_head_row_id: i64,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE session_equipment_resolution_head
         SET head_resolution_row_id = ?,
             head_generation = head_generation + 1
         WHERE session_row_id = ?
           AND head_resolution_row_id = ?
           AND head_generation = ?",
    )
    .bind(new_head_row_id)
    .bind(session_row_id)
    .bind(expected_head_row_id)
    .bind(expected_generation)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "equipment resolution head CAS failed for session {session_row_id}"
        )));
    }
    Ok(())
}

/// Fetch the current equipment resolution head for a session.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no head row exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_equipment_resolution_head(
    pool: &SqlitePool,
    session_row_id: i64,
) -> DbResult<EquipmentResolutionHeadRow> {
    sqlx::query_as::<_, EquipmentResolutionHeadRow>(
        "SELECT session_row_id, head_resolution_row_id, head_generation
         FROM session_equipment_resolution_head
         WHERE session_row_id = ?",
    )
    .bind(session_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        DbError::NotFound(format!("equipment resolution head for session {session_row_id}"))
    })
}

/// Fetch the accepted equipment resolution revision for a session.
///
/// Joins the head pointer to the resolution row.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no head or revision exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_accepted_equipment_resolution(
    pool: &SqlitePool,
    session_row_id: i64,
) -> DbResult<EquipmentResolutionRow> {
    sqlx::query_as::<_, EquipmentResolutionRow>(
        "SELECT er.row_id, er.public_id, er.session_row_id, er.revision_number,
                er.predecessor_resolution_row_id,
                er.camera_row_id, er.optical_profile_row_id,
                er.camera_alias_evidence_row_id, er.optical_alias_evidence_row_id,
                er.focal_length_reported_um, er.focal_length_calculated_um,
                er.comparison_severity, er.assignment_mode,
                er.accepted_proposal_row_id, er.config_revision_row_id,
                er.actor_row_id, er.created_sequence, er.created_at
         FROM session_equipment_resolution er
         INNER JOIN session_equipment_resolution_head erh
             ON erh.head_resolution_row_id = er.row_id
         WHERE erh.session_row_id = ?",
    )
    .bind(session_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("equipment resolution for session {session_row_id}")))
}
