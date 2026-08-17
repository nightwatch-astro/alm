// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! The `execute_plan` forward loop + per-item gate/execute pipeline.

use std::collections::HashMap;

use camino::Utf8PathBuf;
use domain_core::ids::Timestamp;

use crate::failure::{FailureCode, PlanItemFailure, RollbackOutcome};
use crate::ops::cas_check::check_cas;
use crate::ops::path_gate;

use super::dispatch::execute_item;
use super::{
    ApplyOutcome, CancellationToken, ExecutorCallbacks, ExecutorItem, ExecutorItemAction,
    ItemProgressEvent, RetryQueue, SkipSet, TerminalCounts,
};

/// Execute an ordered list of items sequentially.
///
/// Returns `ApplyOutcome` when all items are resolved or a halt condition
/// (cancel / pause) is observed.
///
/// The caller (app/core) is responsible for:
/// - The CAS `approved → applying` transition before calling this.
/// - Batch-cancelling pending items after `Cancelled` is returned.
/// - Calling `pause_run` / `resume_run` on the DB on `Paused`.
/// - Writing the terminal plan state on `Completed`.
#[allow(clippy::too_many_lines)]
#[expect(
    clippy::cognitive_complexity,
    reason = "per-item executor loop with failure taxonomy, cancellation, and retry paths"
)]
pub async fn execute_plan<C: ExecutorCallbacks>(
    items: Vec<ExecutorItem>,
    callbacks: &C,
    cancel: &CancellationToken,
    skip_set: &SkipSet,
    retry_queue: &RetryQueue,
) -> ApplyOutcome {
    let mut counts = TerminalCounts::default();
    let mut cancelled = false;

    // Id-indexed lookup so a retry (which only carries an item id, filed by
    // `retry_plan_item` against an item this loop has already passed) can be
    // re-executed with its original action/paths/CAS-snapshot (issue #742).
    let item_by_id: HashMap<&str, &ExecutorItem> =
        items.iter().map(|i| (i.id.as_str(), i)).collect();

    'items: for item in &items {
        // Skip items that are already in a terminal state (re-apply idempotency).
        if matches!(item.current_state.as_str(), "succeeded" | "skipped" | "cancelled" | "failed") {
            tracing::debug!(item_id = %item.id, state = %item.current_state, "skipping already-terminal item");
            continue;
        }

        // Check cancellation between items (never mid-item).
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }

        // Check user-requested skip.
        if skip_set.take(&item.id) {
            tracing::debug!(item_id = %item.id, "user-skipped item");
            callbacks
                .on_item_progress(ItemProgressEvent::terminal(
                    item.id.clone(),
                    "pending",
                    "skipped",
                    None,
                    None,
                ))
                .await;
            counts.skipped += 1;
            continue;
        }

        match process_single_item(item, callbacks, &mut counts, "pending", true).await {
            ItemOutcome::Pause(reason) => return ApplyOutcome::Paused { reason, counts },
            ItemOutcome::Continue => {}
        }

        // Drain and re-execute any items queued for retry (US4, issue #742).
        // `retry_plan_item` only flips the item's DB row and pushes its id
        // here; nothing previously consumed the queue for real (a single
        // forward pass never revisits an earlier index). Checking between
        // every item — the same boundary already used for cancel/skip —
        // picks up a retry filed against ANY already-passed item, matching
        // this loop's "never mid-item" invariant.
        for retry_id in retry_queue.drain_all() {
            // Check cancellation between retry items too (same "never
            // mid-item" semantics as the forward loop above). Any retry ids
            // already drained but not yet reached here are dropped from the
            // queue without executing — their DB row is already `applying`
            // (flipped eagerly by `retry_plan_item`), so the caller sweeps
            // them via `cancel_orphaned_applying_items` after `Cancelled` is
            // returned (fs_executor has no DB dependency to do so itself).
            if cancel.is_cancelled() {
                cancelled = true;
                break 'items;
            }

            let Some(retry_item) = item_by_id.get(retry_id.as_str()) else {
                tracing::warn!(item_id = %retry_id, "retry queued for unknown item id; ignored");
                continue;
            };
            // `retry_plan_item` already transitioned the DB row `failed ->
            // applying` before queuing, so `on_item_start` (which would
            // double-decrement `items_pending`) must NOT run again, and the
            // gate/terminal events' prior_state is "applying", not "pending".
            match process_single_item(retry_item, callbacks, &mut counts, "applying", false).await {
                ItemOutcome::Pause(reason) => return ApplyOutcome::Paused { reason, counts },
                ItemOutcome::Continue => {}
            }
        }
    }

    if cancelled {
        ApplyOutcome::Cancelled(counts)
    } else {
        ApplyOutcome::Completed(counts)
    }
}

/// Outcome of processing a single item through the gate/execute pipeline.
enum ItemOutcome {
    /// Resolved (terminal or otherwise) — the caller should move on.
    Continue,
    /// A pause condition was hit; the caller should halt the whole run.
    Pause(String),
}

/// Run one item through the destructive-confirm gate, path gate, CAS check,
/// protection check, and (if all pass) the filesystem mutation itself,
/// emitting progress events and updating `counts` throughout.
///
/// Shared by the main forward pass (`emit_start: true`, `gate_prior_state:
/// "pending"`) and mid-run retry re-execution (`emit_start: false`,
/// `gate_prior_state: "applying"` — the DB row is already `applying` via
/// `retry_plan_item`, and calling `on_item_start` again would double-decrement
/// `plans.items_pending`, which the retry path never re-incremented).
#[allow(clippy::too_many_lines)]
async fn process_single_item<C: ExecutorCallbacks>(
    item: &ExecutorItem,
    callbacks: &C,
    counts: &mut TerminalCounts,
    gate_prior_state: &str,
    emit_start: bool,
) -> ItemOutcome {
    // Destructive-confirm gate (FR-003, D9, T020).
    // `requires_destructive_confirm` is derived from the action type (delete/trash),
    // independent of protection status. Replaces the old `confirm_required = is_protected`
    // inversion at plan_apply.rs:199.
    if item.requires_destructive_confirm && !item.destructive_confirmed {
        let failure = PlanItemFailure::with_code(
            FailureCode::DestructiveUnconfirmed,
            format!(
                "item {} requires destructive confirmation (action is destructive); \
                 confirm before applying",
                item.id
            ),
        );
        callbacks
            .on_item_progress(ItemProgressEvent::terminal(
                item.id.clone(),
                gate_prior_state,
                "refused",
                Some(failure),
                Some("destructive_unconfirmed".to_owned()),
            ))
            .await;
        counts.failed += 1;
        return ItemOutcome::Continue;
    }

    // Notify start.
    if emit_start {
        callbacks.on_item_start(&item.id).await;
    }

    // Path-resolution gate (FR-001/002, D8, T018): resolve + validate source path
    // against the library root before any filesystem CAS or mutation.
    if let (Some(ref src_rel), Some(ref root)) = (&item.source_path, &item.library_root) {
        match path_gate::resolve_and_validate(root, src_rel) {
            Err(gate_failure) => {
                let audit_reason = gate_failure.code.as_str().to_owned();
                let triggers_pause = gate_failure.code.triggers_pause();
                callbacks
                    .on_item_progress(ItemProgressEvent::terminal(
                        item.id.clone(),
                        "applying",
                        "refused",
                        Some(gate_failure),
                        Some(audit_reason),
                    ))
                    .await;
                counts.failed += 1;
                if triggers_pause {
                    return ItemOutcome::Pause("path.invalid".to_owned());
                }
                return ItemOutcome::Continue;
            }
            Ok(_resolved) => {
                // Path is safe; the resolved absolute path will be used by execute_item.
            }
        }
    }

    // Per-item FS CAS revalidation (R-FS-1).
    // Use the library-root-resolved path if available; otherwise use the raw path (legacy).
    let resolved_source_for_cas: Option<Utf8PathBuf> =
        if let (Some(ref src_rel), Some(ref root)) = (&item.source_path, &item.library_root) {
            // Already validated above; re-resolve (cheap lexical op).
            path_gate::resolve_and_validate(root, src_rel).ok().map(|r| r.0)
        } else {
            item.source_path.clone()
        };

    if let Some(ref src) = resolved_source_for_cas {
        if let Err(stale_failure) = check_cas(src, &item.cas_snapshot) {
            let triggers_pause = stale_failure.code.triggers_pause();
            let failure_clone = stale_failure.clone();

            callbacks
                .on_item_progress(ItemProgressEvent::terminal(
                    item.id.clone(),
                    "applying",
                    "stale",
                    Some(failure_clone),
                    Some("stale".to_owned()),
                ))
                .await;

            counts.failed += 1;

            if triggers_pause {
                return ItemOutcome::Pause(stale_failure.code.as_str().to_owned());
            }
            return ItemOutcome::Continue;
        }
    }

    // Protection check (FR-008).
    if item.is_protected
        && !matches!(item.action, ExecutorItemAction::NoOp | ExecutorItemAction::Catalogue)
    {
        let failure = PlanItemFailure::with_code(
            FailureCode::ProtectedSource,
            format!("item {} is protected by source policy", item.id),
        );
        callbacks
            .on_item_progress(ItemProgressEvent::terminal(
                item.id.clone(),
                "applying",
                "failed",
                Some(failure),
                Some("protected".to_owned()),
            ))
            .await;
        counts.failed += 1;
        return ItemOutcome::Continue;
    }

    // Execute the operation.
    //
    // T212: the filesystem primitives in `execute_item` are synchronous and
    // blocking (`std::fs::rename`/`copy`/`remove_file`, trash). Running them
    // directly on a tokio worker thread would stall the async runtime, so we
    // hand the work to `spawn_blocking`, which dispatches it onto the
    // dedicated blocking thread pool and yields the worker thread back to the
    // runtime until the fs op completes.
    let item_for_blocking = item.clone();
    let op_result = tokio::task::spawn_blocking(move || execute_item(&item_for_blocking))
        .await
        .unwrap_or_else(|join_err| {
            // The blocking task panicked. Surface it as an internal failure
            // rather than propagating the panic through the executor loop.
            Err((
                PlanItemFailure::with_code(
                    FailureCode::Unknown,
                    format!("filesystem worker task failed: {join_err}"),
                ),
                false,
                RollbackOutcome::NotApplicable,
                None,
            ))
        });

    match op_result {
        Ok(()) => {
            callbacks
                .on_item_progress(ItemProgressEvent::terminal(
                    item.id.clone(),
                    "applying",
                    "succeeded",
                    None,
                    None,
                ))
                .await;
            counts.succeeded += 1;
            ItemOutcome::Continue
        }
        Err((failure, rollback_attempted, rollback_outcome, rollback_message)) => {
            let triggers_pause = failure.code.triggers_pause();
            let failure_clone = failure.clone();

            callbacks
                .on_item_progress(ItemProgressEvent {
                    item_id: item.id.clone(),
                    prior_state: "applying".to_owned(),
                    new_state: "failed".to_owned(),
                    at: Timestamp::now_iso(),
                    failure: Some(failure_clone),
                    rollback_attempted,
                    rollback_outcome,
                    rollback_message,
                    audit_reason: None,
                })
                .await;
            counts.failed += 1;

            if triggers_pause {
                return ItemOutcome::Pause(failure.code.as_str().to_owned());
            }
            ItemOutcome::Continue
        }
    }
}
