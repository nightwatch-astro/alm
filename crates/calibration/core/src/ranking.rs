// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Confidence scoring, ranking utilities, and `MatchingRuleConfig`.
#![allow(clippy::must_use_candidate)]
//!
//! Default tolerances per data-model.md:
//! - Dark: gain (hard), offset (hard), exposure ±5% (soft, max_penalty 0.3),
//!   temperature ±2°C (soft, max_penalty 0.4).
//! - Flat: filter (hard), binning (hard), optic_train (hard), gain (hard),
//!   rotation ±0.5° (soft, max_penalty 0.5), observing_night_proximity
//!   (soft, 0 nights preferred / ±7 nights tolerated, max_penalty 0.4).
//! - Bias: gain (hard), offset (hard).

use crate::candidate::{CalibrationMatch, SelectionReason};

// ── Soft dimension tolerance config ──────────────────────────────────────────

/// Tolerance configuration for a single soft dimension.
#[derive(Clone, Debug)]
pub struct SoftDimConfig {
    /// Maximum absolute deviation accepted (units: °C, %, °, nights).
    pub tolerance: f64,
    /// Maximum confidence penalty when at the tolerance boundary (0.0–1.0).
    pub max_penalty: f64,
}

impl SoftDimConfig {
    #[must_use]
    pub const fn new(tolerance: f64, max_penalty: f64) -> Self {
        Self { tolerance, max_penalty }
    }

    /// Compute the penalty for a given absolute delta.
    ///
    /// Returns `None` when `delta > tolerance` (out of tolerance), and when any
    /// input is unusable: a delta or tolerance that is NaN, infinite, or negative,
    /// or a `max_penalty` that is NaN or infinite. The boundary is inclusive, so
    /// `delta == tolerance` is still a match.
    ///
    /// Returns `0.0` when `delta == 0.0` (exact match). Otherwise scales linearly
    /// from 0 to `max_penalty` as delta approaches tolerance, clamped to 0.0–1.0.
    #[must_use]
    pub fn penalty(&self, delta: f64) -> Option<f64> {
        // Every comparison against a NaN is false, so the bare `delta >
        // self.tolerance` below took the match branch — scoring an unrelated
        // master as a full-confidence match, which is silent corruption of the
        // user's calibration — for a NaN delta, a NaN tolerance, and a NaN
        // `max_penalty`. A `+INFINITY` tolerance reached it too, by admitting
        // every delta there is.
        //
        // A negative or `-INFINITY` tolerance was NOT one of these: `delta` is
        // non-negative past the first guard, so `delta > tolerance` held and the
        // function already returned `None` for every input. The guard below keeps
        // that behaviour and states it locally instead of leaving it to a
        // comparison two branches away.
        if !delta.is_finite() || delta < 0.0 {
            return None;
        }
        if !self.tolerance.is_finite() || self.tolerance < 0.0 {
            return None;
        }
        if !self.max_penalty.is_finite() {
            return None;
        }
        if delta > self.tolerance {
            return None;
        }
        if self.tolerance == 0.0 {
            return Some(0.0);
        }
        Some(((delta / self.tolerance) * self.max_penalty).clamp(0.0, 1.0))
    }
}

// ── MatchingRuleConfig ────────────────────────────────────────────────────────

/// User-configurable tolerances consumed by the matcher.
///
/// Loaded from the settings keys `calibrationDarkTempTolerance`,
/// `calibrationDarkOverridePenalty`, `calibrationFlatOverridePenalty`,
/// `calibrationBiasOverridePenalty`, `calibrationPrefillSuggestion`.
#[derive(Clone, Debug)]
pub struct MatchingRuleConfig {
    // ── Dark tolerances ──
    /// Dark exposure soft tolerance (percentage, 0–100). Default 5.0 → ±5%.
    pub dark_exposure_tolerance_pct: f64,
    /// Dark exposure soft max penalty. Default 0.3.
    pub dark_exposure_max_penalty: f64,
    /// Dark temperature soft tolerance in °C. Default 2.0.
    pub dark_temp_tolerance_c: f64,
    /// Dark temperature soft max penalty. Default 0.4.
    pub dark_temp_max_penalty: f64,
    /// Confidence penalty when a dark is assigned as override. Default 0.3.
    pub dark_override_penalty: f64,

    // ── Flat tolerances ──
    /// Flat rotation soft tolerance in degrees. Default 0.5.
    pub flat_rotation_tolerance_deg: f64,
    /// Flat rotation soft max penalty. Default 0.5.
    pub flat_rotation_max_penalty: f64,
    /// Flat observing-night proximity tolerance in nights. Default 7.
    pub flat_night_tolerance_nights: f64,
    /// Flat observing-night soft max penalty. Default 0.4.
    pub flat_night_max_penalty: f64,
    /// Confidence penalty when a flat is assigned as override. Default 0.3.
    pub flat_override_penalty: f64,

    // ── Bias ──
    /// Confidence penalty when a bias is assigned as override. Default 0.3.
    pub bias_override_penalty: f64,

    // ── Policy flags ──
    /// When true, a master must carry the same OFFSET as the light session for
    /// dark and bias matching (hard rule). When false, a missing or mismatched
    /// offset is treated as a metadata-missing soft penalty instead of excluding
    /// the candidate. Default: `true` (offset always required, matching the
    /// original strict behaviour).
    pub require_same_offset: bool,

    // ── Age tolerance (dark and bias) ──
    /// Maximum accepted age gap in days between the light session's observing
    /// night and the master's. Default 365. A master beyond the limit is kept
    /// as a candidate and reported as an out-of-tolerance `DateProximity`
    /// mismatch; the dimension is skipped entirely when either observing night
    /// is unknown, so a master with no date is never penalised for age.
    pub age_limit_days: f64,
    /// Age soft max penalty. Default 0.3.
    pub age_max_penalty: f64,

    // ── UI ──
    /// When true, the assign dialog pre-fills with the top candidate (R-Prefill).
    pub prefill_suggestion: bool,
}

impl Default for MatchingRuleConfig {
    fn default() -> Self {
        Self {
            dark_exposure_tolerance_pct: 5.0,
            dark_exposure_max_penalty: 0.3,
            dark_temp_tolerance_c: 2.0,
            dark_temp_max_penalty: 0.4,
            dark_override_penalty: 0.3,
            flat_rotation_tolerance_deg: 0.5,
            flat_rotation_max_penalty: 0.5,
            flat_night_tolerance_nights: 7.0,
            flat_night_max_penalty: 0.4,
            flat_override_penalty: 0.3,
            bias_override_penalty: 0.3,
            require_same_offset: true,
            age_limit_days: 365.0,
            age_max_penalty: 0.3,
            prefill_suggestion: true,
        }
    }
}

impl MatchingRuleConfig {
    /// `SoftDimConfig` for dark exposure tolerance.
    #[must_use]
    pub fn dark_exposure_config(&self) -> SoftDimConfig {
        SoftDimConfig::new(self.dark_exposure_tolerance_pct, self.dark_exposure_max_penalty)
    }

    /// `SoftDimConfig` for dark temperature tolerance.
    #[must_use]
    pub fn dark_temp_config(&self) -> SoftDimConfig {
        SoftDimConfig::new(self.dark_temp_tolerance_c, self.dark_temp_max_penalty)
    }

    /// `SoftDimConfig` for flat rotation tolerance.
    #[must_use]
    pub fn flat_rotation_config(&self) -> SoftDimConfig {
        SoftDimConfig::new(self.flat_rotation_tolerance_deg, self.flat_rotation_max_penalty)
    }

    /// `SoftDimConfig` for flat observing-night proximity tolerance.
    #[must_use]
    pub fn flat_night_config(&self) -> SoftDimConfig {
        SoftDimConfig::new(self.flat_night_tolerance_nights, self.flat_night_max_penalty)
    }

    /// `SoftDimConfig` for dark/bias master age tolerance.
    #[must_use]
    pub fn age_config(&self) -> SoftDimConfig {
        SoftDimConfig::new(self.age_limit_days, self.age_max_penalty)
    }
}

// ── Ranking ───────────────────────────────────────────────────────────────────

/// Rank a non-finite confidence last.
///
/// `partial_cmp(&NaN)` returns `None`, and the `unwrap_or(Ordering::Equal)` that
/// used to follow it left such a pair in input order, so a NaN confidence could
/// hold the top slot — and then suppress the ambiguity warning in
/// [`suggest_status`], because `(NaN - second).abs() < 0.05` is also false. A
/// confidence that is not a real number cannot be compared, so it is ranked
/// below every real one instead.
fn confidence_key(confidence: f64) -> f64 {
    if confidence.is_finite() {
        confidence
    } else {
        f64::NEG_INFINITY
    }
}

/// Sort `CalibrationMatch` list in-place:
/// 1. Descending confidence, with non-finite confidences last.
/// 2. Ascending `SelectionReason::priority()` (same_session > same_night > compatible_fallback).
pub fn rank_matches(matches: &mut [CalibrationMatch]) {
    matches.sort_by(|a, b| {
        confidence_key(b.confidence)
            .total_cmp(&confidence_key(a.confidence))
            .then_with(|| a.selection_reason.priority().cmp(&b.selection_reason.priority()))
    });
}

/// Classify the suggestion status based on ranked matches.
///
/// Returns `"match"`, `"ambiguous"`, or `"no_match"`.
#[must_use]
pub fn suggest_status(matches: &[CalibrationMatch]) -> &'static str {
    match matches.len() {
        0 => "no_match",
        // Same rule as the multi-candidate arm below: a confidence that is not a
        // real number cannot be shown to be a clear match, and having no runner-up
        // does not make it one. Without this, a lone NaN-confidence candidate was
        // reported as a settled match while two of them were reported ambiguous.
        1 if !matches[0].confidence.is_finite() => "ambiguous",
        1 => "match",
        _ => {
            // Ambiguous when top two are within 0.05 confidence.
            let top = matches[0].confidence;
            let second = matches[1].confidence;
            if !top.is_finite() || !second.is_finite() {
                // A confidence that is not a real number cannot be shown to be
                // clear of its runner-up, so the user decides rather than the app.
                return "ambiguous";
            }
            if (top - second).abs() < 0.05 {
                "ambiguous"
            } else {
                "match"
            }
        }
    }
}

// ── Selection reason helper ───────────────────────────────────────────────────

/// Determine the `SelectionReason` for a flat master given session/master observing dates.
#[must_use]
pub fn flat_selection_reason(
    session_night: Option<&str>,
    master_night: Option<&str>,
    session_id: &str,
    master_source_session_id: Option<&str>,
) -> SelectionReason {
    // Same session takes highest priority.
    if let Some(src_id) = master_source_session_id {
        if src_id == session_id {
            return SelectionReason::SameSession;
        }
    }
    // Same observing night.
    if let (Some(sn), Some(mn)) = (session_night, master_night) {
        if sn == mn {
            return SelectionReason::SameNight;
        }
    }
    SelectionReason::CompatibleFallback
}

/// Compute observing-night distance in nights (0 = same night).
///
/// Accepts ISO-8601 date strings (YYYY-MM-DD). Returns `None` on parse failure.
///
/// Tokenization + day arithmetic (T201) use `time::Date`: `Date::parse` against
/// the `[year]-[month]-[day]` description and `Date::to_julian_day`, which
/// yields the same proleptic-Gregorian Julian Day Number as the prior
/// hand-rolled algorithm, so the night distance is unchanged.
#[must_use]
pub fn night_distance(date_a: &str, date_b: &str) -> Option<f64> {
    // Zero-padded `YYYY-MM-DD`, matching the DB-stored observing-night format.
    let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").ok()?;
    let parse = |s: &str| -> Option<time::Date> { time::Date::parse(s, &format).ok() };

    let da = parse(date_a)?.to_julian_day();
    let db = parse(date_b)?.to_julian_day();
    // `to_julian_day` returns an i32 JDN; the absolute day difference is the
    // observing-night distance. Cast to f64 is exact for the i32 day range.
    #[allow(clippy::cast_precision_loss)]
    Some(i64::from((da - db).abs()) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::{CalibrationMatch, SelectionReason};
    use crate::CalibrationKind;

    fn make_match(confidence: f64, reason: SelectionReason) -> CalibrationMatch {
        CalibrationMatch::new(
            "ses".to_owned(),
            "master".to_owned(),
            CalibrationKind::Dark,
            confidence,
            vec![],
            vec![],
            reason,
        )
    }

    #[test]
    fn rank_matches_descending_confidence() {
        let mut v = vec![
            make_match(0.7, SelectionReason::CompatibleFallback),
            make_match(1.0, SelectionReason::CompatibleFallback),
            make_match(0.5, SelectionReason::CompatibleFallback),
        ];
        rank_matches(&mut v);
        assert!((v[0].confidence - 1.0).abs() < 1e-9);
        assert!((v[1].confidence - 0.7).abs() < 1e-9);
        assert!((v[2].confidence - 0.5).abs() < 1e-9);
    }

    #[test]
    fn rank_matches_tiebreak_by_selection_reason() {
        let mut v = vec![
            make_match(0.9, SelectionReason::CompatibleFallback),
            make_match(0.9, SelectionReason::SameSession),
            make_match(0.9, SelectionReason::SameNight),
        ];
        rank_matches(&mut v);
        assert_eq!(v[0].selection_reason, SelectionReason::SameSession);
        assert_eq!(v[1].selection_reason, SelectionReason::SameNight);
        assert_eq!(v[2].selection_reason, SelectionReason::CompatibleFallback);
    }

    #[test]
    fn rank_matches_puts_a_nan_confidence_last() {
        // `CalibrationMatch::new` clamps both infinities into 0.0..=1.0, but
        // `f64::clamp` returns NaN for a NaN input, so NaN is the one non-finite
        // confidence that reaches the ranker.
        let mut v = vec![
            make_match(f64::NAN, SelectionReason::CompatibleFallback),
            make_match(0.4, SelectionReason::CompatibleFallback),
            make_match(0.9, SelectionReason::CompatibleFallback),
        ];
        rank_matches(&mut v);
        assert!((v[0].confidence - 0.9).abs() < 1e-9, "NaN outranked 0.9");
        assert!((v[1].confidence - 0.4).abs() < 1e-9, "NaN outranked 0.4");
        assert!(v[2].confidence.is_nan());
    }

    #[test]
    fn the_constructor_clamps_an_infinite_confidence_before_ranking_sees_it() {
        assert!(
            (make_match(f64::INFINITY, SelectionReason::SameSession).confidence - 1.0).abs() < 1e-9
        );
        assert!(
            make_match(f64::NEG_INFINITY, SelectionReason::SameSession).confidence.abs() < 1e-9
        );
    }

    #[test]
    fn suggest_status_no_match() {
        assert_eq!(suggest_status(&[]), "no_match");
    }

    #[test]
    fn suggest_status_is_ambiguous_when_a_confidence_is_not_a_number() {
        // A NaN top used to read as a clear winner: (NaN - 0.1).abs() < 0.05 is
        // false, so the ambiguity warning never fired.
        let a = make_match(f64::NAN, SelectionReason::CompatibleFallback);
        let b = make_match(0.1, SelectionReason::CompatibleFallback);
        assert_eq!(suggest_status(&[a, b]), "ambiguous");

        let c = make_match(0.9, SelectionReason::CompatibleFallback);
        let d = make_match(f64::NAN, SelectionReason::CompatibleFallback);
        assert_eq!(suggest_status(&[c, d]), "ambiguous");
    }

    #[test]
    fn penalty_rejects_a_delta_that_is_not_a_usable_number() {
        let cfg = SoftDimConfig::new(2.0, 0.4);
        for delta in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.5] {
            assert_eq!(cfg.penalty(delta), None, "delta {delta} was scored as a match");
        }
    }

    #[test]
    fn penalty_rejects_a_tolerance_that_is_not_a_usable_number() {
        // NaN and INFINITY used to return Some: `delta > tolerance` is false
        // against both. The negative cases already returned None and are here to
        // pin that they still do — the guard moved, the behaviour did not.
        for tolerance in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            let cfg = SoftDimConfig::new(tolerance, 0.4);
            assert_eq!(cfg.penalty(1.0), None, "tolerance {tolerance} admitted a delta of 1.0");
        }
        assert_eq!(SoftDimConfig::new(2.0, f64::NAN).penalty(1.0), None);
    }

    #[test]
    fn penalty_boundary_is_inclusive_and_the_result_stays_within_zero_and_one() {
        let cfg = SoftDimConfig::new(2.0, 0.4);
        assert_eq!(cfg.penalty(0.0), Some(0.0));
        assert_eq!(cfg.penalty(1.0), Some(0.2));
        assert_eq!(cfg.penalty(2.0), Some(0.4));
        assert_eq!(cfg.penalty(2.000_001), None);
        // A max_penalty above 1.0 is a misconfiguration; the penalty is still a
        // confidence delta, so it cannot exceed the whole confidence range.
        assert_eq!(SoftDimConfig::new(2.0, 5.0).penalty(2.0), Some(1.0));
        // A zero tolerance means exact-match-only, and reports no penalty.
        assert_eq!(SoftDimConfig::new(0.0, 0.4).penalty(0.0), Some(0.0));
        assert_eq!(SoftDimConfig::new(0.0, 0.4).penalty(0.1), None);
    }

    #[test]
    fn suggest_status_single_match() {
        let m = make_match(0.9, SelectionReason::CompatibleFallback);
        assert_eq!(suggest_status(&[m]), "match");
    }

    /// The lone-candidate arm had no finiteness check, so it reported a settled
    /// match for exactly the confidence the multi-candidate arm calls ambiguous.
    ///
    /// NaN only: `CalibrationMatch::new` clamps confidence into `[0.0, 1.0]`, so
    /// `INFINITY` arrives as `1.0` and `NEG_INFINITY` as `0.0` — both finite, both
    /// legitimately a match. `f64::clamp` passes a NaN through, which is why that
    /// one value still reaches here.
    #[test]
    fn suggest_status_is_ambiguous_for_a_lone_candidate_with_no_real_confidence() {
        let m = make_match(f64::NAN, SelectionReason::CompatibleFallback);
        assert_eq!(suggest_status(&[m]), "ambiguous");
    }

    /// Pins the clamp this file depends on: if `new` ever stops sanitizing the
    /// infinities, the guard above needs to cover them too.
    #[test]
    fn an_infinite_confidence_is_clamped_before_it_reaches_suggest_status() {
        for (input, expected) in [(f64::INFINITY, 1.0), (f64::NEG_INFINITY, 0.0)] {
            let m = make_match(input, SelectionReason::CompatibleFallback);
            assert!(
                (m.confidence - expected).abs() < f64::EPSILON,
                "{input} became {}",
                m.confidence
            );
            assert_eq!(suggest_status(std::slice::from_ref(&m)), "match");
        }
    }

    #[test]
    fn suggest_status_ambiguous_when_close() {
        let a = make_match(0.9, SelectionReason::CompatibleFallback);
        let b = make_match(0.88, SelectionReason::CompatibleFallback);
        assert_eq!(suggest_status(&[a, b]), "ambiguous");
    }

    #[test]
    fn suggest_status_match_when_clear_winner() {
        let a = make_match(0.9, SelectionReason::CompatibleFallback);
        let b = make_match(0.5, SelectionReason::CompatibleFallback);
        assert_eq!(suggest_status(&[a, b]), "match");
    }

    #[test]
    fn soft_dim_config_exact_zero_penalty() {
        let cfg = SoftDimConfig::new(2.0, 0.4);
        assert_eq!(cfg.penalty(0.0), Some(0.0));
    }

    #[test]
    fn soft_dim_config_midpoint_penalty() {
        let cfg = SoftDimConfig::new(2.0, 0.4);
        let p = cfg.penalty(1.0).unwrap();
        assert!((p - 0.2).abs() < 1e-9);
    }

    #[test]
    fn soft_dim_config_over_tolerance_is_none() {
        let cfg = SoftDimConfig::new(2.0, 0.4);
        assert!(cfg.penalty(3.0).is_none());
    }

    #[test]
    fn night_distance_same_night() {
        assert_eq!(night_distance("2026-01-15", "2026-01-15"), Some(0.0));
    }

    #[test]
    fn night_distance_one_night() {
        assert_eq!(night_distance("2026-01-16", "2026-01-15"), Some(1.0));
    }

    #[test]
    fn flat_selection_reason_same_session() {
        let r = flat_selection_reason(
            Some("2026-01-15"),
            Some("2026-01-15"),
            "ses-001",
            Some("ses-001"),
        );
        assert_eq!(r, SelectionReason::SameSession);
    }

    #[test]
    fn flat_selection_reason_same_night() {
        let r = flat_selection_reason(
            Some("2026-01-15"),
            Some("2026-01-15"),
            "ses-001",
            Some("ses-002"),
        );
        assert_eq!(r, SelectionReason::SameNight);
    }

    #[test]
    fn flat_selection_reason_compatible_fallback() {
        let r = flat_selection_reason(
            Some("2026-01-15"),
            Some("2026-01-10"),
            "ses-001",
            Some("ses-002"),
        );
        assert_eq!(r, SelectionReason::CompatibleFallback);
    }
}
