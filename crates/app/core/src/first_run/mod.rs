// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! First-run source registration and wizard use cases (spec 003).
//!
//! Thin orchestration layer adding path validation, error mapping to
//! contract error codes, and audit event emission on top of the
//! persistence repository.
//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b) as a pure
//! leaf: it has zero `crate::` references and nothing else in `app_core`
//! references it. `app_core` re-exports this crate at `app_core::first_run` so the
//! public surface stays byte-identical.
//!
//! Split by responsibility (refactor sweep #975): `sources` is
//! register/list/remove + the batch pipeline; `wizard` is the
//! `first_run_state` singleton (get/complete/restart); `root_remap` is
//! P6a (`roots.remap`/`.apply`); `root_ops` is P6b (active toggle +
//! delete). Path validation, error mapping, and the shared audit-writing
//! helpers used by more than one use case stay here.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use audit::bus::EventBus;
use audit::{AuditLogEntry, Outcome, Severity};
use contracts_core::first_run::SourceKind;
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::EntityId;
use domain_core::lifecycle::data_asset::EntityType;
use persistence_lifecycle::repositories::first_run as repo;
use sqlx::SqlitePool;

use crate::audit_ids::deterministic_entity_id;
use crate::errors::bus_err;

mod root_ops;
mod root_remap;
mod sources;
mod wizard;

#[cfg(test)]
mod tests;

pub use root_ops::{delete_source, set_source_active};
pub use root_remap::{apply_root_remap, remap_root};
pub use sources::{
    get_source_organization_state, list_sources, register_source, register_source_batch,
    remove_source, set_source_organization_state,
};
pub use wizard::{complete_first_run, get_first_run_state, restart_first_run};

// ── Path validation ─────────────────────────────────────────────────────────

/// Validate that the given path exists, is a directory, and is readable.
///
/// Returns a `ContractError` with a dotted error code on failure.
fn validate_path(path: &str) -> Result<(), Box<ContractError>> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        Box::new(if e.kind() == std::io::ErrorKind::NotFound {
            ContractError::new(
                ErrorCode::PathNotExists,
                format!("Path does not exist: {path}"),
                ErrorSeverity::Blocking,
                false,
            )
        } else if e.kind() == std::io::ErrorKind::PermissionDenied {
            ContractError::new(
                ErrorCode::PathPermissionDenied,
                format!("Permission denied: {path}"),
                ErrorSeverity::Blocking,
                false,
            )
        } else {
            ContractError::new(
                ErrorCode::PathNotExists,
                format!("Cannot access path: {path}: {e}"),
                ErrorSeverity::Blocking,
                false,
            )
        })
    })?;

    if !metadata.is_dir() {
        return Err(Box::new(ContractError::new(
            ErrorCode::PathNotDirectory,
            format!("Path is not a directory: {path}"),
            ErrorSeverity::Blocking,
            false,
        )));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o444 == 0 {
            return Err(Box::new(ContractError::new(
                ErrorCode::PathPermissionDenied,
                format!("No read permission on: {path}"),
                ErrorSeverity::Blocking,
                false,
            )));
        }
    }

    Ok(())
}

/// Check whether a path is already registered, and if so, whether it's
/// under a different kind.
async fn check_duplicate(
    pool: &SqlitePool,
    path: &str,
    kind: SourceKind,
) -> Result<(), ContractError> {
    let matches = repo::find_sources_by_path(pool, path).await.map_err(db_to_contract)?;

    if let Some(source) = matches.first() {
        if source.kind == kind {
            // Issue #501: registering the exact same path a second time cannot
            // proceed — this must hard-stop like `path.not_exists` /
            // `path.not_directory`, not be a bypassable `Warning`.
            return Err(ContractError::new(
                ErrorCode::PathAlreadyRegistered,
                format!("Path is already registered as {kind:?}: {path}"),
                ErrorSeverity::Blocking,
                false,
            ));
        }
        return Err(ContractError::new(
            ErrorCode::PathAlreadyRegisteredDifferentKind,
            format!(
                "Path is already registered as {:?} (requested {:?}): {path}",
                source.kind, kind
            ),
            ErrorSeverity::Warning,
            false,
        ));
    }

    Ok(())
}

/// Path-overlap relationship between `candidate` and `other`, or `None` if
/// they don't overlap. Case-folds both sides on Windows (nJ01a review carry-
/// over): NTFS/ReFS/FAT are case-insensitive/case-preserving, so `C:\Foo` and
/// `c:\foo` name the same root and a lexical `starts_with` alone would miss
/// the overlap. Unix filesystems default to case-sensitive, so the exact
/// bytes are compared there — folding unconditionally would falsely reject
/// distinct same-name-different-case Linux/macOS(HFS+ case-sensitive) roots,
/// which is not the failure mode we're guarding against.
fn path_overlap_relationship(
    candidate: &std::path::Path,
    other: &std::path::Path,
) -> Option<&'static str> {
    #[cfg(windows)]
    let (candidate, other): (std::path::PathBuf, std::path::PathBuf) = (
        candidate.to_string_lossy().to_lowercase().into(),
        other.to_string_lossy().to_lowercase().into(),
    );
    #[cfg(windows)]
    let (candidate, other) = (candidate.as_path(), other.as_path());

    if other.starts_with(candidate) {
        Some("parent")
    } else if candidate.starts_with(other) {
        Some("child")
    } else {
        None
    }
}

/// Check whether a candidate root path overlaps (is a parent of, or is nested
/// within) any already-registered root, or any path already accepted earlier
/// in the same batch request (`extra_paths`, still unpersisted). Cross-cutting
/// across categories: an inbox root inside a light-frames root is still an
/// overlap (issue #501, rules 3/4). Exact-path equality is left to
/// [`check_duplicate`], which already covers it with a more specific error.
async fn check_overlap(
    pool: &SqlitePool,
    path: &str,
    extra_paths: &[String],
) -> Result<(), ContractError> {
    let candidate = std::path::Path::new(path);
    let existing = repo::list_sources(pool).await.map_err(db_to_contract)?;

    for other in
        existing.iter().map(|s| s.path.as_str()).chain(extra_paths.iter().map(String::as_str))
    {
        if other == path {
            continue;
        }
        let other_path = std::path::Path::new(other);
        let Some(relationship) = path_overlap_relationship(candidate, other_path) else {
            continue;
        };
        return Err(ContractError::new(
            ErrorCode::PathOverlapsExisting,
            format!("Path overlaps an already-registered root ({relationship}): {other}"),
            ErrorSeverity::Blocking,
            false,
        )
        .with_details(
            serde_json::json!({"conflictingPath": other, "relationship": relationship}),
        ));
    }

    Ok(())
}

fn db_to_contract(e: persistence_core::DbError) -> ContractError {
    let msg = e.to_string();
    if msg.contains("UNIQUE constraint failed") {
        // Same code as `check_duplicate`'s same-kind branch above — keep
        // severity consistent (issue #501).
        ContractError::new(ErrorCode::PathAlreadyRegistered, msg, ErrorSeverity::Blocking, false)
    } else {
        // Delegate the non-UNIQUE fallback to the canonical mapper (T1-c) so
        // `NotFound` is classified `Blocking`/non-retryable instead of the
        // hand-rolled `Fatal`/`retryable=true` this used to apply to every
        // variant, including missing rows.
        crate::errors::db_err(e)
    }
}

/// Render an `ErrorCode` as its dotted wire string (e.g. `"path.not_exists"`),
/// for use as an audit `reason_code`.
fn error_code_str(code: ErrorCode) -> String {
    serde_json::to_string(&code)
        .map_or_else(|_| "internal.error".to_owned(), |s| s.trim_matches('"').to_owned())
}

/// Write a durable audit row for a `source.register` attempt (T125,
/// FR-130/FR-131). `entity_seed` is the created `source_id` on success, or
/// the attempted `path` on refusal (no source id exists yet, so repeated
/// refused attempts against the same path still correlate under one
/// `entity_id`).
async fn write_source_register_audit(
    bus: &EventBus,
    entity_seed: &str,
    path: &str,
    kind: SourceKind,
    outcome: Outcome,
    reason_code: Option<&str>,
) -> Result<(), ContractError> {
    let kind_str: &'static str = kind.into();
    let mut entry = AuditLogEntry::new(
        EntityType::DataSource,
        deterministic_entity_id("source", entity_seed),
        "source.register",
        "user",
        outcome,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_payload(serde_json::json!({"path": path, "kind": kind_str}));
    if let Some(code) = reason_code {
        entry = entry.with_reason_code(code.to_owned());
    }
    bus.write_audit(
        entry,
        "source.registered",
        audit::event_bus::Source::User,
        serde_json::json!({"path": path, "kind": kind_str, "outcome": outcome.as_str()}),
    )
    .await
    .map_err(bus_err)?;
    Ok(())
}

/// Look up a root's kind + path, mapping a missing row to `source.not_found`.
async fn get_root_or_not_found(
    pool: &SqlitePool,
    root_id: &str,
) -> Result<(SourceKind, String), ContractError> {
    repo::get_source_kind_and_path(pool, root_id).await.map_err(db_to_contract)?.ok_or_else(|| {
        ContractError::new(
            ErrorCode::SourceNotFound,
            format!("root not found: {root_id}"),
            ErrorSeverity::Blocking,
            false,
        )
    })
}

/// Write a durable `Outcome::Failed` audit row for a root operation attempted
/// against a missing/invalid root (T125/T127, FR-130). `action` names the
/// specific operation (`root.remap.apply`, `root.active_changed`,
/// `root.deleted`) so refusals for different root ops stay distinguishable
/// under the same `entity_id`. `outcome` is `Failed` for a not-found/DB-level
/// failure or `Refused` for a business-rule block (e.g. `root.has_dependents`).
async fn write_root_op_refusal(
    bus: &EventBus,
    root_id: &str,
    action: &str,
    outcome: Outcome,
    reason_code: &str,
) -> Result<(), ContractError> {
    let entry = AuditLogEntry::new(
        EntityType::LibraryRoot,
        deterministic_entity_id("root", root_id),
        action,
        "user",
        outcome,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_reason_code(reason_code.to_owned())
    .with_payload(serde_json::json!({"rootId": root_id}));
    bus.write_audit(
        entry,
        "root.op_failed",
        audit::event_bus::Source::User,
        serde_json::json!({"rootId": root_id, "action": action, "outcome": outcome.as_str(), "reasonCode": reason_code}),
    )
    .await
    .map_err(bus_err)?;
    Ok(())
}
