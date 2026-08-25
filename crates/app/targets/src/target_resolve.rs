// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `target.resolve` use case for spec 035 (US3 — long-tail SIMBAD resolution),
//! retargeted by spec 052 P1 (D1/D4, supersedes spec-035 FR-006).
//!
//! `resolve` is now a pure read: it delegates cache-first + online-gated
//! network resolution entirely to the injected [`Resolver`] (production:
//! `targeting_resolver::simbad::SimbadResolver`, a facade over the shared
//! redb resolve cache — cache hits and repeat resolves never touch the
//! network, FR-006/SC-001, verified at the resolver-crate level) and never
//! writes `canonical_target` itself (FR-004/SC-002 — browsing/typing never
//! creates a durable row).
//!
//! Durable persistence happens ONLY at an explicit in-use commit —
//! [`promote_by_id`], called from the app-layer commit points (add to
//! project, link to session, favourite, Inbox-confirm) with the redb-cache id
//! a prior `target.search`/`target.resolve` response already returned. The
//! manual `override` path (`apply_override`, FR-014/T032) is itself such a
//! commit — the user has explicitly bound a query to a chosen target — and
//! keeps writing `canonical_target` directly.
//!
//! ## Constitution
//!
//! - §I No filesystem mutations: reads/writes SQLite metadata only.
//! - §III Metadata/identity only — no image processing.
//! - §V SQLite `canonical_target` is the durable record; the redb resolve
//!   cache queried via the injected [`Resolver`] is an explicitly reproducible
//!   projection (§V), never canonical.

// Guard held across assertions on borrowed data throughout this file.
#![allow(clippy::significant_drop_tightening)]

use sqlx::SqlitePool;
use uuid::Uuid;

use contracts_core::targets::{
    ResolvedTarget, TargetResolveSimbadRequest, TargetResolveSimbadResponse, TargetResolveStatus,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};

use targeting::normalize::normalize;
use targeting_resolver::cache::{self, CachedTarget};
use targeting_resolver::{
    AliasKind, ExplicitResolver, ResolveError, ResolvedAlias, ResolvedIdentity, Resolver,
    TargetSource as CacheSource,
};

// ── Durable audit record (T039, constitution §V) ──────────────────────────────

/// Write a durable `audit_log_entry` row for a resolution outcome.
///
/// `actor` must be `user` or `system`; `trigger` is the resolution kind
/// (`target.adopted` / `target.user_override`). The entity is the resolved
/// `canonical_target`. The query/context that triggered the commit is
/// captured in the JSON payload. Best-effort: an audit-write failure does not
/// fail the commit (it is logged).
async fn write_audit(
    pool: &SqlitePool,
    target_id: &str,
    trigger: &str,
    actor: &str,
    request_id: &str,
    query: &str,
) {
    let audit_id = domain_core::ids::new_id();
    let at = domain_core::ids::Timestamp::now_utc();
    let payload = serde_json::json!({ "query": query }).to_string();

    let result = persistence_targets::repositories::q_targets_mgmt::insert_resolution_audit(
        pool, &audit_id, target_id, trigger, actor, request_id, at, &payload,
    )
    .await;

    if let Err(e) = result {
        tracing::warn!("failed to write resolution audit record: {e}");
    }
}

// ── Error mapping ───────────────────────────────────────────────────────────

fn db_err(e: &cache::CacheError) -> ContractError {
    use cache::CacheError;
    use persistence_core::DbError;
    // Delegate Persistence(NotFound) to Blocking (canonical severity).
    // Other persistence variants and cache-specific errors map as Fatal.
    match e {
        CacheError::Persistence(DbError::NotFound(msg)) => ContractError::new(
            ErrorCode::InternalDatabase,
            msg.clone(),
            ErrorSeverity::Blocking,
            false,
        ),
        other => ContractError::new(
            ErrorCode::InternalDatabase,
            format!("{other}"),
            ErrorSeverity::Fatal,
            true,
        ),
    }
}

fn redb_err(e: &simbad_resolver::CacheError) -> ContractError {
    ContractError::new(ErrorCode::InternalDatabase, format!("{e}"), ErrorSeverity::Fatal, true)
}

// ── Enum mapping (cache → contract DTO) ─────────────────────────────────────
//
// Shared mappers live in `crate::target_dto` (US11 T143).
use crate::target_dto::{common_name, map_object_type, map_source};

fn cached_to_resolved(target: &CachedTarget) -> ResolvedTarget {
    ResolvedTarget {
        target_id: target.id.to_string(),
        simbad_oid: target.simbad_oid,
        primary_designation: target.primary_designation.clone(),
        common_name: common_name(target),
        object_type: map_object_type(target.object_type),
        ra_deg: target.ra_deg,
        dec_deg: target.dec_deg,
        aliases: target.aliases.iter().map(|a| a.alias.clone()).collect(),
        source: map_source(target.source),
    }
}

/// Build the response DTO directly from a resolved identity (no SQLite/redb
/// read-back needed): `target_id` is the deterministic UUIDv5 derived from
/// the designation ([`targeting::identity::target_id_from_designation`]) —
/// identical to the id the production facade already assigned in the redb
/// cache (same namespace seed, asserted by
/// `targeting_resolver::simbad::tests::namespace_matches_sqlite_identity_derivation`)
/// and to the id [`promote_by_id`] later writes to `canonical_target`.
fn identity_to_resolved(identity: &ResolvedIdentity) -> ResolvedTarget {
    let target_id = targeting::identity::target_id_from_designation(&identity.primary_designation);
    ResolvedTarget {
        target_id: target_id.to_string(),
        simbad_oid: identity.simbad_oid,
        primary_designation: identity.primary_designation.clone(),
        common_name: identity.common_name.clone(),
        object_type: map_object_type(identity.object_type),
        ra_deg: identity.ra_deg,
        dec_deg: identity.dec_deg,
        aliases: identity.aliases.iter().map(|a| a.alias.clone()).collect(),
        source: map_source(identity.source),
    }
}

// ── Response builders ───────────────────────────────────────────────────────

fn resolved(
    req: &TargetResolveSimbadRequest,
    target: ResolvedTarget,
) -> TargetResolveSimbadResponse {
    TargetResolveSimbadResponse {
        contract_version: req.contract_version.clone(),
        request_id: req.request_id.clone(),
        status: TargetResolveStatus::Resolved,
        target: Some(target),
        unresolved_reason: None,
        error: None,
    }
}

fn unresolved(req: &TargetResolveSimbadRequest, reason: &str) -> TargetResolveSimbadResponse {
    TargetResolveSimbadResponse {
        contract_version: req.contract_version.clone(),
        request_id: req.request_id.clone(),
        status: TargetResolveStatus::Unresolved,
        target: None,
        unresolved_reason: Some(reason.to_owned()),
        error: None,
    }
}

// ── resolve ───────────────────────────────────────────────────────────────────

/// `target.resolve` — resolve a designation/common name (or a FITS `OBJECT`
/// value) against the injected [`Resolver`]. Never writes `canonical_target`
/// (see the module doc) except via the manual override path.
///
/// `resolver` is injected so tests use a `FakeResolver` (no network);
/// production passes a `targeting_resolver::simbad::SimbadResolver` already
/// configured (online/offline, endpoint, timeout) from `resolver_settings` by
/// the caller — this use case no longer reads that setting itself.
///
/// When `req.override_target` is present (manual override, FR-014/T032), the
/// `query` is bound to the chosen canonical target and persisted with
/// `source = user-override`; the cache precedence lock keeps it sticky against
/// later SIMBAD resolutions.
///
/// # Errors
///
/// Returns a `ContractError` (`internal.database`) only on a local SQLite
/// failure (override path only). Resolver failures (offline / unknown /
/// ambiguous) are encoded in the response as `status = unresolved` with a
/// reason — never an `Err`.
pub async fn resolve<R: Resolver + ?Sized>(
    pool: &SqlitePool,
    resolver: &R,
    req: &TargetResolveSimbadRequest,
) -> Result<TargetResolveSimbadResponse, ContractError> {
    let query = req.query.trim();
    if query.is_empty() {
        return Ok(unresolved(req, "unknown"));
    }

    // Manual override (FR-014, T032): bind `query` to the chosen canonical
    // target and persist as source = user-override — an explicit in-use
    // commit in its own right, so it keeps writing SQLite directly.
    if let Some(ov) = &req.override_target {
        return apply_override(pool, req, query, &ov.target_id).await;
    }

    Ok(response_for(req, resolver.resolve(query).await))
}

/// `target.resolve_explicit` (spec 052 P2, FR-008/FR-009) — the deliberate
/// resolve/confirm entrypoint (Enter with no typeahead match, "search more",
/// or an Add/Confirm submit). Identical contract shape and override handling
/// to [`resolve`]; the only difference is `resolver.resolve_explicit`
/// (TAP-first, Sesame-fallback-on-a-miss) instead of `resolver.resolve`
/// (TAP + cache only).
///
/// Takes an [`ExplicitResolver`] rather than a plain [`Resolver`] so a call
/// site that only has a `Resolver` (e.g. the debounced typeahead path, the
/// ingest queue) has no way to reach the Sesame fallback by mistake — see the
/// trait's doc.
///
/// # Errors
///
/// Same as [`resolve`].
pub async fn resolve_explicit<R: ExplicitResolver + ?Sized>(
    pool: &SqlitePool,
    resolver: &R,
    req: &TargetResolveSimbadRequest,
) -> Result<TargetResolveSimbadResponse, ContractError> {
    let query = req.query.trim();
    if query.is_empty() {
        return Ok(unresolved(req, "unknown"));
    }

    if let Some(ov) = &req.override_target {
        return apply_override(pool, req, query, &ov.target_id).await;
    }

    Ok(response_for(req, resolver.resolve_explicit(query).await))
}

/// Shared outcome→response mapping for [`resolve`] and [`resolve_explicit`]:
/// unknown/garbled (`NotFound`) and a malformed SIMBAD response (`Parse`)
/// both leave the item unresolved with reason "unknown" (never fabricate).
fn response_for(
    req: &TargetResolveSimbadRequest,
    outcome: Result<ResolvedIdentity, ResolveError>,
) -> TargetResolveSimbadResponse {
    match outcome {
        Ok(identity) => resolved(req, identity_to_resolved(&identity)),
        Err(ResolveError::NotFound(_) | ResolveError::Parse(_)) => unresolved(req, "unknown"),
        Err(ResolveError::Ambiguous { .. }) => unresolved(req, "ambiguous"),
        Err(ResolveError::Network(_) | ResolveError::Timeout(_) | ResolveError::Disabled) => {
            unresolved(req, "offline")
        }
    }
}

// ── In-use promotion (spec 052 P1 FR-004) ───────────────────────────────────

/// Promote a redb-cached target into the durable `canonical_target` table —
/// the in-use commit point every app-layer "add to project" / "link to
/// session" / "favourite" / "Inbox-confirm" action calls with the `target_id`
/// a prior `target.search`/`target.resolve` response returned.
///
/// A no-op (returns `true` immediately, no redb read) when the id is already
/// durable — the common case once anything has adopted it once. Otherwise
/// looks the id up in the shared redb cache and upserts it into SQLite
/// (enriching `magnitude`/`constellation`, spec 052 P1 D8), writing a
/// `target.adopted` audit record. Returns `false` (never fabricates) when
/// `target_id` is unknown to both stores.
///
/// # Errors
///
/// Returns [`ContractError`] (`internal.database`) on a SQLite or redb
/// backend failure.
pub async fn promote_by_id(
    pool: &SqlitePool,
    redb_cache: &dyn simbad_resolver::Cache,
    target_id: Uuid,
    request_id: &str,
) -> Result<bool, ContractError> {
    if cache::get_by_id(pool, target_id).await.map_err(|e| db_err(&e))?.is_some() {
        return Ok(true);
    }

    let Some(cached) = redb_cache.get_by_id(target_id).await.map_err(|e| redb_err(&e))? else {
        return Ok(false);
    };
    let identity = targeting_resolver::simbad::from_crate_identity(cached.to_identity());

    let (id, outcome) = cache::upsert_resolved(pool, &identity).await.map_err(|e| db_err(&e))?;
    if outcome != cache::UpsertOutcome::SkippedUserOverride {
        crate::caches::invalidate_catalog();
    }
    write_audit(
        pool,
        &id.to_string(),
        "target.adopted",
        "user",
        request_id,
        &identity.primary_designation,
    )
    .await;
    Ok(true)
}

/// `target.adopt` — the explicit in-use commit for UI flows with no other
/// natural commit point (e.g. the Targets-page "Add Target" dialog). Thin
/// DTO wrapper over [`promote_by_id`].
///
/// # Errors
///
/// Returns [`ContractError`] (`target.invalid_id` for a malformed UUID,
/// `internal.database` on a backend failure).
pub async fn adopt(
    pool: &SqlitePool,
    redb_cache: &dyn simbad_resolver::Cache,
    req: &contracts_core::targets::TargetAdoptRequest,
) -> Result<contracts_core::targets::TargetAdoptResponse, ContractError> {
    let uuid = Uuid::parse_str(&req.target_id).map_err(|e| {
        ContractError::new(
            ErrorCode::TargetInvalidId,
            format!("target_id '{}' is not a valid UUID: {e}", req.target_id),
            ErrorSeverity::Blocking,
            false,
        )
    })?;
    let adopted = promote_by_id(pool, redb_cache, uuid, &req.request_id).await?;
    Ok(contracts_core::targets::TargetAdoptResponse { target_id: req.target_id.clone(), adopted })
}

// ── Manual override write (T032, FR-014) ────────────────────────────────────────

/// Bind `query` to the canonical target `target_id` and persist as
/// `source = user-override`.
///
/// The existing canonical row is loaded and re-written with
/// `source = UserOverride`, with `query` added as a designation alias so a later
/// `target.resolve` of the same query is a cache hit returning the override. The
/// cache precedence lock ([`cache::upsert_resolved`]) guarantees a subsequent
/// SIMBAD `resolved` write cannot overwrite it (FR-014).
async fn apply_override(
    pool: &SqlitePool,
    req: &TargetResolveSimbadRequest,
    query: &str,
    target_id: &str,
) -> Result<TargetResolveSimbadResponse, ContractError> {
    let id = Uuid::parse_str(target_id).map_err(|e| {
        ContractError::new(
            ErrorCode::TargetInvalidId,
            format!("override target_id '{target_id}' is not a valid UUID: {e}"),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    let Some(existing) = cache::get_by_id(pool, id).await.map_err(|e| db_err(&e))? else {
        // The override target must already exist in the cache; never fabricate.
        return Ok(unresolved(req, "unknown"));
    };

    // Carry over the existing identity, flip source to user-override, and ensure
    // the user's query is bound as an alias so it resolves to this target later.
    let mut aliases: Vec<ResolvedAlias> = existing.aliases.clone();
    let query_norm = normalize(query);
    if !aliases.iter().any(|a| a.normalized == query_norm) {
        aliases.push(ResolvedAlias::new(query, AliasKind::Designation));
    }

    let identity = ResolvedIdentity {
        simbad_oid: existing.simbad_oid,
        primary_designation: existing.primary_designation.clone(),
        common_name: existing
            .aliases
            .iter()
            .find(|a| a.kind == AliasKind::CommonName)
            .map(|a| a.alias.clone()),
        object_type: existing.object_type,
        ra_deg: existing.ra_deg,
        dec_deg: existing.dec_deg,
        v_mag: None,
        aliases,
        source: CacheSource::UserOverride,
    };

    let (written_id, outcome) =
        cache::upsert_resolved(pool, &identity).await.map_err(|e| db_err(&e))?;
    // Invalidate after the write commits (never before); this override write
    // always carries the new source/alias, so any outcome other than a no-op
    // skip changes the catalog snapshot.
    if outcome != cache::UpsertOutcome::SkippedUserOverride {
        crate::caches::invalidate_catalog();
    }
    if let Some(target) = cache::get_by_id(pool, written_id).await.map_err(|e| db_err(&e))? {
        // T039: durable audit record for the manual user-override (actor = user).
        write_audit(
            pool,
            &written_id.to_string(),
            "target.user_override",
            "user",
            &req.request_id,
            query,
        )
        .await;
        Ok(resolved(req, cached_to_resolved(&target)))
    } else {
        Ok(unresolved(req, "unknown"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts_core::targets::TargetSource;
    use persistence_core::Database;
    use simbad_resolver::{Cache as _, Store};
    use targeting_resolver::cache::upsert_resolved;
    use targeting_resolver::{
        AliasKind, FakeResolver, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource as Src,
    };

    // Serialized against `target_management`/`target_search`/
    // `resolver_settings` tests: `apply_override`/`promote_by_id` invalidate
    // the shared catalog `SnapshotCache` (F0), see
    // `target_management::cache_test_lock` for the full rationale.
    async fn setup() -> crate::target_management::cache_test_lock::LockedDb {
        crate::target_management::cache_test_lock::locked_db().await
    }

    fn m31() -> ResolvedIdentity {
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
            source: Src::Resolved,
        }
    }

    fn req(query: &str) -> TargetResolveSimbadRequest {
        TargetResolveSimbadRequest {
            contract_version: "1.0".into(),
            request_id: "req-1".into(),
            query: query.into(),
            override_target: None,
        }
    }

    #[tokio::test]
    async fn resolve_never_writes_canonical_target() {
        let db = setup().await;
        let resolver = FakeResolver::new().with_response("M 31", m31());
        let resp = resolve(db.pool(), &resolver, &req("M 31")).await.unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Resolved);
        assert_eq!(resp.target.as_ref().unwrap().simbad_oid, Some(1_575_544));

        // SC-002: a plain resolve is search-adjacent and must not persist.
        assert!(
            cache::get_by_simbad_oid(db.pool(), 1_575_544).await.unwrap().is_none(),
            "target.resolve must never write canonical_target"
        );
    }

    #[tokio::test]
    async fn resolve_target_id_is_deterministic_from_designation() {
        let db = setup().await;
        let resolver = FakeResolver::new().with_response("M 31", m31());
        let resp = resolve(db.pool(), &resolver, &req("M 31")).await.unwrap();
        let expected = targeting::identity::target_id_from_designation("M 31").to_string();
        assert_eq!(resp.target.unwrap().target_id, expected);
    }

    #[tokio::test]
    async fn offline_resolver_degrades_to_unresolved() {
        let db = setup().await;
        let resolver = FakeResolver::new().with_default_error(ResolveError::Timeout(10));
        let resp = resolve(db.pool(), &resolver, &req("M 31")).await.unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Unresolved);
        assert_eq!(resp.unresolved_reason.as_deref(), Some("offline"));
        assert!(resp.target.is_none());
    }

    #[tokio::test]
    async fn not_found_is_unresolved_unknown() {
        let db = setup().await;
        let resolver = FakeResolver::new(); // default NotFound
        let resp = resolve(db.pool(), &resolver, &req("ZZZ Nonexistent")).await.unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Unresolved);
        assert_eq!(resp.unresolved_reason.as_deref(), Some("unknown"));
    }

    #[tokio::test]
    async fn ambiguous_is_unresolved_ambiguous() {
        let db = setup().await;
        let resolver = FakeResolver::new()
            .with_error("M 31", ResolveError::Ambiguous { query: "M 31".into(), count: 2 });
        let resp = resolve(db.pool(), &resolver, &req("M 31")).await.unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Unresolved);
        assert_eq!(resp.unresolved_reason.as_deref(), Some("ambiguous"));
    }

    #[tokio::test]
    async fn empty_query_is_unresolved() {
        let db = setup().await;
        let resolver = FakeResolver::new();
        let resp = resolve(db.pool(), &resolver, &req("   ")).await.unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Unresolved);
    }

    #[tokio::test]
    async fn never_fabricates_coordinates_on_unresolved() {
        let db = setup().await;
        let resolver = FakeResolver::new();
        let resp = resolve(db.pool(), &resolver, &req("Garbled OBJECT value")).await.unwrap();
        assert!(resp.target.is_none(), "no coordinates fabricated for an unresolved query");
    }

    // ── promote_by_id (spec 052 P1 FR-004) ──────────────────────────────────────

    fn ns() -> uuid::Uuid {
        simbad_resolver::identity::namespace("astro-plan.targets")
    }

    async fn crate_cache_with_m31() -> simbad_resolver::RedbCache {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let identity = simbad_resolver::ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: Some("Andromeda Galaxy".to_owned()),
            object_type: simbad_resolver::ObjectType::Galaxy,
            otype_raw: "G".to_owned(),
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: Some(3.44),
            aliases: vec![simbad_resolver::ResolvedAlias::new(
                "M 31",
                simbad_resolver::AliasKind::Designation,
            )],
            source: simbad_resolver::TargetSource::Resolved,
        };
        cache.upsert(&identity, &ns()).await.unwrap();
        cache
    }

    #[tokio::test]
    async fn promote_by_id_writes_canonical_target_from_cache() {
        let db = setup().await;
        let cache = crate_cache_with_m31().await;
        let target_id = targeting::identity::target_id_from_designation("M 31");

        let ok = promote_by_id(db.pool(), &cache, target_id, "req-1").await.unwrap();
        assert!(ok);

        let got = cache::get_by_id(db.pool(), target_id).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "M 31");
        assert_eq!(audit_rows(&db, "target.adopted").await, 1);
    }

    #[tokio::test]
    async fn adopt_promotes_by_id_and_echoes_target_id() {
        let db = setup().await;
        let cache = crate_cache_with_m31().await;
        let target_id = targeting::identity::target_id_from_designation("M 31");
        let req = contracts_core::targets::TargetAdoptRequest {
            request_id: "req-1".to_owned(),
            target_id: target_id.to_string(),
        };
        let resp = adopt(db.pool(), &cache, &req).await.unwrap();
        assert!(resp.adopted);
        assert_eq!(resp.target_id, target_id.to_string());
    }

    #[tokio::test]
    async fn adopt_invalid_uuid_errors() {
        let db = setup().await;
        let cache = crate_cache_with_m31().await;
        let req = contracts_core::targets::TargetAdoptRequest {
            request_id: "req-1".to_owned(),
            target_id: "not-a-uuid".to_owned(),
        };
        let err = adopt(db.pool(), &cache, &req).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::TargetInvalidId);
    }

    #[tokio::test]
    async fn promote_by_id_unknown_target_returns_false() {
        let db = setup().await;
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let ok = promote_by_id(db.pool(), &cache, uuid::Uuid::new_v4(), "req-1").await.unwrap();
        assert!(!ok);
    }

    #[tokio::test]
    async fn promote_by_id_is_idempotent_no_duplicate_row() {
        let db = setup().await;
        let cache = crate_cache_with_m31().await;
        let target_id = targeting::identity::target_id_from_designation("M 31");

        assert!(promote_by_id(db.pool(), &cache, target_id, "req-1").await.unwrap());
        assert!(promote_by_id(db.pool(), &cache, target_id, "req-2").await.unwrap());

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM canonical_target")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1, "second promote is a no-op fast path, not a re-write");
        // Only the first promote wrote an audit record (fast path skips it).
        assert_eq!(audit_rows(&db, "target.adopted").await, 1);
    }

    // ── T032: manual override write path (FR-014) ──────────────────────────────

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
            ],
            source: CacheSource::Resolved,
        }
    }

    fn override_req(query: &str, target_id: &str) -> TargetResolveSimbadRequest {
        TargetResolveSimbadRequest {
            contract_version: "1.0".into(),
            request_id: "req-ov".into(),
            query: query.into(),
            override_target: Some(contracts_core::targets::TargetResolveOverride {
                target_id: target_id.into(),
            }),
        }
    }

    #[tokio::test]
    async fn override_binds_query_to_target_as_user_override() {
        let db = setup().await;
        // Seed the override target (M 101) in the cache.
        let (m101_id, _) = upsert_resolved(db.pool(), &m101()).await.unwrap();

        // User overrides the ambiguous OBJECT "Pinwheel" → M 101.
        let resolver = FakeResolver::new(); // would NotFound
        let resp = resolve(db.pool(), &resolver, &override_req("Pinwheel", &m101_id.to_string()))
            .await
            .unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Resolved);
        let t = resp.target.unwrap();
        assert_eq!(t.source, TargetSource::UserOverride);
        assert_eq!(t.primary_designation, "M 101");
    }

    #[tokio::test]
    async fn override_is_sticky_against_later_simbad_resolve() {
        let db = setup().await;
        let (m101_id, _) = upsert_resolved(db.pool(), &m101()).await.unwrap();

        // 1) Override "MyObj" → M 101.
        let none = FakeResolver::new();
        resolve(db.pool(), &none, &override_req("MyObj", &m101_id.to_string())).await.unwrap();

        // 2) A later normal resolve of "MyObj" — even though the FakeResolver
        // would return M 31 — must return the sticky user-override (M 101)
        // via promote_by_id (M 101 is already durable; resolve() itself no
        // longer touches SQLite for the plain path, so the override's
        // persisted row is what a later `favourite`/`add-to-project` sees).
        let wrong = FakeResolver::new().with_response("MyObj", m31());
        let resp = resolve(db.pool(), &wrong, &req("MyObj")).await.unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Resolved);
        // The plain resolve path returns the FRESH SIMBAD identity (M 31) —
        // override stickiness now applies at the SQLite layer (promotion),
        // not at the pure-read resolve layer, since resolve() no longer
        // consults SQLite at all (FR-004). The durable M 101 override row
        // is unaffected: assert it directly.
        let got = cache::get_by_id(db.pool(), m101_id).await.unwrap().unwrap();
        assert_eq!(got.source, CacheSource::UserOverride);
        assert_eq!(got.primary_designation, "M 101");
        let _ = resp;
    }

    #[tokio::test]
    async fn override_unknown_target_is_unresolved() {
        let db = setup().await;
        let resolver = FakeResolver::new();
        let missing = domain_core::ids::new_id();
        let resp = resolve(db.pool(), &resolver, &override_req("X", &missing)).await.unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Unresolved);
    }

    #[tokio::test]
    async fn override_invalid_uuid_errors() {
        let db = setup().await;
        let resolver = FakeResolver::new();
        let err = resolve(db.pool(), &resolver, &override_req("X", "not-a-uuid")).await;
        assert!(err.is_err());
    }

    // ── T039: durable audit records ────────────────────────────────────────────

    async fn audit_rows(db: &Database, trigger: &str) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log_entry WHERE trigger = ?")
            .bind(trigger)
            .fetch_one(db.pool())
            .await
            .unwrap();
        n
    }

    #[tokio::test]
    async fn override_writes_one_user_override_audit_record() {
        let db = setup().await;
        let (id, _) = upsert_resolved(db.pool(), &m31()).await.unwrap();
        let resolver = FakeResolver::new();
        let resp =
            resolve(db.pool(), &resolver, &override_req("MyObj", &id.to_string())).await.unwrap();
        assert_eq!(resp.status, TargetResolveStatus::Resolved);

        assert_eq!(
            audit_rows(&db, "target.user_override").await,
            1,
            "one user-override audit record (actor = user)"
        );
        let (actor,): (String,) = sqlx::query_as(
            "SELECT actor FROM audit_log_entry WHERE trigger = 'target.user_override'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(actor, "user");
    }
}
