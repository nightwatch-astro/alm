// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Root containment: the one rule for resolving a caller-supplied path against
//! a library, project, or archive root.
//!
//! See `specs/tiny/root-relative-path-containment.md`.
//!
//! The verdict is purely lexical. `canonicalize` follows symlinks, which the
//! Product Constraints forbid, and it fails on a path that does not exist yet,
//! which is the normal case for a write destination. Both the path and the root
//! are normalized before the prefix comparison: `starts_with` against an
//! unnormalized root accepts `/mnt/library/../../a` as contained.
//!
//! Link refusal is a separate concern and stays in
//! `fs_executor::ops::path_gate`, which walks each component with `lstat`.

use std::path::{Path, PathBuf};

use path_clean::PathClean as _;

/// A path proven to resolve inside its root.
///
/// The inner path is lexically normalized and absolute whenever the root was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainedPath(PathBuf);

impl ContainedPath {
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

/// Why a path could not be resolved inside a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainmentError {
    /// A root-relative path was supplied with no root to resolve it against.
    RootMissing { path: PathBuf },
    /// The root itself is relative, so everything resolved against it would
    /// depend on the process working directory.
    RootNotAbsolute { root: PathBuf },
    /// The path resolves outside its root.
    Escapes { root: PathBuf, resolved: PathBuf },
    /// An unrooted path is relative, so it would resolve against the process
    /// working directory.
    NotAbsolute { path: PathBuf },
    /// An unrooted absolute path carries `.` or `..` components, so its target
    /// depends on where the caller reads it.
    NotNormalized { path: PathBuf, normalized: PathBuf },
}

impl std::fmt::Display for ContainmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootMissing { path } => {
                write!(f, "path '{}' is root-relative but no root was resolved", path.display())
            }
            Self::RootNotAbsolute { root } => write!(
                f,
                "root '{}' is not absolute; it would resolve against the process working \
                 directory",
                root.display()
            ),
            Self::Escapes { root, resolved } => write!(
                f,
                "path escapes root '{}' after normalization; resolved to '{}'",
                root.display(),
                resolved.display()
            ),
            Self::NotAbsolute { path } => write!(
                f,
                "path '{}' has no root and is not absolute; it would resolve against the \
                 process working directory",
                path.display()
            ),
            Self::NotNormalized { path, normalized } => write!(
                f,
                "unrooted path '{}' carries '.' or '..' components; normalizes to '{}'",
                path.display(),
                normalized.display()
            ),
        }
    }
}

impl std::error::Error for ContainmentError {}

/// Lexically normalize a path: collapse `.` and inner `..`, preserve the root or
/// prefix component, make no filesystem call.
#[must_use]
pub fn normalize(path: &Path) -> PathBuf {
    path.clean()
}

/// Resolve `path` against `root` and prove the result stays inside it.
///
/// An absolute `path` is used as supplied rather than joined, so containment is
/// decided by the same prefix check either way. A leaf that does not exist is
/// contained on its lexical form alone.
///
/// # Errors
///
/// [`ContainmentError::RootNotAbsolute`] for a relative root;
/// [`ContainmentError::Escapes`] when the normalized result does not start with
/// the normalized root.
pub fn resolve_in_root(root: &Path, path: &Path) -> Result<ContainedPath, ContainmentError> {
    if !root.is_absolute() {
        return Err(ContainmentError::RootNotAbsolute { root: root.to_path_buf() });
    }
    let root = normalize(root);
    let resolved = normalize(&root.join(path));
    if resolved.starts_with(&root) {
        Ok(ContainedPath(resolved))
    } else {
        Err(ContainmentError::Escapes { root, resolved })
    }
}

/// Accept a path that carries no root, which requires it to be already absolute
/// and already normal.
///
/// Callers name this entry point to opt into an unrooted path (plan items that
/// store a pre-resolved absolute destination). No arm of [`resolve_in_root`]
/// reaches it, so a missing root can never silently produce one.
///
/// # Errors
///
/// [`ContainmentError::NotAbsolute`] for a relative path;
/// [`ContainmentError::NotNormalized`] when normalization changes the path.
pub fn resolve_unrooted(path: &Path) -> Result<ContainedPath, ContainmentError> {
    if !path.is_absolute() {
        return Err(ContainmentError::NotAbsolute { path: path.to_path_buf() });
    }
    let normalized = normalize(path);
    if normalized == path {
        Ok(ContainedPath(normalized))
    } else {
        Err(ContainmentError::NotNormalized { path: path.to_path_buf(), normalized })
    }
}

/// Report whether `path` resolves inside at least one of `roots`.
///
/// An empty `roots` is not contained: no registered root means nothing to be
/// inside of.
#[must_use]
pub fn contained_in_any(path: &Path, roots: &[&Path]) -> bool {
    roots.iter().any(|root| resolve_in_root(root, path).is_ok())
}

/// Camino-typed [`resolve_in_root`] for callers that hold `Utf8Path`.
///
/// # Errors
///
/// As [`resolve_in_root`].
pub fn resolve_in_root_utf8(
    root: &camino::Utf8Path,
    path: &camino::Utf8Path,
) -> Result<camino::Utf8PathBuf, ContainmentError> {
    let contained = resolve_in_root(root.as_std_path(), path.as_std_path())?;
    Ok(to_utf8(contained.into_path_buf()))
}

/// Camino-typed [`resolve_unrooted`].
///
/// # Errors
///
/// As [`resolve_unrooted`].
pub fn resolve_unrooted_utf8(
    path: &camino::Utf8Path,
) -> Result<camino::Utf8PathBuf, ContainmentError> {
    let contained = resolve_unrooted(path.as_std_path())?;
    Ok(to_utf8(contained.into_path_buf()))
}

/// Camino-typed [`normalize`].
#[must_use]
pub fn normalize_utf8(path: &camino::Utf8Path) -> camino::Utf8PathBuf {
    to_utf8(normalize(path.as_std_path()))
}

/// `clean` only drops, pops, and reorders existing components, so a UTF-8 input
/// cannot produce a non-UTF-8 result; the fallback is unreachable in practice.
fn to_utf8(path: PathBuf) -> camino::Utf8PathBuf {
    camino::Utf8PathBuf::from_path_buf(path)
        .unwrap_or_else(|p| camino::Utf8PathBuf::from(p.to_string_lossy().into_owned()))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::abs_path as p;

    fn root() -> PathBuf {
        p("/mnt/library")
    }

    /// The collapse is delegated to `path-clean`; these are the equivalence
    /// guard for it, moved here when `path_gate::lexical_normalize` folded into
    /// this one implementation.
    #[test]
    fn normalize_collapses_dot_and_dotdot() {
        assert_eq!(
            normalize(Path::new("/lib/root/./sub/../file.fits")),
            Path::new("/lib/root/file.fits")
        );
    }

    #[test]
    fn normalize_does_not_escape_at_the_root() {
        assert_eq!(normalize(Path::new("/../../file.fits")), Path::new("/file.fits"));
    }

    #[test]
    fn normalize_collapses_deep_traversal() {
        assert_eq!(normalize(Path::new("/a/b/c/../../d")), Path::new("/a/d"));
    }

    #[test]
    fn relative_path_inside_the_root_resolves() {
        let contained =
            resolve_in_root(&root(), Path::new("targets/m31/light.fits")).expect("inside the root");
        assert_eq!(contained.as_path(), p("/mnt/library/targets/m31/light.fits"));
    }

    #[test]
    fn relative_path_that_escapes_after_normalization_is_refused() {
        let err = resolve_in_root(&root(), Path::new("targets/../../outside/x.fits")).unwrap_err();
        assert!(matches!(err, ContainmentError::Escapes { .. }), "{err}");
    }

    #[test]
    fn absolute_path_inside_the_root_resolves() {
        let contained =
            resolve_in_root(&root(), &p("/mnt/library/views/a.fits")).expect("inside the root");
        assert_eq!(contained.as_path(), p("/mnt/library/views/a.fits"));
    }

    #[test]
    fn absolute_path_outside_the_root_does_not_replace_it() {
        let err = resolve_in_root(&root(), &p("/etc/passwd")).unwrap_err();
        assert!(matches!(err, ContainmentError::Escapes { .. }), "{err}");
    }

    #[test]
    fn a_leaf_that_does_not_exist_is_contained() {
        // No filesystem entry is created anywhere in this crate's tests; the
        // verdict is lexical, which is what makes a write destination resolvable.
        let contained = resolve_in_root(&root(), Path::new("brand/new/dir/file.fits")).unwrap();
        assert_eq!(contained.as_path(), p("/mnt/library/brand/new/dir/file.fits"));
    }

    #[test]
    fn a_path_that_traverses_out_of_an_absolute_root_is_refused() {
        // The defect this rule replaces: `starts_with` on unnormalized text
        // accepted this pair, because the text prefix matched.
        let err = resolve_in_root(&root(), &p("/mnt/library/../../a")).unwrap_err();
        assert!(matches!(err, ContainmentError::Escapes { .. }), "{err}");
    }

    #[test]
    fn a_relative_root_is_refused() {
        let err = resolve_in_root(Path::new("library"), Path::new("x.fits")).unwrap_err();
        assert!(matches!(err, ContainmentError::RootNotAbsolute { .. }), "{err}");
    }

    #[test]
    fn unrooted_accepts_an_absolute_normal_path() {
        let contained = resolve_unrooted(&p("/mnt/archive/plan/x.fits")).unwrap();
        assert_eq!(contained.as_path(), p("/mnt/archive/plan/x.fits"));
    }

    #[test]
    fn unrooted_refuses_a_relative_path() {
        let err = resolve_unrooted(Path::new("plan/x.fits")).unwrap_err();
        assert!(matches!(err, ContainmentError::NotAbsolute { .. }), "{err}");
    }

    #[test]
    fn unrooted_refuses_a_traversing_absolute_path() {
        let err = resolve_unrooted(&p("/mnt/archive/../../etc/passwd")).unwrap_err();
        assert!(matches!(err, ContainmentError::NotNormalized { .. }), "{err}");
    }

    #[test]
    fn contained_in_any_finds_the_matching_root() {
        let (a, b) = (p("/mnt/a"), p("/mnt/b"));
        let roots = [a.as_path(), b.as_path()];
        assert!(contained_in_any(&p("/mnt/b/project"), &roots));
        assert!(!contained_in_any(&p("/tmp/scratch"), &roots));
    }

    #[test]
    fn contained_in_any_refuses_when_no_root_is_registered() {
        assert!(!contained_in_any(&p("/anywhere"), &[]));
    }

    #[test]
    fn contained_in_any_normalizes_before_comparing() {
        assert!(!contained_in_any(&p("/mnt/library/../../a"), &[root().as_path()]));
    }
}
