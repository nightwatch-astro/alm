// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `reopen_plan` (US3, T023).

use audit::bus::EventBus;
use audit::event_bus::{PlanReopened, Source, TOPIC_PLAN_REOPENED};
use contracts_core::lifecycle::PlanState;
use contracts_core::plans::PlanReopenResponse;
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::Timestamp;
use domain_core::lifecycle::plan as plan_lifecycle;
use persistence_plans::repositories::plans as repo;
use sqlx::SqlitePool;

use crate::errors::bus_err;

use super::{db_err, parse_plan_state};

// ── reopen_plan ───────────────────────────────────────────────────────────────

/// Return a plan to `draft` for further editing (US3, T023).
///
/// Allowed only from states with a `→ draft` edge in
/// [`domain_core::lifecycle::plan::TRANSITIONS`]: `approved` and
/// `ready_for_review`. Every other state is refused with
/// `plan.invalid_state` — once a plan reaches `applying` its filesystem actions
/// are in flight or done, so the reviewed intent can no longer be withdrawn
/// (constitution II).
///
/// The approval token and `approvedAt` are cleared, so a token held from before
/// the reopen fails `plan.apply` with `plan.approval.stale`.
///
/// # Errors
///
/// Returns `ContractError` with code:
/// - `plan.not_found` — no matching plan.
/// - `plan.invalid_state` — plan has no `→ draft` transition from its state.
pub async fn reopen_plan(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    actor: &str,
) -> Result<PlanReopenResponse, ContractError> {
    let row = repo::get_plan(pool, plan_id, false).await.map_err(db_err)?;

    let state = parse_plan_state(&row.state)?;

    // Idempotent: already a draft, nothing to reverse.
    if state == PlanState::Draft {
        return Ok(PlanReopenResponse {
            plan_id: plan_id.to_owned(),
            new_state: "draft".to_owned(),
            prior_state: row.state.clone(),
            reopened_at: Timestamp::now_iso(),
        });
    }

    if !plan_lifecycle::is_allowed(state, PlanState::Draft) {
        return Err(ContractError::new(
            ErrorCode::PlanInvalidState,
            format!("cannot reopen a plan in state {:?}; only an approved or ready_for_review plan can return to draft", row.state),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    repo::set_reopened(pool, plan_id).await.map_err(db_err)?;

    let reopened_at = Timestamp::now_iso();

    bus.publish(
        TOPIC_PLAN_REOPENED,
        Source::User,
        PlanReopened {
            plan_id: plan_id.to_owned(),
            prior_state: row.state.clone(),
            actor: actor.to_owned(),
            reopened_at: reopened_at.clone(),
        },
    )
    .await
    .map_err(bus_err)?;

    Ok(PlanReopenResponse {
        plan_id: plan_id.to_owned(),
        new_state: "draft".to_owned(),
        prior_state: row.state,
        reopened_at,
    })
}
