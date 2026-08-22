// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::missing_errors_doc,
    clippy::type_complexity,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    dead_code
)]

//! Repository methods for `materialization_update_plan` and related tables
//! (spec 062 US3/US5 FR-055–FR-059, FR-093–FR-100).
//!
//! All functions operate on a caller-supplied `SqliteConnection` inside a
//! `BEGIN IMMEDIATE` transaction. None of these functions commit or roll back.
//!
//! ## Schema notes
//!
//! `materialization_update_plan` DB state `draft` maps to contract `open`.
//! Conflict count is derived at query time from `materialization_plan_entry
//! WHERE collision_state = 'collision'`.

use sqlx::SqliteConnection;

use persistence_core::{DbError, DbResult};

// ── Row types ─────────────────────────────────────────────────────────────────

/// Full `spec062_project` projection including both head columns.
#[derive(Debug, Clone)]
pub struct ProjectMatRow {
    pub row_id: i64,
    pub public_id: String,
    pub membership_head_revision_row_id: Option<i64>,
    pub membership_head_generation: i64,
    pub materialization_head_snapshot_row_id: Option<i64>,
    pub materialization_head_generation: i64,
}

/// A `materialization_update_plan` row.
#[derive(Debug, Clone)]
pub struct UpdatePlanRow {
    pub row_id: i64,
    pub public_id: String,
    pub project_row_id: i64,
    pub base_snapshot_row_id: Option<i64>,
    pub target_membership_revision_row_id: i64,
    /// DB value: `draft`, `approved`, `applying`, `stopped`, `applied`,
    /// `failed`, `discarded`, or `stale`. Maps to contract `open` for `draft`.
    pub state: String,
    pub content_digest: String,
    pub session_count: i64,
    pub item_count: i64,
    pub source_frame_count: i64,
    pub source_byte_count: i64,
    pub remaining_session_count: i64,
    pub next_session_row_id: Option<i64>,
    pub actor_row_id: i64,
    pub created_at: String,
}

/// One `materialization_plan_entry` row.
#[derive(Debug, Clone)]
pub struct PlanEntryRow {
    pub row_id: i64,
    pub public_id: String,
    pub plan_row_id: i64,
    pub session_row_id: i64,
    pub frame_row_id: i64,
    pub destination_root_row_id: i64,
    pub relative_path: String,
    pub approved_fingerprint: String,
    pub collision_state: String,
    pub ordinal: i64,
}

/// One pinned-session row from `materialization_update_plan_session`.
#[derive(Debug, Clone)]
pub struct PlanSessionRow {
    pub session_row_id: i64,
    pub session_public_id: String,
    pub ordinal: i64,
}

/// One overlay-mapping row from `materialization_plan_overlay_mapping`.
#[derive(Debug, Clone)]
pub struct OverlayMappingRow {
    pub predecessor_entry_row_id: i64,
    pub predecessor_entry_public_id: String,
    pub replacement_plan_entry_row_id: Option<i64>,
    pub replacement_plan_entry_public_id: Option<String>,
    pub exclusion_reason_code: Option<String>,
    pub ordinal: i64,
}

/// `spec062_destination_root` row.
#[derive(Debug, Clone)]
pub struct DestinationRootSnapshot {
    pub row_id: i64,
    pub public_id: String,
}

/// A frame row with source information for plan generation.
#[derive(Debug, Clone)]
pub struct SessionFrameRow {
    pub frame_row_id: i64,
    pub frame_public_id: String,
    pub file_row_id: i64,
    pub byte_size: i64,
}

// ── Project helpers ───────────────────────────────────────────────────────────

/// Fetch full `spec062_project` row.
pub async fn get_project(conn: &mut SqliteConnection, public_id: &str) -> DbResult<ProjectMatRow> {
    let row: Option<(i64, String, Option<i64>, i64, Option<i64>, i64)> = sqlx::query_as(
        "SELECT row_id, public_id,
                membership_head_revision_row_id, membership_head_generation,
                materialization_head_snapshot_row_id, materialization_head_generation
         FROM spec062_project WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(&mut *conn)
    .await?;

    row.map(|(row_id, public_id, mem_rev, mem_gen, mat_snap, mat_gen)| ProjectMatRow {
        row_id,
        public_id,
        membership_head_revision_row_id: mem_rev,
        membership_head_generation: mem_gen,
        materialization_head_snapshot_row_id: mat_snap,
        materialization_head_generation: mat_gen,
    })
    .ok_or_else(|| DbError::NotFound(format!("spec062_project {public_id}")))
}

/// Count sessions in the membership revision not present in the base snapshot.
pub async fn count_unmaterialized_sessions(
    conn: &mut SqliteConnection,
    membership_revision_row_id: i64,
    base_snapshot_row_id: Option<i64>,
) -> DbResult<i64> {
    let count: (i64,) = match base_snapshot_row_id {
        None => {
            sqlx::query_as(
                "SELECT COUNT(*) FROM project_membership_revision_session
             WHERE revision_row_id = ?",
            )
            .bind(membership_revision_row_id)
            .fetch_one(&mut *conn)
            .await?
        }
        Some(snap) => {
            sqlx::query_as(
                "SELECT COUNT(*) FROM project_membership_revision_session pmrs
             WHERE pmrs.revision_row_id = ?
               AND pmrs.session_row_id NOT IN (
                   SELECT session_row_id
                   FROM project_materialization_snapshot_session
                   WHERE snapshot_row_id = ?
               )",
            )
            .bind(membership_revision_row_id)
            .bind(snap)
            .fetch_one(&mut *conn)
            .await?
        }
    };
    Ok(count.0)
}

/// List unmaterialized sessions (row_id, public_id, created_at) ordered by row_id.
pub async fn list_unmaterialized_sessions(
    conn: &mut SqliteConnection,
    membership_revision_row_id: i64,
    base_snapshot_row_id: Option<i64>,
    from_row_id: Option<i64>,
    limit: i64,
) -> DbResult<Vec<(i64, String, String)>> {
    let rows: Vec<(i64, String, String)> = match (base_snapshot_row_id, from_row_id) {
        (None, None) => {
            sqlx::query_as(
                "SELECT s.row_id, s.public_id, s.created_at
             FROM project_membership_revision_session pmrs
             INNER JOIN session s ON s.row_id = pmrs.session_row_id
             WHERE pmrs.revision_row_id = ?
             ORDER BY s.row_id ASC LIMIT ?",
            )
            .bind(membership_revision_row_id)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?
        }
        (None, Some(from)) => {
            sqlx::query_as(
                "SELECT s.row_id, s.public_id, s.created_at
             FROM project_membership_revision_session pmrs
             INNER JOIN session s ON s.row_id = pmrs.session_row_id
             WHERE pmrs.revision_row_id = ? AND s.row_id >= ?
             ORDER BY s.row_id ASC LIMIT ?",
            )
            .bind(membership_revision_row_id)
            .bind(from)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?
        }
        (Some(snap), None) => {
            sqlx::query_as(
                "SELECT s.row_id, s.public_id, s.created_at
             FROM project_membership_revision_session pmrs
             INNER JOIN session s ON s.row_id = pmrs.session_row_id
             WHERE pmrs.revision_row_id = ?
               AND s.row_id NOT IN (
                   SELECT session_row_id
                   FROM project_materialization_snapshot_session
                   WHERE snapshot_row_id = ?
               )
             ORDER BY s.row_id ASC LIMIT ?",
            )
            .bind(membership_revision_row_id)
            .bind(snap)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?
        }
        (Some(snap), Some(from)) => {
            sqlx::query_as(
                "SELECT s.row_id, s.public_id, s.created_at
             FROM project_membership_revision_session pmrs
             INNER JOIN session s ON s.row_id = pmrs.session_row_id
             WHERE pmrs.revision_row_id = ? AND s.row_id >= ?
               AND s.row_id NOT IN (
                   SELECT session_row_id
                   FROM project_materialization_snapshot_session
                   WHERE snapshot_row_id = ?
               )
             ORDER BY s.row_id ASC LIMIT ?",
            )
            .bind(membership_revision_row_id)
            .bind(from)
            .bind(snap)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?
        }
    };
    Ok(rows)
}

/// Fetch frame rows for a session, ordered by ordinal.
pub async fn list_session_frames_for_plan(
    conn: &mut SqliteConnection,
    session_row_id: i64,
) -> DbResult<Vec<SessionFrameRow>> {
    let rows: Vec<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT fr.row_id, fr.public_id, fr.file_row_id, fr.byte_size
         FROM session_frame sf
         INNER JOIN frame_record fr ON fr.row_id = sf.frame_row_id
         WHERE sf.session_row_id = ?
         ORDER BY sf.ordinal ASC",
    )
    .bind(session_row_id)
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(row_id, pub_id, file_row_id, byte_size)| SessionFrameRow {
            frame_row_id: row_id,
            frame_public_id: pub_id,
            file_row_id,
            byte_size,
        })
        .collect())
}

/// Fetch `spec062_destination_root` for a project (first by row_id).
pub async fn get_destination_root(
    conn: &mut SqliteConnection,
    project_row_id: i64,
) -> DbResult<DestinationRootSnapshot> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT row_id, public_id FROM spec062_destination_root
         WHERE project_row_id = ? ORDER BY row_id ASC LIMIT 1",
    )
    .bind(project_row_id)
    .fetch_optional(&mut *conn)
    .await?;

    row.map(|(row_id, public_id)| DestinationRootSnapshot { row_id, public_id })
        .ok_or_else(|| DbError::NotFound(format!("destination_root project={project_row_id}")))
}

/// Return true if a `materialized_entry` already occupies the destination path.
pub async fn destination_path_exists(
    conn: &mut SqliteConnection,
    project_row_id: i64,
    destination_root_row_id: i64,
    relative_path: &str,
) -> DbResult<bool> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT row_id FROM materialized_entry
         WHERE project_row_id = ? AND destination_root_row_id = ? AND relative_path = ?",
    )
    .bind(project_row_id)
    .bind(destination_root_row_id)
    .bind(relative_path)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

// ── Plan CRUD ─────────────────────────────────────────────────────────────────

/// Parameters for inserting a new `materialization_update_plan` row.
pub struct InsertUpdatePlan<'a> {
    pub public_id: &'a str,
    pub project_row_id: i64,
    pub base_snapshot_row_id: Option<i64>,
    pub target_membership_revision_row_id: i64,
    pub content_digest: &'a str,
    pub session_count: i64,
    pub item_count: i64,
    pub source_frame_count: i64,
    pub source_byte_count: i64,
    pub remaining_session_count: i64,
    pub next_session_row_id: Option<i64>,
    pub actor_row_id: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Insert a `materialization_update_plan` row in `draft` state. Returns `row_id`.
pub async fn insert_update_plan(
    conn: &mut SqliteConnection,
    p: InsertUpdatePlan<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO materialization_update_plan (
             public_id, project_row_id, base_snapshot_row_id,
             target_membership_revision_row_id, state, content_digest,
             session_count, item_count, source_frame_count, source_byte_count,
             remaining_session_count, next_session_row_id,
             actor_row_id, created_sequence, created_at
         ) VALUES (?,?,?, ?,'draft',?, ?,?,?,?, ?,?, ?,?,?)",
    )
    .bind(p.public_id)
    .bind(p.project_row_id)
    .bind(p.base_snapshot_row_id)
    .bind(p.target_membership_revision_row_id)
    .bind(p.content_digest)
    .bind(p.session_count)
    .bind(p.item_count)
    .bind(p.source_frame_count)
    .bind(p.source_byte_count)
    .bind(p.remaining_session_count)
    .bind(p.next_session_row_id)
    .bind(p.actor_row_id)
    .bind(p.created_sequence)
    .bind(p.created_at)
    .execute(&mut *conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Update final counts + digest on a plan row after item generation.
pub async fn update_plan_counts(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
    session_count: i64,
    item_count: i64,
    source_frame_count: i64,
    source_byte_count: i64,
    remaining_session_count: i64,
    next_session_row_id: Option<i64>,
    content_digest: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE materialization_update_plan
         SET session_count = ?, item_count = ?, source_frame_count = ?,
             source_byte_count = ?, remaining_session_count = ?,
             next_session_row_id = ?, content_digest = ?
         WHERE row_id = ?",
    )
    .bind(session_count)
    .bind(item_count)
    .bind(source_frame_count)
    .bind(source_byte_count)
    .bind(remaining_session_count)
    .bind(next_session_row_id)
    .bind(content_digest)
    .bind(plan_row_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Insert a `materialization_update_plan_session` row.
pub async fn insert_plan_session(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
    session_row_id: i64,
    ordinal: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO materialization_update_plan_session
             (plan_row_id, session_row_id, ordinal)
         VALUES (?,?,?)",
    )
    .bind(plan_row_id)
    .bind(session_row_id)
    .bind(ordinal)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Parameters for inserting a `materialization_plan_entry` row.
pub struct InsertPlanEntry<'a> {
    pub public_id: &'a str,
    pub plan_row_id: i64,
    pub session_row_id: i64,
    pub frame_row_id: i64,
    pub destination_root_row_id: i64,
    pub relative_path: &'a str,
    pub approved_fingerprint: &'a str,
    pub collision_state: &'a str,
    pub ordinal: i64,
}

/// Insert a `materialization_plan_entry` row. Returns `row_id`.
pub async fn insert_plan_entry(
    conn: &mut SqliteConnection,
    e: InsertPlanEntry<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO materialization_plan_entry (
             public_id, plan_row_id, session_row_id, frame_row_id,
             destination_root_row_id, relative_path, approved_fingerprint,
             collision_state, ordinal
         ) VALUES (?,?,?,?, ?,?,?, ?,?)",
    )
    .bind(e.public_id)
    .bind(e.plan_row_id)
    .bind(e.session_row_id)
    .bind(e.frame_row_id)
    .bind(e.destination_root_row_id)
    .bind(e.relative_path)
    .bind(e.approved_fingerprint)
    .bind(e.collision_state)
    .bind(e.ordinal)
    .execute(&mut *conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Fetch a plan row by public UUID.
pub async fn get_plan_by_public_id(
    conn: &mut SqliteConnection,
    public_id: &str,
) -> DbResult<UpdatePlanRow> {
    fetch_plan(conn, Some(public_id), None).await
}

/// Fetch a plan row by integer row_id.
pub async fn get_plan_by_row_id(
    conn: &mut SqliteConnection,
    row_id: i64,
) -> DbResult<UpdatePlanRow> {
    fetch_plan(conn, None, Some(row_id)).await
}

async fn fetch_plan(
    conn: &mut SqliteConnection,
    public_id: Option<&str>,
    row_id: Option<i64>,
) -> DbResult<UpdatePlanRow> {
    let row: Option<(
        i64,
        String,
        i64,
        Option<i64>,
        i64,
        String,
        String,
        i64,
        i64,
        i64,
        i64,
        i64,
        Option<i64>,
        i64,
        String,
    )> = if let Some(pid) = public_id {
        sqlx::query_as(
            "SELECT row_id, public_id, project_row_id, base_snapshot_row_id,
                        target_membership_revision_row_id, state, content_digest,
                        session_count, item_count, source_frame_count, source_byte_count,
                        remaining_session_count, next_session_row_id, actor_row_id, created_at
                 FROM materialization_update_plan WHERE public_id = ?",
        )
        .bind(pid)
        .fetch_optional(&mut *conn)
        .await?
    } else {
        sqlx::query_as(
            "SELECT row_id, public_id, project_row_id, base_snapshot_row_id,
                        target_membership_revision_row_id, state, content_digest,
                        session_count, item_count, source_frame_count, source_byte_count,
                        remaining_session_count, next_session_row_id, actor_row_id, created_at
                 FROM materialization_update_plan WHERE row_id = ?",
        )
        .bind(row_id.unwrap_or(0))
        .fetch_optional(&mut *conn)
        .await?
    };

    row.map(
        |(
            rid,
            pub_id,
            proj,
            base,
            target,
            state,
            digest,
            sess,
            items,
            frames,
            bytes,
            remaining,
            next,
            actor,
            created,
        )| {
            UpdatePlanRow {
                row_id: rid,
                public_id: pub_id,
                project_row_id: proj,
                base_snapshot_row_id: base,
                target_membership_revision_row_id: target,
                state,
                content_digest: digest,
                session_count: sess,
                item_count: items,
                source_frame_count: frames,
                source_byte_count: bytes,
                remaining_session_count: remaining,
                next_session_row_id: next,
                actor_row_id: actor,
                created_at: created,
            }
        },
    )
    .ok_or_else(|| {
        let key =
            public_id.map_or_else(|| format!("row_id={}", row_id.unwrap_or(0)), ToOwned::to_owned);
        DbError::NotFound(format!("materialization_update_plan {key}"))
    })
}

/// Read a plan's state by row id. `None` when no such row exists.
///
/// # Errors
/// Returns [`DbError::Database`] on query failure.
pub async fn plan_state(conn: &mut SqliteConnection, plan_row_id: i64) -> DbResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT state FROM materialization_update_plan WHERE row_id = ?")
        .bind(plan_row_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// CAS state transition.
pub async fn transition_plan_state(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
    expected_state: &str,
    new_state: &str,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE materialization_update_plan SET state = ?
         WHERE row_id = ? AND state = ?",
    )
    .bind(new_state)
    .bind(plan_row_id)
    .bind(expected_state)
    .execute(&mut *conn)
    .await?;

    if result.rows_affected() == 0 {
        Err(DbError::CasFailed(format!(
            "plan {plan_row_id}: {expected_state} → {new_state} CAS failed"
        )))
    } else {
        Ok(())
    }
}

/// Count collision entries for a plan.
pub async fn count_conflicts(conn: &mut SqliteConnection, plan_row_id: i64) -> DbResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM materialization_plan_entry
         WHERE plan_row_id = ? AND collision_state = 'collision'",
    )
    .bind(plan_row_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.0)
}

// ── Plan pagination ───────────────────────────────────────────────────────────

/// List plan sessions ordered by ordinal.
pub async fn list_plan_sessions(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
    after_ordinal: Option<i64>,
    limit: i64,
) -> DbResult<Vec<PlanSessionRow>> {
    let rows: Vec<(i64, String, i64)> = match after_ordinal {
        None => {
            sqlx::query_as(
                "SELECT mups.session_row_id, s.public_id, mups.ordinal
             FROM materialization_update_plan_session mups
             INNER JOIN session s ON s.row_id = mups.session_row_id
             WHERE mups.plan_row_id = ?
             ORDER BY mups.ordinal ASC LIMIT ?",
            )
            .bind(plan_row_id)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?
        }
        Some(after) => {
            sqlx::query_as(
                "SELECT mups.session_row_id, s.public_id, mups.ordinal
             FROM materialization_update_plan_session mups
             INNER JOIN session s ON s.row_id = mups.session_row_id
             WHERE mups.plan_row_id = ? AND mups.ordinal > ?
             ORDER BY mups.ordinal ASC LIMIT ?",
            )
            .bind(plan_row_id)
            .bind(after)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|(sess, pub_id, ord)| PlanSessionRow {
            session_row_id: sess,
            session_public_id: pub_id,
            ordinal: ord,
        })
        .collect())
}

/// List plan entries ordered by ordinal (all or collision-filtered).
pub async fn list_plan_entries(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
    after_ordinal: Option<i64>,
    limit: i64,
) -> DbResult<Vec<PlanEntryRow>> {
    list_entries_filtered(conn, plan_row_id, None, after_ordinal, limit).await
}

/// List only collision entries.
pub async fn list_conflict_entries(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
    after_ordinal: Option<i64>,
    limit: i64,
) -> DbResult<Vec<PlanEntryRow>> {
    list_entries_filtered(conn, plan_row_id, Some("collision"), after_ordinal, limit).await
}

async fn list_entries_filtered(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
    collision_state: Option<&str>,
    after_ordinal: Option<i64>,
    limit: i64,
) -> DbResult<Vec<PlanEntryRow>> {
    let rows: Vec<(i64, String, i64, i64, i64, i64, String, String, String, i64)> =
        match (collision_state, after_ordinal) {
            (None, None) => {
                sqlx::query_as(
                    "SELECT row_id, public_id, plan_row_id, session_row_id, frame_row_id,
                        destination_root_row_id, relative_path, approved_fingerprint,
                        collision_state, ordinal
                 FROM materialization_plan_entry WHERE plan_row_id = ?
                 ORDER BY ordinal ASC LIMIT ?",
                )
                .bind(plan_row_id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
            }
            (None, Some(after)) => {
                sqlx::query_as(
                    "SELECT row_id, public_id, plan_row_id, session_row_id, frame_row_id,
                        destination_root_row_id, relative_path, approved_fingerprint,
                        collision_state, ordinal
                 FROM materialization_plan_entry WHERE plan_row_id = ? AND ordinal > ?
                 ORDER BY ordinal ASC LIMIT ?",
                )
                .bind(plan_row_id)
                .bind(after)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
            }
            (Some(coll), None) => {
                sqlx::query_as(
                    "SELECT row_id, public_id, plan_row_id, session_row_id, frame_row_id,
                        destination_root_row_id, relative_path, approved_fingerprint,
                        collision_state, ordinal
                 FROM materialization_plan_entry
                 WHERE plan_row_id = ? AND collision_state = ?
                 ORDER BY ordinal ASC LIMIT ?",
                )
                .bind(plan_row_id)
                .bind(coll)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
            }
            (Some(coll), Some(after)) => {
                sqlx::query_as(
                    "SELECT row_id, public_id, plan_row_id, session_row_id, frame_row_id,
                        destination_root_row_id, relative_path, approved_fingerprint,
                        collision_state, ordinal
                 FROM materialization_plan_entry
                 WHERE plan_row_id = ? AND collision_state = ? AND ordinal > ?
                 ORDER BY ordinal ASC LIMIT ?",
                )
                .bind(plan_row_id)
                .bind(coll)
                .bind(after)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
            }
        };

    Ok(rows
        .into_iter()
        .map(|(rid, pub_id, plan, sess, frame, dest, path, fp, coll, ord)| PlanEntryRow {
            row_id: rid,
            public_id: pub_id,
            plan_row_id: plan,
            session_row_id: sess,
            frame_row_id: frame,
            destination_root_row_id: dest,
            relative_path: path,
            approved_fingerprint: fp,
            collision_state: coll,
            ordinal: ord,
        })
        .collect())
}

/// List overlay mappings ordered by ordinal.
pub async fn list_overlay_mappings(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
    after_ordinal: Option<i64>,
    limit: i64,
) -> DbResult<Vec<OverlayMappingRow>> {
    let rows: Vec<(i64, String, Option<i64>, Option<String>, Option<String>, i64)> =
        match after_ordinal {
            None => {
                sqlx::query_as(
                    "SELECT mom.predecessor_entry_row_id, me.public_id,
                        mom.replacement_plan_entry_row_id, mpe.public_id,
                        mom.exclusion_reason_code, mom.ordinal
                 FROM materialization_plan_overlay_mapping mom
                 INNER JOIN materialized_entry me ON me.row_id = mom.predecessor_entry_row_id
                 LEFT JOIN materialization_plan_entry mpe
                     ON mpe.row_id = mom.replacement_plan_entry_row_id
                 WHERE mom.plan_row_id = ?
                 ORDER BY mom.ordinal ASC LIMIT ?",
                )
                .bind(plan_row_id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
            }
            Some(after) => {
                sqlx::query_as(
                    "SELECT mom.predecessor_entry_row_id, me.public_id,
                        mom.replacement_plan_entry_row_id, mpe.public_id,
                        mom.exclusion_reason_code, mom.ordinal
                 FROM materialization_plan_overlay_mapping mom
                 INNER JOIN materialized_entry me ON me.row_id = mom.predecessor_entry_row_id
                 LEFT JOIN materialization_plan_entry mpe
                     ON mpe.row_id = mom.replacement_plan_entry_row_id
                 WHERE mom.plan_row_id = ? AND mom.ordinal > ?
                 ORDER BY mom.ordinal ASC LIMIT ?",
                )
                .bind(plan_row_id)
                .bind(after)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
            }
        };

    Ok(rows
        .into_iter()
        .map(|(pred, pred_pub, repl, repl_pub, excl, ord)| OverlayMappingRow {
            predecessor_entry_row_id: pred,
            predecessor_entry_public_id: pred_pub,
            replacement_plan_entry_row_id: repl,
            replacement_plan_entry_public_id: repl_pub,
            exclusion_reason_code: excl,
            ordinal: ord,
        })
        .collect())
}

// ── Snapshot writes ───────────────────────────────────────────────────────────

/// Parameters for inserting a `materialized_entry` row.
pub struct InsertMaterializedEntry<'a> {
    pub public_id: &'a str,
    pub project_row_id: i64,
    pub first_snapshot_row_id: i64,
    pub source_session_row_id: i64,
    pub source_frame_row_id: i64,
    pub destination_root_row_id: i64,
    pub relative_path: &'a str,
    pub content_fingerprint: Option<&'a str>,
    pub created_by_plan_row_id: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Insert a `materialized_entry` row. Returns `row_id`.
pub async fn insert_materialized_entry(
    conn: &mut SqliteConnection,
    e: InsertMaterializedEntry<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO materialized_entry (
             public_id, project_row_id, first_snapshot_row_id,
             source_session_row_id, source_frame_row_id,
             destination_root_row_id, relative_path, content_fingerprint,
             created_by_plan_row_id, created_sequence, created_at
         ) VALUES (?,?,?, ?,?, ?,?,?, ?,?,?)",
    )
    .bind(e.public_id)
    .bind(e.project_row_id)
    .bind(e.first_snapshot_row_id)
    .bind(e.source_session_row_id)
    .bind(e.source_frame_row_id)
    .bind(e.destination_root_row_id)
    .bind(e.relative_path)
    .bind(e.content_fingerprint)
    .bind(e.created_by_plan_row_id)
    .bind(e.created_sequence)
    .bind(e.created_at)
    .execute(&mut *conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Parameters for inserting a `project_materialization_snapshot` row.
pub struct InsertMatSnapshot<'a> {
    pub public_id: &'a str,
    pub project_row_id: i64,
    pub membership_revision_row_id: i64,
    pub predecessor_snapshot_row_id: Option<i64>,
    pub applied_plan_row_id: i64,
    pub entry_count: i64,
    pub session_count: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// Insert a `project_materialization_snapshot` row. Returns `row_id`.
pub async fn insert_mat_snapshot(
    conn: &mut SqliteConnection,
    s: InsertMatSnapshot<'_>,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT INTO project_materialization_snapshot (
             public_id, project_row_id, membership_revision_row_id,
             predecessor_snapshot_row_id, applied_plan_row_id,
             entry_count, session_count, created_sequence, created_at
         ) VALUES (?,?,?, ?,?, ?,?,?,?)",
    )
    .bind(s.public_id)
    .bind(s.project_row_id)
    .bind(s.membership_revision_row_id)
    .bind(s.predecessor_snapshot_row_id)
    .bind(s.applied_plan_row_id)
    .bind(s.entry_count)
    .bind(s.session_count)
    .bind(s.created_sequence)
    .bind(s.created_at)
    .execute(&mut *conn)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert a `project_materialization_snapshot_session` row.
pub async fn insert_snapshot_session(
    conn: &mut SqliteConnection,
    snapshot_row_id: i64,
    session_row_id: i64,
    ordinal: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO project_materialization_snapshot_session
             (snapshot_row_id, session_row_id, ordinal) VALUES (?,?,?)",
    )
    .bind(snapshot_row_id)
    .bind(session_row_id)
    .bind(ordinal)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Insert a `project_materialization_snapshot_entry` row.
pub async fn insert_snapshot_entry(
    conn: &mut SqliteConnection,
    snapshot_row_id: i64,
    entry_row_id: i64,
    ordinal: i64,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO project_materialization_snapshot_entry
             (snapshot_row_id, entry_row_id, ordinal) VALUES (?,?,?)",
    )
    .bind(snapshot_row_id)
    .bind(entry_row_id)
    .bind(ordinal)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Advance `materialization_head_snapshot_row_id` via CAS.
pub async fn advance_materialization_head(
    conn: &mut SqliteConnection,
    project_row_id: i64,
    new_snapshot_row_id: i64,
    expected_generation: i64,
    accepted_sequence: i64,
) -> DbResult<()> {
    let result = sqlx::query(
        "UPDATE spec062_project
         SET materialization_head_snapshot_row_id = ?,
             materialization_head_generation = materialization_head_generation + 1
         WHERE row_id = ? AND materialization_head_generation = ?",
    )
    .bind(new_snapshot_row_id)
    .bind(project_row_id)
    .bind(expected_generation)
    .execute(&mut *conn)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DbError::CasFailed(format!(
            "project {project_row_id} mat head CAS failed at gen {expected_generation}"
        )));
    }

    let new_gen = expected_generation + 1;
    let _ = sqlx::query(
        "INSERT INTO project_materialization_head_history
             (project_row_id, generation, head_snapshot_row_id, accepted_sequence)
         VALUES (?,?,?,?)",
    )
    .bind(project_row_id)
    .bind(new_gen)
    .bind(new_snapshot_row_id)
    .bind(accepted_sequence)
    .execute(&mut *conn)
    .await;

    Ok(())
}

// ── Install-intent and journal ────────────────────────────────────────────────

/// Upsert a `materialization_install_intent` row in `prepared` state.
pub async fn upsert_install_intent(
    conn: &mut SqliteConnection,
    plan_item_row_id: i64,
    plan_row_id: i64,
    collision_key: &str,
    canonical_destination: &str,
    approved_fingerprint: &str,
    ownership_token: &str,
    command_row_id: i64,
    lease_owner: &str,
    lease_generation: i64,
    now: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO materialization_install_intent (
             plan_item_row_id, plan_row_id, collision_key, canonical_destination,
             approved_fingerprint, ownership_token, command_row_id,
             lease_owner, lease_generation, state, updated_at
         ) VALUES (?,?,?,?, ?,?,?, ?,?,'prepared',?)
         ON CONFLICT(plan_item_row_id) DO UPDATE SET
             ownership_token = excluded.ownership_token,
             command_row_id  = excluded.command_row_id,
             lease_owner     = excluded.lease_owner,
             lease_generation= excluded.lease_generation,
             state           = 'prepared',
             updated_at      = excluded.updated_at",
    )
    .bind(plan_item_row_id)
    .bind(plan_row_id)
    .bind(collision_key)
    .bind(canonical_destination)
    .bind(approved_fingerprint)
    .bind(ownership_token)
    .bind(command_row_id)
    .bind(lease_owner)
    .bind(lease_generation)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Mark an install intent as `installed`.
pub async fn mark_intent_installed(
    conn: &mut SqliteConnection,
    plan_item_row_id: i64,
    lease_owner: &str,
    lease_generation: i64,
    now: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE materialization_install_intent
         SET state = 'installed', updated_at = ?
         WHERE plan_item_row_id = ? AND lease_owner = ? AND lease_generation = ?",
    )
    .bind(now)
    .bind(plan_item_row_id)
    .bind(lease_owner)
    .bind(lease_generation)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Insert a `materialization_item_journal` row and advance intent to `journaled`.
pub async fn complete_item_journal(
    conn: &mut SqliteConnection,
    plan_item_row_id: i64,
    plan_row_id: i64,
    operation_command_row_id: i64,
    resulting_entry_row_id: i64,
    destination_root_row_id: i64,
    relative_path: &str,
    content_fingerprint: &str,
    lease_owner: &str,
    lease_generation: i64,
    now: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO materialization_item_journal (
             plan_item_row_id, plan_row_id, operation_command_row_id,
             resulting_entry_row_id, destination_root_row_id, relative_path,
             content_fingerprint, lease_owner, lease_generation, completed_at
         ) VALUES (?,?,?, ?,?,?, ?,?,?,?)",
    )
    .bind(plan_item_row_id)
    .bind(plan_row_id)
    .bind(operation_command_row_id)
    .bind(resulting_entry_row_id)
    .bind(destination_root_row_id)
    .bind(relative_path)
    .bind(content_fingerprint)
    .bind(lease_owner)
    .bind(lease_generation)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE materialization_install_intent SET state = 'journaled', updated_at = ?
         WHERE plan_item_row_id = ?",
    )
    .bind(now)
    .bind(plan_item_row_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Fetch journal entries for a plan.
pub async fn list_journal_entries(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
) -> DbResult<Vec<(i64, i64, String, String)>> {
    let rows: Vec<(i64, i64, String, String)> = sqlx::query_as(
        "SELECT plan_item_row_id, resulting_entry_row_id, relative_path, content_fingerprint
         FROM materialization_item_journal WHERE plan_row_id = ?",
    )
    .bind(plan_row_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Fetch incomplete intents (prepared or installed) for recovery.
pub async fn list_incomplete_intents(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
) -> DbResult<Vec<(i64, String, String, String, String, i64)>> {
    let rows: Vec<(i64, String, String, String, String, i64)> = sqlx::query_as(
        "SELECT plan_item_row_id, collision_key, canonical_destination,
                approved_fingerprint, ownership_token, lease_generation
         FROM materialization_install_intent
         WHERE plan_row_id = ? AND state IN ('prepared','installed')",
    )
    .bind(plan_row_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

// ── Actor / command helpers ───────────────────────────────────────────────────

/// Find or create a `spec062_actor` row. Returns `row_id`.
pub async fn ensure_actor(
    conn: &mut SqliteConnection,
    public_id: &str,
    now: &str,
) -> DbResult<i64> {
    let result =
        sqlx::query("INSERT OR IGNORE INTO spec062_actor (public_id, created_at) VALUES (?,?)")
            .bind(public_id)
            .bind(now)
            .execute(&mut *conn)
            .await?;
    if result.rows_affected() > 0 {
        return Ok(result.last_insert_rowid());
    }
    let row: (i64,) = sqlx::query_as("SELECT row_id FROM spec062_actor WHERE public_id = ?")
        .bind(public_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

/// Insert a `command_execution` row in `executing` state. Returns `row_id`.
pub async fn ensure_command(
    conn: &mut SqliteConnection,
    public_id: &str,
    actor_row_id: i64,
    operation: &str,
    now: &str,
) -> DbResult<i64> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO command_execution (
             public_id, actor_row_id, operation, canonical_payload_digest,
             state, state_version, lease_generation, created_at
         ) VALUES (?,?,?,'n/a','executing',0,0,?)",
    )
    .bind(public_id)
    .bind(actor_row_id)
    .bind(operation)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    if result.rows_affected() > 0 {
        return Ok(result.last_insert_rowid());
    }
    let row: (i64,) = sqlx::query_as("SELECT row_id FROM command_execution WHERE public_id = ?")
        .bind(public_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

// ── Public-id resolution helpers ─────────────────────────────────────────────

/// Fetch `public_id` of a `session` by `row_id`.
pub async fn session_public_id(conn: &mut SqliteConnection, row_id: i64) -> DbResult<String> {
    let row: (String,) = sqlx::query_as("SELECT public_id FROM session WHERE row_id = ?")
        .bind(row_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

/// Fetch `public_id` of a `project_membership_revision` by `row_id`.
pub async fn revision_public_id(conn: &mut SqliteConnection, row_id: i64) -> DbResult<String> {
    let row: (String,) =
        sqlx::query_as("SELECT public_id FROM project_membership_revision WHERE row_id = ?")
            .bind(row_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(row.0)
}

/// Fetch `public_id` of a `project_materialization_snapshot` by `row_id`.
pub async fn snapshot_public_id(conn: &mut SqliteConnection, row_id: i64) -> DbResult<String> {
    let row: (String,) =
        sqlx::query_as("SELECT public_id FROM project_materialization_snapshot WHERE row_id = ?")
            .bind(row_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(row.0)
}

/// Fetch `public_id` of a `spec062_actor` by `row_id`.
pub async fn actor_public_id(conn: &mut SqliteConnection, row_id: i64) -> DbResult<String> {
    let row: (String,) = sqlx::query_as("SELECT public_id FROM spec062_actor WHERE row_id = ?")
        .bind(row_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

/// Fetch `public_id` of a `spec062_project` by `row_id`.
pub async fn project_public_id(conn: &mut SqliteConnection, row_id: i64) -> DbResult<String> {
    let row: (String,) = sqlx::query_as("SELECT public_id FROM spec062_project WHERE row_id = ?")
        .bind(row_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

/// Count sessions in the base snapshot's materialized-session set.
pub async fn count_base_snapshot_sessions(
    conn: &mut SqliteConnection,
    base_snapshot_row_id: Option<i64>,
) -> DbResult<i64> {
    let Some(snap) = base_snapshot_row_id else {
        return Ok(0);
    };
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM project_materialization_snapshot_session
         WHERE snapshot_row_id = ?",
    )
    .bind(snap)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.0)
}

/// List base snapshot sessions ordered by ordinal.
pub async fn list_base_snapshot_sessions(
    conn: &mut SqliteConnection,
    base_snapshot_row_id: i64,
) -> DbResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT session_row_id FROM project_materialization_snapshot_session
         WHERE snapshot_row_id = ? ORDER BY ordinal ASC",
    )
    .bind(base_snapshot_row_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

/// List plan sessions for finalize (to add to successor snapshot).
pub async fn list_plan_session_row_ids(
    conn: &mut SqliteConnection,
    plan_row_id: i64,
) -> DbResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT session_row_id FROM materialization_update_plan_session
         WHERE plan_row_id = ? ORDER BY ordinal ASC",
    )
    .bind(plan_row_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

/// Fetch plan item details needed for snapshot entry creation.
pub async fn get_plan_item_details(
    conn: &mut SqliteConnection,
    item_row_id: i64,
) -> DbResult<(i64, i64, i64)> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT session_row_id, frame_row_id, destination_root_row_id
         FROM materialization_plan_entry WHERE row_id = ?",
    )
    .bind(item_row_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}
