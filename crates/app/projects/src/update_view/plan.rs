// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(
    clippy::missing_errors_doc,
    clippy::explicit_auto_deref,
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    dead_code
)]

//! `project.update_view.plan` use case (spec 062 FR-055–FR-059, FR-093).
//!
//! Generates a bounded additive `materialization_update_plan`. Does not touch
//! the filesystem. All SQL delegated to `persistence_plans::repositories::update_view`.

use sha2::{Digest as _, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::Timestamp;
use persistence_plans::repositories::update_view as repo;
use persistence_sessions::repositories::{change_sequence, tx};

use super::query::{contract_state, UpdateViewPlan};

// ── Work limits (FR-093) ─────────────────────────────────────────────────────

pub const MAX_SESSIONS: i64 = 500;
pub const MAX_ITEMS: i64 = 100_000;
pub const MAX_SOURCE_FRAMES: i64 = 100_000;
pub const MAX_SOURCE_BYTES: i64 = 17_592_186_044_416; // 16 TiB

// ── Request / Response ────────────────────────────────────────────────────────

pub struct PlanUpdateViewRequest<'a> {
    pub project_id: &'a str,
    pub expected_project_revision: i64,
    pub actor_id: &'a str,
    pub command_id: &'a str,
}

#[derive(Debug)]
pub struct PlanUpdateViewResponse {
    pub plan: UpdateViewPlan,
}

// ── Use case ─────────────────────────────────────────────────────────────────

pub async fn plan_update_view(
    pool: &SqlitePool,
    req: &PlanUpdateViewRequest<'_>,
) -> Result<PlanUpdateViewResponse, ContractError> {
    let now = Timestamp::now_iso();
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;
    tx::begin_immediate(&mut conn).await.map_err(app_core_errors::db_err)?;

    let result = plan_inner(&mut *conn, req, &now).await;

    match result {
        Ok(plan) => {
            tx::commit(&mut conn).await.map_err(app_core_errors::db_err)?;
            Ok(PlanUpdateViewResponse { plan })
        }
        Err(e) => {
            tx::rollback(&mut conn).await;
            Err(e)
        }
    }
}

#[expect(
    clippy::cognitive_complexity,
    reason = "plan build: per-precondition error mapping then per-source diff classification"
)]
async fn plan_inner(
    conn: &mut sqlx::SqliteConnection,
    req: &PlanUpdateViewRequest<'_>,
    now: &str,
) -> Result<UpdateViewPlan, ContractError> {
    let project = repo::get_project(conn, req.project_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => ContractError::new(
            ErrorCode::ProjectNotFound,
            format!("project {} not found", req.project_id),
            ErrorSeverity::Blocking,
            false,
        ),
        other => app_core_errors::db_err(other),
    })?;

    if project.membership_head_generation != req.expected_project_revision {
        return Err(ContractError::new(
            ErrorCode::ProjectMembershipConflict,
            format!(
                "project {} expected revision {} but found {}",
                req.project_id, req.expected_project_revision, project.membership_head_generation
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let mem_rev_row_id = project.membership_head_revision_row_id.ok_or_else(|| {
        ContractError::new(
            ErrorCode::ProjectUpdateViewNoAdditions,
            format!("project {} has no membership head", req.project_id),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    let unmaterialized_count = repo::count_unmaterialized_sessions(
        conn,
        mem_rev_row_id,
        project.materialization_head_snapshot_row_id,
    )
    .await
    .map_err(app_core_errors::db_err)?;

    if unmaterialized_count == 0 {
        return Err(ContractError::new(
            ErrorCode::ProjectUpdateViewNoAdditions,
            format!("project {} has no unmaterialized sessions", req.project_id),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let dest_root =
        repo::get_destination_root(conn, project.row_id).await.map_err(app_core_errors::db_err)?;

    let actor_row_id =
        repo::ensure_actor(conn, req.actor_id, now).await.map_err(app_core_errors::db_err)?;

    repo::ensure_command(conn, req.command_id, actor_row_id, "project.update_view.plan", now)
        .await
        .map_err(app_core_errors::db_err)?;

    let seq = change_sequence::insert_repository_change(conn, None, now)
        .await
        .map_err(app_core_errors::db_err)?;

    let plan_public_id = Uuid::new_v4().to_string();
    let plan_row_id = repo::insert_update_plan(
        conn,
        repo::InsertUpdatePlan {
            public_id: &plan_public_id,
            project_row_id: project.row_id,
            base_snapshot_row_id: project.materialization_head_snapshot_row_id,
            target_membership_revision_row_id: mem_rev_row_id,
            content_digest: "pending",
            session_count: 0,
            item_count: 0,
            source_frame_count: 0,
            source_byte_count: 0,
            remaining_session_count: 0,
            next_session_row_id: None,
            actor_row_id,
            created_sequence: seq,
            created_at: now,
        },
    )
    .await
    .map_err(app_core_errors::db_err)?;

    let candidate_sessions = repo::list_unmaterialized_sessions(
        conn,
        mem_rev_row_id,
        project.materialization_head_snapshot_row_id,
        None,
        MAX_SESSIONS + 1,
    )
    .await
    .map_err(app_core_errors::db_err)?;

    let mut total_sessions: i64 = 0;
    let mut total_items: i64 = 0;
    let mut total_frames: i64 = 0;
    let mut total_bytes: i64 = 0;
    let mut item_ordinal: i64 = 0;
    let mut next_session_row_id: Option<i64> = None;
    let mut remaining: i64 = 0;
    let mut hasher = Sha256::new();

    for (i, (sess_row_id, sess_public_id, _created_at)) in candidate_sessions.iter().enumerate() {
        if total_sessions >= MAX_SESSIONS {
            next_session_row_id = Some(*sess_row_id);
            remaining = candidate_sessions.len() as i64 - i as i64;
            break;
        }

        let frames = repo::list_session_frames_for_plan(conn, *sess_row_id)
            .await
            .map_err(app_core_errors::db_err)?;

        let sess_items = frames.len() as i64;
        let sess_bytes: i64 = frames.iter().map(|f| f.byte_size).sum();

        if sess_items > MAX_ITEMS || sess_bytes > MAX_SOURCE_BYTES {
            if total_sessions == 0 {
                return Err(ContractError::new(
                    ErrorCode::ProjectUpdateViewSessionTooLarge,
                    format!(
                        "session {sess_public_id} exceeds Update View limits: items={sess_items} bytes={sess_bytes}"
                    ),
                    ErrorSeverity::Blocking,
                    false,
                ));
            }
            next_session_row_id = Some(*sess_row_id);
            remaining = candidate_sessions.len() as i64 - i as i64;
            break;
        }

        if total_items + sess_items > MAX_ITEMS || total_bytes + sess_bytes > MAX_SOURCE_BYTES {
            if total_sessions == 0 {
                return Err(ContractError::new(
                    ErrorCode::ProjectUpdateViewSessionTooLarge,
                    format!("session {sess_public_id} alone exceeds cumulative limits"),
                    ErrorSeverity::Blocking,
                    false,
                ));
            }
            next_session_row_id = Some(*sess_row_id);
            remaining = candidate_sessions.len() as i64 - i as i64;
            break;
        }

        repo::insert_plan_session(conn, plan_row_id, *sess_row_id, total_sessions)
            .await
            .map_err(app_core_errors::db_err)?;

        hasher.update(sess_public_id.as_bytes());
        hasher.update(b"|");

        for frame in &frames {
            let dest_path = format!("{sess_public_id}/{}", frame.frame_public_id);

            let collision =
                repo::destination_path_exists(conn, project.row_id, dest_root.row_id, &dest_path)
                    .await
                    .map_err(app_core_errors::db_err)?;

            let collision_state = if collision { "collision" } else { "clear" };

            let entry_public_id = Uuid::new_v4().to_string();
            repo::insert_plan_entry(
                conn,
                repo::InsertPlanEntry {
                    public_id: &entry_public_id,
                    plan_row_id,
                    session_row_id: *sess_row_id,
                    frame_row_id: frame.frame_row_id,
                    destination_root_row_id: dest_root.row_id,
                    relative_path: &dest_path,
                    approved_fingerprint: "pending",
                    collision_state,
                    ordinal: item_ordinal,
                },
            )
            .await
            .map_err(app_core_errors::db_err)?;

            hasher.update(entry_public_id.as_bytes());
            hasher.update(dest_path.as_bytes());
            hasher.update(collision_state.as_bytes());
            hasher.update(b"|");

            item_ordinal += 1;
        }

        total_sessions += 1;
        total_items += sess_items;
        total_frames += sess_items; // frame count == item count for additive plans
        total_bytes += sess_bytes;
    }

    let digest = format!("sha256:{}", hex::encode(hasher.finalize()));

    repo::update_plan_counts(
        conn,
        plan_row_id,
        total_sessions,
        total_items,
        total_frames,
        total_bytes,
        remaining,
        next_session_row_id,
        &digest,
    )
    .await
    .map_err(app_core_errors::db_err)?;

    let next_session_id = match next_session_row_id {
        Some(rid) => {
            Some(repo::session_public_id(conn, rid).await.map_err(app_core_errors::db_err)?)
        }
        None => None,
    };
    let target_rev_id =
        repo::revision_public_id(conn, mem_rev_row_id).await.map_err(app_core_errors::db_err)?;
    let base_snapshot_id = match project.materialization_head_snapshot_row_id {
        Some(rid) => {
            Some(repo::snapshot_public_id(conn, rid).await.map_err(app_core_errors::db_err)?)
        }
        None => None,
    };

    Ok(UpdateViewPlan {
        plan_id: plan_public_id,
        state: contract_state("draft").to_owned(),
        project_id: req.project_id.to_owned(),
        base_snapshot_id,
        target_membership_revision_id: target_rev_id,
        plan_digest: digest,
        session_count: total_sessions,
        item_count: total_items,
        source_frame_count: total_frames,
        source_byte_count: total_bytes,
        conflict_count: 0, // computed at query time
        remaining_session_count: remaining,
        next_session_id,
        created_at: now.to_owned(),
        created_by: req.actor_id.to_owned(),
    })
}
