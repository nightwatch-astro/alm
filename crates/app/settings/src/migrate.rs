// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Settings schema migration harness (spec 018 US5, T030 / T031).
//!
//! ## Purpose
//!
//! This module provides a **structured, testable harness** for migrating stored
//! settings rows from one schema version to the next. There is no v2 defined
//! yet; the implementation is an identity/no-op v1→v2 mapping whose drop and
//! reset lists are explicitly declared and currently empty. Adding real rules
//! for a future v2 is a single-site edit (add keys to `DROP_KEYS` /
//! `RESET_KEYS` below).
//!
//! ## Design notes
//!
//! - **Single audit event** (T031): exactly one `settings.migration` event is
//!   emitted at `info` level after the run, carrying `{ migrated, dropped,
//!   reset }` counts. No per-key events are emitted.
//! - **Idempotent** when called repeatedly: keys absent from the DB are silently
//!   skipped; dropping a non-existent key is a no-op.
//! - **Layer choice**: this module lives in `app_core_settings` (not in
//!   `persistence_db`) because it needs to know the v1 key set (from
//!   `descriptors`) and the default-value logic (`default_value_for_key`),
//!   both of which are app-layer concerns.
//!
//! ## v1→v2 migration rules (current: identity)
//!
//! | Rule       | Keys         | Effect                                 |
//! |------------|--------------|----------------------------------------|
//! | `DROP`     | *(none yet)* | Row deleted; key no longer valid in v2 |
//! | `RESET`    | *(none yet)* | Row deleted; v2 default applied on read |
//! | `RETAIN`   | all v1 keys  | Row kept as-is                         |
//!
//! To add a rule for a future v2 key rename/drop, edit `DROP_KEYS` or
//! `RESET_KEYS` and bump the migration label.

use audit::bus::EventBus;
use audit::event_bus::{SettingsMigration, Source, TOPIC_SETTINGS_MIGRATION};
use domain_core::ids::Timestamp;
use persistence_lifecycle::repositories::settings as repo;
use sqlx::SqlitePool;

use crate::descriptors;

// ── Migration rule tables ─────────────────────────────────────────────────────
//
// Extend these slices when a real v2 schema is defined.

/// Keys that are OBSOLETE in v2 and must be deleted from the settings table.
///
/// Currently empty — the v1→v2 bump is a no-op placeholder.
const DROP_KEYS: &[&str] = &[];

/// Keys whose stored value has CHANGED SEMANTICS in v2 and must be reset to
/// their new in-code default (the stored value is discarded).
///
/// Currently empty — the v1→v2 bump is a no-op placeholder.
const RESET_KEYS: &[&str] = &[];

// ── Public types ──────────────────────────────────────────────────────────────

/// Summary returned by [`migrate_v1_to_v2`].
///
/// Mirrors the `SettingsMigration` audit payload so callers can inspect
/// the outcome without parsing the audit log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationSummary {
    /// Number of v1 keys still present in the DB that were left unchanged
    /// (i.e. neither dropped nor reset).
    pub migrated: usize,
    /// Number of keys deleted because they are obsolete in v2.
    pub dropped: usize,
    /// Number of keys deleted so the v2 default takes effect on the next read.
    pub reset: usize,
}

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that can arise during a settings schema migration.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("database error during settings migration: {0}")]
    Db(#[from] persistence_core::DbError),
    #[error("audit bus error during settings migration: {0}")]
    Bus(#[from] audit::bus::BusError),
}

// ── Migration entry point ─────────────────────────────────────────────────────

/// Migrate all stored settings rows from the v1 key set to v2.
///
/// The migration applies three rules in key-declaration order:
///
/// 1. **Drop** — keys in `DROP_KEYS` are deleted from the `settings` table.
/// 2. **Reset** — keys in `RESET_KEYS` are deleted so the v2 in-code default
///    applies on the next `get_settings` call.
/// 3. **Retain** — all other v1 keys (from `descriptors::all_keys`) that are
///    actually stored in the DB are left unchanged.
///
/// After applying the rules, exactly one `settings.migration` audit event is
/// emitted (T031).
///
/// This function is **idempotent**: calling it a second time finds nothing to
/// drop/reset (the keys are already gone) and emits a summary of `{ 0, 0, 0 }`
/// for any subsequently absent keys.
///
/// # Errors
///
/// Returns [`MigrateError::Db`] on any database failure.
/// Returns [`MigrateError::Bus`] if the audit event cannot be written.
pub async fn migrate_v1_to_v2(
    pool: &SqlitePool,
    bus: &EventBus,
) -> Result<MigrationSummary, MigrateError> {
    let mut dropped = 0usize;
    let mut reset = 0usize;

    // 1. Drop obsolete keys.
    for &key in DROP_KEYS {
        repo::delete_key(pool, key).await?;
        dropped += 1;
    }

    // 2. Reset changed-semantics keys.
    for &key in RESET_KEYS {
        repo::delete_key(pool, key).await?;
        reset += 1;
    }

    // 3. Count retained keys (v1 stable keys that are stored and not dropped/reset).
    let drop_set: std::collections::HashSet<&str> =
        DROP_KEYS.iter().chain(RESET_KEYS.iter()).copied().collect();

    let all_stored = repo::get_all_raw(pool).await?;
    let migrated = all_stored
        .iter()
        .filter(|(key, _)| {
            // Only count keys that are in the v1 stable key set and not touched by
            // drop/reset rules.
            descriptors::descriptor_for(key).is_some() && !drop_set.contains(key.as_str())
        })
        .count();

    // 4. Emit a single info-level audit event (T031).
    let at = Timestamp::now_iso();
    bus.publish(
        TOPIC_SETTINGS_MIGRATION,
        Source::System,
        SettingsMigration { migration: "v1->v2".to_owned(), migrated, dropped, reset, at },
    )
    .await?;

    Ok(MigrationSummary { migrated, dropped, reset })
}

// ── Unit tests (pure logic, no I/O) ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Structural sanity: the drop and reset lists must not overlap and must
    /// not contain keys that are not in the v1 descriptor table.
    #[test]
    fn drop_and_reset_lists_are_valid_v1_keys() {
        // No key should appear in both lists.
        let drop_set: std::collections::HashSet<&str> = DROP_KEYS.iter().copied().collect();
        for &key in RESET_KEYS {
            assert!(
                !drop_set.contains(key),
                "key '{key}' appears in both DROP_KEYS and RESET_KEYS"
            );
        }

        // Every listed key must be a known v1 descriptor key.
        let all_v1: std::collections::HashSet<&str> = descriptors::all_keys().collect();
        for &key in DROP_KEYS.iter().chain(RESET_KEYS.iter()) {
            assert!(
                all_v1.contains(key),
                "key '{key}' in migration lists is not a v1 descriptor key"
            );
        }
    }

    // `v1_to_v2_is_currently_identity` removed: it only mirrored the
    // DROP_KEYS/RESET_KEYS const definitions against themselves, with zero
    // discriminating power. The real, currently-identity migration behavior
    // is already exercised end-to-end (real DB, real `migrate_v1_to_v2` call)
    // by `migration_summary_counts_are_correct` and `dropped_key_is_removed_from_db`
    // in `tests/settings_v1_to_v2.rs`, which assert `dropped == 0` / `reset == 0`
    // via actual execution rather than reading the const back.
}
