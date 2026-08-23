// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Boot reconciliation of filesystem-mutation intents (constitution v1.1.0 §V).
//!
//! Runs once at startup, after [`super::sweep_crashed_applying_plans`] has
//! flipped crashed `applying` plans to `paused`. For every non-terminal item of
//! those plans it resolves the source/destination paths (the same resolution
//! the executor uses), probes filesystem reality via [`fs_executor::classify`],
//! and persists the verdict:
//!
//! - **Completed** — the mutation is visible on disk; heal the record to
//!   `succeeded` (unambiguous, re-derivable — auto-heal per constitution).
//! - **NotStarted** — no effect on disk; leave the item for the user-approved
//!   resume path to re-run.
//! - **Ambiguous** — inconsistent or unresolvable; mark `failed` with a
//!   `reconcile.ambiguous` reason so the unclean-shutdown prompt surfaces it for
//!   an explicit user resume/repair decision.
//!
//! No new write-ahead journal is introduced: the intent is the existing plan
//! `applying` state + `plan_apply_runs` row, and the outcome is the existing
//! per-item `plan_items.item_state`. This pass only reads those and reconciles.

use std::collections::HashMap;

use camino::Utf8PathBuf;
use domain_core::ids::{new_id, Timestamp};
use sqlx::SqlitePool;

use super::{apply_repo, resolve_root_path};
use fs_executor::{classify, MutationShape, ReconcileVerdict};
use fs_pathsafe::contain::normalize_utf8;

/// Outcome of the boot reconciliation pass, consumed by the unclean-shutdown
/// recovery prompt (kyo7.48).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    /// Items healed to `succeeded` because the filesystem showed the mutation
    /// completed.
    pub healed: usize,
    /// Items left `pending`/`applying` for the user-approved resume path.
    pub left_for_resume: usize,
    /// Ids of plans holding at least one item that needs an explicit user
    /// resume/repair decision (ambiguous filesystem state).
    pub ambiguous_plan_ids: Vec<String>,
}

impl ReconcileReport {
    /// Whether any item requires the user's explicit resume/repair decision.
    #[must_use]
    pub fn needs_user_review(&self) -> bool {
        !self.ambiguous_plan_ids.is_empty()
    }
}

/// Map the persistence-layer action string to the executor's filesystem-effect
/// shape. `link`/`mkdir`/`write_manifest` create a destination entry; `move`/
/// `archive` relocate; `delete`/`trash` remove; `catalogue`/unknown have no
/// filesystem effect to probe.
fn shape_for(action: &str) -> MutationShape {
    match action {
        "move" | "archive" => MutationShape::Relocate,
        "delete" | "trash" => MutationShape::Remove,
        "mkdir" | "link" | "write_manifest" => MutationShape::Create,
        _ => MutationShape::None,
    }
}

/// Reconcile the intents of the just-swept crashed plans against the filesystem.
///
/// `plan_ids` is the list returned by [`super::sweep_crashed_applying_plans`].
/// Returns a [`ReconcileReport`] summarising the classification.
///
/// # Errors
///
/// Returns [`persistence_core::DbError`] on connection failure. Path-resolution
/// gaps for individual items are not fatal — they classify as ambiguous.
pub async fn reconcile_crashed_plans(
    pool: &SqlitePool,
    plan_ids: &[String],
) -> Result<ReconcileReport, persistence_core::DbError> {
    let items = apply_repo::list_unreconciled_items(pool, plan_ids).await?;
    if items.is_empty() {
        return Ok(ReconcileReport::default());
    }

    // Resolve each referenced root once. A root that no longer resolves leaves
    // its items' paths as None, which the classifier treats as ambiguous.
    let mut root_map: HashMap<String, Utf8PathBuf> = HashMap::new();
    for item in &items {
        for rid in [item.from_root_id.as_ref(), item.to_root_id.as_ref()].into_iter().flatten() {
            if !root_map.contains_key(rid) {
                if let Some(path) = resolve_root_path(pool, rid).await {
                    root_map.insert(rid.clone(), Utf8PathBuf::from(path));
                }
            }
        }
    }

    let mut report = ReconcileReport::default();
    let mut ambiguous: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for item in &items {
        let shape = shape_for(&item.action);
        let source = resolve(&root_map, item.from_root_id.as_deref(), &item.from_relative_path);
        // Archive stores its destination in `archive_path` (root-relative when a
        // from_root is set, else absolute); other relocates/creates use
        // `to_relative_path` against the destination root.
        let destination = if item.action == "archive" {
            resolve_archive(&root_map, item)
        } else {
            resolve(&root_map, item.to_root_id.as_deref(), &item.to_relative_path)
        };

        let verdict = classify(shape, source.as_deref(), destination.as_deref());
        match verdict {
            ReconcileVerdict::Completed => {
                let run_id = active_run_id(pool, &item.plan_id).await;
                apply_repo::record_reconciled_outcome(
                    pool,
                    item,
                    "pending",
                    "succeeded",
                    &run_id,
                    None,
                    &Timestamp::now_iso(),
                    &new_id(),
                )
                .await?;
                report.healed += 1;
            }
            ReconcileVerdict::NotStarted => {
                report.left_for_resume += 1;
            }
            ReconcileVerdict::Ambiguous => {
                let run_id = active_run_id(pool, &item.plan_id).await;
                apply_repo::record_reconciled_outcome(
                    pool,
                    item,
                    "pending",
                    "failed",
                    &run_id,
                    Some("filesystem state ambiguous at boot; needs user resume/repair"),
                    &Timestamp::now_iso(),
                    &new_id(),
                )
                .await?;
                ambiguous.insert(item.plan_id.clone());
            }
        }
    }

    report.ambiguous_plan_ids = ambiguous.into_iter().collect();
    Ok(report)
}

/// Resolve a root-relative path to an absolute path, mirroring the executor's
/// path resolution (`root.join(relative)` then lexical normalization). Returns
/// `None` when the root is unresolved or the relative path is empty — either
/// makes the item ambiguous.
fn resolve(
    root_map: &HashMap<String, Utf8PathBuf>,
    root_id: Option<&str>,
    relative: &str,
) -> Option<Utf8PathBuf> {
    if relative.is_empty() {
        return None;
    }
    let root = root_id.and_then(|rid| root_map.get(rid))?;
    Some(normalize_utf8(&root.join(relative)))
}

/// Resolve an archive item's destination from `archive_path`. When
/// `from_root_id` is set the stored path is root-relative; otherwise it is
/// already absolute (matching `archive_generator`'s two storage modes).
fn resolve_archive(
    root_map: &HashMap<String, Utf8PathBuf>,
    item: &apply_repo::UnreconciledItem,
) -> Option<Utf8PathBuf> {
    let archive = item.archive_path.as_deref()?;
    if archive.is_empty() {
        return None;
    }
    match item.from_root_id.as_deref().and_then(|rid| root_map.get(rid)) {
        Some(root) => Some(normalize_utf8(&root.join(archive))),
        None => Some(normalize_utf8(Utf8PathBuf::from(archive).as_path())),
    }
}

/// The crashed run id for a plan, used to link the reconciliation event. Falls
/// back to an empty string when no run row is found (the event still records
/// the transition; the run linkage is best-effort).
async fn active_run_id(pool: &SqlitePool, plan_id: &str) -> String {
    apply_repo::get_active_run(pool, plan_id).await.ok().flatten().map(|r| r.id).unwrap_or_default()
}
