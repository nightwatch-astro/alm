// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Session contract DTOs for the Tauri IPC surface.
//!
//! These types mirror the hand-written TypeScript `AcquisitionSession` in
//! `apps/desktop/src/api/types.ts` and the fixture data in
//! `apps/desktop/src/data/fixtures/sessions.ts`. They are the Rust-side
//! source of truth once tauri-specta generates the typed bindings.
//!
//! Spec 041 FR-051 (T076, Phase 13): sessions are derived, already-confirmed
//! inventory — the review-state field (`state: SessionState`) that used to
//! sit on `AcquisitionSession`/`SessionDetail` was removed along with the
//! rest of the review lifecycle.

use serde::{Deserialize, Serialize};
use specta::Type;

// Re-use existing enum from a sibling contract module to avoid a specta
// name collision (`ProvenanceOrigin`).
pub use crate::provenance::ProvenanceOrigin;

use crate::calibration::CalibrationKind;

pub mod heterogeneity;

/// Confidence level for inferred or reviewed metadata.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    Unknown,
    Low,
    Medium,
    High,
    Confirmed,
    Rejected,
}

/// A single metadata value with provenance and confidence tracking.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MetaValue {
    /// Free-form JSON value. Uses [`crate::JsonAny`] to avoid specta's
    /// infinite-recursion issue with raw `serde_json::Value`.
    pub value: crate::JsonAny,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    pub origin: ProvenanceOrigin,
    pub confidence: ConfidenceLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
}

/// Composite key that uniquely identifies an acquisition session by its
/// observing parameters.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionKey {
    pub target: String,
    pub filter: String,
    pub binning: String,
    pub gain: String,
    /// ISO date of the observing night (local sunset date).
    pub night: String,
}

/// An acquisition session as seen through the IPC boundary.
///
/// This is the list-level representation returned by `sessions.list`. It
/// matches the frontend's `AcquisitionSession` interface in `types.ts`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionSession {
    pub id: String,
    pub session_key: SessionKey,
    pub confidence: ConfidenceLevel,
    pub optical_train_id: String,
    pub frame_count: u32,
    pub total_integration_seconds: f64,
    pub total_size_bytes: u64,
    pub metadata: std::collections::HashMap<String, MetaValue>,
    pub target_ids: Vec<String>,
    pub project_ids: Vec<String>,
    pub warnings: Vec<String>,
}

/// Slim session summary for list views that only need identification and counts
/// (GF-10: session pickers, label surfaces). Avoids transferring `metadata`,
/// `warnings`, and `project_ids` over IPC when unused.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub session_key: SessionKey,
    pub frame_count: u32,
}

// ── Detail types ────────────────────────────────────────────────────────────

/// A group of frames within a session (per-filter breakdown).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Frameset {
    pub filter: String,
    pub count: u32,
    pub integration_s: f64,
}

/// A calibration match entry for a session detail view.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionCalibrationMatch {
    pub master_id: String,
    pub kind: CalibrationKind,
    pub score: f64,
    pub soft_mismatches: Vec<String>,
    /// Whether this assignment was made via the hard-rule override path
    /// (spec 007 SC-003) — persisted so the UI can distinguish an override
    /// from a normal match on reopen instead of losing the distinction.
    pub was_override: bool,
}

/// A history entry for a session detail view.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionHistoryEntry {
    pub timestamp: String,
    pub event: String,
    pub actor: String,
}

/// Extended detail view of an acquisition session.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    // Flattened base fields from AcquisitionSession.
    pub id: String,
    pub session_key: SessionKey,
    pub confidence: ConfidenceLevel,
    pub optical_train_id: String,
    pub frame_count: u32,
    pub total_integration_seconds: f64,
    pub total_size_bytes: u64,
    pub metadata: std::collections::HashMap<String, MetaValue>,
    pub target_ids: Vec<String>,
    pub project_ids: Vec<String>,
    pub warnings: Vec<String>,
    // Detail-only fields.
    pub framesets: Vec<Frameset>,
    pub calibration_matches: Vec<SessionCalibrationMatch>,
    pub history: Vec<SessionHistoryEntry>,
}

// ── Calendar types ──────────────────────────────────────────────────────────

/// A session stub within a calendar day.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSessionStub {
    pub id: String,
    pub target: String,
    pub filter: String,
}

/// A single day in the session calendar.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDay {
    pub day: u32,
    pub sessions: Vec<CalendarSessionStub>,
}

/// A single month in the session calendar.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CalendarMonth {
    pub year: u32,
    pub month: u32,
    pub days: Vec<CalendarDay>,
}

/// Calendar data for the sessions calendar view.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CalendarData {
    pub months: Vec<CalendarMonth>,
}
