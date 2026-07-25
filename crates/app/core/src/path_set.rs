// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Path-set overlap comparison for the cross-plan concurrency guard
//! (spec 025 FR-017 / R-Concur-1, T007).
//!
//! Two plans may apply concurrently only if their (source ∪ destination ∪
//! archive) path sets are disjoint at subtree-prefix granularity. This module
//! owns the pure comparison; the apply use case computes each plan's claimed
//! paths and calls `PlanPathSet::first_overlap` before registering a run.
//!
//! Inputs are expected to be **lexically normalized** paths (no `.` / `..`
//! components) that are absolute whenever the owning library root is known.
//! Comparison is component-wise (via `Utf8Path::starts_with`), so sibling
//! names that share a string prefix (`/a/b` vs `/a/bc`) do NOT overlap.
//! Comparison is case-sensitive; case-insensitive filesystem semantics are a
//! documented future refinement (research R7).

use camino::{Utf8Path, Utf8PathBuf};

/// The set of path prefixes (sources, destinations, archive destinations)
/// claimed by one filesystem plan for the duration of an apply run.
#[derive(Clone, Debug, Default)]
pub struct PlanPathSet {
    prefixes: Vec<Utf8PathBuf>,
}

impl PlanPathSet {
    /// Create an empty path set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a claimed path prefix. Empty paths are ignored — they carry no
    /// claim and would otherwise prefix-match everything.
    pub fn insert(&mut self, path: Utf8PathBuf) {
        if !path.as_str().is_empty() {
            self.prefixes.push(path);
        }
    }

    /// Whether this set claims no paths at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
    }

    /// Number of claimed prefixes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.prefixes.len()
    }

    /// First overlapping pair between the two sets, if any.
    ///
    /// Two sets overlap iff any prefix in one is component-wise equal to, an
    /// ancestor of, or a descendant of any prefix in the other (subtree-prefix
    /// granularity, research R7).
    #[must_use]
    pub fn first_overlap<'a>(&'a self, other: &'a Self) -> Option<(&'a Utf8Path, &'a Utf8Path)> {
        for a in &self.prefixes {
            for b in &other.prefixes {
                if a.starts_with(b) || b.starts_with(a) {
                    return Some((a.as_path(), b.as_path()));
                }
            }
        }
        None
    }

    /// Whether the two sets overlap at subtree-prefix granularity.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.first_overlap(other).is_some()
    }
}

impl FromIterator<Utf8PathBuf> for PlanPathSet {
    fn from_iter<T: IntoIterator<Item = Utf8PathBuf>>(iter: T) -> Self {
        let mut set = Self::new();
        for p in iter {
            set.insert(p);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> PlanPathSet {
        paths.iter().map(Utf8PathBuf::from).collect()
    }

    #[test]
    fn disjoint_sets_do_not_overlap() {
        let a = set(&["/lib/raw/m31", "/lib/masters/darks"]);
        let b = set(&["/lib/raw/m42", "/archive/2026"]);
        assert!(!a.overlaps(&b));
        assert!(!b.overlaps(&a));
    }

    #[test]
    fn identical_paths_overlap() {
        let a = set(&["/lib/raw/m31/file.fits"]);
        let b = set(&["/lib/raw/m31/file.fits"]);
        assert!(a.overlaps(&b));
    }

    #[test]
    fn ancestor_descendant_overlap_both_directions() {
        let parent = set(&["/lib/raw"]);
        let child = set(&["/lib/raw/m31/file.fits"]);
        assert!(parent.overlaps(&child), "ancestor must claim descendant");
        assert!(child.overlaps(&parent), "descendant must claim ancestor");
    }

    #[test]
    fn sibling_string_prefix_does_not_overlap() {
        // Component-wise comparison: "/a/b" is NOT a prefix of "/a/bc".
        let a = set(&["/lib/raw/m3"]);
        let b = set(&["/lib/raw/m31"]);
        assert!(!a.overlaps(&b));
        assert!(!b.overlaps(&a));
    }

    #[test]
    fn relative_and_absolute_do_not_falsely_overlap() {
        let rel = set(&["raw/m31"]);
        let abs = set(&["/raw/m31/other"]);
        assert!(!rel.overlaps(&abs));
    }

    #[test]
    fn relative_prefixes_still_compared() {
        // Legacy items without a resolvable root compare as-is.
        let a = set(&["raw/m31"]);
        let b = set(&["raw/m31/file.fits"]);
        assert!(a.overlaps(&b));
    }

    #[test]
    fn empty_paths_are_ignored() {
        let mut a = PlanPathSet::new();
        a.insert(Utf8PathBuf::from(""));
        assert!(a.is_empty());
        let b = set(&["/lib/raw"]);
        assert!(!a.overlaps(&b), "empty set claims nothing");
    }

    #[test]
    fn first_overlap_reports_the_pair() {
        let a = set(&["/x/one", "/lib/raw/m31"]);
        let b = set(&["/y/two", "/lib/raw"]);
        let (pa, pb) = a.first_overlap(&b).expect("must overlap");
        assert_eq!(pa, Utf8Path::new("/lib/raw/m31"));
        assert_eq!(pb, Utf8Path::new("/lib/raw"));
    }
}
