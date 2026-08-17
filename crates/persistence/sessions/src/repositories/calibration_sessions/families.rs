// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for `calibration_family` and the three recipe/identity subtype
//! tables (`dark_recipe_identity`, `bias_recipe_identity`,
//! `flat_family_identity`).
//!
//! A family row is inserted atomically with its subtype row in one transaction.
//! The unique-index constraints in the schema enforce one family per
//! camera+kind+digest (dark/bias) or optical-profile+filter+digest (flat).

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

// ── Row projections ────────────────────────────────────────────────────────────

/// Core `calibration_family` row, without subtype columns.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CalibrationFamilyRow {
    pub row_id: i64,
    pub public_id: String,
    pub kind: String,
    pub camera_row_id: Option<i64>,
    pub optical_profile_row_id: Option<i64>,
    pub filter_label_row_id: Option<i64>,
    pub identity_digest: String,
    pub representative_session_row_id: i64,
    pub camera_regulation_decision_row_id: Option<i64>,
    pub created_sequence: i64,
    pub created_at: String,
}

/// `dark_recipe_identity` subtype columns.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DarkRecipeRow {
    pub family_row_id: i64,
    pub temperature_mode: String,
    pub cooling_setpoint_millic: Option<i64>,
    pub representative_exposure_us: i64,
    pub gain_text: String,
    pub offset_state: String,
    pub offset_value: Option<i64>,
    pub binning_state: String,
    pub bin_x: Option<i64>,
    pub bin_y: Option<i64>,
    pub readout_state: String,
    pub readout_mode: Option<String>,
    pub raster_width: i64,
    pub raster_height: i64,
}

/// `bias_recipe_identity` subtype columns.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BiasRecipeRow {
    pub family_row_id: i64,
    pub gain_text: String,
    pub offset_state: String,
    pub offset_value: Option<i64>,
    pub binning_state: String,
    pub bin_x: Option<i64>,
    pub bin_y: Option<i64>,
    pub readout_state: String,
    pub readout_mode: Option<String>,
    pub raster_width: i64,
    pub raster_height: i64,
}

/// `flat_family_identity` subtype columns.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FlatFamilyRow {
    pub family_row_id: i64,
    pub gain_text: String,
    pub offset_state: String,
    pub offset_value: Option<i64>,
    pub binning_state: String,
    pub bin_x: Option<i64>,
    pub bin_y: Option<i64>,
    pub readout_state: String,
    pub readout_mode: Option<String>,
    pub raster_width: i64,
    pub raster_height: i64,
    pub physical_rotator_state: String,
    pub physical_rotator_udeg: Option<i64>,
}

// ── Insert parameters ─────────────────────────────────────────────────────────

/// Parameters for inserting one `calibration_family` row.
pub struct InsertCalibrationFamily<'a> {
    pub public_id: &'a str,
    pub kind: &'a str,
    pub camera_row_id: Option<i64>,
    pub optical_profile_row_id: Option<i64>,
    pub filter_label_row_id: Option<i64>,
    pub identity_digest: &'a str,
    pub representative_session_row_id: i64,
    /// Required for dark families; must match the camera's accepted regulation
    /// decision.
    pub camera_regulation_decision_row_id: Option<i64>,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Parameters for inserting a `dark_recipe_identity` row.
pub struct InsertDarkRecipe<'a> {
    pub family_row_id: i64,
    pub temperature_mode: &'a str,
    pub cooling_setpoint_millic: Option<i64>,
    pub representative_exposure_us: i64,
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
}

/// Parameters for inserting a `bias_recipe_identity` row.
pub struct InsertBiasRecipe<'a> {
    pub family_row_id: i64,
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
}

/// Parameters for inserting a `flat_family_identity` row.
pub struct InsertFlatFamily<'a> {
    pub family_row_id: i64,
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
    pub physical_rotator_state: &'a str,
    pub physical_rotator_udeg: Option<i64>,
}

// ── Writes ────────────────────────────────────────────────────────────────────

/// Insert one `calibration_family` row.
///
/// Returns the new `row_id`. Callers insert the kind-specific subtype row
/// immediately after in the same transaction.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_calibration_family(
    conn: &mut SqliteConnection,
    params: &InsertCalibrationFamily<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO calibration_family (
            public_id, kind,
            camera_row_id, optical_profile_row_id, filter_label_row_id,
            identity_digest, representative_session_row_id,
            camera_regulation_decision_row_id,
            created_sequence, created_at
         ) VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.public_id)
    .bind(params.kind)
    .bind(params.camera_row_id)
    .bind(params.optical_profile_row_id)
    .bind(params.filter_label_row_id)
    .bind(params.identity_digest)
    .bind(params.representative_session_row_id)
    .bind(params.camera_regulation_decision_row_id)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert one `dark_recipe_identity` row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_dark_recipe(
    conn: &mut SqliteConnection,
    params: &InsertDarkRecipe<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO dark_recipe_identity (
            family_row_id, temperature_mode, cooling_setpoint_millic,
            representative_exposure_us, gain_text,
            offset_state, offset_value,
            binning_state, bin_x, bin_y,
            readout_state, readout_mode,
            raster_width, raster_height
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.family_row_id)
    .bind(params.temperature_mode)
    .bind(params.cooling_setpoint_millic)
    .bind(params.representative_exposure_us)
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
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert one `bias_recipe_identity` row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_bias_recipe(
    conn: &mut SqliteConnection,
    params: &InsertBiasRecipe<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO bias_recipe_identity (
            family_row_id, gain_text,
            offset_state, offset_value,
            binning_state, bin_x, bin_y,
            readout_state, readout_mode,
            raster_width, raster_height
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.family_row_id)
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
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert one `flat_family_identity` row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violations or SQL errors.
pub async fn insert_flat_family(
    conn: &mut SqliteConnection,
    params: &InsertFlatFamily<'_>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO flat_family_identity (
            family_row_id, gain_text,
            offset_state, offset_value,
            binning_state, bin_x, bin_y,
            readout_state, readout_mode,
            raster_width, raster_height,
            physical_rotator_state, physical_rotator_udeg
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(params.family_row_id)
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
    .bind(params.physical_rotator_state)
    .bind(params.physical_rotator_udeg)
    .execute(conn)
    .await?;
    Ok(())
}

// ── Reads ─────────────────────────────────────────────────────────────────────

/// Fetch a `calibration_family` by `public_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_family_by_public_id(
    pool: &SqlitePool,
    public_id: &str,
) -> DbResult<CalibrationFamilyRow> {
    sqlx::query_as::<_, CalibrationFamilyRow>(
        "SELECT row_id, public_id, kind,
                camera_row_id, optical_profile_row_id, filter_label_row_id,
                identity_digest, representative_session_row_id,
                camera_regulation_decision_row_id,
                created_sequence, created_at
         FROM calibration_family
         WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("calibration_family {public_id}")))
}

/// Fetch a `calibration_family` by `row_id`.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists, or
/// `persistence_core::DbError::Database` on SQL errors.
pub async fn get_family_by_row_id(
    pool: &SqlitePool,
    row_id: i64,
) -> DbResult<CalibrationFamilyRow> {
    sqlx::query_as::<_, CalibrationFamilyRow>(
        "SELECT row_id, public_id, kind,
                camera_row_id, optical_profile_row_id, filter_label_row_id,
                identity_digest, representative_session_row_id,
                camera_regulation_decision_row_id,
                created_sequence, created_at
         FROM calibration_family
         WHERE row_id = ?",
    )
    .bind(row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("calibration_family row_id={row_id}")))
}

/// Fetch the `dark_recipe_identity` row for a family.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists.
pub async fn get_dark_recipe(pool: &SqlitePool, family_row_id: i64) -> DbResult<DarkRecipeRow> {
    sqlx::query_as::<_, DarkRecipeRow>(
        "SELECT family_row_id, temperature_mode, cooling_setpoint_millic,
                representative_exposure_us, gain_text,
                offset_state, offset_value,
                binning_state, bin_x, bin_y,
                readout_state, readout_mode,
                raster_width, raster_height
         FROM dark_recipe_identity
         WHERE family_row_id = ?",
    )
    .bind(family_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("dark_recipe_identity family_row_id={family_row_id}")))
}

/// Fetch the `bias_recipe_identity` row for a family.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists.
pub async fn get_bias_recipe(pool: &SqlitePool, family_row_id: i64) -> DbResult<BiasRecipeRow> {
    sqlx::query_as::<_, BiasRecipeRow>(
        "SELECT family_row_id, gain_text,
                offset_state, offset_value,
                binning_state, bin_x, bin_y,
                readout_state, readout_mode,
                raster_width, raster_height
         FROM bias_recipe_identity
         WHERE family_row_id = ?",
    )
    .bind(family_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("bias_recipe_identity family_row_id={family_row_id}")))
}

/// Fetch the `flat_family_identity` row for a family.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if no matching row exists.
pub async fn get_flat_family(pool: &SqlitePool, family_row_id: i64) -> DbResult<FlatFamilyRow> {
    sqlx::query_as::<_, FlatFamilyRow>(
        "SELECT family_row_id, gain_text,
                offset_state, offset_value,
                binning_state, bin_x, bin_y,
                readout_state, readout_mode,
                raster_width, raster_height,
                physical_rotator_state, physical_rotator_udeg
         FROM flat_family_identity
         WHERE family_row_id = ?",
    )
    .bind(family_row_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("flat_family_identity family_row_id={family_row_id}")))
}

/// Look up an existing `calibration_family` for a dark or bias kind by its
/// uniqueness key: `(camera_row_id, kind, identity_digest)`.
///
/// Returns `None` when no matching family exists.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn find_dark_bias_family(
    pool: &SqlitePool,
    camera_row_id: i64,
    kind: &str,
    identity_digest: &str,
) -> DbResult<Option<CalibrationFamilyRow>> {
    sqlx::query_as::<_, CalibrationFamilyRow>(
        "SELECT row_id, public_id, kind,
                camera_row_id, optical_profile_row_id, filter_label_row_id,
                identity_digest, representative_session_row_id,
                camera_regulation_decision_row_id,
                created_sequence, created_at
         FROM calibration_family
         WHERE camera_row_id = ? AND kind = ? AND identity_digest = ?",
    )
    .bind(camera_row_id)
    .bind(kind)
    .bind(identity_digest)
    .fetch_optional(pool)
    .await
    .map_err(DbError::from)
}

/// Look up an existing `calibration_family` for a flat by its uniqueness key:
/// `(optical_profile_row_id, filter_label_row_id, identity_digest)`.
///
/// Returns `None` when no matching family exists.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on SQL errors.
pub async fn find_flat_family(
    pool: &SqlitePool,
    optical_profile_row_id: i64,
    filter_label_row_id: i64,
    identity_digest: &str,
) -> DbResult<Option<CalibrationFamilyRow>> {
    sqlx::query_as::<_, CalibrationFamilyRow>(
        "SELECT row_id, public_id, kind,
                camera_row_id, optical_profile_row_id, filter_label_row_id,
                identity_digest, representative_session_row_id,
                camera_regulation_decision_row_id,
                created_sequence, created_at
         FROM calibration_family
         WHERE optical_profile_row_id = ?
           AND filter_label_row_id = ?
           AND identity_digest = ?",
    )
    .bind(optical_profile_row_id)
    .bind(filter_label_row_id)
    .bind(identity_digest)
    .fetch_optional(pool)
    .await
    .map_err(DbError::from)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Open an in-memory database with all migrations applied.
    async fn setup_db() -> sqlx::SqlitePool {
        let db = persistence_core::Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db.pool().clone()
    }

    /// Seed the minimal FK-required rows for family insertion tests.
    ///
    /// Inserts: actor, config_revision, repository_change, command_execution,
    /// session_materialization_operation, camera (no regulation decision yet),
    /// optical_profile, filter_label, plus session rows for each kind tested.
    ///
    /// For dark family tests that need `camera_regulation_decision`,
    /// call [`seed_dark_regulation`] after this.
    async fn seed_prerequisites(pool: &sqlx::SqlitePool) {
        let ts = "2026-07-22T00:00:00.000000Z";
        // Core booking rows
        sqlx::query(
            "INSERT INTO spec062_actor VALUES (1,'00000000-0000-7000-a000-000000000001',?)",
        )
        .bind(ts)
        .execute(pool)
        .await
        .expect("actor");
        sqlx::query("INSERT INTO spec062_config_revision VALUES (1,'00000000-0000-7000-a000-000000000002',1,'cfg-digest',?)").bind(ts).execute(pool).await.expect("config");
        sqlx::query("INSERT INTO repository_change(command_row_id,created_at) VALUES (NULL,?)")
            .bind(ts)
            .execute(pool)
            .await
            .expect("repo_change");
        sqlx::query(
            "INSERT INTO command_execution (row_id,public_id,actor_row_id,operation,canonical_payload_digest,state,response_json,created_at,finished_at)
             VALUES (1,'00000000-0000-7000-a000-000000000003',1,'inbox.materialization.apply','pd','applied','{}',?,?)"
        ).bind(ts).bind(ts).execute(pool).await.expect("command");
        sqlx::query(
            "INSERT INTO session_materialization_operation (row_id,public_id,kind,command_row_id,config_revision_row_id,state,created_sequence,created_at)
             VALUES (1,'00000000-0000-7000-a000-000000000004','inbox_ingestion',1,1,'ready',1,?)"
        ).bind(ts).execute(pool).await.expect("operation");

        // camera (no regulation head yet)
        sqlx::query(
            "INSERT INTO camera (row_id,public_id,display_name,head_generation,created_sequence,created_at)
             VALUES (1,'cam-pub-001','ASI294MC Pro',0,1,?)"
        ).bind(ts).execute(pool).await.expect("camera");

        // optical_profile (all NOT NULL columns required)
        sqlx::query(
            "INSERT INTO optical_profile (row_id,public_id,display_name,representative_focal_length_um,representative_raster_width,representative_raster_height,created_sequence,created_at)
             VALUES (1,'op-pub-001','80ED',560000,4096,2160,1,?)"
        ).bind(ts).execute(pool).await.expect("optical_profile");
        sqlx::query(
            "INSERT INTO filter_label (row_id,public_id,optical_profile_row_id,state,normalized_label,created_sequence,created_at)
             VALUES (1,'fl-pub-001',1,'captured','Ha',1,?)"
        ).bind(ts).execute(pool).await.expect("filter_label");

        // Representative session rows for each calibration kind
        for (row_id, kind, ordinal, digest) in [
            (1i64, "dark", 0i64, "id-digest-dark"),
            (2i64, "bias", 1i64, "id-digest-bias"),
            (3i64, "flat", 2i64, "id-digest-flat"),
        ] {
            sqlx::query(
                "INSERT INTO session (row_id,public_id,materialization_operation_row_id,kind,ordinal_in_operation,identity_digest,observing_night_date,night_derivation,created_sequence,created_at)
                 VALUES (?,?,1,?,?,?,'2026-01-15','reviewed_local_fallback',1,?)"
            )
            .bind(row_id)
            .bind(format!("ses-pub-{row_id:03}"))
            .bind(kind)
            .bind(ordinal)
            .bind(digest)
            .bind(ts)
            .execute(pool).await.expect("session");
        }
    }

    /// Seed a `relation_proposal` + `camera_regulation_decision` for the dark
    /// family test. `camera_regulation_decision` requires a `relation_proposal`
    /// FK (proposal_row_id NOT NULL), so we seed that first.
    async fn seed_dark_regulation(pool: &sqlx::SqlitePool) {
        let ts = "2026-07-22T00:00:00.000000Z";
        sqlx::query(
            "INSERT INTO relation_proposal (row_id,public_id,proposal_revision,kind,basis_digest,evidence_digest,config_revision_row_id,state,actor_row_id,created_sequence,created_at,decided_at)
             VALUES (1,'prop-pub-001',1,'manual_relation','basis-01','evid-01',1,'accepted',1,1,?,?)"
        ).bind(ts).bind(ts).execute(pool).await.expect("relation_proposal");
        sqlx::query(
            "INSERT INTO camera_regulation_decision (row_id,public_id,camera_row_id,mode,proposal_row_id,config_revision_row_id,actor_row_id,created_sequence,created_at)
             VALUES (1,'reg-pub-001',1,'regulated',1,1,1,1,?)"
        ).bind(ts).execute(pool).await.expect("camera_regulation_decision");
        sqlx::query(
            "UPDATE camera SET regulation_head_decision_row_id=1, head_generation=1 WHERE row_id=1",
        )
        .execute(pool)
        .await
        .expect("camera head update");
    }

    #[tokio::test]
    async fn insert_and_get_bias_family() {
        let pool = setup_db().await;
        seed_prerequisites(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        let family_row_id = insert_calibration_family(
            &mut conn,
            &InsertCalibrationFamily {
                public_id: "fam-pub-bias-001",
                kind: "bias",
                camera_row_id: Some(1),
                optical_profile_row_id: None,
                filter_label_row_id: None,
                identity_digest: "bias-digest-001",
                representative_session_row_id: 2, // session row 2 is bias kind
                camera_regulation_decision_row_id: None,
                created_sequence: 1,
                created_at: "2026-07-22T00:00:00.000000Z",
            },
        )
        .await
        .expect("insert bias family");

        insert_bias_recipe(
            &mut conn,
            &InsertBiasRecipe {
                family_row_id,
                gain_text: "100",
                offset_state: "present",
                offset_value: Some(50),
                binning_state: "present",
                bin_x: Some(1),
                bin_y: Some(1),
                readout_state: "absent",
                readout_mode: None,
                raster_width: 4096,
                raster_height: 2160,
            },
        )
        .await
        .expect("insert bias recipe");

        let fam = get_family_by_public_id(&pool, "fam-pub-bias-001").await.expect("get family");
        assert_eq!(fam.kind, "bias");
        assert_eq!(fam.camera_row_id, Some(1));

        let recipe = get_bias_recipe(&pool, family_row_id).await.expect("get bias recipe");
        assert_eq!(recipe.gain_text, "100");
        assert_eq!(recipe.raster_width, 4096);
    }

    #[tokio::test]
    async fn insert_and_get_flat_family() {
        let pool = setup_db().await;
        seed_prerequisites(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        let family_row_id = insert_calibration_family(
            &mut conn,
            &InsertCalibrationFamily {
                public_id: "fam-pub-flat-001",
                kind: "flat",
                camera_row_id: None,
                optical_profile_row_id: Some(1),
                filter_label_row_id: Some(1),
                identity_digest: "flat-digest-001",
                representative_session_row_id: 3, // session row 3 is flat kind
                camera_regulation_decision_row_id: None,
                created_sequence: 1,
                created_at: "2026-07-22T00:00:00.000000Z",
            },
        )
        .await
        .expect("insert flat family");

        insert_flat_family(
            &mut conn,
            &InsertFlatFamily {
                family_row_id,
                gain_text: "100",
                offset_state: "present",
                offset_value: Some(50),
                binning_state: "present",
                bin_x: Some(1),
                bin_y: Some(1),
                readout_state: "absent",
                readout_mode: None,
                raster_width: 4096,
                raster_height: 2160,
                physical_rotator_state: "verified",
                physical_rotator_udeg: Some(0),
            },
        )
        .await
        .expect("insert flat family identity");

        let fam = get_family_by_row_id(&pool, family_row_id).await.expect("get by row_id");
        assert_eq!(fam.optical_profile_row_id, Some(1));

        let ff = get_flat_family(&pool, family_row_id).await.expect("get flat family");
        assert_eq!(ff.physical_rotator_state, "verified");
        assert_eq!(ff.physical_rotator_udeg, Some(0));
        assert_eq!(ff.gain_text, "100");

        let found = find_flat_family(&pool, 1, 1, "flat-digest-001").await.expect("find flat");
        assert!(found.is_some());

        let not_found =
            find_flat_family(&pool, 1, 1, "wrong-digest").await.expect("find flat none");
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn insert_and_get_dark_family_with_regulation_decision() {
        let pool = setup_db().await;
        seed_prerequisites(&pool).await;
        seed_dark_regulation(&pool).await;

        let mut conn = pool.acquire().await.expect("conn");
        let family_row_id = insert_calibration_family(
            &mut conn,
            &InsertCalibrationFamily {
                public_id: "fam-pub-dark-001",
                kind: "dark",
                camera_row_id: Some(1),
                optical_profile_row_id: None,
                filter_label_row_id: None,
                identity_digest: "dark-digest-001",
                representative_session_row_id: 1, // session row 1 is dark kind
                camera_regulation_decision_row_id: Some(1),
                created_sequence: 1,
                created_at: "2026-07-22T00:00:00.000000Z",
            },
        )
        .await
        .expect("insert dark family");

        insert_dark_recipe(
            &mut conn,
            &InsertDarkRecipe {
                family_row_id,
                temperature_mode: "regulated",
                cooling_setpoint_millic: Some(-10_000),
                representative_exposure_us: 60_000_000,
                gain_text: "100",
                offset_state: "present",
                offset_value: Some(50),
                binning_state: "present",
                bin_x: Some(1),
                bin_y: Some(1),
                readout_state: "absent",
                readout_mode: None,
                raster_width: 4096,
                raster_height: 2160,
            },
        )
        .await
        .expect("insert dark recipe");

        let fam = get_family_by_public_id(&pool, "fam-pub-dark-001").await.expect("get family");
        assert_eq!(fam.kind, "dark");
        assert_eq!(fam.camera_row_id, Some(1));
        assert_eq!(fam.camera_regulation_decision_row_id, Some(1));

        let recipe = get_dark_recipe(&pool, family_row_id).await.expect("get dark recipe");
        assert_eq!(recipe.temperature_mode, "regulated");
        assert_eq!(recipe.cooling_setpoint_millic, Some(-10_000));
        assert_eq!(recipe.representative_exposure_us, 60_000_000);
        assert_eq!(recipe.gain_text, "100");

        let found =
            find_dark_bias_family(&pool, 1, "dark", "dark-digest-001").await.expect("find dark");
        assert!(found.is_some());

        let none = find_dark_bias_family(&pool, 1, "dark", "nonexistent").await.expect("none");
        assert!(none.is_none());
    }
}
