// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for immutable `session` and `session_frame` rows, plus the
//! watermarked `session.list` query.
//!
//! Every accepted session is inserted together with its full frame membership in
//! one transaction. The spec requires at least one membership and exactly one
//! representative row before commit; the caller's transaction is expected to
//! enforce this via a deferred invariant query or application-layer check.
//!
//! `light_session_identity` is inserted atomically with the light session row.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

// ── Row projections ────────────────────────────────────────────────────────────

/// Minimal session row returned by get/list queries.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionRow {
    pub row_id: i64,
    pub public_id: String,
    pub materialization_operation_row_id: i64,
    pub kind: String,
    pub ordinal_in_operation: i64,
    pub identity_digest: String,
    pub observing_night_date: String,
    pub site_row_id: Option<i64>,
    pub timezone_name_snapshot: Option<String>,
    pub night_derivation: String,
    pub canonical_target_row_id: Option<i64>,
    pub created_sequence: i64,
    pub created_at: String,
}

/// Frame membership row returned by `session.frame.list`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionFrameRow {
    pub session_row_id: i64,
    pub frame_row_id: i64,
    pub materialization_operation_row_id: i64,
    pub ordinal: i64,
    pub is_representative: i64,
    pub created_sequence: i64,
}

/// Parameters for inserting one session row.
///
/// `canonical_target_row_id` is required for `light` sessions and must be
/// `None` for calibration sessions. The caller is responsible for the
/// consistency check before calling this function.
pub struct InsertSession<'a> {
    pub public_id: &'a str,
    pub materialization_operation_row_id: i64,
    pub kind: &'a str,
    pub ordinal_in_operation: i64,
    pub identity_digest: &'a str,
    pub observing_night_date: &'a str,
    pub site_row_id: Option<i64>,
    pub timezone_name_snapshot: Option<&'a str>,
    pub night_derivation: &'a str,
    pub canonical_target_row_id: Option<i64>,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Parameters for inserting one `session_frame` membership row.
pub struct InsertSessionFrame<'a> {
    pub session_row_id: i64,
    pub frame_row_id: i64,
    pub materialization_operation_row_id: i64,
    pub ordinal: i64,
    pub is_representative: bool,
    pub created_sequence: i64,
    pub _phantom: std::marker::PhantomData<&'a ()>,
}

/// Parameters for inserting one `light_session_identity` row.
pub struct InsertLightSessionIdentity<'a> {
    pub session_row_id: i64,
    pub optical_profile_row_id: i64,
    pub filter_label_row_id: i64,
    pub exposure_us: i64,
    pub gain_text: &'a str,
    pub offset_state: &'a str,
    pub offset_value: Option<i64>,
    pub binning_state: &'a str,
    pub bin_x: Option<i64>,
    pub bin_y: Option<i64>,
    pub readout_state: &'a str,
    pub readout_mode: Option<&'a str>,
    pub raster_width: i64,
    pub raster_height: i64,
    pub crop_state: &'a str,
    pub crop_payload: Option<&'a str>,
    pub parity: &'a str,
    pub footprint_digest: &'a str,
    pub representative_orientation_udeg: i64,
}

/// Filter parameters for `session.list`.
#[derive(Debug, Default)]
pub struct SessionListFilter<'a> {
    pub canonical_target_row_id: Option<i64>,
    pub kind: Option<&'a str>,
    pub observing_night_from: Option<&'a str>,
    pub observing_night_to: Option<&'a str>,
    pub camera_row_id: Option<i64>,
    pub optical_profile_row_id: Option<i64>,
    /// `None` → exclude superseded (default, matching the contract's omitted value);
    /// `Some(false)` → include all (superseded and non-superseded);
    /// `Some(true)` → only superseded.
    pub superseded_only: Option<bool>,
}

/// Cursor for watermarked `session.list` pagination.
///
/// The watermark pins the `repository_change` sequence observed on the first
/// page. Subsequent pages re-apply the same visibility window.
#[derive(Debug, Clone)]
pub struct SessionListCursor {
    pub watermark: i64,
    pub last_created_at: String,
    pub last_public_id: String,
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Insert one immutable session row and return its `row_id`.
///
/// The caller must hold `BEGIN IMMEDIATE` and must insert at least one
/// `session_frame` membership (including one representative) before commit.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_session(
    conn: &mut SqliteConnection,
    params: &InsertSession<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO session (
            public_id, materialization_operation_row_id, kind,
            ordinal_in_operation, identity_digest, observing_night_date,
            site_row_id, timezone_name_snapshot, night_derivation,
            canonical_target_row_id, created_sequence, created_at
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.materialization_operation_row_id)
    .bind(params.kind)
    .bind(params.ordinal_in_operation)
    .bind(params.identity_digest)
    .bind(params.observing_night_date)
    .bind(params.site_row_id)
    .bind(params.timezone_name_snapshot)
    .bind(params.night_derivation)
    .bind(params.canonical_target_row_id)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert one `session_frame` membership row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations.
pub async fn insert_session_frame(
    conn: &mut SqliteConnection,
    params: &InsertSessionFrame<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_frame (
            session_row_id, frame_row_id, materialization_operation_row_id,
            ordinal, is_representative, created_sequence
         ) VALUES (?,?,?,?,?,?)",
    )
    .bind(params.session_row_id)
    .bind(params.frame_row_id)
    .bind(params.materialization_operation_row_id)
    .bind(params.ordinal)
    .bind(i64::from(params.is_representative))
    .bind(params.created_sequence)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert one `light_session_identity` row alongside its parent session.
///
/// Must be called in the same transaction as [`insert_session`] for the same
/// `session_row_id`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations.
pub async fn insert_light_session_identity(
    conn: &mut SqliteConnection,
    params: &InsertLightSessionIdentity<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO light_session_identity (
            session_row_id, optical_profile_row_id, filter_label_row_id,
            exposure_us, gain_text,
            offset_state, offset_value,
            binning_state, bin_x, bin_y,
            readout_state, readout_mode,
            raster_width, raster_height,
            crop_state, crop_payload,
            parity, footprint_digest, representative_orientation_udeg
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.session_row_id)
    .bind(params.optical_profile_row_id)
    .bind(params.filter_label_row_id)
    .bind(params.exposure_us)
    .bind(params.gain_text)
    .bind(params.offset_state)
    .bind(params.offset_value)
    .bind(params.binning_state)
    .bind(params.bin_x)
    .bind(params.bin_y)
    .bind(params.readout_state)
    .bind(params.readout_mode)
    .bind(params.raster_width)
    .bind(params.raster_height)
    .bind(params.crop_state)
    .bind(params.crop_payload)
    .bind(params.parity)
    .bind(params.footprint_digest)
    .bind(params.representative_orientation_udeg)
    .execute(conn)
    .await?;
    Ok(())
}

/// Fetch a session by its `public_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_session_by_public_id(pool: &SqlitePool, public_id: &str) -> DbResult<SessionRow> {
    sqlx::query_as::<_, SessionRow>(
        "SELECT row_id, public_id, materialization_operation_row_id, kind,
                ordinal_in_operation, identity_digest, observing_night_date,
                site_row_id, timezone_name_snapshot, night_derivation,
                canonical_target_row_id, created_sequence, created_at
         FROM session WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("session {public_id}")))
}

/// Return the current `repository_change` sequence for use as a list watermark.
///
/// A `session.list` first page captures this value; subsequent pages supply it
/// as `cursor.watermark` to reconstruct the same visibility window.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn current_change_sequence(pool: &SqlitePool) -> DbResult<i64> {
    let seq: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(sequence), 0) FROM repository_change")
        .fetch_one(pool)
        .await?;
    Ok(seq.0)
}

/// Return a page of sessions visible at `watermark`, applying optional filters.
///
/// Sessions created after `watermark` and sessions whose visibility closed at
/// or before `watermark` are excluded. The result is ordered by
/// `(created_at DESC, public_id ASC)` and limited to `page_size` rows.
///
/// When `cursor` is `None` the first page is returned. When `cursor` is
/// `Some`, only rows whose `(created_at, public_id)` come after the cursor
/// values in the declared order are returned.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
#[allow(clippy::too_many_arguments)]
pub async fn list_sessions_at_watermark(
    pool: &SqlitePool,
    watermark: i64,
    filter: &SessionListFilter<'_>,
    cursor_created_at: Option<&str>,
    cursor_public_id: Option<&str>,
    page_size: i64,
) -> DbResult<Vec<SessionRow>> {
    // Visibility predicate: session was created at or before watermark and has
    // not been hidden at or before watermark.
    let rows = sqlx::query_as::<_, SessionRow>(
        "SELECT s.row_id, s.public_id, s.materialization_operation_row_id, s.kind,
                s.ordinal_in_operation, s.identity_digest, s.observing_night_date,
                s.site_row_id, s.timezone_name_snapshot, s.night_derivation,
                s.canonical_target_row_id, s.created_sequence, s.created_at
         FROM session s
         INNER JOIN session_visibility_history vh
             ON vh.session_row_id = s.row_id
            AND vh.visible_sequence <= ?1
            AND (vh.hidden_sequence IS NULL OR vh.hidden_sequence > ?1)
         WHERE s.created_sequence <= ?1
           AND (?2 IS NULL OR s.canonical_target_row_id = ?2)
           AND (?3 IS NULL OR s.kind = ?3)
           AND (?4 IS NULL OR s.observing_night_date >= ?4)
           AND (?5 IS NULL OR s.observing_night_date <= ?5)
           AND (?6 IS NULL OR EXISTS (
                SELECT 1 FROM session_equipment_resolution_head erh
                INNER JOIN session_equipment_resolution er
                    ON er.row_id = erh.head_resolution_row_id
                WHERE erh.session_row_id = s.row_id
                  AND er.camera_row_id = ?6
               ))
           AND (?7 IS NULL OR EXISTS (
                SELECT 1 FROM session_equipment_resolution_head erh
                INNER JOIN session_equipment_resolution er
                    ON er.row_id = erh.head_resolution_row_id
                WHERE erh.session_row_id = s.row_id
                  AND er.optical_profile_row_id = ?7
               ))
           AND (?8 IS NULL OR (
                CASE ?8
                    WHEN 1 THEN EXISTS (SELECT 1 FROM session_supersession WHERE predecessor_session_row_id = s.row_id)
                    WHEN 0 THEN NOT EXISTS (SELECT 1 FROM session_supersession WHERE predecessor_session_row_id = s.row_id)
                    ELSE 1
                END
               ))
           AND (?9 IS NULL OR s.created_at < ?9
                OR (s.created_at = ?9 AND s.public_id > ?10))
         ORDER BY s.created_at DESC, s.public_id ASC
         LIMIT ?11",
    )
    .bind(watermark)
    .bind(filter.canonical_target_row_id)
    .bind(filter.kind)
    .bind(filter.observing_night_from)
    .bind(filter.observing_night_to)
    .bind(filter.camera_row_id)
    .bind(filter.optical_profile_row_id)
    // None → exclude (SQL 0); Some(false) → include all (SQL NULL bypasses the CASE);
    // Some(true) → only superseded (SQL 1).
    .bind(match filter.superseded_only {
        None => Some(0i64),
        Some(false) => None,
        Some(true) => Some(1i64),
    })
    .bind(cursor_created_at)
    .bind(cursor_public_id)
    .bind(page_size)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Return all frame memberships for a session in ordinal order.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the session `public_id` does not exist, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn list_session_frames(
    pool: &SqlitePool,
    session_public_id: &str,
) -> DbResult<Vec<SessionFrameRow>> {
    let session = get_session_by_public_id(pool, session_public_id).await?;
    let rows = sqlx::query_as::<_, SessionFrameRow>(
        "SELECT session_row_id, frame_row_id, materialization_operation_row_id,
                ordinal, is_representative, created_sequence
         FROM session_frame
         WHERE session_row_id = ?
         ORDER BY ordinal ASC",
    )
    .bind(session.row_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Insert a `session_visibility_history` row when a session is first made visible.
///
/// Called atomically with the session insert in the materialization apply
/// transaction.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_session_visibility(
    conn: &mut SqliteConnection,
    session_row_id: i64,
    visible_sequence: i64,
    reason_code: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO session_visibility_history
         (session_row_id, visible_sequence, reason_code)
         VALUES (?,?,?)",
    )
    .bind(session_row_id)
    .bind(visible_sequence)
    .bind(reason_code)
    .execute(conn)
    .await?;
    Ok(())
}
