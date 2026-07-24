// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::too_many_lines)]

//! `project.session_pin.add` use case (spec 062 US3 FR-051–FR-054).
//!
//! Atomically pins one exact session to a spec-062 project. Guards:
//! - project must exist as a `spec062_project` row;
//! - lifecycle must be `setup_incomplete`, `ready`, `prepared`, `processing`,
//!   or `blocked` (completed/archived refused per FR-053);
//! - session must exist and must not already be pinned in the current head.
//!
//! On success:
//! - a new `project_membership_revision` row is inserted;
//! - the session and all existing pins are carried forward;
//! - the `spec062_project` membership head is advanced via CAS;
//! - a `project_membership_head_history` row is inserted.

use sqlx::SqlitePool;

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::Timestamp;
use persistence_sessions::repositories::{change_sequence, tx};
use persistence_topology::repositories::project_membership as repo;
use uuid::Uuid;

use super::{cas_err, lifecycle_allows_add, project_db_err, session_db_err};

/// Request for `project.session_pin.add`.
pub struct AddSessionPinRequest<'a> {
    /// Public UUID of the `spec062_project`.
    pub project_id: &'a str,
    /// Public UUID of the session to pin.
    pub session_id: &'a str,
    /// Expected `membership_head_generation`; used for client-side optimistic
    /// locking. Pass the value returned by the most recent `view_state.query`.
    pub expected_project_revision: i64,
    /// Public UUID of the actor performing the operation.
    pub actor_id: &'a str,
    /// Optional evidence identifier from the related-session surface that
    /// triggered this explicit add (audit provenance only).
    // TODO(ic9h.20): project_membership_revision_session has no
    // related_session_evidence_id column yet; accepted per contract but not
    // persisted until the .20 migration lands.
    pub related_session_evidence_id: Option<&'a str>,
}

/// Response for `project.session_pin.add`.
#[derive(Debug)]
pub struct AddSessionPinResponse {
    /// The new membership revision public UUID.
    pub revision_id: String,
    /// Current generation after the head advance.
    pub new_project_revision: i64,
    /// Whether the project view is now stale (true when there was a prior
    /// materialization snapshot that does not include the newly added session).
    pub view_stale: bool,
}

/// Execute `project.session_pin.add`.
///
/// # Errors
///
/// - `project.not_found` — no `spec062_project` row for `project_id`.
/// - `project.lifecycle_disallows_session_add` — lifecycle is completed or archived.
/// - `session.not_found` — no `session` row for `session_id`.
/// - `project.session_already_pinned` — session is already in the current head.
/// - `project.membership_conflict` — CAS failed (concurrent write) or stale revision.
pub async fn add_session_pin(
    pool: &SqlitePool,
    req: &AddSessionPinRequest<'_>,
) -> Result<AddSessionPinResponse, ContractError> {
    let now = Timestamp::now_iso();

    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;

    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;
    tx::begin_immediate(&mut conn).await.map_err(app_core_errors::db_err)?;

    let result = add_session_pin_inner(&mut conn, req, &now).await;

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

async fn add_session_pin_inner(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    req: &AddSessionPinRequest<'_>,
    now: &str,
) -> Result<AddSessionPinResponse, ContractError> {
    // 1. Load project row.
    let project =
        repo::get_spec062_project(&mut *conn, req.project_id).await.map_err(project_db_err)?;

    // 2. Check lifecycle via the legacy `projects` table.
    let lifecycle = repo::fetch_legacy_lifecycle(&mut *conn, req.project_id)
        .await
        .map_err(app_core_errors::db_err)?;

    if !lifecycle_allows_add(&lifecycle) {
        return Err(ContractError::new(
            ErrorCode::ProjectLifecycleDisallowsSessionAdd,
            format!(
                "project {} lifecycle '{}' does not allow session addition",
                req.project_id, lifecycle
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 3. Check expected_project_revision.
    if project.membership_head_generation != req.expected_project_revision {
        return Err(ContractError::new(
            ErrorCode::ProjectMembershipConflict,
            format!(
                "project {} expected revision {} but current is {}",
                req.project_id, req.expected_project_revision, project.membership_head_generation
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 4. Look up session.
    let (session_row_id, _) =
        repo::lookup_session_row_id(&mut *conn, req.session_id).await.map_err(session_db_err)?;

    // 5. Load existing pins.
    let existing_pins = match project.membership_head_revision_row_id {
        Some(rev_row_id) => repo::fetch_pins_for_revision(&mut *conn, rev_row_id)
            .await
            .map_err(app_core_errors::db_err)?,
        None => Vec::new(),
    };

    // 6. Guard: session must not already be pinned.
    if existing_pins.iter().any(|p| p.session_row_id == session_row_id) {
        return Err(ContractError::new(
            ErrorCode::ProjectSessionAlreadyPinned,
            format!("session {} is already pinned in project {}", req.session_id, req.project_id),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 7. Resolve actor row.
    let actor_row_id = repo::ensure_spec062_actor(&mut *conn, req.actor_id, now)
        .await
        .map_err(app_core_errors::db_err)?;

    // 8. Insert change sequence row.
    let seq = change_sequence::insert_repository_change(conn, None, now)
        .await
        .map_err(app_core_errors::db_err)?;

    // 9. Compute next revision number.
    let new_revision_number = match project.membership_head_revision_row_id {
        Some(rev_row_id) => {
            repo::fetch_revision_number(&mut *conn, rev_row_id)
                .await
                .map_err(app_core_errors::db_err)?
                + 1
        }
        None => 1,
    };

    // 10. Insert new membership revision.
    let rev_public_id = Uuid::new_v4().to_string();
    let rev_row_id = repo::insert_membership_revision(
        &mut *conn,
        &repo::InsertMembershipRevision {
            public_id: &rev_public_id,
            project_row_id: project.row_id,
            revision_number: new_revision_number,
            parent_revision_row_id: project.membership_head_revision_row_id,
            proposal_row_id: None,
            actor_row_id,
            created_sequence: seq,
            created_at: now,
        },
    )
    .await
    .map_err(app_core_errors::db_err)?;

    // 11. Carry forward all existing pins.
    for pin in &existing_pins {
        repo::insert_pin(
            &mut *conn,
            &repo::InsertPin {
                revision_row_id: rev_row_id,
                session_row_id: pin.session_row_id,
                pin_revision: pin.pin_revision,
                source: &pin.source,
                replaces_session_row_id: pin.replaces_session_row_id,
                applied_reclassification_plan_revision_row_id: pin
                    .applied_reclassification_plan_revision_row_id,
                pinned_by_actor_row_id: pin.pinned_by_actor_row_id,
                pinned_at: &pin.pinned_at,
            },
        )
        .await
        .map_err(app_core_errors::db_err)?;
    }

    // 12. Compute next pin_revision for the new session.
    let pin_revision = repo::next_pin_revision(&mut *conn, project.row_id, session_row_id)
        .await
        .map_err(app_core_errors::db_err)?;

    // 13. Insert the new pin.
    repo::insert_pin(
        &mut *conn,
        &repo::InsertPin {
            revision_row_id: rev_row_id,
            session_row_id,
            pin_revision,
            source: "explicit_add",
            replaces_session_row_id: None,
            applied_reclassification_plan_revision_row_id: None,
            pinned_by_actor_row_id: actor_row_id,
            pinned_at: now,
        },
    )
    .await
    .map_err(app_core_errors::db_err)?;

    // 14. Advance the membership head CAS.
    repo::advance_membership_head(
        &mut *conn,
        project.row_id,
        rev_row_id,
        project.membership_head_generation,
        seq,
    )
    .await
    .map_err(cas_err)?;

    // 15. Detect view staleness.
    let view_stale = repo::is_view_stale_after_add(&mut *conn, project.row_id, session_row_id)
        .await
        .map_err(app_core_errors::db_err)?;

    Ok(AddSessionPinResponse {
        revision_id: rev_public_id,
        new_project_revision: project.membership_head_generation + 1,
        view_stale,
    })
}
