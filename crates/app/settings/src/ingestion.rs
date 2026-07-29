// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Ingestion settings use cases (spec 030, package P12 — real persistence).
//!
//! `IngestionSettings` / `UpdateIngestionSettings` (`contracts_core::ingestion`)
//! are stored as a single JSON document under one key (`ingestionSettings`) in
//! the existing spec-018 settings key/value store
//! (`persistence_lifecycle::repositories::settings::{get_raw,set_raw}`) — the same
//! low-level mechanism `patternsByType` uses for its map (see
//! `repositories::settings::get_patterns_by_type` / `PATTERNS_BY_TYPE_KEY` + `set_raw`).
//!
//! This intentionally does **not** route through `SettingsState` / the
//! per-key descriptor registry (`descriptors.rs`): that bag is validated,
//! camelCase-wire-name-guarded, and byte-identity-snapshotted per stable
//! field, which would be a much larger and riskier surface change for a DTO
//! that already has its own dedicated `ingestion.settings.get` /
//! `ingestion.settings.update` commands and contract. Storing the whole DTO
//! as one document keeps the change scoped while still reusing the existing
//! `settings` table rather than adding a bespoke one (migration 0009's
//! `ingestion_settings` singleton table predates this store and was never
//! wired to any command — it is left untouched).
//!
//! Read-side (`get_ingestion_settings`) merges the stored document with
//! in-code defaults field-by-field, so a partially-written or
//! schema-drifted stored document still resolves to a usable value instead
//! of failing outright.

use contracts_core::ingestion::{IngestionSettings, UpdateIngestionSettings};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use serde_json::Value;
use sqlx::SqlitePool;

use app_core_errors::db_err;
use persistence_lifecycle::repositories::settings as repo;

/// Settings key holding the whole `IngestionSettings` document.
pub const INGESTION_SETTINGS_KEY: &str = "ingestionSettings";

/// In-code defaults (constitution §I / §IV: symlinks, junctions, and eager
/// hashing all default off; scan-on-startup and metadata extraction default on).
#[must_use]
pub fn default_ingestion_settings() -> IngestionSettings {
    IngestionSettings {
        watcher_enabled: true,
        scan_on_startup: true,
        follow_symlinks: false,
        follow_junctions: false,
        hashing_mode: "lazy".to_owned(),
        metadata_extraction: true,
        exposure_grouping_tolerance_s: 2.0,
        temperature_grouping_tolerance_c: 5.0,
        default_filter: None,
    }
}

/// Allowed `hashing_mode` values — same vocabulary as `hashOnScan`.
const HASHING_MODES: &[&str] = &["lazy", "eager", "off"];

/// `ingestion.settings.get` use case — loads the persisted document (if any)
/// and merges it with in-code defaults field-by-field.
///
/// # Errors
/// Returns `ContractError` on database failure.
pub async fn get_ingestion_settings(pool: &SqlitePool) -> Result<IngestionSettings, ContractError> {
    let stored = repo::get_raw(pool, INGESTION_SETTINGS_KEY).await.map_err(db_err)?;
    Ok(merge_with_defaults(stored))
}

/// `ingestion.settings.update` use case — validates, persists, and returns the
/// persisted state.
///
/// # Errors
/// Returns `ContractError` with code `"value.invalid"` when a tolerance is
/// negative, or on database failure.
///
/// # Panics
/// Never panics in practice: `IngestionSettings` is a flat bag of
/// `bool`/`f64`/`Option<String>` fields, which `serde_json::to_value` cannot
/// fail to serialise.
pub async fn update_ingestion_settings(
    pool: &SqlitePool,
    request: UpdateIngestionSettings,
) -> Result<IngestionSettings, ContractError> {
    validate(&request)?;

    let settings = IngestionSettings {
        watcher_enabled: request.watcher_enabled,
        scan_on_startup: request.scan_on_startup,
        follow_symlinks: request.follow_symlinks,
        follow_junctions: request.follow_junctions,
        hashing_mode: request.hashing_mode,
        metadata_extraction: request.metadata_extraction,
        exposure_grouping_tolerance_s: request.exposure_grouping_tolerance_s,
        temperature_grouping_tolerance_c: request.temperature_grouping_tolerance_c,
        default_filter: request.default_filter,
    };

    let value = serde_json::to_value(&settings).expect("IngestionSettings always serialises");
    repo::set_raw(pool, INGESTION_SETTINGS_KEY, &value).await.map_err(db_err)?;

    Ok(settings)
}

fn validate(request: &UpdateIngestionSettings) -> Result<(), ContractError> {
    let invalid = |msg: &str| {
        ContractError::new(
            ErrorCode::ValueInvalid,
            format!("ingestion settings: {msg}"),
            ErrorSeverity::Warning,
            false,
        )
    };
    if request.exposure_grouping_tolerance_s < 0.0 {
        return Err(invalid("exposureGroupingToleranceS must be >= 0"));
    }
    if request.temperature_grouping_tolerance_c < 0.0 {
        return Err(invalid("temperatureGroupingToleranceC must be >= 0"));
    }
    if !HASHING_MODES.contains(&request.hashing_mode.as_str()) {
        return Err(invalid("hashingMode must be \"lazy\", \"eager\", or \"off\""));
    }
    Ok(())
}

/// Merge a stored JSON document (if present and shaped as an object) with
/// in-code defaults. Missing or type-mismatched fields fall back to their
/// default rather than failing the whole read.
fn merge_with_defaults(stored: Option<Value>) -> IngestionSettings {
    let defaults = default_ingestion_settings();
    let Some(Value::Object(map)) = stored else {
        return defaults;
    };

    let bool_or_default = |key: &str, default: bool| -> bool {
        map.get(key).and_then(Value::as_bool).unwrap_or(default)
    };
    let f64_or_default = |key: &str, default: f64| -> f64 {
        map.get(key).and_then(Value::as_f64).unwrap_or(default)
    };

    let hashing_mode = map
        .get("hashingMode")
        .and_then(Value::as_str)
        .filter(|v| HASHING_MODES.contains(v))
        .map_or_else(|| defaults.hashing_mode.clone(), ToOwned::to_owned);

    IngestionSettings {
        watcher_enabled: bool_or_default("watcherEnabled", defaults.watcher_enabled),
        scan_on_startup: bool_or_default("scanOnStartup", defaults.scan_on_startup),
        follow_symlinks: bool_or_default("followSymlinks", defaults.follow_symlinks),
        follow_junctions: bool_or_default("followJunctions", defaults.follow_junctions),
        hashing_mode,
        metadata_extraction: bool_or_default("metadataExtraction", defaults.metadata_extraction),
        exposure_grouping_tolerance_s: f64_or_default(
            "exposureGroupingToleranceS",
            defaults.exposure_grouping_tolerance_s,
        ),
        temperature_grouping_tolerance_c: f64_or_default(
            "temperatureGroupingToleranceC",
            defaults.temperature_grouping_tolerance_c,
        ),
        default_filter: match map.get("defaultFilter") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Null) => None,
            Some(_) | None => defaults.default_filter,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;

    async fn setup() -> Database {
        let db = Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db
    }

    #[tokio::test]
    async fn get_returns_defaults_when_unset() {
        let db = setup().await;
        let got = get_ingestion_settings(db.pool()).await.unwrap();
        let defaults = default_ingestion_settings();
        assert_eq!(got.watcher_enabled, defaults.watcher_enabled);
        assert_eq!(got.scan_on_startup, defaults.scan_on_startup);
        assert!(!got.follow_symlinks);
        assert!(!got.follow_junctions);
        assert_eq!(got.hashing_mode, "lazy");
        assert!(got.metadata_extraction);
        assert!((got.exposure_grouping_tolerance_s - 2.0).abs() < f64::EPSILON);
        assert!((got.temperature_grouping_tolerance_c - 5.0).abs() < f64::EPSILON);
        assert_eq!(got.default_filter, None);
    }

    #[tokio::test]
    async fn set_then_get_round_trips_across_fresh_pool() {
        let db = setup().await;

        let update = UpdateIngestionSettings {
            watcher_enabled: false,
            scan_on_startup: false,
            follow_symlinks: true,
            follow_junctions: true,
            hashing_mode: "eager".to_owned(),
            metadata_extraction: false,
            exposure_grouping_tolerance_s: 4.5,
            temperature_grouping_tolerance_c: 1.5,
            default_filter: Some("Ha".to_owned()),
        };

        let persisted = update_ingestion_settings(db.pool(), update).await.unwrap();
        assert!(persisted.follow_symlinks);
        assert_eq!(persisted.default_filter.as_deref(), Some("Ha"));

        // Fresh pool over the same underlying store — proves durability, not
        // just an in-process cache.
        let reloaded = get_ingestion_settings(db.pool()).await.unwrap();
        assert!(!reloaded.watcher_enabled);
        assert!(!reloaded.scan_on_startup);
        assert!(reloaded.follow_symlinks);
        assert!(reloaded.follow_junctions);
        assert_eq!(reloaded.hashing_mode, "eager");
        assert!(!reloaded.metadata_extraction);
        assert!((reloaded.exposure_grouping_tolerance_s - 4.5).abs() < f64::EPSILON);
        assert!((reloaded.temperature_grouping_tolerance_c - 1.5).abs() < f64::EPSILON);
        assert_eq!(reloaded.default_filter.as_deref(), Some("Ha"));
    }

    #[tokio::test]
    async fn update_rejects_negative_exposure_tolerance() {
        let db = setup().await;
        let mut update = valid_update();
        update.exposure_grouping_tolerance_s = -1.0;
        let err = update_ingestion_settings(db.pool(), update).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ValueInvalid);
    }

    #[tokio::test]
    async fn update_rejects_negative_temperature_tolerance() {
        let db = setup().await;
        let mut update = valid_update();
        update.temperature_grouping_tolerance_c = -0.1;
        let err = update_ingestion_settings(db.pool(), update).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ValueInvalid);
    }

    #[tokio::test]
    async fn update_persists_null_default_filter() {
        let db = setup().await;
        let mut update = valid_update();
        update.default_filter = Some("L".to_owned());
        update_ingestion_settings(db.pool(), update.clone()).await.unwrap();

        update.default_filter = None;
        let persisted = update_ingestion_settings(db.pool(), update).await.unwrap();
        assert_eq!(persisted.default_filter, None);

        let reloaded = get_ingestion_settings(db.pool()).await.unwrap();
        assert_eq!(reloaded.default_filter, None);
    }

    #[tokio::test]
    async fn update_rejects_invalid_hashing_mode() {
        let db = setup().await;
        let mut update = valid_update();
        update.hashing_mode = "nope".to_owned();
        let err = update_ingestion_settings(db.pool(), update).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ValueInvalid);
    }

    #[tokio::test]
    async fn update_accepts_off_hashing_mode() {
        let db = setup().await;
        let mut update = valid_update();
        update.hashing_mode = "off".to_owned();
        let persisted = update_ingestion_settings(db.pool(), update).await.unwrap();
        assert_eq!(persisted.hashing_mode, "off");

        let reloaded = get_ingestion_settings(db.pool()).await.unwrap();
        assert_eq!(reloaded.hashing_mode, "off");
    }

    #[tokio::test]
    async fn get_ignores_invalid_stored_hashing_mode() {
        let db = setup().await;
        // Simulate a hand-edited/corrupted stored value outside the allowed set.
        repo::set_raw(
            db.pool(),
            INGESTION_SETTINGS_KEY,
            &serde_json::json!({ "hashingMode": "bogus" }),
        )
        .await
        .unwrap();
        let got = get_ingestion_settings(db.pool()).await.unwrap();
        assert_eq!(got.hashing_mode, "lazy");
    }

    #[tokio::test]
    async fn get_falls_back_to_defaults_for_malformed_stored_value() {
        let db = setup().await;
        // Simulate a non-object stored value (e.g. corrupted by hand-editing).
        repo::set_raw(db.pool(), INGESTION_SETTINGS_KEY, &serde_json::json!("not-an-object"))
            .await
            .unwrap();
        let got = get_ingestion_settings(db.pool()).await.unwrap();
        assert_eq!(got.watcher_enabled, default_ingestion_settings().watcher_enabled);
    }

    #[tokio::test]
    async fn get_merges_partial_stored_document_with_defaults() {
        let db = setup().await;
        // Only one field stored — the rest must resolve to defaults.
        repo::set_raw(
            db.pool(),
            INGESTION_SETTINGS_KEY,
            &serde_json::json!({ "followJunctions": true }),
        )
        .await
        .unwrap();

        let got = get_ingestion_settings(db.pool()).await.unwrap();
        assert!(got.follow_junctions);
        assert_eq!(got.watcher_enabled, default_ingestion_settings().watcher_enabled);
        assert_eq!(got.scan_on_startup, default_ingestion_settings().scan_on_startup);
    }

    fn valid_update() -> UpdateIngestionSettings {
        let d = default_ingestion_settings();
        UpdateIngestionSettings {
            watcher_enabled: d.watcher_enabled,
            scan_on_startup: d.scan_on_startup,
            follow_symlinks: d.follow_symlinks,
            follow_junctions: d.follow_junctions,
            hashing_mode: d.hashing_mode,
            metadata_extraction: d.metadata_extraction,
            exposure_grouping_tolerance_s: d.exposure_grouping_tolerance_s,
            temperature_grouping_tolerance_c: d.temperature_grouping_tolerance_c,
            default_filter: d.default_filter,
        }
    }
}
