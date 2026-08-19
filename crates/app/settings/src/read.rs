use app_core_cache::SnapshotCache;

use super::{
    bus_err, db_err, descriptors, is_catalogues_enabled_key, is_locale_key, is_tools_bundle_id_key,
    repair, repo, validate_value, ContractError, EventBus, SettingsGetResponse, SettingsRepair,
    SettingsState, Source, SqlitePool, Timestamp, Value, TOPIC_SETTINGS_REPAIR,
};

// ── get_settings ──────────────────────────────────────────────────────────

/// Load all settings with defaults hydrated for missing rows (T018).
///
/// Invalid stored values are deleted and reset to default, with a
/// `settings.repair` audit event emitted (T019).
///
/// `cache` is a per-instance snapshot slot. Production callers pass
/// `&state.caches.settings.bag`; tests pass `&SnapshotCache::new()` so every
/// test starts with an empty slot and never sees another test's snapshot.
///
/// # Errors
///
/// Returns `ContractError` on database or audit failure.
pub async fn get_settings(
    pool: &SqlitePool,
    bus: &EventBus,
    cache: &SnapshotCache<SettingsState>,
) -> Result<SettingsGetResponse, ContractError> {
    // Read-through: on hit, skip the DB entirely (F0 in-memory caching layer).
    if let Some(cached) = cache.load() {
        return Ok(SettingsGetResponse { settings: (*cached).clone() });
    }

    let all_raw = repo::get_all_raw(pool).await.map_err(db_err)?;
    let mut settings = SettingsState::default();

    for (key, value) in all_raw {
        // Validate stored value.
        if let Err(_validation_err) = validate_value(&key, &value) {
            // Partial repair: for array-valued keys whose entries stand alone,
            // keep the entries that validate and store the remainder, so one
            // malformed observing site no longer discards every other site the
            // user typed in (constitution V, Tier 1).
            if let Some(salvaged) = repair::salvage(&key, &value) {
                repo::set_raw(pool, &key, &salvaged.kept).await.map_err(db_err)?;
                publish_repair(bus, &key, salvaged.dropped, salvaged.kept.clone()).await?;
                apply_value_to_state(&key, salvaged.kept, &mut settings);
                continue;
            }

            // Repair: delete the bad row and emit a warn audit event.
            repo::delete_key(pool, &key).await.map_err(db_err)?;
            publish_repair(bus, &key, value.clone(), default_value_for_key(&key)).await?;

            // Use the default — do not apply the invalid stored value.
            continue;
        }

        // Apply the validated value.
        apply_value_to_state(&key, value, &mut settings);
    }

    cache.store(std::sync::Arc::new(settings.clone()));
    Ok(SettingsGetResponse { settings })
}

/// Emit the `settings.repair` audit event.
///
/// `discarded` is the whole stored value for a full reset and only the dropped
/// entries for a partial repair. `restored` is the value now stored: the in-code
/// default, or the salvaged remainder.
async fn publish_repair(
    bus: &EventBus,
    key: &str,
    discarded: Value,
    restored: Value,
) -> Result<(), ContractError> {
    bus.publish(
        TOPIC_SETTINGS_REPAIR,
        Source::System,
        SettingsRepair {
            key: key.to_owned(),
            invalid_value: discarded,
            default_value: restored,
            at: Timestamp::now_iso(),
        },
    )
    .await
    .map_err(bus_err)?;
    Ok(())
}

/// Apply a raw JSON value to the correct field of `SettingsState`.
///
/// Unknown keys (structured-path keys like tools.*) are stored in the DB but
/// not mapped to static SettingsState fields.
pub fn apply_value_to_state(key: &str, value: Value, state: &mut SettingsState) {
    if let Some(descriptor) = descriptors::descriptor_for(key) {
        (descriptor.apply)(value, state);
    }
    // Else: structured-path keys are not mapped to static SettingsState
    // fields. Use resolve_setting(key, source_id) to read them individually.
}

/// Return the in-code default value for a given key as `serde_json::Value`.
pub fn default_value_for_key(key: &str) -> Value {
    if let Some(descriptor) = descriptors::descriptor_for(key) {
        return (descriptor.default)(SettingsState::default());
    }
    // Structured-path: tools.<id>.bundle_id resolves the seed default
    // when no user override is stored (spec 018 T042).
    if is_tools_bundle_id_key(key) {
        if let Some(tool_id) = key.strip_prefix("tools.").and_then(|r| r.strip_suffix(".bundle_id"))
        {
            if let Some(profile) = workflow_profiles::seed::find(tool_id) {
                return match profile.bundle_id {
                    // Known bundle ID from seed (may be wrong on non-macOS; callers
                    // should prefer the per-OS auto-detected value when available).
                    Some(id) => Value::String(id.to_owned()),
                    None => Value::Null,
                };
            }
        }
        return Value::Null;
    }
    if is_catalogues_enabled_key(key) {
        return serde_json::json!(["M", "NGC", "IC", "Sh2"]);
    }
    // T002: a store with no stored `locale` row answers the base locale
    // (data-model.md "Stored state" — `en-GB`), never an empty value.
    if is_locale_key(key) {
        return Value::String("en-GB".to_owned());
    }
    Value::Null
}
