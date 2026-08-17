// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Bundled-seed loader: warms the persistent redb resolve cache at first run
//! (spec 035 T016/R3; retargeted to redb by spec 052 P1 D2/D4/T012).
//!
//! Loads the bundled seed index (popular catalogue objects: the Messier
//! catalogue, the Caldwell objects, and a slice of NGC/IC) into the shared
//! [`crate::simbad::ResolveCache`] with `source = seed`. Seed rows are
//! superseded by `resolved`/`user-override` entries per the source-precedence
//! rules the crate's own [`simbad_resolver::Cache::upsert`] enforces, so this
//! load is safe to re-run.
//!
//! The redb cache is a **reproducible projection** (constitution §V) — it is
//! never the durable record. `canonical_target` in SQLite stays durable and is
//! written ONLY at an in-use commit ([`crate::cache::upsert_resolved`]); this
//! module also lazily copies any EXISTING durable rows into the cache
//! ([`warm_from_canonical_target`]) so targets adopted by a prior app version
//! (or a different machine sharing the SQLite file) still participate in
//! typeahead/search.
//!
//! # Asset format
//!
//! The committed asset (`assets/seed/seed.json`) is a JSON document:
//!
//! ```json
//! {
//!   "version": 1,
//!   "generated_at": "2026-06-18T00:00:00Z",
//!   "source": "SIMBAD TAP (CDS) + OpenNGC",
//!   "entries": [
//!     {
//!       "simbad_oid": 1575544,
//!       "primary_designation": "M 31",
//!       "common_name": "Andromeda Galaxy",
//!       "object_type": "galaxy",
//!       "ra_deg": 10.6847083,
//!       "dec_deg": 41.26875,
//!       "v_mag": 3.44,
//!       "aliases": [
//!         { "alias": "M 31", "kind": "designation" },
//!         { "alias": "NGC 224", "kind": "designation" },
//!         { "alias": "Andromeda Galaxy", "kind": "common_name" }
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! `v_mag` is optional (`#[serde(default)]`) since not every SIMBAD object
//! carries `allfluxes.V` photometry (galaxies and pure nebulae are frequently
//! `None`; see `bundled_asset_has_v_mag_coverage` for the measured
//! per-object-type split). The committed asset was regenerated after the
//! seed-builder V-magnitude fix (spec 052 P1 T003, #684) landed — see #696.
//!
//! The asset is built once, offline, by the `seed-builder` tool; see
//! `crates/tools/seed-builder`.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use simbad_resolver::{Cache, CacheError};

use crate::{AliasKind, ObjectType, TargetSource};

/// The bundled seed asset shipped at `assets/seed/seed.json` and embedded into
/// the binary via [`bundled`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeedAsset {
    /// Asset schema version (currently `1`).
    pub version: u32,
    /// RFC-3339 timestamp recording when the asset was generated.
    #[serde(default)]
    pub generated_at: String,
    /// Human-readable provenance string (e.g. data sources used).
    #[serde(default)]
    pub source: String,
    /// The seeded objects.
    pub entries: Vec<SeedEntry>,
}

/// One seeded canonical object plus its aliases.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeedEntry {
    /// SIMBAD physical-object id (the dedup key) when known.
    #[serde(default)]
    pub simbad_oid: Option<i64>,
    /// Canonical display designation (e.g. `M 31`).
    pub primary_designation: String,
    /// Curated common name (SIMBAD `NAME …`) when one exists.
    #[serde(default)]
    pub common_name: Option<String>,
    /// Closed object-type enum (snake_case wire value).
    pub object_type: ObjectType,
    /// ICRS J2000 right ascension in decimal degrees.
    pub ra_deg: f64,
    /// ICRS J2000 declination in decimal degrees.
    pub dec_deg: f64,
    /// Johnson V-band apparent magnitude, when the seed-builder pull found V
    /// photometry (spec 052 P1 T003). `#[serde(default)]` so entries written by
    /// the pre-052 seed-builder still parse (as `None`).
    #[serde(default)]
    pub v_mag: Option<f64>,
    /// All designations + common names for this object (the typeahead surface).
    pub aliases: Vec<SeedAlias>,
}

/// A seed alias (the normalized form is derived at load time).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeedAlias {
    /// Verbatim designation or common name.
    pub alias: String,
    /// Whether this alias is a designation or a curated common name.
    pub kind: AliasKind,
}

/// Errors raised while loading the bundled seed.
#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    /// The seed JSON could not be parsed.
    #[error("failed to parse seed asset: {0}")]
    Parse(#[from] serde_json::Error),
    /// A cache write failed.
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// A query against the durable `canonical_target` table failed (used only
    /// by [`warm_from_canonical_target`]'s lazy backfill).
    #[error(transparent)]
    Sqlite(#[from] crate::cache::CacheError),
}

impl SeedEntry {
    /// Convert a seed entry into a crate [`simbad_resolver::ResolvedIdentity`]
    /// with `source = seed`, ready for [`simbad_resolver::Cache::upsert`].
    fn to_crate_identity(&self) -> simbad_resolver::ResolvedIdentity {
        let aliases: Vec<simbad_resolver::ResolvedAlias> = self
            .aliases
            .iter()
            .map(|a| {
                simbad_resolver::ResolvedAlias::new(a.alias.clone(), to_crate_alias_kind(a.kind))
            })
            .collect();
        simbad_resolver::ResolvedIdentity {
            simbad_oid: self.simbad_oid,
            primary_designation: self.primary_designation.clone(),
            common_name: self.common_name.clone(),
            object_type: to_crate_object_type(self.object_type),
            otype_raw: String::new(),
            ra_deg: self.ra_deg,
            dec_deg: self.dec_deg,
            v_mag: self.v_mag,
            aliases,
            source: simbad_resolver::TargetSource::Seed,
        }
    }
}

fn to_crate_alias_kind(k: AliasKind) -> simbad_resolver::AliasKind {
    match k {
        AliasKind::Designation => simbad_resolver::AliasKind::Designation,
        AliasKind::CommonName => simbad_resolver::AliasKind::CommonName,
        AliasKind::User => simbad_resolver::AliasKind::User,
    }
}

fn to_crate_object_type(o: ObjectType) -> simbad_resolver::ObjectType {
    simbad_resolver::ObjectType::from_wire(o.as_wire())
}

fn to_crate_source(s: TargetSource) -> simbad_resolver::TargetSource {
    match s {
        TargetSource::Seed => simbad_resolver::TargetSource::Seed,
        TargetSource::Resolved => simbad_resolver::TargetSource::Resolved,
        TargetSource::UserOverride => simbad_resolver::TargetSource::UserOverride,
    }
}

impl SeedAsset {
    /// Parse a [`SeedAsset`] from raw JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SeedError::Parse`] if the bytes are not a valid seed document.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SeedError> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// The seed asset compiled into the binary from `assets/seed/seed.json`.
///
/// # Errors
///
/// Returns [`SeedError::Parse`] if the embedded asset is malformed (a build-time
/// guarantee in practice, since the asset is committed and tested).
pub fn bundled() -> Result<SeedAsset, SeedError> {
    // spec 042 (T250): this crate moved one level deeper (crates/targeting →
    // crates/targeting/resolver), so the repo-root `assets/seed/seed.json` is
    // now three `..` hops up instead of two.
    const RAW: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/seed/seed.json"));
    SeedAsset::from_json(RAW)
}

/// Stripped E2E seed (~200 entries vs ~13k in the full seed) compiled from
/// `assets/seed/seed-e2e.json`. Contains all Messier objects plus a
/// coordinate/type-diverse NGC/IC/other subset that covers every E2E test
/// scenario without the ~2.5s warm cost of the full 13k-row seed.
///
/// Used by the E2E pre-warm harness (`crates/e2e-tests`) to build a cache
/// file once per shard job, then copy it into each instance's appdata.
///
/// # Errors
///
/// Returns [`SeedError::Parse`] if the embedded asset is malformed.
pub fn bundled_e2e() -> Result<SeedAsset, SeedError> {
    const RAW: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/seed/seed-e2e.json"));
    SeedAsset::from_json(RAW)
}

/// Whether the redb cache is empty (first run — spec 052 P1 D2/T012).
///
/// An empty cache means neither the bundled seed nor any durable
/// `canonical_target` row has been warmed into it yet. The warm itself is
/// idempotent (upsert dedups), so this is an optimization, not a correctness
/// requirement.
///
/// NOT used by [`warm_bundled_on_first_run`] (see the warm-complete sentinel
/// below) — emptiness is unreliable there: a durable single-item write (a
/// user search mid-warm, since [`Cache::upsert`] always stays durable even
/// against an [`simbad_resolver::BatchDurability::Eventual`] store) can
/// persist earlier `Eventual` seed chunks too (redb commits are cumulative),
/// so a crash mid-warm can leave a non-empty but PARTIAL cache — this check
/// would then see "not empty" and skip the re-warm forever (#818 follow-up).
/// Still useful on its own (kept for `warm_cache`'s callers and the
/// `canonical_target` backfill, which has no version concept to gate on).
///
/// # Errors
///
/// Returns [`SeedError::Cache`] on a cache backend failure.
pub async fn is_first_run(cache: &dyn Cache) -> Result<bool, SeedError> {
    Ok(cache.list().await?.is_empty())
}

/// Reserved, never-real `simbad_oid` for the warm-complete sentinel row
/// below — real SIMBAD physical-object ids are always positive, so a
/// negative value can never collide with an actual resolved/seeded object.
const SENTINEL_SIMBAD_OID: i64 = -1;

/// Whether `simbad_oid` identifies the warm-complete sentinel row (see
/// `sentinel_identity`) rather than a real cached target. The bundled seed
/// is stored in the SAME `Cache`-backed store any typeahead/cone-search
/// query reads from (the crate exposes no separate metadata table), so any
/// app-facing surface that reads broadly from the cache (`target.search`'s
/// `cache.search`, in particular) MUST filter this out explicitly — a
/// reserved designation string alone is not a structural guarantee against a
/// fuzzy/substring match.
#[must_use]
pub fn is_warm_sentinel(simbad_oid: Option<i64>) -> bool {
    simbad_oid == Some(SENTINEL_SIMBAD_OID)
}

/// Reserved designation for the warm-complete sentinel row. The crate has no
/// metadata table separate from the target/alias store it already exposes
/// via [`Cache`] (confirmed against `simbad-resolver` 0.3.2's public API), so
/// the sentinel lives as an ordinary row in that same store instead — but
/// always looked up and deduped by [`SENTINEL_SIMBAD_OID`] (`simbad_oid`
/// takes precedence over a designation-derived id in the crate's own
/// dedup — see `simbad-resolver`'s `upsert_within`), so this string's exact
/// spelling is not a correctness requirement, only a debugging aid; the
/// leading `∅` plus spaces make it visually obvious in a redb dump or log
/// line that this is not a real catalogue designation.
const SENTINEL_DESIGNATION: &str = "\u{2205} ALM SEED WARM SENTINEL";

/// Build the warm-complete sentinel row for `seed`, carrying its
/// `generated_at` in `common_name` as the version key (spec 052 P4/#818
/// follow-up — "prefer a content hash or that timestamp"; the timestamp is
/// simpler and the seed-builder tool already bumps it on every regen, so a
/// hash would only guard against a same-timestamp-different-content mistake
/// that tool doesn't make).
fn sentinel_identity(seed: &SeedAsset) -> simbad_resolver::ResolvedIdentity {
    simbad_resolver::ResolvedIdentity {
        simbad_oid: Some(SENTINEL_SIMBAD_OID),
        primary_designation: SENTINEL_DESIGNATION.to_owned(),
        common_name: Some(seed.generated_at.clone()),
        object_type: simbad_resolver::ObjectType::Other,
        otype_raw: String::new(),
        ra_deg: 0.0,
        dec_deg: 0.0,
        v_mag: None,
        aliases: vec![simbad_resolver::ResolvedAlias::new(
            SENTINEL_DESIGNATION,
            simbad_resolver::AliasKind::Designation,
        )],
        source: simbad_resolver::TargetSource::Seed,
    }
}

/// Whether the cache already holds a warm-complete sentinel matching `seed`'s
/// `generated_at` — i.e. the bundled seed warm can be skipped. `false` for
/// both "never warmed" (no sentinel) and "warmed a since-superseded seed
/// version" (sentinel present, `generated_at` differs, e.g. after #696-style
/// regenerated-asset ships in a new app build) — either way
/// [`warm_bundled_on_first_run`] must (re-)run, which is safe: the warm is
/// idempotent (upsert dedups).
///
/// # Errors
///
/// Returns [`SeedError::Cache`] on a cache backend failure.
async fn sentinel_matches(cache: &dyn Cache, seed: &SeedAsset) -> Result<bool, SeedError> {
    Ok(cache
        .get_by_simbad_oid(SENTINEL_SIMBAD_OID)
        .await?
        .is_some_and(|row| row.common_name.as_deref() == Some(seed.generated_at.as_str())))
}

/// Chunk size for `chunked_upsert_batch` (spec 052 P4/#818 follow-up): one
/// write transaction for the WHOLE seed leaves nothing visible to a reader
/// until it all commits, so a `target.search` query racing a multi-second
/// warm can miss an object the seed does contain simply because its part of
/// the batch hasn't committed yet. Chunking restores incremental
/// visibility — each chunk's rows become searchable the moment ITS
/// transaction commits — while keeping nearly all of batching's fsync
/// savings (~13 write transactions for the full ~13k-object bundled seed,
/// instead of 1 atomic transaction or ~13k per-entry ones pre-#695).
const WARM_CHUNK_SIZE: usize = 1000;

/// Upsert `identities` in `WARM_CHUNK_SIZE`-sized slices, one
/// [`Cache::upsert_batch`] write transaction per chunk, summing the loaded
/// count across chunks. Shared by [`warm_cache`] and
/// [`warm_from_canonical_target`], which both build a full identity list up
/// front and then need this exact chunk-and-count upsert.
async fn chunked_upsert_batch(
    cache: &dyn Cache,
    identities: &[simbad_resolver::ResolvedIdentity],
    namespace: &Uuid,
) -> Result<usize, SeedError> {
    let mut loaded = 0usize;
    for chunk in identities.chunks(WARM_CHUNK_SIZE) {
        let results = cache.upsert_batch(chunk, namespace).await?;
        loaded += results
            .iter()
            .filter(|(_, outcome)| {
                !matches!(outcome, simbad_resolver::UpsertOutcome::SkippedUserOverride)
            })
            .count();
    }
    Ok(loaded)
}

/// Warm the redb cache from a seed asset, writing rows with `source = seed`.
///
/// Entries are upserted via `chunked_upsert_batch` (one
/// [`simbad_resolver::Cache::upsert_batch`] write transaction per
/// `WARM_CHUNK_SIZE`-sized chunk, rather than one per entry — spec 052
/// P4/#695 — or one atomic transaction for the whole seed, which left
/// nothing visible until it all committed — #818 follow-up), so the warm is
/// idempotent: re-running it dedups by `simbad_oid` (or the
/// designation-derived id) and never overwrites a sticky `user-override`
/// row. Returns the number of entries that resulted in a new or refreshed
/// cache row.
///
/// `namespace` MUST be the same id-namespace the production facade uses
/// ([`crate::simbad`]'s `"astro-plan.targets"` seed) so warmed ids match the
/// ids a later in-use promotion writes to SQLite.
///
/// This unconditionally warms `seed`; call [`is_first_run`] first if you only
/// want to seed an empty cache.
///
/// # Errors
///
/// Returns [`SeedError::Cache`] if a cache write fails.
pub async fn warm_cache(
    cache: &dyn Cache,
    seed: &SeedAsset,
    namespace: &Uuid,
) -> Result<usize, SeedError> {
    let identities: Vec<simbad_resolver::ResolvedIdentity> =
        seed.entries.iter().map(SeedEntry::to_crate_identity).collect();
    chunked_upsert_batch(cache, &identities, namespace).await
}

/// Warm the redb cache from the bundled seed **only when it isn't already
/// warmed for this exact seed version**.
///
/// Gated on the warm-complete sentinel (`sentinel_matches`), not cache
/// emptiness (spec 052 P4/#818 follow-up) — a crash partway through a
/// chunked `Eventual` warm can leave a non-empty but partial cache (see
/// [`is_first_run`]'s doc comment), which an emptiness check would mistake
/// for "already done" forever. The sentinel also carries the seed's
/// `generated_at`, so a newer app build shipping a regenerated asset
/// (#696-style) re-warms existing installs instead of leaving them stuck on
/// the version they first launched with.
///
/// Returns `Some(count)` of warmed entries when a warm happened (first run,
/// a prior partial/crashed warm, or a seed version bump), or `None` when the
/// cache already matches the current bundled seed (no-op).
///
/// # Errors
///
/// Returns [`SeedError`] on a malformed embedded asset or a cache failure.
pub async fn warm_bundled_on_first_run(
    cache: &dyn Cache,
    namespace: &Uuid,
) -> Result<Option<usize>, SeedError> {
    let seed = bundled()?;
    if sentinel_matches(cache, &seed).await? {
        return Ok(None);
    }
    let loaded = warm_cache(cache, &seed, namespace).await?;
    // Sentinel written LAST, via a single-item `Cache::upsert` (always
    // durable regardless of the store's bulk `BatchDurability` — see
    // `simbad-resolver` 0.3.2's `BatchDurability` doc comment): this one
    // commit persists itself AND every prior `Eventual` chunk from
    // `warm_cache` above (redb commits are cumulative), so "sentinel
    // present and matching" is only ever true once the whole seed warm it
    // attests to is itself durably on disk.
    let identity = sentinel_identity(&seed);
    cache.upsert(&identity, namespace).await?;
    Ok(Some(loaded))
}

/// Lazily copy every durable `canonical_target` row into the redb cache
/// (spec 052 P1 T012 — "warm ... lazily from existing canonical_target
/// rows"), so targets already adopted (in a prior app version, or a shared
/// SQLite file) participate in typeahead/search without waiting for a
/// re-resolve. A no-op when SQLite holds no rows. Idempotent (upsert dedups);
/// safe to call on every startup.
///
/// # Errors
///
/// Returns [`SeedError::Sqlite`] on a SQLite read failure, or
/// [`SeedError::Cache`] on a cache write failure.
pub async fn warm_from_canonical_target(
    cache: &dyn Cache,
    pool: &SqlitePool,
    namespace: &Uuid,
) -> Result<usize, SeedError> {
    let rows = crate::cache::list_all(pool).await?;
    let mut identities = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(durable) = crate::cache::get_by_id(pool, row.id).await? else { continue };
        identities.push(simbad_resolver::ResolvedIdentity {
            simbad_oid: durable.simbad_oid,
            primary_designation: durable.primary_designation,
            common_name: durable
                .aliases
                .iter()
                .find(|a| a.kind == AliasKind::CommonName)
                .map(|a| a.alias.clone()),
            object_type: to_crate_object_type(durable.object_type),
            otype_raw: String::new(),
            ra_deg: durable.ra_deg,
            dec_deg: durable.dec_deg,
            v_mag: row.magnitude,
            aliases: durable
                .aliases
                .into_iter()
                .map(|a| simbad_resolver::ResolvedAlias::new(a.alias, to_crate_alias_kind(a.kind)))
                .collect(),
            source: to_crate_source(durable.source),
        });
    }
    chunked_upsert_batch(cache, &identities, namespace).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use simbad_resolver::Store;

    fn ns() -> Uuid {
        simbad_resolver::identity::namespace("astro-plan.targets")
    }

    fn fixture() -> SeedAsset {
        SeedAsset {
            version: 1,
            generated_at: "2026-06-18T00:00:00Z".to_owned(),
            source: "test fixture".to_owned(),
            entries: vec![
                SeedEntry {
                    simbad_oid: Some(1_575_544),
                    primary_designation: "M 31".to_owned(),
                    common_name: Some("Andromeda Galaxy".to_owned()),
                    object_type: ObjectType::Galaxy,
                    ra_deg: 10.684_708,
                    dec_deg: 41.268_75,
                    v_mag: Some(3.44),
                    aliases: vec![
                        SeedAlias { alias: "M 31".to_owned(), kind: AliasKind::Designation },
                        SeedAlias { alias: "NGC 224".to_owned(), kind: AliasKind::Designation },
                        SeedAlias {
                            alias: "Andromeda Galaxy".to_owned(),
                            kind: AliasKind::CommonName,
                        },
                    ],
                },
                SeedEntry {
                    simbad_oid: Some(1_237_320),
                    primary_designation: "M 42".to_owned(),
                    common_name: Some("Orion Nebula".to_owned()),
                    object_type: ObjectType::EmissionNebula,
                    ra_deg: 83.822_083,
                    dec_deg: -5.391_111,
                    v_mag: None,
                    aliases: vec![
                        SeedAlias { alias: "M 42".to_owned(), kind: AliasKind::Designation },
                        SeedAlias { alias: "NGC 1976".to_owned(), kind: AliasKind::Designation },
                        SeedAlias { alias: "Orion Nebula".to_owned(), kind: AliasKind::CommonName },
                    ],
                },
            ],
        }
    }

    #[test]
    fn json_round_trip() {
        let asset = fixture();
        let json = serde_json::to_vec(&asset).unwrap();
        let back = SeedAsset::from_json(&json).unwrap();
        assert_eq!(back.entries.len(), 2);
        assert_eq!(back.entries[0].primary_designation, "M 31");
        assert_eq!(back.entries[0].object_type, ObjectType::Galaxy);
        assert_eq!(back.entries[0].v_mag, Some(3.44));
    }

    #[test]
    fn json_parses_without_v_mag_field_pre_052_asset() {
        // Pre-052 committed assets have no `v_mag` key at all.
        let json = r#"{"version":1,"entries":[{"primary_designation":"M 1",
            "object_type":"planetary_nebula","ra_deg":1.0,"dec_deg":2.0,"aliases":[]}]}"#;
        let asset = SeedAsset::from_json(json.as_bytes()).unwrap();
        assert_eq!(asset.entries[0].v_mag, None);
    }

    #[tokio::test]
    async fn first_run_then_not() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        assert!(is_first_run(&cache).await.unwrap());
        warm_cache(&cache, &fixture(), &ns()).await.unwrap();
        assert!(!is_first_run(&cache).await.unwrap());
    }

    #[tokio::test]
    async fn warm_populates_cache_and_offline_lookup_works() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let loaded = warm_cache(&cache, &fixture(), &ns()).await.unwrap();
        assert_eq!(loaded, 2);

        // Offline lookup by a non-primary alias resolves to the canonical row.
        let norm = targeting::normalize::normalize("NGC 224");
        let got = cache.get_by_normalized(&norm).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "M 31");
        assert_eq!(got.source, simbad_resolver::TargetSource::Seed);
        assert_eq!(got.v_mag, Some(3.44));

        // Common-name lookup also works.
        let norm = targeting::normalize::normalize("Orion Nebula");
        let got = cache.get_by_normalized(&norm).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "M 42");
    }

    #[tokio::test]
    async fn warm_is_idempotent() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        warm_cache(&cache, &fixture(), &ns()).await.unwrap();
        warm_cache(&cache, &fixture(), &ns()).await.unwrap();
        assert_eq!(cache.list().await.unwrap().len(), 2, "re-warming must not duplicate rows");
    }

    #[tokio::test]
    async fn first_run_warm_is_noop_when_populated() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        warm_cache(&cache, &fixture(), &ns()).await.unwrap();
        assert!(!is_first_run(&cache).await.unwrap());
    }

    #[tokio::test]
    async fn seed_does_not_overwrite_user_override() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        // Simulate a user override for M 31's oid.
        let override_identity = simbad_resolver::ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "My Andromeda".to_owned(),
            common_name: None,
            object_type: simbad_resolver::ObjectType::Galaxy,
            otype_raw: String::new(),
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: None,
            aliases: vec![simbad_resolver::ResolvedAlias::new(
                "My Andromeda",
                simbad_resolver::AliasKind::Designation,
            )],
            source: simbad_resolver::TargetSource::UserOverride,
        };
        cache.upsert(&override_identity, &ns()).await.unwrap();

        // Seeding must not clobber the override.
        warm_cache(&cache, &fixture(), &ns()).await.unwrap();
        let got = cache.get_by_simbad_oid(1_575_544).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "My Andromeda");
        assert_eq!(got.source, simbad_resolver::TargetSource::UserOverride);
    }

    #[test]
    fn bundled_asset_loads_and_covers_messier_and_caldwell() {
        // The committed asset must parse and cover the MVP set.
        let asset = bundled().expect("bundled seed asset must parse");
        assert!(asset.version >= 1);
        assert!(
            asset.entries.len() >= 110,
            "expected at least the full Messier catalogue, got {}",
            asset.entries.len()
        );
        // Spot-check a Messier object is present.
        let has_m31 = asset.entries.iter().any(|e| e.primary_designation == "M 31");
        assert!(has_m31, "seed must include M 31");
    }

    #[test]
    fn bundled_e2e_asset_covers_all_test_scenarios() {
        let asset = bundled_e2e().expect("e2e seed asset must parse");
        assert!(asset.version >= 1);
        // Must exceed the bundled_asset_loads_and_covers threshold.
        assert!(
            asset.entries.len() >= 110,
            "e2e seed must have >= 110 entries for unit test compatibility, got {}",
            asset.entries.len()
        );
        // All E2E-required targets present.
        for required in ["M 1", "M 31", "M 33", "M 42"] {
            assert!(
                asset.entries.iter().any(|e| e.primary_designation == required),
                "e2e seed must include {required}"
            );
        }
        // v_mag coverage must exceed the 20% threshold from
        // `bundled_asset_has_v_mag_coverage`.
        let with_v_mag = asset.entries.iter().filter(|e| e.v_mag.is_some()).count();
        #[allow(clippy::cast_precision_loss)]
        let coverage = with_v_mag as f64 / asset.entries.len() as f64;
        assert!(
            coverage > 0.20,
            "e2e seed v_mag coverage {:.1}% is below the 20% threshold",
            coverage * 100.0
        );
    }

    /// Regression guard for #696: the committed asset shipped for a long
    /// while with zero `v_mag` coverage (generated before the seed-builder
    /// `f.V` / `allfluxes` join fix, spec 052 P1 T003, landed in #684). A
    /// live re-run measured ~26% coverage (galaxies ~14%; double
    /// stars/clusters/planetary nebulae 55-92%; pure emission/dark/reflection
    /// nebulae mostly 0% — SIMBAD's `allfluxes.V` is sparse for those object
    /// types). The 20% floor sits safely under that measured value so it
    /// only trips on a genuine stale (all-null) regen, not routine SIMBAD
    /// drift between rebuilds.
    #[test]
    #[allow(clippy::cast_precision_loss)] // counts are ~13k, far below f64's exact-integer range
    fn bundled_asset_has_v_mag_coverage() {
        let asset = bundled().expect("bundled seed asset must parse");
        let with_v_mag = asset.entries.iter().filter(|e| e.v_mag.is_some()).count();
        let coverage = with_v_mag as f64 / asset.entries.len() as f64;
        assert!(
            coverage > 0.20,
            "bundled seed v_mag coverage regressed to {:.1}% ({with_v_mag}/{}) — asset looks stale, rerun seed-builder",
            coverage * 100.0,
            asset.entries.len()
        );

        let m31 = asset
            .entries
            .iter()
            .find(|e| e.primary_designation == "M 31")
            .expect("seed must include M 31");
        let v_mag = m31.v_mag.expect("M 31 must carry a v_mag");
        assert!((3.0..4.0).contains(&v_mag), "M 31 v_mag should be ~3.4, got {v_mag}");
    }

    /// A real Messier-only slice of the committed bundled asset (~110 objects,
    /// including M 31/M 42) — fast enough for redb-touching tests. [`warm_cache`]
    /// goes through `chunked_upsert_batch` (one [`simbad_resolver::Cache::
    /// upsert_batch`] write transaction per `WARM_CHUNK_SIZE`-sized chunk —
    /// spec 052 P4/#695, chunked by the first #818 follow-up), which on a
    /// `simbad_resolver::BatchDurability::Eventual` file-backed store
    /// (`crate::simbad::ResolveCache::open`'s production configuration since
    /// the second #818 follow-up) measures ~3.2s total (warm + one flush)
    /// for the full ~13k-object bundled seed, debug build — close to a
    /// single atomic transaction's ~2.4s, and far better than the ~5.8s a
    /// `Durable`-store chunked warm costs (13 fsyncs instead of 1) — but
    /// still needless overhead to pay on every `cargo test`/`nextest`
    /// invocation across every test that only needs a handful of real,
    /// known objects. A Messier-only slice keeps this suite fast regardless.
    /// [`bundled_asset_loads_and_covers_messier_and_caldwell`] separately
    /// proves the full committed asset's shape (pure JSON parse, no redb).
    fn messier_only_seed() -> SeedAsset {
        let full = bundled().expect("bundled seed asset must parse");
        let entries: Vec<SeedEntry> =
            full.entries.into_iter().filter(|e| e.primary_designation.starts_with("M ")).collect();
        SeedAsset {
            version: full.version,
            generated_at: full.generated_at,
            source: full.source,
            entries,
        }
    }

    #[tokio::test]
    async fn bundled_seed_warms_cache() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let seed = messier_only_seed();
        let loaded = warm_cache(&cache, &seed, &ns()).await.unwrap();
        assert!(loaded >= 80, "expected the full Messier catalogue, got {loaded}");

        // Offline typeahead for a seeded Messier object works with no network.
        let norm = targeting::normalize::normalize("M 31");
        let got = cache.get_by_normalized(&norm).await.unwrap();
        assert!(got.is_some(), "M 31 must be resolvable from the seeded cache");

        // Re-warming is a no-op (idempotent upsert).
        assert!(!cache.list().await.unwrap().is_empty());
        let reloaded = warm_cache(&cache, &seed, &ns()).await.unwrap();
        assert!(reloaded >= 80, "re-warm dedups by oid/id, not by row count delta");
    }

    /// Regression guard for the #818 follow-up (crash-recovery for partial
    /// warms): a crashed process can leave the cache non-empty but PARTIAL
    /// (an unrelated durable single-item write — e.g. a live resolve — can
    /// persist earlier `Eventual` seed chunks too, since redb commits are
    /// cumulative, without the warm ever reaching its sentinel step). The
    /// NEXT `warm_bundled_on_first_run` call — modelling the next app
    /// launch — must recognize this as incomplete (no sentinel yet) and
    /// finish the job, not treat "non-empty" as "already done".
    #[tokio::test]
    async fn warm_bundled_on_first_run_recovers_from_a_partial_warm() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let namespace = ns();
        let full = bundled().expect("bundled seed asset must parse");

        // Simulate the crash: durably persist a slice of the real bundled
        // seed directly via `warm_cache` (bypassing `warm_bundled_on_first_run`
        // entirely, so no sentinel is ever written) — the cache ends up
        // non-empty but with only part of the seed present.
        let partial = SeedAsset {
            version: full.version,
            generated_at: full.generated_at.clone(),
            source: full.source.clone(),
            entries: full.entries.iter().take(50).cloned().collect(),
        };
        let partial_loaded = warm_cache(&cache, &partial, &namespace).await.unwrap();
        assert_eq!(partial_loaded, 50, "the simulated partial warm must land exactly 50 rows");
        assert!(
            !sentinel_matches(&cache, &full).await.unwrap(),
            "a partial (crashed) warm must not look complete — no sentinel was ever written"
        );

        // The next warm call: must NOT skip (an emptiness check would have —
        // the cache is non-empty), and must warm the FULL seed, not just
        // whatever was missing.
        let loaded = warm_bundled_on_first_run(&cache, &namespace)
            .await
            .unwrap()
            .expect("a partial/unsentineled cache must trigger a real warm, not a no-op");
        assert!(
            loaded >= full.entries.len(),
            "expected the full seed to be (re-)warmed, got {loaded}"
        );
        assert!(
            sentinel_matches(&cache, &full).await.unwrap(),
            "warm_bundled_on_first_run must write a matching sentinel once it completes"
        );

        // Idempotent: a further call now correctly no-ops (sentinel matches).
        let noop = warm_bundled_on_first_run(&cache, &namespace).await.unwrap();
        assert!(noop.is_none(), "a matching sentinel must skip the warm on the next call");
    }

    /// Regression guard for the #818 follow-up: a newer seed build shipping
    /// a regenerated asset (#696-style) must trigger a re-warm on existing
    /// installs rather than being masked by an old, no-longer-matching
    /// sentinel — proven directly against `sentinel_matches` (the exact
    /// mechanism `warm_bundled_on_first_run` gates on), independent of the
    /// committed asset's real size.
    #[tokio::test]
    async fn seed_version_change_invalidates_the_sentinel() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let namespace = ns();

        let mut seed = messier_only_seed();
        seed.generated_at = "2020-01-01T00:00:00Z".to_owned();
        warm_cache(&cache, &seed, &namespace).await.unwrap();
        let identity = sentinel_identity(&seed);
        cache.upsert(&identity, &namespace).await.unwrap();
        assert!(
            sentinel_matches(&cache, &seed).await.unwrap(),
            "a freshly-written sentinel must match the seed it was written for"
        );

        let mut newer_seed = seed.clone();
        newer_seed.generated_at = "2026-07-14T00:00:00Z".to_owned();
        assert!(
            !sentinel_matches(&cache, &newer_seed).await.unwrap(),
            "a version-mismatched sentinel must not be treated as complete"
        );
    }

    /// A synthetic seed of `n` distinct objects (unique `simbad_oid` +
    /// designation each), for exercising chunk boundaries without depending
    /// on the committed asset's real size.
    fn synthetic_seed(n: usize) -> SeedAsset {
        let entries = (0..n)
            .map(|i| SeedEntry {
                simbad_oid: Some(9_000_000 + i64::try_from(i).expect("test seed size fits i64")),
                primary_designation: format!("SYN {i}"),
                common_name: None,
                object_type: ObjectType::Galaxy,
                ra_deg: f64::from(u32::try_from(i % 360).expect("modulo 360 fits u32")),
                dec_deg: 0.0,
                v_mag: None,
                aliases: vec![SeedAlias {
                    alias: format!("SYN {i}"),
                    kind: AliasKind::Designation,
                }],
            })
            .collect();
        SeedAsset { version: 1, generated_at: String::new(), source: String::new(), entries }
    }

    /// Regression guard for the #818 follow-up (chunked batching,
    /// `WARM_CHUNK_SIZE`): a seed spanning multiple chunks must warm
    /// EVERY entry exactly once — no drop, duplicate, or corruption at a
    /// chunk boundary — proving `chunked_upsert_batch`'s per-chunk loop is
    /// equivalent to the old single whole-seed `upsert_batch` call for the
    /// final state, even though visibility now arrives incrementally.
    #[tokio::test]
    async fn warm_cache_across_multiple_chunks_loses_nothing() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let n = WARM_CHUNK_SIZE * 2 + 500; // spans 3 chunks
        let seed = synthetic_seed(n);

        let loaded = warm_cache(&cache, &seed, &ns()).await.unwrap();
        assert_eq!(loaded, n, "every synthetic entry must be counted as loaded");

        let rows = cache.list().await.unwrap();
        assert_eq!(rows.len(), n, "every synthetic entry must be a distinct cache row");

        // First entry (chunk 1), one crossing the chunk-1/chunk-2 boundary,
        // and the last entry (final chunk) must all be independently queryable.
        for i in [0, WARM_CHUNK_SIZE - 1, WARM_CHUNK_SIZE, n - 1] {
            let norm = targeting::normalize::normalize(&format!("SYN {i}"));
            assert!(
                cache.get_by_normalized(&norm).await.unwrap().is_some(),
                "SYN {i} must be resolvable after a multi-chunk warm"
            );
        }
    }

    /// SC-001: repeat search of a cached object issues zero network calls, and
    /// resolves fast, entirely from the local redb cache (no SQLite, no TAP).
    #[tokio::test]
    async fn sc001_offline_seed_typeahead_under_100ms() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();

        let loaded =
            warm_cache(&cache, &messier_only_seed(), &ns()).await.expect("seed warm must not fail");
        assert!(
            loaded >= 80,
            "seed must warm the Messier catalogue for this test to be meaningful"
        );

        let t0 = std::time::Instant::now();
        let results_m42 = cache.search("m 42", 20).await.expect("search must not fail");
        let elapsed_m42 = t0.elapsed();

        assert!(
            !results_m42.is_empty(),
            "offline search for 'm 42' must return results from seeded cache"
        );
        assert_eq!(
            results_m42[0].target.primary_designation, "M 42",
            "top result for 'm 42' must be M 42 (Orion Nebula)"
        );
        assert!(
            elapsed_m42 < std::time::Duration::from_millis(100),
            "SC-001 violated: offline typeahead for 'm 42' took {elapsed_m42:?}, must be < 100 ms",
        );
    }

    /// Lazy backfill (T012): an existing durable `canonical_target` row (e.g.
    /// from a prior app version) is copied into a freshly-opened redb cache.
    #[tokio::test]
    async fn warm_from_canonical_target_backfills_durable_rows() {
        let db = persistence_core::Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        let identity = crate::ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: Some("Andromeda Galaxy".to_owned()),
            object_type: ObjectType::Galaxy,
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: Some(3.44),
            aliases: vec![crate::ResolvedAlias::new("M 31", AliasKind::Designation)],
            source: TargetSource::Resolved,
        };
        crate::cache::upsert_resolved(db.pool(), &identity).await.unwrap();

        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        assert!(is_first_run(&cache).await.unwrap());

        let warmed = warm_from_canonical_target(&cache, db.pool(), &ns()).await.unwrap();
        assert_eq!(warmed, 1);

        let norm = targeting::normalize::normalize("M 31");
        let got = cache.get_by_normalized(&norm).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "M 31");
        assert_eq!(got.v_mag, Some(3.44));
    }
}
