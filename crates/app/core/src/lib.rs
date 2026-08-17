// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Application use-case orchestration boundary.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

// ── Per-domain crate split (spec 042 / T252–T253) ───────────────────────
//
// Six domain crates plus the `errors` leaf kernel were extracted into their own
// sibling crates under `crates/app/`. They are re-exported below under their
// original crate-root names so that `app_core::<domain>` paths remain
// byte-identical for `desktop_shell` and every other consumer. Every other
// module stays in-crate as a `pub mod` (file layout under `src/`), keeping the
// crate split per-domain rather than per-module.

// `calibration` was extracted into its own leaf crate. Re-export it under the
// original `app_core::calibration` path so consumers keep the byte-identical
// surface.
pub use app_core_calibration as calibration;
// `errors` was extracted into its own leaf crate (zero `crate::` refs). The
// folded-back in-crate modules reference it as `crate::errors::*`, which resolves
// through this re-export, so every consumer path stays byte-identical.
pub use app_core_errors as errors;
// `inbox` was extracted into its own domain crate. Re-export it under the
// original `app_core::inbox` path.
pub use app_core_inbox as inbox;
// `lifecycle` was extracted into its own domain crate. Re-export it under the
// original `app_core::lifecycle` path; the per-module re-exports below keep
// `app_core::<module>` byte-identical.
pub use app_core_lifecycle as lifecycle;
// `projects` was extracted into its own domain crate. Re-export it under the
// original `app_core::projects` path; the per-module re-exports below keep
// `app_core::<module>` byte-identical.
pub use app_core_projects as projects;
// `settings` was extracted into its own domain crate. Re-export it under the
// original `app_core::settings` path.
pub use app_core_settings as settings;
// `targets` was extracted into its own domain crate. Re-export it under the
// original `app_core::targets` path; the per-module re-exports below keep
// `app_core::<module>` byte-identical.
pub use app_core_targets as targets;

// Re-export grouped modules under their original crate-root paths.
// `equipment` historically resolved to `app_core::equipment`; it now lives in
// the extracted `app_core_calibration` crate and is re-exported here.
pub use calibration::equipment;
pub use lifecycle::{
    artifact, ledger_use_case, lifecycle_use_case, provenance_use_case, transition_use_case,
};
pub use projects::{
    prepared_views, project_health, project_manifests, project_notes, project_setup,
    source_view_generate, source_view_verify,
};
pub use targets::{
    ingest_resolution, ingest_sessions, resolver_settings, target_dto, target_favourites,
    target_management, target_resolve, target_search,
};

// In-crate modules (file layout under `src/`). These live in `app_core` itself
// and may reference the extracted domain crates as `crate::errors`,
// `crate::lifecycle`, etc. via the re-exports above.
pub mod archive_generator;
/// Deterministic `entity_id` derivation for audit rows keyed by a plain
/// string (source id, attempted path) rather than a real UUID (T123/T125).
mod audit_ids;
/// Process-global in-memory cache statics for `app_core`-owned domain types
/// (in-memory caching layer, F0 foundation): `library_root` path lookups and
/// `source_protection_state`/defaults. See each accessor's doc comment for
/// its owning write-site invalidation calls.
pub mod caches;
/// #886 calibration master archive plan generator (mirrors
/// [`archive_generator`] for a single master instead of a whole project).
pub mod calibration_archive_generator;
pub mod cleanup_generator;
#[cfg(feature = "dev-tools")]
pub mod dev_contracts;
/// `entity.names` — batch display-name lookup (GF-7 / DS-14).
pub mod entity_names;
pub mod first_run;
/// Per-frame inventory use cases (spec 048): `inventory.frame.list` and the
/// on-demand `inventory.reconcile.run` pass.
pub mod frame_inventory;
/// `framing.list` / `framing.merge` / `framing.split` / `framing.reassign`
/// use cases (spec 008 Q27, F-Framing-3).
pub mod framing;
/// Inbox plan use-cases (spec 041). Lives in `app_core` (not `app_core_inbox`)
/// because it orchestrates `plans` + `plan_apply`, which are `app_core` modules.
pub mod inbox_plan;
/// Resolves scan traversal options from persisted ingestion settings.
pub mod inbox_scan;
pub mod inventory;
pub mod log_stream;
pub mod native;
/// Onboarding use cases and item registry (spec 056) — successor to the
/// removed spec-010 first-project coach.
pub mod onboarding;
/// Path-set overlap comparison for the cross-plan concurrency guard
/// (spec 025 FR-017 / R-Concur-1). Pure, camino-only helper consumed by
/// [`plan_apply`]; relocated here after the vestigial `fs/planner` crate was
/// removed (task #26 / #402).
pub mod path_set;
pub mod patterns;
pub mod plan_apply;
/// `plans.free_space_estimate` (issue #876) — split out of `plans.rs` to stay
/// clear of the in-flight #979 split on that file; depends only on
/// `plans::get_plan`.
pub mod plan_free_space;
pub mod plans;
/// Project-create orchestration (create + mkdir-only scaffolding auto-apply,
/// user decision 2026-07-04). Lives in `app_core` (not `app_core_projects`)
/// because it orchestrates `plans` + `plan_apply`, which are `app_core` modules.
pub mod project_create;
pub mod protection;
pub mod search;
pub mod sessions;
pub mod tool_launch;

use std::collections::BTreeMap;
use std::sync::Arc;

use contracts_core::{
    error_code::ErrorCode, ContractError, ErrorSeverity, OperationName, RequestEnvelope, RequestId,
    ResponseEnvelope,
};
use serde_json::Value;

// ── Unified per-instance cache state ─────────────────────────────────────────

/// All per-instance app-layer caches bundled into one `Arc`-shareable struct.
///
/// Thread a single `Arc<AppCaches>` alongside `SqlitePool`/`EventBus` instead
/// of passing per-crate cache structs separately. Tests construct a fresh
/// `AppCaches::new()` per test to eliminate cross-contamination from
/// process-global statics.
pub struct AppCaches {
    pub core: caches::CoreCaches,
    pub settings: app_core_settings::caches::SettingsCaches,
    pub targets: app_core_targets::caches::TargetsCaches,
    pub calibration: app_core_calibration::caches::CalibrationCaches,
    pub protection_defaults: app_core_cache::ProtectionDefaultsCaches,
}

impl AppCaches {
    /// Construct a fresh, empty set of caches (all slots start as misses).
    #[must_use]
    pub fn new() -> Self {
        Self {
            core: caches::CoreCaches::new(),
            settings: app_core_settings::caches::SettingsCaches::new(),
            targets: app_core_targets::caches::TargetsCaches::new(),
            calibration: app_core_calibration::caches::CalibrationCaches::new(),
            protection_defaults: app_core_cache::ProtectionDefaultsCaches::new(),
        }
    }

    /// Wrap in an `Arc` for sharing across async tasks and Tauri state.
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl Default for AppCaches {
    fn default() -> Self {
        Self::new()
    }
}

pub const CRATE_NAME: &str = "app_core";

pub type OperationResult = Result<Value, ContractError>;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OperationSafetyClass {
    ReadOnly,
    DatabaseMutation,
    PlanGenerating,
    MutationApplying,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationBehavior {
    pub safety_class: OperationSafetyClass,
    pub long_running: bool,
}

impl OperationBehavior {
    #[must_use]
    pub const fn new(safety_class: OperationSafetyClass, long_running: bool) -> Self {
        Self { safety_class, long_running }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationContext {
    pub request_id: RequestId,
    pub operation: OperationName,
    pub actor: String,
}

impl OperationContext {
    #[must_use]
    pub fn system(request_id: RequestId, operation: OperationName) -> Self {
        Self { request_id, operation, actor: "system".to_owned() }
    }
}

pub trait OperationHandler: Send + Sync {
    fn operation(&self) -> OperationName;

    fn behavior(&self) -> OperationBehavior;

    /// # Errors
    /// Returns a `ContractError` if the operation fails to produce a valid result.
    #[allow(clippy::result_large_err)] // ContractError size is acceptable at this boundary
    fn handle(&self, context: &OperationContext, payload: Value) -> OperationResult;
}

pub trait OperationDispatcher {
    fn dispatch(&self, request: RequestEnvelope<Value>) -> ResponseEnvelope<Value>;
}

#[derive(Default)]
pub struct OperationRegistry {
    handlers: BTreeMap<String, Box<dyn OperationHandler>>,
}

impl OperationRegistry {
    #[must_use]
    pub const fn new() -> Self {
        Self { handlers: BTreeMap::new() }
    }

    /// # Errors
    /// Returns `ContractError` if a handler is already registered for the same operation name.
    #[allow(clippy::result_large_err)] // ContractError size is acceptable at this boundary
    pub fn register<H>(&mut self, handler: H) -> Result<(), ContractError>
    where
        H: OperationHandler + 'static,
    {
        let operation = handler.operation().0;

        if self.handlers.contains_key(&operation) {
            return Err(ContractError::new(
                ErrorCode::OperationHandlerDuplicate,
                format!("Operation handler already registered for {operation}."),
                ErrorSeverity::Fatal,
                false,
            ));
        }

        self.handlers.insert(operation, Box::new(handler));
        Ok(())
    }

    #[must_use]
    pub fn behavior_for(&self, operation: &OperationName) -> Option<OperationBehavior> {
        self.handlers.get(&operation.0).map(|handler| handler.behavior())
    }
}

impl OperationDispatcher for OperationRegistry {
    fn dispatch(&self, request: RequestEnvelope<Value>) -> ResponseEnvelope<Value> {
        let request_id = request.request_id;
        let operation = request.operation;
        let Some(handler) = self.handlers.get(&operation.0) else {
            return ResponseEnvelope::error(request_id, unknown_operation_error(&operation));
        };
        let context = OperationContext::system(request_id.clone(), operation);

        match handler.handle(&context, request.payload) {
            Ok(payload) => ResponseEnvelope::ok(request_id, payload),
            Err(error) => ResponseEnvelope::error(request_id, error),
        }
    }
}

fn unknown_operation_error(operation: &OperationName) -> ContractError {
    ContractError::new(
        ErrorCode::OperationNotFound,
        format!("No handler is registered for operation {}.", operation.0),
        ErrorSeverity::Blocking,
        false,
    )
}

#[cfg(test)]
mod tests {
    use contracts_core::{
        error_code::ErrorCode, OperationName, RequestEnvelope, RequestId, ResponseStatus,
    };
    use serde_json::{json, Value};

    use super::{
        ContractError, ErrorSeverity, OperationBehavior, OperationContext, OperationHandler,
        OperationRegistry, OperationSafetyClass, CRATE_NAME,
    };
    use crate::OperationDispatcher;

    struct EchoHandler;

    impl OperationHandler for EchoHandler {
        fn operation(&self) -> OperationName {
            OperationName("library.inventory.query".to_owned())
        }

        fn behavior(&self) -> OperationBehavior {
            OperationBehavior::new(OperationSafetyClass::ReadOnly, false)
        }

        fn handle(
            &self,
            context: &OperationContext,
            payload: Value,
        ) -> Result<Value, ContractError> {
            Ok(json!({
                "operation": context.operation,
                "payload": payload
            }))
        }
    }

    struct FailingHandler;

    impl OperationHandler for FailingHandler {
        fn operation(&self) -> OperationName {
            OperationName("plan.apply.start".to_owned())
        }

        fn behavior(&self) -> OperationBehavior {
            OperationBehavior::new(OperationSafetyClass::MutationApplying, true)
        }

        fn handle(
            &self,
            _context: &OperationContext,
            _payload: Value,
        ) -> Result<Value, ContractError> {
            Err(ContractError::new(
                ErrorCode::PlanApprovalRequired,
                "Plan approval is required.",
                ErrorSeverity::Blocking,
                false,
            ))
        }
    }

    #[test]
    fn exposes_crate_name() {
        // CRATE_NAME has no consumers today (nothing reads it outside this
        // test): assert against the real source of truth (Cargo.toml's
        // `name`, via CARGO_PKG_NAME) instead of mirroring the same literal
        // the constant is defined with, so a Cargo.toml rename that forgets
        // to update CRATE_NAME is caught.
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }

    #[test]
    fn dispatches_registered_operation() {
        let mut registry = OperationRegistry::new();
        registry.register(EchoHandler).unwrap();
        let response = registry.dispatch(RequestEnvelope::new(
            OperationName("library.inventory.query".to_owned()),
            RequestId("req-1".to_owned()),
            json!({ "limit": 25 }),
        ));

        assert_eq!(response.status, ResponseStatus::Ok);
        assert_eq!(
            response.payload.unwrap(),
            json!({
                "operation": "library.inventory.query",
                "payload": { "limit": 25 }
            })
        );
    }

    #[test]
    fn returns_contract_error_for_unknown_operation() {
        let registry = OperationRegistry::new();
        let response = registry.dispatch(RequestEnvelope::new(
            OperationName("unknown.operation".to_owned()),
            RequestId("req-1".to_owned()),
            json!({}),
        ));

        assert_eq!(response.status, ResponseStatus::Error);
        assert_eq!(response.error.unwrap().code, ErrorCode::OperationNotFound);
    }

    #[test]
    fn returns_handler_contract_error() {
        let mut registry = OperationRegistry::new();
        registry.register(FailingHandler).unwrap();
        let response = registry.dispatch(RequestEnvelope::new(
            OperationName("plan.apply.start".to_owned()),
            RequestId("req-1".to_owned()),
            json!({}),
        ));

        assert_eq!(response.status, ResponseStatus::Error);
        assert_eq!(response.error.unwrap().code, ErrorCode::PlanApprovalRequired);
    }

    #[test]
    fn exposes_registered_operation_behavior() {
        let mut registry = OperationRegistry::new();
        registry.register(FailingHandler).unwrap();

        assert_eq!(
            registry.behavior_for(&OperationName("plan.apply.start".to_owned())).unwrap(),
            OperationBehavior::new(OperationSafetyClass::MutationApplying, true)
        );
    }
}
