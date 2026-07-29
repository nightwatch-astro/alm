// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Resolution cache: read/write, dedupe by SIMBAD oid, source precedence.
//!
//! The local SQLite cache is the durable record (constitution §V). Entries are
//! deduplicated by `simbad_oid` when non-null (spec 035 FR-007); a
//! `user-override` row takes precedence over `resolved`/`seed` and a later
//! SIMBAD resolution MUST NOT overwrite it (FR-014).
//!
//! These functions take a borrowed [`sqlx::SqlitePool`] rather than owning a
//! connection, matching the spec-013 loader (`crate::load`) and the
//! `persistence_db` repositories. Raw SQL lives in
//! `persistence_targets::repositories::q_resolver` (db-boundary-zero); this module
//! keeps the dedup/precedence/ranking business logic and converts between the
//! repository's flat rows and this crate's domain types. Identities are
//! written to `canonical_target` / `target_alias` (migration 0031).
//!
//! Typeahead/search moved to the `simbad-resolver` facade's redb cache (spec
//! 052 P1 D1) — this module is now the durable read/write surface for
//! already-adopted (in-use) targets only.
//!
//! Writes here are the in-use "promote from cache" commit points (FR-004):
//! favourite, session-link (ingest), manual override, and project-create.
//! Every write enriches `magnitude` (from `ResolvedIdentity.v_mag`) and
//! `constellation` (IAU constellation-from-coordinates via skymath 0.3) —
//! spec 052 P1 D8 — never fabricated: both stay `NULL` when the source lacks
//! them or the coordinates are out of range.

use domain_core::ids::Timestamp;
use persistence_targets::repositories::q_resolver;
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::{AliasKind, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource};
use targeting::identity::target_id_from_designation;

/// IAU constellation abbreviation (e.g. `"And"`) for `(ra_deg, dec_deg)`, or
/// `None` when the coordinates are out of the valid ICRS J2000 range (never
/// fabricated — spec 052 P1 D8, INV-4).
fn constellation_abbreviation(ra_deg: f64, dec_deg: f64) -> Option<String> {
    let eq = skymath::Equatorial::j2000(
        skymath::Angle::from_degrees(ra_deg),
        skymath::Angle::from_degrees(dec_deg),
    )
    .ok()?;
    Some(skymath::constellation(eq).abbreviation().to_owned())
}

// ── Error ───────────────────────────────────────────────────────────────────────

/// Error type for resolution-cache reads/writes.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Underlying SQLite query failure.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    /// A `persistence_db` repository call failed.
    #[error("persistence error: {0}")]
    Persistence(#[from] persistence_core::DbError),
    /// A stored `canonical_target.id` was not a valid UUID.
    #[error("failed to parse target uuid '{0}': {1}")]
    InvalidUuid(String, uuid::Error),
    /// A stored `canonical_target.source` was outside the closed enum.
    #[error("unknown source value in cache: '{0}'")]
    InvalidSource(String),
}

/// Convenience result alias for cache operations.
pub type CacheResult<T> = Result<T, CacheError>;

// ── Read model ────────────────────────────────────────────────────────────────

/// A cached canonical target plus its aliases, as read back from the cache.
///
/// Mirrors [`ResolvedIdentity`] but additionally carries the persisted
/// [`CachedTarget::id`], [`CachedTarget::resolved_at`], and the optional
/// user-set [`CachedTarget::display_alias`] (FR-012 — user-owned, never
/// overwritten by re-resolution).
#[derive(Clone, Debug, PartialEq)]
pub struct CachedTarget {
    /// The persisted `canonical_target.id` (UUIDv5).
    pub id: Uuid,
    /// SIMBAD physical-object id (dedup key) when resolved online.
    pub simbad_oid: Option<i64>,
    /// Canonical display designation.
    pub primary_designation: String,
    /// User-set presentation label; `None` when not set.
    pub display_alias: Option<String>,
    /// Closed object-type enum.
    pub object_type: ObjectType,
    /// ICRS J2000 right ascension in decimal degrees.
    pub ra_deg: f64,
    /// ICRS J2000 declination in decimal degrees.
    pub dec_deg: f64,
    /// Provenance of the stored identity.
    pub source: TargetSource,
    /// RFC 3339 timestamp of the last seed/resolve/override.
    pub resolved_at: String,
    /// All aliases (designations + common names + user-added) for this target.
    pub aliases: Vec<ResolvedAlias>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// The deterministic id a brand-new row would get for `designation`.
#[must_use]
fn derived_id(designation: &str) -> Uuid {
    target_id_from_designation(designation)
}

// ── Reads ───────────────────────────────────────────────────────────────────

/// Load the aliases for a target id, ordered by alias.
async fn load_aliases(pool: &SqlitePool, target_id: &str) -> CacheResult<Vec<ResolvedAlias>> {
    let rows = q_resolver::select_aliases_for_target(pool, target_id).await?;
    Ok(rows
        .into_iter()
        .map(|r| ResolvedAlias {
            alias: r.alias,
            normalized: r.normalized,
            kind: AliasKind::from_wire(&r.kind),
        })
        .collect())
}

/// Assemble a [`CachedTarget`] from a `canonical_target` repository row.
async fn assemble(
    pool: &SqlitePool,
    row: q_resolver::CanonicalTargetRow,
) -> CacheResult<CachedTarget> {
    let q_resolver::CanonicalTargetRow {
        id: id_str,
        simbad_oid,
        primary_designation,
        display_alias,
        object_type,
        ra_deg,
        dec_deg,
        source,
        resolved_at,
    } = row;
    let id = Uuid::parse_str(&id_str).map_err(|e| CacheError::InvalidUuid(id_str.clone(), e))?;
    let source = TargetSource::from_wire(&source)
        .ok_or_else(|| CacheError::InvalidSource(source.clone()))?;
    let aliases = load_aliases(pool, &id_str).await?;
    Ok(CachedTarget {
        id,
        simbad_oid,
        primary_designation,
        display_alias,
        object_type: ObjectType::from_wire(&object_type),
        ra_deg,
        dec_deg,
        source,
        resolved_at,
        aliases,
    })
}

/// Read a cached target by its persisted id.
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure, or [`CacheError::InvalidUuid`] /
/// [`CacheError::InvalidSource`] on a corrupt stored value.
pub async fn get_by_id(pool: &SqlitePool, id: Uuid) -> CacheResult<Option<CachedTarget>> {
    let row = q_resolver::select_canonical_target_by_id(pool, &id.to_string()).await?;
    match row {
        None => Ok(None),
        Some(r) => Ok(Some(assemble(pool, r).await?)),
    }
}

/// Read a cached target by its SIMBAD physical-object id (the dedup key).
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure, or a parse error on a
/// corrupt stored value.
pub async fn get_by_simbad_oid(
    pool: &SqlitePool,
    simbad_oid: i64,
) -> CacheResult<Option<CachedTarget>> {
    let row = q_resolver::select_canonical_target_by_simbad_oid(pool, simbad_oid).await?;
    match row {
        None => Ok(None),
        Some(r) => Ok(Some(assemble(pool, r).await?)),
    }
}

/// Read a cached target by a normalized alias (the typeahead match surface).
///
/// `normalized` must already be normalized via [`targeting::normalize::normalize`],
/// which delegates to [`simbad_resolver::normalize::normalize`] — the single
/// normalization choke-point (spec 052 D6/T004, #701) that this crate's own
/// cache writes also normalize through, so lookups always agree with stored
/// aliases regardless of which copy a caller reaches through. This is an
/// exact-alias lookup, NOT a prefix/substring search (that is T010).
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure, or a parse error on a
/// corrupt stored value.
pub async fn get_by_normalized(
    pool: &SqlitePool,
    normalized: &str,
) -> CacheResult<Option<CachedTarget>> {
    let target_id = q_resolver::select_target_id_by_normalized_alias(pool, normalized).await?;
    match target_id {
        None => Ok(None),
        Some(tid) => {
            let uuid =
                Uuid::parse_str(&tid).map_err(|e| CacheError::InvalidUuid(tid.clone(), e))?;
            get_by_id(pool, uuid).await
        }
    }
}

// ── Typeahead search ─────────────────────────────────────────────────────────
//
// Spec 052 P1 (D1): the hand-rolled SQLite `search_by_normalized`/
// `search_fuzzy` typeahead were replaced by the `simbad-resolver` facade's own
// `SimbadResolver::search()` over the shared redb cache (see
// `crate::simbad::SimbadResolver::search` +
// `crate::simbad::from_crate_search_hit`); pure search/typeahead no longer
// touches SQLite at all (FR-004/SC-002 — browsing never writes
// `canonical_target`). `targeting_resolver::cache` keeps only the durable
// read/write surface for already-adopted (in-use) targets; [`SearchHit`]
// itself stays here as the shared read-model shape both the (now-removed)
// SQL search and the redb-backed search converge on.

/// One ranked typeahead hit — the matched (redb-cache) canonical target plus
/// the alias that matched and its rank bucket (`0` exact, `1` prefix, `2`
/// substring, `3` fuzzy; see [`simbad_resolver::RANK_EXACT`] and friends).
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    /// The matched target (aliases loaded).
    pub target: CachedTarget,
    /// The display form of the alias that matched.
    pub matched_alias: String,
    /// Rank bucket.
    pub rank: u8,
}

// ── Writes ──────────────────────────────────────────────────────────────────

/// Outcome of an [`upsert_resolved`] call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpsertOutcome {
    /// A new canonical_target row was inserted.
    Inserted,
    /// An existing row was updated (matched by oid or derived id).
    Updated,
    /// The write was skipped because an existing `user-override` row takes
    /// precedence (FR-014).
    SkippedUserOverride,
}

/// The result of resolving the target row to upsert against.
struct ExistingRow {
    id: String,
    source: TargetSource,
}

/// Find the row this identity should upsert into.
///
/// Dedup precedence (FR-007): if `simbad_oid` is non-null and a row with that
/// oid exists, that row is the canonical one (keep its id so alias / ingest
/// FKs stay valid). Otherwise fall back to the designation-derived id.
async fn find_existing(
    conn: &mut SqliteConnection,
    identity: &ResolvedIdentity,
    derived: &str,
) -> CacheResult<Option<ExistingRow>> {
    fn into_existing_row(row: q_resolver::ExistingTargetRow) -> CacheResult<ExistingRow> {
        let source = TargetSource::from_wire(&row.source)
            .ok_or_else(|| CacheError::InvalidSource(row.source.clone()))?;
        Ok(ExistingRow { id: row.id, source })
    }

    if let Some(oid) = identity.simbad_oid {
        if let Some(row) = q_resolver::find_existing_by_simbad_oid_conn(conn, oid).await? {
            return into_existing_row(row).map(Some);
        }
    }
    match q_resolver::find_existing_by_id_conn(conn, derived).await? {
        None => Ok(None),
        Some(row) => into_existing_row(row).map(Some),
    }
}

/// Replace all alias rows for `target_id` with `aliases`.
///
/// Aliases are rewritten wholesale (delete + insert) so a re-resolution that
/// adds/removes aliases stays consistent. `INSERT OR IGNORE` tolerates the
/// `(target_id, normalized)` uniqueness constraint when SIMBAD returns the same
/// normalized form twice.
async fn write_aliases(
    conn: &mut SqliteConnection,
    target_id: &str,
    aliases: &[ResolvedAlias],
) -> CacheResult<()> {
    q_resolver::delete_aliases_for_target_conn(conn, target_id).await?;
    for a in aliases {
        let alias_id = Uuid::new_v4().to_string();
        q_resolver::insert_alias_conn(
            conn,
            &alias_id,
            target_id,
            &a.alias,
            &a.normalized,
            a.kind.as_wire(),
        )
        .await?;
    }
    Ok(())
}

/// Upsert a resolved identity (and its aliases) into the cache.
///
/// Dedup + precedence (FR-007 / FR-014):
/// - The target row is matched by `simbad_oid` when non-null, else by the
///   designation-derived UUIDv5 id. Aliases of one physical object therefore
///   collapse onto one row.
/// - An existing `user-override` row is sticky: an incoming `resolved`/`seed`
///   write is skipped ([`UpsertOutcome::SkippedUserOverride`]) and never
///   overwrites the override. An incoming `user-override` always wins.
/// - Otherwise an equal-or-higher-precedence source overwrites (re-resolving
///   refreshes a `resolved` row; see [`TargetSource::may_overwrite`]).
///
/// Returns the persisted target id and the [`UpsertOutcome`].
///
/// This is the per-call entry point: it acquires a connection from `pool` and
/// delegates to [`upsert_resolved_conn`]. To batch many upserts in a single
/// transaction (e.g. the seed loader), open a transaction and call
/// [`upsert_resolved_conn`] directly with `&mut *tx`.
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure, or a parse error when an
/// existing row holds a corrupt value.
pub async fn upsert_resolved(
    pool: &SqlitePool,
    identity: &ResolvedIdentity,
) -> CacheResult<(Uuid, UpsertOutcome)> {
    let mut conn = pool.acquire().await?;
    upsert_resolved_conn(&mut conn, identity).await
}

/// Upsert a resolved identity (and its aliases) onto an existing connection or
/// transaction (`&mut *tx`).
///
/// Dedup + precedence semantics are identical to [`upsert_resolved`]; this
/// variant lets a caller batch many upserts inside one transaction so the whole
/// batch commits with a single fsync (the seed loader uses this).
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure, or a parse error when an
/// existing row holds a corrupt value.
pub async fn upsert_resolved_conn(
    conn: &mut SqliteConnection,
    identity: &ResolvedIdentity,
) -> CacheResult<(Uuid, UpsertOutcome)> {
    let derived = derived_id(&identity.primary_designation).to_string();
    let existing = find_existing(&mut *conn, identity, &derived).await?;
    let resolved_at = Timestamp::now_iso();
    // Enrichment (spec 052 P1 T014, D8): computed once per write, never
    // fabricated (constellation stays None on an out-of-range coordinate;
    // magnitude stays None when the source has no V photometry).
    let constellation = constellation_abbreviation(identity.ra_deg, identity.dec_deg);
    let magnitude = identity.v_mag;

    match existing {
        Some(row) if !identity.source.may_overwrite(row.source) => {
            // Existing row wins (a user-override is sticky vs resolved/seed).
            let id =
                Uuid::parse_str(&row.id).map_err(|e| CacheError::InvalidUuid(row.id.clone(), e))?;
            Ok((id, UpsertOutcome::SkippedUserOverride))
        }
        Some(row) => {
            // Update in place, keeping the existing id (preserve FK targets).
            // display_alias is NOT included — it is user-owned and must never
            // be overwritten by a re-resolution or seed load (FR-012).
            q_resolver::update_canonical_target_conn(
                conn,
                &row.id,
                identity.simbad_oid,
                &identity.primary_designation,
                identity.object_type.as_wire(),
                identity.ra_deg,
                identity.dec_deg,
                identity.source.as_wire(),
                &resolved_at,
                constellation.as_deref(),
                magnitude,
            )
            .await?;
            write_aliases(&mut *conn, &row.id, &identity.aliases).await?;
            let id =
                Uuid::parse_str(&row.id).map_err(|e| CacheError::InvalidUuid(row.id.clone(), e))?;
            Ok((id, UpsertOutcome::Updated))
        }
        None => {
            q_resolver::insert_canonical_target_conn(
                conn,
                &derived,
                identity.simbad_oid,
                &identity.primary_designation,
                identity.object_type.as_wire(),
                identity.ra_deg,
                identity.dec_deg,
                identity.source.as_wire(),
                &resolved_at,
                constellation.as_deref(),
                magnitude,
            )
            .await?;
            write_aliases(&mut *conn, &derived, &identity.aliases).await?;
            let id = Uuid::parse_str(&derived)
                .map_err(|e| CacheError::InvalidUuid(derived.clone(), e))?;
            Ok((id, UpsertOutcome::Inserted))
        }
    }
}

// ── Gen-3 management operations (spec 036) ──────────────────────────────────

/// A minimal list-row for the `target.list` surface.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetListRow {
    pub id: Uuid,
    pub primary_designation: String,
    /// User-set label; `None` when not set. `effectiveLabel = display_alias ?? primary_designation`.
    pub display_alias: Option<String>,
    pub object_type: String,
    /// ICRS J2000 right ascension in decimal degrees.
    pub ra_deg: f64,
    /// ICRS J2000 declination in decimal degrees.
    pub dec_deg: f64,
    /// IAU constellation abbreviation; `None` when the column is absent or
    /// not yet populated (schema before migration 0046).
    pub constellation: Option<String>,
    /// Visual magnitude; `None` when not stored or not applicable.
    pub magnitude: Option<f64>,
    /// All alias display forms (designations, common names, user-added).
    /// Empty when none are stored.
    pub aliases: Vec<String>,
}

/// List all canonical targets ordered by `primary_designation` (gen-3).
///
/// Reads `ra_deg`, `dec_deg`, `constellation`, and `magnitude` from the row;
/// `constellation`/`magnitude` are `NULL`-tolerant — they were added in
/// migration 0046 and may be absent for earlier entries.
/// `aliases` is collected from `target_alias` in a second pass (one batch
/// query per list call, not N+1).
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure, or [`CacheError::InvalidUuid`]
/// on a corrupt stored id.
pub async fn list_all(pool: &SqlitePool) -> CacheResult<Vec<TargetListRow>> {
    let rows = q_resolver::list_canonical_targets(pool).await?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // Batch-load aliases for all returned targets (avoids N+1 queries).
    let alias_rows = q_resolver::list_all_target_aliases(pool).await?;

    // Group aliases by target_id into a lookup map.
    let mut aliases_by_id: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for r in alias_rows {
        aliases_by_id.entry(r.target_id).or_default().push(r.alias);
    }

    rows.into_iter()
        .map(|row| {
            let id =
                Uuid::parse_str(&row.id).map_err(|e| CacheError::InvalidUuid(row.id.clone(), e))?;
            let aliases = aliases_by_id.remove(&row.id).unwrap_or_default();
            Ok(TargetListRow {
                id,
                primary_designation: row.primary_designation,
                display_alias: row.display_alias,
                object_type: row.object_type,
                ra_deg: row.ra_deg,
                dec_deg: row.dec_deg,
                constellation: row.constellation,
                magnitude: row.magnitude,
                aliases,
            })
        })
        .collect()
}

/// Insert a user-added alias for `target_id`.
///
/// The alias is stored with `kind = 'user'`. The `normalized` form is computed
/// here via [`simbad_resolver::normalize::normalize`] — the single
/// normalization choke-point (spec 052 P1 T004, FR-007): every identity
/// string entering the cache (TAP, Sesame, Caldwell, user query, seed, and
/// this user-added alias) is normalized by the SAME function before
/// caching/persisting, so alias variants of one physical object dedup
/// identically regardless of source. Rejects a duplicate via the
/// `UNIQUE(target_id, normalized)` constraint — returns `false` when the alias
/// already exists (idempotent), `true` when newly inserted.
///
/// # Errors
///
/// Returns [`CacheError::Database`] for any failure other than the uniqueness
/// constraint.
pub async fn insert_user_alias(
    pool: &SqlitePool,
    target_id: Uuid,
    alias: &str,
) -> CacheResult<Option<(String, String)>> {
    let normalized = simbad_resolver::normalize::normalize(alias);
    if normalized.is_empty() {
        return Ok(None);
    }
    let alias_id = Uuid::new_v4().to_string();
    let target_id_str = target_id.to_string();
    let rows_affected =
        q_resolver::insert_user_alias(pool, &alias_id, &target_id_str, alias, &normalized).await?;

    if rows_affected == 0 {
        // Alias already exists — return the existing id.
        let existing =
            q_resolver::select_alias_id_by_target_and_normalized(pool, &target_id_str, &normalized)
                .await?;
        Ok(existing.map(|id| (id, alias.to_owned())))
    } else {
        Ok(Some((alias_id, alias.to_owned())))
    }
}

/// Delete a user alias by its id, but only if its `kind = 'user'`.
///
/// Returns `true` if a row was deleted, `false` if not found or not a user
/// alias (SIMBAD designations/common names are not removable).
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure.
pub async fn delete_user_alias(pool: &SqlitePool, alias_id: &str) -> CacheResult<bool> {
    Ok(q_resolver::delete_user_alias(pool, alias_id).await?)
}

/// Set the `display_alias` column for a target (FR-012).
///
/// Blank/empty input is stored as NULL (treated as a clear). Returns `true` if
/// the target exists and was updated.
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure.
pub async fn set_display_alias(
    pool: &SqlitePool,
    target_id: Uuid,
    display_alias: &str,
) -> CacheResult<bool> {
    let value: Option<&str> =
        if display_alias.trim().is_empty() { None } else { Some(display_alias) };
    Ok(q_resolver::set_display_alias(pool, &target_id.to_string(), value).await?)
}

/// Clear the `display_alias` column for a target (sets to NULL).
///
/// Returns `true` if the target exists and was updated.
///
/// # Errors
///
/// Returns [`CacheError::Database`] on query failure.
pub async fn clear_display_alias(pool: &SqlitePool, target_id: Uuid) -> CacheResult<bool> {
    Ok(q_resolver::clear_display_alias(pool, &target_id.to_string()).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::persistence_core::Database;

    async fn setup() -> Database {
        let db = Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db
    }

    fn m31(source: TargetSource) -> ResolvedIdentity {
        ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: Some("Andromeda Galaxy".to_owned()),
            object_type: ObjectType::Galaxy,
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: None,
            aliases: vec![
                ResolvedAlias::new("M 31", AliasKind::Designation),
                ResolvedAlias::new("NGC 224", AliasKind::Designation),
                ResolvedAlias::new("Andromeda Galaxy", AliasKind::CommonName),
            ],
            source,
        }
    }

    #[tokio::test]
    async fn insert_then_read_by_oid() {
        let db = setup().await;
        let (id, outcome) = upsert_resolved(db.pool(), &m31(TargetSource::Resolved)).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Inserted);

        let got = get_by_simbad_oid(db.pool(), 1_575_544).await.unwrap().unwrap();
        assert_eq!(got.id, id);
        assert_eq!(got.primary_designation, "M 31");
        assert_eq!(got.object_type, ObjectType::Galaxy);
        assert_eq!(got.source, TargetSource::Resolved);
        assert_eq!(got.aliases.len(), 3);
    }

    #[tokio::test]
    async fn read_by_normalized_alias() {
        let db = setup().await;
        upsert_resolved(db.pool(), &m31(TargetSource::Resolved)).await.unwrap();

        let norm = targeting::normalize::normalize("NGC 224");
        let got = get_by_normalized(db.pool(), &norm).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "M 31");

        let common = targeting::normalize::normalize("Andromeda Galaxy");
        let got2 = get_by_normalized(db.pool(), &common).await.unwrap().unwrap();
        assert_eq!(got2.id, got.id);
    }

    #[tokio::test]
    async fn dedup_by_oid_updates_single_row() {
        let db = setup().await;
        // First resolve.
        let (id1, _) = upsert_resolved(db.pool(), &m31(TargetSource::Resolved)).await.unwrap();
        // Re-resolve the SAME oid but under a different primary designation
        // (e.g. NGC view); must reuse the existing row, not create a second.
        let mut alt = m31(TargetSource::Resolved);
        alt.primary_designation = "NGC 224".to_owned();
        let (id2, outcome) = upsert_resolved(db.pool(), &alt).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Updated);
        assert_eq!(id1, id2, "dedup by oid must keep the same row id");

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM canonical_target")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);

        let got = get_by_id(db.pool(), id1).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "NGC 224");
    }

    #[tokio::test]
    async fn user_override_is_sticky_against_resolved() {
        let db = setup().await;
        // User override first.
        let (id, _) = upsert_resolved(db.pool(), &m31(TargetSource::UserOverride)).await.unwrap();
        // A later SIMBAD resolution must NOT overwrite it (FR-014).
        let mut later = m31(TargetSource::Resolved);
        later.primary_designation = "WRONG".to_owned();
        let (id2, outcome) = upsert_resolved(db.pool(), &later).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::SkippedUserOverride);
        assert_eq!(id, id2);

        let got = get_by_id(db.pool(), id).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "M 31");
        assert_eq!(got.source, TargetSource::UserOverride);
    }

    #[tokio::test]
    async fn user_override_overwrites_resolved() {
        let db = setup().await;
        upsert_resolved(db.pool(), &m31(TargetSource::Resolved)).await.unwrap();
        let mut override_id = m31(TargetSource::UserOverride);
        override_id.primary_designation = "Andromeda".to_owned();
        let (_, outcome) = upsert_resolved(db.pool(), &override_id).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Updated);

        let got = get_by_simbad_oid(db.pool(), 1_575_544).await.unwrap().unwrap();
        assert_eq!(got.source, TargetSource::UserOverride);
        assert_eq!(got.primary_designation, "Andromeda");
    }

    #[tokio::test]
    async fn resolved_refreshes_existing_resolved_row() {
        let db = setup().await;
        upsert_resolved(db.pool(), &m31(TargetSource::Resolved)).await.unwrap();
        let mut refreshed = m31(TargetSource::Resolved);
        refreshed.dec_deg = 41.0;
        let (_, outcome) = upsert_resolved(db.pool(), &refreshed).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Updated);
        let got = get_by_simbad_oid(db.pool(), 1_575_544).await.unwrap().unwrap();
        assert!((got.dec_deg - 41.0).abs() < f64::EPSILON);
    }

    // ── Enrichment at the in-use write (spec 052 P1 T007/T014, D8) ─────────────

    #[tokio::test]
    async fn upsert_populates_magnitude_and_constellation_when_present() {
        let db = setup().await;
        let mut identity = m31(TargetSource::Resolved);
        identity.v_mag = Some(3.44);
        let (id, _) = upsert_resolved(db.pool(), &identity).await.unwrap();

        let rows = list_all(db.pool()).await.unwrap();
        let row = rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(row.magnitude, Some(3.44), "magnitude must come from ResolvedIdentity.v_mag");
        // M 31 (ra=10.68, dec=41.27) sits in Andromeda.
        assert_eq!(
            row.constellation.as_deref(),
            Some("And"),
            "constellation must be derived from (ra_deg, dec_deg) via skymath"
        );
    }

    #[tokio::test]
    async fn upsert_leaves_magnitude_null_when_source_has_none() {
        let db = setup().await;
        let mut identity = m31(TargetSource::Resolved);
        identity.v_mag = None;
        let (id, _) = upsert_resolved(db.pool(), &identity).await.unwrap();

        let rows = list_all(db.pool()).await.unwrap();
        let row = rows.iter().find(|r| r.id == id).unwrap();
        assert!(row.magnitude.is_none(), "magnitude must never be fabricated when v_mag is None");
        // Coordinates are always present/valid, so constellation is still derived.
        assert_eq!(row.constellation.as_deref(), Some("And"));
    }

    #[tokio::test]
    async fn null_oid_dedups_by_derived_id() {
        let db = setup().await;
        let mut seed = m31(TargetSource::Seed);
        seed.simbad_oid = None;
        let (id1, o1) = upsert_resolved(db.pool(), &seed).await.unwrap();
        assert_eq!(o1, UpsertOutcome::Inserted);
        // Same designation, still no oid → same derived id, updated not inserted.
        let (id2, o2) = upsert_resolved(db.pool(), &seed).await.unwrap();
        assert_eq!(id1, id2);
        assert_eq!(o2, UpsertOutcome::Updated);
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM canonical_target")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let db = setup().await;
        assert!(get_by_simbad_oid(db.pool(), 999).await.unwrap().is_none());
        assert!(get_by_normalized(db.pool(), "nothing").await.unwrap().is_none());
        assert!(get_by_id(db.pool(), Uuid::new_v4()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn aliases_rewritten_on_update() {
        let db = setup().await;
        upsert_resolved(db.pool(), &m31(TargetSource::Resolved)).await.unwrap();
        let mut fewer = m31(TargetSource::Resolved);
        fewer.aliases = vec![ResolvedAlias::new("M 31", AliasKind::Designation)];
        let (id, _) = upsert_resolved(db.pool(), &fewer).await.unwrap();
        let got = get_by_id(db.pool(), id).await.unwrap().unwrap();
        assert_eq!(got.aliases.len(), 1);
    }

    // ── list_all fixtures (typeahead search moved to the redb facade, D1) ──────

    fn m101() -> ResolvedIdentity {
        ResolvedIdentity {
            simbad_oid: Some(3_456_789),
            primary_designation: "M 101".to_owned(),
            common_name: Some("Pinwheel Galaxy".to_owned()),
            object_type: ObjectType::Galaxy,
            ra_deg: 210.802_42,
            dec_deg: 54.348_95,
            v_mag: None,
            aliases: vec![
                ResolvedAlias::new("M 101", AliasKind::Designation),
                ResolvedAlias::new("NGC 5457", AliasKind::Designation),
                ResolvedAlias::new("Pinwheel Galaxy", AliasKind::CommonName),
            ],
            source: TargetSource::Seed,
        }
    }

    async fn seeded(db: &Database) {
        upsert_resolved(db.pool(), &m31(TargetSource::Resolved)).await.unwrap();
        upsert_resolved(db.pool(), &m101()).await.unwrap();
    }

    // ── list_all alias population ─────────────────────────────────────────────

    #[tokio::test]
    async fn list_all_carries_aliases_for_resolved_target() {
        let db = setup().await;
        // M31 fixture has three aliases: "M 31" (designation), "NGC 224"
        // (designation), "Andromeda Galaxy" (common_name).
        upsert_resolved(db.pool(), &m31(TargetSource::Resolved)).await.unwrap();

        let rows = list_all(db.pool()).await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.primary_designation, "M 31");
        assert_eq!(row.aliases.len(), 3, "expected 3 aliases, got {:?}", row.aliases);
        // Aliases are ordered alphabetically by the SQL ORDER BY alias ASC.
        assert!(row.aliases.contains(&"M 31".to_owned()), "M 31 alias missing");
        assert!(row.aliases.contains(&"NGC 224".to_owned()), "NGC 224 alias missing");
        assert!(
            row.aliases.contains(&"Andromeda Galaxy".to_owned()),
            "Andromeda Galaxy alias missing"
        );
    }

    #[tokio::test]
    async fn list_all_aliases_empty_when_no_alias_rows() {
        let db = setup().await;
        // Insert a canonical_target directly without any aliases.
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO canonical_target
             (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at)
             VALUES (?, NULL, 'Bare Target', 'galaxy', 1.0, 2.0, 'seed', '2026-01-01T00:00:00Z')",
        )
        .bind(&id)
        .execute(db.pool())
        .await
        .unwrap();

        let rows = list_all(db.pool()).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].aliases.is_empty(), "aliases must be empty when no alias rows exist");
    }

    #[tokio::test]
    async fn list_all_aliases_for_multiple_targets_do_not_cross_contaminate() {
        let db = setup().await;
        seeded(&db).await; // seeds M31 (3 aliases) and M101 (3 aliases)

        let rows = list_all(db.pool()).await.unwrap();
        assert_eq!(rows.len(), 2);

        let m31_row = rows.iter().find(|r| r.primary_designation == "M 31").unwrap();
        let m101_row = rows.iter().find(|r| r.primary_designation == "M 101").unwrap();

        assert_eq!(m31_row.aliases.len(), 3, "M31 aliases: {:?}", m31_row.aliases);
        assert_eq!(m101_row.aliases.len(), 3, "M101 aliases: {:?}", m101_row.aliases);

        // Aliases must not bleed across targets.
        assert!(
            !m31_row.aliases.contains(&"NGC 5457".to_owned()),
            "NGC 5457 must not appear on M31"
        );
        assert!(
            !m101_row.aliases.contains(&"NGC 224".to_owned()),
            "NGC 224 must not appear on M101"
        );
    }
}
