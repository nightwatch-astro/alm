// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Pattern contract DTOs for spec 015 (T3.7).
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks
//!
//! These types mirror the JSON Schemas in
//! `specs/015-token-pattern-builder/contracts/` and are used by the Tauri
//! command surface in `apps/desktop/src-tauri/src/commands/patterns.rs`.
//!
//! Four operations:
//! - `pattern.validate`  — structural validation without metadata.
//! - `pattern.resolve`   — full resolution against a metadata bundle.
//! - `pattern.preview`   — preview resolution against sample metadata for the UI.
//! - `pattern.path_preview` — preview resolution of a per-type **path-string**
//!   pattern (spec 041 `{token}`/literal path segments) against sample
//!   metadata, for the Settings per-frame-type destination pattern editor.

use serde::{Deserialize, Serialize};
use specta::Type;

// ── PatternPart ───────────────────────────────────────────────────────────────

/// One element of an ordered token pattern (data-model.md §PatternPart).
///
/// Re-exported from `crates/contracts/core` so the Tauri command layer can
/// reference it without importing `crates/patterns` directly. The shape matches
/// `patterns::PatternPart` exactly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternPartDto {
    /// Stable client-side identifier.
    pub id: String,
    /// `"token"` or `"separator"`.
    pub kind: String,
    /// Token name (e.g. `"target"`) or literal separator character.
    pub value: String,
}

// ── MetadataBundle ────────────────────────────────────────────────────────────

/// Flat metadata map for resolution / preview (data-model.md §MetadataBundle).
///
/// All fields are optional; absent keys cause fallback substitution. The
/// `frame_type` field accepts the closed enum
/// `["light","dark","flat","bias","dark_flat"]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MetadataBundleDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Local date `YYYY-MM-DD`. Falls back to UTC when observer_location unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Per-file frame type. `"mixed"` is NOT valid here (folder-level only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_temp: Option<String>,
}

impl MetadataBundleDto {
    /// Convert to a flat `HashMap<String, String>` for the resolver.
    #[must_use]
    pub fn to_bundle(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        if let Some(v) = &self.target {
            map.insert("target".to_owned(), v.clone());
        }
        if let Some(v) = &self.filter {
            map.insert("filter".to_owned(), v.clone());
        }
        if let Some(v) = &self.date {
            map.insert("date".to_owned(), v.clone());
        }
        if let Some(v) = &self.frame_type {
            map.insert("frame_type".to_owned(), v.clone());
        }
        if let Some(v) = &self.camera {
            map.insert("camera".to_owned(), v.clone());
        }
        if let Some(v) = &self.exposure {
            map.insert("exposure".to_owned(), v.clone());
        }
        if let Some(v) = &self.gain {
            map.insert("gain".to_owned(), v.clone());
        }
        if let Some(v) = &self.binning {
            map.insert("binning".to_owned(), v.clone());
        }
        if let Some(v) = &self.set_temp {
            map.insert("set_temp".to_owned(), v.clone());
        }
        map
    }
}

// ── pattern.validate ──────────────────────────────────────────────────────────

/// Request for `pattern.validate`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternValidateRequest {
    pub pattern: Vec<PatternPartDto>,
}

/// Response for `pattern.validate`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternValidateResponse {
    pub valid: bool,
    pub warnings: Vec<String>,
    /// Present when `valid = false`. First error code and message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Details payload for `token.unknown`: the offending token name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_token: Option<String>,
}

// ── pattern.resolve ───────────────────────────────────────────────────────────

/// Request for `pattern.resolve`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternResolveRequest {
    pub pattern: Vec<PatternPartDto>,
    pub metadata: MetadataBundleDto,
}

/// Response for `pattern.resolve`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternResolveResponse {
    pub relative_path: String,
    pub missing_tokens: Vec<String>,
    pub warnings: Vec<String>,
}

// ── pattern.preview ───────────────────────────────────────────────────────────

/// Request for `pattern.preview` (UI live preview, Ref: R-Preview).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternPreviewRequest {
    pub pattern: Vec<PatternPartDto>,
    pub sample_metadata: MetadataBundleDto,
}

/// Successful preview response.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternPreviewResponse {
    /// The resolved relative path for display.
    pub resolved_path: String,
    /// Token names resolved via fallback (shown as dim segments in the UI).
    pub missing_tokens: Vec<String>,
    pub warnings: Vec<String>,
}

// ── pattern.path_preview (spec 041 per-type destination patterns, package P11) ──

/// Request for `pattern.path_preview` — preview a per-type destination
/// **path-string** pattern (e.g. `masters/flats/{filter}/`) against sample
/// metadata, for the Settings per-frame-type destination pattern editor.
///
/// Unlike [`PatternPreviewRequest`] (which carries the `PatternPart[]`
/// token/separator model), `pattern` here is a raw path string that may
/// interleave `{token}` placeholders with literal directory segments — the
/// form produced by [`crate::patterns`] (this module) is not applicable;
/// resolution is delegated to `crates/patterns::resolver::resolve_pattern_str`,
/// which reuses the v1 token registry as the single token-name authority.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PathPatternPreviewRequest {
    pub pattern: String,
    pub sample_metadata: MetadataBundleDto,
}

/// Successful response for `pattern.path_preview`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PathPatternPreviewResponse {
    /// The resolved relative path for display.
    pub resolved_path: String,
    /// Token names resolved via fallback (shown as dim segments in the UI).
    pub missing_tokens: Vec<String>,
    pub warnings: Vec<String>,
}
