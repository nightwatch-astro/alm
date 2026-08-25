// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Assignment logic (spec 007 US4, T025).
#![allow(clippy::must_use_candidate)]
//!
//! Determines whether a master can be assigned to a session given the active
//! matching rule config and an optional override flag.
//!
//! This module is pure domain — no persistence writes happen here.
//! The Tauri command layer hands the `AssignDecision` to the persistence
//! repository to write the `calibration_assignment` row.

use crate::ranking::MatchingRuleConfig;
use crate::{CalibrationKind, Dimension, MasterInfo, SessionInfo};

/// The outcome of an assign evaluation.
#[derive(Debug, Clone)]
pub struct AssignDecision {
    /// The computed confidence at assignment time.
    pub confidence: f64,
    /// Whether the assignment used the override mechanism.
    pub was_override: bool,
    /// Hard-rule dimensions that were violated (non-empty only on override).
    pub mismatched_dimensions: Vec<Dimension>,
}

/// Error returned by `evaluate_assign` when the assignment is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignError {
    /// Master has hard-rule dimension mismatches and `override_flag` was false.
    IncompatibleDimensions { dimensions: Vec<Dimension> },
    /// The session was `mixed` — must split first.
    SessionMixedState,
    /// Observer location or exposure_start_utc is missing.
    ObserverLocationMissing,
}

impl AssignError {
    /// Contract error code string used in the Tauri response envelope.
    #[must_use]
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::IncompatibleDimensions { .. } => "incompatible.dimensions",
            Self::SessionMixedState => "session.mixed_state",
            Self::ObserverLocationMissing => "match.observer_location_missing",
        }
    }
}

/// Evaluate whether a master can be assigned to a session.
///
/// - If session is mixed → `AssignError::SessionMixedState`.
/// - If session lacks observer location or exposure_start_utc →
///   `AssignError::ObserverLocationMissing`.
/// - If hard-rule dimensions mismatch and `override_flag` is false →
///   `AssignError::IncompatibleDimensions`.
/// - If `override_flag` is true → `AssignDecision` with `was_override=true`,
///   confidence reduced by the type-specific override penalty, and
///   mismatched dimensions recorded.
/// - Otherwise → `AssignDecision` with `was_override=false`.
///
/// # Errors
/// Returns `AssignError` for guard failures or incompatible-without-override.
pub fn evaluate_assign(
    session: &SessionInfo,
    master: &MasterInfo,
    override_flag: bool,
    config: &MatchingRuleConfig,
) -> Result<AssignDecision, AssignError> {
    // Guard E5.
    if session.is_mixed() {
        return Err(AssignError::SessionMixedState);
    }

    // Guard A6 (issue #867): no longer hard-blocks — see suggest() doc comment
    // in lib.rs. Kept as a no-op guard site (rather than deleted) so the
    // `AssignError::ObserverLocationMissing` variant and its contract error
    // code stay available if a future caller needs to surface a warning.

    // Check hard-rule violations.
    let hard_violations = collect_hard_violations(session, master);

    if !hard_violations.is_empty() {
        if override_flag {
            // Override accepted: apply override penalty.
            let penalty = override_penalty_for(master.kind, config);
            let confidence = (1.0_f64 - penalty).clamp(0.0, 1.0);
            return Ok(AssignDecision {
                confidence,
                was_override: true,
                mismatched_dimensions: hard_violations,
            });
        }
        return Err(AssignError::IncompatibleDimensions { dimensions: hard_violations });
    }

    // Run the normal suggest path to get the computed confidence.
    let confidence = compute_assign_confidence(session, master, config);

    Ok(AssignDecision {
        confidence: confidence.clamp(0.0, 1.0),
        was_override: false,
        mismatched_dimensions: vec![],
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Collect dimensions with hard-rule violations for the given master type.
///
/// Uses [`crate::rules::hard_rule_numeric`] / [`crate::rules::hard_rule_string`]
/// / [`crate::rules::camera_bodies_conflict`] — the same exact-match-or-exclude
/// policy the `rules::{bias,dark,flat}` evaluators apply — so an assign
/// override's violation list always agrees with what `suggest` would have
/// excluded.
fn collect_hard_violations(session: &SessionInfo, master: &MasterInfo) -> Vec<Dimension> {
    use crate::rules::{camera_bodies_conflict, hard_rule_numeric, hard_rule_string};

    let mut violations = Vec::new();
    // Dark, flat and bias all apply the camera-body rule, so the check sits
    // outside the per-kind arms rather than being repeated in each.
    if camera_bodies_conflict(session.camera_body_id.as_deref(), master.camera_body_id.as_deref()) {
        violations.push(Dimension::Camera);
    }
    match master.kind {
        CalibrationKind::Dark | CalibrationKind::Bias => {
            if !hard_rule_numeric(session.gain, master.gain) {
                violations.push(Dimension::Gain);
            }
            if !hard_rule_numeric(session.offset, master.offset) {
                violations.push(Dimension::Offset);
            }
        }
        CalibrationKind::Flat => {
            if !hard_rule_string(session.filter.as_deref(), master.filter.as_deref()) {
                violations.push(Dimension::Filter);
            }
            if !hard_rule_string(session.binning.as_deref(), master.binning.as_deref()) {
                violations.push(Dimension::Binning);
            }
            if !hard_rule_string(session.optic_train.as_deref(), master.optic_train.as_deref()) {
                violations.push(Dimension::OpticTrain);
            }
            if !hard_rule_numeric(session.gain, master.gain) {
                violations.push(Dimension::Gain);
            }
        }
        CalibrationKind::DarkFlat => {
            // DarkFlat is never matchable in v1.
            violations.push(Dimension::Gain); // sentinel — always rejected
        }
    }
    violations
}

fn override_penalty_for(kind: CalibrationKind, config: &MatchingRuleConfig) -> f64 {
    match kind {
        CalibrationKind::Dark => config.dark_override_penalty,
        CalibrationKind::Flat => config.flat_override_penalty,
        CalibrationKind::Bias => config.bias_override_penalty,
        CalibrationKind::DarkFlat => 1.0,
    }
}

/// Compute confidence for an assign call by running the appropriate suggest rule.
fn compute_assign_confidence(
    session: &SessionInfo,
    master: &MasterInfo,
    config: &MatchingRuleConfig,
) -> f64 {
    match master.kind {
        CalibrationKind::Dark => {
            crate::rules::dark::evaluate(session, master, config).map_or(0.0, |m| m.confidence)
        }
        CalibrationKind::Flat => {
            crate::rules::flat::evaluate(session, master, config).map_or(0.0, |m| m.confidence)
        }
        CalibrationKind::Bias => {
            crate::rules::bias::evaluate(session, master, config).map_or(0.0, |m| m.confidence)
        }
        CalibrationKind::DarkFlat => 0.0,
    }
}

// ── Mismatch dim helper for assign response DTOs ──────────────────────────────

/// Extract dimension name strings for contract DTOs.
#[must_use]
pub fn dimension_names(dims: &[Dimension]) -> Vec<String> {
    dims.iter().map(|d| d.as_str().to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(session_type: &str, gain: f64, offset: f64) -> SessionInfo {
        SessionInfo {
            id: "ses-001".to_owned(),
            session_type: session_type.to_owned(),
            gain: Some(gain),
            offset: Some(offset),
            has_observer_location: true,
            has_exposure_start_utc: true,
            ..Default::default()
        }
    }

    fn dark_master(gain: f64, offset: f64) -> MasterInfo {
        MasterInfo {
            id: "m-001".to_owned(),
            kind: CalibrationKind::Dark,
            gain: Some(gain),
            offset: Some(offset),
            exposure_s: Some(300.0),
            temp_c: Some(-10.0),
            filter: None,
            rotation_deg: None,
            binning: None,
            optic_train: None,
            camera_body_id: None,
            source_session_id: None,
            observing_night_date: None,
        }
    }

    fn bias_master_info(gain: f64, offset: f64) -> MasterInfo {
        MasterInfo {
            id: "m-bias-001".to_owned(),
            kind: CalibrationKind::Bias,
            gain: Some(gain),
            offset: Some(offset),
            exposure_s: None,
            temp_c: None,
            filter: None,
            rotation_deg: None,
            binning: None,
            optic_train: None,
            camera_body_id: None,
            source_session_id: None,
            observing_night_date: None,
        }
    }

    /// `collect_hard_violations` and the `rules::*` evaluators must agree on what
    /// a camera-body conflict is, or an override reports a violation list that
    /// contradicts the exclusion `suggest` actually made. Both read
    /// [`crate::rules::camera_bodies_conflict`]; these cases pin that two
    /// different bodies are a violation and an absent id on either side is not.
    #[test]
    fn a_camera_body_conflict_is_a_hard_violation_but_a_missing_id_is_not() {
        let with_body = |body: Option<&str>| SessionInfo {
            camera_body_id: body.map(str::to_owned),
            ..session("light", 100.0, 50.0)
        };
        let master_with = |body: Option<&str>| MasterInfo {
            camera_body_id: body.map(str::to_owned),
            ..dark_master(100.0, 50.0)
        };
        let config = MatchingRuleConfig::default();

        let conflict = evaluate_assign(
            &with_body(Some("body-a")),
            &master_with(Some("body-b")),
            false,
            &config,
        );
        assert!(
            matches!(
                conflict,
                Err(AssignError::IncompatibleDimensions { ref dimensions })
                    if dimensions.contains(&Dimension::Camera)
            ),
            "two different bodies must be refused, got {conflict:?}"
        );

        for (session_body, master_body) in
            [(None, None), (Some("body-a"), None), (None, Some("body-a"))]
        {
            let decision = evaluate_assign(
                &with_body(session_body),
                &master_with(master_body),
                false,
                &config,
            )
            .expect("an absent camera id must not refuse an assign");
            assert!(
                !decision.mismatched_dimensions.contains(&Dimension::Camera),
                "{session_body:?}/{master_body:?} was reported as a camera violation"
            );
        }
    }

    #[test]
    fn compatible_dark_assign_succeeds() {
        let r = evaluate_assign(
            &session("light", 100.0, 50.0),
            &dark_master(100.0, 50.0),
            false,
            &MatchingRuleConfig::default(),
        );
        assert!(r.is_ok());
        let d = r.unwrap();
        assert!(!d.was_override);
        assert!(d.mismatched_dimensions.is_empty());
    }

    #[test]
    fn incompatible_dark_without_override_fails() {
        let r = evaluate_assign(
            &session("light", 100.0, 50.0),
            &dark_master(200.0, 50.0), // different gain
            false,
            &MatchingRuleConfig::default(),
        );
        assert_eq!(r.unwrap_err().error_code(), "incompatible.dimensions");
    }

    #[test]
    fn incompatible_dark_with_override_succeeds() {
        let r = evaluate_assign(
            &session("light", 100.0, 50.0),
            &dark_master(200.0, 50.0), // different gain
            true,
            &MatchingRuleConfig::default(),
        );
        let d = r.unwrap();
        assert!(d.was_override);
        assert!(d.mismatched_dimensions.contains(&Dimension::Gain));
        // confidence = 1.0 - dark_override_penalty (0.3) = 0.7
        assert!((d.confidence - 0.7).abs() < 1e-9);
    }

    #[test]
    fn mixed_session_returns_error() {
        let r = evaluate_assign(
            &session("mixed", 100.0, 50.0),
            &dark_master(100.0, 50.0),
            false,
            &MatchingRuleConfig::default(),
        );
        assert_eq!(r.unwrap_err().error_code(), "session.mixed_state");
    }

    #[test]
    fn missing_observer_location_no_longer_blocks_assign() {
        // #867: a session without an acquisition fingerprint yet must still
        // be assignable (degraded-but-usable path).
        let mut s = session("light", 100.0, 50.0);
        s.has_observer_location = false;
        let r =
            evaluate_assign(&s, &dark_master(100.0, 50.0), false, &MatchingRuleConfig::default());
        assert!(r.is_ok(), "missing observer_location must not block assign");
    }

    #[test]
    fn bias_compatible_assign_full_confidence() {
        let r = evaluate_assign(
            &session("light", 100.0, 50.0),
            &bias_master_info(100.0, 50.0),
            false,
            &MatchingRuleConfig::default(),
        );
        let d = r.unwrap();
        assert!((d.confidence - 1.0).abs() < 1e-9);
        assert!(!d.was_override);
    }

    #[test]
    fn dimension_names_returns_strings() {
        let dims = vec![Dimension::Gain, Dimension::Filter];
        let names = dimension_names(&dims);
        assert_eq!(names, vec!["gain".to_owned(), "filter".to_owned()]);
    }
}
