// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! SIMBAD resolver adapter: delegates to the published `simbad-resolver`
//! crate's own [`simbad_resolver::SimbadResolver`] cache-first facade (spec
//! 052 P1, D1) rather than calling [`simbad_resolver::TapResolver`] directly.
//!
//! The facade owns cache-first resolution (persistent redb, `cache::ResolveCache`),
//! Caldwell translation, and (from 0.2.0) `v_mag` enrichment; this module is a
//! thin boundary converting between astro-plan's stable local resolver types
//! (unchanged public API since spec 035 — every existing call site keeps
//! compiling untouched) and the crate's types.
//!
//! ## Cache lifetime (D2 — one global redb file, no TTL)
//!
//! `cache::ResolveCache` wraps the crate's [`simbad_resolver::Store`] (an `Arc`
//! handle over one open `redb::Database`) and is opened ONCE at app startup
//! (`<app_data>/simbad-cache.redb`); every `resolver::SimbadResolver` built afterward
//! (settings changes rebuild the resolver, e.g. after `target.resolution
//! .settings.update`) shares that same store via a cheap clone
//! (`CacheBackend::custom`) instead of re-opening the file.
//!
//! ## Dual lookup (spec 052 P2, D5 — TAP-first, Sesame fallback)
//!
//! `resolver::SimbadResolver` wraps TWO facade instances sharing the SAME
//! `cache::ResolveCache`: `typeahead` (the `Resolver` trait impl — TAP +
//! cache only, used by the debounced as-you-type path) and
//! `explicit` (consulted only by `resolve_explicit` — TAP
//! first, [`simbad_resolver::SesameResolver`] fallback only on a TAP miss).
//! This two-entrypoint split is what makes FR-009 ("the fallback MUST NOT
//! fire during as-you-type suggestions") a structural guarantee rather than a
//! runtime flag the typeahead call site could forget to pass: the typeahead
//! path's resolver type (`network_resolvers::EitherNetworkResolver`) has no Sesame reference to
//! call, at any query. Both facades share one `cache::ResolveCache`, so a
//! Sesame-recovered identity is cached/deduped exactly like a TAP hit.
//!
//! Split by responsibility (refactor sweep #993): `cache` is the durable
//! resolve-cache handle; `network_resolvers` are the small `Resolver`
//! compositions (`network_resolvers::EitherNetworkResolver`,
//! `network_resolvers::DualLookupResolver`,
//! `network_resolvers::ExplicitNetworkResolver`) `resolver::SimbadResolver`
//! is built from; `resolver` is the production `SimbadResolver` facade
//! itself plus `resolver::OfflineResolver`; `convert` holds the
//! boundary conversions between astro-plan's local types and the upstream
//! crate's types.

mod cache;
mod convert;
mod network_resolvers;
mod resolver;

#[cfg(test)]
mod tests;

pub use cache::ResolveCache;
pub use convert::{from_crate_cached_target, from_crate_identity, from_crate_search_hit};
pub use resolver::{OfflineResolver, SimbadResolver};

/// Shared SIMBAD `basic`-row TSV tokenizer, re-exported from the upstream
/// crate. `crates/tools/seed-builder` consumes this directly. 0.2.0 widens the
/// tuple to `(oid, main_id, ra, dec, otype, v_mag)`.
pub use simbad_resolver::wire::parse_basic_row;

/// Default SIMBAD TAP sync endpoint (CDS). Must match
/// [`simbad_resolver::SimbadConfig::default`]'s endpoint (asserted by
/// `tests::default_endpoint_matches_upstream_crate`).
pub const DEFAULT_TAP_ENDPOINT: &str = "https://simbad.cds.unistra.fr/simbad/sim-tap/sync";

/// Polite identifying `User-Agent` (CDS norm) for astro-plan's own requests
/// (distinct from the upstream crate's default; both identify politely).
pub const DEFAULT_USER_AGENT: &str = "astro-plan/0.1 (+https://github.com/; spec-035 resolver)";

/// The id-namespace seed used to derive stable target ids from a designation.
/// MUST match `targeting::identity`'s hardcoded `"astro-plan.targets"` seed
/// exactly (asserted by
/// `tests::namespace_matches_sqlite_identity_derivation`) so a redb-cache id
/// and the SQLite id later written for the same designation by
/// `crate::cache::upsert_resolved` are bit-identical — the in-use "promote
/// from cache" write never needs to special-case ids.
const NAMESPACE_SEED: &str = "astro-plan.targets";

/// Configuration for a [`resolver::SimbadResolver`] — a type alias for the upstream
/// crate's config (identical field shape: `endpoint`, `timeout`, `user_agent`).
pub type SimbadConfig = simbad_resolver::SimbadConfig;
