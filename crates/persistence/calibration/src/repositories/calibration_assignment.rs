// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for `calibration_assignment` table (spec 007, T006/T026).
//!
//! Operates on the `calibration_assignment` table from migration 0022.
//! Upsert semantics: a second assign to the same `(session_id, calibration_type)`
//! replaces the prior record (data-model invariant 4).
//!
//! Uses dynamic SQL (not `sqlx::query!` compile-time macros) consistent with
//! other repositories in this crate that do not require a sqlx offline cache.

use domain_core::ids::Timestamp;
use sqlx::types::Json;
use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

// ── Row type ──────────────────────────────────────────────────────────────────

/// Database row for a calibration assignment.
#[derive(Debug, Clone)]
pub struct CalibrationAssignmentRow {
    pub id: String,
    pub session_id: String,
    pub calibration_type: String,
    pub master_id: String,
    pub confidence: f64,
    pub was_override: bool,
    /// JSON array of dimension name strings.
    pub mismatched_dimensions: String,
    pub assigned_at: String,
}

/// Inputs for [`upsert`].
pub struct UpsertParams<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub calibration_type: &'a str,
    pub master_id: &'a str,
    pub confidence: f64,
    pub was_override: bool,
    pub mismatched_dimensions: &'a [String],
    /// ISO-8601 UTC timestamp. When `None`, uses the current UTC time.
    pub assigned_at: Option<&'a str>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn row_to_struct(
    (
        id,
        session_id,
        calibration_type,
        master_id,
        confidence,
        was_override,
        mismatched_dimensions,
        assigned_at,
    ): (String, String, String, String, f64, i64, String, String),
) -> CalibrationAssignmentRow {
    CalibrationAssignmentRow {
        id,
        session_id,
        calibration_type,
        master_id,
        confidence,
        was_override: was_override != 0,
        mismatched_dimensions,
        assigned_at,
    }
}

// ── Write ─────────────────────────────────────────────────────────────────────

/// Upsert a calibration assignment.
///
/// Replaces any existing row for the same `(session_id, calibration_type)` pair.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure (including JSON encoding
/// of `mismatched_dimensions`, encoded via `sqlx::types::Json`).
pub async fn upsert(pool: &SqlitePool, params: UpsertParams<'_>) -> DbResult<()> {
    let at = params.assigned_at.map_or_else(Timestamp::now_iso, str::to_owned);
    let override_int = i64::from(params.was_override);

    sqlx::query(
        "INSERT INTO calibration_assignment
            (id, session_id, calibration_type, master_id, confidence,
             was_override, mismatched_dimensions, assigned_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(session_id, calibration_type)
        DO UPDATE SET
            id = excluded.id,
            master_id = excluded.master_id,
            confidence = excluded.confidence,
            was_override = excluded.was_override,
            mismatched_dimensions = excluded.mismatched_dimensions,
            assigned_at = excluded.assigned_at",
    )
    .bind(params.id)
    .bind(params.session_id)
    .bind(params.calibration_type)
    .bind(params.master_id)
    .bind(params.confidence)
    .bind(override_int)
    .bind(Json(params.mismatched_dimensions))
    .bind(&at)
    .execute(pool)
    .await
    .map_err(DbError::Database)?;

    Ok(())
}

/// Delete the assignment for a `(session_id, calibration_type)` pair (#875:
/// un-assign — returns the session to "no master assigned" for that type).
///
/// Returns `true` when a row was deleted, `false` when none existed.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn delete(pool: &SqlitePool, session_id: &str, calibration_type: &str) -> DbResult<bool> {
    let result = sqlx::query(
        "DELETE FROM calibration_assignment WHERE session_id = ? AND calibration_type = ?",
    )
    .bind(session_id)
    .bind(calibration_type)
    .execute(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(result.rows_affected() > 0)
}

/// Get the current assignment for a `(session_id, calibration_type)` pair.
///
/// Returns `None` when no assignment exists.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get(
    pool: &SqlitePool,
    session_id: &str,
    calibration_type: &str,
) -> DbResult<Option<CalibrationAssignmentRow>> {
    let row: Option<(String, String, String, String, f64, i64, String, String)> = sqlx::query_as(
        "SELECT id, session_id, calibration_type, master_id, confidence,
                   was_override, mismatched_dimensions, assigned_at
            FROM calibration_assignment
            WHERE session_id = ? AND calibration_type = ?",
    )
    .bind(session_id)
    .bind(calibration_type)
    .fetch_optional(pool)
    .await
    .map_err(DbError::Database)?;

    Ok(row.map(row_to_struct))
}

/// List all assignments for a session.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_for_session(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Vec<CalibrationAssignmentRow>> {
    let rows: Vec<(String, String, String, String, f64, i64, String, String)> = sqlx::query_as(
        "SELECT id, session_id, calibration_type, master_id, confidence,
                   was_override, mismatched_dimensions, assigned_at
            FROM calibration_assignment
            WHERE session_id = ?
            ORDER BY calibration_type",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::Database)?;

    Ok(rows.into_iter().map(row_to_struct).collect())
}

// ── Missing-frame awareness (spec 048 US5, FR-024/025) ─────────────────────────
//
// A "master" here is always a `calibration_session` row (`master_id` ==
// `calibration_session.id`, per the `calibration_master_view` join in
// migration 0033) — there is no separate generated-master-file table in the
// active matching path. `calibration_master` (migration 0002) is a distinct,
// currently-unpopulated table for a generated master FILE derived from that
// session; the two presence checks below cover both possibilities (PATH A:
// the generated file: PATH B: the session's own raw sub-frames) without
// assuming either is populated.

/// PATH A: state of the generated master artifact for `master_id`'s session,
/// via `calibration_master.source_session_id` → `.artifact_id` →
/// spec-012 `processing_artifacts.state`. `None` when no such artifact is
/// tracked (the common case today — nothing populates `calibration_master`
/// yet); `Some(state)` is one of `present` / `missing` / `user_resolved_missing`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn master_artifact_state(pool: &SqlitePool, master_id: &str) -> DbResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT pa.state
         FROM calibration_master cm
         JOIN processing_artifacts pa ON pa.id = cm.artifact_id
         WHERE cm.source_session_id = ?
         LIMIT 1",
    )
    .bind(master_id)
    .fetch_optional(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(row.map(|(state,)| state))
}

/// PATH B: does `master_id`'s own `calibration_session` currently have any
/// member frame (`frame_ids`) whose `file_record.state = 'missing'`?
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn master_has_missing_source_frame(pool: &SqlitePool, master_id: &str) -> DbResult<bool> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)
         FROM calibration_session cs
         JOIN json_each(cs.frame_ids) je
         JOIN file_record fr ON fr.id = je.value
         WHERE cs.id = ? AND fr.state = 'missing'",
    )
    .bind(master_id)
    .fetch_one(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(count > 0)
}

/// Assignments whose master (`calibration_session`) currently lists
/// `frame_id` among its `frame_ids` — i.e. matches PATH B would affect when
/// `frame_id` transitions missing/recovered. Used to scope
/// `calibration_match.source_missing` / `.source_recovered` audit emission
/// to exactly the assignments a raw-frame reconcile outcome affects.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_by_source_frame(
    pool: &SqlitePool,
    frame_id: &str,
) -> DbResult<Vec<CalibrationAssignmentRow>> {
    let like = format!("%\"{frame_id}\"%");
    let rows: Vec<(String, String, String, String, f64, i64, String, String)> = sqlx::query_as(
        "SELECT ca.id, ca.session_id, ca.calibration_type, ca.master_id, ca.confidence,
                ca.was_override, ca.mismatched_dimensions, ca.assigned_at
         FROM calibration_assignment ca
         JOIN calibration_session cs ON cs.id = ca.master_id
         WHERE cs.frame_ids LIKE ?",
    )
    .bind(like)
    .fetch_all(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(rows.into_iter().map(row_to_struct).collect())
}

/// Assignments whose master has a generated-master artifact `artifact_id` —
/// i.e. matches PATH A would affect when that artifact transitions
/// missing/recovered. Used to scope `calibration_match.source_missing` /
/// `.source_recovered` audit emission to exactly the assignments an
/// artifact reconcile outcome affects.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_by_source_artifact(
    pool: &SqlitePool,
    artifact_id: &str,
) -> DbResult<Vec<CalibrationAssignmentRow>> {
    let rows: Vec<(String, String, String, String, f64, i64, String, String)> = sqlx::query_as(
        "SELECT ca.id, ca.session_id, ca.calibration_type, ca.master_id, ca.confidence,
                ca.was_override, ca.mismatched_dimensions, ca.assigned_at
         FROM calibration_assignment ca
         JOIN calibration_master cm ON cm.source_session_id = ca.master_id
         WHERE cm.artifact_id = ?",
    )
    .bind(artifact_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::Database)?;
    Ok(rows.into_iter().map(row_to_struct).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;

    async fn setup() -> SqlitePool {
        let db = Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db.pool().clone()
    }

    fn params<'a>(
        id: &'a str,
        session_id: &'a str,
        calibration_type: &'a str,
        master_id: &'a str,
        confidence: f64,
        was_override: bool,
        mismatched_dimensions: &'a [String],
    ) -> UpsertParams<'a> {
        UpsertParams {
            id,
            session_id,
            calibration_type,
            master_id,
            confidence,
            was_override,
            mismatched_dimensions,
            assigned_at: None,
        }
    }

    #[tokio::test]
    async fn upsert_and_get_assignment() {
        let pool = setup().await;
        upsert(&pool, params("assign-001", "ses-001", "dark", "master-001", 0.95, false, &[]))
            .await
            .unwrap();

        let row = get(&pool, "ses-001", "dark").await.unwrap().unwrap();
        assert_eq!(row.id, "assign-001");
        assert_eq!(row.master_id, "master-001");
        assert!((row.confidence - 0.95).abs() < 1e-9);
        assert!(!row.was_override);
    }

    #[tokio::test]
    async fn upsert_replaces_prior_assignment() {
        let pool = setup().await;
        upsert(&pool, params("assign-001", "ses-001", "dark", "master-001", 0.9, false, &[]))
            .await
            .unwrap();
        let override_dims = vec!["gain".to_owned()];
        upsert(
            &pool,
            params("assign-002", "ses-001", "dark", "master-002", 0.7, true, &override_dims),
        )
        .await
        .unwrap();

        let row = get(&pool, "ses-001", "dark").await.unwrap().unwrap();
        assert_eq!(row.id, "assign-002");
        assert_eq!(row.master_id, "master-002");
        assert!(row.was_override);
        // Round-trips through the `sqlx::types::Json` write-side codec.
        let got: Vec<String> = serde_json::from_str(&row.mismatched_dimensions).unwrap_or_default();
        assert_eq!(got, override_dims);
    }

    #[tokio::test]
    async fn get_returns_none_when_absent() {
        let pool = setup().await;
        let row = get(&pool, "ses-999", "dark").await.unwrap();
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn list_for_session_returns_all_types() {
        let pool = setup().await;
        upsert(&pool, params("a-1", "ses-001", "dark", "m-1", 1.0, false, &[])).await.unwrap();
        upsert(&pool, params("a-2", "ses-001", "flat", "m-2", 0.9, false, &[])).await.unwrap();
        upsert(&pool, params("a-3", "ses-001", "bias", "m-3", 1.0, false, &[])).await.unwrap();

        let rows = list_for_session(&pool, "ses-001").await.unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[tokio::test]
    async fn dark_flat_calibration_type_rejected_by_db_constraint() {
        let pool = setup().await;
        let result =
            upsert(&pool, params("a-df", "ses-001", "dark_flat", "m-1", 1.0, false, &[])).await;
        assert!(result.is_err(), "dark_flat should be rejected by DB CHECK constraint");
    }
}
