// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Application use-case for spec 016 source protection (US2–US4).
//!
//! - US2: per-source protection override (get + set + resolve).
//! - US3: plan gating — `plan_protection_check` returns protected items.
//! - US4: category enforcement — category membership elevates level via resolver.

//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b). Its only
//! cross-module dependency was on the now-extracted `app_core_errors` leaf and
//! nothing else in `app_core` references it. `app_core` re-exports this crate at
//! `app_core::protection` so the public surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use app_core_cache::ProtectionDefaultsSnapshot;
use contracts_core::ContractError;
use persistence_lifecycle::repositories::settings as settings_repo;
use persistence_plans::repositories::source_protection as prot_repo;
use sqlx::SqlitePool;
use std::sync::Arc;

// ── Error helpers ─────────────────────────────────────────────────────────
//
// Canonical mappers live in `app_core_errors` (US11 T142). `db_err` routes
// `DbError::NotFound` to the recoverable `Blocking`/`retryable=false`
// classification instead of the previous blanket `Fatal` (L2 divergence fix).
use crate::errors::db_err;

mod cleanup_plan;
mod global_defaults;
mod plan_check;
mod source_protection;
#[cfg(test)]
mod tests;

pub use cleanup_plan::{
    generate_cleanup_plan, generate_plan, CleanupPlanItem, GenerateCleanupPlanRequest,
    GenerateCleanupPlanResponse, GeneratePlanRequest,
};
pub use global_defaults::set_global_protection_default;
pub use plan_check::{acknowledge_protected_item, plan_protection_check};
pub use source_protection::{get_source_protection, seed_source_protection, set_source_protection};

// ── Global settings helpers ───────────────────────────────────────────────

/// Load the three protection-relevant global settings from the DB.
///
/// Reads from `protection_defaults` (scope="global") first (migration 0035,
/// FR-018). Falls back to the legacy `settings` table rows for backwards
/// compatibility, then to hard-coded defaults when both are absent.
pub(crate) async fn load_global_protection(
    pool: &SqlitePool,
) -> Result<GlobalProtection, ContractError> {
    use serde_json::Value;

    // Read-through `app_core_cache::protection_defaults` (F0): on a hit, skip
    // the three-row DB read below entirely. Cache lives in the `app_core_cache`
    // leaf (not `crate::caches`) so `app_core_settings` can invalidate it too
    // without a dependency cycle.
    if let Some(cached) = app_core_cache::protection_defaults().load() {
        return Ok(GlobalProtection {
            level: cached.level.clone(),
            block_permanent_delete: cached.block_permanent_delete,
            categories: cached.categories.clone(),
        });
    }

    // Prefer protection_defaults table (migration 0035).
    let pd_level = prot_repo::get_protection_default(pool, "global", "defaultProtection")
        .await
        .map_err(db_err)?;
    let pd_bpd = prot_repo::get_protection_default(pool, "global", "blockPermanentDelete")
        .await
        .map_err(db_err)?;
    let pd_cats = prot_repo::get_protection_default(pool, "global", "protectedCategories")
        .await
        .map_err(db_err)?;

    // Fall back to legacy settings table.
    let level_val = if pd_level.is_some() {
        pd_level
    } else {
        settings_repo::get_raw(pool, "defaultProtection").await.map_err(db_err)?
    };
    let bpd_val = if pd_bpd.is_some() {
        pd_bpd
    } else {
        settings_repo::get_raw(pool, "blockPermanentDelete").await.map_err(db_err)?
    };
    let cats_val = if pd_cats.is_some() {
        pd_cats
    } else {
        settings_repo::get_raw(pool, "protectedCategories").await.map_err(db_err)?
    };

    let level = level_val.as_ref().and_then(Value::as_str).unwrap_or("protected").to_owned();
    let block_permanent_delete = bpd_val.as_ref().and_then(Value::as_bool).unwrap_or(true);
    let categories: Vec<String> = match cats_val {
        Some(Value::Array(arr)) => {
            arr.into_iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()
        }
        _ => vec!["lights".to_owned(), "masters".to_owned(), "finals".to_owned()],
    };

    let global = GlobalProtection { level, block_permanent_delete, categories };
    app_core_cache::store_protection_defaults(Arc::new(ProtectionDefaultsSnapshot {
        level: global.level.clone(),
        block_permanent_delete: global.block_permanent_delete,
        categories: global.categories.clone(),
    }));
    Ok(global)
}

/// Whether a resolved protection level makes an item unactionable.
///
/// The apply executor refuses any non-`NoOp`/`Catalogue` action on an item
/// whose stored protection resolves to this level
/// (`fs_executor::run::loop_`, gated by `plan_apply::paths`), and nothing
/// clears that column — acknowledgement only writes an audit event. So a
/// preview that counts these bytes as reclaimable is advertising space no
/// apply can free.
pub(crate) fn is_protected(level: &str) -> bool {
    level == "protected"
}

#[derive(Clone)]
pub(crate) struct GlobalProtection {
    pub(crate) level: String,
    pub(crate) block_permanent_delete: bool,
    pub(crate) categories: Vec<String>,
}

/// Serializes every test — in this module and in `cleanup_generator.rs`
/// (`pub(crate)` so that sibling module can reach it) — that reads or writes
/// the process-global `protection_defaults` cache (directly, or via
/// `load_global_protection` / `set_global_protection_default`). That cache is
/// a single unkeyed slot shared by every in-memory DB in this test binary, so
/// e.g. `t041_set_global_default_persists_and_emits_event` mutating it to
/// `"unprotected"` could otherwise race a concurrently-running,
/// value-sensitive read elsewhere that expects the default `"protected"`
/// (`cleanup_generator::tests::generate_protected_final_gates_approval`).
/// Acquired for the whole test body via `setup()`'s returned guard — an
/// invalidate-at-setup reset alone only guards against a *completed* prior
/// test's leftover value, not a genuinely concurrent mutation.
#[cfg(test)]
pub(crate) static PROTECTION_DEFAULTS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
