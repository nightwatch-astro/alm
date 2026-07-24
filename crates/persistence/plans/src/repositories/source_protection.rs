// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository for per-source protection overrides (spec 016 US2–US4, migration 0026).
//!
//! The `source_protection_state` table stores an explicit level override and
//! optional per-source `block_permanent_delete` and category overrides.
//! Absence of a row means the source inherits global defaults.

use domain_core::ids::Timestamp;
use serde_json::Value;
use sqlx::types::Json;
use sqlx::SqliteConnection;
use sqlx::SqlitePool;

use persistence_core::{DbError, DbResult};

// ── Row type ──────────────────────────────────────────────────────────────

/// Raw DB row for `source_protection_state`.
///
/// `categories` stays raw `String` (not `sqlx::types::Json`, unlike the other
/// columns in this file): `crates/app/core::protection::set_source_protection`
/// reads this field directly with its own lenient `serde_json::from_str`
/// fallback, so this row's shape is a cross-crate contract this file cannot
/// unilaterally change.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct SourceProtectionRow {
    pub source_id: String,
    pub level: String,
    pub block_permanent_delete: Option<i64>,
    pub categories: Option<String>,
    pub updated_at: String,
    pub updated_by: String,
}

// ── Resolved effective protection ────────────────────────────────────────

/// Effective protection resolved for a source.
#[derive(Clone, Debug)]
pub struct ResolvedProtection {
    /// Effective protection level.
    pub level: String,
    /// Effective block_permanent_delete flag.
    pub block_permanent_delete: bool,
    /// Effective protected categories.
    pub categories: Vec<String>,
    /// True when no per-source override row exists (global defaults used).
    pub inherits_default: bool,
}

// ── Global defaults fallback ──────────────────────────────────────────────

const DEFAULT_LEVEL: &str = "protected";
const DEFAULT_BLOCK_PERMANENT_DELETE: bool = true;
const DEFAULT_CATEGORIES: &[&str] = &["lights", "masters", "finals"];

// ── Helpers ───────────────────────────────────────────────────────────────

fn parse_categories(json: Option<&str>) -> DbResult<Vec<String>> {
    match json {
        None => Ok(vec![]),
        Some(s) => {
            let v: Value = serde_json::from_str(s).map_err(DbError::Serialise)?;
            let arr = v.as_array().ok_or_else(|| {
                DbError::Serialise(serde_json::from_str::<Vec<String>>("null").unwrap_err())
            })?;
            Ok(arr.iter().filter_map(|x| x.as_str().map(std::borrow::ToOwned::to_owned)).collect())
        }
    }
}

// ── CRUD ──────────────────────────────────────────────────────────────────

/// Upsert a per-source protection override.
///
/// `block_permanent_delete = None` means inherit global (stores NULL in DB).
/// `categories = None` means inherit global categories (stores NULL in DB).
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure or [`DbError::Serialise`]
/// on JSON encoding failure.
pub async fn upsert_source_protection(
    pool: &SqlitePool,
    source_id: &str,
    level: &str,
    block_permanent_delete: Option<bool>,
    categories: Option<&[String]>,
    updated_by: &str,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    let bpd: Option<i64> = block_permanent_delete.map(i64::from);
    let cats_json = categories.map(Json);

    sqlx::query(
        "INSERT INTO source_protection_state \
         (source_id, level, block_permanent_delete, categories, updated_at, updated_by) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(source_id) DO UPDATE SET \
           level = excluded.level, \
           block_permanent_delete = excluded.block_permanent_delete, \
           categories = excluded.categories, \
           updated_at = excluded.updated_at, \
           updated_by = excluded.updated_by",
    )
    .bind(source_id)
    .bind(level)
    .bind(bpd)
    .bind(cats_json)
    .bind(&now)
    .bind(updated_by)
    .execute(pool)
    .await?;

    Ok(())
}

/// Read the raw override row for a source, if one exists.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn get_source_protection_row(
    pool: &SqlitePool,
    source_id: &str,
) -> DbResult<Option<SourceProtectionRow>> {
    let row = sqlx::query_as::<_, SourceProtectionRow>(
        "SELECT source_id, level, block_permanent_delete, categories, updated_at, updated_by \
         FROM source_protection_state WHERE source_id = ?",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Delete the per-source override row, reverting to global default inheritance.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn delete_source_protection(pool: &SqlitePool, source_id: &str) -> DbResult<()> {
    sqlx::query("DELETE FROM source_protection_state WHERE source_id = ?")
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Resolve effective protection for a source.
///
/// Implements the precedence rule from data-model.md §Resolver:
/// 1. If an override row exists, return it unconditionally (categories NOT checked).
/// 2. Otherwise use global defaults; if `category` is in protected list, level → `protected`.
///
/// Global defaults are read from the `settings` table via the caller-supplied values.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure or [`DbError::Serialise`]
/// on JSON decode failure.
pub async fn resolve_protection(
    pool: &SqlitePool,
    source_id: &str,
    category: Option<&str>,
    global_level: &str,
    global_block_permanent_delete: bool,
    global_categories: &[String],
) -> DbResult<ResolvedProtection> {
    if let Some(row) = get_source_protection_row(pool, source_id).await? {
        // Override row exists — it wins unconditionally.
        let cats = parse_categories(row.categories.as_deref())?;
        let effective_cats = if cats.is_empty() { global_categories.to_vec() } else { cats };
        let bpd = match row.block_permanent_delete {
            None => global_block_permanent_delete,
            Some(v) => v != 0,
        };
        return Ok(ResolvedProtection {
            level: row.level,
            block_permanent_delete: bpd,
            categories: effective_cats,
            inherits_default: false,
        });
    }

    // No override row — use global defaults, with optional category elevation.
    let effective_level = if let Some(cat) = category {
        if global_categories.iter().any(|c| c == cat) {
            "protected"
        } else {
            global_level
        }
    } else {
        global_level
    };

    Ok(ResolvedProtection {
        level: effective_level.to_owned(),
        block_permanent_delete: global_block_permanent_delete,
        categories: global_categories.to_vec(),
        inherits_default: true,
    })
}

// ── Protection defaults (migration 0035, FR-018) ─────────────────────────

/// Raw DB row from the `protection_defaults` table.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ProtectionDefaultRow {
    pub scope: String,
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

/// Get the raw JSON value for a specific (scope, key) protection default.
///
/// Returns `None` when no row exists (caller should fall back to hard-coded
/// defaults).
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn get_protection_default(
    pool: &SqlitePool,
    scope: &str,
    key: &str,
) -> DbResult<Option<serde_json::Value>> {
    let row: Option<(Json<Value>,)> =
        sqlx::query_as("SELECT value FROM protection_defaults WHERE scope = ? AND key = ?")
            .bind(scope)
            .bind(key)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|(Json(v),)| v))
}

/// Upsert a (scope, key, value) protection default row.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure or [`DbError::Serialise`]
/// on JSON encoding failure.
pub async fn set_protection_default(
    pool: &SqlitePool,
    scope: &str,
    key: &str,
    value: &serde_json::Value,
) -> DbResult<()> {
    let now = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO protection_defaults (scope, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(scope)
    .bind(key)
    .bind(Json(value))
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Write (upsert) a protection default on an existing connection.
///
/// Accepts a bare `SqliteConnection` so callers can participate in an
/// open transaction without needing a pool.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure or [`DbError::Serialise`]
/// on JSON encoding failure.
pub async fn set_protection_default_with_conn(
    conn: &mut SqliteConnection,
    scope: &str,
    key: &str,
    value: &serde_json::Value,
) -> DbResult<()> {
    let now = Timestamp::now_iso();

    sqlx::query(
        "INSERT INTO protection_defaults (scope, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(scope)
    .bind(key)
    .bind(Json(value))
    .bind(&now)
    .execute(conn)
    .await?;

    Ok(())
}

/// List all protection defaults for a given scope.
///
/// # Errors
///
/// Returns [`DbError::Database`] on query failure.
pub async fn list_protection_defaults(
    pool: &SqlitePool,
    scope: &str,
) -> DbResult<Vec<ProtectionDefaultRow>> {
    let rows: Vec<ProtectionDefaultRow> = sqlx::query_as(
        "SELECT scope, key, value, updated_at FROM protection_defaults WHERE scope = ? ORDER BY key ASC",
    )
    .bind(scope)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Resolve effective protection using hard-coded fallback defaults.
///
/// Used when global settings row is absent (e.g. first run before migration).
///
/// # Errors
///
/// Returns [`DbError::Database`] or [`DbError::Serialise`] on failure.
pub async fn resolve_protection_with_fallback(
    pool: &SqlitePool,
    source_id: &str,
    category: Option<&str>,
) -> DbResult<ResolvedProtection> {
    let defaults: Vec<String> = DEFAULT_CATEGORIES.iter().map(|s| (*s).to_owned()).collect();
    resolve_protection(
        pool,
        source_id,
        category,
        DEFAULT_LEVEL,
        DEFAULT_BLOCK_PERMANENT_DELETE,
        &defaults,
    )
    .await
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::setup_db;

    #[tokio::test]
    async fn upsert_and_read_round_trip() {
        let db = setup_db().await;
        let source_id = "src-001";

        upsert_source_protection(db.pool(), source_id, "unprotected", Some(false), None, "user")
            .await
            .unwrap();

        let row = get_source_protection_row(db.pool(), source_id)
            .await
            .unwrap()
            .expect("row should exist");

        assert_eq!(row.level, "unprotected");
        assert_eq!(row.block_permanent_delete, Some(0));
        assert!(row.categories.is_none());
    }

    #[tokio::test]
    async fn upsert_updates_existing_row() {
        let db = setup_db().await;
        let source_id = "src-002";

        upsert_source_protection(db.pool(), source_id, "protected", None, None, "user")
            .await
            .unwrap();
        upsert_source_protection(db.pool(), source_id, "unprotected", Some(true), None, "user")
            .await
            .unwrap();

        let row = get_source_protection_row(db.pool(), source_id)
            .await
            .unwrap()
            .expect("row should exist");

        assert_eq!(row.level, "unprotected");
        assert_eq!(row.block_permanent_delete, Some(1));
    }

    #[tokio::test]
    async fn delete_removes_row() {
        let db = setup_db().await;
        let source_id = "src-003";

        upsert_source_protection(db.pool(), source_id, "unprotected", None, None, "user")
            .await
            .unwrap();
        delete_source_protection(db.pool(), source_id).await.unwrap();

        let row = get_source_protection_row(db.pool(), source_id).await.unwrap();
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn resolve_returns_override_when_present() {
        let db = setup_db().await;
        let source_id = "src-004";
        let global_cats = vec!["lights".to_owned(), "masters".to_owned()];

        upsert_source_protection(db.pool(), source_id, "unprotected", Some(false), None, "user")
            .await
            .unwrap();

        let resolved = resolve_protection(
            db.pool(),
            source_id,
            Some("lights"),
            "protected",
            true,
            &global_cats,
        )
        .await
        .unwrap();

        // Override wins — level is "unprotected" even though "lights" is in protected categories.
        assert_eq!(resolved.level, "unprotected");
        assert!(!resolved.block_permanent_delete);
        assert!(!resolved.inherits_default);
    }

    #[tokio::test]
    async fn resolve_elevates_level_for_protected_category() {
        let db = setup_db().await;
        let source_id = "src-005";
        let global_cats = vec!["lights".to_owned(), "masters".to_owned()];

        // No override row.
        let resolved = resolve_protection(
            db.pool(),
            source_id,
            Some("lights"),
            "unprotected",
            true,
            &global_cats,
        )
        .await
        .unwrap();

        assert_eq!(resolved.level, "protected");
        assert!(resolved.inherits_default);
    }

    #[tokio::test]
    async fn resolve_uses_global_defaults_when_no_override() {
        let db = setup_db().await;
        let source_id = "src-006";
        let global_cats: Vec<String> = vec![];

        let resolved =
            resolve_protection(db.pool(), source_id, None, "unprotected", false, &global_cats)
                .await
                .unwrap();

        assert_eq!(resolved.level, "unprotected");
        assert!(!resolved.block_permanent_delete);
        assert!(resolved.inherits_default);
    }

    #[tokio::test]
    async fn categories_with_override_row() {
        let db = setup_db().await;
        let source_id = "src-007";
        let per_source_cats = vec!["finals".to_owned()];

        upsert_source_protection(
            db.pool(),
            source_id,
            "protected",
            None,
            Some(&per_source_cats),
            "user",
        )
        .await
        .unwrap();

        let global_cats = vec!["lights".to_owned(), "masters".to_owned()];
        let resolved =
            resolve_protection(db.pool(), source_id, None, "unprotected", true, &global_cats)
                .await
                .unwrap();

        assert_eq!(resolved.categories, vec!["finals".to_owned()]);
        assert!(!resolved.inherits_default);
    }
}
