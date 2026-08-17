// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for the projects store (spec 008).
//!
//! Operates on the `projects`, `project_sources`, and `project_channels`
//! tables from migration 0018.
//!
//! Constitution I: paths stored as library-root-relative strings.
//! Constitution V: SQLite is the durable record; snapshot fields on
//! `project_sources` denormalize Inventory data at link time.
//!
//! Split by responsibility (refactor sweep #977): `crud` is the `projects`
//! table CRUD; `sources` is `project_sources` CRUD; `channels` is
//! `project_channels` CRUD; `create_tx` is the composite atomic create
//! (T2-a) that writes all three tables plus the Constitution II folder plan
//! in one transaction. Row/insert types shared across siblings stay here.

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat row from the `projects` table.
#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub tool: String,
    pub lifecycle: String,
    pub path: String,
    pub notes: Option<String>,
    pub channel_drift: bool,
    pub created_at: String,
    pub updated_at: String,
    /// FR-020: typed blocked reason kind (migration 0037).
    /// Populated when lifecycle == "blocked"; NULL otherwise.
    pub blocked_reason_kind: Option<String>,
    /// FR-020: free-form blocked reason note (migration 0037).
    pub blocked_reason_note: Option<String>,
    /// Mosaic-mode flag (Q27 FR-017, migration 0064). Default false.
    pub is_mosaic: bool,
}

/// Flat row from the `project_sources` table.
#[derive(Clone, Debug)]
pub struct ProjectSourceRow {
    pub id: String,
    pub project_id: String,
    pub inventory_session_id: String,
    pub name_snapshot: String,
    pub frames_snapshot: i64,
    pub filter_snapshot: String,
    pub exposure_snapshot: String,
    pub linked_at: String,
}

/// Flat row from the `project_channels` table.
#[derive(Clone, Debug)]
pub struct ProjectChannelRow {
    pub project_id: String,
    pub label: String,
    pub source: String,
    pub added_at: String,
}

// ── Insert helpers ────────────────────────────────────────────────────────────

/// Data required to insert a new project row.
#[derive(Clone, Debug)]
pub struct InsertProject<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub tool: &'a str,
    pub lifecycle: &'a str,
    pub path: &'a str,
    pub notes: Option<&'a str>,
    /// Optional spec-035 `canonical_target` id (additive; nullable). Coexists
    /// with the legacy spec-013 `target_id` column.
    pub canonical_target_id: Option<&'a str>,
    /// Mosaic-mode flag (Q27 FR-017), default false.
    pub is_mosaic: bool,
}

/// Data required to insert a project source link.
#[derive(Clone, Debug)]
pub struct InsertProjectSource<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub inventory_session_id: &'a str,
    pub name_snapshot: &'a str,
    pub frames_snapshot: i64,
    pub filter_snapshot: &'a str,
    pub exposure_snapshot: &'a str,
    pub linked_at: &'a str,
}

// ── Type aliases for complex query row types ──────────────────────────────────

/// Row tuple returned by `get_project` and `list_projects` queries.
/// Factored out to satisfy clippy::type_complexity.
type ProjectRowTuple = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
);

mod channels;
mod create_tx;
mod crud;
mod sources;

#[cfg(test)]
mod tests;

pub use channels::{list_project_channels, replace_project_channels};
pub use create_tx::{create_project_tx, CreateProjectInput};
pub use crud::{
    apply_project_auto_block, clear_archived_via_plan_id, get_project,
    get_project_canonical_target, get_project_canonical_target_id, insert_project,
    list_archived_projects, list_projects, list_projects_by_canonical_target_id, name_exists,
    path_exists, set_archived_via_plan_id, set_channel_drift, set_project_canonical_target_id,
    update_project_fields, update_project_lifecycle, update_project_lifecycle_unblock,
    ArchivedProjectRow, ProjectAutoBlockOutcome, ProjectCanonicalTargetRow,
};
pub use sources::{
    begin_source_missing_health_check, clear_source_missing_health_check, delete_project_source,
    find_blockable_missing_sources, get_project_source, has_archived_raw_frames_for_project,
    insert_project_source, list_project_ids_for_session, list_project_sources,
};
