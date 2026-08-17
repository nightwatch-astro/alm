// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Boot reconciliation of filesystem-mutation intents against filesystem
//! reality (constitution v1.1.0 §V, unclean-shutdown recovery).
//!
//! After an unclean shutdown, a plan item may hold a committed *intent*
//! (the plan is `applying`/`paused`) whose *outcome* was rewound under
//! `synchronous = NORMAL`. This module classifies each such item by probing
//! the filesystem for the effect the executor's operation would have left.
//! It performs **no mutation** and touches no database — the caller resolves
//! paths and persists the verdict.

use camino::Utf8Path;

/// The filesystem effect a plan item's action produces, abstracted away from
/// the persistence-layer action string so this DB-free crate need not parse it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutationShape {
    /// Source is relocated to a destination (`move`, `archive`).
    Relocate,
    /// Source is removed (`delete`, `trash`).
    Remove,
    /// A destination entry is created (`mkdir`, `link`, `write_manifest`).
    Create,
    /// No filesystem effect (`catalogue`, record-only).
    None,
}

/// The reconciliation verdict for one intent-without-confirmed-outcome item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileVerdict {
    /// The filesystem shows the mutation completed; heal the record to
    /// `succeeded` — re-derivable state, unambiguous (auto-heal permitted).
    Completed,
    /// The filesystem shows the mutation never started; leave the item for the
    /// user-approved resume path to re-run.
    NotStarted,
    /// The filesystem state is inconsistent (both endpoints present/absent) or
    /// a required path could not be resolved; the item MUST be surfaced for an
    /// explicit user resume/repair decision — never auto-healed.
    Ambiguous,
}

/// Classify a single intent by probing the filesystem.
///
/// `source` and `destination` are the resolved absolute paths the executor
/// would have operated on, or `None` when the caller could not resolve them
/// (e.g. an unregistered root) — an unresolvable required path yields
/// [`ReconcileVerdict::Ambiguous`] so custody defers to the user.
///
/// The truth tables are the inverse of each operation's post-condition:
/// a relocate leaves the source gone and the destination present; a remove
/// leaves the source gone; a create leaves the destination present.
#[must_use]
pub fn classify(
    shape: MutationShape,
    source: Option<&Utf8Path>,
    destination: Option<&Utf8Path>,
) -> ReconcileVerdict {
    match shape {
        MutationShape::None => ReconcileVerdict::Completed,
        MutationShape::Relocate => match (source, destination) {
            (Some(src), Some(dst)) => {
                match (src.exists(), dst.exists()) {
                    (false, true) => ReconcileVerdict::Completed,
                    (true, false) => ReconcileVerdict::NotStarted,
                    // Both present: a cross-volume copy that completed but whose
                    // source delete did not, or an unrelated pre-existing dst —
                    // either way the executor's own CAS gate must re-judge it.
                    // Both absent: the source vanished with no destination — lost.
                    _ => ReconcileVerdict::Ambiguous,
                }
            }
            _ => ReconcileVerdict::Ambiguous,
        },
        MutationShape::Remove => match source {
            Some(src) if !src.exists() => ReconcileVerdict::Completed,
            Some(_) => ReconcileVerdict::NotStarted,
            None => ReconcileVerdict::Ambiguous,
        },
        MutationShape::Create => match destination {
            Some(dst) if dst.exists() => ReconcileVerdict::Completed,
            Some(_) => ReconcileVerdict::NotStarted,
            None => ReconcileVerdict::Ambiguous,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn touch(dir: &std::path::Path, name: &str) -> Utf8PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        Utf8PathBuf::from_path_buf(p).unwrap()
    }

    fn absent(dir: &std::path::Path, name: &str) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(dir.join(name)).unwrap()
    }

    #[test]
    fn relocate_completed_when_src_gone_dst_present() {
        let d = tmp();
        let src = absent(d.path(), "src");
        let dst = touch(d.path(), "dst");
        assert_eq!(
            classify(MutationShape::Relocate, Some(&src), Some(&dst)),
            ReconcileVerdict::Completed
        );
    }

    #[test]
    fn relocate_not_started_when_src_present_dst_absent() {
        let d = tmp();
        let src = touch(d.path(), "src");
        let dst = absent(d.path(), "dst");
        assert_eq!(
            classify(MutationShape::Relocate, Some(&src), Some(&dst)),
            ReconcileVerdict::NotStarted
        );
    }

    #[test]
    fn relocate_ambiguous_when_both_present() {
        let d = tmp();
        let src = touch(d.path(), "src");
        let dst = touch(d.path(), "dst");
        assert_eq!(
            classify(MutationShape::Relocate, Some(&src), Some(&dst)),
            ReconcileVerdict::Ambiguous
        );
    }

    #[test]
    fn relocate_ambiguous_when_both_absent() {
        let d = tmp();
        let src = absent(d.path(), "src");
        let dst = absent(d.path(), "dst");
        assert_eq!(
            classify(MutationShape::Relocate, Some(&src), Some(&dst)),
            ReconcileVerdict::Ambiguous
        );
    }

    #[test]
    fn relocate_ambiguous_when_path_unresolvable() {
        let d = tmp();
        let src = touch(d.path(), "src");
        assert_eq!(
            classify(MutationShape::Relocate, Some(&src), None),
            ReconcileVerdict::Ambiguous
        );
    }

    #[test]
    fn remove_completed_when_src_gone() {
        let d = tmp();
        let src = absent(d.path(), "src");
        assert_eq!(classify(MutationShape::Remove, Some(&src), None), ReconcileVerdict::Completed);
    }

    #[test]
    fn remove_not_started_when_src_present() {
        let d = tmp();
        let src = touch(d.path(), "src");
        assert_eq!(classify(MutationShape::Remove, Some(&src), None), ReconcileVerdict::NotStarted);
    }

    #[test]
    fn create_completed_when_dst_present() {
        let d = tmp();
        let dst = touch(d.path(), "dst");
        assert_eq!(classify(MutationShape::Create, None, Some(&dst)), ReconcileVerdict::Completed);
    }

    #[test]
    fn create_not_started_when_dst_absent() {
        let d = tmp();
        let dst = absent(d.path(), "dst");
        assert_eq!(classify(MutationShape::Create, None, Some(&dst)), ReconcileVerdict::NotStarted);
    }

    #[test]
    fn none_shape_always_completed() {
        assert_eq!(classify(MutationShape::None, None, None), ReconcileVerdict::Completed);
    }
}
