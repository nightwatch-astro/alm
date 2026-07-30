// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Fingerprint-row seeding for calibration matching fixtures.
//!
//! Public so sibling crates and the `perf-bench` harness can build a
//! populated matcher fixture without a raw sqlx call outside
//! `crates/persistence/` (see `scripts/check-db-boundary.sh`). The
//! production write path for these tables is the metadata extraction
//! pipeline; nothing here is reachable from the application.

use persistence_core::DbResult;
use sqlx::SqlitePool;

/// Fingerprint dimensions shared by light sessions and calibration masters.
///
/// Every dimension is `Option` because the matcher's hard/soft rules
/// distinguish "absent" from "present and different" (see
/// `calibration_core::rules`).
#[derive(Clone, Debug, Default)]
pub struct SeedFingerprint<'a> {
    pub gain: Option<f64>,
    pub offset_val: Option<f64>,
    pub exposure_s: Option<f64>,
    pub temp_c: Option<f64>,
    pub filter_name: Option<&'a str>,
    pub binning: Option<&'a str>,
    pub optic_train: Option<&'a str>,
    pub observing_night_date: Option<&'a str>,
}

/// Insert an `acquisition_session` + `acquisition_fingerprint` pair for a
/// light session.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn seed_light_session(
    pool: &SqlitePool,
    session_id: &str,
    fp: &SeedFingerprint<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, created_at) \
         VALUES (?, ?, datetime('now'))",
    )
    .bind(session_id)
    .bind(session_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO acquisition_fingerprint \
         (id, session_type, gain, offset_val, exposure_s, temp_c, filter_name, binning, \
          optic_train, observing_night_date, has_observer_location, has_exposure_start_utc) \
         VALUES (?, 'light', ?, ?, ?, ?, ?, ?, ?, ?, 1, 1)",
    )
    .bind(session_id)
    .bind(fp.gain)
    .bind(fp.offset_val)
    .bind(fp.exposure_s)
    .bind(fp.temp_c)
    .bind(fp.filter_name)
    .bind(fp.binning)
    .bind(fp.optic_train)
    .bind(fp.observing_night_date)
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert a `calibration_session` + `calibration_fingerprint` pair for one
/// master.
///
/// `kind` must be one of `dark`, `flat`, `bias` (the `calibration_fingerprint`
/// CHECK constraint); `calibration_session` additionally accepts `flat_dark`,
/// which has no fingerprint representation.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn seed_calibration_master(
    pool: &SqlitePool,
    master_id: &str,
    kind: &str,
    fp: &SeedFingerprint<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES (?, ?, ?, datetime('now'))",
    )
    .bind(master_id)
    .bind(master_id)
    .bind(kind)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO calibration_fingerprint \
         (id, calibration_type, gain, offset_val, exposure_s, temp_c, filter_name, binning, \
          optic_train, observing_night_date) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(master_id)
    .bind(kind)
    .bind(fp.gain)
    .bind(fp.offset_val)
    .bind(fp.exposure_s)
    .bind(fp.temp_c)
    .bind(fp.filter_name)
    .bind(fp.binning)
    .bind(fp.optic_train)
    .bind(fp.observing_night_date)
    .execute(pool)
    .await?;

    Ok(())
}
