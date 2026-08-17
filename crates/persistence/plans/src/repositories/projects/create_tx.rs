// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Composite atomic create (T2-a): project row + initial source links +
//! inferred channels + the Constitution II folder-structure plan, all in one
//! transaction.

use sqlx::SqlitePool;

use crate::repositories::plans::{self, InsertPlan, InsertPlanItem};
use persistence_core::DbResult;

use super::channels::insert_project_channel_conn;
use super::crud::insert_project_conn;
use super::sources::insert_project_source_conn;
use super::{InsertProject, InsertProjectSource};

/// Input for [`create_project_tx`]: the project row, its initial source
/// links, inferred channels, and the Constitution II folder-structure plan —
/// everything `app_projects::project_setup::create` must persist as one
/// atomic unit.
///
/// Folder-plan item resolution (`project_structure::required_folders`) stays
/// in the app layer; this struct only carries the already-resolved plan rows,
/// so `persistence_db` does not gain a dependency on `project_structure`.
pub struct CreateProjectInput<'a> {
    pub project: InsertProject<'a>,
    pub sources: &'a [InsertProjectSource<'a>],
    /// `(label, source)` pairs, already inferred by
    /// `domain_core::project::channels` (persistence does not infer).
    pub channels: &'a [(&'a str, &'a str)],
    /// Timestamp shared by every channel row's `added_at`.
    pub channels_added_at: &'a str,
    pub plan: InsertPlan<'a>,
    pub plan_items: &'a [InsertPlanItem<'a>],
}

/// Atomically create a project: the project row, its initial source links,
/// inferred channels, and the Constitution II folder-structure plan (+ items,
/// advanced to `ready_for_review`) — all in one transaction. A mid-sequence
/// failure rolls back everything; no half-built project is left behind (see
/// `docs/development/duplication-and-abstraction-audit.md` T2-a).
///
/// Composed via a direct `pool.begin()`/`tx.commit()`, matching every other
/// multi-statement transaction in this crate (e.g. `plan_apply.rs`,
/// [`super::replace_project_channels`]).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violation or query
/// failure. On error the transaction is rolled back and no rows from this
/// call persist.
pub async fn create_project_tx(pool: &SqlitePool, input: &CreateProjectInput<'_>) -> DbResult<()> {
    let mut tx = pool.begin().await?;

    insert_project_conn(&mut tx, &input.project).await?;

    for source in input.sources {
        insert_project_source_conn(&mut tx, source).await?;
    }

    for (label, source) in input.channels {
        insert_project_channel_conn(
            &mut tx,
            input.project.id,
            label,
            source,
            input.channels_added_at,
        )
        .await?;
    }

    plans::insert_plan_conn(&mut tx, &input.plan).await?;
    for item in input.plan_items {
        plans::insert_plan_item_conn(&mut tx, item).await?;
    }
    plans::update_plan_state_conn(&mut tx, input.plan.id, "ready_for_review").await?;

    tx.commit().await?;
    Ok(())
}
