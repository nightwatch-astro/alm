// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// The tauri-specta `Builder` / `invoke_handler` command surface.
//
// Isolated from the rest of the composition root (issue #981) because this
// is the single place the full typed-command list is enumerated, and the
// `dev-tools` feature gating here is load-bearing: release binaries MUST
// compile the non-`dev-tools` variant only.
//
// `include!`d from `lib.rs` rather than declared `mod bootstrap::specta;`
// (a plain comment, not `//!`, because this is no longer a module — see
// below). `#[tauri::command]` (on every command below, all `pub fn`)
// generates hidden `__cmd__*`/`__tauri_command_name_*` helper macros via
// `#[macro_export]`, which per macro_rules scoping rules are auto-visible,
// *unqualified*, only at the compiling crate's root textual scope — not
// inside an ordinary `mod`. `collect_commands!`/`tauri::generate_handler!`
// invoke those hidden macros by bare name, so a real module boundary here
// makes every one of the ~185 commands fail to compile with "cannot find
// macro". `include!` splices this file's tokens into `lib.rs`'s crate-root
// module instead, keeping the macro scope intact while still separating
// the file on disk.
//
// Consequence: `cargo fmt` discovers files by walking `mod` declarations, so
// it never reaches this one. `just lint` and the CI "Rust format" step check
// it with an explicit `rustfmt` invocation — keep those in sync if this file
// is renamed or moved.

use tauri_specta::{collect_commands, Builder};

use crate::commands::artifacts::{
    artifact_classify, artifact_list, artifact_mark_resolved, artifact_watcher_attach,
    artifact_watcher_detach, artifact_watcher_refresh,
};
use crate::commands::audit::{audit_export, audit_list, entity_names};
use crate::commands::calibration::{
    calibration_masters_archive_plan_generate, calibration_masters_archive_plan_generate_restore,
    calibration_masters_get, calibration_masters_list, calibration_match_assign,
    calibration_match_suggest, calibration_match_suggest_batch, calibration_match_unassign,
    calibration_matches,
};
use crate::commands::calibration_tolerances::{
    calibration_tolerances_get, calibration_tolerances_update,
};
use crate::commands::cleanup::{
    cleanup_plan_generate, cleanup_policy_get, cleanup_policy_update, cleanup_raw_frames_generate,
    cleanup_raw_frames_scan, cleanup_scan,
};
#[cfg(feature = "dev-tools")]
use crate::commands::dev::{
    dev_calls_list, dev_calls_push, dev_contracts_list, dev_export, dev_schema_get,
};
use crate::commands::equipment::{
    equipment_cameras_create, equipment_cameras_delete, equipment_cameras_list,
    equipment_cameras_update, equipment_filters_create, equipment_filters_delete,
    equipment_filters_list, equipment_filters_update, equipment_telescopes_create,
    equipment_telescopes_delete, equipment_telescopes_list, equipment_telescopes_update,
    equipment_trains_create, equipment_trains_delete, equipment_trains_list,
    equipment_trains_update,
};
use crate::commands::firstrun::{
    firstrun_complete, firstrun_restart, firstrun_state, roots_register_batch,
};
use crate::commands::inbox::{
    inbox_attribution_suggest, inbox_classify, inbox_classify_source_group, inbox_confirm,
    inbox_item_metadata, inbox_list, inbox_plan, inbox_plan_apply, inbox_plan_apply_all,
    inbox_plan_apply_selected, inbox_plan_cancel, inbox_plan_list_open, inbox_property_registry,
    inbox_reclassify, inbox_reclassify_v2, inbox_scan, inbox_scan_folder, inbox_stats,
    inbox_target_recommendations,
};
use crate::commands::ingestion::{ingestion_settings_get, ingestion_settings_update};
use crate::commands::inventory::{inventory_list, inventory_session_notes_update};
use crate::commands::inventory_frame::{
    inventory_frame_list, inventory_frame_relink, inventory_reconcile_run,
    inventory_root_config_get, inventory_root_config_set,
};
use crate::commands::lifecycle::{
    lifecycle_ledger_list, lifecycle_transition_apply, lifecycle_transition_preview,
    provenance_read,
};
use crate::commands::log::{log_export, log_recent};
use crate::commands::manifests::{
    manifest_get, manifest_list, manifest_reveal_in_os, note_get, note_update,
};
use crate::commands::native::{native_directory_pick, native_file_pick, native_reveal};
use crate::commands::onboarding::{
    onboarding_item_set_state, onboarding_orientation_complete, onboarding_restore,
    onboarding_section_set, onboarding_state_get,
};
use crate::commands::patterns::{
    pattern_path_preview, pattern_preview, pattern_resolve, pattern_validate,
};
use crate::commands::plan_apply::{
    plans_apply_direct, plans_apply_real, plans_apply_status, plans_cancel,
    plans_confirm_destructive, plans_item_retry, plans_item_skip, plans_resume,
};
use crate::commands::plans::{
    archive_list, archive_permanently_delete, archive_plan_generate, archive_plan_generate_restore,
    archive_send_to_trash, plans_approve, plans_discard, plans_free_space_estimate, plans_get,
    plans_list, plans_retry,
};
use crate::commands::preferences::{preferences_get, preferences_set};
use crate::commands::prepared_views::{
    preparedview_list, preparedview_regenerate, preparedview_remove, sourceview_destination_get,
    sourceview_destination_set, sourceview_generate, sourceview_verify,
};
use crate::commands::projects::{
    projects_channels_dismiss_drift, projects_channels_reinfer, projects_create,
    projects_create_plan, projects_framing_list, projects_framing_merge, projects_framing_reassign,
    projects_framing_split, projects_get, projects_list, projects_source_add,
    projects_source_remove, projects_update,
};
use crate::commands::protection::{
    plan_protection_check_cmd, protection_plan_acknowledged, source_protection_get,
    source_protection_set,
};
use crate::commands::review::review_queue;
use crate::commands::roots::{
    equipment_list, roots_delete, roots_list, roots_register, roots_remap, roots_remap_apply,
    scan_start, sources_set_active, sources_set_organization_state,
};
use crate::commands::search::search_global;
use crate::commands::sessions::{
    sessions_calendar, sessions_get, sessions_list, sessions_merge, sessions_split,
};
use crate::commands::settings::{
    settings_get, settings_overridable_keys, settings_restore_defaults,
    settings_source_override_set, settings_update,
};
use crate::commands::status::status_summary;
use crate::commands::target_favourites::{
    target_favourites_add, target_favourites_list, target_favourites_remove,
};
use crate::commands::target_lookup::{
    target_adopt, target_astro_format_batch, target_cache_clear, target_cone_search_confirm,
    target_cone_search_suggest, target_moon_opposition_batch, target_resolution_settings,
    target_resolution_settings_update, target_resolve, target_resolve_explicit, target_search,
};
use crate::commands::target_management as target_mgmt_cmds;
use crate::commands::tools::{
    tools_discover, tools_launch, tools_list, tools_update, tools_validate_path,
};

/// Shared base for specta builder — chain common config and all production
/// commands.  Returns the builder before any feature-gated commands are added.
///
/// # Panics / Design Note
/// `collect_commands!` does not accept `cfg` attributes inside its token list,
/// so feature-gated commands must be added in a *separate* `.commands()` call
/// on a different `Builder` value.  Because `.commands()` **replaces** the
/// command set, we handle this by having two cfg-gated public `specta_builder`
/// functions that each call `.commands()` exactly once with the full command
/// list for that build variant.
fn base_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        // Several contract DTOs carry `serde_json::Value` "details" payloads;
        // their `Number` inner type erases width info, so specta would block
        // export by default. Casting BigInts to JS `number` is acceptable for
        // these debug payloads (no numeric precision is required here).
        .dangerously_cast_bigints_to_number()
        // Register `serde_json::Value` as a named (recursive) type so specta
        // emits it once instead of inlining its self-referential shape, which
        // would otherwise fail with "infinitely recursive inline reference".
        .typ::<serde_json::Value>()
        // Spec 042 T011 — ErrorCode enum scaffold. Registered here so the
        // TypeScript union is emitted immediately without waiting for the
        // ContractError.code type change (US2).
        .typ::<contracts_core::error_code::ErrorCode>()
        // Spec 035 — SIMBAD target resolution DTOs (T007). These are pure
        // contract types whose commands land in later tasks (US1–5); register
        // them explicitly so the TypeScript surface exists ahead of the
        // commands that will reference them. Request/response roots pull in all
        // nested structs and enums transitively.
        .typ::<contracts_core::targets::TargetSearchRequest>()
        .typ::<contracts_core::targets::TargetSearchResponse>()
        .typ::<contracts_core::targets::TargetResolveSimbadRequest>()
        .typ::<contracts_core::targets::TargetResolveSimbadResponse>()
        .typ::<contracts_core::targets::ResolverSettingsGetRequest>()
        .typ::<contracts_core::targets::ResolverSettingsUpdateRequest>()
        .typ::<contracts_core::targets::ResolverSettingsResponse>()
}

/// Build the tauri-specta [`Builder`] populated with every typed command.
///
/// Reused by `run` (production) and `tests/bindings.rs` (TS emission).
///
/// When the `dev-tools` feature is enabled this function includes the
/// `dev.contracts.list`, `dev.calls.list`, and `dev.export` commands.
/// Without the feature those commands are absent from the binary entirely.
#[must_use]
#[allow(clippy::too_many_lines)]
#[cfg(not(feature = "dev-tools"))]
pub fn specta_builder() -> Builder<tauri::Wry> {
    base_builder().commands(collect_commands![
        // lifecycle (spec 002)
        provenance_read,
        lifecycle_transition_apply,
        lifecycle_transition_preview,
        lifecycle_ledger_list,
        // sessions
        sessions_list,
        sessions_get,
        sessions_calendar,
        sessions_split,
        sessions_merge,
        // calibration (spec 029 stubs)
        calibration_masters_list,
        calibration_masters_get,
        calibration_matches,
        // calibration matching (spec 007)
        calibration_match_suggest,
        calibration_match_assign,
        calibration_match_suggest_batch,
        calibration_match_unassign,
        // calibration master archive (#886)
        calibration_masters_archive_plan_generate,
        calibration_masters_archive_plan_generate_restore,
        // target management (spec 036 — gen-3, canonical_target model)
        target_mgmt_cmds::target_get,
        target_mgmt_cmds::target_list,
        target_mgmt_cmds::target_alias_add,
        target_mgmt_cmds::target_alias_remove,
        target_mgmt_cmds::target_display_alias_set,
        target_mgmt_cmds::target_display_alias_clear,
        // target history + notes (spec 023 US2/US3/US4)
        target_mgmt_cmds::target_sessions_list,
        target_mgmt_cmds::target_projects_list,
        target_mgmt_cmds::target_note_get,
        target_mgmt_cmds::target_note_update,
        // target favourites (spec 051 US2)
        target_favourites_list,
        target_favourites_add,
        target_favourites_remove,
        // target resolve (spec 035 — SIMBAD cache-first resolution)
        target_resolve,
        // target resolve — explicit entrypoint, TAP-first/Sesame-fallback (spec 052 P2)
        target_resolve_explicit,
        // target search (spec 035, US1)
        target_search,
        // target in-use promotion + resolve-cache clear (spec 052 P1)
        target_adopt,
        target_cache_clear,
        // cone-search suggestion at Inbox ingest (spec 052 P3)
        target_cone_search_suggest,
        target_cone_search_confirm,
        // resolver settings (spec 035, US5)
        target_resolution_settings,
        target_resolution_settings_update,
        // sexagesimal RA/Dec formatting (adopt target-match)
        target_astro_format_batch,
        // moon separation + opposition batch (#634)
        target_moon_opposition_batch,
        // projects (spec 008)
        projects_list,
        projects_get,
        projects_create,
        projects_update,
        projects_source_add,
        projects_source_remove,
        projects_channels_reinfer,
        projects_channels_dismiss_drift,
        projects_framing_list,
        projects_framing_merge,
        projects_framing_split,
        projects_framing_reassign,
        projects_create_plan,
        // plans (spec 017)
        plans_list,
        plans_get,
        plans_free_space_estimate,
        plans_approve,
        plans_discard,
        plans_retry,
        archive_send_to_trash,
        archive_permanently_delete,
        archive_list,
        archive_plan_generate,
        archive_plan_generate_restore,
        // plan apply (spec 025)
        plans_apply_real,
        // channel-free plan apply variant (spec 037)
        plans_apply_direct,
        plans_cancel,
        plans_resume,
        plans_item_skip,
        plans_item_retry,
        plans_apply_status,
        plans_confirm_destructive,
        // audit
        audit_list,
        audit_export,
        entity_names,
        // log stream (spec 019)
        log_recent,
        log_export,
        // review
        review_queue,
        // roots & scan & equipment
        roots_list,
        roots_register,
        roots_register_batch,
        roots_remap,
        roots_remap_apply,
        roots_delete,
        scan_start,
        equipment_list,
        sources_set_organization_state,
        sources_set_active,
        // first-run wizard (spec 003)
        firstrun_state,
        firstrun_complete,
        firstrun_restart,
        // pattern resolver (spec 015)
        pattern_validate,
        pattern_resolve,
        pattern_preview,
        pattern_path_preview,
        // source protection (spec 016 US2–US4)
        source_protection_get,
        source_protection_set,
        plan_protection_check_cmd,
        protection_plan_acknowledged,
        // settings (spec 018)
        settings_get,
        settings_update,
        settings_restore_defaults,
        settings_source_override_set,
        settings_overridable_keys,
        // preferences
        preferences_get,
        preferences_set,
        // search
        search_global,
        // onboarding (spec 056)
        onboarding_state_get,
        onboarding_item_set_state,
        onboarding_orientation_complete,
        onboarding_section_set,
        onboarding_restore,
        // native filesystem controls (spec 004)
        native_directory_pick,
        native_file_pick,
        native_reveal,
        // equipment CRUD (spec 030)
        equipment_cameras_list,
        equipment_cameras_create,
        equipment_cameras_update,
        equipment_cameras_delete,
        equipment_telescopes_list,
        equipment_telescopes_create,
        equipment_telescopes_update,
        equipment_telescopes_delete,
        equipment_trains_list,
        equipment_trains_create,
        equipment_trains_update,
        equipment_trains_delete,
        equipment_filters_list,
        equipment_filters_create,
        equipment_filters_update,
        equipment_filters_delete,
        // status (spec 030)
        status_summary,
        // cleanup policy & scan (spec 030)
        cleanup_policy_get,
        cleanup_policy_update,
        cleanup_scan,
        cleanup_plan_generate,
        cleanup_raw_frames_scan,
        cleanup_raw_frames_generate,
        // calibration tolerances (spec 030)
        calibration_tolerances_get,
        calibration_tolerances_update,
        // inbox (spec 005 + 030 + 039 + 041)
        inbox_scan,
        inbox_scan_folder,
        inbox_classify,
        inbox_classify_source_group,
        inbox_confirm,
        inbox_reclassify,
        inbox_reclassify_v2,
        inbox_item_metadata,
        inbox_list,
        inbox_plan,
        inbox_plan_apply,
        inbox_plan_apply_all,
        inbox_plan_apply_selected,
        inbox_plan_cancel,
        inbox_stats,
        inbox_plan_list_open,
        inbox_property_registry,
        inbox_target_recommendations,
        inbox_attribution_suggest,
        // inventory (spec 006)
        inventory_list,
        inventory_session_notes_update,
        // per-frame inventory (spec 048)
        inventory_frame_list,
        inventory_reconcile_run,
        inventory_frame_relink,
        inventory_root_config_get,
        inventory_root_config_set,
        // ingestion settings (spec 030)
        ingestion_settings_get,
        ingestion_settings_update,
        // tools (spec 011/030)
        tools_launch,
        tools_list,
        tools_update,
        tools_validate_path,
        tools_discover,
        // artifacts (spec 012)
        artifact_list,
        artifact_classify,
        artifact_mark_resolved,
        artifact_watcher_attach,
        artifact_watcher_detach,
        artifact_watcher_refresh,
        // manifests + notes (spec 024)
        manifest_list,
        manifest_get,
        note_get,
        note_update,
        manifest_reveal_in_os,
        // prepared source views (spec 026)
        preparedview_list,
        preparedview_remove,
        preparedview_regenerate,
        // source view generation (spec 049)
        sourceview_generate,
        sourceview_verify,
        sourceview_destination_get,
        sourceview_destination_set,
    ])
}

/// `dev-tools` variant: identical to the production builder plus the three
/// developer-diagnostics commands (spec 021).
///
/// Release binaries MUST NOT be compiled with the `dev-tools` feature.
#[must_use]
#[allow(clippy::too_many_lines)]
#[cfg(feature = "dev-tools")]
pub fn specta_builder() -> Builder<tauri::Wry> {
    base_builder().commands(collect_commands![
        // lifecycle (spec 002)
        provenance_read,
        lifecycle_transition_apply,
        lifecycle_transition_preview,
        lifecycle_ledger_list,
        // sessions
        sessions_list,
        sessions_get,
        sessions_calendar,
        sessions_split,
        sessions_merge,
        // calibration (spec 029 stubs)
        calibration_masters_list,
        calibration_masters_get,
        calibration_matches,
        // calibration matching (spec 007)
        calibration_match_suggest,
        calibration_match_assign,
        calibration_match_suggest_batch,
        calibration_match_unassign,
        // calibration master archive (#886)
        calibration_masters_archive_plan_generate,
        calibration_masters_archive_plan_generate_restore,
        // target management (spec 036 — gen-3, canonical_target model)
        target_mgmt_cmds::target_get,
        target_mgmt_cmds::target_list,
        target_mgmt_cmds::target_alias_add,
        target_mgmt_cmds::target_alias_remove,
        target_mgmt_cmds::target_display_alias_set,
        target_mgmt_cmds::target_display_alias_clear,
        // target history + notes (spec 023 US2/US3/US4)
        target_mgmt_cmds::target_sessions_list,
        target_mgmt_cmds::target_projects_list,
        target_mgmt_cmds::target_note_get,
        target_mgmt_cmds::target_note_update,
        // target favourites (spec 051 US2)
        target_favourites_list,
        target_favourites_add,
        target_favourites_remove,
        // target resolve (spec 035 — SIMBAD cache-first resolution)
        target_resolve,
        // target resolve — explicit entrypoint, TAP-first/Sesame-fallback (spec 052 P2)
        target_resolve_explicit,
        // target search (spec 035, US1)
        target_search,
        // target in-use promotion + resolve-cache clear (spec 052 P1)
        target_adopt,
        target_cache_clear,
        // cone-search suggestion at Inbox ingest (spec 052 P3)
        target_cone_search_suggest,
        target_cone_search_confirm,
        // resolver settings (spec 035, US5)
        target_resolution_settings,
        target_resolution_settings_update,
        // sexagesimal RA/Dec formatting (adopt target-match)
        target_astro_format_batch,
        // moon separation + opposition batch (#634)
        target_moon_opposition_batch,
        // projects (spec 008)
        projects_list,
        projects_get,
        projects_create,
        projects_update,
        projects_source_add,
        projects_source_remove,
        projects_channels_reinfer,
        projects_channels_dismiss_drift,
        projects_framing_list,
        projects_framing_merge,
        projects_framing_split,
        projects_framing_reassign,
        projects_create_plan,
        // plans (spec 017)
        plans_list,
        plans_get,
        plans_free_space_estimate,
        plans_approve,
        plans_discard,
        plans_retry,
        archive_send_to_trash,
        archive_permanently_delete,
        archive_list,
        archive_plan_generate,
        archive_plan_generate_restore,
        // plan apply (spec 025)
        plans_apply_real,
        // channel-free plan apply variant (spec 037)
        plans_apply_direct,
        plans_cancel,
        plans_resume,
        plans_item_skip,
        plans_item_retry,
        plans_apply_status,
        plans_confirm_destructive,
        // audit
        audit_list,
        audit_export,
        entity_names,
        // log stream (spec 019)
        log_recent,
        log_export,
        // review
        review_queue,
        // roots & scan & equipment
        roots_list,
        roots_register,
        roots_register_batch,
        roots_remap,
        roots_remap_apply,
        roots_delete,
        scan_start,
        equipment_list,
        sources_set_organization_state,
        sources_set_active,
        // first-run wizard (spec 003)
        firstrun_state,
        firstrun_complete,
        firstrun_restart,
        // pattern resolver (spec 015)
        pattern_validate,
        pattern_resolve,
        pattern_preview,
        pattern_path_preview,
        // source protection (spec 016 US2–US4)
        source_protection_get,
        source_protection_set,
        plan_protection_check_cmd,
        protection_plan_acknowledged,
        // settings (spec 018)
        settings_get,
        settings_update,
        settings_restore_defaults,
        settings_source_override_set,
        settings_overridable_keys,
        // preferences
        preferences_get,
        preferences_set,
        // search
        search_global,
        // onboarding (spec 056)
        onboarding_state_get,
        onboarding_item_set_state,
        onboarding_orientation_complete,
        onboarding_section_set,
        onboarding_restore,
        // native filesystem controls (spec 004)
        native_directory_pick,
        native_file_pick,
        native_reveal,
        // equipment CRUD (spec 030)
        equipment_cameras_list,
        equipment_cameras_create,
        equipment_cameras_update,
        equipment_cameras_delete,
        equipment_telescopes_list,
        equipment_telescopes_create,
        equipment_telescopes_update,
        equipment_telescopes_delete,
        equipment_trains_list,
        equipment_trains_create,
        equipment_trains_update,
        equipment_trains_delete,
        equipment_filters_list,
        equipment_filters_create,
        equipment_filters_update,
        equipment_filters_delete,
        // status (spec 030)
        status_summary,
        // cleanup policy & scan (spec 030)
        cleanup_policy_get,
        cleanup_policy_update,
        cleanup_scan,
        cleanup_plan_generate,
        cleanup_raw_frames_scan,
        cleanup_raw_frames_generate,
        // calibration tolerances (spec 030)
        calibration_tolerances_get,
        calibration_tolerances_update,
        // inbox (spec 005 + 030 + 039 + 041)
        inbox_scan,
        inbox_scan_folder,
        inbox_classify,
        inbox_classify_source_group,
        inbox_confirm,
        inbox_reclassify,
        inbox_reclassify_v2,
        inbox_item_metadata,
        inbox_list,
        inbox_plan,
        inbox_plan_apply,
        inbox_plan_apply_all,
        inbox_plan_apply_selected,
        inbox_plan_cancel,
        inbox_stats,
        inbox_plan_list_open,
        inbox_property_registry,
        inbox_target_recommendations,
        inbox_attribution_suggest,
        // inventory (spec 006)
        inventory_list,
        inventory_session_notes_update,
        // per-frame inventory (spec 048)
        inventory_frame_list,
        inventory_reconcile_run,
        inventory_frame_relink,
        inventory_root_config_get,
        inventory_root_config_set,
        // ingestion settings (spec 030)
        ingestion_settings_get,
        ingestion_settings_update,
        // tools (spec 011/030)
        tools_launch,
        tools_list,
        tools_update,
        tools_validate_path,
        tools_discover,
        // artifacts (spec 012)
        artifact_list,
        artifact_classify,
        artifact_mark_resolved,
        artifact_watcher_attach,
        artifact_watcher_detach,
        artifact_watcher_refresh,
        // manifests + notes (spec 024)
        manifest_list,
        manifest_get,
        note_get,
        note_update,
        manifest_reveal_in_os,
        // prepared source views (spec 026)
        preparedview_list,
        preparedview_remove,
        preparedview_regenerate,
        // source view generation (spec 049)
        sourceview_generate,
        sourceview_verify,
        sourceview_destination_get,
        sourceview_destination_set,
        // developer diagnostics (spec 021) — dev-tools build only
        dev_contracts_list,
        dev_calls_list,
        dev_calls_push,
        dev_export,
        dev_schema_get,
    ])
}
