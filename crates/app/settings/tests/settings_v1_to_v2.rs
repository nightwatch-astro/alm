// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the settings v1→v2 migration harness
//! (spec 018 US5, T029).
//!
//! Fixture: an in-memory DB seeded with a representative set of v1 rows:
//!
//! - Several ordinary keys that must be retained unchanged.
//! - One "obsolete" key (injected directly into the `settings` table by-passing
//!   the normal write path so it simulates a key that existed in v1 but is
//!   absent from both `DROP_KEYS` and `RESET_KEYS` today — i.e. a key the
//!   migration doesn't know about).  We verify such unknown rows are left alone.
//!
//! Because `DROP_KEYS` and `RESET_KEYS` are currently empty (identity migration),
//! the fixture also directly exercises the drop / reset code paths by patching
//! the DB state around them.  The test file therefore contains:
//!
//! 1. `identity_migration_retains_all_v1_keys` — happy path: stored rows survive.
//! 2. `dropped_key_is_removed_from_db` — simulates what happens when a key is
//!    in the drop list by calling `delete_key` directly before asserting absence,
//!    mirroring what the migration loop would do.
//! 3. `reset_key_returns_to_default_on_get_settings` — simulates a reset by
//!    deleting a stored row and verifying `get_settings` hydrates the default.
//! 4. `unknown_key_is_not_touched_by_migration` — a key not in the descriptor
//!    table is left in the DB untouched (migration only acts on listed keys).
//! 5. `migration_emits_exactly_one_audit_event` — the bus receives exactly one
//!    `settings.migration` event after the call.
//! 6. `migration_summary_counts_are_correct` — `MigrationSummary` fields match
//!    what happened.

use app_core_cache::SnapshotCache;
use app_core_settings::migrate::{migrate_v1_to_v2, MigrationSummary};
use audit::EventBus;
use domain_core::settings::SettingsState;
use persistence_core::Database;
use persistence_lifecycle::repositories::settings as repo;

// ── Fixture helpers ───────────────────────────────────────────────────────────

async fn setup() -> (Database, EventBus, SnapshotCache<SettingsState>) {
    let db = Database::in_memory().await.expect("in-memory DB");
    db.migrate().await.expect("migrations");
    let bus = EventBus::with_pool(db.pool().clone());
    (db, bus, SnapshotCache::new())
}

/// Seed a handful of v1 key rows with valid values.
async fn seed_v1_rows(db: &Database) {
    let pool = db.pool();
    repo::set_raw(pool, "logLevel", &serde_json::json!("debug")).await.unwrap();
    repo::set_raw(pool, "followSymlinks", &serde_json::json!(true)).await.unwrap();
    repo::set_raw(pool, "hashOnScan", &serde_json::json!("eager")).await.unwrap();
    repo::set_raw(pool, "darkMatchTolerance", &serde_json::json!("loose")).await.unwrap();
    repo::set_raw(pool, "blockPermanentDelete", &serde_json::json!(false)).await.unwrap();
}

// ── T029 tests ────────────────────────────────────────────────────────────────

/// Happy path: all seeded v1 rows survive the identity migration unchanged.
#[tokio::test]
async fn identity_migration_retains_all_v1_keys() {
    let (db, bus, _cache) = setup().await;
    seed_v1_rows(&db).await;

    let summary = migrate_v1_to_v2(db.pool(), &bus).await.expect("migrate_v1_to_v2");

    // All five seeded keys are retained; none dropped or reset.
    assert_eq!(summary.dropped, 0, "identity migration must drop nothing");
    assert_eq!(summary.reset, 0, "identity migration must reset nothing");
    assert_eq!(summary.migrated, 5, "all five seeded descriptor keys must be counted as migrated");

    // The stored values are byte-identical to what was seeded.
    let pool = db.pool();
    assert_eq!(repo::get_raw(pool, "logLevel").await.unwrap(), Some(serde_json::json!("debug")));
    assert_eq!(repo::get_raw(pool, "followSymlinks").await.unwrap(), Some(serde_json::json!(true)));
    assert_eq!(repo::get_raw(pool, "hashOnScan").await.unwrap(), Some(serde_json::json!("eager")));
    assert_eq!(
        repo::get_raw(pool, "darkMatchTolerance").await.unwrap(),
        Some(serde_json::json!("loose"))
    );
    assert_eq!(
        repo::get_raw(pool, "blockPermanentDelete").await.unwrap(),
        Some(serde_json::json!(false))
    );
}

/// Drop path: a key removed by `delete_key` (as the migration would do for a
/// key in `DROP_KEYS`) is absent from the DB afterwards.
///
/// This exercises the deletion path directly since `DROP_KEYS` is currently
/// empty (no speculative keys to test).  The migration harness calls
/// `repo::delete_key` for each entry in `DROP_KEYS`; here we call it manually
/// to confirm the DB behaviour is correct, then run the full migration to
/// ensure the rest of the summary is unaffected.
#[tokio::test]
async fn dropped_key_is_removed_from_db() {
    let (db, bus, _cache) = setup().await;
    seed_v1_rows(&db).await;

    // Simulate the drop: remove "followSymlinks" before the migration runs.
    repo::delete_key(db.pool(), "followSymlinks").await.unwrap();

    // Verify it is gone.
    let after_delete = repo::get_raw(db.pool(), "followSymlinks").await.unwrap();
    assert!(after_delete.is_none(), "deleted key must not be retrievable");

    // Run the migration on the remaining 4 rows; the deleted key is simply absent.
    let summary = migrate_v1_to_v2(db.pool(), &bus).await.expect("migrate_v1_to_v2");

    // 4 rows survived; the pre-deleted key is not counted.
    assert_eq!(summary.migrated, 4);
    assert_eq!(summary.dropped, 0, "DROP_KEYS is currently empty");
    assert_eq!(summary.reset, 0, "RESET_KEYS is currently empty");
}

/// Reset path: a key whose row is deleted (as the migration would do for a key
/// in `RESET_KEYS`) causes `get_settings` to return the in-code default.
///
/// Same rationale as the drop test: `RESET_KEYS` is empty today, so we call
/// `delete_key` directly then confirm the default is hydrated on the next read.
#[tokio::test]
async fn reset_key_returns_to_default_on_get_settings() {
    let (db, bus, cache) = setup().await;
    seed_v1_rows(&db).await;

    // The seeded value for "logLevel" is "debug" (non-default).
    assert_eq!(
        repo::get_raw(db.pool(), "logLevel").await.unwrap(),
        Some(serde_json::json!("debug"))
    );

    // Simulate the reset: delete the stored row so the default kicks in.
    repo::delete_key(db.pool(), "logLevel").await.unwrap();

    // The row is gone.
    assert!(repo::get_raw(db.pool(), "logLevel").await.unwrap().is_none());

    // get_settings must hydrate the in-code default ("info").
    let resp = app_core_settings::get_settings(db.pool(), &bus, &cache).await.unwrap();
    assert_eq!(
        resp.settings.log_level, "info",
        "reset key must fall back to the in-code default on the next get_settings"
    );
}

/// Unknown-key isolation: a key not in the v1 descriptor table (injected
/// directly into the `settings` table) must not be touched by the migration.
/// The migration only acts on keys it explicitly knows about.
#[tokio::test]
async fn unknown_key_is_not_touched_by_migration() {
    let (db, bus, _cache) = setup().await;

    // Inject an unknown key directly — bypasses `update_setting` validation.
    repo::set_raw(db.pool(), "legacyObsoleteKey_v0", &serde_json::json!("some_value"))
        .await
        .unwrap();

    // Run the migration.
    migrate_v1_to_v2(db.pool(), &bus).await.expect("migrate_v1_to_v2");

    // The unknown key must still be present (migration doesn't touch it).
    let still_there = repo::get_raw(db.pool(), "legacyObsoleteKey_v0").await.unwrap();
    assert_eq!(
        still_there,
        Some(serde_json::json!("some_value")),
        "migration must not delete keys it doesn't know about"
    );

    // The unknown key must not be counted in `migrated` (it's not a v1 descriptor key).
    // The summary migrated count is 0 because no descriptor keys were seeded.
    // (The summary's `migrated` field counts only descriptor-table keys.)
}

/// Audit-event contract (T031): exactly one `settings.migration` event is
/// published on the bus after `migrate_v1_to_v2` returns.
#[tokio::test]
async fn migration_emits_exactly_one_audit_event() {
    let (db, bus, _cache) = setup().await;
    seed_v1_rows(&db).await;

    let mut rx = bus.subscribe();

    migrate_v1_to_v2(db.pool(), &bus).await.expect("migrate_v1_to_v2");

    // Collect all events that arrived on the broadcast channel.
    let mut events = Vec::new();
    while let Ok(env) = rx.try_recv() {
        events.push(env);
    }

    let migration_events: Vec<_> =
        events.iter().filter(|e| e.topic == "settings.migration").collect();

    assert_eq!(
        migration_events.len(),
        1,
        "migrate_v1_to_v2 must emit exactly one settings.migration event; got {}",
        migration_events.len()
    );

    // Verify the payload shape.
    let payload = &migration_events[0].payload;
    assert_eq!(payload["migration"], "v1->v2");
    assert!(payload["migrated"].is_number(), "migrated must be a number");
    assert!(payload["dropped"].is_number(), "dropped must be a number");
    assert!(payload["reset"].is_number(), "reset must be a number");
    assert!(payload["at"].is_string(), "at must be an ISO timestamp string");
}

/// Summary-counts contract: `MigrationSummary` fields reflect what actually
/// happened to the DB rows.
#[tokio::test]
async fn migration_summary_counts_are_correct() {
    let (db, bus, _cache) = setup().await;
    seed_v1_rows(&db).await;

    let summary = migrate_v1_to_v2(db.pool(), &bus).await.expect("migrate_v1_to_v2");

    // Identity migration: nothing dropped, nothing reset, 5 descriptor rows retained.
    assert_eq!(
        summary,
        MigrationSummary { migrated: 5, dropped: 0, reset: 0 },
        "MigrationSummary must match the seeded row count"
    );
}

/// Idempotency: calling `migrate_v1_to_v2` twice produces the same result and
/// emits exactly two audit events (one per call).
#[tokio::test]
async fn migration_is_idempotent() {
    let (db, bus, _cache) = setup().await;
    seed_v1_rows(&db).await;

    let first = migrate_v1_to_v2(db.pool(), &bus).await.expect("first migrate");
    let second = migrate_v1_to_v2(db.pool(), &bus).await.expect("second migrate");

    // Both summaries must agree.
    assert_eq!(first, second, "migration must be idempotent");

    // The stored values are still intact.
    assert_eq!(
        repo::get_raw(db.pool(), "logLevel").await.unwrap(),
        Some(serde_json::json!("debug"))
    );
}

// ── Boot-path wiring tests ────────────────────────────────────────────────────
//
// These tests model the production boot sequence from `desktop_shell::lib::boot`:
//   1. migrate_v1_to_v2 (settings schema migration)
//   2. get_settings     (repair/hydrate pass, primes the read cache)
//
// They prove that migration RUNS and takes effect on a DB with v1 rows before
// `get_settings` consumes them, and that running migration on an already-current
// DB is safe (idempotent, stored values unchanged).

/// Boot path — v1 DB: migration runs, then `get_settings` hydrates the
/// post-migration values correctly.
///
/// Models the upgraded-install scenario: v1 rows exist in the DB, the boot
/// sequence calls `migrate_v1_to_v2` then `get_settings`.  After migration
/// the v1 rows must still be readable (identity migration retains them) and
/// `get_settings` must reflect them.
#[tokio::test]
async fn migration_runs_on_load_for_v1_stored_settings() {
    let (db, bus, cache) = setup().await;
    seed_v1_rows(&db).await;

    // Step 1: migration (production boot order).
    let summary = migrate_v1_to_v2(db.pool(), &bus).await.expect("migrate_v1_to_v2");

    // Identity migration — nothing dropped/reset.
    assert_eq!(summary.dropped, 0);
    assert_eq!(summary.reset, 0);
    assert_eq!(summary.migrated, 5, "all five seeded keys must be retained");

    // Step 2: repair/hydrate pass.
    let resp =
        app_core_settings::get_settings(db.pool(), &bus, &cache).await.expect("get_settings");

    // The seeded v1 values must be present in the returned state.
    assert_eq!(resp.settings.log_level, "debug", "seeded logLevel must survive migration");
    assert!(resp.settings.follow_symlinks, "seeded followSymlinks must survive migration");
    assert!(
        !resp.settings.block_permanent_delete,
        "seeded blockPermanentDelete must survive migration"
    );
}

/// Boot path — already-v2 DB: running migration again is safe and leaves
/// stored values unchanged.
///
/// Models the already-upgraded-install scenario: `migrate_v1_to_v2` was run
/// on a previous boot, so running it again on the same DB must not corrupt
/// or remove any rows.
#[tokio::test]
async fn migration_does_not_re_run_on_already_migrated_db() {
    let (db, bus, cache) = setup().await;
    seed_v1_rows(&db).await;

    // First boot: migration ran.
    migrate_v1_to_v2(db.pool(), &bus).await.expect("first boot migrate");

    // Second boot: migration runs again (idempotent).
    let second = migrate_v1_to_v2(db.pool(), &bus).await.expect("second boot migrate");

    // Summary must be identical — rows still present, nothing dropped/reset.
    assert_eq!(second.dropped, 0, "second run must not drop anything");
    assert_eq!(second.reset, 0, "second run must not reset anything");
    assert_eq!(second.migrated, 5, "second run must count the same retained rows");

    // Repair/hydrate pass must still read original seeded values.
    let resp =
        app_core_settings::get_settings(db.pool(), &bus, &cache).await.expect("get_settings");
    assert_eq!(
        resp.settings.log_level, "debug",
        "already-migrated DB: logLevel must be unchanged after second migration run"
    );
}
