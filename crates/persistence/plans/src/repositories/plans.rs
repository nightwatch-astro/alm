// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for plan storage (spec 017).
//!
//! Operates on the `plans` and `plan_items` tables from migration 0014.
//! Paths are stored as (root_id, relative_path) pairs; callers resolve to
//! absolute paths for display. This module owns only raw DB operations;
//! state-machine enforcement lives in `crates/app/core/src/plans.rs`.

use domain_core::ids::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

// ── Helpers ───────────────────────────────────────────────────────────────────

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat row returned from the `plans` table for list and get operations.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PlanRow {
    pub id: String,
    pub number: i64,
    pub title: String,
    pub origin: String,
    pub origin_path: Option<String>,
    pub state: String,
    pub plan_type: String,
    pub destructive_destination: String,
    pub parent_plan_id: Option<String>,
    pub items_total: i64,
    pub items_applied: i64,
    pub items_failed: i64,
    pub items_skipped: i64,
    pub items_cancelled: i64,
    pub items_pending: i64,
    pub total_bytes_required: i64,
    pub approval_token: Option<String>,
    pub approved_at: Option<String>,
    pub discarded_at: Option<String>,
    pub created_at: String,
    /// Absolute root every item destination in this plan must resolve under
    /// (migration 0003). `None` leaves the executor's destination gate
    /// inactive, which is the behaviour for every plan type that does not set
    /// it.
    pub destination_root: Option<String>,
}

/// Flat row returned from the `plan_items` table.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PlanItemRow {
    pub id: String,
    pub plan_id: String,
    pub item_index: i64,
    pub name: String,
    pub action: String,
    pub from_root_id: Option<String>,
    pub from_relative_path: String,
    pub to_root_id: Option<String>,
    pub to_relative_path: String,
    pub reason: String,
    pub protection: String,
    pub linked_entity: Option<String>,
    pub item_state: String,
    pub failure_reason: Option<String>,
    pub provenance: Option<String>,
    pub approved_mtime: Option<String>,
    pub approved_size_bytes: Option<i64>,
    pub archive_path: Option<String>,
    pub created_at: String,
    // Fields added by migration 0031.
    pub source_id: Option<String>,
    pub category: Option<String>,
    /// 0/1 bool: derived from action type (delete/trash). Added by migration 0031.
    pub requires_destructive_confirm: Option<i64>,
    pub resolved_pattern: Option<String>,
    /// 0/1 bool: whether the user has confirmed the destructive action.
    /// Real DB column added by migration 0033. Default 0 (false) — safe.
    pub destructive_confirmed: i64,
}

/// Data required to insert a new plan.
#[derive(Clone, Debug)]
pub struct InsertPlan<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub origin: &'a str,
    pub origin_path: Option<&'a str>,
    pub plan_type: &'a str,
    pub destructive_destination: &'a str,
    pub parent_plan_id: Option<&'a str>,
    pub total_bytes_required: i64,
}

/// Data required to insert a new plan item.
#[derive(Clone, Debug)]
pub struct InsertPlanItem<'a> {
    pub id: &'a str,
    pub plan_id: &'a str,
    pub item_index: i64,
    pub name: &'a str,
    pub action: &'a str,
    pub from_root_id: Option<&'a str>,
    pub from_relative_path: &'a str,
    pub to_root_id: Option<&'a str>,
    pub to_relative_path: &'a str,
    pub reason: &'a str,
    pub protection: &'a str,
    pub linked_entity: Option<&'a str>,
    pub provenance_json: Option<&'a str>,
    pub archive_path: Option<&'a str>,
    /// Real source FK (FR-016/017, migration 0031).
    /// Set by plan generators so `plan_protection_check` can surface the real source.
    pub source_id: Option<&'a str>,
    /// Classification category used by protection resolution (FR-016, migration 0031).
    pub category: Option<&'a str>,
}

// ── Plan CRUD ─────────────────────────────────────────────────────────────────

/// Insert a new plan in `draft` state.
///
/// The display number is assigned as `MAX(number)+1` atomically.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn insert_plan(pool: &SqlitePool, plan: &InsertPlan<'_>) -> DbResult<i64> {
    let mut conn = pool.acquire().await?;
    insert_plan_conn(&mut conn, plan).await
}

/// Connection-level variant of [`insert_plan`]: takes `&mut SqliteConnection`
/// (works against a plain connection or a `Transaction` deref) so composite
/// `*_tx` functions elsewhere in this crate (e.g.
/// `repositories::projects::create_project_tx`) can compose it with other
/// writes in one transaction.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub(crate) async fn insert_plan_conn(
    conn: &mut SqliteConnection,
    plan: &InsertPlan<'_>,
) -> DbResult<i64> {
    let now = Timestamp::now_iso();
    let number: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(number), 0) + 1 FROM plans")
        .fetch_one(&mut *conn)
        .await?;

    sqlx::query(
        "INSERT INTO plans (
            id, number, title, origin, origin_path, state, plan_type,
            destructive_destination, parent_plan_id, items_total, items_applied,
            items_failed, items_skipped, items_cancelled, items_pending,
            total_bytes_required, created_at
        ) VALUES (?, ?, ?, ?, ?, 'draft', ?, ?, ?, 0, 0, 0, 0, 0, 0, ?, ?)",
    )
    .bind(plan.id)
    .bind(number)
    .bind(plan.title)
    .bind(plan.origin)
    .bind(plan.origin_path)
    .bind(plan.plan_type)
    .bind(plan.destructive_destination)
    .bind(plan.parent_plan_id)
    .bind(plan.total_bytes_required)
    .bind(&now)
    .execute(&mut *conn)
    .await?;

    Ok(number)
}

/// Insert a single plan item.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub async fn insert_plan_item(pool: &SqlitePool, item: &InsertPlanItem<'_>) -> DbResult<()> {
    let mut conn = pool.acquire().await?;
    insert_plan_item_conn(&mut conn, item).await
}

/// Connection-level variant of [`insert_plan_item`]. See [`insert_plan_conn`].
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on constraint or connection failure.
pub(crate) async fn insert_plan_item_conn(
    conn: &mut SqliteConnection,
    item: &InsertPlanItem<'_>,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO plan_items (
            id, plan_id, item_index, name, action,
            from_root_id, from_relative_path, to_root_id, to_relative_path,
            reason, protection, linked_entity, item_state, provenance,
            archive_path, source_id, category, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?)",
    )
    .bind(item.id)
    .bind(item.plan_id)
    .bind(item.item_index)
    .bind(item.name)
    .bind(item.action)
    .bind(item.from_root_id)
    .bind(item.from_relative_path)
    .bind(item.to_root_id)
    .bind(item.to_relative_path)
    .bind(item.reason)
    .bind(item.protection)
    .bind(item.linked_entity)
    .bind(item.provenance_json)
    .bind(item.archive_path)
    .bind(item.source_id)
    .bind(item.category)
    .bind(&now)
    .execute(&mut *conn)
    .await?;

    // Update items_total and items_pending counters on the parent plan.
    sqlx::query(
        "UPDATE plans SET items_total = items_total + 1, items_pending = items_pending + 1 \
         WHERE id = ?",
    )
    .bind(item.plan_id)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// Fetch a single plan by id (excludes discarded by default; pass `include_discarded = true`
/// to load retry-chain parents).
///
/// # Errors
///
/// Returns [`persistence_core::DbError::NotFound`] if no matching row exists.
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn get_plan(
    pool: &SqlitePool,
    plan_id: &str,
    include_discarded: bool,
) -> DbResult<PlanRow> {
    let row: Option<PlanRow> = if include_discarded {
        sqlx::query_as("SELECT * FROM plans WHERE id = ?")
            .bind(plan_id)
            .fetch_optional(pool)
            .await?
    } else {
        sqlx::query_as("SELECT * FROM plans WHERE id = ? AND state != 'discarded'")
            .bind(plan_id)
            .fetch_optional(pool)
            .await?
    };

    row.ok_or_else(|| DbError::NotFound(format!("plan {plan_id}")))
}

/// List plans ordered by failed-first, then descending creation time.
///
/// Discarded plans are excluded unless `state_filter` contains `"discarded"`.
/// `created_after` is an optional ISO-8601 timestamp cutoff (R-Ret-1).
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn list_plans(
    pool: &SqlitePool,
    state_filter: &[String],
    origin_filter: &[String],
    created_after: Option<&str>,
    limit: i64,
) -> DbResult<Vec<PlanRow>> {
    // SQLite does not support array binding, so the IN clauses are built from
    // the filter lengths only. No caller bytes reach the SQL text; every filter
    // value is bound below, in the order the placeholders appear.
    let placeholders = |n: usize| -> String { vec!["?"; n].join(",") };

    let state_clause = if state_filter.is_empty() {
        // Default: all non-discarded states.
        "state != 'discarded'".to_owned()
    } else {
        format!("state IN ({})", placeholders(state_filter.len()))
    };

    let origin_clause = if origin_filter.is_empty() {
        String::new()
    } else {
        format!("AND origin IN ({})", placeholders(origin_filter.len()))
    };

    let date_clause = if created_after.is_some() { "AND created_at >= ?" } else { "" };

    // failed-first ordering: failed/partially_applied first, then by created_at desc.
    let sql = format!(
        "SELECT * FROM plans \
         WHERE {state_clause} {origin_clause} {date_clause} \
         ORDER BY \
           CASE state WHEN 'failed' THEN 0 WHEN 'partially_applied' THEN 1 ELSE 2 END ASC, \
           created_at DESC \
         LIMIT ?"
    );

    let mut q = sqlx::query_as::<_, PlanRow>(sqlx::AssertSqlSafe(&*sql));
    for state in state_filter {
        q = q.bind(state);
    }
    for origin in origin_filter {
        q = q.bind(origin);
    }
    if let Some(after) = created_after {
        q = q.bind(after);
    }
    q = q.bind(limit);

    Ok(q.fetch_all(pool).await?)
}

/// Fetch all items for a plan, ordered by index.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn list_plan_items(pool: &SqlitePool, plan_id: &str) -> DbResult<Vec<PlanItemRow>> {
    Ok(sqlx::query_as("SELECT * FROM plan_items WHERE plan_id = ? ORDER BY item_index ASC")
        .bind(plan_id)
        .fetch_all(pool)
        .await?)
}

/// Set `destructive_confirmed = 1` on every item in `plan_id` that requires
/// destructive confirmation: `action IN ('delete','trash')`, an
/// `action = 'archive'` item whose plan carries
/// `destructive_destination = 'trash'` (that reroute makes it a real OS-trash
/// removal), or the explicit `requires_destructive_confirm` override column.
/// Mirrors the derivation in
/// `app_core::plan_apply::item_row_to_executor_item`, and MUST stay at least as
/// wide as it: a narrower writer leaves items the executor refuses with
/// `destructive_unconfirmed` even after the user confirmed the plan.
///
/// This is the write half of the FR-003/D9 confirm gate: the executor
/// (`fs_executor::run::execute_plan`) refuses any destructive item where
/// `destructive_confirmed` is still 0 (issue #741 — the column previously had
/// no writer anywhere in the codebase). Idempotent: re-confirming an
/// already-confirmed plan is a no-op.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn confirm_plan_destructive_items(pool: &SqlitePool, plan_id: &str) -> DbResult<u64> {
    let result = sqlx::query(
        "UPDATE plan_items SET destructive_confirmed = 1 \
         WHERE plan_id = ? AND destructive_confirmed = 0 \
         AND (action IN ('delete', 'trash') OR requires_destructive_confirm = 1 \
              OR (action = 'archive' AND EXISTS ( \
                  SELECT 1 FROM plans p \
                  WHERE p.id = plan_items.plan_id \
                  AND p.destructive_destination = 'trash')))",
    )
    .bind(plan_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Record the absolute destination root every item in `plan_id` must resolve
/// under (migration 0003).
///
/// # Errors
///
/// Returns [`persistence_core::DbError::NotFound`] if no plan with `plan_id`
/// exists, or [`persistence_core::DbError::Database`] on connection failure.
pub async fn set_destination_root(
    pool: &SqlitePool,
    plan_id: &str,
    destination_root: &str,
) -> DbResult<()> {
    let rows = sqlx::query("UPDATE plans SET destination_root = ? WHERE id = ?")
        .bind(destination_root)
        .bind(plan_id)
        .execute(pool)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(persistence_core::DbError::NotFound(format!("plan {plan_id}")));
    }
    Ok(())
}

/// Update the plan state.
///
/// Only the review-side states are written by this function:
/// `draft`, `ready_for_review`, `approved`, `discarded`.
/// Apply-side state transitions belong to spec 025.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::NotFound`] if no plan with `plan_id` exists.
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn update_plan_state(pool: &SqlitePool, plan_id: &str, state: &str) -> DbResult<()> {
    let mut conn = pool.acquire().await?;
    update_plan_state_conn(&mut conn, plan_id, state).await
}

/// Connection-level variant of [`update_plan_state`]. See [`insert_plan_conn`].
///
/// # Errors
///
/// Returns [`persistence_core::DbError::NotFound`] if no plan with `plan_id` exists.
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub(crate) async fn update_plan_state_conn(
    conn: &mut SqliteConnection,
    plan_id: &str,
    state: &str,
) -> DbResult<()> {
    let rows = sqlx::query("UPDATE plans SET state = ? WHERE id = ?")
        .bind(state)
        .bind(plan_id)
        .execute(&mut *conn)
        .await?;

    if rows.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("plan {plan_id}")));
    }
    Ok(())
}

/// Approve a plan: `ready_for_review → approved`, recording `approved_at` and
/// `approval_token`.
///
/// The expected state is in the `WHERE` clause, so this loses rather than
/// overwrites when it races another transition. Without that predicate a
/// concurrent [`set_reopened`] could be undone by an approval that had already
/// read `ready_for_review`, leaving the plan `approved` with a live token while
/// the reopen returned success.
///
/// # Errors
///
/// - [`persistence_core::DbError::NotFound`] when no row matches `plan_id`.
/// - [`persistence_core::DbError::CasFailed`] when the row exists in a state
///   other than `ready_for_review`.
/// - [`persistence_core::DbError::Database`] on connection failure.
pub async fn set_approved(
    pool: &SqlitePool,
    plan_id: &str,
    approved_at: &str,
    approval_token: &str,
) -> DbResult<()> {
    let rows = sqlx::query(
        "UPDATE plans SET state = 'approved', approved_at = ?, approval_token = ? \
         WHERE id = ? AND state = 'ready_for_review'",
    )
    .bind(approved_at)
    .bind(approval_token)
    .bind(plan_id)
    .execute(pool)
    .await?;

    if rows.rows_affected() == 0 {
        return Err(state_cas_error(pool, plan_id, "'ready_for_review'").await?);
    }
    Ok(())
}

/// The error a lost state CAS on `plans` should return.
///
/// A guarded `UPDATE` affecting zero rows does not say why. Re-reading the state
/// separates a missing plan from one in a state the transition does not accept:
/// the first is [`persistence_core::DbError::NotFound`], the second
/// [`persistence_core::DbError::CasFailed`], which the app layer maps to
/// `plan.invalid_state`. Reporting both as `NotFound` told the caller a plan it
/// had just read no longer existed.
///
/// `expected` is quoted by the caller, so a transition accepting more than one
/// state can name all of them.
async fn state_cas_error(pool: &SqlitePool, plan_id: &str, expected: &str) -> DbResult<DbError> {
    let found: Option<String> = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
        .bind(plan_id)
        .fetch_optional(pool)
        .await?;
    Ok(match found {
        None => DbError::NotFound(format!("plan {plan_id}")),
        Some(state) => {
            DbError::CasFailed(format!("plan {plan_id} expected state {expected}, found '{state}'"))
        }
    })
}

/// Return a plan to `draft` and clear its approval (spec 017 T023).
///
/// `approval_token` and `approved_at` are nulled in the same statement as the
/// state change: `verify_approval_token` reads a null stored token as
/// `plan.approval.stale`, so a token issued before the reopen cannot authorise
/// an apply of the edited plan. The state predicate is in the `WHERE` clause so
/// a concurrent `applying` transition cannot be overwritten.
///
/// # Errors
///
/// - [`persistence_core::DbError::NotFound`] when no row matches `plan_id`.
/// - [`persistence_core::DbError::CasFailed`] when the row exists in a state
///   with no `→ draft` edge, which the app layer maps to `plan.invalid_state`.
/// - [`persistence_core::DbError::Database`] on connection failure.
pub async fn set_reopened(pool: &SqlitePool, plan_id: &str) -> DbResult<()> {
    let rows = sqlx::query(
        "UPDATE plans SET state = 'draft', approved_at = NULL, approval_token = NULL \
         WHERE id = ? AND state IN ('approved', 'ready_for_review')",
    )
    .bind(plan_id)
    .execute(pool)
    .await?;

    if rows.rows_affected() == 0 {
        return Err(state_cas_error(pool, plan_id, "'approved' or 'ready_for_review'").await?);
    }
    Ok(())
}

/// Set the attribution pick's target framing on a plan (spec 008 Q27,
/// F-Framing-10, migration 0068). Read back at plan-apply completion
/// (`app_core_targets::ingest_sessions`) once the plan's light frames are
/// folded into a real `acquisition_session` — that is the earliest point a
/// session id exists to add as a framing member.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::NotFound`] when no plan carries
/// `plan_id`, and [`persistence_core::DbError::Database`] on connection failure.
pub async fn set_chosen_framing_id(
    pool: &SqlitePool,
    plan_id: &str,
    framing_id: &str,
) -> DbResult<()> {
    let rows = sqlx::query("UPDATE plans SET chosen_framing_id = ? WHERE id = ?")
        .bind(framing_id)
        .bind(plan_id)
        .execute(pool)
        .await?;

    if rows.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("plan {plan_id}")));
    }
    Ok(())
}

/// Read the attribution pick's target framing for a plan, if any (F-Framing-10).
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn get_chosen_framing_id(pool: &SqlitePool, plan_id: &str) -> DbResult<Option<String>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT chosen_framing_id FROM plans WHERE id = ?")
            .bind(plan_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(v,)| v))
}

/// Set `discarded_at` and transition state to `discarded` (soft-delete, A5).
///
/// Row is retained; `parent_plan_id` references remain resolvable.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::NotFound`] if no matching plan exists.
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn soft_delete_plan(
    pool: &SqlitePool,
    plan_id: &str,
    discarded_at: &str,
) -> DbResult<()> {
    let rows = sqlx::query("UPDATE plans SET state = 'discarded', discarded_at = ? WHERE id = ?")
        .bind(discarded_at)
        .bind(plan_id)
        .execute(pool)
        .await?;

    if rows.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("plan {plan_id}")));
    }
    Ok(())
}

/// Update per-item FS snapshot fields (R-FS-1).
///
/// # Errors
///
/// Returns [`persistence_core::DbError::Database`] on connection failure.
pub async fn update_item_fs_snapshot(
    pool: &SqlitePool,
    item_id: &str,
    approved_mtime: Option<&str>,
    approved_size_bytes: Option<i64>,
) -> DbResult<()> {
    sqlx::query("UPDATE plan_items SET approved_mtime = ?, approved_size_bytes = ? WHERE id = ?")
        .bind(approved_mtime)
        .bind(approved_size_bytes)
        .bind(item_id)
        .execute(pool)
        .await?;

    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::setup_db;

    fn sample_plan(id: &str) -> InsertPlan<'_> {
        InsertPlan {
            id,
            title: "Test plan",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        }
    }

    #[tokio::test]
    async fn insert_and_get_plan_roundtrip() {
        let db = setup_db().await;
        let number = insert_plan(db.pool(), &sample_plan("plan-1")).await.unwrap();
        assert_eq!(number, 1);

        let row = get_plan(db.pool(), "plan-1", false).await.unwrap();
        assert_eq!(row.id, "plan-1");
        assert_eq!(row.state, "draft");
        assert_eq!(row.origin, "cleanup");
    }

    // ── chosen_framing_id (F-Framing-10) ────────────────────────────────────

    #[tokio::test]
    async fn set_chosen_framing_id_reports_a_plan_that_is_gone() {
        let db = setup_db().await;

        let err = set_chosen_framing_id(db.pool(), "plan-gone", "framing-1")
            .await
            .expect_err("an attribution write that matches no plan must not report success");

        assert!(
            matches!(err, DbError::NotFound(ref m) if m.contains("plan-gone")),
            "error must name the missing plan: {err}"
        );
    }

    #[tokio::test]
    async fn chosen_framing_id_defaults_to_none_and_round_trips() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("plan-attr")).await.unwrap();
        assert_eq!(get_chosen_framing_id(db.pool(), "plan-attr").await.unwrap(), None);

        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, notes, channel_drift, is_mosaic, created_at, updated_at) \
             VALUES ('proj-attr', 'P', 'PixInsight', 'ready', 'projects/proj-attr', NULL, 0, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO framing (id, project_id, optic_train_key, pointing_ra_deg, pointing_dec_deg, \
             rotation_deg, tolerance_pointing, tolerance_rotation_deg, clustering, created_at, updated_at) \
             VALUES ('framing-attr', 'proj-attr', 'scope-a|cam-a', 10.0, 20.0, 0.0, 0.1, 3.0, 'suggested', \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        set_chosen_framing_id(db.pool(), "plan-attr", "framing-attr").await.unwrap();
        assert_eq!(
            get_chosen_framing_id(db.pool(), "plan-attr").await.unwrap().as_deref(),
            Some("framing-attr")
        );
    }

    #[tokio::test]
    async fn display_numbers_increment() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("plan-a")).await.unwrap();
        insert_plan(db.pool(), &sample_plan("plan-b")).await.unwrap();
        let row_b = get_plan(db.pool(), "plan-b", false).await.unwrap();
        assert_eq!(row_b.number, 2);
    }

    #[tokio::test]
    async fn get_plan_not_found_returns_error() {
        let db = setup_db().await;
        let err = get_plan(db.pool(), "nonexistent", false).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn soft_delete_sets_discarded_state() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("plan-x")).await.unwrap();
        soft_delete_plan(db.pool(), "plan-x", "2026-06-01T00:00:00Z").await.unwrap();

        // Non-discarded query returns NotFound.
        let err = get_plan(db.pool(), "plan-x", false).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));

        // include_discarded query returns the row.
        let row = get_plan(db.pool(), "plan-x", true).await.unwrap();
        assert_eq!(row.state, "discarded");
        assert_eq!(row.discarded_at, Some("2026-06-01T00:00:00Z".to_owned()));
    }

    #[tokio::test]
    async fn list_plans_excludes_discarded_by_default() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p1")).await.unwrap();
        insert_plan(db.pool(), &sample_plan("p2")).await.unwrap();
        soft_delete_plan(db.pool(), "p2", "2026-06-01T00:00:00Z").await.unwrap();

        let rows = list_plans(db.pool(), &[], &[], None, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "p1");
    }

    #[tokio::test]
    async fn list_plans_with_state_filter() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p1")).await.unwrap();
        insert_plan(db.pool(), &sample_plan("p2")).await.unwrap();
        // Update p2 to ready_for_review.
        update_plan_state(db.pool(), "p2", "ready_for_review").await.unwrap();

        let rows = list_plans(db.pool(), &["draft".to_owned()], &[], None, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "p1");
    }

    /// State, origin, and `created_after` placeholders appear in that order in
    /// the SQL text; a bind order that disagrees silently returns wrong rows.
    #[tokio::test]
    async fn list_plans_combines_state_origin_and_created_after_filters() {
        let db = setup_db().await;
        for (id, origin) in [("p-keep", "cleanup"), ("p-origin", "archive"), ("p-state", "cleanup")]
        {
            let mut plan = sample_plan(id);
            plan.origin = origin;
            plan.plan_type = origin;
            insert_plan(db.pool(), &plan).await.unwrap();
        }
        update_plan_state(db.pool(), "p-state", "ready_for_review").await.unwrap();

        let rows = list_plans(
            db.pool(),
            &["draft".to_owned()],
            &["cleanup".to_owned()],
            Some("2000-01-01T00:00:00Z"),
            100,
        )
        .await
        .unwrap();

        assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["p-keep"]);
    }

    /// Filter values are bound, so SQL metacharacters are matched as data and
    /// cannot extend the WHERE clause past the non-discarded default.
    #[tokio::test]
    async fn list_plans_filter_values_cannot_inject_sql() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p1")).await.unwrap();
        insert_plan(db.pool(), &sample_plan("p2")).await.unwrap();
        soft_delete_plan(db.pool(), "p2", "2026-06-01T00:00:00Z").await.unwrap();

        let rows = list_plans(db.pool(), &["draft') OR 1=1 --".to_owned()], &[], None, 100)
            .await
            .expect("a hostile filter value is data, not SQL");

        assert!(rows.is_empty(), "no plan has that literal state");
    }

    #[tokio::test]
    async fn insert_plan_item_updates_counters() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p1")).await.unwrap();

        let item = InsertPlanItem {
            id: "item-1",
            plan_id: "p1",
            item_index: 1,
            name: "file.fits",
            action: "move",
            from_root_id: None,
            from_relative_path: "raw/file.fits",
            to_root_id: None,
            to_relative_path: "archive/file.fits",
            reason: "cleanup",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        };
        insert_plan_item(db.pool(), &item).await.unwrap();

        let plan = get_plan(db.pool(), "p1", false).await.unwrap();
        assert_eq!(plan.items_total, 1);
        assert_eq!(plan.items_pending, 1);
    }

    #[tokio::test]
    async fn confirm_plan_destructive_items_confirms_only_destructive_actions() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p-confirm")).await.unwrap();

        let move_item = InsertPlanItem {
            id: "item-move",
            plan_id: "p-confirm",
            item_index: 1,
            name: "keep.fits",
            action: "move",
            from_root_id: None,
            from_relative_path: "raw/keep.fits",
            to_root_id: None,
            to_relative_path: "archive/keep.fits",
            reason: "cleanup",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        };
        // `plan_items.action` CHECK never admits the literal 'trash' — the
        // archive/trash *destination* choice lives on `plans.destructive_destination`;
        // item-level destructive intent is always the 'delete' action (mirrors
        // `cleanup_generator::action_label` and `item_row_to_executor_item`).
        let delete_item = InsertPlanItem {
            id: "item-delete",
            plan_id: "p-confirm",
            item_index: 2,
            name: "junk.fits",
            action: "delete",
            from_root_id: None,
            from_relative_path: "raw/junk.fits",
            to_root_id: None,
            to_relative_path: "",
            reason: "cleanup",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        };
        insert_plan_item(db.pool(), &move_item).await.unwrap();
        insert_plan_item(db.pool(), &delete_item).await.unwrap();

        let confirmed = confirm_plan_destructive_items(db.pool(), "p-confirm").await.unwrap();
        assert_eq!(confirmed, 1, "only the delete item requires confirmation");

        let items = list_plan_items(db.pool(), "p-confirm").await.unwrap();
        let move_row = items.iter().find(|i| i.id == "item-move").unwrap();
        let delete_row = items.iter().find(|i| i.id == "item-delete").unwrap();
        assert_eq!(move_row.destructive_confirmed, 0);
        assert_eq!(delete_row.destructive_confirmed, 1);

        // Idempotent: a second confirm on an already-confirmed plan touches nothing.
        let confirmed_again = confirm_plan_destructive_items(db.pool(), "p-confirm").await.unwrap();
        assert_eq!(confirmed_again, 0);
    }

    // ── State CAS on approval and reopen ────────────────────────────────────
    //
    // Both statements are guarded `UPDATE`s, so the interesting cases are the
    // ones that affect zero rows: a lost race must be distinguishable from a
    // plan that is not there.

    #[tokio::test]
    async fn approval_requires_ready_for_review() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p-cas")).await.unwrap();

        // Still a draft: approval must not force the state.
        let err =
            set_approved(db.pool(), "p-cas", "2026-08-01T00:00:00Z", "tok-1").await.unwrap_err();
        assert!(matches!(err, DbError::CasFailed(_)), "got {err:?}");
        let row = get_plan(db.pool(), "p-cas", false).await.unwrap();
        assert_eq!(row.state, "draft");
        assert_eq!(row.approval_token, None);

        update_plan_state(db.pool(), "p-cas", "ready_for_review").await.unwrap();
        set_approved(db.pool(), "p-cas", "2026-08-01T00:00:00Z", "tok-1").await.unwrap();
        let row = get_plan(db.pool(), "p-cas", false).await.unwrap();
        assert_eq!(row.state, "approved");
        assert_eq!(row.approval_token.as_deref(), Some("tok-1"));
    }

    #[tokio::test]
    async fn approval_of_a_reopened_plan_loses() {
        // The concurrency case from the review: approval read `ready_for_review`,
        // a reopen landed first, and the approval must not resurrect the token.
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p-race")).await.unwrap();
        update_plan_state(db.pool(), "p-race", "ready_for_review").await.unwrap();
        set_reopened(db.pool(), "p-race").await.unwrap();

        let err = set_approved(db.pool(), "p-race", "2026-08-01T00:00:00Z", "tok-race")
            .await
            .unwrap_err();
        assert!(matches!(err, DbError::CasFailed(_)), "got {err:?}");
        let row = get_plan(db.pool(), "p-race", false).await.unwrap();
        assert_eq!(row.state, "draft");
        assert_eq!(row.approval_token, None);
    }

    #[tokio::test]
    async fn approval_of_a_missing_plan_is_not_found() {
        let db = setup_db().await;
        let err =
            set_approved(db.pool(), "ghost", "2026-08-01T00:00:00Z", "tok").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn reopen_clears_the_approval() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p-reopen")).await.unwrap();
        update_plan_state(db.pool(), "p-reopen", "ready_for_review").await.unwrap();
        set_approved(db.pool(), "p-reopen", "2026-08-01T00:00:00Z", "tok-r").await.unwrap();

        set_reopened(db.pool(), "p-reopen").await.unwrap();
        let row = get_plan(db.pool(), "p-reopen", false).await.unwrap();
        assert_eq!(row.state, "draft");
        assert_eq!(row.approval_token, None);
        assert_eq!(row.approved_at, None);
    }

    #[tokio::test]
    async fn reopen_of_an_applying_plan_reports_invalid_state_not_missing() {
        // `applying` has no `→ draft` edge. Reporting this as NotFound told the
        // caller a plan it had just read had disappeared.
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p-applying")).await.unwrap();
        update_plan_state(db.pool(), "p-applying", "applying").await.unwrap();

        let err = set_reopened(db.pool(), "p-applying").await.unwrap_err();
        assert!(matches!(err, DbError::CasFailed(_)), "got {err:?}");
        assert!(format!("{err}").contains("found 'applying'"), "{err}");
    }

    #[tokio::test]
    async fn reopen_of_a_missing_plan_is_not_found() {
        let db = setup_db().await;
        let err = set_reopened(db.pool(), "ghost").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn failed_first_ordering() {
        let db = setup_db().await;
        insert_plan(db.pool(), &sample_plan("p-draft")).await.unwrap();
        insert_plan(db.pool(), &sample_plan("p-failed")).await.unwrap();
        update_plan_state(db.pool(), "p-failed", "failed").await.unwrap();

        let rows =
            list_plans(db.pool(), &["draft".to_owned(), "failed".to_owned()], &[], None, 100)
                .await
                .unwrap();
        assert_eq!(rows[0].id, "p-failed", "failed plan should be first");
        assert_eq!(rows[1].id, "p-draft");
    }
}
