// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure action → filesystem-op dispatch (no async, no callbacks): calls the
//! matching `ops::*` primitive on paths already resolved by the run loop.

use camino::{Utf8Path, Utf8PathBuf};

use crate::failure::{FailureCode, PlanItemFailure, RollbackOutcome};
use crate::ops::archive_op;
use crate::ops::catalogue_op;
use crate::ops::delete_op;
use crate::ops::link_op;
use crate::ops::mkdir_op;
use crate::ops::move_op;
use crate::ops::trash_op;
use crate::ops::write_manifest_op;

use super::{ExecutorItem, ExecutorItemAction};

pub(super) type OpError = (PlanItemFailure, bool, RollbackOutcome, Option<String>);

/// Both sides of an item, resolved and proven contained by
/// `run::loop_::resolve_item_paths`.
///
/// Dispatch takes these rather than the roots so there is one resolution per
/// item: a second join here disagreed with the gate about which root governs a
/// destination (astro-plan-3v3r.1.12).
#[derive(Debug, Clone, Default)]
pub(super) struct ResolvedItemPaths {
    pub(super) source: Option<Utf8PathBuf>,
    pub(super) destination: Option<Utf8PathBuf>,
    /// The archive destination carried by `Archive`/`Trash`, resolved against
    /// the same root as the source.
    ///
    /// A third destination field reached `move_file` unresolved and ungated
    /// while only the two named sides were gated (astro-plan-zboex).
    pub(super) archive_destination: Option<Utf8PathBuf>,
}

pub(super) fn execute_item(item: &ExecutorItem, paths: &ResolvedItemPaths) -> Result<(), OpError> {
    let resolved_src = paths.source.as_deref();
    let resolved_dst = paths.destination.as_deref();

    match &item.action {
        ExecutorItemAction::NoOp => Ok(()),

        ExecutorItemAction::Move => {
            let src = require_resolved_path(resolved_src, "source")?;
            let dst = require_resolved_path(resolved_dst, "destination")?;
            move_op::move_file(src, dst)
                .map_err(|(f, r)| (f, r.rollback_attempted, r.rollback_outcome, r.rollback_message))
        }

        ExecutorItemAction::Archive { .. } => {
            let src = require_resolved_path(resolved_src, "source")?;
            let dst =
                require_resolved_path(paths.archive_destination.as_deref(), "archive destination")?;
            archive_op::archive_file(src, dst)
                .map_err(|(f, r)| (f, r.rollback_attempted, r.rollback_outcome, r.rollback_message))
        }

        ExecutorItemAction::Trash { .. } => {
            let src = require_resolved_path(resolved_src, "source")?;
            trash_op::trash_file(src, paths.archive_destination.as_deref())
                .map(|_| ()) // discard TrashResult (audit_reason recorded by caller)
                .map_err(|(f, r)| (f, r.rollback_attempted, r.rollback_outcome, r.rollback_message))
        }

        ExecutorItemAction::Delete => {
            let src = require_resolved_path(resolved_src, "source")?;
            // T020: use `destructive_confirmed`, not `is_protected`.
            delete_op::delete_file(src, item.destructive_confirmed)
                .map_err(|(f, r)| (f, r.rollback_attempted, r.rollback_outcome, None))
        }

        ExecutorItemAction::Catalogue => {
            // No filesystem I/O — record-in-place (spec 041, T007).
            catalogue_op::catalogue_noop()
                .map_err(|e| (e, false, RollbackOutcome::NotApplicable, None))
        }

        ExecutorItemAction::Mkdir => {
            let dst = require_resolved_path(resolved_dst, "destination")?;
            mkdir_op::make_dir(dst).map_err(|f| (f, false, RollbackOutcome::NotApplicable, None))
        }

        ExecutorItemAction::Link { kind } => {
            let src = require_resolved_path(resolved_src, "source")?;
            let dst = require_resolved_path(resolved_dst, "destination")?;
            link_op::create_link(src, dst, *kind)
                .map_err(|f| (f, false, RollbackOutcome::NotApplicable, None))
        }

        ExecutorItemAction::WriteManifest { project_id } => {
            let dst = require_resolved_path(resolved_dst, "destination")?;
            write_manifest_op::write_marker(dst, project_id)
                .map_err(|f| (f, false, RollbackOutcome::NotApplicable, None))
        }
    }
}

fn require_resolved_path<'a>(
    p: Option<&'a Utf8Path>,
    label: &str,
) -> Result<&'a Utf8Path, OpError> {
    p.ok_or_else(|| {
        (
            PlanItemFailure::with_code(
                FailureCode::PathInvalid,
                format!("{label} path is not set on this plan item"),
            ),
            false,
            RollbackOutcome::NotApplicable,
            None,
        )
    })
}
