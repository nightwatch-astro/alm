// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-type matching rules for the calibration engine.
pub mod bias;
pub mod dark;
pub mod flat;

use crate::candidate::{MatchedDim, MismatchedDim};
use crate::Dimension;

/// Float-equality tolerance for hard-rule numeric dimensions (gain, offset).
/// Guards against floating-point representation noise from FITS/XISF header
/// parsing — this is not a tuning knob (unlike the soft-dimension tolerances
/// in [`crate::ranking::MatchingRuleConfig`]).
pub const HARD_RULE_EPSILON: f64 = 1e-9;

/// Hard-rule numeric dimension check shared by the gain/offset comparisons in
/// `bias`/`dark`/`flat` and by `assign::collect_hard_violations`: `true` only
/// when both values are present and equal within [`HARD_RULE_EPSILON`].
/// Missing either side always excludes.
#[must_use]
pub fn hard_rule_numeric(session_val: Option<f64>, master_val: Option<f64>) -> bool {
    matches!((session_val, master_val), (Some(s), Some(m)) if (s - m).abs() < HARD_RULE_EPSILON)
}

/// Hard-rule string dimension check shared by the filter/binning/optic_train
/// comparisons in `flat` and by `assign::collect_hard_violations`: `true`
/// only when both values are present and exactly equal. Missing either side
/// always excludes.
#[must_use]
pub fn hard_rule_string(session_val: Option<&str>, master_val: Option<&str>) -> bool {
    matches!((session_val, master_val), (Some(s), Some(m)) if s == m)
}

/// Confidence penalty applied when a relaxed hard rule lets a mismatched or
/// missing dimension through. Shared by gain, offset, and binning so a relaxed
/// toggle costs the same wherever it is set.
pub const RELAXED_HARD_RULE_PENALTY: f64 = 0.2;

/// Numeric hard rule that the user can relax, shared by the gain and offset
/// comparisons in `bias`/`dark`/`flat`.
///
/// Returns `None` to exclude the candidate, or `Some(penalty)` to subtract from
/// its confidence. With `required` true the dimension behaves as an exact hard
/// rule; with it false a mismatch becomes an out-of-tolerance entry and a
/// missing value a metadata-missing entry, each costing
/// [`RELAXED_HARD_RULE_PENALTY`].
pub fn relaxable_numeric(
    dimension: Dimension,
    session_val: Option<f64>,
    master_val: Option<f64>,
    required: bool,
    matched: &mut Vec<MatchedDim>,
    mismatched: &mut Vec<MismatchedDim>,
) -> Option<f64> {
    match (session_val, master_val) {
        (Some(s), Some(m)) if (s - m).abs() < HARD_RULE_EPSILON => {
            matched.push(MatchedDim::exact(dimension));
            Some(0.0)
        }
        _ if required => None,
        (Some(s), Some(m)) => {
            mismatched.push(MismatchedDim::out_of_tolerance(dimension, (s - m).abs()));
            Some(RELAXED_HARD_RULE_PENALTY)
        }
        _ => {
            mismatched.push(MismatchedDim::metadata_missing(dimension));
            Some(RELAXED_HARD_RULE_PENALTY)
        }
    }
}

/// String hard rule that the user can relax, used by the flat binning
/// comparison. Filter and optic train stay unconditional.
///
/// Mirrors [`relaxable_numeric`]: `None` excludes, `Some(penalty)` keeps the
/// candidate at reduced confidence.
pub fn relaxable_string(
    dimension: Dimension,
    session_val: Option<&str>,
    master_val: Option<&str>,
    required: bool,
    matched: &mut Vec<MatchedDim>,
    mismatched: &mut Vec<MismatchedDim>,
) -> Option<f64> {
    match (session_val, master_val) {
        (Some(s), Some(m)) if s == m => {
            matched.push(MatchedDim::exact_string(dimension, s));
            Some(0.0)
        }
        _ if required => None,
        (Some(_), Some(_)) => {
            mismatched.push(MismatchedDim::hard(dimension));
            Some(RELAXED_HARD_RULE_PENALTY)
        }
        _ => {
            mismatched.push(MismatchedDim::metadata_missing(dimension));
            Some(RELAXED_HARD_RULE_PENALTY)
        }
    }
}

/// Soft age rule shared by `dark` and `bias`: penalise a master whose observing
/// night is far from the light session's, bounded by
/// [`MatchingRuleConfig::age_limit_days`](crate::ranking::MatchingRuleConfig::age_limit_days).
///
/// Returns the confidence penalty to subtract, pushing at most one
/// [`Dimension::DateProximity`] entry. An unknown observing night on either
/// side is not evidence of an old master, so the dimension is skipped and no
/// penalty applies — this keeps age-unaware callers on their previous scores.
pub fn apply_age_rule(
    session_night: Option<&str>,
    master_night: Option<&str>,
    config: &crate::ranking::MatchingRuleConfig,
    matched: &mut Vec<MatchedDim>,
    mismatched: &mut Vec<MismatchedDim>,
) -> f64 {
    let (Some(sd), Some(md)) = (session_night, master_night) else {
        return 0.0;
    };
    let Some(age_days) = crate::ranking::night_distance(sd, md) else {
        return 0.0;
    };
    let cfg = config.age_config();
    if let Some(penalty) = cfg.penalty(age_days) {
        matched.push(MatchedDim::soft(Dimension::DateProximity, age_days, 0.0, age_days));
        penalty
    } else {
        mismatched.push(MismatchedDim::out_of_tolerance(Dimension::DateProximity, age_days));
        cfg.max_penalty
    }
}
