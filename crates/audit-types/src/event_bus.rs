// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Event envelope and payload types for the hybrid event bus.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;

use domain_core::ids::Timestamp;
use domain_core::lifecycle::data_asset::EntityType;

/// Who caused the event to be emitted.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    Type,
)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    User,
    Restore,
    System,
}

/// Versioned event envelope wrapping any serialisable payload.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope<P: Type> {
    pub contract_version: String,
    pub topic: String,
    pub source: Source,
    pub emitted_at: Timestamp,
    pub payload: P,
}

impl<P: Type> EventEnvelope<P> {
    #[must_use]
    pub fn new(topic: impl Into<String>, source: Source, payload: P) -> Self {
        Self {
            contract_version: "1.0.0".to_owned(),
            topic: topic.into(),
            source,
            emitted_at: Timestamp::now_utc(),
            payload,
        }
    }
}

/// Payload for the `lifecycle.transition.applied` topic.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleTransitionApplied {
    pub entity_type: EntityType,
    pub entity_id: String,
    pub from_state: String,
    pub to_state: String,
    pub actor: String,
    pub at: Timestamp,
    /// The project whose dependents (research.md §6 fan-out —
    /// `processing_artifact` / `prepared_source_view` rows carrying the same
    /// `project_id`) should be recomputed, when resolvable. `Some(entity_id)`
    /// when `entity_type == Project`; `None` when not resolvable at the
    /// call site (spec 002 FR-003, #713 — minimal slice, no propagation
    /// redesign).
    pub project_id: Option<String>,
}

pub const TOPIC_LIFECYCLE_TRANSITION_APPLIED: &str = "lifecycle.transition.applied";

/// Per-kind source counts for the `first_run.completed` audit event.
///
/// `calibration` replaces the former `dark`, `flat`, and `bias` fields now
/// that the source-folder kind is unified into a single `calibration` bucket.
/// Per-image frame type is detected from image metadata (FITS `IMAGETYP`),
/// not from the source-folder kind.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceCountByKind {
    pub light_frames: usize,
    pub calibration: usize,
    pub project: usize,
    pub inbox: usize,
}

/// Payload for the `first_run.completed` topic (spec 003).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct FirstRunCompleted {
    pub completed_at: String,
    pub source_count_by_kind: SourceCountByKind,
}

pub const TOPIC_FIRST_RUN_COMPLETED: &str = "first_run.completed";

/// Payload for the `root.remapped` topic (P6a — Data Sources "Remap" flow).
///
/// Emitted after a root's stored path is updated via `roots.remap.apply`.
/// `verified` mirrors the `allVerified` flag from the `roots.remap` preview
/// the operator reviewed before applying (constitution Principle II: every
/// filesystem-affecting mutation is audited, with confidence recorded where
/// inference/sampling was used).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct RootRemapped {
    pub root_id: String,
    pub original_path: String,
    pub new_path: String,
    pub verified: bool,
}

pub const TOPIC_ROOT_REMAPPED: &str = "root.remapped";

/// Payload for the `root.active_changed` topic (P6b — Data Sources
/// Disable/Enable flow).
///
/// Emitted after a root's `active` flag is toggled via `sources.set_active`.
/// Disabling excludes the root from scan/ingest surfaces while its history
/// (sessions, plan items, file records) stays fully intact — this is a
/// visibility flag, not a deletion.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct RootActiveChanged {
    pub root_id: String,
    pub path: String,
    pub active: bool,
}

pub const TOPIC_ROOT_ACTIVE_CHANGED: &str = "root.active_changed";

/// Payload for the `root.deleted` topic (P6b — Data Sources Delete flow).
///
/// Emitted after a root's registration is removed from `registered_sources`.
/// Only fires when the root had no dependent records — `roots.delete` blocks
/// with `root.has_dependents` otherwise (decision D8, no cascade-nullify).
/// Files on disk are never touched (constitution Principle I).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct RootDeleted {
    pub root_id: String,
    pub path: String,
    pub kind: String,
}

pub const TOPIC_ROOT_DELETED: &str = "root.deleted";

// ── Native filesystem control audit events (spec 004) ─────────────────────

// #717 FR-006: `native.picker.failed` (`PickerKind`/`NativePickerFailed`/
// `TOPIC_NATIVE_PICKER_FAILED`) was defined but never constructed anywhere —
// tauri-plugin-dialog's blocking picker API (`blocking_pick_folder`/
// `blocking_pick_file`, used by `commands::native::native_directory_pick`/
// `native_file_pick`) returns `Option<FilePath>` with no error variant at
// all, so there was never a real "picker failed" signal distinct from "user
// cancelled" to emit it from. Removed per the issue's own fallback ("emit
// from a real failure path or remove the dead type") rather than fabricate
// a failure condition the underlying dialog API cannot report.

/// Payload for the `native.reveal.failed` topic (spec 004).
///
/// Audit event emitted when a reveal-in-OS operation fails.
/// Does NOT include path or path_hash fields (A2 decision: correlate via entity_id).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct NativeRevealFailed {
    pub error_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    pub request_id: String,
}

pub const TOPIC_NATIVE_REVEAL_FAILED: &str = "native.reveal.failed";

// ── Settings audit events (spec 018, T005) ────────────────────────────────

/// Payload for the `settings.changed` topic (spec 018, T005).
///
/// Emitted for non-noisy key writes when the value actually changed.
/// Noisy keys (pattern, protectedCategories, rememberFollowLogs) appear in
/// `settings.snapshot` instead.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsChanged {
    /// The settings key that was written.
    pub key: String,
    /// Value before the write (JSON value).
    pub prior_value: serde_json::Value,
    /// Value after the write (JSON value).
    pub new_value: serde_json::Value,
    /// ISO-8601 timestamp.
    pub at: String,
}

pub const TOPIC_SETTINGS_CHANGED: &str = "settings.changed";

/// Payload for the `settings.snapshot` topic (spec 018, T005).
///
/// Emitted at session start and after a 5-minute inactivity debounce following
/// noisy-key writes (R-Aud-1). Contains the current value of noisy keys only.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    /// Reason the snapshot was taken: "session_start" or "inactivity_debounce".
    pub trigger: String,
    /// Snapshot of noisy key values at the time of emission.
    pub noisy_keys: serde_json::Value,
    /// ISO-8601 timestamp.
    pub at: String,
}

pub const TOPIC_SETTINGS_SNAPSHOT: &str = "settings.snapshot";

/// Payload for the `settings.repair` topic (spec 018, T005).
///
/// Emitted at warn level when a stored settings value fails schema validation
/// and is reset to its in-code default (T019).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsRepair {
    /// The settings key that was reset.
    pub key: String,
    /// The invalid stored value that triggered the repair.
    pub invalid_value: serde_json::Value,
    /// The default value restored.
    pub default_value: serde_json::Value,
    /// ISO-8601 timestamp.
    pub at: String,
}

pub const TOPIC_SETTINGS_REPAIR: &str = "settings.repair";

// ── Plan lifecycle audit events (spec 017, A7) ────────────────────────────────

/// Payload for the `plan.approved` topic (spec 017, A7).
///
/// Emitted when a reviewer approves a plan. Includes the actor and prior state.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanApproved {
    /// Stable plan id.
    pub plan_id: String,
    /// Prior state before approval (always `ready_for_review`).
    pub prior_state: String,
    /// Actor who approved the plan.
    pub actor: String,
    /// ISO-8601 timestamp of approval.
    pub approved_at: String,
}

pub const TOPIC_PLAN_APPROVED: &str = "plan.approved";

/// Payload for the `plan.discarded` topic (spec 017, A7, A5).
///
/// Emitted when a plan is soft-deleted. The audit record is retained even after
/// the plan's `discardedAt` is set.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanDiscarded {
    /// Stable plan id.
    pub plan_id: String,
    /// State at the time of discard.
    pub prior_state: String,
    /// ISO-8601 timestamp of discard.
    pub discarded_at: String,
}

pub const TOPIC_PLAN_DISCARDED: &str = "plan.discarded";

/// Payload for the `plan.retry_created` topic (spec 017, A7).
///
/// Emitted when a retry plan is created from a terminal parent.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanRetryCreated {
    /// The new (retry) plan id.
    pub new_plan_id: String,
    /// The terminal parent plan id.
    pub parent_plan_id: String,
    /// Which items filter was used: "failed", "cancelled", or "all".
    pub items_filter: String,
    /// Number of items materialised into the retry plan.
    pub items_total: i64,
    /// ISO-8601 timestamp.
    pub at: String,
}

pub const TOPIC_PLAN_RETRY_CREATED: &str = "plan.retry_created";

/// Payload for the `archive.sent_to_trash` topic (spec 017, R-Archive-2).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSentToTrash {
    pub plan_id: String,
    pub items_moved: i64,
    pub at: String,
}

pub const TOPIC_ARCHIVE_SENT_TO_TRASH: &str = "archive.sent_to_trash";

/// Payload for the `archive.permanently_deleted` topic (spec 017, R-Archive-2).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePermanentlyDeleted {
    pub plan_id: String,
    pub items_deleted: i64,
    pub at: String,
}

pub const TOPIC_ARCHIVE_PERMANENTLY_DELETED: &str = "archive.permanently_deleted";

// ── Plan apply audit events (spec 025, A7) ────────────────────────────────────

/// Payload for the `plan.applying.started` topic (spec 025, A7).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanApplyingStarted {
    pub plan_id: String,
    pub run_id: String,
    pub items_total: i64,
    pub at: String,
}

pub const TOPIC_PLAN_APPLYING_STARTED: &str = "plan.applying.started";

/// Payload for the `plan.item.progress` topic (spec 025, A7).
///
/// Emitted per item state transition. `failure` is present when
/// `new_state` is `"failed"` or `"stale"`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemProgress {
    pub plan_id: String,
    pub run_id: String,
    pub item_id: String,
    pub prior_state: String,
    pub new_state: String,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_recoverable: Option<bool>,
}

pub const TOPIC_PLAN_ITEM_PROGRESS: &str = "plan.item.progress";

/// Payload for the `plan.applying.paused` topic (spec 025, A7, R-Pause-1).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanApplyingPaused {
    pub plan_id: String,
    pub run_id: String,
    pub pause_reason: String,
    pub at: String,
}

pub const TOPIC_PLAN_APPLYING_PAUSED: &str = "plan.applying.paused";

/// Payload for the `plan.applying.resumed` topic (spec 025, A7, R-Pause-1).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanApplyingResumed {
    pub plan_id: String,
    pub run_id: String,
    pub at: String,
}

pub const TOPIC_PLAN_APPLYING_RESUMED: &str = "plan.applying.resumed";

/// Payload for the `plan.applying.completed` topic (spec 025, A7).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanApplyingCompleted {
    pub plan_id: String,
    pub run_id: String,
    pub terminal_state: String,
    pub items_applied: i64,
    pub items_failed: i64,
    pub items_skipped: i64,
    pub items_cancelled: i64,
    pub at: String,
}

pub const TOPIC_PLAN_APPLYING_COMPLETED: &str = "plan.applying.completed";

// Catalog download audit events (spec 014) were removed in spec 035 T034: the
// hosted catalog-download surface is superseded by SIMBAD resolve-on-demand.
// See `target.resolved` / `target.resolve_batch.completed` below.

/// Payload for the `tool.launch` topic (spec 011, T009).
///
/// Emitted after a processing tool is spawned (or attempted) for a project.
/// `outcome` values mirror `LaunchOutcome` in the data model:
///   `spawned` | `spawn_failed` | `tool_not_configured` | `executable_not_found`
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolLaunchEvent {
    pub launch_id: String,
    pub project_id: String,
    pub tool_id: String,
    /// Resolved working directory passed to the child process.
    pub working_dir: Option<String>,
    /// BLAKE3 hex of `canonicalized_executable_path || rendered_argv`.
    pub args_hash: Option<String>,
    /// `spawned` | `spawn_failed` | `tool_not_configured` | `executable_not_found`
    pub outcome: String,
    pub at: String,
}

pub const TOPIC_TOOL_LAUNCH: &str = "tool.launch";

// ── Artifact observation audit events (spec 012, T007/T007b/FR-008) ──────────

/// Payload for the `artifact.detected` topic (spec 012, T007).
///
/// Emitted when a new `ProcessingArtifact` row is created.
/// Constitution III: file was observed, never written or opened.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDetected {
    /// UUID of the new `ProcessingArtifact` row.
    pub artifact_id: String,
    pub project_id: String,
    /// Project-relative path of the observed file.
    pub path: String,
    pub kind: String,
    pub tool: String,
    pub classification_source: String,
    pub classification_confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_launch_id: Option<String>,
    pub detected_at: String,
}

pub const TOPIC_ARTIFACT_DETECTED: &str = "artifact.detected";

/// Payload for the `artifact.updated` topic (spec 012, T007, A8).
///
/// Emitted when a tool overwrites a path already indexed; the row is updated
/// in-place (`content_hash` refreshed). No new `artifact.detected` is emitted.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactUpdated {
    pub artifact_id: String,
    pub project_id: String,
    pub path: String,
    pub tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_content_hash: Option<String>,
    pub updated_at: String,
}

pub const TOPIC_ARTIFACT_UPDATED: &str = "artifact.updated";

/// Payload for the `artifact.missing` topic (spec 012, T007).
///
/// Emitted when a reconciliation scan finds that a previously `present` file
/// is no longer on disk (state → `missing`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactMissing {
    pub artifact_id: String,
    pub project_id: String,
    pub path: String,
    pub at: String,
}

pub const TOPIC_ARTIFACT_MISSING: &str = "artifact.missing";

/// Payload for the `artifact.recovered` topic (spec 012, T007).
///
/// Emitted when a `missing` artifact is found again on disk.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRecovered {
    pub artifact_id: String,
    pub project_id: String,
    pub path: String,
    pub at: String,
}

pub const TOPIC_ARTIFACT_RECOVERED: &str = "artifact.recovered";

/// Payload for the `artifact.classify.override` topic (spec 012, T014).
///
/// Emitted when a user manually overrides an artifact's classification kind.
/// Override is sticky; subsequent re-classifications skip manual rows (T015).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactClassifyOverride {
    pub artifact_id: String,
    pub project_id: String,
    pub new_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub at: String,
}

/// Payload for the `artifact.classified` topic (spec 012, FR-009, spec 033 T028).
///
/// Emitted by the artifact watcher after a file is detected AND classified.
/// Carries the classification result with a confidence level (Constitution §II).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactClassified {
    pub artifact_id: String,
    pub project_id: String,
    /// `intermediate` | `master` | `final`
    pub classification: String,
    /// Confidence in [0.0, 1.0]. Present when inference is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    pub classified_at: String,
}

pub const TOPIC_ARTIFACT_CLASSIFIED: &str = "artifact.classified";

pub const TOPIC_ARTIFACT_CLASSIFY_OVERRIDE: &str = "artifact.classify.override";

/// Payload for the `artifact.classify.override.cleared` topic (spec 012, T014, A6).
///
/// Emitted when a `kind: null` call deletes the override row and triggers
/// rule re-classification.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactClassifyOverrideCleared {
    pub artifact_id: String,
    pub project_id: String,
    /// Kind the artifact held before the override was cleared.
    pub prior_kind: String,
    /// Kind assigned by rule re-classification after clearing.
    pub new_kind: String,
    pub at: String,
}

pub const TOPIC_ARTIFACT_CLASSIFY_OVERRIDE_CLEARED: &str = "artifact.classify.override.cleared";

/// Payload for the `artifact.user_resolved` topic (spec 012).
///
/// Emitted when the user marks a `missing` artifact as resolved.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactUserResolved {
    pub artifact_id: String,
    pub project_id: String,
    pub at: String,
}

pub const TOPIC_ARTIFACT_USER_RESOLVED: &str = "artifact.user_resolved";

/// Payload for the `workflow.run_completed` topic (spec 012, FR-010, R-Event-Light).
///
/// Emitted when the attribution pass sets `ToolLaunch.completed_at`.
/// Spec 024 subscribes to this event to write `workflow_run` manifests.
/// Source is always `system` for this event.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunCompleted {
    pub project_id: String,
    pub tool_id: String,
    pub tool_launch_id: String,
    pub completed_at: String,
    /// UUIDs of `ProcessingArtifact` rows attributed to this launch.
    pub artifact_ids: Vec<String>,
}

pub const TOPIC_WORKFLOW_RUN_COMPLETED: &str = "workflow.run_completed";

// ── Spec 016: Source Protection ───────────────────────────────────────────

/// Payload for the `protection.source.set` topic (spec 016 T016).
///
/// Emitted every time a per-source protection override is written or updated.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionSourceSet {
    pub source_id: String,
    pub prior_level: String,
    pub new_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_categories: Option<Vec<String>>,
    pub at: String,
}

pub const TOPIC_PROTECTION_SOURCE_SET: &str = "protection.source.set";

/// Payload for the `protection.plan.acknowledged` topic (spec 016 T025).
///
/// Emitted when the user explicitly acknowledges a protected plan item before
/// plan execution proceeds.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionPlanAcknowledged {
    pub plan_id: String,
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub resolved_level: String,
    pub reason: String,
    pub at: String,
}

pub const TOPIC_PROTECTION_PLAN_ACKNOWLEDGED: &str = "protection.plan.acknowledged";

/// Payload for the `protection.default.changed` topic (spec 033 T045, FR-018).
///
/// Emitted when a global protection default (level, blockPermanentDelete, or
/// protectedCategories) is changed by the user or system.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionDefaultChanged {
    /// Scope of the default (e.g. `"global"`).
    pub scope: String,
    /// Key that changed (e.g. `"defaultProtection"`, `"blockPermanentDelete"`,
    /// `"protectedCategories"`).
    pub key: String,
    /// Prior raw JSON value; absent if the row was newly created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<serde_json::Value>,
    /// New raw JSON value.
    pub new: serde_json::Value,
    pub changed_at: String,
}

pub const TOPIC_PROTECTION_DEFAULT_CHANGED: &str = "protection.default.changed";

// ── Domain-completion audit events ───────────────────────────────────────────

/// Payload for the `inventory.confirmed` topic (spec 005 T027).
///
/// Emitted when an inbox item is confirmed into inventory through the normal
/// confirm path. `source` on the envelope MUST be checked by the onboarding
/// tick subscriber: ignore events where `source == Restore`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConfirmed {
    /// The inbox item id that was confirmed.
    pub inbox_item_id: String,
    /// The resulting plan id.
    pub plan_id: String,
    pub at: String,
}

pub const TOPIC_INVENTORY_CONFIRMED: &str = "inventory.confirmed";

// ── Spec 035: SIMBAD target resolution ────────────────────────────────────────
//
// These topics REPLACE the spec-014 `catalog.download.*` topics, which were
// removed in T034. Emitted by the resolve/upsert path and the ingest
// background drain (FR-013, FR-006).

/// Payload for the `target.resolved` topic (spec 035).
///
/// Emitted when a target identity is resolved and written to the cache — either
/// from an interactive `target.resolve` or from the background ingest drain.
/// Coordinates are never fabricated (FR-009); this event only fires for an
/// actually-resolved canonical target.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetResolved {
    /// Canonical target id (UUIDv5) the object was resolved to.
    pub target_id: String,
    /// SIMBAD physical-object id (dedup key) when resolved online.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simbad_oid: Option<i64>,
    /// Canonical display designation.
    pub primary_designation: String,
    /// Provenance of the identity (`seed` | `resolved` | `user-override`).
    pub source: String,
    /// The query/`OBJECT` value that triggered the resolution, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub at: String,
}

pub const TOPIC_TARGET_RESOLVED: &str = "target.resolved";

/// Payload for the `target.resolve_batch.completed` topic (spec 035, FR-013).
///
/// Emitted when the background ingest-resolution drain finishes a pass over the
/// pending queue. Reports how many images resolved vs. stayed unresolved
/// (retryable — never silently mis-assigned, FR-009).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetResolveBatchCompleted {
    /// Pending rows considered in this drain pass.
    pub considered: usize,
    /// Rows that resolved to a canonical target and were associated.
    pub resolved: usize,
    /// Rows left unresolved — genuine content misses (unknown/ambiguous),
    /// `attempts` incremented, retryable.
    pub unresolved: usize,
    /// Rows left `pending` due to a transient/offline condition (no `attempts`
    /// increment), retried on the next drain pass (FIX-4).
    #[serde(default)]
    pub pending: usize,
    pub at: String,
}

pub const TOPIC_TARGET_RESOLVE_BATCH_COMPLETED: &str = "target.resolve_batch.completed";

// ── Target note audit event (spec 023 US4) ───────────────────────────────────

/// Payload for the `target.note.updated` topic (spec 023, US4).
///
/// Emitted after a successful `target.note.update` write.  The note body is
/// NOT included in the audit payload (privacy); `has_notes` indicates whether
/// a non-empty note is now stored.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetNoteUpdated {
    /// Stable canonical target id (UUID string).
    pub target_id: String,
    /// `true` when a non-empty note is now stored; `false` when cleared.
    pub has_notes: bool,
    /// ISO-8601 timestamp of the write.
    pub at: String,
}

pub const TOPIC_TARGET_NOTE_UPDATED: &str = "target.note.updated";

// ── Settings schema migration audit event (spec 018 US5 T031) ────────────────

/// Payload for the `settings.migration` topic (spec 018 US5, T031).
///
/// Emitted once at `info` level when a settings schema migration run
/// completes (v1 → v2 or any future version bump). Carries counts so the
/// audit record is self-describing without requiring access to the DB diff.
///
/// Fields mirror `MigrationSummary` in `app_core_settings::migrate` so the
/// wire payload is stable even if the internal struct diverges.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMigration {
    /// Human-readable label, e.g. `"v1->v2"`.
    pub migration: String,
    /// Number of keys carried over unchanged (retained).
    pub migrated: usize,
    /// Number of keys deleted because they are obsolete in the new version.
    pub dropped: usize,
    /// Number of keys reset to their in-code default (value semantics changed).
    pub reset: usize,
    /// ISO-8601 timestamp of the migration run.
    pub at: String,
}

pub const TOPIC_SETTINGS_MIGRATION: &str = "settings.migration";

// ── Per-frame inventory audit events (spec 048 T007) ─────────────────────────
//
// Reconciliation only ever updates records/UI, never files (FR-008/INV-2);
// these events are records-only and never imply a filesystem mutation.
// Payload shapes mirror the `Artifact{Missing,Recovered}` precedent above.

/// Payload for the `frame.missing` topic (spec 048 FR-007/FR-009).
///
/// Emitted when a reconciliation pass finds that a previously present
/// `file_record` is no longer at its recorded path.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct FrameMissing {
    pub frame_id: String,
    pub root_id: String,
    pub relative_path: String,
    /// What triggered the reconcile pass: `on_demand` | `on_open` | `scheduled` | `live_event`.
    pub reason: String,
    pub at: String,
}

pub const TOPIC_FRAME_MISSING: &str = "frame.missing";

/// Payload for the `frame.recovered` topic (spec 048 FR-011).
///
/// Emitted when a `missing` frame is found present again at its recorded path.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct FrameRecovered {
    pub frame_id: String,
    pub root_id: String,
    pub relative_path: String,
    pub at: String,
}

pub const TOPIC_FRAME_RECOVERED: &str = "frame.recovered";

/// Payload for the `frame.size_backfilled` topic (spec 048 FR-006, T015).
///
/// Emitted when a present `file_record` whose `size_bytes` was `0`/unknown is
/// corrected to the real on-disk size during a reconcile pass.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct FrameSizeBackfilled {
    pub frame_id: String,
    pub root_id: String,
    pub relative_path: String,
    pub prior_size_bytes: i64,
    pub size_bytes: i64,
    pub at: String,
}

pub const TOPIC_FRAME_SIZE_BACKFILLED: &str = "frame.size_backfilled";

/// Payload for the `frame.relinked` topic (spec 048 FR-012a).
///
/// Emitted after a user-initiated relink succeeds (sha256 content hash
/// confirmed identity, computed on demand for exactly the two files
/// involved — never eager, never size/mtime).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct FrameRelinked {
    pub frame_id: String,
    pub root_id: String,
    pub from_path: String,
    pub to_path: String,
    pub sha256: String,
    pub at: String,
}

pub const TOPIC_FRAME_RELINKED: &str = "frame.relinked";

/// Payload for the `calibration_match.source_missing` topic (spec 048 FR-024).
///
/// Emitted when a calibration frame referenced by a calibration match is
/// marked missing. The match is flagged "source missing / unverifiable" —
/// it is NEVER automatically invalidated or removed.
///
/// Two distinct trigger paths reuse this same payload shape (US5): `frame_id`
/// holds a `file_record.id` for a missing raw source sub-frame (PATH B), or a
/// `processing_artifacts.id` for a missing generated master file (PATH A).
/// `match_id` is the `calibration_assignment.id` of the affected match.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationMatchSourceMissing {
    pub match_id: String,
    pub frame_id: String,
    pub at: String,
}

pub const TOPIC_CALIBRATION_MATCH_SOURCE_MISSING: &str = "calibration_match.source_missing";

/// Payload for the `calibration_match.source_recovered` topic (spec 048 FR-025).
///
/// Emitted when a previously missing referenced frame returns to present,
/// clearing the match's "source missing" flag. See
/// [`CalibrationMatchSourceMissing`] for the dual meaning of `frame_id`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationMatchSourceRecovered {
    pub match_id: String,
    pub frame_id: String,
    pub at: String,
}

pub const TOPIC_CALIBRATION_MATCH_SOURCE_RECOVERED: &str = "calibration_match.source_recovered";

// ── Topic-family prefix constants ─────────────────────────────────────────────
//
// These anchor `from_topic` prefix checks (contracts_core::log) to the same
// crate that defines the individual topic strings, so adding a new topic family
// requires editing only audit-types (bd astro-plan-kyo7.88).

pub const TOPIC_FAMILY_CATALOG: &str = "catalog.";
pub const TOPIC_FAMILY_PLAN: &str = "plan.";
pub const TOPIC_FAMILY_ARCHIVE: &str = "archive.";
pub const TOPIC_FAMILY_WORKFLOW: &str = "workflow.";
pub const TOPIC_FAMILY_ARTIFACT: &str = "artifact.";
pub const TOPIC_FAMILY_LIFECYCLE: &str = "lifecycle.";
pub const TOPIC_FAMILY_INVENTORY: &str = "inventory.";
pub const TOPIC_FAMILY_SETTINGS: &str = "settings.";
pub const TOPIC_FAMILY_PROJECT: &str = "project.";
pub const TOPIC_FAMILY_TARGET: &str = "target.";
pub const TOPIC_FAMILY_TOOL: &str = "tool.";
pub const TOPIC_FAMILY_AUDIT: &str = "audit.";
pub const TOPIC_FAMILY_NATIVE: &str = "native.";
pub const TOPIC_FAMILY_FIRST_RUN: &str = "first_run.";
pub const TOPIC_FAMILY_PROTECTION: &str = "protection.";
