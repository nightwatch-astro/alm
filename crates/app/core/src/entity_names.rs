// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `entity.names` — batch display-name lookup (GF-7 / DS-14).
//!
//! Resolves `(entityType, entityId)` refs to display names in three IN-clause
//! queries (one per entity type) rather than one IPC round-trip per unseen id.
//!
//! Supported entity types: `project`, `plan`, `target`.  Unknown types are
//! silently skipped — the frontend treats absence as "unresolved".

use std::collections::HashMap;

use contracts_core::audit::{EntityNameRef, EntityNamesResponse};
use contracts_core::ContractError;
use persistence_core::repositories::q_core;
use sqlx::SqlitePool;

fn db_err(e: impl std::fmt::Display) -> ContractError {
    contracts_core::ContractError::new(
        contracts_core::error_code::ErrorCode::InternalDatabase,
        format!("{e}"),
        contracts_core::ErrorSeverity::Fatal,
        true,
    )
}

/// Resolve display names for a batch of entity refs.
///
/// Returns a map from `"<entityType>:<entityId>"` → display name.  Refs that
/// do not match a DB row are omitted (caller treats absence as unknown).
///
/// # Errors
/// Returns `ContractError` with code `internal.database` on query failure.
pub async fn entity_names(
    pool: &SqlitePool,
    refs: Vec<EntityNameRef>,
) -> Result<EntityNamesResponse, ContractError> {
    // Partition ids by entity type (skip unsupported types).
    let mut project_ids: Vec<String> = Vec::new();
    let mut plan_ids: Vec<String> = Vec::new();
    let mut target_ids: Vec<String> = Vec::new();

    for r in &refs {
        match r.entity_type.as_str() {
            "project" => project_ids.push(r.entity_id.clone()),
            "plan" => plan_ids.push(r.entity_id.clone()),
            "target" => target_ids.push(r.entity_id.clone()),
            _ => {} // session names come from the inventory-sources query, not here
        }
    }

    // Three IN-clause batch queries (avoids N IPC round-trips).
    let project_rows = q_core::project_names_batch(pool, &project_ids).await.map_err(db_err)?;
    let plan_rows = q_core::plan_titles_batch(pool, &plan_ids).await.map_err(db_err)?;
    let target_rows = q_core::target_names_batch(pool, &target_ids).await.map_err(db_err)?;

    let mut names: HashMap<String, String> = HashMap::new();
    for (id, name) in project_rows {
        names.insert(format!("project:{id}"), name);
    }
    for (id, title) in plan_rows {
        names.insert(format!("plan:{id}"), title);
    }
    for (id, designation) in target_rows {
        names.insert(format!("target:{id}"), designation);
    }

    Ok(EntityNamesResponse { names })
}
