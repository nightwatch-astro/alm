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
use persistence_core::DbError;
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
                leave_applying(pool, plan.row_id, "stopped").await;
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
            leave_applying(pool, plan.row_id, "failed").await;
            Err(e)
        }
    }
}

/// Where a plan ended up after an error path tried to move it out of `applying`.
#[derive(Debug, PartialEq, Eq)]
enum LeftApplying {
    /// The transition committed.
    Transitioned,
    /// Another writer moved the plan out of `applying` first, so there is
    /// nothing to correct. `cancel_update_view` does this to a running plan.
    /// `observed` is the state now on the row, or `None` when the row is gone.
    AlreadyLeft { observed: Option<String> },
    /// The row is still `applying` and nothing remains that will move it.
    Stuck,
}

/// Move a plan out of `applying` on an error path.
///
/// The caller owes the user the original failure, so this cannot return an error
/// of its own. `transition_plan_state` is the only mechanism that moves a plan
/// out of `applying`, so a plan left in that state after this call is
/// indistinguishable at boot from a mutation still in flight, and the log line
/// is its only trace. The returned value is what tests assert on.
async fn leave_applying(pool: &SqlitePool, plan_row_id: i64, new_state: &str) -> LeftApplying {
    let Err(e) = try_leave_applying(pool, plan_row_id, new_state).await else {
        return LeftApplying::Transitioned;
    };

    let outcome = classify_failure_to_leave(pool, plan_row_id, &e).await;
    match &outcome {
        LeftApplying::AlreadyLeft { observed } => tracing::info!(
            plan_row_id,
            new_state,
            observed = observed.as_deref().unwrap_or("<row gone>"),
            "another writer moved the plan out of 'applying' first"
        ),
        LeftApplying::Stuck => tracing::error!(
            plan_row_id,
            new_state,
            error = %e,
            "plan is stuck in 'applying': nothing remains that will move it"
        ),
        LeftApplying::Transitioned => {}
    }
    outcome
}

/// A failure to transition is only a stuck plan when the row is still
/// `applying`; a lost race against `cancel_update_view` is not.
async fn classify_failure_to_leave(
    pool: &SqlitePool,
    plan_row_id: i64,
    error: &DbError,
) -> LeftApplying {
    let Ok(mut conn) = pool.acquire().await else {
        return LeftApplying::Stuck;
    };
    match repo::plan_state(&mut conn, plan_row_id).await {
        Ok(None) => LeftApplying::AlreadyLeft { observed: None },
        Ok(Some(state)) if state != "applying" => {
            LeftApplying::AlreadyLeft { observed: Some(state) }
        }
        Ok(Some(_)) => LeftApplying::Stuck,
        Err(read_error) => {
            tracing::error!(
                plan_row_id,
                transition_error = %error,
                read_error = %read_error,
                "could not read plan state after a failed transition out of 'applying'"
            );
            LeftApplying::Stuck
        }
    }
}

async fn try_leave_applying(
    pool: &SqlitePool,
    plan_row_id: i64,
    new_state: &str,
) -> Result<(), DbError> {
    let mut conn = pool.acquire().await?;
    tx::enable_foreign_keys(&mut conn).await?;
    tx::begin_immediate(&mut conn).await?;
    if let Err(e) =
        repo::transition_plan_state(&mut *conn, plan_row_id, "applying", new_state).await
    {
        tx::rollback(&mut conn).await;
        return Err(e);
    }
    tx::commit(&mut conn).await
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

#[cfg(test)]
mod tests {
    use super::LeftApplying;
    use persistence_core::{test_support::setup_db, Database, DbError};
    use persistence_topology::test_support as support;

    /// Insert a `materialization_update_plan` row in `state`, returning its row id.
    async fn plan_in_state(db: &Database, state: &str) -> i64 {
        let pool = db.pool();
        let project_row_id = support::insert_spec062_project(pool, "proj-leave").await;
        let actor_row_id = support::insert_actor(pool, "actor-leave").await;
        let seq = support::insert_sequence(pool).await;

        let revision_row_id: i64 = sqlx::query_scalar(
            "INSERT INTO project_membership_revision
                 (public_id, project_row_id, revision_number, actor_row_id,
                  created_sequence, created_at)
             VALUES ('rev-leave', ?, 1, ?, ?, '2026-07-22T00:00:00.000000Z')
             RETURNING row_id",
        )
        .bind(project_row_id)
        .bind(actor_row_id)
        .bind(seq)
        .fetch_one(pool)
        .await
        .unwrap();

        sqlx::query_scalar(
            "INSERT INTO materialization_update_plan
                 (public_id, project_row_id, target_membership_revision_row_id, state,
                  content_digest, session_count, item_count, source_frame_count,
                  source_byte_count, remaining_session_count, actor_row_id,
                  created_sequence, created_at)
             VALUES ('plan-leave', ?, ?, ?, 'digest', 0, 0, 0, 0, 0, ?, ?, \
                     '2026-07-22T00:00:00.000000Z')
             RETURNING row_id",
        )
        .bind(project_row_id)
        .bind(revision_row_id)
        .bind(state)
        .bind(actor_row_id)
        .bind(seq)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn leaving_applying_transitions_a_plan_that_is_still_applying() {
        let db = setup_db().await;
        let plan_row_id = plan_in_state(&db, "applying").await;

        assert_eq!(
            super::leave_applying(db.pool(), plan_row_id, "stopped").await,
            LeftApplying::Transitioned
        );

        let mut conn = db.pool().acquire().await.unwrap();
        assert_eq!(
            super::repo::plan_state(&mut conn, plan_row_id).await.unwrap().as_deref(),
            Some("stopped")
        );
    }

    /// `cancel_update_view` moves a running plan out of `applying` while the
    /// loop is still working, so the loop's own transition loses the race. That
    /// plan is not stuck and must not be reported as stuck.
    #[tokio::test]
    async fn a_plan_another_writer_already_moved_is_not_stuck() {
        let db = setup_db().await;
        let plan_row_id = plan_in_state(&db, "stopped").await;

        assert_eq!(
            super::leave_applying(db.pool(), plan_row_id, "stopped").await,
            LeftApplying::AlreadyLeft { observed: Some("stopped".to_owned()) }
        );
    }

    #[tokio::test]
    async fn a_plan_row_that_is_gone_is_not_stuck() {
        let db = setup_db().await;

        assert_eq!(
            super::leave_applying(db.pool(), 999, "stopped").await,
            LeftApplying::AlreadyLeft { observed: None }
        );
    }

    #[tokio::test]
    async fn the_transition_result_is_not_discarded() {
        let db = setup_db().await;

        let err = super::try_leave_applying(db.pool(), 999, "stopped")
            .await
            .expect_err("a transition that matched no row must not read as success");

        assert!(matches!(err, DbError::CasFailed(_)), "expected a CAS failure, got {err}");
    }
}
