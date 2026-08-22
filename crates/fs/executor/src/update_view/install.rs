// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::too_many_lines, clippy::missing_errors_doc)]

//! Additive no-clobber file install primitive (spec 062 FR-100).

use std::io::{Read, Write};

use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest as _, Sha256};

/// Error from an install step.
#[derive(Debug, Clone)]
pub struct InstallError {
    pub code: InstallErrorCode,
    pub message: String,
}

/// Typed install failure codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallErrorCode {
    SourceUnavailable,
    DestinationExists,
    SourcePathUnsafe,
    DestPathUnsafe,
    Io,
    RaceConflict,
}

/// Outcome of a successful install.
#[derive(Debug)]
pub struct InstallOutcome {
    /// SHA-256 hex fingerprint of the installed bytes.
    pub content_fingerprint: String,
    /// Opaque ownership token for crash recovery.
    pub ownership_token: String,
}

/// Install `source_rel_path` (relative to `source_root`) to `dest_rel_path`
/// (relative to `dest_root`) using atomic no-replace semantics.
///
/// # No-clobber guarantee
///
/// If `dest_root/dest_rel_path` exists at any point, `DestinationExists` or
/// `RaceConflict` is returned. The caller must treat either as a collision.
///
/// # Errors
///
/// Returns [`InstallError`] for path-safety violations, I/O failures, or
/// no-clobber conflicts.
pub fn install_item(
    source_root: &Utf8Path,
    source_rel_path: &str,
    dest_root: &Utf8Path,
    dest_rel_path: &str,
) -> Result<InstallOutcome, InstallError> {
    let source_abs = resolve_no_follow(source_root, source_rel_path, true)?;
    let dest_abs = resolve_no_follow(dest_root, dest_rel_path, false)?;

    // No pre-flight exists() check — persist_noclobber below is the sole atomic
    // no-clobber gate (TOCTOU-safe). Callers map RaceConflict as the collision code.

    if let Some(parent) = dest_abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| InstallError {
            code: InstallErrorCode::Io,
            message: format!("create_dir_all {parent}: {e}"),
        })?;
    }

    let mut src_file = open_no_follow(&source_abs)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 65536];
    let mut all_bytes: Vec<u8> = Vec::new();

    loop {
        let n = src_file.read(&mut buf).map_err(|e| InstallError {
            code: InstallErrorCode::Io,
            message: format!("read {source_abs}: {e}"),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        all_bytes.extend_from_slice(&buf[..n]);
    }

    let fingerprint = format!("sha256:{}", hex::encode(hasher.finalize()));

    let dest_parent = dest_abs
        .parent()
        .map_or_else(|| dest_root.as_std_path().to_owned(), |p| p.as_std_path().to_owned());

    let mut tmp = tempfile::NamedTempFile::new_in(&dest_parent).map_err(|e| InstallError {
        code: InstallErrorCode::Io,
        message: format!("tempfile in {}: {e}", dest_parent.display()),
    })?;

    tmp.write_all(&all_bytes).map_err(|e| InstallError {
        code: InstallErrorCode::Io,
        message: format!("write temp: {e}"),
    })?;

    tmp.as_file().sync_all().map_err(|e| InstallError {
        code: InstallErrorCode::Io,
        message: format!("fsync temp: {e}"),
    })?;

    let tmp_path = tmp.path().to_owned();
    tmp.persist_noclobber(dest_abs.as_std_path()).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            InstallError {
                code: InstallErrorCode::RaceConflict,
                message: format!("destination appeared during install: {dest_abs}"),
            }
        } else {
            InstallError {
                code: InstallErrorCode::Io,
                message: format!("atomic rename {} → {dest_abs}: {}", tmp_path.display(), e.error),
            }
        }
    })?;

    if let Some(parent) = dest_abs.parent() {
        if let Ok(dir) = std::fs::File::open(parent.as_std_path()) {
            let _ = dir.sync_all();
        }
    }

    let ownership_token = derive_ownership_token(&dest_abs);
    Ok(InstallOutcome { content_fingerprint: fingerprint, ownership_token })
}

fn resolve_no_follow(
    root: &Utf8Path,
    rel: &str,
    must_exist: bool,
) -> Result<Utf8PathBuf, InstallError> {
    let abs = root.join(rel);
    if rel.contains("..") || rel.starts_with('/') || rel.starts_with('\\') {
        return Err(InstallError {
            code: if must_exist {
                InstallErrorCode::SourcePathUnsafe
            } else {
                InstallErrorCode::DestPathUnsafe
            },
            message: format!("unsafe relative path: {rel}"),
        });
    }
    if must_exist && !abs.exists() {
        return Err(InstallError {
            code: InstallErrorCode::SourceUnavailable,
            message: format!("source not found: {abs}"),
        });
    }
    Ok(abs)
}

fn open_no_follow(abs: &Utf8PathBuf) -> Result<std::fs::File, InstallError> {
    // The shared primitive, not `file_type().is_symlink()`: a Windows junction
    // is a reparse point that `is_symlink()` need not report.
    if fs_pathsafe::is_link_or_junction(abs.as_std_path()) {
        return Err(InstallError {
            code: InstallErrorCode::SourcePathUnsafe,
            message: format!("source is a symlink: {abs}"),
        });
    }
    std::fs::File::open(abs.as_std_path()).map_err(|e| InstallError {
        code: InstallErrorCode::SourceUnavailable,
        message: format!("open {abs}: {e}"),
    })
}

fn derive_ownership_token(dest: &Utf8PathBuf) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if let Ok(meta) = std::fs::metadata(dest.as_std_path()) {
            return format!("inode:{}:{}", meta.ino(), meta.dev());
        }
    }
    #[cfg(not(unix))]
    let _ = dest;
    format!("uuid:{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use sha2::Sha256;

    fn utf8(p: std::path::PathBuf) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(p).expect("utf8 path")
    }

    #[test]
    fn happy_path_installs_file_with_correct_fingerprint() {
        let src_dir = tempfile::TempDir::new().unwrap();
        let dest_dir = tempfile::TempDir::new().unwrap();
        let content = b"hello update view";
        let src_file = src_dir.path().join("frame.fits");
        std::fs::write(&src_file, content).unwrap();

        let outcome = install_item(
            &utf8(src_dir.path().to_owned()),
            "frame.fits",
            &utf8(dest_dir.path().to_owned()),
            "session-a/frame.fits",
        )
        .expect("install should succeed");

        // Destination file must exist with correct content.
        let dest_path = dest_dir.path().join("session-a").join("frame.fits");
        assert!(dest_path.exists(), "destination file must exist");
        assert_eq!(std::fs::read(&dest_path).unwrap(), content);

        let mut h = Sha256::new();
        sha2::Digest::update(&mut h, content);
        let expected = format!("sha256:{}", hex::encode(sha2::Digest::finalize(h)));
        assert_eq!(outcome.content_fingerprint, expected);

        // Ownership token must be non-empty.
        assert!(!outcome.ownership_token.is_empty());
    }

    #[test]
    fn race_conflict_when_destination_already_exists() {
        let src_dir = tempfile::TempDir::new().unwrap();
        let dest_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(src_dir.path().join("f.fits"), b"data").unwrap();

        // Pre-create destination file to trigger no-clobber conflict.
        let dest_subdir = dest_dir.path().join("s");
        std::fs::create_dir_all(&dest_subdir).unwrap();
        std::fs::write(dest_subdir.join("f.fits"), b"existing").unwrap();

        let err = install_item(
            &utf8(src_dir.path().to_owned()),
            "f.fits",
            &utf8(dest_dir.path().to_owned()),
            "s/f.fits",
        )
        .expect_err("should fail with conflict");

        // persist_noclobber returns AlreadyExists → RaceConflict.
        assert_eq!(
            err.code,
            InstallErrorCode::RaceConflict,
            "expected RaceConflict not {:?}",
            err.code
        );
    }

    #[test]
    fn symlink_in_source_is_rejected() {
        let src_dir = tempfile::TempDir::new().unwrap();
        let dest_dir = tempfile::TempDir::new().unwrap();
        let real_file = src_dir.path().join("real.fits");
        std::fs::write(&real_file, b"data").unwrap();
        let link = src_dir.path().join("link.fits");

        // Create a symlink using the platform-appropriate API.
        // The rejection check is `fs_pathsafe::is_link_or_junction`, which is
        // cross-platform, so the assertion must hold on all three platforms.
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_file, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&real_file, &link).unwrap();
        // Non-unix, non-windows targets (wasm etc.) cannot create symlinks;
        // skip the test body rather than fail to compile.
        #[cfg(not(any(unix, windows)))]
        return;

        let err = install_item(
            &utf8(src_dir.path().to_owned()),
            "link.fits",
            &utf8(dest_dir.path().to_owned()),
            "out.fits",
        )
        .expect_err("symlink should be rejected");

        assert_eq!(err.code, InstallErrorCode::SourcePathUnsafe);
    }

    #[test]
    fn path_traversal_in_source_is_rejected() {
        let src_dir = tempfile::TempDir::new().unwrap();
        let dest_dir = tempfile::TempDir::new().unwrap();

        let err = install_item(
            &utf8(src_dir.path().to_owned()),
            "../escape.fits", // traversal
            &utf8(dest_dir.path().to_owned()),
            "out.fits",
        )
        .expect_err("traversal should be rejected");

        assert_eq!(err.code, InstallErrorCode::SourcePathUnsafe);
    }

    #[test]
    fn source_not_found_returns_unavailable() {
        let src_dir = tempfile::TempDir::new().unwrap();
        let dest_dir = tempfile::TempDir::new().unwrap();

        let err = install_item(
            &utf8(src_dir.path().to_owned()),
            "nonexistent.fits",
            &utf8(dest_dir.path().to_owned()),
            "out.fits",
        )
        .expect_err("missing source should fail");

        assert_eq!(err.code, InstallErrorCode::SourceUnavailable);
    }
}
