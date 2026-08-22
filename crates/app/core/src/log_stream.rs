// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Log stream and export use-case (spec 019).
//!
//! Provides:
//! - `recent_entries`: pull the N most-recent log entries from the durable
//!   `events` table (for the initial hydration window).
//! - `project_event`: projection helper mapping an event-bus row to a
//!   `LogEntry` (pure, deterministic, no I/O).
//! - `export_entries`: reads entries from the `events` table respecting
//!   `level_min`, `since`, `until`, and `include_diagnostics`, then writes
//!   a JSON file atomically (temp-file + rename).
//!
//! The live bus→frontend forwarding is handled by the Tauri adapter in
//! `apps/desktop/src-tauri/src/commands/log.rs` which subscribes to the
//! `EventBus` broadcast channel and emits `log:entry` Tauri events.
//!
//! Constitution V compliance: the `events` table is the durable record.
//! `LogEntry` is a derived projection; no new schema is introduced.

//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b) as a pure
//! leaf: it has zero `crate::` references and nothing else in `app_core`
//! references it. `app_core` re-exports this crate at `app_core::log_stream` so the
//! public surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use contracts_core::log::{LogEntry, LogEntrySource, LogExportErrorCode, LogLevel};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use sqlx::SqlitePool;

/// Size of the initial hydration window (matches the UI ring buffer).
pub const LOG_BUFFER_SIZE: usize = 500;

/// Contract error returned by use-case functions.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("database error: {0}")]
    Database(#[from] persistence_core::DbError),
    #[error("serialisation error: {0}")]
    Serialise(#[from] serde_json::Error),
    #[error("{code}: {message}")]
    Export { code: LogExportErrorCode, message: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl LogError {
    #[must_use]
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::Export { code, .. } => match code {
                LogExportErrorCode::PathWriteDenied => "path.write.denied",
                LogExportErrorCode::PathParentMissing => "path.parent.missing",
                LogExportErrorCode::RangeInvalid => "range.invalid",
                LogExportErrorCode::FormatUnsupported => "format.unsupported",
            },
            Self::Database(_) => "database.error",
            Self::Serialise(_) => "serialise.error",
            Self::Io(_) => "io.error",
        }
    }
}

/// Convert a `LogError` to a `ContractError`.
impl From<LogError> for ContractError {
    fn from(e: LogError) -> Self {
        match e {
            LogError::Export { code, message } => {
                let error_code = match code {
                    LogExportErrorCode::FormatUnsupported => ErrorCode::FormatUnsupported,
                    LogExportErrorCode::RangeInvalid => ErrorCode::RangeInvalid,
                    LogExportErrorCode::PathWriteDenied => ErrorCode::PathWriteDenied,
                    LogExportErrorCode::PathParentMissing => ErrorCode::PathParentMissing,
                };
                Self::new(error_code, message, ErrorSeverity::Blocking, false)
            }
            LogError::Database(db_err) => {
                Self::new(ErrorCode::DatabaseError, db_err.to_string(), ErrorSeverity::Fatal, true)
            }
            LogError::Serialise(e) => {
                Self::new(ErrorCode::SerialiseError, e.to_string(), ErrorSeverity::Fatal, false)
            }
            LogError::Io(e) => {
                Self::new(ErrorCode::IoError, e.to_string(), ErrorSeverity::Fatal, false)
            }
        }
    }
}

// ── Level ordering ────────────────────────────────────────────────────────────

fn level_passes(entry_level: LogLevel, level_min: Option<LogLevel>) -> bool {
    match level_min {
        None => true,
        Some(min) => entry_level.rank() >= min.rank(),
    }
}

fn source_passes(entry_source: LogEntrySource, source_filter: &[LogEntrySource]) -> bool {
    if source_filter.is_empty() {
        return true;
    }
    source_filter.contains(&entry_source)
}

// ── Projection ────────────────────────────────────────────────────────────────

/// Project one row from the `events` table into a `LogEntry`.
///
/// `event_id` is the SQLite rowid; it becomes the `aud:<n>` id.
///
/// This function is pure and deterministic — no I/O.
#[must_use]
pub fn project_event(event_id: i64, topic: &str, emitted_at: &str, payload_json: &str) -> LogEntry {
    let payload: serde_json::Value = serde_json::from_str(payload_json)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
    LogEntry::from_event_bus(event_id, topic, emitted_at, &payload)
}

// ── Recent entries (pull) ─────────────────────────────────────────────────────

/// Options for pulling recent entries.
pub struct RecentOptions<'a> {
    pub window_size: usize,
    pub level_min: Option<LogLevel>,
    pub include_diagnostics: bool,
    pub source_filter: &'a [LogEntrySource],
    /// If `Some`, only entries with `event_id > cursor_event_id` are returned.
    pub since_event_id: Option<i64>,
}

impl Default for RecentOptions<'_> {
    fn default() -> Self {
        Self {
            window_size: LOG_BUFFER_SIZE,
            level_min: None,
            include_diagnostics: true,
            source_filter: &[],
            since_event_id: None,
        }
    }
}

/// Result of [`recent_entries`]: the projected entries plus retention-gap
/// info (spec 019 FR-015).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEntries {
    /// Oldest-first window of projected entries.
    pub entries: Vec<LogEntry>,
    /// True when the caller's cursor predates the oldest `event_id` still
    /// retained in the `events` table — i.e. entries between the cursor and
    /// the oldest retained row were evicted before this call. `false` for a
    /// fresh-start subscription (no cursor supplied).
    pub truncated: bool,
    /// Exact count of entries evicted between the cursor and the oldest
    /// still-retained entry. `event_id` is an `AUTOINCREMENT` primary key
    /// that is contiguous under normal insert-only usage, so the gap width
    /// is derived directly (`oldest_retained - cursor - 1`) rather than
    /// estimated. Always `Some` when `truncated` is `true`; always `None`
    /// when `truncated` is `false`.
    pub truncated_count: Option<i64>,
}

/// Pull the most-recent log entries from the `events` table.
///
/// Returns at most `options.window_size` entries (default 500), ordered
/// oldest-first for UI appending.
///
/// When `options.since_event_id` is set, only entries with
/// `event_id > since_event_id` are returned (cursor-based resume). If the
/// supplied cursor predates the oldest entry still retained in `events`
/// (FR-015: entries evicted since the caller's last poll), the returned
/// [`RecentEntries::truncated`] flag is set and, since `event_id` gaps are
/// exactly countable, [`RecentEntries::truncated_count`] carries the exact
/// number of evicted entries.
///
/// # Errors
/// Returns `LogError::Database` if the query fails.
pub async fn recent_entries(
    pool: &SqlitePool,
    options: RecentOptions<'_>,
) -> Result<RecentEntries, LogError> {
    let limit = i64::try_from(options.window_size.min(LOG_BUFFER_SIZE)).unwrap_or(500);
    let since_id = options.since_event_id.unwrap_or(0);

    // Fetch the most recent N rows ordered by event_id descending, then
    // reverse for oldest-first output.
    let rows =
        persistence_lifecycle::repositories::events::list_recent_since(pool, since_id, limit)
            .await
            .map_err(LogError::from)?;

    let mut entries: Vec<LogEntry> = rows
        .into_iter()
        .rev() // oldest-first
        .map(|row| project_event(row.event_id, &row.topic, &row.emitted_at, &row.payload))
        .filter(|e| {
            // Apply level filter.
            if !level_passes(e.level, options.level_min) {
                return false;
            }
            // Apply source filter.
            if !source_passes(e.source, options.source_filter) {
                return false;
            }
            // Apply diagnostics gate.
            if !options.include_diagnostics && e.source == LogEntrySource::Diagnostic {
                return false;
            }
            true
        })
        .collect();

    // Truncate to window_size after filtering.
    let cap = options.window_size.min(LOG_BUFFER_SIZE);
    if entries.len() > cap {
        let drain_count = entries.len() - cap;
        entries.drain(0..drain_count);
    }

    let (truncated, truncated_count) = retention_gap(pool, options.since_event_id).await?;

    Ok(RecentEntries { entries, truncated, truncated_count })
}

/// Detect a retention/eviction gap (FR-015) between `cursor` and the oldest
/// `event_id` currently retained in the `events` table.
///
/// `event_id` is assigned by `AUTOINCREMENT` and is contiguous under normal
/// insert-only usage, so any missing ids between `cursor` and the oldest
/// retained row must have existed and been evicted (rather than never
/// having been written). A fresh-start subscription (`cursor = None`) never
/// reports a gap: there is no prior client state to have gone stale.
async fn retention_gap(
    pool: &SqlitePool,
    cursor: Option<i64>,
) -> Result<(bool, Option<i64>), LogError> {
    let Some(cursor_id) = cursor else {
        return Ok((false, None));
    };

    let oldest_retained = persistence_lifecycle::repositories::events::min_event_id(pool)
        .await
        .map_err(LogError::from)?;

    match oldest_retained {
        Some(min_id) if min_id > cursor_id + 1 => Ok((true, Some(min_id - cursor_id - 1))),
        _ => Ok((false, None)),
    }
}

// ── Export ────────────────────────────────────────────────────────────────────

/// Options for exporting log entries.
pub struct ExportOptions {
    pub file_path: String,
    pub level_min: Option<LogLevel>,
    /// ISO-8601 lower bound (inclusive).
    pub since: Option<String>,
    /// ISO-8601 upper bound (exclusive).
    pub until: Option<String>,
    pub include_diagnostics: bool,
}

/// Export matching log entries to a JSON file at `options.file_path`.
///
/// The destination passes `fs_pathsafe::export_dest`, so an existing file is
/// refused rather than replaced and the write cannot leave the canonicalized
/// parent directory.
///
/// # Errors
/// Returns `LogError::Export` for filesystem / format problems.
/// Returns `LogError::Database` on query failure.
pub async fn export_entries(
    pool: &SqlitePool,
    options: ExportOptions,
) -> Result<(String, usize, u64), LogError> {
    use fs_pathsafe::export_dest::{validate_export_destination, ExportWriteError};
    use std::io::Write;
    use std::path::Path;

    let dest = Path::new(&options.file_path);
    let validated = validate_export_destination(dest).map_err(|rejected| LogError::Export {
        code: export_error_code(rejected),
        message: format!("{}: {}", rejected.message(), dest.display()),
    })?;

    let rows = persistence_lifecycle::repositories::events::list_by_emitted_at_range(
        pool,
        options.since.as_deref(),
        options.until.as_deref(),
    )
    .await
    .map_err(LogError::from)?;

    let entries: Vec<LogEntry> = rows
        .into_iter()
        .map(|row| project_event(row.event_id, &row.topic, &row.emitted_at, &row.payload))
        .filter(|e| {
            if !level_passes(e.level, options.level_min) {
                return false;
            }
            if !options.include_diagnostics && e.source == LogEntrySource::Diagnostic {
                return false;
            }
            true
        })
        .collect();

    let count = entries.len();

    // Serialise to JSON array.
    let json_bytes = serde_json::to_vec_pretty(&entries).map_err(LogError::Serialise)?;

    let bytes =
        validated.write_atomically(|file| file.write_all(&json_bytes)).map_err(|e| match e {
            ExportWriteError::Rejected(rejected) => LogError::Export {
                code: export_error_code(rejected),
                message: format!("{}: {}", rejected.message(), dest.display()),
            },
            ExportWriteError::Io(e) => LogError::Io(e),
        })?;

    Ok((validated.path().to_string_lossy().into_owned(), count, bytes))
}

/// Map a destination rejection onto the two path codes the `log.export`
/// contract defines; the rejection's own message carries the specific rule.
fn export_error_code(
    rejected: fs_pathsafe::export_dest::DestinationRejected,
) -> LogExportErrorCode {
    use fs_pathsafe::export_dest::DestinationRejected as R;
    match rejected {
        R::NotAbsolute | R::NoParent | R::NoFileName | R::ParentMissing | R::ParentNotDirectory => {
            LogExportErrorCode::PathParentMissing
        }
        R::ReservedName | R::AlreadyExists => LogExportErrorCode::PathWriteDenied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn make_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("in-memory pool");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL,\
             source TEXT NOT NULL DEFAULT 'system',\
             emitted_at TEXT NOT NULL,\
             payload TEXT NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .expect("create events table");
        pool
    }

    async fn insert_event(pool: &SqlitePool, topic: &str, payload_json: &str) {
        sqlx::query("INSERT INTO events (topic, source, emitted_at, payload) VALUES (?, 'system', '2026-01-01T00:00:00Z', ?)")
            .bind(topic)
            .bind(payload_json)
            .execute(pool)
            .await
            .expect("insert event");
    }

    #[tokio::test]
    async fn recent_entries_returns_oldest_first() {
        let pool = make_pool().await;
        insert_event(&pool, "plan.approved", r#"{"plan_id":"p1"}"#).await;
        insert_event(&pool, "settings.changed", r#"{"key":"logLevel"}"#).await;

        let result = recent_entries(&pool, RecentOptions::default()).await.unwrap();
        assert_eq!(result.entries.len(), 2);
        // Oldest first: plan.approved came first.
        assert_eq!(result.entries[0].source, LogEntrySource::Plan);
        assert_eq!(result.entries[1].source, LogEntrySource::Settings);
    }

    #[tokio::test]
    async fn recent_entries_respects_level_min() {
        let pool = make_pool().await;
        insert_event(&pool, "plan.approved", "{}").await; // info
        insert_event(&pool, "catalog.download.failed", r#"{"message":"net error"}"#).await; // error

        let result = recent_entries(
            &pool,
            RecentOptions { level_min: Some(LogLevel::Warn), ..Default::default() },
        )
        .await
        .unwrap();

        // Only error-level entry should pass warn+ filter.
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].level, LogLevel::Error);
    }

    #[tokio::test]
    async fn recent_entries_respects_window_size() {
        let pool = make_pool().await;
        for i in 0..10 {
            insert_event(&pool, "plan.approved", &format!(r#"{{"plan_id":"p{i}"}}"#)).await;
        }

        let result = recent_entries(&pool, RecentOptions { window_size: 3, ..Default::default() })
            .await
            .unwrap();
        assert_eq!(result.entries.len(), 3);
    }

    #[tokio::test]
    async fn recent_entries_cursor_resume() {
        let pool = make_pool().await;
        insert_event(&pool, "plan.approved", r#"{"plan_id":"p1"}"#).await;
        insert_event(&pool, "plan.approved", r#"{"plan_id":"p2"}"#).await;
        insert_event(&pool, "plan.approved", r#"{"plan_id":"p3"}"#).await;

        // Get first two entries.
        let all = recent_entries(&pool, RecentOptions::default()).await.unwrap();
        assert_eq!(all.entries.len(), 3);

        // Parse cursor from second entry id: "aud:2"
        let cursor_id: i64 = all.entries[1].id.strip_prefix("aud:").unwrap().parse().unwrap();

        let resumed = recent_entries(
            &pool,
            RecentOptions { since_event_id: Some(cursor_id), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(resumed.entries.len(), 1);
        assert_eq!(resumed.entries[0].id, format!("aud:{}", cursor_id + 1));
        // Nothing was evicted between the cursor and the oldest retained row.
        assert!(!resumed.truncated, "no-gap resume must not report truncated");
        assert_eq!(resumed.truncated_count, None);
    }

    #[tokio::test]
    async fn recent_entries_fresh_start_never_truncated() {
        let pool = make_pool().await;
        // No events at all, and no cursor: fresh-start subscription.
        let result = recent_entries(&pool, RecentOptions::default()).await.unwrap();
        assert!(!result.truncated, "fresh start (no cursor) must never report truncated");
        assert_eq!(result.truncated_count, None);
        assert!(result.entries.is_empty());

        // Even once entries exist, a fresh-start (no cursor) request still
        // reports no gap: there is no prior client state to have gone stale.
        insert_event(&pool, "plan.approved", "{}").await;
        insert_event(&pool, "plan.approved", "{}").await;
        // Simulate a retention policy having evicted the very oldest row.
        sqlx::query("DELETE FROM events WHERE event_id = 1").execute(&pool).await.unwrap();

        let result = recent_entries(&pool, RecentOptions::default()).await.unwrap();
        assert!(!result.truncated, "fresh start must not report truncated even after eviction");
        assert_eq!(result.truncated_count, None);
    }

    #[tokio::test]
    async fn recent_entries_gap_after_eviction_is_detected() {
        let pool = make_pool().await;

        // Fill well past the ring buffer capacity so a retention/vacuum
        // policy would plausibly evict the oldest rows.
        for i in 0..(LOG_BUFFER_SIZE + 20) {
            insert_event(&pool, "plan.approved", &format!(r#"{{"plan_id":"p{i}"}}"#)).await;
        }

        // A client resumes from a stale cursor near the very beginning.
        let stale_cursor: i64 = 2;

        // Simulate eviction: a retention job has pruned the oldest 15 rows,
        // so `stale_cursor` now predates the oldest retained entry.
        sqlx::query("DELETE FROM events WHERE event_id <= 15").execute(&pool).await.unwrap();

        let resumed = recent_entries(
            &pool,
            RecentOptions { since_event_id: Some(stale_cursor), ..Default::default() },
        )
        .await
        .unwrap();

        assert!(resumed.truncated, "resuming from a cursor predating retention must be truncated");
        // Entries 3..=15 (13 entries) were evicted between the cursor and the
        // oldest retained row (16).
        assert_eq!(resumed.truncated_count, Some(13));
        // 505 rows (16..=520) survive eviction, which is more than the
        // window cap (LOG_BUFFER_SIZE = 500), so the returned window is
        // itself capped to the newest 500 — independent of the gap
        // detection above, which is derived purely from `event_id`s, not
        // from what the window happened to return.
        assert_eq!(resumed.entries.len(), LOG_BUFFER_SIZE);
        assert_eq!(resumed.entries.first().unwrap().id, "aud:21");
    }

    #[tokio::test]
    async fn project_event_plan_approved() {
        let payload = r#"{"plan_id":"abc","actor":"user","approved_at":"2026-01-01T00:00:00Z"}"#;
        let entry = project_event(10, "plan.approved", "2026-01-01T00:00:00Z", payload);
        assert_eq!(entry.id, "aud:10");
        assert_eq!(entry.source, LogEntrySource::Plan);
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.entity_type, Some("plan".to_owned()));
        assert_eq!(entry.entity_id, Some("abc".to_owned()));
    }

    #[tokio::test]
    async fn project_event_failed_topic_gives_error_level() {
        let payload =
            r#"{"catalog_id":"ngc","error_code":"network_timeout","message":"timed out"}"#;
        let entry = project_event(5, "catalog.download.failed", "2026-01-01T00:00:00Z", payload);
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.source, LogEntrySource::Catalog);
        assert_eq!(entry.message, "timed out");
    }

    #[tokio::test]
    async fn project_event_settings_changed_has_settings_source() {
        let payload = r#"{"key":"logLevel","prior_value":"info","new_value":"debug","at":"2026-01-01T00:00:00Z"}"#;
        let entry = project_event(3, "settings.changed", "2026-01-01T00:00:00Z", payload);
        assert_eq!(entry.source, LogEntrySource::Settings);
    }

    #[tokio::test]
    async fn source_filter_applied() {
        let pool = make_pool().await;
        insert_event(&pool, "plan.approved", "{}").await;
        insert_event(&pool, "settings.changed", "{}").await;
        insert_event(&pool, "catalog.download.started", "{}").await;

        let result = recent_entries(
            &pool,
            RecentOptions { source_filter: &[LogEntrySource::Plan], ..Default::default() },
        )
        .await
        .unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].source, LogEntrySource::Plan);
    }

    #[tokio::test]
    async fn export_entries_writes_json_file() {
        let pool = make_pool().await;
        insert_event(&pool, "plan.approved", r#"{"plan_id":"p1"}"#).await;
        insert_event(&pool, "plan.approved", r#"{"plan_id":"p2"}"#).await;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("export.json").to_string_lossy().into_owned();

        let (out_path, count, bytes) = export_entries(
            &pool,
            ExportOptions {
                file_path: path.clone(),
                level_min: None,
                since: None,
                until: None,
                include_diagnostics: false,
            },
        )
        .await
        .unwrap();

        // The response reports the canonicalized destination, which differs from
        // the requested string wherever the parent path crosses a symlink.
        let expected = std::path::Path::new(&path)
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .map(|p| p.join("export.json").to_string_lossy().into_owned())
            .expect("canonical parent");
        assert_eq!(out_path, expected);
        assert_eq!(count, 2);
        assert!(bytes > 0);

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[tokio::test]
    async fn export_entries_level_min_filter() {
        let pool = make_pool().await;
        insert_event(&pool, "plan.approved", "{}").await; // info
        insert_event(&pool, "catalog.download.failed", r#"{"message":"err"}"#).await; // error

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("export_warn.json").to_string_lossy().into_owned();

        let (_out_path, count, _bytes) = export_entries(
            &pool,
            ExportOptions {
                file_path: path,
                level_min: Some(LogLevel::Warn),
                since: None,
                until: None,
                include_diagnostics: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn export_entries_parent_missing_returns_error() {
        let pool = make_pool().await;
        let result = export_entries(
            &pool,
            ExportOptions {
                file_path: "/nonexistent/path/export.json".to_owned(),
                level_min: None,
                since: None,
                until: None,
                include_diagnostics: false,
            },
        )
        .await;
        assert!(result.is_err());
        if let Err(LogError::Export { code, .. }) = result {
            assert_eq!(code, LogExportErrorCode::PathParentMissing);
        } else {
            panic!("expected LogError::Export");
        }
    }

    async fn export_to(pool: &SqlitePool, file_path: String) -> Result<(), LogError> {
        export_entries(
            pool,
            ExportOptions {
                file_path,
                level_min: None,
                since: None,
                until: None,
                include_diagnostics: false,
            },
        )
        .await
        .map(|_| ())
    }

    #[tokio::test]
    async fn export_entries_refuses_an_existing_destination() {
        let pool = make_pool().await;
        insert_event(&pool, "plan.approved", "{}").await;
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("keep.json");
        std::fs::write(&dest, b"user data").expect("seed");

        let err = export_to(&pool, dest.to_string_lossy().into_owned()).await.expect_err("refused");

        assert!(matches!(err, LogError::Export { code: LogExportErrorCode::PathWriteDenied, .. }));
        assert_eq!(std::fs::read(&dest).expect("read"), b"user data");
    }

    #[tokio::test]
    async fn export_entries_refuses_a_relative_destination() {
        let pool = make_pool().await;
        let err = export_to(&pool, "../../export.json".to_owned()).await.expect_err("refused");
        assert!(matches!(
            err,
            LogError::Export { code: LogExportErrorCode::PathParentMissing, .. }
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn export_entries_does_not_write_through_a_symlinked_destination() {
        let pool = make_pool().await;
        insert_event(&pool, "plan.approved", "{}").await;
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = dir.path().join("outside.json");
        std::fs::write(&outside, b"outside").expect("seed");
        let link = dir.path().join("export.json");
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");

        let err = export_to(&pool, link.to_string_lossy().into_owned()).await.expect_err("refused");

        assert!(matches!(err, LogError::Export { code: LogExportErrorCode::PathWriteDenied, .. }));
        assert_eq!(std::fs::read(&outside).expect("read"), b"outside");
    }
}
