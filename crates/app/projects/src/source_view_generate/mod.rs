// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 049 US1: `sourceview.generate` — first-materialization of a project
//! source view as a reviewable `prepared_view_generation` plan.
//!
//! Companion to `crate::prepared_views` (spec 026 remove/regenerate), which
//! this module reuses unchanged: the `PreparedSourceView` /
//! `PreparedSourceViewItem` entities, the project-lifecycle gate, and the
//! spec 017/025 plan review→approve→apply pipeline. The first-materialization
//! DB write itself happens on successful apply
//! (`app_core::plan_apply::finalize_view_generation`), not here — this use
//! case only builds and persists the reviewable plan (FR-001).
//!
//! ## Scope (US1 MVP superseded by US2 profile-driven layout)
//!
//! - Selection is **session-level** (spec 048 per-frame selection is a
//!   separate follow-up, CL-9): every project-linked `acquisition_session`
//!   (`project_sources`) contributes all of its **present** frames.
//! - Layout is **profile-driven** (spec 049 US2 T025/T026): the active
//!   project's tool profile (`req.profile_id`, else `projects.tool`, resolved
//!   against `workflow_profiles::seed`) supplies a
//!   [`workflow_profiles::SourceViewLayout`] — a `{token}` directory pattern
//!   for lights (WBPP default: `{date}/{filter}/{exposure}/`, i.e.
//!   session/night → filter → exposure) resolved via
//!   [`patterns::resolve_pattern_str`] against the shared v1 token registry,
//!   plus a calibration-location pattern. Every matched calibration set still
//!   gets its own subdirectory beneath that location (keyed by `master_id`)
//!   so collisions stay impossible by construction (FR-009a/CL-5) without
//!   needing a `master_id` metadata token in the shared registry.
//! - Calibration selection (T027): `calibration_assignment.master_id` always
//!   resolves to a `calibration_session` row. This codebase's calibration
//!   matching engine (`calibration_core::MasterInfo`) has no raw-vs-master
//!   branch at that level — the raw/master distinction (spec 040 `is_master`)
//!   is resolved earlier, during inbox confirm, onto
//!   `inbox_classification_evidence`, and is not carried onto
//!   `calibration_session`/`calibration_assignment`. So "masters when the
//!   match resolved masters, else the matched raw calibration sets" (FR-010/
//!   CL-4) already holds trivially today: there is exactly one resolved frame
//!   set per assignment, and it is linked as-is. If a future schema change
//!   lets one assignment carry both a master and a raw fallback, this
//!   function's calibration loop is the place to add masters-preferred
//!   branching.
//!
//! Split by responsibility (refactor sweep #983): `generate` owns the
//! `generate_source_view` pipeline; `destination_override` is the T041
//! per-project destination override KV read/write; the pure path/layout
//! helpers below are shared by both.

use camino::{Utf8Path, Utf8PathBuf};

mod destination_override;
mod generate;

#[cfg(test)]
mod tests;

pub use destination_override::{get_destination_override, set_destination_override};
pub use generate::generate_source_view;

/// Map a pattern-resolution failure to a blocking `ContractError` (spec 049
/// US2 T026). Layout patterns are fixed per profile, so these are only
/// reachable via pathological metadata values (e.g. a filter/exposure
/// snapshot containing `..`) — treat them the same as the other filesystem
/// safety refusals in this module: refuse and point at the offending pattern
/// input, never silently truncate or substitute.
fn layout_resolve_err(
    e: &patterns::ResolveError,
    dest_hint: &str,
) -> contracts_core::ContractError {
    use contracts_core::error_code::ErrorCode;
    use patterns::ResolveError;

    let code = match e {
        ResolveError::PathTraversal { .. } => ErrorCode::PathTraversal,
        ResolveError::ReservedName { .. } => ErrorCode::PathReservedName,
        ResolveError::Empty
        | ResolveError::UnknownToken { .. }
        | ResolveError::UnicodeConfusable { .. }
        | ResolveError::PathTooLong { .. }
        | ResolveError::EmptySegment { .. } => ErrorCode::PathInvalid,
    };
    contracts_core::ContractError::new(
        code,
        format!("could not resolve source-view layout for '{dest_hint}': {e}"),
        contracts_core::ErrorSeverity::Blocking,
        false,
    )
}

/// Extract the observing-night component (`YYYY-MM-DD`) from a `session_key`
/// (spec 002 T033a format: `target|filter|binning|gain|night`). Falls back to
/// the whole key when it does not contain the expected separator (e.g. test
/// fixtures using a bare id as the key) — the `{date}` token's own fallback
/// (`"undated"`) only applies to an *absent* metadata field, not a malformed
/// one, so this never fails generation.
fn session_night(session_key: &str) -> String {
    session_key.rsplit('|').next().unwrap_or(session_key).to_owned()
}

// ── Row helpers (ad hoc queries — mirrors the pragmatic per-item query style
// already used in `prepared_views::regenerate_prepared_view`) ────────────────

struct FrameRow {
    id: String,
    relative_path: String,
    state: String,
}

/// Resolve `file_record` rows for a set of ids.
///
/// The session-level `root_id` (captured once from the owning
/// `acquisition_session`/`calibration_session` row) is used for every frame
/// in that session rather than re-reading `file_record.root_id` per row —
/// sessions are single-root by construction in this codebase (see
/// `inventory::SessionProjectionRow`).
async fn frames_for_ids(pool: &sqlx::SqlitePool, ids: &[String]) -> Vec<FrameRow> {
    use persistence_plans::repositories::q_projects;

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Ok(Some((relative_path, state))) =
            q_projects::get_file_record_path_and_state(pool, id).await
        {
            out.push(FrameRow { id: id.clone(), relative_path, state });
        }
        // Missing rows are silently absent here — the caller treats an id
        // with no resolved frame as unresolved (FR-019).
    }
    out
}

fn parse_frame_ids(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Append `segment` to `base` with a `/` separator, regardless of platform
/// path-separator conventions.
///
/// `Utf8PathBuf::join` inserts the platform-native separator (`\` on
/// Windows), which mixes with the forward-slash convention documented and
/// used by `resolve_pattern_str` (`crates/patterns`). Building destinations by
/// chaining `.join()` calls on Windows therefore produces paths with both
/// `\` and `/` in them (e.g. `foo\bar/baz\qux.fits`) — cosmetically ugly, but
/// also non-deterministic for anything that persists or compares
/// `to_relative_path`/`name` (spec 049's plan items). Windows path APIs
/// accept `/` as a separator natively, so joining with `/` unconditionally is
/// safe on every supported platform and keeps generated destinations
/// portable, matching the "Portable Contracts and Durable Records"
/// constitution principle.
fn join_portable(base: &Utf8Path, segment: &str) -> Utf8PathBuf {
    if base.as_str().is_empty() {
        Utf8PathBuf::from(segment)
    } else {
        Utf8PathBuf::from(format!("{base}/{segment}"))
    }
}

/// T042/FR-018: the classic Windows `MAX_PATH` limit (260 characters,
/// including the drive/UNC prefix and the trailing NUL the Win32 APIs count
/// against — so the actual usable path length is 259; a 260-character path
/// has no room left for the NUL and already exceeds the limit). Extracted as
/// a pure function (not gated on `cfg!(windows)`) so the length-threshold
/// logic itself is unit-testable on every host platform — only the
/// *emission* of the warning is Windows-only (macOS/Linux filesystems don't
/// share this constraint).
fn exceeds_windows_long_path_limit(path: &str) -> bool {
    path.len() >= 260
}
