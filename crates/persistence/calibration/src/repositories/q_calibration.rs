// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Free-function query repository for calibration matching (spec 007) and
//! equipment auto-detect flags (spec 030), migrated out of
//! `app_calibration::matching` / `app_calibration::equipment`
//! (db-boundary-zero drain). Business logic, domain mapping, and error
//! handling stay in the app layer — this module only executes SQL against the
//! tables it fronts: `cameras`/`telescopes`/`filters` (auto-detect flag only),
//! `calibration_session`, `calibration_fingerprint`, `calibration_assignment`,
//! `calibration_master_view`, `acquisition_fingerprint`, `project_sources`.

use sqlx::SqlitePool;

use persistence_core::DbResult;

// ── Equipment auto-detect flag (spec 030) ──────────────────────────────────

/// Mark a camera row as auto-detected (`cameras.auto_detected = 1`).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn mark_camera_auto_detected(pool: &SqlitePool, id: &str) -> DbResult<()> {
    sqlx::query("UPDATE cameras SET auto_detected = 1 WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

/// Mark a telescope row as auto-detected (`telescopes.auto_detected = 1`).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn mark_telescope_auto_detected(pool: &SqlitePool, id: &str) -> DbResult<()> {
    sqlx::query("UPDATE telescopes SET auto_detected = 1 WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark a filter row as auto-detected (`filters.auto_detected = 1`).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn mark_filter_auto_detected(pool: &SqlitePool, id: &str) -> DbResult<()> {
    sqlx::query("UPDATE filters SET auto_detected = 1 WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

// ── Session / master fingerprint rows (spec 007, migration 0023) ──────────

/// Row from `acquisition_fingerprint`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AcquisitionFingerprintRow {
    pub id: String,
    pub session_type: Option<String>,
    pub gain: Option<f64>,
    pub offset_val: Option<f64>,
    pub exposure_s: Option<f64>,
    pub temp_c: Option<f64>,
    pub filter_name: Option<String>,
    pub rotation_deg: Option<f64>,
    pub binning: Option<String>,
    pub optic_train: Option<String>,
    pub observing_night_date: Option<String>,
    pub has_observer_location: Option<i64>,
    pub has_exposure_start_utc: Option<i64>,
}

/// Row from `calibration_fingerprint`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CalibrationFingerprintRow {
    pub id: String,
    pub calibration_type: String,
    pub gain: Option<f64>,
    pub offset_val: Option<f64>,
    pub exposure_s: Option<f64>,
    pub temp_c: Option<f64>,
    pub filter_name: Option<String>,
    pub rotation_deg: Option<f64>,
    pub binning: Option<String>,
    pub optic_train: Option<String>,
    pub source_session_id: Option<String>,
    pub observing_night_date: Option<String>,
}

/// Row from `calibration_master_view` (migration 0033, redefined by 0065 —
/// Q16 / FR-136 — to emit `NULL` size_bytes instead of a hardcoded `0`; no
/// size column exists on `calibration_session` yet).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CalibrationMasterViewRow {
    pub id: String,
    pub kind: String,
    pub created_at: String,
    pub size_bytes: Option<i64>,
    pub fp_gain: Option<f64>,
    pub fp_exposure_s: Option<f64>,
    pub fp_temp_c: Option<f64>,
    pub fp_filter_name: Option<String>,
    pub fp_binning: Option<String>,
    pub fp_optic_train: Option<String>,
    pub source_session_id: Option<String>,
    /// #642: the master's owning library root, `None` when the master frame
    /// was never resolved to a `file_record` (legacy/unresolved masters).
    pub root_id: Option<String>,
    /// #642: root-relative path of the master's own applied frame file
    /// (`calibration_session.frame_ids[0]` joined to `file_record`).
    pub frame_relative_path: Option<String>,
    /// #886: when this master was archived (plan-apply finalize step only —
    /// never set directly). `None` for an active master.
    pub archived_at: Option<String>,
    /// #886: the `calibration_master_archive` plan that archived this
    /// master, so archive-management/restore can act on it in O(1).
    pub archived_via_plan_id: Option<String>,
}

/// Whether an `acquisition_session` row exists for `session_id`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn acquisition_session_exists(pool: &SqlitePool, session_id: &str) -> DbResult<bool> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM acquisition_session WHERE id = ?")
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

/// Load the `acquisition_fingerprint` row for a session id, if present.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_acquisition_fingerprint(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Option<AcquisitionFingerprintRow>> {
    let row = sqlx::query_as::<_, AcquisitionFingerprintRow>(
        "
        SELECT id, session_type, gain, offset_val, exposure_s,
               temp_c, filter_name, rotation_deg, binning, optic_train,
               observing_night_date, has_observer_location, has_exposure_start_utc
        FROM acquisition_fingerprint
        WHERE id = ?
        ",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// List `acquisition_fingerprint` rows for `light` sessions (candidates for
/// a calibration master's "compatible sessions" list — #868).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_light_acquisition_fingerprints(
    pool: &SqlitePool,
) -> DbResult<Vec<AcquisitionFingerprintRow>> {
    let rows = sqlx::query_as::<_, AcquisitionFingerprintRow>(
        "
        SELECT id, session_type, gain, offset_val, exposure_s,
               temp_c, filter_name, rotation_deg, binning, optic_train,
               observing_night_date, has_observer_location, has_exposure_start_utc
        FROM acquisition_fingerprint
        WHERE session_type = 'light'
        ",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List `calibration_fingerprint` rows restricted to dark/flat/bias kinds.
///
/// Callers apply any requested-kind narrowing in-memory (mirrors prior
/// behavior: SQL restricts to the fixed dark/flat/bias set).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_calibration_fingerprints(
    pool: &SqlitePool,
) -> DbResult<Vec<CalibrationFingerprintRow>> {
    let rows = sqlx::query_as::<_, CalibrationFingerprintRow>(
        "
        SELECT id, calibration_type, gain, offset_val, exposure_s,
               temp_c, filter_name, rotation_deg, binning, optic_train,
               source_session_id, observing_night_date
        FROM calibration_fingerprint
        WHERE calibration_type IN ('dark', 'flat', 'bias')
        ",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Load a single `calibration_fingerprint` row by id.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_calibration_fingerprint(
    pool: &SqlitePool,
    master_id: &str,
) -> DbResult<Option<CalibrationFingerprintRow>> {
    let row = sqlx::query_as::<_, CalibrationFingerprintRow>(
        "
        SELECT id, calibration_type, gain, offset_val, exposure_s,
               temp_c, filter_name, rotation_deg, binning, optic_train,
               source_session_id, observing_night_date
        FROM calibration_fingerprint
        WHERE id = ?
        ",
    )
    .bind(master_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// ── Masters list / get (T037, FR-013) ──────────────────────────────────────

/// List all non-archived `calibration_master_view` rows, newest first.
///
/// #886: `archived_at IS NULL` excludes archived masters from the normal
/// Calibration page list (mirrors archived projects dropping off the
/// Projects list) — the filter lives here, not on the view itself, so
/// [`get_calibration_master`] can still resolve an archived master's detail.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_calibration_masters(
    pool: &SqlitePool,
) -> DbResult<Vec<CalibrationMasterViewRow>> {
    let rows = sqlx::query_as::<_, CalibrationMasterViewRow>(
        "SELECT id, kind, created_at, size_bytes,
                fp_gain, fp_exposure_s, fp_temp_c, fp_filter_name, fp_binning,
                fp_optic_train, source_session_id, root_id, frame_relative_path,
                archived_at, archived_via_plan_id
         FROM calibration_master_view
         WHERE archived_at IS NULL
         ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Load a single `calibration_master_view` row by id — including an archived
/// master, so its detail view / future unarchive can still see it.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_calibration_master(
    pool: &SqlitePool,
    master_id: &str,
) -> DbResult<Option<CalibrationMasterViewRow>> {
    let row = sqlx::query_as::<_, CalibrationMasterViewRow>(
        "SELECT id, kind, created_at, size_bytes,
                fp_gain, fp_exposure_s, fp_temp_c, fp_filter_name, fp_binning,
                fp_optic_train, source_session_id, root_id, frame_relative_path,
                archived_at, archived_via_plan_id
         FROM calibration_master_view
         WHERE id = ?",
    )
    .bind(master_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// List `session_id`s from `calibration_assignment` rows assigned to a master.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_assignment_session_ids(
    pool: &SqlitePool,
    master_id: &str,
) -> DbResult<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT session_id FROM calibration_assignment WHERE master_id = ?")
            .bind(master_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

/// List distinct `project_id`s linked (via `project_sources`) to sessions
/// assigned a given calibration master.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_assignment_project_ids(
    pool: &SqlitePool,
    master_id: &str,
) -> DbResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT ps.project_id
         FROM project_sources ps
         JOIN calibration_assignment ca ON ca.session_id = ps.session_id
         WHERE ca.master_id = ?",
    )
    .bind(master_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(p,)| p).collect())
}

// ── Master archive (#886) ───────────────────────────────────────────────────

/// Mark a `calibration_session` (master) archived, linking the owning plan.
/// Called only from the plan-apply finalize step
/// (`crate::calibration_archive_generator`) on a successfully applied
/// `calibration_master_archive` plan — never by direct UI action
/// (Constitution II: reviewable-plan discipline).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn set_master_archived(
    pool: &SqlitePool,
    master_id: &str,
    plan_id: &str,
    archived_at: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE calibration_session SET archived_at = ?, archived_via_plan_id = ? WHERE id = ?",
    )
    .bind(archived_at)
    .bind(plan_id)
    .bind(master_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Clear the archived flag on a restored (un-archived) master. Counterpart
/// to [`set_master_archived`]; called once a `calibration_master_restore`
/// plan finishes applying.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn clear_master_archived(pool: &SqlitePool, master_id: &str) -> DbResult<()> {
    sqlx::query(
        "UPDATE calibration_session SET archived_at = NULL, archived_via_plan_id = NULL \
         WHERE id = ?",
    )
    .bind(master_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// One archived-master row for the Archive surface (#886), joined with the
/// owning archive plan for the display reason + bytes moved — mirrors
/// `crate::repositories::projects::ArchivedProjectRow`.
#[derive(Debug, Clone)]
pub struct ArchivedMasterRow {
    pub id: String,
    pub kind: String,
    pub root_id: Option<String>,
    pub frame_relative_path: Option<String>,
    pub archived_at: String,
    pub archived_via_plan_id: Option<String>,
    pub plan_title: Option<String>,
    pub archived_bytes: Option<i64>,
}

/// List every `calibration_master_view` row currently archived.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_archived_masters(pool: &SqlitePool) -> DbResult<Vec<ArchivedMasterRow>> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
    )> = sqlx::query_as(
        "SELECT m.id, m.kind, m.root_id, m.frame_relative_path, m.archived_at, \
                m.archived_via_plan_id, pl.title, pl.total_bytes_required \
         FROM calibration_master_view m \
         LEFT JOIN plans pl ON pl.id = m.archived_via_plan_id \
         WHERE m.archived_at IS NOT NULL \
         ORDER BY m.archived_at DESC, m.id ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                kind,
                root_id,
                frame_relative_path,
                archived_at,
                archived_via_plan_id,
                plan_title,
                archived_bytes,
            )| ArchivedMasterRow {
                id,
                kind,
                root_id,
                frame_relative_path,
                // NOT NULL by the WHERE clause; the column stays Option at the
                // SQL layer since `calibration_master_view` is a plain LEFT
                // JOIN projection with no NOT NULL guarantee sqlx can see.
                archived_at: archived_at.unwrap_or_default(),
                archived_via_plan_id,
                plan_title,
                archived_bytes,
            },
        )
        .collect())
}
