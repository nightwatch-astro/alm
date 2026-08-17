// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Artifact filesystem watcher service (spec 012, FR-009, spec 033 T028).
//!
//! Watches registered library-root paths for new or modified files.  Uses
//! `notify 8` and forwards every raw Create/Modify/Remove event downstream,
//! undebounced, via a `tokio::sync::mpsc` channel.
//!
//! ## Debounce strategy (deviation from D10)
//!
//! D10 specified `notify-debouncer-full 0.7.x`.  `notify-debouncer-full` is
//! now a direct dep alongside `notify 8`, satisfying that design intent.
//! However, wiring the debouncer into the event pipeline touches the same
//! watcher files being refactored on branch `fix/watcher-robustness-cluster`
//! (bead kyo7.74); to avoid a merge conflict, debouncer adoption is deferred
//! to that branch.  This crate therefore stays a thin raw-event source for
//! now.  The stable-size debounce (default 2s, spec 012 edge case) is pure
//! logic in `workflow_artifacts::watcher::check_stability`/`FileSnapshot`,
//! applied by the consumer (`apps/desktop/src-tauri/src/watcher.rs`'s
//! per-project forward task) against a `HashMap<PathBuf, FileSnapshot>` it
//! owns.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use camino::Utf8PathBuf;
use notify::{Event, RecommendedWatcher};
use tokio::sync::mpsc;

use crate::notify_bridge::SimpleEventKind;

/// A filesystem event forwarded to the async consumer.
#[derive(Clone, Debug)]
pub struct ArtifactFileEvent {
    /// Absolute path of the file that changed (guaranteed UTF-8).
    pub path: Utf8PathBuf,
    /// The underlying event kind (Create / Modify / Remove / error signals).
    pub kind: ArtifactEventKind,
}

/// Simplified event kind for artifact detection purposes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactEventKind {
    Created,
    Modified,
    Removed,
    /// The OS watcher encountered an error — consumer should trigger a reconcile
    /// pass to recover any missed events (item a: error-callback deafness fix).
    NeedsRescan,
}

impl From<SimpleEventKind> for ArtifactEventKind {
    fn from(kind: SimpleEventKind) -> Self {
        match kind {
            SimpleEventKind::Created => Self::Created,
            SimpleEventKind::Modified => Self::Modified,
            SimpleEventKind::Removed => Self::Removed,
        }
    }
}

/// RAII guard that keeps the watcher alive.
///
/// Drop this to stop watching and close the channel.
///
/// `overflow_flag` is set to `true` by the notify callback whenever the mpsc
/// channel is full and at least one event was dropped. The forward task polls
/// this flag instead of relying on an in-band sentinel event — an
/// `AtomicBool::store` cannot itself be lost the way a `try_send` can.
pub struct WatcherGuard {
    _watcher: RecommendedWatcher,
    _tx: Arc<mpsc::Sender<ArtifactFileEvent>>,
    /// Set to `true` when the channel overflowed; cleared by the consumer
    /// after it triggers reconciliation.
    pub overflow_flag: Arc<AtomicBool>,
}

/// Start the filesystem watcher over `paths`.
///
/// `follow_symlinks` gates traversal (constitution "MUST NOT follow symlinks
/// or junctions unless explicitly enabled per root", duplication-and-
/// abstraction audit T1-a): when `false`, each path is walked ourselves via
/// [`fs_pathsafe::real_dirs_under`] and every real (non-link) subdirectory is
/// watched individually with `RecursiveMode::NonRecursive`, mirroring
/// [`crate::watcher::WatcherService::start`]. When `true`, a single
/// `RecursiveMode::Recursive` watch per path is used (the OS watcher may then
/// follow links under it).
///
/// Returns an `mpsc::Receiver` and a `WatcherGuard`.  Drop the guard to stop.
///
/// # Errors
///
/// Returns an error string if the platform watcher cannot be initialised or a
/// path cannot be watched.
pub fn start_artifact_watcher(
    paths: &[Utf8PathBuf],
    channel_capacity: usize,
    follow_symlinks: bool,
) -> Result<(mpsc::Receiver<ArtifactFileEvent>, WatcherGuard), String> {
    let (tx, rx) = mpsc::channel::<ArtifactFileEvent>(channel_capacity);
    let tx = Arc::new(tx);
    let handler_tx = Arc::clone(&tx);
    // Item (c): atomic flag set on overflow — cannot itself be dropped the
    // way an in-band try_send sentinel can. The forward task polls and clears
    // it on each loop iteration.
    let overflow_flag = Arc::new(AtomicBool::new(false));
    let callback_overflow = Arc::clone(&overflow_flag);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        match res {
            Err(e) => {
                // Item (a): forward OS watcher errors as NeedsRescan so the
                // consumer triggers a reconciliation pass.
                tracing::error!(error = %e, "artifact watcher error");
                let _ = handler_tx.try_send(ArtifactFileEvent {
                    path: Utf8PathBuf::new(),
                    kind: ArtifactEventKind::NeedsRescan,
                });
            }
            Ok(event) => {
                let Some(simple_kind) = crate::notify_bridge::classify(event.kind) else {
                    return;
                };
                let kind = ArtifactEventKind::from(simple_kind);

                for path in &event.paths {
                    let Some(utf8) = crate::notify_bridge::utf8_path_or_skip(
                        path,
                        "fs_inventory::artifact_watcher",
                    ) else {
                        continue;
                    };
                    if utf8.is_dir() {
                        continue;
                    }
                    if let Err(mpsc::error::TrySendError::Full(_)) =
                        handler_tx.try_send(ArtifactFileEvent { path: utf8, kind })
                    {
                        // Store beats try_send: an AtomicBool::store cannot be
                        // dropped if the channel is full.
                        tracing::warn!("artifact watcher: channel full, events dropped");
                        callback_overflow.store(true, Ordering::Release);
                        return;
                    }
                }
            }
        }
    })
    .map_err(|e| format!("failed to create artifact watcher: {e}"))?;

    // Item (d): partial start tolerance — per-subdirectory failures are
    // collected rather than aborting the entire watcher start.
    let report =
        crate::notify_bridge::register_watch_paths(&mut watcher, paths, follow_symlinks, false)?;
    if !report.skipped.is_empty() {
        tracing::warn!(
            "artifact watcher: {} subdirectories could not be watched (partial start)",
            report.skipped.len()
        );
    }

    let guard = WatcherGuard { _watcher: watcher, _tx: tx, overflow_flag };
    Ok((rx, guard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_on_nonexistent_path_returns_error() {
        let result = start_artifact_watcher(
            &[Utf8PathBuf::from("/nonexistent/path/for/watcher/test")],
            16,
            false,
        );
        assert!(result.is_err(), "expected error for nonexistent path");
    }

    #[tokio::test]
    async fn start_on_temp_dir_succeeds_and_guard_drops_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let (mut rx, guard) = start_artifact_watcher(&[root], 16, false).unwrap();

        drop(guard);

        // After dropping the guard, the Sender Arc is released.
        // Channel becomes closed; recv returns None.
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn file_create_fires_event() {
        let dir = tempfile::tempdir().unwrap();
        // Canonicalize so emitted event paths match `file_path` (macOS reports
        // /private/var/... while tempdir() returns /var/...).
        let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
        let (mut rx, _guard) =
            start_artifact_watcher(std::slice::from_ref(&root), 64, false).unwrap();

        let file_path = root.join("test_artifact.xisf");
        std::fs::write(&file_path, b"data").unwrap();

        // Wait for event with a 2-second deadline.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        let received = loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(evt)) if evt.path == file_path => break true,
                Ok(Some(_)) => {}
                _ => break false,
            }
        };
        assert!(received, "expected ArtifactFileEvent for the created file");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_subtree_is_not_traversed_unless_enabled() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let canonical_root = dir.path().canonicalize().unwrap();
        let real_target = canonical_root.join("real_target");
        std::fs::create_dir_all(&real_target).unwrap();

        let scan_root = canonical_root.join("scan_root");
        std::fs::create_dir_all(&scan_root).unwrap();
        symlink(&real_target, scan_root.join("linked")).unwrap();
        let scan_root_utf8 = Utf8PathBuf::from_path_buf(scan_root).unwrap();

        let (mut rx, _guard) =
            start_artifact_watcher(std::slice::from_ref(&scan_root_utf8), 64, false).unwrap();

        // A file written behind the un-enabled symlink must never surface an
        // event — the watcher was never attached to that subtree.
        std::fs::write(real_target.join("hidden.fits"), b"data").unwrap();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
        let received = tokio::time::timeout_at(deadline, rx.recv()).await;
        assert!(
            received.is_err(),
            "must not observe an event for a file behind an un-enabled symlink"
        );
    }
}
