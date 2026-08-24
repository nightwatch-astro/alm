// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Dark frame matching rule (spec 007 US1, FR-003).
//!
//! Hard dimensions: `gain`, `offset` — exact match required unless the user
//! clears the corresponding Settings toggle, which turns the dimension into a
//! soft penalty instead of an exclusion.
//! Soft dimensions: `exposure` ±5% (default), `temperature` ±2°C (default),
//! `date_proximity` (master age) ±365 days (default), the last evaluated only
//! when both the session and the master carry an observing night.
//!
//! Missing temperature metadata falls back to gain+offset+exposure matching
//! with reduced confidence (metadata_missing penalty per spec edge-case).
#![allow(clippy::collapsible_match, clippy::single_match_else, clippy::must_use_candidate)]

use crate::candidate::{CalibrationMatch, MatchedDim, MismatchedDim, SelectionReason};
use crate::ranking::MatchingRuleConfig;
use crate::{CalibrationKind, Dimension, MasterInfo, SessionInfo};

/// Evaluate a single dark master against a light session.
///
/// Returns `None` when any hard-rule dimension fails (candidate excluded).
/// Returns `Some(CalibrationMatch)` otherwise, with confidence reduced by soft penalties.
pub fn evaluate(
    session: &SessionInfo,
    master: &MasterInfo,
    config: &MatchingRuleConfig,
) -> Option<CalibrationMatch> {
    debug_assert_eq!(master.kind, CalibrationKind::Dark);

    let mut matched: Vec<MatchedDim> = Vec::new();
    let mut mismatched: Vec<MismatchedDim> = Vec::new();
    let mut confidence = 1.0_f64;

    // ── Hard rules: gain and offset (relaxable per config) ────────────────────
    confidence -= crate::rules::relaxable_numeric(
        Dimension::Gain,
        session.gain,
        master.gain,
        config.require_same_gain,
        &mut matched,
        &mut mismatched,
    )?;
    confidence -= crate::rules::relaxable_numeric(
        Dimension::Offset,
        session.offset,
        master.offset,
        config.require_same_offset,
        &mut matched,
        &mut mismatched,
    )?;

    // ── Optional hard rule: camera body ───────────────────────────────────────
    confidence -= crate::rules::optional_camera_rule(
        session.camera_body_id.as_deref(),
        master.camera_body_id.as_deref(),
        &mut matched,
        &mut mismatched,
    )?;

    // ── Soft rule: exposure (±tolerance%) ─────────────────────────────────────
    let exp_cfg = config.dark_exposure_config();
    match (session.exposure_s, master.exposure_s) {
        (Some(se), Some(me)) => {
            // An exposure that is zero, negative, or not a number is broken
            // metadata, not a measurement. The old guard caught only zero, so a
            // negative master exposure reached `(se - me).abs() / me`, came out
            // negative, and a negative percentage difference is never greater than
            // the tolerance — the candidate matched at full confidence.
            if !se.is_finite() || !me.is_finite() || se <= 0.0 || me <= 0.0 {
                mismatched.push(MismatchedDim::metadata_missing(Dimension::Exposure));
                confidence -= exp_cfg.max_penalty;
            } else {
                // Compute percentage difference relative to reference exposure.
                let pct_diff = ((se - me).abs() / me) * 100.0;
                match exp_cfg.penalty(pct_diff) {
                    Some(penalty) => {
                        matched.push(MatchedDim::soft(
                            Dimension::Exposure,
                            se,
                            me,
                            (se - me).abs(),
                        ));
                        confidence -= penalty;
                    }
                    None => {
                        mismatched
                            .push(MismatchedDim::out_of_tolerance(Dimension::Exposure, pct_diff));
                        confidence -= exp_cfg.max_penalty;
                    }
                }
            }
        }
        _ => {
            mismatched.push(MismatchedDim::metadata_missing(Dimension::Exposure));
            confidence -= exp_cfg.max_penalty;
        }
    }

    // ── Soft rule: temperature (±tolerance °C) ────────────────────────────────
    let temp_cfg = config.dark_temp_config();
    match (session.temp_c, master.temp_c) {
        (Some(st), Some(mt)) => {
            let delta = (st - mt).abs();
            match temp_cfg.penalty(delta) {
                Some(penalty) => {
                    matched.push(MatchedDim::soft(Dimension::Temperature, st, mt, delta));
                    confidence -= penalty;
                }
                None => {
                    mismatched.push(MismatchedDim::out_of_tolerance(Dimension::Temperature, delta));
                    confidence -= temp_cfg.max_penalty;
                }
            }
        }
        _ => {
            // Missing temperature: falls back to gain+offset+exposure matching.
            // Adds metadata_missing entry but does NOT exclude (spec edge-case).
            mismatched.push(MismatchedDim::metadata_missing(Dimension::Temperature));
            confidence -= temp_cfg.max_penalty;
        }
    }

    // ── Soft rule: master age (±age_limit_days) ───────────────────────────────
    confidence -= crate::rules::apply_age_rule(
        session.observing_night_date.as_deref(),
        master.observing_night_date.as_deref(),
        config,
        &mut matched,
        &mut mismatched,
    );

    Some(CalibrationMatch::new(
        session.id.clone(),
        master.id.clone(),
        CalibrationKind::Dark,
        confidence,
        matched,
        mismatched,
        SelectionReason::CompatibleFallback, // darks don't use observing-night
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(gain: f64, offset: f64, exposure_s: f64, temp_c: f64) -> SessionInfo {
        SessionInfo {
            id: "ses-001".to_owned(),
            session_type: "light".to_owned(),
            gain: Some(gain),
            offset: Some(offset),
            exposure_s: Some(exposure_s),
            temp_c: Some(temp_c),
            has_observer_location: true,
            has_exposure_start_utc: true,
            ..Default::default()
        }
    }

    fn master(gain: f64, offset: f64, exposure_s: f64, temp_c: f64) -> MasterInfo {
        MasterInfo {
            id: "m-dark-001".to_owned(),
            kind: CalibrationKind::Dark,
            gain: Some(gain),
            offset: Some(offset),
            exposure_s: Some(exposure_s),
            temp_c: Some(temp_c),
            filter: None,
            rotation_deg: None,
            binning: None,
            optic_train: None,
            camera_body_id: None,
            source_session_id: None,
            observing_night_date: None,
        }
    }

    #[test]
    fn exact_match_confidence_1_0() {
        let m = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 50.0, 300.0, -10.0),
            &MatchingRuleConfig::default(),
        );
        let m = m.unwrap();
        assert!((m.confidence - 1.0).abs() < 1e-9, "exact match confidence={}", m.confidence);
        assert!(m.dimensions_mismatched.is_empty());
    }

    #[test]
    fn an_unusable_exposure_is_reported_as_missing_metadata_not_scored() {
        // A negative master exposure used to reach `(se - me).abs() / me`, which
        // came out negative, and a negative percentage difference never exceeds the
        // tolerance — a 300s light matched a -300s dark at full confidence.
        for bad in [-300.0, 0.0, f64::NAN, f64::INFINITY] {
            let m = evaluate(
                &session(100.0, 50.0, 300.0, -10.0),
                &master(100.0, 50.0, bad, -10.0),
                &MatchingRuleConfig::default(),
            )
            .unwrap_or_else(|| panic!("master exposure {bad} should still yield a soft result"));
            assert!(
                m.dimensions_mismatched
                    .iter()
                    .any(|dim| dim.dimension == Dimension::Exposure.as_str()),
                "master exposure {bad} was scored as a matching exposure: {m:?}"
            );
            assert!(m.confidence < 1.0, "master exposure {bad} kept full confidence");
        }

        let m = evaluate(
            &session(100.0, 50.0, -300.0, -10.0),
            &master(100.0, 50.0, 300.0, -10.0),
            &MatchingRuleConfig::default(),
        )
        .expect("negative session exposure should still yield a soft result");
        assert!(m.confidence < 1.0, "negative session exposure kept full confidence");
    }

    #[test]
    fn gain_hard_rule_violation_excludes() {
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(200.0, 50.0, 300.0, -10.0),
            &MatchingRuleConfig::default(),
        );
        assert!(r.is_none(), "gain mismatch should exclude");
    }

    #[test]
    fn offset_hard_rule_violation_excludes() {
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 75.0, 300.0, -10.0),
            &MatchingRuleConfig::default(),
        );
        assert!(r.is_none(), "offset mismatch should exclude");
    }

    #[test]
    fn missing_gain_excludes() {
        let mut s = session(100.0, 50.0, 300.0, -10.0);
        s.gain = None;
        let r = evaluate(&s, &master(100.0, 50.0, 300.0, -10.0), &MatchingRuleConfig::default());
        assert!(r.is_none(), "missing session gain should exclude");
    }

    #[test]
    fn missing_master_offset_excludes() {
        let mut m = master(100.0, 50.0, 300.0, -10.0);
        m.offset = None;
        let r = evaluate(&session(100.0, 50.0, 300.0, -10.0), &m, &MatchingRuleConfig::default());
        assert!(r.is_none(), "missing master offset should exclude");
    }

    #[test]
    fn exposure_within_tolerance_matched() {
        // 300s session, 295s master → delta is 5s → 5/295 * 100 ≈ 1.7% < 5%
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 50.0, 295.0, -10.0),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert!(r.confidence < 1.0, "soft penalty should reduce confidence");
        assert!(r.dimensions_matched.iter().any(|d| d.dimension == "exposure"));
    }

    #[test]
    fn exposure_out_of_tolerance_reduces_confidence() {
        // 300s session, 100s master → 200% difference > 5%
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 50.0, 100.0, -10.0),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap(); // still returned (soft fail, not excluded)
        assert!(r.dimensions_mismatched.iter().any(|d| d.dimension == "exposure"));
    }

    #[test]
    fn temperature_within_tolerance_matched() {
        // -10 vs -11.5 → delta 1.5 < 2.0
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 50.0, 300.0, -11.5),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert!(r.dimensions_matched.iter().any(|d| d.dimension == "temperature"));
        assert!(r.confidence < 1.0);
    }

    #[test]
    fn temperature_out_of_tolerance_reported_in_mismatched() {
        // -10 vs -20 → delta 10 > 2
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 50.0, 300.0, -20.0),
            &MatchingRuleConfig::default(),
        );
        let r = r.unwrap();
        assert!(r.dimensions_mismatched.iter().any(|d| d.dimension == "temperature"));
    }

    #[test]
    fn missing_temperature_metadata_fallback_not_excluded() {
        let mut m = master(100.0, 50.0, 300.0, -10.0);
        m.temp_c = None;
        let r = evaluate(&session(100.0, 50.0, 300.0, -10.0), &m, &MatchingRuleConfig::default());
        assert!(r.is_some(), "missing temp should NOT exclude (fallback to gain+offset+exposure)");
        let r = r.unwrap();
        assert!(r.dimensions_mismatched.iter().any(|d| d.dimension == "temperature"
            && d.reason == crate::candidate::MismatchReason::MetadataMissing));
    }

    #[test]
    fn wide_temperature_tolerance_accepts_far_master() {
        let config = MatchingRuleConfig { dark_temp_tolerance_c: 20.0, ..Default::default() };
        // -10 vs -25 → delta 15 < 20
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 50.0, 300.0, -25.0),
            &config,
        );
        let r = r.unwrap();
        assert!(
            r.dimensions_matched.iter().any(|d| d.dimension == "temperature"),
            "wider tolerance should now accept; got mismatched={:?}",
            r.dimensions_mismatched
        );
    }

    // ── require_same_offset tests ─────────────────────────────────────────────

    #[test]
    fn offset_hard_rule_violation_excludes_when_policy_strict() {
        // Default policy: require_same_offset = true → mismatch excludes.
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 75.0, 300.0, -10.0),
            &MatchingRuleConfig::default(),
        );
        assert!(r.is_none(), "strict offset policy should exclude on mismatch");
    }

    #[test]
    fn offset_mismatch_accepted_when_policy_relaxed() {
        let config =
            MatchingRuleConfig { require_same_offset: false, ..MatchingRuleConfig::default() };
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 75.0, 300.0, -10.0),
            &config,
        );
        assert!(r.is_some(), "relaxed offset policy should not exclude on mismatch");
        let r = r.unwrap();
        assert!(r.confidence < 1.0, "offset mismatch with relaxed policy should reduce confidence");
        assert!(
            r.dimensions_mismatched.iter().any(|d| d.dimension == "offset"),
            "offset should appear in mismatched with out_of_tolerance reason"
        );
    }

    #[test]
    fn missing_offset_accepted_when_policy_relaxed() {
        let config =
            MatchingRuleConfig { require_same_offset: false, ..MatchingRuleConfig::default() };
        let mut m = master(100.0, 50.0, 300.0, -10.0);
        m.offset = None;
        let r = evaluate(&session(100.0, 50.0, 300.0, -10.0), &m, &config);
        assert!(r.is_some(), "relaxed policy should not exclude on missing offset");
        let r = r.unwrap();
        assert!(
            r.dimensions_mismatched.iter().any(|d| d.dimension == "offset"
                && d.reason == crate::candidate::MismatchReason::MetadataMissing),
            "missing offset should produce a metadata_missing entry"
        );
    }

    #[test]
    fn missing_offset_excluded_when_policy_strict() {
        let mut m = master(100.0, 50.0, 300.0, -10.0);
        m.offset = None;
        let r = evaluate(&session(100.0, 50.0, 300.0, -10.0), &m, &MatchingRuleConfig::default());
        assert!(r.is_none(), "strict policy should exclude on missing offset");
    }

    #[test]
    fn dimensions_matched_union_mismatched_covers_all() {
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 50.0, 200.0, -15.0),
            &MatchingRuleConfig::default(),
        )
        .unwrap();
        let all_dims: std::collections::HashSet<&str> = r
            .dimensions_matched
            .iter()
            .map(|d| d.dimension.as_str())
            .chain(r.dimensions_mismatched.iter().map(|d| d.dimension.as_str()))
            .collect();
        // All 4 active dimensions for dark should be present
        for dim in &["gain", "offset", "exposure", "temperature"] {
            assert!(all_dims.contains(dim), "missing dimension: {dim}");
        }
    }

    fn dated(night: &str) -> (SessionInfo, MasterInfo) {
        let mut s = session(100.0, 50.0, 300.0, -10.0);
        s.observing_night_date = Some("2026-01-01".to_owned());
        let mut m = master(100.0, 50.0, 300.0, -10.0);
        m.observing_night_date = Some(night.to_owned());
        (s, m)
    }

    fn age_dim_state(r: &CalibrationMatch) -> (bool, bool) {
        (
            r.dimensions_matched.iter().any(|d| d.dimension == "date_proximity"),
            r.dimensions_mismatched.iter().any(|d| d.dimension == "date_proximity"),
        )
    }

    #[test]
    fn aging_limit_days_changes_the_match_outcome() {
        // ~6 years older than the session's observing night.
        let (s, m) = dated("2020-01-01");

        let strict = evaluate(&s, &m, &MatchingRuleConfig::default())
            .expect("an over-age master stays a candidate");
        assert_eq!(
            age_dim_state(&strict),
            (false, true),
            "default 365-day limit should report the master as out of age tolerance"
        );

        let relaxed = MatchingRuleConfig { age_limit_days: 3000.0, ..Default::default() };
        let lenient = evaluate(&s, &m, &relaxed).expect("relaxed limit keeps the candidate");
        assert_eq!(
            age_dim_state(&lenient),
            (true, false),
            "a limit above the gap should accept the age within tolerance"
        );
        assert!(
            lenient.confidence > strict.confidence,
            "raising the limit should raise confidence: {} vs {}",
            lenient.confidence,
            strict.confidence
        );
    }

    #[test]
    fn require_same_gain_changes_the_match_outcome() {
        let s = session(100.0, 50.0, 300.0, -10.0);
        let m = master(200.0, 50.0, 300.0, -10.0);
        assert!(
            evaluate(&s, &m, &MatchingRuleConfig::default()).is_none(),
            "a gain mismatch must exclude the master while the toggle is set"
        );

        let relaxed = MatchingRuleConfig { require_same_gain: false, ..Default::default() };
        let r = evaluate(&s, &m, &relaxed).expect("clearing the toggle keeps the master");
        assert!(
            r.dimensions_mismatched.iter().any(|d| d.dimension == "gain"),
            "the relaxed gain must be reported as a mismatch"
        );
        assert!(r.confidence < 1.0, "a relaxed gain must cost confidence: {}", r.confidence);
    }

    #[test]
    fn an_unknown_observing_night_is_not_penalised_as_age() {
        // `master()` leaves both observing nights unset.
        let r = evaluate(
            &session(100.0, 50.0, 300.0, -10.0),
            &master(100.0, 50.0, 300.0, -10.0),
            &MatchingRuleConfig::default(),
        )
        .unwrap();
        assert_eq!(
            age_dim_state(&r),
            (false, false),
            "age must be skipped, not scored, when an observing night is missing"
        );
        assert!((r.confidence - 1.0).abs() < 1e-9, "confidence={}", r.confidence);
    }
}
