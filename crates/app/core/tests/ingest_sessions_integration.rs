#![allow(clippy::doc_markdown)]
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Layer-1 integration tests for spec 035 US4 — ingest light frames into
//! acquisition sessions grouped by resolved target (T045/T046, FR-016).
//!
//! Real SQLite + real migrations + the real inbox plan listener. A completed
//! plan's applied light frames must form acquisition sessions and link a
//! resolved canonical target (cache hit inline; unknown → pending → back-filled).

use std::io::Write;
use std::path::Path;

use audit::bus::EventBus;
use audit::event_bus::{PlanApplyingCompleted, Source, TOPIC_PLAN_APPLYING_COMPLETED};
use persistence_plans::repositories::plans as plans_repo;
use targeting_resolver::cache::upsert_resolved;
use targeting_resolver::{
    AliasKind, FakeResolver, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource,
};

mod support;

// ── Fixtures ────────────────────────────────────────────────────────────────────

fn m31() -> ResolvedIdentity {
    ResolvedIdentity {
        simbad_oid: Some(1_575_544),
        primary_designation: "M 31".to_owned(),
        common_name: Some("Andromeda Galaxy".to_owned()),
        object_type: ObjectType::Galaxy,
        ra_deg: 10.684_708,
        dec_deg: 41.268_75,
        v_mag: None,
        aliases: vec![
            ResolvedAlias::new("M 31", AliasKind::Designation),
            ResolvedAlias::new("NGC 224", AliasKind::Designation),
            ResolvedAlias::new("Andromeda Galaxy", AliasKind::CommonName),
        ],
        source: TargetSource::Resolved,
    }
}

/// Write a minimal single-block FITS file with the given header cards.
fn write_fits(
    dir: &Path,
    name: &str,
    imagetyp: &str,
    object: Option<&str>,
    filter: Option<&str>,
    date_obs: Option<&str>,
) {
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
    if let Some(f) = filter {
        write_card(&format!("{:<80}", format!("FILTER  = '{f}'")));
    }
    if let Some(d) = date_obs {
        write_card(&format!("{:<80}", format!("DATE-OBS= '{d}'")));
    }
    write_card(&format!("{:<80}", "GAIN    = 100"));
    write_card(&format!("{:<80}", "XBINNING= 1"));
    write_card(&format!("{:<80}", "YBINNING= 1"));
    block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(&block).unwrap();
}

/// Register a `registered_sources` row (R9 mirror path is exercised: ingest must
/// create the `library_root` row itself before inserting the file record).
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

/// Build an applied (state=applied) plan whose `move` items target `root_id` at
/// the given relative paths, all marked `item_state='succeeded'`.
async fn build_applied_plan(pool: &sqlx::SqlitePool, plan_id: &str, root_id: &str, rels: &[&str]) {
    plans_repo::insert_plan(
        pool,
        &plans_repo::InsertPlan {
            id: plan_id,
            title: "Ingest test",
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

    for (i, rel) in rels.iter().enumerate() {
        let item_id = format!("{plan_id}-item-{i}");
        plans_repo::insert_plan_item(
            pool,
            &plans_repo::InsertPlanItem {
                id: &item_id,
                plan_id,
                item_index: i64::try_from(i).unwrap(),
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
    }

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
        items_applied: 2,
        items_failed: 0,
        items_skipped: 0,
        items_cancelled: 0,
        at: "2026-06-21T22:00:00Z".to_owned(),
    };
    bus.publish(TOPIC_PLAN_APPLYING_COMPLETED, Source::System, payload).await.unwrap();
}

async fn session_rows(pool: &sqlx::SqlitePool) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as("SELECT id, frame_ids, canonical_target_id FROM acquisition_session")
        .fetch_all(pool)
        .await
        .unwrap()
}

// ── T045: M31 cache-hit grouping ─────────────────────────────────────────────────

#[tokio::test]
async fn two_m31_frames_group_into_one_linked_session() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();
    let tmp = tempfile::tempdir().unwrap();
    let root_id = "src-raw";
    register_source(pool, root_id, tmp.path().to_str().unwrap()).await;

    // Seed the resolved canonical target so OBJECT resolves inline (cache hit).
    let target_id = upsert_resolved(pool, &m31()).await.unwrap().0.to_string();

    // Two M31 light frames at the destination (same capture identity → one
    // session). Use alias spellings to prove they group under one target.
    write_fits(
        tmp.path(),
        "a.fits",
        "Light Frame",
        Some("M 31"),
        Some("Ha"),
        Some("2026-06-21T22:00:00"),
    );
    write_fits(
        tmp.path(),
        "b.fits",
        "Light Frame",
        Some("NGC 224"),
        Some("Ha"),
        Some("2026-06-21T23:00:00"),
    );

    build_applied_plan(pool, "plan-1", root_id, &["a.fits", "b.fits"]).await;

    app_core::inbox::plan_listener::start_inbox_plan_listener(
        pool.clone(),
        &bus,
        targeting_resolver::simbad::ResolveCache::in_memory().unwrap(),
    );
    publish_applied(&bus, "plan-1").await;
    // Poll until the session exists AND has both frames (the listener writes
    // them one-by-one; polling for non-empty races on the first write).
    support::poll_until(
        || async {
            let rows = session_rows(pool).await;
            let ready = rows.iter().any(|(_id, frame_ids, _ct)| {
                let frames: Vec<String> = serde_json::from_str(frame_ids).unwrap_or_default();
                frames.len() >= 2
            });
            if ready {
                Some(())
            } else {
                None
            }
        },
        "acquisition_session with 2 frames never appeared after plan-1 apply-completed event",
    )
    .await;

    let sessions = session_rows(pool).await;
    assert_eq!(sessions.len(), 1, "two M31 aliases must group into ONE session");
    let (_id, frame_ids, ct) = &sessions[0];
    let frames: Vec<String> = serde_json::from_str(frame_ids).unwrap();
    assert_eq!(frames.len(), 2, "both frames appended to the session");
    assert_eq!(ct.as_deref(), Some(target_id.as_str()), "linked to the seeded M31 target");

    // Sessions read path surfaces frame_count 2 + the canonical target name.
    let listed = app_core::sessions::list_sessions(pool).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].frame_count, 2);
    assert_eq!(listed[0].session_key.target, "M 31", "canonical name surfaced");
    assert!(listed[0].target_ids.contains(&target_id));
    // Regression for #564: the real ingest-written session_key
    // (`target|filter|binning|gain|night`) must round-trip through the read
    // path, not just the target — filter/night previously came back empty
    // because `parse_session_key` only understood a JSON shape nothing ever
    // wrote.
    assert_eq!(listed[0].session_key.filter, "Ha", "filter must surface from session_key");
    assert_eq!(
        listed[0].session_key.night, "2026-06-21",
        "observing night must surface, not created_at"
    );
}

// ── T046: unknown OBJECT → pending → back-fill ───────────────────────────────────

#[tokio::test]
async fn unknown_object_session_backfills_after_resolve() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();
    let tmp = tempfile::tempdir().unwrap();
    let root_id = "src-raw";
    register_source(pool, root_id, tmp.path().to_str().unwrap()).await;

    // No seed: OBJECT is unknown at ingest time → pending, session NULL link.
    write_fits(
        tmp.path(),
        "u.fits",
        "Light Frame",
        Some("WeirdObject 42"),
        Some("L"),
        Some("2026-06-21T22:00:00"),
    );
    build_applied_plan(pool, "plan-2", root_id, &["u.fits"]).await;

    app_core::inbox::plan_listener::start_inbox_plan_listener(
        pool.clone(),
        &bus,
        targeting_resolver::simbad::ResolveCache::in_memory().unwrap(),
    );
    publish_applied(&bus, "plan-2").await;
    support::poll_until(
        || async {
            if session_rows(pool).await.is_empty() {
                None
            } else {
                Some(())
            }
        },
        "acquisition_session row never appeared after plan-2 apply-completed event",
    )
    .await;

    let sessions = session_rows(pool).await;
    assert_eq!(sessions.len(), 1, "session created even when OBJECT unresolved");
    assert!(sessions[0].2.is_none(), "canonical_target_id NULL before resolve (never fabricated)");

    // A pending ingest_resolution row exists for the frame.
    let (pending_state,): (String,) =
        sqlx::query_as("SELECT state FROM ingest_resolution WHERE object_raw = 'WeirdObject 42'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(pending_state, "pending");

    // Drain with a FakeResolver that now returns the target, then back-fill.
    let resolver = FakeResolver::new().with_response("WeirdObject 42", m31());
    let drain = app_core::ingest_resolution::resolve_pending(pool, &resolver, Some(&bus), true, 50)
        .await
        .unwrap();
    assert_eq!(drain.resolved, 1, "pending row resolved on retry");

    let linked = app_core::ingest_sessions::backfill_session_targets(pool).await.unwrap();
    assert_eq!(linked, 1, "the session was back-filled");

    let sessions = session_rows(pool).await;
    assert!(sessions[0].2.is_some(), "canonical_target_id back-filled after resolve");
}

// ── T008 (spec 048): real ingest path populates session size totals ──────────────

/// The ingest path (`plan.apply` completion → frame records → session) must
/// carry real on-disk sizes through to `total_size_bytes`. Frames get different
/// sizes so a sum is distinguishable from `count * size` or a single frame's size.
#[tokio::test]
async fn ingested_session_total_size_is_sum_of_real_frame_sizes() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();
    let tmp = tempfile::tempdir().unwrap();
    let root_id = "src-raw";
    register_source(pool, root_id, tmp.path().to_str().unwrap()).await;

    write_fits(
        tmp.path(),
        "s1.fits",
        "Light Frame",
        Some("M 31"),
        Some("Ha"),
        Some("2026-06-21T22:00:00"),
    );
    write_fits(
        tmp.path(),
        "s2.fits",
        "Light Frame",
        Some("M 31"),
        Some("Ha"),
        Some("2026-06-21T23:00:00"),
    );
    // Pad the second frame by one FITS block so the two sizes differ.
    std::fs::OpenOptions::new()
        .append(true)
        .open(tmp.path().join("s2.fits"))
        .unwrap()
        .write_all(&[0u8; 2880])
        .unwrap();

    let expected_total: u64 = ["s1.fits", "s2.fits"]
        .iter()
        .map(|name| std::fs::metadata(tmp.path().join(name)).unwrap().len())
        .sum();

    build_applied_plan(pool, "plan-3", root_id, &["s1.fits", "s2.fits"]).await;

    app_core::inbox::plan_listener::start_inbox_plan_listener(
        pool.clone(),
        &bus,
        targeting_resolver::simbad::ResolveCache::in_memory().unwrap(),
    );
    publish_applied(&bus, "plan-3").await;
    support::poll_until(
        || async {
            let rows = session_rows(pool).await;
            let ready = rows.iter().any(|(_id, frame_ids, _ct)| {
                let frames: Vec<String> = serde_json::from_str(frame_ids).unwrap_or_default();
                frames.len() >= 2
            });
            ready.then_some(())
        },
        "acquisition_session with 2 frames never appeared after plan-3 apply-completed event",
    )
    .await;

    let listed = app_core::sessions::list_sessions(pool).await.unwrap();
    assert_eq!(listed.len(), 1, "both frames share one capture identity");
    assert_eq!(listed[0].frame_count, 2);
    assert_eq!(
        listed[0].total_size_bytes, expected_total,
        "list total must be the sum of the real on-disk frame sizes"
    );

    let detail = app_core::sessions::get_session(pool, &listed[0].id).await.unwrap();
    assert_eq!(
        detail.total_size_bytes, expected_total,
        "detail total must match the same real sum"
    );
}
