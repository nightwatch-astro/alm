// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Calibration tolerance contract DTOs for the Tauri IPC surface.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationTolerances {
    pub temperature_tolerance_c: f64,
    pub exposure_tolerance_s: f64,
    pub aging_limit_days: i32,
    pub require_same_gain: bool,
    pub require_same_binning: bool,
    /// Hard rule: master must carry the same OFFSET as the light session for
    /// dark/bias matching. Feeds `calibration_core::ranking::MatchingRuleConfig
    /// ::require_same_offset` (spec 007). Default `true` (see migration 0051).
    pub require_same_offset: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCalibrationTolerances {
    pub temperature_tolerance_c: f64,
    pub exposure_tolerance_s: f64,
    pub aging_limit_days: i32,
    pub require_same_gain: bool,
    pub require_same_binning: bool,
    pub require_same_offset: bool,
}
