// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Flat frame matching rule (spec 007 US2, FR-004, FR-008, FR-009).
#![allow(
    clippy::collapsible_match,
    clippy::single_match_else,
    clippy::must_use_candidate,
    clippy::too_many_lines
)]
//!
//! Hard dimensions: `filter`, `binning`, `optic_train`, `gain` (exact, 2026-05-23 decision).
//! `binning` and `gain` become soft penalties when the user clears their
//! Settings "match required" toggles; `filter` and `optic_train` stay exact.
//! Soft dimensions: `rotation` ±0.5° (default), `observing_night_proximity` (±7 nights default).
//!
//! Selection reasons prioritize same_session > same_night > compatible_fallback.

#[cfg(test)]
use crate::candidate::SelectionReason;
use crate::candidate::{CalibrationMatch, MatchedDim, MismatchedDim};
use crate::ranking::{flat_selection_reason, night_distance, MatchingRuleConfig};
use crate::{CalibrationKind, Dimension, MasterInfo, SessionInfo};

/// Evaluate a single flat master against a light session.
///
/// Returns `None` when any hard-rule dimension fails (candidate excluded).
pub fn evaluate(
    session: &SessionInfo,
    master: &MasterInfo,
    config: &MatchingRuleConfig,
) -> Option<CalibrationMatch> {
    debug_assert_eq!(master.kind, CalibrationKind::Flat);

    let mut matched: Vec<MatchedDim> = Vec::new();
    let mut mismatched: Vec<MismatchedDim> = Vec::new();
    let mut confidence = 1.0_f64;

    // ── Hard rule: filter ─────────────────────────────────────────────────────
    match (session.filter.as_deref(), master.filter.as_deref()) {
        (Some(sf), Some(mf)) if crate::rules::hard_rule_string(Some(sf), Some(mf)) => {
            matched.push(MatchedDim::exact_string(Dimension::Filter, sf));
        }
        _ => return None,
    }

    // ── Hard rule: binning (relaxable per config) ─────────────────────────────
    confidence -= crate::rules::relaxable_string(
        Dimension::Binning,
        session.binning.as_deref(),
        master.binning.as_deref(),
        config.require_same_binning,
        &mut matched,
        &mut mismatched,
    )?;

    // ── Hard rule: optic_train ────────────────────────────────────────────────
    match (session.optic_train.as_deref(), master.optic_train.as_deref()) {
        (Some(st), Some(mt)) if crate::rules::hard_rule_string(Some(st), Some(mt)) => {
            matched.push(MatchedDim::exact_string(Dimension::OpticTrain, st));
        }
        _ => return None,
    }

    // ── Hard rule: gain (exact, no tolerance — 2026-05-23 decision) ───────────
    confidence -= crate::rules::relaxable_numeric(
        Dimension::Gain,
        session.gain,
        master.gain,
        config.require_same_gain,
        &mut matched,
        &mut mismatched,
    )?;

    // ── Soft rule: rotation ───────────────────────────────────────────────────
    let rot_cfg = config.flat_rotation_config();
    match (session.rotation_deg, master.rotation_deg) {
        (Some(sr), Some(mr)) => {
            // Issue #921: rotator angles are circular — plain `.abs()` scored
            // 359.9° vs 0.1° as 359.8° instead of the true 0.2° apart.
            let delta = skymath::circular_distance(
                skymath::Angle::from_degrees(sr),
                skymath::Angle::from_degrees(mr),
            )
            .degrees();
            match rot_cfg.penalty(delta) {
                Some(penalty) => {
                    matched.push(MatchedDim::soft(Dimension::Rotation, sr, mr, delta));
                    confidence -= penalty;
                }
                None => {
                    mismatched.push(MismatchedDim::out_of_tolerance(Dimension::Rotation, delta));
                    confidence -= rot_cfg.max_penalty;
                }
            }
        }
        _ => {
            mismatched.push(MismatchedDim::metadata_missing(Dimension::Rotation));
            confidence -= rot_cfg.max_penalty;
        }
    }

    // ── Soft rule: observing night proximity ─────────────────────────────────
    let night_cfg = config.flat_night_config();
    match (session.observing_night_date.as_deref(), master.observing_night_date.as_deref()) {
        (Some(sd), Some(md)) => {
            let dist = night_distance(sd, md).unwrap_or(night_cfg.tolerance + 1.0);
            match night_cfg.penalty(dist) {
                Some(penalty) => {
                    matched.push(MatchedDim::soft(
                        Dimension::ObservingNightProximity,
                        dist,
                        0.0,
                        dist,
                    ));
                    confidence -= penalty;
                }
                None => {
                    mismatched.push(MismatchedDim::out_of_tolerance(
                        Dimension::ObservingNightProximity,
                        dist,
                    ));
                    confidence -= night_cfg.max_penalty;
                }
            }
        }
        _ => {
            mismatched.push(MismatchedDim::metadata_missing(Dimension::ObservingNightProximity));
            confidence -= night_cfg.max_penalty;
        }
    }

    // ── Selection reason ──────────────────────────────────────────────────────
    let reason = flat_selection_reason(
        session.observing_night_date.as_deref(),
        master.observing_night_date.as_deref(),
        &session.id,
        master.source_session_id.as_deref(),
    );

    Some(CalibrationMatch::new(
        session.id.clone(),
        master.id.clone(),
        CalibrationKind::Flat,
        confidence,
        matched,
        mismatched,
        reason,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionInfo {
        SessionInfo {
            id: "ses-001".to_owned(),
            session_type: "light".to_owned(),
            gain: Some(100.0),
            offset: None,
            filter: Some("Ha".to_owned()),
            rotation_deg: Some(0.0),
            binning: Some("1x1".to_owned()),
            optic_train: Some("train-a".to_owned()),
            observing_night_date: Some("2026-01-15".to_owned()),
            has_observer_location: true,
            has_exposure_start_utc: true,
            ..Default::default()
        }
    }

    fn master_flat(
        filter: &str,
        binning: &str,
        optic_train: &str,
        gain: f64,
        rotation: f64,
        night: &str,
    ) -> MasterInfo {
        MasterInfo {
            id: "m-flat-001".to_owned(),
            kind: CalibrationKind::Flat,
            gain: Some(gain),
            offset: None,
            exposure_s: None,
            temp_c: None,
            filter: Some(filter.to_owned()),
            rotation_deg: Some(rotation),
            binning: Some(binning.to_owned()),
            optic_train: Some(optic_train.to_owned()),
            source_session_id: None,
            observing_night_date: Some(night.to_owned()),
        }
    }

    #[test]
    fn exact_match_same_night_high_confidence() {
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-a", 100.0, 0.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert!(r.confidence >= 0.9, "same night exact match confidence={}", r.confidence);
        assert_eq!(r.selection_reason, SelectionReason::SameNight);
    }

    #[test]
    fn filter_hard_rule_mismatch_excludes() {
        let r = evaluate(
            &session(),
            &master_flat("SII", "1x1", "train-a", 100.0, 0.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        assert!(r.is_none(), "filter mismatch should exclude");
    }

    #[test]
    fn binning_hard_rule_mismatch_excludes() {
        let r = evaluate(
            &session(),
            &master_flat("Ha", "2x2", "train-a", 100.0, 0.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        assert!(r.is_none(), "binning mismatch should exclude");
    }

    #[test]
    fn optic_train_hard_rule_mismatch_excludes() {
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-b", 100.0, 0.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        assert!(r.is_none(), "optic_train mismatch should exclude");
    }

    #[test]
    fn gain_hard_rule_mismatch_excludes() {
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-a", 200.0, 0.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        assert!(r.is_none(), "gain mismatch should exclude");
    }

    #[test]
    fn rotation_within_tolerance_matched() {
        // 0.0 vs 0.3 → delta 0.3 < 0.5
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-a", 100.0, 0.3, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert!(r.dimensions_matched.iter().any(|d| d.dimension == "rotation"));
    }

    #[test]
    fn rotation_out_of_tolerance_reported_in_mismatched() {
        // 0.0 vs 5.0 → delta 5.0 > 0.5
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-a", 100.0, 5.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert!(r.dimensions_mismatched.iter().any(|d| d.dimension == "rotation"));
    }

    #[test]
    fn rotation_wraparound_across_0_360_seam_matched() {
        // Issue #921: 359.9° vs 0.1° is truly 0.2° apart, not 359.8° — must
        // match within the default ±0.5° tolerance, not be max-penalized.
        let s = SessionInfo { rotation_deg: Some(359.9), ..session() };
        let r = evaluate(
            &s,
            &master_flat("Ha", "1x1", "train-a", 100.0, 0.1, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert!(
            r.dimensions_matched.iter().any(|d| d.dimension == "rotation"),
            "359.9 vs 0.1 should match within tolerance, got mismatched={:?}",
            r.dimensions_mismatched
        );
        assert!(!r.dimensions_mismatched.iter().any(|d| d.dimension == "rotation"));
    }

    #[test]
    fn rotation_far_apart_delta_is_shortest_arc_not_naive_diff() {
        // 45° vs 295°: circularly 110° apart (the short way, through 0/360°
        // seam via 350°); a naive |a-b| would wrongly give 250°.
        let s = SessionInfo { rotation_deg: Some(45.0), ..session() };
        let r = evaluate(
            &s,
            &master_flat("Ha", "1x1", "train-a", 100.0, 295.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        let delta = r
            .dimensions_mismatched
            .iter()
            .find(|d| d.dimension == "rotation")
            .and_then(|d| d.delta)
            .expect("rotation should be out of tolerance with a delta");
        assert!((delta - 110.0).abs() < 1e-9, "expected 110.0, got {delta}");
    }

    #[test]
    fn rotation_antipodal_boundary_delta_is_180() {
        // 0° vs 180°: maximally distant on the circle either direction.
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-a", 100.0, 180.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        let delta = r
            .dimensions_mismatched
            .iter()
            .find(|d| d.dimension == "rotation")
            .and_then(|d| d.delta)
            .expect("rotation should be out of tolerance with a delta");
        assert!((delta - 180.0).abs() < 1e-9, "expected 180.0, got {delta}");
    }

    #[test]
    fn rotation_circular_distance_property_bounded_and_correct() {
        // Drives evaluate() itself (not skymath::circular_distance in
        // isolation) so a regression that swaps evaluate()'s rotation delta
        // for a naive `(a - b).abs()` — the bug issue #921 fixed — is caught
        // here across a grid of angle pairs, not just the handful of fixed
        // examples in the other rotation_* tests.
        let angles = [0.0, 0.1, 45.0, 90.0, 180.0, 270.0, 295.0, 359.0, 359.9];
        for &a in &angles {
            for &b in &angles {
                let s = SessionInfo { rotation_deg: Some(a), ..session() };
                let r = evaluate(
                    &s,
                    &master_flat("Ha", "1x1", "train-a", 100.0, b, "2026-01-15"),
                    &MatchingRuleConfig::default(),
                )
                .expect("hard rules all satisfied");
                let matched_delta = r
                    .dimensions_matched
                    .iter()
                    .find(|d| d.dimension == "rotation")
                    .and_then(|d| d.delta);
                let mismatched_delta = r
                    .dimensions_mismatched
                    .iter()
                    .find(|d| d.dimension == "rotation")
                    .and_then(|d| d.delta);
                let delta = matched_delta
                    .or(mismatched_delta)
                    .expect("rotation dimension always carries a delta when both angles are Some");

                assert!(
                    (0.0..=180.0).contains(&delta),
                    "evaluate() rotation delta {delta} out of [0,180] for ({a}, {b})"
                );
                let raw = (a - b).abs().rem_euclid(360.0);
                let expected = raw.min(360.0 - raw);
                assert!(
                    (delta - expected).abs() < 1e-9,
                    "({a}, {b}): evaluate() gave {delta}, expected {expected}"
                );
            }
        }
    }

    #[test]
    fn different_night_fallback_reason() {
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-a", 100.0, 0.0, "2026-01-10"),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert_eq!(r.selection_reason, SelectionReason::CompatibleFallback);
    }

    #[test]
    fn same_session_reason_when_source_matches() {
        let mut m = master_flat("Ha", "1x1", "train-a", 100.0, 0.0, "2026-01-15");
        m.source_session_id = Some("ses-001".to_owned());
        let r = evaluate(&session(), &m, &MatchingRuleConfig::default()).unwrap();
        assert_eq!(r.selection_reason, SelectionReason::SameSession);
    }

    #[test]
    fn far_night_out_of_tolerance_confidence_reduced() {
        // 2026-01-15 vs 2025-01-01 → >7 nights
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-a", 100.0, 0.0, "2025-01-01"),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert!(r.dimensions_mismatched.iter().any(|d| d.dimension == "observing_night_proximity"));
    }

    #[test]
    fn no_bias_dimensions_in_flat_response() {
        let r = evaluate(
            &session(),
            &master_flat("Ha", "1x1", "train-a", 100.0, 0.0, "2026-01-15"),
            &MatchingRuleConfig::default(),
        )
        .unwrap();
        let all_dims: Vec<&str> = r
            .dimensions_matched
            .iter()
            .map(|d| d.dimension.as_str())
            .chain(r.dimensions_mismatched.iter().map(|d| d.dimension.as_str()))
            .collect();
        assert!(!all_dims.contains(&"exposure"), "flat should not have exposure dimension");
        assert!(!all_dims.contains(&"temperature"), "flat should not have temperature dimension");
    }

    #[test]
    fn require_same_gain_changes_the_match_outcome() {
        let m = master_flat("Ha", "1x1", "train-a", 200.0, 0.0, "2026-01-15");
        assert!(
            evaluate(&session(), &m, &MatchingRuleConfig::default()).is_none(),
            "a gain mismatch must exclude the flat while the toggle is set"
        );

        let relaxed = MatchingRuleConfig { require_same_gain: false, ..Default::default() };
        let r = evaluate(&session(), &m, &relaxed).expect("clearing the toggle keeps the flat");
        assert!(
            r.dimensions_mismatched.iter().any(|d| d.dimension == "gain"),
            "the relaxed gain must be reported as a mismatch"
        );
        assert!(r.confidence < 1.0, "a relaxed gain must cost confidence: {}", r.confidence);
    }

    #[test]
    fn require_same_binning_changes_the_match_outcome() {
        let m = master_flat("Ha", "2x2", "train-a", 100.0, 0.0, "2026-01-15");
        assert!(
            evaluate(&session(), &m, &MatchingRuleConfig::default()).is_none(),
            "a binning mismatch must exclude the flat while the toggle is set"
        );

        let relaxed = MatchingRuleConfig { require_same_binning: false, ..Default::default() };
        let r = evaluate(&session(), &m, &relaxed).expect("clearing the toggle keeps the flat");
        assert!(
            r.dimensions_mismatched.iter().any(|d| d.dimension == "binning"),
            "the relaxed binning must be reported as a mismatch"
        );
        assert!(r.confidence < 1.0, "a relaxed binning must cost confidence: {}", r.confidence);
    }
}
