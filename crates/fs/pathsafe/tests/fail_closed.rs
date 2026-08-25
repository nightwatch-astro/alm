// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Containment regressions for the link-detection primitive
//! (astro-plan-3v3r.1.26, astro-plan-3v3r.1.14).
//!
//! Kept out of the crate's inline test module so both cases can be run against
//! the pre-fix source with the source files stashed and the tests in place.

#![cfg(unix)]

use std::os::unix::fs::{symlink, PermissionsExt as _};

/// An lstat that fails for a reason other than "absent" leaves link-ness
/// undetermined, and the answer feeds a refusal, so it must refuse.
#[test]
fn an_undeterminable_path_is_treated_as_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    let behind = locked.join("frame.fits");
    std::fs::write(&behind, b"data").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    // root bypasses mode bits, which would make the assertion vacuous.
    let statable = std::fs::symlink_metadata(&behind).is_ok();
    let answer = fs_pathsafe::is_link_or_junction(&behind);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    if statable {
        eprintln!("skipping: this environment can stat behind a mode-000 directory (root?)");
        return;
    }
    assert!(answer, "an lstat failure that is not NotFound must refuse");
}

#[test]
fn a_missing_path_is_not_a_link() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!fs_pathsafe::is_link_or_junction(&dir.path().join("absent")));
}

/// With following enabled the per-entry link skip is off and `is_dir()`
/// resolves through the link, so an ancestor-pointing link re-enters the same
/// subtree. Without a visited set the walk never returns, so the assertion is
/// on a deadline: a non-terminating walk must fail the test rather than hang
/// the suite. The walker thread is abandoned on timeout; the process reaps it.
#[test]
fn a_symlink_cycle_terminates_when_following_is_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("a").join("b").join("c");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("frame.fits"), b"data").unwrap();
    symlink(dir.path(), deep.join("loop")).unwrap();
    symlink(&deep, deep.join("self_loop")).unwrap();

    let root = dir.path().to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(fs_pathsafe::real_files_under(&root, true));
    });

    let files = rx
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("the cycle-following walk must terminate");
    assert_eq!(files, vec![deep.join("frame.fits")]);
}
