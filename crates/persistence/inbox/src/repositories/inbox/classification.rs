// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Classification results, per-file evidence, and property overrides:
//! `inbox_classifications`, `inbox_classification_evidence`, and
//! `inbox_file_overrides`.

use domain_core::ids::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::DbResult;

/// Flat row from `inbox_classifications`.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InboxClassificationRow {
    pub inbox_item_id: String,
    pub result: String,
    pub frame_type: Option<String>,
    pub computed_at: String,
    pub content_signature: String,
    pub unclassified_file_count: i64,
}

/// Data to upsert an `inbox_classifications` row.
#[derive(Clone, Debug)]
pub struct UpsertClassification<'a> {
    pub inbox_item_id: &'a str,
    pub result: &'a str,
    pub frame_type: Option<&'a str>,
    pub content_signature: &'a str,
    pub unclassified_file_count: i64,
}

/// Flat row from `inbox_classification_evidence`, joined with per-file override
/// values from `inbox_file_overrides` (migration 0048).
///
/// The three non-type override fields (`override_filter`, `override_exposure_s`,
/// `override_binning`) are now sourced from `inbox_file_overrides` via the
/// `list_evidence` JOIN query. The struct API is stable so that callers in
/// `app_core` continue to read `row.override_filter` etc. unchanged.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InboxEvidenceRow {
    pub id: String,
    pub inbox_item_id: String,
    pub relative_file_path: String,
    pub frame_type: Option<String>,
    pub evidence_source: String,
    pub raw_value: Option<String>,
    pub unclassified: i64,
    pub manual_override: Option<String>,
    /// 1 when this file was detected as a calibration master (spec 040). Needed
    /// by the confirm path (spec 041 T052) to select the master destination
    /// pattern variant per file.
    pub is_master: i64,
    /// Non-type override fields (migration 0048): populated from
    /// `inbox_file_overrides` (property_key = 'filter'/'exposureS'/'binning')
    /// via the JOIN in `list_evidence`. NULL when no override has been set.
    pub override_filter: Option<String>,
    pub override_exposure_s: Option<f64>,
    pub override_binning: Option<String>,
    /// Durable frame-type override (property_key = 'frameType') from the same
    /// JOIN. Unlike `manual_override` (an evidence-row column that classify's
    /// wipe-and-reinsert can lose to a concurrent reclassify — the #854 CI
    /// race), this survives every evidence rebuild. Effective frame type
    /// resolves `manual_override` → `override_frame_type` → `frame_type`.
    pub override_frame_type: Option<String>,
    /// Durable target override (property_key = 'target') from the same JOIN —
    /// written by `cone_search::confirm`'s best-effort per-file link (#1294)
    /// so the confirmed designation becomes the effective OBJECT for the
    /// mandatory-attribute gate, not just the durable canonical_target row.
    /// NULL when no override has been set.
    pub override_target: Option<String>,
    /// 1 when any override recorded for this file is stale (file size/mtime
    /// changed since it was set — spec 041 R-4).
    pub override_stale: i64,
}

/// Data to insert an `inbox_classification_evidence` row.
#[derive(Clone, Debug)]
pub struct InsertEvidence<'a> {
    pub id: &'a str,
    pub inbox_item_id: &'a str,
    pub relative_file_path: &'a str,
    pub frame_type: Option<&'a str>,
    pub evidence_source: &'a str,
    pub raw_value: Option<&'a str>,
    pub unclassified: bool,
    pub manual_override: Option<&'a str>,
    /// Whether this file was detected as a calibration master (spec 040).
    pub is_master: bool,
    /// Provenance string from the detector, e.g. `"siril"` or `"pixinsight"`.
    pub master_detector: Option<&'a str>,
}

/// Upsert the `inbox_classifications` row for an item.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn upsert_classification(
    pool: &SqlitePool,
    c: &UpsertClassification<'_>,
) -> DbResult<()> {
    upsert_classification_conn(pool.acquire().await?.as_mut(), c).await
}

/// Connection-level variant of [`upsert_classification`].
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn upsert_classification_conn(
    conn: &mut SqliteConnection,
    c: &UpsertClassification<'_>,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO inbox_classifications
            (inbox_item_id, result, frame_type, computed_at, content_signature,
             unclassified_file_count)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(inbox_item_id) DO UPDATE SET
             result = excluded.result,
             frame_type = excluded.frame_type,
             computed_at = excluded.computed_at,
             content_signature = excluded.content_signature,
             unclassified_file_count = excluded.unclassified_file_count",
    )
    .bind(c.inbox_item_id)
    .bind(c.result)
    .bind(c.frame_type)
    .bind(&now)
    .bind(c.content_signature)
    .bind(c.unclassified_file_count)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Fetch the classification for an item, if any.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn get_classification(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> DbResult<Option<InboxClassificationRow>> {
    Ok(sqlx::query_as::<_, InboxClassificationRow>(
        "SELECT * FROM inbox_classifications WHERE inbox_item_id = ?",
    )
    .bind(inbox_item_id)
    .fetch_optional(pool)
    .await?)
}

// ── Evidence CRUD ─────────────────────────────────────────────────────────────

/// Insert a new evidence row.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn insert_evidence(pool: &SqlitePool, ev: &InsertEvidence<'_>) -> DbResult<()> {
    insert_evidence_conn(pool.acquire().await?.as_mut(), ev).await
}

/// Connection-level variant of [`insert_evidence`].
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn insert_evidence_conn(
    conn: &mut SqliteConnection,
    ev: &InsertEvidence<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO inbox_classification_evidence
            (id, inbox_item_id, relative_file_path, frame_type, evidence_source,
             raw_value, unclassified, manual_override, is_master, master_detector)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(ev.id)
    .bind(ev.inbox_item_id)
    .bind(ev.relative_file_path)
    .bind(ev.frame_type)
    .bind(ev.evidence_source)
    .bind(ev.raw_value)
    .bind(i64::from(ev.unclassified))
    .bind(ev.manual_override)
    .bind(i64::from(ev.is_master))
    .bind(ev.master_detector)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Columns per evidence row for batch-insert chunking.
const EVIDENCE_COLS: usize = 10;
/// Maximum rows per batch INSERT (SQLite param limit 32766 / columns).
const EVIDENCE_BATCH_SIZE: usize = 32766 / EVIDENCE_COLS;

/// Batch-insert multiple evidence rows in a single statement per chunk.
///
/// Chunks at `32766 / 10 = 3276` rows to stay within SQLite's 32766-parameter
/// limit. Falls back to single-row insert for any remainder. Each call
/// generates fresh UUIDs for any row whose `id` is the empty string, but the
/// caller is expected to set `ev.id` before passing.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn insert_evidence_batch(
    conn: &mut SqliteConnection,
    rows: &[InsertEvidence<'_>],
) -> DbResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    for chunk in rows.chunks(EVIDENCE_BATCH_SIZE) {
        // Build "VALUES (?,?,?,?,?,?,?,?,?,?), …" with one tuple per row.
        let placeholders =
            (0..chunk.len()).map(|_| "(?,?,?,?,?,?,?,?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT INTO inbox_classification_evidence
                (id, inbox_item_id, relative_file_path, frame_type, evidence_source,
                 raw_value, unclassified, manual_override, is_master, master_detector)
             VALUES {placeholders}"
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for ev in chunk {
            q = q
                .bind(ev.id)
                .bind(ev.inbox_item_id)
                .bind(ev.relative_file_path)
                .bind(ev.frame_type)
                .bind(ev.evidence_source)
                .bind(ev.raw_value)
                .bind(i64::from(ev.unclassified))
                .bind(ev.manual_override)
                .bind(i64::from(ev.is_master))
                .bind(ev.master_detector);
        }
        q.execute(&mut *conn).await?;
    }
    Ok(())
}

/// Connection-level variant of `delete_evidence_for_item`. See `insert_plan_conn` in
/// `plans.rs` for the pattern.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn delete_evidence_for_item_conn(
    conn: &mut SqliteConnection,
    inbox_item_id: &str,
) -> DbResult<()> {
    sqlx::query("DELETE FROM inbox_classification_evidence WHERE inbox_item_id = ?")
        .bind(inbox_item_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Fetch all evidence rows for an item.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn list_evidence(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> DbResult<Vec<InboxEvidenceRow>> {
    // Join inbox_file_overrides to recover the non-type override values
    // (filter/exposureS/binning/target) that live outside the evidence table
    // (migration 0048, plus `target` for #1294). The source_group_id is
    // looked up from inbox_items. Separate LEFT JOINs are used (one per
    // property_key) so that each value is available as a distinct column in
    // the result row, which sqlx::FromRow maps to the named struct fields.
    Ok(sqlx::query_as::<_, InboxEvidenceRow>(
        "SELECT
             ice.id,
             ice.inbox_item_id,
             ice.relative_file_path,
             ice.frame_type,
             ice.evidence_source,
             ice.raw_value,
             ice.unclassified,
             ice.manual_override,
             ice.is_master,
             ov_filter.value   AS override_filter,
             CAST(ov_exp.value AS REAL) AS override_exposure_s,
             ov_bin.value      AS override_binning,
             ov_ft.value       AS override_frame_type,
             ov_target.value   AS override_target,
             ice.override_stale
         FROM inbox_classification_evidence ice
         LEFT JOIN inbox_items ii
             ON ii.id = ice.inbox_item_id
         LEFT JOIN inbox_file_overrides ov_filter
             ON ov_filter.source_group_id = ii.source_group_id
            AND ov_filter.relative_file_path = ice.relative_file_path
            AND ov_filter.property_key = 'filter'
         LEFT JOIN inbox_file_overrides ov_exp
             ON ov_exp.source_group_id = ii.source_group_id
            AND ov_exp.relative_file_path = ice.relative_file_path
            AND ov_exp.property_key = 'exposureS'
         LEFT JOIN inbox_file_overrides ov_bin
             ON ov_bin.source_group_id = ii.source_group_id
            AND ov_bin.relative_file_path = ice.relative_file_path
            AND ov_bin.property_key = 'binning'
         LEFT JOIN inbox_file_overrides ov_ft
             ON ov_ft.source_group_id = ii.source_group_id
            AND ov_ft.relative_file_path = ice.relative_file_path
            AND ov_ft.property_key = 'frameType'
         LEFT JOIN inbox_file_overrides ov_target
             ON ov_target.source_group_id = ii.source_group_id
            AND ov_target.relative_file_path = ice.relative_file_path
            AND ov_target.property_key = 'target'
         WHERE ice.inbox_item_id = ?
         ORDER BY ice.relative_file_path",
    )
    .bind(inbox_item_id)
    .fetch_all(pool)
    .await?)
}

/// Apply a full set of non-type overrides (filter, exposure, binning) and
/// optionally a frame-type override.
///
/// After migration 0048 the non-type overrides (filter/exposure_s/binning) are
/// stored in `inbox_file_overrides` keyed by `(source_group_id,
/// relative_file_path, property_key)`. This function:
///
///   1. Looks up `source_group_id` from `inbox_items` for the given item.
///   2. Upserts each non-None non-type value into `inbox_file_overrides`.
///   3. Updates `manual_override` on the evidence row (frame-type correction).
///   4. Resets `override_stale = 0` on the evidence row.
///
/// `source_group_id` may be NULL for pre-0048 migrated items; in that case the
/// non-type overrides are silently skipped (only frame-type is written). This
/// is safe because: (a) legacy `plan_open` items cannot be reclassified until
/// their plan resolves; (b) on next classify after plan close, classify creates
/// a proper source group and overrides can be re-applied via the UI.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn set_overrides(
    pool: &SqlitePool,
    inbox_item_id: &str,
    relative_file_path: &str,
    frame_type: Option<&str>,
    filter: Option<&str>,
    exposure_s: Option<f64>,
    binning: Option<&str>,
) -> DbResult<bool> {
    use uuid::Uuid;

    // Step 1: update manual_override + reset override_stale on the evidence row.
    let rows = sqlx::query(
        "UPDATE inbox_classification_evidence
         SET manual_override = COALESCE(?, manual_override),
             override_stale  = 0,
             evidence_source = 'manual_override'
         WHERE inbox_item_id = ? AND relative_file_path = ?",
    )
    .bind(frame_type)
    .bind(inbox_item_id)
    .bind(relative_file_path)
    .execute(pool)
    .await?
    .rows_affected();

    // Step 2: look up source_group_id from inbox_items.
    // If the item has no source_group_id yet (e.g. freshly-inserted items that
    // predate migration 0048, or items created by tests without an explicit
    // source group), create a minimal source group on-the-fly and link the item
    // to it. This ensures non-type overrides can always be persisted.
    let source_group_id: String = {
        // Read item row: source_group_id, root_id, relative_path, discovered_at.
        let row: Option<(Option<String>, String, String, String)> = sqlx::query_as(
            "SELECT source_group_id, root_id, relative_path, discovered_at \
             FROM inbox_items WHERE id = ?",
        )
        .bind(inbox_item_id)
        .fetch_optional(pool)
        .await?;

        match row {
            Some((Some(sg_id), _, _, _)) => sg_id,
            Some((None, root_id, relative_path, discovered_at)) => {
                // Auto-create a source group for this item and link it.
                let new_sg_id = format!("sg-auto-{inbox_item_id}");
                let now = time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
                sqlx::query(
                    "INSERT OR IGNORE INTO inbox_source_groups \
                     (id, root_id, relative_path, discovered_at, last_scanned_at, child_count) \
                     VALUES (?, ?, ?, ?, ?, 1)",
                )
                .bind(&new_sg_id)
                .bind(&root_id)
                .bind(&relative_path)
                .bind(&discovered_at)
                .bind(&now)
                .execute(pool)
                .await?;

                sqlx::query("UPDATE inbox_items SET source_group_id = ? WHERE id = ?")
                    .bind(&new_sg_id)
                    .bind(inbox_item_id)
                    .execute(pool)
                    .await?;

                new_sg_id
            }
            // Item not found — rows_affected will be 0; return a placeholder that
            // won't match any source group so the overrides are silently skipped.
            None => return Ok(rows > 0),
        }
    };

    // Step 3: upsert non-type overrides into inbox_file_overrides.
    let sg_id = &source_group_id;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());

    // Helper: upsert a single property_key/value pair.
    // Uses INSERT OR REPLACE so subsequent set_overrides calls overwrite.
    let upsert_override = |key: &'static str, val: String| {
        let id = Uuid::new_v4().to_string();
        let sg = sg_id.clone();
        let rfp = relative_file_path.to_owned();
        let ts = now.clone();
        async move {
            sqlx::query(
                "INSERT INTO inbox_file_overrides \
                 (id, source_group_id, relative_file_path, property_key, value, \
                  override_stale, set_at) \
                 VALUES (?, ?, ?, ?, ?, 0, ?) \
                 ON CONFLICT(source_group_id, relative_file_path, property_key) \
                 DO UPDATE SET value = excluded.value, \
                               override_stale = 0, \
                               set_at = excluded.set_at",
            )
            .bind(id)
            .bind(sg)
            .bind(rfp)
            .bind(key)
            .bind(val)
            .bind(ts)
            .execute(pool)
            .await
        }
    };

    if let Some(f) = filter {
        upsert_override("filter", f.to_owned()).await?;
    }
    if let Some(e) = exposure_s {
        upsert_override("exposureS", e.to_string()).await?;
    }
    if let Some(b) = binning {
        upsert_override("binning", b.to_owned()).await?;
    }

    Ok(rows > 0)
}

/// Write (upsert) a single arbitrary property override for one file in a source
/// group (spec 041 T068 / R-13 generic override write).
///
/// Keyed on `(source_group_id, relative_file_path, property_key)` — the same
/// UNIQUE constraint already present on `inbox_file_overrides`. Subsequent calls
/// for the same key overwrite the previous value and reset `override_stale = 0`.
///
/// `file_size_bytes` and `file_mtime` are the cheap per-file identity used for
/// override staleness detection (R-4). Both may be `None` when the caller cannot
/// stat the file (e.g. pure-index metadata corrections where no path is
/// available).
///
/// This is the generic successor to the fixed-field
/// `filter`/`exposureS`/`binning` path in [`set_overrides`] and accepts any
/// registry-validated property key including those not yet backed by dedicated
/// evidence columns.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn set_file_override(
    pool: &SqlitePool,
    source_group_id: &str,
    relative_file_path: &str,
    property_key: &str,
    value: &str,
    file_size_bytes: Option<i64>,
    file_mtime: Option<&str>,
) -> DbResult<()> {
    use uuid::Uuid;
    let id = Uuid::new_v4().to_string();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
    sqlx::query(
        "INSERT INTO inbox_file_overrides
             (id, source_group_id, relative_file_path, property_key, value,
              file_size_bytes, file_mtime, override_stale, set_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)
         ON CONFLICT(source_group_id, relative_file_path, property_key)
         DO UPDATE SET
             value             = excluded.value,
             file_size_bytes   = COALESCE(excluded.file_size_bytes, file_size_bytes),
             file_mtime        = COALESCE(excluded.file_mtime,        file_mtime),
             override_stale    = 0,
             set_at            = excluded.set_at",
    )
    .bind(&id)
    .bind(source_group_id)
    .bind(relative_file_path)
    .bind(property_key)
    .bind(value)
    .bind(file_size_bytes)
    .bind(file_mtime)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// One row from `inbox_file_overrides` for a source group — used by the
/// field-agnostic reclassifier (T068) to read back all overrides for a group
/// after applying them, so it can feed them into the re-split grouping engine.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct FileOverrideRow {
    pub relative_file_path: String,
    pub property_key: String,
    pub value: String,
    pub override_stale: i64,
    /// File identity recorded when the override was set (spec 041 FR-046,
    /// R-4 staleness detection). `None` for overrides written before this
    /// field was wired, or by a caller with no accessible file stat.
    pub file_size_bytes: Option<i64>,
    pub file_mtime: Option<String>,
}

/// Fetch all property overrides for every file in a source group.
///
/// Returns one row per `(relative_file_path, property_key)` — the full override
/// map the re-split grouping engine needs to compute updated group keys after a
/// reclassify call (T068 R-13 re-split). Includes stale rows — `override_stale`
/// is informational (surfaced to the user), not a filter: a stale override is
/// still the user's most recent explicit decision and stays in effect until
/// they act on the staleness signal.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn list_file_overrides_for_group(
    pool: &SqlitePool,
    source_group_id: &str,
) -> DbResult<Vec<FileOverrideRow>> {
    Ok(sqlx::query_as::<_, FileOverrideRow>(
        "SELECT relative_file_path, property_key, value, override_stale,
                file_size_bytes, file_mtime
         FROM inbox_file_overrides
         WHERE source_group_id = ?
         ORDER BY relative_file_path, property_key",
    )
    .bind(source_group_id)
    .fetch_all(pool)
    .await?)
}

/// Data for one row in a batch upsert of `inbox_classifications`.
pub struct UpsertClassificationRow<'a> {
    pub inbox_item_id: &'a str,
    pub result: &'a str,
    pub frame_type: Option<&'a str>,
    pub content_signature: &'a str,
    pub unclassified_file_count: i64,
}

/// Batch-upsert multiple classification rows in one statement per chunk.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn upsert_classification_batch(
    conn: &mut SqliteConnection,
    rows: &[UpsertClassificationRow<'_>],
) -> DbResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let now = Timestamp::now_iso();
    for chunk in rows.chunks(32766 / 6) {
        let placeholders = (0..chunk.len()).map(|_| "(?,?,?,?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT INTO inbox_classifications
                (inbox_item_id, result, frame_type, computed_at, content_signature,
                 unclassified_file_count)
             VALUES {placeholders}
             ON CONFLICT(inbox_item_id) DO UPDATE SET
                 result = excluded.result,
                 frame_type = excluded.frame_type,
                 computed_at = excluded.computed_at,
                 content_signature = excluded.content_signature,
                 unclassified_file_count = excluded.unclassified_file_count"
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for r in chunk {
            q = q
                .bind(r.inbox_item_id)
                .bind(r.result)
                .bind(r.frame_type)
                .bind(&now)
                .bind(r.content_signature)
                .bind(r.unclassified_file_count);
        }
        q.execute(&mut *conn).await?;
    }
    Ok(())
}

/// Batch-delete evidence rows for a set of item ids.
///
/// Uses `WHERE inbox_item_id IN (?, ?, …)`. Chunks at 32766 to stay within
/// SQLite's parameter limit.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn delete_evidence_for_items(
    conn: &mut SqliteConnection,
    inbox_item_ids: &[&str],
) -> DbResult<()> {
    for chunk in inbox_item_ids.chunks(32766) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "DELETE FROM inbox_classification_evidence WHERE inbox_item_id IN ({placeholders})"
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for id in chunk {
            q = q.bind(*id);
        }
        q.execute(&mut *conn).await?;
    }
    Ok(())
}

/// Mark the override for a file as stale (file size/mtime changed since the
/// override was recorded — spec 041 R-4).
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn mark_override_stale(
    pool: &SqlitePool,
    inbox_item_id: &str,
    relative_file_path: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE inbox_classification_evidence
         SET override_stale = 1
         WHERE inbox_item_id = ? AND relative_file_path = ?",
    )
    .bind(inbox_item_id)
    .bind(relative_file_path)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark every generic property override for one file in a source group as
/// stale (spec 041 FR-046, R-4 — the `inbox_file_overrides` counterpart to
/// [`mark_override_stale`]'s evidence-table check).
///
/// All property rows for the path share one file identity, so this flags
/// every `(source_group_id, relative_file_path, *)` row at once rather than
/// taking a single `property_key`.
///
/// # Errors
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn mark_file_override_stale(
    pool: &SqlitePool,
    source_group_id: &str,
    relative_file_path: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE inbox_file_overrides
         SET override_stale = 1
         WHERE source_group_id = ? AND relative_file_path = ?",
    )
    .bind(source_group_id)
    .bind(relative_file_path)
    .execute(pool)
    .await?;
    Ok(())
}
