// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `target.list` — all canonical targets (gen-3), ordered by
//! `primary_designation`, with optional server-side alias-aware search.

use sqlx::SqlitePool;

use contracts_core::targets::TargetListItem;
use contracts_core::ContractError;
use targeting_resolver::cache;

use super::{db_err, list_row_to_item};

/// `target.list` — list canonical targets (gen-3), ordered by
/// `primary_designation`.
///
/// When `search` is `Some(q)` and non-empty, returns only targets whose
/// primary designation, effective label, or any alias contains `q`
/// case-insensitively.  The in-memory catalog cache is used for both the
/// full list and the search pass (no extra DB round-trips for search).
///
/// Read-through against the whole-catalog snapshot ([`crate::caches::catalog`],
/// in-memory caching layer F0): a cache hit skips the DB round-trip entirely; a
/// miss loads from SQLite and populates the snapshot for subsequent readers
/// (including `target_search::search`, which shares this same cache).
///
/// # Errors
///
/// Returns [`ContractError`] with code `internal.database`.
pub async fn list(
    pool: &SqlitePool,
    search: Option<&str>,
) -> Result<Vec<TargetListItem>, ContractError> {
    let catalog = if let Some(cached) = crate::caches::catalog().load() {
        (*cached).clone()
    } else {
        let rows = cache::list_all(pool).await.map_err(db_err)?;
        // #877: attach real session counts (planner Sessions column) — a target
        // with no linked session simply keeps the DTO default of 0.
        let session_counts = session_counts_by_target(pool).await?;
        let items: Vec<TargetListItem> = rows
            .into_iter()
            .map(|row| {
                let mut item = list_row_to_item(row);
                item.session_count = session_counts.get(&item.id).copied().unwrap_or_default();
                item
            })
            .collect();
        crate::caches::store_catalog(std::sync::Arc::new(items.clone()));
        items
    };

    // Return the whole catalog when search is absent or empty.
    let Some(q_raw) = search.filter(|s| !s.trim().is_empty()) else {
        return Ok(catalog);
    };

    // Dual-mode filter (mirrors client-side `matchesSearch` / `normalizeDesig`):
    // - normalized form collapses whitespace for "M31" → "M 31" matching
    // - lowercase plain form for proper-name substring ("andromeda" in "Andromeda Galaxy")
    let q_norm = targeting::normalize::normalize(q_raw);
    let q_lower = q_raw.to_lowercase();

    // Filter in-process using the cached aliases (never cross IPC; GF-11).
    Ok(catalog
        .into_iter()
        .filter(|t| {
            targeting::normalize::normalize(&t.primary_designation).contains(&q_norm)
                || targeting::normalize::normalize(&t.effective_label).contains(&q_norm)
                || t.effective_label.to_lowercase().contains(&q_lower)
                || t.aliases.iter().any(|a| {
                    targeting::normalize::normalize(a).contains(&q_norm)
                        || a.to_lowercase().contains(&q_lower)
                })
        })
        .collect())
}

/// `target_id -> session_count` map (#877), built from real
/// `acquisition_session` rows via `q_targets_mgmt::session_counts_by_target`.
async fn session_counts_by_target(
    pool: &SqlitePool,
) -> Result<std::collections::HashMap<String, u32>, ContractError> {
    let rows = persistence_targets::repositories::q_targets_mgmt::session_counts_by_target(pool)
        .await
        .map_err(db_err)?;
    Ok(rows
        .into_iter()
        .map(|(tid, count)| (tid, u32::try_from(count.max(0)).unwrap_or(0)))
        .collect())
}
