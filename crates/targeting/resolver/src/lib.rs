// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Target resolution module (spec 035: SIMBAD Target Resolution).
//!
//! Resolves astronomical target identities on demand against SIMBAD, backed by
//! a bundled seed index and a local SQLite cache. This module owns the
//! resolver seam (a testable [`Resolver`] trait — implemented later by
//! `SimbadResolver` and a `FakeResolver`), the cache read/write layer, and
//! the bundled-seed loader.
//!
//! This is metadata/identity resolution only — no image processing
//! (PixInsight boundary, constitution §III).
//!
//! # Module layout
//!
//! - [`simbad`]: SIMBAD TAP/Sesame HTTP client (`reqwest`) → canonical identity.
//! - [`cache`]: cache read/write, dedupe by SIMBAD oid, source precedence.
//! - [`caldwell`]: static C1–C109 → NGC/IC designation map (Caldwell is not in SIMBAD).
//! - [`seed`]: bundled-seed load at first run.
//!
//! # The resolver seam (T004)
//!
//! [`Resolver`] is the no-network-in-unit-tests seam, mirroring the retired
//! `download::CatalogFetcher`/`FakeFetcher` pattern: production code uses the
//! `reqwest`-backed `SimbadResolver` (see [`simbad`], T019); the search /
//! resolve / ingest-queue logic is unit-tested offline with `FakeResolver`.

#![allow(clippy::doc_markdown)] // spec/domain terminology is not suited for backticks

pub mod cache;
pub mod caldwell;
pub mod cone_search;
pub mod seed;
pub mod simbad;

use serde::{Deserialize, Serialize};

// ── ObjectType (T005) ───────────────────────────────────────────────────────────

/// Closed object-type enum mapped from SIMBAD `otype` (data-model.md §Object
/// type mapping; mirrors the `ObjectType` enum in `target.resolve.json` /
/// `target.search.json`).
///
/// Serialized as `snake_case` to match the wire contracts. Any SIMBAD `otype`
/// outside the closed set maps to [`ObjectType::Other`] (see [`map_otype`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    Galaxy,
    PlanetaryNebula,
    EmissionNebula,
    ReflectionNebula,
    DarkNebula,
    OpenCluster,
    GlobularCluster,
    SupernovaRemnant,
    GalaxyCluster,
    DoubleStar,
    Asterism,
    Other,
}

impl ObjectType {
    /// The snake_case wire/DB string for this object type (matches the contract
    /// enum and the `canonical_target.object_type` column).
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Galaxy => "galaxy",
            Self::PlanetaryNebula => "planetary_nebula",
            Self::EmissionNebula => "emission_nebula",
            Self::ReflectionNebula => "reflection_nebula",
            Self::DarkNebula => "dark_nebula",
            Self::OpenCluster => "open_cluster",
            Self::GlobularCluster => "globular_cluster",
            Self::SupernovaRemnant => "supernova_remnant",
            Self::GalaxyCluster => "galaxy_cluster",
            Self::DoubleStar => "double_star",
            Self::Asterism => "asterism",
            Self::Other => "other",
        }
    }

    /// Parse a wire/DB string into an [`ObjectType`]. Unknown strings map to
    /// [`ObjectType::Other`] (closed enum, forward-compatible read).
    #[must_use]
    pub fn from_wire(s: &str) -> Self {
        match s {
            "galaxy" => Self::Galaxy,
            "planetary_nebula" => Self::PlanetaryNebula,
            "emission_nebula" => Self::EmissionNebula,
            "reflection_nebula" => Self::ReflectionNebula,
            "dark_nebula" => Self::DarkNebula,
            "open_cluster" => Self::OpenCluster,
            "globular_cluster" => Self::GlobularCluster,
            "supernova_remnant" => Self::SupernovaRemnant,
            "galaxy_cluster" => Self::GalaxyCluster,
            "double_star" => Self::DoubleStar,
            "asterism" => Self::Asterism,
            _ => Self::Other,
        }
    }
}

/// Map a raw SIMBAD `otype` string to the closed [`ObjectType`] enum.
///
/// SIMBAD `otype` values are short, case-sensitive condensed codes (e.g. `G`
/// for galaxy, `PN` for planetary nebula, `HII` for an emission/H II region).
/// The mapping is total: any unrecognised or unmapped `otype` (including the
/// empty string) returns [`ObjectType::Other`] so an identity is never dropped
/// for lack of a type (FR-009 — never fabricate, but always classifiable).
///
/// The recognised codes follow the SIMBAD object-type vocabulary
/// (<https://simbad.cds.unistra.fr/guide/otypes.htx>); the long-form labels
/// SIMBAD also emits are accepted as aliases for robustness.
#[must_use]
pub fn map_otype(otype: &str) -> ObjectType {
    match otype.trim() {
        // Galaxies (incl. interacting/active galaxy subtypes).
        "G" | "GiC" | "GiG" | "GiP" | "IG" | "PaG" | "AGN" | "SBG" | "rG" | "LSB" | "AG?"
        | "EmG" | "BiC" | "H2G" | "Sy1" | "Sy2" | "SyG" | "Galaxy" => ObjectType::Galaxy,
        // Planetary nebulae.
        "PN" | "PN?" | "pA*" | "PlanetaryNebula" => ObjectType::PlanetaryNebula,
        // Emission nebulae (H II regions, emission objects).
        "HII" | "EmO" | "ISM" | "RNe?" | "EmissionNebula" => ObjectType::EmissionNebula,
        // Reflection nebulae.
        "RNe" | "ReflectionNebula" => ObjectType::ReflectionNebula,
        // Dark / molecular clouds.
        "DNe" | "MoC" | "glb" | "cor" | "GNe" | "DarkNebula" => ObjectType::DarkNebula,
        // Open / galactic clusters.
        "OpC" | "Cl*" | "As*" | "OpenCluster" => ObjectType::OpenCluster,
        // Globular clusters.
        "GlC" | "GlobularCluster" => ObjectType::GlobularCluster,
        // Supernova remnants.
        "SNR" | "SNR?" | "SuperNovaRemnant" | "SupernovaRemnant" => ObjectType::SupernovaRemnant,
        // Clusters of galaxies.
        "ClG" | "GrG" | "CGG" | "SCG" | "GalaxyCluster" => ObjectType::GalaxyCluster,
        // Double / multiple stars.
        "**" | "**?" | "EB*" | "SB*" | "DoubleStar" => ObjectType::DoubleStar,
        // Asterisms (note: SIMBAD uses `As*` for stellar associations; the
        // visual-asterism sense is matched by the long-form label only).
        "Asterism" => ObjectType::Asterism,
        _ => ObjectType::Other,
    }
}

// ── TargetSource ────────────────────────────────────────────────────────────────

/// Provenance of a resolved identity (data-model.md §CanonicalTarget `source`).
///
/// The `UserOverride` variant serializes with the hyphenated `user-override`
/// wire/DB value across all three contracts (DTO↔wire parity, T009).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSource {
    /// Loaded from the bundled seed index at first run.
    Seed,
    /// Resolved live from SIMBAD.
    Resolved,
    /// A manual user correction; wins over `seed`/`resolved` (FR-014).
    #[serde(rename = "user-override")]
    UserOverride,
}

impl TargetSource {
    /// The wire/DB string for this source (matches the `canonical_target.source`
    /// CHECK constraint; `UserOverride` is the hyphenated `user-override`).
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Seed => "seed",
            Self::Resolved => "resolved",
            Self::UserOverride => "user-override",
        }
    }

    /// Parse a wire/DB string into a [`TargetSource`]. Returns `None` for an
    /// unrecognised value (the DB CHECK constraint prevents this in practice).
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "seed" => Some(Self::Seed),
            "resolved" => Some(Self::Resolved),
            "user-override" => Some(Self::UserOverride),
            _ => None,
        }
    }

    /// Source precedence rank for conflicting writes (FR-014): higher wins.
    /// `user-override` (2) > `resolved` (1) > `seed` (0).
    #[must_use]
    pub fn precedence(self) -> u8 {
        match self {
            Self::Seed => 0,
            Self::Resolved => 1,
            Self::UserOverride => 2,
        }
    }

    /// Whether a write with `self` as the incoming source may overwrite an
    /// existing row whose source is `existing` (FR-014).
    ///
    /// A `user-override` row is sticky: a later `resolved`/`seed` result MUST
    /// NOT overwrite it. An equal-or-higher-precedence incoming source wins
    /// (re-resolving refreshes a `resolved` row; an override always wins).
    #[must_use]
    pub fn may_overwrite(self, existing: Self) -> bool {
        self.precedence() >= existing.precedence()
    }
}

// ── ResolvedAlias / ResolvedIdentity ────────────────────────────────────────────

/// The kind of an alias attached to a resolved identity (data-model.md
/// §TargetAlias `kind`).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasKind {
    /// A catalog designation (e.g. `M 31`, `NGC 224`).
    Designation,
    /// A SIMBAD `NAME …` curated common name (e.g. `Andromeda Galaxy`).
    CommonName,
    /// A user-added alias (removable via `target.alias.remove`).
    User,
}

impl AliasKind {
    /// The wire/DB string (matches the `target_alias.kind` CHECK constraint).
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Designation => "designation",
            Self::CommonName => "common_name",
            Self::User => "user",
        }
    }

    /// Parse a wire/DB string into an [`AliasKind`]; unknown → `Designation`.
    #[must_use]
    pub fn from_wire(s: &str) -> Self {
        match s {
            "common_name" => Self::CommonName,
            "user" => Self::User,
            _ => Self::Designation,
        }
    }
}

/// One alternate designation/name for a resolved identity (data-model.md
/// §TargetAlias). The `normalized` form is the typeahead match surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedAlias {
    /// Verbatim designation or common name (e.g. `M 31`, `Andromeda Galaxy`).
    pub alias: String,
    /// Normalized form for matching (spec 013 normalize).
    pub normalized: String,
    /// Whether this alias is a designation or a curated common name.
    pub kind: AliasKind,
}

impl ResolvedAlias {
    /// Build a [`ResolvedAlias`], computing the normalized form from `alias`
    /// via [`simbad_resolver::normalize::normalize`] — the single
    /// normalization choke-point (spec 052 P1 T004, FR-007): every identity
    /// string that ends up cached/persisted (TAP, Sesame, Caldwell, user
    /// query, seed) passes through this one function, so alias variants of
    /// one physical object dedup identically regardless of source.
    #[must_use]
    pub fn new(alias: impl Into<String>, kind: AliasKind) -> Self {
        let alias = alias.into();
        let normalized = simbad_resolver::normalize::normalize(&alias);
        Self { alias, normalized, kind }
    }
}

/// A fully resolved canonical target identity returned by a [`Resolver`].
///
/// Mirrors the persisted `CanonicalTarget` (+ its `TargetAlias` rows) from
/// data-model.md and the `ResolvedTarget` shape in `target.resolve.json`.
/// Coordinates are ICRS J2000 decimal degrees and are never fabricated
/// (FR-009): a resolver that cannot determine a real position returns
/// [`ResolveError::NotFound`] instead of inventing one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedIdentity {
    /// SIMBAD physical-object id (the dedup key) when resolved online; `None`
    /// for seed/override-only entries until enriched.
    pub simbad_oid: Option<i64>,
    /// Canonical display designation (precedence table, spec 013).
    pub primary_designation: String,
    /// Curated common name (SIMBAD `NAME …`) when one exists.
    pub common_name: Option<String>,
    /// Closed object-type enum from the SIMBAD `otype` mapping ([`map_otype`]).
    pub object_type: ObjectType,
    /// ICRS J2000 right ascension in decimal degrees, `[0, 360)`.
    pub ra_deg: f64,
    /// ICRS J2000 declination in decimal degrees, `[-90, 90]`.
    pub dec_deg: f64,
    /// Johnson V-band apparent magnitude (SIMBAD `allfluxes.V`, 0.2.0) when the
    /// object has V photometry; `None` for objects without it (many extended /
    /// dark objects) or for seed/override-only entries. Feeds
    /// `canonical_target.magnitude` at the in-use write (spec 052 P1); never
    /// fabricated.
    pub v_mag: Option<f64>,
    /// All designations + common names for this object (the typeahead surface).
    pub aliases: Vec<ResolvedAlias>,
    /// Provenance of this identity.
    pub source: TargetSource,
}

// ── ResolveError ────────────────────────────────────────────────────────────────

/// Errors produced by a [`Resolver`].
///
/// These map onto the `target.resolve.json` outcomes: [`Self::Network`] /
/// [`Self::Timeout`] degrade to seed+cache (`unresolvedReason = "offline"`),
/// [`Self::NotFound`] and [`Self::Ambiguous`] produce `status = "unresolved"`
/// (reasons `"unknown"` / `"ambiguous"`), and the resolver NEVER fabricates an
/// identity (FR-009). `Disabled` corresponds to the `online_enabled = false`
/// setting (FR-015).
///
/// `Clone` mirrors `download::DownloadError` so callers (the ingest queue, the
/// cache writer) can retain the error across retries without re-running the
/// request.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ResolveError {
    /// Network/transport failure reaching SIMBAD; degrade to seed+cache.
    #[error("simbad unreachable: {0}")]
    Network(String),
    /// The request exceeded the configured timeout; degrade to seed+cache.
    #[error("simbad request timed out after {0}s")]
    Timeout(u64),
    /// Online resolution is disabled by settings (FR-015); seed+cache only.
    #[error("online resolution is disabled")]
    Disabled,
    /// The query did not resolve to any known object (unknown/garbled);
    /// the caller marks the item pending/unresolved (FR-009).
    #[error("no object resolved for query: {0}")]
    NotFound(String),
    /// The query resolved to multiple distinct physical objects; per FR-008 the
    /// caller leaves the item unresolved rather than guessing.
    #[error("query '{query}' is ambiguous ({count} distinct objects)")]
    Ambiguous {
        /// The verbatim query.
        query: String,
        /// Number of distinct physical objects matched.
        count: usize,
    },
    /// The SIMBAD response could not be parsed into a canonical identity.
    #[error("failed to parse simbad response: {0}")]
    Parse(String),
}

// ── Resolver trait (T004) ───────────────────────────────────────────────────────

/// The network-resolution seam (spec 035 R7), mirroring the retired
/// `download::CatalogFetcher`.
///
/// Production code uses the `reqwest`-backed `SimbadResolver` ([`simbad`],
/// T019); unit tests use `FakeResolver` for fast, network-free coverage of
/// the search / resolve / ingest-queue logic.
///
/// Methods are modelled on the `target.resolve.json` operation. Implementations
/// MUST be `Send + Sync` so the resolver can be shared across the ingest
/// background queue (FR-013).
#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve a complete designation or common name to a canonical identity.
    ///
    /// This is the `target.resolve` path: the caller has already missed the
    /// local cache and is consulting the online resolver. Returns the fully
    /// enriched [`ResolvedIdentity`] (ICRS coordinates, object type, aliases,
    /// common name, `simbad_oid`).
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`]: [`ResolveError::Network`]/[`ResolveError::Timeout`]
    /// when SIMBAD is unreachable (degrade to seed+cache), [`ResolveError::NotFound`]
    /// when the query is unknown, [`ResolveError::Ambiguous`] when it maps to
    /// several physical objects, [`ResolveError::Disabled`] when online
    /// resolution is off, or [`ResolveError::Parse`] on a malformed response.
    async fn resolve(&self, query: &str) -> Result<ResolvedIdentity, ResolveError>;

    /// Resolve a verbatim FITS `OBJECT` header value to a canonical identity.
    ///
    /// Used by the ingest queue (FR-013). Matching is exact-normalized only (no
    /// fuzzy/probabilistic match — FR-008); a non-matching or ambiguous value
    /// stays unresolved rather than being guessed. The default delegates to
    /// [`Resolver::resolve`]; SIMBAD treats a FITS `OBJECT` value as just
    /// another identifier, so implementations only override this when they need
    /// FITS-specific pre-normalization.
    ///
    /// # Errors
    ///
    /// Same as [`Resolver::resolve`].
    async fn resolve_object(&self, object_raw: &str) -> Result<ResolvedIdentity, ResolveError> {
        self.resolve(object_raw).await
    }
}

// ── ExplicitResolver seam (spec 052 P2, FR-008/FR-009) ──────────────────────────

/// The deliberate resolve/confirm entrypoint (Enter, confirm, "search harder")
/// — distinct from [`Resolver::resolve`], which the debounced typeahead path
/// uses and MUST stay TAP+cache only.
///
/// Kept as a separate trait (rather than a second parameter on
/// [`Resolver::resolve`]) so a call site generic over `Resolver` alone (e.g.
/// the ingest queue, typeahead) has no way to reach the Sesame fallback even
/// by mistake — only code that explicitly asks for `ExplicitResolver` can.
/// Implemented by the production `targeting_resolver::simbad::SimbadResolver`
/// (TAP-first, Sesame-fallback-on-miss) and, for tests, `FakeResolver`.
#[async_trait::async_trait]
pub trait ExplicitResolver: Resolver {
    /// Same contract as [`Resolver::resolve`], but permitted to consult a
    /// broader/slower fallback (production: SIMBAD Sesame) on a primary-path
    /// miss.
    async fn resolve_explicit(&self, query: &str) -> Result<ResolvedIdentity, ResolveError>;
}

// ── FakeResolver (test double) ──────────────────────────────────────────────────

/// In-memory test double for [`Resolver`] (spec 035 R7).
///
/// Returns canned results without any network access, mirroring the retired
/// `download::FakeFetcher`. Gated behind `cfg(test)` and the `test-fixture`
/// feature so it is available to other crates' integration tests but never
/// compiled into production builds.
///
/// Lookups are keyed by the *normalized* query (spec 013 normalize) so callers
/// can register a canned identity under any of its aliases.
///
/// The `call_count` atomic counter increments on every call to
/// [`Resolver::resolve`]; read it with [`FakeResolver::call_count`] to assert
/// the resolver was invoked an exact number of times (useful for verifying
/// cache-first behaviour in T023).
#[cfg(any(test, feature = "test-fixture"))]
#[derive(Debug, Default)]
pub struct FakeResolver {
    /// Normalized query → canned successful identity.
    responses: std::collections::HashMap<String, ResolvedIdentity>,
    /// Normalized query → canned error (takes precedence over `responses`).
    errors: std::collections::HashMap<String, ResolveError>,
    /// Error returned for any query with no registered response/error.
    default_error: Option<ResolveError>,
    /// Number of times [`Resolver::resolve`] has been called.
    call_count: std::sync::atomic::AtomicUsize,
    /// [`ExplicitResolver::resolve_explicit`]-only canned identity (spec 052
    /// P2): models a Sesame hit the plain `resolve()` path (TAP-only) misses.
    /// Falls back to [`Resolver::resolve`]'s own canned data when a query has
    /// no explicit-only override registered.
    explicit_responses: std::collections::HashMap<String, ResolvedIdentity>,
    /// Number of times [`ExplicitResolver::resolve_explicit`] has been called.
    explicit_call_count: std::sync::atomic::AtomicUsize,
}

#[cfg(any(test, feature = "test-fixture"))]
impl FakeResolver {
    /// Construct an empty fake; unknown queries return [`ResolveError::NotFound`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a canned successful identity, keyed by the normalized `query`.
    #[must_use]
    pub fn with_response(mut self, query: &str, identity: ResolvedIdentity) -> Self {
        self.responses.insert(targeting::normalize::normalize(query), identity);
        self
    }

    /// Register a canned error for `query`, keyed by its normalized form.
    #[must_use]
    pub fn with_error(mut self, query: &str, error: ResolveError) -> Self {
        self.errors.insert(targeting::normalize::normalize(query), error);
        self
    }

    /// Set the error returned for any unregistered query (default:
    /// [`ResolveError::NotFound`]). Use [`ResolveError::Network`] to simulate an
    /// offline resolver for degrade-to-cache tests.
    #[must_use]
    pub fn with_default_error(mut self, error: ResolveError) -> Self {
        self.default_error = Some(error);
        self
    }

    /// Return the number of times [`Resolver::resolve`] has been called.
    ///
    /// Uses `Relaxed` ordering; suitable for single-threaded test assertions
    /// after all async work has completed.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Register a canned identity returned ONLY by
    /// [`ExplicitResolver::resolve_explicit`] — models a Sesame-fallback hit
    /// the plain (TAP-only) `resolve()` path misses (spec 052 P2, FR-008).
    #[must_use]
    pub fn with_explicit_response(mut self, query: &str, identity: ResolvedIdentity) -> Self {
        self.explicit_responses.insert(targeting::normalize::normalize(query), identity);
        self
    }

    /// Return the number of times [`ExplicitResolver::resolve_explicit`] has
    /// been called.
    #[must_use]
    pub fn explicit_call_count(&self) -> usize {
        self.explicit_call_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

// `FakeResolver` cannot derive `Clone` because `AtomicUsize` does not implement
// `Clone`. Provide a manual impl that resets the counter in the clone so each
// clone starts from zero (mirroring the prior derived behaviour).
#[cfg(any(test, feature = "test-fixture"))]
impl Clone for FakeResolver {
    fn clone(&self) -> Self {
        Self {
            responses: self.responses.clone(),
            errors: self.errors.clone(),
            default_error: self.default_error.clone(),
            call_count: std::sync::atomic::AtomicUsize::new(0),
            explicit_responses: self.explicit_responses.clone(),
            explicit_call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[cfg(any(test, feature = "test-fixture"))]
#[async_trait::async_trait]
impl Resolver for FakeResolver {
    async fn resolve(&self, query: &str) -> Result<ResolvedIdentity, ResolveError> {
        self.call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let key = targeting::normalize::normalize(query);
        if let Some(err) = self.errors.get(&key) {
            return Err(err.clone());
        }
        if let Some(identity) = self.responses.get(&key) {
            return Ok(identity.clone());
        }
        Err(self.default_error.clone().unwrap_or_else(|| ResolveError::NotFound(query.to_owned())))
    }
}

#[cfg(any(test, feature = "test-fixture"))]
#[async_trait::async_trait]
impl ExplicitResolver for FakeResolver {
    async fn resolve_explicit(&self, query: &str) -> Result<ResolvedIdentity, ResolveError> {
        self.explicit_call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let key = targeting::normalize::normalize(query);
        if let Some(identity) = self.explicit_responses.get(&key) {
            return Ok(identity.clone());
        }
        // No explicit-only override registered: same canned data as resolve()
        // (does not double-count against `call_count`, which tracks `resolve`
        // specifically).
        if let Some(err) = self.errors.get(&key) {
            Err(err.clone())
        } else if let Some(identity) = self.responses.get(&key) {
            Ok(identity.clone())
        } else {
            Err(self
                .default_error
                .clone()
                .unwrap_or_else(|| ResolveError::NotFound(query.to_owned())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity() -> ResolvedIdentity {
        ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: Some("Andromeda Galaxy".to_owned()),
            object_type: ObjectType::Galaxy,
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: Some(3.44),
            aliases: vec![
                ResolvedAlias::new("M 31", AliasKind::Designation),
                ResolvedAlias::new("NGC 224", AliasKind::Designation),
                ResolvedAlias::new("Andromeda Galaxy", AliasKind::CommonName),
            ],
            source: TargetSource::Resolved,
        }
    }

    // ── Normalization choke-point (spec 052 P1 T004, FR-007) ──────────────────

    /// Alias spellings as they'd arrive from different sources — a TAP
    /// cross-ID (compact, no space), a seed/Caldwell canonical designation
    /// (space-separated), and a raw user query (extra whitespace) — must all
    /// normalize to the identical key through the single choke-point
    /// ([`ResolvedAlias::new`], which calls
    /// [`simbad_resolver::normalize::normalize`]), so they dedup onto one
    /// physical object regardless of which path produced them.
    #[test]
    fn resolved_alias_normalizes_identically_across_sources() {
        let tap_style = ResolvedAlias::new("M31", AliasKind::Designation);
        let seed_style = ResolvedAlias::new("M 31", AliasKind::Designation);
        let caldwell_style = ResolvedAlias::new("M  31", AliasKind::Designation);
        let user_query_style = ResolvedAlias::new("  m31  ", AliasKind::User);

        assert_eq!(tap_style.normalized, "m 31");
        assert_eq!(tap_style.normalized, seed_style.normalized);
        assert_eq!(tap_style.normalized, caldwell_style.normalized);
        assert_eq!(tap_style.normalized, user_query_style.normalized);
    }

    // ── ObjectType mapping (T005) ──────────────────────────────────────────────

    #[test]
    fn map_otype_maps_known_codes() {
        assert_eq!(map_otype("G"), ObjectType::Galaxy);
        assert_eq!(map_otype("PN"), ObjectType::PlanetaryNebula);
        assert_eq!(map_otype("HII"), ObjectType::EmissionNebula);
        assert_eq!(map_otype("RNe"), ObjectType::ReflectionNebula);
        assert_eq!(map_otype("DNe"), ObjectType::DarkNebula);
        assert_eq!(map_otype("OpC"), ObjectType::OpenCluster);
        assert_eq!(map_otype("GlC"), ObjectType::GlobularCluster);
        assert_eq!(map_otype("SNR"), ObjectType::SupernovaRemnant);
        assert_eq!(map_otype("ClG"), ObjectType::GalaxyCluster);
        assert_eq!(map_otype("**"), ObjectType::DoubleStar);
    }

    #[test]
    fn map_otype_unknown_is_other() {
        assert_eq!(map_otype("ZZZ"), ObjectType::Other);
        assert_eq!(map_otype(""), ObjectType::Other);
        assert_eq!(map_otype("Star"), ObjectType::Other);
    }

    #[test]
    fn map_otype_trims_whitespace() {
        assert_eq!(map_otype("  G  "), ObjectType::Galaxy);
    }

    #[test]
    fn object_type_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ObjectType::PlanetaryNebula).unwrap(),
            "\"planetary_nebula\""
        );
        assert_eq!(
            serde_json::to_string(&ObjectType::GalaxyCluster).unwrap(),
            "\"galaxy_cluster\""
        );
        assert_eq!(serde_json::to_string(&ObjectType::Other).unwrap(), "\"other\"");
    }

    // ── TargetSource wire parity ───────────────────────────────────────────────

    #[test]
    fn target_source_user_override_is_hyphenated() {
        assert_eq!(
            serde_json::to_string(&TargetSource::UserOverride).unwrap(),
            "\"user-override\""
        );
        assert_eq!(serde_json::to_string(&TargetSource::Seed).unwrap(), "\"seed\"");
        assert_eq!(serde_json::to_string(&TargetSource::Resolved).unwrap(), "\"resolved\"");
        assert_eq!(
            serde_json::from_str::<TargetSource>("\"user-override\"").unwrap(),
            TargetSource::UserOverride
        );
    }

    // ── FakeResolver (T004 seam) ───────────────────────────────────────────────

    #[tokio::test]
    async fn fake_resolver_returns_canned_response() {
        let resolver = FakeResolver::new().with_response("M31", sample_identity());
        // Registered under normalized "M31"; querying any alias form hits it.
        let got = resolver.resolve("M 31").await.unwrap();
        assert_eq!(got.primary_designation, "M 31");
        assert_eq!(got.object_type, ObjectType::Galaxy);
        assert_eq!(got.simbad_oid, Some(1_575_544));
    }

    #[tokio::test]
    async fn fake_resolver_unknown_is_not_found() {
        let resolver = FakeResolver::new();
        let err = resolver.resolve("does-not-exist").await.unwrap_err();
        assert!(matches!(err, ResolveError::NotFound(_)));
    }

    #[tokio::test]
    async fn fake_resolver_canned_error() {
        let resolver = FakeResolver::new()
            .with_error("M31", ResolveError::Ambiguous { query: "M31".to_owned(), count: 2 });
        let err = resolver.resolve("M 31").await.unwrap_err();
        assert!(matches!(err, ResolveError::Ambiguous { count: 2, .. }));
    }

    #[tokio::test]
    async fn fake_resolver_default_error_simulates_offline() {
        let resolver =
            FakeResolver::new().with_default_error(ResolveError::Network("down".to_owned()));
        let err = resolver.resolve("anything").await.unwrap_err();
        assert!(matches!(err, ResolveError::Network(_)));
    }

    #[tokio::test]
    async fn resolve_object_defaults_to_resolve() {
        let resolver = FakeResolver::new().with_response("M31", sample_identity());
        let got = resolver.resolve_object("M 31").await.unwrap();
        assert_eq!(got.primary_designation, "M 31");
    }
}
