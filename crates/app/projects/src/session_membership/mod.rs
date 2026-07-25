// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Project session-pin use cases (spec 062 US3/US5).
//!
//! Entry points:
//! - [`add_session_pin`]     — explicit additive pin; lifecycle guard; marks view stale.
//! - [`replace_session_pin`] — atomic one-to-many correction replacement; lifecycle guard.
//! - [`list_related_sessions`] — read-only derivation from panel siblings and supersessions.
//! - [`list_session_pins`]   — read-only list of current pinned sessions.
//!
//! All writes acquire `BEGIN IMMEDIATE`, apply CAS on
//! `spec062_project.membership_head_generation`, then commit atomically.
//! A stale-generation conflict bubbles as `ContractError` with
//! `ErrorCode::Conflict`.
//!
//! FR-051–FR-059 governs this module. In particular:
//! - No panel, mosaic, or family foreign key may expand membership (FR-051).
//! - Addition is additive only; removal is not exposed here (FR-059).
//! - Replace is atomic across the complete replacement set (FR-058).
//! - Completed/archived projects refuse all pin changes (FR-053).

use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use persistence_core::DbError;

mod add;
mod query;
mod replace;

pub use add::{add_session_pin, AddSessionPinRequest, AddSessionPinResponse};
pub use query::{
    list_related_sessions, list_session_pins, view_state_query, PinListCursor, ProjectSessionPin,
    ProjectViewState, RelatedSession, RelatedSessionCursor,
};
pub use replace::{replace_session_pin, ReplaceSessionPinRequest, ReplaceSessionPinResponse};

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Lifecycle states that permit session-pin addition or replacement (FR-053).
///
/// `completed` and `archived` are deliberately absent.
const ALLOWED_LIFECYCLES: &[&str] =
    &["setup_incomplete", "ready", "prepared", "processing", "blocked"];

/// True when the project lifecycle allows a pin add or replace.
pub(super) fn lifecycle_allows_add(lifecycle: &str) -> bool {
    ALLOWED_LIFECYCLES.contains(&lifecycle)
}

/// Map a `DbError` arising from a project-lookup to the appropriate
/// `ContractError`.
pub(super) fn project_db_err(e: DbError) -> ContractError {
    match e {
        DbError::NotFound(msg) => {
            ContractError::new(ErrorCode::ProjectNotFound, msg, ErrorSeverity::Blocking, false)
        }
        other => app_core_errors::db_err(other),
    }
}

/// Map a `DbError` arising from a session-lookup to the appropriate
/// `ContractError`.
pub(super) fn session_db_err(e: DbError) -> ContractError {
    match e {
        DbError::NotFound(msg) => {
            ContractError::new(ErrorCode::SessionNotFound, msg, ErrorSeverity::Blocking, false)
        }
        other => app_core_errors::db_err(other),
    }
}

/// Map a `DbError::CasFailed` to a membership-conflict contract error.
pub(super) fn cas_err(e: DbError) -> ContractError {
    match e {
        DbError::CasFailed(msg) => ContractError::new(
            ErrorCode::ProjectMembershipConflict,
            msg,
            ErrorSeverity::Blocking,
            false,
        ),
        other => app_core_errors::db_err(other),
    }
}
