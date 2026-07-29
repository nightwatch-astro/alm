// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! The `first_run_state` singleton row: get/complete/restart.

use domain_core::first_run::{
    FirstRunCompleteResponse, FirstRunRestartResponse, FirstRunStateResponse,
};
use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

use super::sources::list_sources;

/// Get the current first-run wizard state.
///
/// Returns a default state (`last_step = "source_folders"`, `completed_at = None`)
/// if no row exists yet.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_first_run_state(pool: &SqlitePool) -> DbResult<FirstRunStateResponse> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT completed_at, last_step FROM first_run_state WHERE singleton_id = 'first_run'",
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some((completed_at, last_step)) => Ok(FirstRunStateResponse { completed_at, last_step }),
        None => {
            Ok(FirstRunStateResponse { completed_at: None, last_step: "source_folders".to_owned() })
        }
    }
}

/// Mark the first-run wizard as complete.
///
/// # Errors
///
/// Returns [`DbError::NotFound`] if preconditions are not met (at least one
/// light_frames source and one project source must be registered).
pub async fn complete_first_run(pool: &SqlitePool) -> DbResult<FirstRunCompleteResponse> {
    // Check preconditions: at least one light_frames + one project source.
    // Inbox is optional (spec 039 removed it from REQUIRED_KINDS).
    let light_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM registered_sources WHERE kind = 'light_frames'")
            .fetch_one(pool)
            .await?;
    let project_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM registered_sources WHERE kind = 'project'")
            .fetch_one(pool)
            .await?;

    if light_count.0 == 0 || project_count.0 == 0 {
        return Err(DbError::NotFound(
            "first_run.incomplete: at least one light_frames and one project source required"
                .to_owned(),
        ));
    }

    let completed_at = Timestamp::now_iso();

    // Upsert the singleton row.
    sqlx::query(
        "INSERT INTO first_run_state (singleton_id, completed_at, last_step, updated_at) \
         VALUES ('first_run', ?, 'complete', ?) \
         ON CONFLICT(singleton_id) DO UPDATE SET completed_at = excluded.completed_at, \
         last_step = 'complete', updated_at = excluded.updated_at",
    )
    .bind(&completed_at)
    .bind(&completed_at)
    .execute(pool)
    .await?;

    // Count total registered sources for the response.
    let total_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM registered_sources").fetch_one(pool).await?;
    let registered_source_count = usize::try_from(total_count.0.max(0)).unwrap_or(0);

    Ok(FirstRunCompleteResponse { completed_at, registered_source_count })
}

/// Restart the first-run wizard (clear completed_at, return existing sources).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn restart_first_run(pool: &SqlitePool) -> DbResult<FirstRunRestartResponse> {
    let now = Timestamp::now_iso();

    // Clear completed_at and reset to welcome step.
    sqlx::query(
        "INSERT INTO first_run_state (singleton_id, completed_at, last_step, updated_at) \
         VALUES ('first_run', NULL, 'source_folders', ?) \
         ON CONFLICT(singleton_id) DO UPDATE SET completed_at = NULL, \
         last_step = 'source_folders', updated_at = excluded.updated_at",
    )
    .bind(&now)
    .execute(pool)
    .await?;

    // Update created_via for existing sources to 'settings_restart'.
    sqlx::query("UPDATE registered_sources SET created_via = 'settings_restart'")
        .execute(pool)
        .await?;

    let sources = list_sources(pool).await?;

    Ok(FirstRunRestartResponse { restarted_at: now.clone(), prefilled_sources: sources })
}
