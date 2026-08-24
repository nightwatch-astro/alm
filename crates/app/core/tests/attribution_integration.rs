#![allow(clippy::doc_markdown)]
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Layer-1 integration test for spec 008 Q27 SC-008 — the Inbox-confirm
//! attribution apply-path end-to-end (F-Framing-10, FR-022).
//!
//! `inbox.confirm::confirm` writing `plans.chosen_framing_id` for a
//! `chosenAttribution` pick is unit/integration-tested directly against a
//! real classify→confirm pipeline in `crates/app/inbox/src/confirm.rs`'s own
//! test suite (`confirm_applies_add_to_framing_and_persists_the_pick_on_its_
//! own_plan`). This file covers the other half of SC-008 — the part that
//! only exists once a plan actually *applies*: the real plan-apply-completed
//! event, through the real `inbox_plan_listener`, into the real
//! `app_core_targets::ingest_sessions` pipeline, must fold the applied light
//! frame into an `acquisition_session` AND add that session as a member of
//! the framing the plan carried — proving the picked membership is not just
//! persisted on the plan but actually materializes once the plan is applied,
//! never before (§II reviewable — nothing merges until apply completes).
//!
//! Mirrors `ingest_sessions_integration.rs`'s harness (`support::setup()`,
//! `build_applied_plan`, `publish_applied`) — this test's only addition is
//! seeding `plans.chosen_framing_id` before publishing the apply-completed
//! event, simulating what `inbox.confirm` + `attribution::apply_chosen_
//! attribution` already wrote at confirm time.

use std::io::Write;
use std::path::Path;

use audit::bus::EventBus;
use audit::event_bus::{PlanApplyingCompleted, Source, TOPIC_PLAN_APPLYING_COMPLETED};
use persistence_plans::repositories::plans as plans_repo;
use persistence_plans::repositories::projects as projects_repo;
use persistence_targets::repositories::framing as framing_repo;

mod support;

/// Write a minimal single-block FITS file with the given header cards.
fn write_fits(dir: &Path, name: &str, imagetyp: &str, object: Option<&str>) {
    let path = dir.join(name);
    let mut block = vec![b' '; 2880];
    let mut idx = 0usize;
    let mut write_card = |card: &str| {
        let bytes = card.as_bytes();
        let len = bytes.len().min(80);
        block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
        idx += 1;
    };
    write_card(&format!("{:<80}", format!("IMAGETYP= '{imagetyp}'")));
    if let Some(o) = object {
        write_card(&format!("{:<80}", format!("OBJECT  = '{o}'")));
    }
    write_card(&format!("{:<80}", "FILTER  = 'Ha'"));
    write_card(&format!("{:<80}", "DATE-OBS= '2026-06-21T22:00:00'"));
    write_card(&format!("{:<80}", "GAIN    = 100"));
    write_card(&format!("{:<80}", "XBINNING= 1"));
    write_card(&format!("{:<80}", "YBINNING= 1"));
    block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(&block).unwrap();
}

async fn register_source(pool: &sqlx::SqlitePool, id: &str, path: &str) {
    sqlx::query(
        "INSERT INTO registered_sources (id, kind, path, scan_depth, created_at, created_via)
         VALUES (?, 'light_frames', ?, 'recursive', '2026-01-01T00:00:00Z', 'first_run')",
    )
    .bind(id)
    .bind(path)
    .execute(pool)
    .await
    .unwrap();
}

async fn build_applied_plan_with_chosen_framing(
    pool: &sqlx::SqlitePool,
    plan_id: &str,
    root_id: &str,
    rel: &str,
    chosen_framing_id: &str,
) {
    plans_repo::insert_plan(
        pool,
        &plans_repo::InsertPlan {
            id: plan_id,
            title: "Attribution SC-008 test",
            origin: "inbox",
            origin_path: None,
            plan_type: "split",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();

    let item_id = format!("{plan_id}-item-0");
    plans_repo::insert_plan_item(
        pool,
        &plans_repo::InsertPlanItem {
            id: &item_id,
            plan_id,
            item_index: 0,
            name: "[LIGHT] frame.fits",
            action: "move",
            from_root_id: Some(root_id),
            from_relative_path: rel,
            to_root_id: Some(root_id),
            to_relative_path: rel,
            reason: "inbox_split",
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
    sqlx::query("UPDATE plan_items SET item_state = 'succeeded' WHERE id = ?")
        .bind(&item_id)
        .execute(pool)
        .await
        .unwrap();

    // Simulates what `inbox.confirm` + `attribution::apply_chosen_attribution`
    // already wrote at confirm time (F-Framing-10), the two-phase carrier this
    // test proves the apply side of: the pick is recorded on the plan well
    // before any session exists, and only takes effect once the plan applies.
    plans_repo::set_chosen_framing_id(pool, plan_id, chosen_framing_id).await.unwrap();

    sqlx::query("UPDATE plans SET state = 'applied' WHERE id = ?")
        .bind(plan_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn publish_applied(bus: &EventBus, plan_id: &str) {
    let payload = PlanApplyingCompleted {
        plan_id: plan_id.to_owned(),
        run_id: "run-1".to_owned(),
        terminal_state: "applied".to_owned(),
        items_applied: 1,
        items_failed: 0,
        items_skipped: 0,
        items_cancelled: 0,
        at: "2026-06-21T22:00:00Z".to_owned(),
    };
    bus.publish(TOPIC_PLAN_APPLYING_COMPLETED, Source::System, payload).await.unwrap();
}

/// SC-008: a `chosenAttribution` pick persisted at confirm time materializes
/// as real framing membership once — and only once — the plan actually
/// applies, via the real plan-apply-completed event and the real ingest
/// pipeline (no direct call into `ingest_sessions`/`attribution` internals).
#[tokio::test]
async fn chosen_framing_pick_materializes_as_session_membership_once_the_plan_applies() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();
    let tmp = tempfile::tempdir().unwrap();
    let root_id = "src-raw";
    register_source(pool, root_id, tmp.path().to_str().unwrap()).await;

    projects_repo::insert_project(
        pool,
        &projects_repo::InsertProject {
            id: "proj-sc008",
            name: "SC-008 project",
            tool: "PixInsight",
            lifecycle: "ready",
            path: "projects/proj-sc008",
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();
    framing_repo::insert_framing(
        pool,
        &framing_repo::InsertFraming {
            id: "framing-sc008",
            project_id: "proj-sc008",
            target_id: None,
            optic_train_key: "scope|cam|400",
            pointing_ra_deg: 10.0,
            pointing_dec_deg: 20.0,
            rotation_deg: 0.0,
            tolerance_pointing: 0.1,
            tolerance_rotation_deg: 3.0,
            clustering: "suggested",
        },
    )
    .await
    .unwrap();

    write_fits(tmp.path(), "a.fits", "Light Frame", Some("M42"));
    build_applied_plan_with_chosen_framing(pool, "plan-sc008", root_id, "a.fits", "framing-sc008")
        .await;

    // Before apply: the pick is recorded on the plan, but no session exists
    // yet and framing membership is still empty (§II — nothing merges until
    // the reviewable plan actually applies).
    assert!(framing_repo::list_session_ids_for_framing(pool, "framing-sc008")
        .await
        .unwrap()
        .is_empty());

    app_core::inbox::plan_listener::start_inbox_plan_listener(
        pool.clone(),
        &bus,
        targeting_resolver::simbad::ResolveCache::in_memory().unwrap(),
    );
    publish_applied(&bus, "plan-sc008").await;
    // Waits for the framing membership, NOT for the `acquisition_session` row.
    // Ingest writes the session first and binds the framing three awaits later
    // (`ingest_sessions::ingest_light_frame`), so the session row appearing does
    // not imply the binding has run: polling on it and then asserting the
    // membership reads the intermediate state and fails as `None` (this test's
    // own flake, astro-plan-b7h28).
    support::poll_until(
        || async {
            let members =
                framing_repo::list_session_ids_for_framing(pool, "framing-sc008").await.unwrap();
            (!members.is_empty()).then_some(())
        },
        "framing-sc008 never gained a session member after plan-sc008 apply-completed event",
    )
    .await;

    let sessions: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM acquisition_session").fetch_all(pool).await.unwrap();
    assert_eq!(sessions.len(), 1, "the applied light frame must fold into one session");
    let session_id = &sessions[0].0;

    assert_eq!(
        framing_repo::get_framing_id_for_session(pool, session_id).await.unwrap().as_deref(),
        Some("framing-sc008"),
        "the session must become a member of the plan's chosen_framing_id framing"
    );

    // The durable session-level geometry columns (F-Framing-5) are also
    // populated at this same ingest step, from the applied frame's own
    // header data (this fixture carries no TELESCOP/INSTRUME/FOCALLEN/RA/DEC,
    // so they stay NULL here — the point is the write path runs without
    // erroring, not that this particular fixture has geometry to report).
    let geo = framing_repo::get_session_geometry(pool, session_id).await.unwrap();
    assert!(geo.is_some(), "get_session_geometry must find the row (even with NULL fields)");
}

/// Run classify then confirm (no chosenAttribution) against `root_id` at `root_path`.
/// Extracted to keep the calling test within clippy's function-length limit.
async fn classify_and_confirm(
    pool: &sqlx::SqlitePool,
    bus: &EventBus,
    item_id: &str,
    root_path: &std::path::Path,
) -> app_core::inbox::confirm::ConfirmResponse {
    let classify_resp = app_core::inbox::classify::classify(
        pool,
        app_core::inbox::classify::ClassifyRequest {
            inbox_item_id: item_id.to_owned(),
            root_absolute_path: root_path.to_path_buf(),
            force_rescan: false,
        },
    )
    .await
    .expect("classify() must succeed");
    assert_eq!(classify_resp.classification_type, "single_type");
    app_core::inbox::confirm::confirm(
        pool,
        bus,
        app_core::inbox::confirm::ConfirmRequest {
            inbox_item_id: item_id.to_owned(),
            content_signature: classify_resp.content_signature,
            destructive_destination: None,
            root_absolute_path: root_path.to_path_buf(),
            // root_id not forwarded: matches the journey's real invoke payload
            // (no root_id key) — tests catalogue-in-place path.
            root_id: None,
            chosen_attribution: None,
        },
    )
    .await
    .expect("confirm() must succeed for a geometry-less light item with no chosenAttribution")
}

/// Repro attempt for the #898 CI red (`reconcile_drops_externally_deleted_
/// frame_from_real_ui_count`, `crates/e2e-tests/tests/inventory_journeys.rs`)
/// — the REAL `inbox.classify` -> REAL `inbox.confirm` (no chosenAttribution,
/// matching that journey's invoke payload) -> catalogue-in-place apply ->
/// real plan-apply-completed event -> real ingest, for TWO geometry-less
/// light frames (only IMAGETYP/OBJECT/FILTER/DATE-OBS, no TELESCOP/INSTRUME/
/// FOCALLEN/RA/DEC/rotator) sharing one capture identity, exactly mirroring
/// that journey's fixture and organization state (an `organized`
/// `registered_sources` row, so catalogue-in-place, not move).
#[tokio::test]
async fn geometry_less_two_frame_catalogue_in_place_confirm_forms_one_session() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();
    let tmp = tempfile::tempdir().unwrap();

    write_fits(tmp.path(), "light_m33_001.fits", "Light Frame", Some("M 33"));
    write_fits(tmp.path(), "light_m33_002.fits", "Light Frame", Some("M 33"));

    // `roots.register` (organized default per the journey's module docs) —
    // mirrored as a real `registered_sources` row with `organization_state`
    // set explicitly, matching the real `roots_register` command's default.
    let root_id = "root-sc008-repro";
    sqlx::query(
        "INSERT INTO registered_sources
            (id, kind, path, scan_depth, organization_state, created_at, created_via)
         VALUES (?, 'light_frames', ?, 'recursive', 'organized', '2026-01-01T00:00:00Z', 'first_run')",
    )
    .bind(root_id)
    .bind(tmp.path().to_string_lossy().to_string())
    .execute(pool)
    .await
    .unwrap();

    let inbox_item_id = "item-sc008-repro";
    sqlx::query(
        "INSERT INTO inbox_items \
         (id, root_id, relative_path, group_key, discovered_at, last_scanned_at, state, lane) \
         VALUES (?, ?, '', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', \
                 'pending_classification', 'fits')",
    )
    .bind(inbox_item_id)
    .bind(root_id)
    .execute(pool)
    .await
    .unwrap();

    let confirm_resp = classify_and_confirm(pool, &bus, inbox_item_id, tmp.path()).await;
    assert_eq!(confirm_resp.items_total, 2);
    assert!(
        confirm_resp.attribution_candidates.len() == 1
            && confirm_resp.attribution_candidates[0].kind
                == contracts_core::framing::IngestionAttributionKind::NewProject,
        "geometry-less item must degrade to the new_project fallback candidate only: {:?}",
        confirm_resp.attribution_candidates
    );
    assert!(confirm_resp.attribution_applied.is_none());

    sqlx::query("UPDATE plans SET state = 'applied' WHERE id = ?")
        .bind(&confirm_resp.plan_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE plan_items SET item_state = 'succeeded' WHERE plan_id = ?")
        .bind(&confirm_resp.plan_id)
        .execute(pool)
        .await
        .unwrap();

    app_core::inbox::plan_listener::start_inbox_plan_listener(
        pool.clone(),
        &bus,
        targeting_resolver::simbad::ResolveCache::in_memory().unwrap(),
    );
    publish_applied(&bus, &confirm_resp.plan_id).await;
    // Poll until the session has both M 33 frames (listener writes them one by one;
    // polling for non-empty would race before the second frame is appended).
    support::poll_until(
        || async {
            let rows: Vec<(String, String)> =
                sqlx::query_as("SELECT id, frame_ids FROM acquisition_session")
                    .fetch_all(pool)
                    .await
                    .unwrap();
            let ready = rows.iter().any(|(_id, frame_ids)| {
                let frames: Vec<String> = serde_json::from_str(frame_ids).unwrap_or_default();
                frames.len() >= 2
            });
            if ready { Some(()) } else { None }
        },
        "acquisition_session with 2 frames never appeared after confirm_resp plan apply-completed event",
    )
    .await;

    let sessions: Vec<(String, String)> =
        sqlx::query_as("SELECT id, frame_ids FROM acquisition_session")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(sessions.len(), 1, "both M 33 frames must group into ONE session: {sessions:?}");
    let frames: Vec<String> = serde_json::from_str(&sessions[0].1).unwrap();
    assert_eq!(frames.len(), 2, "both frames must be present in the session's frame_ids");
}
