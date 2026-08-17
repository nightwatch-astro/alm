// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Candidate evidence evaluation for external-processor handoff.
//!
//! Converts persistence layer rows and domain types into
//! [`CandidateEvidence`] projections used by the `calibration.candidate.list`
//! query and snapshot creation.
//!
//! All evidence is evaluated at a captured `evaluation_at` instant — the
//! caller supplies a pre-captured `Utc` timestamp; this module does not
//! access a clock.

use calibration_core::families::{
    automatic_eligibility, dark_bias_age_state, dark_thermal_state_from_p95, flat_age_state,
    flat_orientation_state, AgeState, AutomaticEligibility, EligibilityInput, OrientationState,
    RecipeCompatibility, ThermalState,
};
use calibration_core::CalibrationKind;

use persistence_sessions::repositories::calibration_sessions::availability::SourceAvailabilityRollupRow;
use persistence_sessions::repositories::calibration_sessions::sessions::DarkThermalEvidenceRow;

// ── Evidence projection ───────────────────────────────────────────────────────

/// Evaluated candidate evidence for one session × requirement pair.
///
/// This struct is the domain projection consumed by the handoff snapshot
/// builder and by the `calibration.candidate.list` query response mapper.
/// Fields mirror the `CalibrationCandidateEvidence` contract DTO.
#[derive(Debug, Clone)]
pub struct CandidateEvidence {
    /// Unique public evidence identity (UUIDv7 assigned before DB insert).
    pub evidence_public_id: String,
    pub session_public_id: String,
    pub requirement_public_id: String,
    pub kind: CalibrationKind,
    pub recipe_compatibility: RecipeCompatibility,
    pub recipe_evidence_complete: bool,
    pub missing_recipe_fields: Vec<String>,
    pub temperature_mode_str: String,
    pub age: AgeEvidence,
    pub thermal: ThermalEvidence,
    pub orientation: OrientationEvidence,
    pub source_availability: SourceAvailabilityEvidence,
    /// `sufficient`: required evidence complete AND at least one readable frame.
    pub sufficient: bool,
    pub automatic_eligibility: AutomaticEligibility,
    pub warning_codes: Vec<String>,
    /// SHA-256 over a deterministic serialization of the evidence inputs.
    pub basis_fingerprint: String,
}

/// Age evidence for a dark/bias (elapsed days) or flat (observing-night
/// distance).
#[derive(Debug, Clone)]
pub struct AgeEvidence {
    /// `"elapsed_days"` for dark/bias, `"observing_night_distance"` for flat.
    pub basis: String,
    pub state: AgeState,
    /// Elapsed days (dark/bias) or night distance (flat). `None` when unknown.
    pub age_value: Option<u32>,
    pub fresh_threshold: u32,
    pub red_threshold: u32,
    /// Settings revision that supplied the thresholds.
    pub settings_revision: u64,
}

/// Thermal deviation evidence for regulated dark sessions.
#[derive(Debug, Clone)]
pub struct ThermalEvidence {
    pub state: ThermalState,
    /// `Some` when valid readings were available.
    pub valid_reading_percent: Option<f64>,
    pub minimum_deviation_deg: Option<f64>,
    pub median_deviation_deg: Option<f64>,
    pub maximum_deviation_deg: Option<f64>,
    pub percentile95_deviation_deg: Option<f64>,
    pub missing_reading_count: u32,
    pub invalid_reading_count: u32,
    /// `Some` only for regulated dark; `None` for all others.
    pub settings_revision: Option<u64>,
}

/// Physical-orientation evidence for flat sessions.
#[derive(Debug, Clone)]
pub struct OrientationEvidence {
    pub state: OrientationState,
    pub minimum_circular_delta_deg: Option<f64>,
    pub normal_through_deg: Option<f64>,
    pub red_above_deg: Option<f64>,
    pub settings_revision: Option<u64>,
}

/// Frame source availability at evaluation time.
#[derive(Debug, Clone)]
pub struct SourceAvailabilityEvidence {
    pub indexed_frame_count: u32,
    pub available_readable_indexed_frame_count: u32,
    pub checked_at: String,
}

// ── Input structs ─────────────────────────────────────────────────────────────

/// Thresholds from `matching_settings_revision` used by this evaluator.
#[derive(Debug, Clone)]
pub struct MatchingSettings {
    pub revision_number: u64,
    /// Dark/bias fresh-through days (default 270, FR-070).
    pub dark_bias_fresh_days: u32,
    /// Dark/bias red-after days (default 365, FR-070).
    pub dark_bias_red_days: u32,
    /// Flat red-after nights (default 7, FR-074).
    pub flat_red_nights: u32,
    /// Dark thermal moderate threshold milli-Celsius (default 500, FR-088).
    pub dark_thermal_moderate_millic: i32,
    /// Dark thermal severe threshold milli-Celsius (default 2000, FR-088).
    pub dark_thermal_severe_millic: i32,
    /// Flat orientation normal threshold micro-degrees (default 2_000_000, FR-073).
    pub flat_orientation_normal_udeg: u32,
    /// Flat orientation red threshold micro-degrees (default 5_000_000, FR-073).
    pub flat_orientation_red_udeg: u32,
}

/// Per-camera-kind age overrides (from `matching_settings_camera_policy`).
#[derive(Debug, Clone)]
pub struct CameraKindAgePolicy {
    pub fresh_age_days: u32,
    pub red_age_days: u32,
}

/// Physical rotator state of the flat candidate session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalRotatorStateInput {
    Verified,
    Absent,
    Unverified,
}

/// All inputs needed to evaluate one candidate evidence record.
pub struct EvaluateEvidenceInput<'a> {
    pub evidence_public_id: &'a str,
    pub session_public_id: &'a str,
    pub session_row_id: i64,
    pub requirement_public_id: &'a str,
    pub kind: CalibrationKind,
    pub recipe_compatibility: RecipeCompatibility,
    pub required_evidence_complete: bool,
    pub candidate_evidence_complete: bool,
    pub missing_recipe_fields: Vec<String>,
    /// Temperature mode string: `"regulated"` / `"unregulated"` / `"unknown"` / `"not_applicable"`.
    pub temperature_mode_str: String,
    /// Age in elapsed calendar days (dark/bias) or observing-night distance (flat).
    pub age_value: Option<u32>,
    /// For flat: true when the candidate night equals the requirement night.
    pub is_same_night_flat: bool,
    /// Dark thermal evidence row; `None` for non-dark or unregulated.
    pub thermal_evidence: Option<&'a DarkThermalEvidenceRow>,
    /// Physical rotator state for flat orientation evaluation.
    pub flat_rotator_state: Option<PhysicalRotatorStateInput>,
    /// Circular delta in micro-degrees between candidate and requirement flat angles.
    /// `None` when either is absent or unverified.
    pub flat_rotator_delta_udeg: Option<u32>,
    /// Source availability rollup; `None` means zero indexed frames.
    pub availability: Option<&'a SourceAvailabilityRollupRow>,
    /// UTC RFC 3339 string; already captured by the caller at `evaluation_at`.
    pub observed_at: &'a str,
    pub settings: &'a MatchingSettings,
    pub camera_kind_policy: Option<&'a CameraKindAgePolicy>,
    /// Deterministic evidence digest; callers compute via canonical serialization.
    pub basis_fingerprint: String,
}

// ── Evaluation logic ──────────────────────────────────────────────────────────

/// Evaluate full candidate evidence for one session × requirement pair.
///
/// Does not access the database or the filesystem. All inputs are already
/// resolved by the caller before this function runs.
#[must_use]
#[allow(
    clippy::too_many_lines,        // evaluation logic necessarily covers all evidence branches
    clippy::cast_possible_truncation, // DB row i64 values are schema-constrained to fit target types
    clippy::cast_sign_loss,        // DB row values are non-negative by schema constraints
    clippy::cast_precision_loss,   // frame counts and percentages don't need full i64 precision
)]
#[expect(
    clippy::cognitive_complexity,
    reason = "spec-transcribed evidence scoring across age, count, and equipment dimensions"
)]
pub fn evaluate_candidate_evidence(input: &EvaluateEvidenceInput<'_>) -> CandidateEvidence {
    // ── Age ──────────────────────────────────────────────────────────────────
    let (fresh_threshold, red_threshold, age_basis) = if input.kind == CalibrationKind::Flat {
        (
            calibration_core::families::FLAT_FRESH_NIGHTS_DEFAULT,
            input.settings.flat_red_nights,
            "observing_night_distance",
        )
    } else {
        let (f, r) = match input.camera_kind_policy {
            Some(p) => (p.fresh_age_days, p.red_age_days),
            None => (input.settings.dark_bias_fresh_days, input.settings.dark_bias_red_days),
        };
        (f, r, "elapsed_days")
    };

    let age_state = match input.age_value {
        None => AgeState::Unknown,
        Some(v) => match input.kind {
            CalibrationKind::Flat => flat_age_state(v, red_threshold),
            _ => dark_bias_age_state(v, fresh_threshold, red_threshold),
        },
    };

    let age = AgeEvidence {
        basis: age_basis.to_owned(),
        state: age_state,
        age_value: input.age_value,
        fresh_threshold,
        red_threshold,
        settings_revision: input.settings.revision_number,
    };

    // ── Thermal ──────────────────────────────────────────────────────────────
    let thermal = match input.kind {
        CalibrationKind::Dark => match input.temperature_mode_str.as_str() {
            "regulated" => {
                if let Some(ev) = input.thermal_evidence {
                    let state = dark_thermal_state_from_p95(
                        ev.p95_abs_deviation_millic.map(|v| v as i32),
                        ev.valid_ratio_ppm as u32,
                        input.settings.dark_thermal_moderate_millic,
                        input.settings.dark_thermal_severe_millic,
                    );
                    let valid_pct = if ev.valid_count + ev.missing_count + ev.invalid_count > 0 {
                        let total = (ev.valid_count + ev.missing_count + ev.invalid_count) as f64;
                        Some(ev.valid_count as f64 / total * 100.0)
                    } else {
                        None
                    };
                    let millic_to_deg = |v: Option<i64>| v.map(|x| x as f64 / 1000.0);
                    ThermalEvidence {
                        state,
                        valid_reading_percent: valid_pct,
                        minimum_deviation_deg: millic_to_deg(ev.minimum_abs_deviation_millic),
                        median_deviation_deg: millic_to_deg(ev.median_abs_deviation_millic),
                        maximum_deviation_deg: millic_to_deg(ev.maximum_abs_deviation_millic),
                        percentile95_deviation_deg: millic_to_deg(ev.p95_abs_deviation_millic),
                        missing_reading_count: ev.missing_count as u32,
                        invalid_reading_count: ev.invalid_count as u32,
                        settings_revision: Some(input.settings.revision_number),
                    }
                } else {
                    ThermalEvidence {
                        state: ThermalState::Unknown,
                        valid_reading_percent: None,
                        minimum_deviation_deg: None,
                        median_deviation_deg: None,
                        maximum_deviation_deg: None,
                        percentile95_deviation_deg: None,
                        missing_reading_count: 0,
                        invalid_reading_count: 0,
                        settings_revision: Some(input.settings.revision_number),
                    }
                }
            }
            "unregulated" => not_applicable_thermal(),
            _ => unknown_thermal(Some(input.settings.revision_number)),
        },
        _ => not_applicable_thermal(),
    };

    // ── Orientation ───────────────────────────────────────────────────────────
    let orientation = match input.kind {
        CalibrationKind::Flat => {
            let delta = match input.flat_rotator_state {
                Some(PhysicalRotatorStateInput::Verified) => input.flat_rotator_delta_udeg,
                _ => None,
            };
            let state = flat_orientation_state(
                delta,
                input.settings.flat_orientation_normal_udeg,
                input.settings.flat_orientation_red_udeg,
            );
            OrientationEvidence {
                state,
                minimum_circular_delta_deg: delta.map(|d| f64::from(d) / 1_000_000.0),
                normal_through_deg: Some(
                    f64::from(input.settings.flat_orientation_normal_udeg) / 1_000_000.0,
                ),
                red_above_deg: Some(
                    f64::from(input.settings.flat_orientation_red_udeg) / 1_000_000.0,
                ),
                settings_revision: Some(input.settings.revision_number),
            }
        }
        _ => OrientationEvidence {
            state: OrientationState::NotApplicable,
            minimum_circular_delta_deg: None,
            normal_through_deg: None,
            red_above_deg: None,
            settings_revision: None,
        },
    };

    // ── Source availability ───────────────────────────────────────────────────
    let (indexed, readable, checked_at) = match input.availability {
        Some(av) => {
            (av.indexed_frame_count as u32, av.readable_frame_count as u32, av.observed_at.clone())
        }
        None => (0u32, 0u32, input.observed_at.to_owned()),
    };
    let source_availability = SourceAvailabilityEvidence {
        indexed_frame_count: indexed,
        available_readable_indexed_frame_count: readable,
        checked_at,
    };

    let sufficient = input.required_evidence_complete && readable >= 1;

    // ── Temperature mode (for eligibility) ────────────────────────────────────
    let temperature_mode = match input.temperature_mode_str.as_str() {
        "regulated" => calibration_core::families::TemperatureMode::Regulated,
        "unregulated" => calibration_core::families::TemperatureMode::Unregulated,
        "not_applicable" => calibration_core::families::TemperatureMode::NotApplicable,
        _ => calibration_core::families::TemperatureMode::Unknown,
    };

    // ── Eligibility ────────────────────────────────────────────────────────────
    let eligibility_input = EligibilityInput {
        kind: input.kind,
        recipe_compatibility: input.recipe_compatibility,
        required_evidence_complete: input.required_evidence_complete,
        candidate_evidence_complete: input.candidate_evidence_complete,
        sufficient,
        age_state,
        thermal_state: thermal.state,
        temperature_mode,
        orientation_state: orientation.state,
        is_same_night_flat: input.is_same_night_flat,
    };
    let auto_eligibility = automatic_eligibility(&eligibility_input);

    // ── Warning codes ─────────────────────────────────────────────────────────
    let mut warning_codes: Vec<String> = Vec::new();

    // Missing recipe fields
    for f in &input.missing_recipe_fields {
        warning_codes.push(format!("calibration.missing_recipe_field:{f}"));
    }
    // Thermal warnings
    if matches!(thermal.state, ThermalState::Yellow) {
        warning_codes.push("calibration.thermal_yellow".to_owned());
    }
    if matches!(thermal.state, ThermalState::Red) {
        warning_codes.push("calibration.thermal_red".to_owned());
    }
    // Orientation warnings
    if matches!(orientation.state, OrientationState::Unknown) && input.kind == CalibrationKind::Flat
    {
        warning_codes.push("calibration.orientation_compatibility_unverified".to_owned());
    }
    if matches!(orientation.state, OrientationState::Yellow) {
        warning_codes.push("calibration.orientation_yellow".to_owned());
    }
    if matches!(orientation.state, OrientationState::Red) {
        warning_codes.push("calibration.orientation_red".to_owned());
    }
    // Age warnings
    if matches!(age_state, AgeState::Yellow) {
        warning_codes.push("calibration.age_yellow".to_owned());
    }
    if matches!(age_state, AgeState::Red) {
        warning_codes.push("calibration.age_red".to_owned());
    }
    // Cross-night flat
    if input.kind == CalibrationKind::Flat && !input.is_same_night_flat {
        warning_codes.push("calibration.flat_cross_night".to_owned());
    }
    // Unregulated dark
    if input.kind == CalibrationKind::Dark
        && matches!(temperature_mode, calibration_core::families::TemperatureMode::Unregulated)
    {
        warning_codes.push("calibration.dark_unregulated".to_owned());
    }

    CandidateEvidence {
        evidence_public_id: input.evidence_public_id.to_owned(),
        session_public_id: input.session_public_id.to_owned(),
        requirement_public_id: input.requirement_public_id.to_owned(),
        kind: input.kind,
        recipe_compatibility: input.recipe_compatibility,
        recipe_evidence_complete: input.candidate_evidence_complete,
        missing_recipe_fields: input.missing_recipe_fields.clone(),
        temperature_mode_str: input.temperature_mode_str.clone(),
        age,
        thermal,
        orientation,
        source_availability,
        sufficient,
        automatic_eligibility: auto_eligibility,
        warning_codes,
        basis_fingerprint: input.basis_fingerprint.clone(),
    }
}

fn not_applicable_thermal() -> ThermalEvidence {
    ThermalEvidence {
        state: ThermalState::NotApplicable,
        valid_reading_percent: None,
        minimum_deviation_deg: None,
        median_deviation_deg: None,
        maximum_deviation_deg: None,
        percentile95_deviation_deg: None,
        missing_reading_count: 0,
        invalid_reading_count: 0,
        settings_revision: None,
    }
}

fn unknown_thermal(settings_revision: Option<u64>) -> ThermalEvidence {
    ThermalEvidence {
        state: ThermalState::Unknown,
        valid_reading_percent: None,
        minimum_deviation_deg: None,
        median_deviation_deg: None,
        maximum_deviation_deg: None,
        percentile95_deviation_deg: None,
        missing_reading_count: 0,
        invalid_reading_count: 0,
        settings_revision,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use calibration_core::families::{
        DARK_BIAS_FRESH_DAYS_DEFAULT, DARK_BIAS_RED_DAYS_DEFAULT,
        DARK_THERMAL_MODERATE_MILLIC_DEFAULT, DARK_THERMAL_SEVERE_MILLIC_DEFAULT,
        FLAT_ORIENTATION_NORMAL_UDEG_DEFAULT, FLAT_ORIENTATION_RED_UDEG_DEFAULT,
        FLAT_RED_NIGHTS_DEFAULT,
    };

    fn default_settings() -> MatchingSettings {
        MatchingSettings {
            revision_number: 1,
            dark_bias_fresh_days: DARK_BIAS_FRESH_DAYS_DEFAULT,
            dark_bias_red_days: DARK_BIAS_RED_DAYS_DEFAULT,
            flat_red_nights: FLAT_RED_NIGHTS_DEFAULT,
            dark_thermal_moderate_millic: DARK_THERMAL_MODERATE_MILLIC_DEFAULT,
            dark_thermal_severe_millic: DARK_THERMAL_SEVERE_MILLIC_DEFAULT,
            flat_orientation_normal_udeg: FLAT_ORIENTATION_NORMAL_UDEG_DEFAULT,
            flat_orientation_red_udeg: FLAT_ORIENTATION_RED_UDEG_DEFAULT,
        }
    }

    fn fresh_dark_input(settings: &MatchingSettings) -> EvaluateEvidenceInput<'_> {
        EvaluateEvidenceInput {
            evidence_public_id: "ev-001",
            session_public_id: "ses-001",
            session_row_id: 1,
            requirement_public_id: "req-001",
            kind: CalibrationKind::Dark,
            recipe_compatibility: RecipeCompatibility::Compatible,
            required_evidence_complete: true,
            candidate_evidence_complete: true,
            missing_recipe_fields: vec![],
            temperature_mode_str: "regulated".to_owned(),
            age_value: Some(100),
            is_same_night_flat: false,
            thermal_evidence: None,
            flat_rotator_state: None,
            flat_rotator_delta_udeg: None,
            availability: None,
            observed_at: "2026-07-22T00:00:00.000000Z",
            settings,
            camera_kind_policy: None,
            basis_fingerprint: "fingerprint-001".to_owned(),
        }
    }

    #[test]
    fn fresh_dark_with_no_availability_is_blocked() {
        let settings = default_settings();
        let input = fresh_dark_input(&settings);
        let ev = evaluate_candidate_evidence(&input);
        // No availability → sufficient = false → blocked
        assert!(!ev.sufficient);
        assert_eq!(ev.automatic_eligibility, AutomaticEligibility::Blocked);
        assert_eq!(ev.age.state, AgeState::Fresh);
    }

    #[test]
    fn fresh_dark_with_availability_and_thermal_is_eligible() {
        let settings = default_settings();
        let avail = SourceAvailabilityRollupRow {
            session_row_id: 1,
            indexed_frame_count: 100,
            available_frame_count: 100,
            readable_frame_count: 100,
            source_byte_count: 1_000_000,
            observed_at: "2026-07-22T00:00:00.000000Z".to_owned(),
        };
        let thermal = DarkThermalEvidenceRow {
            session_row_id: 1,
            valid_count: 100,
            missing_count: 0,
            invalid_count: 0,
            minimum_abs_deviation_millic: Some(100),
            median_abs_deviation_millic: Some(200),
            maximum_abs_deviation_millic: Some(400),
            p95_abs_deviation_millic: Some(350),
            valid_ratio_ppm: 1_000_000,
            severity: "normal".to_owned(),
            created_sequence: 1,
        };
        let mut input = fresh_dark_input(&settings);
        input.availability = Some(&avail);
        input.thermal_evidence = Some(&thermal);
        let ev = evaluate_candidate_evidence(&input);
        assert!(ev.sufficient);
        assert_eq!(ev.automatic_eligibility, AutomaticEligibility::Eligible);
        assert_eq!(ev.thermal.state, ThermalState::Normal);
        assert!(ev.warning_codes.is_empty());
    }

    #[test]
    fn red_age_dark_yields_review_required() {
        let avail = SourceAvailabilityRollupRow {
            session_row_id: 1,
            indexed_frame_count: 10,
            available_frame_count: 10,
            readable_frame_count: 10,
            source_byte_count: 100_000,
            observed_at: "2026-07-22T00:00:00.000000Z".to_owned(),
        };
        let thermal = DarkThermalEvidenceRow {
            session_row_id: 1,
            valid_count: 100,
            missing_count: 0,
            invalid_count: 0,
            minimum_abs_deviation_millic: Some(100),
            median_abs_deviation_millic: Some(200),
            maximum_abs_deviation_millic: Some(300),
            p95_abs_deviation_millic: Some(250),
            valid_ratio_ppm: 1_000_000,
            severity: "normal".to_owned(),
            created_sequence: 1,
        };
        let settings_red = MatchingSettings {
            dark_bias_fresh_days: 10,
            dark_bias_red_days: 20,
            ..default_settings()
        };
        let mut input = fresh_dark_input(&settings_red);
        input.age_value = Some(400); // red age
        input.availability = Some(&avail);
        input.thermal_evidence = Some(&thermal);
        let ev = evaluate_candidate_evidence(&input);
        assert_eq!(ev.age.state, AgeState::Red);
        assert_eq!(ev.automatic_eligibility, AutomaticEligibility::ReviewRequired);
        assert!(ev.warning_codes.contains(&"calibration.age_red".to_owned()));
    }

    #[test]
    fn same_night_flat_eligible_with_normal_orientation() {
        let settings = default_settings();
        let avail = SourceAvailabilityRollupRow {
            session_row_id: 2,
            indexed_frame_count: 20,
            available_frame_count: 20,
            readable_frame_count: 20,
            source_byte_count: 200_000,
            observed_at: "2026-07-22T00:00:00.000000Z".to_owned(),
        };
        let input = EvaluateEvidenceInput {
            evidence_public_id: "ev-002",
            session_public_id: "ses-002",
            session_row_id: 2,
            requirement_public_id: "req-002",
            kind: CalibrationKind::Flat,
            recipe_compatibility: RecipeCompatibility::Compatible,
            required_evidence_complete: true,
            candidate_evidence_complete: true,
            missing_recipe_fields: vec![],
            temperature_mode_str: "not_applicable".to_owned(),
            age_value: Some(0), // same night
            is_same_night_flat: true,
            thermal_evidence: None,
            flat_rotator_state: Some(PhysicalRotatorStateInput::Verified),
            flat_rotator_delta_udeg: Some(500_000), // 0.5 degrees — within normal
            availability: Some(&avail),
            observed_at: "2026-07-22T00:00:00.000000Z",
            settings: &settings,
            camera_kind_policy: None,
            basis_fingerprint: "fp-002".to_owned(),
        };
        let ev = evaluate_candidate_evidence(&input);
        assert!(ev.sufficient);
        assert_eq!(ev.orientation.state, OrientationState::Normal);
        assert_eq!(ev.automatic_eligibility, AutomaticEligibility::Eligible);
    }

    #[test]
    fn cross_night_flat_yields_review_required() {
        let settings = default_settings();
        let avail = SourceAvailabilityRollupRow {
            session_row_id: 2,
            indexed_frame_count: 20,
            available_frame_count: 20,
            readable_frame_count: 20,
            source_byte_count: 200_000,
            observed_at: "2026-07-22T00:00:00.000000Z".to_owned(),
        };
        let input = EvaluateEvidenceInput {
            evidence_public_id: "ev-003",
            session_public_id: "ses-003",
            session_row_id: 2,
            requirement_public_id: "req-003",
            kind: CalibrationKind::Flat,
            recipe_compatibility: RecipeCompatibility::Compatible,
            required_evidence_complete: true,
            candidate_evidence_complete: true,
            missing_recipe_fields: vec![],
            temperature_mode_str: "not_applicable".to_owned(),
            age_value: Some(3),        // 3 nights away — yellow age
            is_same_night_flat: false, // not same night
            thermal_evidence: None,
            flat_rotator_state: Some(PhysicalRotatorStateInput::Verified),
            flat_rotator_delta_udeg: Some(0),
            availability: Some(&avail),
            observed_at: "2026-07-22T00:00:00.000000Z",
            settings: &settings,
            camera_kind_policy: None,
            basis_fingerprint: "fp-003".to_owned(),
        };
        let ev = evaluate_candidate_evidence(&input);
        assert_eq!(ev.automatic_eligibility, AutomaticEligibility::ReviewRequired);
        assert!(ev.warning_codes.contains(&"calibration.flat_cross_night".to_owned()));
    }

    #[test]
    fn bias_has_not_applicable_thermal_and_orientation() {
        let settings = default_settings();
        let avail = SourceAvailabilityRollupRow {
            session_row_id: 3,
            indexed_frame_count: 50,
            available_frame_count: 50,
            readable_frame_count: 50,
            source_byte_count: 500_000,
            observed_at: "2026-07-22T00:00:00.000000Z".to_owned(),
        };
        let input = EvaluateEvidenceInput {
            evidence_public_id: "ev-004",
            session_public_id: "ses-004",
            session_row_id: 3,
            requirement_public_id: "req-004",
            kind: CalibrationKind::Bias,
            recipe_compatibility: RecipeCompatibility::Compatible,
            required_evidence_complete: true,
            candidate_evidence_complete: true,
            missing_recipe_fields: vec![],
            temperature_mode_str: "not_applicable".to_owned(),
            age_value: Some(50),
            is_same_night_flat: false,
            thermal_evidence: None,
            flat_rotator_state: None,
            flat_rotator_delta_udeg: None,
            availability: Some(&avail),
            observed_at: "2026-07-22T00:00:00.000000Z",
            settings: &settings,
            camera_kind_policy: None,
            basis_fingerprint: "fp-004".to_owned(),
        };
        let ev = evaluate_candidate_evidence(&input);
        assert_eq!(ev.thermal.state, ThermalState::NotApplicable);
        assert_eq!(ev.orientation.state, OrientationState::NotApplicable);
        assert_eq!(ev.automatic_eligibility, AutomaticEligibility::Eligible);
    }
}
