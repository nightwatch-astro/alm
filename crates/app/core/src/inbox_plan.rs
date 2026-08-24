// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Inbox plan use-cases (spec 041, US1).
//!
//! Entry points:
//! - `get_inbox_plan`       — fetch the plan linked to an inbox item as `InboxPlanView`.
//! - `apply_inbox_plan`     — apply the plan the caller has already approved; on
//!   success marks the inbox item `resolved`.
//! - `apply_all_inbox_plans`— apply every `plan_open` item's plan; per-plan results.
//! - `cancel_inbox_plan`    — discard the linked plan; item returns to `classified`.
//!
//! FR-003 / FR-003a / FR-005 / FR-006 / FR-007.
//!
//! The apply path reuses the existing `crates/app/core/src/plan_apply.rs` executor
//! (CAS, staleness detection, audit records). Staleness surfaces as `plan.stale`
//! (FR-007) rather than a silent partial move.

use audit::bus::EventBus;
use contracts_core::error_code::ErrorCode;
use contracts_core::inbox::{
    InboxApplyAllResponse, InboxOpenPlan, InboxOpenPlansResponse, InboxPlanAction,
    InboxPlanApplyResult, InboxPlanCancelResponse, InboxPlanView,
};
use contracts_core::plan_apply::PlanApplyResponse;
use contracts_core::{ContractError, ErrorSeverity};
use persistence_inbox::repositories::inbox as inbox_repo;
use persistence_plans::repositories::plans as plans_repo;
use sqlx::SqlitePool;

// ── Error helpers ─────────────────────────────────────────────────────────────

fn db_err_not_found(msg: impl Into<String>) -> ContractError {
    ContractError::new(ErrorCode::InboxItemNotFound, msg, ErrorSeverity::Blocking, false)
}

fn db_err_internal(e: impl std::fmt::Display) -> ContractError {
    ContractError::new(ErrorCode::InternalDatabase, e.to_string(), ErrorSeverity::Fatal, true)
}

/// Serialise an `ErrorCode` to its dotted wire string for response `error` fields.
fn error_code_str(code: ErrorCode) -> String {
    serde_json::to_string(&code)
        .map_or_else(|_| "internal.error".to_owned(), |s| s.trim_matches('"').to_owned())
}

fn no_plan_err(inbox_item_id: &str) -> ContractError {
    ContractError::new(
        ErrorCode::InboxItemNoPlan,
        format!("no linked plan found for inbox item {inbox_item_id}"),
        ErrorSeverity::Blocking,
        false,
    )
}

/// Refusal for an inbox apply whose plan carries no approval (Constitution II).
fn approval_required_err(plan_id: &str) -> ContractError {
    ContractError::new(
        ErrorCode::PlanApprovalRequired,
        format!("plan {plan_id} is not approved; approve it before applying"),
        ErrorSeverity::Blocking,
        false,
    )
}

/// Map persisted plan-item rows to the contract `InboxPlanAction` shape.
///
/// Shared by `get_inbox_plan` (single-item view) and `list_open_inbox_plans`
/// (aggregate surface) so the action projection stays in one place.
fn map_plan_actions(item_rows: Vec<plans_repo::PlanItemRow>) -> Vec<InboxPlanAction> {
    item_rows
        .into_iter()
        .map(|r| {
            let dest_preview = if r.to_relative_path.is_empty() {
                r.from_relative_path.clone()
            } else {
                r.to_relative_path.clone()
            };
            InboxPlanAction {
                index: u32::try_from(r.item_index).unwrap_or(0) + 1,
                action: r.action,
                from_path: r.from_relative_path,
                to_path: r.to_relative_path,
                destination_preview: dest_preview,
                requires_destructive_confirm: r.requires_destructive_confirm.unwrap_or(0) != 0,
                // `category` carries the per-file frame type for inbox confirm plans
                // (stored at plan creation; `None` for legacy rows and other plan types).
                frame_type: r.category,
            }
        })
        .collect()
}

// ── get_inbox_plan ────────────────────────────────────────────────────────────

/// `inbox.plan` — fetch the plan linked to an inbox item (FR-003/FR-004).
///
/// Reads via `inbox_plan_links` then loads the plan header + items.
/// The `state` field reflects the current DB plan state.
///
/// # Errors
///
/// - `inbox.item.not_found` — item not found.
/// - `inbox.item.no_plan`   — no linked plan.
/// - `plan.not_found`       — plan row missing (shouldn't happen in practice).
/// - `internal.database`    — DB failure.
pub async fn get_inbox_plan(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> Result<InboxPlanView, ContractError> {
    // Verify the inbox item exists.
    inbox_repo::get_inbox_item(pool, inbox_item_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => {
            db_err_not_found(format!("inbox item {inbox_item_id} not found"))
        }
        other => db_err_internal(other),
    })?;

    // Look up the plan link.
    let link = inbox_repo::get_plan_link(pool, inbox_item_id)
        .await
        .map_err(db_err_internal)?
        .ok_or_else(|| no_plan_err(inbox_item_id))?;

    let plan_id = link.plan_id;

    // Load plan header.
    let plan_row = plans_repo::get_plan(pool, &plan_id, false).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => ContractError::new(
            ErrorCode::PlanNotFound,
            format!("plan {plan_id} not found"),
            ErrorSeverity::Blocking,
            false,
        ),
        other => db_err_internal(other),
    })?;

    // Load plan items.
    let item_rows = plans_repo::list_plan_items(pool, &plan_id).await.map_err(db_err_internal)?;

    let actions = map_plan_actions(item_rows);

    // FR-007: surface staleness so the UI can disable Apply and prompt
    // re-classify/re-confirm.  A plan is stale when its DB state is `stale`
    // (the executor's CAS check transitioned it there on a prior apply attempt).
    let stale = plan_row.state == "stale";

    Ok(InboxPlanView { plan_id, state: plan_row.state, stale, actions })
}

// ── apply_inbox_plan ──────────────────────────────────────────────────────────

/// `inbox.plan.apply` — apply the already-approved plan linked to this inbox item.
///
/// Pipeline:
/// 1. Resolve the linked plan.
/// 2. Read the approval the caller recorded via `plans.approve`; refuse when absent.
/// 3. Call the existing `apply_plan` executor (CAS, staleness, audit).
/// 4. The plan listener already handles `resolved` transition when the plan
///    reaches `applied` state — we don't set it here to avoid double-write.
///
/// Constitution II: this path must never approve on the user's behalf. It once
/// called `approve_plan(actor = "inbox.apply")` itself, which both attributed
/// the approval to the code rather than the person and made the approval-token
/// gate in `apply_plan` unable to fail for every inbox apply
/// (`astro-plan-tykek`). The mkdir-only auto-apply path
/// (`crate::plans::auto_apply`) remains the one documented exception, and it
/// does not route through here.
///
/// Staleness (FR-007): the executor's CAS check fires per-item and transitions
/// stale items to `stale` state, which surfaces as `plan.stale` error code.
///
/// # Errors
///
/// - `inbox.item.not_found`     — item not found.
/// - `inbox.item.no_plan`       — no linked plan.
/// - `plan.approval_required`   — the plan carries no approval.
/// - `plan.invalid_state`       — plan cannot be applied from its current state.
/// - `plan.stale`               — one or more source files changed since planning.
/// - `internal.database`        — DB failure.
pub async fn apply_inbox_plan(
    pool: &SqlitePool,
    bus: &EventBus,
    inbox_item_id: &str,
) -> Result<PlanApplyResponse, ContractError> {
    // Verify the inbox item exists.
    inbox_repo::get_inbox_item(pool, inbox_item_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => {
            db_err_not_found(format!("inbox item {inbox_item_id} not found"))
        }
        other => db_err_internal(other),
    })?;

    // Resolve the linked plan.
    let link = inbox_repo::get_plan_link(pool, inbox_item_id)
        .await
        .map_err(db_err_internal)?
        .ok_or_else(|| no_plan_err(inbox_item_id))?;

    let plan_id = link.plan_id;

    // Read the approval the caller recorded; never mint one here.
    let plan_row = plans_repo::get_plan(pool, &plan_id, false).await.map_err(db_err_internal)?;
    let approval_token = plan_row
        .approval_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| approval_required_err(&plan_id))?;

    // Run the executor (CAS, audit, staleness).
    // The plan listener will catch `plan.applying.completed` and transition the
    // inbox item to `resolved` automatically.
    let resp = crate::plan_apply::apply_plan(pool, bus, &plan_id, &approval_token, None).await?;

    Ok(resp)
}

// ── apply_all_inbox_plans ─────────────────────────────────────────────────────

/// `inbox.plan.apply_all` — apply all `plan_open` inbox items' plans (FR-003a).
///
/// Iterates items in `plan_open` state across roots, applies each plan
/// sequentially. Returns per-plan results. Each action is individually audited
/// by the executor. A plan the caller has not approved is reported as a per-plan
/// `plan.approval_required` error, not applied.
///
/// # Errors
///
/// Returns a `ContractError` only on list-query failure (hard error). Per-plan
/// failures are captured in `results` with their error codes.
pub async fn apply_all_inbox_plans(
    pool: &SqlitePool,
    bus: &EventBus,
) -> Result<InboxApplyAllResponse, ContractError> {
    // Fetch all plan_open items.
    let all_items =
        inbox_repo::list_unacknowledged_across_roots(pool, 500).await.map_err(db_err_internal)?;

    let plan_open_items: Vec<_> =
        all_items.into_iter().filter(|r| r.state == "plan_open").collect();

    let mut results = Vec::new();
    for item in plan_open_items {
        let item_id = item.id.clone();
        match apply_inbox_plan(pool, bus, &item_id).await {
            Ok(resp) => results.push(InboxPlanApplyResult {
                inbox_item_id: item_id,
                plan_id: resp.plan_id,
                state: resp.new_state,
                error: None,
            }),
            Err(e) => results.push(InboxPlanApplyResult {
                inbox_item_id: item_id,
                plan_id: String::new(),
                state: "error".to_owned(),
                error: Some(error_code_str(e.code)),
            }),
        }
    }

    Ok(InboxApplyAllResponse { results })
}

// ── list_open_inbox_plans ─────────────────────────────────────────────────────

/// `inbox.plan.list_open` — return every open plan across all roots (spec 041, US2).
///
/// Fetches all `plan_open` inbox items, loads each item's linked plan header +
/// items, and projects them into the aggregate surface shape so the UI can show
/// every active planned action at once without selecting items one by one.
///
/// Items whose plan link is missing are skipped defensively (an item should not
/// be in `plan_open` without a link, but we never hard-fail the whole surface
/// over a single inconsistent row).
///
/// # Errors
/// Returns `internal.database` if the list query or a plan load fails.
pub async fn list_open_inbox_plans(
    pool: &SqlitePool,
) -> Result<InboxOpenPlansResponse, ContractError> {
    let all_items =
        inbox_repo::list_unacknowledged_across_roots(pool, 500).await.map_err(db_err_internal)?;

    let plan_open_items: Vec<_> =
        all_items.into_iter().filter(|r| r.state == "plan_open").collect();

    let mut plans = Vec::new();
    let mut total_actions: u32 = 0;

    for item in plan_open_items {
        // Resolve the plan link; skip defensively when absent.
        let Some(link) =
            inbox_repo::get_plan_link(pool, &item.id).await.map_err(db_err_internal)?
        else {
            continue;
        };
        let plan_id = link.plan_id;

        // Load the plan header; skip defensively when the row is missing.
        let plan_row = match plans_repo::get_plan(pool, &plan_id, false).await {
            Ok(row) => row,
            Err(persistence_core::DbError::NotFound(_)) => continue,
            Err(other) => return Err(db_err_internal(other)),
        };

        let item_rows =
            plans_repo::list_plan_items(pool, &plan_id).await.map_err(db_err_internal)?;
        let actions = map_plan_actions(item_rows);

        total_actions = total_actions.saturating_add(u32::try_from(actions.len()).unwrap_or(0));

        plans.push(InboxOpenPlan {
            inbox_item_id: item.id,
            item_name: item.relative_path,
            plan_id,
            state: plan_row.state.clone(),
            stale: plan_row.state == "stale",
            actions,
        });
    }

    Ok(InboxOpenPlansResponse { plans, total_actions })
}

// ── apply_selected_inbox_plans ────────────────────────────────────────────────

/// `inbox.plan.apply_selected` — apply a caller-chosen subset of inbox plans
/// (spec 041, US2).
///
/// Selection is plan-level (per inbox item / ingestion group), not per
/// individual action. Iterates only the provided ids; ids that are not in
/// `plan_open` state are reported as a per-item error (`plan.invalid_state`)
/// rather than hard-failing the whole call, mirroring `apply_all_inbox_plans`'
/// partial-failure semantics. An id whose plan carries no approval is reported
/// as `plan.approval_required`, not applied.
///
/// # Errors
/// Returns `internal.database` only if the membership query fails; per-plan
/// errors are captured inside `InboxApplyAllResponse.results`.
pub async fn apply_selected_inbox_plans(
    pool: &SqlitePool,
    bus: &EventBus,
    inbox_item_ids: &[String],
) -> Result<InboxApplyAllResponse, ContractError> {
    // Build the set of currently-`plan_open` item ids so we can validate each
    // requested id without a per-id round trip.
    let all_items =
        inbox_repo::list_unacknowledged_across_roots(pool, 500).await.map_err(db_err_internal)?;
    let plan_open_ids: std::collections::HashSet<String> =
        all_items.into_iter().filter(|r| r.state == "plan_open").map(|r| r.id).collect();

    let mut results = Vec::new();
    for item_id in inbox_item_ids {
        if !plan_open_ids.contains(item_id) {
            results.push(InboxPlanApplyResult {
                inbox_item_id: item_id.clone(),
                plan_id: String::new(),
                state: "error".to_owned(),
                error: Some("plan.invalid_state".to_owned()),
            });
            continue;
        }
        match apply_inbox_plan(pool, bus, item_id).await {
            Ok(resp) => results.push(InboxPlanApplyResult {
                inbox_item_id: item_id.clone(),
                plan_id: resp.plan_id,
                state: resp.new_state,
                error: None,
            }),
            Err(e) => results.push(InboxPlanApplyResult {
                inbox_item_id: item_id.clone(),
                plan_id: String::new(),
                state: "error".to_owned(),
                error: Some(error_code_str(e.code)),
            }),
        }
    }

    Ok(InboxApplyAllResponse { results })
}

// ── cancel_inbox_plan ─────────────────────────────────────────────────────────

/// `inbox.plan.cancel` — discard the linked plan and return the item to `classified`.
///
/// Calls `discard_plan` (emits `plan.discarded` audit event). The plan listener
/// catches the event and transitions the item back to `classified`. If the
/// listener is not yet caught up, we also explicitly reset the state here to
/// ensure immediate consistency (FR-006).
///
/// # Errors
///
/// - `inbox.item.not_found` — item not found.
/// - `inbox.item.no_plan`   — no linked plan.
/// - `plan.not_found`       — plan row missing.
/// - `plan.in_progress`     — plan is currently applying (cannot cancel via this path).
/// - `internal.database`    — DB failure.
pub async fn cancel_inbox_plan(
    pool: &SqlitePool,
    bus: &EventBus,
    inbox_item_id: &str,
) -> Result<InboxPlanCancelResponse, ContractError> {
    // Verify the inbox item exists.
    inbox_repo::get_inbox_item(pool, inbox_item_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => {
            db_err_not_found(format!("inbox item {inbox_item_id} not found"))
        }
        other => db_err_internal(other),
    })?;

    // Resolve the linked plan.
    let link = inbox_repo::get_plan_link(pool, inbox_item_id)
        .await
        .map_err(db_err_internal)?
        .ok_or_else(|| no_plan_err(inbox_item_id))?;

    let plan_id = link.plan_id.clone();

    // Discard the plan (emits plan.discarded; the plan listener will handle
    // the inbox item state transition, but we also do it eagerly below).
    crate::plans::discard_plan(pool, bus, &plan_id).await?;

    // Eagerly reset the inbox item and remove the link so the list shows it as
    // unconfirmed immediately (the listener catches up async). The target state
    // is derived from the row's own frame type, not asserted — see
    // `reset_inbox_item_to_unconfirmed` (spec 058 SC-003).
    inbox_repo::reset_inbox_item_to_unconfirmed(pool, inbox_item_id)
        .await
        .map_err(db_err_internal)?;
    inbox_repo::delete_plan_link(pool, inbox_item_id).await.map_err(db_err_internal)?;

    Ok(InboxPlanCancelResponse {
        inbox_item_id: inbox_item_id.to_owned(),
        plan_id,
        state: "discarded".to_owned(),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox::confirm::{confirm, ConfirmRequest};
    use audit::bus::EventBus;
    use persistence_core::Database;
    use persistence_inbox::repositories::inbox as inbox_repo;
    use sqlx::SqlitePool;
    use std::io::Write as IoWrite;
    use std::path::PathBuf;
    use tempfile::tempdir;

    async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    fn make_bus(pool: &SqlitePool) -> EventBus {
        EventBus::with_pool(pool.clone())
    }

    /// Write a minimal FITS file (valid enough for fits_io to open).
    fn write_fits(path: &std::path::Path, image_type: &str) {
        // 36-card primary header with IMAGETYP, END.
        let mut f = std::fs::File::create(path).unwrap();
        let card = |s: &str| -> [u8; 80] {
            let mut buf = [b' '; 80];
            let bytes = s.as_bytes();
            buf[..bytes.len().min(80)].copy_from_slice(&bytes[..bytes.len().min(80)]);
            buf
        };
        // SIMPLE + BITPIX + NAXIS + IMAGETYP + END (5 cards; pad to 36 to complete a block).
        let cards: Vec<[u8; 80]> = vec![
            card("SIMPLE  =                    T / file conforms to FITS standard"),
            card("BITPIX  =                   16 / number of bits per data pixel"),
            card("NAXIS   =                    0 / number of data axes"),
            card(&format!("IMAGETYP= '{image_type:<8}' / frame type")),
            card("END"),
        ];
        for c in &cards {
            f.write_all(c).unwrap();
        }
        // Pad to 2880-byte block.
        let used = cards.len() * 80;
        let pad = 2880 - (used % 2880);
        f.write_all(&vec![b' '; pad]).unwrap();
    }

    /// Set up a registered source + classified inbox item, returning
    /// `(item_id, root_path, tempdir)`.
    ///
    /// The `TempDir` is returned rather than dropped here: dropping it deletes
    /// the fixture's files, so an apply test would run against a root that no
    /// longer exists.
    async fn setup_classified_item(db: &Database) -> (String, PathBuf, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let root_path = dir.path().to_path_buf();
        let item_dir = root_path.join("lights");
        std::fs::create_dir_all(&item_dir).unwrap();
        write_fits(&item_dir.join("img001.fits"), "Light Frame");

        let root_id = "root-plan-test";
        let item_id = "item-plan-test";

        // Register source.
        sqlx::query(
            "INSERT INTO registered_sources (id, kind, path, kind_subtype, scan_depth, created_at, created_via)
             VALUES (?, 'inbox', ?, NULL, 'recursive', '2025-01-01T00:00:00Z', 'first_run')
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(root_id)
        .bind(root_path.to_str().unwrap())
        .execute(db.pool())
        .await
        .unwrap();

        // Insert inbox item in `classified` state. `frame_type` is set because
        // spec 058 SC-003 only lets a row carrying one report `classified`; a
        // fixture without it was silently modelling an illegal row.
        sqlx::query(
            "INSERT INTO inbox_items
             (id, root_id, relative_path, file_count, discovered_at, last_scanned_at,
              content_signature, state, lane, frame_type)
             VALUES (?, ?, 'lights', 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z',
                     'sig-abc', 'classified', 'fits', 'light')",
        )
        .bind(item_id)
        .bind(root_id)
        .execute(db.pool())
        .await
        .unwrap();

        // Upsert classification.
        sqlx::query(
            "INSERT INTO inbox_classifications
             (inbox_item_id, result, frame_type, computed_at, content_signature, unclassified_file_count)
             VALUES (?, 'classified', 'light', '2025-01-01T00:00:00Z', 'sig-abc', 0)
             ON CONFLICT(inbox_item_id) DO UPDATE SET
                 result = excluded.result, frame_type = excluded.frame_type,
                 computed_at = excluded.computed_at, content_signature = excluded.content_signature,
                 unclassified_file_count = excluded.unclassified_file_count",
        )
        .bind(item_id)
        .execute(db.pool())
        .await
        .unwrap();

        // Insert evidence for classification.
        sqlx::query(
            "INSERT INTO inbox_classification_evidence
             (id, inbox_item_id, relative_file_path, frame_type, evidence_source, raw_value,
              unclassified, is_master)
             VALUES (?, ?, 'img001.fits', 'light', 'imagetyp_header', 'Light Frame', 0, 0)",
        )
        .bind(format!("ev-{item_id}"))
        .bind(item_id)
        .execute(db.pool())
        .await
        .unwrap();

        // Insert a minimal active naming pattern via settings (required by confirm).
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('pattern', ?, '2025-01-01T00:00:00Z') ON CONFLICT(key) DO NOTHING",
        )
        .bind("[{\"id\":\"p0\",\"kind\":\"token\",\"value\":\"frame_type\"},{\"id\":\"p1\",\"kind\":\"separator\",\"value\":\"/\"}]")
        .execute(db.pool())
        .await
        .unwrap();

        // Override per-type destination patterns with a literal-only pattern so
        // test fixtures without FITS metadata attributes (target/filter/date)
        // can confirm without triggering InboxMissingPathAttributes (spec 041).
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('patternsByType', ?, '2025-01-01T00:00:00Z') ON CONFLICT(key) DO NOTHING",
        )
        .bind("{\"light\":\"lights/\"}")
        .execute(db.pool())
        .await
        .unwrap();

        // Register a light_frames library root so spec-041 destination-root
        // selection succeeds (select_destination_root requires a non-inbox
        // registered_sources row with matching kind).
        let dest_root_id = "dest-root-plan-test";
        sqlx::query(
            "INSERT INTO registered_sources (id, kind, path, kind_subtype, scan_depth, created_at, created_via)
             VALUES (?, 'light_frames', ?, NULL, 'recursive', '2025-01-01T00:00:00Z', 'first_run')
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(dest_root_id)
        .bind(root_path.to_str().unwrap())
        .execute(db.pool())
        .await
        .unwrap();
        (item_id.to_owned(), root_path, dir)
    }

    /// Confirm an item (creates the plan, sets state to plan_open).
    async fn do_confirm(db: &Database, item_id: &str, root_path: &std::path::Path) -> String {
        let req = ConfirmRequest {
            inbox_item_id: item_id.to_owned(),
            content_signature: "sig-abc".to_owned(),
            destructive_destination: None,
            root_absolute_path: root_path.to_path_buf(),
            root_id: None,
            chosen_attribution: None,
        };
        let bus = EventBus::with_pool(db.pool().clone());
        let resp = confirm(db.pool(), &bus, req).await.unwrap();
        assert!(!resp.plan_id.is_empty(), "confirm must return a plan_id");
        resp.plan_id
    }

    // ── T015a: confirm creates plan link; item visible as plan_open ──────────

    #[tokio::test]
    async fn confirm_creates_plan_link_and_plan_open_state() {
        let db = test_db().await;
        let (item_id, root_path, _dir) = setup_classified_item(&db).await;

        let plan_id = do_confirm(&db, &item_id, &root_path).await;

        // Item should now be plan_open.
        let item = inbox_repo::get_inbox_item(db.pool(), &item_id).await.unwrap();
        assert_eq!(item.state, "plan_open");

        // Plan link should exist.
        let link = inbox_repo::get_plan_link(db.pool(), &item_id).await.unwrap().unwrap();
        assert_eq!(link.plan_id, plan_id);

        // Item should appear in list_unacknowledged_across_roots.
        let rows = inbox_repo::list_unacknowledged_across_roots(db.pool(), 100).await.unwrap();
        let found = rows.iter().any(|r| r.id == item_id);
        assert!(found, "plan_open item must remain visible in list");
    }

    // ── T015b: get_inbox_plan returns the plan view ──────────────────────────

    #[tokio::test]
    async fn get_inbox_plan_returns_view() {
        let db = test_db().await;
        let bus = make_bus(db.pool());
        let _ = bus; // bus not needed for get
        let (item_id, root_path, _dir) = setup_classified_item(&db).await;

        let plan_id = do_confirm(&db, &item_id, &root_path).await;

        let view = get_inbox_plan(db.pool(), &item_id).await.unwrap();
        assert_eq!(view.plan_id, plan_id);
        assert!(!view.actions.is_empty(), "plan must have at least one action");
    }

    // ── T015c: cancel returns item to classified ─────────────────────────────

    #[tokio::test]
    async fn cancel_inbox_plan_returns_to_classified() {
        let db = test_db().await;
        let bus = make_bus(db.pool());
        let (item_id, root_path, _dir) = setup_classified_item(&db).await;

        do_confirm(&db, &item_id, &root_path).await;

        // Cancel the plan.
        let resp = cancel_inbox_plan(db.pool(), &bus, &item_id).await.unwrap();
        assert_eq!(resp.state, "discarded");

        // Item should be back to classified.
        let item = inbox_repo::get_inbox_item(db.pool(), &item_id).await.unwrap();
        assert_eq!(item.state, "classified");

        // Plan link must be gone.
        let link = inbox_repo::get_plan_link(db.pool(), &item_id).await.unwrap();
        assert!(link.is_none(), "plan link must be removed after cancel");
    }

    /// spec 058 SC-003 on the plan-lifecycle writers.
    ///
    /// `confirm` gates on `needs_review` and the cached classification result,
    /// never on the item's own `state`/`frame_type`, so a row with no frame
    /// type is confirmable. Cancelling its plan must not be the thing that
    /// stamps `classified` onto it — that is the #711 badge defect arriving by
    /// the back door, on a path no classify-side test covers.
    #[tokio::test]
    async fn cancel_does_not_report_classified_without_a_frame_type_sc003() {
        let db = test_db().await;
        let bus = make_bus(db.pool());
        let (item_id, root_path, _dir) = setup_classified_item(&db).await;

        sqlx::query("UPDATE inbox_items SET frame_type = NULL WHERE id = ?")
            .bind(&item_id)
            .execute(db.pool())
            .await
            .unwrap();

        do_confirm(&db, &item_id, &root_path).await;
        cancel_inbox_plan(db.pool(), &bus, &item_id).await.unwrap();

        let item = inbox_repo::get_inbox_item(db.pool(), &item_id).await.unwrap();
        assert_eq!(
            item.state, "pending_classification",
            "an item with no frame type must not report `classified` after its plan is cancelled"
        );
    }

    // ── T015d: get_inbox_plan not_found for item without plan ───────────────

    #[tokio::test]
    async fn get_inbox_plan_errors_when_no_plan_linked() {
        let db = test_db().await;
        let (item_id, _, _dir) = setup_classified_item(&db).await;

        let err = get_inbox_plan(db.pool(), &item_id).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InboxItemNoPlan);
    }

    // ── T015e: cancel errors when no plan linked ─────────────────────────────

    #[tokio::test]
    async fn cancel_inbox_plan_errors_when_no_plan_linked() {
        let db = test_db().await;
        let bus = make_bus(db.pool());
        let (item_id, _, _dir) = setup_classified_item(&db).await;

        let err = cancel_inbox_plan(db.pool(), &bus, &item_id).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InboxItemNoPlan);
    }

    // ── spec 041 US2: aggregate open-plans surface ──────────────────────────

    /// Parameterized variant of [`setup_classified_item`] that creates a
    /// distinctly-identified inbox item under its own root, so multiple
    /// `plan_open` items can coexist in one DB.  `suffix` makes ids/paths unique.
    async fn setup_classified_item_suffixed(db: &Database, suffix: &str) -> (String, PathBuf) {
        let dir = tempdir().unwrap();
        // Leak the tempdir so the on-disk files survive past this fn for the
        // plan executor (apply path reads real files). Tests are short-lived.
        let root_path = dir.keep();
        let item_dir = root_path.join("lights");
        std::fs::create_dir_all(&item_dir).unwrap();
        write_fits(&item_dir.join("img001.fits"), "Light Frame");

        let root_id = format!("root-plan-{suffix}");
        let item_id = format!("item-plan-{suffix}");

        sqlx::query(
            "INSERT INTO registered_sources (id, kind, path, kind_subtype, scan_depth, created_at, created_via)
             VALUES (?, 'inbox', ?, NULL, 'recursive', '2025-01-01T00:00:00Z', 'first_run')
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(&root_id)
        .bind(root_path.to_str().unwrap())
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO inbox_items
             (id, root_id, relative_path, file_count, discovered_at, last_scanned_at,
              content_signature, state, lane)
             VALUES (?, ?, 'lights', 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z',
                     'sig-abc', 'classified', 'fits')",
        )
        .bind(&item_id)
        .bind(&root_id)
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO inbox_classifications
             (inbox_item_id, result, frame_type, computed_at, content_signature, unclassified_file_count)
             VALUES (?, 'classified', 'light', '2025-01-01T00:00:00Z', 'sig-abc', 0)
             ON CONFLICT(inbox_item_id) DO UPDATE SET
                 result = excluded.result, frame_type = excluded.frame_type,
                 computed_at = excluded.computed_at, content_signature = excluded.content_signature,
                 unclassified_file_count = excluded.unclassified_file_count",
        )
        .bind(&item_id)
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO inbox_classification_evidence
             (id, inbox_item_id, relative_file_path, frame_type, evidence_source, raw_value,
              unclassified, is_master)
             VALUES (?, ?, 'img001.fits', 'light', 'imagetyp_header', 'Light Frame', 0, 0)",
        )
        .bind(format!("ev-{item_id}"))
        .bind(&item_id)
        .execute(db.pool())
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('pattern', ?, '2025-01-01T00:00:00Z') ON CONFLICT(key) DO NOTHING",
        )
        .bind("[{\"id\":\"p0\",\"kind\":\"token\",\"value\":\"frame_type\"},{\"id\":\"p1\",\"kind\":\"separator\",\"value\":\"/\"}]")
        .execute(db.pool())
        .await
        .unwrap();

        // Override per-type destination patterns with literal-only pattern so
        // test fixtures without FITS metadata can confirm (spec 041).
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('patternsByType', ?, '2025-01-01T00:00:00Z') ON CONFLICT(key) DO NOTHING",
        )
        .bind("{\"light\":\"lights/\"}")
        .execute(db.pool())
        .await
        .unwrap();

        // Register a single shared light_frames destination root for inbox → library routing.
        // All suffixed setups share one dest root so there is exactly one candidate
        // and confirm auto-selects it (no InboxDestinationRootRequired).
        sqlx::query(
            "INSERT INTO registered_sources (id, kind, path, kind_subtype, scan_depth, created_at, created_via)
             VALUES ('dest-root-shared', 'light_frames', '/tmp/dest-shared', NULL, 'recursive', '2025-01-01T00:00:00Z', 'first_run')
             ON CONFLICT(id) DO NOTHING",
        )
        .execute(db.pool())
        .await
        .unwrap();

        (item_id, root_path)
    }

    /// `list_open_inbox_plans` returns every `plan_open` plan with its actions
    /// and a correct `total_actions` sum.
    #[tokio::test]
    async fn list_open_returns_all_plan_open_plans() {
        let db = test_db().await;

        let (id_a, root_a) = setup_classified_item_suffixed(&db, "a").await;
        let (id_b, root_b) = setup_classified_item_suffixed(&db, "b").await;
        do_confirm(&db, &id_a, &root_a).await;
        do_confirm(&db, &id_b, &root_b).await;

        let resp = list_open_inbox_plans(db.pool()).await.unwrap();

        assert_eq!(resp.plans.len(), 2, "both plan_open items should appear");
        let mut ids: Vec<&str> = resp.plans.iter().map(|p| p.inbox_item_id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![id_a.as_str(), id_b.as_str()]);

        // Every plan carries its actions, item_name, and a plan_id.
        let summed: u32 = resp.plans.iter().map(|p| u32::try_from(p.actions.len()).unwrap()).sum();
        assert_eq!(resp.total_actions, summed, "total_actions == sum of per-plan actions");
        assert!(resp.total_actions > 0, "confirmed plans should have actions");
        for p in &resp.plans {
            assert_eq!(p.item_name, "lights");
            assert!(!p.plan_id.is_empty());
            assert!(!p.actions.is_empty());
        }
    }

    /// A confirmed folder placeholder must stay on the open-plans surface even
    /// though `classify` has materialized a sub-item for its source group.
    ///
    /// `classify` runs `materialize_sub_items` for EVERY source-group-backed
    /// item, so an ordinary single-type folder ends up with a placeholder plus
    /// one sub-item — the same shape as a real split. The #711 placeholder
    /// dedup in `list_unacknowledged_across_roots` therefore also hid the row
    /// the user actually confirmed, and `list_open_inbox_plans` reads that same
    /// query, so the plan vanished from the review surface and "Review plans"
    /// never rendered (regression from #1038, caught by the Layer-2 journeys
    /// `inbox_ui_catalogue_in_place_zero_moves_byte_identical` and
    /// `inbox_ui_confirm_does_not_move_then_apply_moves_to_shown_destination`).
    #[tokio::test]
    async fn list_open_keeps_confirmed_placeholder_with_materialized_sub_item() {
        let db = test_db().await;
        let (item_id, root_path, _dir) = setup_classified_item(&db).await;

        // Give the placeholder a source group and materialize the single-type
        // sub-item classify would have written for it.
        let root_id = "root-plan-test";
        inbox_repo::upsert_inbox_source_group(
            db.pool(),
            &inbox_repo::UpsertSourceGroup {
                id: "sg-plan-test",
                root_id,
                relative_path: "lights",
                content_signature: Some("sig-abc"),
                format: Some("fits"),
                lane: Some("move"),
                file_count: 1,
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE inbox_items SET source_group_id = 'sg-plan-test' WHERE id = ?")
            .bind(&item_id)
            .execute(db.pool())
            .await
            .unwrap();
        inbox_repo::upsert_inbox_sub_item(
            db.pool(),
            &inbox_repo::UpsertInboxSubItem {
                id: "sub-plan-test",
                root_id,
                relative_path: "lights",
                source_group_id: "sg-plan-test",
                group_key: "type=light",
                group_label: "(root) · light",
                frame_type: Some("light"),
                content_signature: "sig-sub",
                file_count: 1,
                lane: "fits",
                needs_review: false,
            },
        )
        .await
        .unwrap();

        do_confirm(&db, &item_id, &root_path).await;

        let resp = list_open_inbox_plans(db.pool()).await.unwrap();
        let ids: Vec<&str> = resp.plans.iter().map(|p| p.inbox_item_id.as_str()).collect();
        assert!(
            ids.contains(&item_id.as_str()),
            "the confirmed placeholder's plan must stay on the open-plans surface, got {ids:?}"
        );
    }

    /// Spec 058 T028 / FR-024: after confirming a SUB-ITEM of a genuinely split
    /// folder, its plan must be reachable on the plan surface.
    ///
    /// The test above covers the unsplit shape, where the confirmed row is the
    /// placeholder and the `> 1` bound in `exclude_split_placeholder!` is what
    /// keeps it visible. This covers the other side of that bound: with two
    /// distinct sub-item group keys the suppression IS active, the placeholder
    /// is correctly hidden, and the row the user actually confirmed is a
    /// sub-item. FR-001 makes that the only shape there is, so the plan surface
    /// must reach it without depending on the placeholder at all.
    #[tokio::test]
    async fn list_open_reaches_a_confirmed_sub_item_of_a_split_folder() {
        let db = test_db().await;
        let (placeholder_id, root_path, _dir) = setup_classified_item(&db).await;
        let root_id = "root-plan-test";

        inbox_repo::upsert_inbox_source_group(
            db.pool(),
            &inbox_repo::UpsertSourceGroup {
                id: "sg-split",
                root_id,
                relative_path: "lights",
                content_signature: Some("sig-abc"),
                format: Some("fits"),
                lane: Some("move"),
                file_count: 1,
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE inbox_items SET source_group_id = 'sg-split' WHERE id = ?")
            .bind(&placeholder_id)
            .execute(db.pool())
            .await
            .unwrap();

        // Two distinct group keys → the placeholder is suppressed.
        for (id, key, frame_type) in
            [("sub-light", "type=light", "light"), ("sub-flat", "type=flat", "flat")]
        {
            inbox_repo::upsert_inbox_sub_item(
                db.pool(),
                &inbox_repo::UpsertInboxSubItem {
                    id,
                    root_id,
                    relative_path: "lights",
                    source_group_id: "sg-split",
                    group_key: key,
                    group_label: key,
                    frame_type: Some(frame_type),
                    content_signature: "sig-abc",
                    file_count: 1,
                    lane: "fits",
                    needs_review: false,
                },
            )
            .await
            .unwrap();
        }

        // Only the light sub-item is confirmed, so only it needs its own
        // classification + evidence.
        sqlx::query(
            "INSERT INTO inbox_classifications
             (inbox_item_id, result, frame_type, computed_at, content_signature,
              unclassified_file_count)
             VALUES ('sub-light', 'classified', 'light', '2025-01-01T00:00:00Z', 'sig-abc', 0)",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO inbox_classification_evidence
             (id, inbox_item_id, relative_file_path, frame_type, evidence_source, raw_value,
              unclassified, is_master)
             VALUES ('ev-sub-light', 'sub-light', 'img001.fits', 'light', 'imagetyp_header',
                     'Light Frame', 0, 0)",
        )
        .execute(db.pool())
        .await
        .unwrap();

        do_confirm(&db, "sub-light", &root_path).await;

        let resp = list_open_inbox_plans(db.pool()).await.unwrap();
        let ids: Vec<&str> = resp.plans.iter().map(|p| p.inbox_item_id.as_str()).collect();
        assert!(
            ids.contains(&"sub-light"),
            "FR-024: the confirmed sub-item's plan must be reachable on the plan surface, got \
             {ids:?}"
        );
        assert!(
            resp.plans.iter().any(|p| p.inbox_item_id == "sub-light" && !p.actions.is_empty()),
            "FR-024: reaching the plan means reaching its actions, not just its id"
        );
    }

    /// `apply_selected_inbox_plans` applies only the named items and leaves the
    /// others in `plan_open`.
    #[tokio::test]
    async fn apply_selected_applies_only_named_items() {
        let db = test_db().await;
        let bus = make_bus(db.pool());

        let (id_a, root_a) = setup_classified_item_suffixed(&db, "a").await;
        let (id_b, root_b) = setup_classified_item_suffixed(&db, "b").await;
        let plan_a = do_confirm(&db, &id_a, &root_a).await;
        do_confirm(&db, &id_b, &root_b).await;

        // The production sequence: the user's Apply gesture records the approval
        // (`plans.approve`) before the batch apply runs.
        crate::plans::approve_plan(db.pool(), &bus, &plan_a, "user").await.unwrap();

        let resp =
            apply_selected_inbox_plans(db.pool(), &bus, std::slice::from_ref(&id_a)).await.unwrap();

        assert_eq!(resp.results.len(), 1, "only the selected item is processed");
        let r = &resp.results[0];
        assert_eq!(r.inbox_item_id, id_a, "only the named item is in the result set");
        assert!(r.error.is_none(), "selected apply should succeed: {:?}", r.error);

        // The un-selected item is untouched: it still has its open plan visible
        // in the aggregate surface (apply_selected never iterated it).  The
        // inbox item's eventual `resolved` transition for the applied item is
        // owned by the async plan listener, not by apply_selected, so we assert
        // only what this use-case deterministically controls.
        let open = list_open_inbox_plans(db.pool()).await.unwrap();
        let still_open: Vec<&str> = open.plans.iter().map(|p| p.inbox_item_id.as_str()).collect();
        assert!(
            still_open.contains(&id_b.as_str()),
            "un-selected item b should still be plan_open, got {still_open:?}"
        );
    }

    /// `apply_selected_inbox_plans` reports a per-item error (not a hard failure)
    /// for an id that is not in `plan_open` state.
    #[tokio::test]
    async fn apply_selected_reports_per_item_error_for_non_plan_open() {
        let db = test_db().await;
        let bus = make_bus(db.pool());

        // A `classified` item (never confirmed) is not `plan_open`.
        let (item_id, _) = setup_classified_item_suffixed(&db, "x").await;

        let resp = apply_selected_inbox_plans(db.pool(), &bus, std::slice::from_ref(&item_id))
            .await
            .unwrap();

        assert_eq!(resp.results.len(), 1);
        let r = &resp.results[0];
        assert_eq!(r.inbox_item_id, item_id);
        assert!(r.error.is_some(), "non-plan_open id should yield a per-item error");
        assert_eq!(r.error.as_deref(), Some("plan.invalid_state"));
    }

    // ── Constitution II: application is explicit (astro-plan-tykek) ──────────

    /// The production inbox apply must refuse a plan the user never approved.
    ///
    /// `inbox.confirm` leaves the plan at `ready_for_review`. Before the fix this
    /// path minted its own approval, so no inbox apply could ever be refused.
    #[tokio::test]
    async fn apply_inbox_plan_refuses_a_plan_the_user_never_approved() {
        let db = test_db().await;
        let bus = make_bus(db.pool());
        let (item_id, root_path, _dir) = setup_classified_item(&db).await;

        let plan_id = do_confirm(&db, &item_id, &root_path).await;

        let err = apply_inbox_plan(db.pool(), &bus, &item_id)
            .await
            .expect_err("an unapproved plan must not apply");
        assert_eq!(err.code, ErrorCode::PlanApprovalRequired);

        let row = plans_repo::get_plan(db.pool(), &plan_id, false).await.unwrap();
        assert_eq!(
            row.state, "ready_for_review",
            "the refused plan must stay reviewable, not advance to applying"
        );
        assert!(row.approval_token.is_none(), "the refusal must not record an approval");
    }

    /// The same call succeeds once the user's approval is on record, so the
    /// refusal above gates on the approval rather than on the inbox path itself.
    #[tokio::test]
    async fn apply_inbox_plan_applies_after_an_explicit_approval() {
        let db = test_db().await;
        let bus = make_bus(db.pool());
        let (item_id, root_path, _dir) = setup_classified_item(&db).await;

        let plan_id = do_confirm(&db, &item_id, &root_path).await;
        crate::plans::approve_plan(db.pool(), &bus, &plan_id, "user").await.unwrap();

        let resp =
            apply_inbox_plan(db.pool(), &bus, &item_id).await.expect("an approved plan must apply");
        assert_eq!(resp.plan_id, plan_id);
        assert_ne!(
            resp.new_state, "ready_for_review",
            "the approved plan must leave the review state"
        );
    }

    /// The batch path the Inbox UI actually calls inherits the same refusal.
    #[tokio::test]
    async fn apply_selected_refuses_a_plan_the_user_never_approved() {
        let db = test_db().await;
        let bus = make_bus(db.pool());
        let (item_id, root_path) = setup_classified_item_suffixed(&db, "unapproved").await;
        do_confirm(&db, &item_id, &root_path).await;

        let resp = apply_selected_inbox_plans(db.pool(), &bus, std::slice::from_ref(&item_id))
            .await
            .unwrap();

        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].error.as_deref(), Some("plan.approval_required"));
    }
}
