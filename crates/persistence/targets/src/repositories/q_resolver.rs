// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository query functions for the spec-035/042 target resolution cache and
//! bundled-seed loader (`crates/targeting/resolver`).
//!
//! Writes/reads the same `canonical_target` / `target_alias` tables as
//! [`super::targets`]. SIMBAD dedup/precedence business logic (source
//! overwrite rules, ranking, alias assembly) stays in
//! `targeting_resolver::cache` / `::seed` — this module only carries the raw
//! SQL.
//!
//! Most functions take `&SqlitePool` (matching [`super::targets`]). The
//! `*_conn` functions take `&mut SqliteConnection` instead: they are the
//! pieces of the find-existing/update-or-insert/replace-aliases sequence that
//! the resolver's `upsert_resolved_conn` runs against one shared connection so
//! the bundled-seed loader can batch many upserts into a single transaction
//! (one fsync for ~14k rows) — see `targeting_resolver::seed::load_seed`. A
//! `&mut sqlx::Transaction<'_, Sqlite>` derefs to `&mut SqliteConnection`, so
//! callers pass `&mut *tx` for the batched path and a plain acquired
//! connection otherwise.
//!
//! Constitution §I: read/write SQLite metadata only; no filesystem mutations.
//! Constitution §V: SQLite is the durable record.

use sqlx::{SqliteConnection, SqlitePool};

use persistence_core::DbResult;

// ── Row types ─────────────────────────────────────────────────────────────────

/// Flat `canonical_target` row (all 9 identity columns).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CanonicalTargetRow {
    pub id: String,
    pub simbad_oid: Option<i64>,
    pub primary_designation: String,
    pub display_alias: Option<String>,
    pub object_type: String,
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub source: String,
    pub resolved_at: String,
}

/// Flat `target_alias` row (alias + normalized + kind), no `target_id`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AliasRow {
    pub alias: String,
    pub normalized: String,
    pub kind: String,
}

/// Flat row for the gen-3 `target.list` surface (no aliases — see
/// [`list_all_target_aliases`] for the batched alias pass).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TargetListQueryRow {
    pub id: String,
    pub primary_designation: String,
    pub display_alias: Option<String>,
    pub object_type: String,
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub constellation: Option<String>,
    pub magnitude: Option<f64>,
}

/// `(target_id, alias)` pair used to batch-group aliases onto [`TargetListQueryRow`]s.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TargetAliasPairRow {
    pub target_id: String,
    pub alias: String,
}

/// The row a dedup lookup matches against: just enough to decide precedence
/// (`id` to upsert into, `source` to compare via `TargetSource::may_overwrite`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExistingTargetRow {
    pub id: String,
    pub source: String,
}

// ── Reads (pool) ─────────────────────────────────────────────────────────────

/// Read a `canonical_target` row by its persisted id.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn select_canonical_target_by_id(
    pool: &SqlitePool,
    id: &str,
) -> DbResult<Option<CanonicalTargetRow>> {
    let row = sqlx::query_as::<_, CanonicalTargetRow>(
        "SELECT id, simbad_oid, primary_designation, display_alias, object_type, ra_deg, dec_deg, source, resolved_at
         FROM canonical_target WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Read a `canonical_target` row by its SIMBAD physical-object id (the dedup key).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn select_canonical_target_by_simbad_oid(
    pool: &SqlitePool,
    simbad_oid: i64,
) -> DbResult<Option<CanonicalTargetRow>> {
    let row = sqlx::query_as::<_, CanonicalTargetRow>(
        "SELECT id, simbad_oid, primary_designation, display_alias, object_type, ra_deg, dec_deg, source, resolved_at
         FROM canonical_target WHERE simbad_oid = ?",
    )
    .bind(simbad_oid)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Resolve a normalized alias to its owning `target_id`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn select_target_id_by_normalized_alias(
    pool: &SqlitePool,
    normalized: &str,
) -> DbResult<Option<String>> {
    let target_id: Option<String> =
        sqlx::query_scalar("SELECT target_id FROM target_alias WHERE normalized = ? LIMIT 1")
            .bind(normalized)
            .fetch_optional(pool)
            .await?;
    Ok(target_id)
}

/// List the aliases for a target id, ordered by alias.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn select_aliases_for_target(
    pool: &SqlitePool,
    target_id: &str,
) -> DbResult<Vec<AliasRow>> {
    let rows = sqlx::query_as::<_, AliasRow>(
        "SELECT alias, normalized, kind
         FROM target_alias
         WHERE target_id = ?
         ORDER BY alias ASC",
    )
    .bind(target_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List all `canonical_target` rows (gen-3 `target.list` surface), ordered by
/// `primary_designation`. Aliases are NOT included — batch-load them
/// separately with [`list_all_target_aliases`] (avoids N+1 queries).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_canonical_targets(pool: &SqlitePool) -> DbResult<Vec<TargetListQueryRow>> {
    let rows = sqlx::query_as::<_, TargetListQueryRow>(
        "SELECT id, primary_designation, display_alias, object_type,
                ra_deg, dec_deg, constellation, magnitude
         FROM canonical_target
         ORDER BY primary_designation ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List every `(target_id, alias)` pair, ordered by `target_id` then `alias`,
/// for the batched grouping pass behind [`list_canonical_targets`].
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn list_all_target_aliases(pool: &SqlitePool) -> DbResult<Vec<TargetAliasPairRow>> {
    let rows = sqlx::query_as::<_, TargetAliasPairRow>(
        "SELECT target_id, alias
         FROM target_alias
         ORDER BY target_id ASC, alias ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Count `canonical_target` rows — the resolver's first-run detector (an
/// empty cache means the bundled seed has not been loaded yet).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn count_canonical_targets(pool: &SqlitePool) -> DbResult<i64> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM canonical_target").fetch_one(pool).await?;
    Ok(count)
}

// ── Writes (pool) — gen-3 management operations (spec 036/051) ─────────────

/// Insert a user-added alias (`kind = 'user'`). `INSERT OR IGNORE` tolerates
/// the `(target_id, normalized)` uniqueness constraint. Returns the number of
/// rows affected (`0` when the alias already existed).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn insert_user_alias(
    pool: &SqlitePool,
    alias_id: &str,
    target_id: &str,
    alias: &str,
    normalized: &str,
) -> DbResult<u64> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO target_alias (id, target_id, alias, normalized, kind)
         VALUES (?, ?, ?, ?, 'user')",
    )
    .bind(alias_id)
    .bind(target_id)
    .bind(alias)
    .bind(normalized)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Look up an existing alias id by `(target_id, normalized)` — used when
/// [`insert_user_alias`] reports 0 rows affected (already exists).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn select_alias_id_by_target_and_normalized(
    pool: &SqlitePool,
    target_id: &str,
    normalized: &str,
) -> DbResult<Option<String>> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT id FROM target_alias WHERE target_id = ? AND normalized = ?")
            .bind(target_id)
            .bind(normalized)
            .fetch_optional(pool)
            .await?;
    Ok(id)
}

/// Delete a user alias by its id, but only if its `kind = 'user'`. Returns
/// `true` if a row was deleted.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn delete_user_alias(pool: &SqlitePool, alias_id: &str) -> DbResult<bool> {
    let result = sqlx::query("DELETE FROM target_alias WHERE id = ? AND kind = 'user'")
        .bind(alias_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Set `canonical_target.display_alias`. Returns `true` if the target exists
/// and was updated.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn set_display_alias(
    pool: &SqlitePool,
    target_id: &str,
    display_alias: Option<&str>,
) -> DbResult<bool> {
    let result = sqlx::query("UPDATE canonical_target SET display_alias = ? WHERE id = ?")
        .bind(display_alias)
        .bind(target_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Clear `canonical_target.display_alias` (sets to NULL). Returns `true` if
/// the target exists and was updated.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn clear_display_alias(pool: &SqlitePool, target_id: &str) -> DbResult<bool> {
    let result = sqlx::query("UPDATE canonical_target SET display_alias = NULL WHERE id = ?")
        .bind(target_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// ── Atomic-unit steps (conn) — dedup upsert, spec 035 FR-007/FR-014 ────────
//
// These share one caller-supplied connection so `targeting_resolver::seed`
// can batch many upserts into a single transaction. See module docs.

/// Find the dedup-target row by SIMBAD oid (FR-007 precedence: an oid match
/// wins over the designation-derived id).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_existing_by_simbad_oid_conn(
    conn: &mut SqliteConnection,
    simbad_oid: i64,
) -> DbResult<Option<ExistingTargetRow>> {
    let row = sqlx::query_as::<_, ExistingTargetRow>(
        "SELECT id, source FROM canonical_target WHERE simbad_oid = ?",
    )
    .bind(simbad_oid)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// Find the dedup-target row by its designation-derived id (the fallback when
/// `simbad_oid` is null or did not match).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn find_existing_by_id_conn(
    conn: &mut SqliteConnection,
    id: &str,
) -> DbResult<Option<ExistingTargetRow>> {
    let row = sqlx::query_as::<_, ExistingTargetRow>(
        "SELECT id, source FROM canonical_target WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// Update an existing `canonical_target` row in place (keeps `id`, preserving
/// FK targets). `display_alias` is intentionally NOT touched — it is
/// user-owned and must never be overwritten by a re-resolution or seed load
/// (FR-012). `constellation`/`magnitude` are the spec-052 P1 enrichment
/// (`NULL`-tolerant — never fabricated when the caller passes `None`).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
#[allow(clippy::too_many_arguments)]
pub async fn update_canonical_target_conn(
    conn: &mut SqliteConnection,
    id: &str,
    simbad_oid: Option<i64>,
    primary_designation: &str,
    object_type: &str,
    ra_deg: f64,
    dec_deg: f64,
    source: &str,
    resolved_at: &str,
    constellation: Option<&str>,
    magnitude: Option<f64>,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE canonical_target SET
             simbad_oid          = ?,
             primary_designation = ?,
             object_type         = ?,
             ra_deg              = ?,
             dec_deg             = ?,
             source              = ?,
             resolved_at         = ?,
             constellation       = ?,
             magnitude           = ?
         WHERE id = ?",
    )
    .bind(simbad_oid)
    .bind(primary_designation)
    .bind(object_type)
    .bind(ra_deg)
    .bind(dec_deg)
    .bind(source)
    .bind(resolved_at)
    .bind(constellation)
    .bind(magnitude)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Insert a brand-new `canonical_target` row. `constellation`/`magnitude` are
/// the spec-052 P1 enrichment (`NULL`-tolerant — never fabricated when the
/// caller passes `None`).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
#[allow(clippy::too_many_arguments)]
pub async fn insert_canonical_target_conn(
    conn: &mut SqliteConnection,
    id: &str,
    simbad_oid: Option<i64>,
    primary_designation: &str,
    object_type: &str,
    ra_deg: f64,
    dec_deg: f64,
    source: &str,
    resolved_at: &str,
    constellation: Option<&str>,
    magnitude: Option<f64>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO canonical_target
             (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at, constellation, magnitude)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(simbad_oid)
    .bind(primary_designation)
    .bind(object_type)
    .bind(ra_deg)
    .bind(dec_deg)
    .bind(source)
    .bind(resolved_at)
    .bind(constellation)
    .bind(magnitude)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Delete all alias rows for `target_id` — the first half of the
/// delete+insert wholesale alias replacement a re-resolution does.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn delete_aliases_for_target_conn(
    conn: &mut SqliteConnection,
    target_id: &str,
) -> DbResult<()> {
    sqlx::query("DELETE FROM target_alias WHERE target_id = ?")
        .bind(target_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Insert one alias row. `INSERT OR IGNORE` tolerates the
/// `(target_id, normalized)` uniqueness constraint when SIMBAD returns the
/// same normalized form twice.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn insert_alias_conn(
    conn: &mut SqliteConnection,
    alias_id: &str,
    target_id: &str,
    alias: &str,
    normalized: &str,
    kind: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO target_alias (id, target_id, alias, normalized, kind)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(alias_id)
    .bind(target_id)
    .bind(alias)
    .bind(normalized)
    .bind(kind)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
