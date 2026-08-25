// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Filesystem watcher abstraction (spec 012 T003/T004).
//!
//! The real OS-level watcher is injected via the `FsWatcher` trait so that
//! debounce + stable-size logic can be unit-tested without real filesystem
//! events or `sleep` calls (spec constraint: no real fs-watch in unit tests).
//!
//! The watcher trait contract:
//! - `WatchEvent` is the raw notification from the OS watcher.
//! - `FsWatcher` is implemented by `notify`-based watchers in production and
//!   by `FakeWatcher` in tests.
//! - `AppClock` is injected for the stable-size debounce timestamp; tests
//!   inject `FakeClock` to advance time programmatically.
//!
//! Constitution III: the watcher NEVER opens observed files. It only inspects
//! file metadata (size, mtime) for the stable-size check.

use std::path::{Path, PathBuf};
use std::time::Duration;

// ── Event types ───────────────────────────────────────────────────────────────

/// Coarse event kind emitted by the underlying OS watcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchEventKind {
    /// A new file appeared or an existing file changed.
    CreateOrModify,
    /// A file was deleted.
    Remove,
}

/// A raw filesystem watch notification before debounce/stable-size filtering.
#[derive(Clone, Debug)]
pub struct WatchEvent {
    pub path: PathBuf,
    pub kind: WatchEventKind,
    /// Monotonic clock instant at which the event arrived (R-AppClock: NOT mtime).
    pub arrived_at: std::time::Instant,
}

// ── Stable-size tracking ──────────────────────────────────────────────────────

/// Metadata snapshot used by the stable-size check.
#[derive(Clone, Debug)]
pub struct FileSnapshot {
    pub size_bytes: u64,
    pub arrived_at: std::time::Instant,
}

/// Outcome of a stable-size check for a single path.
#[derive(Debug, Eq, PartialEq)]
pub enum StabilityStatus {
    /// File size has been stable across the debounce window; ready to classify.
    Stable { size_bytes: u64 },
    /// File is still being written; retry after the debounce window.
    Writing,
    /// File disappeared before becoming stable.
    Gone,
}

/// Check whether the file at `path` appears stable (same size as `prior`
/// and `elapsed >= debounce`).
///
/// This function uses the injected `size_probe` closure so that tests can
/// return synthetic sizes without hitting the real filesystem.
pub fn check_stability<F>(
    path: &std::path::Path,
    prior: &FileSnapshot,
    now: std::time::Instant,
    debounce: Duration,
    size_probe: F,
) -> StabilityStatus
where
    F: Fn(&std::path::Path) -> Option<u64>,
{
    if now.duration_since(prior.arrived_at) < debounce {
        return StabilityStatus::Writing;
    }
    match size_probe(path) {
        None => StabilityStatus::Gone,
        Some(current_size) if current_size == prior.size_bytes => {
            StabilityStatus::Stable { size_bytes: current_size }
        }
        Some(_) => StabilityStatus::Writing,
    }
}

// ── Extension pre-filter ──────────────────────────────────────────────────────

/// Default watch extensions per R-ExtAllow.
pub const DEFAULT_WATCH_EXTENSIONS: &[&str] =
    &[".xisf", ".fits", ".fit", ".tif", ".tiff", ".png", ".jpg", ".jpeg", ".ser", ".avi"];

/// Default stable-size debounce window (spec 012 edge case: "bounded debounce
/// window (default 2s)"). Consumers call [`check_stability`] on a timer no
/// coarser than this to detect the "no further events" quiet period.
pub const DEFAULT_STABILITY_DEBOUNCE: Duration = Duration::from_secs(2);

/// Returns `true` when the file name's extension is in the watch-extensions
/// list (case-insensitive comparison).
///
/// The comparison is against the real extension, not the tail of the whole
/// name, so the extensionless dotfile `.xisf` is rejected. Allowlist entries
/// may be spelled with or without a leading dot: the defaults are dotted and a
/// configured list may not be.
#[must_use]
pub fn extension_allowed(file_name: &str, extensions: &[&str]) -> bool {
    let Some(actual) = Path::new(file_name).extension() else { return false };
    let actual = actual.to_string_lossy().to_ascii_lowercase();
    extensions.iter().any(|ext| ext.trim_start_matches('.').eq_ignore_ascii_case(actual.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // ── extension_allowed ─────────────────────────────────────────────────────

    #[test]
    fn xisf_allowed_case_insensitive() {
        assert!(extension_allowed("MasterDark.XISF", DEFAULT_WATCH_EXTENSIONS));
        assert!(extension_allowed("image.xisf", DEFAULT_WATCH_EXTENSIONS));
    }

    #[test]
    fn unknown_extension_rejected() {
        assert!(!extension_allowed("calibration.py", DEFAULT_WATCH_EXTENSIONS));
        assert!(!extension_allowed("notes.txt", DEFAULT_WATCH_EXTENSIONS));
    }

    #[test]
    fn fits_and_fit_allowed() {
        assert!(extension_allowed("file.fits", DEFAULT_WATCH_EXTENSIONS));
        assert!(extension_allowed("file.fit", DEFAULT_WATCH_EXTENSIONS));
        assert!(extension_allowed("file.FIT", DEFAULT_WATCH_EXTENSIONS));
    }

    /// An extensionless dotfile whose name equals a watched extension is not
    /// an artifact.
    #[test]
    fn an_extensionless_dotfile_is_rejected() {
        assert!(!extension_allowed(".xisf", DEFAULT_WATCH_EXTENSIONS));
        assert!(!extension_allowed(".fits", DEFAULT_WATCH_EXTENSIONS));
        assert!(extension_allowed("a.XISF", DEFAULT_WATCH_EXTENSIONS));
    }

    /// A name that merely ends with a watched extension without a separating
    /// dot is not that extension.
    #[test]
    fn a_suffix_without_a_dot_is_not_an_extension() {
        assert!(!extension_allowed("notxisf", &["xisf", "fits"]));
        assert!(!extension_allowed("myfits", &["xisf", "fits"]));
        assert!(!extension_allowed("notxisf", DEFAULT_WATCH_EXTENSIONS));
    }

    /// A configured allowlist may be spelled without leading dots; the
    /// dotted defaults must keep working alongside it.
    #[test]
    fn a_dotless_configured_allowlist_still_matches() {
        assert!(extension_allowed("a.xisf", &["xisf"]));
        assert!(extension_allowed("a.XISF", &["xisf"]));
        assert!(!extension_allowed(".xisf", &["xisf"]));
        assert!(extension_allowed("a.xisf", DEFAULT_WATCH_EXTENSIONS));
    }

    // ── check_stability ───────────────────────────────────────────────────────

    #[test]
    fn stable_when_size_unchanged_and_window_elapsed() {
        let t0 = Instant::now();
        let prior = FileSnapshot { size_bytes: 1024, arrived_at: t0 };
        let now = t0 + Duration::from_secs(3);
        let debounce = Duration::from_secs(2);

        let result =
            check_stability(std::path::Path::new("/fake/file.xisf"), &prior, now, debounce, |_| {
                Some(1024)
            });
        assert_eq!(result, StabilityStatus::Stable { size_bytes: 1024 });
    }

    #[test]
    fn writing_when_window_not_elapsed() {
        let t0 = Instant::now();
        let prior = FileSnapshot { size_bytes: 1024, arrived_at: t0 };
        let now = t0 + Duration::from_millis(500);
        let debounce = Duration::from_secs(2);

        let result =
            check_stability(std::path::Path::new("/fake/file.xisf"), &prior, now, debounce, |_| {
                Some(1024)
            });
        assert_eq!(result, StabilityStatus::Writing);
    }

    #[test]
    fn writing_when_size_changed() {
        let t0 = Instant::now();
        let prior = FileSnapshot { size_bytes: 1024, arrived_at: t0 };
        let now = t0 + Duration::from_secs(5);
        let debounce = Duration::from_secs(2);

        // Size grew — file still being written.
        let result =
            check_stability(std::path::Path::new("/fake/file.xisf"), &prior, now, debounce, |_| {
                Some(2048)
            });
        assert_eq!(result, StabilityStatus::Writing);
    }

    #[test]
    fn gone_when_file_disappeared() {
        let t0 = Instant::now();
        let prior = FileSnapshot { size_bytes: 1024, arrived_at: t0 };
        let now = t0 + Duration::from_secs(5);
        let debounce = Duration::from_secs(2);

        let result =
            check_stability(std::path::Path::new("/fake/file.xisf"), &prior, now, debounce, |_| {
                None
            });
        assert_eq!(result, StabilityStatus::Gone);
    }
}
