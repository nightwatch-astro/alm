// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-backend integration test: an artifact under a directory the
//! reconciliation walk cannot read stays `present` and the skip is recorded.
//!
//! Companion to `artifact_watcher_missing_reconciliation.rs`, which covers the
//! `Gone` branch. Unreadable is not absent: the walk skips the subtree so the
//! rest of the project still reconciles, and the rows underneath it must not be
//! rewritten as `missing` on the strength of a directory nobody could open.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use audit::bus::EventBus;
use audit::event_bus::{
    EventEnvelope, TOPIC_ARTIFACT_DETECTED, TOPIC_ARTIFACT_MISSING, TOPIC_ARTIFACT_SCAN_INCOMPLETE,
};
use persistence_core::Database;

use desktop_shell::watcher::{
    attach_project_watcher, detach_project_watcher, new_artifact_watcher_registry,
};

async fn insert_projects_row(pool: &sqlx::SqlitePool, path: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO projects \
         (id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at) \
         VALUES (?, 'Watcher Unreadable-Subdir Project', 'PixInsight', 'setup_incomplete', ?, NULL, 0, \
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&id)
    .bind(path)
    .execute(pool)
    .await
    .expect("insert projects row");
    id
}

async fn wait_for_topic(
    rx: &mut tokio::sync::broadcast::Receiver<EventEnvelope<serde_json::Value>>,
    topic: &str,
    deadline: tokio::time::Instant,
) -> Option<EventEnvelope<serde_json::Value>> {
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(env)) if env.topic == topic => return Some(env),
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

/// `true` when this process can still read `dir` at mode 000 — root and any
/// `CAP_DAC_READ_SEARCH` holder bypass mode bits, which would make the whole
/// test vacuous.
fn mode_bits_are_enforced(dir: &Path) -> bool {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o000))
        .expect("chmod 000 the subdirectory");
    std::fs::read_dir(dir).is_err()
}

#[tokio::test]
async fn artifact_under_an_unreadable_subdirectory_is_not_marked_missing() {
    let db = Database::in_memory().await.expect("in-memory database");
    db.migrate().await.expect("run migrations");
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let dir = tempfile::tempdir().expect("tempdir");
    let project_root: PathBuf = dir.path().canonicalize().expect("canonicalize tempdir");
    let project_id = insert_projects_row(&pool, &project_root.to_string_lossy()).await;

    let output_dir = project_root.join("output");
    std::fs::create_dir(&output_dir).expect("create output subdirectory");
    let file_path = output_dir.join("unreadable_subdir_M42_L.xisf");
    std::fs::write(&file_path, b"not-a-real-xisf-file").expect("write artifact file");

    let registry = new_artifact_watcher_registry();

    let mut rx_detect = bus.subscribe();
    attach_project_watcher(&pool, &bus, &registry, &project_id)
        .await
        .expect("first attach_project_watcher");
    let detect_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let detected_env = wait_for_topic(&mut rx_detect, TOPIC_ARTIFACT_DETECTED, detect_deadline)
        .await
        .expect("artifact.detected must fire from the on-attach reconciliation pass");
    let artifact_id =
        detected_env.payload["artifactId"].as_str().expect("artifactId is a string").to_owned();

    detach_project_watcher(&registry, &project_id).await;

    if !mode_bits_are_enforced(&output_dir) {
        std::fs::set_permissions(&output_dir, std::fs::Permissions::from_mode(0o755))
            .expect("restore permissions");
        eprintln!("skipping: this environment can read a mode-000 directory (running as root?)");
        return;
    }

    let mut rx_scan = bus.subscribe();
    let attached = attach_project_watcher(&pool, &bus, &registry, &project_id).await;
    let scan_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let scan_env =
        wait_for_topic(&mut rx_scan, TOPIC_ARTIFACT_SCAN_INCOMPLETE, scan_deadline).await;
    let missing_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    let missing_env = wait_for_topic(&mut rx_scan, TOPIC_ARTIFACT_MISSING, missing_deadline).await;

    let state =
        sqlx::query_as::<_, (String,)>("SELECT state FROM processing_artifacts WHERE id = ?")
            .bind(&artifact_id)
            .fetch_one(&pool)
            .await
            .map(|row| row.0);

    // Restore before asserting so the tempdir can always be removed.
    std::fs::set_permissions(&output_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore permissions");

    attached.expect("second attach_project_watcher must survive the unreadable subdirectory");
    let scan_env = scan_env.expect("artifact.scan_incomplete must record the skipped directory");
    assert_eq!(scan_env.payload["projectId"].as_str(), Some(project_id.as_str()));
    assert_eq!(
        scan_env.payload["unreadablePaths"].as_array().map(Vec::len),
        Some(1),
        "the skipped directory must be named in the audit payload"
    );
    assert!(
        missing_env.is_none(),
        "no artifact.missing may fire for a subtree the walk could not read"
    );
    assert_eq!(
        state.expect("query artifact state"),
        "present",
        "an artifact under an unreadable directory is unknown state, not absent state"
    );
}
