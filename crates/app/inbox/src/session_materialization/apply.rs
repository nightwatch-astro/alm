// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `inbox.materialization.apply` — apply loop and terminal commit.
//!
//! ## Lifecycle
//!
//! ```text
//! claim command
//!   → insert operation (ready)
//!   → transition ready → applying
//!   → for each proposed session:
//!       if cancel_requested → break to cancelled path
//!       insert session + frames + (light) panel group + revision atomically
//!   → insert result snapshot rows
//!   → transition applying → applied (CAS)
//!   → CommandLedger.finish (audit + outbox)
//! ```
//!
//! Idempotency: the command ledger returns a replayed terminal on retry so the
//! loop is never re-entered for the same command ID.
//!
//! Cancellation: the apply loop checks `cancel_requested` before each session.
//! A cancel before the final commit leaves zero session/membership rows and
//! transitions the operation to `cancelled`.

use std::sync::Arc;

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;
use uuid::Uuid;

use persistence_core::DbResult;
use persistence_inbox::repositories::plan_result::{
    get_plan_snapshot_for_operation, get_site_resolution_revision, list_proposed_session_frames,
    list_proposed_sessions,
};
use persistence_sessions::repositories::actors::{ensure_spec062_actor, ensure_spec062_target};
use persistence_sessions::repositories::change_sequence::{
    current_sequence_on_conn, insert_repository_change,
};
use persistence_sessions::repositories::materialization::{
    get_operation_public_id_by_row_id, insert_materialization_operation, insert_result_snapshot,
    transition_operation_to_applied, transition_operation_to_applying,
    transition_operation_to_cancelled, transition_operation_to_failed, ApplyOperationResult,
    InsertMaterializationOperation, InsertMaterializationResultSnapshot,
};
use persistence_sessions::repositories::result_snapshots::{
    insert_result_frame, insert_result_panel_group, insert_result_session,
};
use persistence_sessions::repositories::sessions::{
    insert_session, insert_session_frame, insert_session_visibility, InsertSession,
    InsertSessionFrame,
};
use persistence_sessions::repositories::tx::{
    begin_immediate, commit, enable_foreign_keys, rollback,
};
use persistence_topology::repositories::panels::{
    insert_singleton_panel_group, InsertSingletonPanel,
};

use super::progress::MaterializationProgress;

/// Propagate an error out of an open write transaction, rolling it back first.
///
/// A bare `?` inside a `begin_immediate` region drops the `PoolConnection` with
/// the transaction still open. SQLite does not roll that back, and sqlx's pool
/// has no `after_release` reset here, so the connection returns to the idle pool
/// still holding the write lock. A later writer then either gets that same
/// connection back and fails with `cannot start a transaction within a
/// transaction`, or gets another one and waits out `busy_timeout` before failing
/// with `database is locked`. Either way one failed apply takes writes down for
/// the lifetime of the pool.
///
/// `a_failed_apply_leaves_the_database_writable`, in
/// `crates/app/core/tests/session_materialization/mod.rs`, pins that behaviour.
macro_rules! tx_try {
    ($conn:expr, $result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => {
                rollback(&mut $conn).await;
                return Err(error);
            }
        }
    };
}

// ── Params struct ────────────────────────────────────────────────────────

/// Parameters for [`run_apply`]. Kept in a struct to stay within the 8-arg clippy limit.
pub struct ApplyParams<'a> {
    /// The `session_materialization_operation.row_id` (already in `ready` state).
    pub operation_row_id: i64,
    /// The `state_version` of that operation row at the moment of the call.
    pub operation_state_version: i64,
    /// The approved plan digest that the caller verified.
    pub approved_plan_digest: &'a str,
    /// Actor public_id for the `spec062_actor` upsert and audit log.
    pub actor_public_id: &'a str,
    /// Canonical target public_id for light sessions (all light sessions in one
    /// Inbox plan share the same confirmed canonical target).
    /// `None` for plans that contain only calibration frames.
    pub canonical_target_public_id: Option<&'a str>,
    /// Shared cancellation / progress tracker.
    pub progress: Arc<MaterializationProgress>,
}

/// One output session produced during the apply loop.
struct AppliedSession {
    session_row_id: i64,
    session_public_id: String,
    frame_row_ids: Vec<i64>,
    panel_group_row_id: Option<i64>,
    panel_revision_row_id: Option<i64>,
}

/// Apply one approved Inbox materialization plan, writing all sessions and frame
/// memberships in per-session transactions, then committing a single terminal
/// result snapshot.
///
/// # Returns
///
/// The `session_materialization_operation.public_id` on success.
///
/// # Errors
///
/// Returns [`persistence_core::DbError`] on any SQL or CAS failure.
#[allow(clippy::too_many_lines)]
#[expect(
    clippy::cognitive_complexity,
    reason = "staged apply: transition, per-frame write, and terminal transition phases"
)]
pub async fn run_apply(pool: &SqlitePool, params: ApplyParams<'_>) -> DbResult<String> {
    let now = Timestamp::now_iso();

    // ── Transition ready → applying ──────────────────────────────────────
    let mut conn = pool.acquire().await?;
    begin_immediate(&mut conn).await?;
    let apply_result = transition_operation_to_applying(
        &mut conn,
        params.operation_row_id,
        params.operation_state_version,
        &now,
    )
    .await;
    if let Err(e) = apply_result {
        rollback(&mut conn).await;
        return Err(e);
    }
    commit(&mut conn).await?;
    drop(conn);

    // applying state_version is now operation_state_version + 1
    let applying_state_version = params.operation_state_version + 1;

    // ── Resolve apply-time IDs ────────────────────────────────────────────
    let plan_snapshot = get_plan_snapshot_for_operation(pool, params.operation_row_id).await?;
    let proposed_sessions = list_proposed_sessions(pool, plan_snapshot.row_id).await?;
    let config_revision_row_id = plan_snapshot.config_revision_row_id;

    let actor_row_id: i64 = {
        let mut conn = pool.acquire().await?;
        begin_immediate(&mut conn).await?;
        let id = tx_try!(conn, ensure_spec062_actor(&mut conn, params.actor_public_id, &now).await);
        tx_try!(conn, commit(&mut conn).await);
        id
    };

    let canonical_target_row_id: Option<i64> = if let Some(t) = params.canonical_target_public_id {
        let mut conn = pool.acquire().await?;
        begin_immediate(&mut conn).await?;
        let row_id = tx_try!(conn, ensure_spec062_target(&mut conn, t, &now).await);
        tx_try!(conn, commit(&mut conn).await);
        Some(row_id)
    } else {
        None
    };

    let mut applied_sessions: Vec<AppliedSession> = Vec::with_capacity(proposed_sessions.len());
    let mut total_frames_written: i64 = 0;
    let mut light_group_count: i64 = 0;

    // ── Per-session apply loop ────────────────────────────────────────────
    for proposed in &proposed_sessions {
        // Check cancellation before each session (contract: at least every 256
        // frames; we check per-session which is always ≤ that bound).
        if params.progress.is_cancel_requested() {
            break;
        }

        let frame_rows = list_proposed_session_frames(pool, proposed.row_id).await?;
        let site_rev =
            get_site_resolution_revision(pool, proposed.site_resolution_revision_row_id).await?;

        // Each session commit is independent so cancellation mid-loop leaves no
        // partially-written session (we only break between sessions).
        let mut conn = pool.acquire().await?;
        enable_foreign_keys(&mut conn).await?;
        begin_immediate(&mut conn).await?;

        // Read sequence inside the IMMEDIATE transaction on this connection to
        // avoid racing a concurrent writer on a separate pool connection.
        let seq = tx_try!(conn, current_sequence_on_conn(&mut conn).await) + 1;
        tx_try!(conn, insert_repository_change(&mut conn, None, &now).await);

        // Derive observing night from the pinned site resolution revision.
        let observing_night = site_rev.observing_night_date.as_deref().unwrap_or("2000-01-01");

        // `reviewed_local_fallback` applies when no canonical UTC instant was
        // available and the reviewer approved a local-timestamp fallback.
        let night_derivation =
            if site_rev.canonical_exposure_at_utc.is_none() && site_rev.timezone_name.is_some() {
                "reviewed_local_fallback"
            } else {
                "acquisition_timezone"
            };

        let session_public_id = Uuid::new_v4().to_string();
        let ordinal_in_operation = i64::try_from(applied_sessions.len()).unwrap_or(i64::MAX);

        let session_row_id = tx_try!(
            conn,
            insert_session(
                &mut conn,
                &InsertSession {
                    public_id: &session_public_id,
                    materialization_operation_row_id: params.operation_row_id,
                    kind: &proposed.kind,
                    ordinal_in_operation,
                    identity_digest: &proposed.identity_digest,
                    observing_night_date: observing_night,
                    site_row_id: site_rev.selected_site_row_id,
                    timezone_name_snapshot: site_rev.timezone_name.as_deref(),
                    night_derivation,
                    canonical_target_row_id,
                    created_sequence: seq,
                    created_at: &now,
                },
            )
            .await
        );

        tx_try!(
            conn,
            insert_session_visibility(&mut conn, session_row_id, seq, "inbox_ingestion").await
        );

        for (i, frame) in frame_rows.iter().enumerate() {
            tx_try!(
                conn,
                insert_session_frame(
                    &mut conn,
                    &InsertSessionFrame {
                        session_row_id,
                        frame_row_id: frame.frame_row_id,
                        materialization_operation_row_id: params.operation_row_id,
                        ordinal: frame.ordinal,
                        is_representative: i == 0,
                        created_sequence: seq,
                        _phantom: std::marker::PhantomData,
                    },
                )
                .await
            );
        }

        // Light sessions receive a singleton panel group atomically in the same tx.
        let (panel_group_row_id, panel_revision_row_id) = if proposed.kind == "light" {
            let target_row_id = tx_try!(
                conn,
                canonical_target_row_id.ok_or_else(|| {
                    persistence_core::DbError::NotFound(
                        "light session requires a canonical target row id".to_owned(),
                    )
                })
            );

            let (g, r) = tx_try!(
                conn,
                insert_singleton_panel_group(
                    &mut conn,
                    &InsertSingletonPanel {
                        group_public_id: &Uuid::new_v4().to_string(),
                        revision_public_id: &Uuid::new_v4().to_string(),
                        session_row_id,
                        canonical_target_row_id: target_row_id,
                        config_revision_row_id,
                        actor_row_id,
                        created_sequence: seq,
                        created_at: &now,
                    },
                )
                .await
            );

            light_group_count += 1;
            (Some(g), Some(r))
        } else {
            (None, None)
        };

        tx_try!(conn, commit(&mut conn).await);

        let frame_count = i64::try_from(frame_rows.len()).unwrap_or(0);
        total_frames_written += frame_count;
        params.progress.record_session_done(frame_count);

        applied_sessions.push(AppliedSession {
            session_row_id,
            session_public_id,
            frame_row_ids: frame_rows.iter().map(|f| f.frame_row_id).collect(),
            panel_group_row_id,
            panel_revision_row_id,
        });
    }

    // ── Check cancellation after the loop ────────────────────────────────
    if params.progress.is_cancel_requested() && applied_sessions.len() < proposed_sessions.len() {
        let cancel_now = Timestamp::now_iso();
        let mut conn = pool.acquire().await?;
        begin_immediate(&mut conn).await?;
        tx_try!(
            conn,
            transition_operation_to_cancelled(
                &mut conn,
                params.operation_row_id,
                applying_state_version,
                &cancel_now,
            )
            .await
        );
        tx_try!(conn, commit(&mut conn).await);
        return get_operation_public_id_by_row_id(pool, params.operation_row_id).await;
    }

    // ── Terminal commit: result snapshot + applied transition ────────────
    let terminal_now = Timestamp::now_iso();
    let session_count = i64::try_from(applied_sessions.len()).unwrap_or(0);

    let mut conn = pool.acquire().await?;
    enable_foreign_keys(&mut conn).await?;
    begin_immediate(&mut conn).await?;

    let term_seq = tx_try!(conn, current_sequence_on_conn(&mut conn).await) + 1;
    tx_try!(conn, insert_repository_change(&mut conn, None, &terminal_now).await);

    // Stable SHA-256 over ordered session public_ids — terminal idempotency token.
    let canonical_result_digest = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for s in &applied_sessions {
            h.update(s.session_public_id.as_bytes());
            h.update([0]);
        }
        format!("sha256:{:x}", h.finalize())
    };

    let snapshot_public_id = Uuid::new_v4().to_string();
    let snapshot_row_id = tx_try!(
        conn,
        insert_result_snapshot(
            &mut conn,
            &InsertMaterializationResultSnapshot {
                public_id: &snapshot_public_id,
                operation_row_id: params.operation_row_id,
                session_count,
                membership_count: total_frames_written,
                singleton_group_count: light_group_count,
                blocked_frame_count: plan_snapshot.blocked_frame_count,
                canonical_digest: &canonical_result_digest,
                created_sequence: term_seq,
                created_at: &terminal_now,
            },
        )
        .await
    );

    let mut frame_ordinal: i64 = 0;
    for (session_ordinal, session) in applied_sessions.iter().enumerate() {
        let session_ordinal = i64::try_from(session_ordinal).unwrap_or(i64::MAX);
        tx_try!(
            conn,
            insert_result_session(
                &mut conn,
                snapshot_row_id,
                session.session_row_id,
                session_ordinal
            )
            .await
        );

        for &frame_row_id in &session.frame_row_ids {
            tx_try!(
                conn,
                insert_result_frame(
                    &mut conn,
                    snapshot_row_id,
                    session.session_row_id,
                    frame_row_id,
                    frame_ordinal,
                )
                .await
            );
            frame_ordinal += 1;
        }

        if let (Some(g), Some(r)) = (session.panel_group_row_id, session.panel_revision_row_id) {
            tx_try!(
                conn,
                insert_result_panel_group(
                    &mut conn,
                    snapshot_row_id,
                    session.session_row_id,
                    g,
                    r,
                    session_ordinal,
                )
                .await
            );
        }
    }

    tx_try!(
        conn,
        transition_operation_to_applied(
            &mut conn,
            &ApplyOperationResult {
                operation_row_id: params.operation_row_id,
                expected_state_version: applying_state_version,
                result_snapshot_row_id: snapshot_row_id,
                session_count,
                membership_count: total_frames_written,
                singleton_group_count: light_group_count,
                blocked_frame_count: plan_snapshot.blocked_frame_count,
                finished_at: &terminal_now,
            },
        )
        .await
    );

    tx_try!(conn, commit(&mut conn).await);

    get_operation_public_id_by_row_id(pool, params.operation_row_id).await
}

/// Create a new `session_materialization_operation` in `ready` state and return
/// its `row_id` and `state_version`.
///
/// The `command_row_id` is the `command_execution.row_id` for the active lease.
///
/// # Errors
///
/// Returns [`persistence_core::DbError`] on SQL or constraint errors.
pub async fn insert_ready_operation(
    pool: &SqlitePool,
    operation_public_id: &str,
    command_row_id: i64,
    config_revision_row_id: i64,
) -> DbResult<(i64, i64)> {
    let now = Timestamp::now_iso();
    let mut conn = pool.acquire().await?;
    begin_immediate(&mut conn).await?;
    let seq = tx_try!(conn, current_sequence_on_conn(&mut conn).await);
    tx_try!(conn, insert_repository_change(&mut conn, Some(command_row_id), &now).await);
    let row_id = tx_try!(
        conn,
        insert_materialization_operation(
            &mut conn,
            &InsertMaterializationOperation {
                public_id: operation_public_id,
                kind: "inbox_ingestion",
                command_row_id,
                config_revision_row_id,
                created_sequence: seq + 1,
                created_at: &now,
            },
        )
        .await
    );
    tx_try!(conn, commit(&mut conn).await);
    // state_version starts at 0 on insert
    Ok((row_id, 0))
}

/// Transition a `ready` or `applying` operation to `failed`.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::CasFailed`] on state-version mismatch, or
/// [`persistence_core::DbError::Database`] on SQL errors.
pub async fn mark_operation_failed(
    pool: &SqlitePool,
    operation_row_id: i64,
    state_version: i64,
    failure_code: &str,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    let mut conn = pool.acquire().await?;
    begin_immediate(&mut conn).await?;
    tx_try!(
        conn,
        transition_operation_to_failed(
            &mut conn,
            operation_row_id,
            state_version,
            failure_code,
            &now
        )
        .await
    );
    tx_try!(conn, commit(&mut conn).await);
    Ok(())
}
