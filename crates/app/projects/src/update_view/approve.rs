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

//! `project.update_view.approve` use case (spec 062 / contract).

use sqlx::SqlitePool;

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use persistence_plans::repositories::update_view as repo;
use persistence_sessions::repositories::tx;

use super::query::{contract_state, UpdateViewPlan};

#[derive(Debug)]
pub struct ApproveUpdateViewRequest<'a> {
    pub plan_id: &'a str,
    pub approval_digest: &'a str,
    pub actor_id: &'a str,
    pub command_id: &'a str,
}

#[derive(Debug)]
pub struct ApproveUpdateViewResponse {
    pub plan: UpdateViewPlan,
}

pub async fn approve_update_view(
    pool: &SqlitePool,
    req: &ApproveUpdateViewRequest<'_>,
) -> Result<ApproveUpdateViewResponse, ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;
    tx::begin_immediate(&mut conn).await.map_err(app_core_errors::db_err)?;

    let result = approve_inner(&mut *conn, req).await;

    match result {
        Ok(plan) => {
            tx::commit(&mut conn).await.map_err(app_core_errors::db_err)?;
            Ok(ApproveUpdateViewResponse { plan })
        }
        Err(e) => {
            tx::rollback(&mut conn).await;
            Err(e)
        }
    }
}

async fn approve_inner(
    conn: &mut sqlx::SqliteConnection,
    req: &ApproveUpdateViewRequest<'_>,
) -> Result<UpdateViewPlan, ContractError> {
    let plan = repo::get_plan_by_public_id(conn, req.plan_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => ContractError::new(
            ErrorCode::ProjectUpdateViewPlanNotFound,
            format!("plan {} not found", req.plan_id),
            ErrorSeverity::Blocking,
            false,
        ),
        other => app_core_errors::db_err(other),
    })?;

    if plan.state != "draft" {
        return Err(ContractError::new(
            ErrorCode::ProjectUpdateViewPlanNotOpen,
            format!("plan {} is in state '{}', not open", req.plan_id, contract_state(&plan.state)),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let conflicts =
        repo::count_conflicts(conn, plan.row_id).await.map_err(app_core_errors::db_err)?;
    if conflicts > 0 {
        return Err(ContractError::new(
            ErrorCode::ProjectUpdateViewPathConflict,
            format!("plan {} has {conflicts} path conflicts; resolve before approval", req.plan_id),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    if req.approval_digest != plan.content_digest {
        return Err(ContractError::new(
            ErrorCode::ProjectUpdateViewPlanDigestMismatch,
            format!("plan {} approval digest mismatch", req.plan_id),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    repo::transition_plan_state(conn, plan.row_id, "draft", "approved").await.map_err(
        |e| match e {
            persistence_core::DbError::CasFailed(msg) => ContractError::new(
                ErrorCode::ProjectUpdateViewPlanStale,
                msg,
                ErrorSeverity::Blocking,
                false,
            ),
            other => app_core_errors::db_err(other),
        },
    )?;

    let project_pub = repo::project_public_id(conn, plan.project_row_id)
        .await
        .map_err(app_core_errors::db_err)?;
    let base_snapshot_id = match plan.base_snapshot_row_id {
        Some(rid) => {
            Some(repo::snapshot_public_id(conn, rid).await.map_err(app_core_errors::db_err)?)
        }
        None => None,
    };
    let target_rev_id = repo::revision_public_id(conn, plan.target_membership_revision_row_id)
        .await
        .map_err(app_core_errors::db_err)?;
    let next_session_id = match plan.next_session_row_id {
        Some(rid) => {
            Some(repo::session_public_id(conn, rid).await.map_err(app_core_errors::db_err)?)
        }
        None => None,
    };
    let actor_pub =
        repo::actor_public_id(conn, plan.actor_row_id).await.map_err(app_core_errors::db_err)?;

    Ok(UpdateViewPlan {
        plan_id: plan.public_id,
        state: contract_state("approved").to_owned(),
        project_id: project_pub,
        base_snapshot_id,
        target_membership_revision_id: target_rev_id,
        plan_digest: plan.content_digest,
        session_count: plan.session_count,
        item_count: plan.item_count,
        source_frame_count: plan.source_frame_count,
        source_byte_count: plan.source_byte_count,
        conflict_count: 0,
        remaining_session_count: plan.remaining_session_count,
        next_session_id,
        created_at: plan.created_at,
        created_by: actor_pub,
    })
}
