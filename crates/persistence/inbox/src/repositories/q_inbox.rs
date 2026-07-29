// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! DB-boundary-zero drain: free-fn queries migrated out of
//! `crates/app/inbox` (`confirm.rs`, `plan_listener.rs`, `reclassify.rs`).
//!
//! Queries that matched an existing `repositories::inbox` fn exactly were
//! left calling that fn directly from the app layer; only genuinely new query
//! shapes live here. Follows the `inventory.rs`/`targets.rs` free-fn idiom —
//! no repo struct/trait.

use sqlx::SqlitePool;

use crate::repositories::inbox::InboxSourceGroupRow;
use persistence_core::DbResult;

// ── inbox_source_groups ─────────────────────────────────────────────────────

/// Fetch one `inbox_source_groups` row by primary key `id`.
///
/// `reclassify_v2` and `classify_source_group` only have the group id on hand
/// (from a `source_group_id` request field or an item lookup), not the
/// `(root_id, relative_path)` natural key.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn get_source_group_by_id(
    pool: &SqlitePool,
    id: &str,
) -> DbResult<Option<InboxSourceGroupRow>> {
    let row = sqlx::query_as::<_, InboxSourceGroupRow>(
        "SELECT id, root_id, relative_path, discovered_at, last_scanned_at,
                content_signature, format, lane, child_count
         FROM inbox_source_groups
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// ── inbox_classification_evidence ───────────────────────────────────────────

/// Write a `frameType` correction onto an evidence row and clear its
/// staleness flag (spec 041 T068 R-13 — the one explicit-correction
/// exception to fill-missing-only overrides).
///
/// Distinct from `repositories::inbox::set_manual_override`: this variant
/// also resets `override_stale = 0`, matching `reclassify_v2`'s original
/// inline query exactly.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn set_manual_override_reset_stale(
    pool: &SqlitePool,
    inbox_item_id: &str,
    relative_file_path: &str,
    frame_type: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE inbox_classification_evidence
         SET manual_override = ?,
             override_stale  = 0,
             evidence_source = 'manual_override'
         WHERE inbox_item_id = ? AND relative_file_path = ?",
    )
    .bind(frame_type)
    .bind(inbox_item_id)
    .bind(relative_file_path)
    .execute(pool)
    .await?;
    Ok(())
}

// ── calibration_session / calibration_fingerprint (master registration) ────

/// Data required to insert a `calibration_session` row for a registered
/// master (spec 041 US4/T032, spec 048 US1/T012).
#[derive(Clone, Debug)]
pub struct InsertCalibrationSession<'a> {
    pub id: &'a str,
    pub session_key: &'a str,
    /// JSON array of `file_record` ids (`"[]"` when the master frame could
    /// not be resolved to a `file_record`).
    pub frame_ids_json: &'a str,
    pub kind: &'a str,
    pub root_id: Option<&'a str>,
    pub source_inbox_item_id: &'a str,
}

/// Data required to insert a `calibration_fingerprint` row.
#[derive(Clone, Debug)]
pub struct InsertCalibrationFingerprint<'a> {
    pub calibration_session_id: &'a str,
    pub calibration_type: &'a str,
    pub exposure_s: Option<f64>,
    pub filter_name: Option<&'a str>,
}

/// Idempotency guard for master registration: does a `calibration_session`
/// row already reference this inbox item?
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn calibration_session_exists_for_inbox_item(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> DbResult<bool> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM calibration_session WHERE source_inbox_item_id = ? LIMIT 1")
            .bind(inbox_item_id)
            .fetch_optional(pool)
            .await?;
    Ok(existing.is_some())
}

/// Insert a `calibration_session` row registering a detected master.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn insert_calibration_session(
    pool: &SqlitePool,
    session: &InsertCalibrationSession<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO calibration_session
            (id, session_key, frame_ids, kind, root_id, created_at, source_inbox_item_id)
         VALUES (?, ?, ?, ?, ?, datetime('now'), ?)",
    )
    .bind(session.id)
    .bind(session.session_key)
    .bind(session.frame_ids_json)
    .bind(session.kind)
    .bind(session.root_id)
    .bind(session.source_inbox_item_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert a `calibration_fingerprint` row for a just-created
/// `calibration_session`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn insert_calibration_fingerprint(
    pool: &SqlitePool,
    fp: &InsertCalibrationFingerprint<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO calibration_fingerprint
            (id, calibration_type, exposure_s, filter_name)
         VALUES (?, ?, ?, ?)",
    )
    .bind(fp.calibration_session_id)
    .bind(fp.calibration_type)
    .bind(fp.exposure_s)
    .bind(fp.filter_name)
    .execute(pool)
    .await?;
    Ok(())
}
