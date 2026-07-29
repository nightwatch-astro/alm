// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Plan apply contract DTOs for the Tauri IPC surface (spec 025).
//!
//! Mirrors the JSON Schema contracts under
//! `specs/025-filesystem-plan-application/contracts/`.
//!
//! Field names are camelCase (R-Env-1). Apply-side state transitions are
//! exclusively owned by spec 025's executor.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;

// ── plan.apply ────────────────────────────────────────────────────────────────

/// Request for `plans.apply` — start applying an approved plan.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanApplyRequest {
    pub plan_id: String,
    /// Approval token from `plans.approve` — v1 is an opaque
    /// `tok-{plan_id}-{uuid_v4}` string (no cryptographic binding to plan
    /// content; see `app_core::plans::approve_plan`).
    pub approval_token: String,
}

/// Response for `plans.apply` on success.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanApplyResponse {
    pub plan_id: String,
    /// Id of the `PlanApplyRun` row (mandatory, R-Run-1).
    pub run_id: String,
    pub new_state: String,
}

// ── plan.cancel ───────────────────────────────────────────────────────────────

/// Response for `plans.cancel`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanCancelResponse {
    pub plan_id: String,
    pub cancelled_at: String,
    /// Items that finished (succeeded or failed) before cancellation.
    pub items_applied: i64,
    /// Items transitioned from pending to cancelled.
    pub items_cancelled: i64,
}

// ── plan.resume ───────────────────────────────────────────────────────────────

/// Response for `plans.resume`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanResumeResponse {
    pub plan_id: String,
    pub run_id: String,
    pub resumed_at: String,
}

// ── plan.item.skip ────────────────────────────────────────────────────────────

/// Response for `plans.item.skip`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemSkipResponse {
    pub item_id: String,
    pub new_state: String,
}

// ── plan.item.retry ───────────────────────────────────────────────────────────

/// Response for `plans.item.retry`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemRetryResponse {
    pub item_id: String,
    pub new_state: String,
}

// ── Apply status / progress DTOs ─────────────────────────────────────────────

/// Per-item progress event emitted during apply.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemProgressEvent {
    pub run_id: String,
    pub plan_id: String,
    pub item_id: String,
    pub prior_state: String,
    pub new_state: String,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<PlanItemFailureDto>,
}

/// Structured failure detail in a progress event.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemFailureDto {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

/// Terminal event emitted after a plan apply run completes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanTerminalEvent {
    pub run_id: String,
    pub plan_id: String,
    pub terminal_state: String,
    pub at: String,
    pub counts: PlanTerminalCounts,
}

/// Counter breakdown at plan termination.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanTerminalCounts {
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub cancelled: i64,
}

/// Apply status for the frontend (current run state).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanApplyStatus {
    pub plan_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub plan_state: String,
    pub items_total: i64,
    pub items_applied: i64,
    pub items_failed: i64,
    pub items_skipped: i64,
    pub items_cancelled: i64,
    pub items_pending: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_reason: Option<String>,
}

// ── recovery.status ─────────────────────────────────────────────────────────

/// Response for `recovery.status` — the unclean-shutdown recovery prompt input.
///
/// `unclean_shutdown` is true when the clean-shutdown marker was absent at
/// boot (the previous process did not exit gracefully). `interrupted_plan_ids`
/// lists plans left mid-apply by the crash (still `applying`, or `paused` with
/// a `crash` reason) — the recovery prompt offers these for review/resume.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryStatus {
    pub unclean_shutdown: bool,
    pub interrupted_plan_ids: Vec<String>,
}
