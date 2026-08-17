// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `project.session_pin.replace` use case (spec 062 US5 FR-058).
//!
//! Atomically replaces a predecessor session pin with the complete non-empty
//! replacement set authorized by an applied reclassification plan revision.
//!
//! Guards:
//! - lifecycle must allow addition (FR-053);
//! - predecessor must be currently pinned;
//! - `replacement_session_ids` must be non-empty, contain no duplicates, and
//!   equal the complete replacement set authorized by the supplied revision;
//! - no replacement session may already be pinned.
//!
//! A stale revision or invalid replacement aborts the complete set without
//! changing project membership (FR-058 atomicity).

use sqlx::SqlitePool;

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::Timestamp;
use persistence_sessions::repositories::{change_sequence, tx};
use persistence_topology::repositories::project_membership as repo;
use uuid::Uuid;

use super::{cas_err, lifecycle_allows_add, project_db_err, session_db_err};

/// Request for `project.session_pin.replace`.
pub struct ReplaceSessionPinRequest<'a> {
    pub project_id: &'a str,
    pub predecessor_session_id: &'a str,
    /// Must be non-empty; must equal the complete replacement set authorized
    /// by `applied_reclassification_plan_revision_id`.
    pub replacement_session_ids: &'a [&'a str],
    /// Public UUID of an `applied` `reclassification_plan_revision` row that
    /// authorizes the replacement of `predecessor_session_id`.
    pub applied_reclassification_plan_revision_id: &'a str,
    pub expected_project_revision: i64,
    pub actor_id: &'a str,
}

/// Response for `project.session_pin.replace`.
#[derive(Debug)]
pub struct ReplaceSessionPinResponse {
    pub revision_id: String,
    pub new_project_revision: i64,
}

/// Execute `project.session_pin.replace`.
///
/// # Errors
///
/// - `project.not_found`
/// - `project.lifecycle_disallows_session_add`
/// - `project.session_not_pinned` — predecessor not in current head
/// - `project.reclassification_revision_invalid` — revision absent, not applied,
///   or does not authorize exactly `replacement_session_ids`
/// - `project.session_already_pinned` — a replacement is already pinned
/// - `project.membership_conflict` — CAS failed
pub async fn replace_session_pin(
    pool: &SqlitePool,
    req: &ReplaceSessionPinRequest<'_>,
) -> Result<ReplaceSessionPinResponse, ContractError> {
    if req.replacement_session_ids.is_empty() {
        return Err(ContractError::new(
            ErrorCode::ProjectReclassificationRevisionInvalid,
            "replacement_session_ids must be non-empty".to_owned(),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Dedup check.
    let mut deduped = req.replacement_session_ids.to_vec();
    deduped.sort_unstable();
    deduped.dedup();
    if deduped.len() != req.replacement_session_ids.len() {
        return Err(ContractError::new(
            ErrorCode::ProjectReclassificationRevisionInvalid,
            "replacement_session_ids contains duplicates".to_owned(),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let now = Timestamp::now_iso();

    let mut conn = pool.acquire().await.map_err(|e| app_core_errors::db_err(e.into()))?;
    tx::enable_foreign_keys(&mut conn).await.map_err(app_core_errors::db_err)?;
    tx::begin_immediate(&mut conn).await.map_err(app_core_errors::db_err)?;

    let result = replace_inner(&mut conn, req, &deduped, &now).await;

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

#[allow(clippy::too_many_lines)]
async fn replace_inner(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    req: &ReplaceSessionPinRequest<'_>,
    deduped: &[&str],
    now: &str,
) -> Result<ReplaceSessionPinResponse, ContractError> {
    // 1. Load project.
    let project =
        repo::get_spec062_project(&mut *conn, req.project_id).await.map_err(project_db_err)?;

    // 2. Check lifecycle.
    let lifecycle = repo::fetch_legacy_lifecycle(&mut *conn, req.project_id)
        .await
        .map_err(app_core_errors::db_err)?;
    if !lifecycle_allows_add(&lifecycle) {
        return Err(ContractError::new(
            ErrorCode::ProjectLifecycleDisallowsSessionAdd,
            format!(
                "project {} lifecycle '{}' does not allow session replacement",
                req.project_id, lifecycle
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 3. Check expected revision.
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

    // 4. Load existing pins — required (project must have a membership head).
    let head_rev_id = project.membership_head_revision_row_id.ok_or_else(|| {
        ContractError::new(
            ErrorCode::ProjectSessionNotPinned,
            format!(
                "project {} has no pinned sessions; {} is not pinned",
                req.project_id, req.predecessor_session_id
            ),
            ErrorSeverity::Blocking,
            false,
        )
    })?;
    let existing_pins = repo::fetch_pins_for_revision(&mut *conn, head_rev_id)
        .await
        .map_err(app_core_errors::db_err)?;

    // 5. Look up predecessor session.
    let (predecessor_row_id, _) =
        repo::lookup_session_row_id(&mut *conn, req.predecessor_session_id)
            .await
            .map_err(session_db_err)?;

    // 6. Guard: predecessor must be pinned.
    if !existing_pins.iter().any(|p| p.session_row_id == predecessor_row_id) {
        return Err(ContractError::new(
            ErrorCode::ProjectSessionNotPinned,
            format!(
                "session {} is not pinned in project {}",
                req.predecessor_session_id, req.project_id
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 7. Validate reclassification plan revision and derive authorized replacements.
    let (plan_rev_row_id, authorized_replacements) =
        repo::lookup_applied_reclassification_revision(
            &mut *conn,
            req.applied_reclassification_plan_revision_id,
        )
        .await
        .map_err(|_| {
            ContractError::new(
                ErrorCode::ProjectReclassificationRevisionInvalid,
                format!(
                    "reclassification_plan_revision {} is absent or not applied",
                    req.applied_reclassification_plan_revision_id
                ),
                ErrorSeverity::Blocking,
                false,
            )
        })?;

    // 8. Verify the authorized replacement set matches the request exactly.
    let mut authorized_ids: Vec<&str> =
        authorized_replacements.iter().map(|(_, pub_id)| pub_id.as_str()).collect();
    authorized_ids.sort_unstable();

    let mut requested_sorted = deduped.to_vec();
    requested_sorted.sort_unstable();

    if authorized_ids != requested_sorted {
        return Err(ContractError::new(
            ErrorCode::ProjectReclassificationRevisionInvalid,
            format!(
                "replacement_session_ids do not match the complete authorized set for \
                 predecessor {} under revision {}",
                req.predecessor_session_id, req.applied_reclassification_plan_revision_id
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 9. Look up replacement session row_ids; guard none are already pinned.
    let mut replacement_rows: Vec<(i64, String)> = Vec::new();
    for &repl_id in deduped {
        let (row_id, _) =
            repo::lookup_session_row_id(&mut *conn, repl_id).await.map_err(session_db_err)?;

        if existing_pins.iter().any(|p| p.session_row_id == row_id) {
            return Err(ContractError::new(
                ErrorCode::ProjectSessionAlreadyPinned,
                format!(
                    "replacement session {} is already pinned in project {}",
                    repl_id, req.project_id
                ),
                ErrorSeverity::Blocking,
                false,
            ));
        }
        replacement_rows.push((row_id, repl_id.to_owned()));
    }

    // 10. Resolve actor.
    let actor_row_id = repo::ensure_spec062_actor(&mut *conn, req.actor_id, now)
        .await
        .map_err(app_core_errors::db_err)?;

    // 11. Insert change sequence.
    let seq = change_sequence::insert_repository_change(conn, None, now)
        .await
        .map_err(app_core_errors::db_err)?;

    // 12. Compute new revision number.
    let new_revision_number = repo::fetch_revision_number(&mut *conn, head_rev_id)
        .await
        .map_err(app_core_errors::db_err)?
        + 1;

    // 13. Insert new revision.
    let rev_public_id = Uuid::new_v4().to_string();
    let rev_row_id = repo::insert_membership_revision(
        &mut *conn,
        &repo::InsertMembershipRevision {
            public_id: &rev_public_id,
            project_row_id: project.row_id,
            revision_number: new_revision_number,
            parent_revision_row_id: Some(head_rev_id),
            proposal_row_id: None,
            actor_row_id,
            created_sequence: seq,
            created_at: now,
        },
    )
    .await
    .map_err(app_core_errors::db_err)?;

    // 14. Carry forward all pins except the predecessor.
    for pin in &existing_pins {
        if pin.session_row_id == predecessor_row_id {
            continue;
        }
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

    // 15. Insert replacement pins.
    for (repl_row_id, _) in &replacement_rows {
        let pin_revision = repo::next_pin_revision(&mut *conn, project.row_id, *repl_row_id)
            .await
            .map_err(app_core_errors::db_err)?;
        repo::insert_pin(
            &mut *conn,
            &repo::InsertPin {
                revision_row_id: rev_row_id,
                session_row_id: *repl_row_id,
                pin_revision,
                source: "explicit_replacement",
                replaces_session_row_id: Some(predecessor_row_id),
                applied_reclassification_plan_revision_row_id: Some(plan_rev_row_id),
                pinned_by_actor_row_id: actor_row_id,
                pinned_at: now,
            },
        )
        .await
        .map_err(app_core_errors::db_err)?;
    }

    // 16. Advance head CAS.
    repo::advance_membership_head(
        &mut *conn,
        project.row_id,
        rev_row_id,
        project.membership_head_generation,
        seq,
    )
    .await
    .map_err(cas_err)?;

    Ok(ReplaceSessionPinResponse {
        revision_id: rev_public_id,
        new_project_revision: project.membership_head_generation + 1,
    })
}
