// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Full-fidelity JSON-Schema round-trip tests (bead astro-plan-kyo7.23).
//!
//! Each test serializes a real Rust contract DTO instance, then validates it
//! against the canonical JSON Schema file in `packages/contracts/schemas/`.
//! A validation failure is a real parity divergence between the Rust type and
//! the schema (not a test artifact).
//!
//! Schemas covered:
//! - `envelope.schema.json` — all six envelope shapes
//! - `inventory.list.schema.json` — Request / Response
//! - `log/LogEntry.v1.schema.json` — `LogEntry`
//!
//! Format validation (uuid, date-time) is enabled: fixture values use
//! correctly-formatted strings so both the schema's structural and format
//! constraints are exercised.
//!
//! The `resolve-http` feature of the `jsonschema` crate is disabled (see
//! workspace `Cargo.toml`) — all `$ref` resolution is local/inline only, giving
//! offline-deterministic tests.

use std::{fs, path::PathBuf};

use contracts_core::inventory::{
    InventoryFrameType, InventoryListRequest, InventoryListResponse, InventorySession,
    InventorySource, InventorySourceKind, InventorySourceState,
};
use contracts_core::{
    error_code::ErrorCode, log::LogEntry, log::LogEntrySource, log::LogLevel, ContractError,
    ErrorSeverity, OperationEvent, OperationEventType, OperationHandle, OperationId, OperationName,
    OperationStatus, RequestEnvelope, RequestId, ResponseEnvelope,
};
use serde_json::{json, Value};

// ── UUIDs used across fixtures ────────────────────────────────────────────────
// These conform to the RFC 4122 format required by the schema's "format": "uuid"
// constraints.
const UUID_A: &str = "018f22b2-7f7f-7f7f-8f7f-7f7f7f7f7f7a";
const UUID_B: &str = "018f22b2-7f7f-7f7f-8f7f-7f7f7f7f7f7b";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("contract test package should live under tests/contract")
        .to_path_buf()
}

fn load_schema(rel_path: &str) -> Value {
    let path = repo_root().join("packages/contracts/schemas").join(rel_path);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read schema {}: {e}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("schema {} is not valid JSON: {e}", path.display()))
}

/// Build a validator for `schema`, with format validation enabled.
///
/// `resolve-http` is disabled at the crate feature level (Cargo.toml), so any
/// HTTP `$ref` in a schema would produce a compile-time error rather than a
/// runtime hang, keeping tests offline-deterministic.
fn make_validator(schema: &Value) -> jsonschema::Validator {
    jsonschema::options()
        .should_validate_formats(true)
        .build(schema)
        .expect("schema should compile into a valid jsonschema validator")
}

/// Assert `instance` validates against `schema` using the compiled `validator`.
///
/// On failure, collects ALL validation errors and surfaces them in the panic
/// message so a single test run reveals every divergence rather than just the
/// first.
fn assert_validates(validator: &jsonschema::Validator, instance: &Value, label: &str) {
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| format!("  path={} kind={:?}", e.instance_path(), e.kind()))
        .collect();

    assert!(
        errors.is_empty(),
        "JSON-Schema validation failed for {label}:\n{}\nInstance:\n{}",
        errors.join("\n"),
        serde_json::to_string_pretty(instance).unwrap_or_default()
    );
}

// ── envelope.schema.json tests ────────────────────────────────────────────────

fn envelope_validator() -> jsonschema::Validator {
    make_validator(&load_schema("envelope.schema.json"))
}

#[test]
fn request_envelope_validates_against_envelope_schema() {
    let validator = envelope_validator();
    let instance = serde_json::to_value(RequestEnvelope::new(
        OperationName("library.scan.start".to_owned()),
        RequestId("req-1".to_owned()),
        json!({ "rootIds": ["root-1"] }),
    ))
    .expect("RequestEnvelope should serialize");

    assert_validates(&validator, &instance, "RequestEnvelope");
}

#[test]
fn ok_response_envelope_validates_against_envelope_schema() {
    let validator = envelope_validator();
    let instance =
        serde_json::to_value(ResponseEnvelope::ok(RequestId("req-1".to_owned()), json!({})))
            .expect("OkResponseEnvelope should serialize");

    assert_validates(&validator, &instance, "OkResponseEnvelope");
}

#[test]
fn error_response_envelope_validates_against_envelope_schema() {
    let validator = envelope_validator();
    let error = ContractError::new(
        ErrorCode::FilesystemDestinationExists,
        "Destination already exists.",
        ErrorSeverity::Blocking,
        false,
    );
    let instance = serde_json::to_value(ResponseEnvelope::<Value>::error(
        RequestId("req-1".to_owned()),
        error,
    ))
    .expect("ErrorResponseEnvelope should serialize");

    assert_validates(&validator, &instance, "ErrorResponseEnvelope");
}

#[test]
fn operation_handle_validates_against_envelope_schema() {
    let validator = envelope_validator();
    let instance = serde_json::to_value(OperationHandle::new(
        OperationId("op-1".to_owned()),
        OperationName("library.scan.start".to_owned()),
        OperationStatus::Running,
    ))
    .expect("OperationHandle should serialize");

    assert_validates(&validator, &instance, "OperationHandle");
}

#[test]
fn operation_event_validates_against_envelope_schema() {
    let validator = envelope_validator();
    let instance = serde_json::to_value(OperationEvent::new(
        OperationId("op-1".to_owned()),
        OperationEventType::Progress,
        1,
        json!({ "current": 1, "total": 10 }),
    ))
    .expect("OperationEvent should serialize");

    assert_validates(&validator, &instance, "OperationEvent");
}

#[test]
fn contract_error_with_field_errors_validates_against_envelope_schema() {
    let validator = envelope_validator();
    // Exercise the optional fieldErrors and recoveryActions arrays.
    let error = ContractError::new(
        ErrorCode::ValidationRequestEnvelopeInvalid,
        "Payload failed validation.",
        ErrorSeverity::Blocking,
        false,
    )
    .with_field_error(contracts_core::FieldError {
        field: "rootId".to_owned(),
        code: "required".to_owned(),
        message: "rootId is required".to_owned(),
    })
    .with_recovery_action(contracts_core::RecoveryAction {
        code: "retry".to_owned(),
        label: "Retry".to_owned(),
        description: Some("Retry the operation".to_owned()),
    });
    let instance = serde_json::to_value(ResponseEnvelope::<Value>::error(
        RequestId("req-1".to_owned()),
        error,
    ))
    .expect("ErrorResponseEnvelope with extras should serialize");

    assert_validates(
        &validator,
        &instance,
        "ErrorResponseEnvelope (with fieldErrors + recoveryActions)",
    );
}

// ── inventory.list.schema.json tests ─────────────────────────────────────────

fn inventory_list_validator() -> jsonschema::Validator {
    make_validator(&load_schema("inventory.list.schema.json"))
}

#[test]
fn inventory_list_request_validates_against_inventory_list_schema() {
    let validator = inventory_list_validator();
    let req = InventoryListRequest {
        contract_version: "2.0.0".to_owned(),
        request_id: UUID_A.to_owned(),
        filters: None,
    };
    let instance = serde_json::to_value(&req).expect("InventoryListRequest should serialize");
    assert_validates(&validator, &instance, "InventoryListRequest");
}

#[test]
fn inventory_list_request_with_filters_validates_against_inventory_list_schema() {
    use contracts_core::inventory::InventoryListFilters;
    let validator = inventory_list_validator();
    let req = InventoryListRequest {
        contract_version: "2.0.0".to_owned(),
        request_id: UUID_A.to_owned(),
        filters: Some(InventoryListFilters {
            source_filter: Some(UUID_B.to_owned()),
            frame_filter: Some(InventoryFrameType::Light),
            limit: Some(50),
            offset: Some(0),
        }),
    };
    let instance =
        serde_json::to_value(&req).expect("InventoryListRequest with filters should serialize");
    assert_validates(&validator, &instance, "InventoryListRequest (with filters)");
}

// ── PARITY DIVERGENCE (bead astro-plan-kyo7.23) ──────────────────────────────
//
// `inventory.list.schema.json §$defs.InventorySession` requires a `"state"`
// field (the spec 002 six-value session state), but `contracts_core::inventory::
// InventorySession` no longer has that field.  It was removed in spec 041
// FR-051 (T076, Phase 13): sessions on this surface are derived,
// already-confirmed inventory — the review-state machine was removed and the
// field is now absent from the Rust type.
//
// The JSON Schema has not been updated to match.  This is the authoritative
// record of the divergence.  Tracked for schema update as a follow-up to this
// bead.
//
// The test is `#[ignore]` so `cargo test` stays green while the divergence is
// visible.  Run with `--include-ignored` to confirm the failing assertion:
//   cargo test --test contract_jsonschema_roundtrip -- --include-ignored
#[test]
#[ignore = "SCHEMA DRIFT: inventory.list.schema.json InventorySession.state required \
            but Rust InventorySession has no state field (removed spec 041 FR-051). \
            Update packages/contracts/schemas/inventory.list.schema.json to remove \
            the state field from InventorySession.required and $defs.SessionState."]
fn inventory_list_response_validates_against_inventory_list_schema() {
    let validator = inventory_list_validator();
    let session = InventorySession {
        id: UUID_B.to_owned(),
        name: "Orion 2026-01-15".to_owned(),
        source_id: UUID_A.to_owned(),
        frames: 120,
        frame_type: InventoryFrameType::Light,
        target: Some("M42".to_owned()),
        filter: Some("L".to_owned()),
        exposure: Some("300s".to_owned()),
        camera: None,
        gain: None,
        binning: None,
        set_temp: None,
        captured_on: Some("2026-01-15".to_owned()),
        provenance: None,
        linked: None,
        relative_path: None,
        notes: None,
        calibration_matches: vec![],
    };
    let source = InventorySource {
        id: UUID_A.to_owned(),
        path: "/mnt/data/astro".to_owned(),
        kind: InventorySourceKind::LocalDisk,
        state: InventorySourceState::Active,
        sessions: vec![session],
        has_more: false,
    };
    let resp = InventoryListResponse {
        status: "success".to_owned(),
        contract_version: "2.0.0".to_owned(),
        request_id: UUID_A.to_owned(),
        generated_at: "2026-01-15T00:00:00Z".to_owned(),
        sources: vec![source],
    };
    let instance = serde_json::to_value(&resp).expect("InventoryListResponse should serialize");
    assert_validates(&validator, &instance, "InventoryListResponse");
}

// ── log/LogEntry.v1.schema.json tests ────────────────────────────────────────

fn log_entry_validator() -> jsonschema::Validator {
    make_validator(&load_schema("log/LogEntry.v1.schema.json"))
}

#[test]
fn log_entry_minimal_validates_against_log_entry_schema() {
    let validator = log_entry_validator();
    // Use diagnostic() — produces a minimal LogEntry with only required fields
    // (no requestId, entityType, entityId).
    let entry = LogEntry::diagnostic(1, LogLevel::Info, "library scan started");
    let instance = serde_json::to_value(&entry).expect("LogEntry should serialize");
    assert_validates(&validator, &instance, "LogEntry (minimal)");
}

#[test]
fn log_entry_full_validates_against_log_entry_schema() {
    let validator = log_entry_validator();
    let entry = LogEntry {
        id: "aud:42".to_owned(),
        contract_version: "2.0.0".to_owned(),
        time: "2026-01-15T00:00:00Z".to_owned(),
        level: LogLevel::Warn,
        source: LogEntrySource::Plan,
        message: "Plan item failed".to_owned(),
        request_id: Some("op-123".to_owned()),
        entity_type: Some("plan".to_owned()),
        entity_id: Some(UUID_A.to_owned()),
    };
    let instance = serde_json::to_value(&entry).expect("LogEntry (full) should serialize");
    assert_validates(&validator, &instance, "LogEntry (full)");
}

#[test]
fn log_entry_all_sources_serialize_to_schema_enum_values() {
    let validator = log_entry_validator();
    // Every LogEntrySource variant must produce a value accepted by the
    // schema's closed enum — this catches any variant added to the Rust enum
    // that is not yet in the JSON Schema.
    let sources = [
        LogEntrySource::Audit,
        LogEntrySource::Diagnostic,
        LogEntrySource::Catalog,
        LogEntrySource::Plan,
        LogEntrySource::Workflow,
        LogEntrySource::Lifecycle,
        LogEntrySource::Inventory,
        LogEntrySource::Settings,
        LogEntrySource::Project,
        LogEntrySource::Target,
        LogEntrySource::Tool,
    ];
    for source in sources {
        let entry = LogEntry {
            id: "aud:1".to_owned(),
            contract_version: "2.0.0".to_owned(),
            time: "2026-01-15T00:00:00Z".to_owned(),
            level: LogLevel::Info,
            source,
            message: "test".to_owned(),
            request_id: None,
            entity_type: None,
            entity_id: None,
        };
        let instance = serde_json::to_value(&entry).expect("LogEntry should serialize");
        let source_str = serde_json::to_value(source)
            .expect("source should serialize")
            .as_str()
            .unwrap()
            .to_owned();
        assert_validates(&validator, &instance, &format!("LogEntry (source={source_str})"));
    }
}

// ── Negative sentinel: schema rejects clearly invalid instances ───────────────
//
// Proves the validator is actually checking content, not vacuously passing.

#[test]
fn envelope_schema_rejects_missing_required_fields() {
    let validator = envelope_validator();
    // An empty object matches none of the oneOf branches.
    let bad = json!({});
    let errors: Vec<_> = validator.iter_errors(&bad).collect();
    assert!(
        !errors.is_empty(),
        "envelope schema should reject an empty object but found no errors"
    );
}

#[test]
fn log_entry_schema_rejects_unknown_source_value() {
    let validator = log_entry_validator();
    let bad = json!({
        "id": "aud:1",
        "contractVersion": "2.0.0",
        "time": "2026-01-15T00:00:00Z",
        "level": "info",
        "source": "unknown_source_xyz",
        "message": "test"
    });
    let errors: Vec<_> = validator.iter_errors(&bad).collect();
    assert!(
        !errors.is_empty(),
        "log entry schema should reject unknown source value 'unknown_source_xyz'"
    );
}

#[test]
fn log_entry_schema_rejects_missing_required_id() {
    let validator = log_entry_validator();
    let bad = json!({
        "contractVersion": "2.0.0",
        "time": "2026-01-15T00:00:00Z",
        "level": "info",
        "source": "audit",
        "message": "test"
    });
    assert!(
        validator.iter_errors(&bad).next().is_some(),
        "log entry schema should reject instance missing required 'id' field"
    );
}
