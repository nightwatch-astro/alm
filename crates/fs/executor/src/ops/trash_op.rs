// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! OS trash primitive (spec 025, spec 033 T022, FR-006, D4).
//!
//! Moves the file to the OS recycle bin / Trash / XDG trash via the `trash`
//! crate (MIT, 5.2.x). When the trash API is unavailable or fails, falls back
//! to moving the file into `fallback_archive_dest` (an archive path computed
//! by the caller) and records which destination was actually used.
//!
//! Constitution §II: destructive ops prefer trash/archive over permanent delete;
//! trash is never assumed to succeed; failure is logged and recorded distinctly.

use camino::Utf8Path;

use crate::failure::{FailureCode, PlanItemFailure, RollbackOutcome};

/// Result of a trash (or fallback-archive) operation.
///
/// `destination_used` records whether `"trash"` or `"archive"` was the actual
/// destination — required by FR-006 ("record which destination was used").
#[derive(Debug)]
pub struct TrashResult {
    pub rollback_attempted: bool,
    pub rollback_outcome: RollbackOutcome,
    pub rollback_message: Option<String>,
    /// `"trash"` if the OS bin was used; `"archive"` if archive fallback fired.
    pub destination_used: &'static str,
}

impl Default for TrashResult {
    fn default() -> Self {
        Self {
            rollback_attempted: false,
            rollback_outcome: RollbackOutcome::NotApplicable,
            rollback_message: None,
            destination_used: "trash",
        }
    }
}

/// Headless OS-trash double for the e2e harness.
///
/// The OS Shell trash needs an interactive window-station/desktop; on a
/// headless CI runner `trash::delete` (`IFileOperation::PerformOperations` on
/// Windows) blocks forever. A real Recycle-Bin move is unperformable there, so
/// under this seam [`trash_file`] removes the file deterministically instead.
/// The observable side effect the real-UI journeys assert — the file leaves the
/// archive subtree — is identical, and the real OS-trash primitive stays
/// covered by this module's unit tests and by live use.
///
/// The whole module is behind the default-off `e2e-trash-fake` feature, so a
/// release binary contains no way to divert a real destructive plan apply
/// (constitution §II).
///
/// Two switches because there are two kinds of consumer:
///
/// - [`FakeTrashGuard`] for a test that calls the executor in-process. Setting
///   the environment instead would mean `set_var` in a threaded test binary,
///   which races every concurrent `var_os` reader — `tempfile`'s `TMPDIR`
///   lookup among them — undefined behaviour on glibc and already banned in
///   `apps/desktop/src-tauri/src/data_dir.rs`.
/// - `PV_E2E_OS_TRASH_FAKE` for the e2e harness, which cannot use the guard: it
///   drives a separate `desktop_shell` process and sets the variable in that
///   child's initial environment (`crates/e2e-tests/tests/common/helpers.rs`),
///   never in its own.
#[cfg(feature = "e2e-trash-fake")]
pub mod fake {
    use std::sync::atomic::{AtomicBool, Ordering};

    static FORCED: AtomicBool = AtomicBool::new(false);

    /// Divert OS trash to a plain file removal for as long as the guard lives.
    ///
    /// Process-global: the executor is reached through async plumbing that no
    /// test can thread a parameter through.
    pub struct FakeTrashGuard;

    impl FakeTrashGuard {
        #[must_use]
        pub fn new() -> Self {
            FORCED.store(true, Ordering::Relaxed);
            Self
        }
    }

    impl Default for FakeTrashGuard {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Restores real OS trash on drop, including on panic unwind, so one test
    /// cannot divert the trash path of every test that follows it.
    impl Drop for FakeTrashGuard {
        fn drop(&mut self) {
            FORCED.store(false, Ordering::Relaxed);
        }
    }

    pub(super) fn enabled() -> bool {
        FORCED.load(Ordering::Relaxed) || std::env::var_os("PV_E2E_OS_TRASH_FAKE").is_some()
    }
}

/// Send `path` to the OS trash.
///
/// When `fallback_archive_dest` is `Some`, a failed trash attempt falls back
/// to archiving the file there and returns `Ok` with `destination_used =
/// "archive"`. When it is `None`, a failed trash returns `Err`.
///
/// # Errors
///
/// Returns `(PlanItemFailure, TrashResult)` when both trash and the archive
/// fallback fail (or when no fallback is provided and trash fails).
pub fn trash_file(
    path: &Utf8Path,
    fallback_archive_dest: Option<&Utf8Path>,
) -> Result<TrashResult, (PlanItemFailure, TrashResult)> {
    #[cfg(feature = "e2e-trash-fake")]
    if fake::enabled() {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(TrashResult { destination_used: "trash", ..TrashResult::default() }),
            Err(e) => Err((
                PlanItemFailure::with_code(
                    FailureCode::TrashUnavailable,
                    format!("e2e fake-trash removal failed for '{path}': {e}"),
                ),
                TrashResult::default(),
            )),
        };
    }

    match trash::delete(path) {
        Ok(()) => Ok(TrashResult { destination_used: "trash", ..TrashResult::default() }),
        Err(trash_err) => {
            // Classify the trash error.
            let (failure_code, message) = classify_trash_error(&trash_err, path);
            tracing::warn!(
                path = %path,
                error = %trash_err,
                "OS trash failed; {}",
                if fallback_archive_dest.is_some() { "trying archive fallback" } else { "no fallback configured" }
            );

            if let Some(archive_dest) = fallback_archive_dest {
                // Try archive fallback (FR-006).
                match crate::ops::archive_op::archive_file(path, archive_dest) {
                    Ok(()) => {
                        tracing::info!(
                            path = %path,
                            archive_dest = %archive_dest,
                            "archive fallback succeeded after trash failure"
                        );
                        return Ok(TrashResult {
                            rollback_attempted: false,
                            rollback_outcome: RollbackOutcome::NotApplicable,
                            rollback_message: Some(format!(
                                "trash unavailable ({trash_err}); fell back to archive at {archive_dest}"
                            )),
                            destination_used: "archive",
                        });
                    }
                    Err((archive_failure, archive_result)) => {
                        return Err((
                            PlanItemFailure::with_code(
                                FailureCode::TrashUnavailable,
                                format!(
                                    "trash failed ({trash_err}) and archive fallback also failed: {}",
                                    archive_failure.message
                                ),
                            ),
                            TrashResult {
                                rollback_attempted: archive_result.rollback_attempted,
                                rollback_outcome: archive_result.rollback_outcome,
                                rollback_message: archive_result.rollback_message,
                                destination_used: "archive",
                            },
                        ));
                    }
                }
            }

            Err((PlanItemFailure::with_code(failure_code, message), TrashResult::default()))
        }
    }
}

fn classify_trash_error(err: &trash::Error, path: &Utf8Path) -> (FailureCode, String) {
    // The `trash` crate's Error enum includes OS-specific variants.
    // Map to our FailureCode taxonomy.
    let msg = format!("OS trash failed for '{path}': {err}");
    // `trash::Error` doesn't expose a rich enum in all versions; use Display
    // to detect common cases.
    let failure_code = classify_trash_error_message(&err.to_string());
    (failure_code, msg)
}

fn classify_trash_error_message(message: &str) -> FailureCode {
    let err_str = message.to_lowercase();
    if err_str.contains("permission") || err_str.contains("access denied") {
        FailureCode::OsTrashPermissionDenied
    } else if err_str.contains("full") || err_str.contains("no space") {
        FailureCode::OsTrashFull
    } else {
        FailureCode::TrashUnavailable
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn utf8(p: &std::path::Path) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(p.to_path_buf()).expect("temp dir path is UTF-8")
    }

    #[test]
    fn trash_error_messages_map_to_producible_codes() {
        let cases = [
            ("permission denied", FailureCode::OsTrashPermissionDenied),
            ("access denied", FailureCode::OsTrashPermissionDenied),
            ("trash is full", FailureCode::OsTrashFull),
            ("no space left", FailureCode::OsTrashFull),
            ("trash service unavailable", FailureCode::TrashUnavailable),
        ];

        for (message, expected) in cases {
            assert_eq!(classify_trash_error_message(message), expected, "failed for {message}");
        }
    }

    /// T014: trash destination moves to OS bin; archive fallback recorded when
    /// unavailable; replaces the old `trash_returns_unavailable_in_v1` stub test.
    #[test]
    fn trash_moves_file_to_os_bin_or_uses_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let file = utf8(dir.path()).join("to_trash.fits");
        std::fs::write(&file, b"data").unwrap();

        // Attempt trash with no fallback.
        // On Linux CI environments the XDG trash may or may not be available.
        // We assert on the contract invariants rather than a hard success/failure:
        // - If trash succeeds: file gone, destination_used = "trash".
        // - If trash fails (TrashUnavailable / OsTrashPermissionDenied): Err returned,
        //   file still present (no silent loss).
        let result = trash_file(&file, None);
        match result {
            Ok(r) => {
                assert_eq!(r.destination_used, "trash");
                assert!(!file.exists(), "file should be gone after successful trash");
            }
            Err((failure, _)) => {
                assert!(
                    matches!(
                        failure.code,
                        FailureCode::TrashUnavailable
                            | FailureCode::OsTrashPermissionDenied
                            | FailureCode::OsTrashFull
                    ),
                    "unexpected failure code: {:?}",
                    failure.code
                );
                // File must still be present — no silent loss.
                assert!(file.exists(), "file must survive a failed trash (no silent loss)");
            }
        }
    }

    #[test]
    fn trash_uses_archive_fallback_when_trash_unavailable() {
        // We can't force the OS trash to fail in a unit test, but we CAN verify
        // the fallback logic by using a path that we know the trash crate will
        // successfully trash or, if not, that the archive fallback fires.
        //
        // Strategy: use a separate directory as the archive destination and verify
        // the file ends up somewhere (not silently lost) regardless of whether
        // the trash or the archive fallback fires.
        let src_dir = tempfile::tempdir().unwrap();
        let archive_dir = tempfile::tempdir().unwrap();
        let file = utf8(src_dir.path()).join("important.fits");
        std::fs::write(&file, b"precious data").unwrap();
        let archive_dest = utf8(archive_dir.path()).join("important.fits");

        let result = trash_file(&file, Some(&archive_dest));
        match result {
            Ok(r) => {
                // Either trash or archive succeeded — file is safe.
                assert!(
                    matches!(r.destination_used, "trash" | "archive"),
                    "destination must be trash or archive"
                );
                assert!(!file.exists(), "source must be gone after successful operation");
                // If archive fallback was used, archive_dest exists.
                if r.destination_used == "archive" {
                    assert!(archive_dest.exists(), "archive dest should exist after fallback");
                }
            }
            Err((failure, _)) => {
                // Both trash AND archive failed (very unlikely in tests).
                assert_eq!(failure.code, FailureCode::TrashUnavailable);
                // Source must survive — no silent loss.
                assert!(file.exists(), "source must survive double-failure");
            }
        }
    }
}
