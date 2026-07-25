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

    if dest_abs.exists() {
        return Err(InstallError {
            code: InstallErrorCode::DestinationExists,
            message: format!("destination already exists: {dest_abs}"),
        });
    }

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
    let is_symlink =
        std::fs::symlink_metadata(abs.as_std_path()).is_ok_and(|m| m.file_type().is_symlink());
    if is_symlink {
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
