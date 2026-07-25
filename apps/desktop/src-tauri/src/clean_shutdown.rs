// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Clean-shutdown marker for unclean-shutdown detection (astro-plan-kyo7.48).
//!
//! A single marker file under the app-data root records that the previous
//! process exited gracefully. It is written on `RunEvent::Exit` (the final,
//! non-cancellable event-loop hook) and consumed at boot: if the marker is
//! present it is cleared and the shutdown was clean; if it is absent the
//! process was killed or crashed (SIGKILL/power loss never runs `Exit`), which
//! is exactly the signal the recovery prompt needs.
//!
//! This is detection state, not durability state — it carries no filesystem
//! intent and adds no write-ahead journal. The authoritative crash evidence is
//! the plan rows reconciled at boot (astro-plan-kyo7.49); the marker only
//! decides whether to surface the prompt at all.

use std::path::{Path, PathBuf};

/// Managed boot-time verdict: `true` when the previous shutdown was unclean.
/// Read by the `recovery_status` command.
pub struct UncleanShutdown(pub bool);

const MARKER_FILE: &str = ".clean-shutdown";

fn marker_path(data_dir: &Path) -> PathBuf {
    data_dir.join(MARKER_FILE)
}

/// Consume the marker at boot: return `true` when the previous shutdown was
/// clean (marker present, now removed), `false` when it was unclean (marker
/// absent). Clearing on read means the next boot defaults to "unclean" unless
/// a graceful exit re-writes it.
pub fn take_was_clean(data_dir: &Path) -> bool {
    let path = marker_path(data_dir);
    match std::fs::remove_file(&path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        // A marker we cannot remove would make every subsequent boot look
        // clean; treat an un-removable marker as unclean so the prompt is not
        // permanently suppressed.
        Err(_) => false,
    }
}

/// Write the marker on graceful exit. Best-effort: a failure only means the
/// next boot treats this shutdown as unclean and may show a benign prompt.
pub fn write(data_dir: &Path) {
    let _ = std::fs::write(marker_path(data_dir), b"");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_marker_reads_unclean() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!take_was_clean(dir.path()), "no marker => unclean");
    }

    #[test]
    fn written_marker_reads_clean_once_then_unclean() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path());
        assert!(take_was_clean(dir.path()), "marker present => clean");
        assert!(!take_was_clean(dir.path()), "marker cleared on read => unclean next time");
    }
}
