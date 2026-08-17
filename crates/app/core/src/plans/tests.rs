// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use audit::EventBus;
use contracts_core::plans::{PlanListRequest, RetryItemsFilter};
use persistence_core::Database;
use persistence_plans::repositories::plans as repo;

async fn setup() -> (Database, EventBus) {
    let db = Database::in_memory().await.expect("in-memory DB");
    db.migrate().await.expect("migrations");
    let bus = EventBus::with_pool(db.pool().clone());
    (db, bus)
}

async fn insert_draft(db: &Database, id: &str) {
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id,
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();
}

async fn add_item(db: &Database, plan_id: &str, item_id: &str, action: &str) {
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: item_id,
            plan_id,
            item_index: 1,
            name: "file.fits",
            action,
            from_root_id: None,
            from_relative_path: "raw/file.fits",
            to_root_id: None,
            to_relative_path: "archive/file.fits",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: Some(".astro-plan-archive/p1/file.fits"),
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();
}

/// Like `add_item`, but `archive_path` points at a real, caller-supplied
/// absolute path (`from_root_id: None`, matching every real archive
/// generator — `resolve_archive_abs_path` then uses it as-is), so
/// `send_archive_to_trash`/`permanently_delete_archive` real-fs tests can
/// exercise an on-disk file.
async fn add_item_with_real_archive_path(
    db: &Database,
    plan_id: &str,
    item_id: &str,
    archive_abs_path: &str,
) {
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: item_id,
            plan_id,
            item_index: 1,
            name: "file.fits",
            action: "archive",
            from_root_id: None,
            from_relative_path: "raw/file.fits",
            to_root_id: None,
            to_relative_path: archive_abs_path,
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: Some(archive_abs_path),
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();
}

// ── list_plans ────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_plans_returns_non_discarded() {
    let (db, _bus) = setup().await;
    insert_draft(&db, "p1").await;
    insert_draft(&db, "p2").await;
    repo::soft_delete_plan(db.pool(), "p2", "2026-06-01T00:00:00Z").await.unwrap();

    let resp = list_plans(
        db.pool(),
        &PlanListRequest {
            created_after: Some("1970-01-01T00:00:00Z".to_owned()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(resp.plans.len(), 1);
    assert_eq!(resp.plans[0].id, "p1");
}

#[tokio::test]
async fn list_plans_failed_first_ordering() {
    let (db, _bus) = setup().await;
    insert_draft(&db, "p-draft").await;
    insert_draft(&db, "p-failed").await;
    repo::update_plan_state(db.pool(), "p-failed", "failed").await.unwrap();

    let resp = list_plans(
        db.pool(),
        &PlanListRequest {
            state_filter: Some(vec!["draft".to_owned(), "failed".to_owned()]),
            created_after: Some("1970-01-01T00:00:00Z".to_owned()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(resp.plans[0].id, "p-failed", "failed plan should be first");
}

// ── get_plan ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_plan_returns_not_found_for_missing() {
    let (db, _bus) = setup().await;
    let err = get_plan(db.pool(), "does-not-exist").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanNotFound);
}

#[tokio::test]
async fn get_plan_returns_items() {
    let (db, _bus) = setup().await;
    insert_draft(&db, "p1").await;
    add_item(&db, "p1", "item-1", "move").await;

    let detail = get_plan(db.pool(), "p1").await.unwrap();
    assert_eq!(detail.id, "p1");
    assert_eq!(detail.items.len(), 1);
    assert_eq!(detail.items[0].name, "file.fits");
}

// ── parse_plan_state (audit T1-b) ────────────────────────────────────────

#[test]
fn parse_plan_state_accepts_every_stored_snake_case_value() {
    for (raw, expected) in [
        ("draft", PlanState::Draft),
        ("ready_for_review", PlanState::ReadyForReview),
        ("approved", PlanState::Approved),
        ("applying", PlanState::Applying),
        ("paused", PlanState::Paused),
        ("applied", PlanState::Applied),
        ("partially_applied", PlanState::PartiallyApplied),
        ("failed", PlanState::Failed),
        ("cancelled", PlanState::Cancelled),
        ("discarded", PlanState::Discarded),
    ] {
        assert_eq!(parse_plan_state(raw).unwrap(), expected, "for {raw:?}");
    }
}

#[test]
fn parse_plan_state_errors_on_unknown_value_instead_of_defaulting() {
    // Previously silently coerced to `PlanState::Draft` (T1-b bug). A
    // `plans.state` CHECK constraint (migration) additionally blocks a
    // corrupt value from ever being persisted via SQL, so the direct
    // parser-level regression below is the reachable case; `parse_plan_state`
    // is still the load-bearing guard against pre-constraint or
    // out-of-band-corrupted rows.
    let err = parse_plan_state("bogus_corrupt_state").unwrap_err();
    assert_eq!(err.code, ErrorCode::InternalData);
}

// ── approve_plan ──────────────────────────────────────────────────────────

#[tokio::test]
async fn approve_plan_rejects_wrong_state() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;
    add_item(&db, "p1", "item-1", "move").await;

    let err = approve_plan(db.pool(), &bus, "p1", "tester").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanInvalidState);
}

#[tokio::test]
async fn approve_plan_rejects_empty_plan() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;
    repo::update_plan_state(db.pool(), "p1", "ready_for_review").await.unwrap();

    let err = approve_plan(db.pool(), &bus, "p1", "tester").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanItemsEmpty);
}

#[tokio::test]
async fn approve_plan_happy_path() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;
    add_item(&db, "p1", "item-1", "move").await;
    repo::update_plan_state(db.pool(), "p1", "ready_for_review").await.unwrap();

    let resp = approve_plan(db.pool(), &bus, "p1", "tester").await.unwrap();
    assert_eq!(resp.plan_id, "p1");
    assert_eq!(resp.new_state, "approved");
    assert!(!resp.approval_token.is_empty());

    let row = repo::get_plan(db.pool(), "p1", false).await.unwrap();
    assert_eq!(row.state, "approved");
}

/// #829: `approve_plan` must snapshot per-item FS metadata
/// (`approved_mtime`/`approved_size_bytes`) for a real source file, so
/// `check_cas` at apply time has a baseline instead of silently skipping.
#[tokio::test]
async fn n829_approve_plan_snapshots_item_fs_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("m51.fits");
    std::fs::write(&file_path, b"fits-data").unwrap();
    let abs = file_path.to_str().unwrap();

    let (db, bus) = setup().await;
    insert_draft(&db, "p-fs-snap").await;
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "p-fs-snap-item-0",
            plan_id: "p-fs-snap",
            item_index: 1,
            name: "m51.fits",
            action: "move",
            // No from_root_id: from_relative_path is used as-is, mirroring
            // item_row_to_executor_item's "legacy" no-root resolution.
            from_root_id: None,
            from_relative_path: abs,
            to_root_id: None,
            to_relative_path: "archive/m51.fits",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();
    repo::update_plan_state(db.pool(), "p-fs-snap", "ready_for_review").await.unwrap();

    approve_plan(db.pool(), &bus, "p-fs-snap", "tester").await.unwrap();

    let items = repo::list_plan_items(db.pool(), "p-fs-snap").await.unwrap();
    let item = items.iter().find(|i| i.id == "p-fs-snap-item-0").unwrap();
    assert!(item.approved_mtime.is_some(), "approved_mtime must be stamped");
    assert_eq!(
        item.approved_size_bytes,
        Some(9),
        "approved_size_bytes must match the real file size"
    );
}

/// #829: an item whose source cannot be stat'd (destination-only, e.g.
/// `mkdir`, or already-gone) must not fail approval — the snapshot stays
/// `NULL` (permissive `check_cas` fallback), not an approval error.
#[tokio::test]
async fn n829_approve_plan_tolerates_unstattable_source() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p-fs-snap-missing").await;
    add_item(&db, "p-fs-snap-missing", "item-1", "move").await; // relative, non-existent path
    repo::update_plan_state(db.pool(), "p-fs-snap-missing", "ready_for_review").await.unwrap();

    let resp = approve_plan(db.pool(), &bus, "p-fs-snap-missing", "tester").await.unwrap();
    assert_eq!(resp.new_state, "approved");

    let items = repo::list_plan_items(db.pool(), "p-fs-snap-missing").await.unwrap();
    assert_eq!(items[0].approved_mtime, None);
    assert_eq!(items[0].approved_size_bytes, None);
}

// ── discard_plan ──────────────────────────────────────────────────────────

#[tokio::test]
async fn discard_plan_happy_path() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;

    let resp = discard_plan(db.pool(), &bus, "p1").await.unwrap();
    assert_eq!(resp.plan_id, "p1");
    assert!(!resp.discarded_at.is_empty());

    let err = get_plan(db.pool(), "p1").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanNotFound);
}

#[tokio::test]
async fn discard_plan_rejects_applying() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;
    repo::update_plan_state(db.pool(), "p1", "applying").await.unwrap();

    let err = discard_plan(db.pool(), &bus, "p1").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanInProgress);
}

#[tokio::test]
async fn discard_plan_idempotent_already_discarded() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;
    discard_plan(db.pool(), &bus, "p1").await.unwrap();

    // Second call should return the existing discarded_at without error.
    let resp = discard_plan(db.pool(), &bus, "p1").await.unwrap();
    assert_eq!(resp.plan_id, "p1");
}

// ── retry_plan ────────────────────────────────────────────────────────────

#[tokio::test]
async fn retry_plan_requires_terminal_parent() {
    let (db, bus) = setup().await;
    insert_draft(&db, "parent").await;

    let err = retry_plan(db.pool(), &bus, "parent", RetryItemsFilter::Failed).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ParentNotTerminal);
}

#[tokio::test]
async fn retry_plan_no_items_to_retry() {
    let (db, bus) = setup().await;
    insert_draft(&db, "parent").await;
    add_item(&db, "parent", "item-1", "move").await;
    repo::update_plan_state(db.pool(), "parent", "failed").await.unwrap();
    // item is still in "pending" state (not failed).

    let err = retry_plan(db.pool(), &bus, "parent", RetryItemsFilter::Failed).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::NoItemsToRetry);
}

#[tokio::test]
async fn retry_plan_all_filter_creates_new_plan() {
    let (db, bus) = setup().await;
    insert_draft(&db, "parent").await;
    add_item(&db, "parent", "item-1", "move").await;
    repo::update_plan_state(db.pool(), "parent", "failed").await.unwrap();

    let resp = retry_plan(db.pool(), &bus, "parent", RetryItemsFilter::All).await.unwrap();
    assert_eq!(resp.parent_plan_id, "parent");
    assert_eq!(resp.items_total, 1);

    // Parent is not mutated.
    let parent_row = repo::get_plan(db.pool(), "parent", false).await.unwrap();
    assert_eq!(parent_row.state, "failed");

    // New plan has parent_plan_id set.
    let new_row = repo::get_plan(db.pool(), &resp.new_plan_id, false).await.unwrap();
    assert_eq!(new_row.parent_plan_id, Some("parent".to_owned()));
    assert_eq!(new_row.state, "draft");
    assert_eq!(new_row.items_total, 1);
}

// ── permanently_delete_archive ────────────────────────────────────────────

#[tokio::test]
async fn permanently_delete_requires_delete_confirm_text() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;

    let err = permanently_delete_archive(db.pool(), &bus, "p1", "wrong", false).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ConfirmTextMismatch);
}

#[tokio::test]
async fn permanently_delete_blocked_by_spec016_protection() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;
    add_item(&db, "p1", "item-1", "move").await;

    let err = permanently_delete_archive(db.pool(), &bus, "p1", "DELETE", true).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanBlockedByProtection);
}

/// #732: `permanently_delete_archive` must actually remove the on-disk
/// archived file, not just record an audit event over an untouched
/// filesystem. Deterministic (unlike OS trash): `delete_op::delete_file`
/// is a direct `std::fs::remove_file`.
#[tokio::test]
async fn permanently_delete_archive_removes_real_file() {
    let (db, bus) = setup().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("archived.fits");
    std::fs::write(&file, b"data").unwrap();
    let abs_path = file.to_str().unwrap();

    insert_draft(&db, "p1").await;
    add_item_with_real_archive_path(&db, "p1", "item-1", abs_path).await;

    let resp = permanently_delete_archive(db.pool(), &bus, "p1", "DELETE", false).await.unwrap();
    assert_eq!(resp.items_deleted, 1);
    assert!(!file.exists(), "the real archived file must be gone from disk");
}

/// A repeated call (file already deleted) is an idempotent no-op, not a
/// failure — the item's archive_path row survives a first successful
/// call, so a second click must not error.
#[tokio::test]
async fn permanently_delete_archive_is_idempotent_when_already_gone() {
    let (db, bus) = setup().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("already_gone.fits");
    // Never created on disk.
    let abs_path = file.to_str().unwrap();

    insert_draft(&db, "p1").await;
    add_item_with_real_archive_path(&db, "p1", "item-1", abs_path).await;

    let resp = permanently_delete_archive(db.pool(), &bus, "p1", "DELETE", false).await.unwrap();
    assert_eq!(resp.items_deleted, 0);
}

// ── send_archive_to_trash ─────────────────────────────────────────────────

#[tokio::test]
async fn send_archive_to_trash_rejects_empty_archive() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p1").await;
    // Item with no `archive_path` set (`add_item` always sets one; build
    // this row directly so the archive-empty precondition is real).
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "item-1",
            plan_id: "p1",
            item_index: 1,
            name: "file.fits",
            action: "move",
            from_root_id: None,
            from_relative_path: "raw/file.fits",
            to_root_id: None,
            to_relative_path: "moved/file.fits",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();

    let err = send_archive_to_trash(db.pool(), &bus, "p1").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ArchiveEmpty);
}

/// #732: exercises the real `fs_executor::ops::trash_op` primitive.
/// OS trash availability is environment-dependent (CI sandboxes may lack
/// XDG trash) — mirrors `trash_op`'s own test precedent
/// (`crates/fs/executor/src/ops/trash_op.rs`): assert on the contract
/// invariant (no silent success without either the file being gone or a
/// real, typed trash error) rather than a single hard-coded outcome.
#[tokio::test]
async fn send_archive_to_trash_moves_real_file_or_reports_real_failure() {
    let (db, bus) = setup().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("archived.fits");
    std::fs::write(&file, b"data").unwrap();
    let abs_path = file.to_str().unwrap();

    insert_draft(&db, "p1").await;
    add_item_with_real_archive_path(&db, "p1", "item-1", abs_path).await;

    match send_archive_to_trash(db.pool(), &bus, "p1").await {
        Ok(resp) => {
            assert_eq!(resp.items_moved, 1);
            assert!(!file.exists(), "trashed file must be gone from its original path");
        }
        Err(err) => {
            assert!(
                matches!(
                    err.code,
                    ErrorCode::OsTrashUnavailable | ErrorCode::OsTrashPermissionDenied
                ),
                "unexpected error code: {:?}",
                err.code
            );
            // No silent loss: the file must survive a genuinely failed trash.
            assert!(file.exists());
        }
    }
}

/// Review #2: `cleanup_generator::generate_raw_frame_plan` sets
/// `from_root_id: Some(row.root_id)` with a root-*relative* `archive_path`
/// (`protection::compute_archive_destination`) — the `Some(root) =>
/// root.join(archive_path)` branch of `resolve_archive_abs_path` is a real
/// production path, not just the `from_root_id: None`/absolute-path shape
/// every other test above exercises. Registers a real source (mirrors
/// `plan_apply::resolve_root_path_reflects_remap_not_stale_cache`) so
/// `resolve_root_path`'s `registered_sources` fallback resolves it, then
/// exercises BOTH commands against real files anchored under that root.
#[tokio::test]
async fn archive_commands_resolve_root_relative_archive_path_via_registered_root() {
    use contracts_core::first_run::{
        OrganizationState, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let (db, bus) = setup().await;
    let root_dir = tempfile::tempdir().unwrap();

    let reg = crate::first_run::register_source(
        db.pool(),
        &bus,
        &RegisterSourceRequest {
            kind: SourceKind::Project,
            path: root_dir.path().to_str().unwrap().to_owned(),
            kind_subtype: None,
            scan_depth: ScanDepth::Recursive,
            organization_state: OrganizationState::Organized,
        },
    )
    .await
    .unwrap();
    let root_id = reg.source_id;

    // Plan A: send_archive_to_trash, root-relative archive_path.
    let trash_rel = "raw/.astro-plan-archive/plan-a/item-1-file.fits";
    std::fs::create_dir_all(root_dir.path().join("raw/.astro-plan-archive/plan-a")).unwrap();
    let trash_abs = root_dir.path().join(trash_rel);
    std::fs::write(&trash_abs, b"data").unwrap();

    insert_draft(&db, "plan-a").await;
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "item-a-1",
            plan_id: "plan-a",
            item_index: 1,
            name: "file.fits",
            action: "archive",
            from_root_id: Some(&root_id),
            from_relative_path: "raw/file.fits",
            to_root_id: None,
            to_relative_path: "",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: Some(trash_rel),
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();

    match send_archive_to_trash(db.pool(), &bus, "plan-a").await {
        Ok(resp) => {
            assert_eq!(resp.items_moved, 1);
            assert!(
                !trash_abs.exists(),
                "root-relative archive_path must resolve to a real, trashable file"
            );
        }
        Err(err) => {
            // Environment-dependent (no OS trash in CI sandbox, review #732
            // precedent) — the root MUST still have resolved (a failure to
            // resolve the root at all would surface as archive.empty, not
            // a trash-specific code).
            assert!(matches!(
                err.code,
                ErrorCode::OsTrashUnavailable | ErrorCode::OsTrashPermissionDenied
            ));
            assert!(trash_abs.exists());
        }
    }

    // Plan B: permanently_delete_archive, root-relative archive_path.
    // Deterministic (std::fs::remove_file, no OS-trash dependency).
    let delete_rel = "raw/.astro-plan-archive/plan-b/item-1-file.fits";
    std::fs::create_dir_all(root_dir.path().join("raw/.astro-plan-archive/plan-b")).unwrap();
    let delete_abs = root_dir.path().join(delete_rel);
    std::fs::write(&delete_abs, b"data").unwrap();

    insert_draft(&db, "plan-b").await;
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "item-b-1",
            plan_id: "plan-b",
            item_index: 1,
            name: "file.fits",
            action: "archive",
            from_root_id: Some(&root_id),
            from_relative_path: "raw/file.fits",
            to_root_id: None,
            to_relative_path: "",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: Some(delete_rel),
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();

    let resp =
        permanently_delete_archive(db.pool(), &bus, "plan-b", "DELETE", false).await.unwrap();
    assert_eq!(resp.items_deleted, 1);
    assert!(
        !delete_abs.exists(),
        "root-relative archive_path must resolve to a real, deletable file"
    );
}

// ── mkdir-only auto-apply predicate (user decision 2026-07-04) ────────────

#[test]
fn predicate_accepts_mkdir_only_plan() {
    assert!(plan_qualifies_for_mkdir_auto_apply(["mkdir", "mkdir", "mkdir"]));
}

#[test]
fn predicate_accepts_scaffolding_shape_mkdir_plus_write_manifest() {
    // The project scaffolding plan: N mkdir folders + 1 app-owned marker.
    assert!(plan_qualifies_for_mkdir_auto_apply(["mkdir", "mkdir", "write_manifest"]));
}

#[test]
fn predicate_rejects_single_user_file_action_among_mkdirs() {
    for user_action in ["move", "copy", "link", "delete", "archive", "trash", "catalogue"] {
        assert!(
            !plan_qualifies_for_mkdir_auto_apply(["mkdir", user_action, "mkdir"]),
            "one '{user_action}' action must disable auto-apply"
        );
    }
}

#[test]
fn predicate_rejects_unknown_actions() {
    assert!(!plan_qualifies_for_mkdir_auto_apply(["mkdir", "junction"]));
    assert!(!plan_qualifies_for_mkdir_auto_apply(["frobnicate"]));
}

#[test]
fn predicate_rejects_empty_plan() {
    assert!(!plan_qualifies_for_mkdir_auto_apply([]));
}

#[test]
fn predicate_rejects_write_manifest_only_plan() {
    // No directory creation → nothing to auto-apply; keep review flow.
    assert!(!plan_qualifies_for_mkdir_auto_apply(["write_manifest"]));
}

// ── mkdir-only auto-apply use-case ─────────────────────────────────────────

/// Insert a `ready_for_review` plan with the given item actions and
/// per-item destination paths.
///
/// `write_manifest` items are linked to a fixed test project id, mirroring
/// `project_setup::create` (astro-plan-l3y0: the executor refuses a
/// `write_manifest` item with no linked project).
async fn insert_review_plan(db: &Database, id: &str, actions: &[(&str, &str)]) {
    insert_draft(db, id).await;
    for (idx, (action, dest)) in actions.iter().enumerate() {
        let linked_entity = (*action == "write_manifest").then_some("test-project");
        repo::insert_plan_item(
            db.pool(),
            &repo::InsertPlanItem {
                id: &format!("{id}-item-{idx}"),
                plan_id: id,
                item_index: i64::try_from(idx).unwrap(),
                name: "entry",
                action,
                from_root_id: None,
                from_relative_path: "",
                to_root_id: None,
                to_relative_path: dest,
                reason: "test",
                protection: "normal",
                linked_entity,
                provenance_json: None,
                archive_path: None,
                source_id: None,
                category: None,
            },
        )
        .await
        .unwrap();
    }
    repo::update_plan_state(db.pool(), id, "ready_for_review").await.unwrap();
}

/// Poll until the plan reaches a terminal state (bounded).
async fn wait_terminal(db: &Database, plan_id: &str) -> String {
    for _ in 0..200 {
        let row = repo::get_plan(db.pool(), plan_id, false).await.unwrap();
        match row.state.as_str() {
            "applied" | "partially_applied" | "failed" | "cancelled" | "paused" | "stale" => {
                return row.state;
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    }
    panic!("plan {plan_id} never reached a terminal state");
}

/// A qualifying mkdir-only plan is approved + applied and the directories
/// really exist on disk afterwards (same executor as manual apply).
#[tokio::test]
async fn auto_apply_creates_directories_for_mkdir_only_plan() {
    let (db, bus) = setup().await;
    let root = tempfile::tempdir().unwrap();
    let base = root.path().to_str().unwrap().to_owned();

    insert_review_plan(
        &db,
        "p-auto",
        &[
            ("mkdir", &format!("{base}/proj/lights")),
            ("mkdir", &format!("{base}/proj/darks")),
            ("write_manifest", &format!("{base}/proj/.marker.json")),
        ],
    )
    .await;

    let resp = auto_apply_mkdir_only_plan(db.pool(), &bus, "p-auto").await.unwrap();
    assert!(resp.is_some(), "qualifying plan must be auto-applied");

    let terminal = wait_terminal(&db, "p-auto").await;
    assert_eq!(terminal, "applied");
    assert!(std::path::Path::new(&format!("{base}/proj/lights")).is_dir());
    assert!(std::path::Path::new(&format!("{base}/proj/darks")).is_dir());
    // astro-plan-l3y0: the write_manifest item must actually write the
    // marker file, not just report success.
    assert!(std::path::Path::new(&format!("{base}/proj/.marker.json")).is_file());
}

/// A plan containing a user-file action is left untouched in
/// `ready_for_review` (normal review flow).
#[tokio::test]
async fn auto_apply_skips_plan_with_user_file_action() {
    let (db, bus) = setup().await;
    insert_review_plan(&db, "p-mixed", &[("mkdir", "/tmp/x"), ("move", "/tmp/y")]).await;

    let resp = auto_apply_mkdir_only_plan(db.pool(), &bus, "p-mixed").await.unwrap();
    assert!(resp.is_none(), "non-qualifying plan must not be auto-applied");

    let row = repo::get_plan(db.pool(), "p-mixed", false).await.unwrap();
    assert_eq!(row.state, "ready_for_review", "plan must remain reviewable");
    assert!(row.approval_token.is_none(), "no approval may be recorded");
}

/// A failed auto-apply surfaces like a failed manual apply: the plan ends
/// in a terminal failure state and remains visible/reviewable.
#[tokio::test]
async fn auto_apply_failure_leaves_plan_reviewable() {
    let (db, bus) = setup().await;
    let root = tempfile::tempdir().unwrap();
    let base = root.path().to_str().unwrap().to_owned();
    // Destination exists as a FILE → mkdir fails with
    // conflict.destination_exists (never overwrite silently).
    let blocker = format!("{base}/blocked");
    std::fs::write(&blocker, b"file in the way").unwrap();

    insert_review_plan(&db, "p-fail", &[("mkdir", &blocker)]).await;

    let resp = auto_apply_mkdir_only_plan(db.pool(), &bus, "p-fail").await.unwrap();
    assert!(resp.is_some(), "the apply run must start");

    let terminal = wait_terminal(&db, "p-fail").await;
    assert_eq!(terminal, "failed", "failed apply must land in the failed state");
    // The blocking file was not overwritten.
    assert_eq!(std::fs::read(&blocker).unwrap(), b"file in the way");
}

// ── reopen_plan (T023) ────────────────────────────────────────────────────────

async fn approved_plan(db: &Database, bus: &EventBus, id: &str) -> String {
    insert_draft(db, id).await;
    add_item(db, id, &format!("{id}-item-1"), "move").await;
    repo::update_plan_state(db.pool(), id, "ready_for_review").await.unwrap();
    approve_plan(db.pool(), bus, id, "tester").await.unwrap().approval_token
}

/// T023: `approved → draft` clears the state and the stored approval.
#[tokio::test]
async fn reopen_plan_returns_approved_plan_to_draft() {
    let (db, bus) = setup().await;
    approved_plan(&db, &bus, "p-reopen").await;

    let resp = reopen_plan(db.pool(), &bus, "p-reopen", "tester").await.unwrap();
    assert_eq!(resp.new_state, "draft");
    assert_eq!(resp.prior_state, "approved");

    let row = repo::get_plan(db.pool(), "p-reopen", false).await.unwrap();
    assert_eq!(row.state, "draft");
    assert_eq!(row.approval_token, None, "the approval token must not survive a reopen");
    assert_eq!(row.approved_at, None);
}

/// T023 mutation guard: the token issued before the reopen must not authorise
/// an apply. `apply_plan` rejects a non-`approved` state before it reads the
/// token, so the plan is driven back to `approved` by a bare state write — the
/// one path that does not re-mint — leaving the cleared stored token as the only
/// thing between the stale token and a filesystem mutation.
#[tokio::test]
async fn reopen_plan_invalidates_the_approval_token_for_apply() {
    let (db, bus) = setup().await;
    let stale_token = approved_plan(&db, &bus, "p-stale").await;

    reopen_plan(db.pool(), &bus, "p-stale", "tester").await.unwrap();
    repo::update_plan_state(db.pool(), "p-stale", "approved").await.unwrap();

    let err = crate::plan_apply::apply_plan(db.pool(), &bus, "p-stale", &stale_token, None)
        .await
        .expect_err("the pre-reopen token must be refused");
    assert_eq!(err.code, ErrorCode::PlanApprovalStale);
}

/// T023: reopening records the reversed user decision (constitution V Tier 1).
#[tokio::test]
async fn reopen_plan_emits_an_audit_event() {
    let (db, bus) = setup().await;
    approved_plan(&db, &bus, "p-audit").await;

    reopen_plan(db.pool(), &bus, "p-audit", "tester").await.unwrap();

    let events = persistence_lifecycle::repositories::events::list_since_by_topic(
        db.pool(),
        0,
        audit::event_bus::TOPIC_PLAN_REOPENED,
    )
    .await
    .unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0].payload.contains("p-audit"));
    assert!(events[0].payload.contains("approved"), "prior state is recorded");
}

/// T023 / constitution II: an applied plan's filesystem actions have happened,
/// so the approval cannot be withdrawn.
#[tokio::test]
async fn reopen_plan_refuses_an_applied_plan() {
    let (db, bus) = setup().await;
    approved_plan(&db, &bus, "p-applied").await;
    repo::update_plan_state(db.pool(), "p-applied", "applying").await.unwrap();
    repo::update_plan_state(db.pool(), "p-applied", "applied").await.unwrap();

    let err = reopen_plan(db.pool(), &bus, "p-applied", "tester")
        .await
        .expect_err("an applied plan is not reopenable");
    assert_eq!(err.code, ErrorCode::PlanInvalidState);

    let row = repo::get_plan(db.pool(), "p-applied", false).await.unwrap();
    assert_eq!(row.state, "applied", "the refusal must not mutate state");
    assert!(row.approval_token.is_some(), "the refusal must not clear the token");
}

/// T023 / constitution II: a mid-apply plan has filesystem work in flight.
#[tokio::test]
async fn reopen_plan_refuses_a_mid_apply_plan() {
    let (db, bus) = setup().await;
    approved_plan(&db, &bus, "p-applying").await;
    repo::update_plan_state(db.pool(), "p-applying", "applying").await.unwrap();

    let err = reopen_plan(db.pool(), &bus, "p-applying", "tester")
        .await
        .expect_err("a mid-apply plan is not reopenable");
    assert_eq!(err.code, ErrorCode::PlanInvalidState);

    let row = repo::get_plan(db.pool(), "p-applying", false).await.unwrap();
    assert_eq!(row.state, "applying");
    assert!(row.approval_token.is_some());
}

/// T023: `ready_for_review → draft` is the other edge with a `→ draft`
/// transition; it carries no approval to invalidate.
#[tokio::test]
async fn reopen_plan_returns_a_review_plan_to_draft() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p-review").await;
    add_item(&db, "p-review", "p-review-item-1", "move").await;
    repo::update_plan_state(db.pool(), "p-review", "ready_for_review").await.unwrap();

    let resp = reopen_plan(db.pool(), &bus, "p-review", "tester").await.unwrap();
    assert_eq!(resp.new_state, "draft");
    assert_eq!(resp.prior_state, "ready_for_review");
}

/// T023: reopening a draft is a no-op rather than an error.
#[tokio::test]
async fn reopen_plan_is_idempotent_on_a_draft() {
    let (db, bus) = setup().await;
    insert_draft(&db, "p-draft").await;

    let resp = reopen_plan(db.pool(), &bus, "p-draft", "tester").await.unwrap();
    assert_eq!(resp.new_state, "draft");
}
