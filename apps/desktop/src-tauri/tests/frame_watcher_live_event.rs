// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-backend integration test for the per-root live frame watcher (spec 048
//! T023/T024/T026): a file appearing in an attached raw root strictly AFTER
//! attach schedules a reconcile pass, which recovers a record previously
//! flagged `missing` and emits `frame.recovered`.
//!
//! This is the property research R2 requires and that no unit test can prove on
//! its own: the OS watcher, the debounce, and `run_reconcile` wired together
//! against a real database, a real `EventBus`, and a real filesystem. It pins
//! the "live events schedule, never write" rule — the recovery is observed
//! through the reconcile pass's own event, not a write from the watcher task.

use audit::bus::EventBus;
use audit::event_bus::TOPIC_FRAME_RECOVERED;
use desktop_shell::frame_watcher::{attach_root_watcher, new_frame_watcher_registry};
use persistence_core::Database;

/// Insert a `library_root` row plus a `file_record` already flagged `missing`,
/// so a reconcile pass that finds the file on disk must report it recovered.
async fn seed_missing_frame(pool: &sqlx::SqlitePool, root_path: &str, relative_path: &str) {
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
         VALUES ('root-live', 'Live Root', ?, 'local', 'active', datetime('now'))",
    )
    .bind(root_path)
    .execute(pool)
    .await
    .expect("insert library_root");

    app_core::targets::frame_writer::upsert_frame_record(
        pool,
        "root-live",
        relative_path,
        4096,
        "2026-01-01T00:00:00Z",
        "missing",
    )
    .await
    .expect("insert file_record");
}

#[tokio::test]
async fn live_file_creation_schedules_a_reconcile_that_recovers_the_frame() {
    let db = Database::in_memory().await.expect("in-memory database");
    db.migrate().await.expect("run migrations");
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    // Canonicalize so the watcher's paths match the recorded root on macOS
    // (`/private/var/...` vs `/var/...`).
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonicalize tempdir");
    seed_missing_frame(&pool, &root.to_string_lossy(), "light_001.fits").await;

    let mut rx = bus.subscribe();

    let registry = new_frame_watcher_registry();
    attach_root_watcher(&pool, &bus, &registry, "root-live").await.expect("attach_root_watcher");

    // Written strictly after attach: this exercises the live notify path, not
    // an attach-time pass (`detection.on_open` defaults to false anyway).
    std::fs::write(root.join("light_001.fits"), vec![0u8; 4096]).expect("write frame file");

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut recovered = false;
    while !recovered {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(env)) if env.topic == TOPIC_FRAME_RECOVERED => {
                assert_eq!(env.payload["rootId"].as_str(), Some("root-live"));
                recovered = true;
            }
            Ok(Ok(_)) => {} // other topics on the shared bus — keep draining
            Ok(Err(_)) | Err(_) => break,
        }
    }

    assert!(
        recovered,
        "a live filesystem event must schedule a reconcile pass that emits frame.recovered"
    );

    // The pass, not the watcher, is what wrote the state — assert the record
    // actually left `missing`.
    let (state,): (String,) =
        sqlx::query_as("SELECT state FROM file_record WHERE relative_path = 'light_001.fits'")
            .fetch_one(&pool)
            .await
            .expect("read back file_record state");
    assert_ne!(state, "missing");

    // Detach must release the watch so no OS watcher is held on an idle root.
    desktop_shell::frame_watcher::detach_root_watcher(&registry, "root-live").await;
    assert!(registry.lock().await.entries.is_empty());
}
