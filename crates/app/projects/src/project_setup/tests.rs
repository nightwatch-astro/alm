// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration-style tests for the whole `project_setup` public API
//! (`create`/`update`/`add_source`/`remove_source`/`reinfer_channels`/
//! `dismiss_drift`/`list`/`get`). Kept as one suite rather than split per
//! use-case file: fixtures (`setup`, `make_create_req`, `seed_*`) and most
//! assertions exercise several use cases together (e.g. create → add_source
//! → remove_source lifecycle transitions).

use super::*;
use crate::test_support::{abs, register_project_root, TEST_PROJECT_ROOT};
use audit::bus::EventBus;
use contracts_core::error_code::ErrorCode;
use contracts_core::projects_v2::{
    ProjectChannelsDismissDriftRequest, ProjectChannelsReinferRequest, ProjectCreateRequest,
    ProjectSourceAddRequest, ProjectSourceRemoveRequest, ProjectTool, ProjectUpdateRequest,
};
use persistence_core::Database;

async fn setup() -> (SqlitePool, EventBus) {
    let db = Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let bus = EventBus::with_pool(db.pool().clone());
    let pool = db.pool().clone();
    // Register a project-kind source so relative request paths have an
    // anchor (mirrors the first-run wizard registering a project folder).
    register_project_root(&pool, TEST_PROJECT_ROOT).await;
    (pool, bus)
}

/// An empty redb resolve cache — these tests exercise `create()` either
/// with no `canonical_target_id` or with one already seeded directly into
/// SQLite (`seed_canonical_target`), so promotion's cache fallback is
/// never exercised here (see `promotes_from_redb_cache_when_not_yet_durable`).
fn empty_cache() -> simbad_resolver::RedbCache {
    simbad_resolver::Store::in_memory().unwrap().cache()
}

// ── P7: exposure parsing + channel aggregation (pure helpers) ─────────────

#[test]
fn parse_exposure_seconds_whole_and_fractional() {
    assert_eq!(parse_exposure_seconds("300s"), 300);
    assert_eq!(parse_exposure_seconds("60s"), 60);
    // Fractional seconds truncate toward zero.
    assert_eq!(parse_exposure_seconds("1.5s"), 1);
    // Missing trailing "s" is still accepted.
    assert_eq!(parse_exposure_seconds("120"), 120);
}

#[test]
fn parse_exposure_seconds_degrades_to_zero_without_panicking() {
    assert_eq!(parse_exposure_seconds(""), 0);
    assert_eq!(parse_exposure_seconds("na"), 0);
    assert_eq!(parse_exposure_seconds("Mixed"), 0);
    assert_eq!(parse_exposure_seconds("-5s"), 0);
    assert_eq!(parse_exposure_seconds("   "), 0);
}

#[test]
fn channel_totals_by_filter_groups_and_sums() {
    let sources = vec![
        repo::ProjectSourceRow {
            id: "s1".to_owned(),
            project_id: "p1".to_owned(),
            inventory_session_id: "inv-1".to_owned(),
            name_snapshot: "Ha 1".to_owned(),
            frames_snapshot: 18,
            filter_snapshot: "Ha".to_owned(),
            exposure_snapshot: "120s".to_owned(),
            linked_at: "2026-01-01T00:00:00Z".to_owned(),
        },
        repo::ProjectSourceRow {
            id: "s2".to_owned(),
            project_id: "p1".to_owned(),
            inventory_session_id: "inv-2".to_owned(),
            name_snapshot: "Ha 2".to_owned(),
            frames_snapshot: 10,
            filter_snapshot: "Ha".to_owned(),
            exposure_snapshot: "60s".to_owned(),
            linked_at: "2026-01-01T00:00:00Z".to_owned(),
        },
        repo::ProjectSourceRow {
            id: "s3".to_owned(),
            project_id: "p1".to_owned(),
            inventory_session_id: "inv-3".to_owned(),
            name_snapshot: "OIII 1".to_owned(),
            frames_snapshot: 20,
            filter_snapshot: "OIII".to_owned(),
            exposure_snapshot: "300s".to_owned(),
            linked_at: "2026-01-01T00:00:00Z".to_owned(),
        },
        // Empty filter (unconfirmed source) must not contribute a bogus group.
        repo::ProjectSourceRow {
            id: "s4".to_owned(),
            project_id: "p1".to_owned(),
            inventory_session_id: "inv-4".to_owned(),
            name_snapshot: String::new(),
            frames_snapshot: 5,
            filter_snapshot: String::new(),
            exposure_snapshot: String::new(),
            linked_at: "2026-01-01T00:00:00Z".to_owned(),
        },
    ];

    let totals = channel_totals_by_filter(&sources);
    assert_eq!(totals.get("Ha"), Some(&(28, 18 * 120 + 10 * 60)));
    assert_eq!(totals.get("OIII"), Some(&(20, 20 * 300)));
    assert_eq!(totals.len(), 2, "empty filter_snapshot must not create a group");
}

fn make_create_req(name: &str, tool: ProjectTool) -> ProjectCreateRequest {
    ProjectCreateRequest {
        request_id: domain_core::ids::new_id(),
        name: name.to_owned(),
        tool,
        path: format!("projects/{name}"),
        initial_sources: vec![],
        notes: None,
        canonical_target_id: None,
        is_mosaic: false,
    }
}

#[tokio::test]
async fn create_project_setup_incomplete_no_sources() {
    let (pool, bus) = setup().await;
    let req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();
    assert_eq!(result.lifecycle, "setup_incomplete");
    assert!(result.channels.is_empty());
    // Constitution II: create always produces a folder-structure plan.
    assert!(result.plan_id.is_some(), "create must return a plan_id");
}

#[tokio::test]
async fn create_project_duplicate_name_rejected() {
    let (pool, bus) = setup().await;
    let req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    create(&pool, &bus, &empty_cache(), &req).await.unwrap();
    let req2 = ProjectCreateRequest { path: "projects/other".to_owned(), ..req };
    let err = create(&pool, &bus, &empty_cache(), &req2).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::NameDuplicate);
    // bd astro-plan-qnj0: field_errors must name the offending request field,
    // not stay permanently empty.
    assert_eq!(err.field_errors.len(), 1);
    assert_eq!(err.field_errors[0].field, "name");
    assert_eq!(err.field_errors[0].code, "name.duplicate");
}

#[tokio::test]
async fn create_project_duplicate_path_rejected() {
    let (pool, bus) = setup().await;
    let req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    create(&pool, &bus, &empty_cache(), &req).await.unwrap();
    let req2 = ProjectCreateRequest { name: "Other Name".to_owned(), ..req };
    let err = create(&pool, &bus, &empty_cache(), &req2).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathCollision);
    assert_eq!(err.field_errors.len(), 1);
    assert_eq!(err.field_errors[0].field, "path");
    assert_eq!(err.field_errors[0].code, "path.collision");
}

// ── Constitution I: path anchoring tests ──────────────────────────────────

#[tokio::test]
async fn create_anchors_relative_path_to_registered_project_root() {
    let (pool, bus) = setup().await;
    let req = make_create_req("M31 LRGB", ProjectTool::PixInsight);
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    let detail = get(&pool, &result.project_id).await.unwrap();
    assert_eq!(
        detail.path,
        format!("{TEST_PROJECT_ROOT}/projects/M31 LRGB"),
        "relative wizard path must be anchored to the registered project folder"
    );
    assert!(
        std::path::Path::new(&detail.path).is_absolute(),
        "stored project path must be absolute (CWD-independent)"
    );
}

/// CWD-independence proof at this layer: every scaffolding plan item
/// destination is absolute, so the mkdir executor and every other
/// consumer resolve the same location regardless of process CWD.
#[tokio::test]
async fn create_folder_plan_items_are_absolute_regardless_of_cwd() {
    use persistence_plans::repositories::plans as plans_repo;

    let (pool, bus) = setup().await;
    let req = make_create_req("NGC 7000 CWD", ProjectTool::PixInsight);
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    let plan_id = result.plan_id.expect("plan_id must be present");
    let items = plans_repo::list_plan_items(&pool, &plan_id).await.unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert!(
            std::path::Path::new(&item.to_relative_path).is_absolute(),
            "scaffolding destination must be absolute, got: {}",
            item.to_relative_path
        );
        assert!(
            item.to_relative_path.starts_with(TEST_PROJECT_ROOT),
            "scaffolding destination must live under the project root, got: {}",
            item.to_relative_path
        );
    }
}

#[tokio::test]
async fn create_absolute_path_stored_as_is() {
    let (pool, bus) = setup().await;
    // Platform-absolute: "/elsewhere/m101" alone is not absolute on
    // Windows and would be anchored instead of stored as-is.
    let req = ProjectCreateRequest {
        path: abs("/elsewhere/m101"),
        ..make_create_req("M101 Abs", ProjectTool::PixInsight)
    };
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();
    let detail = get(&pool, &result.project_id).await.unwrap();
    assert_eq!(detail.path, abs("/elsewhere/m101"));
}

#[tokio::test]
async fn create_relative_path_without_project_root_rejected() {
    // Fresh DB WITHOUT a registered project folder.
    let db = Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let bus = EventBus::with_pool(db.pool().clone());
    let req = make_create_req("No Root", ProjectTool::PixInsight);
    let err = create(db.pool(), &bus, &empty_cache(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathInvalid);
}

#[tokio::test]
async fn create_parent_dir_components_rejected() {
    let (pool, bus) = setup().await;
    let req = ProjectCreateRequest {
        path: "projects/../../etc".to_owned(),
        ..make_create_req("Escape Attempt", ProjectTool::PixInsight)
    };
    let err = create(&pool, &bus, &empty_cache(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathInvalid);
    assert_eq!(err.field_errors.len(), 1);
    assert_eq!(err.field_errors[0].field, "path");
    assert_eq!(err.field_errors[0].code, "path.invalid");
}

/// astro-plan-3v3r.9.12: an accepted `/` becomes the source-view destination
/// root, so scaffolding and generated links land directly under the filesystem
/// root.
#[tokio::test]
async fn create_filesystem_root_path_rejected() {
    let (pool, bus) = setup().await;
    for path in [abs("/"), ".".to_owned()] {
        let req = ProjectCreateRequest {
            path: path.clone(),
            ..make_create_req("Root Attempt", ProjectTool::PixInsight)
        };
        let err = create(&pool, &bus, &empty_cache(), &req).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::PathInvalid, "path {path} must be refused");
        assert_eq!(err.field_errors[0].field, "path");
    }
}

#[tokio::test]
async fn create_project_empty_name_rejected() {
    let (pool, bus) = setup().await;
    let req = make_create_req("", ProjectTool::PixInsight);
    let err = create(&pool, &bus, &empty_cache(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::NameEmpty);
    assert_eq!(err.field_errors.len(), 1);
    assert_eq!(err.field_errors[0].field, "name");
    assert_eq!(err.field_errors[0].code, "name.empty");
}

#[tokio::test]
async fn update_project_name() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("Old Name", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    let update_req = ProjectUpdateRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        name: Some("New Name".to_owned()),
        tool: None,
        notes: None,
        is_mosaic: None,
    };
    let result = update(&pool, &bus, &update_req).await.unwrap();
    assert!(result.fields_updated.contains(&"name".to_owned()));
}

#[tokio::test]
async fn update_archived_project_rejected() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("My Project", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    // Manually set lifecycle to archived.
    repo::update_project_lifecycle(&pool, &created.project_id, "archived").await.unwrap();

    let update_req = ProjectUpdateRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id,
        name: Some("New Name".to_owned()),
        tool: None,
        notes: None,
        is_mosaic: None,
    };
    let err = update(&pool, &bus, &update_req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::LifecycleReadOnly);
}

#[tokio::test]
async fn add_source_triggers_ready_transition() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();
    assert_eq!(created.lifecycle, "setup_incomplete");

    let add_req = ProjectSourceAddRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        inventory_session_id: "inv-001".to_owned(),
    };
    let result = add_source(&pool, &bus, &add_req).await.unwrap();
    assert_eq!(result.new_lifecycle, Some("ready".to_owned()));
}

/// #665: `add_source` must fire the `SourceChange` manifest trigger with
/// the POST-mutation lifecycle (here, the auto `ready` transition), not
/// the stale pre-mutation value.
#[tokio::test]
async fn add_source_writes_source_change_manifest() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("M31 Andromeda", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    // `TEST_PROJECT_ROOT` is a synthetic, non-writable path (existing
    // convention across this module's tests) — point the project at a
    // real tempdir so the manifest file write can actually succeed.
    let dir = tempfile::tempdir().unwrap();
    sqlx::query("UPDATE projects SET path = ? WHERE id = ?")
        .bind(dir.path().to_str().unwrap())
        .bind(&created.project_id)
        .execute(&pool)
        .await
        .unwrap();

    let add_req = ProjectSourceAddRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        inventory_session_id: "inv-001".to_owned(),
    };
    add_source(&pool, &bus, &add_req).await.unwrap();

    let (rows, _) = persistence_plans::repositories::manifests::list_manifests_for_project(
        &pool,
        &created.project_id,
        None,
        10,
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reason, "source_change");
    let manifest = crate::project_manifests::get(&pool, &rows[0].id).await.unwrap();
    assert_eq!(manifest.manifest.body.lifecycle_state, "ready");
    assert!(manifest.manifest.body.source_map.is_some());
}

#[tokio::test]
async fn add_source_duplicate_rejected() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    let add_req = ProjectSourceAddRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        inventory_session_id: "inv-001".to_owned(),
    };
    add_source(&pool, &bus, &add_req).await.unwrap();
    let err = add_source(
        &pool,
        &bus,
        &ProjectSourceAddRequest { request_id: domain_core::ids::new_id(), ..add_req },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::SourceAlreadyLinked);
}

#[tokio::test]
async fn remove_source_last_requires_confirmation() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    let add_req = ProjectSourceAddRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        inventory_session_id: "inv-001".to_owned(),
    };
    add_source(&pool, &bus, &add_req).await.unwrap();

    // Attempt remove without confirmation.
    let rm_req = ProjectSourceRemoveRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        project_source_id: "inv-001".to_owned(),
        confirm_last_source: false,
    };
    let err = remove_source(&pool, &bus, &rm_req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::LifecycleLastConfirmedSource);
}

#[tokio::test]
async fn remove_source_with_confirmation_succeeds() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    let add_req = ProjectSourceAddRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        inventory_session_id: "inv-001".to_owned(),
    };
    add_source(&pool, &bus, &add_req).await.unwrap();

    let rm_req = ProjectSourceRemoveRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        project_source_id: "inv-001".to_owned(),
        confirm_last_source: true,
    };
    let result = remove_source(&pool, &bus, &rm_req).await.unwrap();
    assert_eq!(result.new_lifecycle, Some("setup_incomplete".to_owned()));
}

/// #665: `remove_source` must also fire the `SourceChange` manifest
/// trigger, with the POST-removal regressed lifecycle.
#[tokio::test]
async fn remove_source_writes_source_change_manifest() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("NGC 6960 Veil", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    sqlx::query("UPDATE projects SET path = ? WHERE id = ?")
        .bind(dir.path().to_str().unwrap())
        .bind(&created.project_id)
        .execute(&pool)
        .await
        .unwrap();

    let add_req = ProjectSourceAddRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        inventory_session_id: "inv-001".to_owned(),
    };
    add_source(&pool, &bus, &add_req).await.unwrap();

    let rm_req = ProjectSourceRemoveRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        project_source_id: "inv-001".to_owned(),
        confirm_last_source: true,
    };
    remove_source(&pool, &bus, &rm_req).await.unwrap();

    let (rows, _) = persistence_plans::repositories::manifests::list_manifests_for_project(
        &pool,
        &created.project_id,
        None,
        10,
    )
    .await
    .unwrap();
    // One from add_source, one from remove_source.
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.reason == "source_change"));
    let latest = crate::project_manifests::get(&pool, &rows[0].id).await.unwrap();
    assert_eq!(latest.manifest.body.lifecycle_state, "setup_incomplete");
}

#[tokio::test]
async fn remove_source_from_prepared_rejected() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    // Add a source so lifecycle can be moved to ready.
    let add_req = ProjectSourceAddRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        inventory_session_id: "inv-001".to_owned(),
    };
    add_source(&pool, &bus, &add_req).await.unwrap();
    // Move to prepared manually (lifecycle gate test).
    repo::update_project_lifecycle(&pool, &created.project_id, "prepared").await.unwrap();

    let rm_req = ProjectSourceRemoveRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
        project_source_id: "inv-001".to_owned(),
        confirm_last_source: true,
    };
    let err = remove_source(&pool, &bus, &rm_req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::LifecycleReadOnly);
}

#[tokio::test]
async fn reinfer_clears_drift_flag() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    // Artificially set drift.
    repo::set_channel_drift(&pool, &created.project_id, true).await.unwrap();

    let req = ProjectChannelsReinferRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
    };
    reinfer_channels(&pool, &bus, &req).await.unwrap();
    let detail = get(&pool, &created.project_id).await.unwrap();
    assert!(!detail.channel_drift.has_new_sources);
}

#[tokio::test]
async fn dismiss_drift_clears_flag() {
    let (pool, bus) = setup().await;
    let create_req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &create_req).await.unwrap();

    repo::set_channel_drift(&pool, &created.project_id, true).await.unwrap();

    let req = ProjectChannelsDismissDriftRequest {
        request_id: domain_core::ids::new_id(),
        project_id: created.project_id.clone(),
    };
    dismiss_drift(&pool, &bus, &req).await.unwrap();
    let detail = get(&pool, &created.project_id).await.unwrap();
    assert!(!detail.channel_drift.has_new_sources);
}

#[tokio::test]
async fn list_projects_returns_summary() {
    let (pool, bus) = setup().await;
    create(&pool, &bus, &empty_cache(), &make_create_req("A", ProjectTool::PixInsight))
        .await
        .unwrap();
    create(&pool, &bus, &empty_cache(), &make_create_req("B", ProjectTool::Siril)).await.unwrap();
    let list = list(&pool).await.unwrap();
    assert_eq!(list.len(), 2);

    // Verify summary content is real, not just row count — each summary
    // must carry its own name/tool/path, not a copy or a default.
    // `path` is resolved against the project root at create time (not a
    // literal echo of the request path), so check the suffix rather than
    // an exact match.
    let a = list.iter().find(|p| p.name == "A").expect("project A must be in the list");
    assert_eq!(a.tool, ProjectTool::PixInsight);
    assert!(a.path.ends_with("projects/A"), "path must resolve under project A's name: {}", a.path);

    let b = list.iter().find(|p| p.name == "B").expect("project B must be in the list");
    assert_eq!(b.tool, ProjectTool::Siril);
    assert!(b.path.ends_with("projects/B"), "path must resolve under project B's name: {}", b.path);

    assert_ne!(a.id, b.id, "summaries must carry distinct project ids");
}

// ── Constitution II: folder plan tests ────────────────────────────────────

#[tokio::test]
async fn create_returns_plan_id() {
    let (pool, bus) = setup().await;
    let req = make_create_req("NGC 7000 NB", ProjectTool::PixInsight);
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();
    assert!(result.plan_id.is_some(), "create must return a plan_id for the folder-structure plan");
}

#[tokio::test]
async fn create_pixinsight_plan_has_correct_folders() {
    use persistence_plans::repositories::plans as plans_repo;

    let (pool, bus) = setup().await;
    let req = make_create_req("NGC 7000 PI", ProjectTool::PixInsight);
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    let plan_id = result.plan_id.expect("plan_id must be present");
    let plan = plans_repo::get_plan(&pool, &plan_id, false).await.unwrap();
    let items = plans_repo::list_plan_items(&pool, &plan_id).await.unwrap();

    assert_eq!(plan.state, "ready_for_review", "plan should be ready for review");
    assert_eq!(plan.origin, "project");

    let actions: Vec<&str> = items.iter().map(|i| i.action.as_str()).collect();
    // PixInsight: 6 mkdir + 1 write_manifest = 7 items
    let mkdir_count = actions.iter().filter(|&&a| a == "mkdir").count();
    let manifest_count = actions.iter().filter(|&&a| a == "write_manifest").count();
    assert_eq!(
        mkdir_count, 6,
        "PixInsight needs 6 sub-folders (lights, darks, flats, bias, output, processing)"
    );
    assert_eq!(manifest_count, 1, "exactly one marker write item");

    let folder_names: Vec<&str> =
        items.iter().filter(|i| i.action == "mkdir").map(|i| i.name.as_str()).collect();
    assert!(folder_names.contains(&"lights"));
    assert!(folder_names.contains(&"darks"));
    assert!(folder_names.contains(&"flats"));
    assert!(folder_names.contains(&"bias"));
    assert!(folder_names.contains(&"output"));
    assert!(folder_names.contains(&"processing"));

    // Marker item name should be the app marker filename
    let has_marker = items.iter().any(|i| i.name == ".astro-plan-project.json");
    assert!(has_marker, "marker file item must be present");
}

#[tokio::test]
async fn create_siril_plan_has_five_folders() {
    use persistence_plans::repositories::plans as plans_repo;

    let (pool, bus) = setup().await;
    let req = make_create_req("M31 Siril", ProjectTool::Siril);
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    let plan_id = result.plan_id.expect("plan_id must be present");
    let items = plans_repo::list_plan_items(&pool, &plan_id).await.unwrap();

    let mkdir_count = items.iter().filter(|i| i.action == "mkdir").count();
    // Siril: 5 sub-folders (no processing/)
    assert_eq!(mkdir_count, 5);
    assert!(
        !items
            .iter()
            .filter(|i| i.action == "mkdir")
            .map(|i| i.name.as_str())
            .any(|n| n == "processing"),
        "Siril has no processing/ folder"
    );
}

// ── spec 035 US1 #2: project ↔ canonical_target association ──────────────────

/// Insert a minimal `canonical_target` row so the create use case's existence
/// check passes and the FK target is present.
async fn seed_canonical_target(pool: &SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO canonical_target
            (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at)
         VALUES (?, NULL, 'M 31', 'galaxy', 10.68, 41.27, 'resolved', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn create_without_canonical_target_stores_null() {
    let (pool, bus) = setup().await;
    let req = make_create_req("No Target Project", ProjectTool::PixInsight);
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();
    let stored = repo::get_project_canonical_target_id(&pool, &result.project_id).await.unwrap();
    assert_eq!(stored, None, "absent canonicalTargetId must persist as NULL");
}

#[tokio::test]
async fn create_with_canonical_target_persists_and_reads_back() {
    let (pool, bus) = setup().await;
    let ctid = "11111111-1111-5111-8111-111111111111";
    seed_canonical_target(&pool, ctid).await;

    let mut req = make_create_req("Targeted Project", ProjectTool::PixInsight);
    req.canonical_target_id = Some(ctid.to_owned());
    let result = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    let stored = repo::get_project_canonical_target_id(&pool, &result.project_id).await.unwrap();
    assert_eq!(stored.as_deref(), Some(ctid), "canonicalTargetId must round-trip");
}

#[tokio::test]
async fn create_with_unknown_canonical_target_is_rejected() {
    let (pool, bus) = setup().await;
    let mut req = make_create_req("Dangling Target", ProjectTool::PixInsight);
    req.canonical_target_id = Some("22222222-2222-5222-8222-222222222222".to_owned());
    let err = create(&pool, &bus, &empty_cache(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::CanonicalTargetNotFound);
}

// ── spec 035 US1 #2: canonical target surfaced on the detail READ path ───────

#[tokio::test]
async fn get_returns_canonical_target_with_designation_and_common_name() {
    let (pool, bus) = setup().await;
    let ctid = "33333333-3333-5333-8333-333333333333";
    seed_canonical_target(&pool, ctid).await;
    // A common_name alias so the read populates `common_name`.
    sqlx::query(
        "INSERT INTO target_alias (id, target_id, alias, normalized, kind)
         VALUES ('a1', ?, 'Andromeda Galaxy', 'andromeda galaxy', 'common_name')",
    )
    .bind(ctid)
    .execute(&pool)
    .await
    .unwrap();

    let mut req = make_create_req("Detail With Target", ProjectTool::PixInsight);
    req.canonical_target_id = Some(ctid.to_owned());
    let created = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    let detail = get(&pool, &created.project_id).await.unwrap();
    let ct = detail.canonical_target.expect("canonical_target must be present");
    assert_eq!(ct.id, ctid);
    assert_eq!(ct.primary_designation, "M 31");
    assert_eq!(ct.common_name.as_deref(), Some("Andromeda Galaxy"));
}

#[tokio::test]
async fn get_canonical_target_is_none_without_alias_common_name() {
    let (pool, bus) = setup().await;
    let ctid = "44444444-4444-5444-8444-444444444444";
    seed_canonical_target(&pool, ctid).await; // no common_name alias

    let mut req = make_create_req("Detail No Common Name", ProjectTool::PixInsight);
    req.canonical_target_id = Some(ctid.to_owned());
    let created = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    let detail = get(&pool, &created.project_id).await.unwrap();
    let ct = detail.canonical_target.expect("association present");
    assert_eq!(ct.primary_designation, "M 31");
    assert_eq!(ct.common_name, None, "no common-name alias → null");
}

#[tokio::test]
async fn get_returns_no_canonical_target_when_unassociated() {
    let (pool, bus) = setup().await;
    let req = make_create_req("Detail No Target", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    let detail = get(&pool, &created.project_id).await.unwrap();
    assert!(detail.canonical_target.is_none(), "no association → None");
}

// ── P7: server-side channel aggregation (end-to-end via `get`) ────────────

/// Directly insert a `project_sources` row with real snapshot data
/// (bypassing `add_source`, which — pending spec 003 Inventory
/// integration — always writes empty/zero snapshot fields). This mirrors
/// the fixture shape used in `persistence_plans::repositories::projects`
/// tests and is the only way to exercise the aggregation with non-zero
/// frames/exposure until spec 003 lands.
async fn seed_real_source(
    pool: &SqlitePool,
    project_id: &str,
    inv_id: &str,
    filter: &str,
    frames: i64,
    exposure: &str,
) {
    repo::insert_project_source(
        pool,
        &repo::InsertProjectSource {
            id: &domain_core::ids::new_id(),
            project_id,
            inventory_session_id: inv_id,
            name_snapshot: &format!("{filter} {inv_id}"),
            frames_snapshot: frames,
            filter_snapshot: filter,
            exposure_snapshot: exposure,
            linked_at: "2026-01-01T00:00:00Z",
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn get_aggregates_sub_frames_and_integration_seconds_per_channel() {
    let (pool, bus) = setup().await;
    let req = make_create_req("Aggregation Project", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    seed_real_source(&pool, &created.project_id, "inv-1", "Ha", 18, "120s").await;
    seed_real_source(&pool, &created.project_id, "inv-2", "Ha", 10, "60s").await;
    seed_real_source(&pool, &created.project_id, "inv-3", "OIII", 20, "300s").await;
    repo::replace_project_channels(
        &pool,
        &created.project_id,
        &[("Ha", "inferred"), ("OIII", "inferred")],
    )
    .await
    .unwrap();

    let detail = get(&pool, &created.project_id).await.unwrap();
    let ha = detail.channels.iter().find(|c| c.label == "Ha").expect("Ha channel");
    assert_eq!(ha.sub_frames, 28);
    assert_eq!(ha.total_integration_s, 18 * 120 + 10 * 60);

    let oiii = detail.channels.iter().find(|c| c.label == "OIII").expect("OIII channel");
    assert_eq!(oiii.sub_frames, 20);
    assert_eq!(oiii.total_integration_s, 20 * 300);
}

#[tokio::test]
async fn get_channel_totals_ignore_unparseable_exposure_without_panicking() {
    let (pool, bus) = setup().await;
    let req = make_create_req("Unparseable Exposure Project", ProjectTool::PixInsight);
    let created = create(&pool, &bus, &empty_cache(), &req).await.unwrap();

    seed_real_source(&pool, &created.project_id, "inv-1", "Ha", 5, "Mixed").await;
    repo::replace_project_channels(&pool, &created.project_id, &[("Ha", "inferred")])
        .await
        .unwrap();

    let detail = get(&pool, &created.project_id).await.unwrap();
    let ha = detail.channels.iter().find(|c| c.label == "Ha").expect("Ha channel");
    assert_eq!(ha.sub_frames, 5, "frame count is still summed even when exposure is unparseable");
    assert_eq!(ha.total_integration_s, 0, "unparseable exposure degrades to 0s, never panics");
}
