// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Native filesystem control use cases (spec 004).
//!
//! Thin orchestration layer providing input validation, error mapping to
//! contract error codes, and audit event emission for the three native
//! filesystem operations: directory pick, file pick, and reveal.

//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b) as a pure
//! leaf: it has zero `crate::` references and nothing else in `app_core`
//! references it. `app_core` re-exports this crate at `app_core::native` so the
//! public surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use audit::bus::EventBus;
use audit::event_bus::{NativeRevealFailed, Source, TOPIC_NATIVE_REVEAL_FAILED};
use contracts_core::native::{
    DirectoryPickRequest, DirectoryPickResponse, FilePickRequest, FilePickResponse, RevealRequest,
    RevealResponse, RevealSelection,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};

// ── Directory picker ────────────────────────────────────────────────────────

/// Validate a directory-pick request.
///
/// The actual dialog is opened by the Tauri command layer (which owns the
/// window handle). This use case validates inputs and maps errors.
///
/// # Errors
///
/// Returns `ContractError` with `picker.unavailable` if something prevents
/// the dialog from being shown.
#[allow(clippy::result_large_err)] // ContractError size is acceptable at this boundary
pub fn validate_directory_pick(_req: &DirectoryPickRequest) -> Result<(), ContractError> {
    // No server-side validation needed for directory pick beyond what the
    // contract schema enforces. The default_path is silently ignored when
    // it does not exist (per contract spec).
    Ok(())
}

/// Build a cancelled response for directory pick.
#[must_use]
pub fn directory_pick_cancelled() -> DirectoryPickResponse {
    DirectoryPickResponse { path: None, cancelled: true }
}

/// Build a success response for directory pick.
#[must_use]
pub fn directory_pick_selected(path: String) -> DirectoryPickResponse {
    DirectoryPickResponse { path: Some(path), cancelled: false }
}

// ── File picker ─────────────────────────────────────────────────────────────

/// Validate a file-pick request, including filter rules.
///
/// The wildcard `*` extension is ONLY valid in a filter named exactly
/// `"All files"`; any other filter containing `*` returns `filters.invalid`.
///
/// # Errors
///
/// Returns `ContractError` with `filters.invalid` when filters violate the
/// wildcard rule (D-004-1).
#[allow(clippy::result_large_err)] // ContractError size is acceptable at this boundary
pub fn validate_file_pick(req: &FilePickRequest) -> Result<(), ContractError> {
    for filter in &req.filters {
        let has_wildcard = filter.extensions.iter().any(|ext| ext.contains('*'));
        if has_wildcard && filter.name != "All files" {
            return Err(ContractError::new(
                ErrorCode::FiltersInvalid,
                format!(
                    "Wildcard '*' extension is only valid in a filter named \"All files\", \
                     but found in filter \"{}\"",
                    filter.name
                ),
                ErrorSeverity::Blocking,
                false,
            ));
        }
    }
    Ok(())
}

/// Build a cancelled response for file pick.
#[must_use]
pub fn file_pick_cancelled() -> FilePickResponse {
    FilePickResponse { path: None, selected_filter: None, cancelled: true }
}

/// Build a success response for file pick.
#[must_use]
pub fn file_pick_selected(path: String, selected_filter: Option<String>) -> FilePickResponse {
    FilePickResponse { path: Some(path), selected_filter, cancelled: false }
}

// ── Reveal ──────────────────────────────────────────────────────────────────

/// Check that the target path exists before attempting to reveal it.
///
/// Emits a `native.reveal.failed` audit event on failure.
///
/// # Errors
///
/// Returns `ContractError` with `path.not_exists` when the path does not
/// exist on disk.
#[allow(clippy::result_large_err)] // ContractError size is acceptable at this boundary
pub async fn validate_reveal_path(
    bus: &EventBus,
    req: &RevealRequest,
) -> Result<(), ContractError> {
    match std::fs::metadata(&req.path) {
        Ok(_) => Ok(()),
        Err(e) => {
            let error_code = if e.kind() == std::io::ErrorKind::NotFound {
                ErrorCode::PathNotExists
            } else {
                ErrorCode::OsCommandFailed
            };
            // Serialise the code to a dotted string for the audit event.
            let error_code_str = serde_json::to_string(&error_code)
                .map_or_else(|_| "internal.error".to_owned(), |s| s.trim_matches('"').to_owned());

            // Emit audit event (best-effort).
            let _ = bus
                .publish(
                    TOPIC_NATIVE_REVEAL_FAILED,
                    Source::System,
                    NativeRevealFailed {
                        error_code: error_code_str,
                        entity_kind: req.entity_kind.map(|k| {
                            serde_json::to_value(k)
                                .ok()
                                .and_then(|v| v.as_str().map(String::from))
                                .unwrap_or_default()
                        }),
                        entity_id: req.entity_id.clone(),
                        request_id: req.request_id.clone(),
                    },
                )
                .await;

            Err(ContractError::new(
                error_code,
                format!("Path does not exist: {}", req.path),
                ErrorSeverity::Blocking,
                false,
            ))
        }
    }
}

/// Build a success response for reveal.
#[must_use]
pub fn reveal_success(selection: RevealSelection) -> RevealResponse {
    RevealResponse { revealed: true, selection }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use contracts_core::native::{EntityKind, FileFilter};

    use super::*;

    // ── validate_file_pick ──────────────────────────────────────────────────

    #[test]
    fn rejects_wildcard_in_non_all_files_filter() {
        let req = FilePickRequest {
            request_id: "req-001".to_owned(),
            filters: vec![FileFilter {
                name: "Images".to_owned(),
                extensions: vec!["*".to_owned()],
            }],
            default_path: None,
        };
        let err = validate_file_pick(&req).unwrap_err();
        assert_eq!(err.code, ErrorCode::FiltersInvalid);
    }

    #[test]
    fn allows_wildcard_in_all_files_filter() {
        let req = FilePickRequest {
            request_id: "req-002".to_owned(),
            filters: vec![FileFilter {
                name: "All files".to_owned(),
                extensions: vec!["*".to_owned()],
            }],
            default_path: None,
        };
        assert!(validate_file_pick(&req).is_ok());
    }

    #[test]
    fn allows_normal_filters_without_wildcard() {
        let req = FilePickRequest {
            request_id: "req-003".to_owned(),
            filters: vec![
                FileFilter {
                    name: "FITS files".to_owned(),
                    extensions: vec!["fits".to_owned(), "fit".to_owned()],
                },
                FileFilter { name: "XISF files".to_owned(), extensions: vec!["xisf".to_owned()] },
            ],
            default_path: None,
        };
        assert!(validate_file_pick(&req).is_ok());
    }

    #[test]
    fn rejects_wildcard_mixed_with_normal_extensions() {
        let req = FilePickRequest {
            request_id: "req-004".to_owned(),
            filters: vec![FileFilter {
                name: "Mixed".to_owned(),
                extensions: vec!["fits".to_owned(), "*".to_owned()],
            }],
            default_path: None,
        };
        let err = validate_file_pick(&req).unwrap_err();
        assert_eq!(err.code, ErrorCode::FiltersInvalid);
    }

    // ── validate_reveal_path ────────────────────────────────────────────────

    #[tokio::test]
    async fn reveal_rejects_nonexistent_path() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL,\
             source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL,\
             payload TEXT NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        let bus = EventBus::with_pool(pool);

        let req = RevealRequest {
            request_id: "req-reveal-001".to_owned(),
            path: "/nonexistent/path/that/does/not/exist".to_owned(),
            entity_kind: Some(EntityKind::InventoryRow),
            entity_id: Some("inv-42".to_owned()),
        };

        let err = validate_reveal_path(&bus, &req).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::PathNotExists);
    }

    #[tokio::test]
    async fn reveal_accepts_existing_path() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL,\
             source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL,\
             payload TEXT NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        let bus = EventBus::with_pool(pool);

        // /tmp should exist on Unix.
        if cfg!(unix) {
            let req = RevealRequest {
                request_id: "req-reveal-002".to_owned(),
                path: "/tmp".to_owned(),
                entity_kind: None,
                entity_id: None,
            };
            assert!(validate_reveal_path(&bus, &req).await.is_ok());
        }
    }

    #[tokio::test]
    async fn reveal_failure_emits_audit_event() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events (\
             event_id INTEGER PRIMARY KEY AUTOINCREMENT,\
             topic TEXT NOT NULL,\
             source TEXT NOT NULL,\
             emitted_at TEXT NOT NULL,\
             payload TEXT NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        let bus = EventBus::with_pool(pool.clone());

        let req = RevealRequest {
            request_id: "req-audit-001".to_owned(),
            path: "/nonexistent/path/for/audit/test".to_owned(),
            entity_kind: Some(EntityKind::MasterCalibration),
            entity_id: Some("cal-99".to_owned()),
        };

        let _ = validate_reveal_path(&bus, &req).await;

        // Verify the audit event was written to the durable store.
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM events WHERE topic = 'native.reveal.failed'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1, "should have emitted one native.reveal.failed audit event");
    }

    // ── Response builders ───────────────────────────────────────────────────

    #[test]
    fn directory_pick_cancelled_response() {
        let resp = directory_pick_cancelled();
        assert!(resp.path.is_none());
        assert!(resp.cancelled);
    }

    #[test]
    fn directory_pick_selected_response() {
        let resp = directory_pick_selected("/astro/raw".to_owned());
        assert_eq!(resp.path.as_deref(), Some("/astro/raw"));
        assert!(!resp.cancelled);
    }

    #[test]
    fn file_pick_cancelled_response() {
        let resp = file_pick_cancelled();
        assert!(resp.path.is_none());
        assert!(resp.selected_filter.is_none());
        assert!(resp.cancelled);
    }

    #[test]
    fn file_pick_selected_response() {
        let resp = file_pick_selected(
            "/astro/darks/master.fits".to_owned(),
            Some("FITS files".to_owned()),
        );
        assert_eq!(resp.path.as_deref(), Some("/astro/darks/master.fits"));
        assert_eq!(resp.selected_filter.as_deref(), Some("FITS files"));
        assert!(!resp.cancelled);
    }

    #[test]
    fn reveal_success_response() {
        let resp = reveal_success(RevealSelection::Target);
        assert!(resp.revealed);
        assert_eq!(resp.selection, RevealSelection::Target);
    }
}
