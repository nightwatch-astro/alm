use std::sync::Mutex;
use std::time::{Duration, Instant};

use fs_executor::failure::RollbackOutcome;
use persistence_core::repositories::audit_writes;
use persistence_lifecycle::repositories::events as events_repo;
use persistence_plans::repositories::plan_apply as apply_repo;

use super::{
    deterministic_entity_id, json, new_id, AuditLogEntry, EntityType, EventBus, ExecutorCallbacks,
    ItemProgressEvent, OpEventEmitter, OperationEventType, Outcome, PlanItemProgress, Severity,
    Source, SqlitePool, TOPIC_PLAN_ITEM_PROGRESS,
};

// ── Flush constants ───────────────────────────────────────────────────────────

/// Maximum number of items buffered before a flush is triggered at the next
/// item boundary.
const FLUSH_ITEM_COUNT: usize = 100;
/// Maximum wall-clock time between flushes, checked at item boundaries (no
/// background timer thread).
const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

// ── Buffered item ─────────────────────────────────────────────────────────────

/// One item's data captured at `on_item_progress` time, deferred until flush.
struct BufferedItem {
    item_id: String,
    prior_state: String,
    new_state: String,
    at: String,
    failure_reason: Option<String>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    failure_recoverable: Option<bool>,
    rollback_attempted: bool,
    rollback_outcome: RollbackOutcome,
    rollback_message: Option<String>,
    audit_entry: AuditLogEntry,
    /// Pre-serialised `PlanItemProgress` payload for the events-table row.
    bus_payload_json: String,
    /// Data for the live long-op projection emit after the flush (best-effort,
    /// not part of the durable tx — additive per constitution §II).
    op_event: Option<OpEventPayload>,
}

struct OpEventPayload {
    event_type: OperationEventType,
    payload: serde_json::Value,
}

// ── Flush buffer ──────────────────────────────────────────────────────────────

struct FlushBuffer {
    items: Vec<BufferedItem>,
    last_flush: Instant,
}

impl FlushBuffer {
    fn new() -> Self {
        Self { items: Vec::with_capacity(FLUSH_ITEM_COUNT + 1), last_flush: Instant::now() }
    }

    fn should_flush(&self) -> bool {
        self.items.len() >= FLUSH_ITEM_COUNT
            || (!self.items.is_empty() && self.last_flush.elapsed() >= FLUSH_INTERVAL)
    }
}

// ── PlanApplyCallbacks ────────────────────────────────────────────────────────

pub(super) struct PlanApplyCallbacks {
    pub(super) pool: SqlitePool,
    pub(super) bus: EventBus,
    pub(super) plan_id: String,
    pub(super) run_id: String,
    /// Optional live long-op projection (spec 042 US16). `None` when the caller
    /// (e.g. a unit test) does not subscribe; the DB audit trail is unaffected.
    pub(super) op_emitter: Option<OpEventEmitter>,
    /// Buffered items awaiting the next flush transaction.
    buffer: Mutex<FlushBuffer>,
}

impl PlanApplyCallbacks {
    pub(super) fn new(
        pool: SqlitePool,
        bus: EventBus,
        plan_id: String,
        run_id: String,
        op_emitter: Option<OpEventEmitter>,
    ) -> Self {
        Self { pool, bus, plan_id, run_id, op_emitter, buffer: Mutex::new(FlushBuffer::new()) }
    }

    /// Drain all buffered items into a single DB transaction.
    ///
    /// Called at mandatory flush points (before returning any `ApplyOutcome`,
    /// before `pause_run` / `complete_run` / batch-cancel). Swallows DB errors
    /// with a warning — the run still completes, matching the existing per-item
    /// best-effort error handling for plan_apply_events / audit rows.
    pub(super) async fn flush(&self) {
        let items = {
            let mut buf = self.buffer.lock().expect("flush buffer poisoned");
            if buf.items.is_empty() {
                return;
            }
            let drained =
                std::mem::replace(&mut buf.items, Vec::with_capacity(FLUSH_ITEM_COUNT + 1));
            buf.last_flush = Instant::now();
            drained
        };

        self.flush_items(items).await;
    }

    #[expect(
        clippy::cognitive_complexity,
        reason = "batched item flush aggregating per-outcome counter deltas before one UPDATE"
    )]
    async fn flush_items(&self, items: Vec<BufferedItem>) {
        if items.is_empty() {
            return;
        }

        // Compute deltas for the aggregated plans-counter UPDATE.
        let mut delta_applied: i64 = 0;
        let mut delta_failed: i64 = 0;
        let mut delta_skipped: i64 = 0;
        let mut owned_states: Vec<(String, String, Option<String>, bool)> =
            Vec::with_capacity(items.len());
        for item in &items {
            match item.new_state.as_str() {
                "succeeded" => delta_applied += 1,
                "skipped" => delta_skipped += 1,
                _ => delta_failed += 1, // failed / stale / refused
            }
            owned_states.push((
                item.item_id.clone(),
                item.new_state.clone(),
                item.failure_reason.clone(),
                item.new_state == "stale",
            ));
        }
        let batch_states: Vec<apply_repo::BatchItemState<'_>> = owned_states
            .iter()
            .map(|(id, state, reason, is_stale)| apply_repo::BatchItemState {
                item_id: id.as_str(),
                new_state: state.as_str(),
                failure_reason: reason.as_deref(),
                is_stale: *is_stale,
            })
            .collect();

        // One transaction: plan_items + plans counter + plan_apply_events +
        // audit_log_entry + events.
        let plan_id = self.plan_id.as_str();
        let run_id = self.run_id.as_str();
        let mut tx = match self.pool.begin().await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    item_count = items.len(),
                    "group-commit flush: begin tx failed; items lost from audit trail"
                );
                return;
            }
        };

        if let Err(e) = apply_repo::batch_flush_item_states(
            &mut tx,
            plan_id,
            &batch_states,
            delta_applied,
            delta_failed,
            delta_skipped,
        )
        .await
        {
            tracing::error!(error = %e, "group-commit flush: batch_flush_item_states failed");
            return;
        }

        // plan_apply_events — one row per item.
        for item in &items {
            let failure_ref = item.failure_code.as_ref().map(|_| apply_repo::EventFailure {
                code: item.failure_code.as_deref().unwrap_or(""),
                message: item.failure_message.as_deref().unwrap_or(""),
                recoverable: item.failure_recoverable.unwrap_or(false),
            });
            let rollback_ref = item.rollback_attempted.then(|| apply_repo::EventRollback {
                attempted: item.rollback_attempted,
                outcome: item.rollback_outcome.as_str(),
                message: item.rollback_message.as_deref(),
            });

            if let Err(e) = apply_repo::append_event_conn(
                &mut tx,
                &new_id(),
                run_id,
                plan_id,
                Some(item.item_id.as_str()),
                item.prior_state.as_str(),
                item.new_state.as_str(),
                item.at.as_str(),
                failure_ref.as_ref(),
                rollback_ref.as_ref(),
            )
            .await
            {
                tracing::error!(item_id = %item.item_id, error = %e, "group-commit flush: append_event_conn failed");
            }
        }

        // audit_log_entry — one row per item.
        for item in &items {
            if let Err(e) = audit_writes::insert_audit_entry_conn(&mut tx, &item.audit_entry).await
            {
                tracing::error!(item_id = %item.item_id, error = %e, "group-commit flush: insert_audit_entry_conn failed");
            }
        }

        // events — one row per item (the log forwarder wakes once per flush
        // broadcast, but each item still gets a durable events row).
        let emitted_at = domain_core::ids::Timestamp::now_iso();
        for item in &items {
            if let Err(e) = events_repo::insert_event_conn(
                &mut tx,
                TOPIC_PLAN_ITEM_PROGRESS,
                "system",
                emitted_at.as_str(),
                item.bus_payload_json.as_str(),
            )
            .await
            {
                tracing::error!(item_id = %item.item_id, error = %e, "group-commit flush: insert_event_conn failed");
            }
        }

        if let Err(e) = tx.commit().await {
            tracing::error!(error = %e, item_count = items.len(), "group-commit flush: commit failed; retrying once");
            // GF-15: Retry once — terminal-state persist is TIER-1 durability.
            match self.retry_flush_once(&items).await {
                Ok(()) => {}
                Err(retry_err) => {
                    tracing::error!(error = %retry_err, item_count = items.len(), "group-commit flush: retry also failed; recording divergence");
                    self.record_divergence(&items).await;
                    return;
                }
            }
        }

        // One broadcast per flush — wakes the log forwarder once per flush
        // window instead of once per item.
        let _ = self.bus.broadcast_only(TOPIC_PLAN_ITEM_PROGRESS);

        // Live long-op projection (spec 042 US16, kyo7.52): one batched
        // Progress envelope per flush instead of one event per item.
        // Individual failure emits are preserved — they carry detail the UI
        // uses to show per-item error messages (rare, far below the item rate).
        // Plan-level Started / Completed / Paused events are unchanged.
        if let Some(emitter) = self.op_emitter.as_ref() {
            // Collect window failures for the batch Progress envelope.
            let window_failures: Vec<serde_json::Value> = items
                .iter()
                .filter(|i| i.failure_code.is_some())
                .map(|i| {
                    json!({
                        "itemId": i.item_id,
                        "code": i.failure_code,
                        "message": i.failure_message,
                    })
                })
                .collect();

            // Individual failure emits (kept for per-item detail in the UI).
            for item in &items {
                if let Some(ref op) = item.op_event {
                    if op.event_type == OperationEventType::ItemFailed {
                        emitter.emit(op.event_type, op.payload.clone());
                    }
                }
            }

            // One Progress event for the whole flush window. `itemsFailed` is
            // intentionally 0: individual ItemFailed emits above already
            // increment the UI counter, so including a non-zero delta here
            // would double-count failures (kyo7.52 finding 2).
            let last_item_id = items
                .last()
                .map_or(serde_json::Value::Null, |i| serde_json::Value::String(i.item_id.clone()));
            emitter.emit(
                OperationEventType::Progress,
                json!({
                    "planId": self.plan_id,
                    "runId": self.run_id,
                    "itemsApplied": delta_applied,
                    "itemsFailed": 0,
                    "itemsSkipped": delta_skipped,
                    "lastItemId": last_item_id,
                    "windowFailures": window_failures,
                }),
            );
        }
    }

    /// GF-15: Single retry of the full flush transaction after initial commit
    /// failure. Terminal-state persistence is TIER-1 durability (constitution
    /// v1.1.0) — a transient SQLite busy/lock timeout must not silently drop
    /// item outcomes.
    async fn retry_flush_once(&self, items: &[BufferedItem]) -> Result<(), sqlx::Error> {
        let plan_id = self.plan_id.as_str();
        let run_id = self.run_id.as_str();

        let mut delta_applied: i64 = 0;
        let mut delta_failed: i64 = 0;
        let mut delta_skipped: i64 = 0;
        let mut owned_states: Vec<(String, String, Option<String>, bool)> =
            Vec::with_capacity(items.len());
        for item in items {
            match item.new_state.as_str() {
                "succeeded" => delta_applied += 1,
                "skipped" => delta_skipped += 1,
                _ => delta_failed += 1,
            }
            owned_states.push((
                item.item_id.clone(),
                item.new_state.clone(),
                item.failure_reason.clone(),
                item.new_state == "stale",
            ));
        }
        let batch_states: Vec<apply_repo::BatchItemState<'_>> = owned_states
            .iter()
            .map(|(id, state, reason, is_stale)| apply_repo::BatchItemState {
                item_id: id.as_str(),
                new_state: state.as_str(),
                failure_reason: reason.as_deref(),
                is_stale: *is_stale,
            })
            .collect();

        let mut tx = self.pool.begin().await?;

        apply_repo::batch_flush_item_states(
            &mut tx,
            plan_id,
            &batch_states,
            delta_applied,
            delta_failed,
            delta_skipped,
        )
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        for item in items {
            let failure_ref = item.failure_code.as_ref().map(|_| apply_repo::EventFailure {
                code: item.failure_code.as_deref().unwrap_or(""),
                message: item.failure_message.as_deref().unwrap_or(""),
                recoverable: item.failure_recoverable.unwrap_or(false),
            });
            let rollback_ref = item.rollback_attempted.then(|| apply_repo::EventRollback {
                attempted: item.rollback_attempted,
                outcome: item.rollback_outcome.as_str(),
                message: item.rollback_message.as_deref(),
            });
            let _ = apply_repo::append_event_conn(
                &mut tx,
                &new_id(),
                run_id,
                plan_id,
                Some(item.item_id.as_str()),
                item.prior_state.as_str(),
                item.new_state.as_str(),
                item.at.as_str(),
                failure_ref.as_ref(),
                rollback_ref.as_ref(),
            )
            .await;
        }

        tx.commit().await
    }

    /// GF-15: Record a durable divergence marker when both flush attempts fail.
    /// The crash-recovery sweep can later reconcile items whose DB state
    /// diverges from what the executor observed on disk.
    async fn record_divergence(&self, items: &[BufferedItem]) {
        for item in items {
            let entry = AuditLogEntry::new(
                EntityType::FilesystemPlan,
                deterministic_entity_id("plan_apply.divergence", &item.item_id),
                "plan_item.persist_divergence",
                "system",
                Outcome::Failed,
                Severity::Workflow,
                domain_core::ids::EntityId::new(),
            )
            .with_transition(item.prior_state.clone(), item.new_state.clone())
            .with_reason_code("flush_commit_failed_twice")
            .with_payload(json!({
                "planId": self.plan_id,
                "runId": self.run_id,
                "itemId": item.item_id,
                "intendedState": item.new_state,
            }));
            if let Err(e) =
                persistence_core::repositories::audit_writes::insert_audit_entry(&self.pool, &entry)
                    .await
            {
                tracing::error!(
                    item_id = %item.item_id,
                    error = %e,
                    "CRITICAL: divergence record itself failed to persist"
                );
            }
        }
    }
}

impl ExecutorCallbacks for PlanApplyCallbacks {
    fn on_item_start(
        &self,
        _item_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        // The per-item 'applying' DB write (item_start_applying) is dropped in
        // the group-commit design: items flush from pending → terminal in one
        // batch tx. cancel_orphaned_applying_items is unaffected — it only
        // targets items flipped to 'applying' by retry_plan_item, which writes
        // directly via item_retry_applying (not buffered).
        Box::pin(std::future::ready(()))
    }

    fn on_item_progress(
        &self,
        event: ItemProgressEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let item_id = event.item_id.clone();
            let at = event.at.clone();

            let bus_payload = PlanItemProgress {
                plan_id: self.plan_id.clone(),
                run_id: self.run_id.clone(),
                item_id: item_id.clone(),
                prior_state: event.prior_state.clone(),
                new_state: event.new_state.clone(),
                at: at.clone(),
                failure_code: event.failure.as_ref().map(|f| f.code.as_str().to_owned()),
                failure_message: event.failure.as_ref().map(|f| f.message.clone()),
                failure_recoverable: event.failure.as_ref().map(|f| f.recoverable),
            };
            let bus_payload_json =
                serde_json::to_string(&bus_payload).unwrap_or_else(|_| "{}".to_owned());

            let outcome = match event.new_state.as_str() {
                "succeeded" => Outcome::Applied,
                "failed" => Outcome::Failed,
                _ => Outcome::Refused,
            };
            let reason_code = event
                .audit_reason
                .clone()
                .or_else(|| event.failure.as_ref().map(|f| f.code.as_str().to_owned()));

            let mut audit_entry = AuditLogEntry::new(
                EntityType::FilesystemPlan,
                deterministic_entity_id("plan_apply.item", &item_id),
                format!("plan_item.{}", event.new_state),
                "user",
                outcome,
                Severity::Workflow,
                domain_core::ids::EntityId::new(),
            )
            .with_transition(event.prior_state.clone(), event.new_state.clone())
            .with_payload(json!({
                "planId": self.plan_id,
                "runId": self.run_id,
                "itemId": item_id,
                "failureCode": event.failure.as_ref().map(|f| f.code.as_str()),
                "failureMessage": event.failure.as_ref().map(|f| f.message.clone()),
            }));
            if let Some(reason) = reason_code {
                audit_entry = audit_entry.with_reason_code(reason);
            }

            let op_event = self.op_emitter.as_ref().map(|_| {
                let event_type = match event.new_state.as_str() {
                    "succeeded" => OperationEventType::ItemApplied,
                    "failed" | "stale" => OperationEventType::ItemFailed,
                    _ => OperationEventType::Progress,
                };
                OpEventPayload {
                    event_type,
                    payload: json!({
                        "planId": self.plan_id,
                        "runId": self.run_id,
                        "itemId": item_id,
                        "priorState": event.prior_state,
                        "newState": event.new_state,
                        "at": at,
                        "failureCode": event.failure.as_ref().map(|f| f.code.as_str()),
                        "failureMessage": event.failure.as_ref().map(|f| f.message.clone()),
                    }),
                }
            });

            let buffered = BufferedItem {
                item_id: item_id.clone(),
                prior_state: event.prior_state.clone(),
                new_state: event.new_state.clone(),
                at: at.clone(),
                failure_reason: event.failure.as_ref().map(std::string::ToString::to_string),
                failure_code: event.failure.as_ref().map(|f| f.code.as_str().to_owned()),
                failure_message: event.failure.as_ref().map(|f| f.message.clone()),
                failure_recoverable: event.failure.as_ref().map(|f| f.recoverable),
                rollback_attempted: event.rollback_attempted,
                rollback_outcome: event.rollback_outcome,
                rollback_message: event.rollback_message,
                audit_entry,
                bus_payload_json,
                op_event,
            };

            let should_flush = {
                let mut buf = self.buffer.lock().expect("flush buffer poisoned");
                buf.items.push(buffered);
                buf.should_flush()
            };

            if should_flush {
                self.flush().await;
            }
        })
    }
}

/// Emit both a `plan_apply_events` row and a durable `audit_log_entry` row for
/// one item forced into `cancelled` by a bulk-cancel path (#750). The forward
/// executor loop's terminal transitions go through `on_item_progress`; this
/// helper only covers the bulk-cancel paths that bypass it.
pub(super) async fn audit_item_cancelled(
    pool: &SqlitePool,
    bus: &EventBus,
    run_id: &str,
    plan_id: &str,
    item_id: &str,
    prior_state: &str,
    at: &str,
) {
    let _ = apply_repo::append_event(
        pool,
        &new_id(),
        run_id,
        plan_id,
        Some(item_id),
        prior_state,
        "cancelled",
        at,
        None,
        None,
    )
    .await;

    let entry = AuditLogEntry::new(
        EntityType::FilesystemPlan,
        deterministic_entity_id("plan_apply.item", item_id),
        "plan_item.cancelled",
        "user",
        Outcome::Refused,
        Severity::Workflow,
        domain_core::ids::EntityId::new(),
    )
    .with_transition(prior_state.to_owned(), "cancelled".to_owned())
    .with_payload(json!({ "planId": plan_id, "runId": run_id, "itemId": item_id }));

    let bus_payload = PlanItemProgress {
        plan_id: plan_id.to_owned(),
        run_id: run_id.to_owned(),
        item_id: item_id.to_owned(),
        prior_state: prior_state.to_owned(),
        new_state: "cancelled".to_owned(),
        at: at.to_owned(),
        failure_code: None,
        failure_message: None,
        failure_recoverable: None,
    };

    if let Err(e) =
        bus.write_audit(entry, TOPIC_PLAN_ITEM_PROGRESS, Source::System, bus_payload).await
    {
        tracing::error!(%item_id, error=%e, "durable audit write failed for cancelled item");
    }
}
