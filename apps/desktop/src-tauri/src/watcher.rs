// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Bridge between the `fs_inventory::watcher::WatcherService` and the Tauri
//! event system.
//!
//! `start_watcher` spawns a background task that subscribes to watcher events
//! and forwards them to the webview via `app_handle.emit("inbox:file-change", payload)`.
//! `stop_watcher` shuts the watcher down and the forwarding task exits on the
//! next loop iteration when the broadcast channel is closed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;

use audit::bus::EventBus;
use fs_inventory::artifact_watcher::{
    start_artifact_watcher, ArtifactEventKind, ArtifactFileEvent, WatcherGuard,
};
use fs_inventory::watcher::{InboxFileEvent, SharedWatcherService};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use workflow_artifacts::{
    check_stability, FileSnapshot, StabilityStatus, DEFAULT_STABILITY_DEBOUNCE,
};

/// How often the per-project watcher re-checks its open tool launches for
/// completion (#727). The attribution window itself defaults to 6h
/// (`workflow_artifacts::DEFAULT_ATTRIBUTION_WINDOW`); this only bounds how
/// stale that check can be, so a coarse interval is deliberate — there is no
/// user-facing latency requirement on top of the attribution window itself.
const STALE_LAUNCH_SWEEP_INTERVAL: Duration = Duration::from_mins(5);

/// Start watching the given inbox paths and forward events to the Tauri webview.
///
/// # Errors
///
/// Returns an error string if the underlying watcher cannot be started.
pub async fn start_watcher(
    app_handle: AppHandle,
    watcher_service: SharedWatcherService,
    paths: &[Utf8PathBuf],
) -> Result<(), String> {
    let mut rx = {
        let mut svc = watcher_service.lock().await;
        let rx = svc.subscribe();
        // Constitution: never follow symlinks/junctions unless explicitly
        // enabled per root (spec 048 T004). Inbox folders are not (yet) a
        // per-root-configurable surface, so they default to the safe gate.
        svc.start(paths, false)?;
        rx
    };

    // Spawn a background task to forward watcher events to the webview.
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let payload = match &event {
                        InboxFileEvent::Added { path } => {
                            serde_json::json!({ "kind": "added", "path": path })
                        }
                        InboxFileEvent::Removed { path } => {
                            serde_json::json!({ "kind": "removed", "path": path })
                        }
                        InboxFileEvent::Modified { path } => {
                            serde_json::json!({ "kind": "modified", "path": path })
                        }
                        InboxFileEvent::NeedsRescan { reason } => {
                            // Design note: the inbox watcher forwards NeedsRescan to
                            // the frontend rather than running a backend rescan here.
                            // Inbox folders hold unprocessed drops; there is no per-path
                            // DB state to reconcile server-side. Recovery is a watcher
                            // restart (re-calling `start_watcher` / `inbox.watcher.start`
                            // from the frontend), which re-enumerates the current dir
                            // set — the correct shape for inbox. This is an intentional
                            // asymmetry with the artifact watcher (which does have DB
                            // state to reconcile and owns its own backend recovery).
                            serde_json::json!({ "kind": "needsRescan", "reason": reason })
                        }
                    };
                    // Best-effort emit — the webview may not be listening.
                    let _ = app_handle.emit("inbox:file-change", payload);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!("watcher broadcast channel closed, stopping forwarder");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("watcher event forwarder lagged by {n} events");
                }
            }
        }
    });

    Ok(())
}

/// Stop the filesystem watcher.
///
/// The background forwarding task will exit once the broadcast channel is
/// closed (which happens when the watcher is dropped).
pub async fn stop_watcher(watcher_service: SharedWatcherService) {
    let mut svc = watcher_service.lock().await;
    svc.stop();
}

// ── Artifact watcher (spec 012, FR-009, T008) ──────────────────────────────────
//
// One OS watcher per *active* project, attached when the project drawer opens
// and detached when it closes (plan.md §Sequencing: "One watcher per active
// project... This bounds the number of OS watchers, which is a scarce
// resource on macOS and Windows"). A global always-on watcher over every
// registered library root was tried first and dropped: it never bounded the
// OS-watcher count, watched entire drives recursively regardless of whether
// any project was open, and stamped the *library root* id onto
// `ProcessingArtifact.project_id` (never the real project). Re-attaching runs
// an on-attach reconciliation pass first (T005's `reconcile`) so files written
// while detached are still picked up (spec 012 edge case).

/// A single project's live watcher + its event-forwarding task.
pub struct ArtifactWatcherEntry {
    /// Kept alive only to hold the OS watcher open; dropping it stops watching
    /// and closes the event channel, which ends `forward_task`.
    _guard: WatcherGuard,
    forward_task: JoinHandle<()>,
}

/// Inner state guarded by the registry Mutex.
///
/// `entries` holds the live watcher per project. `detach_requested` is a
/// tombstone set: `detach_project_watcher` writes here when the project has
/// no live entry yet (race: detach arrived while attach was reconciling
/// unlocked). `attach_project_watcher` consumes the tombstone at final-insert
/// time and discards the watcher instead of inserting it.
pub struct WatcherRegistryInner {
    pub entries: HashMap<String, ArtifactWatcherEntry>,
    pub detach_requested: std::collections::HashSet<String>,
}

/// Registry of per-project artifact watchers.
///
/// Managed as Tauri state so `artifact_watcher_attach`/`artifact_watcher_detach`
/// commands can look up and mutate it.
pub type ArtifactWatcherRegistry = Arc<Mutex<WatcherRegistryInner>>;

/// Construct an empty registry (call once at app startup and `app.manage()` it).
#[must_use]
pub fn new_artifact_watcher_registry() -> ArtifactWatcherRegistry {
    Arc::new(Mutex::new(WatcherRegistryInner {
        entries: HashMap::new(),
        detach_requested: std::collections::HashSet::new(),
    }))
}

/// Derive the stable `[a-z][a-z0-9_]*` tool id from a project's display `tool`
/// field (data-model.md §`WorkflowBinding` resolution rule). Mirrors the
/// frontend's `toolIdFromProjectTool` in `features/projects/tool-launch.ts`:
/// `"PixInsight"` → `"pixinsight"`, `"Planetary Suite"` → `"planetary_suite"`.
fn tool_id_from_project_tool(project_tool: &str) -> String {
    project_tool.to_lowercase().split_whitespace().collect::<Vec<_>>().join("_")
}

/// Recursive directory listing of real files only (#780).
///
/// Must agree with the live watcher's `RecursiveMode::Recursive`
/// (`fs_inventory::artifact_watcher`, e.g. output/ subfolders) — a
/// non-recursive on-attach reconcile only ever saw the project root's
/// top-level files, so every reopen (a) marked every real artifact (which
/// lives under a subfolder like `output/`) `Gone`→`missing`, and (b) never
/// `detect`-ed files written to a subfolder while the project was closed.
///
/// Constitution: never follows symlinks/junctions (no per-root opt-in exists
/// for project output folders in v1) — neither into a symlinked directory
/// nor as a symlinked file.
fn real_read_dir(dir: &Path) -> Result<workflow_artifacts::DirListing, String> {
    let mut listing = workflow_artifacts::DirListing::default();
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("read_dir({}) failed: {e}", dir.display()))?;
    real_read_dir_into(entries, &mut listing.files, &mut listing.unreadable);
    Ok(listing)
}

/// Recursion helper for [`real_read_dir_reporting`]; `files` accumulates real
/// file paths across the whole subtree and `skipped` the directories whose
/// `read_dir` failed.
///
/// An unreadable subdirectory is skipped, not propagated: one
/// permission-denied directory must not abort reconciliation for every
/// artifact elsewhere under the project root (matches
/// `fs_pathsafe::real_files_under`). Callers must treat `skipped` as unknown
/// state, never as absence.
fn real_read_dir_into(
    entries: std::fs::ReadDir,
    files: &mut Vec<PathBuf>,
    skipped: &mut Vec<PathBuf>,
) {
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            match std::fs::read_dir(&path) {
                Ok(child) => real_read_dir_into(child, files, skipped),
                Err(_) => skipped.push(path),
            }
        } else if file_type.is_file() {
            files.push(path);
        }
    }
}

/// Real (size, mtime) probe. Uses `symlink_metadata` and rejects symlinks
/// explicitly — belt-and-suspenders alongside `real_read_dir`'s filter.
fn real_metadata(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        return None;
    }
    let modified = meta.modified().ok()?;
    Some((meta.len(), modified))
}

// ── Live-watch debounce (spec 012 T003/T004, #729) ─────────────────────────────

/// Record or cancel debounce tracking for a raw watcher event.
///
/// `Removed` events cancel any in-flight debounce for that path — no
/// `detect()` fires for a file that vanished before stabilizing; removals
/// otherwise stay handled by the on-attach reconciliation pass, matching the
/// existing T005 design. Non-watched extensions are ignored (pre-existing
/// filter). `Created`/`Modified` events (re-)seed the path's [`FileSnapshot`]
/// with the size observed *now*; [`sweep_pending_artifacts`] decides once the
/// size has held steady for the debounce window.
fn record_raw_event(
    evt: &ArtifactFileEvent,
    extensions: &[String],
    pending: &mut HashMap<PathBuf, FileSnapshot>,
) {
    let path = evt.path.as_std_path().to_path_buf();
    if evt.kind == ArtifactEventKind::Removed {
        pending.remove(&path);
        return;
    }

    let file_name = evt.path.file_name().unwrap_or_default();
    let ext_refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
    if !workflow_artifacts::extension_allowed(file_name, &ext_refs) {
        return;
    }

    // File already gone by the time we could stat it — nothing to track.
    if let Some((size_bytes, _)) = real_metadata(&path) {
        pending.insert(path, FileSnapshot { size_bytes, arrived_at: Instant::now() });
    }
}

/// Re-check every pending path's stability: `detect()` any that stabilized
/// with their real observed size (fixing the previously-hardcoded
/// `size_bytes: 0`), drop any that disappeared, and re-arm the debounce
/// window for any still being written.
#[expect(
    clippy::cognitive_complexity,
    reason = "per-pending-path sweep loop with stability and classification branches"
)]
async fn sweep_pending_artifacts(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    tool_id: &str,
    pending: &mut HashMap<PathBuf, FileSnapshot>,
) {
    let now = Instant::now();
    let probe = |p: &Path| real_metadata(p).map(|(size, _)| size);
    let paths: Vec<PathBuf> = pending.keys().cloned().collect();

    for path in paths {
        let Some(snapshot) = pending.get(&path).cloned() else { continue };
        match check_stability(&path, &snapshot, now, DEFAULT_STABILITY_DEBOUNCE, probe) {
            StabilityStatus::Stable { size_bytes } => {
                pending.remove(&path);
                let path_str = path.to_string_lossy().into_owned();
                let now_iso = OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
                let size_i64 = i64::try_from(size_bytes).unwrap_or(i64::MAX);
                if let Err(e) = app_core::artifact::detect(
                    pool, bus, project_id, &path_str, tool_id, size_i64, &now_iso, &now_iso,
                )
                .await
                {
                    tracing::debug!("artifact watcher: detect failed for {path_str}: {e}");
                } else {
                    tracing::debug!(
                        "artifact watcher: detected {path_str} (project {project_id}, {size_bytes} bytes)"
                    );
                }
            }
            StabilityStatus::Gone => {
                pending.remove(&path);
            }
            StabilityStatus::Writing => {
                // Re-arm only once the window actually elapsed against a
                // changed size, so the debounce restarts from the latest
                // write instead of comparing against the original snapshot
                // forever (`check_stability` alone can't distinguish "too
                // early to check" from "checked, still growing").
                if now.duration_since(snapshot.arrived_at) >= DEFAULT_STABILITY_DEBOUNCE {
                    if let Some(size_bytes) = probe(&path) {
                        pending.insert(path, FileSnapshot { size_bytes, arrived_at: now });
                    } else {
                        pending.remove(&path);
                    }
                }
            }
        }
    }
}

/// Run the on-attach reconciliation pass (T005): scan `project_root`,
/// transition DB rows that are `Gone`/`Seen`, and `detect` any `new_files`.
///
/// Split out of [`attach_project_watcher`] to keep that function's line count
/// within the workspace lint budget; it has no independent lifecycle of its
/// own (always called immediately before the live watcher starts).
#[expect(
    clippy::cognitive_complexity,
    reason = "reconciliation sweep over on-disk artifacts against recorded rows"
)]
async fn run_attach_reconciliation(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    project_root: &Path,
    tool_id: &str,
    ext_refs: &[&str],
) -> Result<(), String> {
    let known_rows = persistence_plans::repositories::artifacts::list_artifacts_for_project(
        pool,
        project_id,
        &["present"],
    )
    .await
    .map_err(|e| format!("{e}"))?;
    let known_paths: Vec<String> = known_rows.iter().map(|r| r.path.clone()).collect();

    let report = workflow_artifacts::reconcile(
        project_root,
        &known_paths,
        ext_refs,
        &real_read_dir,
        &real_metadata,
    )
    .map_err(|e| format!("reconcile failed: {e}"))?;

    let unreadable: Vec<String> =
        report.unreadable_dirs.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    if !unreadable.is_empty() {
        tracing::warn!(
            count = unreadable.len(),
            "artifact watcher: unreadable directories skipped during reconcile: {}",
            unreadable.join(", ")
        );
    }

    let mut seen_ids: Vec<String> = Vec::new();
    let mut gone: Vec<app_core::artifact::GoneArtifact> = Vec::new();
    for (rel_path, outcome) in report.existing {
        let Some(row) = known_rows.iter().find(|r| r.path == rel_path) else { continue };
        match outcome {
            workflow_artifacts::ReconcileOutcome::Gone => {
                gone.push(app_core::artifact::GoneArtifact {
                    id: row.id.clone(),
                    path: row.path.clone(),
                });
            }
            workflow_artifacts::ReconcileOutcome::Seen => seen_ids.push(row.id.clone()),
            workflow_artifacts::ReconcileOutcome::Unknown => {}
        }
    }

    let detected: Vec<app_core::artifact::DetectedFile> = report
        .new_files
        .into_iter()
        .map(|new_file| {
            let detected_at = OffsetDateTime::from(new_file.file_mtime)
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
            app_core::artifact::DetectedFile {
                path: new_file.absolute_path.to_string_lossy().into_owned(),
                size_bytes: i64::try_from(new_file.size_bytes).unwrap_or(i64::MAX),
                file_mtime: detected_at.clone(),
                detected_at,
            }
        })
        .collect();

    // One transaction per phase (kyo7.54): a project with thousands of outputs
    // otherwise pays one commit per row on every drawer open. A failed phase is
    // logged, not propagated — the remaining phases still run, matching the old
    // per-row loops' tolerance of individual failures.
    app_core::artifact::report_scan_incomplete(bus, project_id, &unreadable).await;
    if let Err(e) = app_core::artifact::touch_seen(pool, &seen_ids).await {
        tracing::warn!(count = seen_ids.len(), "artifact watcher: seen phase failed: {e}");
    }
    if let Err(e) = app_core::artifact::mark_missing_batch(pool, bus, project_id, &gone).await {
        tracing::warn!(count = gone.len(), "artifact watcher: missing phase failed: {e}");
    }
    if let Err(e) =
        app_core::artifact::detect_batch(pool, bus, project_id, tool_id, &detected).await
    {
        tracing::warn!(count = detected.len(), "artifact watcher: detect phase failed: {e}");
    }

    Ok(())
}

/// Spawn the per-project event forward task that debounces raw watcher events,
/// triggers reconciliation on error/overflow signals, and sweeps stale launches.
fn spawn_artifact_forward_task(
    mut rx: tokio::sync::mpsc::Receiver<ArtifactFileEvent>,
    overflow_flag: Arc<AtomicBool>,
    pool: SqlitePool,
    bus: EventBus,
    project_id: String,
    tool_id: String,
    extensions: Vec<String>,
    project_root: PathBuf,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut pending: HashMap<PathBuf, FileSnapshot> = HashMap::new();
        let mut debounce_tick = tokio::time::interval(DEFAULT_STABILITY_DEBOUNCE / 2);
        debounce_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut launch_sweep_tick = tokio::time::interval(STALE_LAUNCH_SWEEP_INTERVAL);
        launch_sweep_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                maybe_evt = rx.recv() => {
                    let Some(evt) = maybe_evt else {
                        tracing::debug!(
                            "artifact watcher: channel closed for project {project_id}"
                        );
                        break;
                    };
                    // Item (a): NeedsRescan means the OS watcher hit an error —
                    // run a full reconciliation pass to recover consistency.
                    if matches!(evt.kind, ArtifactEventKind::NeedsRescan) {
                        tracing::warn!(
                            "artifact watcher: NeedsRescan for project {project_id}, triggering reconciliation"
                        );
                        pending.clear();
                        let ext_refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
                        if let Err(e) = run_attach_reconciliation(
                            &pool, &bus, &project_id, &project_root, &tool_id, &ext_refs,
                        ).await {
                            tracing::warn!(
                                "artifact watcher: reconciliation after NeedsRescan failed for {project_id}: {e}"
                            );
                        }
                        continue;
                    }
                    record_raw_event(&evt, &extensions, &mut pending);
                }
                _ = debounce_tick.tick() => {
                    // Item (c): poll the atomic overflow flag (set by the notify
                    // callback when the channel was full). An AtomicBool::store
                    // cannot itself be dropped the way a try_send sentinel can.
                    if overflow_flag.swap(false, Ordering::AcqRel) {
                        tracing::warn!(
                            "artifact watcher: overflow for project {project_id}, triggering reconciliation"
                        );
                        pending.clear();
                        let ext_refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
                        if let Err(e) = run_attach_reconciliation(
                            &pool, &bus, &project_id, &project_root, &tool_id, &ext_refs,
                        ).await {
                            tracing::warn!(
                                "artifact watcher: reconciliation after overflow failed for {project_id}: {e}"
                            );
                        }
                        continue;
                    }
                    sweep_pending_artifacts(&pool, &bus, &project_id, &tool_id, &mut pending).await;
                }
                _ = launch_sweep_tick.tick() => {
                    if let Err(e) =
                        app_core::artifact::sweep_stale_launches(&pool, &bus, &project_id).await
                    {
                        tracing::debug!(
                            "artifact watcher: stale-launch sweep failed for project {project_id}: {e}"
                        );
                    }
                }
            }
        }
    })
}

/// Attach a live filesystem watcher for `project_id`'s output folder
/// (currently `Project.path`; source-view folders are out of scope, see
/// `tool_launch::launch`'s identical v1 simplification).
///
/// Idempotent: attaching an already-attached project is a no-op (guards
/// against duplicate mount effects, e.g. React `StrictMode`).
///
/// Runs an on-attach reconciliation pass (T005) before starting the live
/// watcher so files written while detached are still detected, then emits
/// `artifact.detected`/`artifact.missing` for the reconciliation results
/// exactly as the live watcher would.
///
/// # Lock discipline (audit kyo7.1)
/// The registry lock is held only for the two O(1) map operations (idempotency
/// check and final insert). All blocking work — DB queries, directory walk,
/// OS-watcher startup — runs outside the lock so concurrent attach/detach
/// calls for other projects are never serialised behind one project's
/// reconciliation. A second lock acquisition at the end re-checks the key to
/// detect a concurrent racer that inserted the same `project_id` while we were
/// reconciling; if detected, the duplicate watcher is discarded cleanly.
///
/// # Errors
/// Returns `Err(String)` if the project cannot be loaded or the watcher
/// cannot be started. An unavailable output folder (e.g. a removed external
/// drive) is NOT an error — it logs and returns `Ok(())` so the caller can
/// retry later (spec 012 edge case).
#[expect(
    clippy::cognitive_complexity,
    reason = "watcher attach sequence: idempotency check, root resolution, and debounce wiring"
)]
pub async fn attach_project_watcher(
    pool: &SqlitePool,
    bus: &EventBus,
    registry: &ArtifactWatcherRegistry,
    project_id: &str,
) -> Result<(), String> {
    // ── Fast idempotency check — drop the lock before the slow path ────────
    {
        let mut reg = registry.lock().await;
        if reg.entries.contains_key(project_id) {
            return Ok(());
        }
        // Item (e): clear any stale tombstone from a prior detach that arrived
        // when no attach was in flight. Without this, a detach/attach cycle
        // where detach lands first leaves a tombstone that kills the next
        // legitimate attach. The tombstone is only meaningful when an attach IS
        // currently in flight (between this check and the final insert below);
        // clearing it here ensures we start clean.
        reg.detach_requested.remove(project_id);
    }

    // ── All slow work runs without holding the registry lock ───────────────
    let project = persistence_plans::repositories::projects::get_project(pool, project_id)
        .await
        .map_err(|e| format!("{e}"))?;

    let project_root = PathBuf::from(&project.path);
    if !project_root.is_dir() {
        tracing::warn!(
            "artifact watcher: project {project_id} output folder unavailable: {}",
            project.path
        );
        return Ok(());
    }

    let tool_id = tool_id_from_project_tool(&project.tool);
    let extensions = app_core::tool_launch::read_watch_extensions(pool, &tool_id).await;
    let ext_refs: Vec<&str> = extensions.iter().map(String::as_str).collect();

    // ── On-attach reconciliation pass (T005) ───────────────────────────────
    run_attach_reconciliation(pool, bus, project_id, &project_root, &tool_id, &ext_refs).await?;

    // ── Start the live per-project watcher ─────────────────────────────────
    let root_utf8 = Utf8PathBuf::from_path_buf(project_root.clone())
        .map_err(|_| format!("project path is not valid UTF-8: {}", project.path))?;

    // Constitution §II: never follow symlinks/junctions unless explicitly
    // enabled per root; project output folders have no per-root override
    // surface yet, so they default to the safe gate (matches
    // `start_watcher`'s inbox gating above).
    let (rx, guard) = start_artifact_watcher(std::slice::from_ref(&root_utf8), 256, false)
        .map_err(|e| format!("failed to start artifact watcher: {e}"))?;

    let forward_task = spawn_artifact_forward_task(
        rx,
        Arc::clone(&guard.overflow_flag),
        pool.clone(),
        bus.clone(),
        project_id.to_owned(),
        tool_id,
        extensions,
        project_root,
    );

    // ── Re-acquire lock briefly to insert; handle racer and zombie guard ───
    let mut reg = registry.lock().await;

    if reg.entries.contains_key(project_id) {
        // A concurrent attach completed while we were reconciling. Discard our
        // duplicate: abort the forwarding task and drop the guard so the OS
        // watcher shuts down cleanly.
        forward_task.abort();
        tracing::debug!(
            "artifact watcher: concurrent attach for {project_id}, discarding duplicate"
        );
        return Ok(());
    }

    if reg.detach_requested.remove(project_id) {
        // detach_project_watcher was called while we were reconciling unlocked.
        // Discard the watcher we just built so no zombie entry is left in the
        // registry for a project the user already detached.
        forward_task.abort();
        tracing::debug!(
            "artifact watcher: detach arrived during attach for {project_id}, discarding"
        );
        return Ok(());
    }

    tracing::info!("artifact watcher: attached for project {project_id} ({})", project.path);
    reg.entries.insert(project_id.to_owned(), ArtifactWatcherEntry { _guard: guard, forward_task });
    drop(reg);
    Ok(())
}

/// Re-attempt attaching watchers for projects whose output folder has become
/// available (item f: unavailable-drive auto-reattach). Called by the periodic
/// volume-availability sweep and the manual `artifact.watcher.refresh` command.
///
/// Returns the list of project IDs that were successfully (re-)attached.
#[expect(
    clippy::cognitive_complexity,
    reason = "per-project reattach loop, each failure class logged and skipped independently"
)]
pub async fn reattach_unavailable_projects(
    pool: &SqlitePool,
    bus: &EventBus,
    registry: &ArtifactWatcherRegistry,
) -> Vec<String> {
    let projects = match persistence_plans::repositories::projects::list_projects(pool).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("reattach sweep: failed to list projects: {e}");
            return Vec::new();
        }
    };

    let mut reattached = Vec::new();
    for project in projects {
        // Skip projects already attached.
        {
            let reg = registry.lock().await;
            if reg.entries.contains_key(&project.id) {
                continue;
            }
        }
        // Only attempt attach for paths that now exist (drive just mounted).
        // spawn_blocking: is_dir() can stall the async runtime when the path
        // is on a slow/stale NFS or SMB mount — exactly the environment this
        // sweep targets.
        let path_str = project.path.clone();
        let is_available =
            tokio::task::spawn_blocking(move || std::path::Path::new(&path_str).is_dir())
                .await
                .unwrap_or(false);
        if !is_available {
            continue;
        }
        match attach_project_watcher(pool, bus, registry, &project.id).await {
            Ok(()) => {
                tracing::info!(
                    "reattach sweep: successfully attached watcher for project {} ({})",
                    project.id,
                    project.path
                );
                reattached.push(project.id);
            }
            Err(e) => {
                tracing::debug!("reattach sweep: attach failed for project {}: {e}", project.id);
            }
        }
    }
    reattached
}

/// Start a background task that periodically checks whether previously
/// unavailable project output folders have become reachable (e.g. external
/// drive mounted). Interval is deliberately coarse (30s) — volume mounts are
/// rare events, and the manual refresh command provides immediate feedback.
pub fn spawn_volume_availability_sweep(
    pool: SqlitePool,
    bus: EventBus,
    registry: ArtifactWatcherRegistry,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let reattached = reattach_unavailable_projects(&pool, &bus, &registry).await;
            if !reattached.is_empty() {
                tracing::info!("volume sweep: reattached {} project watcher(s)", reattached.len());
            }
        }
    })
}

/// Detach the live filesystem watcher for `project_id`, if attached.
///
/// Idempotent: detaching an unattached (or already-detached) project is a
/// silent no-op.
///
/// If no live entry is present (attach is in-flight, unlocked), a tombstone
/// is written to `detach_requested` so the finishing attach discards its
/// watcher instead of inserting a zombie entry.
pub async fn detach_project_watcher(registry: &ArtifactWatcherRegistry, project_id: &str) {
    let mut reg = registry.lock().await;
    if let Some(entry) = reg.entries.remove(project_id) {
        entry.forward_task.abort();
        // Dropping `_guard` here (end of scope) stops the OS watcher.
        tracing::info!("artifact watcher: detached for project {project_id}");
    } else {
        // No live entry — record the intent so a racing attach discards its
        // watcher at final-insert time rather than leaving a zombie.
        reg.detach_requested.insert(project_id.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xisf_extensions() -> Vec<String> {
        vec![".xisf".to_owned()]
    }

    fn evt(path: &Path, kind: ArtifactEventKind) -> ArtifactFileEvent {
        ArtifactFileEvent { path: Utf8PathBuf::from_path_buf(path.to_path_buf()).unwrap(), kind }
    }

    #[test]
    fn record_raw_event_ignores_unwatched_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"hello").unwrap();
        let mut pending = HashMap::new();

        record_raw_event(&evt(&path, ArtifactEventKind::Created), &xisf_extensions(), &mut pending);

        assert!(pending.is_empty(), "unwatched extension must not be tracked");
    }

    #[test]
    fn record_raw_event_seeds_pending_snapshot_with_real_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("MasterDark.xisf");
        std::fs::write(&path, b"0123456789").unwrap(); // 10 bytes
        let mut pending = HashMap::new();

        record_raw_event(&evt(&path, ArtifactEventKind::Created), &xisf_extensions(), &mut pending);

        let snapshot = pending.get(&path).expect("watched extension must be tracked");
        assert_eq!(snapshot.size_bytes, 10, "must record the real observed size, not 0");
    }

    #[test]
    fn record_raw_event_removed_cancels_pending_debounce() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("MasterDark.xisf");
        std::fs::write(&path, b"0123456789").unwrap();
        let mut pending = HashMap::new();
        record_raw_event(&evt(&path, ArtifactEventKind::Created), &xisf_extensions(), &mut pending);
        assert!(!pending.is_empty());

        record_raw_event(&evt(&path, ArtifactEventKind::Removed), &xisf_extensions(), &mut pending);

        assert!(pending.is_empty(), "a Removed event must cancel any in-flight debounce");
    }

    #[test]
    fn record_raw_event_ignores_vanished_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("MasterDark.xisf"); // never created
        let mut pending = HashMap::new();

        record_raw_event(&evt(&path, ArtifactEventKind::Created), &xisf_extensions(), &mut pending);

        assert!(pending.is_empty(), "a path that can't be stat'd must not be tracked");
    }

    // ── real_read_dir (#780) ──────────────────────────────────────────────────

    #[test]
    fn real_read_dir_finds_files_nested_under_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        let output_dir = dir.path().join("output");
        std::fs::create_dir_all(&output_dir).unwrap();
        let top_file = dir.path().join("top.fits");
        let nested_file = output_dir.join("nested.xisf");
        std::fs::write(&top_file, b"x").unwrap();
        std::fs::write(&nested_file, b"x").unwrap();

        let found = real_read_dir(dir.path()).unwrap().files;

        assert!(found.contains(&top_file), "top-level file must be found");
        assert!(found.contains(&nested_file), "file nested under a subfolder must be found");
    }

    #[test]
    fn real_read_dir_recurses_multiple_levels() {
        let dir = tempfile::tempdir().unwrap();
        let deep_dir = dir.path().join("output").join("2026-01-01");
        std::fs::create_dir_all(&deep_dir).unwrap();
        let deep_file = deep_dir.join("integration.xisf");
        std::fs::write(&deep_file, b"x").unwrap();

        let found = real_read_dir(dir.path()).unwrap().files;

        assert!(found.contains(&deep_file), "file nested two levels deep must be found");
    }

    #[cfg(unix)]
    #[test]
    fn real_read_dir_never_descends_into_a_symlinked_directory() {
        // Constitution: never follow symlinks/junctions unless explicitly
        // enabled per root — a symlinked subdirectory's contents must not
        // leak into the reconcile pass's file list.
        let dir = tempfile::tempdir().unwrap();
        let real_target = tempfile::tempdir().unwrap();
        let hidden_file = real_target.path().join("outside.fits");
        std::fs::write(&hidden_file, b"x").unwrap();
        let link_path = dir.path().join("linked_output");
        std::os::unix::fs::symlink(real_target.path(), &link_path).unwrap();

        let found = real_read_dir(dir.path()).unwrap().files;

        assert!(found.is_empty(), "must not descend into a symlinked directory: found {found:?}");
    }

    #[cfg(unix)]
    #[test]
    fn real_read_dir_skips_an_unreadable_subdirectory_and_reports_it() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let sibling_dir = dir.path().join("output");
        std::fs::create_dir_all(&sibling_dir).unwrap();
        let sibling_file = sibling_dir.join("integration.xisf");
        std::fs::write(&sibling_file, b"x").unwrap();
        let top_file = dir.path().join("top.fits");
        std::fs::write(&top_file, b"x").unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("hidden.fits"), b"x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        // root (and any CAP_DAC_READ_SEARCH holder) bypasses mode bits, which
        // would make the assertion vacuous.
        if std::fs::read_dir(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            eprintln!(
                "skipping: this environment can read a mode-000 directory (running as root?)"
            );
            return;
        }

        let whole_walk = real_read_dir(dir.path());

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        let listing = whole_walk.expect("an unreadable subdirectory must not fail the whole walk");
        assert!(listing.files.contains(&top_file), "top-level sibling must still be reconciled");
        assert!(listing.files.contains(&sibling_file), "nested sibling must still be reconciled");
        assert_eq!(listing.unreadable, vec![locked], "the unreadable directory must be reported");
    }
}
