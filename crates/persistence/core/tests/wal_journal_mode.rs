// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::doc_markdown)] // "SQLite" is a proper noun, not code -- matches src/lib.rs
//! Regression coverage for #830 (background poll queries routinely exceeding
//! the 1s slow-query threshold every 30-90s against ~10-row tables).
//!
//! Root cause: `Database::connect` opened the pool without an explicit
//! journal mode, so file-backed databases stayed on SQLite's default
//! rollback-journal (`DELETE`) mode. Under rollback-journal, a writer holding
//! an exclusive lock (taken for the life of a `BEGIN EXCLUSIVE` transaction,
//! or briefly at commit for any write) blocks *every* reader sharing the same
//! file, including unrelated poller queries on other pooled connections. WAL
//! mode gives readers a consistent snapshot independent of an in-flight
//! writer, removing that contention class.
//!
//! `Database::connect` pairs WAL with `synchronous = NORMAL`, not SQLite's
//! compiled-in `FULL` default; see the durability tier policy on
//! `Database::connect`. `database_connect_reports_synchronous_normal` asserts
//! the value SQLite reports back on a file-backed connection.
//!
//! These tests reproduce the underlying SQLite locking behaviour directly
//! (via `BEGIN EXCLUSIVE`) rather than racing a real poll loop, so they are
//! deterministic: rollback-journal readers are provably blocked for the
//! writer's held duration; WAL readers are provably not.

use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use persistence_core::Database;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::{Connection, SqliteConnection};

/// How long the writer holds its exclusive lock. Enough for the reader to
/// complete (WAL) or be blocked (rollback-journal) on any scheduler.
const HOLD: Duration = Duration::from_millis(600);
/// Generous slack for CI/WSL scheduling jitter around the blocking case.
const BLOCKED_FLOOR: Duration = Duration::from_millis(400);

/// Baseline: on rollback-journal (SQLite's un-configured default), a
/// concurrent writer holding `BEGIN EXCLUSIVE` blocks a reader on a separate
/// connection to the same file for the duration of the write.
#[tokio::test]
async fn rollback_journal_reader_blocks_behind_exclusive_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("rollback.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());

    let writer_opts = SqliteConnectOptions::from_str(&url)
        .expect("parse opts")
        .journal_mode(SqliteJournalMode::Delete);
    let mut writer = SqliteConnection::connect_with(&writer_opts).await.expect("writer connect");
    sqlx::query("CREATE TABLE scratch (id INTEGER)").execute(&mut writer).await.expect("create");

    // Reader connects *before* the writer's exclusive lock, mirroring a
    // poller that already holds a live connection from the shared pool
    // (rather than re-connecting per poll) when the writer starts.
    let reader_opts = SqliteConnectOptions::from_str(&url)
        .expect("parse opts")
        .journal_mode(SqliteJournalMode::Delete)
        .busy_timeout(Duration::from_secs(5));
    let mut reader = SqliteConnection::connect_with(&reader_opts).await.expect("reader connect");

    sqlx::query("BEGIN EXCLUSIVE").execute(&mut writer).await.expect("begin exclusive");

    let writer_task = tokio::spawn(async move {
        tokio::time::sleep(HOLD).await;
        sqlx::query("COMMIT").execute(&mut writer).await.expect("commit");
    });

    // Give the writer a moment to actually acquire the exclusive lock before
    // the reader queries, matching the poller-arrives-mid-write scenario.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let start = Instant::now();
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scratch")
        .fetch_one(&mut reader)
        .await
        .expect("blocked read eventually succeeds");
    let elapsed = start.elapsed();

    writer_task.await.expect("writer task");

    assert_eq!(count, 0);
    assert!(
        elapsed >= BLOCKED_FLOOR,
        "expected rollback-journal reader to block behind the exclusive writer for \
         roughly {HOLD:?}, actually returned after {elapsed:?}"
    );
}

/// Fix: `Database::connect` requests WAL, so the same "reader concurrent
/// with an exclusive writer" scenario does not block.
///
/// Structural (ordering) assertion: the reader completes while the writer
/// is still inside its sleep hold — i.e. reader_finished_at <
/// writer_released_at — proving the reader was not serialised behind the
/// exclusive lock. This replaces the previous wall-time upper-bound check
/// (`elapsed < UNBLOCKED_CEILING`) which was fragile on loaded CI runners
/// where Tokio scheduler latency alone could exceed the 300 ms ceiling.
#[tokio::test]
async fn database_connect_wal_reader_does_not_block_behind_exclusive_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("wal.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());

    let db = Database::connect(&url).await.expect("Database::connect");
    let pool = db.pool();

    sqlx::query("CREATE TABLE scratch (id INTEGER)")
        .execute(pool)
        .await
        .expect("create scratch table");

    // Shared slot: writer stores its release instant just before committing.
    let writer_release: Arc<std::sync::Mutex<Option<std::time::Instant>>> =
        Arc::new(std::sync::Mutex::new(None));
    let writer_release_clone = Arc::clone(&writer_release);

    let mut writer_conn = pool.acquire().await.expect("acquire writer connection");
    sqlx::query("BEGIN EXCLUSIVE").execute(&mut *writer_conn).await.expect("begin exclusive");

    let writer_task = tokio::spawn(async move {
        tokio::time::sleep(HOLD).await;
        // Record the instant immediately before releasing the lock — any reader
        // that finished before this moment was not waiting on the writer.
        *writer_release_clone.lock().unwrap() = Some(std::time::Instant::now());
        sqlx::query("COMMIT").execute(&mut *writer_conn).await.expect("commit");
    });

    // Let the writer acquire its exclusive lock before the reader starts.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let reader_finished_at = {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scratch")
            .fetch_one(pool)
            .await
            .expect("unblocked WAL read");
        assert_eq!(count, 0);
        std::time::Instant::now()
    };

    writer_task.await.expect("writer task");
    let writer_released_at =
        writer_release.lock().unwrap().expect("writer task must have set the release instant");

    assert!(
        reader_finished_at < writer_released_at,
        "WAL reader must complete while the writer still holds the exclusive lock \
         (regression: reader blocked like rollback-journal mode). \
         reader_finished_at={reader_finished_at:?}, writer_released_at={writer_released_at:?}"
    );
}

/// `Database::connect` actually reports `journal_mode = wal` back from
/// SQLite, not just requests it (WAL can silently fall back in some
/// environments, e.g. network filesystems).
#[tokio::test]
async fn database_connect_reports_wal_journal_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("journal-mode-check.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());

    let db = Database::connect(&url).await.expect("Database::connect");
    let (mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
        .fetch_one(db.pool())
        .await
        .expect("read journal_mode");

    assert_eq!(mode.to_lowercase(), "wal");
}

/// `Database::connect` reports `synchronous = NORMAL` back from SQLite on a
/// file-backed database, where the setting has durability consequences (an
/// in-memory database has none).
#[tokio::test]
async fn database_connect_reports_synchronous_normal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("synchronous-check.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());

    let db = Database::connect(&url).await.expect("Database::connect");
    // 0=OFF, 1=NORMAL, 2=FULL, 3=EXTRA.
    let (level,): (i64,) =
        sqlx::query_as("PRAGMA synchronous").fetch_one(db.pool()).await.expect("read synchronous");

    assert_eq!(level, 1, "file-backed pool must run synchronous = NORMAL (1)");
}
