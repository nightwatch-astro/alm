// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Target search/resolve Tauri commands (spec 035 SIMBAD resolution; redb
//! resolve-cache facade adoption + in-use promotion by spec 052 P1).
#![allow(clippy::doc_markdown)] // spec/domain terminology not suited for backticks
//!
//! ## Commands
//!
//! - `target.resolve` — cache-first SIMBAD resolution, TAP + cache only (spec
//!   035); no longer writes `canonical_target` (spec 052 P1 FR-004). The
//!   debounced typeahead entrypoint — MUST NOT be called per keystroke with
//!   the Sesame fallback (it never reaches it at all).
//! - `target.resolve_explicit` — the deliberate resolve/confirm entrypoint
//!   (Enter with no typeahead match, "search more", Add/Confirm submit): TAP
//!   first, SIMBAD Sesame fallback only on a TAP miss (spec 052 P2,
//!   FR-008/FR-009). Same request/response contract as `target.resolve`.
//! - `target.search` — local typeahead search over the shared redb resolve
//!   cache (spec 035/052).
//! - `target.adopt` — explicit in-use commit for UI flows with no other
//!   natural commit point (spec 052 P1 FR-004).
//! - `target.cache.clear` — wipe the redb resolve cache; re-warm runs as a
//!   background task (FR-002; issue #695).
//! - `target.resolution.settings` / `target.resolution.settings.update` — resolver settings.
//! - `target.astro_format.batch` — batched sexagesimal RA/Dec formatting (adopt target-match).
//! - `target.moon_opposition.batch` — batched Moon-separation + next-opposition
//!   for N targets in one call, replacing per-row TS ephemeris math (#634).
//!
//! Spec-013 commands `target.lookup` and `target.resolve.fits` have been
//! removed by spec 036 (superseded by spec-035 `target.search`/`target.resolve`).

use contracts_core::cone_search::{
    ConeSearchConfirmRequest, ConeSearchConfirmResponse, ConeSearchSuggestRequest,
    ConeSearchSuggestResponse,
};
use contracts_core::targets::{
    ResolverSettingsGetRequest, ResolverSettingsResponse, ResolverSettingsUpdateRequest,
    TargetAdoptRequest, TargetAdoptResponse, TargetAstroFormat, TargetAstroFormatBatchRequest,
    TargetAstroFormatBatchResponse, TargetCacheClearResponse, TargetResolveSimbadRequest,
    TargetResolveSimbadResponse, TargetSearchRequest, TargetSearchResponse,
};
use contracts_core::ContractError;
use persistence_targets::repositories::q_desktop::get_resolver_settings;
use tauri::State;

use crate::commands::lifecycle::AppState;

// ── target.resolve (spec 035 — SIMBAD cache-first resolution, US3) ───────────────

/// `target.resolve` — resolve a designation / common name (or FITS OBJECT
/// value) against the shared redb resolve cache, falling back to SIMBAD's
/// tabular (TAP) path on a miss when online resolution is enabled (spec 035).
/// The debounced typeahead entrypoint — TAP + cache only, never the Sesame
/// fallback (spec 052 P2 FR-009; see `target_resolve_explicit` for that).
/// Never writes `canonical_target` itself (spec 052 P1 FR-004) except via the
/// manual `override` path (T032) — see `app_core::target_resolve` for the
/// in-use promotion commit points.
///
/// The live `SimbadResolver` facade is built on demand from the persisted
/// `resolver_settings` (endpoint + timeout) plus the app-lifetime shared
/// redb cache. Cache hits never re-query SIMBAD (FR-006); offline / unknown /
/// ambiguous outcomes return `unresolved` and never fabricate coordinates
/// (FR-009).
///
/// # Errors
///
/// Returns `Err(String)` only on a local database failure. Resolver outcomes
/// (offline / unknown / ambiguous) are encoded in the response status.
#[tauri::command]
#[specta::specta]
pub async fn target_resolve(
    state: State<'_, AppState>,
    req: TargetResolveSimbadRequest,
) -> Result<TargetResolveSimbadResponse, ContractError> {
    tracing::debug!("target.resolve query={:?}", req.query);
    let pool = state.repo.pool();
    let resolver = build_simbad_resolver(&state).await?;
    app_core::target_resolve::resolve(pool, &resolver, &req).await
}

// ── target.resolve_explicit (spec 052 P2 — FR-008/FR-009) ────────────────────

/// `target.resolve_explicit` — the deliberate resolve/confirm entrypoint
/// (Enter with no typeahead match, "search more", or an Add/Confirm submit).
/// Same request/response contract as `target.resolve`; consults
/// [`targeting_resolver::simbad::SimbadResolver::resolve_explicit`]
/// (TAP-first, Sesame-fallback-on-a-miss) instead of the TAP+cache-only path
/// `target.resolve` uses — the frontend MUST NOT call this per keystroke
/// (FR-009).
///
/// # Errors
///
/// Returns `Err(String)` only on a local database failure. Resolver outcomes
/// (offline / unknown / ambiguous) are encoded in the response status.
#[tauri::command]
#[specta::specta]
pub async fn target_resolve_explicit(
    state: State<'_, AppState>,
    req: TargetResolveSimbadRequest,
) -> Result<TargetResolveSimbadResponse, ContractError> {
    tracing::debug!("target.resolve_explicit query={:?}", req.query);
    let pool = state.repo.pool();
    let resolver = build_simbad_resolver(&state).await?;
    app_core::target_resolve::resolve_explicit(pool, &resolver, &req).await
}

/// Build the live `SimbadResolver` facade from persisted `resolver_settings`
/// plus the app-lifetime shared redb cache. Shared by `target.resolve` and
/// `target.resolve_explicit` (spec 052 P1/P2) so both entrypoints construct
/// the resolver identically.
///
/// # Errors
///
/// Returns `Err(String)` on a local database failure or if the underlying
/// `reqwest`/TLS client cannot be built.
async fn build_simbad_resolver(
    state: &State<'_, AppState>,
) -> Result<targeting_resolver::simbad::SimbadResolver, ContractError> {
    use targeting_resolver::simbad::{SimbadConfig, SimbadResolver, DEFAULT_TAP_ENDPOINT};

    let pool = state.repo.pool();
    let settings =
        get_resolver_settings(pool).await.map_err(|e| ContractError::internal(e.to_string()))?;
    let (online_enabled, endpoint, timeout_secs) = settings.map_or_else(
        || (true, DEFAULT_TAP_ENDPOINT.to_owned(), 10),
        |r| (r.online_enabled != 0, r.simbad_endpoint, r.request_timeout_secs),
    );

    let config = SimbadConfig::from_settings(
        endpoint,
        u64::from(app_core::targets::resolver_settings::positive_or_default(timeout_secs, 10)),
    );
    // FIX-3 preserved: when offline, `SimbadResolver::new` never builds a
    // reqwest/TLS client (see `EitherNetworkResolver`) — cache hits still
    // resolve; a miss reports an offline-shaped unresolved outcome.
    let resolve_cache = state.resolve_cache.read().await.clone();
    SimbadResolver::new(&config, &resolve_cache, online_enabled)
        .map_err(|e| ContractError::internal(e.to_string()))
}

// ── target.search (spec 035, US1) ───────────────────────────────────────────────

/// `target.search` — as-you-type target suggestions from the shared redb
/// resolve cache (seed + anything resolved/warmed so far).
///
/// Served purely from the local cache (no network); long-tail SIMBAD
/// enrichment is a separate `target.resolve` call. Returns ranked,
/// de-duplicated [`TargetSuggestion`](contracts_core::targets::TargetSuggestion)s
/// for the project-creation / target-selection typeahead (spec 035 FR-005).
///
/// # Errors
///
/// Returns `Err(String)` on an unexpected internal (cache) failure.
#[tauri::command]
#[specta::specta]
pub async fn target_search(
    state: State<'_, AppState>,
    req: TargetSearchRequest,
) -> Result<TargetSearchResponse, ContractError> {
    tracing::debug!("target.search query={:?} limit={}", req.query, req.limit);
    let cache = state.resolve_cache.read().await.clone();
    let mut resp = app_core::target_search::search(&cache.cache(), &req).await?;
    // #818: the pure use case always answers `false`; only this wrapper has
    // the live `AppState` flag a background re-warm sets/clears.
    resp.cache_warming = state.cache_warming.load(std::sync::atomic::Ordering::Relaxed);
    Ok(resp)
}

// ── target.adopt (spec 052 P1 FR-004) ───────────────────────────────────────────

/// `target.adopt` — promote a redb-cache-only target into the durable
/// `canonical_target` table. The explicit in-use commit for UI flows with no
/// other natural commit point (e.g. the Targets-page "Add Target" dialog).
///
/// # Errors
///
/// Returns `Err(String)` on an invalid `target_id` or a local backend failure.
#[tauri::command]
#[specta::specta]
pub async fn target_adopt(
    state: State<'_, AppState>,
    req: TargetAdoptRequest,
) -> Result<TargetAdoptResponse, ContractError> {
    tracing::debug!("target.adopt target_id={}", req.target_id);
    let cache = state.resolve_cache.read().await.clone();
    app_core::target_resolve::adopt(state.repo.pool(), &cache.cache(), &req).await
}

// ── target.cache.clear (spec 052 P1 FR-002) ─────────────────────────────────────

/// `target.cache.clear` — wipe the redb resolve cache and schedule its
/// re-warm (bundled seed + existing durable `canonical_target` rows) as a
/// background task, returning as soon as the swap is done. Never touches
/// `canonical_target` itself (§V — the redb cache is a reproducible
/// projection, never canonical).
///
/// Best-effort on the underlying file swap: a transient failure to remove the
/// old redb file (e.g. a concurrent read still has it open) is reported as an
/// internal error rather than silently leaving a stale cache in place.
///
/// Fix for #695: this used to await the full re-warm (bundled seed +
/// durable rows — up to ~14k individually fsync'd redb writes) inline,
/// freezing the caller for minutes on a debug build. `rewarmed_count` is
/// therefore always `0` now — "re-warm scheduled in the background, count
/// not known synchronously" rather than "0 entries re-warmed" — kept as a
/// response-meaning change instead of widening the contract.
///
/// # Errors
///
/// Returns `Err(String)` if the cache file cannot be removed/reopened.
#[tauri::command]
#[specta::specta]
pub async fn target_cache_clear(
    state: State<'_, AppState>,
) -> Result<TargetCacheClearResponse, ContractError> {
    tracing::info!("target.cache.clear");
    crate::resolve_cache::clear_and_rewarm(&state).await?;
    Ok(TargetCacheClearResponse { rewarmed_count: 0 })
}

// ── target.resolution.settings (spec 035, US5 — FR-015) ─────────────────────────

/// `target.resolution.settings` — read the SIMBAD resolver settings.
///
/// # Errors
///
/// Returns `Err(String)` on a local database failure.
#[tauri::command]
#[specta::specta]
pub async fn target_resolution_settings(
    state: State<'_, AppState>,
    req: ResolverSettingsGetRequest,
) -> Result<ResolverSettingsResponse, ContractError> {
    tracing::debug!("target.resolution.settings (get)");
    app_core::resolver_settings::get(state.repo.pool(), &req).await
}

/// `target.resolution.settings.update` — persist new resolver settings.
///
/// # Errors
///
/// Returns `Err(String)` on a local database failure.
#[tauri::command]
#[specta::specta]
pub async fn target_resolution_settings_update(
    state: State<'_, AppState>,
    req: ResolverSettingsUpdateRequest,
) -> Result<ResolverSettingsResponse, ContractError> {
    tracing::debug!(
        "target.resolution.settings.update online_enabled={}",
        req.settings.online_enabled
    );
    app_core::resolver_settings::update(state.repo.pool(), &req).await
}

// ── target.astro_format.batch (adopt target-match) ───────────────────────────

/// `target.astro_format.batch` — sexagesimal RA/Dec formatting for N targets
/// in one call (never per-row round trips). Pure geometry (`targeting::astro_format`,
/// backed by `skymath::Equatorial`'s carry-safe sexagesimal formatting) —
/// no database access, so this never fails on a well-formed request.
///
/// Targets whose RA/Dec is non-finite are omitted from `formatted` (never a
/// fabricated string); callers key results by `id`.
///
/// # Errors
///
/// This command does not fail; the `Result` shape matches the rest of the
/// command surface for a consistent IPC error contract.
#[tauri::command]
#[specta::specta]
pub async fn target_astro_format_batch(
    req: TargetAstroFormatBatchRequest,
) -> Result<TargetAstroFormatBatchResponse, ContractError> {
    tracing::debug!("target.astro_format.batch count={}", req.targets.len());
    let formatted =
        req.targets
            .into_iter()
            .filter_map(|t| {
                targeting::astro_format::sexagesimal(t.ra_deg, t.dec_deg).map(|s| {
                    TargetAstroFormat { id: t.id, ra_sexagesimal: s.ra, dec_sexagesimal: s.dec }
                })
            })
            .collect();
    Ok(TargetAstroFormatBatchResponse { formatted })
}

// ── target.cone_search.suggest / .confirm (spec 052 P3, US3) ────────────────

/// `target.cone_search.suggest` — cone-search a light-frameset's derived
/// pointing (WCS → mount → none, FR-012) and return ranked,
/// confidence-carrying target suggestions. Advisory only — creates nothing;
/// requires online resolution (offline reports `resolve.offline`, FR-018).
///
/// # Errors
///
/// `frameset.not_found` for an unknown `frameset_id`; `resolve.offline` when
/// online resolution is disabled or the cone-search fails (non-blocking,
/// FR-018); `internal.database` on a local query failure.
#[tauri::command]
#[specta::specta]
pub async fn target_cone_search_suggest(
    state: State<'_, AppState>,
    req: ConeSearchSuggestRequest,
) -> Result<ConeSearchSuggestResponse, ContractError> {
    tracing::debug!("target.cone_search.suggest frameset_id={}", req.frameset_id);
    let pool = state.repo.pool();
    let resolver = build_simbad_resolver(&state).await?;
    app_core::inbox::cone_search::suggest(pool, &resolver, &req.frameset_id, req.reason).await
}

/// `target.cone_search.confirm` — the single point at which a cone-search
/// suggestion becomes durable (FR-016, SC-006): adopts the candidate via the
/// existing in-use promotion path (spec 052 P1) and links it to the
/// frameset.
///
/// # Errors
///
/// `frameset.not_found` for an unknown `frameset_id`; `candidate.invalid`
/// when the candidate no longer resolves; `internal.database` on a local
/// query failure.
#[tauri::command]
#[specta::specta]
pub async fn target_cone_search_confirm(
    state: State<'_, AppState>,
    req: ConeSearchConfirmRequest,
) -> Result<ConeSearchConfirmResponse, ContractError> {
    tracing::debug!(
        "target.cone_search.confirm frameset_id={} candidate={}",
        req.frameset_id,
        req.candidate.primary_designation
    );
    let cache = state.resolve_cache.read().await.clone();
    app_core::inbox::cone_search::confirm(state.repo.pool(), &cache.cache(), &req).await
}

// ── target.moon_opposition.batch (#634) ──────────────────────────────────────
//
// Replaces the per-row TS ephemeris math (`astro/lunar-separation.ts`,
// `astro/opposition.ts`) with a single batched Rust call, same geocentric
// simplification and ±2°/±7-day tolerance those modules documented (no
// observer coordinates; catalogued RA/Dec + a shared instant only —
// Track A). `skymath::sun_position`/`moon_position` (Meeus low-accuracy
// solar + truncated ELP-2000/82 lunar theory) replace the hand-rolled
// `astronomy-engine` calls; `skymath::separation`/`circular_distance` replace
// the hand-rolled vector/RA-diff helpers.

/// One target's catalogued J2000 coordinates for a moon-separation/opposition
/// batch request.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoonOppositionTargetInput {
    /// Caller-defined id, echoed back on the matching result (never re-derived).
    pub id: String,
    pub ra_deg: f64,
    pub dec_deg: f64,
}

/// `target.moon_opposition.batch` request.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetMoonOppositionBatchRequest {
    pub targets: Vec<MoonOppositionTargetInput>,
    /// RFC3339 instant shared by every target in the batch: the Moon's
    /// geocentric position, and the opposition search's start day, are the
    /// same for every row on one observing night (same memoization rationale
    /// as the TS `sunRaTable` this replaces) — pass the SAME instant for a
    /// whole table render, not a per-row `now()`.
    pub at: String,
}

/// Opposition search result for one target.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OppositionResult {
    /// RFC3339 date (whole-day resolution) of the next opposition-like
    /// midnight culmination.
    pub date: String,
    pub days_until: u32,
}

/// Moon-separation + opposition result for one target.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoonOppositionResult {
    pub id: String,
    /// Angular separation from the Moon in degrees (0..=180), or `None` for
    /// out-of-domain RA/Dec (never a fabricated value).
    pub moon_separation_deg: Option<f64>,
    /// `None` for out-of-domain RA/Dec.
    pub opposition: Option<OppositionResult>,
}

/// `target.moon_opposition.batch` response.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetMoonOppositionBatchResponse {
    pub results: Vec<MoonOppositionResult>,
}

/// Search window: matches the TS `nextOpposition` scan (a touch over one
/// synodic year of solar RA drift).
const OPPOSITION_SCAN_DAYS: u32 = 366;

/// Build a J2000 `Equatorial` from decimal-degree RA/Dec, wrapping RA into
/// `[0, 360)` and clamping Dec into `[-90, 90]` (same domain-safety treatment
/// as `targeting::coords::to_equatorial`). Returns `None` for non-finite input.
fn target_equatorial(ra_deg: f64, dec_deg: f64) -> Option<targeting::Equatorial> {
    if !ra_deg.is_finite() || !dec_deg.is_finite() {
        return None;
    }
    let ra = targeting::Angle::from_degrees(ra_deg).normalized_0_360();
    let dec = targeting::Angle::from_degrees(dec_deg.clamp(-90.0, 90.0));
    targeting::Equatorial::j2000(ra, dec).ok()
}

/// The Sun's geocentric RA (degrees) for each day offset `0..=OPPOSITION_SCAN_DAYS`
/// from `at` — computed once per batch (see [`TargetMoonOppositionBatchRequest::at`]),
/// never per target.
fn sun_ra_table(at: time::OffsetDateTime) -> Vec<f64> {
    (0..=OPPOSITION_SCAN_DAYS)
        .map(|day| skymath::sun_position(at + time::Duration::days(i64::from(day))).ra().degrees())
        .collect()
}

/// Next opposition-like (midnight-culmination) date for a target at `ra_deg`,
/// searching forward from `at` using the memoized `table` (mirrors the TS
/// `nextOpposition` coarse daily scan, FR-014/SC-003).
fn next_opposition(ra_deg: f64, at: time::OffsetDateTime, table: &[f64]) -> OppositionResult {
    let target_opposition_ra = targeting::Angle::from_degrees(ra_deg - 180.0).normalized_0_360();
    let (best_day, _) = table
        .iter()
        .enumerate()
        .map(|(day, &sun_ra)| {
            let diff = skymath::circular_distance(
                targeting::Angle::from_degrees(sun_ra),
                target_opposition_ra,
            )
            .degrees()
            .abs();
            (u32::try_from(day).unwrap_or(u32::MAX), diff)
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .unwrap_or((0, 0.0));

    let date = at + time::Duration::days(i64::from(best_day));
    OppositionResult {
        date: date.format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
        days_until: best_day,
    }
}

/// `target.moon_opposition.batch` — batched Moon-separation + next-opposition
/// computation for N targets in one call (never per-row round trips, #634).
/// Pure geometry/ephemeris — no database access, so this never fails on a
/// well-formed request; a malformed `at` is the only error case.
///
/// # Errors
///
/// Returns `Err(ContractError)` with code `"value.invalid"` when `at` is not
/// a valid RFC3339 instant.
#[tauri::command]
#[specta::specta]
pub async fn target_moon_opposition_batch(
    req: TargetMoonOppositionBatchRequest,
) -> Result<TargetMoonOppositionBatchResponse, ContractError> {
    tracing::debug!("target.moon_opposition.batch count={}", req.targets.len());
    let at = time::OffsetDateTime::parse(&req.at, &time::format_description::well_known::Rfc3339)
        .map_err(|e| {
        ContractError::new(
            contracts_core::error_code::ErrorCode::ValueInvalid,
            format!("at: invalid RFC3339 instant: {e}"),
            contracts_core::ErrorSeverity::Warning,
            false,
        )
    })?;

    // Computed once per batch, not per target (moon position + the Sun's
    // daily RA table are the same for every row on one observing night).
    let moon = skymath::moon_position(at);
    let table = sun_ra_table(at);

    let results = req
        .targets
        .into_iter()
        .map(|t| {
            let eq = target_equatorial(t.ra_deg, t.dec_deg);
            MoonOppositionResult {
                id: t.id,
                moon_separation_deg: eq.map(|e| skymath::separation(moon, e).degrees()),
                opposition: eq.map(|_| next_opposition(t.ra_deg, at, &table)),
            }
        })
        .collect();

    Ok(TargetMoonOppositionBatchResponse { results })
}

#[cfg(test)]
mod moon_opposition_tests {
    use super::{
        target_moon_opposition_batch, MoonOppositionTargetInput, TargetMoonOppositionBatchRequest,
    };

    /// #634 SUCCESS: one batched call returns both moon-separation and
    /// opposition for N targets, keyed by the caller's `id`.
    #[tokio::test]
    async fn batched_moon_opposition_for_multiple_targets() {
        let req = TargetMoonOppositionBatchRequest {
            targets: vec![
                MoonOppositionTargetInput { id: "m31".to_owned(), ra_deg: 10.68, dec_deg: 41.27 },
                MoonOppositionTargetInput { id: "m42".to_owned(), ra_deg: 83.82, dec_deg: -5.39 },
            ],
            at: "2026-01-15T00:00:00Z".to_owned(),
        };
        let resp = target_moon_opposition_batch(req).await.unwrap();
        assert_eq!(resp.results.len(), 2);
        for r in &resp.results {
            let sep = r.moon_separation_deg.expect("finite RA/Dec must produce a separation");
            assert!((0.0..=180.0).contains(&sep), "separation out of range: {sep}");
            let opp = r.opposition.as_ref().expect("finite RA/Dec must produce an opposition");
            assert!(opp.days_until <= 366);
            assert!(!opp.date.is_empty());
        }
        assert_eq!(resp.results[0].id, "m31");
        assert_eq!(resp.results[1].id, "m42");
    }

    /// Non-finite RA/Dec returns `None` rather than a fabricated value.
    #[tokio::test]
    async fn non_finite_coordinates_return_none() {
        let req = TargetMoonOppositionBatchRequest {
            targets: vec![MoonOppositionTargetInput {
                id: "bad".to_owned(),
                ra_deg: f64::NAN,
                dec_deg: 0.0,
            }],
            at: "2026-01-15T00:00:00Z".to_owned(),
        };
        let resp = target_moon_opposition_batch(req).await.unwrap();
        assert_eq!(resp.results.len(), 1);
        assert!(resp.results[0].moon_separation_deg.is_none());
        assert!(resp.results[0].opposition.is_none());
    }

    /// A malformed `at` is rejected, not silently defaulted.
    #[tokio::test]
    async fn invalid_at_is_rejected() {
        let req = TargetMoonOppositionBatchRequest { targets: vec![], at: "not-a-date".to_owned() };
        let err = target_moon_opposition_batch(req).await.unwrap_err();
        assert_eq!(err.code, contracts_core::error_code::ErrorCode::ValueInvalid);
    }
}
