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

//! `project.update_view.discard` use case (spec 062 / contract).

use sqlx::SqlitePool;

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use persistence_plans::repositories::update_view as repo;
use persistence_sessions::repositories::tx;

pub struct DiscardUpdateViewRequest<'a> {
    pub plan_id: &'a str,
    pub actor_id: &'a str,
    pub command_id: &'a str,
}

pub async fn discard_update_view(
    pool: &SqlitePool,
    req: &DiscardUpdateViewRequest<'_>,
) -> Result<(), ContractError> {
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;
    tx::begin_immediate(&mut conn).await.map_err(app_core_errors::db_err)?;

    let result = discard_inner(&mut *conn, req).await;

    match result {
        Ok(()) => {
            tx::commit(&mut conn).await.map_err(app_core_errors::db_err)?;
            Ok(())
        }
        Err(e) => {
            tx::rollback(&mut conn).await;
            Err(e)
        }
    }
}

async fn discard_inner(
    conn: &mut sqlx::SqliteConnection,
    req: &DiscardUpdateViewRequest<'_>,
) -> Result<(), ContractError> {
    let plan = repo::get_plan_by_public_id(conn, req.plan_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => ContractError::new(
            ErrorCode::ProjectUpdateViewPlanNotFound,
            format!("plan {} not found", req.plan_id),
            ErrorSeverity::Blocking,
            false,
        ),
        other => app_core_errors::db_err(other),
    })?;

    if plan.state != "draft" && plan.state != "stale" {
        return Err(ContractError::new(
            ErrorCode::ProjectUpdateViewPlanNotOpen,
            format!(
                "plan {} is in state '{}'; only open or stale plans may be discarded",
                req.plan_id, plan.state
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    repo::transition_plan_state(conn, plan.row_id, &plan.state, "discarded")
        .await
        .map_err(app_core_errors::db_err)?;

    Ok(())
}
