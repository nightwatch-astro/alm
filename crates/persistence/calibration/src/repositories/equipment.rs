// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for equipment management (spec 030, T016).
//!
//! CRUD operations for cameras, telescopes, optical trains, and filters.
//! Operates on `cameras`, `telescopes`, `optical_trains`, and `filters`
//! tables (migration 0007).

use domain_core::equipment::{
    Camera, CreateCamera, CreateFilter, CreateOpticalTrain, CreateTelescope, Filter,
    FilterCategory, OpticalTrain, SensorType, Telescope, UpdateCamera, UpdateFilter,
    UpdateOpticalTrain, UpdateTelescope,
};
use domain_core::ids::Timestamp;
use sqlx::SqlitePool;
use uuid::Uuid;

use persistence_core::{DbError, DbResult};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn category_to_str(cat: FilterCategory) -> &'static str {
    match cat {
        FilterCategory::Narrowband => "narrowband",
        FilterCategory::Broadband => "broadband",
        FilterCategory::DualBand => "dual_band",
        FilterCategory::Other => "other",
        FilterCategory::Custom => "custom",
    }
}

fn str_to_category(s: &str) -> FilterCategory {
    match s {
        "narrowband" => FilterCategory::Narrowband,
        "broadband" => FilterCategory::Broadband,
        "dual_band" => FilterCategory::DualBand,
        "custom" => FilterCategory::Custom,
        // "other" and any unknown value default to Other.
        _ => FilterCategory::Other,
    }
}

// Camera sensor type (spec 044 iteration 2026-07-15, FR-035/migration 0067):
// stored as TEXT 'mono'|'osc'; NULL/unknown values read back as None so
// unknown always behaves as mono downstream (FR-038).
fn sensor_type_to_str(sensor: SensorType) -> &'static str {
    match sensor {
        SensorType::Mono => "mono",
        SensorType::Osc => "osc",
    }
}

fn str_to_sensor_type(s: &str) -> Option<SensorType> {
    match s {
        "mono" => Some(SensorType::Mono),
        "osc" => Some(SensorType::Osc),
        _ => None,
    }
}

/// Passband is stored like `aliases`: a JSON string array (`["Ha","OIII"]`);
/// NULL = plain color camera ('rgb' default, FR-035).
fn parse_passband(json_str: Option<&str>) -> Option<Vec<String>> {
    json_str.and_then(|s| serde_json::from_str(s).ok())
}

fn parse_aliases(json_str: &str) -> Vec<String> {
    serde_json::from_str(json_str).unwrap_or_default()
}

/// Row tuple shared by every `cameras` SELECT below. The column list itself
/// must stay a literal in each query (sqlx's `SqlSafeStr` bound rejects
/// composed SQL strings), so it is the row->struct mapping that is shared,
/// via [`row_to_camera`]. Column order must match those literals.
type CameraRow = (
    String,
    String,
    String,
    i32,
    Option<String>,
    Option<String>,
    Option<f64>,
    Option<i64>,
    Option<i64>,
);

fn row_to_camera(row: CameraRow) -> Camera {
    let (
        id,
        name,
        aliases,
        auto_detected,
        sensor_type,
        passband,
        pixel_size_um,
        sensor_width_px,
        sensor_height_px,
    ) = row;
    Camera {
        id,
        name,
        aliases: parse_aliases(&aliases),
        auto_detected: auto_detected != 0,
        sensor_type: sensor_type.as_deref().and_then(str_to_sensor_type),
        passband: parse_passband(passband.as_deref()),
        pixel_size_um,
        sensor_width_px,
        sensor_height_px,
    }
}

fn encode_aliases(aliases: &[String]) -> String {
    serde_json::to_string(aliases).unwrap_or_else(|_| "[]".to_owned())
}

/// Shared NotFound-check for every `update_*`/`delete_*` below: all four
/// entity kinds (Camera/Telescope/OpticalTrain/Filter) key their mutation on
/// `id` and treat "no row touched" as `DbError::NotFound`.
fn ensure_row_affected(rows_affected: u64, entity: &str, id: &str) -> DbResult<()> {
    if rows_affected == 0 {
        return Err(DbError::NotFound(format!("{entity} {id} not found")));
    }
    Ok(())
}

// ── Camera ──────────────────────────────────────────────────────────────────

/// List all cameras.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_cameras(pool: &SqlitePool) -> DbResult<Vec<Camera>> {
    let rows: Vec<CameraRow> = sqlx::query_as(
        "SELECT id, name, aliases, auto_detected, sensor_type, passband, \
         pixel_size_um, sensor_width_px, sensor_height_px \
         FROM cameras ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_camera).collect())
}

/// Create a new camera.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violation.
pub async fn create_camera(pool: &SqlitePool, req: &CreateCamera) -> DbResult<Camera> {
    let id = Uuid::new_v4().to_string();
    let aliases_json = encode_aliases(&req.aliases);
    let created_at = Timestamp::now_iso();
    let sensor_type_str = req.sensor_type.map(sensor_type_to_str);
    let passband_json = req.passband.as_deref().map(encode_aliases);

    sqlx::query(
        "INSERT INTO cameras (id, name, aliases, auto_detected, created_at, sensor_type, passband, \
         pixel_size_um, sensor_width_px, sensor_height_px) \
         VALUES (?, ?, ?, 0, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&aliases_json)
    .bind(&created_at)
    .bind(sensor_type_str)
    .bind(&passband_json)
    .bind(req.pixel_size_um)
    .bind(req.sensor_width_px)
    .bind(req.sensor_height_px)
    .execute(pool)
    .await?;

    Ok(Camera {
        id,
        name: req.name.clone(),
        aliases: req.aliases.clone(),
        auto_detected: false,
        sensor_type: req.sensor_type,
        passband: req.passband.clone(),
        pixel_size_um: req.pixel_size_um,
        sensor_width_px: req.sensor_width_px,
        sensor_height_px: req.sensor_height_px,
    })
}

/// Update an existing camera.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn update_camera(pool: &SqlitePool, req: &UpdateCamera) -> DbResult<Camera> {
    let aliases_json = encode_aliases(&req.aliases);
    let sensor_type_str = req.sensor_type.map(sensor_type_to_str);
    let passband_json = req.passband.as_deref().map(encode_aliases);

    let result = sqlx::query(
        "UPDATE cameras SET name = ?, aliases = ?, sensor_type = ?, passband = ?, \
         pixel_size_um = ?, sensor_width_px = ?, sensor_height_px = ? WHERE id = ?",
    )
    .bind(&req.name)
    .bind(&aliases_json)
    .bind(sensor_type_str)
    .bind(&passband_json)
    .bind(req.pixel_size_um)
    .bind(req.sensor_width_px)
    .bind(req.sensor_height_px)
    .bind(&req.id)
    .execute(pool)
    .await?;

    ensure_row_affected(result.rows_affected(), "camera", &req.id)?;

    // Fetch the full row to return auto_detected.
    let row: (i32,) = sqlx::query_as("SELECT auto_detected FROM cameras WHERE id = ?")
        .bind(&req.id)
        .fetch_one(pool)
        .await?;

    Ok(Camera {
        id: req.id.clone(),
        name: req.name.clone(),
        aliases: req.aliases.clone(),
        auto_detected: row.0 != 0,
        sensor_type: req.sensor_type,
        passband: req.passband.clone(),
        pixel_size_um: req.pixel_size_um,
        sensor_width_px: req.sensor_width_px,
        sensor_height_px: req.sensor_height_px,
    })
}

/// Delete a camera by ID.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn delete_camera(pool: &SqlitePool, id: &str) -> DbResult<()> {
    let result = sqlx::query("DELETE FROM cameras WHERE id = ?").bind(id).execute(pool).await?;

    ensure_row_affected(result.rows_affected(), "camera", id)
}

/// Find a camera by alias. Matches an exact member of the JSON aliases array.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_camera_by_alias(pool: &SqlitePool, alias: &str) -> DbResult<Option<Camera>> {
    // SQLite JSON: match an exact alias value inside the aliases JSON array.
    // `EXISTS` rather than a `json_each` cross join so the selected columns
    // need no table qualifier; same rows, same LIMIT 1.
    let row: Option<CameraRow> = sqlx::query_as(
        "SELECT id, name, aliases, auto_detected, sensor_type, passband, \
         pixel_size_um, sensor_width_px, sensor_height_px \
         FROM cameras \
         WHERE EXISTS (SELECT 1 FROM json_each( \
                           CASE WHEN json_valid(cameras.aliases) \
                                THEN cameras.aliases ELSE '[]' END) j \
                       WHERE j.value = ?) \
         LIMIT 1",
    )
    .bind(alias)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_camera))
}

// ── Telescope ───────────────────────────────────────────────────────────────

/// List all telescopes.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_telescopes(pool: &SqlitePool) -> DbResult<Vec<Telescope>> {
    let rows: Vec<(String, String, String, Option<i32>, i32)> = sqlx::query_as(
        "SELECT id, name, aliases, focal_length_mm, auto_detected FROM telescopes ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, aliases, focal_length_mm, auto_detected)| Telescope {
            id,
            name,
            aliases: parse_aliases(&aliases),
            focal_length_mm,
            auto_detected: auto_detected != 0,
        })
        .collect())
}

/// Create a new telescope.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violation.
pub async fn create_telescope(pool: &SqlitePool, req: &CreateTelescope) -> DbResult<Telescope> {
    let id = Uuid::new_v4().to_string();
    let aliases_json = encode_aliases(&req.aliases);
    let created_at = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO telescopes (id, name, aliases, focal_length_mm, auto_detected, created_at) \
         VALUES (?, ?, ?, ?, 0, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&aliases_json)
    .bind(req.focal_length_mm)
    .bind(&created_at)
    .execute(pool)
    .await?;

    Ok(Telescope {
        id,
        name: req.name.clone(),
        aliases: req.aliases.clone(),
        focal_length_mm: req.focal_length_mm,
        auto_detected: false,
    })
}

/// Update an existing telescope.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn update_telescope(pool: &SqlitePool, req: &UpdateTelescope) -> DbResult<Telescope> {
    let aliases_json = encode_aliases(&req.aliases);

    let result = sqlx::query(
        "UPDATE telescopes SET name = ?, aliases = ?, focal_length_mm = ? WHERE id = ?",
    )
    .bind(&req.name)
    .bind(&aliases_json)
    .bind(req.focal_length_mm)
    .bind(&req.id)
    .execute(pool)
    .await?;

    ensure_row_affected(result.rows_affected(), "telescope", &req.id)?;

    let row: (i32,) = sqlx::query_as("SELECT auto_detected FROM telescopes WHERE id = ?")
        .bind(&req.id)
        .fetch_one(pool)
        .await?;

    Ok(Telescope {
        id: req.id.clone(),
        name: req.name.clone(),
        aliases: req.aliases.clone(),
        focal_length_mm: req.focal_length_mm,
        auto_detected: row.0 != 0,
    })
}

/// Delete a telescope by ID.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn delete_telescope(pool: &SqlitePool, id: &str) -> DbResult<()> {
    let result = sqlx::query("DELETE FROM telescopes WHERE id = ?").bind(id).execute(pool).await?;

    ensure_row_affected(result.rows_affected(), "telescope", id)
}

/// Find a telescope by alias. Searches the JSON aliases array.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_telescope_by_alias(
    pool: &SqlitePool,
    alias: &str,
) -> DbResult<Option<Telescope>> {
    let row: Option<(String, String, String, Option<i32>, i32)> = sqlx::query_as(
        "SELECT t.id, t.name, t.aliases, t.focal_length_mm, t.auto_detected \
         FROM telescopes t, json_each( \
                  CASE WHEN json_valid(t.aliases) \
                       THEN t.aliases ELSE '[]' END) j \
         WHERE j.value = ? \
         LIMIT 1",
    )
    .bind(alias)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, name, aliases, focal_length_mm, auto_detected)| Telescope {
        id,
        name,
        aliases: parse_aliases(&aliases),
        focal_length_mm,
        auto_detected: auto_detected != 0,
    }))
}

// ── Optical Train ───────────────────────────────────────────────────────────

/// A train row joined to its camera's sensor geometry: `(id, name,
/// telescope_id, camera_id, focal_length_mm, pixel_size_um, sensor_width_px,
/// sensor_height_px)`. The trailing three are `None` when no camera is linked
/// or the linked camera has no recorded geometry.
type TrainWithGeometryRow =
    (String, String, Option<String>, Option<String>, i32, Option<f64>, Option<i64>, Option<i64>);

/// Derived diagonal FOV for a train, in degrees.
///
/// Delegates to [`sessions::fov_diagonal_deg`], the workspace's single FOV
/// geometry implementation (also used by the inbox attribution pass), so the
/// two surfaces can never disagree. Returns `None` when any operand is absent
/// or non-positive — never a fabricated `0.0`.
fn train_fov_deg(
    focal_length_mm: i32,
    pixel_size_um: Option<f64>,
    sensor_width_px: Option<i64>,
    sensor_height_px: Option<i64>,
) -> Option<f64> {
    sessions::fov_diagonal_deg(
        f64::from(focal_length_mm),
        pixel_size_um?,
        sensor_width_px?,
        sensor_height_px?,
    )
}

/// Look up the linked camera's geometry and derive the train FOV.
///
/// Used by `create_optical_train`/`update_optical_train`, which return the
/// train they just wrote and so cannot reuse the list query's join.
async fn fetch_train_fov_deg(
    pool: &SqlitePool,
    camera_id: Option<&String>,
    focal_length_mm: i32,
) -> DbResult<Option<f64>> {
    let Some(camera_id) = camera_id else {
        return Ok(None);
    };
    let row: Option<(Option<f64>, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT pixel_size_um, sensor_width_px, sensor_height_px FROM cameras WHERE id = ?",
    )
    .bind(camera_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|(px, w, h)| train_fov_deg(focal_length_mm, px, w, h)))
}

/// List all optical trains.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_optical_trains(pool: &SqlitePool) -> DbResult<Vec<OpticalTrain>> {
    // LEFT JOIN: a train with no camera, or a camera with no recorded
    // geometry, still lists — it just reports no FOV.
    let rows: Vec<TrainWithGeometryRow> = sqlx::query_as(
        "SELECT t.id, t.name, t.telescope_id, t.camera_id, t.focal_length_mm, \
             c.pixel_size_um, c.sensor_width_px, c.sensor_height_px \
             FROM optical_trains t LEFT JOIN cameras c ON c.id = t.camera_id \
             ORDER BY t.name ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, telescope_id, camera_id, focal_length_mm, px, w, h)| OpticalTrain {
            id,
            name,
            telescope_id,
            camera_id,
            focal_length_mm,
            fov_diagonal_deg: train_fov_deg(focal_length_mm, px, w, h),
        })
        .collect())
}

/// Create a new optical train.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violation.
pub async fn create_optical_train(
    pool: &SqlitePool,
    req: &CreateOpticalTrain,
) -> DbResult<OpticalTrain> {
    let id = Uuid::new_v4().to_string();
    let created_at = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO optical_trains (id, name, telescope_id, camera_id, focal_length_mm, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.telescope_id)
    .bind(&req.camera_id)
    .bind(req.focal_length_mm)
    .bind(&created_at)
    .execute(pool)
    .await?;

    let fov_diagonal_deg =
        fetch_train_fov_deg(pool, req.camera_id.as_ref(), req.focal_length_mm).await?;

    Ok(OpticalTrain {
        id,
        name: req.name.clone(),
        telescope_id: req.telescope_id.clone(),
        camera_id: req.camera_id.clone(),
        focal_length_mm: req.focal_length_mm,
        fov_diagonal_deg,
    })
}

/// Update an existing optical train.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn update_optical_train(
    pool: &SqlitePool,
    req: &UpdateOpticalTrain,
) -> DbResult<OpticalTrain> {
    let result = sqlx::query(
        "UPDATE optical_trains SET name = ?, telescope_id = ?, camera_id = ?, focal_length_mm = ? \
         WHERE id = ?",
    )
    .bind(&req.name)
    .bind(&req.telescope_id)
    .bind(&req.camera_id)
    .bind(req.focal_length_mm)
    .bind(&req.id)
    .execute(pool)
    .await?;

    ensure_row_affected(result.rows_affected(), "optical train", &req.id)?;

    let fov_diagonal_deg =
        fetch_train_fov_deg(pool, req.camera_id.as_ref(), req.focal_length_mm).await?;

    Ok(OpticalTrain {
        id: req.id.clone(),
        name: req.name.clone(),
        telescope_id: req.telescope_id.clone(),
        camera_id: req.camera_id.clone(),
        focal_length_mm: req.focal_length_mm,
        fov_diagonal_deg,
    })
}

/// Delete an optical train by ID.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn delete_optical_train(pool: &SqlitePool, id: &str) -> DbResult<()> {
    let result =
        sqlx::query("DELETE FROM optical_trains WHERE id = ?").bind(id).execute(pool).await?;

    ensure_row_affected(result.rows_affected(), "optical train", id)
}

// ── Filter ──────────────────────────────────────────────────────────────────

/// List all filters.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_filters(pool: &SqlitePool) -> DbResult<Vec<Filter>> {
    let rows: Vec<(String, String, String, i32)> =
        sqlx::query_as("SELECT id, name, category, auto_detected FROM filters ORDER BY name ASC")
            .fetch_all(pool)
            .await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, category, auto_detected)| Filter {
            id,
            name,
            category: str_to_category(&category),
            auto_detected: auto_detected != 0,
        })
        .collect())
}

/// Create a new filter.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violation (e.g. duplicate name).
pub async fn create_filter(pool: &SqlitePool, req: &CreateFilter) -> DbResult<Filter> {
    let id = Uuid::new_v4().to_string();
    let category_str = category_to_str(req.category);
    let created_at = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO filters (id, name, category, auto_detected, created_at) VALUES (?, ?, ?, 0, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(category_str)
    .bind(&created_at)
    .execute(pool)
    .await?;

    Ok(Filter { id, name: req.name.clone(), category: req.category, auto_detected: false })
}

/// Update an existing filter.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn update_filter(pool: &SqlitePool, req: &UpdateFilter) -> DbResult<Filter> {
    let category_str = category_to_str(req.category);

    let result = sqlx::query("UPDATE filters SET name = ?, category = ? WHERE id = ?")
        .bind(&req.name)
        .bind(category_str)
        .bind(&req.id)
        .execute(pool)
        .await?;

    ensure_row_affected(result.rows_affected(), "filter", &req.id)?;

    let row: (i32,) = sqlx::query_as("SELECT auto_detected FROM filters WHERE id = ?")
        .bind(&req.id)
        .fetch_one(pool)
        .await?;

    Ok(Filter {
        id: req.id.clone(),
        name: req.name.clone(),
        category: req.category,
        auto_detected: row.0 != 0,
    })
}

/// Delete a filter by ID.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if the ID does not exist.
pub async fn delete_filter(pool: &SqlitePool, id: &str) -> DbResult<()> {
    let result = sqlx::query("DELETE FROM filters WHERE id = ?").bind(id).execute(pool).await?;

    ensure_row_affected(result.rows_affected(), "filter", id)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use domain_core::equipment::{
        CreateCamera, CreateFilter, CreateOpticalTrain, CreateTelescope, FilterCategory,
        UpdateCamera, UpdateFilter, UpdateOpticalTrain, UpdateTelescope,
    };
    use sqlx::SqlitePool;

    use super::*;
    use persistence_core::Database;

    async fn setup_db() -> SqlitePool {
        let db = Database::in_memory().await.expect("in-memory connect");
        db.migrate().await.expect("migrations");
        db.pool().clone()
    }

    // ── Camera tests ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn camera_crud() {
        let pool = setup_db().await;

        let camera = create_camera(
            &pool,
            &CreateCamera {
                name: "ASI2600MM".to_owned(),
                aliases: vec!["ZWO 2600".to_owned()],
                sensor_type: None,
                passband: None,
                pixel_size_um: None,
                sensor_width_px: None,
                sensor_height_px: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(camera.name, "ASI2600MM");
        assert!(!camera.auto_detected);
        // FR-038: sensor type is unknown until the user sets it.
        assert_eq!(camera.sensor_type, None);
        assert_eq!(camera.passband, None);

        let all = list_cameras(&pool).await.unwrap();
        assert_eq!(all.len(), 1);

        // FR-035 round-trip: set OSC + a dual-band passband on update.
        let updated = update_camera(
            &pool,
            &UpdateCamera {
                id: camera.id.clone(),
                name: "ASI2600MM Pro".to_owned(),
                aliases: vec!["ZWO 2600".to_owned(), "ASI2600".to_owned()],
                sensor_type: Some(SensorType::Osc),
                passband: Some(vec!["Ha".to_owned(), "OIII".to_owned()]),
                pixel_size_um: None,
                sensor_width_px: None,
                sensor_height_px: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.name, "ASI2600MM Pro");
        assert_eq!(updated.aliases.len(), 2);
        assert_eq!(updated.sensor_type, Some(SensorType::Osc));
        assert_eq!(updated.passband, Some(vec!["Ha".to_owned(), "OIII".to_owned()]));

        // The persisted row (not just the returned DTO) carries the fields.
        let listed = list_cameras(&pool).await.unwrap();
        assert_eq!(listed[0].sensor_type, Some(SensorType::Osc));
        assert_eq!(listed[0].passband, Some(vec!["Ha".to_owned(), "OIII".to_owned()]));

        delete_camera(&pool, &camera.id).await.unwrap();
        assert!(list_cameras(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn camera_find_by_alias() {
        let pool = setup_db().await;

        create_camera(
            &pool,
            &CreateCamera {
                name: "ASI2600MM".to_owned(),
                aliases: vec!["ZWO 2600".to_owned(), "ASI2600".to_owned()],
                sensor_type: None,
                passband: None,
                pixel_size_um: None,
                sensor_width_px: None,
                sensor_height_px: None,
            },
        )
        .await
        .unwrap();

        let found = find_camera_by_alias(&pool, "ZWO 2600").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "ASI2600MM");

        let not_found = find_camera_by_alias(&pool, "nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    /// Writes a row whose JSON `aliases` column holds a non-JSON string, which
    /// the schema permits: `aliases` is `TEXT NOT NULL DEFAULT '[]'` with no
    /// `json_valid` CHECK. `cameras` and `telescopes` share the four columns
    /// touched here.
    ///
    /// Called before the intact row so the corrupt one takes the lower rowid: a
    /// full table scan then reaches it first and the by-alias probes' `LIMIT 1`
    /// cannot skip past it.
    async fn insert_malformed_aliases_row(pool: &SqlitePool, table: &str, id: &str) {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {table} (id, name, aliases, created_at) \
             VALUES (?, 'corrupt', 'oops', '2026-01-01T00:00:00Z')"
        )))
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn camera_find_by_alias_survives_a_malformed_aliases_row() {
        let pool = setup_db().await;
        insert_malformed_aliases_row(&pool, "cameras", "cam-corrupt").await;

        create_camera(
            &pool,
            &CreateCamera {
                name: "ASI2600MM".to_owned(),
                aliases: vec!["ZWO 2600".to_owned()],
                sensor_type: None,
                passband: None,
                pixel_size_um: None,
                sensor_width_px: None,
                sensor_height_px: None,
            },
        )
        .await
        .unwrap();

        let found = find_camera_by_alias(&pool, "ZWO 2600")
            .await
            .expect("a malformed aliases row must not fail the whole query")
            .expect("the intact camera is still matched");
        assert_eq!(found.name, "ASI2600MM");
        assert_eq!(found.aliases, vec!["ZWO 2600".to_owned()]);
    }

    #[tokio::test]
    async fn camera_delete_nonexistent() {
        let pool = setup_db().await;
        let result = delete_camera(&pool, "nonexistent").await;
        assert!(result.is_err());
    }

    /// `aliases` is a named graceful-degradation site (spec `n4_jsoncodec`,
    /// duplication-and-abstraction-audit.md T2-d): a row with a corrupt
    /// `aliases` cell (hand-edited DB) must still list, with an empty
    /// alias list, not fail the whole query.
    #[tokio::test]
    async fn list_cameras_degrades_on_corrupt_aliases_cell() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO cameras (id, name, aliases, auto_detected, created_at) \
             VALUES ('cam-corrupt', 'Corrupt Cam', 'not valid json', 0, '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let cameras = list_cameras(&pool).await.unwrap();
        let corrupt = cameras.iter().find(|c| c.id == "cam-corrupt").expect("row present");
        assert!(corrupt.aliases.is_empty(), "corrupt aliases cell must degrade, not error");
    }

    // ── Telescope tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn telescope_crud() {
        let pool = setup_db().await;

        let scope = create_telescope(
            &pool,
            &CreateTelescope {
                name: "Esprit 100ED".to_owned(),
                aliases: vec!["SW Esprit".to_owned()],
                focal_length_mm: Some(550),
            },
        )
        .await
        .unwrap();
        assert_eq!(scope.name, "Esprit 100ED");
        assert_eq!(scope.focal_length_mm, Some(550));

        let updated = update_telescope(
            &pool,
            &UpdateTelescope {
                id: scope.id.clone(),
                name: "Esprit 100ED f/5.5".to_owned(),
                aliases: vec!["SW Esprit".to_owned()],
                focal_length_mm: Some(550),
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.name, "Esprit 100ED f/5.5");

        delete_telescope(&pool, &scope.id).await.unwrap();
        assert!(list_telescopes(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn telescope_find_by_alias() {
        let pool = setup_db().await;

        create_telescope(
            &pool,
            &CreateTelescope {
                name: "Esprit 100ED".to_owned(),
                aliases: vec!["SW Esprit".to_owned()],
                focal_length_mm: Some(550),
            },
        )
        .await
        .unwrap();

        let found = find_telescope_by_alias(&pool, "SW Esprit").await.unwrap();
        assert!(found.is_some());

        let not_found = find_telescope_by_alias(&pool, "missing").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn telescope_find_by_alias_survives_a_malformed_aliases_row() {
        let pool = setup_db().await;
        insert_malformed_aliases_row(&pool, "telescopes", "tel-corrupt").await;

        create_telescope(
            &pool,
            &CreateTelescope {
                name: "Esprit 100ED".to_owned(),
                aliases: vec!["SW Esprit".to_owned()],
                focal_length_mm: Some(550),
            },
        )
        .await
        .unwrap();

        let found = find_telescope_by_alias(&pool, "SW Esprit")
            .await
            .expect("a malformed aliases row must not fail the whole query")
            .expect("the intact telescope is still matched");
        assert_eq!(found.name, "Esprit 100ED");
        assert_eq!(found.aliases, vec!["SW Esprit".to_owned()]);
        assert_eq!(found.focal_length_mm, Some(550));
    }

    // ── Optical Train tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn optical_train_crud() {
        let pool = setup_db().await;

        let train = create_optical_train(
            &pool,
            &CreateOpticalTrain {
                name: "Main imaging".to_owned(),
                telescope_id: None,
                camera_id: None,
                focal_length_mm: 550,
            },
        )
        .await
        .unwrap();
        assert_eq!(train.name, "Main imaging");
        assert_eq!(train.focal_length_mm, 550);

        let updated = update_optical_train(
            &pool,
            &UpdateOpticalTrain {
                id: train.id.clone(),
                name: "Main imaging (updated)".to_owned(),
                telescope_id: None,
                camera_id: None,
                focal_length_mm: 600,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.focal_length_mm, 600);

        delete_optical_train(&pool, &train.id).await.unwrap();
        assert!(list_optical_trains(&pool).await.unwrap().is_empty());
    }

    /// Helper: register a camera, optionally with sensor geometry.
    async fn camera_with_geometry(
        pool: &SqlitePool,
        name: &str,
        geometry: Option<(f64, i64, i64)>,
    ) -> Camera {
        let (pixel_size_um, sensor_width_px, sensor_height_px) = match geometry {
            Some((px, w, h)) => (Some(px), Some(w), Some(h)),
            None => (None, None, None),
        };
        create_camera(
            pool,
            &CreateCamera {
                name: name.to_owned(),
                aliases: vec![],
                sensor_type: None,
                passband: None,
                pixel_size_um,
                sensor_width_px,
                sensor_height_px,
            },
        )
        .await
        .unwrap()
    }

    async fn train_with_camera(pool: &SqlitePool, camera: &Camera, focal_mm: i32) -> OpticalTrain {
        create_optical_train(
            pool,
            &CreateOpticalTrain {
                name: format!("train for {}", camera.name),
                telescope_id: None,
                camera_id: Some(camera.id.clone()),
                focal_length_mm: focal_mm,
            },
        )
        .await
        .unwrap()
    }

    /// A camera WITH geometry yields a train FOV on every read path, and the
    /// value is exactly `sessions::fov_diagonal_deg` — asserting against the
    /// shared helper rather than a re-derived constant is what keeps this
    /// crate from growing a second, divergent FOV formula.
    #[tokio::test]
    async fn train_reports_fov_when_camera_has_geometry() {
        let pool = setup_db().await;
        // ASI2600MM: 3.76 um pixels, 6248 x 4176, on a 530 mm scope.
        let camera = camera_with_geometry(&pool, "ASI2600MM", Some((3.76, 6248, 4176))).await;
        let expected = sessions::fov_diagonal_deg(530.0, 3.76, 6248, 4176)
            .expect("helper must resolve a FOV for complete geometry");

        let created = train_with_camera(&pool, &camera, 530).await;
        assert_eq!(created.fov_diagonal_deg, Some(expected));

        let listed = list_optical_trains(&pool).await.unwrap();
        assert_eq!(listed[0].fov_diagonal_deg, Some(expected));

        let updated = update_optical_train(
            &pool,
            &UpdateOpticalTrain {
                id: created.id.clone(),
                name: created.name.clone(),
                telescope_id: None,
                camera_id: Some(camera.id.clone()),
                focal_length_mm: 530,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.fov_diagonal_deg, Some(expected));

        // Magnitude check, so a wrong-unit result would still fail here.
        assert!((expected - 3.054).abs() < 0.01, "expected ~3.05 deg diagonal, got {expected}");
    }

    /// A camera WITHOUT geometry yields an ABSENT FOV — not `Some(0.0)`.
    /// A fabricated zero would render as a real, degenerate field of view.
    #[tokio::test]
    async fn train_fov_absent_when_camera_lacks_geometry() {
        let pool = setup_db().await;
        let camera = camera_with_geometry(&pool, "Unknown cam", None).await;

        let created = train_with_camera(&pool, &camera, 530).await;
        assert_eq!(created.fov_diagonal_deg, None);
        assert_ne!(created.fov_diagonal_deg, Some(0.0));

        let listed = list_optical_trains(&pool).await.unwrap();
        assert_eq!(listed[0].fov_diagonal_deg, None);
    }

    /// Partial geometry is as unusable as none: a pixel size with no sensor
    /// height must not produce a FOV.
    #[tokio::test]
    async fn train_fov_absent_when_geometry_is_partial() {
        let pool = setup_db().await;
        let camera = create_camera(
            &pool,
            &CreateCamera {
                name: "Half-known".to_owned(),
                aliases: vec![],
                sensor_type: None,
                passband: None,
                pixel_size_um: Some(3.76),
                sensor_width_px: Some(6248),
                sensor_height_px: None,
            },
        )
        .await
        .unwrap();

        let train = train_with_camera(&pool, &camera, 530).await;
        assert_eq!(train.fov_diagonal_deg, None);
    }

    /// A train with no camera at all still lists, reporting no FOV.
    #[tokio::test]
    async fn train_fov_absent_when_no_camera_linked() {
        let pool = setup_db().await;
        create_optical_train(
            &pool,
            &CreateOpticalTrain {
                name: "Cameraless".to_owned(),
                telescope_id: None,
                camera_id: None,
                focal_length_mm: 530,
            },
        )
        .await
        .unwrap();

        let listed = list_optical_trains(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].fov_diagonal_deg, None);
    }

    /// Geometry round-trips through create/list/update (migration 0079).
    #[tokio::test]
    async fn camera_geometry_round_trips() {
        let pool = setup_db().await;
        let camera = camera_with_geometry(&pool, "ASI533MC", Some((3.76, 3008, 3008))).await;
        assert_eq!(camera.pixel_size_um, Some(3.76));
        assert_eq!(camera.sensor_width_px, Some(3008));
        assert_eq!(camera.sensor_height_px, Some(3008));

        let listed = list_cameras(&pool).await.unwrap();
        assert_eq!(listed[0].pixel_size_um, Some(3.76));
        assert_eq!(listed[0].sensor_height_px, Some(3008));

        // Clearing geometry must persist as absent, not as zero.
        let cleared = update_camera(
            &pool,
            &UpdateCamera {
                id: camera.id.clone(),
                name: camera.name.clone(),
                aliases: vec![],
                sensor_type: None,
                passband: None,
                pixel_size_um: None,
                sensor_width_px: None,
                sensor_height_px: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(cleared.pixel_size_um, None);
        assert_eq!(list_cameras(&pool).await.unwrap()[0].sensor_width_px, None);
    }

    /// The 0079 CHECK constraints reject a degenerate value at the storage
    /// boundary, so it can never reach the FOV computation even if a caller
    /// bypasses form validation.
    #[tokio::test]
    async fn camera_rejects_non_positive_geometry() {
        let pool = setup_db().await;
        let result = create_camera(
            &pool,
            &CreateCamera {
                name: "Zero sensor".to_owned(),
                aliases: vec![],
                sensor_type: None,
                passband: None,
                pixel_size_um: Some(3.76),
                sensor_width_px: Some(0),
                sensor_height_px: Some(4176),
            },
        )
        .await;
        assert!(result.is_err(), "sensor_width_px = 0 must violate the CHECK constraint");
    }

    // ── Filter tests ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn filter_crud() {
        let pool = setup_db().await;

        // The migration seeds predefined filters; count them.
        let seeded = list_filters(&pool).await.unwrap();
        let initial_count = seeded.len();
        assert!(initial_count > 0, "migration should seed predefined filters");

        let filter = create_filter(
            &pool,
            &CreateFilter { name: "Custom UV".to_owned(), category: FilterCategory::Custom },
        )
        .await
        .unwrap();
        assert_eq!(filter.category, FilterCategory::Custom);

        let updated = update_filter(
            &pool,
            &UpdateFilter {
                id: filter.id.clone(),
                name: "Custom UV-IR".to_owned(),
                category: FilterCategory::Other,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.name, "Custom UV-IR");
        assert_eq!(updated.category, FilterCategory::Other);

        delete_filter(&pool, &filter.id).await.unwrap();
        let after = list_filters(&pool).await.unwrap();
        assert_eq!(after.len(), initial_count);
    }
}
