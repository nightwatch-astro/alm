// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Inbox plan-state listener (spec 005, T030).
//!
//! Subscribes to the audit event bus and transitions `InboxItem.state` when a
//! linked plan reaches a terminal state:
//!
//! - `plan.applying.completed` with `terminal_state = "applied"` →
//!   `InboxItem.state = "resolved"` + delete `inbox_plan_links` row.
//! - `plan.applying.completed` with any other terminal
//!   (`partially_applied`, `failed`, `cancelled`) → back to unconfirmed +
//!   delete `inbox_plan_links` row.
//! - `plan.discarded` → back to unconfirmed + delete `inbox_plan_links` row.
//!
//! "Back to unconfirmed" is `classified` or `pending_classification` depending
//! on whether the row carries its own `frame_type` (spec 058 SC-003); it is
//! derived in SQL by `reset_inbox_item_to_unconfirmed`, never asserted here.
//!
//! The listener is started once at application startup via
//! [`start_inbox_plan_listener`] which spawns a detached `tokio::task`. It is
//! NOT the only safety mechanism — [`crate::repair::run_repair`] provides a
//! periodic background sweep for items whose plan closed while the listener was
//! not running (crash, restart, missed event). Both halves of R-PlanOpen are
//! started by [`start_inbox_plan_listener`].
//!
//! # Event bus limitations
//!
//! The tokio broadcast channel drops lagged receivers; if the app is under
//! heavy load and a `plan.applying.completed` event is dropped, the repair
//! sweep will catch it within 5 minutes. This is documented in the spec
//! (Ref: R-PlanOpen) as the expected degraded-mode behaviour.
#![allow(clippy::doc_markdown)]

use std::sync::Arc;

use audit::bus::EventBus;
use audit::event_bus::{
    PlanApplyingCompleted, PlanDiscarded, TOPIC_PLAN_APPLYING_COMPLETED, TOPIC_PLAN_DISCARDED,
};
use contracts_core::lifecycle::PlanState;
use persistence_inbox::repositories::inbox as inbox_repo;
use persistence_inbox::repositories::q_inbox::{
    self, InsertCalibrationFingerprint, InsertCalibrationSession,
};
use persistence_plans::repositories::plans as plans_repo;
use sqlx::SqlitePool;
use targeting_resolver::simbad::ResolveCache;
use tokio::sync::{broadcast, Mutex};

/// Per-plan keyed lock serializing applied-plan side effects between the event
/// listener and repair sweep. Both can observe the same plan link before either
/// path removes it — a global Mutex previously serialized ALL plan completions,
/// blocking unrelated plans (GF-31). The DashMap key is the plan_id; each entry
/// holds a Mutex guarding that plan's completion path only.
///
/// # Bounded growth justification
///
/// This map grows by one entry per distinct plan_id that completes while the
/// app is running. On a desktop client, inbox ingestion plans are created and
/// completed one at a time; the steady-state count matches the number of plans
/// applied in a session, not the historical total (entries persist for the
/// process lifetime, not the DB lifetime). A session processing thousands of
/// plans is not a realistic desktop workload, so the unbounded map is acceptable
/// without an eviction path. If multi-session or high-volume use cases emerge,
/// add eviction after `complete_applied_plan` returns, guarded by a strong-count
/// check (`Arc::strong_count == 1` confirms no concurrent holder remains).
static PLAN_COMPLETION_LOCKS: std::sync::OnceLock<dashmap::DashMap<String, Arc<Mutex<()>>>> =
    std::sync::OnceLock::new();

fn plan_completion_lock(plan_id: &str) -> Arc<Mutex<()>> {
    let map = PLAN_COMPLETION_LOCKS.get_or_init(dashmap::DashMap::new);
    map.entry(plan_id.to_owned()).or_insert_with(|| Arc::new(Mutex::const_new(()))).clone()
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Spawn a detached background task that listens for plan terminal events and
/// updates the corresponding `InboxItem` state.
///
/// Call this once at application startup, after the `SqlitePool` is available.
///
/// The `EventBus` is cloned into the task so the spec-035 light-frame ingest
/// (`handle_plan_completed` → `ingest_light_frames`) can emit `target.resolved`
/// events for inline cache hits. `resolve_cache` (also cheap to clone — an
/// `Arc` handle) is threaded through to
/// `ingest_light_frames_if_applicable`, which uses it to trigger an
/// immediate ingest-resolution drain pass after a plan's light frames are
/// ingested (issue #1256) instead of waiting on the periodic backstop.
pub fn start_inbox_plan_listener(pool: SqlitePool, bus: &EventBus, resolve_cache: ResolveCache) {
    let mut rx = bus.subscribe();
    let bus = bus.clone();
    spawn_repair_sweep(pool.clone(), bus.clone(), resolve_cache.clone());
    tokio::spawn(async move {
        run_listener_loop(pool, bus, resolve_cache, &mut rx).await;
    });
}

/// Interval between repair sweeps (Ref: R-PlanOpen).
const REPAIR_SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_mins(5);

/// Spawn the periodic [`crate::repair::run_repair`] sweep — the safety-net half
/// of R-PlanOpen, started alongside its event-driven counterpart because the
/// two are only correct together.
///
/// `tokio::time::interval` fires its first tick immediately, which is wanted: a
/// plan that closed while the app was down is repaired at startup rather than
/// five minutes into the session.
///
/// `bus` and `resolve_cache` are the same handles the event path uses: the
/// sweep runs the identical applied-plan side effects via
/// [`complete_applied_plan`], and those emit `target.resolved` events and drain
/// the ingest-resolution queue.
fn spawn_repair_sweep(pool: SqlitePool, bus: EventBus, resolve_cache: ResolveCache) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(REPAIR_SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            if let Err(e) = crate::repair::run_repair(&pool, &bus, &resolve_cache).await {
                tracing::warn!(error = %e, "inbox repair sweep failed");
            }
        }
    });
}

// ── Listener loop ─────────────────────────────────────────────────────────────

async fn run_listener_loop(
    pool: SqlitePool,
    bus: EventBus,
    resolve_cache: ResolveCache,
    rx: &mut broadcast::Receiver<audit::event_bus::EventEnvelope<serde_json::Value>>,
) {
    loop {
        match rx.recv().await {
            Ok(envelope) => {
                if let Err(e) = handle_event(&pool, &bus, &resolve_cache, &envelope).await {
                    tracing::warn!(error = %e, "inbox plan_listener: error handling event");
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                // Missed n events. The repair sweep will pick up any orphaned
                // inbox items within the next 5-minute window.
                tracing::warn!(
                    "inbox plan_listener: lagged {n} events — repair sweep will reconcile"
                );
            }
            Err(broadcast::error::RecvError::Closed) => {
                // Event bus shut down — task can exit.
                tracing::info!("inbox plan_listener: event bus closed, stopping");
                break;
            }
        }
    }
}

// ── Event handler ─────────────────────────────────────────────────────────────

async fn handle_event(
    pool: &SqlitePool,
    bus: &EventBus,
    resolve_cache: &ResolveCache,
    envelope: &audit::event_bus::EventEnvelope<serde_json::Value>,
) -> Result<(), String> {
    match envelope.topic.as_str() {
        TOPIC_PLAN_APPLYING_COMPLETED => {
            if let Ok(payload) =
                serde_json::from_value::<PlanApplyingCompleted>(envelope.payload.clone())
            {
                handle_plan_completed(pool, bus, resolve_cache, &payload).await?;
            }
        }
        TOPIC_PLAN_DISCARDED => {
            if let Ok(payload) = serde_json::from_value::<PlanDiscarded>(envelope.payload.clone()) {
                handle_plan_discarded(pool, &payload.plan_id).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Called when a plan reaches a terminal apply state.
async fn handle_plan_completed(
    pool: &SqlitePool,
    bus: &EventBus,
    resolve_cache: &ResolveCache,
    payload: &PlanApplyingCompleted,
) -> Result<(), String> {
    if payload.terminal_state == PlanState::Applied.as_str() {
        complete_applied_plan(pool, bus, resolve_cache, &payload.plan_id).await
    } else {
        // partially_applied, failed, cancelled → allow re-split, back to
        // whatever unconfirmed state the row's own frame type supports.
        transition_via_plan_id(pool, &payload.plan_id, None).await
    }
}

/// Run the applied-plan side effects, then resolve the linked inbox item.
///
/// Shared by the event path ([`handle_plan_completed`]) and the crash-recovery
/// sweep ([`crate::repair::run_repair`]); the sweep re-derives "this plan is
/// applied" from committed state instead of from an event, but must produce the
/// identical outcome, so both call this rather than duplicating the sequence.
///
/// Ordering invariant: [`transition_via_plan_id`] deletes the
/// `inbox_plan_links` row, which is the only work queue the sweep has. It
/// therefore runs strictly after [`register_master_if_applicable`] returns
/// `Ok` — a propagated registration error leaves both the link and the
/// `plan_open` state in place so the next sweep retries.
///
/// `ingest_light_frames_if_applicable` deliberately does not participate in
/// that guard: it logs and swallows its errors (spec 035 US4/T042, R12), so a
/// per-frame metadata/IO problem never strands the inbox item.
pub(crate) async fn complete_applied_plan(
    pool: &SqlitePool,
    bus: &EventBus,
    resolve_cache: &ResolveCache,
    plan_id: &str,
) -> Result<(), String> {
    let lock = plan_completion_lock(plan_id);
    let _completion_guard = lock.lock().await;

    // spec 041 US4/T032: master registration is relocated here from the old
    // confirm-time fast path. When the applied plan belongs to a detected
    // calibration master inbox item, register the master now — this applies
    // whether the master was catalogued (organized source) or moved
    // (unorganized source).
    register_master_if_applicable(pool, plan_id).await?;
    // spec 035 US4/T042: fold the plan's applied light frames into acquisition
    // sessions grouped by capture identity, linking the resolved canonical
    // target (FR-016). Calibration frames are excluded (handled by the master
    // path above).
    ingest_light_frames_if_applicable(pool, bus, plan_id, resolve_cache).await;
    transition_via_plan_id(pool, plan_id, Some("resolved")).await
}

/// Ingest the applied light frames of a completed plan into acquisition sessions
/// (spec 035 US4/T042). A sibling of [`register_master_if_applicable`]: it runs
/// for every applied plan, but [`app_core_targets::ingest_sessions::
/// ingest_light_frames`] processes only `move`/`catalogue` items whose FITS
/// header marks them as light frames, so non-inbox and calibration plans are
/// no-ops. Errors are logged rather than propagated so a metadata/IO problem on
/// one frame never blocks the inbox lifecycle transition.
///
/// issue #1256: on success, spawns a detached ingest-resolution drain pass
/// ([`app_core_targets::ingest_resolution::drain_and_backfill_once`])
/// immediately afterward, rather than leaving newly-`pending` rows (a cache
/// miss enqueued by `ingest_light_frames` above) to wait for the ~30s
/// periodic backstop (`desktop_shell::bootstrap::background::
/// spawn_ingest_resolution_drain`). Spawned (not awaited) so a slow/offline
/// SIMBAD lookup never blocks this listener's event loop from processing the
/// next bus event; it is safe to spawn after the `.await` above because the
/// enqueue it depends on has already committed by then — no read-before-write
/// race with the drain's own query.
async fn ingest_light_frames_if_applicable(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    resolve_cache: &ResolveCache,
) {
    match app_core_targets::ingest_sessions::ingest_light_frames(pool, Some(bus), plan_id).await {
        Ok(summary) if summary.ingested > 0 || summary.skipped > 0 => {
            tracing::info!(
                plan_id,
                ingested = summary.ingested,
                skipped = summary.skipped,
                "inbox plan_listener: ingested light frames into acquisition sessions"
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(plan_id, "inbox plan_listener: light-frame ingest failed: {e:?}");
            return;
        }
    }

    let pool = pool.clone();
    let bus = bus.clone();
    let resolve_cache = resolve_cache.clone();
    tokio::spawn(async move {
        app_core_targets::ingest_resolution::drain_and_backfill_once(&pool, &bus, &resolve_cache)
            .await;
    });
}

/// Register a calibration master at plan-apply completion (spec 041 US4/T032).
///
/// Looks up the inbox item linked to `plan_id`; if it is a detected master
/// (`is_master_item != 0`), inserts the `calibration_session` +
/// `calibration_fingerprint` rows that the deleted confirm-time fast path used
/// to write (same SQL/semantics). Idempotent on the apply path because a plan
/// reaches `applied` exactly once and the link is deleted on transition.
///
/// Non-master items and plans with no linked inbox item are a no-op.
async fn register_master_if_applicable(pool: &SqlitePool, plan_id: &str) -> Result<(), String> {
    let link = inbox_repo::get_plan_link_by_plan_id(pool, plan_id)
        .await
        .map_err(|e| format!("get_plan_link_by_plan_id({plan_id}): {e}"))?;
    let Some(link) = link else {
        // No inbox item linked — non-inbox plan; nothing to register.
        return Ok(());
    };

    let item = inbox_repo::get_inbox_item(pool, &link.inbox_item_id)
        .await
        .map_err(|e| format!("get_inbox_item({}): {e}", link.inbox_item_id))?;

    if item.is_master_item == 0 {
        return Ok(());
    }

    // Idempotency guard: skip if a session already references this inbox item.
    let exists = q_inbox::calibration_session_exists_for_inbox_item(pool, &item.id)
        .await
        .map_err(|e| format!("check existing calibration_session: {e}"))?;
    if exists {
        return Ok(());
    }

    let frame_type_str = item.master_frame_type.as_deref().unwrap_or("dark");
    let cal_kind = match frame_type_str {
        "flat" => "flat",
        "bias" => "bias",
        _ => "dark",
    };

    let session_id = uuid::Uuid::new_v4().to_string();
    let session_key =
        format!("{}-{}", cal_kind, item.master_frame_type.as_deref().unwrap_or("unknown"));

    // spec 048 US1/T012: write a `file_record` for the applied master frame
    // (real on-disk size, via the shared writer) and reference it in
    // `frame_ids` instead of the historical `'[]'` placeholder. A resolution
    // failure (destination path not found / stat failure) is logged and
    // leaves `frame_ids` empty — never blocks master registration.
    //
    // `resolved_root_id` (spec 006's `calibration_session.root_id`, migration
    // 0021) is captured from the same lookup and written below — this table
    // had the identical gap as `acquisition_session` (#470 round 6): the
    // column existed, `persistence_targets::repositories::inventory::
    // update_calibration_session_root_id` existed to set it, but nothing
    // ever called the setter or wrote the column at insert time, so every
    // real calibration master's `root_id` stayed `NULL` (silently decoded as
    // `""` by sqlx-sqlite, masking the gap from a plain string comparison).
    let mut resolved_root_id: Option<String> = None;
    let frame_id = match resolve_applied_frame_path(pool, plan_id).await {
        Ok(Some((root_id, relative_path))) => {
            let outcome = write_calibration_frame_record(pool, &root_id, &relative_path).await;
            resolved_root_id = Some(root_id);
            match outcome {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::warn!(
                        plan_id,
                        "inbox plan_listener: failed to write calibration file_record: {e}"
                    );
                    None
                }
            }
        }
        Ok(None) => {
            tracing::warn!(
                plan_id,
                "inbox plan_listener: no applied path found for master item; frame_ids empty"
            );
            None
        }
        Err(e) => {
            tracing::warn!(plan_id, "inbox plan_listener: resolve applied frame path failed: {e}");
            None
        }
    };
    let frame_ids_json = match &frame_id {
        Some(id) => serde_json::to_string(std::slice::from_ref(id))
            .map_err(|e| format!("serialize frame_ids: {e}"))?,
        None => "[]".to_owned(),
    };

    q_inbox::insert_calibration_session(
        pool,
        &InsertCalibrationSession {
            id: &session_id,
            session_key: &session_key,
            frame_ids_json: &frame_ids_json,
            kind: cal_kind,
            root_id: resolved_root_id.as_deref(),
            source_inbox_item_id: &item.id,
        },
    )
    .await
    .map_err(|e| format!("insert calibration_session: {e}"))?;

    q_inbox::insert_calibration_fingerprint(
        pool,
        &InsertCalibrationFingerprint {
            calibration_session_id: &session_id,
            calibration_type: cal_kind,
            exposure_s: item.master_exposure_s,
            filter_name: item.master_filter.as_deref(),
        },
    )
    .await
    .map_err(|e| format!("insert calibration_fingerprint: {e}"))?;

    // F0 invalidate-after-commit contract (crates/app/cache/src/lib.rs): both
    // inserts above have committed (sqlx pool auto-commits per statement; no
    // explicit transaction wraps them), so the masters snapshot cache is safe
    // to clear now — never before, to avoid a reader repopulating it with a
    // stale pre-commit value.
    app_core_calibration::caches::invalidate_calibration_masters();

    tracing::info!(
        inbox_item_id = %item.id,
        plan_id,
        cal_kind,
        "inbox plan_listener: registered calibration master at apply completion"
    );

    Ok(())
}

/// Resolve the applied destination `(root_id, relative_path)` for a plan's
/// first successful move/catalogue item (spec 048 T012). Master items are a
/// single stacked file, so a plan applying one has exactly one such item;
/// catalogue-in-place items carry no `to_root_id`, so the source
/// (`from_root_id`/`from_relative_path`) is used, matching
/// `app_core_targets::ingest_sessions`' resolution order (T013: moved and
/// catalogued frames are recorded identically).
async fn resolve_applied_frame_path(
    pool: &SqlitePool,
    plan_id: &str,
) -> Result<Option<(String, String)>, String> {
    // `list_plan_items` already orders by item_index ASC; find the first
    // successful move/catalogue item (same ORDER BY + LIMIT 1 semantics as
    // the original inline query).
    let items = plans_repo::list_plan_items(pool, plan_id)
        .await
        .map_err(|e| format!("query plan_items: {e}"))?;
    let Some(row) = items.into_iter().find(|it| {
        matches!(it.action.as_str(), "move" | "catalogue") && it.item_state == "succeeded"
    }) else {
        return Ok(None);
    };

    Ok(match (row.to_root_id, row.from_root_id) {
        (Some(r), _) if !row.to_relative_path.is_empty() => Some((r, row.to_relative_path)),
        (_, Some(r)) => Some((r, row.from_relative_path)),
        _ => None,
    })
}

/// Write the calibration master's `file_record` with its real on-disk size
/// (spec 048 T012), reusing the shared writer (`crate::frame_writer` via
/// `app_core_targets`, T002). Resolves `root_id` the same way light-frame
/// ingest does (`registered_sources` mirrored into `library_root`) so the
/// `file_record.root_id` FK holds.
async fn write_calibration_frame_record(
    pool: &SqlitePool,
    root_id: &str,
    relative_path: &str,
) -> Result<String, String> {
    let root_path = app_core_targets::ingest_sessions::ensure_library_root(pool, root_id)
        .await
        .map_err(|e| format!("ensure_library_root: {e:?}"))?
        .ok_or_else(|| format!("root {root_id} not resolvable to a library_root"))?;

    let abs_path = std::path::Path::new(&root_path).join(relative_path);
    let (size_bytes, mtime) = app_core_targets::frame_writer::stat_frame(&abs_path)
        .ok_or_else(|| format!("stat failed for {}", abs_path.display()))?;

    app_core_targets::frame_writer::upsert_frame_record(
        pool,
        root_id,
        relative_path,
        size_bytes,
        &mtime,
        "classified",
    )
    .await
    .map_err(|e| format!("upsert_frame_record: {e:?}"))
}

/// Called when a plan is discarded (any state → discarded).
async fn handle_plan_discarded(pool: &SqlitePool, plan_id: &str) -> Result<(), String> {
    transition_via_plan_id(pool, plan_id, None).await
}

/// Find the InboxItem linked to `plan_id`, transition it, and delete the plan
/// link row.
///
/// `new_state = None` means "back to unconfirmed": the state is then derived
/// from the row's own `frame_type` instead of being asserted, so a row with no
/// frame type cannot be sent to `classified` (spec 058 SC-003).
pub(crate) async fn transition_via_plan_id(
    pool: &SqlitePool,
    plan_id: &str,
    new_state: Option<&str>,
) -> Result<(), String> {
    // Find the inbox item linked to this plan.
    let link = inbox_repo::get_plan_link_by_plan_id(pool, plan_id)
        .await
        .map_err(|e| format!("get_plan_link_by_plan_id({plan_id}): {e}"))?;

    let Some(link) = link else {
        // No inbox item is linked to this plan — normal (non-inbox plans).
        return Ok(());
    };

    // Update inbox item state.
    match new_state {
        Some(state) => inbox_repo::update_inbox_item_state(pool, &link.inbox_item_id, state).await,
        None => inbox_repo::reset_inbox_item_to_unconfirmed(pool, &link.inbox_item_id).await,
    }
    .map_err(|e| format!("update_inbox_item_state({}): {e}", link.inbox_item_id))?;

    // Delete the plan link so the item can accept a new plan in the future.
    inbox_repo::delete_plan_link(pool, &link.inbox_item_id)
        .await
        .map_err(|e| format!("delete_plan_link({}): {e}", link.inbox_item_id))?;

    tracing::info!(
        inbox_item_id = %link.inbox_item_id,
        plan_id,
        new_state,
        "inbox plan_listener: inbox item transitioned"
    );

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use audit::bus::EventBus;
    use audit::event_bus::{PlanApplyingCompleted, Source};
    use persistence_core::Database;
    use persistence_inbox::repositories::inbox::InsertInboxItem;
    use persistence_plans::repositories::plans;
    use targeting_resolver::cache::upsert_resolved;
    use targeting_resolver::{
        AliasKind, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource,
    };

    pub async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    /// Poll `check` every 25 ms until it returns `Some(T)`, or panic after 2 s.
    ///
    /// Mirrors `app_core/tests/support::poll_until` (PR #1470): replaces
    /// fixed `tokio::time::sleep` barriers that fail on loaded Windows CI
    /// runners where the scheduler may not wake within a short deadline.
    async fn poll_until<F, Fut, T>(mut check: F, deadline_msg: &str) -> T
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Option<T>>,
    {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(v) = check().await {
                return v;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "poll_until timed out after 2 s: {deadline_msg}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// Poll until `inbox_items.state` for `item_id` equals `expected`, panic
    /// after 2 s. Replaces fixed-duration sleeps that are fragile on Windows.
    async fn wait_item_state(pool: &sqlx::SqlitePool, item_id: &str, expected: &str) {
        let owned_id = item_id.to_owned();
        let owned_expected = expected.to_owned();
        poll_until(
            move || {
                let id = owned_id.clone();
                let exp = owned_expected.clone();
                let pool = pool.clone();
                async move {
                    let row: Option<(String,)> =
                        sqlx::query_as("SELECT state FROM inbox_items WHERE id = ?")
                            .bind(&id)
                            .fetch_optional(&pool)
                            .await
                            .expect("poll inbox_items state");
                    match row {
                        Some((s,)) if s == exp => Some(()),
                        _ => None,
                    }
                }
            },
            &format!("inbox item {item_id} never reached state '{expected}'"),
        )
        .await;
    }

    /// Poll until the `inbox_plan_links` row for `item_id` is gone, panic after
    /// 2 s.
    ///
    /// `transition_via_plan_id` updates the item state and deletes the link as
    /// two separate writes, so observing the new state does NOT imply the link
    /// is already gone — asserting on it directly after `wait_item_state` is a
    /// race that fails intermittently under load.
    async fn wait_plan_link_deleted(pool: &sqlx::SqlitePool, item_id: &str) {
        let owned_id = item_id.to_owned();
        poll_until(
            move || {
                let id = owned_id.clone();
                let pool = pool.clone();
                async move {
                    let link = inbox_repo::get_plan_link(&pool, &id).await.expect("poll plan link");
                    link.is_none().then_some(())
                }
            },
            &format!("plan link for inbox item {item_id} was never deleted"),
        )
        .await;
    }

    fn make_bus(db: &Database) -> EventBus {
        EventBus::with_pool(db.pool().clone())
    }

    async fn setup_item_with_plan(db: &Database, item_id: &str, plan_id: &str) {
        inbox_repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "test",
                file_count: 1,
                content_signature: Some("sig"),
                lane: "fits",
            },
        )
        .await
        .unwrap();

        inbox_repo::upsert_classification(
            db.pool(),
            &inbox_repo::UpsertClassification {
                inbox_item_id: item_id,
                result: "classified",
                frame_type: Some("light"),
                content_signature: "sig",
                unclassified_file_count: 0,
            },
        )
        .await
        .unwrap();

        // A confirmable item carries its own frame type (spec 058 SC-003);
        // `insert_inbox_item` leaves it NULL, which would model an illegal row.
        sqlx::query("UPDATE inbox_items SET frame_type = 'light' WHERE id = ?")
            .bind(item_id)
            .execute(db.pool())
            .await
            .unwrap();

        inbox_repo::update_inbox_item_state(db.pool(), item_id, "plan_open").await.unwrap();

        let plan = plans::InsertPlan {
            id: plan_id,
            title: "Test plan",
            origin: "inbox",
            origin_path: None,
            plan_type: "split",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        };
        plans::insert_plan(db.pool(), &plan).await.unwrap();

        inbox_repo::insert_plan_link(db.pool(), item_id, plan_id).await.unwrap();
    }

    #[tokio::test]
    async fn applied_plan_transitions_to_resolved() {
        let db = test_db().await;
        let bus = make_bus(&db);
        setup_item_with_plan(&db, "item-t1", "plan-t1").await;

        start_inbox_plan_listener(db.pool().clone(), &bus, ResolveCache::in_memory().unwrap());

        let payload = PlanApplyingCompleted {
            plan_id: "plan-t1".to_owned(),
            run_id: "run-1".to_owned(),
            terminal_state: "applied".to_owned(),
            items_applied: 1,
            items_failed: 0,
            items_skipped: 0,
            items_cancelled: 0,
            at: "2025-10-10T22:00:00Z".to_owned(),
        };

        bus.publish(TOPIC_PLAN_APPLYING_COMPLETED, Source::System, payload).await.unwrap();

        wait_item_state(db.pool(), "item-t1", "resolved").await;

        let item = inbox_repo::get_inbox_item(db.pool(), "item-t1").await.unwrap();
        assert_eq!(item.state, "resolved");

        wait_plan_link_deleted(db.pool(), "item-t1").await;
    }

    /// issue #1256: a `plan.applying.completed`("applied") event must trigger
    /// prompt resolution of pending `ingest_resolution` rows on its own — no
    /// periodic backstop task is running in this test at all, so if the event
    /// path didn't drain-and-backfill immediately, this row would never
    /// resolve within the test's short sleep window (it previously only
    /// resolved on a ~30s timer this test doesn't run).
    #[tokio::test]
    async fn applied_plan_triggers_prompt_target_resolution() {
        let db = test_db().await;
        let bus = make_bus(&db);
        setup_item_with_plan(&db, "item-t9", "plan-t9").await;

        // A `library_root` + `file_record` so the `ingest_resolution` FK holds,
        // plus a `pending` row left over from an earlier (unrelated) ingest —
        // mirrors `app_core_targets::ingest_resolution`'s own
        // `drain_cache_hit_resolves_without_resolver` test fixture.
        let root_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
             VALUES (?, 'test', '/tmp/test', 'local', 'active', '2026-01-01T00:00:00Z')",
        )
        .bind(&root_id)
        .execute(db.pool())
        .await
        .unwrap();
        let image_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO file_record
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at)
             VALUES (?, ?, 'pending.fits', 1, '2026-01-01T00:00:00Z', 'observed',
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(&image_id)
        .bind(&root_id)
        .execute(db.pool())
        .await
        .unwrap();

        // Seed a resolvable canonical target (cache/seed hit — no network
        // needed) and force a `pending` row directly, bypassing the inline
        // cache-hit path in `associate_or_enqueue`.
        upsert_resolved(
            db.pool(),
            &ResolvedIdentity {
                simbad_oid: Some(1_575_544),
                primary_designation: "M 31".to_owned(),
                common_name: Some("Andromeda Galaxy".to_owned()),
                object_type: ObjectType::Galaxy,
                ra_deg: 10.684_708,
                dec_deg: 41.268_75,
                v_mag: None,
                aliases: vec![
                    ResolvedAlias::new("M 31", AliasKind::Designation),
                    ResolvedAlias::new("NGC 224", AliasKind::Designation),
                ],
                source: TargetSource::Resolved,
            },
        )
        .await
        .unwrap();
        app_core_targets::ingest_resolution::enqueue(db.pool(), &image_id, "NGC 224")
            .await
            .unwrap();

        start_inbox_plan_listener(db.pool().clone(), &bus, ResolveCache::in_memory().unwrap());

        let payload = PlanApplyingCompleted {
            plan_id: "plan-t9".to_owned(),
            run_id: "run-t9".to_owned(),
            terminal_state: "applied".to_owned(),
            items_applied: 1,
            items_failed: 0,
            items_skipped: 0,
            items_cancelled: 0,
            at: "2026-07-20T00:00:00Z".to_owned(),
        };
        bus.publish(TOPIC_PLAN_APPLYING_COMPLETED, Source::System, payload).await.unwrap();

        // Well under the 30s periodic backstop interval — proves the event
        // path itself resolves promptly rather than depending on the timer.
        // Uses poll_until (25 ms poll, 2 s cap) instead of a fixed sleep so
        // Windows CI runners under load don't time out (PR #1470 pattern).
        let owned_image_id = image_id.clone();
        let pool_for_poll = db.pool().clone();
        poll_until(
            move || {
                let id = owned_image_id.clone();
                let pool = pool_for_poll.clone();
                async move {
                    let row: Option<(String,)> =
                        sqlx::query_as("SELECT state FROM ingest_resolution WHERE image_id = ?")
                            .bind(&id)
                            .fetch_optional(&pool)
                            .await
                            .expect("poll ingest_resolution state");
                    match row {
                        Some((s,)) if s == "resolved" => Some(()),
                        _ => None,
                    }
                }
            },
            &format!("ingest_resolution row for image {image_id} never reached 'resolved'"),
        )
        .await;

        let (state, target_id): (String, Option<String>) =
            sqlx::query_as("SELECT state, target_id FROM ingest_resolution WHERE image_id = ?")
                .bind(&image_id)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(
            state, "resolved",
            "plan-applied event must trigger prompt resolution, not wait on a 30s backstop"
        );
        assert!(target_id.is_some());
    }

    #[tokio::test]
    async fn failed_plan_transitions_back_to_classified() {
        let db = test_db().await;
        let bus = make_bus(&db);
        setup_item_with_plan(&db, "item-t2", "plan-t2").await;

        start_inbox_plan_listener(db.pool().clone(), &bus, ResolveCache::in_memory().unwrap());

        let payload = PlanApplyingCompleted {
            plan_id: "plan-t2".to_owned(),
            run_id: "run-2".to_owned(),
            terminal_state: "failed".to_owned(),
            items_applied: 0,
            items_failed: 1,
            items_skipped: 0,
            items_cancelled: 0,
            at: "2025-10-10T22:00:00Z".to_owned(),
        };

        bus.publish(TOPIC_PLAN_APPLYING_COMPLETED, Source::System, payload).await.unwrap();

        wait_item_state(db.pool(), "item-t2", "classified").await;

        let item = inbox_repo::get_inbox_item(db.pool(), "item-t2").await.unwrap();
        assert_eq!(item.state, "classified");
    }

    #[tokio::test]
    async fn discarded_plan_transitions_to_classified() {
        let db = test_db().await;
        let bus = make_bus(&db);
        setup_item_with_plan(&db, "item-t3", "plan-t3").await;

        start_inbox_plan_listener(db.pool().clone(), &bus, ResolveCache::in_memory().unwrap());

        let payload = audit::event_bus::PlanDiscarded {
            plan_id: "plan-t3".to_owned(),
            prior_state: "ready_for_review".to_owned(),
            discarded_at: "2025-10-10T22:00:00Z".to_owned(),
        };

        bus.publish(TOPIC_PLAN_DISCARDED, Source::User, payload).await.unwrap();

        wait_item_state(db.pool(), "item-t3", "classified").await;

        let item = inbox_repo::get_inbox_item(db.pool(), "item-t3").await.unwrap();
        assert_eq!(item.state, "classified");
    }

    /// spec 058 SC-003 on the `plan.discarded` listener — the sibling writer to
    /// `cancel_inbox_plan`. Both used to stamp the literal `classified`, so a
    /// guard on either one alone leaves the other lying about the row.
    #[tokio::test]
    async fn discarded_plan_does_not_report_classified_without_a_frame_type_sc003() {
        let db = test_db().await;
        let bus = make_bus(&db);
        setup_item_with_plan(&db, "item-sc003", "plan-sc003").await;
        sqlx::query("UPDATE inbox_items SET frame_type = NULL WHERE id = 'item-sc003'")
            .execute(db.pool())
            .await
            .unwrap();

        start_inbox_plan_listener(db.pool().clone(), &bus, ResolveCache::in_memory().unwrap());

        let payload = audit::event_bus::PlanDiscarded {
            plan_id: "plan-sc003".to_owned(),
            prior_state: "ready_for_review".to_owned(),
            discarded_at: "2025-10-10T22:00:00Z".to_owned(),
        };
        bus.publish(TOPIC_PLAN_DISCARDED, Source::User, payload).await.unwrap();

        wait_item_state(db.pool(), "item-sc003", "pending_classification").await;

        let item = inbox_repo::get_inbox_item(db.pool(), "item-sc003").await.unwrap();
        assert_eq!(
            item.state, "pending_classification",
            "an item with no frame type must not report `classified` after its plan is discarded"
        );
    }

    // ── spec 048 US1/T012: calibration master frame_ids population ─────────────

    /// Set up a real-file, applied (`item_state='succeeded'`) master-item plan
    /// linked to `item_id`/`plan_id`, with the master file written under
    /// `tmp`/`rel` at `size` bytes. Returns `(root_id, rel)`.
    pub async fn setup_master_item_plan(
        db: &Database,
        tmp: &std::path::Path,
        item_id: &str,
        plan_id: &str,
        size: usize,
    ) -> (&'static str, &'static str) {
        let root_id = "cal-root";
        let rel = "master_dark.fits";
        std::fs::write(tmp.join(rel), vec![0u8; size]).unwrap();

        sqlx::query(
            "INSERT INTO registered_sources (id, kind, path, scan_depth, created_at, created_via)
             VALUES (?, 'calibration', ?, 'recursive', '2026-01-01T00:00:00Z', 'first_run')",
        )
        .bind(root_id)
        .bind(tmp.to_str().unwrap())
        .execute(db.pool())
        .await
        .unwrap();

        inbox_repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id,
                relative_path: rel,
                file_count: 1,
                content_signature: Some("sig"),
                lane: "fits",
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE inbox_items SET is_master_item = 1, master_frame_type = 'dark' WHERE id = ?",
        )
        .bind(item_id)
        .execute(db.pool())
        .await
        .unwrap();
        inbox_repo::update_inbox_item_state(db.pool(), item_id, "plan_open").await.unwrap();

        plans::insert_plan(
            db.pool(),
            &plans::InsertPlan {
                id: plan_id,
                title: "Master apply",
                origin: "inbox",
                origin_path: None,
                plan_type: "split",
                destructive_destination: "archive",
                parent_plan_id: None,
                total_bytes_required: 0,
            },
        )
        .await
        .unwrap();
        plans::insert_plan_item(
            db.pool(),
            &plans::InsertPlanItem {
                id: "master-plan-1-item-0",
                plan_id,
                item_index: 0,
                name: "[DARK MASTER] master_dark.fits",
                action: "catalogue",
                from_root_id: Some(root_id),
                from_relative_path: rel,
                to_root_id: Some(root_id),
                to_relative_path: rel,
                reason: "inbox_master",
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
        sqlx::query("UPDATE plan_items SET item_state = 'succeeded' WHERE plan_id = ?")
            .bind(plan_id)
            .execute(db.pool())
            .await
            .unwrap();

        inbox_repo::insert_plan_link(db.pool(), item_id, plan_id).await.unwrap();
        (root_id, rel)
    }

    /// A completed master-item plan must write a real-sized `file_record` for
    /// the applied master file and reference it in `calibration_session.
    /// frame_ids`, replacing the historical `'[]'` placeholder.
    #[tokio::test]
    async fn master_item_apply_writes_frame_record_and_frame_ids() {
        let db = test_db().await;
        let bus = make_bus(&db);
        let tmp = tempfile::tempdir().unwrap();
        let item_id = "master-item-1";
        let plan_id = "master-plan-1";
        let size: usize = 4096;
        let (root_id, rel) = setup_master_item_plan(&db, tmp.path(), item_id, plan_id, size).await;

        start_inbox_plan_listener(db.pool().clone(), &bus, ResolveCache::in_memory().unwrap());

        let payload = PlanApplyingCompleted {
            plan_id: plan_id.to_owned(),
            run_id: "run-master-1".to_owned(),
            terminal_state: "applied".to_owned(),
            items_applied: 1,
            items_failed: 0,
            items_skipped: 0,
            items_cancelled: 0,
            at: "2026-07-04T00:00:00Z".to_owned(),
        };
        bus.publish(TOPIC_PLAN_APPLYING_COMPLETED, Source::System, payload).await.unwrap();

        // Poll for the calibration_session row rather than sleeping a fixed
        // duration — same PR #1470 pattern applied to the master apply path.
        let item_id_for_poll = item_id.to_owned();
        let pool_for_poll = db.pool().clone();
        poll_until(
            move || {
                let id = item_id_for_poll.clone();
                let pool = pool_for_poll.clone();
                async move {
                    let row: Option<(String,)> = sqlx::query_as(
                        "SELECT frame_ids FROM calibration_session \
                         WHERE source_inbox_item_id = ?",
                    )
                    .bind(&id)
                    .fetch_optional(&pool)
                    .await
                    .expect("poll calibration_session");
                    row.map(|_| ())
                }
            },
            &format!("calibration_session for item {item_id} never appeared"),
        )
        .await;

        let (frame_ids_json,): (String,) = sqlx::query_as(
            "SELECT frame_ids FROM calibration_session WHERE source_inbox_item_id = ?",
        )
        .bind(item_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
        let frame_ids: Vec<String> = serde_json::from_str(&frame_ids_json).unwrap();
        assert_eq!(frame_ids.len(), 1, "frame_ids must reference the applied master file_record");

        let (size_bytes, stored_root, stored_rel): (i64, String, String) = sqlx::query_as(
            "SELECT size_bytes, root_id, relative_path FROM file_record WHERE id = ?",
        )
        .bind(&frame_ids[0])
        .fetch_one(db.pool())
        .await
        .unwrap();
        let expected_size = i64::try_from(size).unwrap();
        assert_eq!(size_bytes, expected_size, "real on-disk size, never 0");
        assert_eq!(stored_root, root_id);
        assert_eq!(stored_rel, rel);

        // Regression for #470 round 6's twin bug: `calibration_session.root_id`
        // (migration 0021) had the identical never-written gap as
        // `acquisition_session.root_id` — this test already drives the real
        // registered_sources → plan-apply → plan_listener path, so it is the
        // right place to lock the fix in rather than adding a parallel test.
        let (session_root_id,): (String,) = sqlx::query_as(
            "SELECT root_id FROM calibration_session WHERE source_inbox_item_id = ?",
        )
        .bind(item_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(
            session_root_id, root_id,
            "calibration_session.root_id must be set to the real registered source id, not left \
             empty/unset"
        );
    }
}
