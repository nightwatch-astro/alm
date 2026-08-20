use app_core_cache::SnapshotCache;

use super::*;
use audit::EventBus;
use persistence_core::Database;
use proptest::prelude::*;
use rstest::rstest;
use serde_json::json;

/// US11 T144 guard: every key in the descriptor table is a valid key, has a
/// non-null in-code default (except the nullable `currentLibraryId`), and
/// is recognised by `is_valid_key`. Locks the descriptor registry against
/// drift from `default_value_for_key` / `SettingsState`.
#[test]
fn descriptor_keys_match_state_defaults() {
    // Keys whose in-code default is legitimately null (nullable by design).
    const NULLABLE_KEYS: &[&str] =
        &["currentLibraryId", "observingDefaultSiteId", "observingActiveSiteId"];
    for key in descriptors::all_keys() {
        assert!(is_valid_key(key), "descriptor key {key} not accepted by is_valid_key");
        let default = default_value_for_key(key);
        if NULLABLE_KEYS.contains(&key) {
            assert!(default.is_null(), "{key} default should be null");
        } else {
            assert!(!default.is_null(), "descriptor key {key} has no in-code default");
        }
    }
}

/// n5_settingstable guard: every descriptor's `apply` and `default`
/// accessors agree — applying a key's in-code default onto a fresh
/// `SettingsState` and re-serialising it must reproduce that same default
/// value at the key's wire name. Catches a mismatched/misspelled field in
/// either accessor (they're independently hand-written closures).
#[test]
fn descriptor_apply_and_default_round_trip() {
    for key in descriptors::all_keys() {
        let default_value = default_value_for_key(key);
        let mut state = SettingsState::default();
        apply_value_to_state(key, default_value.clone(), &mut state);

        let serialized = serde_json::to_value(&state).expect("state serializes");
        let obj = serialized.as_object().expect("state is a JSON object");
        // Missing key means skip_serializing_if hid a None field (e.g.
        // currentLibraryId) — that's equivalent to an explicit null.
        let round_tripped = obj.get(key).cloned().unwrap_or(Value::Null);

        assert_eq!(
            round_tripped, default_value,
            "{key}: apply(default_value_for_key) must round-trip through SettingsState"
        );
    }
}

/// Canonical key naming guard (tinyspec: settings-key-canonicalization).
///
/// Every stable key in the descriptor registry must equal the serde-camelCase
/// wire name of its corresponding `SettingsState` field. We derive the wire
/// names by serialising a fully-populated `SettingsState` (with the optional
/// `currentLibraryId` set to a sentinel so it is not skip-serialised) and
/// collecting the top-level field names — serde `rename_all = "camelCase"`
/// produces exactly the canonical camelCase wire names.
///
/// This test will fail if:
/// - A descriptor key uses the old dotted/snake style instead of camelCase.
/// - A new `SettingsState` field is added without a matching descriptor entry.
/// - A descriptor entry exists for a key not present in `SettingsState`.
#[test]
fn descriptor_keys_are_canonical_camel_case_wire_names() {
    use std::collections::BTreeSet;

    // Populate the optional field so `skip_serializing_if = "Option::is_none"`
    // doesn't hide `currentLibraryId` from the serialised output.
    let state = SettingsState {
        current_library_id: Some("__probe__".to_owned()),
        ..SettingsState::default()
    };

    // Derive wire field names from the actual serde serialisation — ground truth.
    let state_json = serde_json::to_value(&state).expect("SettingsState serialises");
    let wire_fields: BTreeSet<String> =
        state_json.as_object().expect("SettingsState is a JSON object").keys().cloned().collect();

    let descriptor_keys: BTreeSet<String> = descriptors::all_keys().map(str::to_owned).collect();

    // Every descriptor key must appear as a wire field name.
    for key in &descriptor_keys {
        assert!(
            wire_fields.contains(key),
            "descriptor key '{key}' is not a camelCase wire field name of SettingsState; \
             known wire fields: {wire_fields:?}"
        );
    }

    // Every wire field name must have a descriptor entry.
    for field in &wire_fields {
        assert!(
            descriptor_keys.contains(field),
            "SettingsState wire field '{field}' has no descriptor entry; \
             add it to DESCRIPTORS in descriptors.rs"
        );
    }
}

/// Returns a fresh per-instance `SnapshotCache` so tests never share state
/// through the process-global slot and can run in parallel without a mutex.
async fn setup() -> (Database, EventBus, SnapshotCache<SettingsState>) {
    let db = Database::in_memory().await.expect("in-memory DB");
    db.migrate().await.expect("migrations");
    let bus = EventBus::with_pool(db.pool().clone());
    (db, bus, SnapshotCache::new())
}

// ── T007: settings.get contract test ───────────────────────────────

#[tokio::test]
async fn get_settings_returns_defaults_when_empty() {
    let (db, bus, cache) = setup().await;
    let resp = get_settings(db.pool(), &bus, &cache).await.unwrap();
    let defaults = SettingsState::default();
    assert_eq!(resp.settings.log_level, defaults.log_level);
    assert_eq!(resp.settings.follow_symlinks, defaults.follow_symlinks);
    assert_eq!(resp.settings.hash_on_scan, defaults.hash_on_scan);
    assert_eq!(resp.settings.protected_categories, defaults.protected_categories);
    assert!(!resp.settings.dev_mode);
}

// ── T008: settings.update contract tests ───────────────────────────

#[tokio::test]
async fn update_setting_happy_path_non_noisy() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "logLevel".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("debug")),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success);
    assert_eq!(resp.key, "logLevel");
    // Non-noisy key should have an audit_id.
    assert!(resp.audit_id.is_some());
}

/// FR-131/SC-009 (T122): the returned `audit_id` must resolve to a real
/// durable `audit_log_entry` row, not just a bus-only event id.
#[tokio::test]
async fn update_setting_audit_id_resolves_to_durable_row() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "logLevel".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("debug")),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    let audit_id = resp.audit_id.expect("non-noisy key must return an audit_id");

    let row: (String, String) =
        sqlx::query_as("SELECT entity_type, outcome FROM audit_log_entry WHERE audit_id = ?")
            .bind(&audit_id)
            .fetch_one(db.pool())
            .await
            .expect("audit_id must resolve to a durable audit_log_entry row");
    assert_eq!(row.0, "settings");
    assert_eq!(row.1, "applied");
}

/// FR-130 (T122): `pattern` is `noisy` (debounced) but is a durable-data
/// key — it must still audit at its committed value, unlike a UI-state
/// noisy key (see `update_setting_noisy_key_no_audit_id`).
#[tokio::test]
async fn update_setting_noisy_audited_key_pattern_gets_audit_id() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "pattern".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!([{
            "type": "literal", "value": "changed"
        }])),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success);
    assert!(resp.audit_id.is_some(), "durable-data noisy key `pattern` must still be audited");
}

/// T127: a refused `settings.update` (unknown key) writes a durable
/// `Outcome::Refused` row with a reason_code, per FR-130/FR-134.
#[tokio::test]
async fn update_setting_refused_unknown_key_writes_durable_row() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "notARealKey".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("whatever")),
    };
    let err = update_setting(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::KeyUnknown);

    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT outcome, reason_code FROM audit_log_entry WHERE entity_type = 'settings' AND outcome = 'refused'",
    )
    .fetch_one(db.pool())
    .await
    .expect("refused settings.update must write a durable audit row");
    assert_eq!(row.0, "refused");
    assert_eq!(row.1.as_deref(), Some("key.unknown"));
}

/// Theme durability round trip (theme-settings-db): `settings.update`
/// persists the choice to the real settings table, and `resolve_setting`
/// — the same lookup `settings_get` uses — reads it back, proving the DB
/// (not localStorage) is the durable source of truth.
#[tokio::test]
async fn theme_persists_and_resolves_via_settings_db() {
    let (db, bus, cache) = setup().await;
    assert_eq!(
        resolve_setting(db.pool(), "theme", None).await.unwrap(),
        serde_json::json!("system"),
        "default theme should be \"system\" before any write"
    );

    let req = SettingsUpdateRequest {
        key: "theme".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("espresso-dark")),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success);

    assert_eq!(
        resolve_setting(db.pool(), "theme", None).await.unwrap(),
        serde_json::json!("espresso-dark")
    );
}

#[tokio::test]
async fn update_setting_noop_when_value_unchanged() {
    let (db, bus, cache) = setup().await;
    // logLevel default is "info".
    let req = SettingsUpdateRequest {
        key: "logLevel".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("info")),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Noop);
    assert!(resp.audit_id.is_none());
}

#[tokio::test]
async fn update_setting_rejects_unknown_key() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "notARealKey".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("whatever")),
    };
    let err = update_setting(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::KeyUnknown);
}

#[tokio::test]
async fn update_setting_rejects_invalid_value() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "logLevel".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("trace")), // not a valid level
    };
    let err = update_setting(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ValueInvalid);
}

#[tokio::test]
async fn update_setting_noisy_key_no_audit_id() {
    let (db, bus, cache) = setup().await;
    // "rememberFollowLogs" default is false; change to true.
    let req = SettingsUpdateRequest {
        key: "rememberFollowLogs".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!(true)),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success);
    // Noisy key: no per-change audit id.
    assert!(resp.audit_id.is_none());
}

#[tokio::test]
async fn update_setting_pattern_noop_structural_equality() {
    let (db, bus, cache) = setup().await;
    // Get the default pattern and send it back — must be noop (A4, R4.1).
    let defaults = SettingsState::default();
    let pattern_value = serde_json::to_value(&defaults.pattern).unwrap();
    let req = SettingsUpdateRequest {
        key: "pattern".to_owned(),
        value: contracts_core::JsonAny::from(pattern_value),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Noop);
}

#[tokio::test]
async fn update_setting_protected_categories_noop_structural_equality() {
    let (db, bus, cache) = setup().await;
    // Default is ["lights", "masters", "finals"] — same value must be noop (R-Set-1).
    let defaults = SettingsState::default();
    let value = serde_json::to_value(&defaults.protected_categories).unwrap();
    let req = SettingsUpdateRequest {
        key: "protectedCategories".to_owned(),
        value: contracts_core::JsonAny::from(value),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Noop);
}

// ── T016: invalid stored value resets to default ───────────────────

#[tokio::test]
async fn get_settings_repairs_invalid_stored_value() {
    let (db, bus, cache) = setup().await;
    // Inject an invalid value directly.
    repo::set_raw(db.pool(), "logLevel", &serde_json::json!("trace")).await.unwrap();
    let resp = get_settings(db.pool(), &bus, &cache).await.unwrap();
    // Should have been repaired to the default.
    assert_eq!(resp.settings.log_level, "info");
    // The bad row should have been deleted.
    let raw = repo::get_raw(db.pool(), "logLevel").await.unwrap();
    assert!(raw.is_none());
}

// ── Partial repair of observing sites (mq5m) ───────────────────────

/// A valid observing site with the given id and name.
fn site(id: &str, name: &str) -> Value {
    serde_json::json!({
        "id": id, "name": name, "latitudeDeg": 52.1, "longitudeDeg": 5.3,
        "elevationM": 12.0, "timezone": "Europe/Amsterdam",
        "twilight": "astronomical", "minHorizonAltDeg": 0.0
    })
}

/// One malformed site must not take the user's other sites with it. Observing
/// sites are coordinates the user typed in, so the whole-key reset lost Tier-1
/// user knowledge with nothing left to recover from.
#[tokio::test]
async fn get_settings_keeps_the_valid_observing_sites_when_one_is_malformed() {
    let (db, bus, cache) = setup().await;
    let mut bad = site("s2", "Broken");
    bad["latitudeDeg"] = serde_json::json!(91.0);
    let stored = serde_json::json!([site("s1", "Home"), bad, site("s3", "Dark")]);
    repo::set_raw(db.pool(), "observingSites", &stored).await.unwrap();

    let resp = get_settings(db.pool(), &bus, &cache).await.unwrap();

    let ids: Vec<&str> = resp.settings.observing_sites.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["s1", "s3"], "only the malformed site may be dropped");

    // The repaired remainder is written back, not deleted.
    let raw = repo::get_raw(db.pool(), "observingSites").await.unwrap();
    assert_eq!(raw, Some(serde_json::json!([site("s1", "Home"), site("s3", "Dark")])));
}

/// The repair event names the dropped site and the remainder that was kept.
#[tokio::test]
async fn partial_repair_audits_the_dropped_site_and_the_kept_remainder() {
    let (db, bus, cache) = setup().await;
    let mut bad = site("s2", "Broken");
    bad["latitudeDeg"] = serde_json::json!(91.0);
    let stored = serde_json::json!([site("s1", "Home"), bad]);
    repo::set_raw(db.pool(), "observingSites", &stored).await.unwrap();

    get_settings(db.pool(), &bus, &cache).await.unwrap();

    let payload: String =
        sqlx::query_scalar("SELECT payload FROM events WHERE topic = 'settings.repair'")
            .fetch_one(db.pool())
            .await
            .expect("partial repair must still audit");
    let payload: Value = serde_json::from_str(&payload).expect("payload is JSON");
    assert_eq!(payload["invalidValue"], serde_json::json!([bad]));
    assert_eq!(payload["defaultValue"], serde_json::json!([site("s1", "Home")]));
}

/// A duplicate id is only visible against the sites already kept, so the first
/// occurrence survives.
#[tokio::test]
async fn get_settings_keeps_the_first_of_two_sites_sharing_an_id() {
    let (db, bus, cache) = setup().await;
    let stored = serde_json::json!([site("dup", "First"), site("dup", "Second")]);
    repo::set_raw(db.pool(), "observingSites", &stored).await.unwrap();

    let resp = get_settings(db.pool(), &bus, &cache).await.unwrap();

    let names: Vec<&str> = resp.settings.observing_sites.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["First"]);
}

/// Salvage applies to arrays of independently valid entries. A value that is not
/// an array has no entries to keep, so the whole key still resets.
#[tokio::test]
async fn get_settings_resets_observing_sites_when_the_value_is_not_an_array() {
    let (db, bus, cache) = setup().await;
    repo::set_raw(db.pool(), "observingSites", &serde_json::json!("nope")).await.unwrap();

    let resp = get_settings(db.pool(), &bus, &cache).await.unwrap();

    assert!(resp.settings.observing_sites.is_empty());
    assert!(repo::get_raw(db.pool(), "observingSites").await.unwrap().is_none());
}

// ── T021/T022: source override tests ──────────────────────────────

#[tokio::test]
async fn set_source_override_happy_path() {
    let (db, bus, cache) = setup().await;
    let req = SetSourceOverrideRequest {
        source_id: "src-abc".to_owned(),
        key: "defaultProtection".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("unprotected")),
    };
    let resp = set_source_override(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.source_id, "src-abc");
    assert_eq!(resp.key, "defaultProtection");
}

/// Review round 1 #1: `set_source_override`'s durable audit id resolves
/// to a real `audit_log_entry` row (FR-130/FR-131).
#[tokio::test]
async fn set_source_override_writes_durable_applied_audit_row() {
    let (db, bus, cache) = setup().await;
    let req = SetSourceOverrideRequest {
        source_id: "src-abc".to_owned(),
        key: "defaultProtection".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("unprotected")),
    };
    set_source_override(db.pool(), &bus, &cache, &req).await.unwrap();

    let row: (String, String) = sqlx::query_as(
        "SELECT entity_type, outcome FROM audit_log_entry WHERE trigger = 'settings.source_override.set'",
    )
    .fetch_one(db.pool())
    .await
    .expect("set_source_override must write a durable audit row");
    assert_eq!(row.0, "settings");
    assert_eq!(row.1, "applied");
}

#[tokio::test]
async fn set_source_override_rejects_unoverridable_key() {
    let (db, bus, cache) = setup().await;
    let req = SetSourceOverrideRequest {
        source_id: "src-abc".to_owned(),
        key: "logLevel".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("debug")),
    };
    let err = set_source_override(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::KeyUnoverridable);

    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT outcome, reason_code FROM audit_log_entry WHERE trigger = 'settings.source_override.set' AND outcome = 'refused'",
    )
    .fetch_one(db.pool())
    .await
    .expect("refused set_source_override must write a durable audit row");
    assert_eq!(row.0, "refused");
    assert_eq!(row.1.as_deref(), Some("key.unoverridable"));
}

/// Issue #623: `followSymlinks`/`hashOnScan` duplicated the canonical
/// `IngestionSettings` document and could never succeed as a per-source
/// override (`hashOnScan` needs a string, the UI only ever offered a
/// boolean — #646) — removed from the overridable set entirely.
#[rstest]
#[case("followSymlinks")]
#[case("hashOnScan")]
#[tokio::test]
async fn set_source_override_rejects_retired_scan_behavior_keys(#[case] key: &str) {
    let (db, bus, cache) = setup().await;
    let req = SetSourceOverrideRequest {
        source_id: "src-abc".to_owned(),
        key: key.to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!(true)),
    };
    let err = set_source_override(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::KeyUnoverridable);
}

// ── T022: resolution order ─────────────────────────────────────────

#[tokio::test]
async fn resolve_setting_prefers_source_override() {
    let (db, bus, cache) = setup().await;

    // Set global to "protected".
    let req = SettingsUpdateRequest {
        key: "defaultProtection".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("protected")),
    };
    update_setting(db.pool(), &bus, &cache, &req).await.unwrap();

    // Set source override to "unprotected".
    let ov_req = SetSourceOverrideRequest {
        source_id: "src-1".to_owned(),
        key: "defaultProtection".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("unprotected")),
    };
    set_source_override(db.pool(), &bus, &cache, &ov_req).await.unwrap();

    let resolved = resolve_setting(db.pool(), "defaultProtection", Some("src-1")).await.unwrap();
    assert_eq!(resolved, serde_json::json!("unprotected"));
}

#[tokio::test]
async fn resolve_setting_falls_back_to_global() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "defaultProtection".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("unprotected")),
    };
    update_setting(db.pool(), &bus, &cache, &req).await.unwrap();

    // No override for "src-2".
    let resolved = resolve_setting(db.pool(), "defaultProtection", Some("src-2")).await.unwrap();
    assert_eq!(resolved, serde_json::json!("unprotected"));
}

#[tokio::test]
async fn resolve_setting_falls_back_to_default() {
    let (db, _bus, _cache) = setup().await;
    let resolved = resolve_setting(db.pool(), "hashOnScan", None).await.unwrap();
    assert_eq!(resolved, serde_json::json!("lazy")); // default
}

/// #645: the `catalogues` scope's `enabled` key is a known, persistable
/// key (previously silently skipped as unknown), and the persisted value
/// survives a fresh `resolve_setting` read (the reload path).
#[tokio::test]
async fn update_setting_enabled_catalogues_persists_across_reload() {
    let (db, bus, cache) = setup().await;

    // Default (nothing stored yet) is the in-code default subset.
    let default = resolve_setting(db.pool(), "enabled", None).await.unwrap();
    assert_eq!(default, serde_json::json!(["M", "NGC", "IC", "Sh2"]));

    let req = SettingsUpdateRequest {
        key: "enabled".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!(["M", "NGC", "IC", "Sh2", "LBN"])),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success);

    // Simulates "leave the pane and return": a fresh resolve reads the
    // persisted row, not the in-code default.
    let reloaded = resolve_setting(db.pool(), "enabled", None).await.unwrap();
    assert_eq!(reloaded, serde_json::json!(["M", "NGC", "IC", "Sh2", "LBN"]));
}

/// #645: an unknown catalogue id is rejected as `value.invalid`, not
/// silently accepted.
#[tokio::test]
async fn update_setting_enabled_catalogues_rejects_unknown_id() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "enabled".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!(["NotACatalogue"])),
    };
    let err = update_setting(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ValueInvalid);
}

// ── T026: restore_defaults contract tests ──────────────────────────

#[tokio::test]
async fn restore_defaults_restores_changed_keys() {
    let (db, bus, cache) = setup().await;

    // Change logLevel.
    let req = SettingsUpdateRequest {
        key: "logLevel".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("debug")),
    };
    update_setting(db.pool(), &bus, &cache, &req).await.unwrap();

    // Restore logLevel.
    let restore_req = RestoreDefaultsRequest { keys: vec!["logLevel".to_owned()] };
    let resp = restore_defaults(db.pool(), &bus, &cache, &restore_req).await.unwrap();
    assert_eq!(resp.status, RestoreDefaultsStatus::Success);
    assert!(resp.restored.contains(&"logLevel".to_owned()));
    assert!(resp.already_at_default.is_empty());

    // Verify it's back to default.
    let restored_val = repo::get_raw(db.pool(), "logLevel").await.unwrap();
    assert_eq!(restored_val, Some(serde_json::json!("info")));
}

#[tokio::test]
async fn restore_defaults_noop_when_already_at_default() {
    let (db, bus, cache) = setup().await;
    let restore_req = RestoreDefaultsRequest { keys: vec!["logLevel".to_owned()] };
    let resp = restore_defaults(db.pool(), &bus, &cache, &restore_req).await.unwrap();
    assert_eq!(resp.status, RestoreDefaultsStatus::Noop);
    assert!(resp.restored.is_empty());
    assert!(resp.already_at_default.contains(&"logLevel".to_owned()));
}

#[tokio::test]
async fn restore_defaults_rejects_unknown_key() {
    let (db, bus, cache) = setup().await;
    let restore_req = RestoreDefaultsRequest { keys: vec!["notAKey".to_owned()] };
    let err = restore_defaults(db.pool(), &bus, &cache, &restore_req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::KeyUnknown);
}

// ── Key validation unit tests ──────────────────────────────────────

/// Table-driven `is_valid_key` cases. `expected` is whether the key should
/// be accepted as a valid v1 (stable or structured-path) settings key.
#[rstest]
// Valid: stable + structured-path keys.
#[case("logLevel", true)]
#[case("pattern", true)]
#[case("calibrationDarkOverridePenalty", true)]
#[case("calibrationFlatOverridePenalty", true)]
#[case("calibrationBiasOverridePenalty", true)]
#[case("theme", true)]
#[case("framingPointingFractionOfFov", true)]
#[case("framingPointingFallbackDeg", true)]
#[case("framingRotationToleranceDeg", true)]
#[case("framingMosaicEnvelopeFractionOfFov", true)]
#[case("tools.pixinsight.bundle_id", true)]
#[case("tools.pixinsight.executable_path", true)]
#[case("tools.siril.enabled", true)]
#[case("tools.startools.auto_detected", true)]
#[case("workflow_profile.my_profile.watch_extensions", true)]
#[case("workflow_profile.my_profile.launch_attribution_window_hours", true)]
#[case("locale", true)]
// Invalid: unknown + malformed structured-path keys.
#[case("notARealKey", false)]
#[case("tools.UPPERCASE.bundle_id", false)] // tool id must be lowercase
#[case("tools..bundle_id", false)] // empty tool id
#[case("calibration.dark.override_penalty", false)] // old dotted key — no longer valid
#[case("calibration.video.override_penalty", false)] // video not a valid frame type
fn is_valid_key_cases(#[case] key: &str, #[case] expected: bool) {
    assert_eq!(is_valid_key(key), expected);
}

// ── Locale (spec 061 T001-T003, research D8) ────────────────────────

/// T002: a store with no stored `locale` row answers the base locale, not
/// an empty/null value.
#[test]
fn locale_default_is_base_locale() {
    assert_eq!(default_value_for_key("locale"), serde_json::json!("en-GB"));
}

/// A site whose three range-checked numeric fields are set explicitly, so a
/// case can sit exactly on or just past one inclusive bound while every other
/// field stays valid.
fn edge_site(id: &str, latitude_deg: f64, longitude_deg: f64, min_horizon_alt_deg: f64) -> Value {
    serde_json::json!({
        "id": id, "name": "Edge", "latitudeDeg": latitude_deg,
        "longitudeDeg": longitude_deg, "timezone": "UTC",
        "twilight": "astronomical", "minHorizonAltDeg": min_horizon_alt_deg
    })
}

// ── Value validation unit tests ─────────────────────────────────────
//
// These exercise `validate_value` directly. Previously this logic was only
// covered indirectly through the async DB update tests (e.g. logLevel
// "trace" rejected). Table-driven cases make the per-key contract explicit.

/// Cases that must PASS validation for the given key.
#[rstest]
#[case("hashOnScan", serde_json::json!("lazy"))]
#[case("hashOnScan", serde_json::json!("eager"))]
#[case("hashOnScan", serde_json::json!("off"))]
#[case("darkMatchTolerance", serde_json::json!("strict"))]
#[case("darkMatchTolerance", serde_json::json!("loose"))]
#[case("darkMatchTolerance", serde_json::json!("any"))]
#[case("flatMatching", serde_json::json!("filter-rot"))]
#[case("flatMatching", serde_json::json!("filter"))]
#[case("flatMatching", serde_json::json!("manual"))]
#[case("logLevel", serde_json::json!("error"))]
#[case("logLevel", serde_json::json!("warn"))]
#[case("logLevel", serde_json::json!("info"))]
#[case("defaultProtection", serde_json::json!("protected"))]
#[case("defaultProtection", serde_json::json!("unprotected"))]
#[case("calibrationDarkTempTolerance", serde_json::json!(0.0))]
#[case("calibrationDarkTempTolerance", serde_json::json!(5.5))]
#[case("calibrationAgingThresholdDays", serde_json::json!(1))]
#[case("calibrationAgingThresholdDays", serde_json::json!(3650))]
#[case("calibrationDarkOverridePenalty", serde_json::json!(0.0))]
#[case("calibrationFlatOverridePenalty", serde_json::json!(1.0))]
#[case("calibrationBiasOverridePenalty", serde_json::json!(0.5))]
#[case("autoApplyPattern", serde_json::json!(true))]
#[case("calibrationPrefillSuggestion", serde_json::json!(false))]
#[case("currentLibraryId", serde_json::json!(null))]
#[case("currentLibraryId", serde_json::json!("lib-1"))]
#[case("pattern", serde_json::json!([]))]
#[case("protectedCategories", serde_json::json!(["lights"]))]
#[case("tools.pixinsight.bundle_id", serde_json::json!("com.x"))]
#[case("tools.siril.enabled", serde_json::json!(true))]
#[case("workflow_profile.p.watch_extensions", serde_json::json!([".fits"]))]
#[case("workflow_profile.p.launch_attribution_window_hours", serde_json::json!(2))]
#[case("observingSites", serde_json::json!([]))]
#[case("observingSites", serde_json::json!([{
    "id": "s1", "name": "Home", "latitudeDeg": 52.1, "longitudeDeg": 5.3,
    "elevationM": 12.0, "timezone": "Europe/Amsterdam",
    "twilight": "astronomical", "minHorizonAltDeg": 0.0
}]))]
#[case("observingSites", serde_json::json!([{
    "id": "s2", "name": "Dark", "latitudeDeg": -30.0, "longitudeDeg": -70.0,
    "elevationM": null, "timezone": "America/Santiago",
    "twilight": "nautical", "minHorizonAltDeg": 15.0
}]))]
// Inclusive range edges for every `observingSites` numeric field: both ends of
// latitudeDeg, longitudeDeg, and minHorizonAltDeg must be accepted.
#[case("observingSites", serde_json::json!([edge_site("lat-lo", -90.0, 0.0, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("lat-hi", 90.0, 0.0, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("lon-lo", 0.0, -180.0, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("lon-hi", 0.0, 180.0, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("hor-lo", 0.0, 0.0, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("hor-hi", 0.0, 0.0, 90.0)]))]
#[case("observingDefaultSiteId", serde_json::json!(null))]
#[case("observingDefaultSiteId", serde_json::json!("s1"))]
#[case("observingActiveSiteId", serde_json::json!("s1"))]
#[case("usableAltitudeDeg", serde_json::json!(30))]
#[case("usableAltitudeDeg", serde_json::json!(0))]
#[case("usableAltitudeDeg", serde_json::json!(90))]
#[case("cleanupTypeOverrides", serde_json::json!({}))] // empty map: all defaults apply
#[case("cleanupTypeOverrides", serde_json::json!({"1": "Keep", "20": "Delete"}))]
#[case("theme", serde_json::json!("warm-clay"))]
#[case("theme", serde_json::json!("warm-slate"))]
#[case("theme", serde_json::json!("observatory-dark"))]
#[case("theme", serde_json::json!("observatory-cool"))]
#[case("theme", serde_json::json!("observatory-cool-light"))]
#[case("theme", serde_json::json!("espresso-dark"))]
#[case("theme", serde_json::json!("system"))]
#[case("locale", serde_json::json!("en-GB"))]
#[case("locale", serde_json::json!("pt-BR"))]
#[case("framingPointingFractionOfFov", serde_json::json!(0.10))]
#[case("framingPointingFractionOfFov", serde_json::json!(0.01))]
#[case("framingPointingFractionOfFov", serde_json::json!(2.0))]
#[case("framingPointingFallbackDeg", serde_json::json!(0.2))]
#[case("framingRotationToleranceDeg", serde_json::json!(3.0))]
#[case("framingMosaicEnvelopeFractionOfFov", serde_json::json!(1.0))]
fn validate_value_accepts(#[case] key: &str, #[case] value: Value) {
    assert!(validate_value(key, &value).is_ok(), "expected {key}={value} to be accepted");
}

/// Cases that must FAIL validation with `ErrorCode::ValueInvalid`.
#[rstest]
#[case("hashOnScan", serde_json::json!("nope"))] // not an allowed variant
#[case("hashOnScan", serde_json::json!(5))] // not a string
#[case("darkMatchTolerance", serde_json::json!("fuzzy"))]
#[case("flatMatching", serde_json::json!("auto"))]
#[case("defaultProtection", serde_json::json!("locked"))]
// Retired third level (issue #506's 2-level collapse) — no longer valid.
#[case("defaultProtection", serde_json::json!("normal"))]
#[case("calibrationDarkTempTolerance", serde_json::json!(-1.0))] // must be >= 0
#[case("calibrationDarkTempTolerance", serde_json::json!("x"))] // not a number
#[case("calibrationAgingThresholdDays", serde_json::json!(0))] // below [1,3650]
#[case("calibrationAgingThresholdDays", serde_json::json!(3651))] // above range
#[case("calibrationDarkOverridePenalty", serde_json::json!(-0.1))] // below [0,1]
#[case("calibrationFlatOverridePenalty", serde_json::json!(1.1))] // above [0,1]
#[case("autoApplyPattern", serde_json::json!("true"))] // string, not boolean
#[case("currentLibraryId", serde_json::json!(5))] // not string/null
#[case("pattern", serde_json::json!("notarray"))]
#[case("protectedCategories", serde_json::json!({}))] // object, not array
#[case("tools.siril.enabled", serde_json::json!("yes"))] // not a boolean
#[case("workflow_profile.p.watch_extensions", serde_json::json!("x"))] // not an array
#[case("observingSites", serde_json::json!("nope"))] // not an array
#[case("observingSites", serde_json::json!([{"id": "s1"}]))] // missing required fields
#[case("observingSites", serde_json::json!([{
    "id": "s1", "name": "A", "latitudeDeg": 91.0, "longitudeDeg": 0.0,
    "timezone": "UTC", "twilight": "astronomical", "minHorizonAltDeg": 0.0
}]))]
// latitude out of range
// Just outside each inclusive edge — these pin the comparison operators.
#[case("observingSites", serde_json::json!([edge_site("lat-lo", -90.1, 0.0, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("lat-hi", 90.1, 0.0, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("lon-lo", 0.0, -180.1, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("lon-hi", 0.0, 180.1, 0.0)]))]
#[case("observingSites", serde_json::json!([edge_site("hor-lo", 0.0, 0.0, -0.1)]))]
#[case("observingSites", serde_json::json!([edge_site("hor-hi", 0.0, 0.0, 90.1)]))]
#[case("observingSites", serde_json::json!([{
    "id": "s1", "name": "A", "latitudeDeg": 0.0, "longitudeDeg": 0.0,
    "timezone": "UTC", "twilight": "civil", "minHorizonAltDeg": 0.0
}]))] // invalid twilight
#[case("observingSites", serde_json::json!([
    {"id": "dup", "name": "A", "latitudeDeg": 0.0, "longitudeDeg": 0.0,
     "timezone": "UTC", "twilight": "astronomical", "minHorizonAltDeg": 0.0},
    {"id": "dup", "name": "B", "latitudeDeg": 1.0, "longitudeDeg": 1.0,
     "timezone": "UTC", "twilight": "astronomical", "minHorizonAltDeg": 0.0}
]))] // duplicate ids
#[case("observingDefaultSiteId", serde_json::json!(5))] // not string/null
#[case("usableAltitudeDeg", serde_json::json!(-1))] // below [0,90]
#[case("usableAltitudeDeg", serde_json::json!(91))] // above [0,90]
#[case("usableAltitudeDeg", serde_json::json!("x"))] // not a number
#[case("cleanupTypeOverrides", serde_json::json!("nope"))] // not an object
#[case("cleanupTypeOverrides", serde_json::json!({"0": "Keep"}))] // unknown id (below range)
#[case("cleanupTypeOverrides", serde_json::json!({"21": "Keep"}))] // unknown id (above range)
#[case("cleanupTypeOverrides", serde_json::json!({"abc": "Keep"}))] // non-numeric id
#[case("cleanupTypeOverrides", serde_json::json!({"1": "Trash"}))] // not an allowed action
#[case("cleanupTypeOverrides", serde_json::json!({"1": 5}))] // action not a string
#[case("theme", serde_json::json!("neon"))] // not an allowed variant
#[case("theme", serde_json::json!(5))] // not a string
#[case("locale", serde_json::json!("fr-FR"))] // not a shipped locale
#[case("locale", serde_json::json!("en-US"))] // close but not the shipped GB tag
#[case("locale", serde_json::json!(5))] // not a string
#[case("framingPointingFractionOfFov", serde_json::json!(0.0))] // below [0.01, 2.0]
#[case("framingPointingFractionOfFov", serde_json::json!(2.1))] // above range
#[case("framingPointingFractionOfFov", serde_json::json!("x"))] // not a number
#[case("framingPointingFallbackDeg", serde_json::json!(0.0))] // below [0.01, 10.0]
#[case("framingRotationToleranceDeg", serde_json::json!(0.0))] // below [0.1, 45.0]
#[case("framingMosaicEnvelopeFractionOfFov", serde_json::json!(0.0))] // below [0.1, 5.0]
fn validate_value_rejects(#[case] key: &str, #[case] value: Value) {
    let err = validate_value(key, &value).expect_err("expected rejection");
    assert_eq!(err.code, ErrorCode::ValueInvalid, "key {key} value {value}");
}

/// A fully-populated valid `plannerMoonAvoidance` value (all seven bands).
fn valid_planner_moon_avoidance() -> Value {
    default_value_for_key("plannerMoonAvoidance")
}

/// spec 047 T005: `plannerMoonAvoidance` structured-object validation.
#[test]
fn planner_moon_avoidance_accepts_full_valid_bands() {
    assert!(validate_value("plannerMoonAvoidance", &valid_planner_moon_avoidance()).is_ok());
}

#[test]
fn planner_moon_avoidance_default_is_the_shipped_table() {
    let v = valid_planner_moon_avoidance();
    let obj = v.as_object().expect("object");
    assert_eq!(obj.len(), 7);
    assert_eq!(obj["L"]["distanceDeg"], serde_json::json!(120.0));
    assert_eq!(obj["Ha"]["widthDays"], serde_json::json!(7.0));
    assert_eq!(obj["OIII"]["distanceDeg"], serde_json::json!(110.0));
}

#[rstest]
#[case(serde_json::json!("nope"))] // not an object
#[case(serde_json::json!({"L": {"distanceDeg": 120.0, "widthDays": 14.0}}))] // missing bands
#[case(serde_json::json!({"X": {"distanceDeg": 1.0, "widthDays": 1.0}}))] // unknown band
fn planner_moon_avoidance_rejects_shape(#[case] value: Value) {
    let err = validate_value("plannerMoonAvoidance", &value).expect_err("expected rejection");
    assert_eq!(err.code, ErrorCode::ValueInvalid);
}

#[test]
fn planner_moon_avoidance_rejects_out_of_range() {
    let mut v = valid_planner_moon_avoidance();
    v["L"]["distanceDeg"] = serde_json::json!(181.0); // > 180
    assert!(validate_value("plannerMoonAvoidance", &v).is_err());

    let mut v2 = valid_planner_moon_avoidance();
    v2["Ha"]["widthDays"] = serde_json::json!(0.1); // < 0.5
    assert!(validate_value("plannerMoonAvoidance", &v2).is_err());
}

#[test]
fn planner_moon_avoidance_rejects_extra_property() {
    let mut v = valid_planner_moon_avoidance();
    v["L"]["bogus"] = serde_json::json!(1);
    assert!(validate_value("plannerMoonAvoidance", &v).is_err());
}

/// Unknown / unconstrained keys impose no additional value validation.
#[rstest]
#[case("someUnknownKey", serde_json::json!("anything"))]
#[case("anotherUnknown", serde_json::json!(42))]
fn validate_value_passes_unconstrained_keys(#[case] key: &str, #[case] value: Value) {
    assert!(validate_value(key, &value).is_ok());
}

// ── spec 044 T008: observing.* settings round-trip + invariants ──────

/// A fully-populated valid site value.
fn sample_site(id: &str, lat: f64) -> Value {
    serde_json::json!({
        "id": id, "name": format!("Site {id}"), "latitudeDeg": lat,
        "longitudeDeg": 5.3, "elevationM": 12.0, "timezone": "Europe/Amsterdam",
        "twilight": "astronomical", "minHorizonAltDeg": 0.0
    })
}

#[tokio::test]
async fn observing_sites_round_trip_through_db() {
    let (db, bus, cache) = setup().await;

    // Defaults: empty sites, null pointers, threshold 30.
    assert_eq!(resolve_setting(db.pool(), "observingSites", None).await.unwrap(), json!([]));
    assert!(resolve_setting(db.pool(), "observingActiveSiteId", None).await.unwrap().is_null());
    assert_eq!(resolve_setting(db.pool(), "usableAltitudeDeg", None).await.unwrap(), json!(30.0));

    // Persist two sites + active pointer + a custom threshold.
    let sites = json!([sample_site("s1", 52.1), sample_site("s2", -30.0)]);
    for (key, value) in [
        ("observingSites", sites.clone()),
        ("observingDefaultSiteId", json!("s1")),
        ("observingActiveSiteId", json!("s2")),
        ("usableAltitudeDeg", json!(40.0)),
    ] {
        update_setting(
            db.pool(),
            &bus,
            &cache,
            &SettingsUpdateRequest { key: key.to_owned(), value: value.into() },
        )
        .await
        .expect("update ok");
    }

    // Read back — the whole ObserverSite[] survives the round-trip.
    assert_eq!(resolve_setting(db.pool(), "observingSites", None).await.unwrap(), sites);
    assert_eq!(
        resolve_setting(db.pool(), "observingActiveSiteId", None).await.unwrap(),
        json!("s2")
    );
    assert_eq!(resolve_setting(db.pool(), "usableAltitudeDeg", None).await.unwrap(), json!(40.0));

    // Full-state hydration maps the keys onto SettingsState fields.
    let resp = get_settings(db.pool(), &bus, &cache).await.unwrap();
    assert_eq!(resp.settings.observing_sites.len(), 2);
    assert_eq!(resp.settings.observing_active_site_id.as_deref(), Some("s2"));
    assert!((resp.settings.usable_altitude_deg - 40.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn observing_settings_reject_invalid_values() {
    let (db, bus, cache) = setup().await;
    // Out-of-range threshold is rejected as value.invalid.
    let err = update_setting(
        db.pool(),
        &bus,
        &cache,
        &SettingsUpdateRequest { key: "usableAltitudeDeg".to_owned(), value: json!(120).into() },
    )
    .await
    .expect_err("out-of-range threshold rejected");
    assert_eq!(err.code, ErrorCode::ValueInvalid);
}

// ── Framing clustering tunables (spec 008 Q27 F-Framing-11, R11a) ──────

#[tokio::test]
async fn framing_tolerances_round_trip_through_db() {
    let (db, bus, cache) = setup().await;

    // R11a shipped defaults.
    assert_eq!(
        resolve_setting(db.pool(), "framingPointingFractionOfFov", None).await.unwrap(),
        json!(0.10)
    );
    assert_eq!(
        resolve_setting(db.pool(), "framingPointingFallbackDeg", None).await.unwrap(),
        json!(0.2)
    );
    assert_eq!(
        resolve_setting(db.pool(), "framingRotationToleranceDeg", None).await.unwrap(),
        json!(3.0)
    );
    assert_eq!(
        resolve_setting(db.pool(), "framingMosaicEnvelopeFractionOfFov", None).await.unwrap(),
        json!(1.0)
    );

    for (key, value) in [
        ("framingPointingFractionOfFov", json!(0.25)),
        ("framingPointingFallbackDeg", json!(0.5)),
        ("framingRotationToleranceDeg", json!(5.0)),
        ("framingMosaicEnvelopeFractionOfFov", json!(1.5)),
    ] {
        update_setting(
            db.pool(),
            &bus,
            &cache,
            &SettingsUpdateRequest { key: key.to_owned(), value: value.into() },
        )
        .await
        .expect("update ok");
    }

    let resp = get_settings(db.pool(), &bus, &cache).await.unwrap();
    assert!((resp.settings.framing_pointing_fraction_of_fov - 0.25).abs() < f64::EPSILON);
    assert!((resp.settings.framing_pointing_fallback_deg - 0.5).abs() < f64::EPSILON);
    assert!((resp.settings.framing_rotation_tolerance_deg - 5.0).abs() < f64::EPSILON);
    assert!((resp.settings.framing_mosaic_envelope_fraction_of_fov - 1.5).abs() < f64::EPSILON);
}

#[tokio::test]
async fn framing_tolerances_reject_out_of_range_values() {
    let (db, bus, cache) = setup().await;
    let err = update_setting(
        db.pool(),
        &bus,
        &cache,
        &SettingsUpdateRequest {
            key: "framingRotationToleranceDeg".to_owned(),
            value: json!(90).into(),
        },
    )
    .await
    .expect_err("out-of-range rotation tolerance rejected");
    assert_eq!(err.code, ErrorCode::ValueInvalid);
}

// ── T056: aging_threshold_days persists + consumer reads it (FR-023) ──

#[tokio::test]
async fn aging_threshold_days_persists_and_is_readable() {
    let (db, bus, cache) = setup().await;

    // Default should be 90.
    let defaults = SettingsState::default();
    assert!(
        (defaults.calibration_aging_threshold_days - 90.0).abs() < f64::EPSILON,
        "default aging threshold must be 90 days"
    );

    // Persist a custom value.
    let req = SettingsUpdateRequest {
        key: "calibrationAgingThresholdDays".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!(180)),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success);

    // Read it back via get_settings — consumer path.
    let get_resp = get_settings(db.pool(), &bus, &cache).await.unwrap();
    assert!(
        (get_resp.settings.calibration_aging_threshold_days - 180.0).abs() < f64::EPSILON,
        "calibrationAgingThresholdDays must round-trip: got {}",
        get_resp.settings.calibration_aging_threshold_days
    );
}

#[tokio::test]
async fn aging_threshold_days_rejects_bogus_scope_key() {
    // The old dotted key 'calibration.aging_threshold_days' is no longer valid;
    // the canonical key is 'calibrationAgingThresholdDays'.
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "calibration.aging_threshold_days".to_owned(), // old dotted key name
        value: contracts_core::JsonAny::from(serde_json::json!(90)),
    };
    let err = update_setting(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(
        err.code,
        ErrorCode::KeyUnknown,
        "old dotted key 'calibration.aging_threshold_days' must be rejected"
    );
}

// ── Spec 041 FR-026b: patterns_by_type round-trip + validation ─────

#[tokio::test]
async fn update_patterns_by_type_round_trips_via_get() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "patternsByType".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!({"dark": "custom/{gain}/"})),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success);

    let get_resp = get_settings(db.pool(), &bus, &cache).await.unwrap();
    assert_eq!(
        get_resp.settings.patterns_by_type.get("dark").map(String::as_str),
        Some("custom/{gain}/")
    );
}

#[tokio::test]
async fn update_patterns_by_type_accepts_empty_object() {
    let (db, bus, cache) = setup().await;
    // {} is the default; sending it back is a no-op, but it must validate.
    let req = SettingsUpdateRequest {
        key: "patternsByType".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!({})),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Noop);
}

#[tokio::test]
async fn update_patterns_by_type_rejects_invalid_pattern() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "patternsByType".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!({"dark": "{telescope}/"})),
    };
    let err = update_setting(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ValueInvalid);
}

#[tokio::test]
async fn update_patterns_by_type_rejects_bad_class_name() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "patternsByType".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!({"nope": "x/"})),
    };
    let err = update_setting(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ValueInvalid);
}

#[tokio::test]
async fn update_patterns_by_type_rejects_non_object() {
    let (db, bus, cache) = setup().await;
    let req = SettingsUpdateRequest {
        key: "patternsByType".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!(["dark"])),
    };
    let err = update_setting(db.pool(), &bus, &cache, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ValueInvalid);
}

// ── T057: emit_snapshot fires (FR-024) ────────────────────────────

#[tokio::test]
async fn emit_snapshot_fires_and_publishes_event() {
    let (db, bus, _cache) = setup().await;
    let dedupe = SnapshotDedupe::new();
    let mut rx = bus.subscribe();

    // Call emit_snapshot — must not error.
    emit_snapshot(db.pool(), &bus, "test_trigger", &dedupe).await.unwrap();

    // There must be at least one event published on the bus.
    // Use try_recv to avoid blocking — the publish is synchronous inside the call.
    let msg = rx.try_recv();
    assert!(msg.is_ok(), "emit_snapshot must publish a settings.snapshot event on the bus");
}

/// Issue #668: a periodic no-op snapshot (nothing changed since the last
/// PUBLISHED one) must not flood the activity log — mirrors
/// `resolve_pending`'s `considered == 0` suppression for the target
/// resolve-batch heartbeat.
#[tokio::test]
async fn emit_snapshot_suppresses_unchanged_repeat() {
    let (db, bus, _cache) = setup().await;
    let dedupe = SnapshotDedupe::new();

    emit_snapshot(db.pool(), &bus, "first", &dedupe).await.unwrap();
    let mut rx = bus.subscribe();

    // Second call, nothing changed — must be a no-op (no publish).
    emit_snapshot(db.pool(), &bus, "second", &dedupe).await.unwrap();
    assert!(
        rx.try_recv().is_err(),
        "an unchanged repeat snapshot must not publish a second settings.snapshot event"
    );
}

/// Issue #668: once a noisy key actually changes, the next snapshot must
/// still publish (suppression is value-sensitive, not a blanket mute).
#[tokio::test]
async fn emit_snapshot_publishes_again_after_a_real_change() {
    let (db, bus, cache) = setup().await;
    let dedupe = SnapshotDedupe::new();

    emit_snapshot(db.pool(), &bus, "first", &dedupe).await.unwrap();

    // `pattern` is a noisy key (descriptors::DESCRIPTORS) — change it.
    let req = SettingsUpdateRequest {
        key: "pattern".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!(["*.fits"])),
    };
    update_setting(db.pool(), &bus, &cache, &req).await.unwrap();

    let mut rx = bus.subscribe();
    emit_snapshot(db.pool(), &bus, "second", &dedupe).await.unwrap();
    assert!(rx.try_recv().is_ok(), "a snapshot after a real noisy-key change must still publish");
}

/// Table-driven structural-equality cases. `expected_eq` is whether the two
/// values are considered deep-equal (array order is significant).
#[rstest]
#[case(serde_json::json!("info"), serde_json::json!("info"), true)]
#[case(serde_json::json!("info"), serde_json::json!("debug"), false)]
#[case(
    serde_json::json!(["lights", "masters"]),
    serde_json::json!(["lights", "masters"]),
    true
)]
// Order matters: same elements in different order are not equal.
#[case(
    serde_json::json!(["lights", "masters"]),
    serde_json::json!(["masters", "lights"]),
    false
)]
fn settings_value_eq_cases(#[case] a: Value, #[case] b: Value, #[case] expected_eq: bool) {
    assert_eq!(settings_value_eq(&a, &b), expected_eq);
}

// ── T042: tools bundle_id seed default ─────────────────────────────────

/// Resolving `tools.<id>.bundle_id` with no stored override returns the
/// seed profile's bundle ID for known tools (spec 018 T042).
#[test]
fn bundle_id_default_resolves_from_seed_for_pixinsight() {
    let val = default_value_for_key("tools.pixinsight.bundle_id");
    assert_eq!(
        val.as_str(),
        Some("com.pixinsight.PixInsight"),
        "pixinsight seed bundle_id must be the default"
    );
}

#[test]
fn bundle_id_default_resolves_from_seed_for_siril() {
    let val = default_value_for_key("tools.siril.bundle_id");
    assert_eq!(
        val.as_str(),
        Some("org.free-astro.siril"),
        "siril seed bundle_id must be the default"
    );
}

#[test]
fn bundle_id_default_is_null_for_unknown_tool() {
    let val = default_value_for_key("tools.photoshop.bundle_id");
    assert!(val.is_null(), "unknown tool must return null bundle_id default");
}

#[test]
fn overridable_keys_includes_expected_keys() {
    let keys = overridable_keys();
    // Issue #623: followSymlinks/hashOnScan retired from the overridable
    // set — they duplicated the canonical IngestionSettings document and
    // the override could never succeed for either.
    assert!(!keys.contains(&"hashOnScan".to_owned()), "hashOnScan must no longer be overridable");
    assert!(
        !keys.contains(&"followSymlinks".to_owned()),
        "followSymlinks must no longer be overridable"
    );
    // defaultProtection is the sole overridable key per the descriptor table.
    assert!(
        keys.contains(&"defaultProtection".to_owned()),
        "defaultProtection must be overridable"
    );
}

// ── T046: absorbed-key coverage ───────────────────────────────────────

/// (T046-a) `tools.<id>.bundle_id` update round-trip: persists and reads back.
///
/// validate_value happy path is already covered by `validate_value_accepts`
/// (`tools.pixinsight.bundle_id` with `"com.x"`). This test adds the missing
/// async write+read round-trip via `update_setting` + `resolve_setting`.
#[tokio::test]
async fn tools_bundle_id_update_round_trips() {
    let (db, bus, cache) = setup().await;

    let req = SettingsUpdateRequest {
        key: "tools.pixinsight.bundle_id".to_owned(),
        value: contracts_core::JsonAny::from(serde_json::json!("com.example.App")),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success);

    // Read back via resolve_setting (no source override).
    let resolved = resolve_setting(db.pool(), "tools.pixinsight.bundle_id", None).await.unwrap();
    assert_eq!(resolved, serde_json::json!("com.example.App"));
}

/// (T046-b) `devMode` is rejected in release builds (no `dev-tools` feature).
///
/// The `ValidationRule::DevMode` arm in `descriptors::check_rule` returns
/// `ErrorCode::ValueInvalid` when compiled without the `dev-tools` feature.
#[cfg(not(feature = "dev-tools"))]
#[test]
fn dev_mode_rejected_in_release_build() {
    let err = validate_value("devMode", &serde_json::json!(true))
        .expect_err("devMode must be rejected in release builds");
    assert_eq!(
        err.code,
        ErrorCode::ValueInvalid,
        "devMode release rejection must use ValueInvalid error code"
    );
}

/// (T046-c) `calibrationDarkTempTolerance` >= 0 range: positive value is accepted.
///
/// The -1.0 rejection is already in `validate_value_rejects`. This adds the
/// explicit positive `2.0` acceptance case as T046 requires both sides.
#[test]
fn calibration_dark_temp_tolerance_accepts_positive() {
    assert!(
        validate_value("calibrationDarkTempTolerance", &serde_json::json!(2.0)).is_ok(),
        "calibrationDarkTempTolerance must accept 2.0 (>= 0)"
    );
}

/// (T046-d) `imagetypNormalizationUserMappings` deep-equal noop.
///
/// Seed a non-default value, then send a structurally-identical array back.
/// `settings_value_eq` must detect equality and return `status == "noop"`.
/// No audit event should be emitted (the existing noop guard covers this).
#[tokio::test]
async fn imagetyp_mappings_deep_equal_noop() {
    let (db, bus, cache) = setup().await;

    // A non-default mapping (default is the empty vec []).
    let mapping = serde_json::json!([
        { "imagetypString": "BIAS FRAME", "frameType": "bias" }
    ]);

    // First update: must succeed (not noop — differs from empty default).
    let req = SettingsUpdateRequest {
        key: "imagetypNormalizationUserMappings".to_owned(),
        value: contracts_core::JsonAny::from(mapping.clone()),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success, "initial write must succeed");

    // Second update: structurally identical array — must be noop.
    let req2 = SettingsUpdateRequest {
        key: "imagetypNormalizationUserMappings".to_owned(),
        value: contracts_core::JsonAny::from(mapping.clone()),
    };
    let resp2 = update_setting(db.pool(), &bus, &cache, &req2).await.unwrap();
    assert_eq!(
        resp2.status,
        SettingsUpdateStatus::Noop,
        "structurally-identical imagetypNormalizationUserMappings must be noop"
    );
    assert!(resp2.audit_id.is_none(), "noop must not emit audit event");
}

/// (spec 051 US3, T022) `cleanupTypeOverrides`: a real change publishes
/// exactly one `SettingsChanged` bus event and produces an audit record;
/// re-saving the identical map afterwards is a noop that publishes zero
/// further events and no additional audit record (SC-003).
#[tokio::test]
async fn cleanup_type_overrides_emits_one_event_then_noop() {
    let (db, bus, cache) = setup().await;
    let mut rx = bus.subscribe();

    let overrides = serde_json::json!({"1": "Archive", "20": "Delete"});

    // First write: a real change from the empty-map default — must emit
    // exactly one event and produce an audit record.
    let req = SettingsUpdateRequest {
        key: "cleanupTypeOverrides".to_owned(),
        value: contracts_core::JsonAny::from(overrides.clone()),
    };
    let resp = update_setting(db.pool(), &bus, &cache, &req).await.unwrap();
    assert_eq!(resp.status, SettingsUpdateStatus::Success, "initial write must succeed");
    assert!(resp.audit_id.is_some(), "real change must emit an audit event");

    assert!(rx.try_recv().is_ok(), "real change must publish exactly one SettingsChanged event");
    assert!(
        rx.try_recv().is_err(),
        "real change must publish exactly one SettingsChanged event, not more"
    );

    // Second write: structurally identical map — must be a noop, with no
    // further audit record and no further bus event.
    let req2 = SettingsUpdateRequest {
        key: "cleanupTypeOverrides".to_owned(),
        value: contracts_core::JsonAny::from(overrides),
    };
    let resp2 = update_setting(db.pool(), &bus, &cache, &req2).await.unwrap();
    assert_eq!(
        resp2.status,
        SettingsUpdateStatus::Noop,
        "structurally-identical cleanupTypeOverrides must be noop"
    );
    assert!(resp2.audit_id.is_none(), "noop must not emit audit event");
    assert!(rx.try_recv().is_err(), "noop re-save must publish zero events");
}

// ── Property tests ─────────────────────────────────────────────────────
//
// Invariants over arbitrary input. Proptest uses a deterministic default
// RNG seed unless `PROPTEST_RNG_SEED` is set, so failures reproduce.

proptest! {
    // is_valid_key never panics on arbitrary input.
    #[test]
    fn is_valid_key_never_panics(s in ".*") {
        let _ = is_valid_key(&s);
    }

    // validate_value never panics for arbitrary keys paired with a few
    // representative value shapes.
    #[test]
    fn validate_value_never_panics(s in ".*") {
        for v in [
            serde_json::json!(null),
            serde_json::json!(true),
            serde_json::json!(0),
            serde_json::json!("x"),
            serde_json::json!([]),
            serde_json::json!({}),
        ] {
            let _ = validate_value(&s, &v);
        }
    }

    // settings_value_eq is reflexive: any value equals itself.
    #[test]
    fn settings_value_eq_is_reflexive(s in ".*") {
        let v = serde_json::json!(s);
        prop_assert!(settings_value_eq(&v, &v));
    }

    // settings_value_eq is symmetric for arbitrary string pairs.
    #[test]
    fn settings_value_eq_is_symmetric(a in ".*", b in ".*") {
        let va = serde_json::json!(a);
        let vb = serde_json::json!(b);
        prop_assert_eq!(settings_value_eq(&va, &vb), settings_value_eq(&vb, &va));
    }

    // hashOnScan accepts exactly its three allowed variants and rejects all
    // other strings — round-trips the enum contract over arbitrary input.
    #[test]
    fn hash_on_scan_accepts_only_allowed(s in ".*") {
        let allowed = ["lazy", "eager", "off"].contains(&s.as_str());
        let v = serde_json::json!(s);
        prop_assert_eq!(validate_value("hashOnScan", &v).is_ok(), allowed);
    }

    // calibrationAgingThresholdDays bounds: a value validates iff it lies in [1, 3650].
    #[test]
    fn aging_threshold_bounds(n in -10_000i64..10_000i64) {
        #[allow(clippy::cast_precision_loss)]
        let in_range = (1..=3650).contains(&n);
        let v = serde_json::json!(n);
        prop_assert_eq!(
            validate_value("calibrationAgingThresholdDays", &v).is_ok(),
            in_range
        );
    }
}
