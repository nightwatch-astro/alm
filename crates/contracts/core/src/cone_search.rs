// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Cone-search suggestion contract DTOs (spec 052 P3, US3).
//!
//! `target.cone_search.suggest` / `target.cone_search.confirm` — see
//! `specs/052-simbad-caching-dual-lookup-cone-search/contracts/operations.md`.
//! Pure DTOs (no logic): wire parity with that doc is the source of truth.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::targets::TargetObjectType;

/// Coordinate-source quality for a derived pointing (FR-012).
///
/// `Wcs` (plate-solved `CRVAL1/2`) is high confidence; `Mount` (`OBJCTRA`/
/// `OBJCTDEC` or decimal `RA`/`DEC`) is medium; `None` means no reliable
/// pointing (never derived from the filename) — `suggestions` is then empty.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum PointingSource {
    Wcs,
    Mount,
    None,
}

/// Explicit confidence for a cone-search suggestion (FR-014). `High` is the
/// only tier that may carry `preselected: true`; the system never sets it
/// without a qualifying confidence, and never applies a link itself.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ConeSearchConfidence {
    High,
    Medium,
    Low,
}

/// The derived sky pointing a cone-search ran against.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConeSearchPointing {
    pub source: PointingSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_ra_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_dec_deg: Option<f64>,
    pub radius_deg: f64,
    pub optics_known: bool,
}

/// One candidate object, resolved from cache/online but not yet adopted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConeSearchCandidateTarget {
    /// `None` until the candidate is confirmed (FR-004/FR-016) — cone-search
    /// itself never writes `canonical_target`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_target_id: Option<String>,
    pub primary_designation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_name: Option<String>,
    pub object_type: TargetObjectType,
    pub ra_deg: f64,
    pub dec_deg: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constellation: Option<String>,
}

/// A ranked, confidence-carrying cone-search suggestion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConeSearchSuggestion {
    pub candidate: ConeSearchCandidateTarget,
    pub separation_deg: f64,
    pub confidence: ConeSearchConfidence,
    /// `true` only for `confidence = High` (FR-014); never implies a link.
    pub preselected: bool,
    /// `true` when the candidate is in the OQ-2 default exclusion set; still
    /// returned so the UI can show it for manual override (FR-015).
    pub excluded: bool,
}

/// What triggered this cone-search run (FR-017).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ConeSearchReason {
    Ingest,
    OnDemand,
}

/// Request for `target.cone_search.suggest`. The backend derives the
/// pointing from the frameset's frames; the client never supplies
/// coordinates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConeSearchSuggestRequest {
    pub frameset_id: String,
    pub reason: ConeSearchReason,
}

/// Response for `target.cone_search.suggest`. Read-only — produces no
/// filesystem mutation and no `canonical_target` write.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConeSearchSuggestResponse {
    pub pointing: ConeSearchPointing,
    pub suggestions: Vec<ConeSearchSuggestion>,
}

/// The candidate a `target.cone_search.confirm` call binds to the frameset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConeSearchConfirmCandidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_target_id: Option<String>,
    pub primary_designation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simbad_oid: Option<i64>,
}

/// Request for `target.cone_search.confirm` — the single point at which a
/// cone-search suggestion becomes durable (FR-016, SC-006).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConeSearchConfirmRequest {
    pub frameset_id: String,
    pub candidate: ConeSearchConfirmCandidate,
}

/// Response for `target.cone_search.confirm`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConeSearchConfirmResponse {
    pub canonical_target_id: String,
    /// `true` when a new durable row was written, `false` when an existing
    /// dedup match was reused.
    pub created: bool,
    pub linked: bool,
}
