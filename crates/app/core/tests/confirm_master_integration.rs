// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for spec 041 US4 — confirm a detected calibration master
//! inbox item now routes through a reviewable plan (Constitution §II) instead
//! of the old confirm-time "register directly" fast path (spec 040 US3).
//!
//! Tests that:
//! 1. Confirming a master inbox item creates a reviewable PLAN (non-empty
//!    `plan_id`, `registered_as_master = false`) and does NOT register the
//!    master at confirm time.
//! 2. After the plan is applied, the plan listener registers the master
//!    (`calibration_session` + `calibration_fingerprint`) and it appears via
//!    `calibration.masters.list`.
//! 3. A non-master item still goes through the normal plan-creation path
//!    (regression guard).
//! 4. A master from an ORGANIZED source produces a `catalogue` plan item (no
//!    move); from an UNORGANIZED source produces a `move` plan item — both
//!    register the master at apply completion (master parity).

use std::io::Write;
use std::path::Path;

use app_core::calibration::masters_list;
use app_core::inbox::confirm::{confirm, ConfirmRequest};
use app_core::inbox_plan::{apply_inbox_plan, get_inbox_plan};
use app_core::plans::approve_plan;
use audit::bus::EventBus;
use persistence_core::Database;
use persistence_inbox::repositories::inbox::{
    self as inbox_repo, InsertEvidence, InsertInboxItem, UpsertClassification,
};

mod support;

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn test_db(dest_root: &Path) -> Database {
    let db = Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    // Override all per-type destination patterns with literal-only patterns so
    // test fixtures without FITS header metadata (exposure/target/filter/date)
    // can confirm without triggering InboxMissingPathAttributes (spec 041).
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('patternsByType', ?, '2026-01-01T00:00:00Z')
          ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(r#"{"light":"lights/","flat":"flats/","dark":"darks/","bias":"bias/","master_flat":"masters/flats/","master_dark":"masters/darks/","master_bias":"masters/bias/"}"#)
    .execute(db.pool())
    .await
    .unwrap();
    // Register destination library roots so inbox → library routing succeeds.
    // `dest_root` must be a real, writable directory (#765 fix: the executor
    // now correctly joins the destination path against the picked
    // *destination* root instead of silently falling back to the source
    // root, so a fictional path here would make every real move fail).
    std::fs::create_dir_all(dest_root).unwrap();
    let dest_path = dest_root.to_str().unwrap();
    for (id, kind) in &[("dest-light", "light_frames"), ("dest-calib", "calibration")] {
        sqlx::query(
            "INSERT INTO registered_sources (id, kind, path, kind_subtype, scan_depth, created_at, created_via)
             VALUES (?, ?, ?, NULL, 'recursive', '2026-01-01T00:00:00Z', 'first_run')
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(id)
        .bind(kind)
        .bind(dest_path)
        .execute(db.pool())
        .await
        .unwrap();
    }
    db
}

/// Write a minimal FITS file (single 2880-byte block).
fn write_fits(dir: &Path, name: &str, imagetyp: &str) {
    write_fits_with_camera(dir, name, imagetyp, None);
}

/// `camera` writes an `INSTRUME`/`CAMERAID` pair, which the master-registration
/// path reduces to `calibration_fingerprint.camera_body_id`.
fn write_fits_with_camera(dir: &Path, name: &str, imagetyp: &str, camera: Option<(&str, &str)>) {
    let mut block = vec![b' '; 2880];
    let mut idx = 0usize;
    let mut write_card = |card: &str| {
        let bytes = card.as_bytes();
        let len = bytes.len().min(80);
        block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
        idx += 1;
    };
    write_card(&format!("IMAGETYP= '{imagetyp:<8}'"));
    if let Some((instrume, cameraid)) = camera {
        write_card(&format!("INSTRUME= '{instrume}'"));
        write_card(&format!("CAMERAID= '{cameraid}'"));
    }
    block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
    let path = dir.join(name);
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(&block).unwrap();
}

/// Register a source row with an explicit organization state, plus the matching
/// `library_root` row the plan executor uses to resolve absolute paths.
async fn register_source(db: &Database, root_id: &str, kind: &str, path: &str, org_state: &str) {
    sqlx::query(
        "INSERT INTO registered_sources
            (id, kind, path, kind_subtype, scan_depth, created_at, created_via, organization_state)
         VALUES (?, ?, ?, NULL, 'recursive', '2026-01-01T00:00:00Z', 'first_run', ?)",
    )
    .bind(root_id)
    .bind(kind)
    .bind(path)
    .bind(org_state)
    .execute(db.pool())
    .await
    .unwrap();

    // The executor resolves `from_root_id` → absolute path via `library_root`.
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
         VALUES (?, 'test-root', ?, 'local', 'active', '2026-01-01T00:00:00Z')",
    )
    .bind(root_id)
    .bind(path)
    .execute(db.pool())
    .await
    .unwrap();
}

/// Insert a master inbox item row directly (simulating what scan does).
async fn insert_master_inbox_item(
    db: &Database,
    item_id: &str,
    root_id: &str,
    relative_path: &str,
    master_frame_type: &str,
    master_filter: Option<&str>,
    master_exposure_s: Option<f64>,
    sig: &str,
) {
    let now = "2026-06-18T12:00:00Z";
    sqlx::query(
        "INSERT INTO inbox_items
            (id, root_id, relative_path, file_count, discovered_at, last_scanned_at,
             content_signature, state, lane, format, is_master_item,
             master_frame_type, master_filter, master_exposure_s)
         VALUES (?, ?, ?, 1, ?, ?, ?, 'pending_classification',
                 'fits', 'fits', 1, ?, ?, ?)",
    )
    .bind(item_id)
    .bind(root_id)
    .bind(relative_path)
    .bind(now)
    .bind(now)
    .bind(sig)
    .bind(master_frame_type)
    .bind(master_filter)
    .bind(master_exposure_s)
    .execute(db.pool())
    .await
    .unwrap();

    // A master inbox item must carry a classification + evidence so the normal
    // confirm path (no more fast-path) can enumerate its single file.
    inbox_repo::upsert_classification(
        db.pool(),
        &UpsertClassification {
            inbox_item_id: item_id,
            result: "classified",
            frame_type: Some(master_frame_type),
            content_signature: sig,
            unclassified_file_count: 0,
        },
    )
    .await
    .unwrap();

    inbox_repo::insert_evidence(
        db.pool(),
        &InsertEvidence {
            id: &format!("ev-{item_id}"),
            inbox_item_id: item_id,
            relative_file_path: relative_path,
            frame_type: Some(master_frame_type),
            evidence_source: "imagetyp_header",
            raw_value: None,
            unclassified: false,
            manual_override: None,
            is_master: true,
            master_detector: Some("filename"),
        },
    )
    .await
    .unwrap();
}

/// Drive apply to completion. The plan listener (started by the caller before
/// apply) consumes `plan.applying.completed` and registers the master.
async fn apply_and_register(db: &Database, bus: &EventBus, item_id: &str) {
    // Constitution II (astro-plan-tykek): the apply path refuses a plan the user
    // has not approved, so record the approval the UI's Apply gesture records.
    let plan_id = get_inbox_plan(db.pool(), item_id).await.unwrap().plan_id;
    approve_plan(db.pool(), bus, &plan_id, "user").await.unwrap();

    let resp = apply_inbox_plan(db.pool(), bus, item_id).await.unwrap();
    // The executor finishes, publishes TOPIC_PLAN_APPLYING_COMPLETED, then the
    // plan listener runs its registration callback asynchronously. Poll for the
    // observable side effect (a calibration_session row) rather than wall time,
    // so the test waits exactly as long as registration actually takes.
    let pool = db.pool().clone();
    support::poll_until(
        || async {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM calibration_session")
                .fetch_one(&pool)
                .await
                .unwrap();
            if count > 0 {
                Some(())
            } else {
                None
            }
        },
        "calibration_session row never appeared after plan apply-completed event",
    )
    .await;
    let _ = resp; // plan_id captured via closure above
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// US4: confirming a master item now creates a plan and does NOT register at
/// confirm time. The master is registered when the plan is applied.
#[tokio::test]
async fn confirm_master_creates_plan_then_registers_at_apply() {
    let tmp = tempfile::tempdir().unwrap();
    write_fits_with_camera(
        tmp.path(),
        "masterDark_300s.fits",
        "DARK",
        Some(("Poseidon-C PRO", "Player One_CAMD2282B4C061209000")),
    );

    let db = test_db(&tmp.path().join("dest")).await;
    let bus = EventBus::with_pool(db.pool().clone());
    app_core::inbox::plan_listener::start_inbox_plan_listener(
        db.pool().clone(),
        &bus,
        targeting_resolver::simbad::ResolveCache::in_memory().unwrap(),
    );
    let item_id = "master-item-001";
    let sig = "sig-master-001";

    // Unorganized inbox source → master produces a MOVE plan.
    register_source(&db, "root-1", "inbox", tmp.path().to_str().unwrap(), "unorganized").await;
    insert_master_inbox_item(
        &db,
        item_id,
        "root-1",
        "masterDark_300s.fits",
        "dark",
        None,
        Some(300.0),
        sig,
    )
    .await;

    let resp = confirm(
        db.pool(),
        &bus,
        ConfirmRequest {
            inbox_item_id: item_id.to_owned(),
            content_signature: sig.to_owned(),
            destructive_destination: None,
            root_absolute_path: tmp.path().to_owned(),
            root_id: None,
            chosen_attribution: None,
        },
    )
    .await
    .unwrap();

    // NEW behavior: a real plan, no confirm-time registration.
    assert!(!resp.registered_as_master, "master must NOT register at confirm time (US4)");
    assert!(!resp.plan_id.is_empty(), "master confirm must produce a reviewable plan");
    assert_eq!(resp.items_total, 1);
    assert_eq!(resp.move_count, 1, "unorganized master → move plan item");
    assert_eq!(resp.catalogue_count, 0);

    // No calibration rows yet (registration deferred to apply).
    let pre: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM calibration_session")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(pre, 0, "master must NOT be registered before apply");

    // Apply the plan → master registers at completion.
    apply_and_register(&db, &bus, item_id).await;

    let session_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM calibration_session WHERE kind = 'dark'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(session_count, 1, "master must be registered after apply");

    let source_id: Option<String> =
        sqlx::query_scalar("SELECT source_inbox_item_id FROM calibration_session LIMIT 1")
            .fetch_optional(db.pool())
            .await
            .unwrap();
    assert_eq!(source_id.as_deref(), Some(item_id));

    // astro-plan-ugux2: the master-registration path must derive the body id
    // from the master frame's own CAMERAID, normalized the way
    // `sessions::camera_body_id` normalizes.
    let camera_body_id: Option<String> =
        sqlx::query_scalar("SELECT camera_body_id FROM calibration_fingerprint LIMIT 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        camera_body_id.as_deref(),
        Some("player one_camd2282b4c061209000"),
        "CAMERAID must reach the master fingerprint through the real registration derivation"
    );
}

/// US4 master parity: a master from an ORGANIZED source produces a `catalogue`
/// plan item (no move) and still registers at apply.
#[tokio::test]
async fn organized_master_catalogues_then_registers_at_apply() {
    let tmp = tempfile::tempdir().unwrap();
    write_fits(tmp.path(), "masterFlat_Ha.fits", "FLAT");

    let db = test_db(&tmp.path().join("dest")).await;
    let bus = EventBus::with_pool(db.pool().clone());
    app_core::inbox::plan_listener::start_inbox_plan_listener(
        db.pool().clone(),
        &bus,
        targeting_resolver::simbad::ResolveCache::in_memory().unwrap(),
    );
    let item_id = "master-item-flat";
    let sig = "sig-master-flat";

    // Organized (non-inbox) source → master produces a CATALOGUE plan item.
    register_source(&db, "root-1", "calibration", tmp.path().to_str().unwrap(), "organized").await;
    insert_master_inbox_item(
        &db,
        item_id,
        "root-1",
        "masterFlat_Ha.fits",
        "flat",
        Some("Ha"),
        Some(2.0),
        sig,
    )
    .await;

    let resp = confirm(
        db.pool(),
        &bus,
        ConfirmRequest {
            inbox_item_id: item_id.to_owned(),
            content_signature: sig.to_owned(),
            destructive_destination: None,
            root_absolute_path: tmp.path().to_owned(),
            root_id: None,
            chosen_attribution: None,
        },
    )
    .await
    .unwrap();

    assert!(!resp.registered_as_master);
    assert_eq!(resp.catalogue_count, 1, "organized master → catalogue plan item");
    assert_eq!(resp.move_count, 0);

    apply_and_register(&db, &bus, item_id).await;

    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(masters.len(), 1, "organized master must register at apply");
    assert_eq!(
        masters[0].kind,
        contracts_core::calibration::CalibrationKind::Flat,
        "kind must be flat"
    );
    assert_eq!(masters[0].fingerprint.filter.as_deref(), Some("Ha"), "filter must be Ha");
    assert!((masters[0].fingerprint.exposure_s.unwrap() - 2.0).abs() < f64::EPSILON);
}

/// US4 regression guard: non-master items still go through the plan path.
#[tokio::test]
async fn non_master_item_still_creates_plan() {
    let tmp = tempfile::tempdir().unwrap();
    write_fits(tmp.path(), "light_001.fits", "Light Frame");

    let db = test_db(&tmp.path().join("dest")).await;
    let bus = EventBus::with_pool(db.pool().clone());
    let item_id = "non-master-item";
    let sig = "sig-non-master";

    inbox_repo::insert_inbox_item(
        db.pool(),
        &InsertInboxItem {
            id: item_id,
            root_id: "root-1",
            relative_path: "",
            file_count: 1,
            content_signature: Some(sig),
            lane: "fits",
        },
    )
    .await
    .unwrap();

    inbox_repo::upsert_classification(
        db.pool(),
        &UpsertClassification {
            inbox_item_id: item_id,
            result: "classified",
            frame_type: Some("light"),
            content_signature: sig,
            unclassified_file_count: 0,
        },
    )
    .await
    .unwrap();

    inbox_repo::insert_evidence(
        db.pool(),
        &InsertEvidence {
            id: "ev-001",
            inbox_item_id: item_id,
            relative_file_path: "light_001.fits",
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

    let resp = confirm(
        db.pool(),
        &bus,
        ConfirmRequest {
            inbox_item_id: item_id.to_owned(),
            content_signature: sig.to_owned(),
            destructive_destination: None,
            root_absolute_path: tmp.path().to_owned(),
            root_id: None,
            chosen_attribution: None,
        },
    )
    .await
    .unwrap();

    assert!(!resp.registered_as_master, "non-master must not set registered_as_master");
    assert!(!resp.plan_id.is_empty(), "non-master must produce a plan_id");
    assert_eq!(resp.plan_state, "ready_for_review");
    assert_eq!(resp.items_total, 1);
}
