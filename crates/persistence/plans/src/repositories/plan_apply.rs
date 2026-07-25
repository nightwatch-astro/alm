// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for plan apply runs and events (spec 025).
//!
//! Operates on the `plan_apply_runs` and `plan_apply_events` tables from
//! migration 0015. The `plan_apply_events` table is append-only; no UPDATE
//! or DELETE is permitted (FR-003, SC-001, data-model.md invariants).
//!
//! Apply-side plan-state transitions (`applying`, `paused`, `applied`,
//! `partially_applied`, `failed`, `cancelled`) are also written here; the
//! review-side state transitions live in `plans.rs`.

use domain_core::ids::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Force the WAL contents into the main database file and fsync it.
///
/// Tier-1 durability escalation (constitution v1.1.0 §V): the connection pool
/// runs `synchronous = NORMAL`, under which a WAL commit is durable only to the
/// WAL file, and the WAL is fsynced at checkpoint — not at every commit. So a
/// power loss between an intent commit and the next automatic checkpoint could
/// rewind the just-committed `applying`/run row while the executor has already
/// begun mutating the filesystem, violating "intent committed before the fs
/// action." Calling `wal_checkpoint(TRUNCATE)` immediately after the intent
/// commit syncs the database file, closing that window with native SQLite
/// durability rather than a userspace write-ahead journal.
///
/// A failed checkpoint (e.g. a reader holding the WAL open) is not fatal: the
/// intent is already committed to the WAL, boot reconciliation is the backstop,
/// and the next automatic checkpoint will sync it. Callers log and proceed.
async fn checkpoint_intent(conn: &mut SqliteConnection) -> DbResult<()> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(conn).await?;
    Ok(())
}

// ── Row types ─────────────────────────────────────────────────────────────────

/// Row returned from the `plan_apply_runs` table.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PlanApplyRunRow {
    pub id: String,
    pub plan_id: String,
    pub approval_token: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub terminal_state: Option<String>,
    pub items_total: i64,
    pub items_applied: i64,
    pub items_failed: i64,
    pub items_skipped: i64,
    pub items_cancelled: i64,
    pub items_pending: i64,
    pub pause_reason: Option<String>,
}

/// Row returned from the `plan_apply_events` table.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PlanApplyEventRow {
    pub id: String,
    pub run_id: String,
    pub plan_id: String,
    pub item_id: Option<String>,
    pub prior_state: String,
    pub new_state: String,
    pub at: String,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
    pub failure_recoverable: Option<i64>,
    pub rollback_attempted: Option<i64>,
    pub rollback_outcome: Option<String>,
    pub rollback_message: Option<String>,
}

/// Failure detail to store with an event.
#[derive(Clone, Debug)]
pub struct EventFailure<'a> {
    pub code: &'a str,
    pub message: &'a str,
    pub recoverable: bool,
}

/// Rollback detail to store with an event.
#[derive(Clone, Debug)]
pub struct EventRollback<'a> {
    pub attempted: bool,
    pub outcome: &'a str,
    pub message: Option<&'a str>,
}

// ── CAS transition: approved → applying ──────────────────────────────────────

/// Atomically transition a plan from `approved` to `applying` (CAS, R-CAS-1).
/// Creates the `plan_apply_runs` row in the same SQLite transaction.
///
/// Returns the run id on success.
///
/// # Errors
///
/// - `DbError::CasFailed` if the plan state was not `approved` at transition
///   time (concurrent apply race or invalid state).
/// - `DbError::NotFound` if the plan does not exist.
/// - `DbError::Database` on connection failure.
pub async fn cas_approved_to_applying(
    pool: &SqlitePool,
    plan_id: &str,
    run_id: &str,
    approval_token: &str,
    items_total: i64,
    items_pending: i64,
) -> DbResult<()> {
    let now = Timestamp::now_iso();

    // Use a transaction so the CAS + run row insertion are atomic.
    let mut tx = pool.begin().await?;

    // Attempt atomic CAS: only update if current state is 'approved'.
    let rows = sqlx::query(
        "UPDATE plans SET state = 'applying' \
         WHERE id = ? AND state = 'approved'",
    )
    .bind(plan_id)
    .execute(&mut *tx)
    .await?;

    if rows.rows_affected() == 0 {
        // Either not found or not in approved state. Check which.
        let exists: Option<String> = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
            .bind(plan_id)
            .fetch_optional(&mut *tx)
            .await?;
        return match exists {
            None => Err(DbError::NotFound(format!("plan {plan_id}"))),
            Some(state) => Err(DbError::CasFailed(format!(
                "plan {plan_id} expected state 'approved', found '{state}'"
            ))),
        };
    }

    // Create the mandatory PlanApplyRun row (R-Run-1).
    sqlx::query(
        "INSERT INTO plan_apply_runs \
         (id, plan_id, approval_token, started_at, items_total, items_pending) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind(plan_id)
    .bind(approval_token)
    .bind(&now)
    .bind(items_total)
    .bind(items_pending)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Tier-1: sync the just-committed intent before the executor touches the
    // filesystem (see `checkpoint_intent`). Best-effort: a failed checkpoint
    // leaves the intent durable in the WAL, and boot reconciliation is the
    // backstop, so the error is intentionally swallowed (the persistence layer
    // carries no logging facade).
    let mut conn = pool.acquire().await?;
    let _ = checkpoint_intent(&mut conn).await;

    Ok(())
}

// ── Run state updates ─────────────────────────────────────────────────────────

/// Transition plan to terminal state and finalise the run row.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn complete_run(
    pool: &SqlitePool,
    plan_id: &str,
    run_id: &str,
    terminal_state: &str,
    items_applied: i64,
    items_failed: i64,
    items_skipped: i64,
    items_cancelled: i64,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    let items_pending = 0i64;

    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE plans SET \
           state = ?, \
           items_applied = ?, items_failed = ?, items_skipped = ?, \
           items_cancelled = ?, items_pending = ? \
         WHERE id = ?",
    )
    .bind(terminal_state)
    .bind(items_applied)
    .bind(items_failed)
    .bind(items_skipped)
    .bind(items_cancelled)
    .bind(items_pending)
    .bind(plan_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE plan_apply_runs SET \
           ended_at = ?, terminal_state = ?, \
           items_applied = ?, items_failed = ?, items_skipped = ?, \
           items_cancelled = ?, items_pending = 0 \
         WHERE id = ?",
    )
    .bind(&now)
    .bind(terminal_state)
    .bind(items_applied)
    .bind(items_failed)
    .bind(items_skipped)
    .bind(items_cancelled)
    .bind(run_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Pause a run (R-Pause-1): transition plan → paused, update run row.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
#[allow(clippy::too_many_arguments)]
pub async fn pause_run(
    pool: &SqlitePool,
    plan_id: &str,
    run_id: &str,
    pause_reason: &str,
    items_applied: i64,
    items_failed: i64,
    items_skipped: i64,
    items_cancelled: i64,
    items_pending: i64,
) -> DbResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("UPDATE plans SET state = 'paused' WHERE id = ?")
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "UPDATE plan_apply_runs SET \
           terminal_state = 'paused', pause_reason = ?, \
           items_applied = ?, items_failed = ?, items_skipped = ?, \
           items_cancelled = ?, items_pending = ? \
         WHERE id = ?",
    )
    .bind(pause_reason)
    .bind(items_applied)
    .bind(items_failed)
    .bind(items_skipped)
    .bind(items_cancelled)
    .bind(items_pending)
    .bind(run_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Resume a paused run: transition plan → applying, clear pause state.
///
/// # Errors
///
/// Returns [`DbError::CasFailed`] if plan is not in `paused` state.
/// Returns [`DbError::Database`] on connection failure.
pub async fn resume_run(pool: &SqlitePool, plan_id: &str, run_id: &str) -> DbResult<()> {
    let mut tx = pool.begin().await?;

    let rows = sqlx::query("UPDATE plans SET state = 'applying' WHERE id = ? AND state = 'paused'")
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;

    if rows.rows_affected() == 0 {
        let state: Option<String> = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
            .bind(plan_id)
            .fetch_optional(&mut *tx)
            .await?;
        return match state {
            None => Err(DbError::NotFound(format!("plan {plan_id}"))),
            Some(s) => Err(DbError::CasFailed(format!(
                "plan {plan_id} expected state 'paused', found '{s}'"
            ))),
        };
    }

    sqlx::query(
        "UPDATE plan_apply_runs SET terminal_state = NULL, pause_reason = NULL WHERE id = ?",
    )
    .bind(run_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Tier-1: resume re-enters `applying` and the executor re-runs remaining
    // items, so the same intent-durability guarantee as apply-start applies.
    // Best-effort (see `cas_approved_to_applying`).
    let mut conn = pool.acquire().await?;
    let _ = checkpoint_intent(&mut conn).await;

    Ok(())
}

// ── Per-item state transitions ────────────────────────────────────────────────

/// Transition a failed item back to `applying` (per-item retry within a run).
/// Decrements items_failed.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn item_retry_applying(pool: &SqlitePool, item_id: &str, plan_id: &str) -> DbResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE plan_items SET item_state = 'applying', item_stale = 0, failure_reason = NULL \
         WHERE id = ? AND item_state = 'failed'",
    )
    .bind(item_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE plans SET items_failed = items_failed - 1 WHERE id = ? AND items_failed > 0",
    )
    .bind(plan_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// List the IDs of all `pending` items for a plan.
///
/// Used to emit per-item audit rows before batch-cancelling (FR-005, T021).
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn list_pending_items(pool: &SqlitePool, plan_id: &str) -> DbResult<Vec<String>> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM plan_items WHERE plan_id = ? AND item_state = 'pending' ORDER BY item_index ASC",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await?;
    Ok(ids)
}

/// Batch-transition all `pending` items to `cancelled`; update plan counters.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn batch_cancel_pending_items(pool: &SqlitePool, plan_id: &str) -> DbResult<i64> {
    let mut tx = pool.begin().await?;

    let rows = sqlx::query(
        "UPDATE plan_items SET item_state = 'cancelled' \
         WHERE plan_id = ? AND item_state = 'pending'",
    )
    .bind(plan_id)
    .execute(&mut *tx)
    .await?;

    let cancelled_count = i64::try_from(rows.rows_affected()).unwrap_or(i64::MAX);

    if cancelled_count > 0 {
        sqlx::query(
            "UPDATE plans SET \
             items_cancelled = items_cancelled + ?, \
             items_pending = 0 \
             WHERE id = ?",
        )
        .bind(cancelled_count)
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(cancelled_count)
}

/// Cancel any items left in `applying` when a run ends `Cancelled`.
///
/// Under normal forward-loop execution no item is ever `applying` when
/// `fs_executor::run::execute_plan` returns `Cancelled` — cancellation is
/// checked strictly *between* items, so an item picked up for real always
/// runs to a terminal state first. The one exception is a mid-run retry
/// (`retry_plan_item`): it flips the DB row `failed -> applying` and queues
/// the id *eagerly*, independent of whether the executor's retry-drain loop
/// ever actually re-executes it before observing cancellation. Such an item
/// is invisible to [`batch_cancel_pending_items`] (which only targets
/// `pending`) and would otherwise stay `applying` forever with no terminal
/// audit record (review fix for issues #742/#575 follow-up). Returns the
/// cancelled item ids so the caller can emit a per-item audit row for each.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn cancel_orphaned_applying_items(
    pool: &SqlitePool,
    plan_id: &str,
) -> DbResult<Vec<String>> {
    let mut tx = pool.begin().await?;

    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM plan_items WHERE plan_id = ? AND item_state = 'applying' \
         ORDER BY item_index ASC",
    )
    .bind(plan_id)
    .fetch_all(&mut *tx)
    .await?;

    if !ids.is_empty() {
        sqlx::query(
            "UPDATE plan_items SET item_state = 'cancelled' \
             WHERE plan_id = ? AND item_state = 'applying'",
        )
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;

        let cancelled_count = i64::try_from(ids.len()).unwrap_or(i64::MAX);
        sqlx::query("UPDATE plans SET items_cancelled = items_cancelled + ? WHERE id = ?")
            .bind(cancelled_count)
            .bind(plan_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(ids)
}

// ── Audit event writes ────────────────────────────────────────────────────────

/// Append an audit event row (append-only; no UPDATE/DELETE allowed).
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
#[allow(clippy::too_many_arguments)]
pub async fn append_event(
    pool: &SqlitePool,
    event_id: &str,
    run_id: &str,
    plan_id: &str,
    item_id: Option<&str>,
    prior_state: &str,
    new_state: &str,
    at: &str,
    failure: Option<&EventFailure<'_>>,
    rollback: Option<&EventRollback<'_>>,
) -> DbResult<()> {
    let (fc, fm, fr) = failure.map_or((None, None, None), |f| {
        (Some(f.code), Some(f.message), Some(i64::from(f.recoverable)))
    });

    let (ra, ro, rm) = rollback
        .map_or((None, None, None), |r| (Some(i64::from(r.attempted)), Some(r.outcome), r.message));

    sqlx::query(
        "INSERT INTO plan_apply_events \
         (id, run_id, plan_id, item_id, prior_state, new_state, at, \
          failure_code, failure_message, failure_recoverable, \
          rollback_attempted, rollback_outcome, rollback_message) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event_id)
    .bind(run_id)
    .bind(plan_id)
    .bind(item_id)
    .bind(prior_state)
    .bind(new_state)
    .bind(at)
    .bind(fc)
    .bind(fm)
    .bind(fr)
    .bind(ra)
    .bind(ro)
    .bind(rm)
    .execute(pool)
    .await?;

    Ok(())
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Fetch a plan apply run by id.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no run matches.
/// Returns [`DbError::Database`] on connection failure.
pub async fn get_run(pool: &SqlitePool, run_id: &str) -> DbResult<PlanApplyRunRow> {
    let row: Option<PlanApplyRunRow> = sqlx::query_as("SELECT * FROM plan_apply_runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(pool)
        .await?;

    row.ok_or_else(|| DbError::NotFound(format!("run {run_id}")))
}

/// Fetch the most recent active (paused or applying) run for a plan.
///
/// Returns `None` if no run is in-progress.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn get_active_run(pool: &SqlitePool, plan_id: &str) -> DbResult<Option<PlanApplyRunRow>> {
    Ok(sqlx::query_as(
        "SELECT * FROM plan_apply_runs \
         WHERE plan_id = ? AND (terminal_state IS NULL OR terminal_state = 'paused') \
         ORDER BY started_at DESC LIMIT 1",
    )
    .bind(plan_id)
    .fetch_optional(pool)
    .await?)
}

/// List all apply events for a plan in chronological order.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn list_events(pool: &SqlitePool, plan_id: &str) -> DbResult<Vec<PlanApplyEventRow>> {
    Ok(sqlx::query_as("SELECT * FROM plan_apply_events WHERE plan_id = ? ORDER BY at ASC")
        .bind(plan_id)
        .fetch_all(pool)
        .await?)
}

/// Fetch the plan item whose CAS mismatch most recently triggered a pause
/// (`item_state = 'failed'`, `item_stale = 1`).
///
/// The executor halts immediately on the first item that trips a pause
/// condition (R-Pause-1), so the highest `item_index` among stale items is
/// the one that caused the *current* pause. `resume_plan` re-probes this
/// item's source path before allowing `paused -> applying` (spec 025
/// T048/T049/T050).
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn get_last_stale_item(
    pool: &SqlitePool,
    plan_id: &str,
) -> DbResult<Option<crate::repositories::plans::PlanItemRow>> {
    Ok(sqlx::query_as(
        "SELECT * FROM plan_items WHERE plan_id = ? AND item_stale = 1 \
         ORDER BY item_index DESC LIMIT 1",
    )
    .bind(plan_id)
    .fetch_optional(pool)
    .await?)
}

/// Fetch the plan item whose failure reason most recently began with
/// `code_prefix` (e.g. `"volume.unavailable"`, `"disk.full"`).
///
/// Same "highest `item_index` among matching failures = the item that
/// caused the current pause" reasoning as [`get_last_stale_item`].
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn get_last_item_with_failure_prefix(
    pool: &SqlitePool,
    plan_id: &str,
    code_prefix: &str,
) -> DbResult<Option<crate::repositories::plans::PlanItemRow>> {
    let pattern = format!("{code_prefix}%");
    Ok(sqlx::query_as(
        "SELECT * FROM plan_items WHERE plan_id = ? AND item_state = 'failed' \
         AND failure_reason LIKE ? ORDER BY item_index DESC LIMIT 1",
    )
    .bind(plan_id)
    .bind(pattern)
    .fetch_optional(pool)
    .await?)
}

// ── Batch item flush (group-commit writer) ────────────────────────────────────

/// One item's terminal state to persist in a flush batch.
#[derive(Clone, Debug)]
pub struct BatchItemState<'a> {
    pub item_id: &'a str,
    /// Terminal state: `succeeded`, `failed`, `skipped`, or `refused`
    /// (refused persists as `failed`).
    pub new_state: &'a str,
    /// Failure reason text, if any.
    pub failure_reason: Option<&'a str>,
    /// Whether this is a CAS stale failure (sets `item_stale = 1`).
    /// Required so `get_last_stale_item` (used by `revalidate_pause_condition`)
    /// can find the triggering item after a flush-batched stale transition.
    pub is_stale: bool,
}

/// Persist a batch of item terminal-state transitions in one transaction
/// connection.
///
/// Issues one UPDATE per item in `states`, then one aggregated plans-counter
/// UPDATE. Callers open the transaction, call this, write audit and event rows
/// on the same connection, and commit — achieving one fsync per flush window.
///
/// `delta_applied`, `delta_failed`, `delta_skipped` are the net counter
/// changes for this batch.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn batch_flush_item_states(
    conn: &mut SqliteConnection,
    plan_id: &str,
    states: &[BatchItemState<'_>],
    delta_applied: i64,
    delta_failed: i64,
    delta_skipped: i64,
) -> DbResult<()> {
    if states.is_empty() {
        return Ok(());
    }

    for item in states {
        // 'refused' and 'stale' have no distinct item_state CHECK values —
        // both persist as 'failed'; item_stale=1 distinguishes stale items.
        let db_state =
            if matches!(item.new_state, "refused" | "stale") { "failed" } else { item.new_state };
        // item_stale=1 is load-bearing: get_last_stale_item (used by
        // revalidate_pause_condition for "item.stale" pauses) queries
        // WHERE item_stale=1; omitting this flag makes stale-paused plans
        // skip source re-validation on resume.
        let stale_flag = i64::from(item.is_stale);
        sqlx::query(
            "UPDATE plan_items SET item_state = ?, failure_reason = ?, item_stale = ? WHERE id = ?",
        )
        .bind(db_state)
        .bind(item.failure_reason)
        .bind(stale_flag)
        .bind(item.item_id)
        .execute(&mut *conn)
        .await?;
    }

    // One aggregated counter statement for the whole batch.
    let delta_pending = delta_applied + delta_failed + delta_skipped;
    sqlx::query(
        "UPDATE plans SET \
           items_applied = items_applied + ?, \
           items_failed  = items_failed  + ?, \
           items_skipped = items_skipped + ?, \
           items_pending = MAX(0, items_pending - ?) \
         WHERE id = ?",
    )
    .bind(delta_applied)
    .bind(delta_failed)
    .bind(delta_skipped)
    .bind(delta_pending)
    .bind(plan_id)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// Append a `plan_apply_events` row on an existing connection (for use inside
/// a group-commit transaction).
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
#[allow(clippy::too_many_arguments)]
pub async fn append_event_conn(
    conn: &mut SqliteConnection,
    event_id: &str,
    run_id: &str,
    plan_id: &str,
    item_id: Option<&str>,
    prior_state: &str,
    new_state: &str,
    at: &str,
    failure: Option<&EventFailure<'_>>,
    rollback: Option<&EventRollback<'_>>,
) -> DbResult<()> {
    let (fc, fm, fr) = failure.map_or((None, None, None), |f| {
        (Some(f.code), Some(f.message), Some(i64::from(f.recoverable)))
    });

    let (ra, ro, rm) = rollback
        .map_or((None, None, None), |r| (Some(i64::from(r.attempted)), Some(r.outcome), r.message));

    sqlx::query(
        "INSERT INTO plan_apply_events \
         (id, run_id, plan_id, item_id, prior_state, new_state, at, \
          failure_code, failure_message, failure_recoverable, \
          rollback_attempted, rollback_outcome, rollback_message) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event_id)
    .bind(run_id)
    .bind(plan_id)
    .bind(item_id)
    .bind(prior_state)
    .bind(new_state)
    .bind(at)
    .bind(fc)
    .bind(fm)
    .bind(fr)
    .bind(ra)
    .bind(ro)
    .bind(rm)
    .execute(conn)
    .await?;

    Ok(())
}

// ── Startup sweep ─────────────────────────────────────────────────────────────

/// At startup, flip every plan in state `applying` to `paused` with
/// `pause_reason = 'crash'`, and mark its open run row as paused.
///
/// A plan left in `applying` with no live executor (i.e. after a hard crash)
/// can never progress: the `active_runs` registry is empty at boot, so
/// `resume_plan` would immediately fail the live-run check. Flipping these
/// plans to `paused` makes `resume_plan` available and sets a clear reason in
/// the audit trail.
///
/// Returns the ids of the plans that were transitioned.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn sweep_crashed_applying_plans(pool: &SqlitePool) -> DbResult<Vec<String>> {
    let mut tx = pool.begin().await?;

    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM plans WHERE state = 'applying' ORDER BY id ASC")
            .fetch_all(&mut *tx)
            .await?;

    if !ids.is_empty() {
        sqlx::query("UPDATE plans SET state = 'paused' WHERE state = 'applying'")
            .execute(&mut *tx)
            .await?;

        // Mark each plan's open run (terminal_state IS NULL or 'paused' covers
        // in-progress runs; a brand-new 'applying' run has terminal_state = NULL).
        sqlx::query(
            "UPDATE plan_apply_runs \
             SET terminal_state = 'paused', pause_reason = 'crash' \
             WHERE plan_id IN \
               (SELECT value FROM json_each(?)) \
             AND (terminal_state IS NULL OR terminal_state = 'applying')",
        )
        .bind(serde_json::to_string(&ids).unwrap_or_default())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(ids)
}

/// List the plan ids interrupted by an unclean shutdown, for the recovery
/// prompt. A plan qualifies when it is still `applying` (the boot sweep has not
/// yet run) or already `paused` with `pause_reason = 'crash'` (the sweep ran).
/// This is order-independent with respect to the async startup sweep, so the
/// recovery command returns the same set whether it is queried before or after
/// the sweep completes.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn list_crash_interrupted_plans(pool: &SqlitePool) -> DbResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT p.id FROM plans p \
         WHERE p.state = 'applying' \
            OR (p.state = 'paused' AND EXISTS ( \
                 SELECT 1 FROM plan_apply_runs r \
                 WHERE r.plan_id = p.id AND r.pause_reason = 'crash')) \
         ORDER BY p.id ASC",
    )
    .fetch_all(pool)
    .await?)
}

// ── Boot reconciliation ───────────────────────────────────────────────────────

/// A non-terminal plan item awaiting boot reconciliation against the filesystem.
///
/// After [`sweep_crashed_applying_plans`] flips a crashed `applying` plan to
/// `paused`, its items may still be `pending` (never reached — the group-commit
/// design drops the per-item `applying` write) or `applying` (a mid-run retry).
/// Each such row is an intent whose outcome may have been rewound under
/// `synchronous = NORMAL`; the caller probes the filesystem to classify it.
#[derive(Clone, Debug)]
pub struct UnreconciledItem {
    pub id: String,
    pub plan_id: String,
    pub action: String,
    pub from_root_id: Option<String>,
    pub from_relative_path: String,
    pub to_root_id: Option<String>,
    pub to_relative_path: String,
    pub archive_path: Option<String>,
}

/// List the non-terminal items of the given plans (typically the ids returned
/// by [`sweep_crashed_applying_plans`]) so the caller can probe filesystem
/// reality and reconcile each intent's outcome.
///
/// Returns only `pending`/`applying` items — terminal items already recorded
/// their outcome and need no reconciliation.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn list_unreconciled_items(
    pool: &SqlitePool,
    plan_ids: &[String],
) -> DbResult<Vec<UnreconciledItem>> {
    if plan_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<
        _,
        (String, String, String, Option<String>, String, Option<String>, String, Option<String>),
    >(
        "SELECT id, plan_id, action, from_root_id, from_relative_path, \
                to_root_id, to_relative_path, archive_path \
         FROM plan_items \
         WHERE plan_id IN (SELECT value FROM json_each(?)) \
           AND item_state IN ('pending', 'applying') \
         ORDER BY plan_id ASC, item_index ASC",
    )
    .bind(serde_json::to_string(plan_ids).unwrap_or_default())
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                plan_id,
                action,
                from_root_id,
                from_relative_path,
                to_root_id,
                to_relative_path,
                archive_path,
            )| {
                UnreconciledItem {
                    id,
                    plan_id,
                    action,
                    from_root_id,
                    from_relative_path,
                    to_root_id,
                    to_relative_path,
                    archive_path,
                }
            },
        )
        .collect())
}

/// Persist the healed outcome of a boot-reconciled item.
///
/// `new_state` is a terminal `plan_items.item_state` (`succeeded` or `failed`);
/// `failure_reason` carries the reconciliation verdict for ambiguous items. The
/// per-plan counter is adjusted and an append-only `plan_apply_events` row is
/// written recording the `prior_state -> new_state` transition with source
/// `boot.reconcile`, so the audit trail shows recovery healed the row rather
/// than the executor. `run_id` links the event to the crashed run.
///
/// # Errors
///
/// Returns [`DbError::Database`] on connection failure.
pub async fn record_reconciled_outcome(
    pool: &SqlitePool,
    item: &UnreconciledItem,
    prior_state: &str,
    new_state: &str,
    run_id: &str,
    failure_reason: Option<&str>,
    at: &str,
    event_id: &str,
) -> DbResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("UPDATE plan_items SET item_state = ?, failure_reason = ? WHERE id = ?")
        .bind(new_state)
        .bind(failure_reason)
        .bind(&item.id)
        .execute(&mut *tx)
        .await?;

    let (d_applied, d_failed) = match new_state {
        "succeeded" => (1_i64, 0_i64),
        _ => (0, 1),
    };
    sqlx::query(
        "UPDATE plans SET \
           items_applied = items_applied + ?, \
           items_failed  = items_failed  + ?, \
           items_pending = MAX(0, items_pending - 1) \
         WHERE id = ?",
    )
    .bind(d_applied)
    .bind(d_failed)
    .bind(&item.plan_id)
    .execute(&mut *tx)
    .await?;

    let failure = failure_reason.map(|reason| EventFailure {
        code: "reconcile.ambiguous",
        message: reason,
        recoverable: true,
    });
    append_event_conn(
        &mut tx,
        event_id,
        run_id,
        &item.plan_id,
        Some(&item.id),
        prior_state,
        new_state,
        at,
        failure.as_ref(),
        None,
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::plans as plans_repo;
    use persistence_core::Database;

    async fn setup_with_approved_plan(db: &Database, plan_id: &str, item_count: usize) {
        plans_repo::insert_plan(
            db.pool(),
            &plans_repo::InsertPlan {
                id: plan_id,
                title: "Test",
                origin: "cleanup",
                origin_path: None,
                plan_type: "cleanup",
                destructive_destination: "archive",
                parent_plan_id: None,
                total_bytes_required: 0,
            },
        )
        .await
        .unwrap();

        for i in 0..item_count {
            plans_repo::insert_plan_item(
                db.pool(),
                &plans_repo::InsertPlanItem {
                    id: &format!("{plan_id}-item-{i}"),
                    plan_id,
                    item_index: i64::try_from(i + 1).unwrap(),
                    name: "file.fits",
                    action: "move",
                    from_root_id: None,
                    from_relative_path: "raw/file.fits",
                    to_root_id: None,
                    to_relative_path: "archive/file.fits",
                    reason: "test",
                    protection: "normal",
                    linked_entity: None,
                    provenance_json: None,
                    archive_path: None,
                    source_id: None,
                    category: None,
                },
            )
            .await
            .unwrap();
        }

        plans_repo::update_plan_state(db.pool(), plan_id, "ready_for_review").await.unwrap();
        plans_repo::set_approved(db.pool(), plan_id, "2026-06-01T00:00:00Z", "test-token")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cas_approved_to_applying_happy_path() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "p1", 2).await;

        cas_approved_to_applying(db.pool(), "p1", "run-1", "test-token", 2, 2).await.unwrap();

        let plan = plans_repo::get_plan(db.pool(), "p1", false).await.unwrap();
        assert_eq!(plan.state, "applying");

        let run = get_run(db.pool(), "run-1").await.unwrap();
        assert_eq!(run.plan_id, "p1");
        assert_eq!(run.items_total, 2);
    }

    #[tokio::test]
    async fn cas_fails_if_not_approved() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        plans_repo::insert_plan(
            db.pool(),
            &plans_repo::InsertPlan {
                id: "p2",
                title: "Test",
                origin: "cleanup",
                origin_path: None,
                plan_type: "cleanup",
                destructive_destination: "archive",
                parent_plan_id: None,
                total_bytes_required: 0,
            },
        )
        .await
        .unwrap();

        let err =
            cas_approved_to_applying(db.pool(), "p2", "run-x", "tok", 0, 0).await.unwrap_err();
        assert!(matches!(err, DbError::CasFailed(_)), "expected CasFailed, got {err:?}");
    }

    #[tokio::test]
    async fn item_state_transitions() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "p3", 1).await;
        cas_approved_to_applying(db.pool(), "p3", "run-3", "tok", 1, 1).await.unwrap();

        // Batch path: pending → succeeded in one flush (no applying intermediate).
        let mut conn = db.pool().acquire().await.unwrap();
        batch_flush_item_states(
            &mut conn,
            "p3",
            &[BatchItemState {
                item_id: "p3-item-0",
                new_state: "succeeded",
                failure_reason: None,
                is_stale: false,
            }],
            1,
            0,
            0,
        )
        .await
        .unwrap();

        let items = plans_repo::list_plan_items(db.pool(), "p3").await.unwrap();
        assert_eq!(items[0].item_state, "succeeded");

        let plan = plans_repo::get_plan(db.pool(), "p3", false).await.unwrap();
        assert_eq!(plan.items_applied, 1);
    }

    #[tokio::test]
    async fn batch_cancel_pending_items_test() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "p4", 3).await;
        cas_approved_to_applying(db.pool(), "p4", "run-4", "tok", 3, 3).await.unwrap();

        // Apply first item via batch path.
        let mut conn = db.pool().acquire().await.unwrap();
        batch_flush_item_states(
            &mut conn,
            "p4",
            &[BatchItemState {
                item_id: "p4-item-0",
                new_state: "succeeded",
                failure_reason: None,
                is_stale: false,
            }],
            1,
            0,
            0,
        )
        .await
        .unwrap();

        // Cancel remaining 2.
        let cancelled = batch_cancel_pending_items(db.pool(), "p4").await.unwrap();
        assert_eq!(cancelled, 2);

        let plan = plans_repo::get_plan(db.pool(), "p4", false).await.unwrap();
        assert_eq!(plan.items_cancelled, 2);
        assert_eq!(plan.items_pending, 0);
    }

    #[tokio::test]
    async fn append_and_list_events() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "p5", 1).await;
        cas_approved_to_applying(db.pool(), "p5", "run-5", "tok", 1, 1).await.unwrap();

        append_event(
            db.pool(),
            "evt-1",
            "run-5",
            "p5",
            Some("p5-item-0"),
            "pending",
            "applying",
            "2026-06-01T00:00:00Z",
            None,
            None,
        )
        .await
        .unwrap();

        let events = list_events(db.pool(), "p5").await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].prior_state, "pending");
        assert_eq!(events[0].new_state, "applying");
    }

    #[tokio::test]
    async fn sweep_crashed_applying_plans_transitions_to_paused() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "pc1", 2).await;

        // Drive into 'applying' (crash simulation: no executor runs).
        cas_approved_to_applying(db.pool(), "pc1", "run-c1", "tok", 2, 2).await.unwrap();
        let plan = plans_repo::get_plan(db.pool(), "pc1", false).await.unwrap();
        assert_eq!(plan.state, "applying");

        let affected = sweep_crashed_applying_plans(db.pool()).await.unwrap();
        assert_eq!(affected, vec!["pc1".to_owned()]);

        let plan = plans_repo::get_plan(db.pool(), "pc1", false).await.unwrap();
        assert_eq!(plan.state, "paused", "crash-applying plan must become paused");

        let run = get_run(db.pool(), "run-c1").await.unwrap();
        assert_eq!(run.terminal_state, Some("paused".to_owned()));
        assert_eq!(run.pause_reason, Some("crash".to_owned()));
    }

    #[tokio::test]
    async fn sweep_ignores_already_paused_and_terminal_plans() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        // A plan in 'paused' state should not be affected.
        setup_with_approved_plan(&db, "pp1", 1).await;
        cas_approved_to_applying(db.pool(), "pp1", "run-pp1", "tok", 1, 1).await.unwrap();
        pause_run(db.pool(), "pp1", "run-pp1", "item.stale", 0, 1, 0, 0, 0).await.unwrap();

        let affected = sweep_crashed_applying_plans(db.pool()).await.unwrap();
        assert!(affected.is_empty(), "paused plan must not be swept again");

        let plan = plans_repo::get_plan(db.pool(), "pp1", false).await.unwrap();
        assert_eq!(plan.state, "paused");
        // pause_reason stays 'item.stale', not overwritten to 'crash'.
        let run = get_run(db.pool(), "run-pp1").await.unwrap();
        assert_eq!(run.pause_reason, Some("item.stale".to_owned()));
    }

    #[tokio::test]
    async fn complete_run_updates_plan_and_run() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "p6", 2).await;
        cas_approved_to_applying(db.pool(), "p6", "run-6", "tok", 2, 2).await.unwrap();

        complete_run(db.pool(), "p6", "run-6", "applied", 2, 0, 0, 0).await.unwrap();

        let plan = plans_repo::get_plan(db.pool(), "p6", false).await.unwrap();
        assert_eq!(plan.state, "applied");
        assert_eq!(plan.items_applied, 2);

        let run = get_run(db.pool(), "run-6").await.unwrap();
        assert_eq!(run.terminal_state, Some("applied".to_owned()));
    }

    /// batch_flush_item_states must set item_stale=1 for stale items so
    /// get_last_stale_item can find them (required by revalidate_pause_condition).
    #[tokio::test]
    async fn batch_flush_sets_item_stale_flag() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "ps1", 2).await;
        cas_approved_to_applying(db.pool(), "ps1", "run-ps1", "tok", 2, 2).await.unwrap();

        let mut conn = db.pool().acquire().await.unwrap();
        batch_flush_item_states(
            &mut conn,
            "ps1",
            &[
                BatchItemState {
                    item_id: "ps1-item-0",
                    new_state: "stale",
                    failure_reason: Some("item.stale: source changed"),
                    is_stale: true,
                },
                BatchItemState {
                    item_id: "ps1-item-1",
                    new_state: "failed",
                    failure_reason: Some("other error"),
                    is_stale: false,
                },
            ],
            0,
            2,
            0,
        )
        .await
        .unwrap();

        let stale_flag: i64 =
            sqlx::query_scalar("SELECT item_stale FROM plan_items WHERE id = 'ps1-item-0'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(stale_flag, 1, "stale item must have item_stale=1");

        let non_stale_flag: i64 =
            sqlx::query_scalar("SELECT item_stale FROM plan_items WHERE id = 'ps1-item-1'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(non_stale_flag, 0, "non-stale item must keep item_stale=0");

        // Verify get_last_stale_item finds the stale item.
        let found = get_last_stale_item(db.pool(), "ps1").await.unwrap();
        assert!(found.is_some(), "get_last_stale_item must find the stale item");
        assert_eq!(found.unwrap().id, "ps1-item-0");
    }

    #[tokio::test]
    async fn list_unreconciled_returns_only_non_terminal_items() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "pr1", 3).await;
        cas_approved_to_applying(db.pool(), "pr1", "run-r1", "tok", 3, 3).await.unwrap();

        // Flush one item to a terminal state; two remain pending.
        let mut conn = db.pool().acquire().await.unwrap();
        batch_flush_item_states(
            &mut conn,
            "pr1",
            &[BatchItemState {
                item_id: "pr1-item-0",
                new_state: "succeeded",
                failure_reason: None,
                is_stale: false,
            }],
            1,
            0,
            0,
        )
        .await
        .unwrap();
        drop(conn);

        let items = list_unreconciled_items(db.pool(), &["pr1".to_owned()]).await.unwrap();
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["pr1-item-1", "pr1-item-2"], "terminal items are excluded");
        assert_eq!(items[0].action, "move");
        assert_eq!(items[0].from_relative_path, "raw/file.fits");
        assert_eq!(items[0].to_relative_path, "archive/file.fits");

        // Empty plan-id list short-circuits to no rows.
        assert!(list_unreconciled_items(db.pool(), &[]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn record_reconciled_outcome_heals_and_appends_event() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        setup_with_approved_plan(&db, "pr2", 2).await;
        cas_approved_to_applying(db.pool(), "pr2", "run-r2", "tok", 2, 2).await.unwrap();
        sweep_crashed_applying_plans(db.pool()).await.unwrap();

        let items = list_unreconciled_items(db.pool(), &["pr2".to_owned()]).await.unwrap();
        assert_eq!(items.len(), 2);

        // Heal the first item (filesystem showed the move completed).
        record_reconciled_outcome(
            db.pool(),
            &items[0],
            "pending",
            "succeeded",
            "run-r2",
            None,
            "2026-07-25T00:00:00Z",
            "evt-heal-1",
        )
        .await
        .unwrap();

        // Flag the second item ambiguous.
        record_reconciled_outcome(
            db.pool(),
            &items[1],
            "pending",
            "failed",
            "run-r2",
            Some("filesystem state ambiguous at boot; needs user resume/repair"),
            "2026-07-25T00:00:01Z",
            "evt-amb-1",
        )
        .await
        .unwrap();

        let healed: String = sqlx::query_scalar("SELECT item_state FROM plan_items WHERE id = ?")
            .bind(&items[0].id)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(healed, "succeeded");

        let flagged: (String, Option<String>) =
            sqlx::query_as("SELECT item_state, failure_reason FROM plan_items WHERE id = ?")
                .bind(&items[1].id)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(flagged.0, "failed");
        assert!(flagged.1.unwrap().contains("ambiguous"));

        let plan = plans_repo::get_plan(db.pool(), "pr2", false).await.unwrap();
        assert_eq!(plan.items_applied, 1);
        assert_eq!(plan.items_failed, 1);

        // A boot.reconcile event was appended per item with the transition.
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plan_apply_events WHERE plan_id = ? AND prior_state = 'pending'",
        )
        .bind("pr2")
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(events, 2);

        let ambiguous_code: Option<String> =
            sqlx::query_scalar("SELECT failure_code FROM plan_apply_events WHERE id = 'evt-amb-1'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(ambiguous_code, Some("reconcile.ambiguous".to_owned()));
    }

    /// list_crash_interrupted_plans must return the same set before and after
    /// the sweep runs (applying, then paused+crash), and exclude non-crash
    /// paused plans.
    #[tokio::test]
    async fn list_crash_interrupted_is_order_independent_with_sweep() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();

        // pc: crashed mid-apply. pn: paused for a non-crash reason.
        setup_with_approved_plan(&db, "pc", 1).await;
        setup_with_approved_plan(&db, "pn", 1).await;
        cas_approved_to_applying(db.pool(), "pc", "run-pc", "tok", 1, 1).await.unwrap();
        cas_approved_to_applying(db.pool(), "pn", "run-pn", "tok", 1, 1).await.unwrap();
        pause_run(db.pool(), "pn", "run-pn", "item.stale", 0, 1, 0, 0, 0).await.unwrap();

        // Before the sweep: pc is still 'applying'.
        let before = list_crash_interrupted_plans(db.pool()).await.unwrap();
        assert_eq!(
            before,
            vec!["pc".to_owned()],
            "applying plan is interrupted; non-crash pause is not"
        );

        // After the sweep: pc is 'paused' with reason 'crash' — still returned.
        sweep_crashed_applying_plans(db.pool()).await.unwrap();
        let after = list_crash_interrupted_plans(db.pool()).await.unwrap();
        assert_eq!(
            after,
            vec!["pc".to_owned()],
            "crash-paused plan stays in the set after the sweep"
        );
    }
}
