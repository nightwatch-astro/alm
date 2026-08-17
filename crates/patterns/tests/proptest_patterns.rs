// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Proptest invariant suite for `crates/patterns`.
//!
//! Invariants tested:
//! 1. No panic on arbitrary `PatternPart` input (resolve + validate).
//! 2. Resolver output (relative_path) never contains a `..` segment.
//! 3. On a single-token pattern with a full metadata value, `missing_tokens`
//!    is empty — the registry vocabulary is the complete token set.
//! 4. Fallback substitution: when metadata is absent, `missing_tokens` always
//!    lists the token name AND the resolved path still succeeds.
//! 5. `resolve_pattern_str` on well-formed patterns never produces a path that
//!    starts with `/` (relative_path is always relative).
//! 6. `resolve_pattern_str` with `follow_symlinks=false` containment: the
//!    function never returns a relative_path containing `..` on valid input.

#![allow(clippy::doc_markdown)]

use std::collections::HashMap;

use patterns::{
    resolve_pattern_str, resolve_v1, MetadataBundle, PatternPart, ResolveError, V1_REGISTRY,
    VALID_SEPARATORS,
};
use proptest::prelude::*;

// ── V1 token vocabulary ────────────────────────────────────────────────────

/// All registered token names in the v1 vocabulary.
const V1_TOKENS: &[&str] = &[
    "target",
    "filter",
    "date",
    "frame_type",
    "camera",
    "exposure",
    "gain",
    "binning",
    "set_temp",
];

// ── Generators ─────────────────────────────────────────────────────────────

/// Strategy producing an arbitrary `PatternPart` — may be valid or not.
fn arb_pattern_part() -> impl Strategy<Value = PatternPart> {
    (
        any::<String>(), // id
        prop_oneof![Just("token".to_owned()), Just("separator".to_owned()), any::<String>()],
        any::<String>(), // value
    )
        .prop_map(|(id, kind, value)| PatternPart { id, kind, value })
}

/// Strategy producing a valid `PatternPart` token (registered name).
fn arb_valid_token() -> impl Strategy<Value = PatternPart> {
    (any::<String>(), proptest::sample::select(V1_TOKENS)).prop_map(|(id, name)| PatternPart {
        id,
        kind: "token".to_owned(),
        value: name.to_owned(),
    })
}

/// Strategy producing a valid separator part.
fn arb_valid_sep() -> impl Strategy<Value = PatternPart> {
    (any::<String>(), proptest::sample::select(VALID_SEPARATORS)).prop_map(|(id, sep)| {
        PatternPart { id, kind: "separator".to_owned(), value: sep.to_owned() }
    })
}

/// Strategy producing a pattern that is structurally valid (all tokens and
/// separators are from the v1 vocabulary) — length 1–6 parts.
fn arb_valid_pattern() -> impl Strategy<Value = Vec<PatternPart>> {
    prop::collection::vec(prop_oneof![arb_valid_token(), arb_valid_sep()], 1..=6)
}

/// Strategy producing a clean ASCII metadata value that passes sanitization.
///
/// Excludes Windows-reserved chars, leading/trailing dots/spaces, and the
/// exact strings "." and ".." to avoid triggering SanitizeError.
fn arb_clean_meta_value() -> impl Strategy<Value = String> {
    // Mid-segment: alphanumeric + harmless punctuation only; no leading/trailing dot.
    "[A-Za-z0-9][A-Za-z0-9_\\-]{0,18}"
}

/// Strategy producing a `MetadataBundle` with one clean value per v1 token.
///
/// Uses `[Strategy; N].prop_map(...)` to generate one independent value per
/// token, avoiding the duplicate-key problem that `hash_map(select(...))` hits.
fn arb_full_meta() -> impl Strategy<Value = MetadataBundle> {
    // Generate one clean value for each of the 9 v1 tokens.
    [
        arb_clean_meta_value(),
        arb_clean_meta_value(),
        arb_clean_meta_value(),
        arb_clean_meta_value(),
        arb_clean_meta_value(),
        arb_clean_meta_value(),
        arb_clean_meta_value(),
        arb_clean_meta_value(),
        arb_clean_meta_value(),
    ]
    .prop_map(|vals| {
        V1_TOKENS
            .iter()
            .zip(vals.iter())
            .map(|(&name, val)| (name.to_owned(), val.clone()))
            .collect::<MetadataBundle>()
    })
}

// ── Invariant 1: no panic on arbitrary input ──────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `resolve_v1` and `validate` must never panic regardless of what
    /// `PatternPart` values are fed in.
    #[test]
    fn no_panic_on_arbitrary_pattern(parts in prop::collection::vec(arb_pattern_part(), 0..=8)) {
        let _ = resolve_v1(&parts, &HashMap::new());
        let _ = patterns::validate(&parts, &V1_REGISTRY);
    }
}

// ── Invariant 2: no `..` segment in resolved relative_path ────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// When resolution succeeds the relative_path MUST NOT contain a `..`
    /// path segment.  This is the traversal-guard invariant.
    #[test]
    fn resolved_path_never_contains_dotdot(
        parts in arb_valid_pattern(),
        meta in arb_full_meta(),
    ) {
        if let Ok(result) = resolve_v1(&parts, &meta) {
            for segment in result.relative_path.split('/') {
                prop_assert_ne!(
                    segment, "..",
                    "relative_path contained a .. segment: {}",
                    result.relative_path
                );
                prop_assert_ne!(
                    segment, ".",
                    "relative_path contained a . segment: {}",
                    result.relative_path
                );
            }
        }
    }
}

// ── Invariant 3: full metadata → missing_tokens is empty ─────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A single-token pattern with a clean metadata value for that token must
    /// resolve successfully with an empty `missing_tokens` list.
    ///
    /// This encodes: the v1 registry vocabulary is the complete allowed set —
    /// if you supply all token values, nothing should fall back.
    #[test]
    fn single_token_full_meta_has_no_missing(
        token_name in proptest::sample::select(V1_TOKENS),
        meta_val in arb_clean_meta_value(),
    ) {
        let part = PatternPart {
            id: "t".to_owned(),
            kind: "token".to_owned(),
            value: token_name.to_owned(),
        };
        let def = V1_REGISTRY.get(token_name).expect("token must be in registry");
        let mut meta = MetadataBundle::new();
        meta.insert(def.source_field.to_owned(), meta_val);

        let result = resolve_v1(&[part], &meta);
        match result {
            Ok(r) => prop_assert!(
                r.missing_tokens.is_empty(),
                "token '{}' with full metadata should not be in missing_tokens, got: {:?}",
                token_name,
                r.missing_tokens
            ),
            // A sanitization hard error (reserved name, traversal, confusable) is
            // acceptable — the `arb_clean_meta_value` strategy is conservative but
            // doesn't exhaustively rule out all edge cases.
            Err(ResolveError::ReservedName { .. }
                | ResolveError::PathTraversal { .. }
                | ResolveError::UnicodeConfusable { .. }) => {}
            Err(e) => prop_assert!(false, "unexpected error: {e:?}"),
        }
    }
}

// ── Invariant 4: missing metadata → token appears in missing_tokens ───────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// When a token's source field is absent from the metadata bundle, the
    /// resolver must still succeed (using the fallback) AND the token name must
    /// appear in `missing_tokens`.
    #[test]
    fn absent_token_reported_in_missing(
        token_name in proptest::sample::select(V1_TOKENS),
    ) {
        let part = PatternPart {
            id: "t".to_owned(),
            kind: "token".to_owned(),
            value: token_name.to_owned(),
        };
        // Intentionally empty metadata so the token will fall back.
        let meta = MetadataBundle::new();

        let result = resolve_v1(&[part], &meta).expect("fallback must always succeed");
        prop_assert!(
            result.missing_tokens.contains(&token_name.to_owned()),
            "token '{}' should be in missing_tokens when metadata absent, got: {:?}",
            token_name,
            result.missing_tokens
        );
    }
}

// ── Invariant 5: resolve_pattern_str never returns an absolute path ────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `resolve_pattern_str` must not produce a relative_path that starts with
    /// `/` regardless of how many leading slashes the input has.
    #[test]
    fn pattern_str_result_is_relative(
        prefix_slashes in 0usize..=3,
        token_name in proptest::sample::select(V1_TOKENS),
        meta_val in arb_clean_meta_value(),
    ) {
        let slashes: String = "/".repeat(prefix_slashes);
        let pattern = format!("{slashes}{{{token_name}}}/");

        let def = V1_REGISTRY.get(token_name).expect("token must be in registry");
        let mut meta = MetadataBundle::new();
        meta.insert(def.source_field.to_owned(), meta_val);

        if let Ok(result) = resolve_pattern_str(&pattern, &meta) {
            prop_assert!(
                !result.relative_path.starts_with('/'),
                "relative_path must not start with '/', got: {}",
                result.relative_path
            );
        }
    }
}

// ── Invariant 6: resolve_pattern_str result never contains `..` ───────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `resolve_pattern_str` on well-formed token+literal patterns must never
    /// emit a `..` segment in the resolved path (traversal guard).
    #[test]
    fn pattern_str_result_has_no_dotdot(
        token_name in proptest::sample::select(V1_TOKENS),
    ) {
        let pattern = format!("archive/{{{token_name}}}/");
        let meta = MetadataBundle::new(); // triggers fallback path

        if let Ok(result) = resolve_pattern_str(&pattern, &meta) {
            for segment in result.relative_path.split('/') {
                prop_assert_ne!(
                    segment, "..",
                    "resolve_pattern_str produced a .. segment: {}",
                    result.relative_path
                );
            }
        }
    }
}
