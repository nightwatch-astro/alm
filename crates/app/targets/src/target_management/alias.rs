// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `target.alias.add` / `target.alias.remove` (gen-3).

use sqlx::SqlitePool;
use uuid::Uuid;

use contracts_core::targets::{
    AliasKind as ContractAliasKind, TargetAliasAddRequest, TargetAliasAddResult, TargetAliasDto,
    TargetAliasRemoveRequest, TargetAliasRemoveResult,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use targeting_resolver::cache;

use super::{alias_not_removable, cache_err, db_err, invalid_id, not_found};

/// `target.alias.add` — add a user alias to a target (gen-3).
///
/// The alias is normalized before storage; a same-target duplicate (same
/// normalized form) is returned idempotently. A normalized form already owned
/// by a *different* target is rejected (FR-008, #751) rather than silently
/// creating an ambiguous cross-target alias.
///
/// # Errors
///
/// Returns [`ContractError`] with code `target.not_found`, `target.invalid_id`,
/// `alias.blank`, `alias.duplicate`, or `internal.database`.
pub async fn alias_add(
    pool: &SqlitePool,
    req: &TargetAliasAddRequest,
) -> Result<TargetAliasAddResult, ContractError> {
    let uuid = Uuid::parse_str(&req.target_id).map_err(|_| invalid_id(&req.target_id))?;

    // Verify the target exists.
    let exists =
        persistence_targets::repositories::q_targets_mgmt::target_exists(pool, &uuid.to_string())
            .await
            .map_err(db_err)?;
    if !exists {
        return Err(not_found(&req.target_id));
    }

    if req.alias.trim().is_empty() {
        return Err(ContractError::new(
            ErrorCode::AliasBlank,
            "Alias must not be blank.",
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // FR-008 (#751): guard against the same normalized alias already owned by
    // a *different* canonical target. The DB constraint is per-target only
    // (`UNIQUE(target_id, normalized)`), so a same-target duplicate stays the
    // existing idempotent path below; only a cross-target collision errors.
    let target_id_str = uuid.to_string();
    let normalized = simbad_resolver::normalize::normalize(&req.alias);
    if !normalized.is_empty() {
        let owner =
            persistence_targets::repositories::q_resolver::select_target_id_by_normalized_alias(
                pool,
                &normalized,
            )
            .await
            .map_err(db_err)?;
        if let Some(owner_id) = owner {
            if owner_id != target_id_str {
                return Err(ContractError::new(
                    ErrorCode::AliasDuplicate,
                    format!("Alias '{}' already belongs to another target.", req.alias),
                    ErrorSeverity::Blocking,
                    false,
                ));
            }
        }
    }

    let result = cache::insert_user_alias(pool, uuid, &req.alias).await.map_err(cache_err)?;

    match result {
        None => Err(ContractError::new(
            ErrorCode::AliasBlank,
            "Alias normalizes to empty string.",
            ErrorSeverity::Blocking,
            false,
        )),
        Some((alias_id, alias_display)) => {
            // Invalidate after the write commits (never before) per the
            // `SnapshotCache` usage contract (`app_core_cache::SnapshotCache`).
            crate::caches::invalidate_catalog();
            Ok(TargetAliasAddResult {
                alias: TargetAliasDto {
                    id: alias_id,
                    alias: alias_display,
                    kind: ContractAliasKind::User,
                },
            })
        }
    }
}

/// `target.alias.remove` — remove a user alias by id (gen-3).
///
/// Only aliases with `kind='user'` are removable; attempting to remove a
/// SIMBAD designation or common name returns `alias.not_removable`.
///
/// # Errors
///
/// Returns [`ContractError`] with code `alias.not_found`, `alias.not_removable`,
/// or `internal.database`.
pub async fn alias_remove(
    pool: &SqlitePool,
    req: &TargetAliasRemoveRequest,
) -> Result<TargetAliasRemoveResult, ContractError> {
    // First check whether the alias exists at all (to distinguish "not found"
    // from "not removable").
    let row = persistence_targets::repositories::q_targets_mgmt::get_alias_kind(
        pool,
        &req.alias_id,
        &req.target_id,
    )
    .await
    .map_err(db_err)?;

    match row {
        None => Err(ContractError::new(
            ErrorCode::AliasNotFound,
            format!("Alias '{}' not found on target '{}'.", req.alias_id, req.target_id),
            ErrorSeverity::Blocking,
            false,
        )),
        Some(kind) if kind != "user" => Err(alias_not_removable()),
        Some(_) => {
            let deleted = cache::delete_user_alias(pool, &req.alias_id).await.map_err(cache_err)?;
            if deleted {
                // Invalidate after the write commits (never before) per the
                // `SnapshotCache` usage contract.
                crate::caches::invalidate_catalog();
            }
            Ok(TargetAliasRemoveResult { removed: deleted })
        }
    }
}
