// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Inventory projection repository (spec 006).
//!
//! Produces `InventorySource[]` by joining `library_root`,
//! `acquisition_session`, and `calibration_session` tables.  Filters are
//! applied server-side so the wire payload is small.
//!
//! No new tables are introduced — inventory is a read-only projection.
//!
//! Spec 041 FR-051 (T076, Phase 13): sessions no longer carry a review-state
//! column — they are derived, already-confirmed inventory. The `state`
//! projection column and `review_state` filter were removed.

use sqlx::SqlitePool;

use persistence_core::DbResult;

// ── Domain Row types returned by the projection query ─────────────────────────

/// A library root row from the projection.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LibraryRootRow {
    pub id: String,
    pub current_path: String,
    pub kind: String,
    pub state: String,
}

/// A session row (acquisition or calibration) from the projection.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionProjectionRow {
    pub id: String,
    pub session_key: String,
    pub root_id: String,
    /// "acquisition" or "calibration"
    pub session_kind: String,
    /// Frame type string: "light", "dark", "flat", "bias", or derived "mixed".
    pub frame_type: String,
    /// Pre-aggregated frame count from `json_array_length(frame_ids)`.
    pub frame_count: i64,
    /// First element of the `frame_ids` JSON array (`json_extract(...,'$[0]')`),
    /// `None` when the session has no frames.
    pub first_frame_id: Option<String>,
    /// Target id (acquisition sessions only; NULL for calibration).
    pub target_id: Option<String>,
    /// Target primary designation when linked.
    pub target_name: Option<String>,
    pub created_at: String,
    /// User-editable free-text notes (#773). `NULL` when never set.
    pub notes: Option<String>,
}

/// A project reference row for the `linked.projects` lookup.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProjectLinkRow {
    pub project_id: String,
    pub project_name: String,
    pub session_id: String,
}

/// Filters accepted by the inventory projection.
#[derive(Debug, Clone, Default)]
pub struct InventoryFilters {
    /// When `Some`, restrict to a single `LibraryRoot`.
    pub source_id: Option<String>,
    /// When `Some`, restrict to sessions with the given frame type.
    /// `"mixed"` matches heterogeneous sessions.
    pub frame_type: Option<String>,
    /// Cap on sessions returned per source root. `None` means no cap.
    pub limit: Option<u32>,
    /// Number of sessions to skip (per-source, applied after type filter).
    pub offset: Option<u32>,
}

/// List all `LibraryRoot` rows that have at least one session under them.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_roots_with_sessions(pool: &SqlitePool) -> DbResult<Vec<LibraryRootRow>> {
    let rows = sqlx::query_as::<_, LibraryRootRow>(
        r"
        SELECT DISTINCT lr.id, lr.current_path, lr.kind, lr.state
        FROM library_root lr
        WHERE EXISTS (
            SELECT 1 FROM acquisition_session acs
            WHERE acs.root_id = lr.id
        ) OR EXISTS (
            SELECT 1 FROM calibration_session cs
            WHERE cs.root_id = lr.id
        )
        ORDER BY lr.current_path ASC
        ",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List ALL `LibraryRoot` rows (regardless of session presence).
///
/// Used by spec 011 cwd-containment check (R-CwdContain, FR-010).
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_all_roots(pool: &SqlitePool) -> DbResult<Vec<LibraryRootRow>> {
    let rows = sqlx::query_as::<_, LibraryRootRow>(
        "SELECT id, current_path, kind, state FROM library_root ORDER BY current_path ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List sessions under a given `LibraryRoot` with optional filters applied.
///
/// Returns `(rows, has_more)` where `has_more` is `true` when more rows exist
/// beyond the requested page. The sentinel is derived from fetching
/// `limit + 1` rows and trimming the extra — avoids a separate COUNT query.
///
/// LIMIT and OFFSET are pushed into SQL so the database never reads rows
/// beyond the page boundary. Each session type (acquisition, calibration)
/// receives the same limit independently; callers must be aware that a page
/// of N may contain up to N acquisitions and N calibration rows when no
/// frame-type filter is set.
///
/// `target_name` is always `None` — gen-1 `target` table is unreferenced
/// (spec 036, T007); the gen-3 `canonical_target` is the live store.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_sessions_for_root(
    pool: &SqlitePool,
    root_id: &str,
    filters: &InventoryFilters,
) -> DbResult<(Vec<SessionProjectionRow>, bool)> {
    let frame_filter = filters.frame_type.as_deref();
    let offset = i64::from(filters.offset.unwrap_or(0));

    // Fetch limit+1 rows to detect whether more exist beyond this page without
    // an extra COUNT query. `None` limit means unbounded (no sentinel row).
    let (sql_limit, sentinel) = match filters.limit {
        Some(n) => (i64::from(n) + 1, true),
        None => (-1_i64, false), // SQLite: LIMIT -1 = no limit
    };

    // Acquisition sessions — `frame_ids` aggregated in SQL to avoid reading
    // the full JSON blob in Rust; only count and first element are needed.
    let acq_rows: Vec<SessionProjectionRow> = sqlx::query_as(
        r"
        SELECT
            acs.id                                      AS id,
            acs.session_key                             AS session_key,
            acs.root_id                                 AS root_id,
            'acquisition'                               AS session_kind,
            'light'                                     AS frame_type,
            json_array_length(acs.frame_ids)            AS frame_count,
            json_extract(acs.frame_ids, '$[0]')         AS first_frame_id,
            acs.target_id                               AS target_id,
            NULL                                        AS target_name,
            acs.created_at                              AS created_at,
            acs.notes                                   AS notes
        FROM acquisition_session acs
        WHERE acs.root_id = ?
        ORDER BY acs.created_at DESC
        LIMIT ? OFFSET ?
        ",
    )
    .bind(root_id)
    .bind(sql_limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    // Calibration sessions — same shape; LIMIT/OFFSET bound independently.
    let cal_rows: Vec<SessionProjectionRow> = sqlx::query_as(
        r"
        SELECT
            cs.id                                       AS id,
            cs.session_key                              AS session_key,
            cs.root_id                                  AS root_id,
            'calibration'                               AS session_kind,
            cs.kind                                     AS frame_type,
            json_array_length(cs.frame_ids)             AS frame_count,
            json_extract(cs.frame_ids, '$[0]')          AS first_frame_id,
            NULL                                        AS target_id,
            NULL                                        AS target_name,
            cs.created_at                               AS created_at,
            cs.notes                                    AS notes
        FROM calibration_session cs
        WHERE cs.root_id = ?
        ORDER BY cs.created_at DESC
        LIMIT ? OFFSET ?
        ",
    )
    .bind(root_id)
    .bind(sql_limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    // Detect and trim the sentinel per-type BEFORE combining.  Doing it after
    // chain() is incorrect: if both types overflow, pop() removes only one
    // sentinel and the other leaks into the payload; if only acq overflows,
    // pop() removes the last real cal row and the acq sentinel leaks.
    let limit_usize = filters.limit.unwrap_or(0) as usize;
    let (acq_rows, acq_has_more) = if sentinel && acq_rows.len() > limit_usize {
        let mut v = acq_rows;
        v.pop();
        (v, true)
    } else {
        (acq_rows, false)
    };
    let (cal_rows, cal_has_more) = if sentinel && cal_rows.len() > limit_usize {
        let mut v = cal_rows;
        v.pop();
        (v, true)
    } else {
        (cal_rows, false)
    };
    let has_more = acq_has_more || cal_has_more;

    // Apply post-fetch frame_type filter — cannot be trivially done in a
    // single UNION query with dynamic placeholders.
    let type_filtered: Vec<SessionProjectionRow> = acq_rows
        .into_iter()
        .chain(cal_rows)
        .filter(|row| frame_filter.is_none_or(|ff| row.frame_type == ff))
        .collect();

    Ok((type_filtered, has_more))
}

/// A project row used only for session-link lookup.
#[derive(Debug, sqlx::FromRow)]
struct ProjectSourcesRow {
    pub id: String,
    pub name: String,
    pub inventory_session_id: String,
}

/// Load project links for a set of session IDs.
///
/// Returns rows where each `ProjectLinkRow.session_id` is the session being
/// linked and `.project_id` / `.project_name` identify the project.  The
/// caller maps these into `InventoryLinkedRefs.projects`.
///
/// Implementation: fetches all `project_sources` rows whose
/// `inventory_session_id` is in the supplied set.  This is safe for the
/// expected cardinality (a few hundred sessions per root at most).
///
/// # Errors
/// Returns [`crate::DbResult`] on query failure.
pub async fn list_project_links_for_sessions(
    pool: &SqlitePool,
    session_ids: &[String],
) -> DbResult<Vec<ProjectLinkRow>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch all project sources in one pass, then filter in Rust.
    // Avoids dynamic SQL injection and is safe for small session sets.
    let all_sources: Vec<ProjectSourcesRow> = sqlx::query_as(
        r"
        SELECT ps.inventory_session_id, p.id, p.name
        FROM project_sources ps
        JOIN projects p ON p.id = ps.project_id
        ORDER BY p.name ASC
        ",
    )
    .fetch_all(pool)
    .await?;

    let id_set: std::collections::HashSet<&String> = session_ids.iter().collect();

    let rows = all_sources
        .into_iter()
        .filter(|r| id_set.contains(&r.inventory_session_id))
        .map(|r| ProjectLinkRow {
            project_id: r.id,
            project_name: r.name,
            session_id: r.inventory_session_id,
        })
        .collect();

    Ok(rows)
}

/// Session context enrichment row (spec P9): resolved target/filter/night/
/// frame-count for a light (`acquisition_session`) row, keyed by session id.
///
/// `target_name` prefers the user-owned `display_alias` over
/// `primary_designation` (same effective-label rule as the Targets surface).
/// `filter` and `acquisition_night` come from `acquisition_fingerprint`
/// (absent until the metadata extraction pipeline populates a fingerprint
/// row). `frame_count` is derived from `json_array_length(frame_ids)` and is
/// always present for a matched row (the column is `NOT NULL DEFAULT '[]'`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionContextRow {
    pub id: String,
    pub target_name: Option<String>,
    pub filter: Option<String>,
    pub acquisition_night: Option<String>,
    pub frame_count: i64,
}

/// Batch-load session context for calibration match-suggest enrichment
/// (spec P9). One query for the whole `session_ids` set — callers MUST NOT
/// call this per-row (N+1).
///
/// Only `acquisition_session` (light) rows are covered; calibration sessions
/// have no target/filter/night context to enrich. Ids with no matching row
/// (unknown session, or a calibration session id) are simply absent from the
/// returned `Vec` — callers treat a missing id as "no context available".
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn get_session_context_by_ids(
    pool: &SqlitePool,
    session_ids: &[String],
) -> DbResult<Vec<SessionContextRow>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = vec!["?"; session_ids.len()].join(",");
    let sql = format!(
        "SELECT
             acs.id                                              AS id,
             COALESCE(ct.display_alias, ct.primary_designation)  AS target_name,
             af.filter_name                                      AS filter,
             af.observing_night_date                             AS acquisition_night,
             json_array_length(acs.frame_ids)                    AS frame_count
         FROM acquisition_session acs
         LEFT JOIN canonical_target ct ON ct.id = acs.canonical_target_id
         LEFT JOIN acquisition_fingerprint af ON af.id = acs.id
         WHERE acs.id IN ({placeholders})"
    );

    // SQL is built only from a fixed `?` placeholder count (no user strings
    // in the text); every id flows through `bind`. Same pattern as the
    // dynamic `IN (?, …)` lists in `inbox.rs` / `lifecycle.rs`.
    let mut q = sqlx::query_as::<_, SessionContextRow>(sqlx::AssertSqlSafe(sql));
    for id in session_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

/// One `(session, camera)` pairing with the number of the session's active
/// frames that carry that camera string, as written by the capture program
/// into the file header (`INSTRUME`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionCameraRow {
    pub session_id: String,
    /// Raw header string, never blank — callers resolve it to a registered
    /// camera's name and fall back to this value.
    pub camera: String,
    pub frame_count: i64,
}

/// Batch-load the camera recorded against each session's frames (#1343), for
/// the whole `session_ids` set in one query — callers MUST NOT call this
/// per-session (N+1).
///
/// Reads `inbox_file_metadata.instrume` rather than
/// `acquisition_fingerprint.optic_train`: the fingerprint tables have no
/// production writer for that column, so a fingerprint-sourced camera would
/// be `NULL` for every real session.
///
/// Both session kinds are covered — a calibration session names a camera just
/// as a light session does. Rows are ordered so the caller can take the first
/// row per session as the winner: a session is normally homogeneous by
/// camera, and where it is not, the most-frequent string wins with a
/// name-ascending tiebreak so the choice is stable across runs.
///
/// Sessions whose frames resolve no metadata row are simply absent.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_session_cameras(
    pool: &SqlitePool,
    session_ids: &[String],
) -> DbResult<Vec<SessionCameraRow>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = vec!["?"; session_ids.len()].join(",");
    // The inner GROUP BY collapses to one row per frame before counting, so a
    // root carrying several inbox groups cannot fan a single frame out into
    // duplicate votes — same guard as `q_core::active_frame_exposure_seconds`.
    let sql = format!(
        "SELECT per.session_id AS session_id, per.camera AS camera, COUNT(*) AS frame_count
         FROM (
             SELECT s.id AS session_id, fr.id AS frame_id, MAX(ifm.instrume) AS camera
             FROM (
                 SELECT id, frame_ids FROM acquisition_session WHERE id IN ({placeholders})
                 UNION ALL
                 SELECT id, frame_ids FROM calibration_session WHERE id IN ({placeholders})
             ) s
             JOIN json_each(s.frame_ids) je
             JOIN file_record fr ON fr.id = je.value AND fr.state != 'missing'
             LEFT JOIN inbox_items ii ON ii.root_id = fr.root_id
             LEFT JOIN inbox_file_metadata ifm
                 ON ifm.inbox_item_id = ii.id AND ifm.relative_file_path = fr.relative_path
             GROUP BY s.id, fr.id
         ) per
         WHERE TRIM(COALESCE(per.camera, '')) != ''
         GROUP BY per.session_id, per.camera
         ORDER BY per.session_id ASC, frame_count DESC, per.camera ASC"
    );

    // SQL is built only from a fixed `?` placeholder count (no user strings in
    // the text); every id flows through `bind`. Same pattern as
    // `get_session_context_by_ids`. The id list is bound twice — once per
    // branch of the UNION.
    let mut q = sqlx::query_as::<_, SessionCameraRow>(sqlx::AssertSqlSafe(sql));
    for id in session_ids.iter().chain(session_ids) {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

/// Write `notes` to whichever session table owns `session_id` — an
/// inventory session id is always exactly one of `acquisition_session` or
/// `calibration_session` (spec 006 union), so this tries the acquisition
/// table first and falls back to calibration only when it matched nothing.
/// Stores `NULL` when `notes` is `None` (clear). Mirrors
/// `targets::set_target_notes`'s single-column-write shape.
///
/// Returns `true` when a row was updated in either table, `false` when
/// `session_id` matches neither (caller maps that to `session.not_found`).
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn set_session_notes(
    pool: &SqlitePool,
    session_id: &str,
    notes: Option<&str>,
) -> DbResult<bool> {
    let acq = sqlx::query("UPDATE acquisition_session SET notes = ? WHERE id = ?")
        .bind(notes)
        .bind(session_id)
        .execute(pool)
        .await?;
    if acq.rows_affected() > 0 {
        return Ok(true);
    }
    let cal = sqlx::query("UPDATE calibration_session SET notes = ? WHERE id = ?")
        .bind(notes)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(cal.rows_affected() > 0)
}

/// A `calibration_assignment` row enriched with its owning session id, for
/// batch-loading calibration linkage across every session in a root (#772).
/// Mirrors `list_project_links_for_sessions`'s batch-then-filter shape —
/// one query for the whole `session_ids` set, never N+1 per session.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionCalibrationLinkRow {
    pub session_id: String,
    pub master_id: String,
    pub calibration_type: String,
    pub confidence: f64,
    pub mismatched_dimensions: String,
    /// #718 (spec 007 SC-003): persisted so a reopened session detail can
    /// still distinguish an override assignment from a normal match.
    pub was_override: bool,
}

/// Load calibration assignments for a set of session IDs in one query.
///
/// Only acquisition (light) sessions ever have rows here — `calibration_
/// assignment` links a light session to the dark/flat/bias masters that
/// calibrate it, never the reverse — so calibration session ids in the
/// input simply produce no rows.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn list_calibration_matches_for_sessions(
    pool: &SqlitePool,
    session_ids: &[String],
) -> DbResult<Vec<SessionCalibrationLinkRow>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = vec!["?"; session_ids.len()].join(",");
    let sql = format!(
        "SELECT session_id, master_id, calibration_type, confidence, mismatched_dimensions, was_override
         FROM calibration_assignment
         WHERE session_id IN ({placeholders})"
    );

    // Same fixed-placeholder-count pattern as `get_session_context_by_ids`
    // above — no user string embedded in the SQL text, every id is bound.
    let mut q = sqlx::query_as::<_, SessionCalibrationLinkRow>(sqlx::AssertSqlSafe(sql));
    for id in session_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

/// Look up the absolute `current_path` of a `library_root` row (T023a).
///
/// Returns `Some(path_string)` when found, `None` when not found.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn get_library_root_path(pool: &SqlitePool, root_id: &str) -> DbResult<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT current_path FROM library_root WHERE id = ?")
            .bind(root_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(p,)| p))
}

/// Look up the `state` of a `library_root` row.
///
/// Returns `Some(state)` when found, `None` when not found.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn get_library_root_state(pool: &SqlitePool, root_id: &str) -> DbResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT state FROM library_root WHERE id = ?")
        .bind(root_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(s,)| s))
}

/// Minimal `file_record` fields needed to resolve a canonical source's
/// current path + presence for verification (spec 049 US4).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FileRecordLookupRow {
    pub root_id: String,
    pub relative_path: String,
    pub state: String,
}

/// Look up a `file_record` row by id (spec 049 `sourceview.verify`'s
/// read-only source-resolution step). Returns `None` when no row exists.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn get_file_record_lookup(
    pool: &SqlitePool,
    file_record_id: &str,
) -> DbResult<Option<FileRecordLookupRow>> {
    let row = sqlx::query_as::<_, FileRecordLookupRow>(
        "SELECT root_id, relative_path, state FROM file_record WHERE id = ?",
    )
    .bind(file_record_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::setup_db;

    #[tokio::test]
    async fn list_roots_with_sessions_returns_empty_initially() {
        let db = setup_db().await;
        let roots = list_roots_with_sessions(db.pool()).await.unwrap();
        assert!(roots.is_empty(), "expected no roots with sessions on a fresh db");
    }

    #[tokio::test]
    async fn list_sessions_for_root_returns_empty_on_unknown_root() {
        let db = setup_db().await;
        let filters = InventoryFilters::default();
        let (sessions, has_more) =
            list_sessions_for_root(db.pool(), "00000000-0000-0000-0000-000000000000", &filters)
                .await
                .unwrap();
        assert!(sessions.is_empty());
        assert!(!has_more);
    }

    #[tokio::test]
    async fn project_links_empty_for_empty_session_ids() {
        let db = setup_db().await;
        let links = list_project_links_for_sessions(db.pool(), &[]).await.unwrap();
        assert!(links.is_empty());
    }

    #[tokio::test]
    async fn project_links_for_sessions_returns_matching_rows_filtered_by_id_set() {
        let db = setup_db().await;

        sqlx::query(
            "INSERT INTO projects (id, name, tool, path, created_at, updated_at) VALUES \
                ('proj-a', 'Andromeda Widefield', 'PixInsight', 'proj-a', \
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'), \
                ('proj-b', 'Bubble Nebula', 'PixInsight', 'proj-b', \
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO project_sources (id, project_id, inventory_session_id, linked_at) VALUES \
                ('ps-1', 'proj-a', 'acq-link-1', '2026-01-02T00:00:00Z'), \
                ('ps-2', 'proj-b', 'acq-link-2', '2026-01-02T00:00:00Z'), \
                ('ps-3', 'proj-a', 'acq-link-3', '2026-01-02T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        // acq-link-3 is deliberately excluded from the query set, and
        // missing-id has no project_sources row at all — both must be
        // absent from the result, proving the join is filtered by the
        // caller-supplied id set rather than returning every linked row.
        let ids = vec!["acq-link-1".to_owned(), "acq-link-2".to_owned(), "missing-id".to_owned()];
        let links = list_project_links_for_sessions(db.pool(), &ids).await.unwrap();

        assert_eq!(links.len(), 2, "acq-link-3 and missing-id must not appear");
        // Query orders by p.name ASC: "Andromeda Widefield" < "Bubble Nebula".
        assert_eq!(links[0].project_id, "proj-a");
        assert_eq!(links[0].project_name, "Andromeda Widefield");
        assert_eq!(links[0].session_id, "acq-link-1");
        assert_eq!(links[1].project_id, "proj-b");
        assert_eq!(links[1].project_name, "Bubble Nebula");
        assert_eq!(links[1].session_id, "acq-link-2");
    }

    #[tokio::test]
    async fn get_library_root_state_returns_none_for_unknown() {
        let db = setup_db().await;
        let result = get_library_root_state(db.pool(), "00000000-0000-0000-0000-000000000000")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    // ── get_session_context_by_ids (spec P9) ──────────────────────────────────

    #[tokio::test]
    async fn session_context_empty_ids_returns_empty_without_querying() {
        let db = setup_db().await;
        let rows = get_session_context_by_ids(db.pool(), &[]).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn session_context_batches_multiple_ids_in_one_call() {
        let db = setup_db().await;

        sqlx::query(
            "INSERT INTO canonical_target
                (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at, display_alias)
             VALUES ('t-1', NULL, 'M 31', 'galaxy', 10.68, 41.27, 'seed', '2026-01-01T00:00:00Z', 'Andromeda')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at, canonical_target_id) \
             VALUES ('acq-1', 'M31/L/2026-03-01', '[\"f1\",\"f2\",\"f3\"]', '2026-03-01T00:00:00Z', 't-1')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO acquisition_fingerprint (id, filter_name, observing_night_date) \
             VALUES ('acq-1', 'Ha', '2026-03-01')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        // A second session with no fingerprint and no canonical target link —
        // context fields resolve to None, but frame_count is still derived.
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
             VALUES ('acq-2', 'unknown/L/2026-03-02', '[\"f4\"]', '2026-03-02T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let ids = vec!["acq-1".to_owned(), "acq-2".to_owned(), "missing-id".to_owned()];
        let mut rows = get_session_context_by_ids(db.pool(), &ids).await.unwrap();
        rows.sort_by(|a, b| a.id.cmp(&b.id));

        // Unknown session ids are simply absent — no error, no phantom row.
        assert_eq!(rows.len(), 2, "missing-id must not produce a row");

        assert_eq!(rows[0].id, "acq-1");
        assert_eq!(rows[0].target_name.as_deref(), Some("Andromeda"), "display_alias wins");
        assert_eq!(rows[0].filter.as_deref(), Some("Ha"));
        assert_eq!(rows[0].acquisition_night.as_deref(), Some("2026-03-01"));
        assert_eq!(rows[0].frame_count, 3);

        assert_eq!(rows[1].id, "acq-2");
        assert_eq!(rows[1].target_name, None);
        assert_eq!(rows[1].filter, None);
        assert_eq!(rows[1].acquisition_night, None);
        assert_eq!(rows[1].frame_count, 1);
    }

    #[tokio::test]
    async fn session_context_falls_back_to_primary_designation_without_display_alias() {
        let db = setup_db().await;

        sqlx::query(
            "INSERT INTO canonical_target
                (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at)
             VALUES ('t-2', NULL, 'NGC 7000', 'nebula', 20.0, 30.0, 'seed', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at, canonical_target_id) \
             VALUES ('acq-3', 'NGC7000/L/2026-04-01', '[]', '2026-04-01T00:00:00Z', 't-2')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let rows = get_session_context_by_ids(db.pool(), &["acq-3".to_owned()]).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].target_name.as_deref(), Some("NGC 7000"));
        assert_eq!(rows[0].frame_count, 0);
    }

    // ── set_session_notes (#773) ──────────────────────────────────────────────

    #[tokio::test]
    async fn set_session_notes_updates_acquisition_session() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
             VALUES ('acq-notes', 'k', '[]', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let updated =
            set_session_notes(db.pool(), "acq-notes", Some("Great seeing.")).await.unwrap();
        assert!(updated);

        let (notes,): (Option<String>,) =
            sqlx::query_as("SELECT notes FROM acquisition_session WHERE id = 'acq-notes'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(notes.as_deref(), Some("Great seeing."));
    }

    #[tokio::test]
    async fn set_session_notes_updates_calibration_session() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO calibration_session (id, session_key, frame_ids, kind, created_at) \
             VALUES ('cal-notes', 'k', '[]', 'dark', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let updated =
            set_session_notes(db.pool(), "cal-notes", Some("Fresh bias batch.")).await.unwrap();
        assert!(updated);

        let (notes,): (Option<String>,) =
            sqlx::query_as("SELECT notes FROM calibration_session WHERE id = 'cal-notes'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(notes.as_deref(), Some("Fresh bias batch."));
    }

    #[tokio::test]
    async fn set_session_notes_clears_with_none() {
        let db = setup_db().await;
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at, notes) \
             VALUES ('acq-clear', 'k', '[]', '2026-01-01T00:00:00Z', 'old note')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        set_session_notes(db.pool(), "acq-clear", None).await.unwrap();

        let (notes,): (Option<String>,) =
            sqlx::query_as("SELECT notes FROM acquisition_session WHERE id = 'acq-clear'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert!(notes.is_none());
    }

    #[tokio::test]
    async fn set_session_notes_unknown_id_updates_nothing() {
        let db = setup_db().await;
        let updated = set_session_notes(db.pool(), "no-such-session", Some("x")).await.unwrap();
        assert!(!updated);
    }

    // ── list_calibration_matches_for_sessions (#772) ──────────────────────────

    #[tokio::test]
    async fn calibration_matches_empty_for_empty_ids() {
        let db = setup_db().await;
        let rows = list_calibration_matches_for_sessions(db.pool(), &[]).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn calibration_matches_batches_multiple_sessions() {
        let db = setup_db().await;

        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
             VALUES ('acq-cal-1', 'k1', '[]', '2026-01-01T00:00:00Z'), \
                    ('acq-cal-2', 'k2', '[]', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO calibration_assignment \
                (id, session_id, calibration_type, master_id, confidence, mismatched_dimensions, assigned_at) \
             VALUES ('ca-1', 'acq-cal-1', 'dark', 'master-dark-1', 0.95, '[]', '2026-01-02T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let ids = vec!["acq-cal-1".to_owned(), "acq-cal-2".to_owned(), "missing".to_owned()];
        let rows = list_calibration_matches_for_sessions(db.pool(), &ids).await.unwrap();

        assert_eq!(rows.len(), 1, "only acq-cal-1 has an assignment");
        assert_eq!(rows[0].session_id, "acq-cal-1");
        assert_eq!(rows[0].master_id, "master-dark-1");
        assert_eq!(rows[0].calibration_type, "dark");
        assert!((rows[0].confidence - 0.95).abs() < f64::EPSILON);
    }

    // ── list_sessions_for_root — frame_count / first_frame_id aggregation ────

    /// Verifies that `json_array_length` and `json_extract('$[0]')` return the
    /// same values a Rust parse of the raw `frame_ids` column would produce.
    /// This is the parity assertion called for by the task brief.
    #[tokio::test]
    async fn list_sessions_frame_count_and_first_frame_id_match_raw_array() {
        let db = setup_db().await;

        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-fc', 'Lib', 'local', '/lib', 'active', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        // Three frames: frame_count must be 3, first_frame_id must be "f1".
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, root_id, frame_ids, created_at) \
             VALUES ('acq-fc', 'k', 'root-fc', '[\"f1\",\"f2\",\"f3\"]', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        // Empty array: frame_count 0, first_frame_id None.
        sqlx::query(
            "INSERT INTO calibration_session \
                (id, session_key, root_id, frame_ids, kind, created_at) \
             VALUES ('cal-fc', 'k', 'root-fc', '[]', 'dark', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let filters = InventoryFilters::default();
        let (rows, has_more) =
            list_sessions_for_root(db.pool(), "root-fc", &filters).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(!has_more, "unbounded fetch must not signal has_more");

        // Rows are ordered DESC by created_at — both share the same timestamp
        // so order is deterministic within type (acq before cal in UNION).
        let acq = rows.iter().find(|r| r.id == "acq-fc").expect("acq-fc missing");
        assert_eq!(acq.frame_count, 3, "json_array_length should count 3 elements");
        assert_eq!(
            acq.first_frame_id.as_deref(),
            Some("f1"),
            "json_extract should return first element"
        );

        let cal = rows.iter().find(|r| r.id == "cal-fc").expect("cal-fc missing");
        assert_eq!(cal.frame_count, 0, "empty array → 0");
        assert!(cal.first_frame_id.is_none(), "empty array → None");
    }

    /// Verifies offset/limit pagination bounds results per source root.
    #[tokio::test]
    async fn list_sessions_limit_and_offset_bound_results() {
        let db = setup_db().await;

        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-pg', 'Lib', 'local', '/lib', 'active', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        // Insert 4 sessions with distinct timestamps so order is stable.
        for i in 0..4u32 {
            sqlx::query(
                "INSERT INTO acquisition_session (id, session_key, root_id, frame_ids, created_at) \
                 VALUES (?, 'k', 'root-pg', '[]', ?)",
            )
            .bind(format!("acq-pg-{i}"))
            .bind(format!("2026-01-0{d}T00:00:00Z", d = i + 1))
            .execute(db.pool())
            .await
            .unwrap();
        }

        // No cap: all 4, has_more false.
        let (all, hm) = list_sessions_for_root(db.pool(), "root-pg", &InventoryFilters::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 4);
        assert!(!hm, "unbounded fetch must not signal has_more");

        // limit=2 with 4 rows → has_more true, returns 2.
        let (limited, hm) = list_sessions_for_root(
            db.pool(),
            "root-pg",
            &InventoryFilters { limit: Some(2), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(limited.len(), 2);
        assert!(hm, "4 rows with limit=2 must signal has_more");

        // limit=4 exactly fits → has_more false.
        let (exact, hm) = list_sessions_for_root(
            db.pool(),
            "root-pg",
            &InventoryFilters { limit: Some(4), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(exact.len(), 4);
        assert!(!hm, "limit equal to row count must not signal has_more");

        // offset=3, no limit: last 1 row, has_more false.
        let (paged, hm) = list_sessions_for_root(
            db.pool(),
            "root-pg",
            &InventoryFilters { offset: Some(3), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(paged.len(), 1);
        assert!(!hm);

        // offset past end: empty, has_more false.
        let (past, hm) = list_sessions_for_root(
            db.pool(),
            "root-pg",
            &InventoryFilters { offset: Some(10), ..Default::default() },
        )
        .await
        .unwrap();
        assert!(past.is_empty());
        assert!(!hm);
    }

    /// Case A: both acquisition and calibration types overflow their limit.
    /// The old single-pop was wrong here: it only removed one sentinel so
    /// the other leaked into the payload.
    #[tokio::test]
    async fn list_sessions_has_more_both_types_overflow() {
        let db = setup_db().await;

        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-ab', 'Lib', 'local', '/lib', 'active', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        // 2 acquisition sessions
        for i in 0..2u32 {
            sqlx::query(
                "INSERT INTO acquisition_session \
                 (id, session_key, root_id, frame_ids, created_at) VALUES (?, 'k', 'root-ab', '[]', ?)",
            )
            .bind(format!("acq-ab-{i}"))
            .bind(format!("2026-01-0{d}T00:00:00Z", d = i + 1))
            .execute(db.pool())
            .await
            .unwrap();
        }
        // 2 calibration sessions
        for i in 0..2u32 {
            sqlx::query(
                "INSERT INTO calibration_session \
                 (id, session_key, root_id, frame_ids, kind, created_at) VALUES (?, 'k', 'root-ab', '[]', 'dark', ?)",
            )
            .bind(format!("cal-ab-{i}"))
            .bind(format!("2026-01-0{d}T00:00:00Z", d = i + 1))
            .execute(db.pool())
            .await
            .unwrap();
        }

        // limit=1: both types overflow → has_more, exactly 2 payload rows (1 acq + 1 cal),
        // no sentinel in the payload.
        let (rows, hm) = list_sessions_for_root(
            db.pool(),
            "root-ab",
            &InventoryFilters { limit: Some(1), ..Default::default() },
        )
        .await
        .unwrap();

        assert!(hm, "both types have more rows beyond limit=1");
        // ORDER BY created_at DESC → index-0 is newest (acq-ab-1, cal-ab-1).
        // index-1 (acq-ab-0, cal-ab-0) is the sentinel row; it must be absent.
        assert_eq!(rows.len(), 2, "expected 1 acq + 1 cal payload row, got {}", rows.len());
        assert!(rows.iter().any(|r| r.id == "acq-ab-1"), "newest acq row must be in payload");
        assert!(rows.iter().any(|r| r.id == "cal-ab-1"), "newest cal row must be in payload");
        assert!(
            rows.iter().all(|r| r.id != "acq-ab-0" && r.id != "cal-ab-0"),
            "sentinel rows acq-ab-0/cal-ab-0 must not appear in payload: {:?}",
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()
        );
    }

    /// Case B: only the acquisition type overflows; calibration fits exactly.
    /// The old single-pop dropped the last real cal row and leaked the acq sentinel.
    #[tokio::test]
    async fn list_sessions_has_more_only_first_type_overflows() {
        let db = setup_db().await;

        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-ob', 'Lib', 'local', '/lib', 'active', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        // 3 acquisition sessions (will overflow limit=2)
        for i in 0..3u32 {
            sqlx::query(
                "INSERT INTO acquisition_session \
                 (id, session_key, root_id, frame_ids, created_at) VALUES (?, 'k', 'root-ob', '[]', ?)",
            )
            .bind(format!("acq-ob-{i}"))
            .bind(format!("2026-01-0{d}T00:00:00Z", d = i + 1))
            .execute(db.pool())
            .await
            .unwrap();
        }
        // 1 calibration session (fits within limit=2 — no overflow)
        sqlx::query(
            "INSERT INTO calibration_session \
             (id, session_key, root_id, frame_ids, kind, created_at) \
             VALUES ('cal-ob-0', 'k', 'root-ob', '[]', 'dark', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        // limit=2: acq has 3 rows → overflows (has_more); cal has 1 → fits.
        let (rows, hm) = list_sessions_for_root(
            db.pool(),
            "root-ob",
            &InventoryFilters { limit: Some(2), ..Default::default() },
        )
        .await
        .unwrap();

        assert!(hm, "acquisition type overflows so has_more must be true");
        // ORDER BY created_at DESC → acq-ob-2 is newest (payload), acq-ob-0 is oldest (sentinel).
        // With limit=2: SQL fetches 3 rows; sentinel=acq-ob-0 is popped; payload=acq-ob-2,acq-ob-1.
        assert_eq!(rows.len(), 3, "expected 2 acq + 1 cal payload rows, got {}", rows.len());
        assert!(
            rows.iter().all(|r| r.id != "acq-ob-0"),
            "acq-ob-0 is the sentinel row and must not appear in payload"
        );
        assert!(
            rows.iter().any(|r| r.id == "cal-ob-0"),
            "real cal row must be present (old bug: pop() dropped it)"
        );
    }
}
