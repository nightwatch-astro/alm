#![allow(clippy::doc_markdown)]
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Layer-1 integration tests for calibration matching & masters — feature 037 (T005 coverage area #5).
//!
//! Tests use the shared harness (`support::setup`) which provides a real in-memory
//! SQLite DB with all migrations applied. No mocks; all assertions are against
//! persisted state read back from the database.
//!
//! Coverage:
//! - `suggest`: session+fingerprint seeded → candidates returned.
//! - `suggest`: session without a fingerprint row degrades gracefully (#867)
//!   instead of hard-blocking with observer_location_missing.
//! - `assign`: happy-path → assignment persists and audit event emitted.
//! - `unassign` (#875): removes a persisted assignment, returning the session
//!   to "no master assigned" for that type.
//! - `batch_suggest`: two sessions, one with matching master → results per session.

mod support;

use app_core::calibration::{assign, batch_suggest, suggest, unassign};
use contracts_core::calibration_match::{
    CalibrationMatchAssignRequest, CalibrationMatchBatchRequest, CalibrationMatchSuggestRequest,
    CalibrationMatchUnassignRequest, CalibrationType, SuggestStatus, ASSIGN_CONTRACT_VERSION,
    BATCH_CONTRACT_VERSION, SUGGEST_CONTRACT_VERSION, UNASSIGN_CONTRACT_VERSION,
};
use uuid::Uuid;

// ── Seed helpers ──────────────────────────────────────────────────────────────

/// Insert a minimal `acquisition_session` row.
async fn insert_acq_session(pool: &sqlx::SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO acquisition_session \
         (id, session_key, frame_ids, created_at) \
         VALUES (?, ?, '[]', '2026-05-01T00:00:00Z')",
    )
    .bind(id)
    .bind(format!("key-{id}"))
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert acquisition_session failed: {e}"));
}

/// Insert an `acquisition_fingerprint` row with observer-location flags set so
/// the A6 guard passes. Both `gain` and `offset_val` are hard-rule dimensions
/// for dark matching and must be supplied to produce candidates.
async fn insert_acq_fingerprint(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    gain: f64,
    offset: f64,
    temp_c: f64,
    binning: &str,
) {
    sqlx::query(
        "INSERT INTO acquisition_fingerprint \
         (id, session_type, gain, offset_val, temp_c, binning, \
          has_observer_location, has_exposure_start_utc, \
          observing_night_date) \
         VALUES (?, 'light', ?, ?, ?, ?, 1, 1, '2026-05-01')",
    )
    .bind(session_id)
    .bind(gain)
    .bind(offset)
    .bind(temp_c)
    .bind(binning)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert acquisition_fingerprint failed: {e}"));
}

/// Insert a minimal `calibration_session` row (required FK for `calibration_fingerprint`).
async fn insert_cal_session(pool: &sqlx::SqlitePool, id: &str, kind: &str) {
    sqlx::query(
        "INSERT INTO calibration_session \
         (id, session_key, frame_ids, kind, created_at) \
         VALUES (?, ?, '[]', ?, '2026-05-01T00:00:00Z')",
    )
    .bind(id)
    .bind(format!("calkey-{id}"))
    .bind(kind)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert calibration_session failed: {e}"));
}

/// Insert a `calibration_fingerprint` (master) row. `offset_val` is a hard
/// rule dimension alongside `gain`; both must match the session exactly.
async fn insert_cal_fingerprint(
    pool: &sqlx::SqlitePool,
    id: &str,
    kind: &str,
    gain: f64,
    offset: f64,
    temp_c: f64,
    binning: &str,
) {
    sqlx::query(
        "INSERT INTO calibration_fingerprint \
         (id, calibration_type, gain, offset_val, temp_c, binning) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(kind)
    .bind(gain)
    .bind(offset)
    .bind(temp_c)
    .bind(binning)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert calibration_fingerprint failed: {e}"));
}

/// Insert a `canonical_target` row and link it to an acquisition session via
/// `canonical_target_id` (spec P9 context-enrichment source).
async fn link_canonical_target(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    target_id: &str,
    designation: &str,
) {
    sqlx::query(
        "INSERT INTO canonical_target \
         (id, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at) \
         VALUES (?, ?, 'galaxy', 10.0, 20.0, 'seed', '2026-01-01T00:00:00Z')",
    )
    .bind(target_id)
    .bind(designation)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert canonical_target failed: {e}"));

    sqlx::query("UPDATE acquisition_session SET canonical_target_id = ? WHERE id = ?")
        .bind(target_id)
        .bind(session_id)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("link canonical_target_id failed: {e}"));
}

/// Overwrite `frame_ids` on an acquisition session so the `frame_count`
/// enrichment (`json_array_length`) resolves to a known value.
async fn set_frame_ids(pool: &sqlx::SqlitePool, session_id: &str, frame_ids_json: &str) {
    sqlx::query("UPDATE acquisition_session SET frame_ids = ? WHERE id = ?")
        .bind(frame_ids_json)
        .bind(session_id)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("set frame_ids failed: {e}"));
}

/// Set `filter_name` on an `acquisition_fingerprint` row.
async fn set_filter(pool: &sqlx::SqlitePool, session_id: &str, filter: &str) {
    sqlx::query("UPDATE acquisition_fingerprint SET filter_name = ? WHERE id = ?")
        .bind(filter)
        .bind(session_id)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("set filter_name failed: {e}"));
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Seeding a light session + matching dark master returns candidates via `suggest`.
#[tokio::test]
async fn suggest_returns_candidates_when_master_matches() {
    let (db, _repo, _bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();

    // Seed light session with a fingerprint (gain=100, offset=10, temp=-10, binning=1x1).
    insert_acq_session(pool, &session_id).await;
    insert_acq_fingerprint(pool, &session_id, 100.0, 10.0, -10.0, "1x1").await;

    // Seed a matching dark master with identical hard-rule dimensions.
    insert_cal_session(pool, &master_id, "dark").await;
    insert_cal_fingerprint(pool, &master_id, "dark", 100.0, 10.0, -10.0, "1x1").await;

    let req = CalibrationMatchSuggestRequest {
        contract_version: SUGGEST_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        calibration_types: Some(vec![CalibrationType::Dark]),
    };

    let resp = suggest(pool, req).await.expect("suggest should not return Err");

    assert_eq!(resp.status, "success", "expected success, got: {:?}", resp.error);
    let matches = resp.matches.expect("expected Some(matches)");
    assert!(!matches.is_empty(), "expected at least one candidate match");

    // The top candidate should reference our master.
    assert!(
        matches.iter().any(|m| m.master_id == master_id),
        "expected master_id={master_id} in candidates, got: {matches:?}",
    );

    // suggest_status should be Match or Ambiguous (not NoMatch).
    let status = resp.suggest_status.expect("expected Some(suggest_status)");
    assert!(
        matches!(status, SuggestStatus::Match | SuggestStatus::Ambiguous),
        "expected Match or Ambiguous, got {status:?}",
    );
}

/// Set `camera_body_id` on an existing `acquisition_fingerprint` row.
async fn set_session_body(pool: &sqlx::SqlitePool, id: &str, body: &str) {
    sqlx::query("UPDATE acquisition_fingerprint SET camera_body_id = ? WHERE id = ?")
        .bind(body)
        .bind(id)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("set session camera_body_id failed: {e}"));
}

/// Set `camera_body_id` on an existing `calibration_fingerprint` row.
async fn set_master_body(pool: &sqlx::SqlitePool, id: &str, body: &str) {
    sqlx::query("UPDATE calibration_fingerprint SET camera_body_id = ? WHERE id = ?")
        .bind(body)
        .bind(id)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("set master camera_body_id failed: {e}"));
}

/// astro-plan-ugux2 end-to-end: the `camera_body_id` column must survive the
/// round trip through `acquisition_fingerprint`/`calibration_fingerprint` and the
/// matcher loaders, so a master captured with a different physical body is
/// excluded rather than silently offered. Two masters differ only in body id.
#[tokio::test]
async fn suggest_excludes_a_master_from_a_different_camera_body() {
    let (db, _repo, _bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();
    let same_body_master = Uuid::new_v4().to_string();
    let other_body_master = Uuid::new_v4().to_string();

    insert_acq_session(pool, &session_id).await;
    insert_acq_fingerprint(pool, &session_id, 100.0, 10.0, -10.0, "1x1").await;
    set_session_body(pool, &session_id, "player one_serial-a").await;

    for (master_id, body) in
        [(&same_body_master, "player one_serial-a"), (&other_body_master, "player one_serial-b")]
    {
        insert_cal_session(pool, master_id, "dark").await;
        insert_cal_fingerprint(pool, master_id, "dark", 100.0, 10.0, -10.0, "1x1").await;
        set_master_body(pool, master_id, body).await;
    }

    let resp = suggest(
        pool,
        CalibrationMatchSuggestRequest {
            contract_version: SUGGEST_CONTRACT_VERSION.to_owned(),
            request_id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            calibration_types: Some(vec![CalibrationType::Dark]),
        },
    )
    .await
    .expect("suggest should not return Err");

    assert_eq!(resp.status, "success", "expected success, got: {:?}", resp.error);
    let matches = resp.matches.expect("expected Some(matches)");
    assert!(
        matches.iter().any(|m| m.master_id == same_body_master),
        "the master from the same body was not offered: {matches:?}"
    );
    assert!(
        !matches.iter().any(|m| m.master_id == other_body_master),
        "a master from a different camera body was offered: {matches:?}"
    );
}

/// T134 (Q16 / FR-136) regression: calibration matching runs on the
/// Option-typed domain `SessionInfo`/`MasterInfo`, loaded straight from
/// `acquisition_fingerprint`/`calibration_fingerprint` — never through the
/// contract DTOs de-zeroed by T128/T129. Absent gain (a hard-rule dimension
/// for dark matching) must still be handled deterministically; it must
/// never be silently treated as a real 0 that spuriously matches a master
/// whose gain is also absent (the exact failure mode de-zeroing the
/// contract could have introduced had matching read the DTO instead of the
/// domain-layer Option fields).
#[tokio::test]
async fn suggest_deterministic_when_gain_absent_on_both_sides() {
    let (db, _repo, _bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();

    insert_acq_session(pool, &session_id).await;
    // Fingerprint with NO gain/offset_val (both NULL) — every other hard-rule
    // dimension matches the master below.
    sqlx::query(
        "INSERT INTO acquisition_fingerprint \
         (id, session_type, temp_c, binning, has_observer_location, has_exposure_start_utc) \
         VALUES (?, 'light', -10.0, '1x1', 1, 1)",
    )
    .bind(&session_id)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert acquisition_fingerprint failed: {e}"));

    insert_cal_session(pool, &master_id, "dark").await;
    // Master fingerprint ALSO with NO gain/offset_val.
    sqlx::query(
        "INSERT INTO calibration_fingerprint (id, calibration_type, temp_c, binning) \
         VALUES (?, 'dark', -10.0, '1x1')",
    )
    .bind(&master_id)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert calibration_fingerprint failed: {e}"));

    let req = CalibrationMatchSuggestRequest {
        contract_version: SUGGEST_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        calibration_types: Some(vec![CalibrationType::Dark]),
    };

    let resp = suggest(pool, req).await.expect("suggest should not return Err");
    assert_eq!(resp.status, "success", "expected success, got: {:?}", resp.error);
    let matches = resp.matches.expect("expected Some(matches)");
    assert!(
        matches.iter().all(|m| m.master_id != master_id),
        "absent gain must never be silently synthesized as a real 0 that \
         spuriously matches another absent-gain master; got: {matches:?}",
    );
}

/// Session with no fingerprint row (no observer location) no longer hard-blocks
/// suggest (#867): it degrades to a normal no-match result instead of the
/// observer_location_missing guard error.
#[tokio::test]
async fn suggest_degrades_gracefully_when_no_fingerprint() {
    let (db, _repo, _bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();

    // Seed the session but intentionally omit the fingerprint row so that
    // has_observer_location/has_exposure_start_utc default to false.
    insert_acq_session(pool, &session_id).await;

    let req = CalibrationMatchSuggestRequest {
        contract_version: SUGGEST_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        calibration_types: None,
    };

    let resp = suggest(pool, req).await.expect("suggest should not return Err");

    assert_eq!(resp.status, "success", "expected success, got: {:?}", resp.error);

    // No masters were seeded, so the degraded session still resolves to
    // NoMatch rather than the old ObserverLocationMissing hard guard.
    let status = resp.suggest_status;
    assert!(matches!(status, Some(SuggestStatus::NoMatch)), "expected NoMatch, got {status:?}");
}

/// Happy-path `assign`: persists the assignment row and emits an audit event.
#[tokio::test]
async fn assign_persists_assignment_and_emits_audit_event() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();

    // Seed matching light session + dark master (all hard-rule dimensions identical).
    insert_acq_session(pool, &session_id).await;
    insert_acq_fingerprint(pool, &session_id, 200.0, 20.0, -15.0, "2x2").await;
    insert_cal_session(pool, &master_id, "dark").await;
    insert_cal_fingerprint(pool, &master_id, "dark", 200.0, 20.0, -15.0, "2x2").await;

    let req = CalibrationMatchAssignRequest {
        contract_version: ASSIGN_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        master_id: master_id.clone(),
        r#override: false,
    };

    let resp = assign(pool, &bus, req).await.expect("assign should not return Err");

    assert_eq!(resp.status, "success", "expected success, got: {:?}", resp.error);

    let assigned = resp.assigned.expect("expected Some(assigned)");
    assert_eq!(assigned.session_id, session_id);
    assert_eq!(assigned.master_id, master_id);
    assert!(!assigned.assignment_id.is_empty(), "assignment_id must be non-empty");

    // Verify the row was actually written to SQLite.
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM calibration_assignment \
         WHERE session_id = ? AND master_id = ? AND calibration_type = 'dark'",
    )
    .bind(&session_id)
    .bind(&master_id)
    .fetch_one(pool)
    .await
    .expect("calibration_assignment query failed");

    assert_eq!(count, 1, "expected 1 persisted assignment row, found {count}");

    // Verify an audit event was recorded for this assignment.
    // EventBus::publish writes to the `events` table (migration 0003) with
    // column `topic`, not `audit_log_entry`.
    let (audit_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM events \
         WHERE topic = 'calibration.assignment.created'",
    )
    .fetch_one(pool)
    .await
    .expect("events query failed");

    assert_eq!(audit_count, 1, "expected 1 audit event for assignment, found {audit_count}");
}

/// #718 (spec 007 SC-003): an override assignment's `was_override` flag must
/// survive a reopen — read back through the session detail path (the same
/// one the UI hits), not just the raw DB row.
#[tokio::test]
async fn assign_override_flag_is_distinguishable_on_reopen() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();
    let other_master_id = Uuid::new_v4().to_string();

    // Session + a dark master with a mismatched hard-rule dimension (gain),
    // requiring override=true. A second, non-overridden assignment for a
    // different calibration type on the same session acts as a control.
    insert_acq_session(pool, &session_id).await;
    insert_acq_fingerprint(pool, &session_id, 200.0, 20.0, -15.0, "2x2").await;
    insert_cal_session(pool, &master_id, "dark").await;
    insert_cal_fingerprint(pool, &master_id, "dark", 999.0, 20.0, -15.0, "2x2").await;
    insert_cal_session(pool, &other_master_id, "bias").await;
    insert_cal_fingerprint(pool, &other_master_id, "bias", 200.0, 20.0, -15.0, "2x2").await;

    let override_req = CalibrationMatchAssignRequest {
        contract_version: ASSIGN_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        master_id: master_id.clone(),
        r#override: true,
    };
    let override_resp =
        assign(pool, &bus, override_req).await.expect("override assign should not return Err");
    assert_eq!(override_resp.status, "success", "expected success, got: {:?}", override_resp.error);

    let normal_req = CalibrationMatchAssignRequest {
        contract_version: ASSIGN_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        master_id: other_master_id.clone(),
        r#override: false,
    };
    let normal_resp =
        assign(pool, &bus, normal_req).await.expect("normal assign should not return Err");
    assert_eq!(normal_resp.status, "success", "expected success, got: {:?}", normal_resp.error);

    let detail =
        app_core::sessions::get_session(pool, &session_id).await.expect("session must exist");

    let dark_match = detail
        .calibration_matches
        .iter()
        .find(|m| m.master_id == master_id)
        .expect("expected the override assignment in calibration_matches");
    assert!(dark_match.was_override, "override assignment must reopen with was_override=true");

    let bias_match = detail
        .calibration_matches
        .iter()
        .find(|m| m.master_id == other_master_id)
        .expect("expected the normal assignment in calibration_matches");
    assert!(!bias_match.was_override, "normal assignment must reopen with was_override=false");
}

/// #875: `unassign` removes a persisted assignment, returning the session to
/// "no master assigned" for that calibration type.
#[tokio::test]
async fn unassign_removes_persisted_assignment() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();

    insert_acq_session(pool, &session_id).await;
    insert_acq_fingerprint(pool, &session_id, 200.0, 20.0, -15.0, "2x2").await;
    insert_cal_session(pool, &master_id, "dark").await;
    insert_cal_fingerprint(pool, &master_id, "dark", 200.0, 20.0, -15.0, "2x2").await;

    let assign_req = CalibrationMatchAssignRequest {
        contract_version: ASSIGN_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        master_id: master_id.clone(),
        r#override: false,
    };
    let assign_resp = assign(pool, &bus, assign_req).await.expect("assign should not return Err");
    assert_eq!(assign_resp.status, "success", "expected success, got: {:?}", assign_resp.error);

    let unassign_req = CalibrationMatchUnassignRequest {
        contract_version: UNASSIGN_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        calibration_type: CalibrationType::Dark,
    };
    let unassign_resp =
        unassign(pool, &bus, unassign_req).await.expect("unassign should not return Err");
    assert_eq!(unassign_resp.status, "success", "expected success, got: {:?}", unassign_resp.error);

    let detail =
        app_core::sessions::get_session(pool, &session_id).await.expect("session must exist");
    assert!(
        detail.calibration_matches.iter().all(|m| m.master_id != master_id),
        "assignment must be gone from the session detail after unassign"
    );

    // Un-assigning again (nothing left to remove) surfaces a clear error, not a silent success.
    let unassign_again_req = CalibrationMatchUnassignRequest {
        contract_version: UNASSIGN_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id,
        calibration_type: CalibrationType::Dark,
    };
    let again = unassign(pool, &bus, unassign_again_req).await.expect("should not return Err");
    assert_eq!(again.status, "error");
    assert_eq!(again.error.expect("expected error details").code, "assignment.not_found");
}

/// #1120: assign/unassign history must land in `audit_log_entry`, the
/// authoritative durable record — not only in the non-authoritative `events`
/// table that `bus.publish` writes. Asserted on the durable table directly,
/// because an `events` row satisfies the old (weak) check either way.
#[tokio::test]
async fn assign_and_unassign_write_durable_audit_log_entries() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();

    insert_acq_session(pool, &session_id).await;
    insert_acq_fingerprint(pool, &session_id, 200.0, 20.0, -15.0, "2x2").await;
    insert_cal_session(pool, &master_id, "dark").await;
    insert_cal_fingerprint(pool, &master_id, "dark", 200.0, 20.0, -15.0, "2x2").await;

    let assign_resp = assign(
        pool,
        &bus,
        CalibrationMatchAssignRequest {
            contract_version: ASSIGN_CONTRACT_VERSION.to_owned(),
            request_id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            master_id: master_id.clone(),
            r#override: false,
        },
    )
    .await
    .expect("assign should not return Err");
    assert_eq!(assign_resp.status, "success", "expected success, got: {:?}", assign_resp.error);
    let assignment_id = assign_resp.assigned.expect("expected Some(assigned)").assignment_id;

    let (created_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log_entry \
         WHERE trigger = 'calibration.assignment.created' AND entity_type = 'calibration' \
           AND outcome = 'applied' AND payload LIKE ?",
    )
    .bind(format!("%{assignment_id}%"))
    .fetch_one(pool)
    .await
    .expect("audit_log_entry query failed");
    assert_eq!(created_count, 1, "assign must write exactly 1 durable audit row");

    let unassign_resp = unassign(
        pool,
        &bus,
        CalibrationMatchUnassignRequest {
            contract_version: UNASSIGN_CONTRACT_VERSION.to_owned(),
            request_id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            calibration_type: CalibrationType::Dark,
        },
    )
    .await
    .expect("unassign should not return Err");
    assert_eq!(unassign_resp.status, "success", "expected success, got: {:?}", unassign_resp.error);

    let (removed_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log_entry \
         WHERE trigger = 'calibration.assignment.removed' AND entity_type = 'calibration' \
           AND outcome = 'applied' AND payload LIKE ?",
    )
    .bind(format!("%{assignment_id}%"))
    .fetch_one(pool)
    .await
    .expect("audit_log_entry query failed");
    assert_eq!(removed_count, 1, "unassign must write exactly 1 durable audit row");

    // Both mutations must resolve to the same audit entity, so an assignment's
    // full history is reachable from one entity_id.
    let (entity_ids,): (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT entity_id) FROM audit_log_entry WHERE entity_type = 'calibration'",
    )
    .fetch_one(pool)
    .await
    .expect("audit_log_entry query failed");
    assert_eq!(entity_ids, 1, "create+remove must share one entity_id");
}

/// `batch_suggest` across two sessions: one with a matching master, one unknown.
#[tokio::test]
async fn batch_suggest_returns_results_and_errors_per_session() {
    let (db, _repo, _bus) = support::setup().await;
    let pool = db.pool();

    let session_a = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();
    let missing_session = Uuid::new_v4().to_string(); // never inserted → not_found

    // Session A has a fingerprint + a matching bias master.
    insert_acq_session(pool, &session_a).await;
    insert_acq_fingerprint(pool, &session_a, 50.0, 5.0, -5.0, "1x1").await;
    insert_cal_session(pool, &master_id, "bias").await;
    insert_cal_fingerprint(pool, &master_id, "bias", 50.0, 5.0, -5.0, "1x1").await;

    let req = CalibrationMatchBatchRequest {
        contract_version: BATCH_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_ids: vec![session_a.clone(), missing_session.clone()],
        calibration_types: Some(vec![CalibrationType::Bias]),
    };

    let resp = batch_suggest(pool, req).await.expect("batch_suggest should not return Err");

    // Top-level status: partial (results for A + error for missing session).
    assert_eq!(resp.status, "partial", "expected partial (results + errors), got {}", resp.status);

    // Session A should appear in results.
    let results = resp.results.expect("expected Some(results)");
    assert!(
        results.iter().any(|r| r.session_id == session_a),
        "expected session_a in results, got {results:?}",
    );

    // Missing session should appear in errors with session.not_found.
    let errors = resp.errors.expect("expected Some(errors)");
    assert!(
        errors.iter().any(|e| e.session_id.as_deref() == Some(&missing_session)),
        "expected missing_session in errors, got {errors:?}",
    );
    assert!(
        errors.iter().any(|e| e.code == "session.not_found"),
        "expected session.not_found error code, got {errors:?}",
    );
}

/// P9: `suggest` enriches each candidate DTO with session context (target,
/// filter, observing night, frame count) via one batched lookup, keyed by
/// `session_id`. A field with no backing data (filter is left unset here)
/// stays `None` rather than defaulting to a placeholder.
#[tokio::test]
async fn suggest_enriches_candidates_with_session_context() {
    let (db, _repo, _bus) = support::setup().await;
    let pool = db.pool();

    let session_id = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();
    let target_id = Uuid::new_v4().to_string();

    insert_acq_session(pool, &session_id).await;
    insert_acq_fingerprint(pool, &session_id, 100.0, 10.0, -10.0, "1x1").await;
    link_canonical_target(pool, &session_id, &target_id, "M 31").await;
    set_frame_ids(pool, &session_id, r#"["f1","f2","f3","f4"]"#).await;

    insert_cal_session(pool, &master_id, "dark").await;
    insert_cal_fingerprint(pool, &master_id, "dark", 100.0, 10.0, -10.0, "1x1").await;

    let req = CalibrationMatchSuggestRequest {
        contract_version: SUGGEST_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        calibration_types: Some(vec![CalibrationType::Dark]),
    };

    let resp = suggest(pool, req).await.expect("suggest should not return Err");
    let matches = resp.matches.expect("expected Some(matches)");

    let candidate =
        matches.iter().find(|m| m.master_id == master_id).expect("expected master in candidates");
    assert_eq!(candidate.target_name.as_deref(), Some("M 31"));
    // insert_acq_fingerprint hard-codes observing_night_date but leaves
    // filter_name unset — the enrichment must not fabricate a value.
    assert_eq!(candidate.acquisition_night.as_deref(), Some("2026-05-01"));
    assert_eq!(candidate.filter, None, "filter_name was never set — must stay None");
    assert_eq!(candidate.frame_count, Some(4));
}

/// P9 batch coverage: known sessions get enriched independently (one linked
/// to a canonical target, one not), and an id with no matching session at all
/// neither breaks the batched context lookup nor gets a phantom context row —
/// it still surfaces through the existing `session.not_found` error path.
#[tokio::test]
async fn batch_suggest_enriches_known_sessions_and_ignores_missing_ids() {
    let (db, _repo, _bus) = support::setup().await;
    let pool = db.pool();

    let session_a = Uuid::new_v4().to_string();
    let session_b = Uuid::new_v4().to_string();
    let master_id = Uuid::new_v4().to_string();
    let target_id = Uuid::new_v4().to_string();
    let missing_session = Uuid::new_v4().to_string(); // never inserted → not_found

    // Session A: full context (canonical target + known frame count).
    insert_acq_session(pool, &session_a).await;
    insert_acq_fingerprint(pool, &session_a, 50.0, 5.0, -5.0, "1x1").await;
    link_canonical_target(pool, &session_a, &target_id, "NGC 7000").await;
    set_frame_ids(pool, &session_a, r#"["f1","f2"]"#).await;
    set_filter(pool, &session_a, "Ha").await;

    // Session B: matches the same master, but has no canonical_target_id
    // link — target_name must resolve to None while frame_count (from the
    // default `frame_ids = '[]'`) still resolves to 0, not None.
    insert_acq_session(pool, &session_b).await;
    insert_acq_fingerprint(pool, &session_b, 50.0, 5.0, -5.0, "1x1").await;

    insert_cal_session(pool, &master_id, "bias").await;
    insert_cal_fingerprint(pool, &master_id, "bias", 50.0, 5.0, -5.0, "1x1").await;

    let req = CalibrationMatchBatchRequest {
        contract_version: BATCH_CONTRACT_VERSION.to_owned(),
        request_id: Uuid::new_v4().to_string(),
        session_ids: vec![session_a.clone(), session_b.clone(), missing_session.clone()],
        calibration_types: Some(vec![CalibrationType::Bias]),
    };

    let resp = batch_suggest(pool, req).await.expect("batch_suggest should not return Err");
    let results = resp.results.expect("expected Some(results)");

    let result_a =
        results.iter().find(|r| r.session_id == session_a).expect("expected session_a result");
    let candidates_a = result_a.candidates.as_ref().expect("expected candidates for session_a");
    assert_eq!(candidates_a[0].target_name.as_deref(), Some("NGC 7000"));
    assert_eq!(candidates_a[0].filter.as_deref(), Some("Ha"));
    assert_eq!(candidates_a[0].frame_count, Some(2));

    let result_b =
        results.iter().find(|r| r.session_id == session_b).expect("expected session_b result");
    let candidates_b = result_b.candidates.as_ref().expect("expected candidates for session_b");
    assert_eq!(candidates_b[0].target_name, None, "no canonical_target_id linked → None");
    assert_eq!(candidates_b[0].frame_count, Some(0), "default frame_ids '[]' → 0, not None");

    // The unresolvable id must not appear anywhere in results and must still
    // surface via the pre-existing session.not_found error path.
    assert!(!results.iter().any(|r| r.session_id == missing_session));
    let errors = resp.errors.expect("expected Some(errors)");
    assert!(errors.iter().any(|e| e.session_id.as_deref() == Some(&missing_session)));
}
