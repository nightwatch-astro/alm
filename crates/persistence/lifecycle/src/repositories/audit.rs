// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Read access over the durable `audit_log_entry` table (migration
//! `0002_lifecycle.sql`).
//!
//! Rows are written by `LifecycleRepository::record_transition` /
//! `record_refused_transition` (`crate::repositories::lifecycle`), by
//! `targets::target_resolve::write_audit` (target resolution), and by
//! `projects::project_health::write_auto_transition_audit` (system-driven
//! project transitions). This module is the first *read* path over that
//! table — until now `audit_list` / `audit_export` were spec-029 stub
//! handlers returning a hardcoded fixture, never touching the database.
//!
//! Kept as free functions over a plain `&SqlitePool` (mirroring
//! `crate::repositories` sibling `log_stream`-style readers in `app_core`)
//! rather than a new `LifecycleRepository` trait method: the table is a
//! single append-only log with one natural read shape, so a full
//! repository-trait indirection (and the matching in-memory test double)
//! would add ceremony without a second implementation to justify it.

use std::fmt::Write as _;

use audit_types::AuditLogEntry;
use sqlx::SqlitePool;

use persistence_core::repositories::audit_writes;
use persistence_core::DbResult;

/// One row from `audit_log_entry`, as stored (no enum parsing — that is a
/// concern of the IPC/contract layer, which maps these strings onto the
/// `AuditActor`/`AuditOutcome` contract enums).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AuditLogRow {
    pub audit_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub from_state: Option<String>,
    pub to_state: Option<String>,
    pub trigger: String,
    pub actor: String,
    pub outcome: String,
    pub severity: String,
    pub request_id: String,
    pub at: String,
    pub payload: Option<String>,
    /// Machine-readable reason/code for `refused`/`failed` outcomes (T120
    /// migration 0063); `NULL` for `applied`.
    pub reason_code: Option<String>,
}

/// Filter for `audit_log_entry` queries. All fields are AND-combined.
///
/// Deliberately generic: `entity_type` + `entity_id` are the key fields a
/// future per-entity history view (e.g. an archive-detail audit trail) would
/// reuse, so they are plain equality filters rather than something bespoke to
/// the settings Audit Log screen.
#[derive(Clone, Debug, Default)]
pub struct AuditLogFilter {
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    /// Exact match against the `outcome` column (`applied` | `refused` | `failed`).
    pub outcome: Option<String>,
    /// Exact match against the `severity` column (`workflow` | `diagnostic`).
    pub severity: Option<String>,
    /// Exact match against the `reason_code` column (T120 migration 0063).
    pub reason_code: Option<String>,
    /// RFC 3339 lower bound on `at` (inclusive).
    pub from: Option<String>,
    /// RFC 3339 upper bound on `at` (exclusive).
    pub to: Option<String>,
    /// Case-insensitive substring match against `entity_type`, `entity_id`,
    /// `trigger`, or `actor`.
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

// Delegate to the shared helper in persistence_core (bd astro-plan-kyo7.88).
use persistence_core::repositories::sql::escape_like;

/// Shared WHERE-clause builder for `list_audit_entries` / `count_audit_entries`
/// so the filter semantics can never drift between the two queries.
///
/// Returns the `WHERE …` fragment (empty string when unfiltered) and the
/// ordered bind values.
fn build_where(filter: &AuditLogFilter) -> (String, Vec<String>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref v) = filter.entity_type {
        clauses.push("entity_type = ?".to_owned());
        binds.push(v.clone());
    }
    if let Some(ref v) = filter.entity_id {
        clauses.push("entity_id = ?".to_owned());
        binds.push(v.clone());
    }
    if let Some(ref v) = filter.outcome {
        clauses.push("outcome = ?".to_owned());
        binds.push(v.clone());
    }
    if let Some(ref v) = filter.severity {
        clauses.push("severity = ?".to_owned());
        binds.push(v.clone());
    }
    if let Some(ref v) = filter.reason_code {
        clauses.push("reason_code = ?".to_owned());
        binds.push(v.clone());
    }
    if let Some(ref v) = filter.from {
        clauses.push("at >= ?".to_owned());
        binds.push(v.clone());
    }
    if let Some(ref v) = filter.to {
        clauses.push("at < ?".to_owned());
        binds.push(v.clone());
    }
    if let Some(ref v) = filter.search {
        clauses.push(
            "(LOWER(entity_type) LIKE ? ESCAPE '\\' OR LOWER(entity_id) LIKE ? ESCAPE '\\' \
              OR LOWER(trigger) LIKE ? ESCAPE '\\' OR LOWER(actor) LIKE ? ESCAPE '\\')"
                .to_owned(),
        );
        let pattern = format!("%{}%", escape_like(&v.to_lowercase()));
        for _ in 0..4 {
            binds.push(pattern.clone());
        }
    }

    if clauses.is_empty() {
        (String::new(), binds)
    } else {
        (format!(" WHERE {}", clauses.join(" AND ")), binds)
    }
}

/// List `audit_log_entry` rows matching `filter`, newest-first.
///
/// # Errors
/// Returns `DbError::Database` if the query fails.
pub async fn list_audit_entries(
    pool: &SqlitePool,
    filter: &AuditLogFilter,
) -> DbResult<Vec<AuditLogRow>> {
    let (where_sql, binds) = build_where(filter);

    let mut sql = format!(
        "SELECT audit_id, entity_type, entity_id, from_state, to_state, trigger, actor, \
         outcome, severity, request_id, at, payload, reason_code \
         FROM audit_log_entry{where_sql} ORDER BY at DESC, audit_id DESC"
    );

    if let Some(limit) = filter.limit {
        let _ = write!(sql, " LIMIT {limit}");
        if let Some(offset) = filter.offset {
            let _ = write!(sql, " OFFSET {offset}");
        }
    } else if let Some(offset) = filter.offset {
        // SQLite requires LIMIT when OFFSET is present.
        let _ = write!(sql, " LIMIT -1 OFFSET {offset}");
    }

    // AssertSqlSafe: `where_sql` is built exclusively from static clause
    // fragments in `build_where`; every user-supplied value flows through a
    // `?` placeholder bound below. `limit`/`offset` are integer literals
    // derived from typed `u32` filter fields, never user strings.
    let mut q = sqlx::query_as::<_, AuditLogRow>(sqlx::AssertSqlSafe(sql));
    for v in &binds {
        q = q.bind(v);
    }

    Ok(q.fetch_all(pool).await?)
}

/// Count `audit_log_entry` rows matching `filter` (ignores `limit`/`offset`).
///
/// # Errors
/// Returns `DbError::Database` if the query fails.
pub async fn count_audit_entries(pool: &SqlitePool, filter: &AuditLogFilter) -> DbResult<u32> {
    let (where_sql, binds) = build_where(filter);
    let sql = format!("SELECT COUNT(*) FROM audit_log_entry{where_sql}");

    // AssertSqlSafe: same reasoning as `list_audit_entries` — `where_sql` is
    // built only from static fragments; all dynamic values are bound below.
    let mut q = sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(sql));
    for v in &binds {
        q = q.bind(v);
    }

    let (count,) = q.fetch_one(pool).await?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

/// Insert a system-driven ("auto") project lifecycle-transition audit row.
///
/// Owns the raw `INSERT` for `app_core_projects::project_health` so no `sqlx`
/// lives in the app layer (db-boundary rule). Fixed columns for this path:
/// `entity_type = 'project'`, `actor = 'system'`, `outcome = 'applied'`,
/// `severity = 'workflow'`, `payload = NULL`. Generates the `audit_id` /
/// `request_id` UUIDs and the RFC3339 `at` timestamp.
///
/// # Errors
/// Returns `persistence_core::DbError`(persistence_core::DbError) if the insert fails.
pub async fn insert_project_auto_transition(
    pool: &SqlitePool,
    project_id: &str,
    from_state: &str,
    to_state: &str,
    trigger: &str,
) -> DbResult<()> {
    audit_writes::insert_project_auto_transition(pool, project_id, from_state, to_state, trigger)
        .await
}

/// Insert a generalized `audit_log_entry` row from an `AuditLogEntry`.
///
/// Delegates to [`persistence_core::repositories::audit_writes::insert_audit_entry`].
///
/// # Errors
/// Returns `persistence_core::DbError`(persistence_core::DbError) if the insert fails.
pub async fn insert_audit_entry(pool: &SqlitePool, entry: &AuditLogEntry) -> DbResult<()> {
    audit_writes::insert_audit_entry(pool, entry).await
}

#[cfg(test)]
mod tests {
    use super::{count_audit_entries, list_audit_entries, AuditLogFilter};
    use sqlx::SqlitePool;

    async fn make_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("in-memory pool");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS audit_log_entry (\
             audit_id TEXT PRIMARY KEY NOT NULL,\
             entity_type TEXT NOT NULL,\
             entity_id TEXT NOT NULL,\
             from_state TEXT,\
             to_state TEXT,\
             trigger TEXT NOT NULL,\
             actor TEXT NOT NULL CHECK (actor IN ('user', 'system')),\
             outcome TEXT NOT NULL CHECK (outcome IN ('applied', 'refused', 'failed')),\
             severity TEXT NOT NULL CHECK (severity IN ('workflow', 'diagnostic')),\
             request_id TEXT NOT NULL,\
             at TEXT NOT NULL,\
             payload TEXT,\
             reason_code TEXT\
             )",
        )
        .execute(&pool)
        .await
        .expect("create audit_log_entry table");
        pool
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert(
        pool: &SqlitePool,
        audit_id: &str,
        entity_type: &str,
        entity_id: &str,
        trigger: &str,
        actor: &str,
        outcome: &str,
        severity: &str,
        at: &str,
        payload: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO audit_log_entry \
             (audit_id, entity_type, entity_id, from_state, to_state, trigger, actor, \
              outcome, severity, request_id, at, payload) \
             VALUES (?, ?, ?, NULL, NULL, ?, ?, ?, ?, 'req-1', ?, ?)",
        )
        .bind(audit_id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(trigger)
        .bind(actor)
        .bind(outcome)
        .bind(severity)
        .bind(at)
        .bind(payload)
        .execute(pool)
        .await
        .expect("insert audit_log_entry row");
    }

    #[tokio::test]
    async fn list_returns_newest_first() {
        let pool = make_pool().await;
        insert(
            &pool,
            "a1",
            "session",
            "ses-1",
            "confirm",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;
        insert(
            &pool,
            "a2",
            "session",
            "ses-1",
            "confirm",
            "user",
            "applied",
            "workflow",
            "2026-01-02T00:00:00Z",
            None,
        )
        .await;

        let rows = list_audit_entries(&pool, &AuditLogFilter::default()).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].audit_id, "a2");
        assert_eq!(rows[1].audit_id, "a1");
    }

    #[tokio::test]
    async fn filters_by_entity_type_and_entity_id() {
        let pool = make_pool().await;
        insert(
            &pool,
            "a1",
            "session",
            "ses-1",
            "confirm",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;
        insert(
            &pool,
            "a2",
            "plan",
            "plan-1",
            "approve",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { entity_type: Some("plan".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].entity_id, "plan-1");

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { entity_id: Some("ses-1".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audit_id, "a1");
    }

    #[tokio::test]
    async fn filters_by_outcome_and_severity() {
        let pool = make_pool().await;
        insert(
            &pool,
            "a1",
            "session",
            "ses-1",
            "confirm",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;
        insert(
            &pool,
            "a2",
            "session",
            "ses-1",
            "confirm",
            "system",
            "refused",
            "workflow",
            "2026-01-01T00:00:00Z",
            Some(r#"{"refusal":{"code":"x","message":"m"}}"#),
        )
        .await;

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { outcome: Some("refused".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audit_id, "a2");

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { severity: Some("diagnostic".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn filters_by_time_range() {
        let pool = make_pool().await;
        insert(
            &pool,
            "a1",
            "session",
            "ses-1",
            "confirm",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;
        insert(
            &pool,
            "a2",
            "session",
            "ses-1",
            "confirm",
            "user",
            "applied",
            "workflow",
            "2026-02-01T00:00:00Z",
            None,
        )
        .await;

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { from: Some("2026-01-15T00:00:00Z".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audit_id, "a2");

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { to: Some("2026-01-15T00:00:00Z".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audit_id, "a1");
    }

    #[tokio::test]
    async fn filters_by_search_across_columns() {
        let pool = make_pool().await;
        insert(
            &pool,
            "a1",
            "session",
            "ses-1",
            "Confirm session",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;
        insert(
            &pool,
            "a2",
            "plan",
            "plan-1",
            "Approve plan",
            "system",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { search: Some("CONFIRM".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audit_id, "a1");

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { search: Some("system".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audit_id, "a2");
    }

    #[tokio::test]
    async fn search_escapes_like_wildcards() {
        let pool = make_pool().await;
        // Astro names commonly contain `_` (e.g. `M31_L`); it must match
        // literally, not as a single-char LIKE wildcard (which would also
        // match `M31xL`). Same for `%`.
        insert(
            &pool,
            "a1",
            "session",
            "M31_L",
            "Confirm session",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;
        insert(
            &pool,
            "a2",
            "session",
            "M31xL",
            "Confirm session",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;
        insert(
            &pool,
            "a3",
            "session",
            "gain 100%",
            "Confirm session",
            "user",
            "applied",
            "workflow",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;

        // `_` is literal: only the underscore row matches, not `M31xL`.
        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { search: Some("M31_L".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audit_id, "a1");

        // `%` is literal: matches the percent row, not everything.
        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { search: Some("100%".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audit_id, "a3");

        // Count agrees with list under the same escaped filter.
        let total = count_audit_entries(
            &pool,
            &AuditLogFilter { search: Some("M31_L".to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(total, 1);
    }

    #[tokio::test]
    async fn pagination_limit_and_offset() {
        let pool = make_pool().await;
        for i in 0..5 {
            insert(
                &pool,
                &format!("a{i}"),
                "session",
                "ses-1",
                "confirm",
                "user",
                "applied",
                "workflow",
                &format!("2026-01-0{}T00:00:00Z", i + 1),
                None,
            )
            .await;
        }

        let rows = list_audit_entries(
            &pool,
            &AuditLogFilter { limit: Some(2), offset: Some(1), ..Default::default() },
        )
        .await
        .unwrap();
        // Newest-first: a4, a3, a2, a1, a0 — offset 1, limit 2 → a3, a2.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].audit_id, "a3");
        assert_eq!(rows[1].audit_id, "a2");
    }

    #[tokio::test]
    async fn count_matches_filtered_total_ignoring_pagination() {
        let pool = make_pool().await;
        for i in 0..5 {
            insert(
                &pool,
                &format!("a{i}"),
                "session",
                "ses-1",
                "confirm",
                "user",
                "applied",
                "workflow",
                &format!("2026-01-0{}T00:00:00Z", i + 1),
                None,
            )
            .await;
        }

        let total = count_audit_entries(
            &pool,
            &AuditLogFilter { limit: Some(2), offset: Some(1), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(total, 5);
    }
}
