// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for `spec062_calibration_session` and `dark_thermal_evidence`.
//!
//! A calibration session extends a `session` row via `session_row_id`. The
//! insert functions are called from the materialization apply transaction that
//! creates the parent `session` row first.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

// ── Row projections ────────────────────────────────────────────────────────────

/// `spec062_calibration_session` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CalibrationSessionRow {
    pub session_row_id: i64,
    pub kind: String,
    pub family_row_id: Option<i64>,
    pub assignment_state: String,
    pub assignment_proposal_row_id: Option<i64>,
    pub age_anchor_at_utc: String,
    pub cooling_setpoint_state: String,
    pub cooling_setpoint_millic: Option<i64>,
    pub representative_sensor_temperature_state: String,
    pub representative_sensor_temperature_millic: Option<i64>,
    pub created_sequence: i64,
    pub created_at: String,
}

/// `dark_thermal_evidence` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DarkThermalEvidenceRow {
    pub session_row_id: i64,
    pub valid_count: i64,
    pub missing_count: i64,
    pub invalid_count: i64,
    pub minimum_abs_deviation_millic: Option<i64>,
    pub median_abs_deviation_millic: Option<i64>,
    pub maximum_abs_deviation_millic: Option<i64>,
    pub p95_abs_deviation_millic: Option<i64>,
    pub valid_ratio_ppm: i64,
    pub severity: String,
    pub created_sequence: i64,
}

// ── Insert parameters ─────────────────────────────────────────────────────────

/// Parameters for inserting one `spec062_calibration_session` row.
pub struct InsertCalibrationSession<'a> {
    pub session_row_id: i64,
    pub kind: &'a str,
    pub family_row_id: Option<i64>,
    pub assignment_state: &'a str,
    pub assignment_proposal_row_id: Option<i64>,
    pub age_anchor_at_utc: &'a str,
    pub cooling_setpoint_state: &'a str,
    pub cooling_setpoint_millic: Option<i64>,
    pub representative_sensor_temperature_state: &'a str,
    pub representative_sensor_temperature_millic: Option<i64>,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Parameters for inserting one `dark_thermal_evidence` row.
pub struct InsertDarkThermalEvidence<'a> {
    pub session_row_id: i64,
    pub valid_count: i64,
    pub missing_count: i64,
    pub invalid_count: i64,
    pub minimum_abs_deviation_millic: Option<i64>,
    pub median_abs_deviation_millic: Option<i64>,
    pub maximum_abs_deviation_millic: Option<i64>,
    pub p95_abs_deviation_millic: Option<i64>,
    pub valid_ratio_ppm: i64,
    pub severity: &'a str,
    pub created_sequence: i64,
}

// ── Writes ────────────────────────────────────────────────────────────────────

/// Insert one `spec062_calibration_session` row.
///
/// Called inside the materialization apply transaction, after the parent
/// `session` row exists.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_calibration_session(
    conn: &mut SqliteConnection,
    params: &InsertCalibrationSession<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO spec062_calibration_session (
            session_row_id, kind,
            family_row_id, assignment_state, assignment_proposal_row_id,
            age_anchor_at_utc,
            cooling_setpoint_state, cooling_setpoint_millic,
            representative_sensor_temperature_state,
            representative_sensor_temperature_millic,
            created_sequence, created_at
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.session_row_id)
    .bind(params.kind)
    .bind(params.family_row_id)
    .bind(params.assignment_state)
    .bind(params.assignment_proposal_row_id)
    .bind(params.age_anchor_at_utc)
    .bind(params.cooling_setpoint_state)
    .bind(params.cooling_setpoint_millic)
    .bind(params.representative_sensor_temperature_state)
    .bind(params.representative_sensor_temperature_millic)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert one `dark_thermal_evidence` row for a regulated dark session.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_dark_thermal_evidence(
    conn: &mut SqliteConnection,
    params: &InsertDarkThermalEvidence<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO dark_thermal_evidence (
            session_row_id,
            valid_count, missing_count, invalid_count,
            minimum_abs_deviation_millic, median_abs_deviation_millic,
            maximum_abs_deviation_millic, p95_abs_deviation_millic,
            valid_ratio_ppm, severity, created_sequence
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.session_row_id)
    .bind(params.valid_count)
    .bind(params.missing_count)
    .bind(params.invalid_count)
    .bind(params.minimum_abs_deviation_millic)
    .bind(params.median_abs_deviation_millic)
    .bind(params.maximum_abs_deviation_millic)
    .bind(params.p95_abs_deviation_millic)
    .bind(params.valid_ratio_ppm)
    .bind(params.severity)
    .bind(params.created_sequence)
    .execute(conn)
    .await?;
    Ok(())
}

/// Assign a calibration session to a family by updating `family_row_id` and
/// `assignment_state`.
///
/// Uses a compare-and-swap on the current `assignment_state` to prevent
/// concurrent double-assignment. Callers must hold `BEGIN IMMEDIATE`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::CasFailed` when the row was already assigned or the
/// expected prior state does not match.
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn assign_calibration_session_to_family(
    conn: &mut SqliteConnection,
    session_row_id: i64,
    family_row_id: i64,
    expected_prior_state: &str,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE spec062_calibration_session
         SET family_row_id = ?,
             assignment_state = 'assigned'
         WHERE session_row_id = ?
           AND assignment_state = ?",
    )
    .bind(family_row_id)
    .bind(session_row_id)
    .bind(expected_prior_state)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "calibration session {session_row_id} assignment CAS failed \
             (expected state={expected_prior_state})"
        )));
    }
    Ok(())
}

// ── Reads ─────────────────────────────────────────────────────────────────────

/// Fetch a `spec062_calibration_session` by its parent `session_row_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_calibration_session(
    pool: &SqlitePool,
    session_row_id: i64,
) -> DbResult<CalibrationSessionRow> {
    sqlx::query_as::<_, CalibrationSessionRow>(
        "SELECT session_row_id, kind,
                family_row_id, assignment_state, assignment_proposal_row_id,
                age_anchor_at_utc,
                cooling_setpoint_state, cooling_setpoint_millic,
                representative_sensor_temperature_state,
                representative_sensor_temperature_millic,
                created_sequence, created_at
         FROM spec062_calibration_session
         WHERE session_row_id = ?",
    )
    .bind(session_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        DbError::NotFound(format!("spec062_calibration_session row_id={session_row_id}"))
    })
}

/// Fetch a `spec062_calibration_session` by the parent session's `public_id`.
///
/// Performs a join to `session` to resolve the `row_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the session or calibration extension row
/// does not exist.
pub async fn get_calibration_session_by_public_id(
    pool: &SqlitePool,
    session_public_id: &str,
) -> DbResult<CalibrationSessionRow> {
    sqlx::query_as::<_, CalibrationSessionRow>(
        "SELECT cs.session_row_id, cs.kind,
                cs.family_row_id, cs.assignment_state, cs.assignment_proposal_row_id,
                cs.age_anchor_at_utc,
                cs.cooling_setpoint_state, cs.cooling_setpoint_millic,
                cs.representative_sensor_temperature_state,
                cs.representative_sensor_temperature_millic,
                cs.created_sequence, cs.created_at
         FROM spec062_calibration_session cs
         INNER JOIN session s ON s.row_id = cs.session_row_id
         WHERE s.public_id = ?",
    )
    .bind(session_public_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("calibration session {session_public_id}")))
}

/// List all calibration sessions belonging to a family, ordered by
/// `age_anchor_at_utc DESC` then `session_row_id ASC` for deterministic
/// paging. Returns `row_id` and `age_anchor_at_utc` only; callers join further
/// columns as needed.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn list_calibration_sessions_by_family(
    pool: &SqlitePool,
    family_row_id: i64,
) -> DbResult<Vec<CalibrationSessionRow>> {
    sqlx::query_as::<_, CalibrationSessionRow>(
        "SELECT session_row_id, kind,
                family_row_id, assignment_state, assignment_proposal_row_id,
                age_anchor_at_utc,
                cooling_setpoint_state, cooling_setpoint_millic,
                representative_sensor_temperature_state,
                representative_sensor_temperature_millic,
                created_sequence, created_at
         FROM spec062_calibration_session
         WHERE family_row_id = ?
         ORDER BY age_anchor_at_utc DESC, session_row_id ASC",
    )
    .bind(family_row_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::from)
}

/// Fetch `dark_thermal_evidence` for a dark session.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no evidence row exists for this session.
pub async fn get_dark_thermal_evidence(
    pool: &SqlitePool,
    session_row_id: i64,
) -> DbResult<DarkThermalEvidenceRow> {
    sqlx::query_as::<_, DarkThermalEvidenceRow>(
        "SELECT session_row_id,
                valid_count, missing_count, invalid_count,
                minimum_abs_deviation_millic, median_abs_deviation_millic,
                maximum_abs_deviation_millic, p95_abs_deviation_millic,
                valid_ratio_ppm, severity, created_sequence
         FROM dark_thermal_evidence
         WHERE session_row_id = ?",
    )
    .bind(session_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        DbError::NotFound(format!("dark_thermal_evidence session_row_id={session_row_id}"))
    })
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

    /// Seed the minimal rows for a calibration session insert test.
    async fn seed_base(pool: &sqlx::SqlitePool) {
        let ts = "2026-07-22T00:00:00.000000Z";
        sqlx::query(
            "INSERT INTO spec062_actor VALUES (1,'00000000-0000-7000-b000-000000000001',?)",
        )
        .bind(ts)
        .execute(pool)
        .await
        .expect("actor");
        sqlx::query("INSERT INTO spec062_config_revision VALUES (1,'00000000-0000-7000-b000-000000000002',1,'cfg-digest',?)").bind(ts).execute(pool).await.expect("config");
        sqlx::query("INSERT INTO repository_change(command_row_id,created_at) VALUES (NULL,?)")
            .bind(ts)
            .execute(pool)
            .await
            .expect("repo_change");
        sqlx::query(
            "INSERT INTO command_execution (row_id,public_id,actor_row_id,operation,canonical_payload_digest,state,response_json,created_at,finished_at)
             VALUES (1,'00000000-0000-7000-b000-000000000003',1,'inbox.materialization.apply','pd','applied','{}',?,?)"
        ).bind(ts).bind(ts).execute(pool).await.expect("command");
        sqlx::query(
            "INSERT INTO session_materialization_operation (row_id,public_id,kind,command_row_id,config_revision_row_id,state,created_sequence,created_at)
             VALUES (1,'00000000-0000-7000-b000-000000000004','inbox_ingestion',1,1,'ready',1,?)"
        ).bind(ts).execute(pool).await.expect("operation");
    }

    async fn insert_session(
        pool: &sqlx::SqlitePool,
        row_id: i64,
        kind: &str,
        ordinal: i64,
        digest: &str,
    ) {
        let sql = format!(
            "INSERT INTO session (row_id,public_id,materialization_operation_row_id,kind,\
             ordinal_in_operation,identity_digest,observing_night_date,night_derivation,\
             created_sequence,created_at) VALUES ({row_id},'ses-pub-{row_id:03}',1,'{kind}',\
             {ordinal},'{digest}','2026-01-15','reviewed_local_fallback',1,\
             '2026-07-22T00:00:00.000000Z')"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await.expect("session");
    }

    #[tokio::test]
    async fn insert_and_get_dark_calibration_session_blocked_unknown_temperature() {
        let pool = setup_db().await;
        seed_base(&pool).await;
        insert_session(&pool, 1, "dark", 0, "dark-id-001").await;

        let mut conn = pool.acquire().await.expect("conn");
        insert_calibration_session(
            &mut conn,
            &InsertCalibrationSession {
                session_row_id: 1,
                kind: "dark",
                family_row_id: None,
                assignment_state: "blocked_unknown_temperature",
                assignment_proposal_row_id: None,
                age_anchor_at_utc: "2026-01-15T22:00:00.000000Z",
                cooling_setpoint_state: "absent",
                cooling_setpoint_millic: None,
                representative_sensor_temperature_state: "absent",
                representative_sensor_temperature_millic: None,
                created_sequence: 1,
                created_at: "2026-07-22T00:00:00.000000Z",
            },
        )
        .await
        .expect("insert calibration session");

        let row = get_calibration_session(&pool, 1).await.expect("get");
        assert_eq!(row.kind, "dark");
        assert_eq!(row.assignment_state, "blocked_unknown_temperature");
        assert!(row.family_row_id.is_none());
        assert_eq!(row.cooling_setpoint_state, "absent");
    }

    #[tokio::test]
    async fn insert_and_get_bias_calibration_session() {
        let pool = setup_db().await;
        seed_base(&pool).await;
        insert_session(&pool, 1, "bias", 0, "bias-id-001").await;

        let mut conn = pool.acquire().await.expect("conn");
        insert_calibration_session(
            &mut conn,
            &InsertCalibrationSession {
                session_row_id: 1,
                kind: "bias",
                family_row_id: None,
                assignment_state: "needs_review",
                assignment_proposal_row_id: None,
                age_anchor_at_utc: "2026-01-15T22:00:00.000000Z",
                cooling_setpoint_state: "absent",
                cooling_setpoint_millic: None,
                representative_sensor_temperature_state: "absent",
                representative_sensor_temperature_millic: None,
                created_sequence: 1,
                created_at: "2026-07-22T00:00:00.000000Z",
            },
        )
        .await
        .expect("insert bias calibration session");

        let row = get_calibration_session_by_public_id(&pool, "ses-pub-001")
            .await
            .expect("get by public id");
        assert_eq!(row.kind, "bias");
        assert_eq!(row.assignment_state, "needs_review");
    }

    #[tokio::test]
    async fn insert_and_get_dark_thermal_evidence() {
        let pool = setup_db().await;
        seed_base(&pool).await;
        insert_session(&pool, 1, "dark", 0, "dark-id-001").await;

        let mut conn = pool.acquire().await.expect("conn");
        insert_calibration_session(
            &mut conn,
            &InsertCalibrationSession {
                session_row_id: 1,
                kind: "dark",
                family_row_id: None,
                assignment_state: "blocked_unknown_temperature",
                assignment_proposal_row_id: None,
                age_anchor_at_utc: "2026-01-15T22:00:00.000000Z",
                cooling_setpoint_state: "present",
                cooling_setpoint_millic: Some(-10_000),
                representative_sensor_temperature_state: "present",
                representative_sensor_temperature_millic: Some(-9_800),
                created_sequence: 1,
                created_at: "2026-07-22T00:00:00.000000Z",
            },
        )
        .await
        .expect("calibration session");

        insert_dark_thermal_evidence(
            &mut conn,
            &InsertDarkThermalEvidence {
                session_row_id: 1,
                valid_count: 100,
                missing_count: 0,
                invalid_count: 0,
                minimum_abs_deviation_millic: Some(100),
                median_abs_deviation_millic: Some(200),
                maximum_abs_deviation_millic: Some(400),
                p95_abs_deviation_millic: Some(350),
                valid_ratio_ppm: 1_000_000,
                severity: "normal",
                created_sequence: 1,
            },
        )
        .await
        .expect("thermal evidence");

        let ev = get_dark_thermal_evidence(&pool, 1).await.expect("get thermal");
        assert_eq!(ev.valid_count, 100);
        assert_eq!(ev.severity, "normal");
        assert_eq!(ev.p95_abs_deviation_millic, Some(350));
    }
}
