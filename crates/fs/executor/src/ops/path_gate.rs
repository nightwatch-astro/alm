// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Lexical path-resolution gate (spec 033, T018, FR-001/002, D8).
//!
//! Enforces the §II promise that every executor operation resolves under its
//! registered library root and does not traverse symlinks or junctions.
//!
//! ## Algorithm (D8)
//!
//! 1. Join the item's relative path onto the **absolute** library root.
//! 2. Normalize lexically: collapse `.` and `..` components **without** calling
//!    `std::fs::canonicalize` (which would follow symlinks — forbidden by the
//!    Product Constraints).
//! 3. Refuse if the normalized path does not start with the root prefix
//!    (`root_escape`).
//! 4. lstat each component of the *relative* portion: refuse if any is a
//!    symlink or junction (`symlink`).
//!
//! The caller (executor loop) calls `resolve_and_validate` before performing
//! any filesystem mutation.

use camino::{Utf8Path, Utf8PathBuf};

use crate::failure::{FailureCode, PlanItemFailure};

/// Resolved, validated absolute path (guaranteed UTF-8).
#[derive(Debug, Clone)]
pub struct ResolvedPath(pub Utf8PathBuf);

impl ResolvedPath {
    /// Return a reference to the inner path.
    #[must_use]
    pub fn as_path(&self) -> &Utf8Path {
        &self.0
    }
}

/// Resolve `relative` against `root` and validate the result.
///
/// `relative` may already be absolute (source-view plan items store an absolute
/// destination); joining it onto `root` yields the path itself, so containment
/// is decided by the same root-prefix check.
///
/// Returns `Ok(ResolvedPath)` when the path:
/// - resolves under `root` (no escape via `..`)
/// - contains no symlink or junction component
///
/// Returns `Err(PlanItemFailure)` with:
/// - `root_escape` — the normalized path escapes the root.
/// - `symlink` — a path component is a symlink (or junction on Windows).
///
/// # Errors
///
/// Returns a structured `PlanItemFailure` on any validation failure.
pub fn resolve_and_validate(
    root: &Utf8Path,
    relative: &Utf8Path,
) -> Result<ResolvedPath, PlanItemFailure> {
    // Steps 1-2: join, normalize, and prove containment. The rule lives in
    // `fs_pathsafe::contain` so this gate and every other root-relative
    // resolver in the workspace cannot drift apart
    // (`specs/tiny/root-relative-path-containment.md`).
    let normalized =
        fs_pathsafe::contain::resolve_in_root_utf8(root, relative).map_err(containment_failure)?;
    // The containment verdict is against the *normalized* root, so the walk
    // below must strip that same prefix rather than the caller's spelling.
    let root = &fs_pathsafe::contain::normalize_utf8(root);

    // Step 3: per-component lstat for symlinks/junctions.
    // Walk the normalized path's portion below the root only (the root itself
    // may legitimately be a symlink at the mount level — we do not follow
    // further). Taking it from `normalized` rather than from `relative` keeps
    // the walk anchored when `relative` is already absolute: pushing its root
    // component onto `current` would otherwise restart the walk at `/` and
    // refuse every ancestor symlink above the root (`/var` on macOS).
    // Cannot fail: the root-prefix check above already ran. An empty walk is
    // the conservative answer rather than a panic inside a safety gate.
    let below_root = normalized.strip_prefix(root).unwrap_or_else(|_| Utf8Path::new(""));
    let mut current = root.to_path_buf();
    for component in below_root.components() {
        current.push(component);
        // If the path doesn't exist yet (e.g. new destination dir), stop checking —
        // there can be no symlink for a non-existent path component.
        match current.symlink_metadata() {
            Ok(meta) => {
                // Shared reparse-aware classification (symlink + Windows
                // FILE_ATTRIBUTE_REPARSE_POINT junction check) — see
                // `fs_pathsafe` for the one implementation both this gate and
                // `fs_inventory` walk against.
                if fs_pathsafe::is_link_or_junction_metadata(&meta) {
                    return Err(PlanItemFailure::with_code(
                        FailureCode::SymlinkComponent,
                        format!(
                            "path component '{current}' is a symlink or junction; \
                             refusing to traverse (link-following is disabled for this root)"
                        ),
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Component doesn't exist — no symlink possible, stop checking.
                break;
            }
            Err(e) => {
                // Other lstat error (e.g. permission denied). Treat as a hard failure
                // to avoid silently allowing traversal of unreadable paths.
                return Err(PlanItemFailure::with_code(
                    FailureCode::PathInvalid,
                    format!("cannot lstat path component '{current}': {e}"),
                ));
            }
        }
    }

    Ok(ResolvedPath(normalized))
}

/// Map a containment refusal onto the executor's failure taxonomy.
///
/// Only [`ContainmentError::Escapes`] is `root_escape`; a malformed root or an
/// unrooted path that cannot be resolved is a bad plan item, not a traversal
/// attempt, and the two must stay distinguishable in the audit record.
pub fn containment_failure(error: fs_pathsafe::contain::ContainmentError) -> PlanItemFailure {
    use fs_pathsafe::contain::ContainmentError as E;
    let code = match error {
        E::Escapes { .. } => FailureCode::RootEscape,
        E::RootMissing { .. }
        | E::RootNotAbsolute { .. }
        | E::NotAbsolute { .. }
        | E::NotNormalized { .. } => FailureCode::PathInvalid,
    };
    PlanItemFailure::with_code(code, error.to_string())
}

/// Lexically normalize a path by collapsing `.` and `..` components **without**
/// making any filesystem calls (no `canonicalize`).
///
/// - `.` components are dropped.
/// - `..` components pop the last pushed component (or are ignored at root).
/// - Absolute path prefixes (root component) are preserved.
///
/// This is intentionally conservative: it does not strip Windows UNC
/// prefixes or long-path `\\?\` prefixes.
///
/// spec 042 (T206): the lexical collapse is delegated to `path-clean`, whose
/// `PathClean::clean` implements the same purely-lexical algorithm (drop `.`,
/// pop the element preceding an inner `..`, drop a leading `..` on a rooted
/// path, preserve the root/prefix). This is *only* the lexical step — the
/// symlink/junction safety walk in [`resolve_and_validate`] is unchanged and
/// still runs `symlink_metadata` per component of the original relative path,
/// so the no-link-following guard (Product Constraints §II) is preserved. The
/// `lexical_normalize_*` unit tests are the equivalence guard for the collapse.
#[must_use]
pub fn lexical_normalize(path: &Utf8Path) -> Utf8PathBuf {
    use path_clean::PathClean as _;
    // `path-clean` operates on `std::path`; bridge through it. The input is
    // already guaranteed UTF-8 and `clean` only drops/pops/reorders existing
    // components (it never synthesizes new bytes), so the cleaned result is
    // UTF-8 by construction — the back-conversion cannot lose data.
    let cleaned = path.as_std_path().clean();
    Utf8PathBuf::from_path_buf(cleaned)
        .unwrap_or_else(|p| Utf8PathBuf::from(p.to_string_lossy().into_owned()))
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Convert a tempdir path to a guaranteed-UTF-8 path for the tests.
    fn utf8_root(p: &std::path::Path) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(p.to_path_buf()).expect("temp dir path is UTF-8")
    }

    #[test]
    fn lexical_normalize_simple_path() {
        let p = Utf8PathBuf::from("/lib/root/./sub/../file.fits");
        let n = lexical_normalize(&p);
        assert_eq!(n, Utf8PathBuf::from("/lib/root/file.fits"));
    }

    #[test]
    fn lexical_normalize_no_escape_at_root() {
        // `..` at the root should not escape.
        let p = Utf8PathBuf::from("/../../file.fits");
        let n = lexical_normalize(&p);
        assert_eq!(n, Utf8PathBuf::from("/file.fits"));
    }

    #[test]
    fn lexical_normalize_deep_traversal() {
        let p = Utf8PathBuf::from("/a/b/c/../../d");
        let n = lexical_normalize(&p);
        assert_eq!(n, Utf8PathBuf::from("/a/d"));
    }

    #[test]
    fn resolve_and_validate_normal_path_ok() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_root(dir.path());
        let rel = Utf8Path::new("subdir/file.fits");
        std::fs::create_dir_all(root.join("subdir")).unwrap();
        std::fs::write(root.join("subdir/file.fits"), b"data").unwrap();

        let result = resolve_and_validate(&root, rel);
        assert!(result.is_ok(), "should succeed for a normal sub-path");
        let resolved = result.unwrap();
        assert_eq!(resolved.as_path(), root.join("subdir/file.fits"));
    }

    #[test]
    fn resolve_and_validate_nonexistent_path_ok() {
        // A destination that does not exist yet is fine — no symlink to check.
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_root(dir.path());
        let rel = Utf8Path::new("new_dir/new_file.fits");

        let result = resolve_and_validate(&root, rel);
        assert!(result.is_ok());
    }

    #[test]
    fn resolve_and_validate_root_escape_via_dotdot() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_root(dir.path());
        // Try to escape with `../secret.fits`
        let rel = Utf8Path::new("../secret.fits");

        let result = resolve_and_validate(&root, rel);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, FailureCode::RootEscape);
        assert!(err.message.contains("escapes"));
    }

    #[test]
    fn resolve_and_validate_root_escape_nested() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_root(dir.path());
        // Nested path that normalizes to an escape.
        let rel = Utf8Path::new("a/b/../../..");
        let result = resolve_and_validate(&root, rel);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, FailureCode::RootEscape);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_and_validate_symlink_component_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_root(dir.path());
        let target = root.join("actual_dir");
        std::fs::create_dir_all(&target).unwrap();
        let link = root.join("linked");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        // Try to traverse through the symlink.
        let rel = Utf8Path::new("linked/file.fits");

        let result = resolve_and_validate(&root, rel);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, FailureCode::SymlinkComponent);
        assert!(err.message.contains("symlink"));
    }

    /// astro-plan-d8cyr: source-view plan items store an absolute destination,
    /// and the symlink walk must stay below the root. Walking the absolute path
    /// from `/` refuses any ancestor symlink above the root, which on macOS is
    /// every temp dir (`/var` -> `private/var`).
    #[test]
    fn resolve_and_validate_absolute_path_under_root_ok() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_root(dir.path());
        let absolute = root.join("views/frame.fits");

        let resolved = resolve_and_validate(&root, &absolute).expect("absolute path under root");
        assert_eq!(resolved.as_path(), absolute);
    }

    #[test]
    fn resolve_and_validate_absolute_path_outside_root_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_root(dir.path());
        let outside = Utf8PathBuf::from_path_buf(
            dir.path().parent().expect("temp parent").join("outside.fits"),
        )
        .expect("temp dir path is UTF-8");

        let err = resolve_and_validate(&root, &outside).unwrap_err();
        assert_eq!(err.code, FailureCode::RootEscape);
    }
}
