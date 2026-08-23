// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Free-fn repository queries backing `app_core::frame_inventory`,
//! `app_core::search`, and `app_core::sessions` (db-boundary-zero drain).
//!
//! Grouped by the app-layer module each function set backs. Business logic
//! (DTO mapping, filtering, presence-state derivation) stays in `app_core`;
//! this module only executes SQL and returns typed rows.

use sqlx::SqlitePool;

use crate::DbResult;

// ── frame_inventory ─────────────────────────────────────────────────────────

/// `frame_ids` JSON string for an `acquisition_session`, if it exists.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_acquisition_session_frame_ids(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT frame_ids FROM acquisition_session WHERE id = ?")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(f,)| f))
}

/// `(frame_ids, kind)` for a `calibration_session`, if it exists.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_calibration_session_frame_ids_and_kind(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Option<(String, String)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT frame_ids, kind FROM calibration_session WHERE id = ?")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

/// A `file_record` row (spec 048 per-frame inventory).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FileRecordRow {
    pub id: String,
    pub root_id: String,
    pub relative_path: String,
    pub size_bytes: i64,
    pub state: String,
}

/// `file_record` rows matching a set of ids. Empty `ids` short-circuits
/// without querying.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn file_records_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> DbResult<Vec<FileRecordRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, root_id, relative_path, size_bytes, state FROM file_record WHERE id IN (",
    );
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");
    let rows = builder.build_query_as::<FileRecordRow>().fetch_all(pool).await?;
    Ok(rows)
}

/// `file_record` rows for a given root.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn file_records_by_root(
    pool: &SqlitePool,
    root_id: &str,
) -> DbResult<Vec<FileRecordRow>> {
    let rows = sqlx::query_as::<_, FileRecordRow>(
        "SELECT id, root_id, relative_path, size_bytes, state FROM file_record WHERE root_id = ?",
    )
    .bind(root_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Id of the `acquisition_session` whose `frame_ids` JSON array contains
/// `frame_id` as an exact member.
///
/// `json_each` compares decoded array elements, so an id containing `%`, `_`,
/// or `"` matches itself and nothing else.
///
/// `frame_ids` carries no `json_valid` CHECK and `json_each` raises on a
/// malformed value, which fails the whole query rather than the one row. The
/// `CASE` substitutes an empty array so a corrupt row is skipped instead.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_acquisition_session_id_by_frame(
    pool: &SqlitePool,
    frame_id: &str,
) -> DbResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM acquisition_session \
         WHERE EXISTS (SELECT 1 FROM json_each( \
                           CASE WHEN json_valid(acquisition_session.frame_ids) \
                                THEN acquisition_session.frame_ids ELSE '[]' END) je \
                       WHERE je.value = ?) \
         LIMIT 1",
    )
    .bind(frame_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// `(session id, kind)` of the `calibration_session` whose `frame_ids` JSON
/// array contains `frame_id` as an exact member.
///
/// Guards `json_each` against a malformed `frame_ids` value for the reason
/// given on [`find_acquisition_session_id_by_frame`].
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_calibration_session_by_frame(
    pool: &SqlitePool,
    frame_id: &str,
) -> DbResult<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT id, kind FROM calibration_session \
         WHERE EXISTS (SELECT 1 FROM json_each( \
                           CASE WHEN json_valid(calibration_session.frame_ids) \
                                THEN calibration_session.frame_ids ELSE '[]' END) je \
                       WHERE je.value = ?) \
         LIMIT 1",
    )
    .bind(frame_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Mark a `file_record` row `missing`. Preserves the original call site's
/// `last_seen_at = last_seen_at` no-op assignment (leaves the column
/// untouched while still forming a valid `UPDATE`).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn mark_file_record_missing(pool: &SqlitePool, id: &str) -> DbResult<()> {
    sqlx::query(
        "UPDATE file_record SET state = 'missing', last_seen_at = last_seen_at WHERE id = ?",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// `file_record.content_hash` for a frame id. Assumes the row exists (the
/// caller already resolved it via [`file_records_by_ids`]); errors if it
/// does not.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_file_record_content_hash(pool: &SqlitePool, id: &str) -> DbResult<Option<String>> {
    let hash: Option<String> =
        sqlx::query_scalar("SELECT content_hash FROM file_record WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?;
    Ok(hash)
}

/// Re-home a `file_record` after a confirmed relink: update its path,
/// content hash, and last-seen timestamp, and flip its state back to
/// `classified` (spec 048 T025).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn relink_file_record(
    pool: &SqlitePool,
    id: &str,
    relative_path: &str,
    content_hash: &str,
    last_seen_at: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE file_record \
         SET relative_path = ?, content_hash = ?, state = 'classified', last_seen_at = ? \
         WHERE id = ?",
    )
    .bind(relative_path)
    .bind(content_hash)
    .bind(last_seen_at)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── search ───────────────────────────────────────────────────────────────────

/// Row from the target search query: id, resolved label, and the best
/// matching alias (if the match came via an alias rather than the primary
/// designation).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TargetSearchRow {
    pub id: String,
    pub label: String,
    pub alias_match: Option<String>,
}

/// Search `canonical_target` by primary designation or alias, `like_pattern`
/// already wrapped in `%...%`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn search_targets_by_like(
    pool: &SqlitePool,
    like_pattern: &str,
) -> DbResult<Vec<TargetSearchRow>> {
    let rows = sqlx::query_as::<_, TargetSearchRow>(
        // spec 036 reconciliation: query the gen-3 canonical_target / target_alias
        // tables (the legacy spec-013 targets / target_aliases were retired).
        "SELECT t.id, COALESCE(t.display_alias, t.primary_designation) AS label,
                (SELECT ta.alias FROM target_alias ta
                 WHERE ta.target_id = t.id
                   AND ta.normalized LIKE ? ESCAPE '\\'
                 LIMIT 1) AS alias_match
         FROM canonical_target t
         WHERE LOWER(t.primary_designation) LIKE ? ESCAPE '\\'
            OR EXISTS (
                SELECT 1 FROM target_alias ta2
                WHERE ta2.target_id = t.id
                  AND ta2.normalized LIKE ? ESCAPE '\\'
            )
         ORDER BY t.primary_designation ASC
         LIMIT 10",
    )
    .bind(like_pattern)
    .bind(like_pattern)
    .bind(like_pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A generic `(id, label)` row shared by the recent-target, session-search,
/// recent-session, and recent-project queries.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdLabelRow {
    pub id: String,
    pub label: String,
}

/// Most-recently-resolved `canonical_target` rows.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn recent_targets(pool: &SqlitePool) -> DbResult<Vec<IdLabelRow>> {
    let rows = sqlx::query_as::<_, IdLabelRow>(
        "SELECT id, COALESCE(display_alias, primary_designation) AS label FROM canonical_target \
         ORDER BY resolved_at DESC LIMIT 5",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Search `acquisition_session` by `session_key`, `like_pattern` already
/// wrapped in `%...%`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn search_sessions_by_like(
    pool: &SqlitePool,
    like_pattern: &str,
) -> DbResult<Vec<IdLabelRow>> {
    let rows = sqlx::query_as::<_, IdLabelRow>(
        "SELECT id, session_key AS label
         FROM acquisition_session
         WHERE LOWER(session_key) LIKE ? ESCAPE '\\'
         ORDER BY created_at DESC
         LIMIT 10",
    )
    .bind(like_pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Most-recently-created `acquisition_session` rows.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn recent_sessions(pool: &SqlitePool) -> DbResult<Vec<IdLabelRow>> {
    let rows = sqlx::query_as::<_, IdLabelRow>(
        "SELECT id, session_key AS label
         FROM acquisition_session
         ORDER BY created_at DESC
         LIMIT 5",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Row from the project search query.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProjectSearchRow {
    pub id: String,
    pub name: String,
    pub lifecycle: String,
}

/// Search `projects` by `name`, `like_pattern` already wrapped in `%...%`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn search_projects_by_like(
    pool: &SqlitePool,
    like_pattern: &str,
) -> DbResult<Vec<ProjectSearchRow>> {
    let rows = sqlx::query_as::<_, ProjectSearchRow>(
        "SELECT id, name, lifecycle
         FROM projects
         WHERE LOWER(name) LIKE ? ESCAPE '\\'
         ORDER BY name ASC
         LIMIT 10",
    )
    .bind(like_pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Most-recently-created `projects` rows.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn recent_projects(pool: &SqlitePool) -> DbResult<Vec<IdLabelRow>> {
    let rows = sqlx::query_as::<_, IdLabelRow>(
        "SELECT id, name AS label FROM projects ORDER BY created_at DESC LIMIT 5",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── sessions ─────────────────────────────────────────────────────────────────

/// `acquisition_session` row joined with its canonical target (spec 035
/// US4/T044). Shared by `sessions.list` and `sessions.get`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionJoinRow {
    pub id: String,
    pub session_key: String,
    pub target_id: Option<String>,
    pub frame_ids: String,
    pub created_at: String,
    pub canonical_target_id: Option<String>,
    pub canonical_target_name: Option<String>,
}

/// All `acquisition_session` rows, newest first.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_sessions_joined(pool: &SqlitePool) -> DbResult<Vec<SessionJoinRow>> {
    list_sessions_joined_paginated(pool, None, None).await
}

/// Paginated variant of [`list_sessions_joined`].
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_sessions_joined_paginated(
    pool: &SqlitePool,
    limit: Option<u32>,
    offset: Option<u32>,
) -> DbResult<Vec<SessionJoinRow>> {
    let sql = match (limit, offset) {
        (Some(l), Some(o)) => format!(
            "SELECT s.id, s.session_key, s.target_id, s.frame_ids, s.created_at, \
                    s.canonical_target_id, ct.primary_designation AS canonical_target_name \
             FROM acquisition_session s \
             LEFT JOIN canonical_target ct ON ct.id = s.canonical_target_id \
             ORDER BY s.created_at DESC LIMIT {l} OFFSET {o}"
        ),
        (Some(l), None) => format!(
            "SELECT s.id, s.session_key, s.target_id, s.frame_ids, s.created_at, \
                    s.canonical_target_id, ct.primary_designation AS canonical_target_name \
             FROM acquisition_session s \
             LEFT JOIN canonical_target ct ON ct.id = s.canonical_target_id \
             ORDER BY s.created_at DESC LIMIT {l}"
        ),
        _ => "SELECT s.id, s.session_key, s.target_id, s.frame_ids, s.created_at, \
                    s.canonical_target_id, ct.primary_designation AS canonical_target_name \
             FROM acquisition_session s \
             LEFT JOIN canonical_target ct ON ct.id = s.canonical_target_id \
             ORDER BY s.created_at DESC"
            .to_owned(),
    };
    // AssertSqlSafe: limit/offset are u32 literals formatted directly — no user strings.
    let rows =
        sqlx::query_as::<_, SessionJoinRow>(sqlx::AssertSqlSafe(sql)).fetch_all(pool).await?;
    Ok(rows)
}

/// A single `acquisition_session` row by id.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_session_joined(pool: &SqlitePool, id: &str) -> DbResult<Option<SessionJoinRow>> {
    let row = sqlx::query_as::<_, SessionJoinRow>(
        "SELECT s.id, s.session_key, s.target_id, s.frame_ids, s.created_at,
                s.canonical_target_id, ct.primary_designation AS canonical_target_name
         FROM acquisition_session s
         LEFT JOIN canonical_target ct ON ct.id = s.canonical_target_id
         WHERE s.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Canonical precedence for a session's linked target id (legacy `target_id`
/// wins over spec-035's `canonical_target_id` when both happen to be set).
///
/// Reviewer seq=277: `q_targets_mgmt::session_counts_by_target` and
/// `app_core::sessions::{list_sessions, get_session}` MUST agree on this
/// order — a session can end up with both columns set (`backfill_session_
/// targets` only gates on `canonical_target_id IS NULL`, not on `target_id`),
/// and disagreeing precedence would attribute the same session to two
/// different targets depending which code path reads it. Both call sites use
/// this single function so they can't re-drift.
#[must_use]
pub fn resolve_session_target_id(
    target_id: Option<String>,
    canonical_target_id: Option<String>,
) -> Option<String> {
    target_id.or(canonical_target_id)
}

/// Row from `acquisition_fingerprint` supplementary metadata dimensions.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FingerprintRow {
    pub gain: Option<f64>,
    pub filter_name: Option<String>,
    pub binning: Option<String>,
    pub optic_train: Option<String>,
    pub observing_night_date: Option<String>,
}

/// `acquisition_fingerprint` row for a session, if present.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_fingerprint(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Option<FingerprintRow>> {
    let row = sqlx::query_as::<_, FingerprintRow>(
        "SELECT gain, filter_name, binning, optic_train, observing_night_date
         FROM acquisition_fingerprint
         WHERE id = ?",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Active (non-`missing`) `(count, total_size_bytes)` for a set of
/// `file_record` ids (spec 048 US1, INV-5). Empty `ids` short-circuits to
/// `(0, 0)` without querying.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn active_frame_summary(pool: &SqlitePool, ids: &[String]) -> DbResult<(i64, i64)> {
    if ids.is_empty() {
        return Ok((0, 0));
    }
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0) FROM file_record \
         WHERE state != 'missing' AND id IN (",
    );
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");
    let (count, total): (i64, i64) = builder.build_query_as().fetch_one(pool).await?;
    Ok((count, total))
}

/// Sum of real per-frame `exposure_s` for the active (non-`missing`)
/// `file_record` ids of a session (#775). Empty `ids` short-circuits to `0.0`.
///
/// `file_record` carries no exposure column; the real value lives on
/// `inbox_file_metadata`, keyed by `(inbox_item_id, relative_file_path)` where
/// `relative_file_path` is the file's full path relative to its scan root
/// (confirmed: `app_inbox::confirm` joins it directly onto the root's
/// absolute path). So a frame's exposure is found by matching
/// `file_record.relative_path` against an `inbox_file_metadata` row reachable
/// from an `inbox_items` row sharing `file_record.root_id`. The inner query
/// groups by `fr.id` (picking the one real value with `MAX`, nulls ignored)
/// before summing, so a root with multiple inbox groups can never fan out a
/// single frame into multiple summed rows.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn active_frame_exposure_seconds(pool: &SqlitePool, ids: &[String]) -> DbResult<f64> {
    if ids.is_empty() {
        return Ok(0.0);
    }
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT COALESCE(SUM(per_frame.exposure_s), 0.0) FROM ( \
             SELECT fr.id, MAX(ifm.exposure_s) AS exposure_s \
             FROM file_record fr \
             LEFT JOIN inbox_items ii ON ii.root_id = fr.root_id \
             LEFT JOIN inbox_file_metadata ifm \
                 ON ifm.inbox_item_id = ii.id AND ifm.relative_file_path = fr.relative_path \
             WHERE fr.state != 'missing' AND fr.id IN (",
    );
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(") GROUP BY fr.id) AS per_frame");
    let (total,): (f64,) = builder.build_query_as().fetch_one(pool).await?;
    Ok(total)
}

/// The DISTINCT per-sub `exposure_s` values across a session's active
/// (non-`missing`) frames (#1218), ascending. Frames with no reachable
/// metadata row contribute nothing rather than a `NULL` entry.
///
/// Exposure is NOT part of `session_key` (`crates/sessions/src/key.rs`), so a
/// session can legitimately hold more than one per-sub exposure; callers need
/// the distinct set, not the [`active_frame_exposure_seconds`] sum, to decide
/// whether a single per-sub value exists.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn active_frame_exposures(pool: &SqlitePool, ids: &[String]) -> DbResult<Vec<f64>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = sqlx::QueryBuilder::new(
        // Same per-frame join as `active_frame_exposure_seconds` (one row per
        // frame via MAX+GROUP BY, so a root with several inbox groups cannot
        // fan a frame out), then collapsed to the distinct values.
        "SELECT DISTINCT per_frame.exposure_s FROM ( \
             SELECT fr.id, MAX(ifm.exposure_s) AS exposure_s \
             FROM file_record fr \
             LEFT JOIN inbox_items ii ON ii.root_id = fr.root_id \
             LEFT JOIN inbox_file_metadata ifm \
                 ON ifm.inbox_item_id = ii.id AND ifm.relative_file_path = fr.relative_path \
             WHERE fr.state != 'missing' AND fr.id IN (",
    );
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(
        ") GROUP BY fr.id) AS per_frame \
         WHERE per_frame.exposure_s IS NOT NULL ORDER BY per_frame.exposure_s",
    );
    let rows: Vec<(f64,)> = builder.build_query_as().fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(v,)| v).collect())
}

/// `project_id`s linked to a session via `project_sources`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn project_ids_for_session(pool: &SqlitePool, session_id: &str) -> DbResult<Vec<String>> {
    let ids = sqlx::query_scalar::<_, String>(
        "SELECT project_id FROM project_sources WHERE inventory_session_id = ?",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(ids)
}

/// A `calibration_assignment` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CalibrationAssignmentRow {
    pub master_id: String,
    pub calibration_type: String,
    pub confidence: f64,
    pub mismatched_dimensions: String,
    /// #718 (spec 007 SC-003): persisted so a reopened session detail can
    /// still distinguish an override assignment from a normal match.
    pub was_override: bool,
}

/// Calibration matches assigned to a session.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn calibration_matches_for_session(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Vec<CalibrationAssignmentRow>> {
    let rows = sqlx::query_as::<_, CalibrationAssignmentRow>(
        "SELECT master_id, calibration_type, confidence, mismatched_dimensions, was_override
         FROM calibration_assignment
         WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A session's `audit_log_entry` history row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditHistoryRow {
    pub at: String,
    pub trigger: String,
    pub actor: String,
}

/// Audit history for a session (`entity_type = 'acquisition_session'`),
/// oldest first.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn session_history(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Vec<AuditHistoryRow>> {
    let rows = sqlx::query_as::<_, AuditHistoryRow>(
        "SELECT at, trigger, actor
         FROM audit_log_entry
         WHERE entity_type = 'acquisition_session' AND entity_id = ?
         ORDER BY at ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── Frame-session map (DSD-8 LIKE-scan inversion) ───────────────────────────

/// All `(id, frame_ids)` pairs from `acquisition_session`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn all_acquisition_session_frame_ids(
    pool: &SqlitePool,
) -> DbResult<Vec<(String, String)>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, frame_ids FROM acquisition_session").fetch_all(pool).await?;
    Ok(rows)
}

/// All `(id, frame_ids, kind)` triples from `calibration_session`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn all_calibration_session_frame_ids(
    pool: &SqlitePool,
) -> DbResult<Vec<(String, String, String)>> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, frame_ids, kind FROM calibration_session")
            .fetch_all(pool)
            .await?;
    Ok(rows)
}

// ── Batch queries for sessions.list ──────────────────────────────────────────

/// Keyed fingerprint row for batch loading.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KeyedFingerprintRow {
    pub id: String,
    pub gain: Option<f64>,
    pub filter_name: Option<String>,
    pub binning: Option<String>,
    pub optic_train: Option<String>,
    pub observing_night_date: Option<String>,
}

/// Batch-load `acquisition_fingerprint` rows for multiple session ids.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_fingerprints_batch(
    pool: &SqlitePool,
    session_ids: &[String],
) -> DbResult<Vec<KeyedFingerprintRow>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, gain, filter_name, binning, optic_train, observing_night_date \
         FROM acquisition_fingerprint WHERE id IN (",
    );
    let mut sep = builder.separated(", ");
    for id in session_ids {
        sep.push_bind(id);
    }
    sep.push_unseparated(")");
    let rows: Vec<KeyedFingerprintRow> = builder.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

/// Batch-load `project_id` values for multiple sessions via `project_sources`.
/// Returns `(session_id, project_id)` pairs.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn project_ids_for_sessions_batch(
    pool: &SqlitePool,
    session_ids: &[String],
) -> DbResult<Vec<(String, String)>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT inventory_session_id, project_id FROM project_sources \
         WHERE inventory_session_id IN (",
    );
    let mut sep = builder.separated(", ");
    for id in session_ids {
        sep.push_bind(id);
    }
    sep.push_unseparated(")");
    let rows: Vec<(String, String)> = builder.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

// ── entity.names batch lookup ─────────────────────────────────────────────────

/// Batch-fetch display names for a list of project ids via IN-clause.
///
/// Returns `(id, name)` pairs only for ids that exist in the DB.
///
/// # Errors
/// Returns [`crate::DbError::Database`] on query failure.
pub async fn project_names_batch(
    pool: &SqlitePool,
    ids: &[String],
) -> DbResult<Vec<(String, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = sqlx::QueryBuilder::new("SELECT id, name FROM projects WHERE id IN (");
    let mut sep = builder.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    builder.push(")");
    let rows: Vec<(String, String)> = builder.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

/// Batch active frame summary: returns `(session_frame_ids_hash, count, total_size_bytes)`
/// per session. Takes a slice of `(session_id, frame_ids_json)` pairs.
/// Internally collects all frame IDs, runs one query, then re-attributes by session.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn active_frame_summaries_batch(
    pool: &SqlitePool,
    sessions_frame_ids: &[(String, Vec<String>)],
) -> DbResult<std::collections::HashMap<String, (i64, i64)>> {
    use std::collections::{HashMap, HashSet};

    let mut result: HashMap<String, (i64, i64)> = HashMap::new();
    if sessions_frame_ids.is_empty() {
        return Ok(result);
    }

    // Collect all unique frame ids and build reverse map (frame_id -> session_ids).
    let mut all_ids: Vec<&String> = Vec::new();
    let mut frame_to_session: HashMap<&str, &str> = HashMap::new();
    let mut seen: HashSet<&str> = HashSet::new();

    for (session_id, frame_ids) in sessions_frame_ids {
        result.insert(session_id.clone(), (0, 0));
        for fid in frame_ids {
            if seen.insert(fid.as_str()) {
                all_ids.push(fid);
                frame_to_session.insert(fid.as_str(), session_id.as_str());
            }
        }
    }

    if all_ids.is_empty() {
        return Ok(result);
    }

    // Query all at once: id, state, size_bytes for the frame ids.
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, size_bytes FROM file_record WHERE state != 'missing' AND id IN (",
    );
    let mut sep = builder.separated(", ");
    for id in &all_ids {
        sep.push_bind(*id);
    }
    sep.push_unseparated(")");

    let rows: Vec<(String, i64)> = builder.build_query_as().fetch_all(pool).await?;

    for (frame_id, size_bytes) in rows {
        if let Some(&sid) = frame_to_session.get(frame_id.as_str()) {
            let entry = result.entry(sid.to_owned()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += size_bytes;
        }
    }

    Ok(result)
}

/// Batch active frame exposure seconds: one query for all frame ids, re-attributed
/// per session. Returns `session_id -> total_exposure_s`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn active_frame_exposure_seconds_batch(
    pool: &SqlitePool,
    sessions_frame_ids: &[(String, Vec<String>)],
) -> DbResult<std::collections::HashMap<String, f64>> {
    use std::collections::{HashMap, HashSet};

    let mut result: HashMap<String, f64> = HashMap::new();
    if sessions_frame_ids.is_empty() {
        return Ok(result);
    }

    let mut all_ids: Vec<&String> = Vec::new();
    let mut frame_to_session: HashMap<&str, &str> = HashMap::new();
    let mut seen: HashSet<&str> = HashSet::new();

    for (session_id, frame_ids) in sessions_frame_ids {
        result.insert(session_id.clone(), 0.0);
        for fid in frame_ids {
            if seen.insert(fid.as_str()) {
                all_ids.push(fid);
                frame_to_session.insert(fid.as_str(), session_id.as_str());
            }
        }
    }

    if all_ids.is_empty() {
        return Ok(result);
    }

    // Same join as active_frame_exposure_seconds but batched.
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT fr.id, MAX(ifm.exposure_s) AS exposure_s \
         FROM file_record fr \
         LEFT JOIN inbox_items ii ON ii.root_id = fr.root_id \
         LEFT JOIN inbox_file_metadata ifm \
             ON ifm.inbox_item_id = ii.id AND ifm.relative_file_path = fr.relative_path \
         WHERE fr.state != 'missing' AND fr.id IN (",
    );
    let mut sep = builder.separated(", ");
    for id in &all_ids {
        sep.push_bind(*id);
    }
    sep.push_unseparated(") GROUP BY fr.id");

    let rows: Vec<(String, Option<f64>)> = builder.build_query_as().fetch_all(pool).await?;

    for (frame_id, exposure) in rows {
        if let Some(&sid) = frame_to_session.get(frame_id.as_str()) {
            let entry = result.entry(sid.to_owned()).or_insert(0.0);
            *entry += exposure.unwrap_or(0.0);
        }
    }

    Ok(result)
}

/// Batch-fetch display titles for a list of plan ids via IN-clause.
///
/// Returns `(id, title)` pairs only for ids that exist in the DB.
///
/// # Errors
/// Returns [`crate::DbError::Database`] on query failure.
pub async fn plan_titles_batch(
    pool: &SqlitePool,
    ids: &[String],
) -> DbResult<Vec<(String, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = sqlx::QueryBuilder::new("SELECT id, title FROM plans WHERE id IN (");
    let mut sep = builder.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    builder.push(")");
    let rows: Vec<(String, String)> = builder.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

/// Batch-fetch primary designations for a list of canonical target ids via IN-clause.
///
/// Returns `(id, primary_designation)` pairs only for ids that exist in the DB.
///
/// # Errors
/// Returns [`crate::DbError::Database`] on query failure.
pub async fn target_names_batch(
    pool: &SqlitePool,
    ids: &[String],
) -> DbResult<Vec<(String, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT CAST(id AS TEXT), \
         COALESCE(display_alias, primary_designation) \
         FROM canonical_target WHERE CAST(id AS TEXT) IN (",
    );
    let mut sep = builder.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    builder.push(")");
    let rows: Vec<(String, String)> = builder.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::setup_db;

    #[tokio::test]
    async fn frame_ids_absent_session_returns_none() {
        let db = setup_db().await;
        assert!(get_acquisition_session_frame_ids(db.pool(), "no-such").await.unwrap().is_none());
        assert!(get_calibration_session_frame_ids_and_kind(db.pool(), "no-such")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn file_records_by_ids_empty_short_circuits() {
        let db = setup_db().await;
        let rows = file_records_by_ids(db.pool(), &[]).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn active_frame_summary_empty_ids_short_circuits() {
        let db = setup_db().await;
        let (count, total) = active_frame_summary(db.pool(), &[]).await.unwrap();
        assert_eq!((count, total), (0, 0));
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)] // 0.0 is exactly representable; no accumulated arithmetic
    async fn active_frame_exposure_seconds_empty_ids_short_circuits() {
        let db = setup_db().await;
        assert_eq!(active_frame_exposure_seconds(db.pool(), &[]).await.unwrap(), 0.0);
    }

    /// Seed a `library_root` + `file_record` + `inbox_items` + `inbox_file_metadata`
    /// row so a frame's real per-file exposure is reachable via the
    /// `(root_id, relative_path)` join `active_frame_exposure_seconds` uses.
    async fn insert_frame_with_exposure(
        pool: &sqlx::SqlitePool,
        frame_id: &str,
        root_id: &str,
        relative_path: &str,
        state: &str,
        exposure_s: f64,
    ) {
        sqlx::query(
            "INSERT OR IGNORE INTO library_root (id, label, current_path, kind, state, created_at) \
             VALUES (?, ?, '/tmp', 'local', 'active', datetime('now'))",
        )
        .bind(root_id)
        .bind(root_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record \
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES (?, ?, ?, 100, 't0', ?, 't0', 't0')",
        )
        .bind(frame_id)
        .bind(root_id)
        .bind(relative_path)
        .bind(state)
        .execute(pool)
        .await
        .unwrap();
        let item_id = format!("item-{frame_id}");
        sqlx::query(
            "INSERT INTO inbox_items (id, root_id, relative_path, discovered_at, last_scanned_at) \
             VALUES (?, ?, ?, 't0', 't0')",
        )
        .bind(&item_id)
        .bind(root_id)
        .bind(relative_path)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO inbox_file_metadata (id, inbox_item_id, relative_file_path, exposure_s) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(format!("meta-{frame_id}"))
        .bind(&item_id)
        .bind(relative_path)
        .bind(exposure_s)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)] // seeded SUM of exact literal inputs; no rounding involved
    async fn active_frame_exposure_seconds_sums_real_per_frame_values() {
        let db = setup_db().await;
        insert_frame_with_exposure(db.pool(), "f-a", "root-x", "a.fits", "classified", 180.0).await;
        insert_frame_with_exposure(db.pool(), "f-b", "root-x", "b.fits", "classified", 180.0).await;

        let total = active_frame_exposure_seconds(db.pool(), &["f-a".to_owned(), "f-b".to_owned()])
            .await
            .unwrap();
        assert_eq!(total, 360.0, "real per-frame exposures must sum, never stay 0");
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)] // seeded SUM of exact literal inputs; no rounding involved
    async fn active_frame_exposure_seconds_excludes_missing_frames() {
        let db = setup_db().await;
        insert_frame_with_exposure(db.pool(), "f-present", "root-y", "p.fits", "classified", 300.0)
            .await;
        insert_frame_with_exposure(db.pool(), "f-gone", "root-y", "g.fits", "missing", 9999.0)
            .await;

        let total = active_frame_exposure_seconds(
            db.pool(),
            &["f-present".to_owned(), "f-gone".to_owned()],
        )
        .await
        .unwrap();
        assert_eq!(total, 300.0, "a missing frame's exposure drops out, mirroring INV-5");
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)] // 0.0 is exactly representable; no accumulated arithmetic
    async fn active_frame_exposure_seconds_defaults_to_zero_without_metadata() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
             VALUES ('root-z', 'root-z', '/tmp', 'local', 'active', datetime('now'))",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record \
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES ('f-nometa', 'root-z', 'n.fits', 100, 't0', 'classified', 't0', 't0')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let total =
            active_frame_exposure_seconds(db.pool(), &["f-nometa".to_owned()]).await.unwrap();
        assert_eq!(total, 0.0, "a frame with no reachable inbox_file_metadata degrades to 0");
    }

    /// Regression for the `GROUP BY fr.id` fan-out guard: two `inbox_items`
    /// rows in the SAME root (e.g. a stale/superseded duplicate from
    /// reclassify — see the doc comment on `active_frame_exposure_seconds`)
    /// each carry an `inbox_file_metadata` row matching the frame's exact
    /// `relative_path`. Joining `inbox_items` on `root_id` alone (no filter on
    /// its own `relative_path`) produces one row per (file_record, inbox_item)
    /// pair; without the inner `GROUP BY fr.id` + `MAX`, `SUM` would add both
    /// matches and double-count to 200.0 instead of the real 100.0.
    #[tokio::test]
    #[allow(clippy::float_cmp)] // exact literal input, no rounding involved
    async fn active_frame_exposure_seconds_collapses_duplicate_inbox_item_fan_out() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
             VALUES ('root-dup', 'root-dup', '/tmp', 'local', 'active', datetime('now'))",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record \
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES ('f-dup', 'root-dup', 'shared.fits', 100, 't0', 'classified', 't0', 't0')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        // Two DISTINCT inbox_items rows in the same root (different group_key),
        // each with its own inbox_file_metadata row for the identical
        // relative_file_path the frame carries.
        for item_id in ["item-dup-a", "item-dup-b"] {
            sqlx::query(
                "INSERT INTO inbox_items (id, root_id, relative_path, group_key, discovered_at, last_scanned_at) \
                 VALUES (?, 'root-dup', 'shared.fits', ?, 't0', 't0')",
            )
            .bind(item_id)
            .bind(item_id)
            .execute(db.pool())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO inbox_file_metadata (id, inbox_item_id, relative_file_path, exposure_s) \
                 VALUES (?, ?, 'shared.fits', 100.0)",
            )
            .bind(format!("meta-{item_id}"))
            .bind(item_id)
            .execute(db.pool())
            .await
            .unwrap();
        }

        let total = active_frame_exposure_seconds(db.pool(), &["f-dup".to_owned()]).await.unwrap();
        assert_eq!(
            total, 100.0,
            "a duplicate inbox_items row in the same root must not double-count exposure"
        );
    }

    #[tokio::test]
    async fn mark_file_record_missing_updates_state() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
             VALUES ('root-1', 'root-1', '/tmp', 'local', 'active', datetime('now'))",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record \
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES ('f-1', 'root-1', 'a.fits', 100, 't0', 'classified', 't0', 't0')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        mark_file_record_missing(db.pool(), "f-1").await.unwrap();

        let rows = file_records_by_root(db.pool(), "root-1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, "missing");
    }

    #[tokio::test]
    async fn search_and_recent_targets_roundtrip() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO canonical_target
                (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at) \
             VALUES ('t-1', NULL, 'NGC 7000', 'nebula', 10.0, 20.0, 'seed', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let found = search_targets_by_like(db.pool(), "%ngc%").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "t-1");

        let recent = recent_targets(db.pool()).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].label, "NGC 7000");
    }

    #[tokio::test]
    async fn list_and_get_session_joined_roundtrip() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
             VALUES ('s-1', '{}', '[]', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let all = list_sessions_joined(db.pool()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "s-1");

        let one = get_session_joined(db.pool(), "s-1").await.unwrap();
        assert!(one.is_some());
        assert!(get_session_joined(db.pool(), "missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn project_ids_for_session_empty_when_unlinked() {
        let db = setup_db().await;
        let ids = project_ids_for_session(db.pool(), "no-such").await.unwrap();
        assert!(ids.is_empty());
    }

    async fn insert_acquisition_session(pool: &SqlitePool, id: &str, frame_ids: &str) {
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
             VALUES (?, ?, ?, '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(id)
        .bind(frame_ids)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_calibration_session(pool: &SqlitePool, id: &str, frame_ids: &str) {
        sqlx::query(
            "INSERT INTO calibration_session (id, session_key, frame_ids, kind, created_at) \
             VALUES (?, ?, ?, 'dark', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(id)
        .bind(frame_ids)
        .execute(pool)
        .await
        .unwrap();
    }

    /// `_` is a single-character `LIKE` wildcard, so a pattern built from
    /// `f_1` also matched a session holding only `fx1`.
    #[tokio::test]
    async fn acquisition_lookup_rejects_underscore_wildcard_collision() {
        let db = setup_db().await;
        insert_acquisition_session(db.pool(), "s-other", r#"["fx1"]"#).await;

        let found = find_acquisition_session_id_by_frame(db.pool(), "f_1").await.unwrap();
        assert_eq!(found, None);
    }

    /// `%` matches any run of characters, so a pattern built from `f%` matched
    /// every session.
    #[tokio::test]
    async fn acquisition_lookup_rejects_percent_wildcard_collision() {
        let db = setup_db().await;
        insert_acquisition_session(db.pool(), "s-other", r#"["frame-9"]"#).await;

        let found = find_acquisition_session_id_by_frame(db.pool(), "f%").await.unwrap();
        assert_eq!(found, None);
    }

    /// An id that is a substring of a genuine member is not a member.
    #[tokio::test]
    async fn acquisition_lookup_rejects_prefix_collision() {
        let db = setup_db().await;
        insert_acquisition_session(db.pool(), "s-other", r#"["frame-1-extra"]"#).await;

        assert_eq!(find_acquisition_session_id_by_frame(db.pool(), "frame-1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn acquisition_lookup_finds_exact_member_with_metacharacters() {
        let db = setup_db().await;
        insert_acquisition_session(db.pool(), "s-1", r#"["a_b","c%d"]"#).await;

        assert_eq!(
            find_acquisition_session_id_by_frame(db.pool(), "a_b").await.unwrap().as_deref(),
            Some("s-1")
        );
        assert_eq!(
            find_acquisition_session_id_by_frame(db.pool(), "c%d").await.unwrap().as_deref(),
            Some("s-1")
        );
    }

    #[tokio::test]
    async fn calibration_lookup_rejects_wildcard_and_prefix_collisions() {
        let db = setup_db().await;
        insert_calibration_session(db.pool(), "c-other", r#"["fx1","frame-1-extra"]"#).await;

        assert_eq!(find_calibration_session_by_frame(db.pool(), "f_1").await.unwrap(), None);
        assert_eq!(find_calibration_session_by_frame(db.pool(), "f%").await.unwrap(), None);
        assert_eq!(find_calibration_session_by_frame(db.pool(), "frame-1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn calibration_lookup_finds_exact_member_with_metacharacters() {
        let db = setup_db().await;
        insert_calibration_session(db.pool(), "c-1", r#"["a_b"]"#).await;

        let found = find_calibration_session_by_frame(db.pool(), "a_b").await.unwrap();
        assert_eq!(found, Some(("c-1".to_owned(), "dark".to_owned())));
    }

    /// `frame_ids` has no `json_valid` CHECK. An unguarded `json_each` raises
    /// on a malformed row and fails the whole query, losing the sessions that
    /// are intact.
    #[tokio::test]
    async fn acquisition_lookup_skips_malformed_frame_ids() {
        let db = setup_db().await;
        insert_acquisition_session(db.pool(), "s-bad", "oops").await;
        insert_acquisition_session(db.pool(), "s-empty", "").await;
        insert_acquisition_session(db.pool(), "s-good", r#"["f1"]"#).await;

        let found = find_acquisition_session_id_by_frame(db.pool(), "f1")
            .await
            .expect("a corrupt row must not fail the lookup");
        assert_eq!(found.as_deref(), Some("s-good"));
    }

    #[tokio::test]
    async fn calibration_lookup_skips_malformed_frame_ids() {
        let db = setup_db().await;
        insert_calibration_session(db.pool(), "c-bad", "oops").await;
        insert_calibration_session(db.pool(), "c-empty", "").await;
        insert_calibration_session(db.pool(), "c-good", r#"["f1"]"#).await;

        let found = find_calibration_session_by_frame(db.pool(), "f1")
            .await
            .expect("a corrupt row must not fail the lookup");
        assert_eq!(found, Some(("c-good".to_owned(), "dark".to_owned())));
    }

    /// A repeated id in the array must not yield the session twice.
    #[tokio::test]
    async fn acquisition_lookup_returns_one_session_for_a_repeated_id() {
        let db = setup_db().await;
        insert_acquisition_session(db.pool(), "s-1", r#"["f1","f1"]"#).await;

        let found = find_acquisition_session_id_by_frame(db.pool(), "f1").await.unwrap();
        assert_eq!(found.as_deref(), Some("s-1"));
    }
}
