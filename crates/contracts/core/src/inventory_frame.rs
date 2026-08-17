// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-frame inventory contract DTOs (spec 048 T006).
//!
//! Covers `inventory.frame.list`, `inventory.reconcile.run`,
//! `inventory.frame.relink`, and `inventory.root_config.{get,set}` per
//! `specs/048-per-frame-inventory/contracts/operations.md`.
//!
//! These are a **separate namespace** from the spec-006 `inventory.list` /
//! `inventory.session.review` DTOs in [`crate::inventory`] — spec 006 projects
//! `AcquisitionSession`/`CalibrationSession` rows for the review ledger; this
//! module projects individual `file_record` rows (the per-frame inventory
//! entity) for surfaces and cleanup that need to act on one frame at a time.
//!
//! Every operation here is either read-only (list/get) or, for
//! `reconcile.run`/`frame.relink`, updates only records/UI — never the
//! filesystem (Constitution II, spec 048 FR-008).

use serde::{Deserialize, Serialize};
use specta::Type;

// ── Frame type / state ───────────────────────────────────────────────────────

/// Raw frame kind for a per-frame inventory entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RawFrameType {
    Light,
    Dark,
    Flat,
    Bias,
}

/// Presence state of a per-frame inventory entry, projected from
/// `file_record.state` (spec 048 data-model.md).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FramePresenceState {
    Present,
    Missing,
    Protected,
}

// ── inventory.frame.list ──────────────────────────────────────────────────────

/// Scope for `inventory.frame.list` — exactly one of `session_id`/`root_id`
/// is expected to be set.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFrameListScope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
}

/// Request envelope for `inventory.frame.list`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFrameListRequest {
    pub scope: InventoryFrameListScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_missing: Option<bool>,
}

/// One per-frame inventory entry (a `file_record` projection).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFrame {
    pub frame_id: String,
    pub root_id: String,
    pub relative_path: String,
    pub frame_type: RawFrameType,
    pub size_bytes: i64,
    pub state: FramePresenceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Response payload for `inventory.frame.list`. `present_*` exclude `missing`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFrameListResponse {
    pub frames: Vec<InventoryFrame>,
    pub present_count: u32,
    pub present_size_bytes: i64,
}

// ── inventory.reconcile.run ───────────────────────────────────────────────────

/// What triggered a reconcile pass (spec 048 contracts/operations.md).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileReason {
    OnDemand,
    OnOpen,
    Scheduled,
    LiveEvent,
}

/// Request envelope for `inventory.reconcile.run`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryReconcileRunRequest {
    pub root_id: String,
    pub reason: ReconcileReason,
}

/// Terminal summary for a reconcile pass. A future long-running-operation
/// status stream may report `progress_pct` incrementally (SC-005); this
/// scaffold reports the terminal values only.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryReconcileRunResponse {
    pub scanned: u32,
    pub present: u32,
    pub newly_missing: u32,
    pub recovered: u32,
    pub size_backfilled: u32,
    pub progress_pct: u8,
}

// ── inventory.frame.relink ────────────────────────────────────────────────────

/// Request envelope for `inventory.frame.relink`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFrameRelinkRequest {
    pub frame_id: String,
    pub candidate_relative_path: String,
}

/// Response payload for `inventory.frame.relink`. On a hash mismatch, callers
/// receive `hash.mismatch` as a `ContractError` instead (no re-home).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFrameRelinkResponse {
    pub relinked: bool,
    pub matched_hash: String,
}

// ── inventory.root_config.{get,set} ───────────────────────────────────────────

/// Per-root reconcile mode (spec 048 FR-013).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileMode {
    #[default]
    FlagMissing,
    AutoReconcile,
}

/// Per-root detection trigger configuration (spec 048 FR-014/FR-015/FR-017).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // four distinct orthogonal triggers per spec 048 data-model
pub struct DetectionConfig {
    pub live: bool,
    pub scheduled: bool,
    pub on_open: bool,
    pub follow_symlinks: bool,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self { live: true, scheduled: false, on_open: false, follow_symlinks: false }
    }
}

/// A root's full reconcile/detection configuration, with defaults filled in
/// for any unset key (spec 048 data-model.md).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RootInventoryConfig {
    pub reconcile_mode: ReconcileMode,
    pub detection: DetectionConfig,
}

/// Request envelope for `inventory.root_config.get`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RootConfigGetRequest {
    pub root_id: String,
}

/// Partial detection-trigger update for `inventory.root_config.set`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DetectionConfigUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_open: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_symlinks: Option<bool>,
}

/// Request envelope for `inventory.root_config.set`. Unset fields leave the
/// stored value unchanged (partial update).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RootConfigSetRequest {
    pub root_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconcile_mode: Option<ReconcileMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection: Option<DetectionConfigUpdate>,
}

/// Request envelope for `inventory.watcher.attach` / `inventory.watcher.detach`
/// (spec 048 T023/T026): binds a root's live/scheduled detection triggers to the
/// lifetime of the surface showing its frame inventory, so no watch is held on
/// an idle root (research R2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RootWatcherRequest {
    pub root_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_config_default_matches_documented_defaults() {
        let d = DetectionConfig::default();
        assert!(d.live);
        assert!(!d.scheduled);
        assert!(!d.on_open);
        assert!(!d.follow_symlinks);
    }

    #[test]
    fn reconcile_mode_default_is_flag_missing() {
        assert_eq!(ReconcileMode::default(), ReconcileMode::FlagMissing);
    }

    #[test]
    fn reconcile_reason_wire_values() {
        assert_eq!(serde_json::to_string(&ReconcileReason::OnDemand).unwrap(), r#""on_demand""#);
        assert_eq!(serde_json::to_string(&ReconcileReason::LiveEvent).unwrap(), r#""live_event""#);
    }

    #[test]
    fn frame_presence_state_wire_values() {
        assert_eq!(serde_json::to_string(&FramePresenceState::Present).unwrap(), r#""present""#);
        assert_eq!(serde_json::to_string(&FramePresenceState::Missing).unwrap(), r#""missing""#);
    }
}
