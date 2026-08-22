// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `target.search` use case for spec 035 (US1 — project-creation target search).
//!
//! As-you-type target search served PURELY from the shared redb resolve cache
//! (spec 052 P1 D1) — no network, and (per FR-004/SC-002) no SQLite write:
//! browsing/typeahead never creates a `canonical_target` row. Long-tail /
//! SIMBAD enrichment is a separate `target.resolve` call. Results are ranked
//! best-first (exact → prefix → substring, plus fuzzy when the caller's
//! `SimbadResolver` was built with fuzzy enabled) and de-duplicated to one
//! canonical target per hit by the facade itself.
//!
//! ## Constitution
//!
//! - §I No filesystem mutations: this use case only reads the redb cache.
//! - §III Metadata/identity only — no image processing.
//! - §V SQLite stays the durable record for in-use targets; the redb cache
//!   queried here is an explicitly reproducible projection (never written to
//!   from this use case).

use contracts_core::targets::{
    TargetCatalogId, TargetSearchRequest, TargetSearchResponse, TargetSuggestion,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use targeting_resolver::cache::{CachedTarget, SearchHit};
use targeting_resolver::AliasKind;

// ── Error mapping ───────────────────────────────────────────────────────────

fn cache_err(e: &simbad_resolver::CacheError) -> ContractError {
    ContractError::new(ErrorCode::InternalDatabase, format!("{e}"), ErrorSeverity::Fatal, true)
}

// ── Enum mapping (cache → contract DTO) ─────────────────────────────────────
//
// Shared mappers live in `crate::target_dto` (US11 T143).
use crate::target_dto::{common_name, map_object_type, map_source};

// ── Catalogue derivation (T029) ─────────────────────────────────────────────
//
// The cache has no catalogue column, so a target's catalogue membership is
// derived from its alias *designations*: the leading designation prefix maps to
// a closed `TargetCatalogId` (reusing the spec-013 catalogue vocabulary —
// `targeting::normalize` already expands these prefixes, so cached designations
// are space-separated like `M 31`, `NGC 224`, `SH 2-155`). NGC/IC both map to
// `Openngc`. Common names carry no catalogue and are ignored here.

/// Map a single designation to its catalogue, by leading prefix token.
fn designation_to_catalog(designation: &str) -> Option<TargetCatalogId> {
    let norm = targeting::normalize::normalize(designation);
    let prefix = norm.split_whitespace().next().unwrap_or("");
    match prefix {
        "m" => Some(TargetCatalogId::Messier),
        // Caldwell `C n` (normalize keeps the `c` prefix). Caldwell maps to an
        // NGC/IC object physically, but the *designation* prefix is the filter key.
        "c" | "caldwell" => Some(TargetCatalogId::Caldwell),
        "ngc" | "ic" | "openngc" => Some(TargetCatalogId::Openngc),
        "sh2" | "sharpless" => Some(TargetCatalogId::Sharpless),
        "abell" => Some(TargetCatalogId::AbellGalaxies),
        "arp" => Some(TargetCatalogId::Arp),
        "vdb" => Some(TargetCatalogId::Vdb),
        "b" | "barnard" => Some(TargetCatalogId::Barnard),
        "lbn" => Some(TargetCatalogId::Lbn),
        "ldn" => Some(TargetCatalogId::Ldn),
        "mel" | "melotte" => Some(TargetCatalogId::Melotte),
        _ => None,
    }
}

/// The set of catalogues a target belongs to, derived from its designations.
fn target_catalogs(target: &CachedTarget) -> Vec<TargetCatalogId> {
    let mut out: Vec<TargetCatalogId> = Vec::new();
    for a in &target.aliases {
        if a.kind == AliasKind::Designation {
            if let Some(cat) = designation_to_catalog(&a.alias) {
                if !out.contains(&cat) {
                    out.push(cat);
                }
            }
        }
    }
    out
}

/// Whether a target matches the (non-empty) catalogue filter: it belongs to at
/// least one of the requested catalogues.
fn matches_catalog_filter(target: &CachedTarget, filter: &[TargetCatalogId]) -> bool {
    if filter.is_empty() {
        return true;
    }
    let cats = target_catalogs(target);
    filter.iter().any(|f| cats.contains(f))
}

fn hit_to_suggestion(hit: SearchHit) -> TargetSuggestion {
    let object_type = map_object_type(hit.target.object_type);
    let source = map_source(hit.target.source);
    TargetSuggestion {
        target_id: hit.target.id.to_string(),
        primary_designation: hit.target.primary_designation.clone(),
        common_name: common_name(&hit.target),
        object_type,
        matched_alias: Some(hit.matched_alias),
        source,
    }
}

// ── search ──────────────────────────────────────────────────────────────────

const DEFAULT_SEARCH_LIMIT: usize = 20;

/// `target.search.json` §Request declares `limit` as `maximum: 100`. Requests
/// reaching this use case are not schema-validated, so the ceiling is enforced
/// here; `MAX_FETCH_CAP` below is only sound while `limit <= MAX_FETCH_CAP`.
const MAX_SEARCH_LIMIT: usize = 100;

/// Upper bound on the over-fetch that feeds the post-filter.
const MAX_FETCH_CAP: usize = 500;

/// `target.search` — ranked typeahead suggestions from the shared redb resolve
/// cache (seed + anything the facade has resolved/warmed, spec 052 P1 D1/T012).
///
/// Respects `limit` (default 20, contract maximum 100; a larger request is
/// clamped down to 100). A blank query yields an empty suggestion list. Both
/// optional filters are applied as a post-filter and AND together:
/// `catalog_filter` against the target's catalogue membership
/// (derived from its alias designations) and `type_filter` against the object
/// type. Over-fetches a bounded multiple of `limit` from the cache before
/// filtering, so a narrow filter still fills the page.
///
/// # Errors
///
/// Returns a `ContractError` (`internal.database`) when the local cache query
/// fails. There is no network in this path, so there is no resolver error here.
pub async fn search(
    cache: &dyn simbad_resolver::Cache,
    req: &TargetSearchRequest,
) -> Result<TargetSearchResponse, ContractError> {
    let limit = if req.limit == 0 {
        DEFAULT_SEARCH_LIMIT
    } else {
        (req.limit as usize).min(MAX_SEARCH_LIMIT)
    };

    let catalog_filter = &req.catalog_filter;
    let type_filter = &req.type_filter;
    let fetch_cap = if catalog_filter.is_empty() && type_filter.is_empty() {
        limit
    } else {
        limit.saturating_mul(8).min(MAX_FETCH_CAP)
    };

    let hits = cache.search(&req.query, fetch_cap).await.map_err(|e| cache_err(&e))?;

    let suggestions: Vec<TargetSuggestion> = hits
        .into_iter()
        .map(targeting_resolver::simbad::from_crate_search_hit)
        // The warm-complete sentinel (#818 follow-up) lives in this same
        // cache — it must never surface as a typeahead suggestion.
        .filter(|hit| !targeting_resolver::seed::is_warm_sentinel(hit.target.simbad_oid))
        .filter(|hit| matches_catalog_filter(&hit.target, catalog_filter))
        .map(hit_to_suggestion)
        .filter(|s| type_filter.is_empty() || type_filter.contains(&s.object_type))
        .take(limit)
        .collect();

    Ok(TargetSearchResponse {
        contract_version: req.contract_version.clone(),
        request_id: req.request_id.clone(),
        suggestions,
        // Always false here — this pure use case takes no `AppState`, so it
        // cannot know about a live warm. The `target.search` Tauri command
        // wrapper overwrites this from the real flag (#818).
        cache_warming: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts_core::targets::{TargetObjectType, TargetSource};
    use simbad_resolver::{
        AliasKind as CrateAliasKind, Cache as _, ObjectType as CrateObjectType,
        ResolvedAlias as CrateResolvedAlias, ResolvedIdentity as CrateResolvedIdentity, Store,
        TargetSource as CrateTargetSource,
    };

    fn ns() -> uuid::Uuid {
        simbad_resolver::identity::namespace("astro-plan.targets")
    }

    fn m31() -> CrateResolvedIdentity {
        CrateResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: Some("Andromeda Galaxy".to_owned()),
            object_type: CrateObjectType::Galaxy,
            otype_raw: "G".to_owned(),
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: None,
            aliases: vec![
                CrateResolvedAlias::new("M 31", CrateAliasKind::Designation),
                CrateResolvedAlias::new("NGC 224", CrateAliasKind::Designation),
                CrateResolvedAlias::new("Andromeda Galaxy", CrateAliasKind::CommonName),
            ],
            source: CrateTargetSource::Seed,
        }
    }

    fn ngc7000() -> CrateResolvedIdentity {
        CrateResolvedIdentity {
            simbad_oid: Some(2_222_222),
            primary_designation: "NGC 7000".to_owned(),
            common_name: Some("North America Nebula".to_owned()),
            object_type: CrateObjectType::EmissionNebula,
            otype_raw: "EmO".to_owned(),
            ra_deg: 314.75,
            dec_deg: 44.366,
            v_mag: None,
            aliases: vec![
                CrateResolvedAlias::new("NGC 7000", CrateAliasKind::Designation),
                CrateResolvedAlias::new("North America Nebula", CrateAliasKind::CommonName),
            ],
            source: CrateTargetSource::Resolved,
        }
    }

    async fn seeded_cache() -> simbad_resolver::RedbCache {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        cache.upsert(&m31(), &ns()).await.unwrap();
        cache.upsert(&ngc7000(), &ns()).await.unwrap();
        cache
    }

    fn req(query: &str) -> TargetSearchRequest {
        TargetSearchRequest {
            contract_version: "1.0".into(),
            request_id: "req-1".into(),
            query: query.into(),
            catalog_filter: Vec::new(),
            type_filter: Vec::new(),
            limit: 20,
        }
    }

    #[tokio::test]
    async fn search_maps_cached_target_to_suggestion() {
        let cache = seeded_cache().await;
        let resp = search(&cache, &req("M 31")).await.unwrap();
        assert_eq!(resp.suggestions.len(), 1);
        let s = &resp.suggestions[0];
        assert_eq!(s.primary_designation, "M 31");
        assert_eq!(s.common_name.as_deref(), Some("Andromeda Galaxy"));
        assert_eq!(s.object_type, TargetObjectType::Galaxy);
        assert_eq!(s.source, TargetSource::Seed);
        assert_eq!(s.matched_alias.as_deref(), Some("M 31"));
        assert!(!s.target_id.is_empty());
    }

    /// #818 follow-up: the warm-complete sentinel (`simbad_oid = -1`, the
    /// crate has no metadata table separate from this same cache) must never
    /// surface as a typeahead suggestion, even for a query that would
    /// otherwise match it (a substring of its designation/alias).
    #[tokio::test]
    async fn search_excludes_the_warm_complete_sentinel() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let sentinel = CrateResolvedIdentity {
            simbad_oid: Some(-1),
            primary_designation: "ALM SEED WARM SENTINEL".to_owned(),
            common_name: Some("2026-07-14T00:00:00Z".to_owned()),
            object_type: CrateObjectType::Other,
            otype_raw: String::new(),
            ra_deg: 0.0,
            dec_deg: 0.0,
            v_mag: None,
            aliases: vec![CrateResolvedAlias::new(
                "ALM SEED WARM SENTINEL",
                CrateAliasKind::Designation,
            )],
            source: CrateTargetSource::Seed,
        };
        cache.upsert(&sentinel, &ns()).await.unwrap();

        let resp = search(&cache, &req("SENTINEL")).await.unwrap();
        assert!(
            resp.suggestions.is_empty(),
            "the warm-complete sentinel must never surface as a typeahead suggestion, got {:?}",
            resp.suggestions
        );
    }

    #[tokio::test]
    async fn search_prefix_returns_ranked_local_matches() {
        let cache = seeded_cache().await;
        let resp = search(&cache, &req("NGC")).await.unwrap();
        // "NGC 224" (M31) and "NGC 7000" both prefix-match.
        assert_eq!(resp.suggestions.len(), 2);
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let cache = seeded_cache().await;
        let mut r = req("nebula");
        r.limit = 1;
        let resp = search(&cache, &r).await.unwrap();
        assert!(resp.suggestions.len() <= 1);
    }

    #[tokio::test]
    async fn search_zero_limit_defaults_to_20() {
        let cache = seeded_cache().await;
        let mut r = req("NGC");
        r.limit = 0;
        let resp = search(&cache, &r).await.unwrap();
        assert_eq!(resp.suggestions.len(), 2);
    }

    /// bd astro-plan-3v3r.11.26: an over-contract `limit` combined with a
    /// filter made the over-fetch clamp's min exceed its max and panicked.
    #[tokio::test]
    async fn search_limit_above_the_contract_maximum_with_a_filter_does_not_panic() {
        let cache = seeded_cache().await;
        let mut r = req("NGC");
        r.limit = 501;
        r.type_filter = vec![TargetObjectType::EmissionNebula];
        let resp = search(&cache, &r).await.unwrap();
        assert_eq!(resp.suggestions.len(), 1);
    }

    #[tokio::test]
    async fn search_type_filter_narrows_results() {
        let cache = seeded_cache().await;
        let mut r = req("NGC");
        r.type_filter = vec![TargetObjectType::EmissionNebula];
        let resp = search(&cache, &r).await.unwrap();
        assert_eq!(resp.suggestions.len(), 1);
        assert_eq!(resp.suggestions[0].primary_designation, "NGC 7000");
    }

    #[tokio::test]
    async fn search_catalog_filter_messier_narrows_to_m31() {
        let cache = seeded_cache().await;
        // "NGC" prefix-matches both M31 (NGC 224) and NGC 7000, but only M31
        // also belongs to the Messier catalogue.
        let mut r = req("NGC");
        r.catalog_filter = vec![TargetCatalogId::Messier];
        let resp = search(&cache, &r).await.unwrap();
        assert_eq!(resp.suggestions.len(), 1);
        assert_eq!(resp.suggestions[0].primary_designation, "M 31");
    }

    #[tokio::test]
    async fn search_catalog_filter_openngc_matches_both() {
        let cache = seeded_cache().await;
        let mut r = req("NGC");
        r.catalog_filter = vec![TargetCatalogId::Openngc];
        let resp = search(&cache, &r).await.unwrap();
        assert_eq!(resp.suggestions.len(), 2);
    }

    #[tokio::test]
    async fn search_catalog_and_type_filter_and_together() {
        let cache = seeded_cache().await;
        // openngc ∩ galaxy → only M 31 (NGC 7000 is an emission nebula).
        let mut r = req("NGC");
        r.catalog_filter = vec![TargetCatalogId::Openngc];
        r.type_filter = vec![TargetObjectType::Galaxy];
        let resp = search(&cache, &r).await.unwrap();
        assert_eq!(resp.suggestions.len(), 1);
        assert_eq!(resp.suggestions[0].primary_designation, "M 31");
    }

    #[tokio::test]
    async fn search_catalog_filter_excludes_non_members() {
        let cache = seeded_cache().await;
        // Filtering to Sharpless only → neither seeded target qualifies.
        let mut r = req("NGC");
        r.catalog_filter = vec![TargetCatalogId::Sharpless];
        let resp = search(&cache, &r).await.unwrap();
        assert!(resp.suggestions.is_empty());
    }

    #[tokio::test]
    async fn search_blank_query_is_empty() {
        let cache = seeded_cache().await;
        let resp = search(&cache, &req("   ")).await.unwrap();
        assert!(resp.suggestions.is_empty());
    }

    #[tokio::test]
    async fn search_echoes_request_envelope() {
        let cache = seeded_cache().await;
        let resp = search(&cache, &req("M 31")).await.unwrap();
        assert_eq!(resp.contract_version, "1.0");
        assert_eq!(resp.request_id, "req-1");
    }

    /// SC-002: pure search never writes SQLite — this use case takes no
    /// `SqlitePool` at all, so there is no `canonical_target` write path to
    /// exercise here (enforced by the type signature, not a runtime check).
    #[tokio::test]
    async fn search_never_touches_sqlite_by_construction() {
        let cache = seeded_cache().await;
        let _ = search(&cache, &req("M 31")).await.unwrap();
        // No `SqlitePool` parameter exists on `search` — nothing to assert at
        // runtime; this test documents the invariant for future readers.
    }
}
