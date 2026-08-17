// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Settings Tauri commands (spec 018, T014/T015).
//!
//! ## Stable transport (T015)
//!
//! The frontend calls two stable commands that use a `scope/values` shape:
//!
//! - `settings.get { scope } -> SettingsData { scope, values }` — reads all
//!   keys that belong to `scope`, returns them as a flat JSON object.
//! - `settings.update { scope, values }` — persists every key in `values`
//!   using the key names as settings keys (validated per-key by the use case).
//!
//! The scope groups keys by workflow area:
//! - `"advanced"` → `logLevel`, `rememberFollowLogs`, `devMode`
//! - `"general"`  → `theme`, `locale` (durable sources of truth for UI
//!   appearance and language; the frontend keeps synchronous localStorage
//!   boot mirrors, reconciled from this scope at startup)
//! - `"cleanup"`  → `blockPermanentDelete`, `defaultProtection`,
//!   `protectedCategories`, `cleanupTypeOverrides`
//! - `"naming"`   → `pattern`, `autoApplyPattern`, `patternsByType`
//! - `"sources"`  → `followSymlinks`, `hashOnScan`
//! - `"calibration"` → `darkMatchTolerance`, `flatMatching`, `suggestCalibration`,
//!   `calibrationDarkTempTolerance`, `calibrationPrefillSuggestion`
//! - `"sourceViews"` → `sourceViewLinkKindIntraDrive`, `sourceViewLinkKindCrossDrive`
//!   (spec 049 US2 T029 — read-only display in the generation dialog; the
//!   editable Settings pane for these two keys is spec 049 T030,
//!   `apps/desktop/src/features/settings/SourceViews.tsx`)
//! - `"framing"` → `framingPointingFractionOfFov`, `framingPointingFallbackDeg`,
//!   `framingRotationToleranceDeg`, `framingMosaicEnvelopeFractionOfFov`
//!   (spec 008 Q27 F-Framing-11, R11a clustering tolerance tunables)
//! - `"catalogues"` → `enabled` (#645, default-enabled Planner catalogues)
//! - `"ui_state"` → all stored rows with the `uiState.` key prefix; no in-code
//!   defaults (2026-07-24 persisted-ui-state migration).
//! - `""` (empty) → reads the full settings bag (every descriptor key, derived
//!   from the descriptor table so it cannot drift, plus `enabled`).
//!
//! Unknown `values` keys from the frontend that are not valid settings keys are
//! silently skipped (best-effort write) so fixture-driven panes can call
//! `save(scope, {...})` without causing errors.
//!
//! ## Additional commands
//!
//! Richer per-key surface exposed as additional commands (do not disturb the
//! stable transport above):
//! - `settings.restore-defaults`
//! - `settings.source-override.set`

use std::sync::LazyLock;

use contracts_core::settings::{
    RestoreDefaultsRequest, RestoreDefaultsResponse, SetSourceOverrideRequest,
    SetSourceOverrideResponse, SettingsData, SettingsUpdateRequest,
};
use serde_json::Value;
use tauri::State;

use crate::commands::lifecycle::AppState;
use contracts_core::ContractError;

// ── Scope → key mapping ───────────────────────────────────────────────────────

/// Keys owned by each scope (for `settings.get`).
///
/// The `""` / catch-all case returns all known stable keys.
fn scope_keys(scope: &str) -> &'static [&'static str] {
    match scope {
        "advanced" => &["logLevel", "rememberFollowLogs", "devMode"],
        "general" => &["theme", "locale"],
        "cleanup" => &[
            "blockPermanentDelete",
            "defaultProtection",
            "protectedCategories",
            "cleanupTypeOverrides",
        ],
        "naming" => &["pattern", "autoApplyPattern", "patternsByType"],
        "sources" => &["followSymlinks", "hashOnScan", "alwaysPreviewBeforePlan"],
        "calibration" => &[
            "darkMatchTolerance",
            "flatMatching",
            "suggestCalibration",
            "calibrationDarkTempTolerance",
            "calibrationPrefillSuggestion",
            "calibrationDarkOverridePenalty",
            "calibrationFlatOverridePenalty",
            "calibrationBiasOverridePenalty",
            "calibrationAgingThresholdDays",
        ],
        "sourceViews" => &["sourceViewLinkKindIntraDrive", "sourceViewLinkKindCrossDrive"],
        "observing" => &[
            "observingSites",
            "observingDefaultSiteId",
            "observingActiveSiteId",
            "usableAltitudeDeg",
        ],
        "planner" => &["plannerMoonAvoidance"],
        // Default-enabled planner catalogues (#645); key name `enabled` is
        // dictated by the frontend contract (`catalogue-settings.ts`), which
        // reads `settingsGet('catalogues').values.enabled` directly.
        "catalogues" => &["enabled"],
        "framing" => &[
            "framingPointingFractionOfFov",
            "framingPointingFallbackDeg",
            "framingRotationToleranceDeg",
            "framingMosaicEnvelopeFractionOfFov",
        ],
        // UI-state scope — keys with the `uiState.` prefix are open-ended;
        // `settings.get("ui_state")` returns the empty set because the caller
        // must call `settings.get` with the exact key names it registered.
        // The scope exists only so `settings.update` routes here without
        // tracing noise about an "unknown scope" (the catch-all `_ => ALL_KEYS`
        // would still accept the writes, but returning all stable keys on a
        // `settings.get("ui_state")` call would be surprising). Individual key
        // reads happen via the `""` (catch-all) scope or via direct `hydrateScope`.
        "ui_state" => &[],
        // Empty scope or "global" returns every stable key.
        _ => &ALL_KEYS,
    }
}

/// Every key the catch-all scope resolves: the descriptor table plus the
/// structured-path keys that have no descriptor.
///
/// Derived rather than hand-listed because the hand-maintained list drifted
/// from the descriptor table and left four writable keys permanently
/// unreadable (#641).
static ALL_KEYS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut keys = app_core::settings::stable_keys();
    // `enabled` (#645) is validated via `is_catalogues_enabled_key`, not a
    // descriptor, so it is not covered by `stable_keys()`.
    keys.push("enabled");
    keys
});

// ── Stable transport commands ────────────────────────────────────────────────

/// `settings.get` — returns settings for a given scope.
///
/// Accepts `{ scope: string }` and returns `SettingsData { scope, values }`.
/// Each key that belongs to the scope is resolved via the persistence layer
/// (hydrating the in-code default when no stored row exists).
///
/// The `"ui_state"` scope is special: it returns all rows whose key starts with
/// `"uiState."` directly from the DB (no in-code default — unknown keys simply
/// return `null` on a get; the frontend falls back to its own default).
///
/// # Errors
/// Returns `Err(String)` on database failure.
#[tauri::command]
#[specta::specta]
pub async fn settings_get(
    state: State<'_, AppState>,
    scope: String,
) -> Result<SettingsData, ContractError> {
    tracing::debug!("settings.get scope={scope}");
    let pool = state.repo.pool();

    // UI-state scope: return all stored `uiState.*` rows (no in-code defaults).
    if scope == "ui_state" {
        let rows =
            persistence_lifecycle::repositories::settings::get_all_by_prefix(pool, "uiState.")
                .await
                .map_err(app_core::errors::db_to_contract)?;
        let map: serde_json::Map<String, Value> = rows.into_iter().collect();
        return Ok(SettingsData { scope, values: contracts_core::JsonAny(Value::Object(map)) });
    }

    let keys = scope_keys(&scope);
    let mut map = serde_json::Map::with_capacity(keys.len());
    for key in keys {
        let val = app_core::settings::resolve_setting(pool, key, None).await?;
        map.insert((*key).to_owned(), val);
    }

    Ok(SettingsData { scope, values: contracts_core::JsonAny(Value::Object(map)) })
}

/// `settings.update` — persists settings values for a given scope.
///
/// Accepts `{ scope: string, values: Record<string, unknown> }`. Each entry in
/// `values` is persisted as an individual settings key if it is a known valid
/// key. Unknown keys (from fixture-driven panes) are silently skipped so the
/// frontend does not need to filter its payload.
///
/// # Errors
/// Returns `Err(String)` on database or audit failure.
#[tauri::command]
#[specta::specta]
pub async fn settings_update(
    state: State<'_, AppState>,
    scope: String,
    values: contracts_core::JsonAny,
) -> Result<(), ContractError> {
    tracing::debug!("settings.update scope={scope}");
    let pool = state.repo.pool();
    let bus = &state.bus;

    let obj = match values.0.as_object() {
        Some(o) => o.clone(),
        None => return Ok(()), // Nothing to persist.
    };

    for (key, value) in obj {
        // Only persist keys that are known to the settings use case.
        if !app_core::settings::is_valid_key(&key) {
            tracing::debug!("settings.update: skipping unknown key {key}");
            continue;
        }
        let req =
            SettingsUpdateRequest { key: key.clone(), value: contracts_core::JsonAny::from(value) };
        // Swallow noop and value.invalid for forward-compat; log errors.
        match app_core::settings::update_setting(pool, bus, &state.caches.settings.bag, &req).await
        {
            Ok(_) => {}
            Err(e) if e.code == contracts_core::error_code::ErrorCode::ValueInvalid => {
                tracing::warn!("settings.update: value.invalid for key {key}: {}", e.message);
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}

// ── Additional commands (richer per-key surface) ──────────────────────────────

/// `settings.restore-defaults` — restore one, several, or all keys to defaults.
///
/// # Errors
/// Returns `Err(String)` with code `"key.unknown"` for unknown keys.
#[tauri::command]
#[specta::specta]
pub async fn settings_restore_defaults(
    state: State<'_, AppState>,
    request: RestoreDefaultsRequest,
) -> Result<RestoreDefaultsResponse, ContractError> {
    tracing::debug!("settings.restore-defaults keys={:?}", request.keys);
    app_core::settings::restore_defaults(
        state.repo.pool(),
        &state.bus,
        &state.caches.settings.bag,
        &request,
    )
    .await
}

/// `settings.source-override.set` — set a per-source override for an overridable key.
///
/// # Errors
/// Returns `Err(String)` with code `"key.unoverridable"` or `"value.invalid"`.
#[tauri::command]
#[specta::specta]
pub async fn settings_source_override_set(
    state: State<'_, AppState>,
    request: SetSourceOverrideRequest,
) -> Result<SetSourceOverrideResponse, ContractError> {
    tracing::debug!(
        "settings.source-override.set source_id={} key={}",
        request.source_id,
        request.key
    );
    app_core::settings::set_source_override(
        state.repo.pool(),
        &state.bus,
        &state.caches.settings.bag,
        &request,
    )
    .await
}

/// `settings.overridable-keys` — return the list of stable settings keys that
/// can be overridden per source root (spec 018 T025).
///
/// The frontend uses this to populate the key selector in the source override
/// panel without hardcoding key names.
///
/// # Errors
/// Never errors in practice; returns `Ok` always.
#[tauri::command]
#[specta::specta]
pub async fn settings_overridable_keys(
    _state: State<'_, AppState>,
) -> Result<Vec<String>, ContractError> {
    Ok(app_core::settings::overridable_keys())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A writable key that no scope resolves is silently unreadable: the write
    /// succeeds, the pane re-reads defaults, and the user's choice vanishes
    /// with no error (#641).
    #[test]
    fn every_descriptor_key_is_reachable_via_the_catch_all_scope() {
        let reachable = scope_keys("");
        let missing: Vec<_> = app_core::settings::stable_keys()
            .into_iter()
            .filter(|k| !reachable.contains(k))
            .collect();
        assert!(
            missing.is_empty(),
            "keys writable but unreadable via settings_get(''): {missing:?}"
        );
    }

    #[test]
    fn cleanup_scope_resolves_the_per_type_action_overrides() {
        assert!(scope_keys("cleanup").contains(&"cleanupTypeOverrides"));
    }

    #[test]
    fn general_scope_resolves_theme_and_locale() {
        assert_eq!(scope_keys("general"), &["theme", "locale"]);
    }

    /// Guards the other direction: a typo or a key deleted from the descriptor
    /// table leaves a scope entry that `settings_update` would reject.
    #[test]
    fn every_scope_key_is_a_valid_settings_key() {
        for scope in [
            "advanced",
            "general",
            "cleanup",
            "naming",
            "sources",
            "calibration",
            "sourceViews",
            "observing",
            "planner",
            "catalogues",
            "framing",
            "",
        ] {
            for key in scope_keys(scope) {
                assert!(
                    app_core::settings::is_valid_key(key),
                    "scope {scope:?} lists unknown key {key:?}"
                );
            }
        }
    }
}
