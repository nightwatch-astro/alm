// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for the `framing` / `framing_session` tables (spec 008
//! Q27, migration 0064), plus the durable session-level clustering-key
//! accessors these tables depend on (`acquisition_session.pointing_ra_deg`
//! etc).
//!
//! This is schema-adjacent CRUD/query plumbing only (F-Framing-1). Tolerance
//! clustering (F-Framing-2), the merge/split/reassign use cases
//! (F-Framing-3), and confirm-time geometry population (F-Framing-2/5/10)
//! live in `crates/sessions` / `crates/app/core` and are not this module's
//! concern.

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat row from the `framing` table.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct FramingRow {
    pub id: String,
    pub project_id: String,
    pub target_id: Option<String>,
    pub optic_train_key: String,
    pub pointing_ra_deg: f64,
    pub pointing_dec_deg: f64,
    pub rotation_deg: f64,
    pub tolerance_pointing: f64,
    pub tolerance_rotation_deg: f64,
    /// `"suggested"` or `"user_adjusted"`.
    pub clustering: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Durable clustering-key geometry for an `acquisition_session` row.
/// All fields nullable — Q16 null semantics: legacy/unresolved rows keep NULL,
/// never a fabricated zero. `None` fields exclude the session from clustering
/// (F-Framing-2) until backfilled via rescan (Q28).
#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct SessionGeometryRow {
    pub pointing_ra_deg: Option<f64>,
    pub pointing_dec_deg: Option<f64>,
    pub rotation_deg: Option<f64>,
    pub optic_train_key: Option<String>,
}

// ── Insert helpers ───────────────────────────────────────────────────────────

/// Data required to insert a new `framing` row.
#[derive(Clone, Debug)]
pub struct InsertFraming<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub target_id: Option<&'a str>,
    pub optic_train_key: &'a str,
    pub pointing_ra_deg: f64,
    pub pointing_dec_deg: f64,
    pub rotation_deg: f64,
    pub tolerance_pointing: f64,
    pub tolerance_rotation_deg: f64,
    /// `"suggested"` or `"user_adjusted"`.
    pub clustering: &'a str,
}

// ── framing CRUD ──────────────────────────────────────────────────────────────

/// Insert a new framing row. Returns the `created_at`/`updated_at` timestamp.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint violation (e.g. unknown
/// `project_id`/`target_id`) or query failure.
pub async fn insert_framing(pool: &SqlitePool, data: &InsertFraming<'_>) -> DbResult<String> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO framing (
            id, project_id, target_id, optic_train_key,
            pointing_ra_deg, pointing_dec_deg, rotation_deg,
            tolerance_pointing, tolerance_rotation_deg,
            clustering, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(data.id)
    .bind(data.project_id)
    .bind(data.target_id)
    .bind(data.optic_train_key)
    .bind(data.pointing_ra_deg)
    .bind(data.pointing_dec_deg)
    .bind(data.rotation_deg)
    .bind(data.tolerance_pointing)
    .bind(data.tolerance_rotation_deg)
    .bind(data.clustering)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(now)
}

/// Fetch a single framing row by id.
///
/// # Errors
/// Returns [`DbError::NotFound`] when no framing with the given id exists.
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_framing(pool: &SqlitePool, id: &str) -> DbResult<FramingRow> {
    sqlx::query_as::<_, FramingRow>(
        "SELECT id, project_id, target_id, optic_train_key,
                pointing_ra_deg, pointing_dec_deg, rotation_deg,
                tolerance_pointing, tolerance_rotation_deg,
                clustering, created_at, updated_at
         FROM framing WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("framing {id}")))
}

/// List all framings for a project (creation order).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_framings_by_project(
    pool: &SqlitePool,
    project_id: &str,
) -> DbResult<Vec<FramingRow>> {
    let rows = sqlx::query_as::<_, FramingRow>(
        "SELECT id, project_id, target_id, optic_train_key,
                pointing_ra_deg, pointing_dec_deg, rotation_deg,
                tolerance_pointing, tolerance_rotation_deg,
                clustering, created_at, updated_at
         FROM framing WHERE project_id = ? ORDER BY created_at ASC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List every framing sharing an exact `optic_train_key`, across all projects
/// (F-Framing-5's attribution optic-train prefilter, migration 0068's index).
/// Callers apply pointing/rotation tolerance math and the project-level
/// target/mosaic checks in Rust — this is the coarse, cheap SQL-level cut.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_framings_by_optic_train_key(
    pool: &SqlitePool,
    optic_train_key: &str,
) -> DbResult<Vec<FramingRow>> {
    let rows = sqlx::query_as::<_, FramingRow>(
        "SELECT id, project_id, target_id, optic_train_key,
                pointing_ra_deg, pointing_dec_deg, rotation_deg,
                tolerance_pointing, tolerance_rotation_deg,
                clustering, created_at, updated_at
         FROM framing WHERE optic_train_key = ? ORDER BY created_at ASC",
    )
    .bind(optic_train_key)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Update a framing's `clustering` provenance. F-Framing-3's merge/split/
/// reassign use cases call this to flip a touched framing to
/// `"user_adjusted"` (FR-015) — never called with `"suggested"` by any
/// current caller, since re-derivation (F-Framing-2) never demotes a
/// framing back. Returns the new `updated_at` timestamp.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn update_framing_clustering(
    pool: &SqlitePool,
    id: &str,
    clustering: &str,
) -> DbResult<String> {
    let now = Timestamp::now_iso();
    sqlx::query("UPDATE framing SET clustering = ?, updated_at = ? WHERE id = ?")
        .bind(clustering)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(now)
}

/// Delete a framing row. Cascades to its `framing_session` memberships via
/// migration 0064's `ON DELETE CASCADE`. Used by `framing.merge`
/// (F-Framing-3) to remove framings whose sessions were folded into the
/// survivor. No-op (not an error) when the id does not exist.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn delete_framing(pool: &SqlitePool, id: &str) -> DbResult<()> {
    sqlx::query("DELETE FROM framing WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

// ── framing_session membership ───────────────────────────────────────────────

/// Add a light session to a framing's membership.
///
/// A session belongs to at most one framing (`framing_session.session_id` is
/// UNIQUE, migration 0064) — callers must [`remove_session_from_framing`] any
/// prior membership before reassigning (F-Framing-3's `framing.reassign`).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint violation (unknown ids, or the
/// session is already a member of a framing) or query failure.
pub async fn add_session_to_framing(
    pool: &SqlitePool,
    framing_id: &str,
    session_id: &str,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query("INSERT INTO framing_session (framing_id, session_id, added_at) VALUES (?, ?, ?)")
        .bind(framing_id)
        .bind(session_id)
        .bind(&now)
        .execute(pool)
        .await?;
    Ok(())
}

/// Remove a light session from a framing's membership. No-op (not an error)
/// when the pair does not exist.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn remove_session_from_framing(
    pool: &SqlitePool,
    framing_id: &str,
    session_id: &str,
) -> DbResult<()> {
    sqlx::query("DELETE FROM framing_session WHERE framing_id = ? AND session_id = ?")
        .bind(framing_id)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List the member session ids of a framing, in the order they were added.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_session_ids_for_framing(
    pool: &SqlitePool,
    framing_id: &str,
) -> DbResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT session_id FROM framing_session WHERE framing_id = ? ORDER BY added_at ASC",
    )
    .bind(framing_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// The framing id a session currently belongs to, if any (at most one, per the
/// `framing_session.session_id` UNIQUE constraint).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_framing_id_for_session(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT framing_id FROM framing_session WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(id,)| id))
}

// ── acquisition_session durable clustering-key geometry ─────────────────────

/// Read the durable clustering-key geometry for an `acquisition_session` row.
/// Returns `Ok(None)` when no session with the given id exists; returns
/// `Ok(Some(row))` with individually-nullable fields otherwise (Q16 semantics —
/// a session that exists but predates confirm-time geometry population has all
/// four fields `None`).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_session_geometry(
    pool: &SqlitePool,
    session_id: &str,
) -> DbResult<Option<SessionGeometryRow>> {
    let row = sqlx::query_as::<_, SessionGeometryRow>(
        "SELECT pointing_ra_deg, pointing_dec_deg, rotation_deg, optic_train_key
         FROM acquisition_session WHERE id = ?",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Write the durable clustering-key geometry for an `acquisition_session` row
/// (populated at confirm time by a later node — F-Framing-2/5/10). Passing
/// `None` for a field writes NULL; callers MUST NOT synthesize a `0.0`/`""`
/// sentinel for missing header data (Q16 FR-136 — "missing is missing").
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn set_session_geometry(
    pool: &SqlitePool,
    session_id: &str,
    pointing_ra_deg: Option<f64>,
    pointing_dec_deg: Option<f64>,
    rotation_deg: Option<f64>,
    optic_train_key: Option<&str>,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE acquisition_session
         SET pointing_ra_deg = ?, pointing_dec_deg = ?, rotation_deg = ?, optic_train_key = ?
         WHERE id = ?",
    )
    .bind(pointing_ra_deg)
    .bind(pointing_dec_deg)
    .bind(rotation_deg)
    .bind(optic_train_key)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::{insert_project, setup_db};

    async fn insert_acquisition_session(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
             VALUES (?, ?, '[]', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(format!("session-key-{id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    fn insert_data<'a>(id: &'a str, project_id: &'a str) -> InsertFraming<'a> {
        InsertFraming {
            id,
            project_id,
            target_id: None,
            optic_train_key: "scope-a|cam-a",
            pointing_ra_deg: 10.5,
            pointing_dec_deg: -20.25,
            rotation_deg: 3.0,
            tolerance_pointing: 0.1,
            tolerance_rotation_deg: 3.0,
            clustering: "suggested",
        }
    }

    // ── projects.is_mosaic default (F-Framing-1) ───────────────────────────────

    #[tokio::test]
    async fn project_is_mosaic_defaults_to_false() {
        let db = setup_db().await;
        insert_project(db.pool(), "proj-mosaic-default").await;

        let is_mosaic: bool = sqlx::query_scalar("SELECT is_mosaic FROM projects WHERE id = ?")
            .bind("proj-mosaic-default")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert!(!is_mosaic, "is_mosaic must default to false (backward-compatible)");
    }

    // ── framing insert / get / list-by-project ─────────────────────────────────

    #[tokio::test]
    async fn insert_and_get_framing_round_trips() {
        let db = setup_db().await;
        insert_project(db.pool(), "proj-1").await;

        insert_framing(db.pool(), &insert_data("framing-1", "proj-1")).await.unwrap();

        let row = get_framing(db.pool(), "framing-1").await.unwrap();
        assert_eq!(row.project_id, "proj-1");
        assert_eq!(row.target_id, None);
        assert_eq!(row.optic_train_key, "scope-a|cam-a");
        assert!((row.pointing_ra_deg - 10.5).abs() < f64::EPSILON);
        assert!((row.pointing_dec_deg - (-20.25)).abs() < f64::EPSILON);
        assert_eq!(row.clustering, "suggested");
    }

    #[tokio::test]
    async fn get_framing_not_found_for_unknown_id() {
        let db = setup_db().await;
        let err = get_framing(db.pool(), "missing").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_framings_by_project_returns_only_that_projects_framings() {
        let db = setup_db().await;
        insert_project(db.pool(), "proj-a").await;
        insert_project(db.pool(), "proj-b").await;

        insert_framing(db.pool(), &insert_data("framing-a1", "proj-a")).await.unwrap();
        insert_framing(db.pool(), &insert_data("framing-a2", "proj-a")).await.unwrap();
        insert_framing(db.pool(), &insert_data("framing-b1", "proj-b")).await.unwrap();

        let rows = list_framings_by_project(db.pool(), "proj-a").await.unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["framing-a1", "framing-a2"]);
    }

    #[tokio::test]
    async fn list_framings_by_optic_train_key_returns_only_matching_rows_across_projects() {
        let db = setup_db().await;
        insert_project(db.pool(), "proj-a").await;
        insert_project(db.pool(), "proj-b").await;

        let mut same_a = insert_data("framing-a1", "proj-a");
        same_a.optic_train_key = "scope-a|cam-a";
        insert_framing(db.pool(), &same_a).await.unwrap();

        let mut same_b = insert_data("framing-b1", "proj-b");
        same_b.optic_train_key = "scope-a|cam-a";
        insert_framing(db.pool(), &same_b).await.unwrap();

        let mut other = insert_data("framing-a2", "proj-a");
        other.optic_train_key = "scope-b|cam-b";
        insert_framing(db.pool(), &other).await.unwrap();

        let rows = list_framings_by_optic_train_key(db.pool(), "scope-a|cam-a").await.unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["framing-a1", "framing-b1"]);
    }

    // ── framing_session membership ──────────────────────────────────────────────

    #[tokio::test]
    async fn membership_add_list_remove_round_trips() {
        let db = setup_db().await;
        insert_project(db.pool(), "proj-m").await;
        insert_acquisition_session(db.pool(), "sess-1").await;
        insert_acquisition_session(db.pool(), "sess-2").await;
        insert_framing(db.pool(), &insert_data("framing-m", "proj-m")).await.unwrap();

        add_session_to_framing(db.pool(), "framing-m", "sess-1").await.unwrap();
        add_session_to_framing(db.pool(), "framing-m", "sess-2").await.unwrap();

        let members = list_session_ids_for_framing(db.pool(), "framing-m").await.unwrap();
        assert_eq!(members, vec!["sess-1", "sess-2"]);
        assert_eq!(
            get_framing_id_for_session(db.pool(), "sess-1").await.unwrap().as_deref(),
            Some("framing-m")
        );

        remove_session_from_framing(db.pool(), "framing-m", "sess-1").await.unwrap();
        let members = list_session_ids_for_framing(db.pool(), "framing-m").await.unwrap();
        assert_eq!(members, vec!["sess-2"]);
        assert_eq!(get_framing_id_for_session(db.pool(), "sess-1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_session_belongs_to_at_most_one_framing() {
        let db = setup_db().await;
        insert_project(db.pool(), "proj-u").await;
        insert_acquisition_session(db.pool(), "sess-u").await;
        insert_framing(db.pool(), &insert_data("framing-u1", "proj-u")).await.unwrap();
        insert_framing(db.pool(), &insert_data("framing-u2", "proj-u")).await.unwrap();

        add_session_to_framing(db.pool(), "framing-u1", "sess-u").await.unwrap();
        let err = add_session_to_framing(db.pool(), "framing-u2", "sess-u").await.unwrap_err();
        assert!(
            matches!(err, DbError::Database(_)),
            "UNIQUE(session_id) must reject a second framing"
        );
    }

    // ── update_framing_clustering / delete_framing (F-Framing-3) ────────────────

    #[tokio::test]
    async fn update_framing_clustering_flips_to_user_adjusted() {
        let db = setup_db().await;
        insert_project(db.pool(), "proj-c").await;
        insert_framing(db.pool(), &insert_data("framing-c", "proj-c")).await.unwrap();

        update_framing_clustering(db.pool(), "framing-c", "user_adjusted").await.unwrap();

        let row = get_framing(db.pool(), "framing-c").await.unwrap();
        assert_eq!(row.clustering, "user_adjusted");
    }

    #[tokio::test]
    async fn delete_framing_cascades_memberships() {
        let db = setup_db().await;
        insert_project(db.pool(), "proj-d").await;
        insert_acquisition_session(db.pool(), "sess-d").await;
        insert_framing(db.pool(), &insert_data("framing-d", "proj-d")).await.unwrap();
        add_session_to_framing(db.pool(), "framing-d", "sess-d").await.unwrap();

        delete_framing(db.pool(), "framing-d").await.unwrap();

        assert!(matches!(
            get_framing(db.pool(), "framing-d").await.unwrap_err(),
            DbError::NotFound(_)
        ));
        assert_eq!(get_framing_id_for_session(db.pool(), "sess-d").await.unwrap(), None);
    }

    #[tokio::test]
    async fn delete_framing_missing_id_is_not_an_error() {
        let db = setup_db().await;
        delete_framing(db.pool(), "no-such-framing").await.unwrap();
    }

    // ── acquisition_session durable geometry (Q16 null semantics) ──────────────

    #[tokio::test]
    async fn new_session_has_null_geometry_until_populated() {
        let db = setup_db().await;
        insert_acquisition_session(db.pool(), "sess-legacy").await;

        let geo = get_session_geometry(db.pool(), "sess-legacy").await.unwrap().unwrap();
        assert_eq!(
            geo,
            SessionGeometryRow {
                pointing_ra_deg: None,
                pointing_dec_deg: None,
                rotation_deg: None,
                optic_train_key: None,
            },
            "legacy/unpopulated rows must stay NULL — never a fabricated 0.0 sentinel"
        );
    }

    #[tokio::test]
    async fn set_session_geometry_round_trips_and_preserves_none_fields() {
        let db = setup_db().await;
        insert_acquisition_session(db.pool(), "sess-confirmed").await;

        set_session_geometry(
            db.pool(),
            "sess-confirmed",
            Some(83.633_2),
            Some(22.014_5),
            Some(1.5),
            Some("scope-a|cam-a"),
        )
        .await
        .unwrap();

        let geo = get_session_geometry(db.pool(), "sess-confirmed").await.unwrap().unwrap();
        assert_eq!(geo.pointing_ra_deg, Some(83.633_2));
        assert_eq!(geo.pointing_dec_deg, Some(22.014_5));
        assert_eq!(geo.rotation_deg, Some(1.5));
        assert_eq!(geo.optic_train_key.as_deref(), Some("scope-a|cam-a"));
    }

    #[tokio::test]
    async fn get_session_geometry_returns_none_for_unknown_session() {
        let db = setup_db().await;
        let geo = get_session_geometry(db.pool(), "missing").await.unwrap();
        assert!(geo.is_none());
    }
}
