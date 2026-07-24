// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for settings storage (spec 018, T003).
//!
//! Operates on the `settings` and `source_overrides` tables from migration 0013.
//! Each settings key is stored as one row with a JSON-encoded value.

use std::collections::BTreeMap;

use domain_core::ids::Timestamp;
use domain_core::settings::{SettingsState, SourceOverride};
use patterns::{default_pattern, validate_pattern_str, FrameTypeClass};
use serde_json::Value;
use sqlx::types::Json;
use sqlx::SqliteConnection;
use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

/// Settings key holding the per-frame-type destination pattern overrides
/// (spec 041 FR-026b). Stored as a JSON object mapping a [`FrameTypeClass`]
/// name to a pattern string. Only explicit overrides are persisted; missing
/// entries fall back to [`default_pattern`] on read.
pub const PATTERNS_BY_TYPE_KEY: &str = "patternsByType";

// ── Helpers ──────────────────────────────────────────────────────────────

// ── Low-level key/value operations ────────────────────────────────────────

/// Read the raw JSON value for a single key. Returns `None` when no row exists.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn get_raw(pool: &SqlitePool, key: &str) -> DbResult<Option<Value>> {
    let row: Option<(Json<Value>,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|(Json(v),)| v))
}

/// Write (upsert) a raw JSON value for a single key.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
/// Returns [`DbError::Serialise`] if the value cannot be serialised.
pub async fn set_raw(pool: &SqlitePool, key: &str, value: &Value) -> DbResult<()> {
    let now = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(Json(value))
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Write (upsert) a raw JSON value for a single key on an existing connection.
///
/// Accepts a bare `SqliteConnection` so callers can participate in an
/// open transaction without needing a pool.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn set_raw_with_conn(
    conn: &mut SqliteConnection,
    key: &str,
    value: &Value,
) -> DbResult<()> {
    let now = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(Json(value))
    .bind(&now)
    .execute(conn)
    .await?;

    Ok(())
}

/// Delete a stored row for a key (used by repair to reset to default).
///
/// Idempotent — no error if the key does not exist.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn delete_key(pool: &SqlitePool, key: &str) -> DbResult<()> {
    sqlx::query("DELETE FROM settings WHERE key = ?").bind(key).execute(pool).await?;
    Ok(())
}

/// Read all stored settings as a list of (key, JSON value) pairs.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn get_all_raw(pool: &SqlitePool) -> DbResult<Vec<(String, Value)>> {
    let rows: Vec<(String, Json<Value>)> =
        sqlx::query_as("SELECT key, value FROM settings ORDER BY key ASC").fetch_all(pool).await?;

    Ok(rows.into_iter().map(|(key, Json(v))| (key, v)).collect())
}

/// Read all stored rows whose key starts with `prefix` (e.g. `"uiState."`).
///
/// Returns only rows that exist in the database; missing keys carry no default
/// (the caller decides the fallback). Used by the `ui_state` scope to batch-
/// read all persisted UI state keys without enumerating them at the Rust layer.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn get_all_by_prefix(pool: &SqlitePool, prefix: &str) -> DbResult<Vec<(String, Value)>> {
    // SQLite LIKE pattern: append '%' to the prefix. The prefix itself may
    // contain literal '%' or '_' — escape them so they are matched literally.
    let pattern = format!("{}%", prefix.replace('%', "\\%").replace('_', "\\_"));
    let rows: Vec<(String, Json<Value>)> = sqlx::query_as(
        "SELECT key, value FROM settings WHERE key LIKE ? ESCAPE '\\' ORDER BY key ASC",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(key, Json(v))| (key, v)).collect())
}

// ── High-level settings bag ───────────────────────────────────────────────

/// Load the full settings state, merging stored rows with in-code defaults.
///
/// Any key not present in the database is returned at its default value.
/// Caller is responsible for validation and repair (T019).
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn load_settings(pool: &SqlitePool) -> DbResult<SettingsState> {
    let stored = get_all_raw(pool).await?;
    merge_with_defaults(stored)
}

fn merge_with_defaults(stored: Vec<(String, Value)>) -> DbResult<SettingsState> {
    let mut state = SettingsState::default();
    for (key, value) in stored {
        apply_key_to_state(&key, value, &mut state)?;
    }
    Ok(state)
}

/// Generates `apply_key_to_state` from a flat (json_key, field) table.
macro_rules! settings_key_table {
    ( $( $json_key:expr => $field:ident ),+ $(,)? ) => {
        /// Apply a single stored key/value pair to a `SettingsState`.
        ///
        /// Returns `Err(DbError::Serialise)` if the stored JSON cannot be
        /// deserialised into the expected Rust type for that key.
        fn apply_key_to_state(key: &str, value: Value, state: &mut SettingsState) -> DbResult<()> {
            match key {
                $( $json_key => {
                    state.$field = serde_json::from_value(value).map_err(DbError::Serialise)?;
                } )+
                _ => {
                    // Structured-path keys (tools.*, workflow_profile.*) are not in the
                    // static SettingsState bag; they are readable via resolve_setting.
                }
            }
            Ok(())
        }

        /// All JSON key strings handled by `apply_key_to_state` (for parity tests).
        #[cfg(test)]
        const APPLY_KEYS: &[&str] = &[ $( $json_key ),+ ];
    };
}

settings_key_table! {
    "pattern" => pattern,
    "autoApplyPattern" => auto_apply_pattern,
    "alwaysPreviewBeforePlan" => always_preview_before_plan,
    "followSymlinks" => follow_symlinks,
    "hashOnScan" => hash_on_scan,
    "darkMatchTolerance" => dark_match_tolerance,
    "flatMatching" => flat_matching,
    "suggestCalibration" => suggest_calibration,
    "logLevel" => log_level,
    "rememberFollowLogs" => remember_follow_logs,
    "defaultProtection" => default_protection,
    "blockPermanentDelete" => block_permanent_delete,
    "protectedCategories" => protected_categories,
    "currentLibraryId" => current_library_id,
    "devMode" => dev_mode,
    "calibrationDarkTempTolerance" => calibration_dark_temp_tolerance,
    "calibrationPrefillSuggestion" => calibration_prefill_suggestion,
    "calibrationDarkOverridePenalty" => calibration_dark_override_penalty,
    "calibrationFlatOverridePenalty" => calibration_flat_override_penalty,
    "calibrationBiasOverridePenalty" => calibration_bias_override_penalty,
    "calibrationAgingThresholdDays" => calibration_aging_threshold_days,
    "imagetypNormalizationUserMappings" => imagetyp_normalization_user_mappings,
    "patternsByType" => patterns_by_type,
    "toolWatchExtensions" => tool_watch_extensions,
    "toolAttributionWindowHours" => tool_attribution_window_hours,
    "observingSites" => observing_sites,
    "observingDefaultSiteId" => observing_default_site_id,
    "observingActiveSiteId" => observing_active_site_id,
    "usableAltitudeDeg" => usable_altitude_deg,
    "plannerMoonAvoidance" => planner_moon_avoidance,
    "sourceViewLinkKindIntraDrive" => source_view_link_kind_intra_drive,
    "sourceViewLinkKindCrossDrive" => source_view_link_kind_cross_drive,
    "cleanupTypeOverrides" => cleanup_type_overrides,
    "theme" => theme,
    "framingPointingFractionOfFov" => framing_pointing_fraction_of_fov,
    "framingPointingFallbackDeg" => framing_pointing_fallback_deg,
    "framingRotationToleranceDeg" => framing_rotation_tolerance_deg,
    "framingMosaicEnvelopeFractionOfFov" => framing_mosaic_envelope_fraction_of_fov,
}

// ── Per-frame-type destination patterns (spec 041 FR-026b) ────────────────
//
// Storage choice: set/reset operations **validate via the patterns crate before
// storing** (no garbage is ever persisted). The getter *also* falls back to the
// built-in default on read for empty/invalid entries, so a hand-edited or
// migrated DB with a bad value still resolves to a usable pattern.

/// Read the stored per-type pattern override map (only explicit overrides;
/// missing classes are absent). Returns an empty map when unset.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure or [`DbError::Serialise`] if
/// the stored value is not a string→string object.
pub async fn get_patterns_by_type(pool: &SqlitePool) -> DbResult<BTreeMap<String, String>> {
    match get_raw(pool, PATTERNS_BY_TYPE_KEY).await? {
        None => Ok(BTreeMap::new()),
        Some(v) => serde_json::from_value(v).map_err(DbError::Serialise),
    }
}

/// Return the effective destination pattern for a `(frame_type, is_master)`
/// pair: the stored override when present, non-empty, and valid; otherwise the
/// built-in [`default_pattern`].
///
/// Returns `None` only when `frame_type` does not map to a known
/// [`FrameTypeClass`] (e.g. an unclassified frame). Callers (spec 041 confirm,
/// T052) treat `None` as "no destination pattern" and surface it via the
/// needs-review flow.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure or [`DbError::Serialise`] if
/// the stored override map is malformed.
pub async fn effective_pattern_for(
    pool: &SqlitePool,
    frame_type: &str,
    is_master: bool,
) -> DbResult<Option<String>> {
    let Some(class) = patterns::classify_frame(frame_type, is_master) else {
        return Ok(None);
    };
    let overrides = get_patterns_by_type(pool).await?;
    let effective = overrides
        .get(class.as_str())
        .map(String::as_str)
        .filter(|p| validate_pattern_str(p).is_ok())
        .map_or_else(|| default_pattern(class).to_owned(), ToOwned::to_owned);
    Ok(Some(effective))
}

/// Set the destination pattern override for a single frame-type class.
///
/// The pattern is validated via [`validate_pattern_str`] before storage; an
/// invalid (or empty) pattern is rejected with [`DbError::Serialise`] rather
/// than persisted. To revert a class to its default, use [`reset_pattern_for`].
///
/// # Errors
///
/// Returns [`DbError::Serialise`] when `pattern` fails validation, or
/// [`DbError::Database`] on query failure.
pub async fn set_pattern_for(
    pool: &SqlitePool,
    class: FrameTypeClass,
    pattern: &str,
) -> DbResult<()> {
    validate_pattern_str(pattern).map_err(|e| {
        let err: serde_json::Error =
            serde::de::Error::custom(format!("invalid pattern for {}: {e}", class.as_str()));
        DbError::Serialise(err)
    })?;
    let mut overrides = get_patterns_by_type(pool).await?;
    overrides.insert(class.as_str().to_owned(), pattern.to_owned());
    let value = serde_json::to_value(&overrides).map_err(DbError::Serialise)?;
    set_raw(pool, PATTERNS_BY_TYPE_KEY, &value).await
}

/// Reset a single frame-type class to its built-in default by removing its
/// override entry. Idempotent — no error if no override exists.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure or [`DbError::Serialise`] if
/// the stored override map is malformed.
pub async fn reset_pattern_for(pool: &SqlitePool, class: FrameTypeClass) -> DbResult<()> {
    let mut overrides = get_patterns_by_type(pool).await?;
    if overrides.remove(class.as_str()).is_none() {
        return Ok(());
    }
    let value = serde_json::to_value(&overrides).map_err(DbError::Serialise)?;
    set_raw(pool, PATTERNS_BY_TYPE_KEY, &value).await
}

// ── Per-source overrides ──────────────────────────────────────────────────

/// Upsert a per-source override.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn set_source_override(
    pool: &SqlitePool,
    source_id: &str,
    key: &str,
    value: &Value,
) -> DbResult<()> {
    let now = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO source_overrides (source_id, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(source_id, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(source_id)
    .bind(key)
    .bind(Json(value))
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get the raw JSON override value for a specific source + key, if any.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn get_source_override_raw(
    pool: &SqlitePool,
    source_id: &str,
    key: &str,
) -> DbResult<Option<Value>> {
    let row: Option<(Json<Value>,)> =
        sqlx::query_as("SELECT value FROM source_overrides WHERE source_id = ? AND key = ?")
            .bind(source_id)
            .bind(key)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|(Json(v),)| v))
}

/// List all source overrides for a given source.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn list_source_overrides(
    pool: &SqlitePool,
    source_id: &str,
) -> DbResult<Vec<SourceOverride>> {
    let rows: Vec<(String, Json<Value>, String)> = sqlx::query_as(
        "SELECT key, value, updated_at FROM source_overrides WHERE source_id = ? ORDER BY key ASC",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(key, Json(v), updated_at)| SourceOverride {
            source_id: source_id.to_owned(),
            key,
            value: domain_core::JsonAny::from(v),
            updated_at,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::setup_db;

    #[tokio::test]
    async fn get_raw_returns_none_for_missing_key() {
        let db = setup_db().await;
        let result = get_raw(db.pool(), "nonexistent_key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn set_and_get_raw_roundtrip() {
        let db = setup_db().await;
        let value = serde_json::json!("info");
        set_raw(db.pool(), "logLevel", &value).await.unwrap();
        let loaded = get_raw(db.pool(), "logLevel").await.unwrap();
        assert_eq!(loaded, Some(value));
    }

    /// Round-trips a nested object/array shape through the `sqlx::types::Json`
    /// column codec (spec `n4_jsoncodec`) — not just a JSON scalar.
    #[tokio::test]
    async fn set_and_get_raw_roundtrip_nested_value() {
        let db = setup_db().await;
        let value = serde_json::json!({
            "patterns": ["a/{target}/", "b/{filter}/"],
            "nested": { "enabled": true, "count": 3 },
        });
        set_raw(db.pool(), "patternsByType", &value).await.unwrap();
        let loaded = get_raw(db.pool(), "patternsByType").await.unwrap();
        assert_eq!(loaded, Some(value));
    }

    /// A cell that predates or bypasses `set_raw` (hand-edited DB, corrupted
    /// disk) with syntactically invalid JSON must fail the read, not silently
    /// substitute a default — `get_raw` is a strict-propagate site (spec
    /// `n4_jsoncodec`: distinct from the two named lenient sites in
    /// `equipment.rs`/`artifacts.rs`).
    #[tokio::test]
    async fn get_raw_propagates_on_corrupt_cell() {
        let db = setup_db().await;
        sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)")
            .bind("corrupt")
            .bind("not valid json")
            .bind(Timestamp::now_iso())
            .execute(db.pool())
            .await
            .unwrap();

        let result = get_raw(db.pool(), "corrupt").await;
        assert!(result.is_err(), "corrupt JSON cell must error, not degrade");
    }

    #[tokio::test]
    async fn set_raw_upserts_on_conflict() {
        let db = setup_db().await;
        set_raw(db.pool(), "logLevel", &serde_json::json!("info")).await.unwrap();
        set_raw(db.pool(), "logLevel", &serde_json::json!("debug")).await.unwrap();
        let loaded = get_raw(db.pool(), "logLevel").await.unwrap();
        assert_eq!(loaded, Some(serde_json::json!("debug")));
    }

    #[tokio::test]
    async fn load_settings_honors_stored_source_view_link_kind_overrides() {
        // Spec 049 T006/US2 regression: `load_settings` (used directly by
        // `sourceview.generate`, not just the `settings.update` Tauri command
        // path) must apply a stored override for these two keys instead of
        // silently keeping the in-code default (found via the US3
        // regeneration integration test, which needs a non-default kind to
        // exercise the "hardlink unsupported" refusal path).
        let db = setup_db().await;
        set_raw(db.pool(), "sourceViewLinkKindIntraDrive", &serde_json::json!("symlink"))
            .await
            .unwrap();
        set_raw(db.pool(), "sourceViewLinkKindCrossDrive", &serde_json::json!("junction"))
            .await
            .unwrap();

        let loaded = load_settings(db.pool()).await.unwrap();
        assert_eq!(loaded.source_view_link_kind_intra_drive, "symlink");
        assert_eq!(loaded.source_view_link_kind_cross_drive, "junction");
    }

    #[tokio::test]
    async fn delete_key_removes_stored_row() {
        let db = setup_db().await;
        set_raw(db.pool(), "logLevel", &serde_json::json!("debug")).await.unwrap();
        delete_key(db.pool(), "logLevel").await.unwrap();
        let loaded = get_raw(db.pool(), "logLevel").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn load_settings_returns_defaults_when_empty() {
        let db = setup_db().await;
        let state = load_settings(db.pool()).await.unwrap();
        let defaults = SettingsState::default();
        assert_eq!(state.log_level, defaults.log_level);
        assert_eq!(state.follow_symlinks, defaults.follow_symlinks);
        assert_eq!(state.hash_on_scan, defaults.hash_on_scan);
    }

    #[tokio::test]
    async fn load_settings_applies_stored_values() {
        let db = setup_db().await;
        set_raw(db.pool(), "logLevel", &serde_json::json!("debug")).await.unwrap();
        set_raw(db.pool(), "followSymlinks", &serde_json::json!(true)).await.unwrap();
        let state = load_settings(db.pool()).await.unwrap();
        assert_eq!(state.log_level, "debug");
        assert!(state.follow_symlinks);
    }

    /// F-Framing-11 (R11a): `load_settings` (the path `attribution.rs`'s
    /// `tolerance_params` reads through directly) must honour a stored
    /// override for the clustering tunables, not silently drop it via the
    /// `apply_key_to_state` catch-all.
    #[tokio::test]
    async fn load_settings_applies_stored_framing_tolerance_overrides() {
        let db = setup_db().await;
        let defaults = SettingsState::default();
        let state = load_settings(db.pool()).await.unwrap();
        assert!(
            (state.framing_pointing_fraction_of_fov - defaults.framing_pointing_fraction_of_fov)
                .abs()
                < f64::EPSILON
        );

        set_raw(db.pool(), "framingPointingFractionOfFov", &serde_json::json!(0.33)).await.unwrap();
        set_raw(db.pool(), "framingPointingFallbackDeg", &serde_json::json!(0.4)).await.unwrap();
        set_raw(db.pool(), "framingRotationToleranceDeg", &serde_json::json!(6.0)).await.unwrap();
        set_raw(db.pool(), "framingMosaicEnvelopeFractionOfFov", &serde_json::json!(1.25))
            .await
            .unwrap();

        let state = load_settings(db.pool()).await.unwrap();
        assert!((state.framing_pointing_fraction_of_fov - 0.33).abs() < f64::EPSILON);
        assert!((state.framing_pointing_fallback_deg - 0.4).abs() < f64::EPSILON);
        assert!((state.framing_rotation_tolerance_deg - 6.0).abs() < f64::EPSILON);
        assert!((state.framing_mosaic_envelope_fraction_of_fov - 1.25).abs() < f64::EPSILON);
    }

    // ── Per-frame-type patterns ───────────────────────────────────────────

    #[tokio::test]
    async fn patterns_by_type_defaults_when_unset() {
        let db = setup_db().await;
        // No overrides stored → every class resolves to its built-in default,
        // reached via the raw (frame_type, is_master) inputs confirm.rs passes.
        for (raw_type, is_master, class) in raw_inputs_per_class() {
            let got = effective_pattern_for(db.pool(), raw_type, is_master).await.unwrap();
            assert_eq!(got.as_deref(), Some(default_pattern(class)));
        }
        assert!(get_patterns_by_type(db.pool()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn patterns_by_type_override_read_back() {
        let db = setup_db().await;
        set_pattern_for(db.pool(), FrameTypeClass::Dark, "custom/{gain}/").await.unwrap();
        let got = effective_pattern_for(db.pool(), "dark", false).await.unwrap();
        assert_eq!(got.as_deref(), Some("custom/{gain}/"));
        // Other classes are untouched.
        let flat = effective_pattern_for(db.pool(), "flat", false).await.unwrap();
        assert_eq!(flat.as_deref(), Some(default_pattern(FrameTypeClass::Flat)));
    }

    #[tokio::test]
    async fn patterns_by_type_master_routing() {
        let db = setup_db().await;
        set_pattern_for(db.pool(), FrameTypeClass::MasterDark, "m/{exposure}/").await.unwrap();
        // is_master = true selects the master class.
        let master = effective_pattern_for(db.pool(), "dark", true).await.unwrap();
        assert_eq!(master.as_deref(), Some("m/{exposure}/"));
        // raw dark still defaults.
        let raw = effective_pattern_for(db.pool(), "dark", false).await.unwrap();
        assert_eq!(raw.as_deref(), Some(default_pattern(FrameTypeClass::Dark)));
    }

    #[tokio::test]
    async fn set_pattern_rejects_invalid() {
        let db = setup_db().await;
        assert!(set_pattern_for(db.pool(), FrameTypeClass::Bias, "{telescope}/").await.is_err());
        assert!(set_pattern_for(db.pool(), FrameTypeClass::Bias, "").await.is_err());
        // Nothing persisted on rejection → still the default.
        let got = effective_pattern_for(db.pool(), "bias", false).await.unwrap();
        assert_eq!(got.as_deref(), Some(default_pattern(FrameTypeClass::Bias)));
    }

    #[tokio::test]
    async fn stored_invalid_override_falls_back_on_read() {
        // Defensive read-side fallback: a malformed value written out-of-band
        // (e.g. hand-edited DB) must not surface a broken pattern.
        let db = setup_db().await;
        let mut map = BTreeMap::new();
        map.insert("bias".to_owned(), "{telescope}/".to_owned());
        set_raw(db.pool(), PATTERNS_BY_TYPE_KEY, &serde_json::to_value(&map).unwrap())
            .await
            .unwrap();
        let got = effective_pattern_for(db.pool(), "bias", false).await.unwrap();
        assert_eq!(got.as_deref(), Some(default_pattern(FrameTypeClass::Bias)));
    }

    #[tokio::test]
    async fn reset_pattern_restores_default() {
        let db = setup_db().await;
        set_pattern_for(db.pool(), FrameTypeClass::Light, "{target}/x/").await.unwrap();
        reset_pattern_for(db.pool(), FrameTypeClass::Light).await.unwrap();
        let got = effective_pattern_for(db.pool(), "light", false).await.unwrap();
        assert_eq!(got.as_deref(), Some(default_pattern(FrameTypeClass::Light)));
        // reset is idempotent.
        reset_pattern_for(db.pool(), FrameTypeClass::Light).await.unwrap();
    }

    #[tokio::test]
    async fn effective_pattern_for_unknown_type_is_none() {
        let db = setup_db().await;
        assert!(effective_pattern_for(db.pool(), "unclassified", false).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn load_settings_applies_patterns_by_type() {
        let db = setup_db().await;
        set_pattern_for(db.pool(), FrameTypeClass::Flat, "f/{filter}/").await.unwrap();
        let state = load_settings(db.pool()).await.unwrap();
        assert_eq!(state.patterns_by_type.get("flat").map(String::as_str), Some("f/{filter}/"));
    }

    /// Raw `(frame_type, is_master)` inputs that map to each class, as
    /// `classify_frame` expects them (it takes raw header types, not class names).
    fn raw_inputs_per_class() -> [(&'static str, bool, FrameTypeClass); 7] {
        [
            ("light", false, FrameTypeClass::Light),
            ("flat", false, FrameTypeClass::Flat),
            ("dark", false, FrameTypeClass::Dark),
            ("bias", false, FrameTypeClass::Bias),
            ("flat", true, FrameTypeClass::MasterFlat),
            ("dark", true, FrameTypeClass::MasterDark),
            ("bias", true, FrameTypeClass::MasterBias),
        ]
    }

    #[tokio::test]
    async fn source_override_roundtrip() {
        let db = setup_db().await;
        let value = serde_json::json!("eager");
        set_source_override(db.pool(), "source-abc", "hashOnScan", &value).await.unwrap();
        let loaded = get_source_override_raw(db.pool(), "source-abc", "hashOnScan").await.unwrap();
        assert_eq!(loaded, Some(value));
    }

    #[tokio::test]
    async fn source_override_upsert_updates_value() {
        let db = setup_db().await;
        set_source_override(db.pool(), "src-1", "followSymlinks", &serde_json::json!(false))
            .await
            .unwrap();
        set_source_override(db.pool(), "src-1", "followSymlinks", &serde_json::json!(true))
            .await
            .unwrap();
        let loaded = get_source_override_raw(db.pool(), "src-1", "followSymlinks").await.unwrap();
        assert_eq!(loaded, Some(serde_json::json!(true)));
    }

    /// C-12 parity guard: `APPLY_KEYS` (the persistence hydration table) must
    /// match the SettingsState wire field names.
    #[test]
    fn apply_keys_cover_all_settings_state_wire_fields() {
        let state = SettingsState {
            current_library_id: Some("__probe__".to_owned()),
            observing_default_site_id: Some("__probe__".to_owned()),
            observing_active_site_id: Some("__probe__".to_owned()),
            ..SettingsState::default()
        };
        let state_json = serde_json::to_value(&state).expect("SettingsState serialises");
        let wire_fields: std::collections::BTreeSet<&str> =
            state_json.as_object().unwrap().keys().map(String::as_str).collect();
        let apply_set: std::collections::BTreeSet<&str> = APPLY_KEYS.iter().copied().collect();

        for field in &wire_fields {
            assert!(
                apply_set.contains(field),
                "SettingsState wire field '{field}' missing from apply_key_to_state table;                  add it to the settings_key_table! macro in this file"
            );
        }
        for key in &apply_set {
            assert!(
                wire_fields.contains(key),
                "apply_key_to_state handles '{key}' but it is not a SettingsState wire field;                  remove it from the settings_key_table! macro"
            );
        }
    }
}

// ── SettingsState / SourceOverride wire-shape tests (spec 042 T254) ─────
//
// T254 moved the stored settings types (`SettingsState`, `SourceOverride`,
// `PatternPart`, `ImageTypMapping`) from `contracts_core` into `domain_core`
// to fix the `persistence/db → contracts/core` layering inversion.
//
// These tests assert *behavior* (round-trip fidelity, specific field wire
// names/defaults that other code depends on) rather than pinning a frozen
// byte-for-byte JSON snapshot of the whole struct — a full-struct frozen
// literal must be hand-retyped on every new settings field, which is exactly
// the kind of busywork this module intentionally avoids (product decision,
// spec 051 T007 follow-up: the byte-identity guard previously here was
// removed for this reason).
#[cfg(test)]
mod settings_state_shape {
    use domain_core::settings::{SettingsState, SourceOverride};
    use domain_core::JsonAny;

    use super::*;
    use persistence_core::Database;

    /// `SettingsState::default()` round-trips through JSON with no loss —
    /// proves serde field names/shapes are self-consistent without needing a
    /// frozen snapshot of the exact bytes.
    #[test]
    fn settings_state_default_round_trips_through_json() {
        let state = SettingsState::default();
        let value = serde_json::to_value(&state).unwrap();
        let parsed: SettingsState = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, state);
    }

    /// Targeted wire-contract assertion for the field added by spec 051 T007:
    /// `cleanup_type_overrides` must serialize under the camelCase key
    /// `cleanupTypeOverrides` and default to an empty object (data-model.md
    /// §E2 — absent id ⇒ that type's built-in default action applies).
    #[test]
    fn cleanup_type_overrides_wire_key_and_default() {
        let value = serde_json::to_value(SettingsState::default()).unwrap();
        assert_eq!(value["cleanupTypeOverrides"], serde_json::json!({}));
    }

    /// `SourceOverride` round-trips through JSON with no loss.
    #[test]
    fn source_override_round_trips_through_json() {
        let ov = SourceOverride {
            source_id: "src-1".to_owned(),
            key: "hashOnScan".to_owned(),
            value: JsonAny::from(serde_json::json!("eager")),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        let value = serde_json::to_value(&ov).unwrap();
        let parsed: SourceOverride = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, ov);
    }

    /// Round-trip through the real `settings` SQL table: write each known key,
    /// reload via `load_settings`, and assert the hydrated state equals the
    /// values we stored. Proves the column mapping is unchanged end-to-end.
    #[tokio::test]
    async fn settings_store_roundtrip_preserves_bytes() {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let pool = db.pool();

        // Persist a representative spread across string / bool / number / array
        // keys, using the exact stored JSON encodings.
        set_raw(pool, "hashOnScan", &serde_json::json!("eager")).await.unwrap();
        set_raw(pool, "followSymlinks", &serde_json::json!(true)).await.unwrap();
        set_raw(pool, "logLevel", &serde_json::json!("debug")).await.unwrap();
        set_raw(pool, "calibrationDarkTempTolerance", &serde_json::json!(3.5)).await.unwrap();
        set_raw(pool, "protectedCategories", &serde_json::json!(["lights", "masters"]))
            .await
            .unwrap();

        // The raw column bytes must equal exactly what we stored.
        assert_eq!(get_raw(pool, "hashOnScan").await.unwrap(), Some(serde_json::json!("eager")));
        assert_eq!(get_raw(pool, "followSymlinks").await.unwrap(), Some(serde_json::json!(true)));
        assert_eq!(
            get_raw(pool, "protectedCategories").await.unwrap(),
            Some(serde_json::json!(["lights", "masters"]))
        );

        // The hydrated typed state must reflect those stored values.
        let state = load_settings(pool).await.unwrap();
        assert_eq!(state.hash_on_scan, "eager");
        assert!(state.follow_symlinks);
        assert_eq!(state.log_level, "debug");
        assert!((state.calibration_dark_temp_tolerance - 3.5).abs() < f64::EPSILON); // stored as calibrationDarkTempTolerance
        assert_eq!(state.protected_categories, vec!["lights".to_owned(), "masters".to_owned()]);
    }
}
