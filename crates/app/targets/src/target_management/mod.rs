// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Gen-3 target management use cases (spec 036).
//!
//! Implements `target.get`, `target.list`, `target.alias.add`,
//! `target.alias.remove`, `target.display_alias.set`, and
//! `target.display_alias.clear` against the `canonical_target` / `target_alias`
//! tables (migration 0031).
//!
//! ## Constitution
//!
//! - §I No filesystem mutations: read/write SQLite metadata only.
//! - §III Metadata/identity only — no image processing.
//! - §V SQLite (resolution cache / canonical_target) is the durable record.
//!
//! Split by responsibility (refactor sweep #986): `detail` is `target.get`;
//! [`list`] is `target.list` (+ session-count enrichment); `alias` is
//! `target.alias.add`/`.remove`; `display_alias` is `.set`/`.clear`;
//! `sessions_projects` is spec 023 US2/US3 (`target.sessions.list` /
//! `target.projects.list`); `note` is spec 023 US4 (`target.note.get`/
//! `.update`). Error mapping and DTO conversion helpers used by more than one
//! use case stay here.

use sqlx::SqlitePool;

use contracts_core::targets::TargetAliasDto;
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use targeting_resolver::cache::{CachedTarget, TargetListRow};
use targeting_resolver::AliasKind;

mod alias;
mod detail;
mod display_alias;
mod list;
mod note;
mod sessions_projects;

#[cfg(test)]
pub(crate) mod cache_test_lock;
#[cfg(test)]
mod tests;

pub use alias::{alias_add, alias_remove};
pub use detail::get;
pub use display_alias::{display_alias_clear, display_alias_set};
pub use list::list;
pub use note::{note_get, note_update};
pub use sessions_projects::{projects_list, sessions_list};

// ── Error helpers ────────────────────────────────────────────────────────────

fn not_found(id: &str) -> ContractError {
    ContractError::new(
        ErrorCode::TargetNotFound,
        format!("Target '{id}' not found."),
        ErrorSeverity::Blocking,
        false,
    )
}

use app_core_errors::db_err;

/// Map a `targeting_resolver::cache::CacheError` to a `ContractError`.
///
/// Delegates `Persistence(NotFound)` to `Blocking`; other variants are Fatal.
#[allow(clippy::needless_pass_by_value)]
fn cache_err(e: targeting_resolver::cache::CacheError) -> ContractError {
    use persistence_core::DbError;
    use targeting_resolver::cache::CacheError;
    match e {
        CacheError::Persistence(DbError::NotFound(msg)) => {
            ContractError::new(ErrorCode::InternalDatabase, msg, ErrorSeverity::Blocking, false)
        }
        other => ContractError::new(
            ErrorCode::InternalDatabase,
            format!("{other}"),
            ErrorSeverity::Fatal,
            true,
        ),
    }
}

fn invalid_id(id: &str) -> ContractError {
    ContractError::new(
        ErrorCode::TargetInvalidId,
        format!("'{id}' is not a valid target id."),
        ErrorSeverity::Blocking,
        false,
    )
}

fn alias_not_removable() -> ContractError {
    ContractError::new(
        ErrorCode::AliasNotRemovable,
        "Only user-added aliases (kind='user') can be removed.",
        ErrorSeverity::Blocking,
        false,
    )
}

// ── Enum mapping ─────────────────────────────────────────────────────────────

fn map_alias_kind(k: AliasKind) -> contracts_core::targets::AliasKind {
    match k {
        AliasKind::Designation => contracts_core::targets::AliasKind::Designation,
        AliasKind::CommonName => contracts_core::targets::AliasKind::CommonName,
        AliasKind::User => contracts_core::targets::AliasKind::User,
    }
}

// ── Conversion helpers ───────────────────────────────────────────────────────

/// Load all alias rows for a target (with their persisted ids) and map to DTOs.
async fn load_alias_dtos(
    pool: &SqlitePool,
    target_id_str: &str,
) -> Result<Vec<TargetAliasDto>, ContractError> {
    let rows =
        persistence_targets::repositories::q_targets_mgmt::list_target_aliases(pool, target_id_str)
            .await
            .map_err(db_err)?;

    Ok(rows
        .into_iter()
        .map(|r| TargetAliasDto {
            id: r.id,
            alias: r.alias,
            kind: map_alias_kind(AliasKind::from_wire(&r.kind)),
        })
        .collect())
}

fn cached_to_detail(
    target: CachedTarget,
    aliases: Vec<TargetAliasDto>,
) -> contracts_core::targets::TargetDetailV3 {
    let effective_label =
        target.display_alias.clone().unwrap_or_else(|| target.primary_designation.clone());
    contracts_core::targets::TargetDetailV3 {
        id: target.id.to_string(),
        primary_designation: target.primary_designation,
        display_alias: target.display_alias,
        effective_label,
        object_type: target.object_type.as_wire().to_owned(),
        ra_deg: target.ra_deg,
        dec_deg: target.dec_deg,
        simbad_oid: target.simbad_oid,
        source: target.source.as_wire().to_owned(),
        aliases,
    }
}

fn list_row_to_item(row: TargetListRow) -> contracts_core::targets::TargetListItem {
    let effective_label = row.display_alias.unwrap_or_else(|| row.primary_designation.clone());
    contracts_core::targets::TargetListItem {
        id: row.id.to_string(),
        effective_label,
        primary_designation: row.primary_designation,
        object_type: row.object_type,
        ra_deg: row.ra_deg,
        dec_deg: row.dec_deg,
        constellation: row.constellation,
        magnitude: row.magnitude,
        aliases: row.aliases,
        session_count: 0, // filled in by list()'s session_counts_by_target pass
    }
}
