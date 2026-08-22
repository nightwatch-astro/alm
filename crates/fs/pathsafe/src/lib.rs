// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Reparse-aware link/junction detection + safe directory/file walking,
//! shared by `fs_inventory` and `fs_executor` (duplication-and-abstraction
//! audit T1-a).
//!
//! Constitution product constraint: "The product MUST avoid following
//! symlinks or junctions during scans unless the user explicitly enables that
//! behavior for a root". This crate is the single place that decides whether
//! a filesystem entry is a link that should be skipped, and provides
//! link-aware directory/file walkers plus a reparse-aware `create_symlink`
//! dispatch.
//!
//! Detection combines `symlink_metadata().file_type().is_symlink()` with an
//! explicit Windows `FILE_ATTRIBUTE_REPARSE_POINT` check. Modern Rust
//! reports NTFS junctions as symlinks via `is_symlink()` because both are
//! surfaced as reparse points, but the explicit attribute check is defence
//! in depth for reparse-point kinds `is_symlink()` might not classify as a
//! symlink — callers should treat any doubt as "skip".

pub mod export_dest;

use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};

use camino::Utf8Path;

/// Returns `true` when `path` is a symlink or (on Windows) a junction /
/// reparse-point entry, as reported by `symlink_metadata`.
///
/// Uses `symlink_metadata` (not `metadata`) so the check inspects the entry
/// itself rather than following it — inspecting a link's target would defeat
/// the purpose of the gate.
///
/// Fails closed. `NotFound` answers `false`, because a path that does not
/// exist is determinately not a link. Every other stat failure (permission
/// denied, stale mount, I/O error) answers `true`: the result feeds a refusal
/// decision, so "cannot determine" must refuse rather than let the caller
/// descend or traverse. Callers that must distinguish the cases should stat
/// the path themselves and call [`is_link_or_junction_metadata`].
#[must_use]
pub fn is_link_or_junction(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => is_link_or_junction_metadata(&meta),
        Err(e) => e.kind() != io::ErrorKind::NotFound,
    }
}

/// Classify already-fetched [`Metadata`] (from `symlink_metadata`, never
/// `metadata`) as a symlink or junction/reparse-point.
///
/// Split out from [`is_link_or_junction`] so callers that already hold a
/// `symlink_metadata()` result (e.g. a per-component path walk that also
/// needs to distinguish "not found" from "other stat error") can classify it
/// without a second stat syscall.
#[must_use]
pub fn is_link_or_junction_metadata(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// Why a root cannot be walked by [`real_files_under`] / [`real_dirs_under`].
///
/// Callers need this because both walkers return an empty list for a root they
/// refuse to descend, which is indistinguishable from a genuinely empty root.
/// Treating the two as the same thing lets an unmounted drive read as "every
/// file was deleted".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootUnavailable {
    /// The path does not exist (unmounted drive, deleted root).
    Missing,
    /// The path exists but is not a directory.
    NotADirectory,
    /// The root itself is a symlink/junction and `follow_symlinks` is off, so
    /// the walkers will not descend it.
    GatedLink,
    /// The path exists as a directory but cannot be stat'd or listed
    /// (permission denied, I/O error, stale network mount).
    Unreadable,
}

impl std::fmt::Display for RootUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Missing => "root path does not exist",
            Self::NotADirectory => "root path is not a directory",
            Self::GatedLink => "root path is a symlink or junction and follow_symlinks is disabled",
            Self::Unreadable => "root path could not be read",
        })
    }
}

/// Check that `root` is a directory the link-aware walkers will actually
/// descend, under the same `follow_symlinks` rule they apply.
///
/// This is the availability precondition for [`real_files_under`] and
/// [`real_dirs_under`]: it must be checked with the same `follow_symlinks`
/// value, because a symlinked root is walkable with `true` and gated with
/// `false`. Note that `Path::is_dir()` alone is not a substitute — it follows
/// links, so it accepts a symlinked root the gated walkers then refuse.
///
/// # Errors
///
/// Returns the [`RootUnavailable`] reason; `Ok(())` means the walkers will
/// traverse this root.
pub fn probe_root(root: &Path, follow_symlinks: bool) -> Result<(), RootUnavailable> {
    let meta = fs::symlink_metadata(root).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => RootUnavailable::Missing,
        _ => RootUnavailable::Unreadable,
    })?;

    if is_link_or_junction_metadata(&meta) {
        if !follow_symlinks {
            return Err(RootUnavailable::GatedLink);
        }
        // Following is enabled: classify the link's target instead.
        let target = fs::metadata(root).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => RootUnavailable::Missing,
            _ => RootUnavailable::Unreadable,
        })?;
        if !target.is_dir() {
            return Err(RootUnavailable::NotADirectory);
        }
    } else if !meta.is_dir() {
        return Err(RootUnavailable::NotADirectory);
    }

    fs::read_dir(root).map(|_| ()).map_err(|_| RootUnavailable::Unreadable)
}

/// Recursively collect every real (non-link) directory under `root`,
/// including `root` itself when it is not a link.
///
/// When `follow_symlinks` is `true`, this simply returns `[root]` — the
/// caller is expected to use a recursive watch/walk from there instead
/// (matching the pre-gate behaviour). When `false` (the default), the
/// returned list never includes a symlinked/junction directory or descends
/// through one, satisfying the constitution's "MUST NOT follow symlinks or
/// junctions ... unless explicitly enabled" requirement.
///
/// Unreadable directories are skipped rather than failing the whole walk —
/// a single permission-denied subtree should not abort discovery of the rest
/// of the root.
#[must_use]
pub fn real_dirs_under(root: &Path, follow_symlinks: bool) -> Vec<PathBuf> {
    if follow_symlinks {
        return vec![root.to_path_buf()];
    }
    if is_link_or_junction(root) {
        return Vec::new();
    }

    let mut dirs = vec![root.to_path_buf()];
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_link_or_junction(&path) {
                continue;
            }
            if path.is_dir() {
                dirs.push(path.clone());
                stack.push(path);
            }
        }
    }

    dirs
}

/// Recursively collect every real (non-link) **file** under `root`.
///
/// Mirrors [`real_dirs_under`]'s gating: a symlinked/junction directory is
/// never descended into, and a symlinked file is never yielded, unless
/// `follow_symlinks` is `true`. Used by the raw-frame reconcile walker to
/// enumerate on-disk frames without ever following a link (spec 048 R6).
#[must_use]
pub fn real_files_under(root: &Path, follow_symlinks: bool) -> Vec<PathBuf> {
    real_files_under_reporting(root, follow_symlinks).0
}

/// [`real_files_under`], additionally returning the directories whose
/// `read_dir` failed.
///
/// Both walkers skip an unreadable directory rather than aborting, which makes
/// "no files here" ambiguous between empty and unreadable. Callers that must
/// not read a permission-denied subtree as deletion need the second list;
/// treat it as unknown state, never as absence.
///
/// When `follow_symlinks` is on, directories are deduplicated by their
/// canonical path so a link pointing at an ancestor terminates instead of
/// re-entering the subtree forever.
#[must_use]
pub fn real_files_under_reporting(
    root: &Path,
    follow_symlinks: bool,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut unreadable = Vec::new();

    if !follow_symlinks && is_link_or_junction(root) {
        return (files, unreadable);
    }

    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if follow_symlinks
            && !visited.insert(fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone()))
        {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            unreadable.push(dir);
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !follow_symlinks && is_link_or_junction(&path) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }

    (files, unreadable)
}

/// Create a symlink from `source` to `destination`, pointing at a **file**
/// (`std::os::windows::fs::symlink_file` on Windows — this crate does not
/// yet materialize directory junctions; see `Materialization::Junction`
/// callers).
///
/// # Errors
///
/// Returns the underlying `io::Error` from the platform symlink call, or an
/// "unsupported platform" error on targets that are neither unix nor
/// windows.
pub fn create_symlink(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, destination)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(source, destination)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (source, destination);
        Err(io::Error::other("symlink not supported on this platform"))
    }
}

// ── Wire-path helpers ─────────────────────────────────────────────────────────

/// Convert `path` to a forward-slash UTF-8 string for wire and contract fields.
///
/// Uses `camino::Utf8Path` for a lossless conversion when the path is valid
/// UTF-8, falling back to `to_string_lossy` (which replaces invalid sequences
/// with U+FFFD) for the rare non-UTF-8 path. The fallback is defensive only;
/// in production all paths pass the non-UTF-8 skip at the `read_dir` boundary
/// before reaching this function.
///
/// This is the canonical single home for `replace('\\', "/")` on a `Path`
/// so the wire format (`relative_file_path` contract field) cannot drift
/// between scan, classify, and confirm (bd `astro-plan-kyo7.88`).
#[must_use]
pub fn wire_path(path: &Path) -> String {
    Utf8Path::from_path(path).map_or_else(
        || path.to_string_lossy().replace('\\', "/"),
        |u| u.as_str().replace('\\', "/"),
    )
}

/// Compute the forward-slash UTF-8 path of `path` relative to `root`.
///
/// Strips the `root` prefix via `strip_prefix`; falls back to `wire_path(path)`
/// if `path` does not start with `root`. Callers must ensure `path` is a
/// descendant of `root` — the fallback is purely defensive.
#[must_use]
pub fn relative_wire_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    wire_path(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_directory_is_not_a_link() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_link_or_junction(dir.path()));
    }

    #[test]
    fn real_dirs_under_includes_nested_real_directories() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();

        let dirs = real_dirs_under(dir.path(), false);
        assert!(dirs.contains(&dir.path().to_path_buf()));
        assert!(dirs.contains(&dir.path().join("a")));
        assert!(dirs.contains(&nested));
    }

    #[test]
    fn probe_root_classifies_unwalkable_roots() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(probe_root(dir.path(), false), Ok(()));

        let file = dir.path().join("frame.fits");
        std::fs::write(&file, b"data").unwrap();
        assert_eq!(probe_root(&file, false), Err(RootUnavailable::NotADirectory));

        assert_eq!(probe_root(&dir.path().join("absent"), false), Err(RootUnavailable::Missing));
    }

    #[cfg(unix)]
    #[test]
    fn probe_root_gates_a_symlinked_root_unless_following_is_enabled() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = dir.path().join("link");
        symlink(&real, &link).unwrap();

        assert_eq!(probe_root(&link, false), Err(RootUnavailable::GatedLink));
        assert_eq!(probe_root(&link, true), Ok(()));

        // A link whose target is gone is Missing, not GatedLink, when following.
        std::fs::remove_dir_all(&real).unwrap();
        assert_eq!(probe_root(&link, true), Err(RootUnavailable::Missing));
    }

    #[test]
    fn a_missing_path_is_not_a_link() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_link_or_junction(&dir.path().join("absent")));
    }

    #[cfg(unix)]
    #[test]
    fn real_files_under_reporting_names_the_unreadable_directory() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("hidden.fits"), b"data").unwrap();
        std::fs::write(dir.path().join("top.fits"), b"data").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        let readable = std::fs::read_dir(&locked).is_ok();
        let (files, unreadable) = real_files_under_reporting(dir.path(), false);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        if readable {
            eprintln!("skipping: this environment can read a mode-000 directory (root?)");
            return;
        }
        assert_eq!(files, vec![dir.path().join("top.fits")]);
        assert_eq!(unreadable, vec![locked]);
    }

    #[test]
    fn real_files_under_finds_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("frame.fits"), b"data").unwrap();
        std::fs::write(dir.path().join("top.fits"), b"data").unwrap();

        let mut files = real_files_under(dir.path(), false);
        files.sort();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&nested.join("frame.fits")));
        assert!(files.contains(&dir.path().join("top.fits")));
    }

    #[test]
    fn create_symlink_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.fits");
        let link = dir.path().join("link.fits");
        std::fs::write(&target, b"data").unwrap();

        create_symlink(&target, &link).unwrap();

        assert!(is_link_or_junction(&link));
        assert_eq!(std::fs::read(&link).unwrap(), b"data");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directory_is_detected_and_skipped() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let real_target = dir.path().join("real_target");
        std::fs::create_dir_all(&real_target).unwrap();
        std::fs::write(real_target.join("hidden.fits"), b"data").unwrap();

        let scan_root = dir.path().join("scan_root");
        std::fs::create_dir_all(&scan_root).unwrap();
        let link_path = scan_root.join("link_to_target");
        symlink(&real_target, &link_path).unwrap();

        assert!(is_link_or_junction(&link_path));

        // The linked subdirectory must not be descended into.
        let dirs = real_dirs_under(&scan_root, false);
        assert!(!dirs.contains(&link_path));

        let files = real_files_under(&scan_root, false);
        assert!(files.is_empty(), "must not see files behind an un-enabled symlink");
    }

    #[cfg(unix)]
    #[test]
    fn follow_symlinks_true_traverses_link() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let real_target = dir.path().join("real_target");
        std::fs::create_dir_all(&real_target).unwrap();
        std::fs::write(real_target.join("visible.fits"), b"data").unwrap();

        let scan_root = dir.path().join("scan_root");
        std::fs::create_dir_all(&scan_root).unwrap();
        symlink(&real_target, scan_root.join("link_to_target")).unwrap();

        // With follow_symlinks true, real_dirs_under intentionally just
        // returns the root (caller does a recursive OS-level watch/walk that
        // will itself follow links) — verify that contract explicitly.
        let dirs = real_dirs_under(&scan_root, true);
        assert_eq!(dirs, vec![scan_root]);
    }

    /// Windows-only: a directory junction (`mklink /J`) is a reparse point
    /// that is NOT reported by `is_symlink()` on some toolchains — this is
    /// exactly the gap the explicit `FILE_ATTRIBUTE_REPARSE_POINT` check in
    /// [`is_link_or_junction_metadata`] closes. Creates the junction via the
    /// `mklink` shell builtin (no admin privilege required for junctions,
    /// unlike symlinks) rather than adding a dependency for one test.
    #[cfg(windows)]
    #[test]
    fn junction_directory_is_detected_and_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let real_target = dir.path().join("real_target");
        std::fs::create_dir_all(&real_target).unwrap();
        std::fs::write(real_target.join("hidden.fits"), b"data").unwrap();

        let scan_root = dir.path().join("scan_root");
        std::fs::create_dir_all(&scan_root).unwrap();
        let junction_path = scan_root.join("junction_to_target");

        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                junction_path.to_str().unwrap(),
                real_target.to_str().unwrap(),
            ])
            .status()
            .expect("mklink invocation failed");
        assert!(status.success(), "mklink /J failed to create the test junction");

        assert!(is_link_or_junction(&junction_path));

        let dirs = real_dirs_under(&scan_root, false);
        assert!(!dirs.contains(&junction_path));

        let files = real_files_under(&scan_root, false);
        assert!(files.is_empty(), "must not see files behind an un-enabled junction");
    }
}
