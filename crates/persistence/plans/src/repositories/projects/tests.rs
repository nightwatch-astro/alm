// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

use sqlx::SqlitePool;

use super::*;
use crate::repositories::plans::{self, InsertPlan, InsertPlanItem};
use persistence_core::{Database, DbError};

async fn setup() -> Database {
    let db = Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    db
}

fn project_a(id: &str) -> InsertProject<'_> {
    InsertProject {
        id,
        name: "NGC 7000 NB",
        tool: "PixInsight",
        lifecycle: "setup_incomplete",
        path: "projects/NGC7000_NB",
        notes: None,
        canonical_target_id: None,
        is_mosaic: false,
    }
}

#[tokio::test]
async fn insert_and_get_project() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    let row = get_project(db.pool(), "p1").await.unwrap();
    assert_eq!(row.name, "NGC 7000 NB");
    assert_eq!(row.tool, "PixInsight");
    assert_eq!(row.lifecycle, "setup_incomplete");
    assert!(!row.channel_drift);
}

// ── list_projects_by_canonical_target_id (F-Framing-5) ────────────────────

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
async fn list_projects_by_canonical_target_id_returns_only_matching() {
    let db = setup().await;
    seed_canonical_target(db.pool(), "target-1").await;
    seed_canonical_target(db.pool(), "target-2").await;
    insert_project(
        db.pool(),
        &InsertProject { canonical_target_id: Some("target-1"), ..project_a("p1") },
    )
    .await
    .unwrap();
    insert_project(
        db.pool(),
        &InsertProject {
            id: "p2",
            name: "M31 LRGB",
            path: "projects/p2",
            canonical_target_id: Some("target-2"),
            ..project_a("p2")
        },
    )
    .await
    .unwrap();

    let rows = list_projects_by_canonical_target_id(db.pool(), "target-1").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "p1");
}

#[tokio::test]
async fn list_projects_returns_all() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    insert_project(
        db.pool(),
        &InsertProject {
            id: "p2",
            name: "M31 LRGB",
            tool: "Siril",
            lifecycle: "ready",
            path: "projects/M31_LRGB",
            notes: Some("test notes"),
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();
    let rows = list_projects(db.pool()).await.unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn name_exists_detects_duplicate() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    let conflict = name_exists(db.pool(), "NGC 7000 NB", None).await.unwrap();
    assert_eq!(conflict, Some("p1".to_owned()));
    let no_conflict = name_exists(db.pool(), "M31", None).await.unwrap();
    assert!(no_conflict.is_none());
}

#[tokio::test]
async fn update_project_fields_changes_name() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    update_project_fields(db.pool(), "p1", Some("M31 LRGB"), None, None, None).await.unwrap();
    let row = get_project(db.pool(), "p1").await.unwrap();
    assert_eq!(row.name, "M31 LRGB");
}

#[tokio::test]
async fn update_project_fields_changes_is_mosaic() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    update_project_fields(db.pool(), "p1", None, None, None, Some(true)).await.unwrap();
    let row = get_project(db.pool(), "p1").await.unwrap();
    assert!(row.is_mosaic);
}

#[tokio::test]
async fn insert_and_list_project_sources() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    let now = "2026-06-01T00:00:00Z";
    insert_project_source(
        db.pool(),
        &InsertProjectSource {
            id: "src-1",
            project_id: "p1",
            inventory_session_id: "inv-001",
            name_snapshot: "NGC7000 Ha",
            frames_snapshot: 18,
            filter_snapshot: "Ha",
            exposure_snapshot: "120s",
            linked_at: now,
        },
    )
    .await
    .unwrap();
    let sources = list_project_sources(db.pool(), "p1").await.unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].filter_snapshot, "Ha");
}

#[tokio::test]
async fn duplicate_source_link_rejected() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    let now = "2026-06-01T00:00:00Z";
    let src = InsertProjectSource {
        id: "src-1",
        project_id: "p1",
        inventory_session_id: "inv-001",
        name_snapshot: "Ha",
        frames_snapshot: 10,
        filter_snapshot: "Ha",
        exposure_snapshot: "60s",
        linked_at: now,
    };
    insert_project_source(db.pool(), &src).await.unwrap();
    // Second insert with same (project_id, inventory_session_id) must fail
    let result =
        insert_project_source(db.pool(), &InsertProjectSource { id: "src-2", ..src }).await;
    assert!(result.is_err());
}

// ── find_blockable_missing_sources (R-17/FR-052) ──────────────────────────

/// Seeds a root plus one `file_record` in state `missing`. The query joins
/// `json_each` and `file_record` with INNER joins, so an empty `file_record`
/// lets SQLite return no rows without ever evaluating the JSON function.
async fn seed_missing_frame(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
         VALUES ('root-1', 'Main', '/lib', 'local', 'active', '2026-06-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO file_record
            (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at)
         VALUES ('file-1', 'root-1', 'lights/light_001.fits', 1024, '2026-06-01T00:00:00Z',
                 'missing', '2026-06-01T00:00:00Z', '2026-06-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_acquisition_session(pool: &SqlitePool, id: &str, frame_ids: &str) {
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, frame_ids, root_id, created_at)
         VALUES (?, ?, ?, 'root-1', '2026-06-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(frame_ids)
    .execute(pool)
    .await
    .unwrap();
}

/// The one `frame_ids` site that MUST keep raising on malformed JSON. The caller
/// reads the failure as an incomplete health check and leaves a durable retry
/// marker; guarding it reports the corrupt session as having no missing sources,
/// which leaves the project unblockable and writes no marker. Pins the raise so
/// a later guard sweep cannot remove the signal without failing here.
#[tokio::test]
async fn find_blockable_missing_sources_raises_on_a_malformed_frame_ids_session() {
    let db = setup().await;
    seed_missing_frame(db.pool()).await;
    // `acquisition_session.frame_ids` is `TEXT NOT NULL DEFAULT '[]'` with no
    // `json_valid` CHECK, so a non-JSON value is storable.
    seed_acquisition_session(db.pool(), "sess-corrupt", "oops").await;
    seed_acquisition_session(db.pool(), "sess-ok", r#"["file-1"]"#).await;
    insert_project(db.pool(), &InsertProject { lifecycle: "ready", ..project_a("p1") })
        .await
        .unwrap();
    for (row_id, session_id) in [("src-1", "sess-corrupt"), ("src-2", "sess-ok")] {
        insert_project_source(
            db.pool(),
            &InsertProjectSource {
                id: row_id,
                project_id: "p1",
                inventory_session_id: session_id,
                name_snapshot: "NGC7000 Ha",
                frames_snapshot: 1,
                filter_snapshot: "Ha",
                exposure_snapshot: "60s",
                linked_at: "2026-06-01T00:00:00Z",
            },
        )
        .await
        .unwrap();
    }

    let err = find_blockable_missing_sources(db.pool())
        .await
        .expect_err("a malformed frame_ids row must fail the health check loudly");
    assert!(
        err.to_string().contains("malformed JSON"),
        "expected a malformed-JSON raise, got: {err}"
    );
}

// ── has_archived_raw_frames_for_project (F-Framing-6, Q25 warning) ────────

async fn insert_applied_raw_frame_archive_item(pool: &SqlitePool, plan_id: &str, source_id: &str) {
    plans::insert_plan(
        pool,
        &plans::InsertPlan {
            id: plan_id,
            title: "Raw sub-frame cleanup",
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
    plans::insert_plan_item(
        pool,
        &plans::InsertPlanItem {
            id: &format!("{plan_id}-item-0"),
            plan_id,
            item_index: 0,
            name: "light_001.fits",
            action: "archive",
            from_root_id: Some("root-1"),
            from_relative_path: "lights/light_001.fits",
            to_root_id: None,
            to_relative_path: "",
            reason: "raw_frame_cleanup",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: Some(source_id),
            category: Some("raw_frames"),
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE plans SET state = 'applied' WHERE id = ?")
        .bind(plan_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE plan_items SET item_state = 'succeeded' WHERE id = ?")
        .bind(format!("{plan_id}-item-0"))
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn has_archived_raw_frames_is_false_with_no_cleanup_history() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    assert!(!has_archived_raw_frames_for_project(db.pool(), "p1").await.unwrap());
}

#[tokio::test]
async fn has_archived_raw_frames_is_true_after_an_applied_raw_frame_archive() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    insert_project_source(
        db.pool(),
        &InsertProjectSource {
            id: "src-1",
            project_id: "p1",
            inventory_session_id: "sess-1",
            name_snapshot: "Ha",
            frames_snapshot: 10,
            filter_snapshot: "Ha",
            exposure_snapshot: "60s",
            linked_at: "2026-06-01T00:00:00Z",
        },
    )
    .await
    .unwrap();

    insert_applied_raw_frame_archive_item(db.pool(), "cleanup-plan-1", "sess-1").await;

    assert!(has_archived_raw_frames_for_project(db.pool(), "p1").await.unwrap());
}

#[tokio::test]
async fn has_archived_raw_frames_ignores_a_plan_that_is_not_yet_applied() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    insert_project_source(
        db.pool(),
        &InsertProjectSource {
            id: "src-1",
            project_id: "p1",
            inventory_session_id: "sess-1",
            name_snapshot: "Ha",
            frames_snapshot: 10,
            filter_snapshot: "Ha",
            exposure_snapshot: "60s",
            linked_at: "2026-06-01T00:00:00Z",
        },
    )
    .await
    .unwrap();

    // Draft (never applied) plan — must not count as archived.
    plans::insert_plan(
        db.pool(),
        &plans::InsertPlan {
            id: "cleanup-plan-2",
            title: "Raw sub-frame cleanup",
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
    plans::insert_plan_item(
        db.pool(),
        &plans::InsertPlanItem {
            id: "cleanup-plan-2-item-0",
            plan_id: "cleanup-plan-2",
            item_index: 0,
            name: "light_001.fits",
            action: "archive",
            from_root_id: Some("root-1"),
            from_relative_path: "lights/light_001.fits",
            to_root_id: None,
            to_relative_path: "",
            reason: "raw_frame_cleanup",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: Some("sess-1"),
            category: Some("raw_frames"),
        },
    )
    .await
    .unwrap();

    assert!(!has_archived_raw_frames_for_project(db.pool(), "p1").await.unwrap());
}

#[tokio::test]
async fn replace_channels_is_idempotent() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    replace_project_channels(db.pool(), "p1", &[("Ha", "inferred"), ("OIII", "inferred")])
        .await
        .unwrap();
    replace_project_channels(db.pool(), "p1", &[("Ha", "inferred"), ("SII", "manual")])
        .await
        .unwrap();
    let ch = list_project_channels(db.pool(), "p1").await.unwrap();
    assert_eq!(ch.len(), 2);
    let labels: Vec<&str> = ch.iter().map(|r| r.label.as_str()).collect();
    assert!(labels.contains(&"Ha"));
    assert!(labels.contains(&"SII"));
}

#[tokio::test]
async fn delete_project_source_removes_row() {
    let db = setup().await;
    insert_project(db.pool(), &project_a("p1")).await.unwrap();
    let now = "2026-06-01T00:00:00Z";
    insert_project_source(
        db.pool(),
        &InsertProjectSource {
            id: "src-1",
            project_id: "p1",
            inventory_session_id: "inv-001",
            name_snapshot: "Ha",
            frames_snapshot: 10,
            filter_snapshot: "Ha",
            exposure_snapshot: "60s",
            linked_at: now,
        },
    )
    .await
    .unwrap();
    let affected = delete_project_source(db.pool(), "p1", "inv-001").await.unwrap();
    assert_eq!(affected, 1);
    let sources = list_project_sources(db.pool(), "p1").await.unwrap();
    assert!(sources.is_empty());
}

// ── create_project_tx: atomicity (T2-a) ────────────────────────────────

#[tokio::test]
async fn create_project_tx_persists_project_sources_channels_and_plan() {
    let db = setup().await;
    let sources = [InsertProjectSource {
        id: "src-1",
        project_id: "px",
        inventory_session_id: "inv-1",
        name_snapshot: "",
        frames_snapshot: 0,
        filter_snapshot: "",
        exposure_snapshot: "",
        linked_at: "2026-01-01T00:00:00Z",
    }];
    let plan_items = [InsertPlanItem {
        id: "item-1",
        plan_id: "plan-x",
        item_index: 0,
        name: "lights",
        action: "mkdir",
        from_root_id: None,
        from_relative_path: "",
        to_root_id: None,
        to_relative_path: "projects/px/lights",
        reason: "Create project sub-folder",
        protection: "normal",
        linked_entity: Some("px"),
        provenance_json: None,
        archive_path: None,
        source_id: None,
        category: None,
    }];
    let input = CreateProjectInput {
        project: project_a("px"),
        sources: &sources,
        channels: &[("Ha", "inferred")],
        channels_added_at: "2026-01-01T00:00:00Z",
        plan: InsertPlan {
            id: "plan-x",
            title: "Create project folder structure",
            origin: "project",
            origin_path: Some("projects/px"),
            plan_type: "project_create",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
        plan_items: &plan_items,
    };

    create_project_tx(db.pool(), &input).await.unwrap();

    let row = get_project(db.pool(), "px").await.unwrap();
    assert_eq!(row.name, "NGC 7000 NB");
    let sources = list_project_sources(db.pool(), "px").await.unwrap();
    assert_eq!(sources.len(), 1);
    let channels = list_project_channels(db.pool(), "px").await.unwrap();
    assert_eq!(channels.len(), 1);
    let plan = crate::repositories::plans::get_plan(db.pool(), "plan-x", false).await.unwrap();
    assert_eq!(plan.state, "ready_for_review");
    let items = crate::repositories::plans::list_plan_items(db.pool(), "plan-x").await.unwrap();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn create_project_tx_rolls_back_all_writes_on_mid_sequence_failure() {
    let db = setup().await;
    // Two sources sharing the same primary key: the second insert violates
    // `project_sources.id`'s PRIMARY KEY, forcing a failure *after* the
    // project row and the first source row have already been written
    // inside the same transaction.
    let sources = [
        InsertProjectSource {
            id: "dupe-src",
            project_id: "px",
            inventory_session_id: "inv-1",
            name_snapshot: "",
            frames_snapshot: 0,
            filter_snapshot: "",
            exposure_snapshot: "",
            linked_at: "2026-01-01T00:00:00Z",
        },
        InsertProjectSource {
            id: "dupe-src",
            project_id: "px",
            inventory_session_id: "inv-2",
            name_snapshot: "",
            frames_snapshot: 0,
            filter_snapshot: "",
            exposure_snapshot: "",
            linked_at: "2026-01-01T00:00:00Z",
        },
    ];
    let plan_items: [InsertPlanItem; 0] = [];
    let input = CreateProjectInput {
        project: project_a("px"),
        sources: &sources,
        channels: &[("Ha", "inferred")],
        channels_added_at: "2026-01-01T00:00:00Z",
        plan: InsertPlan {
            id: "plan-x",
            title: "Create project folder structure",
            origin: "project",
            origin_path: Some("projects/px"),
            plan_type: "project_create",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
        plan_items: &plan_items,
    };

    let err = create_project_tx(db.pool(), &input).await.unwrap_err();
    assert!(matches!(err, DbError::Database(_)), "expected a UNIQUE constraint violation");

    // Full rollback: neither the project row, the first (successfully
    // inserted-then-rolled-back) source row, the channel row, nor the plan
    // row — none of which were ever reached after the failing statement —
    // may persist.
    let project_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE id = ?")
        .bind("px")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(project_count, 0, "the project row must not persist after a mid-sequence failure");

    let source_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_sources WHERE project_id = ?")
            .bind("px")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(source_count, 0, "no partial source rows may persist");

    let channel_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_channels WHERE project_id = ?")
            .bind("px")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(channel_count, 0, "no channel rows may persist");

    let plan_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plans WHERE id = ?")
        .bind("plan-x")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(plan_count, 0, "the plan row (never reached) must not persist");
}
