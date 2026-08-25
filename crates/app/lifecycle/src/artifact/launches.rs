// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `reattribute` / `complete_run` / `sweep_stale_launches` — tool-launch
//! attribution and lifecycle bookkeeping.

use audit::bus::EventBus;
use audit::event_bus::{Source, WorkflowRunCompleted, TOPIC_WORKFLOW_RUN_COMPLETED};
use sqlx::SqlitePool;
use time::OffsetDateTime;
use workflow_artifacts::{LaunchRef, DEFAULT_ATTRIBUTION_WINDOW};

use domain_core::ids::Timestamp;
use persistence_plans::repositories::artifacts::{self as repo};
use persistence_plans::repositories::tool_launches::{self as tl_repo};

use super::{load_launch_refs, parse_dt};

/// Back-fill `tool_launch_id` for artifacts detected before the launch row was
/// persisted (A7 re-attribution on `tool.launch` event).
///
/// Fetches all artifacts for the project, then updates those whose `detected_at`
/// falls within the attribution window AND whose current attribution is null or
/// earlier than `new_launch`.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn reattribute(
    pool: &SqlitePool,
    project_id: &str,
    new_launch_id: &str,
    new_launch_tool_id: &str,
    new_launch_launched_at: &str,
) -> Result<usize, String> {
    let rows = repo::list_artifacts_for_project(pool, project_id, &[])
        .await
        .map_err(|e| format!("DB list failed: {e}"))?;

    let new_launch_dt = parse_dt(new_launch_launched_at)
        .ok_or_else(|| format!("invalid launched_at: {new_launch_launched_at}"))?;

    let new_launch = LaunchRef {
        id: new_launch_id.to_owned(),
        tool_id: new_launch_tool_id.to_owned(),
        launched_at: new_launch_dt,
    };

    // Load existing launches to determine ordering.
    let existing = load_launch_refs(pool, project_id, new_launch_tool_id).await?;

    // Build candidate list. The producing tool travels with each row;
    // `reattribute_candidates` owns the same-tool rule.
    let candidates_input: Vec<(String, String, OffsetDateTime, Option<String>)> = rows
        .iter()
        .filter_map(|r| {
            let dt = parse_dt(&r.detected_at)?;
            Some((r.id.clone(), r.tool.clone(), dt, r.tool_launch_id.clone()))
        })
        .collect();

    let candidates = workflow_artifacts::reattribute_candidates(
        &new_launch,
        &candidates_input,
        &existing,
        DEFAULT_ATTRIBUTION_WINDOW,
    );

    let mut updated = 0usize;
    for artifact_id in candidates {
        repo::set_tool_launch_id(pool, artifact_id, new_launch_id)
            .await
            .map_err(|e| format!("DB re-attribute failed: {e}"))?;
        updated += 1;
    }
    Ok(updated)
}

/// Mark a tool launch complete and emit `workflow.run_completed` (T022c).
///
/// Sets `tool_launches.completed_at` and emits the event that spec 024
/// subscribes to for manifest creation.
///
/// # Errors
/// Returns `Err(String)` on DB or audit failure.
pub async fn complete_run(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    tool_id: &str,
    tool_launch_id: &str,
) -> Result<bool, String> {
    let completed_at = Timestamp::now_iso();
    let updated = repo::complete_tool_launch(pool, tool_launch_id, &completed_at)
        .await
        .map_err(|e| format!("DB complete_run failed: {e}"))?;

    if updated {
        let artifact_ids = repo::list_artifact_ids_for_launch(pool, tool_launch_id)
            .await
            .map_err(|e| format!("DB artifact ids failed: {e}"))?;

        let _ = bus
            .publish(
                TOPIC_WORKFLOW_RUN_COMPLETED,
                Source::System,
                WorkflowRunCompleted {
                    project_id: project_id.to_owned(),
                    tool_id: tool_id.to_owned(),
                    tool_launch_id: tool_launch_id.to_owned(),
                    completed_at,
                    artifact_ids,
                },
            )
            .await;
    }

    Ok(updated)
}

/// Complete any of a project's open tool launches whose attribution window
/// has closed (#727 / FR-010's stated heuristic: a launch is terminal when
/// the attribution window elapses after the last artifact attributed to it
/// was last seen — or, when nothing was ever attributed, after the launch
/// itself started).
///
/// This is the real production trigger for [`complete_run`]: it is polled
/// periodically by the live per-project watcher
/// (`apps/desktop/src-tauri/src/watcher.rs`) while a project's drawer is
/// open. Previously `complete_run` had no production caller at all, so
/// `workflow.run_completed` (and the spec 024 manifest subscriber that
/// depends on it) never fired outside tests.
///
/// Returns the number of launches completed.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn sweep_stale_launches(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
) -> Result<usize, String> {
    let launches = tl_repo::list_launches_for_project(pool, project_id)
        .await
        .map_err(|e| format!("DB launches failed: {e}"))?;
    let open: Vec<_> = launches
        .into_iter()
        .filter(|l| l.outcome == "spawned" && l.completed_at.is_none())
        .collect();
    if open.is_empty() {
        return Ok(0);
    }

    let artifacts = repo::list_artifacts_for_project(pool, project_id, &[])
        .await
        .map_err(|e| format!("DB artifacts failed: {e}"))?;

    let now = OffsetDateTime::now_utc();
    let mut completed = 0usize;
    for launch in open {
        let last_seen = artifacts
            .iter()
            .filter(|a| a.tool_launch_id.as_deref() == Some(launch.id.as_str()))
            .filter_map(|a| parse_dt(&a.last_seen_at))
            .max();
        let Some(reference) = last_seen.or_else(|| parse_dt(&launch.launched_at)) else {
            continue; // unparseable timestamp; leave for the next sweep
        };
        if now - reference >= DEFAULT_ATTRIBUTION_WINDOW
            && complete_run(pool, bus, project_id, &launch.tool_id, &launch.id).await?
        {
            completed += 1;
        }
    }
    Ok(completed)
}
