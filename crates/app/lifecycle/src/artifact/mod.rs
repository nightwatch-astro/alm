// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Application use cases for processing artifact observation (spec 012).
//!
//! ## Entry points
//!
//! - [`detect`]         — record a newly observed file, classify it, attribute it to a launch.
//! - [`list`]           — list artifacts for a project (for the drawer accordion).
//! - [`classify_override`] — apply / clear a manual classification override.
//! - [`mark_resolved`]  — mark a missing artifact as user-resolved.
//! - [`mark_missing`]/[`mark_recovered`] — on-attach rescan: detect new files + mark gone files as missing.
//! - [`reattribute`]    — back-fill `tool_launch_id` after a new `tool.launch` event (T022b).
//! - [`complete_run`]   — set `ToolLaunch.completed_at` and emit `workflow.run_completed` (T022c).
//! - [`resolve_project_id_for_path`] — path→project attribution by
//!   longest-prefix match against registered project roots (spec 012,
//!   package WP-012-A).
//! - [`reattribute_root_keyed_artifacts`] — one-time idempotent startup
//!   fix-up for rows the retired global watcher (pre-#400) keyed by
//!   library-root id instead of project id (WP-012-A).
//!
//! ## Architecture
//!
//! Classification uses `workflow_artifacts::classify` (pure; no DB or I/O).
//! Attribution uses `workflow_artifacts::attribute` (pure timestamp math).
//! Persistence is delegated to `persistence_plans::repositories::artifacts`.
//! Audit events are emitted via `audit::bus::EventBus`.
//!
//! Constitution III: this module never opens, processes, or modifies observed files.
//! Constitution V: the DB row is the durable record; the file index is reproducible.
//!
//! Split by responsibility (refactor sweep #980): `attribution` is the
//! path→project resolver + startup fix-up; [`detect`] is the observe/insert
//! pipeline; [`list`] is the read projection; `classify` is the manual
//! override; `missing_recovered` handles the reconcile pass; `launches`
//! covers re-attribution, run completion, and the stale-launch sweep.
#![allow(clippy::doc_markdown)]

use sqlx::SqlitePool;
use time::OffsetDateTime;

use persistence_plans::repositories::artifacts::ArtifactRow;
use persistence_plans::repositories::tool_launches as tl_repo;

use contracts_core::tools::ArtifactSummary;
use workflow_artifacts::LaunchRef;

mod attribution;
mod classify;
#[allow(clippy::too_many_arguments)]
mod detect;
mod launches;
mod list;
mod missing_recovered;

#[cfg(test)]
mod tests;

pub use attribution::{reattribute_root_keyed_artifacts, resolve_project_id_for_path};
pub use classify::classify_override;
pub use detect::detect;
pub use launches::{complete_run, reattribute, sweep_stale_launches};
pub use list::list;
pub use missing_recovered::{mark_missing, mark_recovered, mark_resolved};

// ── Shared helpers ───────────────────────────────────────────────────────────

fn parse_dt(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

fn row_to_summary(row: ArtifactRow) -> ArtifactSummary {
    ArtifactSummary {
        id: row.id,
        project_id: row.project_id,
        tool_launch_id: row.tool_launch_id,
        path: row.path,
        kind: row.kind,
        tool: row.tool,
        detected_at: row.detected_at,
        last_seen_at: row.last_seen_at,
        state: row.state,
        classification_confidence: row.classification_confidence,
        classification_source: row.classification_source,
        size_bytes: row.size_bytes,
    }
}

/// Load `LaunchRef` entries for a project + tool from the `tool_launches` table.
async fn load_launch_refs(
    pool: &SqlitePool,
    project_id: &str,
    tool_id: &str,
) -> Result<Vec<LaunchRef>, String> {
    let rows = tl_repo::list_launches_for_project(pool, project_id)
        .await
        .map_err(|e| format!("DB launches failed: {e}"))?;

    let refs = rows
        .into_iter()
        .filter(|r| r.tool_id == tool_id)
        .filter_map(|r| {
            let dt = parse_dt(&r.launched_at)?;
            Some(LaunchRef { id: r.id, tool_id: r.tool_id, launched_at: dt })
        })
        .collect();
    Ok(refs)
}
