// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! The production [`Resolver`] implementation: [`SimbadResolver`], its
//! cache-only offline stub [`OfflineResolver`], and the shared
//! typeahead/explicit resolve body.

use std::sync::Arc;

use async_trait::async_trait;
use simbad_resolver::{CacheBackend, ResolverConfig};

use crate::{ResolveError, ResolvedIdentity, Resolver};

use super::cache::ResolveCache;
use super::convert::{
    from_crate_error, from_crate_identity, from_facade_error, from_unresolved_reason,
};
use super::network_resolvers::{
    DualLookupResolver, EitherNetworkResolver, ExplicitNetworkResolver,
};
use super::{SimbadConfig, NAMESPACE_SEED};

/// Live SIMBAD resolver: the production [`Resolver`] implementation.
///
/// Wraps TWO instances of the crate's own cache-first
/// [`simbad_resolver::SimbadResolver`] facade, sharing one [`ResolveCache`]
/// (see the module doc, "Dual lookup"): `typeahead`-backed
/// [`Resolver`] trait impl (TAP + cache only), `explicit`-backed
/// [`Self::resolve_explicit`] (TAP-first, Sesame-fallback-on-miss).
pub struct SimbadResolver {
    typeahead: simbad_resolver::SimbadResolver<EitherNetworkResolver>,
    explicit: simbad_resolver::SimbadResolver<ExplicitNetworkResolver>,
    /// The same TAP client the typeahead/explicit legs share, kept for direct
    /// [`simbad_resolver::PositionResolver`] access (spec 052 P3, D9).
    /// `None` when built with `online_enabled = false` — cone-search has no
    /// cache-first path (unlike name resolution) so it is simply unavailable
    /// offline (FR-018), never silently degraded.
    position: Option<Arc<simbad_resolver::TapResolver>>,
    /// The same shared cache the typeahead/explicit facades were built with,
    /// kept for [`Self::warm_cache`] (spec 052 P3): cone-search results reach
    /// this resolver via [`Self::resolve_position`]/[`Self::enrich_position_match`],
    /// which — unlike [`Resolver::resolve`] — bypass the facade's own
    /// cache-first path entirely, so nothing else warms the cache for them.
    cache: ResolveCache,
}

impl SimbadResolver {
    /// Construct a resolver from a [`SimbadConfig`] and a shared
    /// [`ResolveCache`].
    ///
    /// When `online_enabled` is `false`, no `reqwest`/TLS client is built at
    /// all (`config` is simply unused for that call) — cache hits still
    /// resolve locally, and a miss reports an offline-shaped unresolved
    /// outcome (never touches the network).
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::Network`] if the underlying `reqwest` client
    /// cannot be built (e.g. TLS backend init failure) or the endpoint is not
    /// a valid URL.
    pub fn new(
        config: &SimbadConfig,
        cache: &ResolveCache,
        online_enabled: bool,
    ) -> Result<Self, ResolveError> {
        let (either, explicit_inner, position) = if online_enabled {
            // One TAP client, shared by the typeahead leg AND (via further
            // `Arc` clones) both the explicit-resolve leg, Sesame's own
            // oid-recovery enricher, and cone-search position resolution — a
            // single `reqwest` connection pool for every TAP-bound path this
            // resolver builds.
            let tap =
                Arc::new(simbad_resolver::TapResolver::new(config).map_err(from_crate_error)?);
            let sesame = simbad_resolver::SesameResolver::new()
                .with_enricher(Arc::clone(&tap) as Arc<dyn simbad_resolver::Resolver>);
            (
                EitherNetworkResolver::Online(Arc::clone(&tap)),
                ExplicitNetworkResolver::Online(DualLookupResolver::new(Arc::clone(&tap), sesame)),
                Some(tap),
            )
        } else {
            (
                EitherNetworkResolver::Offline(simbad_resolver::OfflineResolver),
                ExplicitNetworkResolver::Offline(simbad_resolver::OfflineResolver),
                None,
            )
        };
        let resolver_config = ResolverConfig::new(NAMESPACE_SEED).with_online(online_enabled);
        let typeahead = simbad_resolver::SimbadResolver::new(
            either,
            CacheBackend::custom(cache.cache()),
            resolver_config.clone(),
        )
        .map_err(|e| from_facade_error(&e))?;
        let explicit = simbad_resolver::SimbadResolver::new(
            explicit_inner,
            CacheBackend::custom(cache.cache()),
            resolver_config,
        )
        .map_err(|e| from_facade_error(&e))?;
        Ok(Self { typeahead, explicit, position, cache: cache.clone() })
    }

    /// Convenience constructor using [`SimbadConfig::default`] and an
    /// ephemeral in-memory cache. Test-only in practice (production always
    /// shares the app-data-rooted [`ResolveCache`] via [`Self::new`]).
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::Network`] if the client or cache cannot be built.
    pub fn with_defaults() -> Result<Self, ResolveError> {
        Self::new(&SimbadConfig::default(), &ResolveCache::in_memory()?, true)
    }

    /// Local, network-free typeahead search over the shared redb cache
    /// (`target.search`, FR-005) — exact/prefix/substring ranked, capped at
    /// `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::Network`] on a cache backend failure.
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<simbad_resolver::SearchHit>, ResolveError> {
        self.typeahead.search(query, limit).await.map_err(|e| from_facade_error(&e))
    }

    /// Explicit resolve/confirm entrypoint (spec 052 P2, FR-008/FR-009):
    /// TAP-first, Sesame-fallback only on a TAP miss. Callers wire this to a
    /// deliberate user action (Enter, confirm, "search harder") — never a
    /// per-keystroke typeahead call, which MUST keep using the [`Resolver`]
    /// trait's [`Self::resolve`] (TAP + cache only, no fallback).
    ///
    /// # Errors
    ///
    /// Same error/outcome shape as [`Resolver::resolve`].
    pub async fn resolve_explicit(&self, query: &str) -> Result<ResolvedIdentity, ResolveError> {
        resolve_via(&self.explicit, query).await
    }

    /// Cone-search (spec 052 P3, D9): the top `limit` SIMBAD objects within
    /// `radius_deg` of `(ra_deg, dec_deg)`, nearest first.
    ///
    /// Requires online resolution — cone-search has no cache-first path
    /// (unlike name resolution, FR-018): offline reports
    /// [`ResolveError::Disabled`] rather than degrading, so the caller can
    /// surface "cone-search unavailable offline" instead of an empty result.
    /// Returns the upstream crate's own [`simbad_resolver::PositionMatch`]
    /// (not astro-plan's local, `otype_raw`-stripped type) because OQ-1/OQ-2
    /// ranking needs `otype_raw`/`common_name`/`aliases` — see the module doc
    /// on [`crate::cone_search`].
    ///
    /// # Errors
    ///
    /// [`ResolveError::Disabled`] when offline; otherwise the TAP call's own
    /// network/timeout/parse errors.
    pub async fn resolve_position(
        &self,
        ra_deg: f64,
        dec_deg: f64,
        radius_deg: f64,
        limit: usize,
    ) -> Result<Vec<simbad_resolver::PositionMatch>, ResolveError> {
        match &self.position {
            Some(tap) => simbad_resolver::PositionResolver::resolve_position(
                tap.as_ref(),
                ra_deg,
                dec_deg,
                radius_deg,
                limit,
            )
            .await
            .map_err(from_crate_error),
            None => Err(ResolveError::Disabled),
        }
    }

    /// Enrich a cone-search hit's common name + full alias set.
    ///
    /// [`Self::resolve_position`] intentionally skips the alias round-trip
    /// for performance (see the upstream crate's module doc on
    /// `PositionResolver`) — a cone hit's `common_name` is always `None` and
    /// `aliases` holds only the primary designation. OQ-1 (common-name
    /// promotion) and OQ-2 (retain named stars) both need the real values, so
    /// the caller re-resolves each surviving top-N candidate by its
    /// `primary_designation` (an exact designation lookup, safe against the
    /// same physical object). Falls back to the un-enriched identity on any
    /// resolve failure (transient TAP hiccup) rather than dropping the
    /// candidate.
    #[must_use]
    pub async fn enrich_position_match(
        &self,
        m: simbad_resolver::PositionMatch,
    ) -> simbad_resolver::ResolvedIdentity {
        let Some(tap) = &self.position else { return m.identity };
        simbad_resolver::Resolver::resolve(tap.as_ref(), &m.identity.primary_designation)
            .await
            .unwrap_or(m.identity)
    }

    /// Warm the shared resolve cache with a cone-search identity (spec 052
    /// P3): [`Self::resolve_position`]/[`Self::enrich_position_match`] never
    /// go through the facade's own cache-first `resolve`, so without this a
    /// cone-search suggestion the user has never separately searched/typed
    /// stays cache-cold — `target.cone_search.confirm`'s
    /// `app_core_targets::target_resolve::promote_by_id` requires the id to
    /// already be cache- or SQLite-resident, so a suggestion the caller never
    /// warms could never actually be confirmed. Dedups exactly like every
    /// other cache write (`simbad_oid` → normalized designation, FR-007);
    /// best-effort — a failure here only means a slower confirm, never a
    /// correctness issue (the identity is still fully known to the caller).
    pub async fn warm_cache(&self, identity: &simbad_resolver::ResolvedIdentity) {
        let ns = simbad_resolver::identity::namespace(NAMESPACE_SEED);
        let _ = simbad_resolver::Cache::upsert(&self.cache.cache(), identity, &ns).await;
    }
}

#[async_trait]
impl Resolver for SimbadResolver {
    async fn resolve(&self, query: &str) -> Result<ResolvedIdentity, ResolveError> {
        resolve_via(&self.typeahead, query).await
    }
}

/// Makes the production resolver satisfy [`crate::ExplicitResolver`] (spec
/// 052 P2), so the app layer's `resolve_explicit` use case can stay generic
/// (testable with `FakeResolver`) instead of hardcoding this concrete
/// type.
#[async_trait]
impl crate::ExplicitResolver for SimbadResolver {
    async fn resolve_explicit(&self, query: &str) -> Result<ResolvedIdentity, ResolveError> {
        Self::resolve_explicit(self, query).await
    }
}

/// Shared body for [`SimbadResolver::resolve`] (typeahead) and
/// [`SimbadResolver::resolve_explicit`] — the only difference between the two
/// entrypoints is which facade (and therefore which composed network
/// resolver) `facade` is. Caldwell translation, cache-first check, and the
/// online-gate all live inside the facade's own `resolve_core` (D1); this
/// helper only converts the outcome back to astro-plan's local types.
async fn resolve_via<R: simbad_resolver::Resolver>(
    facade: &simbad_resolver::SimbadResolver<R>,
    query: &str,
) -> Result<ResolvedIdentity, ResolveError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(ResolveError::NotFound(String::new()));
    }

    match facade.resolve(query).await {
        Ok(simbad_resolver::Resolution::Resolved(cached)) => {
            Ok(from_crate_identity(cached.to_identity()))
        }
        Ok(simbad_resolver::Resolution::Unresolved { query, reason }) => {
            Err(from_unresolved_reason(query, reason))
        }
        Err(e) => Err(from_facade_error(&e)),
    }
}

/// A zero-cost [`Resolver`] that never reaches the network (FIX-3).
///
/// Used when online resolution is disabled in settings, so the command layer
/// can run the cache-first use case without constructing a `reqwest`/TLS
/// client. Every call reports [`ResolveError::Disabled`], which the use case
/// maps to an `unresolved("offline")` outcome (FR-015). Distinct from
/// [`SimbadResolver::new`]'s internal `online_enabled = false` path (which
/// STILL cache-checks via the shared [`ResolveCache`]); this stub carries no
/// cache at all and is used by callers with no cache handle in scope (e.g.
/// the background-drain's client-build-failure fallback).
pub struct OfflineResolver;

#[async_trait]
impl Resolver for OfflineResolver {
    async fn resolve(&self, _query: &str) -> Result<ResolvedIdentity, ResolveError> {
        Err(ResolveError::Disabled)
    }
}
