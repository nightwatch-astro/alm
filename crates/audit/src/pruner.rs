// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Events-table retention: age-based pruning run at startup and then daily.
//!
//! ## Design
//!
//! The `events` table is tier-2 (re-derivable per constitution) and grows
//! unbounded without this module.  Pruning removes rows whose `emitted_at` is
//! older than `retention_days` (default: 90).
//!
//! ## Replay-watermark interaction
//!
//! In-process subscribers (`StalePropagator`, `spawn_workflow_run_subscriber`)
//! start their durable-replay cursor at 0 and advance it only when a lag is
//! detected.  There is no persistent cursor registry, so the pruner cannot
//! consult per-subscriber cursors directly.
//!
//! The 90-day default is deliberately conservative: on a desktop application
//! it far exceeds any realistic subscriber lag window.  Hooks are
//! unconditionally idempotent (research.md §6.1), so a subscriber whose cursor
//! predates the pruning cutoff will re-dispatch already-processed events as
//! no-ops on the next lag recovery.
//!
//! Advancing that cursor on the *live* event path — rather than only on lag
//! recovery — was tried and reverted: the only id available there is the
//! table-wide `max_event_id`, not the handled row, so it skipped events the
//! subscriber never saw. Shrinking the replay window that way costs
//! correctness, so the window stays as wide as the pruning floor allows.

use sqlx::SqlitePool;

/// Default retention: events older than this many days are eligible for
/// deletion.
pub const DEFAULT_RETENTION_DAYS: u32 = 90;

/// Interval between scheduled prune passes.
const PRUNE_INTERVAL: std::time::Duration = std::time::Duration::from_hours(24);

/// Run one pruning pass: delete events older than `retention_days`.
///
/// Returns the number of rows deleted.
///
/// # Errors
/// Propagates database errors.
pub async fn run_once(
    pool: &SqlitePool,
    retention_days: u32,
) -> Result<u64, persistence_core::DbError> {
    let cutoff = cutoff_iso(retention_days);
    persistence_lifecycle::repositories::events::prune_events_older_than(pool, &cutoff).await
}

/// Spawn a background task that runs a prune pass at startup and then once
/// every 24 hours.
///
/// Low priority: runs on the same tokio runtime as the rest of the app.  A
/// prune failure is logged and skipped — never fatal.
#[must_use]
pub fn spawn(pool: SqlitePool, retention_days: u32) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(PRUNE_INTERVAL);
        // First tick fires immediately (tokio default MissedTickBehavior::Burst).
        loop {
            interval.tick().await;
            match run_once(&pool, retention_days).await {
                Ok(0) => {} // nothing to prune — silent
                Ok(n) => {
                    tracing::info!(
                        deleted = n,
                        retention_days,
                        "events pruner: removed old diagnostic events"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        retention_days,
                        "events pruner: prune pass failed; will retry tomorrow"
                    );
                }
            }
        }
    })
}

/// Build an ISO-8601 UTC cutoff string for rows older than `retention_days`.
///
/// Uses simple fixed arithmetic (no DST, no leap seconds) which is correct for
/// comparing against RFC-3339 UTC `emitted_at` values.
fn cutoff_iso(retention_days: u32) -> String {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let retention_secs = u64::from(retention_days) * 24 * 60 * 60;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs();
    let cutoff_secs = now.saturating_sub(retention_secs);

    // Convert to a minimal RFC-3339 UTC string (YYYY-MM-DDTHH:MM:SSZ).
    let secs_per_day: u64 = 86_400;
    let secs_per_hour: u64 = 3_600;
    let secs_per_min: u64 = 60;

    // Days since Unix epoch.
    let days = cutoff_secs / secs_per_day;
    let rem = cutoff_secs % secs_per_day;
    let hh = rem / secs_per_hour;
    let mm = (rem % secs_per_hour) / secs_per_min;
    let ss = rem % secs_per_min;

    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Convert days-since-Unix-epoch to (year, month, day).
/// Gregorian calendar, no external deps.
fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    // The casts here are bounded by calendar invariants (days < ~10^6 for any
    // reasonable date, month 1-12, day 1-31), so truncation and sign loss
    // cannot occur in practice.
    #[allow(clippy::cast_possible_wrap)] // days < 2^62 for any calendar date
    let z = days.cast_signed() + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    #[allow(clippy::cast_possible_wrap)] // yoe < 400, well within i64
    let y = yoe.cast_signed() + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    // y is a calendar year (1970..~2500 in practice), m is 1-12, d is 1-31.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (y as u32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::{cutoff_iso, days_to_ymd, DEFAULT_RETENTION_DAYS};

    #[test]
    fn cutoff_iso_format() {
        let iso = cutoff_iso(DEFAULT_RETENTION_DAYS);
        // Must be exactly 20 chars: YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(iso.len(), 20, "unexpected format: {iso}");
        assert!(iso.ends_with('Z'), "must end with Z: {iso}");
    }

    #[test]
    fn days_to_ymd_unix_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2026-07-24: days since 1970-01-01.
        // Computed: (2026-1970)*365 + leap_days + day_of_year
        // Quick check via a known value.
        let (y, m, d) = days_to_ymd(20658); // 2026-07-24
        assert_eq!((y, m, d), (2026, 7, 24), "days_to_ymd(20658)");
    }

    #[tokio::test]
    async fn run_once_prunes_old_rows() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL, source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL, payload TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert two events: one very old, one recent.
        sqlx::query(
            "INSERT INTO events (topic, source, emitted_at, payload) \
             VALUES ('t.a', 'system', '2020-01-01T00:00:00Z', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO events (topic, source, emitted_at, payload) \
             VALUES ('t.b', 'system', '2099-01-01T00:00:00Z', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let deleted = super::run_once(&pool, DEFAULT_RETENTION_DAYS).await.unwrap();
        assert_eq!(deleted, 1, "old row must be pruned");

        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM events").fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1, "recent row must survive");
    }
}
