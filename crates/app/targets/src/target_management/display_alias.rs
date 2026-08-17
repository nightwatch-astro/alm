// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `target.display_alias.set` / `.clear` (gen-3, FR-012).

use sqlx::SqlitePool;
use uuid::Uuid;

use contracts_core::targets::{
    TargetDetailV3, TargetDisplayAliasClearRequest, TargetDisplayAliasSetRequest, TargetGetRequest,
};
use contracts_core::ContractError;
use targeting_resolver::cache;

use super::{cache_err, get, invalid_id, not_found};

/// `target.display_alias.set` — set the user presentation label (gen-3, FR-012).
///
/// Blank input is treated as a clear (sets `display_alias = NULL`).
///
/// # Errors
///
/// Returns [`ContractError`] with code `target.not_found`, `target.invalid_id`,
/// or `internal.database`.
pub async fn display_alias_set(
    pool: &SqlitePool,
    req: &TargetDisplayAliasSetRequest,
) -> Result<TargetDetailV3, ContractError> {
    let uuid = Uuid::parse_str(&req.target_id).map_err(|_| invalid_id(&req.target_id))?;
    let updated =
        cache::set_display_alias(pool, uuid, &req.display_alias).await.map_err(cache_err)?;
    if !updated {
        return Err(not_found(&req.target_id));
    }
    // Invalidate after the write commits (never before): `effective_label` in
    // the catalog snapshot is derived from `display_alias`.
    crate::caches::invalidate_catalog();
    // Re-fetch and return the updated detail.
    get(pool, &TargetGetRequest { target_id: req.target_id.clone() }).await
}

/// `target.display_alias.clear` — clear the user presentation label (gen-3, FR-012).
///
/// # Errors
///
/// Returns [`ContractError`] with code `target.not_found`, `target.invalid_id`,
/// or `internal.database`.
pub async fn display_alias_clear(
    pool: &SqlitePool,
    req: &TargetDisplayAliasClearRequest,
) -> Result<TargetDetailV3, ContractError> {
    let uuid = Uuid::parse_str(&req.target_id).map_err(|_| invalid_id(&req.target_id))?;
    let updated = cache::clear_display_alias(pool, uuid).await.map_err(cache_err)?;
    if !updated {
        return Err(not_found(&req.target_id));
    }
    // Invalidate after the write commits (never before): `effective_label` in
    // the catalog snapshot is derived from `display_alias`.
    crate::caches::invalidate_catalog();
    get(pool, &TargetGetRequest { target_id: req.target_id.clone() }).await
}
