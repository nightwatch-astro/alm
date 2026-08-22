// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-terminal-outcome handlers for `spawn_executor_run`.
//!
//! Extracted from `apply.rs` to keep `spawn_executor_run` at a navigable size
//! while preserving identical runtime behavior.

use super::{
    apply_repo, audit_item_cancelled, deterministic_entity_id, finalize_archive_lifecycle,
    finalize_calibration_master_archive, finalize_calibration_master_restore,
    finalize_project_create_manifest, finalize_restore_lifecycle, finalize_view_generation,
    finalize_view_regeneration, finalize_view_removal, json, new_id, AuditLogEntry, EntityType,
    EventBus, OpEventEmitter, OperationEventType, OperationStatus, Outcome, PlanApplyingCompleted,
    PlanApplyingPaused, Severity, Source, SqlitePool, TerminalCounts, Timestamp,
    TOPIC_PLAN_APPLYING_COMPLETED, TOPIC_PLAN_APPLYING_PAUSED,
};

use super::apply::cumulative_counts;

/// Surface a terminal-state write that did not commit, in place of the state it
/// would have announced.
///
/// The plan row keeps whatever state the failed transaction left, so the run
/// stays `applying` and `sweep_crashed_applying_plans` with
/// `list_crash_interrupted_plans` claim it at the next boot. Callers must return
/// after this instead of publishing the intended state.
fn report_unwritten_state(
    plan_id: &str,
    run_id: &str,
    intended_state: &str,
    reason: Option<&str>,
    error: &str,
    op_emitter: Option<&OpEventEmitter>,
    at: &str,
) {
    tracing::error!(
        plan_id, run_id, intended_state, pause_reason = reason, %error,
        "a terminal plan state was not persisted; the plan row is left for boot recovery"
    );
    if let Some(emitter) = op_emitter.as_ref() {
        let handle = emitter.handle(OperationStatus::Failed);
        emitter.emit(
            OperationEventType::Failed,
            json!({
                "handle": handle,
                "planId": plan_id,
                "runId": run_id,
                "intendedState": intended_state,
                "pauseReason": reason,
                "error": error,
                "at": at,
            }),
        );
    }
}

/// Handle `ApplyOutcome::Completed`: finalize origin-specific side-effects,
/// persist the terminal state, emit audit + bus + long-op events.
#[expect(
    clippy::cognitive_complexity,
    reason = "terminal-state dispatch over plan origin and per-origin finalizer set"
)]
pub(super) async fn handle_completed(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    run_id: &str,
    plan_origin: &str,
    plan_project_id: Option<&str>,
    op_emitter: Option<&OpEventEmitter>,
    counts: TerminalCounts,
) {
    let at = Timestamp::now_iso();

    // GFD-2: Sweep items orphaned `applying` by a mid-run retry whose DB flip
    // landed but whose re-execution never got picked up before the run
    // completed (symmetric with the Cancelled path's identical sweep). Without
    // this, a retry_plan_item call racing with run completion leaves the item
    // permanently stuck in `applying` with no terminal audit record.
    // cumulative_counts is called AFTER this sweep so swept items are counted.
    match apply_repo::cancel_orphaned_applying_items(pool, plan_id).await {
        Ok(orphaned_ids) => {
            for item_id in &orphaned_ids {
                audit_item_cancelled(pool, bus, run_id, plan_id, item_id, "applying", &at).await;
            }
        }
        Err(e) => {
            tracing::error!(error=%e, "failed to sweep orphaned applying items on completion");
        }
    }

    let counts = cumulative_counts(pool, plan_id, &counts).await;
    let terminal = counts.terminal_state(false).to_owned();

    // Origin-specific lifecycle side-effects (only on clean terminals).
    if terminal == "applied" {
        match plan_origin {
            "archive" => {
                if let Some(project_id) = plan_project_id {
                    finalize_archive_lifecycle(pool, bus, plan_id, project_id).await;
                }
            }
            "restore" => {
                if let Some(project_id) = plan_project_id {
                    finalize_restore_lifecycle(pool, bus, project_id).await;
                }
            }
            "calibration_master_archive" => {
                if let Some(master_id) = plan_project_id {
                    finalize_calibration_master_archive(pool, plan_id, master_id).await;
                }
            }
            "calibration_master_restore" => {
                if let Some(master_id) = plan_project_id {
                    finalize_calibration_master_restore(pool, master_id).await;
                }
            }
            "project" => {
                if let Some(project_path) = plan_project_id {
                    finalize_project_create_manifest(pool, bus, project_path).await;
                }
            }
            _ => {}
        }
    }

    // Source-view finalization (applied or partially_applied).
    if terminal == "applied" || terminal == "partially_applied" {
        match plan_origin {
            "prepared_view_generation" => {
                if let Some(project_id) = plan_project_id {
                    finalize_view_generation(pool, plan_id, project_id).await;
                }
            }
            "prepared_view_removal" => {
                finalize_view_removal(pool, plan_id, &terminal).await;
            }
            "prepared_view_regeneration" => {
                finalize_view_regeneration(pool, plan_id).await;
            }
            _ => {}
        }
    }

    if let Err(e) = apply_repo::complete_run(
        pool,
        plan_id,
        run_id,
        &terminal,
        counts.succeeded,
        counts.failed,
        counts.skipped,
        counts.cancelled,
    )
    .await
    {
        report_unwritten_state(plan_id, run_id, &terminal, None, &e.to_string(), op_emitter, &at);
        return;
    }

    let _ = apply_repo::append_event(
        pool,
        &new_id(),
        run_id,
        plan_id,
        None,
        "applying",
        &terminal,
        &at,
        None,
        None,
    )
    .await;

    let _ = bus
        .publish(
            TOPIC_PLAN_APPLYING_COMPLETED,
            Source::System,
            PlanApplyingCompleted {
                plan_id: plan_id.to_owned(),
                run_id: run_id.to_owned(),
                terminal_state: terminal.clone(),
                items_applied: counts.succeeded,
                items_failed: counts.failed,
                items_skipped: counts.skipped,
                items_cancelled: counts.cancelled,
                at: at.clone(),
            },
        )
        .await;

    if let Some(emitter) = op_emitter.as_ref() {
        let failed_run = terminal == "failed";
        let (event_type, status) = if failed_run {
            (OperationEventType::Failed, OperationStatus::Failed)
        } else {
            (OperationEventType::Completed, OperationStatus::Completed)
        };
        let handle = emitter.handle(status);
        emitter.emit(
            event_type,
            json!({
                "handle": handle,
                "planId": plan_id,
                "runId": run_id,
                "terminalState": terminal,
                "itemsApplied": counts.succeeded,
                "itemsFailed": counts.failed,
                "itemsSkipped": counts.skipped,
                "itemsCancelled": counts.cancelled,
                "at": at,
            }),
        );
    }
}

/// Handle `ApplyOutcome::Cancelled`: batch-cancel pending items, sweep orphans,
/// persist terminal state, emit events.
pub(super) async fn handle_cancelled(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    run_id: &str,
    op_emitter: Option<&OpEventEmitter>,
    counts: TerminalCounts,
) {
    let at = Timestamp::now_iso();

    // Batch-cancel remaining pending items (T021: per-item audit row for EACH).
    cancel_pending_items(pool, bus, run_id, plan_id, &at).await;

    // Sweep items orphaned `applying` by a mid-run retry whose DB flip landed
    // but whose re-execution never got picked up before cancellation.
    match apply_repo::cancel_orphaned_applying_items(pool, plan_id).await {
        Ok(orphaned_ids) => {
            for item_id in &orphaned_ids {
                audit_item_cancelled(pool, bus, run_id, plan_id, item_id, "applying", &at).await;
            }
        }
        Err(e) => {
            tracing::error!(error=%e, "failed to sweep orphaned applying items for cancel audit");
        }
    }

    // Fetch cumulative counters AFTER batch-cancel so cancelled items are counted.
    let counts = cumulative_counts(pool, plan_id, &counts).await;

    if let Err(e) = apply_repo::complete_run(
        pool,
        plan_id,
        run_id,
        "cancelled",
        counts.succeeded,
        counts.failed,
        counts.skipped,
        counts.cancelled,
    )
    .await
    {
        report_unwritten_state(plan_id, run_id, "cancelled", None, &e.to_string(), op_emitter, &at);
        return;
    }

    let _ = apply_repo::append_event(
        pool,
        &new_id(),
        run_id,
        plan_id,
        None,
        "applying",
        "cancelled",
        &at,
        None,
        None,
    )
    .await;

    let _ = bus
        .publish(
            TOPIC_PLAN_APPLYING_COMPLETED,
            Source::System,
            PlanApplyingCompleted {
                plan_id: plan_id.to_owned(),
                run_id: run_id.to_owned(),
                terminal_state: "cancelled".to_owned(),
                items_applied: counts.succeeded,
                items_failed: counts.failed,
                items_skipped: counts.skipped,
                items_cancelled: counts.cancelled,
                at: at.clone(),
            },
        )
        .await;

    if let Some(emitter) = op_emitter.as_ref() {
        let handle = emitter.handle(OperationStatus::Cancelled);
        emitter.emit(
            OperationEventType::Completed,
            json!({
                "handle": handle,
                "planId": plan_id,
                "runId": run_id,
                "terminalState": "cancelled",
                "itemsApplied": counts.succeeded,
                "itemsFailed": counts.failed,
                "itemsSkipped": counts.skipped,
                "itemsCancelled": counts.cancelled,
                "at": at,
            }),
        );
    }
}

/// Handle `ApplyOutcome::Paused`: persist pause state, emit events.
///
/// The pause row is a Tier-1 record: `pause_reason` is what
/// `revalidate_pause_condition` re-checks on resume, and a boot sweep can only
/// re-derive the generic `crash` reason. So the paused events are emitted only
/// after `pause_run` commits. When the write fails the plan row stays
/// `applying`, which `sweep_crashed_applying_plans` and
/// `list_crash_interrupted_plans` classify at the next boot, and the long-op
/// event carries the failure instead of a pause.
pub(super) async fn handle_paused(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    run_id: &str,
    reason: &str,
    op_emitter: Option<&OpEventEmitter>,
    counts: TerminalCounts,
) {
    let at = Timestamp::now_iso();
    let counts = cumulative_counts(pool, plan_id, &counts).await;

    // `pause_run`'s last argument is the count of items still to run. It comes
    // from the item rows rather than `plans.items_pending`, which lags a skip
    // the run never reached (`skip_pending_item`) and would inflate this
    // Tier-1 record by one item per such skip.
    let items_pending = match apply_repo::list_pending_items(pool, plan_id).await {
        Ok(ids) => i64::try_from(ids.len()).unwrap_or(i64::MAX),
        Err(e) => {
            report_unwritten_state(
                plan_id,
                run_id,
                "paused",
                Some(reason),
                &e.to_string(),
                op_emitter,
                &at,
            );
            return;
        }
    };

    if let Err(e) = apply_repo::pause_run(
        pool,
        plan_id,
        run_id,
        reason,
        counts.succeeded,
        counts.failed,
        counts.skipped,
        counts.cancelled,
        items_pending,
    )
    .await
    {
        report_unwritten_state(
            plan_id,
            run_id,
            "paused",
            Some(reason),
            &e.to_string(),
            op_emitter,
            &at,
        );
        return;
    }

    let _ = apply_repo::append_event(
        pool,
        &new_id(),
        run_id,
        plan_id,
        None,
        "applying",
        "paused",
        &at,
        None,
        None,
    )
    .await;

    if let Some(emitter) = op_emitter.as_ref() {
        let handle = emitter.handle(OperationStatus::Running);
        emitter.emit(
            OperationEventType::Warning,
            json!({
                "handle": handle,
                "planId": plan_id,
                "runId": run_id,
                "pauseReason": reason,
                "at": at,
            }),
        );
    }

    let _ = bus
        .publish(
            TOPIC_PLAN_APPLYING_PAUSED,
            Source::System,
            PlanApplyingPaused {
                plan_id: plan_id.to_owned(),
                run_id: run_id.to_owned(),
                pause_reason: reason.to_owned(),
                at,
            },
        )
        .await;
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Batch-cancel pending items with retry-once resilience.
#[expect(
    clippy::cognitive_complexity,
    reason = "per-pending-item cancel loop with independent audit emission"
)]
async fn cancel_pending_items(
    pool: &SqlitePool,
    bus: &EventBus,
    run_id: &str,
    plan_id: &str,
    at: &str,
) {
    match apply_repo::list_pending_items(pool, plan_id).await {
        Ok(pending_ids) => {
            let _ = apply_repo::batch_cancel_pending_items(pool, plan_id).await;
            for item_id in &pending_ids {
                audit_item_cancelled(pool, bus, run_id, plan_id, item_id, "pending", at).await;
            }
        }
        Err(e) => {
            tracing::error!(error=%e, "failed to list pending items for per-item cancel audit; retrying once");
            match apply_repo::list_pending_items(pool, plan_id).await {
                Ok(pending_ids) => {
                    let _ = apply_repo::batch_cancel_pending_items(pool, plan_id).await;
                    for item_id in &pending_ids {
                        audit_item_cancelled(pool, bus, run_id, plan_id, item_id, "pending", at)
                            .await;
                    }
                }
                Err(e2) => {
                    tracing::error!(error=%e2, "list_pending_items failed twice; falling back to a single aggregate cancel audit row");
                    let cancelled_count =
                        apply_repo::batch_cancel_pending_items(pool, plan_id).await.unwrap_or(0);
                    let entry = AuditLogEntry::new(
                        EntityType::FilesystemPlan,
                        deterministic_entity_id("plan_apply.bulk_cancel", plan_id),
                        "plan.bulk_cancel_degraded",
                        "user",
                        Outcome::Refused,
                        Severity::Workflow,
                        domain_core::ids::EntityId::new(),
                    )
                    .with_reason_code("list_pending_items_unavailable")
                    .with_payload(json!({
                        "planId": plan_id,
                        "runId": run_id,
                        "cancelledCount": cancelled_count,
                    }));
                    if let Err(e3) = bus
                        .write_audit(
                            entry,
                            TOPIC_PLAN_APPLYING_COMPLETED,
                            Source::System,
                            json!({"planId": plan_id, "cancelledCount": cancelled_count}),
                        )
                        .await
                    {
                        tracing::error!(error=%e3, "durable fallback audit write failed for degraded bulk cancel");
                    }
                }
            }
        }
    }
}
