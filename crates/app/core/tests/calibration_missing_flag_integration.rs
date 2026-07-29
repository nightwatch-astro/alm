#![allow(clippy::doc_markdown)]
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Layer-1 integration tests for calibration match "missing" awareness —
//! spec 048 US5 (FR-024/025, T037).
//!
//! Establishes a real calibration match (`calibration_assignment` row), then
//! exercises both independent trigger paths against the real reconcile
//! use-cases (no mocks):
//!   - PATH A: the generated master artifact (spec-012 `processing_artifacts`)
//!     goes missing/recovers → `app_core::lifecycle::artifact::mark_missing_batch`/
//!     `mark_recovered`.
//!   - PATH B: a raw source sub-frame of the master's own session goes
//!     missing/recovers → `app_core::frame_inventory::run_reconcile`.
//!
//! Asserts: the match is flagged with the PATH-specific wording, the
//! underlying `calibration_assignment` row is NEVER removed (FR-024), the
//! flag clears on recovery (FR-025), matching `calibration_match.source_*`
//! audit events are recorded, and the app performs zero filesystem
//! mutations throughout (INV-2) — only the test itself edits the tempdir to
//! simulate an external change.

mod support;

use app_core::calibration::masters_get;
use app_core::lifecycle::artifact::{mark_missing_batch, mark_recovered, GoneArtifact};
use contracts_core::calibration::CalibrationMatchMissingFlag;
use contracts_core::inventory_frame::{InventoryReconcileRunRequest, ReconcileReason};
use persistence_calibration::repositories::calibration_assignment::{upsert, UpsertParams};
use persistence_plans::repositories::artifacts::{insert_artifact_if_absent, InsertArtifact};

// ── Seed helpers ──────────────────────────────────────────────────────────────

async fn insert_cal_session(pool: &sqlx::SqlitePool, id: &str, kind: &str, frame_ids: &str) {
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, frame_ids, kind, created_at) \
         VALUES (?, ?, ?, ?, '2026-07-01T00:00:00Z')",
    )
    .bind(id)
    .bind(format!("calkey-{id}"))
    .bind(frame_ids)
    .bind(kind)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert calibration_session failed: {e}"));
}

async fn insert_calibration_master(
    pool: &sqlx::SqlitePool,
    id: &str,
    source_session_id: &str,
    artifact_id: &str,
) {
    sqlx::query(
        "INSERT INTO calibration_master \
         (id, source_session_id, artifact_id, kind, reuse_match_key, created_at) \
         VALUES (?, ?, ?, 'master_dark', ?, '2026-07-01T00:00:00Z')",
    )
    .bind(id)
    .bind(source_session_id)
    .bind(artifact_id)
    .bind(format!("reuse-{id}"))
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert calibration_master failed: {e}"));
}

async fn insert_root(pool: &sqlx::SqlitePool, id: &str, path: &str) {
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
         VALUES (?, ?, ?, 'local', 'active', datetime('now'))",
    )
    .bind(id)
    .bind(id)
    .bind(path)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert library_root failed: {e}"));
}

async fn insert_frame_record(
    pool: &sqlx::SqlitePool,
    id: &str,
    root_id: &str,
    relative_path: &str,
    size_bytes: i64,
) {
    sqlx::query(
        "INSERT INTO file_record \
         (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
         VALUES (?, ?, ?, ?, 't0', 'classified', 't0', 't0')",
    )
    .bind(id)
    .bind(root_id)
    .bind(relative_path)
    .bind(size_bytes)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("insert file_record failed: {e}"));
}

async fn seed_assignment(pool: &sqlx::SqlitePool, id: &str, session_id: &str, master_id: &str) {
    upsert(
        pool,
        UpsertParams {
            id,
            session_id,
            calibration_type: "dark",
            master_id,
            confidence: 0.9,
            was_override: false,
            mismatched_dimensions: &[],
            assigned_at: None,
        },
    )
    .await
    .unwrap_or_else(|e| panic!("seed calibration_assignment failed: {e}"));
}

async fn assignment_row_exists(pool: &sqlx::SqlitePool, id: &str) -> bool {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM calibration_assignment WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    count == 1
}

async fn event_count(pool: &sqlx::SqlitePool, topic: &str) -> i64 {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE topic = ?")
        .bind(topic)
        .fetch_one(pool)
        .await
        .unwrap();
    count
}

// ── PATH A: generated master artifact missing/recovered ────────────────────────

/// A calibration match whose master has a tracked generated master artifact
/// (spec-012 `processing_artifacts`, via the `calibration_master` join table)
/// is flagged `MasterMissing` when that artifact goes missing, remains
/// present (never removed), and the flag clears with a
/// `calibration_match.source_recovered` event once the artifact recovers.
#[tokio::test]
async fn master_artifact_missing_flags_match_and_clears_on_recovery() {
    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();

    let master_id = "master-a".to_owned();
    let artifact_id = "art-a".to_owned();
    let assignment_id = "assign-a".to_owned();

    insert_cal_session(pool, &master_id, "dark", "[]").await;
    insert_artifact_if_absent(
        pool,
        InsertArtifact {
            id: &artifact_id,
            project_id: "proj-a",
            tool_launch_id: None,
            path: "output/MasterDark.xisf",
            kind: "master",
            tool: "pixinsight",
            detected_at: "2026-07-01T00:00:00Z",
            state: "present",
            classification_confidence: 0.95,
            classification_source: "rule",
            size_bytes: 4096,
            file_mtime: "2026-07-01T00:00:00Z",
            content_hash: None,
        },
    )
    .await
    .unwrap();
    insert_calibration_master(pool, "cm-a", &master_id, &artifact_id).await;
    seed_assignment(pool, &assignment_id, "light-a", &master_id).await;

    // Before any transition: no flag.
    let detail = masters_get(pool, &master_id).await.unwrap();
    assert_eq!(detail.missing_flag, None);

    // (A) mark the master artifact missing.
    mark_missing_batch(
        pool,
        &bus,
        "proj-a",
        &[GoneArtifact { id: artifact_id.clone(), path: "output/MasterDark.xisf".to_owned() }],
    )
    .await
    .unwrap();

    let detail = masters_get(pool, &master_id).await.unwrap();
    assert_eq!(
        detail.missing_flag,
        Some(CalibrationMatchMissingFlag::MasterMissing),
        "match must be flagged master-missing, not silently broken"
    );
    assert!(
        assignment_row_exists(pool, &assignment_id).await,
        "FR-024: the match must never be auto-invalidated or removed"
    );
    assert_eq!(
        event_count(pool, "calibration_match.source_missing").await,
        1,
        "expected one calibration_match.source_missing audit event"
    );

    // Recovery clears the flag.
    mark_recovered(pool, &bus, "proj-a", &artifact_id, "output/MasterDark.xisf", 4096)
        .await
        .unwrap();

    let detail = masters_get(pool, &master_id).await.unwrap();
    assert_eq!(detail.missing_flag, None, "FR-025: flag must clear once the artifact recovers");
    assert!(assignment_row_exists(pool, &assignment_id).await);
    assert_eq!(event_count(pool, "calibration_match.source_recovered").await, 1);
}

// ── PATH B: raw source sub-frame missing/recovered ──────────────────────────────

/// A calibration match whose master's own raw sub-frames include a frame
/// that goes missing on disk (detected via the real `run_reconcile`
/// reconciliation pass) is flagged `SourceSubsMissing` — distinct wording
/// from PATH A — remains present, and clears on recovery. Zero filesystem
/// mutations are performed by the app; only the test itself deletes/restores
/// the file to simulate an external change (constitution INV-2).
#[tokio::test]
async fn source_sub_frame_missing_flags_match_and_clears_on_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("dark_001.fits");
    std::fs::write(&file_path, vec![0u8; 2048]).unwrap();

    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();

    let root_id = "root-b".to_owned();
    let master_id = "master-b".to_owned();
    let frame_id = "frame-b".to_owned();
    let assignment_id = "assign-b".to_owned();

    insert_root(pool, &root_id, dir.path().to_str().unwrap()).await;
    insert_frame_record(pool, &frame_id, &root_id, "dark_001.fits", 2048).await;
    insert_cal_session(pool, &master_id, "dark", &format!("[\"{frame_id}\"]")).await;
    seed_assignment(pool, &assignment_id, "light-b", &master_id).await;

    // Before any transition: no flag.
    let detail = masters_get(pool, &master_id).await.unwrap();
    assert_eq!(detail.missing_flag, None);

    // (B) simulate an external delete, then trigger the real reconcile pass.
    std::fs::remove_file(&file_path).unwrap();
    let req = InventoryReconcileRunRequest {
        root_id: root_id.clone(),
        reason: ReconcileReason::OnDemand,
    };
    let resp = app_core::frame_inventory::run_reconcile(pool, &bus, &req).await.unwrap();
    assert_eq!(resp.newly_missing, 1);

    let detail = masters_get(pool, &master_id).await.unwrap();
    assert_eq!(
        detail.missing_flag,
        Some(CalibrationMatchMissingFlag::SourceSubsMissing),
        "match must be flagged source-subs-missing (distinct from master-missing wording)"
    );
    assert!(
        assignment_row_exists(pool, &assignment_id).await,
        "FR-024: the match must never be auto-invalidated or removed"
    );
    let (frame_state,): (String,) = sqlx::query_as("SELECT state FROM file_record WHERE id = ?")
        .bind(&frame_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(frame_state, "missing", "INV-4: the frame record is retained, never hard-deleted");
    assert_eq!(event_count(pool, "calibration_match.source_missing").await, 1);

    // Recovery: restore the file and reconcile again.
    std::fs::write(&file_path, vec![0u8; 2048]).unwrap();
    let resp = app_core::frame_inventory::run_reconcile(pool, &bus, &req).await.unwrap();
    assert_eq!(resp.recovered, 1);

    let detail = masters_get(pool, &master_id).await.unwrap();
    assert_eq!(detail.missing_flag, None, "FR-025: flag must clear once the frame recovers");
    assert_eq!(event_count(pool, "calibration_match.source_recovered").await, 1);

    // Zero filesystem mutations by the app: only the one seeded file exists,
    // exactly where the test (not the app) put it back.
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "app must never create/delete/move files during reconcile");
}
