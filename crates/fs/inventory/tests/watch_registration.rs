// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Watch registration must not report success for a root it never watched
//! (review round 1 on astro-plan-551nn, FIX 1).
//!
//! Kept out of the module's inline test module so both cases can be run against
//! the pre-fix source with `src/notify_bridge.rs` stashed and the tests in
//! place.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;

use camino::Utf8PathBuf;
use fs_inventory::notify_bridge::register_watch_paths;

fn watcher() -> notify::RecommendedWatcher {
    notify::recommended_watcher(|_: Result<notify::Event, notify::Error>| {}).unwrap()
}

/// A root the walker refuses yields no directories, so without a probe the
/// registration loop never runs, the fatal root branch is skipped, and the
/// caller registers nothing while being told it succeeded.
#[test]
fn register_watch_paths_errors_on_an_unstatable_root() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir_all(locked.join("root")).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let path = Utf8PathBuf::from_path_buf(locked.join("root")).unwrap();

    // root bypasses mode bits, which would make the assertion vacuous.
    let statable = std::fs::symlink_metadata(path.as_std_path()).is_ok();
    let result = register_watch_paths(&mut watcher(), &[path], false, false);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    if statable {
        eprintln!("skipping: this environment can stat behind a mode-000 directory (root?)");
        return;
    }
    let err = result.expect_err("an unwatchable root must not report success");
    assert!(err.contains("could not be read"), "unexpected message: {err}");
}

/// Observing nothing under a symlinked root while `follow_symlinks` is off is
/// the gate working, so registration succeeds. The root still has to appear in
/// `skipped`, otherwise zero watched paths is indistinguishable from a root
/// that was watched and is empty.
#[test]
fn register_watch_paths_reports_a_gated_symlinked_root_as_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let path = Utf8PathBuf::from_path_buf(link.clone()).unwrap();

    let report = register_watch_paths(&mut watcher(), &[path], false, false)
        .expect("a gated symlinked root is not a registration failure");
    assert!(report.watched.is_empty());
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].0, link);
}
