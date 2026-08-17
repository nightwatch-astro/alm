// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `inbox_plan_links` rows: the "open plan" invariant tying an inbox item
//! to its reviewable filesystem plan (Constitution II).

use domain_core::ids::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use persistence_core::DbResult;

/// Flat row from `inbox_plan_links`.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InboxPlanLinkRow {
    pub inbox_item_id: String,
    pub plan_id: String,
    pub linked_at: String,
}

// ── SourceGroup CRUD ──────────────────────────────────────────────────────────

/// Insert a plan link, establishing the "open plan" invariant.
///
/// Fails with `persistence_core::DbError::Database` if a link already exists (PK conflict).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on constraint or connection failure.
pub async fn insert_plan_link(
    pool: &SqlitePool,
    inbox_item_id: &str,
    plan_id: &str,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO inbox_plan_links (inbox_item_id, plan_id, linked_at)
         VALUES (?, ?, ?)",
    )
    .bind(inbox_item_id)
    .bind(plan_id)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch the plan link for an item, if any.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn get_plan_link(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> DbResult<Option<InboxPlanLinkRow>> {
    Ok(sqlx::query_as::<_, InboxPlanLinkRow>(
        "SELECT * FROM inbox_plan_links WHERE inbox_item_id = ?",
    )
    .bind(inbox_item_id)
    .fetch_optional(pool)
    .await?)
}

/// Delete the plan link for an item (called when a plan closes).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn delete_plan_link(pool: &SqlitePool, inbox_item_id: &str) -> DbResult<()> {
    sqlx::query("DELETE FROM inbox_plan_links WHERE inbox_item_id = ?")
        .bind(inbox_item_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Fetch the plan link row by plan ID (used by the plan listener).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn get_plan_link_by_plan_id(
    pool: &SqlitePool,
    plan_id: &str,
) -> DbResult<Option<InboxPlanLinkRow>> {
    Ok(sqlx::query_as::<_, InboxPlanLinkRow>("SELECT * FROM inbox_plan_links WHERE plan_id = ?")
        .bind(plan_id)
        .fetch_optional(pool)
        .await?)
}

/// Find all inbox item IDs whose linked plan is in a terminal state.
///
/// Used by the background repair query. (Ref: R-PlanOpen)
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on connection failure.
pub async fn find_orphaned_plan_links(
    pool: &SqlitePool,
) -> DbResult<Vec<(String, String, String)>> {
    // Returns (inbox_item_id, plan_id, plan_state)
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT l.inbox_item_id, l.plan_id, p.state
         FROM inbox_plan_links l
         JOIN plans p ON p.id = l.plan_id
         WHERE p.state IN ('applied','partially_applied','failed','cancelled','discarded')",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── Cross-root unacknowledged listing ────────────────────────────────────────
