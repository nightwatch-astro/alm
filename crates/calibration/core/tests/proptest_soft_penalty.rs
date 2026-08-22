// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Proptest invariant suite for `SoftDimConfig::penalty`.
//!
//! A `Some(_)` return is recorded by the rule evaluators as a MATCHED soft
//! dimension and its value is subtracted from confidence, so both the domain of
//! `Some` and the range of the penalty are correctness-bearing.

use calibration_core::ranking::SoftDimConfig;
use proptest::prelude::*;

/// Deltas and configs spanning negatives, zero, subnormals, infinities, and
/// NaN — `proptest::num::f64::ANY` excludes the non-finite cases that the
/// guards exist for.
fn any_f64() -> impl Strategy<Value = f64> {
    prop_oneof![
        9 => proptest::num::f64::NORMAL,
        1 => prop_oneof![
            Just(f64::NAN),
            Just(f64::INFINITY),
            Just(f64::NEG_INFINITY),
            Just(0.0_f64),
            Just(-0.0_f64),
            Just(f64::MIN_POSITIVE),
        ],
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// A penalty is a deduction. A negative return value would add confidence
    /// to a master that deviates from the session more than an exact match
    /// does, letting it outrank the exact match.
    #[test]
    fn negative_delta_never_yields_a_confidence_increase(
        delta in any_f64().prop_filter("negative", |d| d.is_nan() || *d < 0.0),
        tolerance in any_f64(),
        max_penalty in any_f64(),
    ) {
        let config = SoftDimConfig::new(tolerance, max_penalty);
        prop_assert_eq!(config.penalty(delta), None);
    }

    /// Every returned penalty is a real number in `0.0..=1.0`, whatever the
    /// user-configured tolerance and `max_penalty` are.
    #[test]
    fn any_delta_respects_the_penalty_bound(
        delta in any_f64(),
        tolerance in any_f64(),
        max_penalty in any_f64(),
    ) {
        let config = SoftDimConfig::new(tolerance, max_penalty);
        if let Some(penalty) = config.penalty(delta) {
            prop_assert!(penalty.is_finite(), "penalty {penalty} is not finite");
            prop_assert!((0.0..=1.0).contains(&penalty), "penalty {penalty} out of bounds");
        }
    }

    /// Every comparison against a NaN is false, so an unguarded `delta >
    /// tolerance` check takes the inside-tolerance branch. A NaN on any of the
    /// three inputs must yield `None`, never a MATCHED dimension.
    #[test]
    fn nan_delta_is_not_treated_as_inside_tolerance(
        delta in any_f64(),
        tolerance in any_f64(),
        max_penalty in any_f64(),
    ) {
        if delta.is_nan() || tolerance.is_nan() || max_penalty.is_nan() {
            let config = SoftDimConfig::new(tolerance, max_penalty);
            prop_assert_eq!(config.penalty(delta), None);
        }
    }
}
