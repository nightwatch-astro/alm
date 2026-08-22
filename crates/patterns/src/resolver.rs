// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Pattern resolver (spec 015 T3.4 + T3.6).
//!
//! `resolve(pattern, metadata, config) -> Result<ResolveResult, ResolveError>`
//!
//! Steps:
//! 1. Validate the pattern (returns `ResolveError` on hard structural failure).
//! 2. For each part:
//!    - Token: look up source_field in metadata, sanitize, apply transform,
//!      fall back on absent/empty, accumulate missing_tokens.
//!    - Separator: emit the literal (separators are not sanitized beyond
//!      what the validator already checked).
//! 3. Assemble the relative path as a forward-slash string.
//! 4. Post-resolution checks:
//!    - Assembled path must not contain `..` segments (traversal guard).
//!    - Every segment must be ≤ 200 UTF-8 bytes.
//!    - Total relative path must be ≤ 200 chars.

use std::collections::HashMap;

use safe_filename::{sanitize_token_value, SanitizeError};

use crate::registry::{TokenRegistry, TokenTransform};
use crate::validator::{validate, ValidationWarning};
use crate::V1_REGISTRY;

// ── MetadataBundle ─────────────────────────────────────────────────────────

/// A flat map of source-field name → string value (data-model.md §MetadataBundle).
///
/// Keys correspond to `TokenDefinition.source_field`. Absent keys trigger
/// fallback substitution.
pub type MetadataBundle = HashMap<String, String>;

// ── ResolverConfig ─────────────────────────────────────────────────────────

/// Configuration for the resolver.
#[derive(Clone, Debug, Default)]
pub struct ResolverConfig {
    /// Maximum UTF-8 byte length for a single path segment. Default: 200.
    pub max_segment_bytes: Option<usize>,
    /// Maximum total path character length. Default: 200.
    pub max_path_chars: Option<usize>,
}

impl ResolverConfig {
    #[must_use]
    pub fn max_segment_bytes(&self) -> usize {
        self.max_segment_bytes.unwrap_or(200)
    }

    #[must_use]
    pub fn max_path_chars(&self) -> usize {
        self.max_path_chars.unwrap_or(200)
    }
}

// ── ResolveResult ─────────────────────────────────────────────────────────

/// Successful resolution output (data-model.md §ResolveResult).
#[derive(Clone, Debug)]
pub struct ResolveResult {
    /// Forward-slash relative path; never starts with `/` or a drive letter.
    pub relative_path: String,
    /// Token names that were resolved via fallback (including `"date"` when
    /// the caller supplied the UTC date instead of the local date).
    pub missing_tokens: Vec<String>,
    /// Non-fatal structural warnings forwarded from the validator.
    pub warnings: Vec<ValidationWarning>,
}

// ── ResolveError ──────────────────────────────────────────────────────────

/// Errors that prevent a successful resolution (data-model.md §Errors).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    /// Pattern is empty (data-model.md `pattern.empty`).
    #[error("pattern is empty")]
    Empty,
    /// Pattern references an unregistered token name (data-model.md `token.unknown`).
    #[error("unknown token: {token}")]
    UnknownToken { token: String },
    /// A resolved token value contains `.` or `..`, or the path contains
    /// a `..` segment (data-model.md `path.traversal`).
    #[error("path traversal in segment: {segment}")]
    PathTraversal { segment: String },
    /// A path segment matches a Windows reserved device name
    /// (data-model.md `path.reserved_name`).
    #[error("Windows reserved device name: {segment}")]
    ReservedName { segment: String },
    /// A resolved token value contains Unicode confusables or disallowed chars
    /// (data-model.md `pattern.invalid.unicode`).
    #[error("Unicode confusable in token {token}: {value}")]
    UnicodeConfusable { token: String, value: String },
    /// The resolved path violates length limits
    /// (data-model.md `pattern.invalid`).
    #[error("path length violated: resolved_length={resolved_length}, segment_length_bytes={segment_length_bytes}")]
    PathTooLong { resolved_length: usize, segment_length_bytes: usize },
}

// ── resolve ───────────────────────────────────────────────────────────────

/// Resolve `pattern` against `metadata` using `registry` and `config`.
///
/// Uses [`V1_REGISTRY`] when you pass `registry = &V1_REGISTRY`.
///
/// # Errors
///
/// Returns [`ResolveError`] when the pattern is empty, references an unknown
/// token, produces a traversal path, hits a reserved name, contains Unicode
/// confusables, or violates length limits.
pub fn resolve(
    pattern: &[crate::PatternPart],
    metadata: &MetadataBundle,
    registry: &TokenRegistry,
    config: &ResolverConfig,
) -> Result<ResolveResult, ResolveError> {
    // 1. Structural validation (hard errors only).
    let validation = validate(pattern, registry);
    if !validation.valid {
        // Map the first hard error.
        if let Some(e) = validation.errors.first() {
            return Err(map_validate_error(e));
        }
    }

    let mut parts: Vec<String> = Vec::with_capacity(pattern.len());
    let mut missing_tokens: Vec<String> = Vec::new();

    // 2. Resolve each part.
    for part in pattern {
        match part.kind.as_str() {
            "token" => {
                let def = registry
                    .get(&part.value)
                    .ok_or_else(|| ResolveError::UnknownToken { token: part.value.clone() })?;

                // Look up the source field in the metadata bundle.
                let raw_opt = metadata.get(def.source_field).filter(|s| !s.is_empty());

                let (raw, is_fallback) =
                    if let Some(v) = raw_opt { (v.as_str(), false) } else { (def.fallback, true) };

                if is_fallback {
                    missing_tokens.push(part.value.clone());
                }

                // Apply transform before sanitize (some transforms can produce
                // different chars; sanitize runs after).
                let transformed = apply_transform(raw, def.transform);

                // Sanitize.
                let sanitized =
                    sanitize_token_value(&part.value, &transformed).map_err(map_sanitize_error)?;

                // If sanitize ate the entire value, use the fallback.
                let final_value = if sanitized.is_empty() {
                    if !is_fallback {
                        missing_tokens.push(part.value.clone());
                    }
                    def.fallback.to_owned()
                } else {
                    sanitized
                };

                parts.push(final_value);
            }
            "separator" => {
                // Separators are already validated; emit as-is.
                parts.push(part.value.clone());
            }
            _ => {}
        }
    }

    // 3. Assemble relative path.
    let relative_path: String = parts.concat();

    // 4. Traversal guard and length caps on the assembled path.
    check_assembled_path(&relative_path, config)?;

    Ok(ResolveResult { relative_path, missing_tokens, warnings: validation.warnings })
}

/// Convenience wrapper that uses [`V1_REGISTRY`] and default config.
///
/// # Errors
/// See [`resolve`].
pub fn resolve_v1(
    pattern: &[crate::PatternPart],
    metadata: &MetadataBundle,
) -> Result<ResolveResult, ResolveError> {
    resolve(pattern, metadata, &V1_REGISTRY, &ResolverConfig::default())
}

// ── Path-string resolver (spec 041 FR-026a) ───────────────────────────────

/// Resolve a per-type destination pattern expressed as a **path string** with
/// interleaved `{token}` placeholders and literal directory segments.
///
/// Unlike [`resolve`]/[`resolve_v1`] (which operate on the `PatternPart`
/// token/separator model), this handles the per-type defaults from spec 041
/// that contain literal segments, e.g. `flats/{filter}/{date}/`,
/// `masters/darks/{exposure}/`, or the literal `light` in
/// `{target}/{filter}/{date}/light/`.
///
/// Resolution rules:
/// - The pattern is split on `/` into segments. Each segment is walked
///   left-to-right; `{token}` placeholders and surrounding literal text are
///   resolved in place (multi-token / mixed segments are supported).
/// - A `{token}` resolves exactly as in [`resolve`]: look up the token in
///   [`V1_REGISTRY`], read its `source_field` from `bundle`, apply the token's
///   [`TokenTransform`], then [`sanitize_token_value`]. When the value is absent
///   (or sanitizes to empty), the registry `fallback` is used and the token name
///   is pushed onto `missing_tokens`.
/// - Literal text is sanitized for filesystem safety (same pipeline as token
///   values) and emitted verbatim; literals are **never** added to
///   `missing_tokens`.
///
/// `relative_path` follows the same convention as [`resolve`]: forward-slash
/// joined, no leading slash, and no empty trailing segment (a trailing `/` in
/// the pattern is dropped). Empty interior segments (from `//`) are skipped.
///
/// `missing_tokens` lists every token that fell back to its default — callers
/// (spec 041 confirm) gate plan generation on this list.
///
/// # Errors
///
/// Returns [`ResolveError::Empty`] when the pattern is blank, [`ResolveError::UnknownToken`]
/// for an unregistered token, [`ResolveError::PathTraversal`]/[`ResolveError::ReservedName`]/[`ResolveError::UnicodeConfusable`]
/// when a literal or resolved value is unsafe, or [`ResolveError::PathTooLong`]
/// when length caps are exceeded.
pub fn resolve_pattern_str(
    pattern: &str,
    bundle: &MetadataBundle,
) -> Result<ResolveResult, ResolveError> {
    resolve_pattern_str_with(pattern, bundle, &V1_REGISTRY, &ResolverConfig::default())
}

fn resolve_pattern_str_with(
    pattern: &str,
    bundle: &MetadataBundle,
    registry: &TokenRegistry,
    config: &ResolverConfig,
) -> Result<ResolveResult, ResolveError> {
    if pattern.trim().is_empty() {
        return Err(ResolveError::Empty);
    }

    let mut missing_tokens: Vec<String> = Vec::new();
    let mut segments: Vec<String> = Vec::new();

    for raw_segment in pattern.split('/') {
        if raw_segment.is_empty() {
            // Leading/trailing/double slash → empty segment; skip so the
            // joined path has no empty components (matches resolve()).
            continue;
        }
        let resolved = resolve_segment(raw_segment, bundle, registry, &mut missing_tokens)?;
        // A segment that sanitizes to empty (e.g. a literal of only dots) is
        // dropped rather than producing an empty path component.
        if !resolved.is_empty() {
            segments.push(resolved);
        }
    }

    let relative_path = segments.join("/");

    check_assembled_path(&relative_path, config)?;

    Ok(ResolveResult { relative_path, missing_tokens, warnings: Vec::new() })
}

/// Traversal guard and length caps applied to an assembled relative path.
///
/// Per-piece sanitization cannot see traversal that only appears once the
/// pieces are joined: a segment of `{filter}.` whose token value sanitizes to
/// `.` assembles to `..`. Both resolvers therefore re-check the assembled
/// string, and they must apply the same policy -- a resolver that skips this
/// hands its caller a destination that escapes the root it was resolved
/// against.
fn check_assembled_path(relative_path: &str, config: &ResolverConfig) -> Result<(), ResolveError> {
    for segment in relative_path.split('/') {
        if segment == ".." || segment == "." {
            return Err(ResolveError::PathTraversal { segment: segment.to_owned() });
        }
        if segment.is_empty() {
            continue; // Leading/trailing slash produces empty segments — skip.
        }
        let byte_len = segment.len();
        if byte_len > config.max_segment_bytes() {
            return Err(ResolveError::PathTooLong {
                resolved_length: relative_path.chars().count(),
                segment_length_bytes: byte_len,
            });
        }
    }

    let total_chars = relative_path.chars().count();
    if total_chars > config.max_path_chars() {
        return Err(ResolveError::PathTooLong {
            resolved_length: total_chars,
            segment_length_bytes: 0,
        });
    }

    Ok(())
}

/// Resolve one `/`-delimited segment that may interleave `{token}` placeholders
/// with literal text. Tokens append their resolved value; literal runs are
/// sanitized and appended verbatim. The result is the concatenation of the
/// pieces (a segment never contains an internal `/`).
fn resolve_segment(
    segment: &str,
    bundle: &MetadataBundle,
    registry: &TokenRegistry,
    missing_tokens: &mut Vec<String>,
) -> Result<String, ResolveError> {
    let mut out = String::new();
    let mut rest = segment;

    while !rest.is_empty() {
        match rest.find('{') {
            None => {
                // Trailing literal run.
                out.push_str(&sanitize_literal(rest)?);
                break;
            }
            Some(open) => {
                // Literal text before the brace.
                if open > 0 {
                    out.push_str(&sanitize_literal(&rest[..open])?);
                }
                let after = &rest[open + 1..];
                match after.find('}') {
                    None => {
                        // Unterminated `{` — treat the remainder as a literal
                        // (including the brace) so nothing is silently dropped.
                        out.push_str(&sanitize_literal(&rest[open..])?);
                        break;
                    }
                    Some(close) => {
                        let token_name = &after[..close];
                        out.push_str(&resolve_one_token(
                            token_name,
                            bundle,
                            registry,
                            missing_tokens,
                        )?);
                        rest = &after[close + 1..];
                    }
                }
            }
        }
    }

    Ok(out)
}

/// Resolve a single `{token}` to its string value, mirroring the token branch
/// of [`resolve`] (lookup → transform → sanitize → fallback + missing report).
fn resolve_one_token(
    token_name: &str,
    bundle: &MetadataBundle,
    registry: &TokenRegistry,
    missing_tokens: &mut Vec<String>,
) -> Result<String, ResolveError> {
    let def = registry
        .get(token_name)
        .ok_or_else(|| ResolveError::UnknownToken { token: token_name.to_owned() })?;

    let raw_opt = bundle.get(def.source_field).filter(|s| !s.is_empty());
    let (raw, is_fallback) = match raw_opt {
        Some(v) => (v.as_str(), false),
        None => (def.fallback, true),
    };
    if is_fallback {
        missing_tokens.push(token_name.to_owned());
    }

    let transformed = apply_transform(raw, def.transform);
    let sanitized = sanitize_token_value(token_name, &transformed).map_err(map_sanitize_error)?;

    if sanitized.is_empty() {
        if !is_fallback {
            missing_tokens.push(token_name.to_owned());
        }
        Ok(def.fallback.to_owned())
    } else {
        Ok(sanitized)
    }
}

/// Sanitize a literal pattern run through the same filesystem-safety pipeline
/// used for token values (traversal/reserved-name/confusable rejection).
fn sanitize_literal(literal: &str) -> Result<String, ResolveError> {
    sanitize_token_value("literal", literal).map_err(map_sanitize_error)
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn apply_transform(value: &str, transform: TokenTransform) -> String {
    match transform {
        TokenTransform::SanitizeOnly => value.to_owned(),
        TokenTransform::DateIso => {
            // Accept YYYY-MM-DD; anything else passes through for sanitization.
            value.to_owned()
        }
        TokenTransform::Lower => value.to_lowercase(),
        TokenTransform::Upper => value.to_uppercase(),
    }
}

fn map_validate_error(e: &crate::validator::ValidateError) -> ResolveError {
    match e {
        crate::validator::ValidateError::Empty => ResolveError::Empty,
        crate::validator::ValidateError::UnknownToken { token } => {
            ResolveError::UnknownToken { token: token.clone() }
        }
        crate::validator::ValidateError::InvalidSeparator { .. } => {
            // Structural issue — treat as empty for resolve purposes.
            ResolveError::Empty
        }
    }
}

fn map_sanitize_error(e: SanitizeError) -> ResolveError {
    match e {
        SanitizeError::PathTraversal { segment } => ResolveError::PathTraversal { segment },
        SanitizeError::ReservedName { segment } => ResolveError::ReservedName { segment },
        SanitizeError::UnicodeConfusable { token, value } => {
            ResolveError::UnicodeConfusable { token, value }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PatternPart;

    fn tok(id: &str, value: &str) -> PatternPart {
        PatternPart { id: id.to_owned(), kind: "token".to_owned(), value: value.to_owned() }
    }

    fn sep(id: &str, value: &str) -> PatternPart {
        PatternPart { id: id.to_owned(), kind: "separator".to_owned(), value: value.to_owned() }
    }

    fn meta(pairs: &[(&str, &str)]) -> MetadataBundle {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    // ── T3.9: end-to-end canonical fixture ────────────────────────────────

    #[test]
    fn canonical_pattern_resolves_fully() {
        // {target}/{filter}/{date}/{frame_type}/ with full metadata.
        let pattern = vec![
            tok("t", "target"),
            sep("s0", "/"),
            tok("f", "filter"),
            sep("s1", "/"),
            tok("d", "date"),
            sep("s2", "/"),
            tok("ft", "frame_type"),
            sep("s3", "/"),
        ];
        let metadata = meta(&[
            ("target", "M101"),
            ("filter", "Ha"),
            ("date", "2026-04-12"),
            ("frame_type", "light"),
        ]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "M101/Ha/2026-04-12/light/");
        assert!(result.missing_tokens.is_empty());
    }

    #[test]
    fn canonical_pattern_user_story_3_example() {
        // Spec US3 scenario 1: M101/Ha/2026-04-12/light/
        let pattern = vec![
            tok("t", "target"),
            sep("s0", "/"),
            tok("f", "filter"),
            sep("s1", "/"),
            tok("d", "date"),
            sep("s2", "/"),
            tok("ft", "frame_type"),
            sep("s3", "/"),
        ];
        let metadata = meta(&[
            ("target", "M101"),
            ("filter", "Ha"),
            ("date", "2026-04-12"),
            ("frame_type", "light"),
        ]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "M101/Ha/2026-04-12/light/");
    }

    // ── Fallback substitution ──────────────────────────────────────────────

    #[test]
    fn missing_token_uses_fallback_and_reports_it() {
        let pattern = vec![tok("t", "target"), sep("s", "/"), tok("f", "filter")];
        // No "filter" in metadata.
        let metadata = meta(&[("target", "NGC7000")]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "NGC7000/nofilter");
        assert!(result.missing_tokens.contains(&"filter".to_owned()));
    }

    #[test]
    fn all_tokens_missing_uses_all_fallbacks() {
        let pattern = vec![tok("t", "target"), sep("s", "/"), tok("f", "filter")];
        let metadata = meta(&[]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "unclassified/nofilter");
        assert!(result.missing_tokens.contains(&"target".to_owned()));
        assert!(result.missing_tokens.contains(&"filter".to_owned()));
    }

    // ── Empty pattern ──────────────────────────────────────────────────────

    #[test]
    fn empty_pattern_returns_error() {
        let result = resolve_v1(&[], &MetadataBundle::new());
        assert!(matches!(result, Err(ResolveError::Empty)));
    }

    // ── Unknown token ──────────────────────────────────────────────────────

    #[test]
    fn unknown_token_returns_error() {
        let pattern = vec![tok("x", "telescope")];
        let result = resolve_v1(&pattern, &MetadataBundle::new());
        assert!(
            matches!(result, Err(ResolveError::UnknownToken { token }) if token == "telescope")
        );
    }

    // ── Traversal rejection ────────────────────────────────────────────────

    #[test]
    fn dot_dot_token_value_rejected() {
        let pattern = vec![tok("t", "target")];
        let metadata = meta(&[("target", "..")]);
        let result = resolve_v1(&pattern, &metadata);
        assert!(matches!(result, Err(ResolveError::PathTraversal { .. })));
    }

    // ── Reserved name rejection ────────────────────────────────────────────

    #[test]
    fn reserved_name_token_value_rejected() {
        let pattern = vec![tok("t", "target")];
        let metadata = meta(&[("target", "CON")]);
        let result = resolve_v1(&pattern, &metadata);
        assert!(matches!(result, Err(ResolveError::ReservedName { .. })));
    }

    // ── OS character sanitization ──────────────────────────────────────────

    #[test]
    fn colon_in_token_value_sanitized() {
        let pattern = vec![tok("c", "camera")];
        let metadata = meta(&[("camera", "ZWO:ASI2600")]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "ZWO_ASI2600");
    }

    // ── Frame type lower transform ─────────────────────────────────────────

    #[test]
    fn frame_type_lowercased() {
        let pattern = vec![tok("ft", "frame_type")];
        let metadata = meta(&[("frame_type", "Light")]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "light");
    }

    // ── Length caps ────────────────────────────────────────────────────────

    #[test]
    fn segment_over_200_bytes_rejected() {
        let long_name: String = "A".repeat(201);
        let pattern = vec![tok("t", "target")];
        let metadata = meta(&[("target", &long_name)]);
        let result = resolve_v1(&pattern, &metadata);
        assert!(matches!(result, Err(ResolveError::PathTooLong { .. })));
    }

    #[test]
    fn total_path_over_200_chars_rejected() {
        // Build a pattern and metadata that produces > 200 chars total.
        let long_val: String = "A".repeat(101);
        let pattern = vec![tok("t", "target"), sep("s", "/"), tok("f", "filter")];
        let metadata = meta(&[("target", &long_val), ("filter", &long_val)]);
        let result = resolve_v1(&pattern, &metadata);
        assert!(matches!(result, Err(ResolveError::PathTooLong { .. })));
    }

    // ── Date ISO transform pass-through ────────────────────────────────────

    #[test]
    fn date_iso_transform_passes_through_valid_date() {
        let pattern = vec![tok("d", "date")];
        let metadata = meta(&[("date", "2026-04-12")]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "2026-04-12");
        assert!(result.missing_tokens.is_empty());
    }

    #[test]
    fn date_missing_uses_fallback_and_reports() {
        let pattern = vec![tok("d", "date")];
        let metadata = meta(&[]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "undated");
        assert!(result.missing_tokens.contains(&"date".to_owned()));
    }

    // ── Binning default fallback ───────────────────────────────────────────

    #[test]
    fn binning_fallback_is_1x1() {
        let pattern = vec![tok("b", "binning")];
        let metadata = meta(&[]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "1x1");
        assert!(result.missing_tokens.contains(&"binning".to_owned()));
    }

    // ── set_temp with negative sign ────────────────────────────────────────

    #[test]
    fn set_temp_with_minus_passes() {
        let pattern = vec![tok("st", "set_temp")];
        let metadata = meta(&[("set_temp", "-10C")]);
        let result = resolve_v1(&pattern, &metadata).unwrap();
        assert_eq!(result.relative_path, "-10C");
    }

    // ── resolve_pattern_str (spec 041 FR-026a) ─────────────────────────────

    #[test]
    fn pattern_str_light_default_full_metadata() {
        let bundle = meta(&[("target", "M31"), ("filter", "Ha"), ("date", "2026-06-21")]);
        let r = resolve_pattern_str("{target}/{filter}/{date}/light/", &bundle).unwrap();
        assert_eq!(r.relative_path, "M31/Ha/2026-06-21/light");
        assert!(r.missing_tokens.is_empty());
    }

    #[test]
    fn pattern_str_master_flat_default() {
        let bundle = meta(&[("filter", "Ha")]);
        let r = resolve_pattern_str("masters/flats/{filter}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "masters/flats/Ha");
        assert!(r.missing_tokens.is_empty());
    }

    #[test]
    fn pattern_str_dark_default() {
        let bundle = meta(&[("exposure", "300s")]);
        let r = resolve_pattern_str("darks/{exposure}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "darks/300s");
        assert!(r.missing_tokens.is_empty());
    }

    #[test]
    fn pattern_str_bias_literal_only() {
        let bundle = meta(&[]);
        let r = resolve_pattern_str("bias/", &bundle).unwrap();
        assert_eq!(r.relative_path, "bias");
        assert!(r.missing_tokens.is_empty());
    }

    #[test]
    fn pattern_str_flat_missing_filter_falls_back_and_reports() {
        // Flat default `flats/{filter}/{date}/` with no filter in the bundle.
        let bundle = meta(&[("date", "2026-06-21")]);
        let r = resolve_pattern_str("flats/{filter}/{date}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "flats/nofilter/2026-06-21");
        assert!(r.missing_tokens.contains(&"filter".to_owned()));
        assert!(!r.missing_tokens.contains(&"date".to_owned()));
    }

    #[test]
    fn pattern_str_dark_missing_exposure_falls_back_and_reports() {
        let bundle = meta(&[]);
        let r = resolve_pattern_str("darks/{exposure}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "darks/unknown-exposure");
        assert!(r.missing_tokens.contains(&"exposure".to_owned()));
    }

    #[test]
    fn pattern_str_literal_not_reported_missing() {
        // Literal `masters`/`bias` segments must never appear in missing_tokens.
        let bundle = meta(&[]);
        let r = resolve_pattern_str("masters/bias/", &bundle).unwrap();
        assert_eq!(r.relative_path, "masters/bias");
        assert!(r.missing_tokens.is_empty());
    }

    #[test]
    fn pattern_str_unknown_token_errors() {
        let bundle = meta(&[]);
        let err = resolve_pattern_str("{telescope}/x/", &bundle).unwrap_err();
        assert!(matches!(err, ResolveError::UnknownToken { token } if token == "telescope"));
    }

    #[test]
    fn pattern_str_empty_errors() {
        assert!(matches!(resolve_pattern_str("", &meta(&[])), Err(ResolveError::Empty)));
        assert!(matches!(resolve_pattern_str("   ", &meta(&[])), Err(ResolveError::Empty)));
    }

    #[test]
    fn pattern_str_mixed_token_and_literal_in_one_segment() {
        // A segment can interleave literal text and a token.
        let bundle = meta(&[("exposure", "300s")]);
        let r = resolve_pattern_str("darks/exp-{exposure}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "darks/exp-300s");
        assert!(r.missing_tokens.is_empty());
    }

    #[test]
    fn pattern_str_date_iso_transform_applied() {
        // The DateIso transform, exercised through the resolve_pattern_str
        // string-pattern entry point (not resolve_v1/resolve() directly, and
        // not frame_type's Lower transform, which is already covered by
        // frame_type_lowercased above and pattern_str_light_default_full_metadata).
        let bundle = meta(&[("date", "2026-04-12")]);
        let r = resolve_pattern_str("{date}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "2026-04-12");
        assert!(r.missing_tokens.is_empty());
    }

    #[test]
    fn pattern_str_literal_traversal_rejected() {
        let bundle = meta(&[]);
        let err = resolve_pattern_str("masters/../x/", &bundle).unwrap_err();
        assert!(matches!(err, ResolveError::PathTraversal { .. }));
    }

    // ── resolve_pattern_str: weird separators (spec 041 P11 hardening) ─────

    #[test]
    fn pattern_str_leading_slash_is_dropped() {
        // A leading `/` produces one empty leading segment, which is skipped
        // rather than emitting a leading slash in relative_path.
        let bundle = meta(&[("filter", "Ha")]);
        let r = resolve_pattern_str("/flats/{filter}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "flats/Ha");
        assert!(!r.relative_path.starts_with('/'));
    }

    #[test]
    fn pattern_str_double_slash_collapses_empty_segment() {
        let bundle = meta(&[("filter", "Ha")]);
        let r = resolve_pattern_str("flats//{filter}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "flats/Ha");
    }

    #[test]
    fn pattern_str_only_slashes_is_empty_path() {
        let bundle = meta(&[]);
        let r = resolve_pattern_str("///", &bundle).unwrap();
        assert_eq!(r.relative_path, "");
        assert!(r.missing_tokens.is_empty());
    }

    // ── resolve_pattern_str: invalid chars in literal segments ─────────────

    #[test]
    fn pattern_str_literal_windows_reserved_char_substituted() {
        // Colon in a literal segment goes through the same sanitize pipeline
        // as token values: substituted with `_`, not rejected.
        let bundle = meta(&[]);
        let r = resolve_pattern_str("weird:name/{exposure}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "weird_name/unknown-exposure");
    }

    #[test]
    fn pattern_str_literal_reserved_device_name_rejected() {
        // A literal path segment matching a Windows reserved device name is
        // rejected exactly like a resolved token value would be.
        let bundle = meta(&[]);
        let err = resolve_pattern_str("masters/CON/", &bundle).unwrap_err();
        assert!(matches!(err, ResolveError::ReservedName { .. }));
    }

    #[test]
    fn pattern_str_literal_unicode_confusable_rejected() {
        // A mixed-script literal segment (Latin + Cyrillic lookalike) is
        // rejected by the same confusables check used for token values.
        let bundle = meta(&[]);
        // "аbc" — Cyrillic а (U+0430) mixed with Latin "bc".
        let err = resolve_pattern_str("m\u{0430}sters/bc/", &bundle).unwrap_err();
        assert!(matches!(err, ResolveError::UnicodeConfusable { .. }));
    }

    #[test]
    fn pattern_str_literal_all_dots_segment_dropped() {
        // A literal segment made entirely of dots is not `.`/`..` (so it
        // passes the traversal check) but step2's dot-trim collapses it to
        // empty — it is dropped rather than producing an empty path
        // component, mirroring the "sanitizes to empty" fallback rule used
        // for tokens.
        let bundle = meta(&[("exposure", "300s")]);
        let r = resolve_pattern_str("masters/.../{exposure}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "masters/300s");
    }

    #[test]
    fn pattern_str_literal_dot_or_dotdot_segment_rejected() {
        // Unlike a run of dots, an exact `.`/`..` literal segment IS caught
        // by the traversal check (run before the dot-trim so it cannot be
        // laundered into an empty, silently-dropped segment).
        let bundle = meta(&[]);
        let err = resolve_pattern_str("masters/./x/", &bundle).unwrap_err();
        assert!(matches!(err, ResolveError::PathTraversal { .. }));
    }

    /// astro-plan-3v3r.9.10: `sanitize_token_value` leaves `/` alone by design
    /// (`safe-filename` defers separators to the resolver), so a token value
    /// carrying `../` only fails on the assembled-path check that `resolve`
    /// always had and `resolve_pattern_str` skipped.
    #[test]
    fn pattern_str_token_value_traversal_rejected() {
        let bundle = meta(&[("filter", "Ha/../../etc")]);
        let err = resolve_pattern_str("flats/{filter}/", &bundle).unwrap_err();
        assert!(matches!(err, ResolveError::PathTraversal { .. }));
    }

    /// The same value through `resolve`, pinning that both resolvers now answer
    /// alike -- the defect was the divergence.
    #[test]
    fn assembled_traversal_rejected_alike_by_both_resolvers() {
        let bundle = meta(&[("filter", "Ha/../../etc")]);
        let via_parts = resolve_v1(&[tok("f", "filter")], &bundle).unwrap_err();
        let via_pattern_str = resolve_pattern_str("{filter}", &bundle).unwrap_err();
        assert!(matches!(via_parts, ResolveError::PathTraversal { .. }));
        assert!(matches!(via_pattern_str, ResolveError::PathTraversal { .. }));
    }

    #[test]
    fn pattern_str_mixed_literal_and_token_with_invalid_chars() {
        // A segment interleaving literal text with reserved chars and a token.
        let bundle = meta(&[("gain", "100")]);
        let r = resolve_pattern_str("cam:era-{gain}/", &bundle).unwrap();
        assert_eq!(r.relative_path, "cam_era-100");
    }
}
