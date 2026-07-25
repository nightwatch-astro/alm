// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository query functions for the spec-035/048 light-frame ingest
//! pipeline (`app_core_targets::frame_writer` / `ingest_resolution` /
//! `ingest_sessions`).
//!
//! Covers three tables:
//! - `file_record` — per-frame upsert keyed by UNIQUE `(root_id,
//!   relative_path)` (spec 048 T002).
//! - `ingest_resolution` — the async FITS `OBJECT` → `canonical_target`
//!   resolution queue (spec 035 US4).
//! - `acquisition_session` — light frames grouped by capture identity
//!   (spec 035 US4 / spec 041 FR-051/FR-052), plus `plan_items` reads that
//!   drive ingest and `ingest_resolution` reads that drive back-fill.
//!
//! Business logic (session-key derivation, target association, propagation
//! to linked projects) stays in `app_core_targets`; this module is query/exec
//! only. `library_root`/`registered_sources` root-path lookups reuse
//! [`crate::repositories::inventory::get_library_root_path`] and
//! `crate::repositories::first_run::get_source_path` rather than
//! duplicating them.
//!
//! Constitution §I: read/write SQLite metadata only; no filesystem mutations.
//! Constitution §V: SQLite is the durable record.

use sqlx::SqlitePool;
use time::format_description::well_known::Iso8601;
use time::OffsetDateTime;
use uuid::Uuid;

use persistence_core::DbResult;

// ── Row types ─────────────────────────────────────────────────────────────────

/// Internal row for the `file_record` id lookup in [`upsert_file_record`].
#[derive(Debug, Clone, sqlx::FromRow)]
struct FileRecordIdRow {
    id: String,
}

/// A pending `ingest_resolution` row (the drain's work item).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingIngestResolutionRow {
    pub id: String,
    pub object_raw: String,
    pub attempts: i64,
}

/// Applied `plan_items` row read by [`list_applied_light_plan_items`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AppliedPlanItemRow {
    pub to_root_id: Option<String>,
    pub to_relative_path: String,
    pub from_root_id: Option<String>,
    pub from_relative_path: String,
}

/// An `acquisition_session` row keyed by `session_key`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AcquisitionSessionByKeyRow {
    pub id: String,
    pub frame_ids: String,
    pub canonical_target_id: Option<String>,
}

/// A resolved `ingest_resolution` row (`image_id` → `target_id`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ResolvedIngestResolutionRow {
    pub image_id: String,
    pub target_id: String,
}

/// An `acquisition_session` row with no `canonical_target_id` set yet.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UnlinkedAcquisitionSessionRow {
    pub id: String,
    pub frame_ids: String,
}

// ── file_record ──────────────────────────────────────────────────────────────

/// Upsert a `file_record` by its UNIQUE `(root_id, relative_path)`, returning
/// its id. Reuses an existing row's id; (re)writes `state`, `size_bytes`, and
/// `mtime` to the given values (spec 048 FR-001/FR-002 — callers are expected
/// to pass the REAL on-disk size, never a `0` placeholder for a present frame).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn upsert_file_record(
    pool: &SqlitePool,
    root_id: &str,
    relative_path: &str,
    size_bytes: i64,
    mtime: &str,
    state: &str,
) -> DbResult<String> {
    if let Some(row) = sqlx::query_as::<_, FileRecordIdRow>(
        "SELECT id FROM file_record WHERE root_id = ? AND relative_path = ?",
    )
    .bind(root_id)
    .bind(relative_path)
    .fetch_optional(pool)
    .await?
    {
        sqlx::query(
            "UPDATE file_record
             SET state = ?, size_bytes = ?, mtime = ?, last_seen_at = ?
             WHERE id = ?",
        )
        .bind(state)
        .bind(size_bytes)
        .bind(mtime)
        .bind(OffsetDateTime::now_utc().format(&Iso8601::DEFAULT).unwrap_or_default())
        .bind(&row.id)
        .execute(pool)
        .await?;
        return Ok(row.id);
    }

    let id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc().format(&Iso8601::DEFAULT).unwrap_or_default();
    sqlx::query(
        "INSERT INTO file_record
            (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(root_id)
    .bind(relative_path)
    .bind(size_bytes)
    .bind(mtime)
    .bind(state)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(id)
}

// ── ingest_resolution ────────────────────────────────────────────────────────

/// Find an `ingest_resolution` row id for a given `(image_id, object_raw)`
/// pair (idempotency lookup shared by `enqueue` and the inline-resolve path).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_ingest_resolution_id(
    pool: &SqlitePool,
    image_id: &str,
    object_raw: &str,
) -> DbResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM ingest_resolution WHERE image_id = ? AND object_raw = ? LIMIT 1",
    )
    .bind(image_id)
    .bind(object_raw)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Insert a new `ingest_resolution` row with `attempts = 0`.
///
/// `state` is `"pending"` (enqueue path, `target_id = None`) or `"resolved"`
/// (inline cache-hit path, `target_id = Some(..)`).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn insert_ingest_resolution(
    pool: &SqlitePool,
    id: &str,
    image_id: &str,
    object_raw: &str,
    state: &str,
    target_id: Option<&str>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO ingest_resolution (id, image_id, object_raw, state, target_id, attempts)
         VALUES (?, ?, ?, ?, ?, 0)",
    )
    .bind(id)
    .bind(image_id)
    .bind(object_raw)
    .bind(state)
    .bind(target_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark an `ingest_resolution` row `resolved`, linking it to `target_id`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn mark_ingest_resolution_resolved(
    pool: &SqlitePool,
    row_id: &str,
    target_id: &str,
) -> DbResult<()> {
    sqlx::query("UPDATE ingest_resolution SET state = 'resolved', target_id = ? WHERE id = ?")
        .bind(target_id)
        .bind(row_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark an `ingest_resolution` row `unresolved`, incrementing `attempts`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn mark_ingest_resolution_unresolved(
    pool: &SqlitePool,
    row_id: &str,
    attempts: i64,
) -> DbResult<()> {
    sqlx::query("UPDATE ingest_resolution SET state = 'unresolved', attempts = ? WHERE id = ?")
        .bind(attempts + 1)
        .bind(row_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List up to `limit` `pending` `ingest_resolution` rows, oldest-inserted
/// first (`ORDER BY rowid ASC`) — the background drain's work queue.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_pending_ingest_resolutions(
    pool: &SqlitePool,
    limit: i64,
) -> DbResult<Vec<PendingIngestResolutionRow>> {
    let rows = sqlx::query_as::<_, PendingIngestResolutionRow>(
        "SELECT id, object_raw, attempts
         FROM ingest_resolution
         WHERE state = 'pending'
         ORDER BY rowid ASC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List every `resolved` `ingest_resolution` row (`image_id` → `target_id`),
/// used by [`ingest_sessions`](super)'s back-fill pass.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_resolved_ingest_resolutions(
    pool: &SqlitePool,
) -> DbResult<Vec<ResolvedIngestResolutionRow>> {
    let rows = sqlx::query_as::<_, ResolvedIngestResolutionRow>(
        "SELECT image_id, target_id
         FROM ingest_resolution
         WHERE state = 'resolved' AND target_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── plan_items (ingest source) ──────────────────────────────────────────────

/// List applied `move`/`catalogue` plan items for `plan_id`, ordered by
/// `item_index ASC`. Item type (light vs. calibration) is filtered later by
/// reading the FITS header at the resolved path.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_applied_light_plan_items(
    pool: &SqlitePool,
    plan_id: &str,
) -> DbResult<Vec<AppliedPlanItemRow>> {
    let rows = sqlx::query_as::<_, AppliedPlanItemRow>(
        "SELECT to_root_id, to_relative_path, from_root_id, from_relative_path
         FROM plan_items
         WHERE plan_id = ?
           AND action IN ('move', 'catalogue')
           AND item_state = 'succeeded'
         ORDER BY item_index ASC",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── library_root mirroring (R9) ─────────────────────────────────────────────

/// Mirror a `registered_sources` row into a `library_root` row with the SAME
/// id (`kind = 'local'`, `state = 'active'`), so the `file_record.root_id` FK
/// holds. A no-op (`INSERT OR IGNORE`) when the row already exists.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn insert_library_root_mirror(
    pool: &SqlitePool,
    root_id: &str,
    path: &str,
    created_at: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO library_root (id, label, current_path, kind, state, created_at)
         VALUES (?, ?, ?, 'local', 'active', ?)",
    )
    .bind(root_id)
    .bind(root_id)
    .bind(path)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

// ── acquisition_session ──────────────────────────────────────────────────────

/// Find an `acquisition_session` row by its (non-unique lookup index)
/// `session_key`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_acquisition_session_by_key(
    pool: &SqlitePool,
    key: &str,
) -> DbResult<Option<AcquisitionSessionByKeyRow>> {
    let row = sqlx::query_as::<_, AcquisitionSessionByKeyRow>(
        "SELECT id, frame_ids, canonical_target_id FROM acquisition_session WHERE session_key = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Append to an existing `acquisition_session` row, back-filling
/// `canonical_target_id` (previously unset) and `root_id` (`COALESCE`, sticky
/// to the first-known root).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn append_acquisition_session_frames_with_target(
    pool: &SqlitePool,
    id: &str,
    frames_json: &str,
    canonical_target_id: &str,
    root_id: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE acquisition_session \
         SET frame_ids = ?, canonical_target_id = ?, root_id = COALESCE(root_id, ?) \
         WHERE id = ?",
    )
    .bind(frames_json)
    .bind(canonical_target_id)
    .bind(root_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Append to an existing `acquisition_session` row without touching
/// `canonical_target_id`; `root_id` is `COALESCE`d in (sticky to the
/// first-known root).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn append_acquisition_session_frames(
    pool: &SqlitePool,
    id: &str,
    frames_json: &str,
    root_id: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE acquisition_session \
         SET frame_ids = ?, root_id = COALESCE(root_id, ?) WHERE id = ?",
    )
    .bind(frames_json)
    .bind(root_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert a new `acquisition_session` row. Legacy `target_id`/
/// `observer_location` columns are always `NULL` (R10 / v1 scope).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
#[allow(clippy::too_many_arguments)]
pub async fn insert_acquisition_session(
    pool: &SqlitePool,
    id: &str,
    key: &str,
    canonical_target_id: Option<&str>,
    has_observer_location: bool,
    frames_json: &str,
    root_id: &str,
    created_at: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO acquisition_session
            (id, session_key, target_id, canonical_target_id, has_observer_location,
             frame_ids, observer_location, root_id, created_at)
         VALUES (?, ?, NULL, ?, ?, ?, NULL, ?, ?)",
    )
    .bind(id)
    .bind(key)
    .bind(canonical_target_id)
    .bind(i64::from(has_observer_location))
    .bind(frames_json)
    .bind(root_id)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// List `acquisition_session` rows with no `canonical_target_id` set yet
/// (spec 035 US4/T043 back-fill candidates).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_unlinked_acquisition_sessions(
    pool: &SqlitePool,
) -> DbResult<Vec<UnlinkedAcquisitionSessionRow>> {
    let rows = sqlx::query_as::<_, UnlinkedAcquisitionSessionRow>(
        "SELECT id, frame_ids FROM acquisition_session WHERE canonical_target_id IS NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Set `canonical_target_id` on an `acquisition_session` row, only if it is
/// currently `NULL` (never overwrites an already-linked session).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn set_acquisition_session_canonical_target_if_null(
    pool: &SqlitePool,
    session_id: &str,
    target_id: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE acquisition_session SET canonical_target_id = ?
         WHERE id = ? AND canonical_target_id IS NULL",
    )
    .bind(target_id)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}
