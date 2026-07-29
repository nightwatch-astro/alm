// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for spec 003 first-run source registration.
//!
//! Operates on `registered_sources` and `first_run_state` tables
//! (migration 0006).
//!
//! Split by responsibility (refactor sweep #974): `sources` is
//! register/list/find/remove; `source_state` is per-source organization
//! state / path / active-flag get-set; `dependents` is the
//! `roots.delete`/`roots.remap` dependency-counting pair; `wizard_state`
//! is the `first_run_state` singleton (get/complete/restart).

use domain_core::first_run::{OrganizationState, ScanDepth, SourceKind};
use sqlx::SqlitePool;

use persistence_core::DbResult;

mod dependents;
mod source_state;
mod sources;
mod wizard_state;

#[cfg(test)]
mod byte_identity_guard;
#[cfg(test)]
mod tests;

pub use dependents::{count_root_dependents, relative_paths_for_root};
pub use source_state::{
    get_source_kind_and_path, get_source_organization_state, get_source_path, list_active_flags,
    set_source_active, set_source_organization_state, set_source_path,
};
pub use sources::{
    find_sources_by_path, list_sources, register_source, register_source_batch, remove_source,
};
pub use wizard_state::{complete_first_run, get_first_run_state, restart_first_run};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn source_kind_to_str(kind: SourceKind) -> &'static str {
    // `strum::IntoStaticStr` yields the canonical snake_case strings.
    kind.into()
}

fn str_to_source_kind(s: &str) -> SourceKind {
    // `strum::EnumString` parses the canonical strings; "light_frames" and any
    // unknown value default to LightFrames (preserving prior behavior).
    s.parse().unwrap_or(SourceKind::LightFrames)
}

fn scan_depth_to_str(depth: ScanDepth) -> &'static str {
    // `strum::IntoStaticStr` yields the canonical lowercase strings.
    depth.into()
}

fn organization_state_to_str(state: OrganizationState) -> &'static str {
    match state {
        OrganizationState::Organized => "organized",
        OrganizationState::Unorganized => "unorganized",
    }
}

fn str_to_organization_state(s: &str) -> OrganizationState {
    match s {
        "organized" => OrganizationState::Organized,
        _ => OrganizationState::Unorganized,
    }
}

/// Determine `created_via` based on first_run_state.completed_at.
async fn resolve_created_via(pool: &SqlitePool) -> DbResult<&'static str> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT completed_at FROM first_run_state WHERE singleton_id = 'first_run'")
            .fetch_optional(pool)
            .await?;
    match row {
        Some((Some(_completed),)) => Ok("settings_add"),
        _ => Ok("first_run"),
    }
}
