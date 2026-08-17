// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Plan review use cases (spec 017).
//!
//! Entry points:
//! - `list_plans`  — list plans with optional state/origin/date filters, failed-first order.
//! - `get_plan`    — fetch a single plan with its items.
//! - `approve_plan` — transition `ready_for_review` (or `draft`) → `approved`; snapshot item FS metadata.
//! - `discard_plan` — soft-delete a plan (any state except `applying`/`paused`).
//! - `retry_plan`   — create a new plan from failed/cancelled/all items of a terminal parent.
//! - `send_archive_to_trash`       — send `<library_root>/.astro-plan-archive/<planId>/` to OS trash.
//! - `permanently_delete_archive`  — permanently remove archive subtree (requires "DELETE" confirm text + spec-016 guard).
//!
//! The apply-side state transitions (`applying`, `paused`, `applied`, `partially_applied`,
//! `failed`, `cancelled`) are exclusively owned by spec 025's executor; this module
//! guards against overwriting those states.

//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b). Its only
//! cross-module dependency was on the now-extracted `app_core_errors` leaf and
//! nothing else in `app_core` references it. `app_core` re-exports this crate at
//! `app_core::plans` so the public surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use camino::Utf8PathBuf;
use contracts_core::lifecycle::PlanState;
use contracts_core::plans::{
    DestructiveDestination, PlanItemAction, PlanItemDetail, PlanItemProtection, PlanItemState,
    PlanOrigin, PlanSummary, PlanType,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use persistence_plans::repositories::plans as repo;
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::plan_apply::resolve_root_path;

mod approve;
mod archive;
mod auto_apply;
mod discard;
mod read;
mod retry;
#[cfg(test)]
mod tests;

pub use approve::approve_plan;
pub use archive::{permanently_delete_archive, send_archive_to_trash};
pub use auto_apply::{
    auto_apply_mkdir_only_plan, plan_qualifies_for_mkdir_auto_apply, AUTO_APPLY_MKDIR_ACTOR,
};
pub use discard::discard_plan;
pub use read::{get_plan, list_plans};
pub use retry::retry_plan;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default age cutoff for plan list (R-Ret-1). Overridable via spec 018 setting.
pub const DEFAULT_AGE_CUTOFF_DAYS: i64 = 90;

/// Confirm text required for `permanently_delete_archive` (spec 017, T046).
pub const PERMANENT_DELETE_CONFIRM_TEXT: &str = "DELETE";

// ── Error helpers ─────────────────────────────────────────────────────────────

// Domain-scoped DB error mapper: routes NotFound to plan.not_found code.
fn db_err(e: persistence_core::DbError) -> ContractError {
    crate::errors::db_err_with_not_found(ErrorCode::PlanNotFound)(e)
}

// ── Row mapping helpers ───────────────────────────────────────────────────────

/// Parses a stored plan-state string via `PlanState`'s `serde` mapping
/// (`#[serde(rename_all = "snake_case")]`) so an unrecognised/corrupt value
/// ERRORS instead of silently coercing to `Draft` (audit T1-b).
fn parse_plan_state(s: &str) -> Result<PlanState, ContractError> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).map_err(|e| {
        ContractError::new(
            ErrorCode::InternalData,
            format!("corrupt plan state {s:?}: {e}"),
            ErrorSeverity::Fatal,
            false,
        )
    })
}

fn parse_plan_origin(s: &str) -> PlanOrigin {
    match s {
        "inbox" => PlanOrigin::Inbox,
        "restructure" => PlanOrigin::Restructure,
        "archive" => PlanOrigin::Archive,
        "project" => PlanOrigin::Project,
        "prepared_view_removal" => PlanOrigin::PreparedViewRemoval,
        "prepared_view_regeneration" => PlanOrigin::PreparedViewRegeneration,
        "prepared_view_generation" => PlanOrigin::PreparedViewGeneration,
        _ => PlanOrigin::Cleanup,
    }
}

fn parse_plan_type(s: &str) -> PlanType {
    match s {
        "split" => PlanType::Split,
        "restructure" => PlanType::Restructure,
        "archive" => PlanType::Archive,
        "source_map" => PlanType::SourceMap,
        "project_create" => PlanType::ProjectCreate,
        "source_view_removal" => PlanType::SourceViewRemoval,
        "source_view_regeneration" => PlanType::SourceViewRegeneration,
        "source_view_generation" => PlanType::SourceViewGeneration,
        _ => PlanType::Cleanup,
    }
}

fn parse_destructive_destination(s: &str) -> DestructiveDestination {
    if s == "trash" {
        DestructiveDestination::OsTrash
    } else {
        DestructiveDestination::Archive
    }
}

fn parse_item_action(s: &str) -> PlanItemAction {
    match s {
        "archive" => PlanItemAction::Archive,
        "delete" => PlanItemAction::Delete,
        "link" => PlanItemAction::Link,
        "write" => PlanItemAction::Write,
        _ => PlanItemAction::Move,
    }
}

fn parse_item_protection(s: &str) -> PlanItemProtection {
    if s == "protected" {
        PlanItemProtection::Protected
    } else {
        PlanItemProtection::Normal
    }
}

fn parse_item_state(s: &str) -> PlanItemState {
    match s {
        "applying" => PlanItemState::Applying,
        "succeeded" => PlanItemState::Succeeded,
        "failed" => PlanItemState::Failed,
        "skipped" => PlanItemState::Skipped,
        "cancelled" => PlanItemState::Cancelled,
        _ => PlanItemState::Pending,
    }
}

fn row_to_summary(row: repo::PlanRow) -> Result<PlanSummary, ContractError> {
    Ok(PlanSummary {
        id: row.id,
        number: row.number,
        title: row.title,
        origin: parse_plan_origin(&row.origin),
        origin_path: row.origin_path,
        state: parse_plan_state(&row.state)?,
        created_at: row.created_at,
        discarded_at: row.discarded_at,
        items_total: row.items_total,
        items_applied: row.items_applied,
        items_failed: row.items_failed,
        items_skipped: row.items_skipped,
        items_cancelled: row.items_cancelled,
        items_pending: row.items_pending,
        total_bytes_required: row.total_bytes_required,
        destructive_destination: parse_destructive_destination(&row.destructive_destination),
        plan_type: parse_plan_type(&row.plan_type),
        parent_plan_id: row.parent_plan_id,
    })
}

fn item_row_to_detail(row: repo::PlanItemRow) -> PlanItemDetail {
    // Resolve absolute paths: currently stored as relative paths.
    // For now surface relative paths; a root-resolver layer is added in spec 025.
    let from = if row.from_relative_path.is_empty() {
        row.from_root_id.clone().unwrap_or_default()
    } else {
        row.from_relative_path.clone()
    };
    let to = if row.to_relative_path.is_empty() {
        row.to_root_id.clone().unwrap_or_default()
    } else {
        row.to_relative_path.clone()
    };

    // Parse provenance JSON if present.
    let provenance = row.provenance.as_deref().and_then(|json| {
        serde_json::from_str::<Vec<contracts_core::plans::ProvenanceEntry>>(json).ok()
    });

    PlanItemDetail {
        id: row.id,
        index: row.item_index,
        name: row.name,
        action: parse_item_action(&row.action),
        from,
        to,
        reason: row.reason,
        protection: parse_item_protection(&row.protection),
        linked: row.linked_entity,
        state: parse_item_state(&row.item_state),
        failure_reason: row.failure_reason,
        provenance,
        approved_mtime: row.approved_mtime,
        approved_size_bytes: row.approved_size_bytes,
        archive_path: row.archive_path,
    }
}

// ── Archive path resolution helpers (shared by approve + archive) ─────────────

/// Build a `from_root_id → absolute library root path` map for the given
/// archived items (T023a pattern, `crate::plan_apply::resolve_root_path`).
async fn build_root_map(
    pool: &SqlitePool,
    items: &[&repo::PlanItemRow],
) -> HashMap<String, Utf8PathBuf> {
    let mut root_map = HashMap::new();
    for rid in items.iter().filter_map(|i| i.from_root_id.as_deref()) {
        if root_map.contains_key(rid) {
            continue;
        }
        if let Some(path) = resolve_root_path(pool, rid).await {
            root_map.insert(rid.to_owned(), Utf8PathBuf::from(path));
        }
    }
    root_map
}
