// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Plan review Tauri commands (spec 017).
//!
//! Implements the five JSON-Schema contracts under
//! `specs/017-cleanup-archive-review-plans/contracts/` plus the two
//! archive-management contracts (`archive.send_to_trash`,
//! `archive.permanently_delete`).
//!
//! All state-machine enforcement lives in `crates/app/core/src/plans.rs`.
//! These commands are thin adapters: validate inputs, delegate to use cases,
//! return contract DTOs.
//!
//! The `plans.apply` stub is retained for spec 025 compatibility — it returns
//! `unimplemented` and will be replaced when spec 025 lands.

use app_core::archive_generator::{
    generate as generate_archive_plan, generate_restore as generate_restore_plan, list_archived,
};
use app_core::plan_free_space::estimate_free_space;
use app_core::plans::{
    approve_plan, discard_plan, get_plan, list_plans, permanently_delete_archive, reopen_plan,
    retry_plan, send_archive_to_trash,
};
use contracts_core::archive::{
    ArchiveListResponse, GenerateArchivePlanResult, GenerateRestorePlanResult,
};
use contracts_core::plans::{
    ArchivePermanentlyDeleteResponse, ArchiveSendToTrashResponse, PlanApproveResponse, PlanDetail,
    PlanDiscardResponse, PlanFreeSpaceEstimate, PlanListRequest, PlanListResponse,
    PlanReopenResponse, PlanRetryResponse, RetryItemsFilter,
};
use tauri::State;

use crate::commands::lifecycle::AppState;
use contracts_core::ContractError;

// ── plans.list ────────────────────────────────────────────────────────────────

/// `plans.list` — list reviewable plans, failed-first ordering (US1, T014).
///
/// # Errors
///
/// Returns `Err(String)` with the contract error code on failure.
#[tauri::command]
#[specta::specta]
pub async fn plans_list(
    state: State<'_, AppState>,
    state_filter: Option<Vec<String>>,
    origin_filter: Option<Vec<String>>,
    created_after: Option<String>,
    limit: Option<i64>,
) -> Result<PlanListResponse, ContractError> {
    let req = PlanListRequest { state_filter, origin_filter, created_after, limit };
    list_plans(state.repo.pool(), &req).await
}

// ── plans.get ─────────────────────────────────────────────────────────────────

/// `plans.get` — fetch a plan with all its items (US1, T014).
///
/// # Errors
///
/// Returns `Err(String)` with `"plan.not_found"` if the plan does not exist.
#[tauri::command]
#[specta::specta]
pub async fn plans_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<PlanDetail, ContractError> {
    get_plan(state.repo.pool(), &id).await
}

// ── plans.free_space_estimate ─────────────────────────────────────────────────

/// `plans.free_space_estimate` — advisory destination free-space estimate for
/// a plan under review, before approval (issue #876). Never blocks approval;
/// see `app_core::plans::estimate_free_space`.
///
/// # Errors
///
/// Returns `Err(String)` with `"plan.not_found"` if the plan does not exist.
#[tauri::command]
#[specta::specta]
pub async fn plans_free_space_estimate(
    state: State<'_, AppState>,
    id: String,
) -> Result<PlanFreeSpaceEstimate, ContractError> {
    estimate_free_space(state.repo.pool(), &id).await
}

// ── plans.approve ─────────────────────────────────────────────────────────────

/// `plans.approve` — move a plan to `approved`; snapshot item FS metadata (US3, T025).
///
/// # Errors
///
/// Returns `Err(String)` with `"plan.not_found"`, `"plan.invalid_state"`, or
/// `"plan.items.empty"` on failure.
#[tauri::command]
#[specta::specta]
pub async fn plans_approve(
    state: State<'_, AppState>,
    id: String,
) -> Result<PlanApproveResponse, ContractError> {
    approve_plan(state.repo.pool(), &state.bus, &id, "user").await
}

// ── plans.reopen ──────────────────────────────────────────────────────────────

/// `plans.reopen` — return a plan to `draft`, invalidating its approval
/// (US3, T023).
///
/// Any `approvalToken` held from a previous `plans.approve` stops working.
///
/// # Errors
///
/// Returns `Err(String)` with `"plan.not_found"` or `"plan.invalid_state"` on
/// failure.
#[tauri::command]
#[specta::specta]
pub async fn plans_reopen(
    state: State<'_, AppState>,
    id: String,
) -> Result<PlanReopenResponse, ContractError> {
    reopen_plan(state.repo.pool(), &state.bus, &id, "user").await
}

// ── plans.apply (superseded by spec 025) ─────────────────────────────────────
// The real `plans.apply` command lives in `commands/plan_apply.rs` (spec 025).
// This stub is retained for source compatibility only and is NOT registered
// in the collect_commands! macro.

// ── plans.discard ─────────────────────────────────────────────────────────────

/// `plans.discard` — soft-delete a plan (US4, T030).
///
/// # Errors
///
/// Returns `Err(String)` with `"plan.not_found"` or `"plan.in_progress"` on failure.
#[tauri::command]
#[specta::specta]
pub async fn plans_discard(
    state: State<'_, AppState>,
    id: String,
) -> Result<PlanDiscardResponse, ContractError> {
    discard_plan(state.repo.pool(), &state.bus, &id).await
}

// ── plans.retry ───────────────────────────────────────────────────────────────

/// `plans.retry` — create a new plan from failed/cancelled/all items of a
/// terminal parent (US5, T035).
///
/// `items_filter` must be one of `"failed"`, `"cancelled"`, or `"all"` (R-Retry-1).
///
/// # Errors
///
/// Returns `Err(String)` with `"parent.not_found"`, `"parent.not_terminal"`,
/// `"no.items.to.retry"`, or `"value.invalid"` on failure.
#[tauri::command]
#[specta::specta]
pub async fn plans_retry(
    state: State<'_, AppState>,
    parent_plan_id: String,
    items_filter: String,
) -> Result<PlanRetryResponse, ContractError> {
    let filter = match items_filter.as_str() {
        "failed" => RetryItemsFilter::Failed,
        "cancelled" => RetryItemsFilter::Cancelled,
        "all" => RetryItemsFilter::All,
        other => {
            return Err(ContractError::new(
                contracts_core::error_code::ErrorCode::ValueInvalid,
                format!("unknown items_filter: '{other}'"),
                contracts_core::ErrorSeverity::Blocking,
                false,
            ))
        }
    };

    retry_plan(state.repo.pool(), &state.bus, &parent_plan_id, filter).await
}

// ── archive.list ────────────────────────────────────────────────────────────

/// `archive.list` — list archived projects (spec 017 C5) AND archived
/// calibration masters (#886), each row carrying `archivedViaPlanId` so the
/// management commands (`archive.send_to_trash`/`archive.permanently_delete`,
/// already entity-agnostic — they act purely on a plan's items) work on
/// either kind. The two listings are composed here rather than folded into
/// one generator function, keeping each generator scoped to the entity it
/// owns (`app_core::archive_generator` / `app_core::calibration_archive_generator`).
///
/// # Errors
///
/// Returns `Err` with the contract error code on database failure.
#[tauri::command]
#[specta::specta]
pub async fn archive_list(
    state: State<'_, AppState>,
) -> Result<ArchiveListResponse, ContractError> {
    let pool = state.repo.pool();
    let mut entries = list_archived(pool).await?.entries;
    entries.extend(app_core::calibration_archive_generator::list_archived(pool).await?);
    Ok(ArchiveListResponse { entries })
}

// ── archive.plan.generate ─────────────────────────────────────────────────────

/// `archive.plan.generate` — build a reviewable whole-project archive plan
/// (spec 017 US2/WP-B). Creates a `ready_for_review` plan; performs NO
/// filesystem mutation and never auto-applies (constitution II / FR-002).
///
/// # Errors
///
/// Returns `Err` with the contract error code on database failure.
#[tauri::command]
#[specta::specta]
pub async fn archive_plan_generate(
    state: State<'_, AppState>,
    project_id: String,
    title: Option<String>,
) -> Result<GenerateArchivePlanResult, ContractError> {
    generate_archive_plan(state.repo.pool(), &project_id, title.as_deref()).await
}

// ── archive.plan.generate_restore ─────────────────────────────────────────────

/// `archive.plan.generate_restore` — build a reviewable restore (un-archive)
/// plan from a previously applied archive plan (#885, decision D15). Creates
/// a `ready_for_review` plan; performs NO filesystem mutation and never
/// auto-applies (constitution II / FR-002).
///
/// # Errors
///
/// Returns `Err` with `"plan.not_found"`, `"plan.invalid_state"` (not an
/// applied archive plan), or `"archive.empty"` (nothing to restore).
#[tauri::command]
#[specta::specta]
pub async fn archive_plan_generate_restore(
    state: State<'_, AppState>,
    archived_plan_id: String,
    title: Option<String>,
) -> Result<GenerateRestorePlanResult, ContractError> {
    generate_restore_plan(state.repo.pool(), &archived_plan_id, title.as_deref()).await
}

// ── archive.send_to_trash ─────────────────────────────────────────────────────

/// `archive.send_to_trash` — send the archive subtree to OS trash (US6, T045).
///
/// # Errors
///
/// Returns `Err(String)` with `"plan.not_found"` or `"archive.empty"` on failure.
#[tauri::command]
#[specta::specta]
pub async fn archive_send_to_trash(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<ArchiveSendToTrashResponse, ContractError> {
    send_archive_to_trash(state.repo.pool(), &state.bus, &plan_id).await
}

// ── archive.permanently_delete ────────────────────────────────────────────────

/// `archive.permanently_delete` — permanently remove archive subtree (US6, T046).
///
/// Requires `confirm_text == "DELETE"`. Blocked if spec-016 `blockPermanentDelete` is true.
///
/// # Errors
///
/// Returns `Err(String)` with `"confirm.text.mismatch"`, `"plan.blocked_by_protection"`,
/// `"plan.not_found"`, or `"archive.empty"` on failure.
#[tauri::command]
#[specta::specta]
pub async fn archive_permanently_delete(
    state: State<'_, AppState>,
    plan_id: String,
    confirm_text: String,
) -> Result<ArchivePermanentlyDeleteResponse, ContractError> {
    // Read blockPermanentDelete from settings (spec 016 protection gate).
    // We load the setting directly from the settings store rather than caching
    // in AppState. Resolved via `app_core::settings::resolve_setting` (not the
    // raw `persistence_lifecycle::repositories::settings` table) because
    // `blockPermanentDelete` is a global-protection-default key backed by the
    // dedicated `protection_defaults` table (spec 016 T-003/T-005) — reading
    // the legacy generic settings table directly would silently ignore every
    // value the user actually saved via the Cleanup settings pane.
    let pool = state.repo.pool();
    let block: bool = app_core::settings::resolve_setting(pool, "blockPermanentDelete", None)
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    permanently_delete_archive(pool, &state.bus, &plan_id, &confirm_text, block).await
}
