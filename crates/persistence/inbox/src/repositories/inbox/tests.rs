// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

use sqlx::SqlitePool;

use super::*;
use persistence_core::Database;

async fn test_db() -> Database {
    let db = Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    db
}

fn sample_item(id: &str) -> InsertInboxItem<'_> {
    InsertInboxItem {
        id,
        root_id: "root-1",
        relative_path: "2025-10-10/lights",
        file_count: 20,
        content_signature: Some("sig-abc"),
        lane: "fits",
    }
}

/// Like `sample_item`, but with an explicit `relative_path` — needed
/// whenever a test inserts more than one item, since `sample_item`'s
/// fixed path collides on the `(root_id, relative_path, group_key)`
/// UNIQUE index once a second item shares it.
fn sample_item_at<'a>(id: &'a str, path: &'a str) -> InsertInboxItem<'a> {
    InsertInboxItem {
        id,
        root_id: "root-1",
        relative_path: path,
        file_count: 2,
        content_signature: Some("sig"),
        lane: "fits",
    }
}

#[tokio::test]
async fn insert_and_get_inbox_item() {
    let db = test_db().await;
    insert_inbox_item(db.pool(), &sample_item("item-1")).await.unwrap();
    let row = get_inbox_item(db.pool(), "item-1").await.unwrap();
    assert_eq!(row.id, "item-1");
    assert_eq!(row.state, "pending_classification");
    assert_eq!(row.lane, "fits");
}

#[tokio::test]
async fn update_inbox_item_state_transitions() {
    let db = test_db().await;
    insert_inbox_item(db.pool(), &sample_item("item-2")).await.unwrap();
    update_inbox_item_state(db.pool(), "item-2", "classified").await.unwrap();
    let row = get_inbox_item(db.pool(), "item-2").await.unwrap();
    assert_eq!(row.state, "classified");
}

#[tokio::test]
async fn upsert_classification_and_get() {
    let db = test_db().await;
    insert_inbox_item(db.pool(), &sample_item("item-3")).await.unwrap();

    // Migration 0048 renamed 'single_type' → 'classified' in the CHECK constraint.
    let c = UpsertClassification {
        inbox_item_id: "item-3",
        result: "classified",
        frame_type: Some("light"),
        content_signature: "sig-xyz",
        unclassified_file_count: 0,
    };
    upsert_classification(db.pool(), &c).await.unwrap();

    let row = get_classification(db.pool(), "item-3").await.unwrap().unwrap();
    assert_eq!(row.result, "classified");
    assert_eq!(row.frame_type, Some("light".to_owned()));
}

#[tokio::test]
async fn insert_and_list_evidence() {
    let db = test_db().await;
    insert_inbox_item(db.pool(), &sample_item("item-4")).await.unwrap();

    let ev = InsertEvidence {
        id: "ev-1",
        inbox_item_id: "item-4",
        relative_file_path: "2025-10-10/lights/frame_001.fits",
        frame_type: Some("light"),
        evidence_source: "imagetyp_header",
        raw_value: Some("Light Frame"),
        unclassified: false,
        manual_override: None,
        is_master: false,
        master_detector: None,
    };
    insert_evidence(db.pool(), &ev).await.unwrap();

    let rows = list_evidence(db.pool(), "item-4").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].relative_file_path, "2025-10-10/lights/frame_001.fits");
    assert_eq!(rows[0].frame_type, Some("light".to_owned()));
}

#[tokio::test]
async fn plan_link_insert_and_get() {
    let db = test_db().await;
    insert_inbox_item(db.pool(), &sample_item("item-6")).await.unwrap();

    // Need a real plan row to satisfy FK
    let plan_insert = persistence_plans::repositories::plans::InsertPlan {
        id: "plan-inbox-1",
        title: "Inbox Split",
        origin: "inbox",
        origin_path: None,
        plan_type: "split",
        destructive_destination: "archive",
        parent_plan_id: None,
        total_bytes_required: 0,
    };
    persistence_plans::repositories::plans::insert_plan(db.pool(), &plan_insert).await.unwrap();

    insert_plan_link(db.pool(), "item-6", "plan-inbox-1").await.unwrap();
    let link = get_plan_link(db.pool(), "item-6").await.unwrap().unwrap();
    assert_eq!(link.plan_id, "plan-inbox-1");
}

#[tokio::test]
async fn duplicate_plan_link_fails() {
    let db = test_db().await;
    insert_inbox_item(db.pool(), &sample_item("item-7")).await.unwrap();

    let plan_insert = persistence_plans::repositories::plans::InsertPlan {
        id: "plan-inbox-2",
        title: "Inbox Split 2",
        origin: "inbox",
        origin_path: None,
        plan_type: "split",
        destructive_destination: "archive",
        parent_plan_id: None,
        total_bytes_required: 0,
    };
    persistence_plans::repositories::plans::insert_plan(db.pool(), &plan_insert).await.unwrap();

    insert_plan_link(db.pool(), "item-7", "plan-inbox-2").await.unwrap();
    // Second insert must fail (PK constraint)
    let err = insert_plan_link(db.pool(), "item-7", "plan-inbox-2").await;
    assert!(err.is_err());
}

/// C1 integration test (no mocks): register a real source via
/// `register_source_batch`, insert an inbox item for that source's id, then
/// call `list_unacknowledged_across_roots` and assert the row comes back
/// with the correct `root_path`. Verifies the JOIN hits `registered_sources`
/// not the absent `library_root` table.
#[tokio::test]
async fn list_unacknowledged_joins_registered_sources() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    // Register a source via the real batch function (same path the wizard uses).
    let batch_req = RegisterSourceBatchRequest {
        sources: vec![RegisterSourceRequest {
            kind: SourceKind::Inbox,
            path: "/astro/inbox".to_owned(),
            kind_subtype: None,
            scan_depth: ScanDepth::Recursive,
            organization_state: OrganizationState::Unorganized,
        }],
    };
    let batch_resp =
        persistence_lifecycle::repositories::first_run::register_source_batch(pool, &batch_req)
            .await
            .unwrap();
    let source_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();

    // Insert an inbox item pointing at that source id.
    let item = InsertInboxItem {
        id: "cross-root-item-1",
        root_id: &source_id,
        relative_path: "2025-11-01/lights",
        file_count: 5,
        content_signature: Some("sig-cross"),
        lane: "fits",
    };
    insert_inbox_item(pool, &item).await.unwrap();

    // Must return ≥1 row with the correct root_path.
    let rows = list_unacknowledged_across_roots(pool, 100).await.unwrap();
    assert_eq!(rows.len(), 1, "expected 1 unacknowledged item");
    assert_eq!(rows[0].root_path, "/astro/inbox", "root_path must match registered_sources.path");
    assert_eq!(rows[0].id, "cross-root-item-1");
    assert_eq!(rows[0].state, "pending_classification");
    assert_eq!(
        rows[0].organization_state, "unorganized",
        "org-state must be carried from registered_sources (inbox ⇒ unorganized)"
    );
}

/// T012 / T042, SC-002b (spec 058): a scanned-but-unclassified folder is
/// exactly ONE row, and that row is not an inbox item.
///
/// This is the property the whole feature turns on. Scan writes the source
/// group and no item (FR-015); `list_unclassified_source_groups` surfaces
/// it precisely because no item references it. While scan still wrote a
/// folder placeholder, that `NOT EXISTS` clause was false for every scanned
/// folder, so this query returned nothing and the source-group row was
/// unreachable in practice.
///
/// The second half is the two-direction control and is the real point:
/// linking a single item to the group makes the group row VANISH. That
/// proves the placeholder is what was hiding it, rather than asserting the
/// happy path and inferring the cause.
///
/// Worth recording why this test exists at Layer 1 at all: when the
/// scan-time placeholder was deleted, the entire workspace suite stayed
/// green (2528/2528). `inbox_scan_folder`'s placeholder behaviour had no
/// Layer-1 coverage whatsoever — only the `#[ignore]`d L3 journeys touched
/// it. A silent green is exactly how this change could have shipped broken.
#[tokio::test]
async fn scanned_folder_is_one_source_group_row_and_no_item() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    let batch_req = RegisterSourceBatchRequest {
        sources: vec![RegisterSourceRequest {
            kind: SourceKind::Inbox,
            path: "/astro/inbox".to_owned(),
            kind_subtype: None,
            scan_depth: ScanDepth::Recursive,
            organization_state: OrganizationState::Unorganized,
        }],
    };
    let batch_resp =
        persistence_lifecycle::repositories::first_run::register_source_batch(pool, &batch_req)
            .await
            .unwrap();
    let source_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();

    // Exactly what scan now writes for a folder: the group, and nothing else.
    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-scanned",
            root_id: &source_id,
            relative_path: "2025-11-01/lights",
            content_signature: Some("sig-scanned"),
            format: Some("fits"),
            lane: Some("move"),
            file_count: 4,
        },
    )
    .await
    .unwrap();

    let groups = list_unclassified_source_groups(pool, 100).await.unwrap();
    assert_eq!(groups.len(), 1, "a scanned folder must be exactly one row");
    assert_eq!(groups[0].id, "sg-scanned");

    // ...and that row is NOT an inbox item. Nothing is confirmable yet.
    let items = list_unacknowledged_across_roots(pool, 100).await.unwrap();
    assert!(
        items.is_empty(),
        "a scanned-but-unclassified folder must produce no inbox item; got {:?}",
        items.iter().map(|i| &i.id).collect::<Vec<_>>()
    );

    // Two-direction control: give the group an item — as the deleted
    // scan-time placeholder did — and the group row disappears.
    upsert_inbox_sub_item(
        pool,
        &UpsertInboxSubItem {
            id: "item-materialized",
            root_id: &source_id,
            relative_path: "2025-11-01/lights",
            source_group_id: "sg-scanned",
            group_key: "type=light",
            group_label: "(root) · lights",
            frame_type: Some("light"),
            content_signature: "sig-sub",
            file_count: 4,
            lane: "fits",
            needs_review: false,
        },
    )
    .await
    .unwrap();

    let groups_after = list_unclassified_source_groups(pool, 100).await.unwrap();
    assert!(
        groups_after.is_empty(),
        "once the folder has an item row the source-group row must not \
         also be listed — that double-count is the contradiction FR-001 removes"
    );
}

/// FR-015 master carve-out: a folder whose files are ALL detected masters
/// has no sub-frames left to classify, so it must NOT surface a
/// source-group row. Scan records `file_count = 0` for exactly this case,
/// and the `file_count > 0` predicate is what keeps the queue honest.
#[tokio::test]
async fn masters_only_folder_surfaces_no_source_group_row() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    let batch_req = RegisterSourceBatchRequest {
        sources: vec![RegisterSourceRequest {
            kind: SourceKind::Inbox,
            path: "/astro/inbox".to_owned(),
            kind_subtype: None,
            scan_depth: ScanDepth::Recursive,
            organization_state: OrganizationState::Unorganized,
        }],
    };
    let batch_resp =
        persistence_lifecycle::repositories::first_run::register_source_batch(pool, &batch_req)
            .await
            .unwrap();
    let source_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();

    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-masters-only",
            root_id: &source_id,
            relative_path: "calibration/masters",
            content_signature: Some("sig-masters"),
            format: Some("fits"),
            lane: Some("move"),
            file_count: 0,
        },
    )
    .await
    .unwrap();

    let groups = list_unclassified_source_groups(pool, 100).await.unwrap();
    assert!(
        groups.is_empty(),
        "a masters-only folder has nothing left to classify and must not \
         appear as an unclassified source group"
    );
}

/// T034 / CHK016 (spec 058): siblings of one folder come back in a stable,
/// deterministic order.
///
/// Siblings materialised from one folder share a root path AND a relative
/// path, so `ORDER BY r.path, i.relative_path` ties on *both* keys and
/// SQLite may return them in any order — the Inbox list could reorder under
/// the user between two identical queries. `group_key` is the only column
/// that distinguishes siblings.
///
/// The fixture inserts the three siblings in *descending* `group_key` order
/// so insertion order (rowid) is the exact reverse of the expected result.
/// A query that has lost the tiebreak therefore cannot pass by coincidence.
///
/// Two-direction control (recorded 2026-07-20): dropping `, i.group_key`
/// from the ORDER BY fails this test with
/// `["type=light", "type=dark", "type=bias"]` — i.e. rowid order, the exact
/// non-determinism CHK016 describes. Restoring it passes.
#[tokio::test]
async fn list_unacknowledged_orders_siblings_by_group_key() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    let batch_req = RegisterSourceBatchRequest {
        sources: vec![RegisterSourceRequest {
            kind: SourceKind::Inbox,
            path: "/astro/inbox".to_owned(),
            kind_subtype: None,
            scan_depth: ScanDepth::Recursive,
            organization_state: OrganizationState::Unorganized,
        }],
    };
    let batch_resp =
        persistence_lifecycle::repositories::first_run::register_source_batch(pool, &batch_req)
            .await
            .unwrap();
    let source_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();

    // The siblings' owning source group must exist — `source_group_id` is a
    // real FK, so this is not optional scaffolding.
    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-siblings",
            root_id: &source_id,
            relative_path: "2025-11-01/session",
            content_signature: Some("sig-sg"),
            format: Some("fits"),
            lane: Some("move"),
            file_count: 3,
        },
    )
    .await
    .unwrap();

    // Deliberately inserted in reverse of the expected order.
    for (id, key, ft) in [
        ("sib-light", "type=light", "light"),
        ("sib-dark", "type=dark", "dark"),
        ("sib-bias", "type=bias", "bias"),
    ] {
        upsert_inbox_sub_item(
            pool,
            &UpsertInboxSubItem {
                id,
                root_id: &source_id,
                relative_path: "2025-11-01/session",
                source_group_id: "sg-siblings",
                group_key: key,
                group_label: "(root) · session",
                frame_type: Some(ft),
                content_signature: "sig-sib",
                file_count: 1,
                lane: "fits",
                needs_review: false,
            },
        )
        .await
        .unwrap();
    }

    let rows = list_unacknowledged_across_roots(pool, 100).await.unwrap();
    let keys: Vec<&str> = rows.iter().map(|r| r.group_key.as_str()).collect();

    assert_eq!(
        keys,
        vec!["type=bias", "type=dark", "type=light"],
        "siblings of one folder must order by group_key; got insertion order instead, \
         so the Inbox list can reorder under the user between identical queries"
    );
}

/// Spec 041 regression: the inbox list must carry each item's owning source
/// organization_state (not a hardcoded "unorganized"), so the grouping
/// "Org. state" dimension is correct for organized library roots too.
#[tokio::test]
async fn list_unacknowledged_carries_real_organization_state() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    // Two sources: an unorganized inbox and an organized light-frames library,
    // each registered via the real batch path the wizard uses.
    let batch_req = RegisterSourceBatchRequest {
        sources: vec![
            RegisterSourceRequest {
                kind: SourceKind::Inbox,
                path: "/astro/inbox".to_owned(),
                kind_subtype: None,
                scan_depth: ScanDepth::Recursive,
                organization_state: OrganizationState::Unorganized,
            },
            RegisterSourceRequest {
                kind: SourceKind::LightFrames,
                path: "/astro/library".to_owned(),
                kind_subtype: None,
                scan_depth: ScanDepth::Recursive,
                organization_state: OrganizationState::Organized,
            },
        ],
    };
    let batch_resp =
        persistence_lifecycle::repositories::first_run::register_source_batch(pool, &batch_req)
            .await
            .unwrap();
    let inbox_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();
    let library_id = batch_resp.items[1].source_id.as_deref().unwrap().to_owned();

    insert_inbox_item(
        pool,
        &InsertInboxItem {
            id: "org-item-inbox",
            root_id: &inbox_id,
            relative_path: "2025-11-01/lights",
            file_count: 3,
            content_signature: Some("sig-inbox"),
            lane: "fits",
        },
    )
    .await
    .unwrap();
    insert_inbox_item(
        pool,
        &InsertInboxItem {
            id: "org-item-library",
            root_id: &library_id,
            relative_path: "M31/lights",
            file_count: 7,
            content_signature: Some("sig-library"),
            lane: "fits",
        },
    )
    .await
    .unwrap();

    let rows = list_unacknowledged_across_roots(pool, 100).await.unwrap();
    let by_id: std::collections::HashMap<&str, &InboxListRow> =
        rows.iter().map(|r| (r.id.as_str(), r)).collect();

    assert_eq!(by_id.get("org-item-inbox").unwrap().organization_state, "unorganized");
    assert_eq!(
        by_id.get("org-item-library").unwrap().organization_state,
        "organized",
        "organized library source must surface as 'organized' in the list"
    );
}

/// Spec 058 FR-028/FR-029 (T005, T011): `needs_review` is its own column,
/// and resolving an item out of needs-review moves the flag, the frame
/// type and the state in ONE statement (FR-029).
///
/// SC-003 closed here at T018: the state is now derived from whether the
/// row carries a frame type, so an UNRESOLVED needs-review row reports
/// `pending_classification` rather than claiming to be classified with a
/// NULL `frame_type`.
///
/// The last assertion is the one that matters for T006: the resolve
/// converges onto the item's NATURAL classification key via `ON CONFLICT`,
/// leaving exactly one row. That is why `clear_needs_review_sentinel`'s
/// synthetic `type=<ft>·resolved=<item-id>` key can be deleted rather than
/// replaced — it existed only to dodge the UNIQUE constraint that this
/// convergence is supposed to enforce.
#[tokio::test]
async fn needs_review_resolves_atomically_onto_its_natural_key_058() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    let batch_resp = persistence_lifecycle::repositories::first_run::register_source_batch(
        pool,
        &RegisterSourceBatchRequest {
            sources: vec![RegisterSourceRequest {
                kind: SourceKind::Inbox,
                path: "/astro/inbox".to_owned(),
                kind_subtype: None,
                scan_depth: ScanDepth::Recursive,
                organization_state: OrganizationState::Unorganized,
            }],
        },
    )
    .await
    .unwrap();
    let source_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();

    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-nr",
            root_id: &source_id,
            relative_path: "lights",
            content_signature: Some("sig"),
            format: Some("fits"),
            lane: Some("move"),
            file_count: 1,
        },
    )
    .await
    .unwrap();

    // An unresolved needs-review item. Its group_key is already the
    // classification identity it will eventually resolve to; needs_review
    // carries the review status separately (FR-028).
    let unresolved = UpsertInboxSubItem {
        id: "sub-nr",
        root_id: &source_id,
        relative_path: "lights",
        source_group_id: "sg-nr",
        group_key: "type=light",
        group_label: "(root) · light",
        frame_type: None,
        content_signature: "sig-sub",
        file_count: 1,
        lane: "fits",
        needs_review: true,
    };
    let first_id = upsert_inbox_sub_item(pool, &unresolved).await.unwrap();

    let (nr, ft, state): (i64, Option<String>, String) =
        sqlx::query_as("SELECT needs_review, frame_type, state FROM inbox_items WHERE id = ?")
            .bind(&first_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(nr, 1, "needs_review must persist as its own field, not as a group_key value");
    assert_eq!(ft, None, "an unresolved needs-review item carries no frame type");
    assert_eq!(
        state, "pending_classification",
        "SC-003: a row with no frame type must not report `classified` (T018)"
    );

    // The user supplies the missing attributes; re-materialization writes
    // the resolved item with a DIFFERENT freshly-generated id but the SAME
    // natural key.
    let resolved = UpsertInboxSubItem {
        id: "sub-nr-regenerated",
        frame_type: Some("light"),
        needs_review: false,
        ..unresolved.clone()
    };
    let persisted_id = upsert_inbox_sub_item(pool, &resolved).await.unwrap();

    assert_eq!(
        persisted_id, first_id,
        "ON CONFLICT must converge onto the pre-existing row — two rows sharing a \
         classification identity in one folder ARE the same item (T006)"
    );

    let (nr, ft, state): (i64, Option<String>, String) =
        sqlx::query_as("SELECT needs_review, frame_type, state FROM inbox_items WHERE id = ?")
            .bind(&persisted_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(nr, 0, "resolving must clear needs_review");
    assert_eq!(ft.as_deref(), Some("light"), "resolving must record the frame type");
    assert_eq!(state, "classified", "resolving must record the classified state");

    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM inbox_items WHERE root_id = ? AND relative_path = 'lights'",
    )
    .bind(&source_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        rows, 1,
        "the resolve must not leave a second row behind — no synthetic \
         `resolved=<id>` discriminator is needed to avoid the UNIQUE constraint"
    );
}

// ── grouping_keys_for_items (spec 041 multi-level grouping) ───────────────

/// Helper: upsert one metadata row with the common header fields set.
#[allow(clippy::too_many_arguments)]
async fn meta_row(
    pool: &SqlitePool,
    item: &str,
    path: &str,
    object: Option<&str>,
    date_obs: Option<&str>,
    filter: Option<&str>,
    exposure_s: Option<f64>,
    instrume: Option<&str>,
) {
    let m = UpsertFileMetadata {
        inbox_item_id: item,
        relative_file_path: path,
        object,
        date_obs,
        filter,
        exposure_s,
        instrume,
        ..Default::default()
    };
    upsert_inbox_file_metadata(pool, &m).await.unwrap();
}

#[tokio::test]
async fn grouping_uniform_metadata_yields_single_values() {
    let db = test_db().await;
    let pool = db.pool();
    insert_inbox_item(pool, &sample_item("g-uniform")).await.unwrap();

    // Two files agree on every dimension; date_obs carries a full timestamp.
    meta_row(
        pool,
        "g-uniform",
        "a.fits",
        Some("M31"),
        Some("2025-10-10T22:01:00"),
        Some("Ha"),
        Some(300.0),
        Some("ASI2600"),
    )
    .await;
    meta_row(
        pool,
        "g-uniform",
        "b.fits",
        Some("M31"),
        Some("2025-10-10T23:59:00"),
        Some("Ha"),
        Some(300.0),
        Some("ASI2600"),
    )
    .await;

    let keys = grouping_keys_for_items(pool, &["g-uniform".to_owned()]).await.unwrap();
    let g = keys.get("g-uniform").expect("item present");
    assert_eq!(g.group_target.as_deref(), Some("M31"));
    // Same calendar day despite differing timestamps -> single date label.
    assert_eq!(g.group_date.as_deref(), Some("2025-10-10"));
    assert_eq!(g.group_filter.as_deref(), Some("Ha"));
    // 300.0 trims to "300s".
    assert_eq!(g.group_exposure.as_deref(), Some("300s"));
    assert_eq!(g.group_instrument.as_deref(), Some("ASI2600"));
}

#[tokio::test]
async fn grouping_divergent_metadata_yields_mixed() {
    let db = test_db().await;
    let pool = db.pool();
    insert_inbox_item(pool, &sample_item("g-mixed")).await.unwrap();

    meta_row(
        pool,
        "g-mixed",
        "a.fits",
        Some("M31"),
        Some("2025-10-10T22:00:00"),
        Some("Ha"),
        Some(300.0),
        Some("ASI2600"),
    )
    .await;
    meta_row(
        pool,
        "g-mixed",
        "b.fits",
        Some("NGC7000"),
        Some("2025-10-11T22:00:00"),
        Some("OIII"),
        Some(120.0),
        Some("ASI1600"),
    )
    .await;

    let keys = grouping_keys_for_items(pool, &["g-mixed".to_owned()]).await.unwrap();
    let g = keys.get("g-mixed").unwrap();
    assert_eq!(g.group_target.as_deref(), Some("Mixed"));
    assert_eq!(g.group_date.as_deref(), Some("Mixed"));
    assert_eq!(g.group_filter.as_deref(), Some("Mixed"));
    assert_eq!(g.group_exposure.as_deref(), Some("Mixed"));
    assert_eq!(g.group_instrument.as_deref(), Some("Mixed"));
}

#[tokio::test]
async fn grouping_absent_metadata_yields_none() {
    let db = test_db().await;
    let pool = db.pool();
    insert_inbox_item(pool, &sample_item("g-empty")).await.unwrap();

    // No metadata, no evidence rows at all.
    let keys = grouping_keys_for_items(pool, &["g-empty".to_owned()]).await.unwrap();
    // Either absent from the map or present with all-None — both default to None.
    let g = keys.get("g-empty").cloned().unwrap_or_default();
    assert_eq!(g.group_target, None);
    assert_eq!(g.group_frame_type, None);
    assert_eq!(g.group_date, None);
    assert_eq!(g.group_filter, None);
    assert_eq!(g.group_exposure, None);
    assert_eq!(g.group_instrument, None);
}

#[tokio::test]
async fn grouping_partial_nulls_count_as_distinct_non_null() {
    let db = test_db().await;
    let pool = db.pool();
    insert_inbox_item(pool, &sample_item("g-partial")).await.unwrap();

    // One file has a filter, the other is null -> 1 distinct non-null value.
    meta_row(pool, "g-partial", "a.fits", None, None, Some("Lum"), None, None).await;
    meta_row(pool, "g-partial", "b.fits", None, None, None, None, None).await;

    let keys = grouping_keys_for_items(pool, &["g-partial".to_owned()]).await.unwrap();
    let g = keys.get("g-partial").unwrap();
    assert_eq!(g.group_filter.as_deref(), Some("Lum"));
    assert_eq!(g.group_target, None);
    assert_eq!(g.group_exposure, None);
}

#[tokio::test]
async fn grouping_exposure_fractional_label() {
    let db = test_db().await;
    let pool = db.pool();
    insert_inbox_item(pool, &sample_item("g-frac")).await.unwrap();

    meta_row(pool, "g-frac", "a.fits", None, None, None, Some(1.5), None).await;

    let keys = grouping_keys_for_items(pool, &["g-frac".to_owned()]).await.unwrap();
    let g = keys.get("g-frac").unwrap();
    assert_eq!(g.group_exposure.as_deref(), Some("1.5s"));
}

#[tokio::test]
async fn grouping_dominant_frame_type_from_evidence() {
    let db = test_db().await;
    let pool = db.pool();
    insert_inbox_item(pool, &sample_item("g-dom")).await.unwrap();

    // 3 darks vs 1 light -> dominant = "dark" (NOT "Mixed").
    for (i, ft) in [("e1", "dark"), ("e2", "dark"), ("e3", "dark"), ("e4", "light")] {
        let path = format!("{i}.fits");
        let ev = InsertEvidence {
            id: i,
            inbox_item_id: "g-dom",
            relative_file_path: &path,
            frame_type: Some(ft),
            evidence_source: "imagetyp_header",
            raw_value: Some(ft),
            unclassified: false,
            manual_override: None,
            is_master: false,
            master_detector: None,
        };
        insert_evidence(pool, &ev).await.unwrap();
    }

    let keys = grouping_keys_for_items(pool, &["g-dom".to_owned()]).await.unwrap();
    let g = keys.get("g-dom").unwrap();
    assert_eq!(g.group_frame_type.as_deref(), Some("dark"));
}

#[tokio::test]
async fn grouping_dominant_frame_type_respects_manual_override() {
    let db = test_db().await;
    let pool = db.pool();
    insert_inbox_item(pool, &sample_item("g-ovr")).await.unwrap();

    // Two files extracted as light, but both overridden to flat -> dominant flat.
    for (i, ft) in [("o1", "light"), ("o2", "light")] {
        let path = format!("{i}.fits");
        let ev = InsertEvidence {
            id: i,
            inbox_item_id: "g-ovr",
            relative_file_path: &path,
            frame_type: Some(ft),
            evidence_source: "imagetyp_header",
            raw_value: Some(ft),
            unclassified: false,
            manual_override: Some("flat"),
            is_master: false,
            master_detector: None,
        };
        insert_evidence(pool, &ev).await.unwrap();
    }

    let keys = grouping_keys_for_items(pool, &["g-ovr".to_owned()]).await.unwrap();
    assert_eq!(keys.get("g-ovr").unwrap().group_frame_type.as_deref(), Some("flat"));
}

/// Issue #711 Instance A (unsplit-folder variant): `grouping_keys_for_items`
/// must surface the item's own cached `inbox_classifications.result` so the
/// list badge can distinguish a genuinely-unclassified unsplit folder from
/// one that resolved to a single type, instead of trusting
/// `inbox_items.state` (which `classify()` sets to `"classified"`
/// unconditionally regardless of the actual result).
#[tokio::test]
async fn grouping_surfaces_cached_classification_result() {
    let db = test_db().await;
    let pool = db.pool();
    insert_inbox_item(pool, &sample_item_at("g-cls-unclassified", "folder-a")).await.unwrap();
    upsert_classification(
        pool,
        &UpsertClassification {
            inbox_item_id: "g-cls-unclassified",
            result: "unclassified",
            frame_type: None,
            content_signature: "sig",
            unclassified_file_count: 2,
        },
    )
    .await
    .unwrap();

    insert_inbox_item(pool, &sample_item_at("g-cls-classified", "folder-b")).await.unwrap();
    upsert_classification(
        pool,
        &UpsertClassification {
            inbox_item_id: "g-cls-classified",
            result: "classified",
            frame_type: Some("dark"),
            content_signature: "sig",
            unclassified_file_count: 0,
        },
    )
    .await
    .unwrap();

    // Never classified — no inbox_classifications row at all.
    insert_inbox_item(pool, &sample_item_at("g-cls-never", "folder-c")).await.unwrap();

    let keys = grouping_keys_for_items(
        pool,
        &["g-cls-unclassified".to_owned(), "g-cls-classified".to_owned(), "g-cls-never".to_owned()],
    )
    .await
    .unwrap();

    assert_eq!(
        keys.get("g-cls-unclassified").unwrap().classification_result.as_deref(),
        Some("unclassified")
    );
    assert_eq!(
        keys.get("g-cls-classified").unwrap().classification_result.as_deref(),
        Some("classified")
    );
    assert_eq!(keys.get("g-cls-never").cloned().unwrap_or_default().classification_result, None);
}

#[tokio::test]
async fn grouping_empty_ids_returns_empty_map() {
    let db = test_db().await;
    let pool = db.pool();
    let keys = grouping_keys_for_items(pool, &[]).await.unwrap();
    assert!(keys.is_empty());
}

/// set_overrides writes the frame-type override and resets override_stale.
///
/// NOTE (migration 0048): override_filter/override_exposure_s/override_binning
/// have been moved to inbox_file_overrides. set_overrides now only updates
/// manual_override (frame-type correction) on the evidence row. Non-type
/// override parameters (_filter, _exposure_s, _binning) are accepted but
/// silently ignored until T069 rewrites the override persistence layer.
#[tokio::test]
async fn set_overrides_writes_all_columns_and_resets_stale() {
    let db = test_db().await;
    let pool = db.pool();

    // Set up: source group + item + evidence row.
    // An inbox_source_groups row is required so set_overrides can write
    // non-type values to inbox_file_overrides (migration 0048 data path).
    sqlx::query(
        "INSERT INTO inbox_source_groups \
         (id, root_id, relative_path, discovered_at, last_scanned_at, child_count) \
         VALUES ('sg-overrides-1', 'root-1', '2025-10-10/lights', \
                 '2025-10-10T20:00:00Z', '2025-10-10T20:00:00Z', 1)",
    )
    .execute(pool)
    .await
    .unwrap();

    // Insert the inbox_item with source_group_id set.
    sqlx::query(
        "INSERT INTO inbox_items \
         (id, root_id, relative_path, source_group_id, group_key, \
          discovered_at, last_scanned_at, state, lane) \
         VALUES ('item-overrides-1', 'root-1', '2025-10-10/lights', \
                 'sg-overrides-1', '', \
                 '2025-10-10T20:00:00Z', '2025-10-10T20:00:00Z', \
                 'pending_classification', 'fits')",
    )
    .execute(pool)
    .await
    .unwrap();

    insert_evidence(
        pool,
        &InsertEvidence {
            id: "ev-overrides-1",
            inbox_item_id: "item-overrides-1",
            relative_file_path: "folder/file.fits",
            frame_type: None,
            evidence_source: "none",
            raw_value: None,
            unclassified: true,
            manual_override: None,
            is_master: false,
            master_detector: None,
        },
    )
    .await
    .unwrap();

    // First manually mark stale so we can verify it is reset.
    mark_override_stale(pool, "item-overrides-1", "folder/file.fits").await.unwrap();

    // Apply full overrides — now actually writes non-type values to
    // inbox_file_overrides and frame-type to the evidence row.
    let updated = set_overrides(
        pool,
        "item-overrides-1",
        "folder/file.fits",
        Some("dark"),
        Some("Ha"),
        Some(120.0),
        Some("2x2"),
    )
    .await
    .unwrap();
    assert!(updated, "set_overrides must return true (row found)");

    // Read back via list_evidence — override values are JOIN'd from
    // inbox_file_overrides by the updated query.
    let rows = list_evidence(pool, "item-overrides-1").await.unwrap();
    assert_eq!(rows.len(), 1);
    let ev = &rows[0];
    assert_eq!(ev.manual_override.as_deref(), Some("dark"));
    assert_eq!(ev.override_stale, 0, "freshly-set override must not be stale");
    assert_eq!(ev.evidence_source, "manual_override");
    // Non-type overrides are read back from inbox_file_overrides via the JOIN.
    assert_eq!(ev.override_filter.as_deref(), Some("Ha"));
    assert_eq!(ev.override_exposure_s, Some(120.0));
    assert_eq!(ev.override_binning.as_deref(), Some("2x2"));
}

/// mark_override_stale sets override_stale=1.
#[tokio::test]
async fn mark_override_stale_sets_flag() {
    let db = test_db().await;
    let pool = db.pool();

    insert_inbox_item(pool, &sample_item("item-stale-1")).await.unwrap();
    insert_evidence(
        pool,
        &InsertEvidence {
            id: "ev-stale-1",
            inbox_item_id: "item-stale-1",
            relative_file_path: "folder/stale.fits",
            frame_type: None,
            evidence_source: "none",
            raw_value: None,
            unclassified: true,
            manual_override: None,
            is_master: false,
            master_detector: None,
        },
    )
    .await
    .unwrap();

    // Initially stale=0 (DEFAULT).
    let rows_before = list_evidence(pool, "item-stale-1").await.unwrap();
    assert_eq!(rows_before[0].override_stale, 0);

    mark_override_stale(pool, "item-stale-1", "folder/stale.fits").await.unwrap();

    let rows_after = list_evidence(pool, "item-stale-1").await.unwrap();
    assert_eq!(rows_after[0].override_stale, 1, "override_stale must be 1 after mark");
}

/// spec 041 FR-046: `set_file_override` round-trips the file identity
/// (size/mtime) through `list_file_overrides_for_group`, and
/// `mark_file_override_stale` flips `override_stale` on every property row
/// for that path without touching other files' rows.
#[tokio::test]
async fn mark_file_override_stale_sets_flag_for_path_only() {
    let db = test_db().await;
    let pool = db.pool();

    // inbox_file_overrides.source_group_id is FK-constrained to
    // inbox_source_groups(id).
    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-fovr-1",
            root_id: "root-1",
            relative_path: "folder",
            content_signature: None,
            format: Some("fits"),
            lane: Some("move"),
            file_count: 1,
        },
    )
    .await
    .unwrap();

    set_file_override(
        pool,
        "sg-fovr-1",
        "folder/a.fits",
        "temperatureC",
        "-10.0",
        Some(4_194_304),
        Some("2025-10-10T22:00:00Z"),
    )
    .await
    .unwrap();
    set_file_override(
        pool,
        "sg-fovr-1",
        "folder/a.fits",
        "gain",
        "100",
        Some(4_194_304),
        Some("2025-10-10T22:00:00Z"),
    )
    .await
    .unwrap();
    set_file_override(pool, "sg-fovr-1", "folder/b.fits", "gain", "200", None, None).await.unwrap();

    let before = list_file_overrides_for_group(pool, "sg-fovr-1").await.unwrap();
    let a_temp = before
        .iter()
        .find(|o| o.relative_file_path == "folder/a.fits" && o.property_key == "temperatureC")
        .unwrap();
    assert_eq!(a_temp.file_size_bytes, Some(4_194_304), "identity must round-trip");
    assert_eq!(a_temp.file_mtime.as_deref(), Some("2025-10-10T22:00:00Z"));
    assert_eq!(a_temp.override_stale, 0);

    mark_file_override_stale(pool, "sg-fovr-1", "folder/a.fits").await.unwrap();

    let after = list_file_overrides_for_group(pool, "sg-fovr-1").await.unwrap();
    for o in after.iter().filter(|o| o.relative_file_path == "folder/a.fits") {
        assert_eq!(o.override_stale, 1, "every property row for the stale path must flip");
    }
    let b = after.iter().find(|o| o.relative_file_path == "folder/b.fits").unwrap();
    assert_eq!(b.override_stale, 0, "unrelated file must be untouched");
}

/// #1294: `list_evidence` must join the `target` override the same way it
/// already joins filter/exposureS/binning/frameType — otherwise a target
/// override (written by `cone_search::confirm`) is a write nobody reads
/// back, and the mandatory-attribute gate in `app_core_inbox::metadata`
/// (which reads `list_evidence` rows) never sees it.
#[tokio::test]
async fn list_evidence_joins_target_override() {
    let db = test_db().await;
    let pool = db.pool();

    // inbox_file_overrides.source_group_id is FK-constrained to
    // inbox_source_groups(id); the evidence row's item must link to it.
    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-target-1",
            root_id: "root-1",
            relative_path: "folder",
            content_signature: None,
            format: Some("fits"),
            lane: Some("move"),
            // Spec 058 T013 added this field after #1294 was written. The
            // group is only an FK anchor for the evidence row here, so the
            // count is never asserted; 1 matches the single item below.
            file_count: 1,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inbox_items \
         (id, root_id, relative_path, source_group_id, group_key, \
          discovered_at, last_scanned_at, state, lane) \
         VALUES ('item-target-1', 'root-1', 'folder', 'sg-target-1', '', \
                 '2025-10-10T20:00:00Z', '2025-10-10T20:00:00Z', \
                 'pending_classification', 'fits')",
    )
    .execute(pool)
    .await
    .unwrap();
    insert_evidence(
        pool,
        &InsertEvidence {
            id: "ev-target-1",
            inbox_item_id: "item-target-1",
            relative_file_path: "folder/light.fits",
            frame_type: Some("light"),
            evidence_source: "imagetyp_header",
            raw_value: Some("Light Frame"),
            unclassified: false,
            manual_override: None,
            is_master: false,
            master_detector: None,
        },
    )
    .await
    .unwrap();

    let before = list_evidence(pool, "item-target-1").await.unwrap();
    assert_eq!(before[0].override_target, None, "no override set yet");

    set_file_override(pool, "sg-target-1", "folder/light.fits", "target", "M 31", None, None)
        .await
        .unwrap();

    let after = list_evidence(pool, "item-target-1").await.unwrap();
    assert_eq!(after[0].override_target.as_deref(), Some("M 31"));
}

/// get_file_metadata returns None before any classify and Some after upsert.
#[tokio::test]
async fn get_file_metadata_returns_row_after_upsert() {
    let db = test_db().await;
    let pool = db.pool();

    insert_inbox_item(pool, &sample_item("item-getmeta-1")).await.unwrap();

    // Before upsert: None.
    let before = get_file_metadata(pool, "item-getmeta-1", "folder/light.fits").await.unwrap();
    assert!(before.is_none());

    // Upsert metadata.
    upsert_inbox_file_metadata(
        pool,
        &UpsertFileMetadata {
            inbox_item_id: "item-getmeta-1",
            relative_file_path: "folder/light.fits",
            filter: Some("Ha"),
            exposure_s: Some(300.0),
            file_size_bytes: Some(4_194_304),
            file_mtime: Some("2025-10-10T22:00:00Z"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // After upsert: row present.
    let after =
        get_file_metadata(pool, "item-getmeta-1", "folder/light.fits").await.unwrap().unwrap();
    assert_eq!(after.filter.as_deref(), Some("Ha"));
    assert_eq!(after.exposure_s, Some(300.0));
    assert_eq!(after.file_size_bytes, Some(4_194_304));
}

/// T040 — `inbox_stats` returns per-type counts across active items.
///
/// Fixture:
///   item-stats-1  (state=classified):  2 light frames (is_master=0)
///   item-stats-2  (state=classified):  1 dark frame  (is_master=0)
///   item-stats-3  (state=classified):  1 dark master (is_master=1)
///
/// Expected stats:
///   light → folder_count=1, image_count=2, master_count=0
///   dark  → folder_count=2, image_count=1, master_count=1
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn inbox_stats_returns_per_type_counts() {
    let db = test_db().await;
    let pool = db.pool();

    // item-stats-1: two light frames
    insert_inbox_item(
        pool,
        &InsertInboxItem {
            id: "item-stats-1",
            root_id: "root-1",
            relative_path: "2025-10-10/lights-stats",
            file_count: 2,
            content_signature: Some("sig-s1"),
            lane: "fits",
        },
    )
    .await
    .unwrap();
    update_inbox_item_state(pool, "item-stats-1", "classified").await.unwrap();
    for (ev_id, path) in [
        ("ev-stats-1a", "lights-stats/frame_001.fits"),
        ("ev-stats-1b", "lights-stats/frame_002.fits"),
    ] {
        insert_evidence(
            pool,
            &InsertEvidence {
                id: ev_id,
                inbox_item_id: "item-stats-1",
                relative_file_path: path,
                frame_type: Some("light"),
                evidence_source: "imagetyp_header",
                raw_value: Some("Light Frame"),
                unclassified: false,
                manual_override: None,
                is_master: false,
                master_detector: None,
            },
        )
        .await
        .unwrap();
    }

    // item-stats-2: one dark frame
    insert_inbox_item(
        pool,
        &InsertInboxItem {
            id: "item-stats-2",
            root_id: "root-1",
            relative_path: "2025-10-10/darks-stats",
            file_count: 1,
            content_signature: Some("sig-s2"),
            lane: "fits",
        },
    )
    .await
    .unwrap();
    update_inbox_item_state(pool, "item-stats-2", "classified").await.unwrap();
    insert_evidence(
        pool,
        &InsertEvidence {
            id: "ev-stats-2",
            inbox_item_id: "item-stats-2",
            relative_file_path: "darks-stats/dark_001.fits",
            frame_type: Some("dark"),
            evidence_source: "imagetyp_header",
            raw_value: Some("Dark Frame"),
            unclassified: false,
            manual_override: None,
            is_master: false,
            master_detector: None,
        },
    )
    .await
    .unwrap();

    // item-stats-3: one dark master (is_master=true)
    insert_inbox_item(
        pool,
        &InsertInboxItem {
            id: "item-stats-3",
            root_id: "root-1",
            relative_path: "2025-10-10/dark-masters-stats",
            file_count: 1,
            content_signature: Some("sig-s3"),
            lane: "fits",
        },
    )
    .await
    .unwrap();
    update_inbox_item_state(pool, "item-stats-3", "classified").await.unwrap();
    insert_evidence(
        pool,
        &InsertEvidence {
            id: "ev-stats-3",
            inbox_item_id: "item-stats-3",
            relative_file_path: "dark-masters-stats/master_dark.fits",
            frame_type: Some("dark"),
            evidence_source: "imagetyp_header",
            raw_value: Some("Dark Frame"),
            unclassified: false,
            manual_override: None,
            is_master: true,
            master_detector: None,
        },
    )
    .await
    .unwrap();

    let rows = inbox_stats(pool).await.unwrap();

    let light = rows.iter().find(|r| r.frame_type == "light").unwrap();
    assert_eq!(light.image_count, 2, "light image_count");
    assert_eq!(light.master_count, 0, "light master_count");
    assert_eq!(light.folder_count, 1, "light folder_count");

    let dark = rows.iter().find(|r| r.frame_type == "dark").unwrap();
    assert_eq!(dark.image_count, 1, "dark image_count");
    assert_eq!(dark.master_count, 1, "dark master_count");
    assert_eq!(dark.folder_count, 2, "dark folder_count");
}

// ── Source-group upsert tests (T065) ──────────────────────────────────────

/// First scan inserts the source group row with the expected fields.
#[tokio::test]
async fn upsert_source_group_inserts_on_first_scan() {
    let db = test_db().await;
    let pool = db.pool();

    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-t065-1",
            root_id: "root-1",
            relative_path: "2025-10-10/lights",
            content_signature: Some("sig-abc123"),
            format: Some("fits"),
            lane: Some("move"),
            file_count: 1,
        },
    )
    .await
    .unwrap();

    let row = crate::repositories::q_inbox::get_source_group_by_id(pool, "sg-t065-1")
        .await
        .unwrap()
        .expect("source group must exist after upsert");

    assert_eq!(row.id, "sg-t065-1");
    assert_eq!(row.root_id, "root-1");
    assert_eq!(row.relative_path, "2025-10-10/lights");
    assert_eq!(row.content_signature.as_deref(), Some("sig-abc123"));
    assert_eq!(row.format.as_deref(), Some("fits"));
    assert_eq!(row.lane.as_deref(), Some("move"));
    assert_eq!(row.child_count, 0, "child_count starts at 0 (classify sets it)");
}

/// Rescan refreshes last_scanned_at and content_signature without duplicating the row.
#[tokio::test]
async fn upsert_source_group_rescan_refreshes_without_duplicate() {
    let db = test_db().await;
    let pool = db.pool();

    // First scan.
    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-t065-2",
            root_id: "root-2",
            relative_path: "2025-11-01/darks",
            content_signature: Some("sig-old"),
            format: Some("fits"),
            lane: Some("catalogue"),
            file_count: 1,
        },
    )
    .await
    .unwrap();

    let first = crate::repositories::q_inbox::get_source_group_by_id(pool, "sg-t065-2")
        .await
        .unwrap()
        .unwrap();

    // Record discovered_at so we can verify it is preserved on rescan.
    let discovered_at_first = first.discovered_at.clone();

    // Rescan: same (root_id, relative_path), new signature.
    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-t065-2-ignored", // id ignored on conflict; original preserved
            root_id: "root-2",
            relative_path: "2025-11-01/darks",
            content_signature: Some("sig-new"),
            format: Some("fits"),
            lane: Some("catalogue"),
            file_count: 1,
        },
    )
    .await
    .unwrap();

    // Look up by the ORIGINAL id: the rescan's "sg-t065-2-ignored" id must
    // never have been persisted (ON CONFLICT DO UPDATE does not touch id).
    let second = crate::repositories::q_inbox::get_source_group_by_id(pool, "sg-t065-2")
        .await
        .unwrap()
        .unwrap();

    // Row count is still 1 (not duplicated).
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM inbox_source_groups WHERE root_id = 'root-2' AND relative_path = '2025-11-01/darks'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(count.0, 1, "rescan must not duplicate the source group row");

    // content_signature updated.
    assert_eq!(second.content_signature.as_deref(), Some("sig-new"));

    // discovered_at preserved.
    assert_eq!(second.discovered_at, discovered_at_first);

    // child_count still 0 (classify hasn't run).
    assert_eq!(second.child_count, 0);
}

/// Two distinct leaf folders under the same root produce two source group rows.
#[tokio::test]
async fn upsert_source_group_two_leaf_folders_produce_two_rows() {
    let db = test_db().await;
    let pool = db.pool();

    for (id, path) in [("sg-t065-a", "session/lights"), ("sg-t065-b", "session/darks")] {
        upsert_inbox_source_group(
            pool,
            &UpsertSourceGroup {
                id,
                root_id: "root-multi",
                relative_path: path,
                content_signature: Some("sig"),
                format: Some("fits"),
                lane: Some("move"),
                file_count: 1,
            },
        )
        .await
        .unwrap();
    }

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM inbox_source_groups WHERE root_id = 'root-multi'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count.0, 2, "each leaf folder is a distinct source group row");
}

/// Video-lane leaf folder is stored with lane = "move" (video sources are never
/// catalogue-in-place).  Format field carries "video".
#[tokio::test]
async fn upsert_source_group_video_lane_stored() {
    let db = test_db().await;
    let pool = db.pool();

    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-t065-vid",
            root_id: "root-vid",
            relative_path: "planetary/jupiter",
            content_signature: None,
            format: Some("video"),
            lane: Some("move"),
            file_count: 1,
        },
    )
    .await
    .unwrap();

    let row = crate::repositories::q_inbox::get_source_group_by_id(pool, "sg-t065-vid")
        .await
        .unwrap()
        .expect("video source group must be persisted");

    assert_eq!(row.format.as_deref(), Some("video"));
    assert_eq!(row.lane.as_deref(), Some("move"));
}

// ── last_scanned_by_root (P6a) ─────────────────────────────────────────────

/// No source-group rows for a root → absent from the map (never scanned).
#[tokio::test]
async fn last_scanned_by_root_empty_when_no_scans() {
    let db = test_db().await;
    let map = last_scanned_by_root(db.pool()).await.unwrap();
    assert!(map.is_empty());
}

/// Rescanning a root's leaf folder advances its `last_scanned_at`, and the
/// map reports the MOST RECENT scan across all of that root's leaf folders.
#[tokio::test]
async fn last_scanned_by_root_reports_max_across_leaf_folders() {
    let db = test_db().await;
    let pool = db.pool();

    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-scan-a",
            root_id: "root-scan",
            relative_path: "2025-10-10/lights",
            content_signature: Some("sig-a"),
            format: Some("fits"),
            lane: Some("move"),
            file_count: 1,
        },
    )
    .await
    .unwrap();

    // A second leaf folder under the same root, scanned slightly later.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-scan-b",
            root_id: "root-scan",
            relative_path: "2025-10-11/lights",
            content_signature: Some("sig-b"),
            format: Some("fits"),
            lane: Some("move"),
            file_count: 1,
        },
    )
    .await
    .unwrap();

    let later = crate::repositories::q_inbox::get_source_group_by_id(pool, "sg-scan-b")
        .await
        .unwrap()
        .expect("second group must exist");

    let map = last_scanned_by_root(pool).await.unwrap();
    assert_eq!(
        map.get("root-scan"),
        Some(&later.last_scanned_at),
        "must report the most recent scan across the root's leaf folders"
    );
}

/// Distinct roots are reported independently.
#[tokio::test]
async fn last_scanned_by_root_keys_by_root_id() {
    let db = test_db().await;
    let pool = db.pool();

    for root_id in ["root-x", "root-y"] {
        upsert_inbox_source_group(
            pool,
            &UpsertSourceGroup {
                id: &format!("sg-{root_id}"),
                root_id,
                relative_path: "leaf",
                content_signature: None,
                format: Some("fits"),
                lane: Some("move"),
                file_count: 1,
            },
        )
        .await
        .unwrap();
    }

    let map = last_scanned_by_root(pool).await.unwrap();
    assert!(map.contains_key("root-x"));
    assert!(map.contains_key("root-y"));
}

// ── list_inbox_attribution_geometry (spec 008 Q27, F-Framing-5) ──────────

#[tokio::test]
async fn list_inbox_attribution_geometry_round_trips_staged_fields() {
    let db = test_db().await;
    let pool = db.pool();

    sqlx::query(
        "INSERT INTO inbox_items \
         (id, root_id, relative_path, group_key, \
          discovered_at, last_scanned_at, state, lane) \
         VALUES ('item-geo', 'root-1', '2025-10-10/lights', '', \
                 '2025-10-10T20:00:00Z', '2025-10-10T20:00:00Z', \
                 'pending_classification', 'fits')",
    )
    .execute(pool)
    .await
    .unwrap();

    upsert_inbox_file_metadata(
        pool,
        &UpsertFileMetadata {
            inbox_item_id: "item-geo",
            relative_file_path: "light_001.fits",
            telescop: Some("RASA 8"),
            instrume: Some("ASI2600MM"),
            focal_length_mm: Some(400.0),
            ra_deg: Some(83.633),
            dec_deg: Some(22.0145),
            rotator_angle_deg: Some(1.5),
            pixel_size_um: Some(3.76),
            naxis1: Some(6248),
            naxis2: Some(4176),
            object: Some("M 42"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let rows = list_inbox_attribution_geometry(pool, "item-geo").await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.telescop.as_deref(), Some("RASA 8"));
    assert_eq!(row.instrume.as_deref(), Some("ASI2600MM"));
    assert_eq!(row.focal_length_mm, Some(400.0));
    assert_eq!(row.ra_deg, Some(83.633));
    assert_eq!(row.dec_deg, Some(22.0145));
    assert_eq!(row.rotator_angle_deg, Some(1.5));
    assert_eq!(row.pixel_size_um, Some(3.76));
    assert_eq!(row.naxis1, Some(6248));
    assert_eq!(row.naxis2, Some(4176));
    assert_eq!(row.object.as_deref(), Some("M 42"));
}

#[tokio::test]
async fn list_inbox_attribution_geometry_empty_for_unknown_item() {
    let db = test_db().await;
    let rows = list_inbox_attribution_geometry(db.pool(), "no-such-item").await.unwrap();
    assert!(rows.is_empty());
}

/// Register one inbox source and return its id.
async fn seed_inbox_source(pool: &SqlitePool, path: &str) -> String {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };
    let resp = persistence_lifecycle::repositories::first_run::register_source_batch(
        pool,
        &RegisterSourceBatchRequest {
            sources: vec![RegisterSourceRequest {
                kind: SourceKind::Inbox,
                path: path.to_owned(),
                kind_subtype: None,
                scan_depth: ScanDepth::Recursive,
                organization_state: OrganizationState::Unorganized,
            }],
        },
    )
    .await
    .unwrap();
    resp.items[0].source_id.as_deref().unwrap().to_owned()
}

/// Spec 058 T013, FR-016 + FR-017: a scanned folder is represented in the
/// list by its source group *only* while it has produced no item rows, and
/// stops being represented that way the moment materialization writes one.
///
/// FR-017's "the source-group row is replaced by the folder's item rows" is
/// asserted here as a property of the query, not of a separate swap step.
#[tokio::test]
async fn unclassified_source_group_is_listed_until_it_has_items_058() {
    let db = test_db().await;
    let pool = db.pool();
    let source_id = seed_inbox_source(pool, "/astro/inbox").await;

    upsert_inbox_source_group(
        pool,
        &UpsertSourceGroup {
            id: "sg-unclassified",
            root_id: &source_id,
            relative_path: "2026-01-01/lights",
            content_signature: Some("sig-folder"),
            format: Some("fits"),
            lane: Some("move"),
            file_count: 7,
        },
    )
    .await
    .unwrap();

    let rows = list_unclassified_source_groups(pool, 100).await.unwrap();
    assert_eq!(rows.len(), 1, "a scanned folder with no items must be listed (FR-016)");
    assert_eq!(rows[0].id, "sg-unclassified");
    assert_eq!(rows[0].root_path, "/astro/inbox", "root_path must join registered_sources");
    assert_eq!(rows[0].relative_path, "2026-01-01/lights");
    assert_eq!(rows[0].file_count, 7, "the scanned sub-frame count must survive on the group");
    assert_eq!(rows[0].lane.as_deref(), Some("move"), "group lane, not the fits/video lane");

    // Classification materializes one item for the group.
    upsert_inbox_sub_item(
        pool,
        &UpsertInboxSubItem {
            id: "sub-light",
            root_id: &source_id,
            relative_path: "2026-01-01/lights",
            source_group_id: "sg-unclassified",
            group_key: "type=light",
            group_label: "(root) · light",
            frame_type: Some("light"),
            content_signature: "sig-sub",
            file_count: 7,
            lane: "fits",
            needs_review: false,
        },
    )
    .await
    .unwrap();

    let rows = list_unclassified_source_groups(pool, 100).await.unwrap();
    assert!(
        rows.is_empty(),
        "FR-017: once the group has item rows it must no longer be listed as a \
         source-group row — otherwise the folder is represented twice"
    );
}

/// Spec 058 FR-015 master carve-out — pinned at the layer that actually
/// implements it.
///
/// This query has no master-awareness and structurally cannot have any:
/// `q_desktop::insert_inbox_master_item` omits `source_group_id` from its
/// INSERT, so master rows always carry NULL there and can never satisfy the
/// `NOT EXISTS` clause. The ONLY thing that excludes a masters-only folder
/// is `file_count = 0`, written by scan's `sub_frame_count()`, which
/// subtracts the detected masters. The asserted pair below states exactly
/// that, so this cannot be misread as coverage of the carve-out arithmetic
/// itself — that lives in
/// `crates/app/core/tests/scan_masters_integration.rs`.
#[tokio::test]
async fn only_file_count_excludes_a_masters_only_folder_058() {
    let db = test_db().await;
    let pool = db.pool();
    let source_id = seed_inbox_source(pool, "/astro/inbox").await;

    let group = |file_count: i64| UpsertSourceGroup {
        id: "sg-masters",
        root_id: &source_id,
        relative_path: "masters",
        content_signature: Some("sig-masters"),
        format: Some("fits"),
        lane: Some("move"),
        file_count,
    };

    // Two real master rows, exactly as scan writes them.
    for (id, name) in [("m-1", "masterDark.fits"), ("m-2", "masterBias.fits")] {
        persistence_targets::repositories::q_desktop::insert_inbox_master_item(
            pool,
            id,
            &source_id,
            &format!("masters/{name}"),
            "move",
            "fits",
            "dark",
            None,
            Some(30.0),
        )
        .await
        .unwrap();
    }

    upsert_inbox_source_group(pool, &group(0)).await.unwrap();
    let rows = list_unclassified_source_groups(pool, 100).await.unwrap();
    assert!(
        rows.is_empty(),
        "a folder with no sub-frames left to classify must not appear as a \
         scanned-but-unclassified row (FR-015 master carve-out)"
    );

    // Converse: the same master rows are still present, yet a non-zero
    // `file_count` lists the folder anyway. `file_count` is load-bearing on
    // its own; nothing in this query reads `is_master_item`.
    upsert_inbox_source_group(pool, &group(2)).await.unwrap();
    let rows = list_unclassified_source_groups(pool, 100).await.unwrap();
    assert_eq!(
        rows.len(),
        1,
        "the query is master-blind: only `file_count` excludes the folder, so \
         scan's `sub_frame_count()` is the whole carve-out"
    );
}

// ── #711 stale-placeholder regression tests ───────────────────────────────────
//
// `materialize_sub_items` previously never purged the `group_key = ''`
// placeholder inserted before classify runs, because `list_inbox_sub_items`
// excludes that row.  The three tests below pin that the projections never
// see it once a real sibling sub-item exists.

/// A stale `group_key = ''` placeholder must be absent from
/// `list_unacknowledged_across_roots` once a real sub-item exists for the
/// same source group.
#[tokio::test]
async fn list_unacknowledged_excludes_stale_placeholder_when_sub_item_exists() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    let batch_resp = persistence_lifecycle::repositories::first_run::register_source_batch(
        pool,
        &RegisterSourceBatchRequest {
            sources: vec![RegisterSourceRequest {
                kind: SourceKind::Inbox,
                path: "/astro/inbox-711".to_owned(),
                kind_subtype: None,
                scan_depth: ScanDepth::Recursive,
                organization_state: OrganizationState::Unorganized,
            }],
        },
    )
    .await
    .unwrap();
    let root_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();

    sqlx::query(
        "INSERT INTO inbox_source_groups \
         (id, root_id, relative_path, discovered_at, last_scanned_at, child_count) \
         VALUES ('sg-711', ?, '2025-11-01/needs-review', \
                 '2025-11-01T00:00:00Z', '2025-11-01T00:00:00Z', 1)",
    )
    .bind(&root_id)
    .execute(pool)
    .await
    .unwrap();

    // Stale placeholder: group_key='' left un-purged by materialize_sub_items.
    sqlx::query(
        "INSERT INTO inbox_items \
         (id, root_id, relative_path, source_group_id, group_key, \
          discovered_at, last_scanned_at, content_signature, state, lane) \
         VALUES ('placeholder-item', ?, '2025-11-01/needs-review', 'sg-711', '', \
                 '2025-11-01T00:00:00Z', '2025-11-01T00:00:00Z', 'sig-shared', \
                 'classified', 'fits')",
    )
    .bind(&root_id)
    .execute(pool)
    .await
    .unwrap();

    // Two real materialized sub-items: exclusion requires >= 2 siblings so that
    // an unsplit single-type folder (placeholder + 1 sub-item) is not affected.
    upsert_inbox_sub_item(
        pool,
        &UpsertInboxSubItem {
            id: "sub-item-needs-review",
            root_id: &root_id,
            relative_path: "2025-11-01/needs-review",
            source_group_id: "sg-711",
            group_key: "type=unknown",
            group_label: "(root) · needs review",
            frame_type: None,
            content_signature: "sig-shared",
            file_count: 1,
            lane: "fits",
            needs_review: true,
        },
    )
    .await
    .unwrap();
    upsert_inbox_sub_item(
        pool,
        &UpsertInboxSubItem {
            id: "sub-item-light",
            root_id: &root_id,
            relative_path: "2025-11-01/needs-review",
            source_group_id: "sg-711",
            group_key: "type=light",
            group_label: "(root) · light",
            frame_type: Some("light"),
            content_signature: "sig-light",
            file_count: 1,
            lane: "fits",
            needs_review: false,
        },
    )
    .await
    .unwrap();

    let rows = list_unacknowledged_across_roots(pool, 100).await.unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert!(
        !ids.contains(&"placeholder-item"),
        "stale placeholder must be excluded once two real sub-items exist: {ids:?}"
    );
    assert!(
        ids.contains(&"sub-item-needs-review"),
        "the real materialized sub-item must still be listed: {ids:?}"
    );
}

/// A standalone `group_key = ''` item with no `source_group_id` (a freshly
/// scanned, not-yet-classified folder) must NOT be filtered — the #711
/// exclusion only applies when a real sibling sub-item exists.
#[tokio::test]
async fn list_unacknowledged_keeps_unclassified_item_with_no_siblings() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    let batch_resp = persistence_lifecycle::repositories::first_run::register_source_batch(
        pool,
        &RegisterSourceBatchRequest {
            sources: vec![RegisterSourceRequest {
                kind: SourceKind::Inbox,
                path: "/astro/inbox-711b".to_owned(),
                kind_subtype: None,
                scan_depth: ScanDepth::Recursive,
                organization_state: OrganizationState::Unorganized,
            }],
        },
    )
    .await
    .unwrap();
    let root_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();

    insert_inbox_item(
        pool,
        &InsertInboxItem {
            id: "fresh-item",
            root_id: &root_id,
            relative_path: "2025-11-02/lights",
            file_count: 3,
            content_signature: Some("sig-fresh"),
            lane: "fits",
        },
    )
    .await
    .unwrap();

    let rows = list_unacknowledged_across_roots(pool, 100).await.unwrap();
    assert!(rows.iter().any(|r| r.id == "fresh-item"));
}

/// #711 follow-up: `inbox_stats`, `count_distinct_inbox_folders`, and
/// `list_unacknowledged_across_roots` must all agree on exactly one real
/// folder when a stale placeholder that carries its own evidence rows is
/// present alongside its materialized sibling.
#[tokio::test]
#[allow(clippy::too_many_lines)] // linear fixture setup for 2 rows × 2 evidence each
async fn list_and_stats_agree_when_stale_placeholder_has_evidence() {
    use domain_core::first_run::{
        OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    let db = test_db().await;
    let pool = db.pool();

    let batch_resp = persistence_lifecycle::repositories::first_run::register_source_batch(
        pool,
        &RegisterSourceBatchRequest {
            sources: vec![RegisterSourceRequest {
                kind: SourceKind::Inbox,
                path: "/astro/inbox-711c".to_owned(),
                kind_subtype: None,
                scan_depth: ScanDepth::Recursive,
                organization_state: OrganizationState::Unorganized,
            }],
        },
    )
    .await
    .unwrap();
    let root_id = batch_resp.items[0].source_id.as_deref().unwrap().to_owned();

    sqlx::query(
        "INSERT INTO inbox_source_groups \
         (id, root_id, relative_path, discovered_at, last_scanned_at, child_count) \
         VALUES ('sg-711c', ?, '2025-11-03/lights-2f', \
                 '2025-11-03T00:00:00Z', '2025-11-03T00:00:00Z', 1)",
    )
    .bind(&root_id)
    .execute(pool)
    .await
    .unwrap();

    // Stale placeholder that still carries evidence rows from the original
    // classify pass — it satisfies the evidence JOIN in inbox_stats.
    sqlx::query(
        "INSERT INTO inbox_items \
         (id, root_id, relative_path, source_group_id, group_key, \
          discovered_at, last_scanned_at, content_signature, file_count, state, lane) \
         VALUES ('placeholder-2f', ?, '2025-11-03/lights-2f', 'sg-711c', '', \
                 '2025-11-03T00:00:00Z', '2025-11-03T00:00:00Z', 'sig-2f', 2, \
                 'classified', 'fits')",
    )
    .bind(&root_id)
    .execute(pool)
    .await
    .unwrap();
    for (ev_id, path) in [
        ("ev-placeholder-2f-a", "lights-2f/frame_001.fits"),
        ("ev-placeholder-2f-b", "lights-2f/frame_002.fits"),
    ] {
        insert_evidence(
            pool,
            &InsertEvidence {
                id: ev_id,
                inbox_item_id: "placeholder-2f",
                relative_file_path: path,
                frame_type: Some("light"),
                evidence_source: "imagetyp_header",
                raw_value: Some("Light Frame"),
                unclassified: false,
                manual_override: None,
                is_master: false,
                master_detector: None,
            },
        )
        .await
        .unwrap();
    }

    // Two real materialized sub-items for the same folder (lights + darks split).
    // The >= 2 bound requires both to be present before the placeholder is excluded.
    let sub_id = upsert_inbox_sub_item(
        pool,
        &UpsertInboxSubItem {
            id: "sub-2f",
            root_id: &root_id,
            relative_path: "2025-11-03/lights-2f",
            source_group_id: "sg-711c",
            group_key: "type=light",
            group_label: "(root) · light",
            frame_type: Some("light"),
            content_signature: "sig-2f",
            file_count: 2,
            lane: "fits",
            needs_review: false,
        },
    )
    .await
    .unwrap();
    for (ev_id, path) in
        [("ev-sub-2f-a", "lights-2f/frame_001.fits"), ("ev-sub-2f-b", "lights-2f/frame_002.fits")]
    {
        insert_evidence(
            pool,
            &InsertEvidence {
                id: ev_id,
                inbox_item_id: &sub_id,
                relative_file_path: path,
                frame_type: Some("light"),
                evidence_source: "imagetyp_header",
                raw_value: Some("Light Frame"),
                unclassified: false,
                manual_override: None,
                is_master: false,
                master_detector: None,
            },
        )
        .await
        .unwrap();
    }
    // Second sub-item (dark group) to reach the >= 2 sibling threshold.
    upsert_inbox_sub_item(
        pool,
        &UpsertInboxSubItem {
            id: "sub-2f-dark",
            root_id: &root_id,
            relative_path: "2025-11-03/lights-2f",
            source_group_id: "sg-711c",
            group_key: "type=dark",
            group_label: "(root) · dark",
            frame_type: Some("dark"),
            content_signature: "sig-2f-dark",
            file_count: 1,
            lane: "fits",
            needs_review: false,
        },
    )
    .await
    .unwrap();

    // 1. List: only the real sub-item, never the stale placeholder.
    let rows = list_unacknowledged_across_roots(pool, 100).await.unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert!(!ids.contains(&"placeholder-2f"), "list must exclude the stale placeholder: {ids:?}");
    assert!(ids.contains(&"sub-2f"), "list must include the real sub-item: {ids:?}");

    // 2. count_distinct_inbox_folders: exactly 1, not 2.
    let distinct = count_distinct_inbox_folders(pool).await.unwrap();
    assert_eq!(distinct, 1, "stale placeholder must not inflate the distinct-folder count");

    // 3. inbox_stats: 'light' folder_count is 1, not 2.
    let stats = inbox_stats(pool).await.unwrap();
    let light = stats.iter().find(|r| r.frame_type == "light").expect("light row present");
    assert_eq!(
        light.folder_count, 1,
        "stale placeholder must not inflate inbox_stats' per-type folder_count"
    );
}
