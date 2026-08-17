// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Constitution gate for the per-root live frame watcher (spec 048 T023, R6):
//! a root whose registered path is a symlink is NOT traversed while that root's
//! `detection.follow_symlinks` is false (the default).
//!
//! The negative half alone could pass vacuously (a watcher that never works at
//! all also never fires), so the same test then enables `follow_symlinks` for
//! the root, re-attaches, and asserts the identical event DOES arrive. Only the
//! per-root gate differs between the two halves.

#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::time::Duration;

use audit::bus::EventBus;
use audit::event_bus::TOPIC_FRAME_RECOVERED;
use contracts_core::inventory_frame::{DetectionConfigUpdate, RootConfigSetRequest};
use desktop_shell::frame_watcher::{
    attach_root_watcher, detach_root_watcher, new_frame_watcher_registry,
};
use persistence_core::Database;

/// Wait up to `budget` for a `frame.recovered` event for `root_id`.
async fn recovered_within(
    rx: &mut tokio::sync::broadcast::Receiver<audit::event_bus::EventEnvelope<serde_json::Value>>,
    root_id: &str,
    budget: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(env))
                if env.topic == TOPIC_FRAME_RECOVERED
                    && env.payload["rootId"].as_str() == Some(root_id) =>
            {
                return true;
            }
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => return false,
        }
    }
}

#[tokio::test]
async fn symlinked_root_is_not_traversed_until_follow_symlinks_is_enabled() {
    let db = Database::in_memory().await.expect("in-memory database");
    db.migrate().await.expect("run migrations");
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().canonicalize().expect("canonicalize tempdir");
    let real_dir = base.join("real_frames");
    std::fs::create_dir_all(&real_dir).expect("create real dir");
    // The registered root path IS the symlink — the case the constitution's
    // "MUST NOT follow symlinks or junctions" constraint covers.
    let linked_root = base.join("linked_root");
    symlink(&real_dir, &linked_root).expect("create symlink");

    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
         VALUES ('root-link', 'Linked Root', ?, 'local', 'active', datetime('now'))",
    )
    .bind(linked_root.to_string_lossy().as_ref())
    .execute(&pool)
    .await
    .expect("insert library_root");

    app_core::targets::frame_writer::upsert_frame_record(
        &pool,
        "root-link",
        "light_001.fits",
        4096,
        "2026-01-01T00:00:00Z",
        "missing",
    )
    .await
    .expect("insert file_record");

    let registry = new_frame_watcher_registry();

    // ── Gate closed (default follow_symlinks = false) ──────────────────────
    let mut rx = bus.subscribe();
    attach_root_watcher(&pool, &bus, &registry, "root-link")
        .await
        .expect("attach with gate closed");
    std::fs::write(real_dir.join("light_001.fits"), vec![0u8; 4096]).expect("write frame file");

    assert!(
        !recovered_within(&mut rx, "root-link", Duration::from_secs(6)).await,
        "a symlinked root must not be traversed while detection.follow_symlinks is false"
    );
    let (state,): (String,) =
        sqlx::query_as("SELECT state FROM file_record WHERE relative_path = 'light_001.fits'")
            .fetch_one(&pool)
            .await
            .expect("read state");
    assert_eq!(state, "missing", "no record may be written for an ungated symlinked root");

    // ── Gate opened — same root, same file, same event path ────────────────
    detach_root_watcher(&registry, "root-link").await;
    app_core::settings::root_config::set_root_config(
        &pool,
        &RootConfigSetRequest {
            root_id: "root-link".to_owned(),
            reconcile_mode: None,
            detection: Some(DetectionConfigUpdate {
                follow_symlinks: Some(true),
                ..Default::default()
            }),
        },
    )
    .await
    .expect("enable follow_symlinks");

    let mut rx = bus.subscribe();
    attach_root_watcher(&pool, &bus, &registry, "root-link").await.expect("attach with gate open");
    // Re-touch the file so the live watcher sees an event for it.
    std::fs::write(real_dir.join("light_001.fits"), vec![1u8; 4096]).expect("rewrite frame file");

    assert!(
        recovered_within(&mut rx, "root-link", Duration::from_secs(15)).await,
        "with follow_symlinks enabled the same live event must schedule a reconcile"
    );
}
