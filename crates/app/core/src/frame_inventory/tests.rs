// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

use super::relink::sha256_hex;
use super::*;
use app_core_targets::frame_writer::upsert_frame_record;
use contracts_core::inventory_frame::{
    FramePresenceState, InventoryFrameListRequest, InventoryFrameListScope,
    InventoryFrameRelinkRequest, InventoryReconcileRunRequest, ReconcileMode, ReconcileReason,
};
use persistence_core::Database;

async fn test_db() -> Database {
    let db = Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    db
}

async fn insert_root(pool: &SqlitePool, id: &str, path: &str) {
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
         VALUES (?, ?, ?, 'local', 'active', datetime('now'))",
    )
    .bind(id)
    .bind(id)
    .bind(path)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_acquisition_session(pool: &SqlitePool, id: &str, frame_ids: &[&str]) {
    let frame_ids_json = serde_json::to_string(frame_ids).unwrap();
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at)
         VALUES (?, '{}', ?, datetime('now'))",
    )
    .bind(id)
    .bind(frame_ids_json)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn list_frames_by_session_excludes_missing_by_default() {
    let db = test_db().await;
    insert_root(db.pool(), "root-1", "/tmp").await;
    let f1 =
        upsert_frame_record(db.pool(), "root-1", "a.fits", 100, "t0", "classified").await.unwrap();
    let f2 =
        upsert_frame_record(db.pool(), "root-1", "b.fits", 200, "t0", "missing").await.unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&f1, &f2]).await;

    let req = InventoryFrameListRequest {
        scope: InventoryFrameListScope { session_id: Some("sess-1".to_owned()), root_id: None },
        include_missing: None,
    };
    let resp = list_frames(db.pool(), &req).await.unwrap();

    assert_eq!(resp.frames.len(), 1);
    assert_eq!(resp.present_count, 1);
    assert_eq!(resp.present_size_bytes, 100);
}

#[tokio::test]
async fn list_frames_by_session_includes_missing_when_requested() {
    let db = test_db().await;
    insert_root(db.pool(), "root-1", "/tmp").await;
    let f1 =
        upsert_frame_record(db.pool(), "root-1", "a.fits", 100, "t0", "missing").await.unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&f1]).await;

    let req = InventoryFrameListRequest {
        scope: InventoryFrameListScope { session_id: Some("sess-1".to_owned()), root_id: None },
        include_missing: Some(true),
    };
    let resp = list_frames(db.pool(), &req).await.unwrap();

    assert_eq!(resp.frames.len(), 1);
    assert_eq!(resp.frames[0].state, FramePresenceState::Missing);
    assert_eq!(resp.present_count, 0);
}

#[tokio::test]
async fn reconcile_run_backfills_zero_size_and_reports_missing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("present.fits"), vec![0u8; 1024]).unwrap();
    // "deleted.fits" intentionally not written — simulates an external delete.

    let db = test_db().await;
    insert_root(db.pool(), "root-1", dir.path().to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    upsert_frame_record(db.pool(), "root-1", "present.fits", 0, "t0", "classified").await.unwrap();
    upsert_frame_record(db.pool(), "root-1", "deleted.fits", 4096, "t0", "classified")
        .await
        .unwrap();

    let req = InventoryReconcileRunRequest {
        root_id: "root-1".to_owned(),
        reason: ReconcileReason::OnDemand,
    };
    let resp = run_reconcile(db.pool(), &bus, &req).await.unwrap();

    assert_eq!(resp.scanned, 2);
    assert_eq!(resp.present, 1);
    assert_eq!(resp.newly_missing, 1);
    assert_eq!(resp.size_backfilled, 1);

    let (size, state): (i64, String) =
        sqlx::query_as("SELECT size_bytes, state FROM file_record WHERE relative_path = ?")
            .bind("present.fits")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(size, 1024);
    assert_eq!(state, "classified");

    let (deleted_state,): (String,) =
        sqlx::query_as("SELECT state FROM file_record WHERE relative_path = ?")
            .bind("deleted.fits")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(deleted_state, "missing");
}

#[tokio::test]
async fn reconcile_run_recovers_previously_missing_frame() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("back.fits"), vec![0u8; 512]).unwrap();

    let db = test_db().await;
    insert_root(db.pool(), "root-1", dir.path().to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    upsert_frame_record(db.pool(), "root-1", "back.fits", 512, "t0", "missing").await.unwrap();

    let req = InventoryReconcileRunRequest {
        root_id: "root-1".to_owned(),
        reason: ReconcileReason::OnDemand,
    };
    let resp = run_reconcile(db.pool(), &bus, &req).await.unwrap();

    assert_eq!(resp.recovered, 1);

    let (state,): (String,) =
        sqlx::query_as("SELECT state FROM file_record WHERE relative_path = ?")
            .bind("back.fits")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(state, "classified");
}

#[tokio::test]
async fn reconcile_run_unregistered_root_returns_root_unavailable() {
    let db = test_db().await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());
    let req = InventoryReconcileRunRequest {
        root_id: "no-such-root".to_owned(),
        reason: ReconcileReason::OnDemand,
    };
    let err = run_reconcile(db.pool(), &bus, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::RootUnavailable);
}

// ── Tier-1 custody: a missing frame keeps both its record and its session
// attribution, in either reconcile mode (astro-plan-2uwa) ────────────────

#[tokio::test]
async fn auto_reconcile_mode_retains_record_and_session_attribution() {
    use app_core_settings::root_config::set_root_config;
    use contracts_core::inventory_frame::RootConfigSetRequest;

    let dir = tempfile::tempdir().unwrap();
    // "gone.fits" intentionally never written — simulates an external delete.

    let db = test_db().await;
    insert_root(db.pool(), "root-1", dir.path().to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    let frame_id = upsert_frame_record(db.pool(), "root-1", "gone.fits", 100, "t0", "classified")
        .await
        .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&frame_id]).await;

    set_root_config(
        db.pool(),
        &RootConfigSetRequest {
            root_id: "root-1".to_owned(),
            reconcile_mode: Some(ReconcileMode::AutoReconcile),
            detection: None,
        },
    )
    .await
    .unwrap();

    let req = InventoryReconcileRunRequest {
        root_id: "root-1".to_owned(),
        reason: ReconcileReason::OnDemand,
    };
    run_reconcile(db.pool(), &bus, &req).await.unwrap();

    // Retained: the file_record row still exists, marked missing.
    let (state,): (String,) = sqlx::query_as("SELECT state FROM file_record WHERE id = ?")
        .bind(&frame_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "missing", "auto-reconcile must never hard-delete the record (INV-4)");

    let (frame_ids_json,): (String,) =
        sqlx::query_as("SELECT frame_ids FROM acquisition_session WHERE id = 'sess-1'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let ids: Vec<String> = serde_json::from_str(&frame_ids_json).unwrap();
    assert!(
        ids.contains(&frame_id),
        "frame-to-session attribution is Tier-1 user knowledge and must survive a missing frame"
    );

    // Still queryable with include_missing via the root scope (INV-4).
    let list_req = InventoryFrameListRequest {
        scope: InventoryFrameListScope { session_id: None, root_id: Some("root-1".to_owned()) },
        include_missing: Some(true),
    };
    let listed = list_frames(db.pool(), &list_req).await.unwrap();
    assert_eq!(listed.frames.len(), 1);
    assert_eq!(listed.frames[0].state, FramePresenceState::Missing);
}

#[tokio::test]
async fn flag_missing_mode_retains_frame_in_session_membership() {
    // Default mode (flag_missing): the id stays in frame_ids even after
    // going missing — only the `state != 'missing'` filter excludes it
    // from active counts/totals.
    let dir = tempfile::tempdir().unwrap();
    let db = test_db().await;
    insert_root(db.pool(), "root-1", dir.path().to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    let frame_id = upsert_frame_record(db.pool(), "root-1", "gone.fits", 100, "t0", "classified")
        .await
        .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&frame_id]).await;

    let req = InventoryReconcileRunRequest {
        root_id: "root-1".to_owned(),
        reason: ReconcileReason::OnDemand,
    };
    run_reconcile(db.pool(), &bus, &req).await.unwrap();

    let (frame_ids_json,): (String,) =
        sqlx::query_as("SELECT frame_ids FROM acquisition_session WHERE id = 'sess-1'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let ids: Vec<String> = serde_json::from_str(&frame_ids_json).unwrap();
    assert!(ids.contains(&frame_id), "flag-missing must retain the id in the array");
}

/// Regression (astro-plan-2uwa): a root that vanished — an unmounted external
/// drive — must abort the pass. Before the availability guard the empty disk
/// listing was read as "every frame deleted", which marked every record
/// missing on an unattended timer.
#[tokio::test]
async fn unavailable_root_aborts_reconcile_without_touching_frame_state() {
    let dir = tempfile::tempdir().unwrap();
    let root_path = dir.path().join("external_drive");
    std::fs::create_dir_all(&root_path).unwrap();
    std::fs::write(root_path.join("light_001.fits"), vec![0u8; 100]).unwrap();

    let db = test_db().await;
    insert_root(db.pool(), "root-1", root_path.to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    let frame_id =
        upsert_frame_record(db.pool(), "root-1", "light_001.fits", 100, "t0", "classified")
            .await
            .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&frame_id]).await;

    // The drive unmounts between passes.
    std::fs::remove_dir_all(&root_path).unwrap();

    let req = InventoryReconcileRunRequest {
        root_id: "root-1".to_owned(),
        reason: ReconcileReason::Scheduled,
    };
    let err = run_reconcile(db.pool(), &bus, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::RootUnavailable);
    assert!(err.retryable, "the root may come back; the caller should retry, not act");

    let (state,): (String,) = sqlx::query_as("SELECT state FROM file_record WHERE id = ?")
        .bind(&frame_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "classified", "an absent root is not evidence that a frame was deleted");

    let (frame_ids_json,): (String,) =
        sqlx::query_as("SELECT frame_ids FROM acquisition_session WHERE id = 'sess-1'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let ids: Vec<String> = serde_json::from_str(&frame_ids_json).unwrap();
    assert!(ids.contains(&frame_id), "session attribution must survive an unavailable root");
}

/// Regression (astro-plan-2uwa): a root whose registered path is a symlink
/// passes a `Path::is_dir()` probe (which follows links) but the gated walker
/// refuses to descend it, yielding an empty listing. That combination must
/// abort, not mark every frame missing.
#[cfg(unix)]
#[tokio::test]
async fn symlinked_root_aborts_reconcile_when_following_is_disabled() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let real_root = dir.path().join("real_root");
    std::fs::create_dir_all(&real_root).unwrap();
    std::fs::write(real_root.join("light_001.fits"), vec![0u8; 100]).unwrap();
    let linked_root = dir.path().join("linked_root");
    symlink(&real_root, &linked_root).unwrap();

    let db = test_db().await;
    insert_root(db.pool(), "root-1", linked_root.to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    let frame_id =
        upsert_frame_record(db.pool(), "root-1", "light_001.fits", 100, "t0", "classified")
            .await
            .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&frame_id]).await;

    let req = InventoryReconcileRunRequest {
        root_id: "root-1".to_owned(),
        reason: ReconcileReason::Scheduled,
    };
    let err = run_reconcile(db.pool(), &bus, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::RootUnavailable);

    let (state,): (String,) = sqlx::query_as("SELECT state FROM file_record WHERE id = ?")
        .bind(&frame_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "classified");
}

// ── T019/T025: relink confirms identity by sha256, not size/mtime ────────

#[tokio::test]
async fn relink_first_attempt_populates_hash_and_rehomes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("rejects")).unwrap();
    std::fs::write(dir.path().join("rejects").join("light_001.fits"), b"same-content").unwrap();

    let db = test_db().await;
    insert_root(db.pool(), "root-1", dir.path().to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    let frame_id =
        upsert_frame_record(db.pool(), "root-1", "lights/light_001.fits", 12, "t0", "missing")
            .await
            .unwrap();

    let req = InventoryFrameRelinkRequest {
        frame_id: frame_id.clone(),
        candidate_relative_path: "rejects/light_001.fits".to_owned(),
    };
    let resp = relink_frame(db.pool(), &bus, &req).await.unwrap();
    assert!(resp.relinked);
    assert_eq!(resp.matched_hash, sha256_hex(&dir.path().join("rejects/light_001.fits")).unwrap());

    let (relative_path, content_hash, state): (String, Option<String>, String) =
        sqlx::query_as("SELECT relative_path, content_hash, state FROM file_record WHERE id = ?")
            .bind(&frame_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(relative_path, "rejects/light_001.fits");
    assert_eq!(content_hash.as_deref(), Some(resp.matched_hash.as_str()));
    assert_eq!(state, "classified");
}

#[tokio::test]
async fn relink_second_attempt_same_size_different_content_is_hash_mismatch() {
    // Proves size is not the identity key (FR-012a/R3): both candidates
    // are exactly 4 bytes, but their content differs.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("candidate_a.fits"), b"AAAA").unwrap();
    std::fs::write(dir.path().join("candidate_b.fits"), b"BBBB").unwrap();

    let db = test_db().await;
    insert_root(db.pool(), "root-1", dir.path().to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    let frame_id =
        upsert_frame_record(db.pool(), "root-1", "lights/light_001.fits", 4, "t0", "missing")
            .await
            .unwrap();

    // First relink establishes the baseline hash from candidate_a.
    let first = InventoryFrameRelinkRequest {
        frame_id: frame_id.clone(),
        candidate_relative_path: "candidate_a.fits".to_owned(),
    };
    relink_frame(db.pool(), &bus, &first).await.unwrap();

    // A second relink attempt against a same-size, different-content
    // file must fail — size alone would have let this through.
    let second = InventoryFrameRelinkRequest {
        frame_id: frame_id.clone(),
        candidate_relative_path: "candidate_b.fits".to_owned(),
    };
    let err = relink_frame(db.pool(), &bus, &second).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HashMismatch);

    // Not re-homed on mismatch — relative_path is unchanged from the
    // first (successful) relink.
    let (relative_path,): (String,) =
        sqlx::query_as("SELECT relative_path FROM file_record WHERE id = ?")
            .bind(&frame_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(relative_path, "candidate_a.fits");
}

#[tokio::test]
async fn relink_missing_candidate_path_returns_file_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let db = test_db().await;
    insert_root(db.pool(), "root-1", dir.path().to_str().unwrap()).await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    let frame_id =
        upsert_frame_record(db.pool(), "root-1", "lights/light_001.fits", 4, "t0", "missing")
            .await
            .unwrap();

    let req = InventoryFrameRelinkRequest {
        frame_id,
        candidate_relative_path: "does/not/exist.fits".to_owned(),
    };
    let err = relink_frame(db.pool(), &bus, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::FileNotFound);
}

#[tokio::test]
async fn relink_unknown_frame_id_returns_frame_not_found() {
    let db = test_db().await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());
    let req = InventoryFrameRelinkRequest {
        frame_id: "no-such-frame".to_owned(),
        candidate_relative_path: "x.fits".to_owned(),
    };
    let err = relink_frame(db.pool(), &bus, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::FrameNotFound);
}
