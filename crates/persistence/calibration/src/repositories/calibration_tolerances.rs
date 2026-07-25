// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for the calibration matching tolerances singleton row
//! (spec 007 / spec 043 P8, migration 0008 + 0051).
//!
//! `calibration_tolerances` holds exactly one row (`singleton_id = 'default'`)
//! seeded unconditionally by migration 0008, so `get` can always `fetch_one`.

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::DbResult;

/// Raw persisted row from `calibration_tolerances`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // Distinct orthogonal per-field match-required flags
pub struct CalibrationTolerancesRow {
    pub temperature_tolerance_c: f64,
    pub exposure_tolerance_s: f64,
    pub aging_limit_days: i64,
    pub require_same_camera: bool,
    pub require_same_gain: bool,
    pub require_same_binning: bool,
    /// Hard rule: master must carry the same OFFSET as the light session
    /// (migration 0051). Feeds `MatchingRuleConfig::require_same_offset`.
    pub require_same_offset: bool,
}

/// Load the calibration tolerances singleton row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure (including the unexpected
/// case where the migration-0008 seed row is missing).
pub async fn get(pool: &SqlitePool) -> DbResult<CalibrationTolerancesRow> {
    let row: (f64, f64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT temperature_tolerance_c, exposure_tolerance_s, aging_limit_days, \
                require_same_camera, require_same_gain, require_same_binning, \
                require_same_offset \
         FROM calibration_tolerances WHERE singleton_id = 'default'",
    )
    .fetch_one(pool)
    .await?;

    let (
        temperature_tolerance_c,
        exposure_tolerance_s,
        aging_limit_days,
        require_same_camera,
        require_same_gain,
        require_same_binning,
        require_same_offset,
    ) = row;

    Ok(CalibrationTolerancesRow {
        temperature_tolerance_c,
        exposure_tolerance_s,
        aging_limit_days,
        require_same_camera: require_same_camera != 0,
        require_same_gain: require_same_gain != 0,
        require_same_binning: require_same_binning != 0,
        require_same_offset: require_same_offset != 0,
    })
}

/// Upsert the full tolerances row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn update(
    pool: &SqlitePool,
    row: &CalibrationTolerancesRow,
) -> DbResult<CalibrationTolerancesRow> {
    let now = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO calibration_tolerances \
         (singleton_id, temperature_tolerance_c, exposure_tolerance_s, aging_limit_days, \
          require_same_camera, require_same_gain, require_same_binning, require_same_offset, \
          updated_at) \
         VALUES ('default', ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(singleton_id) DO UPDATE SET \
             temperature_tolerance_c = excluded.temperature_tolerance_c, \
             exposure_tolerance_s    = excluded.exposure_tolerance_s, \
             aging_limit_days        = excluded.aging_limit_days, \
             require_same_camera     = excluded.require_same_camera, \
             require_same_gain       = excluded.require_same_gain, \
             require_same_binning    = excluded.require_same_binning, \
             require_same_offset     = excluded.require_same_offset, \
             updated_at              = excluded.updated_at",
    )
    .bind(row.temperature_tolerance_c)
    .bind(row.exposure_tolerance_s)
    .bind(row.aging_limit_days)
    .bind(i64::from(row.require_same_camera))
    .bind(i64::from(row.require_same_gain))
    .bind(i64::from(row.require_same_binning))
    .bind(i64::from(row.require_same_offset))
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(*row)
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

    #[tokio::test]
    async fn get_returns_seeded_defaults() {
        let pool = setup().await;
        let row = get(&pool).await.unwrap();
        assert!((row.temperature_tolerance_c - 5.0).abs() < f64::EPSILON);
        assert!((row.exposure_tolerance_s - 2.0).abs() < f64::EPSILON);
        assert_eq!(row.aging_limit_days, 365);
        assert!(row.require_same_camera);
        assert!(row.require_same_gain);
        assert!(row.require_same_binning);
        assert!(row.require_same_offset, "migration 0051 default must be true");
    }

    #[tokio::test]
    async fn update_and_get_roundtrip() {
        let pool = setup().await;
        let new_row = CalibrationTolerancesRow {
            temperature_tolerance_c: 3.5,
            exposure_tolerance_s: 1.0,
            aging_limit_days: 90,
            require_same_camera: false,
            require_same_gain: true,
            require_same_binning: false,
            require_same_offset: false,
        };
        update(&pool, &new_row).await.unwrap();

        let loaded = get(&pool).await.unwrap();
        assert_eq!(loaded, new_row);
    }

    #[tokio::test]
    async fn update_is_idempotent_upsert() {
        let pool = setup().await;
        let row_a = CalibrationTolerancesRow {
            temperature_tolerance_c: 4.0,
            exposure_tolerance_s: 1.5,
            aging_limit_days: 180,
            require_same_camera: true,
            require_same_gain: false,
            require_same_binning: true,
            require_same_offset: false,
        };
        update(&pool, &row_a).await.unwrap();
        let row_b = CalibrationTolerancesRow { require_same_offset: true, ..row_a };
        update(&pool, &row_b).await.unwrap();

        let loaded = get(&pool).await.unwrap();
        assert_eq!(loaded, row_b);
    }
}
