// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Settings use cases (spec 018, T006/T013/T018/T019/T023/T024/T027).
//!
//! Entry points:
//! - `get_settings` — load all settings, hydrating defaults for missing rows,
//!   repairing invalid stored values (T018, T019).
//! - `update_setting` — write a single key with no-op guard and audit emit (T013).
//! - `restore_defaults` — restore one, several, or all keys to their in-code
//!   defaults (T027).
//! - `set_source_override` — set a per-source override for an overridable key (T023).
//! - `resolve_setting` — resolution order: per-source → global → default (T024).
//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b). Its only
//! cross-module dependency was on the now-extracted `app_core_errors` leaf.
//! `app_core` re-exports this crate at `app_core::settings` so the public
//! surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use audit::bus::EventBus;
use audit::event_bus::{
    ProtectionDefaultChanged, SettingsChanged, SettingsRepair, SettingsSnapshot, Source,
    TOPIC_PROTECTION_DEFAULT_CHANGED, TOPIC_SETTINGS_CHANGED, TOPIC_SETTINGS_REPAIR,
    TOPIC_SETTINGS_SNAPSHOT,
};
use audit::{AuditLogEntry, Outcome, Severity};
use contracts_core::settings::{
    RestoreDefaultsRequest, RestoreDefaultsResponse, RestoreDefaultsStatus,
    SetSourceOverrideRequest, SetSourceOverrideResponse, SettingsGetResponse, SettingsState,
    SettingsUpdateRequest, SettingsUpdateResponse, SettingsUpdateStatus,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::EntityId;
use domain_core::lifecycle::data_asset::EntityType;
use persistence_lifecycle::repositories::settings as repo;
use persistence_plans::repositories::source_protection as protection_repo;
use serde_json::Value;
use sqlx::SqlitePool;

// ── Settings descriptor table (US11 T144) ────────────────────────────────
//
// The stable key registry + per-key rules now live in one place:
// `descriptors::DESCRIPTORS`. The key set, noisy/overridable membership,
// value validation, `SettingsState` hydration (`apply_value_to_state`), and
// in-code defaults (`default_value_for_key`) are all derived from that single
// table.
mod descriptors;

// ── Ingestion settings (spec 030, package P12) ────────────────────────────
//
// Stored as a single JSON document via the low-level key/value store below
// (`repo::get_raw`/`set_raw`), not through the descriptor table — see the
// module doc comment in `ingestion.rs` for the rationale.
pub mod ingestion;

// ── Settings schema migration harness (spec 018 US5, T030 / T031) ────────────
pub mod migrate;

// ── Per-root reconcile/detection configuration (spec 048 T005) ──────────────
pub mod root_config;

// ── In-memory settings-bag snapshot cache (F0 foundation) ────────────────────
//
// Defines the per-instance `SettingsCaches` struct and the process-global
// `settings_bag()`/`invalidate_settings_bag()` shims used by call sites not
// yet migrated to per-instance caches.
pub mod caches;

// ── Error mapping ──────────────────────────────────────────────────────────
//
// Canonical mappers live in `app_core_errors` (US11 T142). `db_err` now routes
// `DbError::NotFound` to the recoverable `Blocking`/`retryable=false`
// classification instead of the previous blanket `Fatal` (L2 divergence fix).
use app_core_errors::{bus_err, db_err};

// ── ISO timestamp helper ──────────────────────────────────────────────────
// Canonical helper lives in `domain_core::ids::Timestamp` (US11 T140).
use domain_core::ids::Timestamp;

mod keys;
mod read;
mod repair;
mod validation;
mod writes;

#[cfg(test)]
mod tests;

pub use keys::{
    is_global_protection_default_key, is_ui_state_key, is_valid_key, overridable_keys, stable_keys,
};
pub use read::get_settings;
pub use validation::{settings_value_eq, validate_value};
pub use writes::{
    emit_snapshot, resolve_setting, restore_defaults, set_source_override, update_setting,
    SnapshotDedupe,
};

use keys::{
    is_catalogues_enabled_key, is_locale_key, is_noisy_audited_key, is_tools_auto_detected_key,
    is_tools_bundle_id_key, is_tools_enabled_key, is_tools_executable_path_key,
    is_workflow_profile_attribution_window_key, is_workflow_profile_watch_extensions_key,
    settings_entity_id, GLOBAL_PROTECTION_DEFAULT_SCOPE, SHIPPED_LOCALES,
};
#[cfg(test)]
use read::apply_value_to_state;
use read::default_value_for_key;
