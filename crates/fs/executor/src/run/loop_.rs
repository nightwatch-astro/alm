// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! The `execute_plan` forward loop + per-item gate/execute pipeline.

use std::collections::HashMap;

use camino::Utf8PathBuf;
use domain_core::ids::Timestamp;

use crate::failure::{FailureCode, PlanItemFailure, RollbackOutcome};
use crate::ops::cas_check::check_cas;
use crate::ops::path_gate;

use super::dispatch::{execute_item, ResolvedItemPaths};
use super::{
    ApplyOutcome, CancellationToken, ExecutorCallbacks, ExecutorItem, ExecutorItemAction,
    ItemProgressEvent, RetryQueue, SkipSet, TerminalCounts,
};

/// Execute an ordered list of items sequentially.
///
/// Returns `ApplyOutcome` when all items are resolved or a halt condition
/// (cancel / pause) is observed.
///
/// Owns `retry_queue`'s lifecycle: the queue is always closed by the time this
/// returns, so a `retry_plan_item` call arriving after this run stopped
/// draining is refused rather than accepted into a queue nobody will read
/// (astro-plan-ts1z). Any id accepted but not executed is reported by
/// [`RetryQueue::take_orphaned`] for the caller to restore.
///
/// The caller (app/core) is responsible for:
/// - The CAS `approved → applying` transition before calling this.
/// - Batch-cancelling pending items after `Cancelled` is returned.
/// - Calling `pause_run` / `resume_run` on the DB on `Paused`.
/// - Writing the terminal plan state on `Completed`.
/// - Restoring the database rows of [`RetryQueue::take_orphaned`] ids.
pub async fn execute_plan<C: ExecutorCallbacks>(
    items: Vec<ExecutorItem>,
    callbacks: &C,
    cancel: &CancellationToken,
    skip_set: &SkipSet,
    retry_queue: &RetryQueue,
) -> ApplyOutcome {
    let outcome = drive_plan(items, callbacks, cancel, skip_set, retry_queue).await;
    // Idempotent: the `Completed` path already closed the queue jointly with
    // its final drain. This covers the cancel and pause exits, where remaining
    // ids become orphans instead.
    retry_queue.close();
    outcome
}

#[allow(clippy::too_many_lines)]
// `allow` rather than `expect`: the loop crosses the cognitive-complexity
// threshold only when `test-pacing` compiles the extra branch, so an `expect`
// goes unfulfilled in the default feature set.
#[allow(clippy::cognitive_complexity)]
async fn drive_plan<C: ExecutorCallbacks>(
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

        #[cfg(feature = "test-pacing")]
        pacing::gate_item(&item.plan_id).await;

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
        match drain_retries(retry_queue, &item_by_id, callbacks, &mut counts, cancel).await {
            DrainOutcome::Continue => {}
            DrainOutcome::Cancelled => {
                cancelled = true;
                break 'items;
            }
            DrainOutcome::Pause(reason) => return ApplyOutcome::Paused { reason, counts },
        }
    }

    if cancelled {
        return ApplyOutcome::Cancelled(counts);
    }

    // Close the queue jointly with a final drain. A retry pushed at any point
    // before the close is executed here; one arriving after is refused at
    // `push`. There is no interval in which an accepted retry has no executor
    // (astro-plan-ts1z windows b and c).
    loop {
        if retry_queue.close_if_empty() {
            break;
        }
        match drain_retries(retry_queue, &item_by_id, callbacks, &mut counts, cancel).await {
            DrainOutcome::Continue => {}
            DrainOutcome::Cancelled => return ApplyOutcome::Cancelled(counts),
            DrainOutcome::Pause(reason) => return ApplyOutcome::Paused { reason, counts },
        }
    }

    ApplyOutcome::Completed(counts)
}

/// Path-resolution gate (FR-001/002, D8, T018): resolve and validate both sides
/// of an item once, before any filesystem CAS or mutation.
///
/// The resolved paths are what `dispatch::execute_item` operates on. This is the
/// only place in the executor that chooses a root or performs a join: a second
/// resolution downstream is how a destination gated against `destination_root`
/// reached `mkdir`/`link`/`move` joined onto `library_root` instead
/// (astro-plan-3v3r.1.12). `destination_root` still takes precedence over
/// `library_root` (#765), and the fallback now exists exactly once.
///
/// A side with no root is refused unless it carries an absolute, already-normal
/// path, which is the legacy mode for items storing a pre-resolved destination
/// (`fs_pathsafe::contain::resolve_unrooted`).
fn resolve_item_paths(item: &ExecutorItem) -> Result<ResolvedItemPaths, PlanItemFailure> {
    Ok(ResolvedItemPaths {
        source: resolve_side(item.source_path.as_deref(), item.library_root.as_deref())?,
        destination: resolve_side(
            item.destination_path.as_deref(),
            item.destination_root.as_deref().or(item.library_root.as_deref()),
        )?,
    })
}

fn resolve_side(
    path: Option<&camino::Utf8Path>,
    root: Option<&camino::Utf8Path>,
) -> Result<Option<Utf8PathBuf>, PlanItemFailure> {
    match (path, root) {
        (None, _) => Ok(None),
        (Some(path), Some(root)) => path_gate::resolve_and_validate(root, path).map(|r| Some(r.0)),
        (Some(path), None) => fs_pathsafe::contain::resolve_unrooted_utf8(path)
            .map(Some)
            .map_err(|e| path_gate::containment_failure(&e)),
    }
}

/// Outcome of one retry-queue drain pass.
enum DrainOutcome {
    Continue,
    Cancelled,
    Pause(String),
}

/// Drain the retry queue once and re-execute each drained item.
///
/// Cancellation is checked between retry items (same "never mid-item"
/// semantics as the forward loop). A halt part-way through the batch hands the
/// unprocessed remainder back as orphans: `drain_all` already removed them from
/// the queue, and their database row is `applying`, so the caller must be able
/// to see them (`fs_executor` has no database dependency to restore them
/// itself). An id with no matching item is also an orphan — nothing in this run
/// can execute it.
async fn drain_retries<C: ExecutorCallbacks>(
    retry_queue: &RetryQueue,
    item_by_id: &HashMap<&str, &ExecutorItem>,
    callbacks: &C,
    counts: &mut TerminalCounts,
    cancel: &CancellationToken,
) -> DrainOutcome {
    let drained = retry_queue.drain_all();
    let mut remaining = drained.iter();

    while let Some(retry_id) = remaining.next() {
        if cancel.is_cancelled() {
            retry_queue.orphan(std::iter::once(retry_id.clone()).chain(remaining.cloned()));
            return DrainOutcome::Cancelled;
        }

        let Some(retry_item) = item_by_id.get(retry_id.as_str()) else {
            tracing::warn!(item_id = %retry_id, "retry queued for unknown item id; ignored");
            retry_queue.orphan(std::iter::once(retry_id.clone()));
            continue;
        };
        // `retry_plan_item` already transitioned the DB row `failed ->
        // applying` before queuing, so `on_item_start` (which would
        // double-decrement `items_pending`) must NOT run again, and the
        // gate/terminal events' prior_state is "applying", not "pending".
        match process_single_item(retry_item, callbacks, counts, "applying", false).await {
            ItemOutcome::Pause(reason) => {
                retry_queue.orphan(remaining.cloned());
                return DrainOutcome::Pause(reason);
            }
            ItemOutcome::Continue => {}
        }
    }
    DrainOutcome::Continue
}

/// Per-item gate for tests that need the forward pass to still be running when
/// they act on it.
///
/// A test that files a mid-run retry, pause, or cancel has to reach the
/// executor before the forward pass drains its queue. Without a seam the only
/// lever is item count, which makes the outcome a race: 30 small
/// same-directory moves against a warm pool finish inside three awaits often
/// enough to fail in CI (`astro-plan-ytx7`).
///
/// The gate parks the loop at each non-terminal item boundary and publishes the
/// arrival, so a test waits for an ordering it observes rather than for a
/// duration it guessed.
///
/// Keyed by plan id, because integration tests share one process: an un-scoped
/// registry lets a concurrent test's executor consume the arrival and release
/// permits the gating test issued for its own run.
///
/// The whole module is behind the `test-pacing` feature, which is off by
/// default and reachable only through a `[dev-dependencies]` edge, so no
/// release binary contains a way to stall a real plan apply.
///
/// A static rather than a parameter of [`execute_plan`]: the production call
/// signature carries no test-only handle.
#[cfg(feature = "test-pacing")]
pub mod pacing {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
    use std::time::Duration;

    use tokio::sync::Semaphore;

    /// Bounds a wait whose ordering is already supposed to hold. Exceeding it
    /// means the loop never reached the boundary, which a panic reports as a
    /// test failure instead of a hang. Never used to establish ordering, so it
    /// only has to exceed the slowest legitimate arrival.
    const ARRIVAL_TIMEOUT: Duration = Duration::from_secs(30);

    struct GateState {
        /// Gains a permit each time the loop parks at an item boundary.
        arrived: Semaphore,
        /// Loses a permit each time the loop leaves an item boundary. Closed by
        /// [`ItemGate`]'s drop, which releases every later boundary at once.
        release: Semaphore,
    }

    static GATES: Mutex<Option<HashMap<String, Arc<GateState>>>> = Mutex::new(None);

    /// Recovers from poisoning: a test panicking while the loop is parked must
    /// still leave the registry usable, and a panicking `Drop` would abort.
    fn registry() -> MutexGuard<'static, Option<HashMap<String, Arc<GateState>>>> {
        GATES.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Park the forward pass until the gate installed for `plan_id` releases
    /// this item.
    ///
    /// A no-op when no [`ItemGate`] is installed for this plan, which is every
    /// run other than the one a gating test drives.
    pub(crate) async fn gate_item(plan_id: &str) {
        let Some(gate) = registry().as_ref().and_then(|g| g.get(plan_id).cloned()) else {
            return;
        };
        gate.arrived.add_permits(1);
        // Bound to a local declared after `gate` so the permit's borrow of it
        // ends before `gate` itself drops.
        let released = gate.release.acquire().await;
        // `Err` only after `ItemGate`'s drop closed the semaphore, which is the
        // signal to run the rest of the plan ungated.
        if let Ok(permit) = released {
            permit.forget();
        }
    }

    /// An installed gate. Dropping it releases every remaining boundary,
    /// including on a panic unwind, so a failed assertion cannot strand a
    /// spawned executor.
    pub struct ItemGate {
        plan_id: String,
        state: Arc<GateState>,
    }

    impl ItemGate {
        /// Gate every item boundary `plan_id`'s run reaches from now on. Runs of
        /// other plans are unaffected.
        ///
        /// # Panics
        /// If a gate is already installed for `plan_id`.
        #[must_use]
        pub fn install(plan_id: &str) -> Self {
            let state =
                Arc::new(GateState { arrived: Semaphore::new(0), release: Semaphore::new(0) });
            let prior = registry()
                .get_or_insert_with(HashMap::new)
                .insert(plan_id.to_owned(), Arc::clone(&state));
            assert!(prior.is_none(), "a gate is already installed for plan {plan_id}");
            Self { plan_id: plan_id.to_owned(), state }
        }

        /// Wait until the forward pass parks at an item boundary.
        ///
        /// Returning proves a run is inside its forward loop, so its retry
        /// queue is still open and a mid-run retry, pause, or cancel lands
        /// there rather than after the run closed it.
        ///
        /// # Panics
        /// If no boundary is reached within 30 seconds, meaning the run never
        /// started or already ended.
        pub async fn wait_for_arrival(&self) {
            tokio::time::timeout(ARRIVAL_TIMEOUT, self.state.arrived.acquire())
                .await
                .expect("executor reached no item boundary: the run never started or already ended")
                .expect("the arrival semaphore is never closed")
                .forget();
        }

        /// Let the forward pass leave `n` further item boundaries.
        pub fn release(&self, n: usize) {
            self.state.release.add_permits(n);
        }
    }

    impl Drop for ItemGate {
        fn drop(&mut self) {
            self.state.release.close();
            if let Some(gates) = registry().as_mut() {
                gates.remove(&self.plan_id);
            }
        }
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

    let resolved_paths = match resolve_item_paths(item) {
        Ok(paths) => paths,
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
    };

    // Per-item FS CAS revalidation (R-FS-1) against the same resolved path the
    // mutation will use, so the snapshot cannot be checked on a different file.
    if let Some(ref src) = resolved_paths.source {
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
    let op_result =
        tokio::task::spawn_blocking(move || execute_item(&item_for_blocking, &resolved_paths))
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
