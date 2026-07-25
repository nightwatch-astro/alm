// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed enumeration of every `ContractError.code` string produced by the
//! application.
//!
//! ## Purpose
//!
//! `ContractError.code` is currently a plain `String`.  This enum gives the
//! TypeScript surface a discriminated union of the exact wire strings so that
//! client code can exhaustively switch on error codes without string literals.
//!
//! **Wire strings are preserved byte-exactly via explicit `#[serde(rename = "...")]`
//! on every variant.**  `rename_all` is NOT used because most codes contain
//! dots, which `snake_case` / `camelCase` transforms cannot reproduce.
//!
//! ## Scope
//!
//! Covers every code produced by the **central `ContractError` type**.
//! Separate typed sub-enums (`TransitionErrorCode`, `TargetResolveErrorCode`,
//! `LogExportErrorCode`, …) are left in place; their codes do **not** flow
//! through `ContractError.code` in production code and are therefore excluded.
//!
//! ## This task (042-stdlib-adoption T011)
//!
//! This enum is additive scaffolding only.  `ContractError.code` remains a
//! `String` in this task; the type change happens in US2.  `ErrorCode` is
//! registered for specta export so the TypeScript union is emitted immediately.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Exhaustive set of `ContractError.code` wire strings.
///
/// Every variant carries an explicit `#[serde(rename = "exact.wire.string")]`
/// so round-trips are byte-exact regardless of derive rename rules.
///
/// ## Verification
///
/// ```rust
/// use contracts_core::error_code::ErrorCode;
/// assert_eq!(
///     serde_json::to_string(&ErrorCode::InternalDatabase).unwrap(),
///     r#""internal.database""#
/// );
/// assert_eq!(
///     serde_json::to_string(&ErrorCode::PlanNotFound).unwrap(),
///     r#""plan.not_found""#
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ErrorCode {
    // ── Validation ──────────────────────────────────────────────────────────
    #[serde(rename = "validation.request_envelope_invalid")]
    ValidationRequestEnvelopeInvalid,

    // ── Dev-mode ────────────────────────────────────────────────────────────
    #[serde(rename = "dev_mode.disabled")]
    DevModeDisabled,

    // ── Equipment ───────────────────────────────────────────────────────────
    #[serde(rename = "equipment.duplicate")]
    EquipmentDuplicate,
    #[serde(rename = "equipment.not_found")]
    EquipmentNotFound,

    // ── Internal ────────────────────────────────────────────────────────────
    #[serde(rename = "internal.database")]
    InternalDatabase,
    #[serde(rename = "internal.audit")]
    InternalAudit,
    #[serde(rename = "internal.data")]
    InternalData,

    // ── First-run ───────────────────────────────────────────────────────────
    #[serde(rename = "firstrun.incomplete")]
    FirstrunIncomplete,

    // ── Path ────────────────────────────────────────────────────────────────
    #[serde(rename = "path.already_registered")]
    PathAlreadyRegistered,
    #[serde(rename = "path.already_registered.different_kind")]
    PathAlreadyRegisteredDifferentKind,
    #[serde(rename = "path.not_directory")]
    PathNotDirectory,
    #[serde(rename = "path.not_exists")]
    PathNotExists,
    #[serde(rename = "path.permission_denied")]
    PathPermissionDenied,
    #[serde(rename = "path.reserved_name")]
    PathReservedName,
    #[serde(rename = "path.traversal")]
    PathTraversal,
    #[serde(rename = "path.collision")]
    PathCollision,
    #[serde(rename = "path.invalid")]
    PathInvalid,
    /// `roots.register`/`roots.register.batch`: a candidate root path is a
    /// parent of, or nested within, an already-registered root (issue #501).
    /// Cross-cutting across categories — an inbox root inside a light-frames
    /// root is still an overlap.
    #[serde(rename = "path.overlaps_existing")]
    PathOverlapsExisting,

    // ── Inbox ───────────────────────────────────────────────────────────────
    #[serde(rename = "inbox.item.not_found")]
    InboxItemNotFound,
    #[serde(rename = "inbox.has.open.plan")]
    InboxHasOpenPlan,
    #[serde(rename = "inbox.item.no_plan")]
    InboxItemNoPlan,
    // Destination model (spec 041 destination iteration: US8/US9, FR-025–FR-033).
    #[serde(rename = "inbox.no_destination_root")]
    InboxNoDestinationRoot,
    #[serde(rename = "inbox.destination_root_required")]
    InboxDestinationRootRequired,
    #[serde(rename = "inbox.invalid_destination_root")]
    InboxInvalidDestinationRoot,
    #[serde(rename = "inbox.missing_path_attributes")]
    InboxMissingPathAttributes,

    // ── Metadata / classification ────────────────────────────────────────────
    #[serde(rename = "metadata.unreadable")]
    MetadataUnreadable,
    #[serde(rename = "classification.ambiguous")]
    ClassificationAmbiguous,
    #[serde(rename = "classification.stale")]
    ClassificationStale,

    // ── Pattern / token ──────────────────────────────────────────────────────
    #[serde(rename = "pattern.unset")]
    PatternUnset,
    #[serde(rename = "pattern.empty")]
    PatternEmpty,
    #[serde(rename = "pattern.invalid")]
    PatternInvalid,
    #[serde(rename = "pattern.invalid.unicode")]
    PatternInvalidUnicode,
    #[serde(rename = "token.unknown")]
    TokenUnknown,

    // ── File ────────────────────────────────────────────────────────────────
    #[serde(rename = "file.not_found")]
    FileNotFound,

    // ── Notes ────────────────────────────────────────────────────────────────
    #[serde(rename = "note.content_too_large")]
    NoteContentTooLarge,

    // ── Session ──────────────────────────────────────────────────────────────
    #[serde(rename = "session.not_found")]
    SessionNotFound,
    #[serde(rename = "session.mixed_state")]
    SessionMixedState,

    // ── Operation ───────────────────────────────────────────────────────────
    #[serde(rename = "operation.handler_duplicate")]
    OperationHandlerDuplicate,
    #[serde(rename = "operation.not_found")]
    OperationNotFound,

    // ── Plan ─────────────────────────────────────────────────────────────────
    /// Plan approval is outstanding (sent as `ContractError`, not `TransitionError`).
    #[serde(rename = "plan.approval_required")]
    PlanApprovalRequired,
    #[serde(rename = "plan.approval.stale")]
    PlanApprovalStale,
    /// Concurrent apply rejected: the plan's (source ∪ destination ∪ archive)
    /// path set overlaps an active apply run's path set (spec 025 FR-017,
    /// R-Concur-1).
    #[serde(rename = "plan.conflict.overlap")]
    PlanConflictOverlap,
    #[serde(rename = "plan.invalid_state")]
    PlanInvalidState,
    #[serde(rename = "plan.not_found")]
    PlanNotFound,
    #[serde(rename = "plan.not_in_apply")]
    PlanNotInApply,
    #[serde(rename = "plan.blocked_by_protection")]
    PlanBlockedByProtection,
    #[serde(rename = "plan.in_progress")]
    PlanInProgress,
    #[serde(rename = "plan.items.empty")]
    PlanItemsEmpty,

    // ── Plan-apply item / run ────────────────────────────────────────────────
    #[serde(rename = "item.not_failed")]
    ItemNotFailed,
    #[serde(rename = "item.not_found")]
    ItemNotFound,
    #[serde(rename = "item.not_pending")]
    ItemNotPending,
    #[serde(rename = "run.not_found")]
    RunNotFound,
    #[serde(rename = "run.not_paused")]
    RunNotPaused,
    /// `plan.resume` re-validated the pause condition (R-Pause-1, R-Env-1)
    /// and found the paused item's source is still stale; resume is
    /// refused and the plan stays `paused` (spec 025 T048/T049/T050).
    #[serde(rename = "item.still.stale")]
    ItemStillStale,
    /// `plan.resume` re-validated and the paused item's volume is still
    /// unreachable.
    #[serde(rename = "volume.still.unavailable")]
    VolumeStillUnavailable,
    /// `plan.resume` re-validated and the destination volume is still full.
    #[serde(rename = "disk.still.full")]
    DiskStillFull,

    // ── Archive ──────────────────────────────────────────────────────────────
    #[serde(rename = "archive.empty")]
    ArchiveEmpty,
    /// OS trash unavailable or failed for every item in an
    /// `archive.send_to_trash` run (spec 017 US6, spec 025 `FailureCode::OsTrashUnavailable`).
    #[serde(rename = "os_trash.unavailable")]
    OsTrashUnavailable,
    /// OS trash denied permission for every item in an
    /// `archive.send_to_trash` run.
    #[serde(rename = "os_trash.permission.denied")]
    OsTrashPermissionDenied,
    /// Non-permission delete failure (e.g. the file vanished mid-run, its
    /// volume went unavailable, or the destination disk filled) for every
    /// item in an `archive.permanently_delete` run. Permission failures use
    /// the more specific `path.permission_denied`.
    #[serde(rename = "archive.delete_failed")]
    ArchiveDeleteFailed,
    /// #886: `calibration_archive_generator::generate` refuses to archive a
    /// master currently assigned to one or more sessions unless the caller
    /// re-calls with `confirm_in_use = true` (decisions.md: warn + require
    /// confirm before archiving an in-use master).
    #[serde(rename = "calibration.master_in_use")]
    CalibrationMasterInUse,
    /// #886: the master has no `root_id`/`relative_path` resolved to a real
    /// `file_record` (legacy master, or the applied-frame write at
    /// master-confirm time failed) — nothing to archive.
    #[serde(rename = "calibration.master_untracked")]
    CalibrationMasterUntracked,
    /// #886: `calibration.masters.get`/`.archive` target id has no matching
    /// `calibration_session` row.
    #[serde(rename = "master.not_found")]
    MasterNotFound,

    // ── Confirm ──────────────────────────────────────────────────────────────
    #[serde(rename = "confirm.text.mismatch")]
    ConfirmTextMismatch,

    // ── Retry / no-op ────────────────────────────────────────────────────────
    #[serde(rename = "no.items.to.retry")]
    NoItemsToRetry,
    #[serde(rename = "no_op")]
    NoOp,

    // ── Hierarchy ────────────────────────────────────────────────────────────
    #[serde(rename = "parent.not_found")]
    ParentNotFound,
    #[serde(rename = "parent.not_terminal")]
    ParentNotTerminal,

    // ── Lifecycle ────────────────────────────────────────────────────────────
    #[serde(rename = "lifecycle.read_only")]
    LifecycleReadOnly,
    #[serde(rename = "lifecycle.last_confirmed_source")]
    LifecycleLastConfirmedSource,

    // ── Project ──────────────────────────────────────────────────────────────
    #[serde(rename = "project.not_found")]
    ProjectNotFound,
    #[serde(rename = "project.read_only")]
    ProjectReadOnly,
    // ── Project session membership (spec 062) ────────────────────────────────
    #[serde(rename = "project.session_already_pinned")]
    ProjectSessionAlreadyPinned,
    #[serde(rename = "project.session_not_pinned")]
    ProjectSessionNotPinned,
    #[serde(rename = "project.lifecycle_disallows_session_add")]
    ProjectLifecycleDisallowsSessionAdd,
    #[serde(rename = "project.reclassification_revision_invalid")]
    ProjectReclassificationRevisionInvalid,
    #[serde(rename = "project.membership_conflict")]
    ProjectMembershipConflict,

    // ── Framing (spec 008 Q27, F-Framing-3) ────────────────────────────────────
    #[serde(rename = "framing.not_found")]
    FramingNotFound,
    #[serde(rename = "framing.project_mismatch")]
    FramingProjectMismatch,
    #[serde(rename = "framing.merge.requires_two")]
    FramingMergeRequiresTwo,
    #[serde(rename = "framing.merge.duplicate_id")]
    FramingMergeDuplicateId,
    #[serde(rename = "framing.split.empty_selection")]
    FramingSplitEmptySelection,
    #[serde(rename = "framing.split.invalid_session")]
    FramingSplitInvalidSession,
    #[serde(rename = "framing.split.would_empty_source")]
    FramingSplitWouldEmptySource,
    #[serde(rename = "framing.reassign.empty_selection")]
    FramingReassignEmptySelection,

    // ── Attribution (spec 008 Q27, F-Framing-5/10) ──────────────────────────
    #[serde(rename = "attribution.not_light_frame")]
    AttributionNotLightFrame,
    #[serde(rename = "attribution.geometry_unavailable")]
    AttributionGeometryUnavailable,

    // ── View ────────────────────────────────────────────────────────────────
    #[serde(rename = "view.mixed_kind")]
    ViewMixedKind,
    #[serde(rename = "view.not_found")]
    ViewNotFound,
    #[serde(rename = "view.unsupported_kind")]
    ViewUnsupportedKind,

    // ── Source view generation (spec 049) ───────────────────────────────────
    #[serde(rename = "no_selection")]
    NoSelection,
    #[serde(rename = "no_link_kind")]
    NoLinkKind,
    #[serde(rename = "destination.collision")]
    DestinationCollision,
    #[serde(rename = "destination.exists")]
    DestinationExists,
    #[serde(rename = "profile.not_found")]
    ProfileNotFound,

    // ── Canonical target ─────────────────────────────────────────────────────
    #[serde(rename = "canonical_target.not_found")]
    CanonicalTargetNotFound,

    // ── Name ─────────────────────────────────────────────────────────────────
    #[serde(rename = "name.duplicate")]
    NameDuplicate,
    #[serde(rename = "name.empty")]
    NameEmpty,
    #[serde(rename = "name.too_long")]
    NameTooLong,

    // ── Source ───────────────────────────────────────────────────────────────
    #[serde(rename = "source.already.linked")]
    SourceAlreadyLinked,
    #[serde(rename = "source.not_found")]
    SourceNotFound,
    #[serde(rename = "source.invalid_organization_state")]
    SourceInvalidOrganizationState,

    // ── Root ─────────────────────────────────────────────────────────────────
    /// Returned by `roots.delete` (P6b, decision D8) when dependent records
    /// (inbox items, plan items, file records, sessions) still reference the
    /// root; deletion is blocked rather than cascade-nullified.
    #[serde(rename = "root.has_dependents")]
    RootHasDependents,
    /// `roots.remap.apply` (issue #707): the two-step Verify → Apply flow
    /// requires a successful Verify before Apply may mutate the root's path.
    #[serde(rename = "remap.not_verified")]
    RemapNotVerified,

    // ── Tool ────────────────────────────────────────────────────────────────
    #[serde(rename = "tool.locked")]
    ToolLocked,
    #[serde(rename = "tool.unknown")]
    ToolUnknown,

    // ── Resolver / settings ──────────────────────────────────────────────────
    #[serde(rename = "resolver.endpoint_invalid")]
    ResolverEndpointInvalid,
    #[serde(rename = "key.unknown")]
    KeyUnknown,
    #[serde(rename = "key.unoverridable")]
    KeyUnoverridable,
    #[serde(rename = "value.invalid")]
    ValueInvalid,

    // ── Filesystem (plan-apply item error codes) ─────────────────────────────
    /// Used in `ContractError` tests in lib.rs; also may appear via plan-apply.
    #[serde(rename = "filesystem.destination_exists")]
    FilesystemDestinationExists,

    // ── Transition codes that flow through ContractError in some paths ────────
    /// `transition.refused` appears in `ContractError` payloads in
    /// `transition_use_case.rs` (passed as a String literal alongside
    /// `TransitionErrorCode`).  Included here for completeness.
    #[serde(rename = "transition.refused")]
    TransitionRefused,
    /// `plan.required` appears in `ContractError` in `transition_use_case.rs`.
    #[serde(rename = "plan.required")]
    PlanRequired,

    // ── Target / alias codes ───────────────────────────────────────────────────
    /// `target.alias.add` (#751, FR-008): the normalized alias already belongs
    /// to a *different* canonical target. A same-target duplicate stays
    /// idempotent (no error) — only a cross-target collision returns this.
    #[serde(rename = "alias.duplicate")]
    AliasDuplicate,
    #[serde(rename = "alias.blank")]
    AliasBlank,
    #[serde(rename = "alias.not_found")]
    AliasNotFound,
    #[serde(rename = "alias.not_removable")]
    AliasNotRemovable,
    #[serde(rename = "target.not_found")]
    TargetNotFound,
    #[serde(rename = "target.invalid_id")]
    TargetInvalidId,

    // ── Cone-search (spec 052 P3) ─────────────────────────────────────────────
    /// Online resolution disabled or network unavailable — non-blocking
    /// degraded state (FR-018), not a failure; ingest proceeds without a
    /// suggestion.
    #[serde(rename = "resolve.offline")]
    ResolveOffline,
    #[serde(rename = "frameset.not_found")]
    FramesetNotFound,
    /// Equivalent to a pointing `source = "none"` response; kept as a named
    /// code for callers that prefer an error over an empty-suggestions 200.
    #[serde(rename = "pointing.unavailable")]
    PointingUnavailable,
    /// A `target.cone_search.confirm` candidate no longer resolves (e.g. the
    /// object vanished from SIMBAD between suggest and confirm).
    #[serde(rename = "candidate.invalid")]
    CandidateInvalid,

    // ── Launch ───────────────────────────────────────────────────────────────
    /// Appears in `ToolLaunchError.code: String` (`tool_launch.rs`).
    /// Included per task instruction.
    #[serde(rename = "launch.failed")]
    LaunchFailed,
    #[serde(rename = "macos.quarantine.detected")]
    MacosQuarantineDetected,

    // ── Native filesystem / picker ───────────────────────────────────────────
    #[serde(rename = "filters.invalid")]
    FiltersInvalid,
    #[serde(rename = "os.command_failed")]
    OsCommandFailed,
    #[serde(rename = "picker.unavailable")]
    PickerUnavailable,

    // ── Log export ───────────────────────────────────────────────────────────
    #[serde(rename = "format.unsupported")]
    FormatUnsupported,
    #[serde(rename = "range.invalid")]
    RangeInvalid,
    #[serde(rename = "path.write.denied")]
    PathWriteDenied,
    #[serde(rename = "path.parent.missing")]
    PathParentMissing,
    #[serde(rename = "database.error")]
    DatabaseError,
    #[serde(rename = "serialise.error")]
    SerialiseError,
    #[serde(rename = "io.error")]
    IoError,

    // ── Per-frame inventory (spec 048) ───────────────────────────────────────
    /// A root's storage is unavailable (e.g. a removable drive is
    /// disconnected). Frames under it are reported unavailable/missing —
    /// this is a non-destructive terminal state, never an implicit delete.
    #[serde(rename = "root.unavailable")]
    RootUnavailable,
    /// A user-initiated relink's candidate file did not match the missing
    /// frame's sha256 content hash; the record is not re-homed.
    #[serde(rename = "hash.mismatch")]
    HashMismatch,
    /// Referenced `file_record` id does not exist.
    #[serde(rename = "frame.not_found")]
    FrameNotFound,

    // ── Onboarding (spec 056) ──────────────────────────────────────────────────
    /// `onboarding.item.set_state` referenced an `item_id` not in the
    /// registry (contracts/onboarding-commands.md `unknown_item`).
    #[serde(rename = "onboarding.item.unknown")]
    OnboardingItemUnknown,
    /// A request violated an onboarding state-shape rule: `item.set_state`
    /// with an auto state (`unchecked`/`auto_checked`), or `section.set` with
    /// `hidden: false` — unhiding is exclusively `onboarding.restore`
    /// (contracts/onboarding-commands.md `invalid_state`).
    #[serde(rename = "onboarding.invalid_state")]
    OnboardingInvalidState,

    // ── Generic fallback ─────────────────────────────────────────────────────
    /// Used when a legacy `String` error is wrapped into `ContractError`.
    #[serde(rename = "internal.error")]
    InternalError,
}

#[cfg(test)]
mod tests {
    use super::ErrorCode;

    fn wire(v: ErrorCode) -> String {
        serde_json::to_string(&v).expect("serialize")
    }

    fn round_trip(v: ErrorCode) {
        let s = wire(v);
        let back: ErrorCode = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back, v, "round-trip failed for {s}");
    }

    #[test]
    fn exact_wire_strings_for_required_subset() {
        // The task mandates these exact strings.
        assert_eq!(wire(ErrorCode::InternalDatabase), r#""internal.database""#);
        assert_eq!(wire(ErrorCode::InternalAudit), r#""internal.audit""#);
        assert_eq!(wire(ErrorCode::PlanRequired), r#""plan.required""#);
        assert_eq!(wire(ErrorCode::PlanNotFound), r#""plan.not_found""#);
        assert_eq!(wire(ErrorCode::TransitionRefused), r#""transition.refused""#);
        assert_eq!(wire(ErrorCode::AliasDuplicate), r#""alias.duplicate""#);
        assert_eq!(wire(ErrorCode::TargetNotFound), r#""target.not_found""#);
        assert_eq!(wire(ErrorCode::LaunchFailed), r#""launch.failed""#);
        assert_eq!(wire(ErrorCode::ProjectNotFound), r#""project.not_found""#);
        assert_eq!(wire(ErrorCode::ViewNotFound), r#""view.not_found""#);
    }

    #[test]
    fn all_required_variants_round_trip() {
        for v in [
            ErrorCode::InternalDatabase,
            ErrorCode::InternalAudit,
            ErrorCode::PlanRequired,
            ErrorCode::PlanNotFound,
            ErrorCode::TransitionRefused,
            ErrorCode::AliasDuplicate,
            ErrorCode::TargetNotFound,
            ErrorCode::LaunchFailed,
            ErrorCode::ProjectNotFound,
            ErrorCode::ViewNotFound,
        ] {
            round_trip(v);
        }
    }
}
