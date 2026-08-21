// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Reconciler: on-attach rescan + missing-state transitions (spec 012 T005).
//!
//! When a project drawer opens (or the watcher re-attaches after a drive
//! disconnect), the reconciler scans the output folder and compares existing
//! `ProcessingArtifact` rows against the current filesystem state:
//!
//! - Files present on disk that are **not** in the DB → passed to the caller
//!   as new detections (caller inserts them).
//! - Files in the DB that are **absent** from disk → returned as `Gone` entries
//!   (caller transitions them to `missing` state).
//! - Files in the DB that are **present** → returned as `Seen` (caller can
//!   refresh `last_seen_at`).
//! - Files in the DB that live under a directory the walk could not read →
//!   returned as `Unknown` (caller transitions nothing).
//!
//! Absent and unreadable are distinct: an unreadable subtree is unknown state,
//! not absent state, so its rows must never be reported as missing.
//!
//! The reconciler never modifies the database itself: all state changes are
//! passed back to the caller so the persistence + audit path is centralised
//! in `app_core::artifact`.
//!
//! Constitution III: reconciler is read-only (stat + readdir only). It NEVER
//! opens, writes, or renames any observed file.

use std::path::{Path, PathBuf};

/// The relative project path stored in the DB row.
pub type RelativePath = String;

/// Outcome for a single DB row after a reconciliation scan.
#[derive(Debug, Eq, PartialEq)]
pub enum ReconcileOutcome {
    /// File is still on disk; refresh `last_seen_at`.
    Seen,
    /// File is no longer on disk; transition to `missing`.
    Gone,
    /// The file's directory could not be read, so presence is undetermined;
    /// change nothing.
    Unknown,
}

/// One directory walk's result: the real files found, plus the directories whose
/// contents could not be read.
///
/// An unreadable directory is skipped rather than aborting the walk, so its
/// paths are reported separately — `reconcile` needs them to tell an absent file
/// apart from an unobservable one.
#[derive(Debug, Default)]
pub struct DirListing {
    pub files: Vec<PathBuf>,
    pub unreadable: Vec<PathBuf>,
}

/// A new file detected by the reconciliation scan (not yet in the DB).
#[derive(Debug)]
pub struct NewDetection {
    pub absolute_path: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
    /// Filesystem mtime at scan time (stored but NOT used for attribution).
    pub file_mtime: std::time::SystemTime,
}

/// Result of a reconciliation pass.
#[derive(Debug, Default)]
pub struct ReconcileReport {
    /// Outcomes for existing DB rows: `(relative_path, outcome)`.
    pub existing: Vec<(RelativePath, ReconcileOutcome)>,
    /// Files found on disk that are not yet in the DB.
    pub new_files: Vec<NewDetection>,
    /// Directories the walk could not read. The caller surfaces these; every row
    /// beneath one is reported `Unknown` rather than `Gone`.
    pub unreadable_dirs: Vec<PathBuf>,
}

/// Reconcile the output folder at `output_dir` against the set of known paths
/// already in the DB (`known_paths`) and filtered by `allowed_extensions`.
///
/// `read_dir_fn` and `metadata_fn` are injected so the caller controls
/// filesystem access (enables unit testing without real I/O).
///
/// # Errors
/// Returns `Err(String)` if the output directory cannot be read.
pub fn reconcile<ReadDir, Meta>(
    output_dir: &Path,
    known_paths: &[RelativePath],
    allowed_extensions: &[&str],
    read_dir_fn: &ReadDir,
    metadata_fn: &Meta,
) -> Result<ReconcileReport, String>
where
    ReadDir: Fn(&Path) -> Result<DirListing, String>,
    Meta: Fn(&Path) -> Option<(u64, std::time::SystemTime)>,
{
    let DirListing { files: on_disk, unreadable } = read_dir_fn(output_dir)?;

    // Build a set of file names (lowercased) for fast lookup.
    let on_disk_lower: std::collections::HashSet<String> = on_disk
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()))
        .collect();

    // Classify existing DB rows.
    let existing = known_paths
        .iter()
        .map(|rel| {
            // `rel` is a project-relative path; take the file name component.
            let file_name = Path::new(rel)
                .file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let outcome = if on_disk_lower.contains(&file_name) {
                ReconcileOutcome::Seen
            } else if under_unreadable(output_dir, rel, &unreadable) {
                ReconcileOutcome::Unknown
            } else {
                ReconcileOutcome::Gone
            };
            (rel.clone(), outcome)
        })
        .collect();

    // Detect new files (on disk but not in DB, extension allowed).
    let known_lower: std::collections::HashSet<String> = known_paths
        .iter()
        .filter_map(|r| Path::new(r).file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()))
        .collect();

    let new_files = on_disk
        .into_iter()
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase());
            name.as_ref().is_some_and(|n| {
                !known_lower.contains(n.as_str())
                    && crate::watcher::extension_allowed(n, allowed_extensions)
            })
        })
        .filter_map(|p| {
            let file_name = p.file_name()?.to_string_lossy().into_owned();
            let (size_bytes, file_mtime) = metadata_fn(&p)?;
            Some(NewDetection { absolute_path: p, file_name, size_bytes, file_mtime })
        })
        .collect();

    Ok(ReconcileReport { existing, new_files, unreadable_dirs: unreadable })
}

/// Whether the DB row `rel` resolves to a path inside one of the `unreadable`
/// directories.
///
/// `Path::join` returns `rel` unchanged when it is already absolute, which covers
/// both the absolute paths the detect phase stores and project-relative rows.
fn under_unreadable(output_dir: &Path, rel: &str, unreadable: &[PathBuf]) -> bool {
    if unreadable.is_empty() {
        return false;
    }
    let absolute = output_dir.join(rel);
    unreadable.iter().any(|dir| absolute.starts_with(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn fake_meta(size: u64) -> (u64, SystemTime) {
        (size, SystemTime::UNIX_EPOCH)
    }

    fn make_read_dir(files: Vec<&str>) -> impl Fn(&Path) -> Result<DirListing, String> {
        let dir = PathBuf::from("/output");
        let paths: Vec<PathBuf> = files.into_iter().map(|f| dir.join(f)).collect();
        move |_| Ok(DirListing { files: paths.clone(), unreadable: Vec::new() })
    }

    #[test]
    fn detects_new_file_not_in_db() {
        let read_dir = make_read_dir(vec!["integration_M31.xisf"]);
        let meta = |_: &Path| Some(fake_meta(1024));

        let report = reconcile(
            Path::new("/output"),
            &[],
            crate::watcher::DEFAULT_WATCH_EXTENSIONS,
            &read_dir,
            &meta,
        )
        .unwrap();

        assert_eq!(report.new_files.len(), 1);
        assert_eq!(report.new_files[0].file_name, "integration_M31.xisf");
        assert!(report.existing.is_empty());
    }

    #[test]
    fn existing_file_marked_seen() {
        let read_dir = make_read_dir(vec!["MasterDark.xisf"]);
        let meta = |_: &Path| Some(fake_meta(2048));
        let known = vec!["MasterDark.xisf".to_owned()];

        let report = reconcile(
            Path::new("/output"),
            &known,
            crate::watcher::DEFAULT_WATCH_EXTENSIONS,
            &read_dir,
            &meta,
        )
        .unwrap();

        assert_eq!(report.existing.len(), 1);
        assert!(matches!(report.existing[0], (_, ReconcileOutcome::Seen)));
        assert!(report.new_files.is_empty());
    }

    #[test]
    fn missing_file_marked_gone() {
        // DB knows about "result.xisf" but disk shows only "other.xisf".
        let read_dir = make_read_dir(vec!["other.xisf"]);
        let meta = |_: &Path| Some(fake_meta(512));
        let known = vec!["result.xisf".to_owned()];

        let report = reconcile(
            Path::new("/output"),
            &known,
            crate::watcher::DEFAULT_WATCH_EXTENSIONS,
            &read_dir,
            &meta,
        )
        .unwrap();

        assert_eq!(report.existing.len(), 1);
        assert!(matches!(report.existing[0], (_, ReconcileOutcome::Gone)));
        // "other.xisf" is new.
        assert_eq!(report.new_files.len(), 1);
    }

    #[test]
    fn row_under_an_unreadable_directory_is_unknown_not_gone() {
        let locked = PathBuf::from("/output/locked");
        let read_dir = move |_: &Path| {
            Ok(DirListing {
                files: vec![PathBuf::from("/output/visible.xisf")],
                unreadable: vec![locked.clone()],
            })
        };
        let meta = |_: &Path| Some(fake_meta(512));
        let known = vec![
            "/output/locked/result.xisf".to_owned(),
            "/output/elsewhere/vanished.xisf".to_owned(),
        ];

        let report = reconcile(
            Path::new("/output"),
            &known,
            crate::watcher::DEFAULT_WATCH_EXTENSIONS,
            &read_dir,
            &meta,
        )
        .unwrap();

        assert_eq!(
            report.existing,
            vec![
                ("/output/locked/result.xisf".to_owned(), ReconcileOutcome::Unknown),
                ("/output/elsewhere/vanished.xisf".to_owned(), ReconcileOutcome::Gone),
            ]
        );
        assert_eq!(report.unreadable_dirs, vec![PathBuf::from("/output/locked")]);
    }

    #[test]
    fn extension_filter_excludes_disallowed_files() {
        let read_dir = make_read_dir(vec!["result.xisf", "notes.txt", "script.py"]);
        let meta = |_: &Path| Some(fake_meta(100));

        let report = reconcile(
            Path::new("/output"),
            &[],
            crate::watcher::DEFAULT_WATCH_EXTENSIONS,
            &read_dir,
            &meta,
        )
        .unwrap();

        assert_eq!(report.new_files.len(), 1);
        assert_eq!(report.new_files[0].file_name, "result.xisf");
    }
}
