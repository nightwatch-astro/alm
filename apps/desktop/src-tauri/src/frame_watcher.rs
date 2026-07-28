// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-root live frame watcher registry (spec 048 T023/T024/T026).
//!
//! One OS watcher per *attached* raw/calibration root, attached when a surface
//! that shows frame inventory for that root opens and detached when it closes
//! (research R2: "Detach when the relevant surface closes; do not hold live
//! watches on idle roots indefinitely"). Modeled on
//! [`crate::watcher::ArtifactWatcherRegistry`], including its detach-during-
//! attach tombstone.
//!
//! Live events never write records. A debounced event batch schedules one
//! scoped reconcile pass over the root (R2: "Live events act as triggers that
//! schedule a scoped rescan rather than mutating records directly"), which is
//! also the only path that confirms deletes and moves.
//!
//! Per-root `detection` config decides what runs (`app_core::settings::
//! root_config`):
//!
//! - `live` (default true): attach an OS watcher. `false` is the
//!   removable/network opt-out — the root then relies on its other triggers.
//! - `scheduled` (default false): a periodic reconcile while attached, which
//!   doubles as the polling fallback when `live` is off.
//! - `on_open` (default false): one reconcile at attach time.
//! - `follow_symlinks` (default false): passed through to the watcher and the
//!   reconcile walker, which both refuse to traverse links unless enabled
//!   (constitution; R6).
//!
//! A root with every trigger off is attached as an empty entry so detach stays
//! symmetric and the on-demand `inventory.reconcile.run` command remains the
//! user's escape hatch.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use audit::bus::EventBus;
use camino::Utf8PathBuf;
use contracts_core::inventory_frame::{
    InventoryReconcileRunRequest, ReconcileReason, RootInventoryConfig,
};
use fs_inventory::artifact_watcher::{
    start_artifact_watcher, ArtifactEventKind, ArtifactFileEvent, WatcherGuard,
};
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// How long a burst of filesystem events is collected before one scoped
/// reconcile is scheduled. Matches the artifact watcher's stable-size debounce
/// window: a capture session writes frames continuously, and a reconcile pass
/// walks the whole root, so coalescing a burst into one pass is what keeps the
/// cost bounded.
const EVENT_DEBOUNCE: Duration = workflow_artifacts::DEFAULT_STABILITY_DEBOUNCE;

/// Cadence for the opt-in `detection.scheduled` trigger, which is also the
/// polling fallback when `detection.live` is off (R2). Deliberately coarse:
/// a reconcile walks the entire root, and every other trigger (live, on-open,
/// on-demand) covers the latency-sensitive cases.
const SCHEDULED_INTERVAL: Duration = Duration::from_mins(15);

/// A single root's live watcher plus its event-draining task.
pub struct FrameWatcherEntry {
    /// `None` when the root's `detection.live` is off (opt-out) — the entry
    /// then only carries its scheduled trigger. Dropping the guard stops the
    /// OS watcher and closes the channel, which ends `drain_task`.
    _guard: Option<WatcherGuard>,
    drain_task: Option<JoinHandle<()>>,
    scheduled_task: Option<JoinHandle<()>>,
}

impl FrameWatcherEntry {
    fn abort(&self) {
        if let Some(task) = &self.drain_task {
            task.abort();
        }
        if let Some(task) = &self.scheduled_task {
            task.abort();
        }
    }
}

/// Inner state guarded by the registry mutex.
///
/// `detach_requested` is the same tombstone set
/// [`crate::watcher::WatcherRegistryInner`] uses: `detach_root_watcher` writes
/// here when the root has no live entry yet (detach arrived while attach was
/// doing unlocked work), and `attach_root_watcher` consumes it at final-insert
/// time instead of leaving a zombie entry behind.
pub struct FrameWatcherRegistryInner {
    pub entries: HashMap<String, FrameWatcherEntry>,
    pub detach_requested: HashSet<String>,
}

/// Registry of per-root live frame watchers, managed as Tauri state.
pub type FrameWatcherRegistry = Arc<Mutex<FrameWatcherRegistryInner>>;

/// Construct an empty registry (call once at app startup and `app.manage()` it).
#[must_use]
pub fn new_frame_watcher_registry() -> FrameWatcherRegistry {
    Arc::new(Mutex::new(FrameWatcherRegistryInner {
        entries: HashMap::new(),
        detach_requested: HashSet::new(),
    }))
}

/// Run one scoped reconcile pass over `root_id` with the given trigger reason.
///
/// Failures are logged, never propagated: a trigger is best-effort background
/// work, and the next trigger (or the on-demand command) retries.
async fn schedule_reconcile(
    pool: &SqlitePool,
    bus: &EventBus,
    root_id: &str,
    reason: ReconcileReason,
) {
    let req = InventoryReconcileRunRequest { root_id: root_id.to_owned(), reason };
    match app_core::frame_inventory::run_reconcile(pool, bus, &req).await {
        Ok(resp) => {
            tracing::debug!(
                root_id,
                ?reason,
                scanned = resp.scanned,
                newly_missing = resp.newly_missing,
                recovered = resp.recovered,
                "frame watcher: reconcile pass complete"
            );
        }
        Err(error) => {
            tracing::warn!(
                root_id,
                ?reason,
                error = error.message,
                "frame watcher: reconcile pass failed"
            );
        }
    }
}

/// Drain live events for one root, coalescing each burst into a single
/// `live_event` reconcile.
///
/// A `NeedsRescan` event (OS watcher error) or a dropped-event overflow is
/// treated exactly like a file event: both mean the live signal is incomplete,
/// and the reconcile pass is what recovers consistency either way.
fn spawn_drain_task(
    mut rx: tokio::sync::mpsc::Receiver<ArtifactFileEvent>,
    overflow_flag: Arc<AtomicBool>,
    pool: SqlitePool,
    bus: EventBus,
    root_id: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut dirty = false;
        let mut debounce = tokio::time::interval(EVENT_DEBOUNCE);
        debounce.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                maybe_evt = rx.recv() => {
                    let Some(evt) = maybe_evt else {
                        tracing::debug!(root_id, "frame watcher: event channel closed");
                        break;
                    };
                    // Every kind marks the root dirty. Removals included: the
                    // pass is what confirms a delete or a move (R2), so a
                    // removal must still schedule one.
                    if evt.kind == ArtifactEventKind::NeedsRescan {
                        tracing::warn!(root_id, "frame watcher: OS watcher error, scheduling reconcile");
                    }
                    dirty = true;
                }
                _ = debounce.tick() => {
                    if overflow_flag.swap(false, Ordering::AcqRel) {
                        tracing::warn!(root_id, "frame watcher: event overflow, scheduling reconcile");
                        dirty = true;
                    }
                    if dirty {
                        dirty = false;
                        schedule_reconcile(&pool, &bus, &root_id, ReconcileReason::LiveEvent).await;
                    }
                }
            }
        }
    })
}

/// Spawn the opt-in `detection.scheduled` periodic reconcile for one root.
fn spawn_scheduled_task(pool: SqlitePool, bus: EventBus, root_id: String) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SCHEDULED_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick completes immediately; skip it so `scheduled` does not
        // double up with the `on_open` trigger at attach time.
        interval.tick().await;
        loop {
            interval.tick().await;
            schedule_reconcile(&pool, &bus, &root_id, ReconcileReason::Scheduled).await;
        }
    })
}

/// Resolve a root id to its on-disk path, preferring the `library_root` row
/// and falling back to the `registered_sources` row a root registered through
/// the setup wizard has before its first ingest mirrors it (spec 048's
/// reconcile command resolves `library_root` only, but a root can be attached
/// from the UI before any frame has been ingested under it).
async fn resolve_root_path(pool: &SqlitePool, root_id: &str) -> Option<String> {
    match persistence_targets::repositories::inventory::get_library_root_path(pool, root_id).await {
        Ok(Some(path)) => return Some(path),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(root_id, %error, "frame watcher: library_root lookup failed");
            return None;
        }
    }
    match persistence_lifecycle::repositories::first_run::get_source_path(pool, root_id).await {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(root_id, %error, "frame watcher: registered_sources lookup failed");
            None
        }
    }
}

/// Build the entry for a root whose config has been read and whose path is
/// known to be a directory. Split out of [`attach_root_watcher`] so that
/// function stays within the workspace line budget.
fn build_entry(
    pool: &SqlitePool,
    bus: &EventBus,
    root_id: &str,
    root_path: &Path,
    config: RootInventoryConfig,
) -> Result<FrameWatcherEntry, String> {
    let live = if config.detection.live {
        let root_utf8 = Utf8PathBuf::from_path_buf(root_path.to_path_buf())
            .map_err(|_| format!("root path is not valid UTF-8: {}", root_path.display()))?;
        let (rx, guard) = start_artifact_watcher(
            std::slice::from_ref(&root_utf8),
            256,
            config.detection.follow_symlinks,
        )
        .map_err(|e| format!("failed to start frame watcher: {e}"))?;
        let drain_task = spawn_drain_task(
            rx,
            Arc::clone(&guard.overflow_flag),
            pool.clone(),
            bus.clone(),
            root_id.to_owned(),
        );
        Some((guard, drain_task))
    } else {
        None
    };

    let scheduled_task = config
        .detection
        .scheduled
        .then(|| spawn_scheduled_task(pool.clone(), bus.clone(), root_id.to_owned()));

    let (guard, drain_task) = match live {
        Some((guard, task)) => (Some(guard), Some(task)),
        None => (None, None),
    };
    Ok(FrameWatcherEntry { _guard: guard, drain_task, scheduled_task })
}

/// Attach the live/scheduled detection triggers for `root_id`.
///
/// Idempotent: attaching an already-attached root is a no-op (guards against
/// duplicate mount effects, e.g. React `StrictMode`).
///
/// An unavailable root directory (e.g. an unmounted external drive) is NOT an
/// error — it logs and returns `Ok(())` so the caller can retry later, matching
/// [`crate::watcher::attach_project_watcher`].
///
/// # Lock discipline
/// The registry lock is held only for the two O(1) map operations. The config
/// read, the path probe, the on-open reconcile, and OS-watcher startup all run
/// unlocked, so one root's slow external drive never serialises another root's
/// attach. The second acquisition re-checks the key to detect a concurrent
/// racer and consumes any detach tombstone written while we were unlocked.
///
/// # Errors
/// Returns `Err(String)` if the root's config cannot be read or its OS watcher
/// cannot be started.
pub async fn attach_root_watcher(
    pool: &SqlitePool,
    bus: &EventBus,
    registry: &FrameWatcherRegistry,
    root_id: &str,
) -> Result<(), String> {
    {
        let mut reg = registry.lock().await;
        if reg.entries.contains_key(root_id) {
            return Ok(());
        }
        // Clear a stale tombstone from a detach that arrived with no attach in
        // flight; without this a detach/attach cycle kills the next attach.
        reg.detach_requested.remove(root_id);
    }

    let config = app_core::settings::root_config::get_root_config(pool, root_id)
        .await
        .map_err(|e| format!("failed to read root config: {}", e.message))?;

    let Some(root_path_str) = resolve_root_path(pool, root_id).await else {
        tracing::warn!(root_id, "frame watcher: root is not registered, not attaching");
        return Ok(());
    };

    // spawn_blocking: is_dir() can stall the runtime on a stale NFS/SMB mount —
    // exactly the storage class this feature's opt-out targets.
    let probe_path = root_path_str.clone();
    let available =
        tokio::task::spawn_blocking(move || Path::new(&probe_path).is_dir()).await.unwrap_or(false);
    if !available {
        tracing::warn!(root_id, path = %root_path_str, "frame watcher: root unavailable, not attaching");
        return Ok(());
    }

    if config.detection.on_open {
        schedule_reconcile(pool, bus, root_id, ReconcileReason::OnOpen).await;
    }

    let entry = build_entry(pool, bus, root_id, Path::new(&root_path_str), config)?;

    let mut reg = registry.lock().await;
    if reg.entries.contains_key(root_id) {
        entry.abort();
        tracing::debug!(root_id, "frame watcher: concurrent attach, discarding duplicate");
        return Ok(());
    }
    if reg.detach_requested.remove(root_id) {
        entry.abort();
        tracing::debug!(root_id, "frame watcher: detach arrived during attach, discarding");
        return Ok(());
    }
    tracing::info!(
        root_id,
        live = config.detection.live,
        scheduled = config.detection.scheduled,
        "frame watcher: attached"
    );
    reg.entries.insert(root_id.to_owned(), entry);
    drop(reg);
    Ok(())
}

/// Detach `root_id`'s watcher and scheduled trigger, if attached.
///
/// Idempotent. If no live entry is present (an attach is in flight, unlocked),
/// a tombstone is recorded so the finishing attach discards its watcher rather
/// than leaving a zombie holding an OS watcher on an idle root.
pub async fn detach_root_watcher(registry: &FrameWatcherRegistry, root_id: &str) {
    let mut reg = registry.lock().await;
    if let Some(entry) = reg.entries.remove(root_id) {
        entry.abort();
        // Dropping `entry` here stops the OS watcher.
        tracing::info!(root_id, "frame watcher: detached");
    } else {
        reg.detach_requested.insert(root_id.to_owned());
    }
}

/// Re-attach `root_id` only if it is currently attached, so a detection-config
/// change takes effect on the live watch without attaching a root whose surface
/// is closed.
pub async fn reattach_if_attached(
    pool: &SqlitePool,
    bus: &EventBus,
    registry: &FrameWatcherRegistry,
    root_id: &str,
) {
    {
        let reg = registry.lock().await;
        if !reg.entries.contains_key(root_id) {
            return;
        }
    }
    detach_root_watcher(registry, root_id).await;
    if let Err(error) = attach_root_watcher(pool, bus, registry, root_id).await {
        tracing::warn!(root_id, %error, "frame watcher: re-attach after config change failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn attach_unregistered_root_is_not_an_error_and_attaches_nothing() {
        let db = persistence_core::Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let bus = EventBus::with_pool(db.pool().clone());
        let registry = new_frame_watcher_registry();

        attach_root_watcher(db.pool(), &bus, &registry, "no-such-root").await.unwrap();

        assert!(registry.lock().await.entries.is_empty());
    }

    #[tokio::test]
    async fn detach_before_attach_leaves_a_tombstone_that_attach_consumes() {
        let registry = new_frame_watcher_registry();
        detach_root_watcher(&registry, "root-1").await;
        assert!(registry.lock().await.detach_requested.contains("root-1"));
    }
}
