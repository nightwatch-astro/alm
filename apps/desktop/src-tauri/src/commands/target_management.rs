// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Gen-3 target management Tauri commands (spec 036 / spec 023 US2-US4).
//!
//! ## Commands
//!
//! - `target.get` — full detail for one canonical target (gen-3).
//! - `target.list` — list all canonical targets.
//! - `target.alias.add` — add a user alias to a target.
//! - `target.alias.remove` — remove a user alias by id (kind='user' only).
//! - `target.display_alias.set` — set the user presentation label (FR-012).
//! - `target.display_alias.clear` — clear the user presentation label (FR-012).
//! - `target.sessions.list` — list acquisition sessions linked to a target (spec 023 US2).
//! - `target.projects.list` — list projects linked to a target (spec 023 US3).
//! - `target.note.get` — read observing notes for a target (spec 023 US4).
//! - `target.note.update` — write observing notes for a target (spec 023 US4).
//!
//! The existing dotted command names `target.get`, `target.alias.add`, and
//! `target.alias.remove` are reused here (repointed from gen-2 to gen-3 per
//! spec 036 T016). New names `target.list`, `target.display_alias.set`, and
//! `target.display_alias.clear` are also registered in this module.
#![allow(clippy::doc_markdown)] // spec/domain terminology not suited for backticks

use contracts_core::targets::{
    TargetAliasAddRequest, TargetAliasAddResult, TargetAliasRemoveRequest, TargetAliasRemoveResult,
    TargetDetailV3, TargetDisplayAliasClearRequest, TargetDisplayAliasSetRequest, TargetGetRequest,
    TargetListItem, TargetNoteGetRequest, TargetNoteGetResult, TargetNoteUpdateRequest,
    TargetNoteUpdateResult, TargetProjectItem, TargetProjectsListRequest, TargetSessionItem,
    TargetSessionsListRequest,
};
use contracts_core::ContractError;
use tauri::State;

use crate::commands::lifecycle::AppState;

// ── target.get ───────────────────────────────────────────────────────────────

/// `target.get` — return full detail for a canonical target (gen-3).
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `target.not_found`,
/// `target.invalid_id`, or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_get(
    state: State<'_, AppState>,
    req: TargetGetRequest,
) -> Result<TargetDetailV3, ContractError> {
    tracing::debug!("target.get id={}", req.target_id);
    app_core::target_management::get(state.repo.pool(), &req).await
}

// ── target.list ──────────────────────────────────────────────────────────────

/// `target.list` — list canonical targets ordered by primary designation.
///
/// When `search` is provided, returns only targets whose primary designation,
/// effective label, or any alias (designation, common name, user-added)
/// case-insensitively contains the query string.  An empty `search` string is
/// treated as no filter (returns all targets).
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_list(
    state: State<'_, AppState>,
    search: Option<String>,
) -> Result<Vec<TargetListItem>, ContractError> {
    tracing::debug!("target.list search={search:?}");
    app_core::target_management::list(state.repo.pool(), search.as_deref()).await
}

// ── target.alias.add ─────────────────────────────────────────────────────────

/// `target.alias.add` — add a user alias to a canonical target (gen-3).
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `target.not_found`, `alias.blank`,
/// or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_alias_add(
    state: State<'_, AppState>,
    req: TargetAliasAddRequest,
) -> Result<TargetAliasAddResult, ContractError> {
    tracing::debug!("target.alias.add target_id={} alias={:?}", req.target_id, req.alias);
    app_core::target_management::alias_add(state.repo.pool(), &req).await
}

// ── target.alias.remove ──────────────────────────────────────────────────────

/// `target.alias.remove` — remove a user alias from a canonical target (gen-3).
///
/// Only aliases with `kind='user'` are removable; returns `alias.not_removable`
/// for SIMBAD designations/common names.
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `alias.not_found`,
/// `alias.not_removable`, or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_alias_remove(
    state: State<'_, AppState>,
    req: TargetAliasRemoveRequest,
) -> Result<TargetAliasRemoveResult, ContractError> {
    tracing::debug!("target.alias.remove target_id={} alias_id={}", req.target_id, req.alias_id);
    app_core::target_management::alias_remove(state.repo.pool(), &req).await
}

// ── target.display_alias.set ─────────────────────────────────────────────────

/// `target.display_alias.set` — set the user presentation label (FR-012).
///
/// Blank/empty input is stored as NULL (treated as a clear). Returns the
/// updated full detail.
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `target.not_found`,
/// `target.invalid_id`, or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_display_alias_set(
    state: State<'_, AppState>,
    req: TargetDisplayAliasSetRequest,
) -> Result<TargetDetailV3, ContractError> {
    tracing::debug!(
        "target.display_alias.set target_id={} display_alias={:?}",
        req.target_id,
        req.display_alias
    );
    app_core::target_management::display_alias_set(state.repo.pool(), &req).await
}

// ── target.display_alias.clear ───────────────────────────────────────────────

/// `target.display_alias.clear` — clear the user presentation label (FR-012).
///
/// Sets `display_alias = NULL`; `effectiveLabel` reverts to `primaryDesignation`.
/// Returns the updated full detail.
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `target.not_found`,
/// `target.invalid_id`, or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_display_alias_clear(
    state: State<'_, AppState>,
    req: TargetDisplayAliasClearRequest,
) -> Result<TargetDetailV3, ContractError> {
    tracing::debug!("target.display_alias.clear target_id={}", req.target_id);
    app_core::target_management::display_alias_clear(state.repo.pool(), &req).await
}

// ── target.sessions.list (spec 023 US2) ──────────────────────────────────────

/// `target.sessions.list` — list acquisition sessions linked to a canonical target.
///
/// Returns sessions ordered newest first (by `created_at`).  Returns an empty
/// list when the target exists but has no linked sessions.
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `target.not_found`,
/// `target.invalid_id`, or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_sessions_list(
    state: State<'_, AppState>,
    req: TargetSessionsListRequest,
) -> Result<Vec<TargetSessionItem>, ContractError> {
    tracing::debug!("target.sessions.list target_id={}", req.target_id);
    app_core::target_management::sessions_list(state.repo.pool(), &req).await
}

// ── target.projects.list (spec 023 US3) ──────────────────────────────────────

/// `target.projects.list` — list projects linked to a canonical target.
///
/// Returns projects ordered alphabetically by name.  Returns an empty list
/// when the target exists but has no linked projects.
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `target.not_found`,
/// `target.invalid_id`, or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_projects_list(
    state: State<'_, AppState>,
    req: TargetProjectsListRequest,
) -> Result<Vec<TargetProjectItem>, ContractError> {
    tracing::debug!("target.projects.list target_id={}", req.target_id);
    app_core::target_management::projects_list(state.repo.pool(), &req).await
}

// ── target.note.get (spec 023 US4) ───────────────────────────────────────────

/// `target.note.get` — read observing notes for a canonical target.
///
/// Returns `notes: null` when no notes are stored.
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `target.not_found`,
/// `target.invalid_id`, or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_note_get(
    state: State<'_, AppState>,
    req: TargetNoteGetRequest,
) -> Result<TargetNoteGetResult, ContractError> {
    tracing::debug!("target.note.get target_id={}", req.target_id);
    app_core::target_management::note_get(state.repo.pool(), &req).await
}

// ── target.note.update (spec 023 US4) ────────────────────────────────────────

/// `target.note.update` — write observing notes for a canonical target.
///
/// Empty or whitespace-only `notes` clears the field (stores NULL).
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `target.not_found`,
/// `target.invalid_id`, or `internal.database`.
#[tauri::command]
#[specta::specta]
pub async fn target_note_update(
    state: State<'_, AppState>,
    req: TargetNoteUpdateRequest,
) -> Result<TargetNoteUpdateResult, ContractError> {
    tracing::debug!("target.note.update target_id={} notes_len={}", req.target_id, req.notes.len());
    app_core::target_management::note_update(state.repo.pool(), &state.bus, &req).await
}
