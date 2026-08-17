// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Inbox contract DTOs for the Tauri IPC surface (spec 005 / spec 041).
//!
//! Matches `specs/005-inbox-mixed-folder-split/contracts/`:
//!   - `inbox.classify.json` (v1.1.0)
//!   - `inbox.confirm.json`  (v1.1.0)
//!   - `inbox.reclassify.json` (v1.0.0)
//!
//! Extended by spec 041 (inbox plan surface / phase 2):
//!   - `InboxFileMetadata` / `inbox.item.metadata` (US2/FR-010)
//!   - `InboxStats` / `inbox.stats` (US6/FR-021)
//!   - `InboxReclassifyOverride` gains optional `filter` / `exposure_s` / `binning` (R-3)
//!   - `InboxConfirmResponse` gains `actions_summary` + `organization_state` (US4/US5)
//!   - `InboxPlanView` / `inbox.plan` (US1/FR-003/FR-004)
//!   - `InboxListItem` gains `organization_state` (spec 041)
//!
//! Extended by spec 041 Phase 12 (single-type ingest, T072 contracts/binding
//! regen):
//!   - `InboxListItem` gains `group_id` / `group_key` / `group_label` /
//!     `source_group_id` / `frame_type` (FR-043 provenance).
//!   - `InboxConfirmRequest` drops the legacy `action` field (FR-050 — the
//!     "split"/"mixed" confirm path is removed, T071).
//!   - `InboxFileMetadata` gains the T062 extended extraction fields
//!     (`offset`, `set_temp_c`, `ccd_temp_c`, `ra_deg`, `dec_deg`,
//!     `rotator_angle_deg`, `readout_mode`, `focal_length_mm`, `date_loc`)
//!     for display (FR-044).
//!   - `InboxReclassifyFileOverride`/`InboxReclassifyBulk` use `JsonAny`
//!     instead of raw `serde_json::Value` (T072 binding regen) — specta's
//!     TypeScript exporter cannot inline `serde_json::Value`'s recursive enum
//!     definition (infinite-recursion panic); `JsonAny` is the established
//!     wire-transparent workaround also used by `ProvenanceValue`/`ErrorDetails`.

use crate::JsonAny;
use serde::{Deserialize, Serialize};
use specta::Type;

// ── Legacy scan DTOs (retained for backward compat with spec 030) ─────────────

/// A file entry discovered during an inbox scan.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxFileEntry {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub extension: String,
}

/// Result of an inbox scan operation.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxScanResult {
    pub root_id: String,
    pub entries: Vec<InboxFileEntry>,
    pub total_count: u32,
    pub total_size_bytes: u64,
}

// ── inbox.classify ────────────────────────────────────────────────────────────

/// Request for `inbox.classify`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxClassifyRequest {
    pub inbox_item_id: String,
    #[serde(default)]
    pub force_rescan: bool,
    /// Absolute path to the inbox root on disk (needed by the use case to
    /// locate files). Not in the JSON Schema (transport detail).
    pub root_absolute_path: String,
}

// ── inbox.classify.sourceGroup (spec 058 FR-015/T012) ────────────────────────

/// Request for `inbox.classify.sourceGroup` — classify a scanned folder that
/// has no `inbox_items` row yet.
///
/// Spec 058 removes the scan-time folder placeholder (FR-015/T020), and with it
/// the only route into `materialize_sub_items`: both existing entry points are
/// keyed on an item id. This request is keyed on the source group instead, so a
/// bare group can become item rows without one ever having existed.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxClassifySourceGroupRequest {
    pub source_group_id: String,
    /// Absolute path to the registered root on disk, so the use case can locate
    /// the group's files. Transport detail, as on `InboxClassifyRequest`.
    pub root_absolute_path: String,
}

/// Response from `inbox.classify.sourceGroup`.
///
/// Deliberately returns a **count, not a plan, and no confirmable id**. The
/// source-group row is the thing you classify, never the thing you confirm
/// (FR-016) — confirmation continues to happen only against the item rows this
/// materializes, which the next `inbox.list` returns in `items` while the group
/// drops out of `sourceGroups` (FR-017, a consequence of that query's zero-item
/// predicate rather than a separate step).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxClassifySourceGroupResponse {
    pub source_group_id: String,
    pub materialized_sub_item_count: u32,
}

/// One frame-type breakdown entry in a classify response.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxBreakdownEntry {
    pub kind: String,
    pub count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination_preview: Option<String>,
    pub sample_files: Vec<String>,
}

/// Response from `inbox.classify`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxClassifyResponse {
    pub inbox_item_id: String,
    /// `"single_type"` | `"mixed"` | `"unclassified"`
    #[serde(rename = "type")]
    pub classification_type: String,
    /// Present only when `type == "single_type"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type: Option<String>,
    pub content_signature: String,
    pub breakdown: Vec<InboxBreakdownEntry>,
    /// Relative file paths whose IMAGETYP was absent, unreadable, or unmapped.
    pub unclassified_files: Vec<String>,
    pub sample_files: Vec<String>,
    pub computed_at: String,
}

// ── inbox.confirm ─────────────────────────────────────────────────────────────

/// Request for `inbox.confirm`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxConfirmRequest {
    pub inbox_item_id: String,
    pub content_signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_destination: Option<String>,
    /// Absolute path to the inbox root on disk (needed to read FITS/XISF
    /// headers for destination resolution). Not in the JSON Schema contract
    /// (Tauri transport detail only).
    pub root_absolute_path: String,
    /// Caller-selected destination library root (spec 041 US8/FR-029).
    ///
    /// Only consulted for inbox sources whose frame-type category has more than
    /// one candidate library root. When exactly one candidate exists it is
    /// auto-selected and this field is ignored; for non-inbox sources the file
    /// stays in place. Supplying a root that is not a valid candidate for the
    /// item's category is rejected with `inbox.invalid_destination_root`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    /// Attribution apply-path (spec 008 Q27, F-Framing-10, FR-022) — additive.
    /// The user's pick from a prior `attributionCandidates` list. Only
    /// meaningful for light-frame items (`attribution.not_light_frame` on any
    /// other frame type); omitting it leaves the confirmed session's framing
    /// membership unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen_attribution: Option<crate::framing::ChosenAttributionDto>,
}

/// A candidate destination library root for an inbox item's frame-type
/// category (spec 041 US8/FR-029).
///
/// Returned in the `inbox.destination_root_required` error data so the UI can
/// render a picker, with `kind` for grouping/labelling.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxDestinationRoot {
    pub root_id: String,
    pub path: String,
    /// Source kind: `"light_frames"` | `"calibration"` | `"project"` | `"inbox"`.
    pub kind: String,
}

/// Summary of plan actions split by type (spec 041 US4/US5/FR-020).
///
/// Lets the UI show "N move / M catalogue" without iterating plan items.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxConfirmActionsSummary {
    /// Number of plan items with `action = "move"`.
    pub move_count: u32,
    /// Number of plan items with `action = "catalogue"`.
    pub catalogue_count: u32,
}

/// Resolved destination preview for one confirmed plan action (spec 041
/// US8/FR-031).
///
/// Carries the **absolute** destination (chosen root path + resolved relative
/// path) so the UI can show the full on-disk path without re-resolving roots.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxConfirmDestination {
    pub from_path: String,
    /// Resolved relative path under the destination root.
    pub to_relative_path: String,
    /// Absolute destination = chosen root path + `/` + `to_relative_path`
    /// (equals the source location for `catalogue` actions).
    pub to_absolute_path: String,
    /// Id of the chosen destination root.
    pub to_root_id: String,
    /// `"move"` | `"catalogue"`.
    pub action: String,
}

/// Response from `inbox.confirm`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxConfirmResponse {
    pub plan_id: String,
    /// Always `"ready_for_review"` for plans created here; empty string for
    /// master-registration responses.
    pub plan_state: String,
    pub items_total: u32,
    /// True when the item was a detected calibration master that was registered
    /// directly to `calibration_session` + `calibration_fingerprint` (Path 1 —
    /// no file move).  `plan_id` is an empty string in this case.
    pub registered_as_master: bool,
    /// Breakdown of plan actions produced (spec 041 US4/FR-020).
    /// `None` when `registered_as_master` is true (no plan was created).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions_summary: Option<InboxConfirmActionsSummary>,
    /// Organization state of the source owning this item (spec 041 R-7).
    /// `"organized"` | `"unorganized"`. `None` when `registered_as_master`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_state: Option<String>,
    /// Per-action absolute destination previews (spec 041 US8/FR-031).
    /// Empty for master-registration responses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destinations: Vec<InboxConfirmDestination>,
    /// Inbox-confirm attribution pass (spec 008 Q27, F-Framing-5, FR-019).
    /// Ranked suggestions for where this item's light session belongs — a
    /// suggestion surface only, never auto-applied. Empty for non-light items
    /// or when no candidate matched.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attribution_candidates: Vec<crate::framing::IngestionAttributionCandidateDto>,
    /// Present when the request carried a `chosenAttribution` that was
    /// successfully applied (F-Framing-10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_applied: Option<crate::framing::AttributionAppliedDto>,
}

// ── inbox.reclassify ──────────────────────────────────────────────────────────

/// A single file override in a reclassify request.
///
/// Extended by spec 041 (R-3) to carry optional filter/exposure/binning
/// overrides alongside frame type. Any subset of fields may be set per file;
/// omitted fields leave the existing persisted override unchanged.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxReclassifyOverride {
    pub file_path: String,
    /// Override for the IMAGETYP / frame type.  `None` = leave unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type: Option<String>,
    /// Override for the FILTER header value.  `None` = leave unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Override for exposure in seconds (EXPTIME/EXPOSURE).  `None` = leave unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure_s: Option<f64>,
    /// Override for binning as a human-readable string e.g. `"2x2"`.  `None` = leave unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binning: Option<String>,
}

/// Request for `inbox.reclassify`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxReclassifyRequest {
    pub inbox_item_id: String,
    pub overrides: Vec<InboxReclassifyOverride>,
}

/// Response from `inbox.reclassify`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxReclassifyResponse {
    pub inbox_item_id: String,
    /// `"single_type"` | `"mixed"` | `"unclassified"` after re-aggregation.
    pub updated_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type: Option<String>,
    pub remaining_unclassified: u32,
    /// Number of files whose overrides were applied (FR-014).
    pub applied_count: u32,
    /// Rebuilt breakdown after overrides (FR-015).
    pub breakdown: Vec<InboxBreakdownEntry>,
}

// ── inbox.scan.folder ─────────────────────────────────────────────────────────

/// Request to scan a root directory and discover inbox items.
///
/// Traversal behaviour is deliberately not a request field: `inbox.scan.folder`
/// resolves `followSymlinks` from persisted ingestion settings so every caller
/// gets the behaviour the user configured (issue #878). Every previous caller
/// hardcoded `false`, which silently overrode that setting.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxScanFolderRequest {
    pub root_id: String,
    pub root_absolute_path: String,
}

/// A discovered inbox item returned from the scan.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxItemSummary {
    pub inbox_item_id: String,
    pub relative_path: String,
    pub file_count: u32,
    pub lane: String,
    /// Real file format for this item: `"fits"` | `"xisf"` | `"video"` | `"mixed"`.
    ///
    /// Unlike `lane` (which only distinguishes FITS vs video), `format` tells
    /// the UI whether the item contains FITS files, XISF files, a mix of both,
    /// or video files.  Spec 040 FR-006.
    pub format: String,
    pub state: String,
    pub content_signature: String,
    /// `true` when this item represents a single detected calibration master
    /// file (`relative_path` is a file path, not a folder path).  Spec 040 FR-005.
    pub is_master: bool,
    /// Base frame type for master items (`"dark"` | `"flat"` | `"bias"` | …).
    /// `null` for grouped sub-frame folder items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_frame_type: Option<String>,
    /// Filter label extracted from master file metadata (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_filter: Option<String>,
    /// Exposure in seconds extracted from master file metadata (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_exposure_s: Option<f64>,
}

/// A directory that could not be read during a scan (non-fatal).
///
/// Collected by the phase-1 walk and surfaced in [`InboxScanFolderResponse`]
/// so the UI can report partial-scan conditions without aborting the operation.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanDirWarning {
    /// Absolute path of the directory that could not be read.
    pub path: String,
    /// OS error description (e.g. "Permission denied (os error 13)").
    pub reason: String,
}

/// Response from `inbox.scan.folder`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxScanFolderResponse {
    pub root_id: String,
    pub items: Vec<InboxItemSummary>,
    /// Directories that could not be read during the scan.
    /// Empty when all directories were accessible. Added in GF-13.
    #[serde(default)]
    pub scan_warnings: Vec<ScanDirWarning>,
}

// ── Cross-root unacknowledged list (spec 039) ─────────────────────────────────

/// One unacknowledged inbox item returned by `inbox.list`.
///
/// Extends `InboxItemSummary` with the root's id and absolute path so the UI
/// can group/label items by root without a second call.
///
/// Spec 041: gains `organization_state` so the list can show
/// move-vs-catalogue intent per item without a separate source lookup.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxListItem {
    pub inbox_item_id: String,
    pub root_id: String,
    /// Absolute path of the registered root (for display and confirm calls).
    pub root_absolute_path: String,
    pub relative_path: String,
    pub file_count: u32,
    pub lane: String,
    /// Real file format: `"fits"` | `"xisf"` | `"video"` | `"mixed"`.  Spec 040 FR-006.
    pub format: String,
    pub state: String,
    pub content_signature: String,
    /// `true` when this row represents a single detected calibration master file.
    pub is_master: bool,
    /// Base frame type for master items; `null` for grouped sub-frame folders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_frame_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_exposure_s: Option<f64>,
    /// Organization state of the owning source: `"organized"` | `"unorganized"`.
    /// Spec 041 — lets the list surface move-vs-catalogue intent per item.
    pub organization_state: String,
    // ── Per-item grouping keys (spec 041 — multi-level grouping UI) ─────────────
    //
    // Each is an aggregate LABEL computed across the item's persisted per-file
    // metadata (`inbox_file_metadata`) / classification evidence:
    //   - 0 distinct non-null values  -> `None`     (frontend buckets as "(none)")
    //   - exactly 1 distinct value    -> `Some(value)`
    //   - 2+ distinct values          -> `Some("Mixed")`
    // Exception: `group_frame_type` is the item's DOMINANT frame type (largest
    // group), never `"Mixed"`; `None` only when no frame type is known.
    /// Object / target (FITS `OBJECT`). `Some("Mixed")` if files disagree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_target: Option<String>,
    /// Dominant effective frame type across the item's files (largest group).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_frame_type: Option<String>,
    /// Capture date as `YYYY-MM-DD` from the earliest `DATE-OBS`.
    /// `Some("Mixed")` if files span multiple distinct dates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_date: Option<String>,
    /// Filter label (FITS `FILTER`). `Some("Mixed")` if files disagree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_filter: Option<String>,
    /// Exposure formatted like `"300s"` (trailing zeros trimmed).
    /// `Some("Mixed")` if files have multiple distinct exposures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_exposure: Option<String>,
    /// Camera / instrument (FITS `INSTRUME`). `Some("Mixed")` if files disagree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_instrument: Option<String>,
    /// Per-item rollup of missing mandatory attribute keys (spec 041 T070 /
    /// FR-047 / R-14). Non-empty when this item is in the needs-review bucket
    /// because one or more files are missing a mandatory attribute. The set is
    /// the union of all per-file missing-mandatory lists across the item's files.
    /// Empty for fully-resolved items. Blocks plan creation (FR-048/SC-015).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_mandatory: Vec<String>,
    // ── Single-type sub-item identity (spec 041 Phase 12, T072 / FR-043) ────
    //
    // Additive fields exposing the T063–T066 single-type sub-item identity
    // directly from the `inbox_items` row, alongside the pre-existing
    // aggregate `group_*` fields above (kept for backward compat — some
    // callers still use the multi-level grouping UI aggregates).
    /// The item's own identity, restated as its "group" id for symmetry with
    /// `group_key`/`group_label`. Equals `inbox_item_id`.
    pub group_id: String,
    /// Deterministic canonical group key (R-11). Empty string for legacy
    /// pre-Phase-12 rows that have not yet been materialized into a
    /// single-type sub-item (e.g. the original leaf-folder row).
    pub group_key: String,
    /// Human-readable label `"(root) · <type> · <dims>"` (R-12). `None` until
    /// classified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_label: Option<String>,
    /// Id of the `inbox_source_groups` row (leaf folder) this sub-item was
    /// materialized from (FR-043 provenance). `None` for legacy rows that
    /// predate source groups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_group_id: Option<String>,
    /// Authoritative frame type, singular — items are single-type post
    /// materialization (T066), so this is a real value rather than the
    /// aggregate-with-"Mixed"-fallback `group_frame_type` above. `None`
    /// until classified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type: Option<String>,
    /// Spec 058 FR-028: the persisted verdict of the mandatory-attribute gate,
    /// its own field rather than a `group_key` value. `true` means the item
    /// cannot be confirmed until the missing attributes are supplied.
    pub needs_review: bool,
    /// Cached classification result, DB vocabulary (`"classified"` /
    /// `"unclassified"`) — the SAME value `inbox.classify` reads for this
    /// item. `None` when the item has never been classified.
    ///
    /// Issue #711 Instance A (unsplit-folder variant): `state` is
    /// unconditionally `"classified"` once a folder has been scanned even
    /// when it has no dominant frame type (empty/mixed/needs-review), so the
    /// list's classification badge must not fall back to `state` alone —
    /// this field lets it agree with `inbox.classify`/the detail panel by
    /// construction instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification_result: Option<String>,
}

/// A scanned-but-unclassified folder in `inbox.list` (spec 058 FR-016).
///
/// Carries **no** `inboxItemId`. Non-confirmability is structural: there is
/// nothing to pass to `inbox.confirm`, so no new guard and no new error code
/// exist for it. A discriminated union with [`InboxListItem`] was rejected
/// precisely because it would restore a row that looks like an item and
/// carries an id the UI is tempted to confirm (FR-004).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxSourceGroupListItem {
    pub source_group_id: String,
    pub root_id: String,
    /// Absolute path of the registered root (for display).
    pub root_absolute_path: String,
    pub relative_path: String,
    /// Sub-frames the scan found, excluding detected calibration masters.
    pub file_count: u32,
    /// Dominant file format: `"fits"` | `"xisf"` | `"video"` | `"mixed"`.
    pub format: String,
    /// The source group's `"move"` | `"catalogue"` lane — NOT the
    /// `"fits"`/`"video"` item lane. The two columns share a name only.
    pub lane: String,
    pub content_signature: String,
    pub discovered_at: String,
}

/// Response from `inbox.list`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxListResponse {
    pub items: Vec<InboxListItem>,
    /// Folders that have been scanned but have produced no items yet. Contains
    /// only groups with zero item rows, so FR-017's "replaced by its item
    /// rows" is a consequence of the query rather than a separate step.
    pub source_groups: Vec<InboxSourceGroupListItem>,
    /// Whether the list was capped at `limit` (true = there may be more).
    pub capped: bool,
    /// Maximum items per response (matches the server-side cap).
    pub limit: u32,
}

// ── inbox.item.metadata (spec 041 US2/FR-010) ─────────────────────────────────

/// Per-file metadata entry for one file within an inbox item.
///
/// All header fields are nullable — not every file type carries all headers.
/// `frame_type_effective` reflects override-if-present-else-extracted.
/// `override_stale` is true when the file was changed (size/mtime) since the
/// override was recorded (R-4); the override is surfaced but flagged.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxFileMetadata {
    pub relative_file_path: String,
    /// Effective frame type (override ?? extracted from header).
    /// `None` when the file remains unclassified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type_effective: Option<String>,
    /// Raw `IMAGETYP` header value (before normalization), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_typ: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binning_x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binning_y: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_obs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telescop: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naxis1: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naxis2: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_count: Option<i32>,
    /// True when this file has been identified as a stacked calibration master.
    pub is_master: bool,
    /// True when the persisted override no longer matches the file's size/mtime (R-4).
    pub override_stale: bool,
    /// Path-load-bearing attributes the file is missing for its frame type's
    /// destination pattern (spec 041 US9/FR-032/FR-033). Empty when the file can
    /// resolve a destination. These are the pattern token names that fell back to
    /// a registry default (e.g. `["target", "date"]` for a light with no OBJECT /
    /// DATE-OBS). Supplying the value via reclassify clears the gate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_path_attributes: Vec<String>,
    /// Registry key names of mandatory attributes that are absent for this file
    /// (spec 041 T070 / FR-047 / R-14). Empty when all mandatory attributes are
    /// present. The union of mandatory grouping properties and hard per-type keys
    /// (e.g. `["target"]` for a light with no OBJECT and no resolved target).
    /// Non-empty means this file's sub-item is in the needs-review bucket and
    /// blocks plan creation until the value is supplied via reclassify.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_mandatory: Vec<String>,
    // ── T062 extended extraction, exposed for display (spec 041 T072/FR-044) ─
    //
    // Raw extracted values (NOT override-merged — unlike `filter`/`exposure_s`
    // above, these do not yet consult `inbox_file_overrides`). `None` when the
    // source header was absent.
    /// Camera read-out offset / pedestal (`OFFSET` / `BLKLEVEL`). ADU.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    /// Sensor set/target temperature (`SET-TEMP`). Degrees Celsius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_temp_c: Option<f64>,
    /// Sensor actual temperature (`CCD-TEMP` / `DET-TEMP`). Degrees Celsius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccd_temp_c: Option<f64>,
    /// Right ascension, decimal degrees (`RA` / `OBJCTRA`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ra_deg: Option<f64>,
    /// Declination, decimal degrees (`DEC` / `OBJCTDEC`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dec_deg: Option<f64>,
    /// Mechanical rotator angle, degrees (`ROTATANG` / `ROTATOR`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotator_angle_deg: Option<f64>,
    /// Sensor readout mode (`READOUTM`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readout_mode: Option<String>,
    /// Focal length, millimetres (`FOCALLEN`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length_mm: Option<f64>,
    /// Local civil observation date (`DATE-LOC`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_loc: Option<String>,
}

/// Request for `inbox.item.metadata`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxItemMetadataRequest {
    pub inbox_item_id: String,
}

/// Response from `inbox.item.metadata`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxItemMetadataResponse {
    pub inbox_item_id: String,
    pub files: Vec<InboxFileMetadata>,
}

// ── inbox.stats (spec 041 US6/FR-021) ────────────────────────────────────────

/// Per-frame-type queue stats entry.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxStatsPerType {
    pub frame_type: String,
    /// Number of folder-grouped inbox items of this type.
    pub folder_count: u32,
    /// Number of master inbox items of this type.
    pub master_count: u32,
    /// Total image (file) count across all items of this type.
    pub image_count: u32,
}

/// Aggregate totals across all frame types.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxStatsTotals {
    pub folders: u32,
    pub masters: u32,
    pub images: u32,
}

/// Response from `inbox.stats`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxStatsResponse {
    pub per_type: Vec<InboxStatsPerType>,
    pub totals: InboxStatsTotals,
}

// ── inbox.target_recommendations (spec 041 R-17 / FR-052) ─────────────────────

/// Request for `inbox.target_recommendations`.
///
/// Identify a light sub-group by its `inboxItemId` (R-17: a sub-group is one
/// homogeneous light group). The legacy `sourceGroupId` alternative was dropped
/// in spec 058 (D-002): a recommendation belongs to exactly one inbox item, so
/// a source group is no longer a resolvable target.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxTargetRecommendationsRequest {
    /// The single-type inbox item (light sub-group) to resolve a target for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbox_item_id: Option<String>,
}

/// The sky pointing a recommendation set was computed from (decimal degrees).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxPointing {
    /// Right ascension, ICRS J2000 decimal degrees.
    pub ra_deg: f64,
    /// Declination, ICRS J2000 decimal degrees.
    pub dec_deg: f64,
}

/// One ranked target candidate (R-17 coordinate nearest-neighbour).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxTargetCandidate {
    /// Persisted `canonical_target.id` (UUID string).
    pub target_id: String,
    /// Effective display name (`display_alias ?? primary_designation`).
    pub name: String,
    /// Great-circle angular separation from the sub-group's pointing, in degrees.
    pub separation_deg: f64,
}

/// Response from `inbox.target_recommendations`.
///
/// `candidates` is ranked ascending by angular separation within the configured
/// FOV-aware (or fixed-fallback) radius; empty when no pointing is available.
/// `pointing` is `None` when the light sub-group has no RA/Dec. `objectHint`
/// carries the raw `OBJECT` header for **display only** — never used for
/// matching/search (R-17).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxTargetRecommendationsResponse {
    pub candidates: Vec<InboxTargetCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointing: Option<InboxPointing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_hint: Option<String>,
}

// ── inbox.plan (spec 041 US1/FR-003/FR-004) ───────────────────────────────────

/// One plan action entry in the in-context plan panel.
///
/// `action` is `"move"` | `"catalogue"` | `"archive"` | `"trash"`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxPlanAction {
    /// 1-based ordinal within the plan.
    pub index: u32,
    /// `"move"` | `"catalogue"` | `"archive"` | `"trash"`
    pub action: String,
    pub from_path: String,
    pub to_path: String,
    /// Human-readable resolved destination preview
    /// (equals `from_path` for catalogue actions).
    pub destination_preview: String,
    /// True when this action requires explicit destructive confirmation before apply.
    pub requires_destructive_confirm: bool,
    /// Frame type of the file being acted on (e.g. `"light"`, `"dark"`, `"flat"`,
    /// `"bias"`, `"master_flat"`, etc.). `None` for legacy plans that pre-date this
    /// field or for catalogue / archive / trash actions without a classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type: Option<String>,
}

/// Response from `inbox.plan` — plan(s) linked to an inbox item (spec 041).
///
/// Read via `inbox_plan_links` so the inbox surface can show plan detail
/// without navigating to the Archive page (FR-004).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxPlanView {
    pub plan_id: String,
    pub state: String,
    /// True when the executor's CAS detected that one or more source files
    /// changed since the plan was created (FR-007 / T011).
    /// When `stale` is true the UI should disable Apply and prompt the user
    /// to re-classify and re-confirm.
    pub stale: bool,
    pub actions: Vec<InboxPlanAction>,
}

/// Per-plan result from `inbox.plan.apply_all` (spec 041, FR-003a).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxPlanApplyResult {
    pub inbox_item_id: String,
    pub plan_id: String,
    pub state: String,
    pub error: Option<String>,
}

/// Response from `inbox.plan.apply_all` (spec 041, FR-003a).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxApplyAllResponse {
    pub results: Vec<InboxPlanApplyResult>,
}

/// Response from `inbox.plan.cancel` (spec 041, FR-006).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxPlanCancelResponse {
    pub inbox_item_id: String,
    pub plan_id: String,
    pub state: String,
}

/// One open plan in the aggregate inbox plan surface (spec 041, US2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxOpenPlan {
    pub inbox_item_id: String,
    /// Display label for the ingestion group (the item's relative path / folder name).
    pub item_name: String,
    pub plan_id: String,
    pub state: String,
    pub stale: bool,
    pub actions: Vec<InboxPlanAction>,
}

/// Response from `inbox.plan.list_open` — all open plans across roots (spec 041, US2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxOpenPlansResponse {
    pub plans: Vec<InboxOpenPlan>,
    /// Sum of actions across all plans (for the surface header count).
    pub total_actions: u32,
}

// ── inbox.property_registry (spec 041 R-13 / FR-044) ─────────────────────────

/// Discriminant for the value kind of a property in the property registry.
///
/// Mirrors the `kind` column in the R-13 property table.  The UI uses this to
/// select the appropriate editor widget (number input, enum dropdown, etc.).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PropertyKind {
    /// Free-form text (e.g. camera name, filter label).
    String,
    /// IEEE-754 double (e.g. exposureS, raDeg).
    Number,
    /// Whole number (e.g. offset in ADU).
    Integer,
    /// Either a numeric or text representation (e.g. gain on some cameras).
    NumberOrString,
    /// One of a fixed set of string values (e.g. frameType).
    Enum,
    /// Calendar date (`YYYY-MM-DD`).
    Date,
    /// ISO-8601 date-time (e.g. obsTimeUtc, dateEnd).
    Datetime,
}

/// One entry in the property registry exposed by `inbox.property_registry`.
///
/// Describes a named inbox-file property that the field-agnostic reclassifier
/// (spec 041 R-13) can accept, validate, and persist as an index-side override.
/// The UI uses this registry to render a generic metadata editor without
/// hard-coding field names.
///
/// `sourceHeaders` — the FITS/XISF header keywords that feed this property
/// during extraction; empty for derived or resolve-only properties.
///
/// `overridable` — when `false` the property is informational or derived and
/// the reclassify endpoint will reject an explicit override for it.
///
/// `appliesTo` — frame types for which this property is meaningful; the UI
/// SHOULD hide non-applicable properties rather than blocking on them.
///
/// `validation` — optional human-readable constraint description shown in the
/// UI tooltip; not a machine-parseable expression (use for display only).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PropertyRegistryEntry {
    /// Registry key, camelCase (e.g. `"frameType"`, `"exposureS"`).
    pub key: String,
    /// Value kind discriminant used by the UI for widget selection.
    pub kind: PropertyKind,
    /// Physical unit label for display (e.g. `"s"`, `"deg"`, `"ADU"`).  `None`
    /// when the property is dimensionless or a free-form string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// FITS/XISF source header keyword(s), in priority order.  Empty for
    /// properties that are derived or resolved from external sources (e.g.
    /// `target`, `opticTrain`).
    pub source_headers: Vec<String>,
    /// Whether a user override is accepted by the reclassify use case.
    pub overridable: bool,
    /// Frame types for which this property is applicable.
    pub applies_to: Vec<String>,
    /// Human-readable validation constraint for display in the UI.  `None`
    /// means no documented constraint beyond the `kind` type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<String>,
}

/// Response from `inbox.property_registry` (spec 041 FR-044).
///
/// The full ordered list of known per-file properties, their value kinds, and
/// their editing semantics.  The UI renders this as a generic metadata editor
/// without requiring hard-coded field knowledge.
pub type InboxPropertyRegistryResponse = Vec<PropertyRegistryEntry>;

/// Request for `inbox.plan.apply_selected` (spec 041, US2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxApplySelectedRequest {
    pub inbox_item_ids: Vec<String>,
}

// ── inbox.reclassify v2 — field-agnostic + bulk (spec 041 T068 / R-13) ────────

/// One per-file property override entry in the field-agnostic reclassify request
/// (T068).
///
/// `properties` is an open map of registry-validated property keys (camelCase,
/// as in `inbox.property_registry`) to their JSON values. Unknown or
/// non-overridable keys are rejected by the use case. Only MISSING / unreadable
/// header values may be filled; the frame-type correction (`frameType`) is the
/// one exception (it is always accepted regardless of header presence — R-13).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxReclassifyFileOverride {
    /// Relative file path within the source group (must match an evidence row).
    pub file_path: String,
    /// Property map: `{ "exposureS": 300.0, "filter": "Ha", … }`. Values are
    /// JSON scalars; the use case validates them against the registry `kind`.
    pub properties: std::collections::HashMap<String, JsonAny>,
}

/// One bulk "set all" entry: apply one value to many files at once.
///
/// `file_paths` is optional: when absent the value is applied to **all** files
/// in the source group (the "set all" affordance). When present only the listed
/// paths are updated.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxReclassifyBulk {
    /// Registry property key (camelCase) to set uniformly.
    pub property: String,
    /// Value to apply (JSON scalar; validated against registry `kind`).
    pub value: JsonAny,
    /// Subset of file paths to apply to; `None` / absent = all files in the
    /// source group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_paths: Option<Vec<String>>,
}

/// Request for `inbox.reclassify` — field-agnostic + bulk form (spec 041 T068).
///
/// Scope is the **source group** (R-13): a reclassify may re-partition files
/// across sub-items, so operating at sub-item scope is unsafe. Identify the
/// group by either `sourceGroupId` or `inboxItemId` (the use case looks up the
/// owning source group from the item).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxReclassifyV2Request {
    /// Identify the source group directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_group_id: Option<String>,
    /// Alternatively, identify the group by one of its sub-item IDs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbox_item_id: Option<String>,
    /// Per-file property overrides. Each entry targets one file; multiple
    /// entries may target the same file (last-writer-wins per `property_key`).
    #[serde(default)]
    pub overrides: Vec<InboxReclassifyFileOverride>,
    /// Bulk operations applied after per-file overrides. Processed in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bulk: Vec<InboxReclassifyBulk>,
    /// Absolute path to the inbox root on disk. Not in the JSON Schema
    /// (transport detail), mirroring `InboxClassifyRequest`.
    ///
    /// Required, not optional: without it the re-split cannot hash the group's
    /// files, so every re-materialized sub-item inherits the signature of the
    /// empty set — a fixed constant that compares equal across unrelated items
    /// and silently disables the confirm staleness guard (spec 058 Q-5).
    pub root_absolute_path: String,
}

/// Summary of one re-materialized sub-item returned after reclassify (T068).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxSubItemSummary {
    /// The stable `inbox_items.id` for this sub-item (new UUID if it was
    /// just created, or the existing one if it matched the upsert key).
    pub inbox_item_id: String,
    /// Deterministic canonical group key (R-11).
    pub group_key: String,
    /// Human-readable label `"(root) · <type> · <dims>"`.
    pub group_label: String,
    /// Authoritative frame type; `None` for the needs-review sentinel bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type: Option<String>,
    /// Number of files belonging to this sub-item after re-split.
    pub file_count: u32,
    /// Missing mandatory attributes across the sub-item's files (T070 gate).
    /// Empty when the sub-item can proceed to confirm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_mandatory: Vec<String>,
}

/// Response from `inbox.reclassify` v2 — field-agnostic + bulk (T068).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InboxReclassifyV2Response {
    /// Source group that was operated on.
    pub source_group_id: String,
    /// Re-materialized single-type sub-items after applying overrides +
    /// re-running classification + grouping (R-14 re-split loop).
    pub sub_items: Vec<InboxSubItemSummary>,
    /// Number of sub-items (or files) that still land in the needs-review
    /// sentinel bucket (= still have missing mandatory attributes).
    pub needs_review_count: u32,
}
