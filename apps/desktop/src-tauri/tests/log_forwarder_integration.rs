// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-backend coverage of `start_log_forwarder`'s startup contracts: the
//! pre-broadcast catch-up, the refusal to replay the whole `events` table when
//! the cursor cannot be initialised, and the bound on what a replay emits.
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
/// Startup failures go to `tracing`, not to a `log:entry`: the forwarder starts
/// before `create_main_window`, so no webview listener exists yet and Tauri drops
/// events that nobody is listening for. A listener attached here stands in for
/// the frontend's; it must stay empty, because an entry it *does* receive is one
/// the real app would have lost.
#[tokio::test]
async fn cursor_init_failure_stops_the_forwarder_instead_of_replaying_everything() {
    let (app, pool, bus) = setup().await;

    sqlx::query("DROP TABLE events").execute(&pool).await.expect("drop events table");

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&seen);
    tauri::Listener::listen(app.handle(), "log:entry", move |event| {
        sink.lock().expect("sink lock").push(event.payload().to_owned());
    });

    let handle = start_log_forwarder(app.handle().clone(), &bus, LogLevel::Debug, pool.clone());

    let stopped = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    assert!(
        stopped.is_ok(),
        "forwarder must terminate when its cursor cannot be initialised; a live \
         task means it fell back to cursor 0 and is replaying the whole table"
    );
    stopped.expect("forwarder terminated").expect("forwarder task did not panic");

    let emitted = seen.lock().expect("sink lock").clone();
    assert!(
        emitted.is_empty(),
        "a startup diagnostic cannot reach the panel — the window does not exist \
         yet — so emitting one only hides the failure; saw {emitted:?}"
    );
}

/// A replay that skips history emits the window's rows and nothing else.
///
/// The frontend ring buffer holds exactly `LOG_BUFFER_SIZE` entries and evicts
/// oldest-first (`apps/desktop/src/data/logStore.ts`), so any extra startup entry
/// costs one of the rows the replay exists to deliver. Asserting on the emitted
/// ids (not just their count) is what catches a re-added startup diagnostic: a
/// `dia:` entry would both appear here and push `aud:1` out of the panel.
#[tokio::test]
async fn skipped_history_replay_emits_only_the_window_rows() {
    let (app, pool, bus) = setup().await;

    // One row beyond the window, so `rewound_cursor` must skip history.
    let seeded = app_core::log_stream::LOG_BUFFER_SIZE + 1;
    for i in 0..seeded {
        persistence_lifecycle::repositories::events::insert_event(
            &pool,
            "lifecycle.transition.applied",
            "system",
            &format!("2026-01-01T00:00:00.{i:04}Z"),
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

    let window = app_core::log_stream::LOG_BUFFER_SIZE;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ids = seen.lock().expect("sink lock").clone();
        if ids.len() >= window {
            // The newest `window` rows, oldest-first: ids 2..=seeded. Row 1 is
            // outside the window and is reachable through `log.export`.
            let expected: Vec<String> = (2..=seeded).map(|i| format!("aud:{i}")).collect();
            assert_eq!(
                ids, expected,
                "a skipped-history replay must emit the window rows and no extra entry, \
                 which would evict one of them from the ring buffer"
            );
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "replay must emit the whole window; saw {} of {window}",
            ids.len()
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Production ordering: the listener attaches AFTER the forwarder starts.
///
/// `start_log_forwarder` is spawned in `run_app` well before
/// `create_main_window`, so the startup catch-up emits into a webview that does
/// not exist. Every other test here pre-attaches its listener, which is the one
/// condition production never satisfies. If the catch-up consumed its rows, they
/// would be past the cursor of every later drain and no broadcast could ever
/// deliver them — permanent loss, not delay.
///
/// The later broadcast stands in for the first event after the window opens.
#[tokio::test]
async fn rows_emitted_before_any_listener_are_not_consumed_by_the_catch_up() {
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

    let _handle = start_log_forwarder(app.handle().clone(), &bus, LogLevel::Debug, pool.clone());

    // Let the startup catch-up run to completion with nobody listening, exactly
    // as it does between `start_log_forwarder` and `create_main_window`.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&seen);
    tauri::Listener::listen(app.handle(), "log:entry", move |event| {
        let entry: contracts_core::log::LogEntry =
            serde_json::from_str(event.payload()).expect("log:entry payload is a LogEntry");
        sink.lock().expect("sink lock").push(entry.id);
    });

    bus.publish(
        "lifecycle.transition.applied",
        audit::event_bus::Source::System,
        serde_json::json!({}),
    )
    .await
    .expect("publish post-listener event");

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let ids = seen.lock().expect("sink lock").clone();
        if ids.len() >= 4 {
            assert_eq!(
                ids,
                vec!["aud:1", "aud:2", "aud:3", "aud:4"],
                "the pre-listener rows must still be reachable: a catch-up that advanced \
                 the cursor puts them past every later drain"
            );
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "rows committed before the listener existed must be redelivered on the next \
             broadcast; saw {ids:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
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
