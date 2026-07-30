// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Log stream and export Tauri commands (spec 019).
//!
//! ## Commands
//! - `log.recent` — pull the most-recent log entries from the durable events
//!   table (initial hydration window). Returns up to 500 entries oldest-first.
//! - `log.export` — write a filtered window of log entries to a JSON file.
//!
//! ## Live streaming
//! The bus→frontend forwarding is implemented via `start_log_forwarder`, which
//! is called from app setup and subscribes to the `EventBus` broadcast channel,
//! projecting each event to a `LogEntry` and emitting it as a `log:entry` Tauri
//! event. The frontend subscribes with `listen("log:entry", ...)`.
//!
//! **Startup seam note (deferred)**: the forwarder needs `AppHandle` (for
//! `emit`) and the `EventBus` (for `subscribe`). The current setup wires both
//! in `run_app`; see `start_log_forwarder` below. If the handle is not yet
//! available at state-init time, callers can defer the spawn until the first
//! window is created (`RunEvent::WindowEvent`). The pull path (`log.recent`)
//! is always real and delivers durable entries at any time.

use app_core::log_stream::{self, ExportOptions, RecentOptions};
use contracts_core::log::{
    LogEntry, LogEntrySource, LogExportResponse, LogLevel, LogRecentResponse,
    LOG_ENTRY_CONTRACT_VERSION,
};
use tauri::State;

use crate::commands::lifecycle::AppState;
use contracts_core::ContractError;

// ── log.recent ────────────────────────────────────────────────────────────────

/// `log.recent` — return the most-recent log entries (initial hydration window).
///
/// Accepts optional `cursor`, `level_min`, `include_diagnostics`, and
/// `source_filter` parameters.
///
/// # Errors
/// Returns `Err(String)` on database failure.
#[tauri::command]
#[specta::specta]
pub async fn log_recent(
    state: State<'_, AppState>,
    cursor: Option<String>,
    level_min: Option<LogLevel>,
    include_diagnostics: Option<bool>,
    source_filter: Option<Vec<LogEntrySource>>,
    window_size: Option<usize>,
) -> Result<LogRecentResponse, ContractError> {
    let pool = state.repo.pool();

    // Parse the cursor into an event_id.
    let since_event_id: Option<i64> =
        cursor.as_deref().and_then(|c| c.strip_prefix("aud:").and_then(|n| n.parse::<i64>().ok()));

    // We need owned source_filter for the lifetime in options.
    let owned_filter: Vec<LogEntrySource> = source_filter.unwrap_or_default();
    let opts_window = window_size.unwrap_or(log_stream::LOG_BUFFER_SIZE);
    let opts_level_min = level_min;
    let opts_include_diag = include_diagnostics.unwrap_or(true);

    let options = RecentOptions {
        window_size: opts_window,
        level_min: opts_level_min,
        include_diagnostics: opts_include_diag,
        source_filter: &owned_filter,
        since_event_id,
    };

    let result = log_stream::recent_entries(pool, options).await.map_err(ContractError::from)?;

    Ok(LogRecentResponse {
        contract_version: LOG_ENTRY_CONTRACT_VERSION.to_owned(),
        truncated: result.truncated,
        truncated_count: result.truncated_count,
        entries: result.entries,
    })
}

// ── log.export ────────────────────────────────────────────────────────────────

/// `log.export` — export filtered log entries to a JSON file.
///
/// # Errors
/// Returns `Err(String)` with code `"path.parent.missing"`, `"path.write.denied"`,
/// `"range.invalid"`, or `"format.unsupported"`.
#[tauri::command]
#[specta::specta]
pub async fn log_export(
    state: State<'_, AppState>,
    request_id: String,
    file_path: String,
    format: Option<String>,
    level_min: Option<LogLevel>,
    since: Option<String>,
    until: Option<String>,
    include_diagnostics: Option<bool>,
) -> Result<LogExportResponse, ContractError> {
    // Validate format field (fixed to "json" in v1).
    if let Some(ref fmt) = format {
        if fmt != "json" {
            return Err(ContractError::new(
                contracts_core::error_code::ErrorCode::FormatUnsupported,
                "only \"json\" format is supported",
                contracts_core::ErrorSeverity::Blocking,
                false,
            ));
        }
    }

    // Validate time range when both bounds are provided.
    if let (Some(ref s), Some(ref u)) = (&since, &until) {
        if s >= u {
            return Err(ContractError::new(
                contracts_core::error_code::ErrorCode::RangeInvalid,
                "since must be before until",
                contracts_core::ErrorSeverity::Blocking,
                false,
            ));
        }
    }

    let pool = state.repo.pool();
    let options = ExportOptions {
        file_path: file_path.clone(),
        level_min,
        since,
        until,
        include_diagnostics: include_diagnostics.unwrap_or(false),
    };

    let (out_path, count, bytes) =
        log_stream::export_entries(pool, options).await.map_err(ContractError::from)?;

    Ok(LogExportResponse {
        status: "success".to_owned(),
        contract_version: LOG_ENTRY_CONTRACT_VERSION.to_owned(),
        request_id,
        file_path: out_path,
        count,
        bytes: Some(bytes),
    })
}

// ── Live bus→frontend forwarder ───────────────────────────────────────────────

/// Emit every `events` row newer than `cursor` as a `log:entry`, returning the
/// advanced cursor.
///
/// A query failure leaves the cursor untouched so the next call retries the same
/// range rather than skipping it.
async fn drain_since<R: tauri::Runtime>(
    pool: &sqlx::SqlitePool,
    app_handle: &tauri::AppHandle<R>,
    log_level: LogLevel,
    cursor: i64,
) -> i64 {
    let rows = match persistence_lifecycle::repositories::events::list_since(pool, cursor).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("log forwarder: db query failed: {e}");
            return cursor;
        }
    };

    let mut cursor = cursor;
    for row in rows {
        cursor = cursor.max(row.event_id);
        let entry = app_core::log_stream::project_event(
            row.event_id,
            &row.topic,
            &row.emitted_at,
            &row.payload,
        );

        if entry.level.rank() >= log_level.rank() {
            let _ = tauri::Emitter::emit(app_handle, "log:entry", &entry);
        }
    }
    cursor
}

/// Spawn a background task that subscribes to the `EventBus` broadcast channel
/// and emits each new event as a `log:entry` Tauri event.
///
/// On each broadcast signal the forwarder queries the `events` table for all
/// rows since its last-seen cursor, projecting them to `LogEntry` and emitting
/// them. This guarantees the correct `event_id` (and thus the correct `aud:<n>`
/// id) for every entry, matching what `log.recent` returns.
///
/// **Startup seam**: call this from `run_app` after both `AppHandle` and
/// `EventBus` are ready. Rows committed before this task starts are replayed
/// immediately, without waiting for a broadcast, bounded to the newest
/// [`log_stream::LOG_BUFFER_SIZE`] events. Older rows cannot fit the frontend
/// ring buffer, so they are reported through a diagnostic entry (the same signal
/// the lag path emits) and remain reachable via `log.recent`.
///
/// **Cursor-init failure**: if the rewound cursor cannot be determined the
/// forwarder emits a diagnostic and stops. It must not fall back to cursor `0`,
/// which would replay the whole `events` table and defeat the bound.
///
/// The returned handle is how tests observe that stop; production callers ignore
/// it, since a detached forwarder needs no supervision.
pub fn start_log_forwarder<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    bus: &audit::bus::EventBus,
    log_level: LogLevel,
    pool: sqlx::SqlitePool,
) -> tokio::task::JoinHandle<()> {
    let mut rx = bus.subscribe();

    tokio::spawn(async move {
        let mut diag_seq: u64 = 0;

        // Rewind rather than jumping to the table maximum: a jump discards every
        // row committed between app start and forwarder start.
        let window = i64::try_from(log_stream::LOG_BUFFER_SIZE).unwrap_or(i64::MAX);
        let (mut cursor, skipped) =
            match persistence_lifecycle::repositories::events::rewound_cursor(&pool, window).await {
                Ok(seed) => seed,
                Err(e) => {
                    // No retry here: `persistence_core::BUSY_TIMEOUT` already gives
                    // sqlx a 5s window for lock contention, so an error past it is
                    // not transient. Stopping keeps the bound intact; `log.recent`
                    // still serves the durable history.
                    tracing::error!("log forwarder: cursor init failed, not streaming: {e}");
                    diag_seq += 1;
                    let diag = LogEntry::diagnostic(
                        diag_seq,
                        LogLevel::Error,
                        "Live log stream unavailable: could not read the event history. Refresh to reload.",
                    );
                    let _ = tauri::Emitter::emit(&app_handle, "log:entry", &diag);
                    return;
                }
            };

        if skipped > 0 {
            diag_seq += 1;
            let diag = LogEntry::diagnostic(
                diag_seq,
                LogLevel::Warn,
                format!("Log stream started mid-history: {skipped} earlier events not streamed. Refresh to reload."),
            );
            let _ = tauri::Emitter::emit(&app_handle, "log:entry", &diag);
        }

        // Catch up before the first broadcast: rows already inside the rewound
        // window must reach the panel even if no further event is ever emitted.
        cursor = drain_since(&pool, &app_handle, log_level, cursor).await;

        loop {
            match rx.recv().await {
                Ok(_envelope) => {
                    cursor = drain_since(&pool, &app_handle, log_level, cursor).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!("log forwarder: broadcast channel closed");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("log forwarder: lagged by {n} events");
                    // Emit a diagnostic entry for the lag gap.
                    diag_seq += 1;
                    let diag = LogEntry::diagnostic(
                        diag_seq,
                        LogLevel::Warn,
                        format!("Log stream lagged: {n} events dropped. Refresh to reload."),
                    );
                    let _ = tauri::Emitter::emit(&app_handle, "log:entry", &diag);
                }
            }
        }
    })
}
