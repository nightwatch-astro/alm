// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Extracted per-file metadata and classification breakdown:
//! `inbox_file_metadata` (spec 041 US2, migration 0045) and
//! `inbox_classification_breakdown`.

use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::DbResult;

/// Flat row from `inbox_classification_breakdown`.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InboxBreakdownRow {
    pub id: String,
    pub inbox_item_id: String,
    pub kind: String,
    pub count: i64,
    pub destination_preview: Option<String>,
    pub sample_files: String,
}

/// Flat row from `inbox_file_metadata` (spec 041 US2, migration 0045).
///
/// Per-file extracted image-header metadata, keyed 1:1 with the matching
/// `inbox_classification_evidence` row by `(inbox_item_id, relative_file_path)`.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InboxFileMetadataRow {
    pub id: String,
    pub inbox_item_id: String,
    pub relative_file_path: String,
    pub filter: Option<String>,
    pub exposure_s: Option<f64>,
    pub gain: Option<String>,
    pub binning_x: Option<i64>,
    pub binning_y: Option<i64>,
    pub temperature_c: Option<f64>,
    pub object: Option<String>,
    pub date_obs: Option<String>,
    pub instrume: Option<String>,
    pub telescop: Option<String>,
    pub naxis1: Option<i64>,
    pub naxis2: Option<i64>,
    pub stack_count: Option<i64>,
    pub file_size_bytes: Option<i64>,
    pub file_mtime: Option<String>,
    // ── T062 extended extraction (spec 041, migration 0049; wired T072) ─────
    pub offset: Option<i64>,
    pub set_temp_c: Option<f64>,
    pub ccd_temp_c: Option<f64>,
    pub ra_deg: Option<f64>,
    pub dec_deg: Option<f64>,
    pub rotator_angle_deg: Option<f64>,
    pub readout_mode: Option<String>,
    pub focal_length_mm: Option<f64>,
    pub date_loc: Option<String>,
}

/// Data to upsert one `inbox_file_metadata` row (spec 041 US2).
///
/// `gain` stays a string: some cameras report a non-integer/scaled gain value,
/// so the column is TEXT and we never coerce it to a number.
#[derive(Clone, Debug, Default)]
pub struct UpsertFileMetadata<'a> {
    pub inbox_item_id: &'a str,
    pub relative_file_path: &'a str,
    pub filter: Option<&'a str>,
    pub exposure_s: Option<f64>,
    pub gain: Option<&'a str>,
    pub binning_x: Option<i64>,
    pub binning_y: Option<i64>,
    pub temperature_c: Option<f64>,
    pub object: Option<&'a str>,
    pub date_obs: Option<&'a str>,
    pub instrume: Option<&'a str>,
    pub telescop: Option<&'a str>,
    pub naxis1: Option<i64>,
    pub naxis2: Option<i64>,
    pub stack_count: Option<i64>,
    pub file_size_bytes: Option<i64>,
    pub file_mtime: Option<&'a str>,
    // ── T062 extended extraction (spec 041, migration 0049; wired T072) ─────
    //
    // Sourced from `metadata_core::RawFileMetadata`'s T062 fields and wired
    // into this upsert at classify time (`crates/app/inbox/src/classify.rs`
    // `persist_file_metadata`) so `inbox.item.metadata` and
    // `inbox.target_recommendations` (T074) can read real values instead of
    // permanently-NULL columns.
    pub offset: Option<i64>,
    pub set_temp_c: Option<f64>,
    pub ccd_temp_c: Option<f64>,
    pub ra_deg: Option<f64>,
    pub dec_deg: Option<f64>,
    pub rotator_angle_deg: Option<f64>,
    pub readout_mode: Option<&'a str>,
    pub focal_length_mm: Option<f64>,
    pub date_loc: Option<&'a str>,
    // `pixel_size_um`/`sky_rotation_deg` columns have existed since migration
    // 0049 (read by `list_inbox_pointing`/R-17 target recommendations) but
    // were never wired on the write side until spec 052 P3 — without them,
    // FOV-aware radius and sky-PA rotation silently fell back to the fixed
    // radius / axis-aligned frame for every real ingested file.
    pub pixel_size_um: Option<f64>,
    pub sky_rotation_deg: Option<f64>,
    // Plate-solved WCS pointing (spec 052 P3, FR-012), distinct from the
    // mount `ra_deg`/`dec_deg` above — see migration 0062.
    pub wcs_ra_deg: Option<f64>,
    pub wcs_dec_deg: Option<f64>,
    pub wcs_rotation_deg: Option<f64>,
}

/// Fetch a single per-file metadata row for an item+path combination.
///
/// Returns `None` when no row has been persisted yet (file not yet classified).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn get_file_metadata(
    pool: &SqlitePool,
    inbox_item_id: &str,
    relative_file_path: &str,
) -> DbResult<Option<InboxFileMetadataRow>> {
    Ok(sqlx::query_as::<_, InboxFileMetadataRow>(
        "SELECT * FROM inbox_file_metadata
         WHERE inbox_item_id = ? AND relative_file_path = ?",
    )
    .bind(inbox_item_id)
    .bind(relative_file_path)
    .fetch_optional(pool)
    .await?)
}

// ── Breakdown CRUD ────────────────────────────────────────────────────────────

/// Connection-level variant of `delete_breakdown_for_item`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn delete_breakdown_for_item_conn(
    conn: &mut SqliteConnection,
    inbox_item_id: &str,
) -> DbResult<()> {
    sqlx::query("DELETE FROM inbox_classification_breakdown WHERE inbox_item_id = ?")
        .bind(inbox_item_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Upsert a single breakdown row.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn upsert_breakdown_row(
    pool: &SqlitePool,
    id: &str,
    inbox_item_id: &str,
    kind: &str,
    count: i64,
    destination_preview: Option<&str>,
    sample_files_json: &str,
) -> DbResult<()> {
    upsert_breakdown_row_conn(
        pool.acquire().await?.as_mut(),
        id,
        inbox_item_id,
        kind,
        count,
        destination_preview,
        sample_files_json,
    )
    .await
}

/// Connection-level variant of [`upsert_breakdown_row`].
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn upsert_breakdown_row_conn(
    conn: &mut SqliteConnection,
    id: &str,
    inbox_item_id: &str,
    kind: &str,
    count: i64,
    destination_preview: Option<&str>,
    sample_files_json: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO inbox_classification_breakdown
            (id, inbox_item_id, kind, count, destination_preview, sample_files)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(inbox_item_id, kind) DO UPDATE SET
             count = excluded.count,
             destination_preview = excluded.destination_preview,
             sample_files = excluded.sample_files",
    )
    .bind(id)
    .bind(inbox_item_id)
    .bind(kind)
    .bind(count)
    .bind(destination_preview)
    .bind(sample_files_json)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Fetch breakdown rows for an item.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn list_breakdown(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> DbResult<Vec<InboxBreakdownRow>> {
    Ok(sqlx::query_as::<_, InboxBreakdownRow>(
        "SELECT * FROM inbox_classification_breakdown WHERE inbox_item_id = ? ORDER BY kind",
    )
    .bind(inbox_item_id)
    .fetch_all(pool)
    .await?)
}

// ── File metadata CRUD (spec 041 US2) ─────────────────────────────────────────

/// Upsert one per-file metadata row, keyed on
/// `UNIQUE(inbox_item_id, relative_file_path)`.
///
/// Called from the classify/reclassify loop alongside the evidence row.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn upsert_inbox_file_metadata(
    pool: &SqlitePool,
    m: &UpsertFileMetadata<'_>,
) -> DbResult<()> {
    upsert_inbox_file_metadata_conn(pool.acquire().await?.as_mut(), m).await
}

/// Connection-level variant of [`upsert_inbox_file_metadata`].
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn upsert_inbox_file_metadata_conn(
    conn: &mut SqliteConnection,
    m: &UpsertFileMetadata<'_>,
) -> DbResult<()> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO inbox_file_metadata
            (id, inbox_item_id, relative_file_path, filter, exposure_s, gain,
             binning_x, binning_y, temperature_c, object, date_obs, instrume,
             telescop, naxis1, naxis2, stack_count, file_size_bytes, file_mtime,
             offset, set_temp_c, ccd_temp_c, ra_deg, dec_deg, rotator_angle_deg,
             readout_mode, focal_length_mm, date_loc, pixel_size_um, sky_rotation_deg,
             wcs_ra_deg, wcs_dec_deg, wcs_rotation_deg)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(inbox_item_id, relative_file_path) DO UPDATE SET
             filter = excluded.filter,
             exposure_s = excluded.exposure_s,
             gain = excluded.gain,
             binning_x = excluded.binning_x,
             binning_y = excluded.binning_y,
             temperature_c = excluded.temperature_c,
             object = excluded.object,
             date_obs = excluded.date_obs,
             instrume = excluded.instrume,
             telescop = excluded.telescop,
             naxis1 = excluded.naxis1,
             naxis2 = excluded.naxis2,
             stack_count = excluded.stack_count,
             file_size_bytes = excluded.file_size_bytes,
             file_mtime = excluded.file_mtime,
             offset = excluded.offset,
             set_temp_c = excluded.set_temp_c,
             ccd_temp_c = excluded.ccd_temp_c,
             ra_deg = excluded.ra_deg,
             dec_deg = excluded.dec_deg,
             rotator_angle_deg = excluded.rotator_angle_deg,
             readout_mode = excluded.readout_mode,
             focal_length_mm = excluded.focal_length_mm,
             date_loc = excluded.date_loc,
             pixel_size_um = excluded.pixel_size_um,
             sky_rotation_deg = excluded.sky_rotation_deg,
             wcs_ra_deg = excluded.wcs_ra_deg,
             wcs_dec_deg = excluded.wcs_dec_deg,
             wcs_rotation_deg = excluded.wcs_rotation_deg",
    )
    .bind(&id)
    .bind(m.inbox_item_id)
    .bind(m.relative_file_path)
    .bind(m.filter)
    .bind(m.exposure_s)
    .bind(m.gain)
    .bind(m.binning_x)
    .bind(m.binning_y)
    .bind(m.temperature_c)
    .bind(m.object)
    .bind(m.date_obs)
    .bind(m.instrume)
    .bind(m.telescop)
    .bind(m.naxis1)
    .bind(m.naxis2)
    .bind(m.stack_count)
    .bind(m.file_size_bytes)
    .bind(m.file_mtime)
    .bind(m.offset)
    .bind(m.set_temp_c)
    .bind(m.ccd_temp_c)
    .bind(m.ra_deg)
    .bind(m.dec_deg)
    .bind(m.rotator_angle_deg)
    .bind(m.readout_mode)
    .bind(m.focal_length_mm)
    .bind(m.date_loc)
    .bind(m.pixel_size_um)
    .bind(m.sky_rotation_deg)
    .bind(m.wcs_ra_deg)
    .bind(m.wcs_dec_deg)
    .bind(m.wcs_rotation_deg)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Number of bound columns per `inbox_file_metadata` INSERT row for chunking.
const FILE_META_COLS: usize = 32;
/// Maximum rows per batch INSERT (SQLite 32766-param limit / columns).
const FILE_META_BATCH_SIZE: usize = 32766 / FILE_META_COLS;

/// Batch-upsert multiple per-file metadata rows in one statement per chunk.
///
/// Chunks at `32766 / 32 = 1023` rows.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn upsert_inbox_file_metadata_batch(
    conn: &mut SqliteConnection,
    rows: &[UpsertFileMetadata<'_>],
) -> DbResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    // pre-generate ids once so each row has a stable id even in multi-row INSERT
    let ids: Vec<String> = rows.iter().map(|_| uuid::Uuid::new_v4().to_string()).collect();

    for (chunk_rows, chunk_ids) in
        rows.chunks(FILE_META_BATCH_SIZE).zip(ids.chunks(FILE_META_BATCH_SIZE))
    {
        let placeholders = (0..chunk_rows.len())
            .map(|_| "(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "INSERT INTO inbox_file_metadata
                (id, inbox_item_id, relative_file_path, filter, exposure_s, gain,
                 binning_x, binning_y, temperature_c, object, date_obs, instrume,
                 telescop, naxis1, naxis2, stack_count, file_size_bytes, file_mtime,
                 offset, set_temp_c, ccd_temp_c, ra_deg, dec_deg, rotator_angle_deg,
                 readout_mode, focal_length_mm, date_loc, pixel_size_um, sky_rotation_deg,
                 wcs_ra_deg, wcs_dec_deg, wcs_rotation_deg)
             VALUES {placeholders}
             ON CONFLICT(inbox_item_id, relative_file_path) DO UPDATE SET
                 filter = excluded.filter,
                 exposure_s = excluded.exposure_s,
                 gain = excluded.gain,
                 binning_x = excluded.binning_x,
                 binning_y = excluded.binning_y,
                 temperature_c = excluded.temperature_c,
                 object = excluded.object,
                 date_obs = excluded.date_obs,
                 instrume = excluded.instrume,
                 telescop = excluded.telescop,
                 naxis1 = excluded.naxis1,
                 naxis2 = excluded.naxis2,
                 stack_count = excluded.stack_count,
                 file_size_bytes = excluded.file_size_bytes,
                 file_mtime = excluded.file_mtime,
                 offset = excluded.offset,
                 set_temp_c = excluded.set_temp_c,
                 ccd_temp_c = excluded.ccd_temp_c,
                 ra_deg = excluded.ra_deg,
                 dec_deg = excluded.dec_deg,
                 rotator_angle_deg = excluded.rotator_angle_deg,
                 readout_mode = excluded.readout_mode,
                 focal_length_mm = excluded.focal_length_mm,
                 date_loc = excluded.date_loc,
                 pixel_size_um = excluded.pixel_size_um,
                 sky_rotation_deg = excluded.sky_rotation_deg,
                 wcs_ra_deg = excluded.wcs_ra_deg,
                 wcs_dec_deg = excluded.wcs_dec_deg,
                 wcs_rotation_deg = excluded.wcs_rotation_deg"
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for (m, id) in chunk_rows.iter().zip(chunk_ids.iter()) {
            q = q
                .bind(id)
                .bind(m.inbox_item_id)
                .bind(m.relative_file_path)
                .bind(m.filter)
                .bind(m.exposure_s)
                .bind(m.gain)
                .bind(m.binning_x)
                .bind(m.binning_y)
                .bind(m.temperature_c)
                .bind(m.object)
                .bind(m.date_obs)
                .bind(m.instrume)
                .bind(m.telescop)
                .bind(m.naxis1)
                .bind(m.naxis2)
                .bind(m.stack_count)
                .bind(m.file_size_bytes)
                .bind(m.file_mtime)
                .bind(m.offset)
                .bind(m.set_temp_c)
                .bind(m.ccd_temp_c)
                .bind(m.ra_deg)
                .bind(m.dec_deg)
                .bind(m.rotator_angle_deg)
                .bind(m.readout_mode)
                .bind(m.focal_length_mm)
                .bind(m.date_loc)
                .bind(m.pixel_size_um)
                .bind(m.sky_rotation_deg)
                .bind(m.wcs_ra_deg)
                .bind(m.wcs_dec_deg)
                .bind(m.wcs_rotation_deg);
        }
        q.execute(&mut *conn).await?;
    }
    Ok(())
}

/// Batch-delete file-metadata rows for a set of item ids.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn delete_file_metadata_for_items(
    conn: &mut SqliteConnection,
    inbox_item_ids: &[&str],
) -> DbResult<()> {
    for chunk in inbox_item_ids.chunks(32766) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let sql =
            format!("DELETE FROM inbox_file_metadata WHERE inbox_item_id IN ({placeholders})");
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for id in chunk {
            q = q.bind(*id);
        }
        q.execute(&mut *conn).await?;
    }
    Ok(())
}

/// Batch-delete breakdown rows for a set of item ids.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn delete_breakdown_for_items(
    conn: &mut SqliteConnection,
    inbox_item_ids: &[&str],
) -> DbResult<()> {
    for chunk in inbox_item_ids.chunks(32766) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "DELETE FROM inbox_classification_breakdown WHERE inbox_item_id IN ({placeholders})"
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for id in chunk {
            q = q.bind(*id);
        }
        q.execute(&mut *conn).await?;
    }
    Ok(())
}

/// Connection-level variant of `delete_file_metadata_for_item`.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn delete_file_metadata_for_item_conn(
    conn: &mut SqliteConnection,
    inbox_item_id: &str,
) -> DbResult<()> {
    sqlx::query("DELETE FROM inbox_file_metadata WHERE inbox_item_id = ?")
        .bind(inbox_item_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Fetch all per-file metadata rows for an item, ordered by relative path.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn list_inbox_file_metadata(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> DbResult<Vec<InboxFileMetadataRow>> {
    Ok(sqlx::query_as::<_, InboxFileMetadataRow>(
        "SELECT * FROM inbox_file_metadata WHERE inbox_item_id = ? ORDER BY relative_file_path",
    )
    .bind(inbox_item_id)
    .fetch_all(pool)
    .await?)
}

// ── Attribution geometry (spec 008 Q27, F-Framing-5) ────────────────────────

/// Per-file staged geometry read for the Inbox-confirm attribution pass.
///
/// A superset of [`InboxPointingRow`] with `telescop`/`instrume` added (the
/// F-Framing-5 optic-train composite needs them; target-resolution's
/// `InboxPointingRow` does not). Sourced from `inbox_file_metadata` — the
/// "non-durable inbox staging" pointing/rotation referenced by F-Framing-1
/// before it is copied onto the durable `acquisition_session` row at ingest.
#[derive(Clone, Debug, Default, sqlx::FromRow)]
pub struct InboxAttributionGeometryRow {
    pub relative_file_path: String,
    pub telescop: Option<String>,
    pub instrume: Option<String>,
    pub focal_length_mm: Option<f64>,
    pub ra_deg: Option<f64>,
    pub dec_deg: Option<f64>,
    pub rotator_angle_deg: Option<f64>,
    pub pixel_size_um: Option<f64>,
    pub naxis1: Option<i64>,
    pub naxis2: Option<i64>,
    pub object: Option<String>,
}

/// Read per-file attribution geometry for an inbox item (F-Framing-5).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn list_inbox_attribution_geometry(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> DbResult<Vec<InboxAttributionGeometryRow>> {
    Ok(sqlx::query_as::<_, InboxAttributionGeometryRow>(
        "SELECT relative_file_path, telescop, instrume, focal_length_mm,
                ra_deg, dec_deg, rotator_angle_deg, pixel_size_um, naxis1, naxis2, object
         FROM inbox_file_metadata
         WHERE inbox_item_id = ?
         ORDER BY relative_file_path",
    )
    .bind(inbox_item_id)
    .fetch_all(pool)
    .await?)
}

// ── Pointing / optics for target resolution (spec 041 R-17, T074) ─────────────

/// Per-file pointing + optics, read for coordinate-based target resolution.
///
/// Sourced from `inbox_file_metadata` (the T062 extended columns added in
/// migration 0048; `rotator_angle_deg`/`sky_rotation_deg` added in migration
/// 0049). All fields are nullable — best-effort extraction. The caller
/// derives a sub-group pointing (e.g. the first file carrying RA/Dec) and a
/// FOV-aware radius from `focal_length_mm`/`pixel_size_um`/`naxis1/2`.
///
/// Two rotation fields, two different consumers — never conflate them:
/// `rotator_angle_deg` (`ROTATANG`/`ROTATOR`) is the mechanical rotator
/// angle, the flat↔light match key (R-18, used by `grouping`); it is NOT a
/// sky-frame angle. `sky_rotation_deg` (`OBJCTROT`) is the true sky position
/// angle (East of North) — the one FOV frame-rotation matching
/// (`target_recommendations`) needs for `Constraint::frame_rotated`.
#[derive(Clone, Debug, Default, sqlx::FromRow)]
pub struct InboxPointingRow {
    pub relative_file_path: String,
    pub ra_deg: Option<f64>,
    pub dec_deg: Option<f64>,
    pub focal_length_mm: Option<f64>,
    pub pixel_size_um: Option<f64>,
    pub naxis1: Option<i64>,
    pub naxis2: Option<i64>,
    pub rotator_angle_deg: Option<f64>,
    pub sky_rotation_deg: Option<f64>,
    /// Raw `OBJECT` header value — display hint only, NEVER a matching key (R-17).
    pub object: Option<String>,
    /// Plate-solved WCS pointing (spec 052 P3, FR-012, migration 0062) — the
    /// high-confidence source, distinct from the mount `ra_deg`/`dec_deg`
    /// above (medium confidence). `None` when the file has no WCS solve.
    pub wcs_ra_deg: Option<f64>,
    pub wcs_dec_deg: Option<f64>,
    pub wcs_rotation_deg: Option<f64>,
}

/// Read per-file pointing + optics rows for an inbox item (R-17 / T074).
///
/// Returns one row per file with a persisted `inbox_file_metadata` row, ordered
/// by relative path. Rows without RA/Dec are still returned (the caller filters);
/// `object` is carried as a display hint only.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn list_inbox_pointing(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> DbResult<Vec<InboxPointingRow>> {
    Ok(sqlx::query_as::<_, InboxPointingRow>(
        "SELECT relative_file_path, ra_deg, dec_deg, focal_length_mm,
                pixel_size_um, naxis1, naxis2, rotator_angle_deg, sky_rotation_deg, object,
                wcs_ra_deg, wcs_dec_deg, wcs_rotation_deg
         FROM inbox_file_metadata
         WHERE inbox_item_id = ?
         ORDER BY relative_file_path",
    )
    .bind(inbox_item_id)
    .fetch_all(pool)
    .await?)
}
