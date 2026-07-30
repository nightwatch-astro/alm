// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-backend coverage of `start_log_forwarder`'s two startup contracts: the
//! pre-broadcast catch-up, and the refusal to replay the whole `events` table
//! when the cursor cannot be initialised.
//!
//! The mock Tauri app has no webview, but a Rust-side `Listener::listen` on the
//! same handle still receives `log:entry`, so the catch-up asserts the emitted
//! entry ids. The cursor-failure test asserts on the task handle instead: a
//! forwarder that fell back to cursor `0` would stay parked in the receive loop,
//! so "the task finished" is what separates the fix from the bug.

use audit::bus::EventBus;
use contracts_core::log::LogLevel;
use desktop_shell::commands::log::start_log_forwarder;
use persistence_core::Database;

async fn setup() -> (tauri::App<tauri::test::MockRuntime>, sqlx::SqlitePool, EventBus) {
    let db = Database::in_memory().await.expect("in-memory database");
    db.migrate().await.expect("run migrations");
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build mock app");

    (app, pool, bus)
}

/// A `rewound_cursor` failure must stop the forwarder, never seed cursor `0`.
/// Seeding `0` would make the next `list_since(&pool, 0)` load and emit every
/// row in `events`, defeating the bounded `LOG_BUFFER_SIZE` catch-up.
///
/// Dropping the table is a genuine query failure against a live pool: no
/// injected error type, and the same `DbError` a corrupt or migrating DB yields.
#[tokio::test]
async fn cursor_init_failure_stops_the_forwarder_instead_of_replaying_everything() {
    let (app, pool, bus) = setup().await;

    sqlx::query("DROP TABLE events").execute(&pool).await.expect("drop events table");

    let handle = start_log_forwarder(app.handle().clone(), &bus, LogLevel::Debug, pool.clone());

    let stopped = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    assert!(
        stopped.is_ok(),
        "forwarder must terminate when its cursor cannot be initialised; a live \
         task means it fell back to cursor 0 and is replaying the whole table"
    );
    stopped.expect("forwarder terminated").expect("forwarder task did not panic");
}

/// The catch-up must run before the receive loop: rows already inside the
/// rewound window reach the panel even when no further event is broadcast.
/// Proven by cursor advancement — the forwarder consumes the pre-existing rows
/// and then parks in `rx.recv()` rather than exiting.
#[tokio::test]
async fn forwarder_stays_live_after_catching_up_pre_existing_rows() {
    let (app, pool, bus) = setup().await;

    for i in 0..3 {
        persistence_lifecycle::repositories::events::insert_event(
            &pool,
            "lifecycle.transition.applied",
            "system",
            &format!("2026-01-01T00:00:0{i}Z"),
            "{}",
        )
        .await
        .expect("seed event");
    }

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&seen);
    tauri::Listener::listen(app.handle(), "log:entry", move |event| {
        let entry: contracts_core::log::LogEntry =
            serde_json::from_str(event.payload()).expect("log:entry payload is a LogEntry");
        sink.lock().expect("sink lock").push(entry.id);
    });

    let _handle = start_log_forwarder(app.handle().clone(), &bus, LogLevel::Debug, pool.clone());

    // No broadcast is ever published: only a catch-up that runs *before*
    // `rx.recv()` can deliver these rows.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let ids = seen.lock().expect("sink lock").clone();
        if ids.len() >= 3 {
            assert_eq!(ids, vec!["aud:1", "aud:2", "aud:3"], "every seeded row, in id order");
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "catch-up must emit pre-existing rows without a broadcast; saw {ids:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
