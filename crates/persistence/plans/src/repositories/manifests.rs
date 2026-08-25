// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for the `manifests` table (spec 024).
//!
//! Operates on the table from migration 0028.
//!
//! Constitution V: SQLite is the durable record; manifest markdown files are
//! reproducible projections written to disk by the manifest writer.

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

// ── Helpers ───────────────────────────────────────────────────────────────────

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat row from the `manifests` table.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ManifestRow {
    pub id: String,
    pub project_id: String,
    pub reason: String,
    pub timestamp: String,
    pub path: String,
    pub version: i64,
    pub body_json: String,
}

/// Data needed to insert a new manifest row.
#[derive(Clone, Debug)]
pub struct InsertManifest<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub reason: &'a str,
    pub path: &'a str,
    pub body_json: &'a str,
    pub version: i64,
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

/// Insert a new manifest row.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn insert_manifest(pool: &SqlitePool, data: InsertManifest<'_>) -> DbResult<String> {
    let ts = Timestamp::now_iso();
    sqlx::query(
        "\
        INSERT INTO manifests (id, project_id, reason, timestamp, path, version, body_json)
        VALUES (?,?,?,?,?,?,?)
        ",
    )
    .bind(data.id)
    .bind(data.project_id)
    .bind(data.reason)
    .bind(&ts)
    .bind(data.path)
    .bind(data.version)
    .bind(data.body_json)
    .execute(pool)
    .await?;
    Ok(data.id.to_owned())
}

/// Separator between the two cursor components. An RFC-3339 timestamp cannot
/// contain it, so a left-anchored split recovers the timestamp even when the id
/// does.
const CURSOR_SEP: char = '|';

fn encode_cursor(timestamp: &str, id: &str) -> String {
    format!("{timestamp}{CURSOR_SEP}{id}")
}

fn decode_cursor(cursor: &str) -> DbResult<(&str, &str)> {
    match cursor.split_once(CURSOR_SEP) {
        Some((ts, id)) if !ts.is_empty() && !id.is_empty() => Ok((ts, id)),
        _ => Err(DbError::NotFound(format!("malformed manifest list cursor: {cursor}"))),
    }
}

/// List manifests for a project, newest first, with cursor-based pagination.
///
/// `cursor` is an opaque string carrying the whole sort key of the last seen
/// row (`timestamp` and `id`), so rows sharing a timestamp are not skipped at a
/// page boundary. `limit` is clamped to 200.
///
/// Returns `(rows, next_cursor)` where `next_cursor` is `Some` when more
/// results exist beyond the returned page.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure, and
/// `persistence_core::DbError::NotFound` for a cursor this module did not
/// produce.
pub async fn list_manifests_for_project(
    pool: &SqlitePool,
    project_id: &str,
    cursor: Option<&str>,
    limit: i64,
) -> DbResult<(Vec<ManifestRow>, Option<String>)> {
    let limit = limit.clamp(1, 200);
    // Fetch one extra to detect next page.
    let fetch_limit = limit + 1;

    let rows: Vec<ManifestRow> = match cursor {
        None => {
            sqlx::query_as(
                "\
                SELECT id, project_id, reason, timestamp, path, version, body_json
                FROM manifests
                WHERE project_id = ?
                ORDER BY timestamp DESC, id DESC
                LIMIT ?
                ",
            )
            .bind(project_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await?
        }
        Some(cursor) => {
            let (after_ts, after_id) = decode_cursor(cursor)?;
            sqlx::query_as(
                "\
                SELECT id, project_id, reason, timestamp, path, version, body_json
                FROM manifests
                WHERE project_id = ?
                  AND (timestamp < ? OR (timestamp = ? AND id < ?))
                ORDER BY timestamp DESC, id DESC
                LIMIT ?
                ",
            )
            .bind(project_id)
            .bind(after_ts)
            .bind(after_ts)
            .bind(after_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await?
        }
    };

    // `limit` is in [1, 200] so the cast to usize is safe.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let limit_usize = limit as usize;
    if rows.len() > limit_usize {
        let page: Vec<ManifestRow> = rows.into_iter().take(limit_usize).collect();
        let next_cursor = page.last().map(|r| encode_cursor(&r.timestamp, &r.id));
        Ok((page, next_cursor))
    } else {
        Ok((rows, None))
    }
}

/// Fetch a single manifest by id.
///
/// # Errors
/// Returns [`DbError::NotFound`] when no row matches.
pub async fn get_manifest(pool: &SqlitePool, manifest_id: &str) -> DbResult<ManifestRow> {
    sqlx::query_as(
        "\
        SELECT id, project_id, reason, timestamp, path, version, body_json
        FROM manifests
        WHERE id = ?
        ",
    )
    .bind(manifest_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("manifest {manifest_id}")))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::insert_project;
    use sqlx::SqlitePool;

    async fn setup() -> SqlitePool {
        let db = persistence_core::test_support::setup_db().await;
        db.pool().clone()
    }

    #[tokio::test]
    async fn insert_and_get_manifest() {
        let pool = setup().await;
        insert_project(&pool, "proj-1").await;

        let id = insert_manifest(
            &pool,
            InsertManifest {
                id: "m-0001",
                project_id: "proj-1",
                reason: "created",
                path: "notes/manifest-2026-01-01-000000-created.md",
                body_json: "{}",
                version: 1,
            },
        )
        .await
        .unwrap();
        assert_eq!(id, "m-0001");

        let row = get_manifest(&pool, "m-0001").await.unwrap();
        assert_eq!(row.project_id, "proj-1");
        assert_eq!(row.reason, "created");
    }

    #[tokio::test]
    async fn get_manifest_not_found() {
        let pool = setup().await;
        let err = get_manifest(&pool, "no-such-id").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_manifests_pagination() {
        let pool = setup().await;
        insert_project(&pool, "proj-pag").await;

        // Insert 3 manifests with slightly different timestamps.
        for i in 0u32..3 {
            sqlx::query(
                "INSERT INTO manifests (id, project_id, reason, timestamp, path, version, body_json)
                 VALUES (?,?,?,?,?,?,?)",
            )
            .bind(format!("m-{i:04}"))
            .bind("proj-pag")
            .bind("created")
            .bind(format!("2026-01-{:02}T00:00:00Z", i + 1))
            .bind(format!("notes/m-{i}.md"))
            .bind(1i64)
            .bind("{}")
            .execute(&pool)
            .await
            .unwrap();
        }

        // Fetch page of 2 — newest first.
        let (page1, cursor) = list_manifests_for_project(&pool, "proj-pag", None, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(cursor.is_some(), "should have next cursor");

        // Fetch page 2 using cursor.
        let (page2, cursor2) =
            list_manifests_for_project(&pool, "proj-pag", cursor.as_deref(), 2).await.unwrap();
        assert_eq!(page2.len(), 1);
        assert!(cursor2.is_none(), "no more pages");
    }

    #[tokio::test]
    async fn list_manifests_pagination_keeps_rows_sharing_one_timestamp() {
        let pool = setup().await;
        insert_project(&pool, "proj-tie").await;

        for i in 0u32..3 {
            sqlx::query(
                "INSERT INTO manifests (id, project_id, reason, timestamp, path, version, body_json)
                 VALUES (?,?,?,?,?,?,?)",
            )
            .bind(format!("m-{i:04}"))
            .bind("proj-tie")
            .bind("created")
            .bind("2026-01-01T00:00:00Z")
            .bind(format!("notes/m-{i}.md"))
            .bind(1i64)
            .bind("{}")
            .execute(&pool)
            .await
            .unwrap();
        }

        let (page1, cursor) = list_manifests_for_project(&pool, "proj-tie", None, 2).await.unwrap();
        let (page2, _) =
            list_manifests_for_project(&pool, "proj-tie", cursor.as_deref(), 2).await.unwrap();

        let mut seen: Vec<String> =
            page1.iter().chain(page2.iter()).map(|r| r.id.clone()).collect();
        seen.sort();
        assert_eq!(seen, vec!["m-0000", "m-0001", "m-0002"]);
    }

    #[tokio::test]
    async fn list_manifests_rejects_a_cursor_it_did_not_produce() {
        let pool = setup().await;
        insert_project(&pool, "proj-bad").await;
        let err = list_manifests_for_project(&pool, "proj-bad", Some("2026-01-01T00:00:00Z"), 2)
            .await
            .unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }
}
