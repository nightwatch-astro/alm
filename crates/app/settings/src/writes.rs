use app_core_cache::SnapshotCache;

use super::{
    bus_err, caches, db_err, default_value_for_key, descriptors, is_global_protection_default_key,
    is_noisy_audited_key, is_valid_key, protection_repo, repo, settings_entity_id,
    settings_value_eq, validate_value, AuditLogEntry, ContractError, EntityId, EntityType,
    ErrorCode, ErrorSeverity, EventBus, Outcome, ProtectionDefaultChanged, RestoreDefaultsRequest,
    RestoreDefaultsResponse, RestoreDefaultsStatus, SetSourceOverrideRequest,
    SetSourceOverrideResponse, SettingsChanged, SettingsSnapshot, SettingsState,
    SettingsUpdateRequest, SettingsUpdateResponse, SettingsUpdateStatus, Severity, Source,
    SqlitePool, Timestamp, Value, GLOBAL_PROTECTION_DEFAULT_SCOPE,
    TOPIC_PROTECTION_DEFAULT_CHANGED, TOPIC_SETTINGS_CHANGED, TOPIC_SETTINGS_SNAPSHOT,
};

// ── update_setting ────────────────────────────────────────────────────────

/// Write a single settings key (T013).
///
/// Behaviour:
/// - Returns `"key.unknown"` for keys that are not valid v1 keys.
/// - Returns `"value.invalid"` when schema validation fails.
/// - Returns `status = "noop"` when the incoming value is deep-equal to the
///   currently stored (or default) value (A4, R4.1).
/// - For non-noisy keys: upserts the row and emits a `settings.changed` audit
///   event.
/// - For noisy keys: upserts the row without emitting a per-change audit event
///   (snapshot is emitted separately by T020).
///
/// # Errors
///
/// Returns `ContractError` with code `"key.unknown"` or `"value.invalid"`, or
/// an internal error on database/audit failure.
pub async fn update_setting(
    pool: &SqlitePool,
    bus: &EventBus,
    cache: &SnapshotCache<SettingsState>,
    req: &SettingsUpdateRequest,
) -> Result<SettingsUpdateResponse, ContractError> {
    let key = &req.key;
    let is_protection_default = is_global_protection_default_key(key);

    // 1. Validate key.
    if !is_valid_key(key) {
        write_settings_refusal(bus, is_protection_default, key, "key.unknown").await?;
        return Err(ContractError::new(
            ErrorCode::KeyUnknown,
            format!("unknown settings key: {key}"),
            ErrorSeverity::Warning,
            false,
        ));
    }

    // 2. Validate value.
    let new_value = &req.value.0;
    if let Err(e) = validate_value(key, new_value) {
        write_settings_refusal(bus, is_protection_default, key, "value.invalid").await?;
        return Err(e);
    }

    // 3. Load current stored value (or default). Global protection-default
    // keys read/write the dedicated `protection_defaults` table (T-005).
    let prior_raw = if is_protection_default {
        protection_repo::get_protection_default(pool, GLOBAL_PROTECTION_DEFAULT_SCOPE, key)
            .await
            .map_err(db_err)?
    } else {
        repo::get_raw(pool, key).await.map_err(db_err)?
    };
    let prior_value = prior_raw.clone().unwrap_or_else(|| default_value_for_key(key));

    // 4. No-op guard.
    if settings_value_eq(&prior_value, new_value) {
        return Ok(SettingsUpdateResponse {
            status: SettingsUpdateStatus::Noop,
            key: key.clone(),
            prior_value: contracts_core::JsonAny::from(prior_value),
            new_value: contracts_core::JsonAny::from(new_value.clone()),
            audit_id: None,
        });
    }

    // 5. Persist.
    if is_protection_default {
        protection_repo::set_protection_default(
            pool,
            GLOBAL_PROTECTION_DEFAULT_SCOPE,
            key,
            new_value,
        )
        .await
        .map_err(db_err)?;
    } else {
        repo::set_raw(pool, key, new_value).await.map_err(db_err)?;
    }

    // Cache invalidation fan-out (F0 in-memory caching layer): fires only
    // after the write above has committed, never before.
    // `cache` is the per-instance slot passed by the caller; also invalidate
    // the process-global shim so legacy callers that read through it stay
    // consistent during incremental migration.
    cache.invalidate();
    caches::invalidate_settings_bag();
    app_core_calibration::caches::invalidate_calibration_config();
    if is_protection_default {
        app_core_cache::invalidate_protection_defaults();
    }

    // 6. Emit durable audit row + live event (FR-130/FR-131, T122). Global
    // protection-default keys ALWAYS audit under `EntityType::Protection` via
    // `protection.default.changed` (T-004), overriding the noisy-key
    // no-audit policy — `protectedCategories` is `noisy` for the generic
    // `settings.changed` topic but is a named exception here (spec 016
    // plan.md E-016-3: "MUST emit `protection.default.changed` whenever it is
    // updated"). Non-protection noisy keys audit only when in
    // `NOISY_AUDITED_KEYS` (durable-data, e.g. `pattern`); the rest
    // (`rememberFollowLogs`) are UI state and stay fully exempt (FR-134).
    let is_noisy = descriptors::is_noisy(key.as_str());
    let audit_id = if !is_protection_default && is_noisy && !is_noisy_audited_key(key.as_str()) {
        None
    } else {
        let action = if is_protection_default {
            "settings.protection_default.update"
        } else {
            "settings.update"
        };
        let id = write_settings_applied_audit(
            bus,
            is_protection_default,
            action,
            key,
            &prior_value,
            new_value,
        )
        .await?;
        Some(id.as_uuid().to_string())
    };

    Ok(SettingsUpdateResponse {
        status: SettingsUpdateStatus::Success,
        key: key.clone(),
        prior_value: contracts_core::JsonAny::from(prior_value),
        new_value: contracts_core::JsonAny::from(new_value.clone()),
        audit_id,
    })
}

/// Write a durable `Outcome::Applied` audit row + live event for an accepted
/// settings write (T122). Shared by `update_setting`'s success path and
/// `restore_defaults`'s per-key loop — both pick `EntityType`/topic/payload
/// the same way based on `is_protection_default` (protection-default keys
/// audit under `EntityType::Protection` via `protection.default.changed`,
/// spec 016 T-004; everything else audits under `EntityType::Settings` via
/// `settings.changed`).
pub async fn write_settings_applied_audit(
    bus: &EventBus,
    is_protection_default: bool,
    action: &str,
    key: &str,
    prior_value: &Value,
    new_value: &Value,
) -> Result<domain_core::ids::AuditId, ContractError> {
    let entity_type =
        if is_protection_default { EntityType::Protection } else { EntityType::Settings };
    let entry = AuditLogEntry::new(
        entity_type,
        settings_entity_id(key),
        action,
        "user",
        Outcome::Applied,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_payload(serde_json::json!({"key": key, "before": prior_value, "after": new_value}));
    let at = Timestamp::now_iso();

    if is_protection_default {
        bus.write_audit(
            entry,
            TOPIC_PROTECTION_DEFAULT_CHANGED,
            Source::User,
            ProtectionDefaultChanged {
                scope: GLOBAL_PROTECTION_DEFAULT_SCOPE.to_owned(),
                key: key.to_owned(),
                old: Some(prior_value.clone()),
                new: new_value.clone(),
                changed_at: at,
            },
        )
        .await
        .map_err(bus_err)
    } else {
        bus.write_audit(
            entry,
            TOPIC_SETTINGS_CHANGED,
            Source::User,
            SettingsChanged {
                key: key.to_owned(),
                prior_value: prior_value.clone(),
                new_value: new_value.clone(),
                at,
            },
        )
        .await
        .map_err(bus_err)
    }
}

/// Write a durable `Outcome::Refused` audit row for a rejected `settings.update`
/// attempt (FR-130/FR-134, T127) before returning the validation error to the
/// caller. No before/after pair — validation is rejected before any read.
/// `is_protection_default` picks the same `EntityType` the applied path would
/// have used, so a refused global-protection-default write (e.g. `T123`'s
/// "protection refused" coverage) is tagged `EntityType::Protection`, not
/// `EntityType::Settings`.
pub async fn write_settings_refusal(
    bus: &EventBus,
    is_protection_default: bool,
    key: &str,
    reason_code: &str,
) -> Result<(), ContractError> {
    let entity_type =
        if is_protection_default { EntityType::Protection } else { EntityType::Settings };
    let entry = AuditLogEntry::new(
        entity_type,
        settings_entity_id(key),
        "settings.update",
        "user",
        Outcome::Refused,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_reason_code(reason_code.to_owned())
    .with_payload(serde_json::json!({"key": key}));
    bus.write_audit(
        entry,
        TOPIC_SETTINGS_CHANGED,
        Source::User,
        serde_json::json!({
            "key": key,
            "outcome": "refused",
            "reasonCode": reason_code,
        }),
    )
    .await
    .map_err(bus_err)?;
    Ok(())
}

// ── restore_defaults ──────────────────────────────────────────────────────

/// Restore one or more settings keys to their in-code defaults (T027).
///
/// - Empty `keys` slice restores all v1 keys.
/// - Keys already at default are collected in `already_at_default` and skipped
///   (no write, no audit — R-3.1).
/// - When all keys are already at default, returns `status = "noop"`.
/// - For each key actually restored, one audit event is emitted.
///
/// # Errors
///
/// Returns `ContractError` with code `"key.unknown"` if any key is not a valid
/// v1 key (structured-path keys accepted). Returns internal errors on DB/audit
/// failure.
pub async fn restore_defaults(
    pool: &SqlitePool,
    bus: &EventBus,
    cache: &SnapshotCache<SettingsState>,
    req: &RestoreDefaultsRequest,
) -> Result<RestoreDefaultsResponse, ContractError> {
    let keys_to_restore: Vec<String> = if req.keys.is_empty() {
        descriptors::all_keys().map(str::to_owned).collect()
    } else {
        // Validate all requested keys first.
        for key in &req.keys {
            if !is_valid_key(key) {
                return Err(ContractError::new(
                    ErrorCode::KeyUnknown,
                    format!("unknown settings key: {key}"),
                    ErrorSeverity::Warning,
                    false,
                ));
            }
        }
        req.keys.clone()
    };

    // Read current values outside the transaction — reads are cheap and
    // non-destructive; only the writes must be atomic.
    struct PendingWrite {
        key: String,
        current_val: Value,
        default_val: Value,
        is_protection_default: bool,
    }

    let mut pending: Vec<PendingWrite> = Vec::new();
    let mut already_at_default = Vec::new();

    for key in &keys_to_restore {
        let default_val = default_value_for_key(key);
        let is_protection_default = is_global_protection_default_key(key);
        let current_raw = if is_protection_default {
            protection_repo::get_protection_default(pool, GLOBAL_PROTECTION_DEFAULT_SCOPE, key)
                .await
                .map_err(db_err)?
        } else {
            repo::get_raw(pool, key).await.map_err(db_err)?
        };
        let current_val = current_raw.unwrap_or_else(|| default_val.clone());

        if settings_value_eq(&current_val, &default_val) {
            already_at_default.push(key.clone());
        } else {
            pending.push(PendingWrite { key: key.clone(), current_val, default_val, is_protection_default });
        }
    }

    // Write all defaults in a single transaction — all-or-nothing.
    if !pending.is_empty() {
        let mut tx = pool.begin().await.map_err(|e| db_err(e.into()))?;
        for pw in &pending {
            if pw.is_protection_default {
                protection_repo::set_protection_default_with_conn(
                    &mut *tx,
                    GLOBAL_PROTECTION_DEFAULT_SCOPE,
                    &pw.key,
                    &pw.default_val,
                )
                .await
                .map_err(db_err)?;
            } else {
                repo::set_raw_with_conn(&mut *tx, &pw.key, &pw.default_val)
                    .await
                    .map_err(db_err)?;
            }
        }
        tx.commit().await.map_err(|e| db_err(e.into()))?;
    }

    // Emit audit events after the commit (T027, FR-130).
    let mut restored = Vec::new();
    let mut restored_protection_default = false;

    for pw in pending {
        // Write durable audit row + live event (even for noisy keys — restore
        // is an explicit action, FR-130).
        write_settings_applied_audit(
            bus,
            pw.is_protection_default,
            "settings.restore_defaults",
            &pw.key,
            &pw.current_val,
            &pw.default_val,
        )
        .await?;

        restored.push(pw.key);
        if pw.is_protection_default {
            restored_protection_default = true;
        }
    }

    // Cache invalidation fan-out: one shot after the loop (not per-key) since
    // both snapshots are single-slot whole-bag caches. Invalidate both the
    // per-instance slot and the process-global shim for incremental migration.
    if !restored.is_empty() {
        cache.invalidate();
        caches::invalidate_settings_bag();
        app_core_calibration::caches::invalidate_calibration_config();
        if restored_protection_default {
            app_core_cache::invalidate_protection_defaults();
        }
    }

    let status = if restored.is_empty() {
        RestoreDefaultsStatus::Noop
    } else {
        RestoreDefaultsStatus::Success
    };

    Ok(RestoreDefaultsResponse { status, restored, already_at_default })
}

// ── set_source_override ───────────────────────────────────────────────────

/// Set a per-source override for an overridable settings key (T023).
///
/// Validates that `key` is overridable per `descriptors::DESCRIPTORS`
/// (currently just `defaultProtection` — issue #623 removed `followSymlinks`/
/// `hashOnScan` from this list, since they duplicated the canonical
/// `IngestionSettings` document and the per-source override never worked for
/// either). Validates the value type. The `source_id` existence check is
/// best-effort: since the sources repository is in a different crate slice,
/// callers may perform that check before calling this function.
///
/// # Errors
///
/// Returns `ContractError` with code `"key.unoverridable"` for non-overridable
/// keys. Returns `"value.invalid"` for type-invalid values.
pub async fn set_source_override(
    pool: &SqlitePool,
    bus: &EventBus,
    cache: &SnapshotCache<SettingsState>,
    req: &SetSourceOverrideRequest,
) -> Result<SetSourceOverrideResponse, ContractError> {
    let key = &req.key;
    // FR-130/T122 FIX (review round 1 #1): a per-source override is a durable
    // settings mutation regardless of whether a live caller exists today —
    // entity_id keys on (source_id, key) so a source's override history for
    // one key correlates under a single audit entity, distinct from the
    // global-key entity `update_setting` uses.
    let entity_seed = format!("{}:{key}", req.source_id);

    if !descriptors::is_overridable(key.as_str()) {
        write_settings_override_refusal(bus, &entity_seed, key, "key.unoverridable").await?;
        return Err(ContractError::new(
            ErrorCode::KeyUnoverridable,
            format!("key {key} cannot be overridden per source"),
            ErrorSeverity::Warning,
            false,
        ));
    }

    let value = &req.value.0;
    if let Err(e) = validate_value(key, value) {
        write_settings_override_refusal(bus, &entity_seed, key, "value.invalid").await?;
        return Err(e);
    }

    repo::set_source_override(pool, &req.source_id, key, value).await.map_err(db_err)?;

    // `get_settings`'s bag is global-only (no source_id), so a per-source
    // override never actually changes it; invalidating anyway is a cheap,
    // safe no-op that keeps this write site consistent with the other two.
    // Invalidate both per-instance slot and process-global shim.
    cache.invalidate();
    caches::invalidate_settings_bag();

    let entry = AuditLogEntry::new(
        EntityType::Settings,
        settings_entity_id(&entity_seed),
        "settings.source_override.set",
        "user",
        Outcome::Applied,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_payload(serde_json::json!({"sourceId": req.source_id, "key": key, "after": value}));
    bus.write_audit(
        entry,
        TOPIC_SETTINGS_CHANGED,
        Source::User,
        serde_json::json!({"sourceId": req.source_id, "key": key}),
    )
    .await
    .map_err(bus_err)?;

    Ok(SetSourceOverrideResponse { source_id: req.source_id.clone(), key: key.clone() })
}

/// Write a durable `Outcome::Refused` row for a rejected `set_source_override`
/// attempt (FR-130, review round 1 #1).
pub async fn write_settings_override_refusal(
    bus: &EventBus,
    entity_seed: &str,
    key: &str,
    reason_code: &str,
) -> Result<(), ContractError> {
    let entry = AuditLogEntry::new(
        EntityType::Settings,
        settings_entity_id(entity_seed),
        "settings.source_override.set",
        "user",
        Outcome::Refused,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_reason_code(reason_code.to_owned())
    .with_payload(serde_json::json!({"key": key}));
    bus.write_audit(
        entry,
        TOPIC_SETTINGS_CHANGED,
        Source::User,
        serde_json::json!({"key": key, "outcome": "refused", "reasonCode": reason_code}),
    )
    .await
    .map_err(bus_err)?;
    Ok(())
}

// ── resolve_setting ───────────────────────────────────────────────────────

/// Resolve the effective value for a settings key, honouring per-source
/// override → global setting → in-code default (T024, data-model.md §Resolution).
///
/// # Errors
///
/// Returns `ContractError` on database failure.
pub async fn resolve_setting(
    pool: &SqlitePool,
    key: &str,
    source_id: Option<&str>,
) -> Result<Value, ContractError> {
    // 1. Per-source override (only for overridable keys).
    if let Some(sid) = source_id {
        if descriptors::is_overridable(key) {
            if let Some(v) = persistence_lifecycle::repositories::settings::get_source_override_raw(
                pool, sid, key,
            )
            .await
            .map_err(db_err)?
            {
                return Ok(v);
            }
        }
    }

    // 2. Global setting. Global protection-default keys are resolved from the
    // dedicated `protection_defaults` table (spec 016 T-005) so this read path
    // never disagrees with `app_core::protection::load_global_protection`.
    if is_global_protection_default_key(key) {
        if let Some(v) =
            protection_repo::get_protection_default(pool, GLOBAL_PROTECTION_DEFAULT_SCOPE, key)
                .await
                .map_err(db_err)?
        {
            return Ok(v);
        }
    } else if let Some(v) = repo::get_raw(pool, key).await.map_err(db_err)? {
        return Ok(v);
    }

    // 3. In-code default.
    Ok(default_value_for_key(key))
}

// ── emit_snapshot ──────────────────────────────────────────────────────────

/// Dedupe state for [`emit_snapshot`]: the noisy-key values (as a JSON object)
/// from the most recently PUBLISHED `settings.snapshot` event (issue #668).
///
/// Owned by the snapshot loop that drives `emit_snapshot`, so suppression is
/// scoped to that loop rather than to the whole process. Not invalidated by
/// `update_setting`/`restore_defaults`/`set_source_override`: those already
/// emit their own real `settings.changed`/`protection.default.changed`
/// events, so a later snapshot correctly finds "no further noisy-key change"
/// and stays quiet until a *noisy* key changes.
pub type SnapshotDedupe = app_core_cache::SnapshotCache<Value>;

/// Emit a `settings.snapshot` audit event (T020).
///
/// Called at session start and after the 5-minute inactivity debounce
/// (the debounce timer is owned by the caller/command layer).
///
/// Issue #668: a periodic snapshot whose noisy-key values are byte-identical
/// to the last one PUBLISHED via `dedupe` is a no-op heartbeat — it is skipped
/// rather than published, mirroring `target.resolve_batch.completed`'s
/// suppression on `considered == 0` (both stop a periodic internal event from
/// flooding the activity log when there is nothing new to report). The first
/// snapshot against a fresh `dedupe` always publishes.
///
/// # Errors
///
/// Returns `ContractError` on database or audit failure.
pub async fn emit_snapshot(
    pool: &SqlitePool,
    bus: &EventBus,
    trigger: &str,
    dedupe: &SnapshotDedupe,
) -> Result<(), ContractError> {
    // Collect current values of noisy keys.
    let mut noisy_values = serde_json::Map::new();
    for key in descriptors::noisy_keys() {
        let val = repo::get_raw(pool, key)
            .await
            .map_err(db_err)?
            .unwrap_or_else(|| default_value_for_key(key));
        noisy_values.insert(key.to_owned(), val);
    }
    let noisy_keys = Value::Object(noisy_values);

    // Skip the publish when nothing changed since the last one we actually
    // published (#668).
    if dedupe.load().as_deref() == Some(&noisy_keys) {
        return Ok(());
    }

    let at = Timestamp::now_iso();
    bus.publish(
        TOPIC_SETTINGS_SNAPSHOT,
        Source::System,
        SettingsSnapshot { trigger: trigger.to_owned(), noisy_keys: noisy_keys.clone(), at },
    )
    .await
    .map_err(bus_err)?;
    dedupe.store(std::sync::Arc::new(noisy_keys));

    Ok(())
}
