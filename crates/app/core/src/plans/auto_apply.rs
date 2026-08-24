// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! mkdir-only auto-apply (user decision 2026-07-04).

use audit::bus::EventBus;
use contracts_core::ContractError;
use persistence_plans::repositories::plans as repo;
use sqlx::SqlitePool;

use super::db_err;

// ── mkdir-only auto-apply (user decision 2026-07-04) ─────────────────────────

/// Actor recorded on the `plan.approved` audit event when a plan is
/// auto-approved by the mkdir-only auto-apply path.
pub const AUTO_APPLY_MKDIR_ACTOR: &str = "auto.mkdir_only";

/// Returns `true` when a plan's actions qualify for mkdir-only auto-apply.
///
/// Constitution II nuance (user decision 2026-07-04, supersedes handover D16):
/// the reviewable plan record and the full per-action audit trail are STILL
/// written — only the approval *click* is skipped, and only when the plan
/// creates app-owned structure and touches no user file:
///
/// - every action is `mkdir` (directory creation) or `write_manifest` (the
///   app-owned project-marker record item that accompanies the scaffolding
///   mkdirs; the executor performs no user-file mutation for it), and
/// - at least one action is `mkdir`.
///
/// Any user-file action — `move`, `copy`, `link`, `delete`, `archive`,
/// `trash`, `catalogue`, or anything unrecognised — disables auto-apply and
/// the plan goes through the normal review flow unchanged.
pub fn plan_qualifies_for_mkdir_auto_apply<'a, I>(actions: I) -> bool
where
    I: IntoIterator<Item = &'a str>,
{
    let mut saw_mkdir = false;
    for action in actions {
        match action {
            "mkdir" => saw_mkdir = true,
            "write_manifest" => {}
            _ => return false,
        }
    }
    saw_mkdir
}

/// Auto-approve and start applying a freshly persisted plan when (and only
/// when) it qualifies under [`plan_qualifies_for_mkdir_auto_apply`].
///
/// This is the ONLY path permitted to approve without a user gesture; the Inbox
/// apply pipeline used to do the same and no longer does (`astro-plan-tykek`).
///
/// Returns `Ok(None)` when the plan does not qualify — the normal review flow
/// is untouched. When it qualifies, this drives the SAME [`super::approve_plan`] and
/// [`crate::plan_apply::apply_plan`] use-cases as the manual path, so the plan
/// row, the `plan.approved` audit event (actor [`AUTO_APPLY_MKDIR_ACTOR`]),
/// the per-item apply audit records, and the failure handling are identical
/// to a user-clicked apply. A failed auto-apply surfaces exactly like a
/// failed manual apply and leaves the plan reviewable.
///
/// # Errors
///
/// Propagates `ContractError` from the approve or apply use-cases; the plan
/// remains in its current (reviewable) state when this errors.
pub async fn auto_apply_mkdir_only_plan(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
) -> Result<Option<contracts_core::plan_apply::PlanApplyResponse>, ContractError> {
    let items = repo::list_plan_items(pool, plan_id).await.map_err(db_err)?;
    if !plan_qualifies_for_mkdir_auto_apply(items.iter().map(|i| i.action.as_str())) {
        return Ok(None);
    }

    let approve = super::approve_plan(pool, bus, plan_id, AUTO_APPLY_MKDIR_ACTOR).await?;
    let resp =
        crate::plan_apply::apply_plan(pool, bus, plan_id, &approve.approval_token, None).await?;
    Ok(Some(resp))
}
