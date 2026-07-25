// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `project_channels` table CRUD.

use domain_core::ids::Timestamp;
use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::DbResult;

use super::ProjectChannelRow;

/// Replace all channels for a project atomically (delete + insert in one tx).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on transaction failure.
pub async fn replace_project_channels(
    pool: &SqlitePool,
    project_id: &str,
    channels: &[(&str, &str)], // (label, source)
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM project_channels WHERE project_id = ?")
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

    for (label, source) in channels {
        sqlx::query(
            "INSERT INTO project_channels (project_id, label, source, added_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(project_id)
        .bind(label)
        .bind(source)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Fetch all channels for a project, ordered by label ascending.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_project_channels(
    pool: &SqlitePool,
    project_id: &str,
) -> DbResult<Vec<ProjectChannelRow>> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT project_id, label, source, added_at
         FROM project_channels WHERE project_id = ? ORDER BY label ASC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(project_id, label, source, added_at)| ProjectChannelRow {
            project_id,
            label,
            source,
            added_at,
        })
        .collect())
}

/// Insert a single project-channel row (no delete). Only used by
/// [`super::create_project_tx`]: a brand-new project has no prior channel
/// rows, so the delete-then-insert [`replace_project_channels`] does is
/// unnecessary.
pub(super) async fn insert_project_channel_conn(
    conn: &mut SqliteConnection,
    project_id: &str,
    label: &str,
    source: &str,
    added_at: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO project_channels (project_id, label, source, added_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(project_id)
    .bind(label)
    .bind(source)
    .bind(added_at)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
