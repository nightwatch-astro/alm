// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Archive management (US6): `send_archive_to_trash` / `permanently_delete_archive`.

use audit::bus::EventBus;
use audit::event_bus::{
    ArchivePermanentlyDeleted, ArchiveSentToTrash, Source, TOPIC_ARCHIVE_PERMANENTLY_DELETED,
    TOPIC_ARCHIVE_SENT_TO_TRASH,
};
use camino::{Utf8Path, Utf8PathBuf};
use contracts_core::plans::{ArchivePermanentlyDeleteResponse, ArchiveSendToTrashResponse};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::{new_id, Timestamp};
use fs_executor::failure::FailureCode;
use fs_executor::ops::{delete_op, trash_op};
use persistence_plans::repositories::plans as repo;
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::errors::bus_err;

use super::{build_root_map, db_err, PERMANENT_DELETE_CONFIRM_TEXT};

// ── Archive management (US6) ──────────────────────────────────────────────────

/// Resolve an archived item's on-disk absolute path.
///
/// `archive_path` is stored root-relative when the item has a `from_root_id`;
/// archive-plan generators that predate a resolved root (`archive_generator`,
/// `cleanup_generator`) store an already-absolute path with `from_root_id: None`,
/// so that shape is accepted through `resolve_unrooted`.
///
/// A `from_root_id` that does not resolve is an error rather than the no-root
/// shape. Sharing one branch meant an unresolvable root id produced a
/// cwd-relative path, which then reached `trash_file` or `delete_file`
/// (astro-plan-3v3r.5.20).
///
/// # Errors
///
/// [`fs_pathsafe::contain::ContainmentError`] when the path does not resolve
/// inside its root, or when a rootless path is not absolute and normal.
fn resolve_archive_abs_path(
    archive_path: &str,
    from_root_id: Option<&str>,
    root_map: &HashMap<String, Utf8PathBuf>,
) -> Result<Utf8PathBuf, fs_pathsafe::contain::ContainmentError> {
    let path = Utf8Path::new(archive_path);
    match from_root_id {
        Some(rid) => {
            let root = root_map.get(rid).ok_or_else(|| {
                fs_pathsafe::contain::ContainmentError::RootMissing {
                    path: path.as_std_path().to_path_buf(),
                }
            })?;
            fs_pathsafe::contain::resolve_in_root_utf8(root, path)
        }
        None => fs_pathsafe::contain::resolve_unrooted_utf8(path),
    }
}

/// Resolve one archived item, recording an uncontained path as a failure.
///
/// An uncontained path is a failed item, not a skipped one: skipping would let
/// `items_moved` report success for an archive entry nobody could account for.
fn resolve_or_record_failure(
    archive_path: &str,
    from_root_id: Option<&str>,
    root_map: &HashMap<String, Utf8PathBuf>,
    item_id: &str,
    last_failure: &mut Option<(FailureCode, String)>,
) -> Option<Utf8PathBuf> {
    match resolve_archive_abs_path(archive_path, from_root_id, root_map) {
        Ok(path) => Some(path),
        Err(e) => {
            tracing::warn!(item_id = %item_id, error = %e, "archive item path not contained");
            *last_failure = Some((FailureCode::PathInvalid, e.to_string()));
            None
        }
    }
}

/// Map a trash-primitive failure to the closed `archive.send_to_trash` error set.
fn trash_failure_error_code(code: FailureCode) -> ErrorCode {
    match code {
        FailureCode::OsTrashPermissionDenied | FailureCode::PermissionDenied => {
            ErrorCode::OsTrashPermissionDenied
        }
        _ => ErrorCode::OsTrashUnavailable,
    }
}

/// Map a delete-primitive failure to the closed `archive.permanently_delete` error set.
fn delete_failure_error_code(code: FailureCode) -> ErrorCode {
    match code {
        FailureCode::PermissionDenied | FailureCode::ProtectedSource => {
            ErrorCode::PathPermissionDenied
        }
        // Non-permission delete failures (source vanished, volume unavailable,
        // disk full, unknown) — NOT `OsTrashUnavailable`, which is trash-specific
        // and semantically wrong for a non-trash delete failure (review #1).
        _ => ErrorCode::ArchiveDeleteFailed,
    }
}

/// Send the app-managed archive subtree for a plan to the OS trash (T045).
///
/// Archive path: `<library_root>/.astro-plan-archive/<planId>/`. Sends every
/// archived item's real file to the OS trash via `fs_executor::ops::trash_op`
/// (constitution §II: prefer trash over permanent delete). An item whose
/// on-disk file is already gone (e.g. a repeated call) is a no-op, not a
/// failure. `itemsMoved` on success always reflects real trash outcomes, never
/// the DB item count (the prior stub's bug, #732).
///
/// # Errors
///
/// Returns `ContractError` with code:
/// - `plan.not_found` — no matching plan.
/// - `archive.empty` — plan has no archived items.
/// - `os_trash.unavailable` / `os_trash.permission.denied` — every item's real
///   trash attempt failed (no items were moved).
pub async fn send_archive_to_trash(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
) -> Result<ArchiveSendToTrashResponse, ContractError> {
    let row = repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;

    let items = repo::list_plan_items(pool, plan_id).await.map_err(db_err)?;
    let archive_items: Vec<&repo::PlanItemRow> =
        items.iter().filter(|i| i.archive_path.is_some()).collect();

    if archive_items.is_empty() {
        return Err(ContractError::new(
            ErrorCode::ArchiveEmpty,
            format!("plan {} has no archived items", row.id),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let root_map = build_root_map(pool, &archive_items).await;

    let mut items_moved: i64 = 0;
    let mut last_failure: Option<(FailureCode, String)> = None;
    for item in &archive_items {
        // Filtered by `archive_path.is_some()` above.
        let archive_rel = item.archive_path.as_deref().unwrap_or_default();
        let Some(abs_path) = resolve_or_record_failure(
            archive_rel,
            item.from_root_id.as_deref(),
            &root_map,
            &item.id,
            &mut last_failure,
        ) else {
            continue;
        };

        if !abs_path.exists() {
            // Already gone (e.g. a repeated call) — not a failure, no-op.
            continue;
        }

        match trash_op::trash_file(&abs_path, None) {
            Ok(_) => items_moved += 1,
            Err((failure, _)) => {
                tracing::warn!(item_id = %item.id, path = %abs_path, error = %failure, "archive item trash failed");
                last_failure = Some((failure.code, failure.message));
            }
        }
    }

    if items_moved == 0 {
        if let Some((code, message)) = last_failure {
            return Err(ContractError::new(
                trash_failure_error_code(code),
                message,
                ErrorSeverity::Blocking,
                code.is_recoverable(),
            ));
        }
    }

    let at = Timestamp::now_iso();
    let audit_id = new_id();

    // Emit audit event (T045) — real outcome, not the DB item count (#732).
    bus.publish(
        TOPIC_ARCHIVE_SENT_TO_TRASH,
        Source::User,
        ArchiveSentToTrash { plan_id: plan_id.to_owned(), items_moved, at },
    )
    .await
    .map_err(bus_err)?;

    Ok(ArchiveSendToTrashResponse { plan_id: plan_id.to_owned(), items_moved, audit_id })
}

/// Permanently delete the app-managed archive subtree for a plan (T046).
///
/// Requires `confirm_text == "DELETE"` guard. Honors spec-016 protection —
/// if `block_permanent_delete` is true in settings, this operation is blocked.
///
/// # Errors
///
/// Returns `ContractError` with code:
/// - `confirm.text.mismatch` — confirm text is not "DELETE".
/// - `plan.blocked_by_protection` — spec-016 blockPermanentDelete is enabled.
/// - `plan.not_found` — no matching plan.
/// - `archive.empty` — plan has no archived items.
/// - `path.permission_denied` / `archive.delete_failed` — every item's real
///   delete attempt failed (no items were deleted).
pub async fn permanently_delete_archive(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    confirm_text: &str,
    block_permanent_delete: bool,
) -> Result<ArchivePermanentlyDeleteResponse, ContractError> {
    // Confirm text guard.
    if confirm_text != PERMANENT_DELETE_CONFIRM_TEXT {
        return Err(ContractError::new(
            ErrorCode::ConfirmTextMismatch,
            "confirm text must be exactly \"DELETE\"".to_owned(),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Spec-016 protection guard.
    if block_permanent_delete {
        return Err(ContractError::new(
            ErrorCode::PlanBlockedByProtection,
            "permanent delete is disabled by the blockPermanentDelete setting (spec 016)"
                .to_owned(),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let row = repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;

    let items = repo::list_plan_items(pool, plan_id).await.map_err(db_err)?;
    let archive_items: Vec<&repo::PlanItemRow> =
        items.iter().filter(|i| i.archive_path.is_some()).collect();

    if archive_items.is_empty() {
        return Err(ContractError::new(
            ErrorCode::ArchiveEmpty,
            format!("plan {} has no archived items", row.id),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let root_map = build_root_map(pool, &archive_items).await;

    let mut items_deleted: i64 = 0;
    let mut last_failure: Option<(FailureCode, String)> = None;
    for item in &archive_items {
        let archive_rel = item.archive_path.as_deref().unwrap_or_default();
        let Some(abs_path) = resolve_or_record_failure(
            archive_rel,
            item.from_root_id.as_deref(),
            &root_map,
            &item.id,
            &mut last_failure,
        ) else {
            continue;
        };

        if !abs_path.exists() {
            // Already gone (e.g. a repeated call) — not a failure, no-op.
            continue;
        }

        // `confirm_required = true`: the confirm-text guard above already
        // gated entry into this function (constitution §II: permanent
        // delete is always behind explicit confirmation).
        match delete_op::delete_file(&abs_path, true) {
            Ok(()) => items_deleted += 1,
            Err((failure, _)) => {
                tracing::warn!(item_id = %item.id, path = %abs_path, error = %failure, "archive item permanent delete failed");
                last_failure = Some((failure.code, failure.message));
            }
        }
    }

    if items_deleted == 0 {
        if let Some((code, message)) = last_failure {
            return Err(ContractError::new(
                delete_failure_error_code(code),
                message,
                ErrorSeverity::Blocking,
                code.is_recoverable(),
            ));
        }
    }

    let at = Timestamp::now_iso();
    let audit_id = new_id();

    // Emit audit event (T046) — real outcome, not the DB item count (#732).
    bus.publish(
        TOPIC_ARCHIVE_PERMANENTLY_DELETED,
        Source::User,
        ArchivePermanentlyDeleted { plan_id: plan_id.to_owned(), items_deleted, at },
    )
    .await
    .map_err(bus_err)?;

    Ok(ArchivePermanentlyDeleteResponse { plan_id: plan_id.to_owned(), items_deleted, audit_id })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn root_map() -> HashMap<String, Utf8PathBuf> {
        HashMap::from([("root-1".to_owned(), Utf8PathBuf::from("/mnt/library"))])
    }

    #[test]
    fn a_root_relative_archive_path_resolves_under_its_root() {
        let path = resolve_archive_abs_path(
            ".astro-plan-archive/plan-1/x.fits",
            Some("root-1"),
            &root_map(),
        )
        .unwrap();
        assert_eq!(path, "/mnt/library/.astro-plan-archive/plan-1/x.fits");
    }

    /// astro-plan-3v3r.5.20: an unresolvable root id took the no-root branch and
    /// produced a cwd-relative path, which then reached trash_file/delete_file.
    #[test]
    fn an_unresolvable_root_id_is_refused_rather_than_treated_as_rootless() {
        let err = resolve_archive_abs_path("plan-1/x.fits", Some("missing-root"), &root_map())
            .unwrap_err();
        assert!(matches!(err, fs_pathsafe::contain::ContainmentError::RootMissing { .. }), "{err}");
    }

    #[test]
    fn a_root_relative_path_that_escapes_its_root_is_refused() {
        let err =
            resolve_archive_abs_path("../../etc/passwd", Some("root-1"), &root_map()).unwrap_err();
        assert!(matches!(err, fs_pathsafe::contain::ContainmentError::Escapes { .. }), "{err}");
    }

    /// Legacy archive/cleanup generators store an absolute path with no root id.
    #[test]
    fn a_rootless_absolute_path_still_resolves() {
        let path =
            resolve_archive_abs_path("/mnt/library/archive/x.fits", None, &root_map()).unwrap();
        assert_eq!(path, "/mnt/library/archive/x.fits");
    }

    #[test]
    fn a_rootless_relative_path_is_refused() {
        let err = resolve_archive_abs_path("archive/x.fits", None, &root_map()).unwrap_err();
        assert!(matches!(err, fs_pathsafe::contain::ContainmentError::NotAbsolute { .. }), "{err}");
    }
}
