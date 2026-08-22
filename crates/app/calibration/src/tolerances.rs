// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Calibration matching tolerances use cases (spec 007 / spec 043 P8).
//!
//! Bridges the `calibration_tolerances` singleton row (migration 0008 + 0051,
//! `persistence_calibration::repositories::calibration_tolerances`) with the
//! `CalibrationTolerances` / `UpdateCalibrationTolerances` contract DTOs used
//! by the Tauri `calibration.tolerances.get`/`update` commands.
//!
//! `require_same_offset` additionally feeds
//! `calibration_core::ranking::MatchingRuleConfig::require_same_offset` — see
//! `load_config` in `matching.rs`, which reads this same table.

use contracts_core::calibration_tolerances::{CalibrationTolerances, UpdateCalibrationTolerances};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use persistence_calibration::repositories::calibration_tolerances::{
    self as repo, CalibrationTolerancesRow,
};
use sqlx::SqlitePool;

fn db_to_contract(e: persistence_core::DbError) -> ContractError {
    let msg = e.to_string();
    drop(e);
    ContractError::new(ErrorCode::InternalDatabase, msg, ErrorSeverity::Fatal, true)
}

fn row_to_dto(row: CalibrationTolerancesRow) -> CalibrationTolerances {
    CalibrationTolerances {
        temperature_tolerance_c: row.temperature_tolerance_c,
        exposure_tolerance_s: row.exposure_tolerance_s,
        // The table stores a plain i64 with no enforced range; the DTO uses
        // i32 (spec 007). Saturate rather than panic on an out-of-range value.
        aging_limit_days: i32::try_from(row.aging_limit_days).unwrap_or(i32::MAX),
        require_same_gain: row.require_same_gain,
        require_same_binning: row.require_same_binning,
        require_same_offset: row.require_same_offset,
    }
}

/// `calibration.tolerances.get` — read the persisted matching tolerances.
///
/// # Errors
///
/// Returns `ContractError` on database failure.
pub async fn tolerances_get(pool: &SqlitePool) -> Result<CalibrationTolerances, ContractError> {
    repo::get(pool).await.map(row_to_dto).map_err(db_to_contract)
}

/// `calibration.tolerances.update` — persist new matching tolerances.
///
/// # Errors
///
/// Returns `ContractError` on database failure.
pub async fn tolerances_update(
    pool: &SqlitePool,
    request: UpdateCalibrationTolerances,
) -> Result<CalibrationTolerances, ContractError> {
    let row = CalibrationTolerancesRow {
        temperature_tolerance_c: request.temperature_tolerance_c,
        exposure_tolerance_s: request.exposure_tolerance_s,
        aging_limit_days: i64::from(request.aging_limit_days),
        require_same_gain: request.require_same_gain,
        require_same_binning: request.require_same_binning,
        require_same_offset: request.require_same_offset,
    };
    repo::update(pool, &row).await.map(row_to_dto).map_err(db_to_contract)
}
