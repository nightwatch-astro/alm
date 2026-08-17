// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Proptest invariant suite for `crates/fs/pathsafe`.
//!
//! Invariants tested:
//! 1. `is_link_or_junction` never panics on arbitrary `&Path` inputs
//!    (including paths with null bytes, very long names, unicode, etc.).
//! 2. `real_dirs_under` on a real tempdir always includes the root when root
//!    is not a link — containment invariant (every returned path starts with
//!    the root prefix).
//! 3. `real_files_under` containment: every returned path starts with root.
//! 4. `real_dirs_under(root, false)` never returns a path outside root
//!    (anti-traversal invariant — result set is root-contained).
//! 5. `real_dirs_under` with `follow_symlinks=false` is idempotent: calling it
//!    twice on the same directory returns the same set (order-insensitive).

#![allow(clippy::doc_markdown)]

use std::path::Path;

use fs_pathsafe::{is_link_or_junction, real_dirs_under, real_files_under};
use proptest::prelude::*;

// ── Invariant 1: no panic on arbitrary path strings ────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `is_link_or_junction` must never panic when called with an arbitrary
    /// path string — including paths that do not exist, contain special
    /// characters, are empty, or are extremely long.
    #[test]
    fn is_link_no_panic_arbitrary_path(s in ".*") {
        let path = Path::new(&s);
        // Return value doesn't matter; we only care that there's no panic.
        let _ = is_link_or_junction(path);
    }

    /// Non-existent paths must be reported as "not a link" (stat failure →
    /// false), not panic or return true.
    #[test]
    fn is_link_nonexistent_path_is_false(s in "[A-Za-z0-9_/]{1,60}") {
        // Prepend a UUID-ish prefix so the path is almost certainly missing.
        let path_str = format!("/nonexistent-proptest-root-9f4a/{s}");
        prop_assert!(!is_link_or_junction(Path::new(&path_str)));
    }
}

// ── Helpers for filesystem-backed invariants ──────────────────────────────

/// Build a nested directory tree under `root` given a depth-encoded integer.
///
/// `depth` in 0–3: creates 0–3 levels of nested subdirectories with
/// deterministic names. Keeps the filesystem footprint tiny.
fn build_nested_dirs(root: &Path, depth: u8) {
    if depth == 0 {
        return;
    }
    let sub = root.join("sub_a");
    std::fs::create_dir_all(&sub).unwrap();
    if depth > 1 {
        let sub2 = sub.join("sub_b");
        std::fs::create_dir_all(&sub2).unwrap();
        if depth > 2 {
            std::fs::create_dir_all(sub2.join("sub_c")).unwrap();
        }
    }
}

/// Write `n` empty files in `dir`.
fn write_files(dir: &Path, n: usize) {
    for i in 0..n {
        std::fs::write(dir.join(format!("file_{i}.fits")), b"").unwrap();
    }
}

// ── Invariant 2 & 4: real_dirs_under containment ─────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Every path returned by `real_dirs_under(root, false)` must:
    /// - Start with `root` (contained within root).
    /// - Be a directory that exists on-disk.
    ///
    /// Also validates that `root` itself is always in the result.
    #[test]
    fn real_dirs_under_containment(depth in 0u8..=3) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        build_nested_dirs(&root, depth);

        let dirs = real_dirs_under(&root, false);

        // root must always be present (it's not a link).
        prop_assert!(
            dirs.contains(&root),
            "root must be in real_dirs_under result; root={root:?}, dirs={dirs:?}"
        );

        for d in &dirs {
            // Containment: every returned path must start with root.
            prop_assert!(
                d.starts_with(&root),
                "path {d:?} escaped root {root:?}"
            );
            // Must be an actual directory.
            prop_assert!(d.is_dir(), "returned path {d:?} is not a directory");
        }
    }
}

// ── Invariant 3: real_files_under containment ─────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Every path returned by `real_files_under(root, false)` must start with
    /// `root` and refer to an actual file.
    #[test]
    fn real_files_under_containment(depth in 0u8..=2, n_files in 0usize..=3) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        build_nested_dirs(&root, depth);
        write_files(&root, n_files);

        let files = real_files_under(&root, false);

        for f in &files {
            prop_assert!(
                f.starts_with(&root),
                "file {f:?} escaped root {root:?}"
            );
            prop_assert!(f.is_file(), "returned path {f:?} is not a file");
        }
    }
}

// ── Invariant 5: real_dirs_under is idempotent ────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Calling `real_dirs_under(root, false)` twice on the same unchanged
    /// directory tree must return the same set of paths (order-insensitive).
    ///
    /// This catches any accidental mutation of the tree by the walker itself,
    /// or non-determinism in traversal order that could produce different sets.
    #[test]
    fn real_dirs_under_is_idempotent(depth in 0u8..=3) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        build_nested_dirs(&root, depth);

        let mut first = real_dirs_under(&root, false);
        let mut second = real_dirs_under(&root, false);

        first.sort();
        second.sort();

        prop_assert_eq!(first, second, "real_dirs_under returned different results on second call");
    }
}

// ── Invariant: real_files_under is idempotent ─────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `real_files_under(root, false)` must return the same set on repeated calls.
    #[test]
    fn real_files_under_is_idempotent(depth in 0u8..=2, n_files in 0usize..=3) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        build_nested_dirs(&root, depth);
        write_files(&root, n_files);

        let mut first = real_files_under(&root, false);
        let mut second = real_files_under(&root, false);

        first.sort();
        second.sort();

        prop_assert_eq!(first, second, "real_files_under returned different results on second call");
    }
}

// ── Extra: follow_symlinks=true returns only root ─────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// When `follow_symlinks=true`, `real_dirs_under` returns exactly `[root]`
    /// regardless of actual directory depth — this is the documented contract
    /// for follow=true (the caller drives a recursive watch from the root).
    #[test]
    fn real_dirs_follow_symlinks_returns_only_root(depth in 0u8..=3) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        build_nested_dirs(&root, depth);

        let dirs = real_dirs_under(&root, true);

        prop_assert_eq!(
            dirs,
            vec![root],
            "follow_symlinks=true must return exactly [root]"
        );
    }
}
