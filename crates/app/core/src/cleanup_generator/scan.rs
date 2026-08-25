// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Scan (preview) — D11 step 1.

use std::collections::HashMap;

use contracts_core::cleanup::{CleanupAction, CleanupCandidate, CleanupPolicy, CleanupScanResult};
use contracts_core::ContractError;
use persistence_plans::repositories::artifacts as artifacts_repo;
use persistence_plans::repositories::source_protection as prot_repo;
use sqlx::SqlitePool;

use crate::errors::db_err;
use crate::protection;

use super::{policy::get_policy, DataType};

// ── Scan (preview) ──────────────────────────────────────────────────────────

/// Build a `data_type -> action` lookup from a policy.
pub(super) fn action_map(policy: &CleanupPolicy) -> HashMap<String, CleanupAction> {
    policy.entries.iter().map(|e| (e.data_type.clone(), e.action)).collect()
}

/// Human label for a cleanup action.
fn action_label(action: CleanupAction) -> &'static str {
    match action {
        CleanupAction::Keep => "keep",
        CleanupAction::Archive => "archive",
        CleanupAction::Delete => "delete",
    }
}

/// Pure, read-only cleanup preview for a project (D11 step 1).
///
/// Enumerates the project's `present` processing artifacts, classifies each,
/// applies the persisted policy, and returns candidate files (those whose data
/// type is policy-actioned to Archive/Delete) plus the total reclaimable bytes.
/// [`DataType::Unclassified`] files are always excluded. No plan is created.
///
/// Each candidate's `reason` carries the classification rationale (source +
/// confidence) and the resolved protection status so users can see protection
/// BEFORE generating a plan (constitution II).
///
/// `total_reclaimable_bytes` is what applying this plan would free under the
/// CURRENT protection settings, not what relaxing them could free: protected
/// candidates stay in the list (so the user can see what is held back) but
/// contribute no bytes, because apply refuses them.
///
/// # Errors
///
/// Returns `ContractError` on database failure.
pub async fn scan(pool: &SqlitePool, project_id: &str) -> Result<CleanupScanResult, ContractError> {
    let policy = get_policy(pool).await?;
    scan_with_policy(pool, project_id, &policy).await
}

/// [`scan`] against an already-loaded policy. `generate` uses this to avoid
/// reading the policy twice (and to guarantee scan + item-building see the
/// same policy snapshot).
pub(super) async fn scan_with_policy(
    pool: &SqlitePool,
    project_id: &str,
    policy: &CleanupPolicy,
) -> Result<CleanupScanResult, ContractError> {
    let actions = action_map(policy);

    // Load global protection once so we can surface protection status per file.
    let global = protection::load_global_protection(pool).await?;

    let rows = artifacts_repo::list_artifacts_for_project(pool, project_id, &["present"])
        .await
        .map_err(db_err)?;

    let mut candidates: Vec<CleanupCandidate> = Vec::new();
    let mut total_reclaimable_bytes: u64 = 0;

    for row in rows {
        let data_type = DataType::from_artifact_kind(&row.kind);
        // Safe default: never propose files we cannot classify.
        if data_type == DataType::Unclassified {
            continue;
        }

        let action = actions.get(data_type.as_str()).copied().unwrap_or(CleanupAction::Keep);
        if action == CleanupAction::Keep {
            continue;
        }

        let size = u64::try_from(row.size_bytes).unwrap_or(0);

        // Resolve protection so the preview surfaces it (constitution II).
        //
        // DECISION NOTE (constitution IV — pinned by test
        // `project_level_unprotected_override_blankets_protected_categories`):
        // the generator keys protection off the PROJECT id as the source id
        // for every item, and `resolve_protection` gives a per-source override
        // row unconditional precedence — the item's category is NOT consulted
        // in the override branch. Consequence: a project-level `unprotected`
        // override would also un-gate master/final items in that project. No
        // shipped path creates project-level overrides today; if project
        // wiring lands (see SourceProtectionOverride.tsx), revisit whether
        // protected-category elevation should survive an override before
        // relying on it here. Do not change resolver semantics silently.
        let resolved = prot_repo::resolve_protection(
            pool,
            project_id,
            Some(data_type.protection_category()),
            &global.level,
            global.block_permanent_delete,
            &global.categories,
        )
        .await
        .map_err(db_err)?;

        if !protection::is_protected(&resolved.level) {
            total_reclaimable_bytes = total_reclaimable_bytes.saturating_add(size);
        }

        let reason = format!(
            "{} artifact (classified by {}, {:.0}% confidence); protection: {}; policy: {}",
            data_type.as_str(),
            row.classification_source,
            row.classification_confidence * 100.0,
            resolved.level,
            action_label(action),
        );

        candidates.push(CleanupCandidate {
            file_path: row.path,
            data_type: data_type.as_str().to_owned(),
            size_bytes: size,
            reason,
        });
    }

    Ok(CleanupScanResult { project_id: project_id.to_owned(), candidates, total_reclaimable_bytes })
}
