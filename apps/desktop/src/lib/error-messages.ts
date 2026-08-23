// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * User-facing error messages, keyed by the generated `ErrorCode` union and
 * resolved through the message catalog (spec 046 US2).
 *
 * EXHAUSTIVE by design: `Record<ErrorCode, () => string>`. When the backend adds
 * an `ErrorCode` variant, the regenerated TS union gains a member and this map
 * fails to type-check until a catalog message is added — so an untranslated code
 * is caught at build time, never shipped as a blank or a raw code (FR-007, SC-003).
 *
 * Values are catalog message functions (`m.err_*`), never inline literals, so all
 * error wording lives in `messages/en.json` like every other user-facing string
 * (FR-008, FR-001).
 */
import type { ErrorCode } from '@/bindings/index';
import { m } from '@/lib/i18n';

export const ERROR_MESSAGES: Record<ErrorCode, () => string> = {
  'validation.request_envelope_invalid':
    m.err_validation_request_envelope_invalid,
  'dev_mode.disabled': m.err_dev_mode_disabled,
  'equipment.duplicate': m.err_equipment_duplicate,
  'equipment.not_found': m.err_equipment_not_found,
  'internal.database': m.err_internal_database,
  'internal.audit': m.err_internal_audit,
  'internal.data': m.err_internal_data,
  'firstrun.incomplete': m.err_firstrun_incomplete,
  'path.already_registered': m.err_path_already_registered,
  'path.already_registered.different_kind':
    m.err_path_already_registered_different_kind,
  'path.not_directory': m.err_path_not_directory,
  'path.not_exists': m.err_path_not_exists,
  'path.permission_denied': m.err_path_permission_denied,
  'path.reserved_name': m.err_path_reserved_name,
  'path.traversal': m.err_path_traversal,
  'path.collision': m.err_path_collision,
  'path.invalid': m.err_path_invalid,
  'path.overlaps_existing': m.err_path_overlaps_existing,
  'inbox.item.not_found': m.err_inbox_item_not_found,
  'inbox.has.open.plan': m.err_inbox_has_open_plan,
  'inbox.item.no_plan': m.err_inbox_item_no_plan,
  'inbox.no_destination_root': m.err_inbox_no_destination_root,
  'inbox.destination_root_required': m.err_inbox_destination_root_required,
  'inbox.invalid_destination_root': m.err_inbox_invalid_destination_root,
  'inbox.destination_collision': m.err_inbox_destination_collision,
  'inbox.missing_path_attributes': m.err_inbox_missing_path_attributes,
  'metadata.unreadable': m.err_metadata_unreadable,
  'classification.ambiguous': m.err_classification_ambiguous,
  'classification.stale': m.err_classification_stale,
  'pattern.unset': m.err_pattern_unset,
  'pattern.empty': m.err_pattern_empty,
  'pattern.invalid': m.err_pattern_invalid,
  'pattern.invalid.unicode': m.err_pattern_invalid_unicode,
  'token.unknown': m.err_token_unknown,
  'file.not_found': m.err_file_not_found,
  'note.content_too_large': m.err_note_content_too_large,
  'session.not_found': m.err_session_not_found,
  'session.mixed_state': m.calibration_session_mixed_state,
  'operation.handler_duplicate': m.err_operation_handler_duplicate,
  'operation.not_found': m.err_operation_not_found,
  'plan.approval_required': m.err_plan_approval_required,
  'plan.approval.stale': m.err_plan_approval_stale,
  'plan.conflict.overlap': m.err_plan_conflict_overlap,
  'plan.invalid_state': m.err_plan_invalid_state,
  'plan.not_found': m.err_plan_not_found,
  'plan.not_in_apply': m.err_plan_not_in_apply,
  'plan.blocked_by_protection': m.err_plan_blocked_by_protection,
  'plan.in_progress': m.err_plan_in_progress,
  'plan.items.empty': m.err_plan_items_empty,
  'item.not_failed': m.err_item_not_failed,
  'item.not_found': m.err_item_not_found,
  'item.not_pending': m.err_item_not_pending,
  'run.not_found': m.err_run_not_found,
  'run.not_paused': m.err_run_not_paused,
  // Resume re-validation (spec 025 R-Pause-1, issue #575).
  'item.still.stale': m.err_item_still_stale,
  'volume.still.unavailable': m.err_volume_still_unavailable,
  'disk.still.full': m.err_disk_still_full,
  'archive.empty': m.err_archive_empty,
  // #886 — calibration master archive.
  'calibration.master_in_use': m.err_calibration_master_in_use,
  'calibration.master_untracked': m.err_calibration_master_untracked,
  'master.not_found': m.err_master_not_found,
  'os_trash.unavailable': m.err_os_trash_unavailable,
  'os_trash.permission.denied': m.err_os_trash_permission_denied,
  'archive.delete_failed': m.err_archive_delete_failed,
  'confirm.text.mismatch': m.err_confirm_text_mismatch,
  'no.items.to.retry': m.err_no_items_to_retry,
  no_op: m.err_no_op,
  'parent.not_found': m.err_parent_not_found,
  'parent.not_terminal': m.err_parent_not_terminal,
  'lifecycle.read_only': m.err_lifecycle_read_only,
  'lifecycle.last_confirmed_source': m.err_lifecycle_last_confirmed_source,
  'project.not_found': m.err_project_not_found,
  'project.read_only': m.err_project_read_only,
  // spec 062 — project session membership.
  'project.session_already_pinned': m.err_project_session_already_pinned,
  'project.session_not_pinned': m.err_project_session_not_pinned,
  'project.lifecycle_disallows_session_add':
    m.err_project_lifecycle_disallows_session_add,
  'project.reclassification_revision_invalid':
    m.err_project_reclassification_revision_invalid,
  'project.membership_conflict': m.err_project_membership_conflict,
  // spec 062 — Update View (US3/US5).
  'project.update_view_no_additions': m.err_project_update_view_no_additions,
  'project.update_view_plan_not_found':
    m.err_project_update_view_plan_not_found,
  'project.update_view_plan_not_open': m.err_project_update_view_plan_not_open,
  'project.update_view_plan_not_approved':
    m.err_project_update_view_plan_not_approved,
  'project.update_view_plan_stale': m.err_project_update_view_plan_stale,
  'project.update_view_path_conflict': m.err_project_update_view_path_conflict,
  'project.update_view_source_unavailable':
    m.err_project_update_view_source_unavailable,
  'project.update_view_root_changed': m.err_project_update_view_root_changed,
  'project.update_view_plan_digest_mismatch':
    m.err_project_update_view_plan_digest_mismatch,
  'project.update_view_session_too_large':
    m.err_project_update_view_session_too_large,
  'project.update_view_operation_not_cancellable':
    m.err_project_update_view_operation_not_cancellable,
  'view.mixed_kind': m.err_view_mixed_kind,
  'view.not_found': m.err_view_not_found,
  'view.unsupported_kind': m.err_view_unsupported_kind,
  // spec 049 — source view generation.
  no_selection: m.err_no_selection,
  no_link_kind: m.err_no_link_kind,
  'destination.collision': m.err_destination_collision,
  'destination.exists': m.err_destination_exists,
  'profile.not_found': m.err_profile_not_found,
  // Same user-facing meaning as target.not_found — share one catalog key (no
  // duplicated catalog values; spec 046 FR-013).
  'canonical_target.not_found': m.err_target_not_found,
  'name.duplicate': m.err_name_duplicate,
  'name.empty': m.err_name_empty,
  'name.too_long': m.err_name_too_long,
  'source.already.linked': m.err_source_already_linked,
  'source.not_found': m.err_source_not_found,
  'source.invalid_organization_state': m.err_source_invalid_organization_state,
  'root.has_dependents': m.err_root_has_dependents,
  'remap.not_verified': m.err_remap_not_verified,
  'tool.locked': m.err_tool_locked,
  'tool.unknown': m.err_tool_unknown,
  'resolver.endpoint_invalid': m.err_resolver_endpoint_invalid,
  'key.unknown': m.err_key_unknown,
  'key.unoverridable': m.err_key_unoverridable,
  'value.invalid': m.err_value_invalid,
  'filesystem.destination_exists': m.err_filesystem_destination_exists,
  'transition.refused': m.err_transition_refused,
  'plan.required': m.err_plan_required,
  'alias.duplicate': m.err_alias_duplicate,
  'alias.blank': m.err_alias_blank,
  'alias.not_found': m.err_alias_not_found,
  'alias.not_removable': m.err_alias_not_removable,
  'target.not_found': m.err_target_not_found,
  'target.invalid_id': m.err_target_invalid_id,
  // Cone-search (spec 052 P3).
  'resolve.offline': m.err_resolve_offline,
  'frameset.not_found': m.err_frameset_not_found,
  'pointing.unavailable': m.err_pointing_unavailable,
  'candidate.invalid': m.err_candidate_invalid,
  'launch.failed': m.err_launch_failed,
  'macos.quarantine.detected': m.err_macos_quarantine_detected,
  'filters.invalid': m.err_filters_invalid,
  'os.command_failed': m.err_os_command_failed,
  'picker.unavailable': m.err_picker_unavailable,
  'format.unsupported': m.err_format_unsupported,
  'range.invalid': m.err_range_invalid,
  'path.write.denied': m.err_path_write_denied,
  'path.parent.missing': m.err_path_parent_missing,
  'database.error': m.err_database_error,
  'serialise.error': m.err_serialise_error,
  'io.error': m.err_io_error,
  // Per-frame inventory (spec 048).
  'root.unavailable': m.err_root_unavailable,
  'hash.mismatch': m.err_hash_mismatch,
  'frame.not_found': m.err_frame_not_found,
  // Framing layer (spec 008 Q27, F-Framing-3).
  'framing.not_found': m.err_framing_not_found,
  'framing.project_mismatch': m.err_framing_project_mismatch,
  'framing.merge.requires_two': m.err_framing_merge_requires_two,
  'framing.merge.duplicate_id': m.err_framing_merge_duplicate_id,
  'framing.split.empty_selection': m.err_framing_split_empty_selection,
  'framing.split.invalid_session': m.err_framing_split_invalid_session,
  'framing.split.would_empty_source': m.err_framing_split_would_empty_source,
  'framing.reassign.empty_selection': m.err_framing_reassign_empty_selection,
  // Inbox-confirm attribution (spec 008 Q27, F-Framing-5/10).
  'attribution.not_light_frame': m.err_attribution_not_light_frame,
  'attribution.geometry_unavailable': m.err_attribution_geometry_unavailable,
  // Onboarding redesign (spec 056 contracts/onboarding-commands.md).
  'onboarding.item.unknown': m.err_onboarding_item_unknown,
  'onboarding.invalid_state': m.err_onboarding_invalid_state,
  // The generic wrap code maps to the shared generic fallback message (no
  // duplicated catalog values; spec 046 FR-013).
  'internal.error': m.err_generic_fallback,
};

/** Safe generic fallback shown when a code has no mapping (FR-011, SC-005). */
export function errorFallback(): string {
  return m.err_generic_fallback();
}
