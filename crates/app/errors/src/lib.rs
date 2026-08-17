// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical error conversion helpers for `app_core`.
//!
//! Provides a single `db_to_contract` helper that replaces the scattered,
//! divergent `db_err` helpers found previously in `target_search.rs` and
//! `ingest_resolution.rs`.
//!
//! ## Severity contract
//!
//! | `DbError` variant  | Code                       | Severity  | Retryable |
//! |--------------------|----------------------------|-----------|-----------|
//! | `NotFound`         | `internal.database`        | Blocking  | false     |
//! | all others         | `internal.database`        | Fatal     | true      |
//!
//! The `NotFound` path was previously mapped as `Fatal`/`retryable=true` in
//! `target_search.rs` and `ingest_resolution.rs` — that was the bug.  Modules
//! that need a **domain-specific** not-found code (e.g. `view.not_found`,
//! `project.not_found`) must keep their own explicit mapping; this helper
//! is only for generic infrastructure errors.
//!
//! Extracted from `app_core` into a standalone leaf crate (spec 042 / T253
//! O3b): it has zero `crate::` references. `app_core` re-exports this crate at
//! its original `app_core::errors::*` path so the public surface stays
//! byte-identical for every consumer.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity, RecoveryAction};

/// The `retry` recovery action attached to every `retryable=true` error this
/// crate produces (bd `astro-plan-qnj0`).
///
/// `label`/`description` are diagnostic fallback text, not translated copy —
/// the same convention `ContractError.message` already uses (see
/// `apps/desktop/src/lib/errors.ts` FR-009: raw backend strings are never
/// shown to the user). A future UI (issue #930) resolves display text from
/// `code` via an i18n catalog, mirroring how `ContractError.code` already
/// drives `ERROR_MESSAGES` today.
fn retry_action() -> RecoveryAction {
    RecoveryAction {
        code: "retry".to_owned(),
        label: "Retry the operation".to_owned(),
        description: None,
    }
}

/// Convert a database-layer error (`sqlx::Error` or
/// [`persistence_core::DbError`]) to an `internal.database` `ContractError`
/// while preserving the originating error and adding which operation failed.
///
/// The error is wrapped with [`anyhow::Context::context`] so the original
/// error is retained as the source of the resulting chain, then the full chain
/// is rendered with the alternate (`{:#}`) formatter — e.g.
/// `"insert calibration_session: <original sqlx message>"`.  This replaces
/// bare `e.to_string()` sites that discarded the operational context.
///
/// The wire contract is unchanged: the `code` stays
/// [`ErrorCode::InternalDatabase`] (serialized `"internal.database"`),
/// severity stays [`ErrorSeverity::Fatal`], and `retryable` stays `true`.
/// Only the human-readable `message` gains context.
#[must_use]
pub fn db_internal_ctx<E>(e: E, context: &'static str) -> ContractError
where
    E: std::error::Error + Send + Sync + 'static,
{
    use anyhow::Context as _;
    let enriched = Err::<(), _>(e).context(context).unwrap_err();
    ContractError::new(
        ErrorCode::InternalDatabase,
        format!("{enriched:#}"),
        ErrorSeverity::Fatal,
        true,
    )
    .with_recovery_action(retry_action())
}

/// Canonical generic `DbError` → `ContractError` mapper (US11 T142).
///
/// This is the single home every `.map_err(db_err)?` site that does **not**
/// need a domain-specific not-found code now collapses onto.
///
/// - `NotFound` → `internal.database`, `Blocking`, `retryable=false`
///   (recoverable: the caller referenced a missing entity and can recover by
///   referencing an existing one — it is **not** a fatal infrastructure fault).
/// - all others → `internal.database`, `Fatal`, `retryable=true`
///
/// ## L2 divergence fix (US11 T142)
///
/// Previously `settings.rs` and `protection.rs` had blanket `db_err` closures
/// that mapped **every** `DbError`, including `NotFound`, to
/// `Fatal`/`retryable=true`. That mislabeled a recoverable missing-row outcome
/// as a fatal database fault. Routing those sites through this canonical mapper
/// restores the recoverable `Blocking`/`retryable=false` classification for
/// `NotFound`. The wire `code` string is unchanged (`"internal.database"`);
/// only `severity`/`retryable` are corrected.
///
/// Modules that need a **domain-specific** not-found code (e.g. `plan.not_found`,
/// `project.not_found`, `view.not_found`) keep their own explicit `NotFound`
/// arm and delegate only the remaining variants here.
#[must_use]
pub fn db_err(e: persistence_core::DbError) -> ContractError {
    match e {
        persistence_core::DbError::NotFound(msg) => {
            ContractError::new(ErrorCode::InternalDatabase, msg, ErrorSeverity::Blocking, false)
        }
        other => ContractError::new(
            ErrorCode::InternalDatabase,
            format!("{other}"),
            ErrorSeverity::Fatal,
            true,
        )
        .with_recovery_action(retry_action()),
    }
}

/// Returns a `DbError → ContractError` closure that maps `NotFound` to a
/// caller-supplied domain-specific error code (e.g. `project.not_found`) and
/// delegates every other variant to the generic [`db_err`] path.
///
/// # Usage
///
/// ```rust,ignore
/// use app_core_errors::{db_err_with_not_found};
/// use contracts_core::error_code::ErrorCode;
///
/// repo::get_project(pool, id).await.map_err(db_err_with_not_found(ErrorCode::ProjectNotFound))?;
/// ```
///
/// This collapses the 6-line `match`-and-delegate domain wrappers to a single
/// expression while preserving the correct `Blocking`/`retryable=false`
/// classification for `NotFound` and `Fatal`/`retryable=true` for every other
/// variant (bd `astro-plan-kyo7.88`).
#[must_use = "use the returned closure as a .map_err argument"]
pub fn db_err_with_not_found(
    not_found_code: ErrorCode,
) -> impl Fn(persistence_core::DbError) -> ContractError {
    move |e| match e {
        persistence_core::DbError::NotFound(msg) => {
            ContractError::new(not_found_code, msg, ErrorSeverity::Blocking, false)
        }
        other => db_err(other),
    }
}

/// Back-compat alias for the US8 name. Delegates to the canonical [`db_err`].
#[must_use]
pub fn db_to_contract(e: persistence_core::DbError) -> ContractError {
    db_err(e)
}

/// Canonical `BusError` → `ContractError` mapper (US11 T142).
///
/// Every per-module `bus_err` closure was byte-identical
/// (`internal.audit`, `Fatal`, `retryable=true`); this is the single home.
/// Takes the error by value to match the `.map_err(bus_err)` call sites (the
/// per-module copies all carried the same `needless_pass_by_value` allow).
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn bus_err(e: audit::bus::BusError) -> ContractError {
    ContractError::new(ErrorCode::InternalAudit, format!("{e}"), ErrorSeverity::Fatal, true)
        .with_recovery_action(retry_action())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// bd `astro-plan-qnj0`: every `retryable=true` error this crate produces
    /// must carry a `recovery_actions` entry — before this change all three
    /// mappers hardcoded `Vec::new()` via `ContractError::new`.
    #[test]
    fn db_err_fatal_branch_carries_retry_action() {
        let err = db_err(persistence_core::DbError::CasFailed("stale version".to_owned()));
        assert!(err.retryable);
        assert_eq!(err.recovery_actions.len(), 1);
        assert_eq!(err.recovery_actions[0].code, "retry");
    }

    /// `NotFound` stays `Blocking`/non-retryable and has no generic recovery
    /// action to offer (the caller must supply a different reference) — this
    /// path is intentionally left empty, not an oversight.
    #[test]
    fn db_err_not_found_branch_has_no_recovery_action() {
        let err = db_err(persistence_core::DbError::NotFound("widget 1".to_owned()));
        assert!(!err.retryable);
        assert!(err.recovery_actions.is_empty());
    }

    #[test]
    fn db_internal_ctx_carries_retry_action() {
        let src = persistence_core::DbError::CasFailed("stale version".to_owned());
        let err = db_internal_ctx(src, "insert widget");
        assert_eq!(err.recovery_actions.len(), 1);
        assert_eq!(err.recovery_actions[0].code, "retry");
    }

    #[test]
    fn bus_err_carries_retry_action() {
        let src = audit::bus::BusError::Database(persistence_core::DbError::CasFailed(
            "stale version".to_owned(),
        ));
        let err = bus_err(src);
        assert_eq!(err.recovery_actions.len(), 1);
        assert_eq!(err.recovery_actions[0].code, "retry");
    }

    #[test]
    fn db_err_with_not_found_routes_not_found_to_domain_code() {
        use contracts_core::error_code::ErrorCode;
        let f = db_err_with_not_found(ErrorCode::ProjectNotFound);
        let err = f(persistence_core::DbError::NotFound("project 1".to_owned()));
        assert_eq!(err.code, ErrorCode::ProjectNotFound);
        assert!(!err.retryable);
        assert!(err.recovery_actions.is_empty());
    }

    #[test]
    fn db_err_with_not_found_routes_other_to_generic_db_err() {
        use contracts_core::error_code::ErrorCode;
        let f = db_err_with_not_found(ErrorCode::ProjectNotFound);
        let err = f(persistence_core::DbError::CasFailed("stale".to_owned()));
        assert_eq!(err.code, ErrorCode::InternalDatabase);
        assert!(err.retryable);
        assert_eq!(err.recovery_actions.len(), 1);
    }
}

// NOTE (US11 T142): a `From<persistence_core::DbError> for ContractError` impl is
// **not** possible in `app_core` — both `DbError` and `ContractError` are
// foreign types here, so the orphan rule (E0117) forbids the impl. It could
// only live in `contracts_core`, but `contracts_core` must not depend on
// `persistence_db` (that dependency inversion is tracked separately as T254).
// The canonical [`db_err`] free function provides the same mapping for every
// `.map_err(db_err)?` site, which is the achievable form of this task.
