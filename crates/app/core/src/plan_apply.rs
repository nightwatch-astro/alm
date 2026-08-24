// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Plan apply use-case orchestration (spec 025).
//!
//! Entry points:
//! - `apply_plan`        — start applying an approved plan (CAS, R-CAS-1, R-Concur-1).
//! - `cancel_plan`       — cancel an in-flight apply (US3).
//! - `pause_plan`        — (internal, called by executor callbacks on pause conditions).
//! - `resume_plan`       — resume a paused run (R-Pause-1).
//! - `skip_plan_item`    — skip a pending item within an active apply (US4).
//! - `retry_plan_item`   — retry a failed item within an active apply (US4).
//! - `get_apply_status`  — fetch current run state + counters.
//!
//! This module wires `crates/fs/executor` (pure filesystem logic) with
//! `crates/persistence/db` (state + audit records) and `crates/audit`
//! (event bus).
//!
//! Constitution §II: apply is gated by approval token; per-item FS CAS check
//! before each mutation (R-FS-1); audit event per state transition (FR-003).

#![allow(clippy::too_many_lines)]

//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b). Its only
//! cross-module dependency was on the now-extracted `app_core_errors` leaf and
//! nothing else in `app_core` references it. `app_core` re-exports this crate at
//! `app_core::plan_apply` so the public surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use audit::bus::EventBus;
use audit::event_bus::{
    PlanApplyingCompleted, PlanApplyingPaused, PlanApplyingResumed, PlanApplyingStarted,
    PlanItemProgress, Source, TOPIC_PLAN_APPLYING_COMPLETED, TOPIC_PLAN_APPLYING_PAUSED,
    TOPIC_PLAN_APPLYING_RESUMED, TOPIC_PLAN_APPLYING_STARTED, TOPIC_PLAN_ITEM_PROGRESS,
};
use audit::{AuditLogEntry, Outcome, Severity};
use camino::{Utf8Path, Utf8PathBuf};
use contracts_core::plan_apply::{
    PlanApplyResponse, PlanApplyStatus, PlanCancelResponse, PlanItemRetryResponse,
    PlanItemSkipResponse, PlanResumeResponse,
};
use contracts_core::{
    error_code::ErrorCode, ContractError, ErrorSeverity, OperationEvent, OperationEventType,
    OperationHandle, OperationId, OperationName, OperationStatus,
};
use domain_core::ids::{new_id, Timestamp};
use domain_core::lifecycle::data_asset::EntityType;
use fs_executor::ops::cas_check::CasSnapshot;
use fs_executor::run::{
    execute_plan, ApplyOutcome, CancellationToken, ExecutorCallbacks, ExecutorItem,
    ExecutorItemAction, ItemProgressEvent, RetryQueue, SkipSet, TerminalCounts,
};
use persistence_core::DbError;
use persistence_lifecycle::repositories::first_run as first_run_repo;
use persistence_plans::repositories::plan_apply as apply_repo;
use persistence_plans::repositories::plans as plans_repo;
use persistence_targets::repositories::inventory as inventory_repo;
use serde_json::json;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::audit_ids::deterministic_entity_id;
use crate::caches;
use crate::errors::bus_err;
use crate::path_set::PlanPathSet;
use dashmap::DashMap;

// ── Long-operation event sink (spec 042 US16, T240) ───────────────────────────

/// Operation name carried by the long-op contract for plan-apply runs.
pub const OP_NAME_PLAN_APPLY: &str = "plan.apply";

/// Live projection sink for [`OperationEvent`]s emitted by a plan-apply run.
///
/// This is the **live UI projection** of progress (spec 042 US16). It is
/// additive to — and never a replacement for — the durable DB audit trail
/// (`apply_repo::append_event`, constitution §II). The Tauri command supplies
/// a sink that forwards events over a `tauri::ipc::Channel<OperationEvent>`;
/// `app_core` itself stays transport-agnostic (no `tauri` dependency).
///
/// Sends are best-effort and infallible from the caller's perspective: if the
/// webview channel is gone, the closure swallows the error so the run still
/// completes and the audit record is still written.
pub type OperationEventSink = Arc<dyn Fn(OperationEvent) + Send + Sync>;

/// Monotonic per-run sequence + sink wrapper so every emitted event carries a
/// strictly increasing `sequence` (the UI uses it to order/dedupe).
#[derive(Clone)]
struct OpEventEmitter {
    operation_id: OperationId,
    sink: OperationEventSink,
    sequence: Arc<AtomicU64>,
}

impl OpEventEmitter {
    fn new(operation_id: OperationId, sink: OperationEventSink) -> Self {
        Self { operation_id, sink, sequence: Arc::new(AtomicU64::new(0)) }
    }

    /// Build the [`OperationHandle`] for this run at the given status.
    fn handle(&self, status: OperationStatus) -> OperationHandle {
        OperationHandle::new(
            self.operation_id.clone(),
            OperationName(OP_NAME_PLAN_APPLY.to_owned()),
            status,
        )
    }

    /// Emit one event with the next sequence number.
    fn emit(&self, event_type: OperationEventType, payload: serde_json::Value) {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
        let event = OperationEvent::new(self.operation_id.clone(), event_type, sequence, payload);
        (self.sink)(event);
    }
}

// ── Active runs registry ──────────────────────────────────────────────────────

/// Global registry of in-flight plan apply runs.
/// Keyed by `plan_id`; each entry holds the shared control objects.
///
/// Backed by a `DashMap` (concurrent map with internal sharded locking) so the
/// registry can be accessed from sync contexts without holding a `.await` lock.
/// Entries are inserted at apply start and removed by an [`ActiveRunGuard`] RAII
/// guard owned by the executor's background task. Because removal happens in the
/// guard's `Drop`, it runs on every scope exit — completion, cancel, pause, *or*
/// an unwind if `execute_plan` panics (FR-017) — guaranteeing no leaked entries.
static ACTIVE_RUNS: std::sync::OnceLock<Arc<DashMap<String, ActiveRun>>> =
    std::sync::OnceLock::new();

fn active_runs() -> Arc<DashMap<String, ActiveRun>> {
    ACTIVE_RUNS.get_or_init(|| Arc::new(DashMap::new())).clone()
}

struct ActiveRun {
    cancel_token: CancellationToken,
    skip_set: SkipSet,
    retry_queue: RetryQueue,
    #[allow(dead_code)] // retained for run introspection / diagnostics
    run_id: String,
    /// The (source ∪ destination ∪ archive) path prefixes this run claims,
    /// compared against pending applies by the FR-017 overlap check
    /// (R-Concur-1).
    path_set: PlanPathSet,
}

/// RAII guard that removes a plan's [`ActiveRun`] entry from the registry on
/// **any** scope exit — normal return, early break, *or* unwind from a panic
/// inside `execute_plan` (spec 042 US12/FR-017, acceptance scenario 2).
///
/// A plain sequential `registry.remove(&plan_id)` after `execute_plan(...)`
/// returns is skipped if `execute_plan` panics, because the unwind jumps past
/// it. The leaked entry then defeats the no-double-apply guard
/// (`check_overlap_and_register`) for that plan id forever. Holding this guard for the
/// duration of `execute_plan` makes removal run during the unwind instead.
///
/// The guard is constructed at the same point the entry is inserted and owned
/// by the spawned task scope, so its `Drop` is the single removal site for the
/// non-panic outcomes too (Completed / Cancelled / Paused) — removal happens
/// exactly once, regardless of how the task scope exits.
struct ActiveRunGuard {
    registry: Arc<DashMap<String, ActiveRun>>,
    plan_id: String,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        self.registry.remove(&self.plan_id);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn db_err(e: DbError) -> ContractError {
    match e {
        DbError::NotFound(msg) => {
            ContractError::new(ErrorCode::PlanNotFound, msg, ErrorSeverity::Blocking, false)
        }
        DbError::CasFailed(msg) => {
            ContractError::new(ErrorCode::PlanInvalidState, msg, ErrorSeverity::Blocking, false)
        }
        // Raised when the plan was reopened and re-approved between the
        // pre-flight token check and the apply CAS. Same contract code as the
        // pre-flight mismatch, so one cause has one failure.
        DbError::ApprovalStale(msg) => {
            ContractError::new(ErrorCode::PlanApprovalStale, msg, ErrorSeverity::Blocking, false)
        }
        other => crate::errors::db_err(other),
    }
}

mod apply;
mod callbacks;
mod finalizers;
mod lifecycle;
mod paths;
mod reconcile;
mod terminal;

#[cfg(test)]
mod tests;

pub use apply::apply_plan;
pub use lifecycle::{
    cancel_plan, confirm_plan_destructive_items, get_apply_status, list_crash_interrupted_plans,
    resume_plan, retry_plan_item, skip_plan_item, sweep_crashed_applying_plans,
};
pub(crate) use paths::resolve_root_path;
pub use reconcile::{reconcile_crashed_plans, ReconcileReport};

use apply::{spawn_executor_run, SpawnExecutorParams};
use callbacks::{audit_item_cancelled, PlanApplyCallbacks};
use finalizers::{
    finalize_archive_lifecycle, finalize_calibration_master_archive,
    finalize_calibration_master_restore, finalize_project_create_manifest,
    finalize_restore_lifecycle, finalize_view_generation, finalize_view_regeneration,
    finalize_view_removal,
};
use paths::{
    check_overlap_and_register, compute_plan_path_set, item_row_to_executor_item,
    materialization_from_provenance, verify_approval_token,
};
