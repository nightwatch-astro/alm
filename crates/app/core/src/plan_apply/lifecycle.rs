use super::{
    active_runs, apply_repo, audit_item_cancelled, bus_err, check_overlap_and_register,
    compute_plan_path_set, db_err, item_row_to_executor_item, new_id, plans_repo,
    resolve_root_path, spawn_executor_run, ActiveRun, CancellationToken, CasSnapshot,
    ContractError, ErrorCode, ErrorSeverity, EventBus, ExecutorItem, HashMap, PlanApplyStatus,
    PlanApplyingResumed, PlanCancelResponse, PlanItemRetryResponse, PlanItemSkipResponse,
    PlanResumeResponse, RetryQueue, SkipSet, Source, SpawnExecutorParams, SqlitePool, Timestamp,
    Utf8Path, Utf8PathBuf, TOPIC_PLAN_APPLYING_RESUMED,
};

// ── startup sweep ────────────────────────────────────────────────────────────

/// At boot, flip every plan left in state `applying` with no live executor to
/// `paused` (with `pause_reason = 'crash'`), making `resume_plan` available.
///
/// Called once from the desktop app's `boot()` function, before any webview
/// command can be invoked. The `active_runs` registry is always empty at
/// startup, so every `applying` plan qualifies: no live executor will ever
/// advance them.
///
/// Returns the plan ids that were transitioned.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on connection failure (non-fatal at boot —
/// caller should log and continue).
pub async fn sweep_crashed_applying_plans(
    pool: &SqlitePool,
) -> Result<Vec<String>, persistence_core::DbError> {
    apply_repo::sweep_crashed_applying_plans(pool).await
}

/// List plans interrupted by an unclean shutdown (still `applying`, or `paused`
/// with a `crash` reason), for the recovery prompt. Order-independent with
/// respect to the async boot sweep.
///
/// # Errors
///
/// Returns [`persistence_core::DbError`] on connection failure.
pub async fn list_crash_interrupted_plans(
    pool: &SqlitePool,
) -> Result<Vec<String>, persistence_core::DbError> {
    apply_repo::list_crash_interrupted_plans(pool).await
}

// ── cancel_plan ───────────────────────────────────────────────────────────────

/// Cancel an in-flight plan apply (US3, T032).
///
/// Signals the cancellation token; the executor finishes the current item
/// and stops. Remaining pending items are transitioned to `cancelled` by
/// the executor's background task after it observes the cancel signal.
///
/// When the plan is `paused` (no live executor), transitions DB state
/// directly and emits per-item audit rows matching the executor's Cancelled
/// path (GF-5).
///
/// # Errors
///
/// - `plan.not_found` — plan not found.
/// - `plan.not_in_apply` — plan is not in applying or paused state.
#[expect(
    clippy::cognitive_complexity,
    reason = "plan cancel state machine over plan and run status combinations"
)]
pub async fn cancel_plan(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
) -> Result<PlanCancelResponse, ContractError> {
    let plan_row = plans_repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;

    if !matches!(plan_row.state.as_str(), "applying" | "paused") {
        return Err(ContractError::new(
            ErrorCode::PlanNotInApply,
            format!(
                "plan {} is not in applying or paused state; current state is '{}'",
                plan_id, plan_row.state
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Signal cancellation to the running executor (if one is live).
    let has_active_run = {
        let registry = active_runs();
        let entry = registry.get(plan_id);
        if let Some(ref run) = entry {
            run.cancel_token.cancel();
        }
        let found = entry.is_some();
        drop(entry);
        found
    };

    // GF-5: A paused plan has no live ActiveRun (the executor's guard dropped
    // it when it returned Paused). Without a running executor, nothing will
    // ever observe the cancel token — transition DB state directly and emit
    // per-item audit rows matching the executor's Cancelled path.
    if !has_active_run && plan_row.state == "paused" {
        let at = Timestamp::now_iso();
        let run_id = apply_repo::get_active_run(pool, plan_id)
            .await
            .ok()
            .flatten()
            .map_or_else(|| "unknown".to_owned(), |r| r.id);

        // Emit audit rows for pending items before batch-cancelling them so
        // the per-item prior_state is "pending" (matches Cancelled executor
        // path). Mirror apply.rs's Cancelled outcome: list → batch-cancel →
        // per-item audit, with a single retry on list failure.
        let pending_ids = match apply_repo::list_pending_items(pool, plan_id).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "paused-cancel: list_pending_items failed; retrying once"
                );
                apply_repo::list_pending_items(pool, plan_id).await.unwrap_or_default()
            }
        };
        if let Err(e) = apply_repo::batch_cancel_pending_items(pool, plan_id).await {
            tracing::error!(error = %e, "paused-cancel: batch_cancel_pending_items failed");
        }
        for item_id in &pending_ids {
            audit_item_cancelled(pool, bus, &run_id, plan_id, item_id, "pending", &at).await;
        }

        // Sweep orphaned applying items (from retry_plan_item during pause).
        match apply_repo::cancel_orphaned_applying_items(pool, plan_id).await {
            Ok(orphaned) => {
                for item_id in &orphaned {
                    audit_item_cancelled(pool, bus, &run_id, plan_id, item_id, "applying", &at)
                        .await;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "paused-cancel: cancel_orphaned_applying_items failed");
            }
        }

        // Transition plan state to cancelled (TIER-1 durability write).
        if let Err(e) = apply_repo::complete_run(
            pool,
            plan_id,
            &run_id,
            "cancelled",
            plan_row.items_applied,
            plan_row.items_failed,
            plan_row.items_skipped,
            plan_row.items_pending + plan_row.items_cancelled,
        )
        .await
        {
            return Err(db_err(e));
        }

        return Ok(PlanCancelResponse {
            plan_id: plan_id.to_owned(),
            cancelled_at: at,
            items_applied: plan_row.items_applied,
            items_cancelled: plan_row.items_pending,
        });
    }

    let cancelled_at = Timestamp::now_iso();

    Ok(PlanCancelResponse {
        plan_id: plan_id.to_owned(),
        cancelled_at,
        items_applied: plan_row.items_applied,
        items_cancelled: plan_row.items_pending,
    })
}

// ── resume_plan ───────────────────────────────────────────────────────────────

/// Resolve the path to probe when re-validating a `volume.unavailable`/
/// `disk.full` pause for `item` (spec 025 T049/T050).
///
/// `prefer_source` picks which root is tried first: `volume.unavailable`
/// passes `true` (a disconnected drive is far more often the *source* of a
/// move than its destination — probing the destination first let a still-
/// disconnected source pass re-validation whenever the destination happened
/// to be reachable, so resume proceeded and the executor immediately
/// re-paused instead of cleanly refusing); `disk.full` passes `false` (free
/// space is a destination-capacity question). Either way both roots are
/// tried as a fallback if the preferred one isn't resolvable — both are
/// registered roots, which are real, already-existing directories, so no
/// ancestor-walk is needed. Archive destinations are pre-computed absolute
/// paths that may point at a not-yet-created subdirectory (spec 017), so
/// that branch walks up to the nearest existing ancestor before probing.
pub(super) async fn resolve_item_probe_path(
    pool: &SqlitePool,
    item: &plans_repo::PlanItemRow,
    prefer_source: bool,
) -> Option<Utf8PathBuf> {
    let (first, second) = if prefer_source {
        (item.from_root_id.as_deref(), item.to_root_id.as_deref())
    } else {
        (item.to_root_id.as_deref(), item.from_root_id.as_deref())
    };

    for root_id in [first, second].into_iter().flatten() {
        if let Some(root) = resolve_root_path(pool, root_id).await {
            return Some(Utf8PathBuf::from(root));
        }
    }

    if let Some(archive) = item.archive_path.as_deref().filter(|a| !a.is_empty()) {
        let p = Utf8PathBuf::from(archive);
        if p.is_absolute() {
            return Some(nearest_existing_ancestor(&p));
        }
    }
    None
}

/// Walk up from `path` to the nearest ancestor that exists on disk.
/// Returns `path` unchanged if no ancestor (including the filesystem root)
/// exists — the subsequent probe will then fail informatively rather than
/// silently mis-reporting on a bogus path.
pub(super) fn nearest_existing_ancestor(path: &Utf8Path) -> Utf8PathBuf {
    let mut candidate = path.to_path_buf();
    while !candidate.exists() {
        match candidate.parent() {
            Some(parent) if parent != candidate => candidate = parent.to_path_buf(),
            _ => break,
        }
    }
    candidate
}

/// Re-validate the pause condition recorded on a paused run before allowing
/// `resume_plan` to transition it back to `applying` (contract
/// `plan.resume.json`, R-Pause-1/R-Env-1, spec 025 T048-T050).
///
/// `pause_reason` is the code stored on the run row by whichever executor
/// pause path last fired (`item.stale`, `volume.unavailable`, `disk.full`).
/// Each maps to the plan item that triggered it (the executor halts
/// immediately on the first pausing item, so the highest `item_index` among
/// matching items is always the current cause) and re-runs the same check
/// that originally paused the run against that item's current on-disk
/// state.
///
/// Returns `Ok(())` when the condition is resolved, when no matching item
/// can be found (nothing left to re-validate against), or when
/// `pause_reason` is `None`/unrecognized (permissive — v1 only classifies
/// these three R-Pause-1 conditions).
///
/// # Errors
///
/// Returns `ContractError` with `item.still.stale`, `volume.still.unavailable`,
/// or `disk.still.full` when the corresponding condition still holds.
pub(super) async fn revalidate_pause_condition(
    pool: &SqlitePool,
    plan_id: &str,
    pause_reason: Option<&str>,
) -> Result<(), ContractError> {
    let Some(reason) = pause_reason else { return Ok(()) };

    match reason {
        "item.stale" => {
            let Some(item) =
                apply_repo::get_last_stale_item(pool, plan_id).await.map_err(db_err)?
            else {
                return Ok(());
            };
            let Some(root_id) = item.from_root_id.as_deref() else { return Ok(()) };
            let Some(root) = resolve_root_path(pool, root_id).await else { return Ok(()) };
            let abs = Utf8PathBuf::from(root).join(&item.from_relative_path);
            let snapshot = CasSnapshot {
                approved_mtime: item.approved_mtime.clone(),
                approved_size_bytes: item.approved_size_bytes,
            };
            fs_executor::ops::check_cas(&abs, &snapshot).map_err(|failure| {
                ContractError::new(
                    ErrorCode::ItemStillStale,
                    format!("item {} in plan {plan_id} is still stale: {failure}", item.id),
                    ErrorSeverity::Blocking,
                    false,
                )
            })
        }
        "volume.unavailable" => {
            let Some(item) =
                apply_repo::get_last_item_with_failure_prefix(pool, plan_id, "volume.unavailable")
                    .await
                    .map_err(db_err)?
            else {
                return Ok(());
            };
            let Some(probe_path) = resolve_item_probe_path(pool, &item, true).await else {
                return Ok(());
            };
            fs_executor::ops::recheck_volume_available(&probe_path).map_err(|failure| {
                ContractError::new(
                    ErrorCode::VolumeStillUnavailable,
                    format!("plan {plan_id}'s volume is still unavailable: {failure}"),
                    ErrorSeverity::Blocking,
                    false,
                )
            })
        }
        "disk.full" => {
            let Some(item) =
                apply_repo::get_last_item_with_failure_prefix(pool, plan_id, "disk.full")
                    .await
                    .map_err(db_err)?
            else {
                return Ok(());
            };
            let Some(probe_path) = resolve_item_probe_path(pool, &item, false).await else {
                return Ok(());
            };
            let required_bytes = u64::try_from(item.approved_size_bytes.unwrap_or(0)).unwrap_or(0);
            fs_executor::ops::recheck_disk_space(&probe_path, required_bytes).map_err(|failure| {
                ContractError::new(
                    ErrorCode::DiskStillFull,
                    format!("plan {plan_id}'s destination volume is still full: {failure}"),
                    ErrorSeverity::Blocking,
                    false,
                )
            })
        }
        // v1 only classifies the three R-Pause-1 conditions above; any other
        // recorded reason (or none) has nothing to re-validate against.
        _ => Ok(()),
    }
}

/// Resume a paused plan apply run (R-Pause-1, T052).
///
/// Re-validates the pause condition recorded on the run
/// (`revalidate_pause_condition`) before transitioning back to
/// `applying`. If the condition is still present, resume is refused with
/// the matching `*.still.*` code and the plan stays `paused` — it is never
/// silently flipped to `applying` for a run that would immediately stall
/// again (constitution §II, issue #575).
///
/// On success, re-registers an `ActiveRun` (R-Concur-1) and re-spawns the
/// executor (via `spawn_executor_run`) over the plan's remaining
/// `pending` items. Items already `failed` when the run paused — including
/// the one that triggered the pause — stay terminal for this run; per-item
/// retry is a separate affordance (`retry_plan_item`), not part of resume.
///
/// # Errors
///
/// - `plan.not_found` — plan not found.
/// - `run.not_paused` — plan is not in paused state.
/// - `run.not_found` — no active run recorded, or `run_id` does not match it.
/// - `item.still.stale` / `volume.still.unavailable` / `disk.still.full` —
///   the pause condition has not been resolved.
/// - `plan.conflict.overlap` — another active run now claims an overlapping
///   path (FR-017, R-Concur-1) — rare, since the plan's own claim lapsed
///   while paused.
pub async fn resume_plan(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    run_id: &str,
) -> Result<PlanResumeResponse, ContractError> {
    let plan_row = plans_repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;

    if plan_row.state != "paused" {
        return Err(ContractError::new(
            ErrorCode::RunNotPaused,
            format!("plan {} is not paused; current state is '{}'", plan_id, plan_row.state),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Re-validate run id is the active run.
    let active_run_row = apply_repo::get_active_run(pool, plan_id).await.map_err(db_err)?;
    let active_run_row = active_run_row.ok_or_else(|| {
        ContractError::new(
            ErrorCode::RunNotFound,
            format!("no active run found for plan {plan_id}"),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    if active_run_row.id != run_id {
        return Err(ContractError::new(
            ErrorCode::RunNotFound,
            format!("run {run_id} is not the active run for plan {plan_id}"),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // R-Pause-1 / R-Env-1: refuse resume while the pause condition persists.
    revalidate_pause_condition(pool, plan_id, active_run_row.pause_reason.as_deref()).await?;

    // Load the plan's remaining pending items and rebuild the executor's
    // root map (mirrors apply_plan's item preparation, T023a).
    let item_rows = plans_repo::list_plan_items(pool, plan_id).await.map_err(db_err)?;

    let mut root_map: HashMap<String, Utf8PathBuf> = HashMap::new();
    for row in &item_rows {
        for rid in [row.from_root_id.as_ref(), row.to_root_id.as_ref()].into_iter().flatten() {
            if root_map.contains_key(rid) {
                continue;
            }
            if let Some(path) = resolve_root_path(pool, rid).await {
                root_map.insert(rid.clone(), Utf8PathBuf::from(path));
            }
        }
    }

    // `pending` items are the genuine remaining work; `failed` items are
    // included too (not for re-execution — `execute_plan`'s forward loop
    // treats "failed" as an already-terminal state and skips it as a no-op)
    // so that `id -> ExecutorItem` lookup is complete for the resumed run.
    // Without this, `retry_plan_item` can flip a pre-pause-failed item's DB
    // row `failed -> applying` and queue it, but the executor's retry-drain
    // (`item_by_id` in `fs_executor::run::execute_plan`) has no entry for
    // it — the item is then permanently stuck `applying` with no terminal
    // audit record (review fix, resume+retry item-set mismatch).
    // succeeded/skipped/cancelled items are still excluded: they are truly
    // terminal and never eligible for retry.
    let executor_items: Vec<ExecutorItem> = item_rows
        .iter()
        .filter(|r| matches!(r.item_state.as_str(), "pending" | "failed"))
        .map(|r| item_row_to_executor_item(r, &root_map, &plan_row.destructive_destination))
        .collect();

    // Re-register the ActiveRun (R-Concur-1). A paused run has no registry
    // entry — the original spawned task's ActiveRunGuard already dropped it
    // when execute_plan returned Paused — so resume must reclaim the plan's
    // full claimed path set (all items, not just the remaining pending
    // ones: the plan still owns its whole footprint for the run's
    // duration) before the executor can run again. Cancel/skip/retry state
    // does not carry over a pause boundary; a fresh set is correct here.
    let path_set = compute_plan_path_set(&item_rows, &root_map);
    let cancel_token = CancellationToken::new();
    let skip_set = SkipSet::new();
    let retry_queue = RetryQueue::new();
    let run_guard = check_overlap_and_register(
        plan_id,
        ActiveRun {
            cancel_token: cancel_token.clone(),
            skip_set: skip_set.clone(),
            retry_queue: retry_queue.clone(),
            run_id: run_id.to_owned(),
            path_set,
        },
    )?;

    apply_repo::resume_run(pool, plan_id, run_id).await.map_err(db_err)?;

    let resumed_at = Timestamp::now_iso();

    bus.publish(
        TOPIC_PLAN_APPLYING_RESUMED,
        Source::User,
        PlanApplyingResumed {
            plan_id: plan_id.to_owned(),
            run_id: run_id.to_owned(),
            at: resumed_at.clone(),
        },
    )
    .await
    .map_err(bus_err)?;

    let _ = apply_repo::append_event(
        pool,
        &new_id(),
        run_id,
        plan_id,
        None,
        "paused",
        "applying",
        &resumed_at,
        None,
        None,
    )
    .await;

    // Restart the executor over the remaining pending items (issue #575).
    // No live progress-channel caller for resume today (the contract has no
    // `event_sink` parameter) — `op_emitter: None` matches
    // `apply_plan_channel_free`'s no-live-progress mode; the durable audit
    // trail above is unaffected.
    spawn_executor_run(SpawnExecutorParams {
        pool: pool.clone(),
        bus: bus.clone(),
        plan_id: plan_id.to_owned(),
        run_id: run_id.to_owned(),
        executor_items,
        plan_origin: plan_row.origin,
        plan_project_id: plan_row.origin_path,
        cancel_token,
        skip_set,
        retry_queue,
        run_guard,
        op_emitter: None,
    });

    Ok(PlanResumeResponse { plan_id: plan_id.to_owned(), run_id: run_id.to_owned(), resumed_at })
}

// ── skip_plan_item ────────────────────────────────────────────────────────────

/// Skip a pending item within an active apply (US4, T039).
///
/// The item must be `pending` and the plan must be `applying`.
/// The skip is registered in the in-memory SkipSet; the executor picks it
/// up before starting the next item.
///
/// # Errors
///
/// - `plan.not_found` — plan not found.
/// - `plan.not_in_apply` — plan is not applying.
/// - `item.not_found` — item not found.
/// - `item.not_pending` — item is not in pending state.
pub async fn skip_plan_item(
    pool: &SqlitePool,
    plan_id: &str,
    item_id: &str,
) -> Result<PlanItemSkipResponse, ContractError> {
    let plan_row = plans_repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;

    if plan_row.state != "applying" {
        return Err(ContractError::new(
            ErrorCode::PlanNotInApply,
            format!(
                "plan {} is not in applying state; current state is '{}'",
                plan_id, plan_row.state
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Find the item.
    let items = plans_repo::list_plan_items(pool, plan_id).await.map_err(db_err)?;
    let item = items.iter().find(|i| i.id == item_id).ok_or_else(|| {
        ContractError::new(
            ErrorCode::ItemNotFound,
            format!("item {item_id} not found in plan {plan_id}"),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    if item.item_state != "pending" {
        return Err(ContractError::new(
            ErrorCode::ItemNotPending,
            format!("item {} is not pending; current state is '{}'", item_id, item.item_state),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // GF-29: Require a live ActiveRun before injecting the skip — mirrors
    // retry_plan_item's guard. Without this, a skip against a plan whose run
    // already finished fabricates new_state=skipped with nothing to execute it.
    let registry = active_runs();
    let run_ref = registry.get(plan_id).ok_or_else(|| {
        ContractError::new(
            ErrorCode::RunNotFound,
            format!("no active run found for plan {plan_id}; the run may have already finished"),
            ErrorSeverity::Blocking,
            false,
        )
    })?;
    run_ref.skip_set.insert(item_id);
    drop(run_ref);
    drop(registry);

    Ok(PlanItemSkipResponse { item_id: item_id.to_owned(), new_state: "skipped".to_owned() })
}

// ── retry_plan_item ───────────────────────────────────────────────────────────

/// Retry a failed item within an active apply (US4, T040).
///
/// The item must be `failed` and the plan must be `applying`.
/// The item state is reset to `pending` in the database; the executor
/// will re-execute it on the next pass via the retry queue.
///
/// # Errors
///
/// - `plan.not_found` — plan not found.
/// - `plan.not_in_apply` — plan is not applying.
/// - `item.not_found` — item not found.
/// - `item.not_failed` — item is not in failed state.
/// - `run.not_found` — the plan is `applying` in the DB but no `ActiveRun` is
///   registered (the executor task already finished and dropped its
///   registry entry — a narrow race between the run's completion and this
///   call). Rejecting here, before any DB write, avoids flipping the item to
///   `applying` with nothing left to ever re-execute or terminalize it
///   (review fix: an item stuck `applying` forever with no terminal audit
///   record).
pub async fn retry_plan_item(
    pool: &SqlitePool,
    plan_id: &str,
    item_id: &str,
) -> Result<PlanItemRetryResponse, ContractError> {
    let plan_row = plans_repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;

    if plan_row.state != "applying" {
        return Err(ContractError::new(
            ErrorCode::PlanNotInApply,
            format!("plan {plan_id} is not in applying state (use plan.retry for terminal plans)"),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let items = plans_repo::list_plan_items(pool, plan_id).await.map_err(db_err)?;
    let item = items.iter().find(|i| i.id == item_id).ok_or_else(|| {
        ContractError::new(
            ErrorCode::ItemNotFound,
            format!("item {item_id} not found in plan {plan_id}"),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    if item.item_state != "failed" {
        return Err(ContractError::new(
            ErrorCode::ItemNotFailed,
            format!(
                "item {} is not failed; current state is '{}'. \
                 For plan-level retry use plan.retry on a terminal plan.",
                item_id, item.item_state
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Require a live ActiveRun for this plan BEFORE mutating any DB state
    // (see doc comment above). This does not fully eliminate the race (the
    // run could still finish between this check and the DB write below),
    // but it closes the common case: retrying against a plan whose run has
    // already reached a terminal state.
    if !active_runs().contains_key(plan_id) {
        return Err(ContractError::new(
            ErrorCode::RunNotFound,
            format!(
                "no active run found for plan {plan_id}; the run may have already finished. \
                 For plan-level retry use plan.retry on a terminal plan."
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Transition item back to applying in DB (failed → applying).
    apply_repo::item_retry_applying(pool, item_id, plan_id).await.map_err(db_err)?;

    // Enqueue for re-execution, and REFUSE if the run vanished in the meantime.
    //
    // The `active_runs()` check above closes the common case but not the window
    // between it and here. Previously that window was swallowed: `if let
    // Some(run)` simply did nothing when the run was gone, leaving the item
    // `applying` in the DB with nothing that will ever execute it. Run
    // completion then sweeps orphaned `applying` items to `cancelled`
    // (`terminal.rs`), so the user's retry became a silent cancellation --
    // file unmoved, audit trail reading `cancelled`, indistinguishable from a
    // cancellation they asked for. That is a Tier-1 custody failure under
    // constitution II: an approved action recorded terminal that never ran.
    //
    // Losing the race is now reported instead. The DB flip is rolled back
    // first so the item stays `failed` -- retryable, and honest about not
    // having run -- rather than stranded `applying`.
    let enqueued = {
        let registry = active_runs();
        let queued = registry.get(plan_id).is_some_and(|run| {
            run.retry_queue.push(item_id);
            true
        });
        drop(registry);
        queued
    };

    if !enqueued {
        // Best-effort restore. If this fails the item is left `applying` and
        // the completion sweep will cancel it -- the pre-existing behaviour --
        // so log loudly and still return the error: the caller must not be
        // told a retry was accepted when it was not.
        if let Err(e) = apply_repo::item_retry_rollback_to_failed(pool, item_id, plan_id).await {
            tracing::error!(
                error = %e,
                plan_id,
                item_id,
                "retry lost the race with run completion AND could not restore the item to \
                 'failed'; it will be swept to 'cancelled' and must be retried at plan level"
            );
        }
        return Err(ContractError::new(
            ErrorCode::RunNotFound,
            format!(
                "the run for plan {plan_id} finished while the retry of item {item_id} was \
                 being accepted, so nothing re-executed it. The item was not retried. \
                 Use plan.retry on the terminal plan."
            ),
            ErrorSeverity::Blocking,
            true,
        ));
    }

    Ok(PlanItemRetryResponse { item_id: item_id.to_owned(), new_state: "applying".to_owned() })
}

// ── confirm_plan_destructive_items ────────────────────────────────────────────

/// Confirm every destructive (delete/trash) item in a plan, persisting
/// `plan_items.destructive_confirmed = 1` (FR-003, D9, issue #741).
///
/// This is the write half of the executor's destructive-confirm gate
/// (`fs_executor::run::execute_plan`'s `destructive_unconfirmed` refusal,
/// `item_row_to_executor_item`'s `requires_destructive_confirm` derivation):
/// before this function existed, `destructive_confirmed` had no writer
/// anywhere in the codebase, so every delete/trash item was permanently
/// refused at apply time regardless of what the plan's `destructiveDestination`
/// UI showed.
///
/// Plan-level (not per-item): the caller confirms a whole plan's destructive
/// items in one call, matching the existing panel-wide destructive-destination
/// control rather than requiring a per-item id round-trip the frontend DTOs
/// (`InboxPlanAction`, `PlanItemDetail`) don't carry.
///
/// Intentionally permissive on plan state: confirming while a run is
/// `applying`/`paused` cannot retroactively affect an already-snapshotted
/// forward pass, but a `paused` run's remaining `pending` items ARE re-read
/// fresh from the DB on `resume_plan`, so confirming while paused is a
/// legitimate way to unblock a stalled run before resuming.
///
/// Returns the number of items whose `destructive_confirmed` flag flipped
/// (idempotent — already-confirmed items are not re-counted).
///
/// # Errors
///
/// - `plan.not_found` — plan not found.
pub async fn confirm_plan_destructive_items(
    pool: &SqlitePool,
    plan_id: &str,
) -> Result<i64, ContractError> {
    // Existence check only — see docstring for why plan state is not gated.
    plans_repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;

    let confirmed =
        plans_repo::confirm_plan_destructive_items(pool, plan_id).await.map_err(db_err)?;
    i64::try_from(confirmed).map_err(|e| {
        ContractError::new(
            ErrorCode::InternalDatabase,
            format!("confirmed item count overflowed i64: {e}"),
            ErrorSeverity::Fatal,
            true,
        )
    })
}

// ── get_apply_status ──────────────────────────────────────────────────────────

/// Fetch the current apply status for a plan.
///
/// # Errors
///
/// Returns `ContractError` on not-found or database failure.
pub async fn get_apply_status(
    pool: &SqlitePool,
    plan_id: &str,
) -> Result<PlanApplyStatus, ContractError> {
    let plan_row = plans_repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;
    let active_run = apply_repo::get_active_run(pool, plan_id).await.map_err(db_err)?;

    let (run_id, pause_reason) = active_run.map_or((None, None), |r| (Some(r.id), r.pause_reason));

    Ok(PlanApplyStatus {
        plan_id: plan_id.to_owned(),
        run_id,
        plan_state: plan_row.state,
        items_total: plan_row.items_total,
        items_applied: plan_row.items_applied,
        items_failed: plan_row.items_failed,
        items_skipped: plan_row.items_skipped,
        items_cancelled: plan_row.items_cancelled,
        items_pending: plan_row.items_pending,
        pause_reason,
    })
}
