// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Project create/update/source/channel use cases (spec 008 F-3).
//!
//! Entry points:
//! - [`create`]          — validate, persist, infer channels, emit audit.
//! - [`update`]          — patch name/tool/notes; enforce tool-lock + read-only.
//! - [`add_source`]      — link an Inventory session; snapshot fields; recompute channels.
//! - [`remove_source`]   — unlink a source; gate on lifecycle; re-check ready trigger.
//! - [`reinfer_channels`]  — recompute channels from scratch; reset drift.
//! - [`dismiss_drift`]     — reset channel drift flag without changing channels.
//!
//! Auto-transition seam (spec 009): after `create`, `add_source`, and `update`
//! this module checks whether `setup_incomplete → ready` should fire (tool != null
//! AND ≥1 source linked). When the condition is met it directly updates the
//! lifecycle column via the repository. Spec 009 will own the full lifecycle
//! service; the auto-transition here is the thin seam described in tasks.md F-3
//! (R-Ready-Trigger). The `new_lifecycle` field in the response is the signal
//! the UI should surface.
//!
//! Constitution II: `create` generates a reviewable `FilesystemPlan` via
//! `domain_core::lifecycle::plan` + `crates/persistence/db::repositories::plans`.
//! The plan contains one `mkdir` item per folder required by the project's tool
//! (from `crates/project/structure::required_folders`) plus a `write_manifest`
//! item for the project marker.  The plan is returned as `plan_id` in the
//! response; the caller drives approval + application via specs 017/025.
//!
//! Constitution V: SQLite is the durable record; audit events are emitted via
//! the `EventBus` for every mutation.
//!
//! Split by responsibility (refactor sweep #972): [`create`] owns path
//! anchoring + the Constitution II folder plan builder; [`update`] is
//! metadata-only; `sources` links/unlinks Inventory sessions and recomputes
//! channels; `channels` handles the reinfer/dismiss-drift pair; `read`
//! covers the `list`/`get` DTO projections. Helpers shared by more than one
//! use case (error mapping, exposure/channel aggregation, the auto-transition
//! seam, the source-change manifest trigger) stay here so siblings pull them
//! via `super::`.

use contracts_core::projects_v2::ProjectChannelDto;
use contracts_core::{error_code::ErrorCode, ContractError, FieldError};
use domain_core::project::channels::{infer_channels, Channel};
use persistence_core::repositories::q_core;
use persistence_plans::repositories::projects as repo;
use sqlx::SqlitePool;

use crate::project_health;

mod channels;
mod create;
mod read;
mod sources;
mod update;

#[cfg(test)]
mod tests;

pub use channels::{dismiss_drift, reinfer_channels};
pub use create::create;
pub use read::{get, list};
pub use sources::{add_source, remove_source};
pub use update::update;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Map a static `&str` validation error code (returned by `domain_core::project::validate`)
/// to the corresponding `ErrorCode` variant.
fn str_to_error_code(code: &str) -> ErrorCode {
    match code {
        "name.empty" => ErrorCode::NameEmpty,
        "name.too_long" => ErrorCode::NameTooLong,
        "tool.unknown" => ErrorCode::ToolUnknown,
        _ => {
            tracing::warn!(code, "unknown validation code; using InternalData");
            ErrorCode::InternalData
        }
    }
}

/// Build a `FieldError` naming the specific request field a validation
/// failure belongs to (bd `astro-plan-qnj0`).
///
/// `code` is derived from `error_code`'s own wire string via serde rather
/// than a second hardcoded literal, so it cannot drift from the
/// `#[serde(rename = "...")]` on the `ErrorCode` variant.
fn field_error(field: &str, error_code: ErrorCode, message: impl Into<String>) -> FieldError {
    let code = serde_json::to_value(error_code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    FieldError { field: field.to_owned(), code, message: message.into() }
}

// Domain-scoped DB error mapper: routes NotFound to project.not_found code.
fn db_err(e: persistence_core::DbError) -> ContractError {
    app_core_errors::db_err_with_not_found(ErrorCode::ProjectNotFound)(e)
}

/// A project source's snapshot of its inventory session, taken at link time
/// (#1218). Empty strings / `0` mean "the session does not carry this value",
/// which is what every field was hardcoded to before this was wired.
#[derive(Debug, Default)]
pub(super) struct SourceSnapshot {
    pub name: String,
    pub frames: i64,
    pub filter: String,
    pub exposure: String,
}

/// Read the snapshot fields for one inventory (acquisition) session.
///
/// - `filter` comes from the session key, whose format `crates/sessions`
///   owns (`sessions::SessionKey::parse`); it is the same value the Sessions
///   surfaces show.
/// - `frames` is the ACTIVE (non-`missing`) frame count, matching
///   `app_core::sessions`' honest counts rather than the raw `frame_ids` length.
/// - `exposure` is a PER-SUB token (`"300s"`), never total integration time —
///   `parse_exposure_seconds` and the source-view `{exposure}` path token both
///   read it that way. Exposure is not part of the session key, so a session
///   may hold several distinct per-sub exposures; only a session with exactly
///   ONE distinct value gets a token. With zero (no metadata) or several, no
///   scalar is truthful, so the field stays empty and the pattern registry's
///   documented `unknown-exposure` fallback applies rather than inventing a
///   directory name that reads like a real exposure.
///
/// An unknown session id yields an all-empty snapshot: source linking is
/// best-effort and must never fail project creation.
pub(super) async fn source_snapshot(
    pool: &SqlitePool,
    inventory_session_id: &str,
) -> Result<SourceSnapshot, ContractError> {
    let Some(row) = q_core::get_session_joined(pool, inventory_session_id).await.map_err(db_err)?
    else {
        return Ok(SourceSnapshot::default());
    };

    let key = sessions::SessionKey::parse(&row.session_key);
    let target = row.canonical_target_name.filter(|n| !n.is_empty()).or(key.target);
    let filter = key.filter.unwrap_or_default();

    let frame_ids: Vec<String> = serde_json::from_str(&row.frame_ids).unwrap_or_default();
    let (frames, _bytes) = q_core::active_frame_summary(pool, &frame_ids).await.map_err(db_err)?;
    let exposures = q_core::active_frame_exposures(pool, &frame_ids).await.map_err(db_err)?;
    let exposure = match exposures.as_slice() {
        [only] => persistence_inbox::repositories::inbox::format_exposure_label(*only),
        _ => String::new(),
    };

    // Mirrors `app_core::inventory::derive_session_name`, so a linked source
    // shows the same label the Sessions/Inventory surfaces show.
    let name = target.map_or_else(String::new, |t| {
        let filter_part = if filter.is_empty() { "?" } else { filter.as_str() };
        let night = key.night.unwrap_or_default();
        format!("{t} · {filter_part} — {night}")
    });

    Ok(SourceSnapshot { name, frames, filter, exposure })
}

/// Parse an `exposure_snapshot` string (e.g. `"300s"`, `"1.5s"`) into whole
/// seconds (D5: parse at read time; the write path / stored format is
/// unchanged — see `persistence_inbox::repositories::inbox::format_exposure_label`
/// for the writer this mirrors).
///
/// Never panics: missing, empty, or unparseable values (e.g. `"Mixed"` from
/// a multi-value grouping bucket, or a bare `"na"`) degrade to `0` with a
/// debug-level log line rather than surfacing an error. Fractional seconds
/// are truncated toward zero.
fn parse_exposure_seconds(exposure: &str) -> u64 {
    let trimmed = exposure.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let numeric = trimmed.strip_suffix('s').unwrap_or(trimmed);
    match numeric.parse::<f64>() {
        // Guarded by `v.is_finite() && v >= 0.0`, so truncation only ever
        // drops a fractional-second remainder and sign loss cannot occur.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Ok(v) if v.is_finite() && v >= 0.0 => v as u64,
        _ => {
            tracing::debug!(exposure, "unparseable exposure snapshot; treating as 0s");
            0
        }
    }
}

/// Per-channel aggregate totals (sub-frame count, integration seconds),
/// grouped by `filter_snapshot`. Channel labels are matched against this map
/// by exact (case-sensitive) string equality — the same rule
/// `domain_core::project::channels::infer_channels` uses to derive labels
/// from filters in the first place.
fn channel_totals_by_filter(
    sources: &[repo::ProjectSourceRow],
) -> std::collections::HashMap<String, (u32, u64)> {
    let mut totals: std::collections::HashMap<String, (u32, u64)> =
        std::collections::HashMap::new();
    for s in sources {
        if s.filter_snapshot.is_empty() {
            continue;
        }
        let frames = u32::try_from(s.frames_snapshot).unwrap_or(0);
        let secs = parse_exposure_seconds(&s.exposure_snapshot);
        let entry = totals.entry(s.filter_snapshot.clone()).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(frames);
        entry.1 = entry.1.saturating_add(u64::from(frames).saturating_mul(secs));
    }
    totals
}

/// Convert a slice of DB channel rows to contract DTOs, aggregating
/// `subFrames`/`totalIntegrationS` from the project's linked sources (P7).
fn channels_to_dto(
    rows: &[repo::ProjectChannelRow],
    sources: &[repo::ProjectSourceRow],
) -> Vec<ProjectChannelDto> {
    let totals = channel_totals_by_filter(sources);
    rows.iter()
        .map(|r| {
            let (sub_frames, total_integration_s) = totals.get(&r.label).copied().unwrap_or((0, 0));
            ProjectChannelDto {
                label: r.label.clone(),
                source: r.source.clone(),
                added_at: Some(r.added_at.clone()),
                sub_frames,
                total_integration_s,
            }
        })
        .collect()
}

/// Convert a domain `Channel` (label + source only) to a contract DTO,
/// aggregating `subFrames`/`totalIntegrationS` from a pre-computed totals map
/// (P7). Used by the three call sites that build channels from a freshly
/// recomputed `Vec<Channel>` rather than DB rows.
fn channel_dto_from_domain(
    channel: Channel,
    added_at: &str,
    totals: &std::collections::HashMap<String, (u32, u64)>,
) -> ProjectChannelDto {
    let (sub_frames, total_integration_s) = totals.get(&channel.label).copied().unwrap_or((0, 0));
    ProjectChannelDto {
        label: channel.label,
        source: channel.source,
        added_at: Some(added_at.to_owned()),
        sub_frames,
        total_integration_s,
    }
}

/// Convert a DB source row to a contract DTO.
fn source_to_dto(row: &repo::ProjectSourceRow) -> contracts_core::projects_v2::ProjectSourceDto {
    // `role` and `selection` are None until spec 003 Inventory integration
    // provides confirmed session metadata with role + selection snapshots.
    contracts_core::projects_v2::ProjectSourceDto {
        inventory_id: row.inventory_session_id.clone(),
        name: row.name_snapshot.clone(),
        frames: u32::try_from(row.frames_snapshot).unwrap_or(0),
        filter: row.filter_snapshot.clone(),
        exposure: row.exposure_snapshot.clone(),
        linked_at: row.linked_at.clone(),
        role: None,
        selection: None,
    }
}

/// Derive the filter slice from source rows and call domain `infer_channels`.
fn infer_from_sources(sources: &[repo::ProjectSourceRow]) -> Vec<Channel> {
    let filters: Vec<&str> = sources.iter().map(|s| s.filter_snapshot.as_str()).collect();
    infer_channels(&filters)
}

/// Persist channels (replace_project_channels expects `&[(&str, &str)]`).
async fn persist_channels(
    pool: &SqlitePool,
    project_id: &str,
    channels: &[Channel],
) -> Result<(), persistence_core::DbError> {
    let pairs: Vec<(String, String)> =
        channels.iter().map(|c| (c.label.clone(), c.source.clone())).collect();
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(l, s)| (l.as_str(), s.as_str())).collect();
    repo::replace_project_channels(pool, project_id, &refs).await
}

/// Attempt the `setup_incomplete → ready` auto-transition (R-Ready-Trigger).
///
/// Delegates to `project_health::check_project_ready_invariant` which is the
/// single source of truth for this invariant (spec 009 P8). The lifecycle
/// service writes the audit row and publishes `project.lifecycle.ready`.
///
/// Returns the new lifecycle string if the transition was applied.
async fn maybe_auto_ready(
    pool: &SqlitePool,
    bus: &audit::bus::EventBus,
    project_id: &str,
    current_lifecycle: &str,
) -> Result<Option<String>, persistence_core::DbError> {
    if domain_core::lifecycle::ProjectState::parse_str(current_lifecycle)
        != Some(domain_core::lifecycle::ProjectState::SetupIncomplete)
    {
        return Ok(None);
    }
    project_health::check_project_ready_invariant(pool, bus, project_id)
        .await
        .map_err(|e| persistence_core::DbError::NotFound(e.to_string()))
}

/// Attempt the `ready → setup_incomplete` regression when all sources removed.
async fn maybe_regress_to_incomplete(
    pool: &SqlitePool,
    project_id: &str,
    current_lifecycle: &str,
) -> Result<Option<String>, persistence_core::DbError> {
    if domain_core::lifecycle::ProjectState::parse_str(current_lifecycle)
        != Some(domain_core::lifecycle::ProjectState::Ready)
    {
        return Ok(None);
    }
    let sources = repo::list_project_sources(pool, project_id).await?;
    if !sources.is_empty() {
        return Ok(None);
    }
    repo::update_project_lifecycle(pool, project_id, "setup_incomplete").await?;
    Ok(Some("setup_incomplete".to_owned()))
}

/// Fire the `SourceChange` manifest trigger after a source is linked/unlinked
/// (#665 — project create, source add/remove, and lifecycle transitions had
/// no emitters at all; this covers the source add/remove half).
///
/// Best-effort: a manifest write failure must never fail the source
/// add/remove use case itself (Constitution V — the DB row for the mutation
/// itself already succeeded; the manifest is a documentation side-effect).
/// Delegates to the shared `project_manifests::write_lifecycle_manifest`
/// (re-reads the project row, so the just-applied lifecycle change from
/// `maybe_auto_ready`/`maybe_regress_to_incomplete` is already reflected).
async fn write_source_change_manifest(
    pool: &SqlitePool,
    bus: &audit::bus::EventBus,
    project_id: &str,
) {
    use contracts_core::manifests::ManifestReason as DtoManifestReason;

    crate::project_manifests::write_lifecycle_manifest(
        pool,
        bus,
        project_id,
        DtoManifestReason::SourceChange,
    )
    .await;
}
