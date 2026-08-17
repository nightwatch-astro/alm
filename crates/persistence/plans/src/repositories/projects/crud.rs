// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `projects` table CRUD.

use domain_core::ids::Timestamp;
use domain_core::lifecycle::project::{is_allowed, ProjectState};
use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::{DbError, DbResult};

use super::{InsertProject, ProjectRow, ProjectRowTuple};

/// Insert a new project row. Returns `DbError::Database` (UNIQUE violation) when
/// the name or path is already taken.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violation or query failure.
pub async fn insert_project(pool: &SqlitePool, data: &InsertProject<'_>) -> DbResult<String> {
    let mut conn = pool.acquire().await?;
    insert_project_conn(&mut conn, data).await
}

/// Connection-level variant of [`insert_project`]: takes `&mut SqliteConnection`
/// (works against a plain connection or a `Transaction` deref) so composite
/// `*_tx` functions (e.g. [`super::create_project_tx`]) can compose it with
/// other writes in one transaction.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on constraint violation or query failure.
pub(super) async fn insert_project_conn(
    conn: &mut SqliteConnection,
    data: &InsertProject<'_>,
) -> DbResult<String> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO projects (id, name, tool, lifecycle, path, notes, canonical_target_id, channel_drift, is_mosaic, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)",
    )
    .bind(data.id)
    .bind(data.name)
    .bind(data.tool)
    .bind(data.lifecycle)
    .bind(data.path)
    .bind(data.notes)
    .bind(data.canonical_target_id)
    .bind(data.is_mosaic)
    .bind(&now)
    .bind(&now)
    .execute(&mut *conn)
    .await?;
    Ok(now)
}

/// Fetch a single project row by id.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] when no project with the given id exists.
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_project(pool: &SqlitePool, id: &str) -> DbResult<ProjectRow> {
    let row: Option<ProjectRowTuple> = sqlx::query_as(
        "SELECT id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at,
                blocked_reason_kind, blocked_reason_note, is_mosaic
         FROM projects WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let (
        id,
        name,
        tool,
        lifecycle,
        path,
        notes,
        channel_drift,
        created_at,
        updated_at,
        blocked_reason_kind,
        blocked_reason_note,
        is_mosaic,
    ) = row.ok_or_else(|| DbError::NotFound(format!("project {id}")))?;

    Ok(ProjectRow {
        id,
        name,
        tool,
        lifecycle,
        path,
        notes,
        channel_drift: channel_drift != 0,
        created_at,
        updated_at,
        blocked_reason_kind,
        blocked_reason_note,
        is_mosaic: is_mosaic != 0,
    })
}

/// Read the spec-035 `canonical_target_id` association for a project (spec 035
/// US1 #2). Returns `Ok(None)` when the project has no canonical target set, or
/// when the project id does not exist.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_project_canonical_target_id(
    pool: &SqlitePool,
    id: &str,
) -> DbResult<Option<String>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT canonical_target_id FROM projects WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(ctid,)| ctid))
}

/// A project's associated spec-035 canonical target, resolved via LEFT JOIN
/// (spec 035 US1 #2). `None` when the project has no `canonical_target_id` set
/// (or the join finds no matching row).
#[derive(Clone, Debug)]
pub struct ProjectCanonicalTargetRow {
    pub id: String,
    pub primary_designation: String,
    pub common_name: Option<String>,
}

/// Read a project's associated canonical target (id, primary designation, and
/// a `common_name` alias when present) via LEFT JOIN on
/// `projects.canonical_target_id`. Returns `Ok(None)` when there is no
/// association.
///
/// The common name is the first `kind = 'common_name'` alias for the target
/// (alphabetical), or `None` when the target has no common-name alias.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_project_canonical_target(
    pool: &SqlitePool,
    id: &str,
) -> DbResult<Option<ProjectCanonicalTargetRow>> {
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT ct.id, ct.primary_designation,
                (SELECT ta.alias FROM target_alias ta
                  WHERE ta.target_id = ct.id AND ta.kind = 'common_name'
                  ORDER BY ta.alias ASC LIMIT 1) AS common_name
         FROM projects p
         JOIN canonical_target ct ON ct.id = p.canonical_target_id
         WHERE p.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, primary_designation, common_name)| ProjectCanonicalTargetRow {
        id,
        primary_designation,
        common_name,
    }))
}

/// Set a project's spec-035 `canonical_target_id` association, but only if it
/// is currently unset.
///
/// Spec 041 R-17/FR-052: called when a light's resolved target propagates from
/// its acquisition session to a linked project. This never overwrites an
/// existing value — whether it was set manually at project creation
/// (spec-035 US1 #2) or by an earlier propagation — so a project's canonical
/// target is first-write-wins, not last-write-wins.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn set_project_canonical_target_id(
    pool: &SqlitePool,
    id: &str,
    canonical_target_id: &str,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "UPDATE projects SET canonical_target_id = ?, updated_at = ? \
         WHERE id = ? AND canonical_target_id IS NULL",
    )
    .bind(canonical_target_id)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// List all projects ordered by updated_at descending.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_projects(pool: &SqlitePool) -> DbResult<Vec<ProjectRow>> {
    let rows: Vec<ProjectRowTuple> = sqlx::query_as(
        "SELECT id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at,
                blocked_reason_kind, blocked_reason_note, is_mosaic
         FROM projects ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                name,
                tool,
                lifecycle,
                path,
                notes,
                channel_drift,
                created_at,
                updated_at,
                blocked_reason_kind,
                blocked_reason_note,
                is_mosaic,
            )| {
                ProjectRow {
                    id,
                    name,
                    tool,
                    lifecycle,
                    path,
                    notes,
                    channel_drift: channel_drift != 0,
                    created_at,
                    updated_at,
                    blocked_reason_kind,
                    blocked_reason_note,
                    is_mosaic: is_mosaic != 0,
                }
            },
        )
        .collect())
}

/// List every project whose `canonical_target_id` matches (spec 008 Q27,
/// F-Framing-5 attribution — the `flag_optic_difference` candidate detection:
/// same target, different optic-train).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_projects_by_canonical_target_id(
    pool: &SqlitePool,
    canonical_target_id: &str,
) -> DbResult<Vec<ProjectRow>> {
    let rows: Vec<ProjectRowTuple> = sqlx::query_as(
        "SELECT id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at,
                blocked_reason_kind, blocked_reason_note, is_mosaic
         FROM projects WHERE canonical_target_id = ? ORDER BY updated_at DESC",
    )
    .bind(canonical_target_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                name,
                tool,
                lifecycle,
                path,
                notes,
                channel_drift,
                created_at,
                updated_at,
                blocked_reason_kind,
                blocked_reason_note,
                is_mosaic,
            )| {
                ProjectRow {
                    id,
                    name,
                    tool,
                    lifecycle,
                    path,
                    notes,
                    channel_drift: channel_drift != 0,
                    created_at,
                    updated_at,
                    blocked_reason_kind,
                    blocked_reason_note,
                    is_mosaic: is_mosaic != 0,
                }
            },
        )
        .collect())
}

/// Check whether a project with the given name already exists (excluding a
/// specific id — used by update to allow rename to same value).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn name_exists(
    pool: &SqlitePool,
    name: &str,
    exclude_id: Option<&str>,
) -> DbResult<Option<String>> {
    let row: Option<(String,)> = match exclude_id {
        Some(excl) => {
            sqlx::query_as("SELECT id FROM projects WHERE name = ? AND id != ?")
                .bind(name)
                .bind(excl)
                .fetch_optional(pool)
                .await?
        }
        None => {
            sqlx::query_as("SELECT id FROM projects WHERE name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await?
        }
    };
    Ok(row.map(|(id,)| id))
}

/// Check whether a project with the given path already exists.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn path_exists(
    pool: &SqlitePool,
    path: &str,
    exclude_id: Option<&str>,
) -> DbResult<Option<String>> {
    let row: Option<(String,)> = match exclude_id {
        Some(excl) => {
            sqlx::query_as("SELECT id FROM projects WHERE path = ? AND id != ?")
                .bind(path)
                .bind(excl)
                .fetch_optional(pool)
                .await?
        }
        None => {
            sqlx::query_as("SELECT id FROM projects WHERE path = ?")
                .bind(path)
                .fetch_optional(pool)
                .await?
        }
    };
    Ok(row.map(|(id,)| id))
}

/// Update whitelisted metadata fields on a project (name, tool, notes,
/// is_mosaic). Always bumps `updated_at`. Returns the new `updated_at`
/// timestamp.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn update_project_fields(
    pool: &SqlitePool,
    id: &str,
    name: Option<&str>,
    tool: Option<&str>,
    notes: Option<&str>,
    is_mosaic: Option<bool>,
) -> DbResult<String> {
    // Build a dynamic update; at least one field must be supplied (enforced by caller).
    let now = Timestamp::now_iso();
    // Fetch the current row so we can patch only what changed.
    let current = get_project(pool, id).await?;
    let new_name = name.unwrap_or(&current.name);
    let new_tool = tool.unwrap_or(&current.tool);
    let new_notes: Option<&str> = notes.or(current.notes.as_deref());
    let new_is_mosaic = is_mosaic.unwrap_or(current.is_mosaic);

    sqlx::query(
        "UPDATE projects SET name = ?, tool = ?, notes = ?, is_mosaic = ?, updated_at = ? WHERE id = ?",
    )
    .bind(new_name)
    .bind(new_tool)
    .bind(new_notes)
    .bind(new_is_mosaic)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(now)
}

/// Update a project's lifecycle state. Returns the new `updated_at` timestamp.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn update_project_lifecycle(
    pool: &SqlitePool,
    id: &str,
    lifecycle: &str,
) -> DbResult<String> {
    let now = Timestamp::now_iso();
    sqlx::query("UPDATE projects SET lifecycle = ?, updated_at = ? WHERE id = ?")
        .bind(lifecycle)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(now)
}

fn project_state(value: &str) -> Option<ProjectState> {
    match value {
        "setup_incomplete" => Some(ProjectState::SetupIncomplete),
        "ready" => Some(ProjectState::Ready),
        "prepared" => Some(ProjectState::Prepared),
        "processing" => Some(ProjectState::Processing),
        "completed" => Some(ProjectState::Completed),
        "archived" => Some(ProjectState::Archived),
        "blocked" => Some(ProjectState::Blocked),
        _ => None,
    }
}

/// Result of a canonical automatic project-block mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectAutoBlockOutcome {
    /// The lifecycle change and audit row committed atomically.
    Applied,
    /// The requested edge is not canonical for the expected lifecycle.
    Rejected,
    /// The selected lifecycle changed before the compare-and-swap.
    CasLost {
        /// Lifecycle observed after losing the compare-and-swap, if the row
        /// still exists.
        current_lifecycle: Option<String>,
        /// Whether the observed lifecycle still has a canonical edge to
        /// `blocked` and therefore needs a durable retry.
        still_blockable: bool,
    },
}

/// Atomically apply a canonical project auto-block transition and its required
/// audit row when the lifecycle still matches `expected_from_state`.
///
/// Returns [`ProjectAutoBlockOutcome::Rejected`] when the edge is forbidden,
/// or [`ProjectAutoBlockOutcome::CasLost`] when the compare-and-swap loses a
/// concurrent lifecycle change.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on update, audit, or transaction failure.
pub async fn apply_project_auto_block(
    pool: &SqlitePool,
    id: &str,
    expected_from_state: &str,
    reason_kind: &str,
    reason_note: &str,
    trigger: &str,
) -> DbResult<ProjectAutoBlockOutcome> {
    let Some(from_state) = project_state(expected_from_state) else {
        return Ok(ProjectAutoBlockOutcome::Rejected);
    };
    if !is_allowed(from_state, ProjectState::Blocked) {
        return Ok(ProjectAutoBlockOutcome::Rejected);
    }

    let now = Timestamp::now_iso();
    let mut tx = pool.begin().await?;
    let rows_affected = sqlx::query(
        "UPDATE projects SET lifecycle = 'blocked', blocked_reason_kind = ?,
         blocked_reason_note = ?, updated_at = ?
         WHERE id = ? AND lifecycle = ?",
    )
    .bind(reason_kind)
    .bind(reason_note)
    .bind(&now)
    .bind(id)
    .bind(expected_from_state)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        let current_lifecycle =
            sqlx::query_scalar::<_, String>("SELECT lifecycle FROM projects WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let still_blockable = current_lifecycle
            .as_deref()
            .and_then(project_state)
            .is_some_and(|state| is_allowed(state, ProjectState::Blocked));
        tx.commit().await?;
        return Ok(ProjectAutoBlockOutcome::CasLost { current_lifecycle, still_blockable });
    }

    persistence_core::repositories::audit_writes::insert_project_auto_transition_conn(
        &mut tx,
        id,
        expected_from_state,
        "blocked",
        trigger,
    )
    .await?;
    tx.commit().await?;
    Ok(ProjectAutoBlockOutcome::Applied)
}

/// Update a project's lifecycle state and clear the blocked reason columns.
///
/// FR-020 / T053: should be called when transitioning OUT of "blocked" so that
/// stale blocked_reason_kind/note are not left behind.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn update_project_lifecycle_unblock(
    pool: &SqlitePool,
    id: &str,
    lifecycle: &str,
) -> DbResult<String> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "UPDATE projects SET lifecycle = ?, \
         blocked_reason_kind = NULL, blocked_reason_note = NULL, updated_at = ? \
         WHERE id = ?",
    )
    .bind(lifecycle)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(now)
}

/// Record the archive plan that drove a project into the `archived` lifecycle
/// state (spec 017 C5, migration 0053). Idempotent overwrite; does not touch
/// `updated_at` so the archived timestamp set by the lifecycle transition is
/// preserved.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn set_archived_via_plan_id(
    pool: &SqlitePool,
    project_id: &str,
    plan_id: &str,
) -> DbResult<()> {
    sqlx::query("UPDATE projects SET archived_via_plan_id = ? WHERE id = ?")
        .bind(plan_id)
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Clear the archive-plan link on a restored (un-archived) project (#885).
/// Counterpart to [`set_archived_via_plan_id`]; called once the R-Unarchive
/// lifecycle transition succeeds so the Archive listing (`lifecycle =
/// 'archived'` filter) drops the row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn clear_archived_via_plan_id(pool: &SqlitePool, project_id: &str) -> DbResult<()> {
    sqlx::query("UPDATE projects SET archived_via_plan_id = NULL WHERE id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// One archived-project row for the Archive surface (spec 017 `archive.list`).
///
/// Joined with the owning archive plan so the row can surface the plan title
/// (as the archive reason) and the bytes moved into the app-managed archive.
#[derive(Debug, Clone)]
pub struct ArchivedProjectRow {
    pub id: String,
    pub name: String,
    pub path: String,
    /// `updated_at` at the time the project reached `archived`.
    pub archived_at: String,
    pub archived_via_plan_id: Option<String>,
    /// Owning plan's title, when the plan row still exists.
    pub plan_title: Option<String>,
    /// Bytes the owning plan moved into the archive (`total_bytes_required`).
    pub archived_bytes: Option<i64>,
}

type ArchivedProjectTuple =
    (String, String, String, String, Option<String>, Option<String>, Option<i64>);

/// List every project currently in the `archived` lifecycle state, most-recent
/// first (spec 017 C5 — projects-only Archive surface).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_archived_projects(pool: &SqlitePool) -> DbResult<Vec<ArchivedProjectRow>> {
    let rows: Vec<ArchivedProjectTuple> = sqlx::query_as(
        "SELECT p.id, p.name, p.path, p.updated_at, p.archived_via_plan_id, \
                pl.title, pl.total_bytes_required \
         FROM projects p \
         LEFT JOIN plans pl ON pl.id = p.archived_via_plan_id \
         WHERE p.lifecycle = 'archived' \
         ORDER BY p.updated_at DESC, p.id ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, path, archived_at, archived_via_plan_id, plan_title, archived_bytes)| {
            ArchivedProjectRow {
                id,
                name,
                path,
                archived_at,
                archived_via_plan_id,
                plan_title,
                archived_bytes,
            }
        })
        .collect())
}

/// Set channel_drift flag on a project.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn set_channel_drift(pool: &SqlitePool, id: &str, has_drift: bool) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query("UPDATE projects SET channel_drift = ?, updated_at = ? WHERE id = ?")
        .bind(i64::from(has_drift))
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
