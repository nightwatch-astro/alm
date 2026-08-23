// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Pattern use cases (spec 015, T3.7 / T3.8).
//!
//! Entry points exposed to the Tauri command layer:
//! - `validate_pattern` — structural validation without metadata.
//! - `resolve_pattern`  — full resolution against a metadata bundle.
//! - `preview_pattern`  — preview resolution against sample metadata for the UI.
//! - `preview_path_pattern` — preview resolution of a per-type **path-string**
//!   pattern (spec 041 destination model, package P11) against sample
//!   metadata.
//!
//! All four delegate to `crates/patterns` and translate domain errors into
//! `ContractError` codes matching the JSON Schemas in
//! `specs/015-token-pattern-builder/contracts/`.

//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b) as a pure
//! leaf: it has zero `crate::` references and nothing else in `app_core`
//! references it. `app_core` re-exports this crate at `app_core::patterns` so the
//! public surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use contracts_core::patterns::{
    PathPatternPreviewRequest, PathPatternPreviewResponse, PatternPartDto, PatternPreviewRequest,
    PatternPreviewResponse, PatternResolveRequest, PatternResolveResponse, PatternValidateRequest,
    PatternValidateResponse,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use patterns::{
    registry::V1_REGISTRY,
    resolver::{resolve_pattern_str, ResolveError, ResolverConfig},
    validator::ValidateError,
    PatternPart,
};

// ── DTO conversion ────────────────────────────────────────────────────────────

fn dto_to_parts(parts: &[PatternPartDto]) -> Vec<PatternPart> {
    parts
        .iter()
        .map(|p| PatternPart { id: p.id.clone(), kind: p.kind.clone(), value: p.value.clone() })
        .collect()
}

// ── validate_pattern ──────────────────────────────────────────────────────────

/// Validate a pattern structurally (no metadata required).
///
/// Returns a [`PatternValidateResponse`] with `valid`, warnings, and optional
/// error details. Never returns `Err` — all error states are encoded in the
/// response body.
///
/// # Errors
///
/// Returns `ContractError` only on unexpected internal failures (currently
/// none in v1 — this signature future-proofs the function for DB lookups if
/// the registry ever becomes dynamic).
#[allow(clippy::result_large_err)]
pub fn validate_pattern(
    req: &PatternValidateRequest,
) -> Result<PatternValidateResponse, ContractError> {
    let parts = dto_to_parts(&req.pattern);
    let result = patterns::validate(&parts, &V1_REGISTRY);

    if result.valid {
        return Ok(PatternValidateResponse {
            valid: true,
            warnings: result.warnings.iter().map(|w| w.code().to_owned()).collect(),
            error_code: None,
            error_message: None,
            error_token: None,
        });
    }

    // Map the first hard error.
    let (code, message, token) = if let Some(e) = result.errors.first() {
        match e {
            ValidateError::Empty => {
                ("pattern.empty".to_owned(), "Pattern contains zero parts.".to_owned(), None)
            }
            ValidateError::UnknownToken { token } => (
                "token.unknown".to_owned(),
                format!("Pattern references unknown token: {token}"),
                Some(token.clone()),
            ),
            ValidateError::InvalidSeparator { sep } => {
                ("pattern.invalid".to_owned(), format!("Invalid separator: {sep}"), None)
            }
        }
    } else {
        ("pattern.invalid".to_owned(), "Pattern is invalid.".to_owned(), None)
    };

    Ok(PatternValidateResponse {
        valid: false,
        warnings: vec![],
        error_code: Some(code),
        error_message: Some(message),
        error_token: token,
    })
}

// ── resolve_pattern ───────────────────────────────────────────────────────────

/// Resolve a pattern against a metadata bundle.
///
/// # Errors
///
/// Returns `ContractError` with the appropriate error code from
/// `data-model.md §Errors` when the pattern or resolved path is invalid.
#[allow(clippy::result_large_err)]
pub fn resolve_pattern(
    req: &PatternResolveRequest,
) -> Result<PatternResolveResponse, ContractError> {
    let parts = dto_to_parts(&req.pattern);
    let metadata = req.metadata.to_bundle();

    patterns::resolve(&parts, &metadata, &V1_REGISTRY, &ResolverConfig::default())
        .map(|r| PatternResolveResponse {
            relative_path: r.relative_path,
            missing_tokens: r.missing_tokens,
            warnings: r.warnings.iter().map(|w| w.code().to_owned()).collect(),
        })
        .map_err(map_resolve_error)
}

// ── preview_pattern ───────────────────────────────────────────────────────────

/// Preview a pattern against sample metadata (for the Settings UI live preview).
///
/// Uses the same resolution pipeline as `resolve_pattern`.
///
/// # Errors
///
/// Returns `ContractError` for invalid patterns or paths.
#[allow(clippy::result_large_err)]
pub fn preview_pattern(
    req: &PatternPreviewRequest,
) -> Result<PatternPreviewResponse, ContractError> {
    let resolve_req = PatternResolveRequest {
        pattern: req.pattern.clone(),
        metadata: req.sample_metadata.clone(),
    };
    resolve_pattern(&resolve_req).map(|r| PatternPreviewResponse {
        resolved_path: r.relative_path,
        missing_tokens: r.missing_tokens,
        warnings: r.warnings,
    })
}

// ── preview_path_pattern (spec 041 per-type destination patterns, P11) ───────

/// Preview a per-type destination **path-string** pattern (e.g.
/// `masters/flats/{filter}/`) against sample metadata, for the Settings
/// per-frame-type destination pattern editor's live preview.
///
/// Delegates to `patterns::resolver::resolve_pattern_str`, which reuses
/// [`V1_REGISTRY`] as the single token-name authority and applies the same
/// sanitization/traversal/reserved-name/length pipeline as [`resolve_pattern`].
///
/// # Errors
///
/// Returns `ContractError` for an empty pattern, an unknown `{token}`, or a
/// resolved path that fails sanitization or length limits — the same error
/// codes as [`resolve_pattern`].
#[allow(clippy::result_large_err)]
pub fn preview_path_pattern(
    req: &PathPatternPreviewRequest,
) -> Result<PathPatternPreviewResponse, ContractError> {
    let metadata = req.sample_metadata.to_bundle();
    resolve_pattern_str(&req.pattern, &metadata)
        .map(|r| PathPatternPreviewResponse {
            resolved_path: r.relative_path,
            missing_tokens: r.missing_tokens,
            warnings: r.warnings.iter().map(|w| w.code().to_owned()).collect(),
        })
        .map_err(map_resolve_error)
}

// ── Error mapping ─────────────────────────────────────────────────────────────

fn map_resolve_error(e: ResolveError) -> ContractError {
    match e {
        ResolveError::Empty => ContractError::new(
            ErrorCode::PatternEmpty,
            "Pattern is empty.",
            ErrorSeverity::Blocking,
            false,
        ),
        ResolveError::UnknownToken { token } => ContractError::new(
            ErrorCode::TokenUnknown,
            format!("Unknown token: {token}"),
            ErrorSeverity::Blocking,
            false,
        )
        .with_details(serde_json::json!({ "token": token })),
        ResolveError::PathTraversal { segment } => ContractError::new(
            ErrorCode::PathTraversal,
            format!("Path traversal attempt: {segment}"),
            ErrorSeverity::Blocking,
            false,
        )
        .with_details(serde_json::json!({ "offendingSegment": segment })),
        ResolveError::ReservedName { segment } => ContractError::new(
            ErrorCode::PathReservedName,
            format!("Windows reserved device name: {segment}"),
            ErrorSeverity::Blocking,
            false,
        )
        .with_details(serde_json::json!({ "offendingSegment": segment })),
        ResolveError::UnicodeConfusable { token, value } => ContractError::new(
            ErrorCode::PatternInvalidUnicode,
            format!("Unicode confusable in token {token}: {value}"),
            ErrorSeverity::Blocking,
            false,
        )
        .with_details(serde_json::json!({ "token": token, "offendingValue": value })),
        ResolveError::PathTooLong { resolved_length, segment_length_bytes } => ContractError::new(
            ErrorCode::PatternInvalid,
            format!(
                "Path too long: resolved_length={resolved_length}, \
                 segment_length_bytes={segment_length_bytes}"
            ),
            ErrorSeverity::Blocking,
            false,
        )
        .with_details(serde_json::json!({
            "resolvedLength": resolved_length,
            "segmentLengthBytes": segment_length_bytes
        })),
        ResolveError::EmptySegment { value } => ContractError::new(
            ErrorCode::PatternInvalid,
            format!("Pattern segment is empty after sanitization: {value}"),
            ErrorSeverity::Blocking,
            false,
        )
        .with_details(serde_json::json!({ "offendingValue": value })),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use contracts_core::patterns::{MetadataBundleDto, PatternPartDto};

    fn tok(value: &str) -> PatternPartDto {
        PatternPartDto { id: value.to_owned(), kind: "token".to_owned(), value: value.to_owned() }
    }

    fn sep(value: &str) -> PatternPartDto {
        PatternPartDto {
            id: format!("sep-{value}"),
            kind: "separator".to_owned(),
            value: value.to_owned(),
        }
    }

    // ── validate_pattern ───────────────────────────────────────────────────

    #[test]
    fn validate_empty_returns_pattern_empty_code() {
        let req = PatternValidateRequest { pattern: vec![] };
        let resp = validate_pattern(&req).unwrap();
        assert!(!resp.valid);
        assert_eq!(resp.error_code.as_deref(), Some("pattern.empty"));
    }

    #[test]
    fn validate_unknown_token_returns_token_unknown() {
        let req = PatternValidateRequest { pattern: vec![tok("telescope")] };
        let resp = validate_pattern(&req).unwrap();
        assert!(!resp.valid);
        assert_eq!(resp.error_code.as_deref(), Some("token.unknown"));
        assert_eq!(resp.error_token.as_deref(), Some("telescope"));
    }

    #[test]
    fn validate_canonical_pattern_is_valid_with_trailing_sep_warning() {
        let req = PatternValidateRequest {
            pattern: vec![
                tok("target"),
                sep("/"),
                tok("filter"),
                sep("/"),
                tok("date"),
                sep("/"),
                tok("frame_type"),
                sep("/"),
            ],
        };
        let resp = validate_pattern(&req).unwrap();
        assert!(resp.valid);
        assert!(resp.warnings.contains(&"trailing_separator".to_owned()));
    }

    // ── resolve_pattern ────────────────────────────────────────────────────

    #[test]
    fn resolve_canonical_pattern() {
        let req = PatternResolveRequest {
            pattern: vec![
                tok("target"),
                sep("/"),
                tok("filter"),
                sep("/"),
                tok("date"),
                sep("/"),
                tok("frame_type"),
                sep("/"),
            ],
            metadata: MetadataBundleDto {
                target: Some("M101".to_owned()),
                filter: Some("Ha".to_owned()),
                date: Some("2026-04-12".to_owned()),
                frame_type: Some("light".to_owned()),
                ..Default::default()
            },
        };
        let resp = resolve_pattern(&req).unwrap();
        assert_eq!(resp.relative_path, "M101/Ha/2026-04-12/light/");
        assert!(resp.missing_tokens.is_empty());
    }

    #[test]
    fn resolve_empty_pattern_returns_contract_error() {
        let req = PatternResolveRequest { pattern: vec![], metadata: MetadataBundleDto::default() };
        let err = resolve_pattern(&req).unwrap_err();
        assert_eq!(err.code, ErrorCode::PatternEmpty);
    }

    #[test]
    fn resolve_unknown_token_returns_token_unknown() {
        let req = PatternResolveRequest {
            pattern: vec![tok("telescope")],
            metadata: MetadataBundleDto::default(),
        };
        let err = resolve_pattern(&req).unwrap_err();
        assert_eq!(err.code, ErrorCode::TokenUnknown);
    }

    // ── preview_pattern ────────────────────────────────────────────────────

    #[test]
    fn preview_returns_resolved_path() {
        let req = PatternPreviewRequest {
            pattern: vec![tok("target"), sep("/"), tok("filter")],
            sample_metadata: MetadataBundleDto {
                target: Some("NGC7000".to_owned()),
                filter: Some("Ha".to_owned()),
                ..Default::default()
            },
        };
        let resp = preview_pattern(&req).unwrap();
        assert_eq!(resp.resolved_path, "NGC7000/Ha");
    }

    #[test]
    fn preview_missing_token_reports_in_missing_tokens() {
        let req = PatternPreviewRequest {
            pattern: vec![tok("target"), sep("/"), tok("filter")],
            sample_metadata: MetadataBundleDto {
                target: Some("NGC7000".to_owned()),
                ..Default::default()
            },
        };
        let resp = preview_pattern(&req).unwrap();
        assert!(resp.missing_tokens.contains(&"filter".to_owned()));
        assert!(resp.resolved_path.contains("nofilter"));
    }

    // ── preview_path_pattern (spec 041 per-type destination patterns, P11) ──

    #[test]
    fn path_preview_resolves_master_flat_default() {
        let req = PathPatternPreviewRequest {
            pattern: "masters/flats/{filter}/".to_owned(),
            sample_metadata: MetadataBundleDto {
                filter: Some("Ha".to_owned()),
                ..Default::default()
            },
        };
        let resp = preview_path_pattern(&req).unwrap();
        assert_eq!(resp.resolved_path, "masters/flats/Ha");
        assert!(resp.missing_tokens.is_empty());
    }

    #[test]
    fn path_preview_resolves_light_default_with_literal_segment() {
        let req = PathPatternPreviewRequest {
            pattern: "{target}/{filter}/{date}/light/".to_owned(),
            sample_metadata: MetadataBundleDto {
                target: Some("M31".to_owned()),
                filter: Some("Ha".to_owned()),
                date: Some("2026-06-21".to_owned()),
                ..Default::default()
            },
        };
        let resp = preview_path_pattern(&req).unwrap();
        assert_eq!(resp.resolved_path, "M31/Ha/2026-06-21/light");
        assert!(resp.missing_tokens.is_empty());
    }

    #[test]
    fn path_preview_missing_token_falls_back_and_reports() {
        let req = PathPatternPreviewRequest {
            pattern: "darks/{exposure}/".to_owned(),
            sample_metadata: MetadataBundleDto::default(),
        };
        let resp = preview_path_pattern(&req).unwrap();
        assert_eq!(resp.resolved_path, "darks/unknown-exposure");
        assert!(resp.missing_tokens.contains(&"exposure".to_owned()));
    }

    #[test]
    fn path_preview_literal_only_pattern() {
        let req = PathPatternPreviewRequest {
            pattern: "bias/".to_owned(),
            sample_metadata: MetadataBundleDto::default(),
        };
        let resp = preview_path_pattern(&req).unwrap();
        assert_eq!(resp.resolved_path, "bias");
        assert!(resp.missing_tokens.is_empty());
    }

    #[test]
    fn path_preview_empty_pattern_returns_pattern_empty_code() {
        let req = PathPatternPreviewRequest {
            pattern: String::new(),
            sample_metadata: MetadataBundleDto::default(),
        };
        let err = preview_path_pattern(&req).unwrap_err();
        assert_eq!(err.code, ErrorCode::PatternEmpty);
    }

    #[test]
    fn path_preview_unknown_token_returns_token_unknown_code() {
        let req = PathPatternPreviewRequest {
            pattern: "{telescope}/x/".to_owned(),
            sample_metadata: MetadataBundleDto::default(),
        };
        let err = preview_path_pattern(&req).unwrap_err();
        assert_eq!(err.code, ErrorCode::TokenUnknown);
        assert_eq!(err.details.0.get("token").and_then(|t| t.as_str()), Some("telescope"));
    }

    #[test]
    fn path_preview_literal_traversal_rejected() {
        let req = PathPatternPreviewRequest {
            pattern: "masters/../x/".to_owned(),
            sample_metadata: MetadataBundleDto::default(),
        };
        let err = preview_path_pattern(&req).unwrap_err();
        assert_eq!(err.code, ErrorCode::PathTraversal);
    }
}
