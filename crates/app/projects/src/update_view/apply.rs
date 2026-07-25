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

//! `project.update_view.apply` use case (spec 062 FR-099–FR-100 / contract).

use sqlx::SqlitePool;

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::Timestamp;
use persistence_plans::repositories::update_view as repo;
use persistence_sessions::repositories::{change_sequence, tx};

use super::installer::{run_install, InstallItem, InstallerCallbacks};

pub struct ApplyUpdateViewRequest<'a> {
    pub plan_id: &'a str,
    pub approval_digest: &'a str,
    pub actor_id: &'a str,
    pub command_id: &'a str,
}

pub struct ApplyUpdateViewResponse {
    pub operation_id: String,
    pub plan_id: String,
    pub state: String,
}

pub async fn apply_update_view(
    pool: &SqlitePool,
    req: &ApplyUpdateViewRequest<'_>,
) -> Result<ApplyUpdateViewResponse, ContractError> {
    let now = Timestamp::now_iso();
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;
    tx::begin_immediate(&mut conn).await.map_err(app_core_errors::db_err)?;

    let result = apply_inner(&mut *conn, req, &now).await;

    match result {
        Ok(resp) => {
            tx::commit(&mut conn).await.map_err(app_core_errors::db_err)?;
            Ok(resp)
        }
        Err(e) => {
            tx::rollback(&mut conn).await;
            Err(e)
        }
    }
}

async fn apply_inner(
    conn: &mut sqlx::SqliteConnection,
    req: &ApplyUpdateViewRequest<'_>,
    now: &str,
) -> Result<ApplyUpdateViewResponse, ContractError> {
    let plan = repo::get_plan_by_public_id(conn, req.plan_id).await.map_err(|e| match e {
        persistence_core::DbError::NotFound(_) => ContractError::new(
            ErrorCode::ProjectUpdateViewPlanNotFound,
            format!("plan {} not found", req.plan_id),
            ErrorSeverity::Blocking,
            false,
        ),
        other => app_core_errors::db_err(other),
    })?;

    if plan.state != "approved" && plan.state != "stopped" {
        return Err(ContractError::new(
            ErrorCode::ProjectUpdateViewPlanNotApproved,
            format!("plan {} state '{}' is not approved or stopped", req.plan_id, plan.state),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    if req.approval_digest != plan.content_digest {
        return Err(ContractError::new(
            ErrorCode::ProjectUpdateViewPlanDigestMismatch,
            format!("plan {} apply digest mismatch", req.plan_id),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let expected = if plan.state == "stopped" { "stopped" } else { "approved" };
    repo::transition_plan_state(conn, plan.row_id, expected, "applying").await.map_err(
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

    let actor_row_id =
        repo::ensure_actor(conn, req.actor_id, now).await.map_err(app_core_errors::db_err)?;
    repo::ensure_command(conn, req.command_id, actor_row_id, "project.update_view.apply", now)
        .await
        .map_err(app_core_errors::db_err)?;

    Ok(ApplyUpdateViewResponse {
        operation_id: req.command_id.to_owned(),
        plan_id: plan.public_id,
        state: "applying".to_owned(),
    })
}

/// Drive the install loop synchronously (for tests / Tauri adapter).
/// Drive the install loop synchronously (for tests / Tauri adapter).
///
/// `path_resolver(frame_row_id, destination_root_row_id)` returns the resolved
/// absolute source path and destination root path for a plan item. The Tauri
/// adapter supplies these from the registered library-root and destination-root
/// paths. Tests may supply a closure that returns real temp-dir paths.
pub async fn run_apply_loop(
    pool: &SqlitePool,
    plan_id: &str,
    operation_command_id: &str,
    lease_owner: &str,
    lease_generation: i64,
    callbacks: &impl InstallerCallbacks,
    path_resolver: &(impl Fn(i64, i64) -> (camino::Utf8PathBuf, camino::Utf8PathBuf) + Sync),
) -> Result<(), ContractError> {
    let now = Timestamp::now_iso();
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;

    let plan =
        repo::get_plan_by_public_id(&mut *conn, plan_id).await.map_err(app_core_errors::db_err)?;

    if plan.state != "applying" {
        return Err(ContractError::new(
            ErrorCode::ProjectUpdateViewPlanNotApproved,
            format!("plan {plan_id} is not in applying state"),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let entries = repo::list_plan_entries(&mut *conn, plan.row_id, None, plan.item_count + 1)
        .await
        .map_err(app_core_errors::db_err)?;
    let journaled = repo::list_journal_entries(&mut *conn, plan.row_id)
        .await
        .map_err(app_core_errors::db_err)?;
    let journaled_ids: std::collections::HashSet<i64> =
        journaled.iter().map(|(id, _, _, _)| *id).collect();
    drop(conn);

    let mut installed: Vec<(i64, i64, String, String)> = Vec::new();

    for entry in &entries {
        if journaled_ids.contains(&entry.row_id) {
            if let Some(j) = journaled.iter().find(|(id, _, _, _)| *id == entry.row_id) {
                installed.push((entry.row_id, j.1, j.2.clone(), j.3.clone()));
            }
            continue;
        }

        let (source_abs_path, dest_root_abs_path) =
            path_resolver(entry.frame_row_id, entry.destination_root_row_id);
        let item = InstallItem {
            item_row_id: entry.row_id,
            item_public_id: entry.public_id.clone(),
            session_row_id: entry.session_row_id,
            frame_row_id: entry.frame_row_id,
            destination_root_row_id: entry.destination_root_row_id,
            destination_relative_path: entry.relative_path.clone(),
            approved_fingerprint: entry.approved_fingerprint.clone(),
            ordinal: entry.ordinal,
            source_abs_path,
            dest_root_abs_path,
        };

        match run_install(
            pool,
            plan.row_id,
            &item,
            operation_command_id,
            lease_owner,
            lease_generation,
            callbacks,
        )
        .await
        {
            Ok((entry_row_id, fp)) => {
                installed.push((entry.row_id, entry_row_id, entry.relative_path.clone(), fp));
            }
            Err(e) => {
                let mut c =
                    pool.acquire().await.map_err(|e2| app_core_errors::db_err(e2.into()))?;
                tx::enable_foreign_keys(&mut c).await.map_err(app_core_errors::db_err)?;
                tx::begin_immediate(&mut c).await.map_err(app_core_errors::db_err)?;
                let _ =
                    repo::transition_plan_state(&mut *c, plan.row_id, "applying", "stopped").await;
                let _ = tx::commit(&mut c).await;
                return Err(e);
            }
        }
    }

    // Finalize snapshot in one transaction.
    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;
    tx::begin_immediate(&mut conn).await.map_err(app_core_errors::db_err)?;

    let finalize_result = finalize_snapshot(&mut *conn, &plan, &installed, &now).await;

    match finalize_result {
        Ok(()) => {
            tx::commit(&mut conn).await.map_err(app_core_errors::db_err)?;
            Ok(())
        }
        Err(e) => {
            tx::rollback(&mut conn).await;
            let mut c = pool.acquire().await.map_err(|e2| app_core_errors::db_err(e2.into()))?;
            tx::enable_foreign_keys(&mut c).await.map_err(app_core_errors::db_err)?;
            tx::begin_immediate(&mut c).await.map_err(app_core_errors::db_err)?;
            let _ = repo::transition_plan_state(&mut *c, plan.row_id, "applying", "failed").await;
            let _ = tx::commit(&mut c).await;
            Err(e)
        }
    }
}

async fn finalize_snapshot(
    conn: &mut sqlx::SqliteConnection,
    plan: &repo::UpdatePlanRow,
    installed: &[(i64, i64, String, String)],
    now: &str,
) -> Result<(), ContractError> {
    let project_pub = repo::project_public_id(conn, plan.project_row_id)
        .await
        .map_err(app_core_errors::db_err)?;
    let proj = repo::get_project(conn, &project_pub).await.map_err(app_core_errors::db_err)?;

    let seq = change_sequence::insert_repository_change(conn, None, now)
        .await
        .map_err(app_core_errors::db_err)?;

    let base_sess_count = repo::count_base_snapshot_sessions(conn, plan.base_snapshot_row_id)
        .await
        .map_err(app_core_errors::db_err)?;

    let snap_id = uuid::Uuid::new_v4().to_string();
    let snap_row_id = repo::insert_mat_snapshot(
        conn,
        repo::InsertMatSnapshot {
            public_id: &snap_id,
            project_row_id: plan.project_row_id,
            membership_revision_row_id: plan.target_membership_revision_row_id,
            predecessor_snapshot_row_id: plan.base_snapshot_row_id,
            applied_plan_row_id: plan.row_id,
            entry_count: installed.len() as i64,
            session_count: base_sess_count + plan.session_count,
            created_sequence: seq,
            created_at: now,
        },
    )
    .await
    .map_err(app_core_errors::db_err)?;

    for (ordinal, (item_row_id, _entry_row_id_hint, relative_path, fingerprint)) in
        installed.iter().enumerate()
    {
        let (session_row_id, frame_row_id, dest_root_row_id) =
            repo::get_plan_item_details(conn, *item_row_id)
                .await
                .map_err(app_core_errors::db_err)?;

        let entry_pub = uuid::Uuid::new_v4().to_string();
        let entry_row_id = repo::insert_materialized_entry(
            conn,
            repo::InsertMaterializedEntry {
                public_id: &entry_pub,
                project_row_id: plan.project_row_id,
                first_snapshot_row_id: snap_row_id,
                source_session_row_id: session_row_id,
                source_frame_row_id: frame_row_id,
                destination_root_row_id: dest_root_row_id,
                relative_path,
                content_fingerprint: Some(fingerprint.as_str()),
                created_by_plan_row_id: plan.row_id,
                created_sequence: seq,
                created_at: now,
            },
        )
        .await
        .map_err(app_core_errors::db_err)?;

        repo::insert_snapshot_entry(conn, snap_row_id, entry_row_id, ordinal as i64)
            .await
            .map_err(app_core_errors::db_err)?;
    }

    // Copy base sessions + add new plan sessions to snapshot.
    let mut sess_ordinal: i64 = 0;
    if let Some(base_snap) = plan.base_snapshot_row_id {
        for s in repo::list_base_snapshot_sessions(conn, base_snap)
            .await
            .map_err(app_core_errors::db_err)?
        {
            repo::insert_snapshot_session(conn, snap_row_id, s, sess_ordinal)
                .await
                .map_err(app_core_errors::db_err)?;
            sess_ordinal += 1;
        }
    }
    for s in
        repo::list_plan_session_row_ids(conn, plan.row_id).await.map_err(app_core_errors::db_err)?
    {
        repo::insert_snapshot_session(conn, snap_row_id, s, sess_ordinal)
            .await
            .map_err(app_core_errors::db_err)?;
        sess_ordinal += 1;
    }

    repo::advance_materialization_head(
        conn,
        plan.project_row_id,
        snap_row_id,
        proj.materialization_head_generation,
        seq,
    )
    .await
    .map_err(app_core_errors::db_err)?;

    repo::transition_plan_state(conn, plan.row_id, "applying", "applied")
        .await
        .map_err(app_core_errors::db_err)?;

    Ok(())
}
