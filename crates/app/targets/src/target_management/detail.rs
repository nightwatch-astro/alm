// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `target.get` — full detail (gen-3).

use sqlx::SqlitePool;
use uuid::Uuid;

use contracts_core::targets::{TargetDetailV3, TargetGetRequest};
use contracts_core::ContractError;
use targeting_resolver::cache;

use super::{cache_err, cached_to_detail, invalid_id, load_alias_dtos, not_found};

/// `target.get` — return full detail (gen-3).
///
/// # Errors
///
/// Returns [`ContractError`] with code `target.not_found`, `target.invalid_id`,
/// or `internal.database`.
pub async fn get(
    pool: &SqlitePool,
    req: &TargetGetRequest,
) -> Result<TargetDetailV3, ContractError> {
    let uuid = Uuid::parse_str(&req.target_id).map_err(|_| invalid_id(&req.target_id))?;
    let target = cache::get_by_id(pool, uuid).await.map_err(cache_err)?;
    match target {
        None => Err(not_found(&req.target_id)),
        Some(t) => {
            let id_str = t.id.to_string();
            let aliases = load_alias_dtos(pool, &id_str).await?;
            Ok(cached_to_detail(t, aliases))
        }
    }
}
