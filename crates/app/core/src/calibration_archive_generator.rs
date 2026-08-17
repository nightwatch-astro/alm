// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! #886 calibration master archive plan generator + archive listing.
//!
//! Per decisions.md (2026-07-18), a DB-only lifecycle flag for "archived
//! masters" was REJECTED: archiving a master MUST be a reviewable
//! filesystem-mutation plan (Constitution §II), same discipline as
//! [`crate::archive_generator`]'s whole-project archive/restore. This module
//! mirrors that shape for a single calibration master instead of a whole
//! project's artifacts, reusing the same [`crate::protection::generate_plan`]
//! tail and (for restore) `crate::archive_generator::generate_restore_generic`
//! — never duplicating that machinery.
//!
//! ## Candidate resolution
//!
//! A master's file is `calibration_session.frame_ids[0]` joined to
//! `file_record`, surfaced as `root_id`/`relative_path` on the
//! `CalibrationMaster` contract (#642,
//! `crates/persistence/db/migrations/0072_calibration_master_path.sql`).
//! Unlike [`crate::archive_generator`]'s project convention (an already-
//! absolute `processing_artifacts.path`, `from_root_id: None`), a master's
//! path is genuinely root-relative, so items here carry a real
//! `from_root_id` — the executor's path gate (escape/symlink/staleness)
//! fires on them like any other rooted plan item.
//!
//! ## In-use warn + confirm (decisions.md #886 ruling)
//!
//! A master currently assigned to one or more sessions requires an explicit
//! `confirm_in_use = true` to archive — mirrors `calibration.match.assign`'s
//! `override` gate. `generate` returns `calibration.master_in_use` when
//! the caller has not confirmed.
//!
//! ## Lifecycle closure
//!
//! Masters have no lifecycle state machine (migration 0050 dropped
//! `calibration_session.state`), so the terminal step
//! (`crate::plan_apply::finalize_calibration_master_archive`/
//! `finalize_calibration_master_restore`) is a plain flag+link set on
//! `calibration_session.archived_at`/`archived_via_plan_id` — never a
//! lifecycle transition, and never set outside that plan-apply call site.

#![allow(clippy::doc_markdown)] // domain terminology not appropriate for backticks

use camino::Utf8Path;
use contracts_core::archive::{ArchiveEntry, GenerateArchivePlanResult, GenerateRestorePlanResult};
use contracts_core::error_code::ErrorCode;
use contracts_core::{ContractError, ErrorSeverity};
use persistence_calibration::repositories::q_calibration;
use persistence_plans::repositories::plans as plans_repo;
use sqlx::SqlitePool;

use crate::archive_generator::generate_restore_generic;
use crate::cleanup_generator::DataType;
use crate::errors::db_err;
use crate::protection::{self, CleanupPlanItem, GeneratePlanRequest};
use domain_core::ids::new_id;

const ARCHIVE_ORIGIN: &str = "calibration_master_archive";
const RESTORE_ORIGIN: &str = "calibration_master_restore";

/// Take the tail of a relative path (the file name) for display.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Materialise a reviewable single-master archive plan (#886).
///
/// # Errors
///
/// Returns `ContractError` with code:
/// - `master.not_found` — no matching calibration master.
/// - `plan.invalid_state` — the master is already archived.
/// - `calibration.master_untracked` — the master has no `root_id`/
///   `relative_path` resolved to a real file (nothing to archive).
/// - `calibration.master_in_use` — the master is assigned to one or more
///   sessions and `confirm_in_use` was not set.
/// - database failure.
pub async fn generate(
    pool: &SqlitePool,
    master_id: &str,
    title: Option<&str>,
    confirm_in_use: bool,
) -> Result<GenerateArchivePlanResult, ContractError> {
    let row = q_calibration::get_calibration_master(pool, master_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            ContractError::new(
                ErrorCode::MasterNotFound,
                format!("master {master_id} not found"),
                ErrorSeverity::Blocking,
                false,
            )
        })?;

    if row.archived_at.is_some() {
        return Err(ContractError::new(
            ErrorCode::PlanInvalidState,
            format!("master {master_id} is already archived"),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let (Some(root_id), Some(relative_path)) =
        (row.root_id.clone(), row.frame_relative_path.clone())
    else {
        return Err(ContractError::new(
            ErrorCode::CalibrationMasterUntracked,
            format!("master {master_id} has no tracked file to archive"),
            ErrorSeverity::Blocking,
            false,
        ));
    };

    let used_by_sessions =
        q_calibration::list_assignment_session_ids(pool, master_id).await.map_err(db_err)?;
    if !used_by_sessions.is_empty() && !confirm_in_use {
        return Err(ContractError::new(
            ErrorCode::CalibrationMasterInUse,
            format!(
                "master {master_id} is assigned to {} session(s); re-confirm to archive anyway",
                used_by_sessions.len()
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let plan_id = new_id();
    let resolved_title = title
        .map_or_else(|| format!("Archive master: {}", file_name(&relative_path)), str::to_owned);

    let item = CleanupPlanItem {
        id: format!("{plan_id}-item-0"),
        name: file_name(&relative_path).to_owned(),
        action: "archive".to_owned(),
        // Real library root id (unlike the project generator's project-id
        // convention) — per-source protection overrides key on roots
        // (mirrors crate::cleanup_generator's per-file items).
        source_id: root_id.clone(),
        category: DataType::Master.protection_category().to_owned(),
        from_relative_path: relative_path.clone(),
        from_root_id: Some(root_id),
        to_relative_path: relative_path,
    };

    let resp = protection::generate_plan(
        pool,
        &GeneratePlanRequest {
            plan_id: plan_id.clone(),
            title: resolved_title,
            origin: ARCHIVE_ORIGIN.to_owned(),
            plan_type: ARCHIVE_ORIGIN.to_owned(),
            origin_path: Some(master_id.to_owned()),
            destructive_destination: "archive".to_owned(),
            reason: "archive".to_owned(),
            total_bytes_required: row.size_bytes.unwrap_or(0).max(0),
            items: vec![item],
        },
    )
    .await?;

    Ok(GenerateArchivePlanResult {
        plan_id: resp.plan_id,
        item_count: 1,
        protected_item_count: u32::try_from(resp.protected_item_count).unwrap_or(u32::MAX),
        empty_reason: None,
    })
}

/// Materialise a reviewable restore (un-archive) plan for a previously
/// applied master-archive plan (#886). Thin wrapper over the shared
/// `generate_restore_generic` tail (#885's project restore extracted it).
///
/// # Errors
///
/// See `generate_restore_generic`.
pub async fn generate_restore(
    pool: &SqlitePool,
    archived_plan_id: &str,
    title: Option<&str>,
) -> Result<GenerateRestorePlanResult, ContractError> {
    generate_restore_generic(pool, archived_plan_id, title, ARCHIVE_ORIGIN, RESTORE_ORIGIN).await
}

/// Resolve the archived master's app-managed archive folder as an absolute
/// on-disk path (for Reveal), joining the owning plan's item `archive_path`
/// (root-relative, unlike the project generator's already-absolute
/// convention) with its `from_root_id`'s current library root path.
async fn resolve_master_archive_folder_path(pool: &SqlitePool, plan_id: &str) -> Option<String> {
    let items = plans_repo::list_plan_items(pool, plan_id).await.ok()?;
    let item = items.iter().find(|i| i.archive_path.is_some())?;
    let archive_path = item.archive_path.as_deref()?;
    let folder_relative = Utf8Path::new(archive_path).parent()?;
    let root_id = item.from_root_id.as_deref()?;
    let root_path = crate::plan_apply::resolve_root_path(pool, root_id).await?;
    Some(Utf8Path::new(&root_path).join(folder_relative).to_string())
}

/// `archive.list`'s master rows (#886) — every currently-archived
/// calibration master, in the same [`ArchiveEntry`] shape the project rows
/// use (`entity_type = "master"`). Merged with
/// [`crate::archive_generator::list_archived`]'s project rows at the Tauri
/// command layer rather than folded into that function, keeping each
/// generator's listing scoped to the entity it owns.
///
/// # Errors
///
/// Returns `ContractError` on database failure.
pub async fn list_archived(pool: &SqlitePool) -> Result<Vec<ArchiveEntry>, ContractError> {
    let rows = q_calibration::list_archived_masters(pool).await.map_err(db_err)?;

    let mut entries = Vec::with_capacity(rows.len());
    for r in rows {
        let archive_folder_path = match r.archived_via_plan_id.as_deref() {
            Some(plan_id) => resolve_master_archive_folder_path(pool, plan_id).await,
            None => None,
        };
        entries.push(ArchiveEntry {
            id: r.id,
            name: r
                .frame_relative_path
                .as_deref()
                .map_or_else(|| format!("{} master", r.kind), |p| file_name(p).to_owned()),
            entity_type: "master".to_owned(),
            archived_at: r.archived_at,
            reason: r.plan_title,
            original_path: r.frame_relative_path.unwrap_or_default(),
            size_bytes: r.archived_bytes,
            archived_via_plan_id: r.archived_via_plan_id,
            archive_folder_path,
        });
    }

    Ok(entries)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;

    async fn setup() -> Database {
        let db = Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db
    }

    async fn seed_master(db: &Database, id: &str, kind: &str, root_id: &str, rel_path: &str) {
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
             VALUES (?, 'Library', '/data/lib', 'local', 'active', '2026-06-01T00:00:00Z')",
        )
        .bind(root_id)
        .execute(db.pool())
        .await
        .ok();

        let file_record_id = format!("fr-{id}");
        sqlx::query(
            "INSERT INTO file_record \
             (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES (?, ?, ?, 1000, '2026-06-01T00:00:00Z', 'observed', \
                     '2026-06-01T00:00:00Z', '2026-06-01T00:00:00Z')",
        )
        .bind(&file_record_id)
        .bind(root_id)
        .bind(rel_path)
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO calibration_session (id, session_key, kind, frame_ids, root_id, created_at) \
             VALUES (?, ?, ?, ?, ?, '2026-06-01T00:00:00Z')",
        )
        .bind(id)
        .bind(format!("{kind}-key"))
        .bind(kind)
        .bind(format!("[\"{file_record_id}\"]"))
        .bind(root_id)
        .execute(db.pool())
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn generate_builds_a_single_item_archive_plan() {
        let db = setup().await;
        seed_master(&db, "m1", "dark", "root-1", "masters/masterDark_300s.xisf").await;

        let resp = generate(db.pool(), "m1", None, false).await.unwrap();
        assert_eq!(resp.item_count, 1);

        let plan = plans_repo::get_plan(db.pool(), &resp.plan_id, false).await.unwrap();
        assert_eq!(plan.origin, "calibration_master_archive");
        assert_eq!(plan.plan_type, "calibration_master_archive");
        assert_eq!(plan.origin_path.as_deref(), Some("m1"));
        assert_eq!(plan.state, "ready_for_review");

        let items = plans_repo::list_plan_items(db.pool(), &resp.plan_id).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].action, "archive");
        assert_eq!(items[0].from_root_id.as_deref(), Some("root-1"));
        assert_eq!(items[0].from_relative_path, "masters/masterDark_300s.xisf");
        assert_ne!(items[0].to_relative_path, items[0].from_relative_path);
    }

    #[tokio::test]
    async fn generate_refuses_untracked_master() {
        let db = setup().await;
        sqlx::query(
            "INSERT INTO calibration_session (id, session_key, kind, created_at) \
             VALUES ('m-untracked', 'k', 'bias', '2026-06-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let err = generate(db.pool(), "m-untracked", None, false).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::CalibrationMasterUntracked);
    }

    #[tokio::test]
    async fn generate_refuses_unknown_master() {
        let db = setup().await;
        let err = generate(db.pool(), "nonexistent", None, false).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MasterNotFound);
    }

    #[tokio::test]
    async fn generate_warns_on_in_use_master_without_confirm() {
        let db = setup().await;
        seed_master(&db, "m2", "flat", "root-1", "masters/masterFlat.xisf").await;
        sqlx::query(
            "INSERT INTO calibration_assignment \
             (id, session_id, calibration_type, master_id, confidence, was_override, \
              mismatched_dimensions, assigned_at) \
             VALUES ('ca1', 'sess-1', 'flat', 'm2', 1.0, 0, '[]', '2026-06-02T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let err = generate(db.pool(), "m2", None, false).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::CalibrationMasterInUse);

        // Confirmed: proceeds.
        let resp = generate(db.pool(), "m2", None, true).await.unwrap();
        assert_eq!(resp.item_count, 1);
    }

    #[tokio::test]
    async fn generate_refuses_an_already_archived_master() {
        let db = setup().await;
        seed_master(&db, "m3", "dark", "root-1", "masters/masterDark_60s.xisf").await;
        let resp = generate(db.pool(), "m3", None, false).await.unwrap();
        plans_repo::update_plan_state(db.pool(), &resp.plan_id, "applied").await.unwrap();
        q_calibration::set_master_archived(db.pool(), "m3", &resp.plan_id, "2026-06-03T00:00:00Z")
            .await
            .unwrap();

        let err = generate(db.pool(), "m3", None, false).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::PlanInvalidState);
    }

    #[tokio::test]
    async fn generate_restore_reverses_an_applied_master_archive_plan() {
        let db = setup().await;
        seed_master(&db, "m4", "bias", "root-1", "masters/masterBias.xisf").await;
        let archive_resp = generate(db.pool(), "m4", None, false).await.unwrap();
        plans_repo::update_plan_state(db.pool(), &archive_resp.plan_id, "applied").await.unwrap();

        let restore_resp = generate_restore(db.pool(), &archive_resp.plan_id, None).await.unwrap();
        assert_eq!(restore_resp.item_count, 1);

        let plan = plans_repo::get_plan(db.pool(), &restore_resp.plan_id, false).await.unwrap();
        assert_eq!(plan.origin, "calibration_master_restore");
        assert_eq!(plan.origin_path.as_deref(), Some("m4"));
    }

    #[tokio::test]
    async fn generate_restore_refuses_a_non_applied_master_archive_plan() {
        let db = setup().await;
        seed_master(&db, "m5", "dark", "root-1", "masters/masterDark_120s.xisf").await;
        let archive_resp = generate(db.pool(), "m5", None, false).await.unwrap();

        let err = generate_restore(db.pool(), &archive_resp.plan_id, None).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::PlanInvalidState);
    }

    #[tokio::test]
    async fn list_archived_returns_only_archived_masters() {
        let db = setup().await;
        seed_master(&db, "m6", "dark", "root-1", "masters/masterDark_active.xisf").await;
        seed_master(&db, "m7", "dark", "root-1", "masters/masterDark_arch.xisf").await;
        let resp = generate(db.pool(), "m7", None, false).await.unwrap();
        plans_repo::update_plan_state(db.pool(), &resp.plan_id, "applied").await.unwrap();
        q_calibration::set_master_archived(db.pool(), "m7", &resp.plan_id, "2026-06-04T00:00:00Z")
            .await
            .unwrap();

        let entries = list_archived(db.pool()).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "m7");
        assert_eq!(entries[0].entity_type, "master");
        assert_eq!(entries[0].archived_via_plan_id.as_deref(), Some(resp.plan_id.as_str()));
        assert_eq!(entries[0].original_path, "masters/masterDark_arch.xisf");
    }

    #[tokio::test]
    async fn list_archived_empty_when_none_archived() {
        let db = setup().await;
        seed_master(&db, "m8", "dark", "root-1", "masters/masterDark.xisf").await;
        let entries = list_archived(db.pool()).await.unwrap();
        assert!(entries.is_empty());
    }
}
