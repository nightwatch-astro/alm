// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Plan apply Tauri commands (spec 025).
//!
//! Implements the five JSON-Schema contracts under
//! `specs/025-filesystem-plan-application/contracts/`, plus one additional
//! confirm command (issue #741, not part of the original spec 025 contract
//! set — the DB column it writes predates this command by several specs):
//! - `plans.apply`       — start applying an approved plan.
//! - `plans.cancel`      — cancel an in-flight apply.
//! - `plans.resume`      — resume a paused apply run.
//! - `plans.item.skip`   — skip a pending item.
//! - `plans.item.retry`  — retry a failed item.
//! - `plans.apply.status`— fetch current apply status.
//! - `plans.confirm.destructive` — confirm a plan's delete/trash items.
//!
//! All state-machine enforcement lives in `crates/app/core/src/plan_apply.rs`.
//! These commands are thin adapters: validate inputs, delegate, return DTOs.

use std::sync::Arc;

use app_core::plan_apply::{
    apply_plan, cancel_plan, confirm_plan_destructive_items, get_apply_status, resume_plan,
    retry_plan_item, skip_plan_item, OperationEventSink,
};
use contracts_core::plan_apply::{
    PlanApplyResponse, PlanApplyStatus, PlanCancelResponse, PlanItemRetryResponse,
    PlanItemSkipResponse, PlanResumeResponse,
};
use serde::Serialize;
use specta::Type;
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::lifecycle::AppState;
use contracts_core::{ContractError, OperationEvent};

// ── plans.apply ───────────────────────────────────────────────────────────────

/// `plans.apply` — start applying an approved plan (US1, T019; spec 042 US16 T240).
///
/// Returns immediately with the run id and new state (`"applying"`).
///
/// Live progress is streamed over the additive `on_event`
/// `tauri::ipc::Channel<OperationEvent>` (spec 042 US16): the backend emits a
/// `Started` event carrying the running `OperationHandle`, per-item
/// `Progress`/`ItemApplied`/`ItemFailed` events, and a terminal
/// `Completed`/`Failed` event carrying a terminal handle. The channel is the
/// **live UI projection** — the durable audit trail (`plan_apply_events`) and
/// the audit bus topics (`plan.item.progress`, `plan.applying.completed`) are
/// retained unchanged (constitution §II).
///
/// # Errors
///
/// Returns `Err(ContractError)` with:
/// - `"plan.not_found"` — plan not found.
/// - `"plan.invalid_state"` — plan is not approved or CAS race.
/// - `"plan.approval.stale"` — approval token mismatch.
/// - `"plan.conflict.overlap"` — concurrent apply already running.
#[tauri::command]
#[specta::specta]
pub async fn plans_apply_real(
    state: State<'_, AppState>,
    plan_id: String,
    approval_token: String,
    on_event: Channel<OperationEvent>,
) -> Result<PlanApplyResponse, ContractError> {
    // Bridge the long-op contract to the webview channel. Sends are best-effort:
    // if the channel is gone (window closed), swallow the error so the run still
    // completes and the durable audit record is still written.
    let sink: OperationEventSink = Arc::new(move |event: OperationEvent| {
        if let Err(error) = on_event.send(event) {
            tracing::warn!(%error, "plan-apply OperationEvent channel send failed");
        }
    });

    apply_plan(state.repo.pool(), &state.bus, &plan_id, &approval_token, Some(sink)).await
}

// ── plans.apply.direct ───────────────────────────────────────────────────────

/// `plans.apply.direct` — channel-free variant of `plans.apply` (spec 037).
///
/// Runs the same background executor, the same approval-token gate, and the
/// same durable audit trail as `plans_apply_real` — it just takes no
/// `tauri::ipc::Channel`, so it can be invoked directly by the Layer-2
/// `WebDriver` E2E bridge (which could build a `Channel`, but should not have
/// to reach into Tauri internals to do it) or by any UI surface that only
/// needs a fire-and-poll apply (poll `plans.apply.status` for the durable
/// terminal counts) rather than a live progress stream.
///
/// The caller supplies the `approval_token` returned by `plans.approve`. This
/// command must never mint that token itself: the gate in `apply_plan` is the
/// only evidence that a human approved the plan (constitution §II).
///
/// # Errors
///
/// Returns `Err(ContractError)` with:
/// - `"plan.not_found"` — plan not found.
/// - `"plan.invalid_state"` — plan is not `approved` (e.g. still
///   `ready_for_review`, or already applied/discarded/applying).
/// - `"plan.approval.stale"` — approval token absent, or not this plan's.
/// - `"plan.conflict.overlap"` — concurrent apply already running.
#[tauri::command]
#[specta::specta]
pub async fn plans_apply_direct(
    state: State<'_, AppState>,
    plan_id: String,
    approval_token: String,
) -> Result<PlanApplyResponse, ContractError> {
    apply_plan(state.repo.pool(), &state.bus, &plan_id, &approval_token, None).await
}

// ── plans.cancel ──────────────────────────────────────────────────────────────

/// `plans.cancel` — cancel an in-flight apply (US3, T033).
///
/// Signals the cancellation token; the executor finishes its current item
/// and stops. Remaining pending items are batch-transitioned to `cancelled`.
///
/// # Errors
///
/// Returns `Err(String)` with:
/// - `"plan.not_found"` — plan not found.
/// - `"plan.not_in_apply"` — plan is not in applying or paused state.
#[tauri::command]
#[specta::specta]
pub async fn plans_cancel(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<PlanCancelResponse, ContractError> {
    cancel_plan(state.repo.pool(), &state.bus, &plan_id).await
}

// ── plans.resume ──────────────────────────────────────────────────────────────

/// `plans.resume` — resume a paused apply run (R-Pause-1, T053).
///
/// # Errors
///
/// Returns `Err(String)` with:
/// - `"plan.not_found"` — plan not found.
/// - `"run.not_paused"` — plan is not in paused state.
/// - `"run.not_found"` — run id does not match active run.
#[tauri::command]
#[specta::specta]
pub async fn plans_resume(
    state: State<'_, AppState>,
    plan_id: String,
    run_id: String,
) -> Result<PlanResumeResponse, ContractError> {
    resume_plan(state.repo.pool(), &state.bus, &plan_id, &run_id).await
}

// ── plans.item.skip ───────────────────────────────────────────────────────────

/// `plans.item.skip` — skip a pending item during an active apply (US4, T041).
///
/// The item must be `pending` and the plan must be `applying`.
///
/// # Errors
///
/// Returns `Err(String)` with:
/// - `"plan.not_found"` — plan not found.
/// - `"plan.not_in_apply"` — plan is not applying.
/// - `"item.not_found"` — item not found.
/// - `"item.not_pending"` — item is not in pending state.
#[tauri::command]
#[specta::specta]
pub async fn plans_item_skip(
    state: State<'_, AppState>,
    plan_id: String,
    item_id: String,
) -> Result<PlanItemSkipResponse, ContractError> {
    skip_plan_item(state.repo.pool(), &plan_id, &item_id).await
}

// ── plans.item.retry ──────────────────────────────────────────────────────────

/// `plans.item.retry` — retry a failed item within a running apply (US4, T041).
///
/// The item must be `failed` and the plan must be `applying`.
/// Use `plans.retry` for plan-level retry after a terminal plan.
///
/// # Errors
///
/// Returns `Err(String)` with:
/// - `"plan.not_found"` — plan not found.
/// - `"plan.not_in_apply"` — plan is not applying (use plans.retry for terminal).
/// - `"item.not_found"` — item not found.
/// - `"item.not_failed"` — item is not in failed state.
#[tauri::command]
#[specta::specta]
pub async fn plans_item_retry(
    state: State<'_, AppState>,
    plan_id: String,
    item_id: String,
) -> Result<PlanItemRetryResponse, ContractError> {
    retry_plan_item(state.repo.pool(), &plan_id, &item_id).await
}

// ── plans.confirm.destructive ─────────────────────────────────────────────────

/// Response for `plans.confirm.destructive`.
///
/// Local to this command (rather than a `contracts_core::plan_apply` DTO):
/// the shape is a plain confirmation receipt with no other consumer.
#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanDestructiveConfirmResponse {
    pub plan_id: String,
    /// Number of items whose `destructive_confirmed` flag flipped (0 when
    /// every destructive item in the plan was already confirmed).
    pub items_confirmed: i64,
}

/// `plans.confirm.destructive` — confirm every delete/trash item in a plan
/// (FR-003, D9, issue #741).
///
/// Persists `destructive_confirmed = 1` on the plan's destructive items so a
/// subsequent apply does not refuse them at the executor's
/// destructive-confirm gate. Plan-level, not per-item — see
/// `confirm_plan_destructive_items`'s doc comment for why.
///
/// # Errors
///
/// Returns `Err(ContractError)` with `"plan.not_found"` if the plan does not
/// exist.
#[tauri::command]
#[specta::specta]
pub async fn plans_confirm_destructive(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<PlanDestructiveConfirmResponse, ContractError> {
    let items_confirmed = confirm_plan_destructive_items(state.repo.pool(), &plan_id).await?;
    Ok(PlanDestructiveConfirmResponse { plan_id, items_confirmed })
}

// ── plans.apply.status ────────────────────────────────────────────────────────

/// `plans.apply.status` — fetch current apply progress for a plan.
///
/// # Errors
///
/// Returns `Err(String)` with `"plan.not_found"` if the plan does not exist.
#[tauri::command]
#[specta::specta]
pub async fn plans_apply_status(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<PlanApplyStatus, ContractError> {
    get_apply_status(state.repo.pool(), &plan_id).await
}
