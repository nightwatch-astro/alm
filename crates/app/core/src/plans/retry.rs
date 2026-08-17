// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `retry_plan` (US5).

use audit::bus::EventBus;
use audit::event_bus::{PlanRetryCreated, Source, TOPIC_PLAN_RETRY_CREATED};
use contracts_core::plans::{PlanRetryResponse, RetryItemsFilter};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::{new_id, Timestamp};
use persistence_plans::repositories::plans as repo;
use sqlx::SqlitePool;

use crate::errors::bus_err;

use super::{db_err, parse_plan_state};

// ── retry_plan ────────────────────────────────────────────────────────────────

/// Create a retry plan from a terminal parent plan (US5, T035, T036).
///
/// The parent must be in a terminal state (`applied`, `partially_applied`,
/// `failed`, `cancelled`, or `discarded`). The new plan starts in `draft`.
/// `parentPlanId` is set on the new plan; the parent is not mutated (T033).
///
/// Default `items_filter` is `"failed"` (R-Retry-1).
///
/// # Errors
///
/// Returns `ContractError` with code:
/// - `parent.not_found` — parent plan does not exist.
/// - `parent.not_terminal` — parent is not in a terminal state.
/// - `no.items.to.retry` — no items match the filter.
#[allow(clippy::too_many_lines)]
pub async fn retry_plan(
    pool: &SqlitePool,
    bus: &EventBus,
    parent_plan_id: &str,
    items_filter: RetryItemsFilter,
) -> Result<PlanRetryResponse, ContractError> {
    // Load parent (including discarded — discarded plans can have retry children).
    let parent = repo::get_plan(pool, parent_plan_id, true).await.map_err(|_| {
        ContractError::new(
            ErrorCode::ParentNotFound,
            format!("parent plan {parent_plan_id} not found"),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    let parent_state = parse_plan_state(&parent.state)?;

    // Must be terminal.
    if !parent_state.is_terminal() {
        return Err(ContractError::new(
            ErrorCode::ParentNotTerminal,
            format!("parent plan state {:?} is not terminal", parent.state),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Load parent items to determine which to carry forward.
    let parent_items = repo::list_plan_items(pool, parent_plan_id).await.map_err(db_err)?;

    let items_to_retry: Vec<&repo::PlanItemRow> = match items_filter {
        RetryItemsFilter::Failed => {
            parent_items.iter().filter(|i| i.item_state == "failed").collect()
        }
        RetryItemsFilter::Cancelled => {
            parent_items.iter().filter(|i| i.item_state == "cancelled").collect()
        }
        RetryItemsFilter::All => parent_items.iter().collect(),
    };

    if items_to_retry.is_empty() {
        return Err(ContractError::new(
            ErrorCode::NoItemsToRetry,
            "no items match the specified filter".to_owned(),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let new_plan_id = new_id();
    let at = Timestamp::now_iso();

    // Create new plan (draft) referencing parent.
    repo::insert_plan(
        pool,
        &repo::InsertPlan {
            id: &new_plan_id,
            title: &format!("Retry of plan #{}", parent.number),
            origin: &parent.origin,
            origin_path: parent.origin_path.as_deref(),
            plan_type: &parent.plan_type,
            destructive_destination: &parent.destructive_destination,
            parent_plan_id: Some(parent_plan_id),
            total_bytes_required: 0,
        },
    )
    .await
    .map_err(db_err)?;

    // Copy selected items as `pending` into the new plan.
    for (idx, item) in items_to_retry.iter().enumerate() {
        let new_item_id = new_id();
        repo::insert_plan_item(
            pool,
            &repo::InsertPlanItem {
                id: &new_item_id,
                plan_id: &new_plan_id,
                item_index: i64::try_from(idx + 1).unwrap_or(i64::MAX),
                name: &item.name,
                action: &item.action,
                from_root_id: item.from_root_id.as_deref(),
                from_relative_path: &item.from_relative_path,
                to_root_id: item.to_root_id.as_deref(),
                to_relative_path: &item.to_relative_path,
                reason: &item.reason,
                protection: &item.protection,
                linked_entity: item.linked_entity.as_deref(),
                provenance_json: item.provenance.as_deref(),
                archive_path: item.archive_path.as_deref(),
                // Propagate real source identity when retrying items (FR-016).
                source_id: item.source_id.as_deref(),
                category: item.category.as_deref(),
            },
        )
        .await
        .map_err(db_err)?;
    }

    let items_total = i64::try_from(items_to_retry.len()).unwrap_or(i64::MAX);

    // Emit audit event (T036, A7).
    let filter_label = match items_filter {
        RetryItemsFilter::Failed => "failed",
        RetryItemsFilter::Cancelled => "cancelled",
        RetryItemsFilter::All => "all",
    };

    bus.publish(
        TOPIC_PLAN_RETRY_CREATED,
        Source::User,
        PlanRetryCreated {
            new_plan_id: new_plan_id.clone(),
            parent_plan_id: parent_plan_id.to_owned(),
            items_filter: filter_label.to_owned(),
            items_total,
            at,
        },
    )
    .await
    .map_err(bus_err)?;

    Ok(PlanRetryResponse { new_plan_id, parent_plan_id: parent_plan_id.to_owned(), items_total })
}
