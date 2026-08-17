// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `target.cone_search.suggest` / `target.cone_search.confirm` use case
//! (spec 052 P3, US3; see
//! `specs/052-simbad-caching-dual-lookup-cone-search/{spec,data-model,contracts/operations}.md`).
//!
//! Suggests a target for a light-frameset (a single-type light inbox item —
//! the same "sub-group" unit `target_recommendations` (spec 041 R-17) already
//! resolves a pointing for) from its derived coordinates, ranked and
//! confidence-scored per OQ-1 (catalogue prominence)/OQ-2 (default otype
//! exclusion). Advisory only: [`suggest`] writes nothing; [`confirm`] is the
//! sole path that creates the durable link, delegating to
//! `app_core_targets::target_resolve::promote_by_id` (spec 052 P1's existing
//! in-use promotion — no duplicate write path, constitution §V).
#![allow(clippy::doc_markdown)] // RA/Dec/FOV/WCS are domain terms

use std::collections::HashMap;

use sqlx::SqlitePool;

use contracts_core::cone_search::{
    ConeSearchCandidateTarget, ConeSearchConfidence, ConeSearchConfirmRequest,
    ConeSearchConfirmResponse, ConeSearchPointing, ConeSearchReason, ConeSearchSuggestResponse,
    ConeSearchSuggestion, PointingSource,
};
use contracts_core::error_code::ErrorCode;
use contracts_core::targets::TargetObjectType;
use contracts_core::{ContractError, ErrorSeverity};
use persistence_inbox::repositories::inbox::{self as repo, InboxPointingRow};
use target_match::skymath as tm_skymath;
use target_match::{rank, Constraint, SkyObject};
use targeting::coords;
use targeting_resolver::cone_search::{
    dedup_candidates, is_default_excluded, prominence_tier, select_for_display, ConeCandidate,
};
use targeting_resolver::simbad::SimbadResolver;

/// Default cone-search radius (degrees) when optics are unknown (FR-013).
pub const DEFAULT_RADIUS_DEG: f64 = 1.0;

/// Candidates fetched from SIMBAD before local dedup/ranking — wider than the
/// response top-N so ranking/exclusion have real choices to work with.
///
/// Deliberately modest, not "wide enough to never lose the target" (issue
/// #698): live SIMBAD data around a dense field like M 51 returns 1000+
/// catalogued objects within 0.02 deg, so no fetch width short of thousands
/// could *guarantee* a niche-crowded target survives this stage — and each
/// unit of `FETCH_LIMIT` costs one extra `enrich_position_match` TAP
/// round-trip (`suggest`'s per-candidate alias/common-name lookup), so
/// raising it much further trades interactive latency for a guarantee it
/// still can't deliver. The real fix for #698 is
/// `targeting_resolver::cone_search::select_for_display`, which stops
/// default-excluded/niche-tier clutter from crowding a prominent survivor out
/// of the *response* window once fetched; this bump (20 → 40) only shrinks
/// the residual "fell off the raw fetch entirely" window documented there.
/// Excluding niche/excluded otypes at the TAP query itself (the more
/// complete fix) would need a query-shape change in the external
/// `simbad-resolver` crate — out of this crate's scope, left as a follow-up.
const FETCH_LIMIT: usize = 40;

/// How many ranked suggestions [`suggest`] actually returns.
const RESPONSE_LIMIT: usize = 8;

/// Pointing tolerance (degrees) for the "subs disagree" edge case (FR-012) —
/// generous enough for normal dithering/mount jitter within one session,
/// tight enough to catch a genuinely mis-grouped frameset. Documented default,
/// like `target_recommendations::DEFAULT_FIXED_RADIUS_DEG`.
pub const POINTING_TOLERANCE_DEG: f64 = 1.0;

fn not_found(msg: String) -> ContractError {
    ContractError::new(ErrorCode::FramesetNotFound, msg, ErrorSeverity::Blocking, false)
}

/// Cone-search's own non-blocking degraded state (FR-018): offline/disabled
/// or any TAP failure — the caller (Inbox ingest) proceeds without a
/// suggestion rather than treating this as a hard error.
fn offline(msg: impl Into<String>) -> ContractError {
    ContractError::new(ErrorCode::ResolveOffline, msg.into(), ErrorSeverity::Info, true)
}

// Import canonical mapper so `.map_err(db_err)` calls correctly route
// NotFound to Blocking instead of Fatal (bd astro-plan-kyo7.88).
use app_core_errors::db_err;

/// Map a `targeting_resolver::cache::CacheError` to a `ContractError`.
///
/// Delegates the `Persistence(DbError)` variant through the canonical
/// `db_err` so `NotFound` stays `Blocking`; wraps every other variant as
/// `Fatal`/`retryable=true` (infrastructure errors, not domain 404s).
#[allow(clippy::needless_pass_by_value)]
fn cache_err(e: targeting_resolver::cache::CacheError) -> ContractError {
    use targeting_resolver::cache::CacheError;
    match e {
        CacheError::Persistence(db) => db_err(db),
        other => ContractError::new(
            ErrorCode::InternalDatabase,
            format!("{other}"),
            ErrorSeverity::Fatal,
            true,
        ),
    }
}

/// Map a `simbad_resolver::CacheError` to a `ContractError`.
///
/// All variants are non-domain infrastructure failures; map as Fatal/retryable.
#[allow(clippy::needless_pass_by_value)]
fn simbad_cache_err(e: simbad_resolver::CacheError) -> ContractError {
    ContractError::new(ErrorCode::InternalDatabase, format!("{e}"), ErrorSeverity::Fatal, true)
}

// ── Pointing derivation (FR-012) ─────────────────────────────────────────────

struct DerivedPointing {
    ra_deg: f64,
    dec_deg: f64,
    source: PointingSource,
    /// Rotation for the rotated frame footprint: WCS rotation (this tier)
    /// preferred, else the existing `sky_rotation_deg`/`rotator_angle_deg`
    /// fallback chain (same precedence `target_recommendations` uses).
    rotation_deg: Option<f64>,
    focal_length_mm: Option<f64>,
    pixel_size_um: Option<f64>,
    naxis1: Option<i64>,
    naxis2: Option<i64>,
}

/// Derive the frameset's pointing: WCS `CRVAL1/2` (high confidence) → mount
/// `RA`/`DEC`/`OBJCTRA`/`OBJCTDEC` (medium) → none (FR-012). Never the
/// filename. A tier whose rows disagree beyond [`POINTING_TOLERANCE_DEG`]
/// resolves to no reliable pointing for that tier (falls through to the next
/// tier, or to `none` if mount also disagrees).
fn derive_pointing(rows: &[InboxPointingRow]) -> Option<DerivedPointing> {
    pick_tier(rows, |r| r.wcs_ra_deg.zip(r.wcs_dec_deg), PointingSource::Wcs)
        .or_else(|| pick_tier(rows, |r| r.ra_deg.zip(r.dec_deg), PointingSource::Mount))
}

fn pick_tier(
    rows: &[InboxPointingRow],
    coords_of: impl Fn(&InboxPointingRow) -> Option<(f64, f64)>,
    source: PointingSource,
) -> Option<DerivedPointing> {
    let points: Vec<(&InboxPointingRow, f64, f64)> = rows
        .iter()
        .filter_map(|r| coords_of(r).map(|(ra, dec)| (r, ra, dec)))
        .filter(|(_, ra, dec)| ra.is_finite() && dec.is_finite())
        .collect();
    let (first, ra0, dec0) = *points.first()?;

    for (_, ra, dec) in &points[1..] {
        let sep = coords::angular_separation_deg(
            coords::Pointing::new(ra0, dec0),
            coords::Pointing::new(*ra, *dec),
        );
        let within_tolerance = matches!(
            sep.partial_cmp(&POINTING_TOLERANCE_DEG),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        );
        if !within_tolerance {
            return None; // subs disagree beyond tolerance (FR-012 edge case), or NaN
        }
    }

    let rotation_deg =
        if matches!(source, PointingSource::Wcs) { first.wcs_rotation_deg } else { None }
            .or(first.sky_rotation_deg)
            .or(first.rotator_angle_deg);

    Some(DerivedPointing {
        ra_deg: ra0,
        dec_deg: dec0,
        source,
        rotation_deg,
        focal_length_mm: first.focal_length_mm,
        pixel_size_um: first.pixel_size_um,
        naxis1: first.naxis1,
        naxis2: first.naxis2,
    })
}

// ── Candidate assembly ───────────────────────────────────────────────────────

/// Wraps a [`ConeCandidate`]'s position for `target_match::rank`'s rotated
/// footprint test; correlates back via `index` into the candidate vec.
///
/// Stores `target_match::skymath::Equatorial` (0.6.x) because `target_match`
/// 0.5.x re-exports that version and `SkyObject::position` must return it.
/// The workspace-wide `skymath` 0.7.x lives alongside as a transitive dep;
/// we bridge by constructing the 0.6.x type directly from raw degrees.
struct ConeSkyObject {
    index: usize,
    position: tm_skymath::Equatorial,
}

impl SkyObject for ConeSkyObject {
    fn position(&self) -> tm_skymath::Equatorial {
        self.position
    }
}

fn map_object_type(o: simbad_resolver::ObjectType) -> TargetObjectType {
    match o {
        simbad_resolver::ObjectType::Galaxy => TargetObjectType::Galaxy,
        simbad_resolver::ObjectType::PlanetaryNebula => TargetObjectType::PlanetaryNebula,
        simbad_resolver::ObjectType::EmissionNebula => TargetObjectType::EmissionNebula,
        simbad_resolver::ObjectType::ReflectionNebula => TargetObjectType::ReflectionNebula,
        simbad_resolver::ObjectType::DarkNebula => TargetObjectType::DarkNebula,
        simbad_resolver::ObjectType::OpenCluster => TargetObjectType::OpenCluster,
        simbad_resolver::ObjectType::GlobularCluster => TargetObjectType::GlobularCluster,
        simbad_resolver::ObjectType::SupernovaRemnant => TargetObjectType::SupernovaRemnant,
        simbad_resolver::ObjectType::GalaxyCluster => TargetObjectType::GalaxyCluster,
        simbad_resolver::ObjectType::DoubleStar => TargetObjectType::DoubleStar,
        simbad_resolver::ObjectType::Asterism => TargetObjectType::Asterism,
        simbad_resolver::ObjectType::Other => TargetObjectType::Other,
    }
}

/// Live IAU constellation for display only (never persisted here — adoption
/// re-derives and persists it via P1's existing enrichment at confirm time).
/// `None` on an out-of-range coordinate rather than fabricated.
fn live_constellation(ra_deg: f64, dec_deg: f64) -> Option<String> {
    if !ra_deg.is_finite() || !dec_deg.is_finite() {
        return None;
    }
    let eq = skymath::Equatorial::j2000_lenient(ra_deg, dec_deg).ok()?;
    Some(skymath::constellation(eq).abbreviation().to_owned())
}

/// Confidence assignment (FR-014): coordinate-source quality gates the
/// ceiling — only the nearest non-excluded in-field candidate under a
/// plate-solved (WCS) centre reaches `High` (⇒ `preselected`); mount pointing
/// has enough slack (unsolved) to never silently reach `High` (constitution
/// II — never silently auto-apply).
fn confidence_for(
    source: PointingSource,
    is_primary: bool,
    excluded: bool,
) -> ConeSearchConfidence {
    if excluded {
        return ConeSearchConfidence::Low;
    }
    match (source, is_primary) {
        (PointingSource::Wcs, true) => ConeSearchConfidence::High,
        (PointingSource::Wcs, false) | (PointingSource::Mount, true) => {
            ConeSearchConfidence::Medium
        }
        // `PointingSource::None` is unreachable here: `suggest` returns no
        // candidates at all when the pointing source is `None`.
        (PointingSource::Mount, false) | (PointingSource::None, _) => ConeSearchConfidence::Low,
    }
}

// ── suggest ───────────────────────────────────────────────────────────────────

/// `target.cone_search.suggest` (FR-012–FR-015, FR-017). Read-only — never
/// writes `canonical_target`.
///
/// # Errors
///
/// - [`ErrorCode::FramesetNotFound`] — unknown `frameset_id`.
/// - [`ErrorCode::ResolveOffline`] — online resolution disabled or the TAP
///   cone-search failed; non-blocking (FR-018), the caller proceeds without
///   a suggestion.
/// - [`ErrorCode::InternalDatabase`] — a local query failed.
pub async fn suggest(
    pool: &SqlitePool,
    resolver: &SimbadResolver,
    frameset_id: &str,
    reason: ConeSearchReason,
) -> Result<ConeSearchSuggestResponse, ContractError> {
    // #702: annotate every suggest run with its trigger (FR-017) so ingest
    // vs. on-demand ("Re-check") runs are distinguishable in logs — the
    // field carried no other observable effect before this (dead contract
    // field).
    tracing::debug!(frameset_id, reason = ?reason, "cone_search.suggest");

    repo::get_inbox_item(pool, frameset_id)
        .await
        .map_err(|_| not_found(format!("frameset not found: {frameset_id}")))?;

    let rows = repo::list_inbox_pointing(pool, frameset_id).await.map_err(db_err)?;
    let Some(pointing) = derive_pointing(&rows) else {
        return Ok(no_pointing_response());
    };

    let field = coords::field_from_optics(
        pointing.focal_length_mm,
        pointing.pixel_size_um,
        pointing.naxis1,
        pointing.naxis2,
    );
    let optics_known = field.is_some();
    let radius_deg = field.map_or(DEFAULT_RADIUS_DEG, |f| {
        f.radius(target_match::RadiusPolicy::Circumscribed).degrees()
    });

    let raw = resolver
        .resolve_position(pointing.ra_deg, pointing.dec_deg, radius_deg, FETCH_LIMIT)
        .await
        .map_err(|e| offline(format!("cone-search unavailable: {e}")))?;

    let mut enriched = Vec::with_capacity(raw.len());
    for m in raw {
        let separation_deg = m.separation_deg;
        let identity = resolver.enrich_position_match(m).await;
        enriched.push(ConeCandidate { identity, separation_deg });
    }
    let candidates = dedup_candidates(enriched);
    let in_field = in_field_indices(&candidates, field, &pointing);
    let primary = primary_index(&candidates, &in_field);

    // #698: pick which candidates make the response window BEFORE assembling
    // them — `select_for_display` keeps default-excluded/niche-tier clutter
    // from crowding a prominent survivor out of the fixed-size response, and
    // guarantees `primary` isn't silently dropped. Also avoids wasted
    // cache-warm/DB lookups (`assemble_suggestion`) for candidates that would
    // only be truncated away afterwards.
    let display = select_for_display(&candidates, &in_field, primary, RESPONSE_LIMIT);

    let mut suggestions = Vec::with_capacity(display.len());
    for i in display {
        suggestions.push(
            assemble_suggestion(
                pool,
                resolver,
                &candidates[i],
                pointing.source,
                primary == Some(i),
            )
            .await,
        );
    }
    // Display order stays nearest-first (FR-013) — `select_for_display` only
    // decided which candidates survive, not their order.
    suggestions.sort_by(|a, b| {
        a.separation_deg.partial_cmp(&b.separation_deg).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(ConeSearchSuggestResponse {
        pointing: ConeSearchPointing {
            source: pointing.source,
            center_ra_deg: Some(pointing.ra_deg),
            center_dec_deg: Some(pointing.dec_deg),
            radius_deg,
            optics_known,
        },
        suggestions,
    })
}

/// Rotation-aware in-field membership (FR-013): a circular TAP fetch is a
/// conservative superset; refine to the true (optionally rotated) rectangle
/// when optics are known. Unknown optics ⇒ the circular radius itself is the
/// footprint, every dedup'd candidate already qualifies.
fn in_field_indices(
    candidates: &[ConeCandidate],
    field: Option<target_match::Field>,
    pointing: &DerivedPointing,
) -> Vec<usize> {
    let Some(field) = field else {
        return (0..candidates.len()).collect();
    };
    let center = tm_skymath::Equatorial::j2000_lenient(pointing.ra_deg, pointing.dec_deg)
        .expect("derive_pointing only returns finite coords");
    let constraint = pointing.rotation_deg.map_or_else(
        || Constraint::frame(&field),
        |pa| Constraint::frame_rotated(&field, tm_skymath::Angle::from_degrees(pa)),
    );
    let objects: Vec<ConeSkyObject> = candidates
        .iter()
        .enumerate()
        .map(|(index, c)| ConeSkyObject {
            index,
            position: tm_skymath::Equatorial::j2000_lenient(c.identity.ra_deg, c.identity.dec_deg)
                .expect("SIMBAD-resolved candidates have finite coordinates"),
        })
        .collect();
    rank(center, &objects, constraint).into_iter().map(|m| m.object.index).collect()
}

/// Primary object (FR-015): nearest-to-centre among non-excluded in-field
/// candidates, tie-broken by catalogue prominence (OQ-1).
fn primary_index(candidates: &[ConeCandidate], in_field: &[usize]) -> Option<usize> {
    in_field.iter().copied().filter(|&i| !is_default_excluded(&candidates[i].identity)).min_by(
        |&a, &b| {
            candidates[a]
                .separation_deg
                .partial_cmp(&candidates[b].separation_deg)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    prominence_tier(&candidates[b].identity)
                        .cmp(&prominence_tier(&candidates[a].identity))
                })
        },
    )
}

/// Build one [`ConeSearchSuggestion`], looking up whether the candidate is
/// already an adopted `canonical_target` (informational only — `suggest`
/// itself never writes it) and warming the shared resolve cache with the
/// candidate's identity.
///
/// The cache warm is necessary, not cosmetic: `resolve_position`/
/// `enrich_position_match` bypass the facade's own cache-first `resolve`
/// entirely, so a cone-search candidate the user has never separately
/// searched/typed stays cache-cold; `target.cone_search.confirm`'s
/// `promote_by_id` requires the id to already be cache- or SQLite-resident,
/// so without this, confirming a genuinely fresh suggestion would always
/// fail (see `targeting_resolver::simbad::SimbadResolver::warm_cache`).
async fn assemble_suggestion(
    pool: &SqlitePool,
    resolver: &SimbadResolver,
    candidate: &ConeCandidate,
    source: PointingSource,
    is_primary: bool,
) -> ConeSearchSuggestion {
    resolver.warm_cache(&candidate.identity).await;
    let excluded = is_default_excluded(&candidate.identity);
    let confidence = confidence_for(source, is_primary, excluded);
    let canonical_target_id = match candidate.identity.simbad_oid {
        Some(oid) => targeting_resolver::cache::get_by_simbad_oid(pool, oid)
            .await
            .ok()
            .flatten()
            .map(|t| t.id.to_string()),
        None => None,
    };
    ConeSearchSuggestion {
        candidate: ConeSearchCandidateTarget {
            canonical_target_id,
            primary_designation: candidate.identity.primary_designation.clone(),
            common_name: candidate.identity.common_name.clone(),
            object_type: map_object_type(candidate.identity.object_type),
            ra_deg: candidate.identity.ra_deg,
            dec_deg: candidate.identity.dec_deg,
            magnitude: candidate.identity.v_mag,
            constellation: live_constellation(
                candidate.identity.ra_deg,
                candidate.identity.dec_deg,
            ),
        },
        separation_deg: candidate.separation_deg,
        confidence,
        preselected: confidence == ConeSearchConfidence::High,
        excluded,
    }
}

fn no_pointing_response() -> ConeSearchSuggestResponse {
    ConeSearchSuggestResponse {
        pointing: ConeSearchPointing {
            source: PointingSource::None,
            center_ra_deg: None,
            center_dec_deg: None,
            radius_deg: DEFAULT_RADIUS_DEG,
            optics_known: false,
        },
        suggestions: Vec::new(),
    }
}

// ── confirm ───────────────────────────────────────────────────────────────────

/// `target.cone_search.confirm` (FR-016, SC-006) — the single point at which
/// a cone-search suggestion becomes durable. Adopts the candidate (dedup on
/// `simbad_oid` → normalized designation, FR-007) via the existing in-use
/// promotion path (P1's `promote_by_id`, constitution §V — no duplicate
/// write path), then links it to the frameset by setting the `target`
/// property override (the same field-agnostic mechanism `inbox.reclassify`
/// T068 already uses to bind a light group's effective `OBJECT` value) to
/// the confirmed designation for every file in the frameset.
///
/// # Errors
///
/// - [`ErrorCode::FramesetNotFound`] — unknown `frameset_id`.
/// - [`ErrorCode::CandidateInvalid`] — neither an existing `canonicalTargetId`
///   nor a resolvable `primaryDesignation` was supplied.
/// - [`ErrorCode::InternalDatabase`] — a local query failed.
pub async fn confirm(
    pool: &SqlitePool,
    redb_cache: &dyn simbad_resolver::Cache,
    req: &ConeSearchConfirmRequest,
) -> Result<ConeSearchConfirmResponse, ContractError> {
    let item = repo::get_inbox_item(pool, &req.frameset_id)
        .await
        .map_err(|_| not_found(format!("frameset not found: {}", req.frameset_id)))?;

    let id = if let Some(existing) = &req.candidate.canonical_target_id {
        uuid::Uuid::parse_str(existing).map_err(|e| {
            ContractError::new(
                ErrorCode::CandidateInvalid,
                format!("canonicalTargetId '{existing}' is not a valid UUID: {e}"),
                ErrorSeverity::Blocking,
                false,
            )
        })?
    } else {
        // OQ-1/FR-007: prefer the cache's own oid-keyed identity over a
        // designation-derived guess. The redb cache dedups by `simbad_oid`
        // first (spec 052 P1) — if this physical object was already cached
        // under a DIFFERENT alias/primary_designation (e.g. the frame's
        // cone-search hit returned "M 31" but the user had previously
        // searched "NGC 224"), the cache kept the FIRST id assigned; a bare
        // `target_id_from_designation("M 31")` guess would miss it and
        // `promote_by_id` would then treat it as unknown. Falls back to the
        // designation-derived id (existing FR-007 behaviour) when no oid is
        // given or nothing is cached under it yet.
        let by_oid = match req.candidate.simbad_oid {
            Some(oid) => redb_cache.get_by_simbad_oid(oid).await.map_err(simbad_cache_err)?,
            None => None,
        };
        by_oid.map_or_else(
            || targeting::identity::target_id_from_designation(&req.candidate.primary_designation),
            |t| t.id,
        )
    };
    let was_durable_before =
        targeting_resolver::cache::get_by_id(pool, id).await.map_err(cache_err)?.is_some();

    let promoted =
        app_core_targets::target_resolve::promote_by_id(pool, redb_cache, id, &req.frameset_id)
            .await?;
    if !promoted {
        return Err(ContractError::new(
            ErrorCode::CandidateInvalid,
            format!(
                "candidate '{}' no longer resolves — resolve it before confirming",
                req.candidate.primary_designation
            ),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // Best-effort link: write the `target` override for every file in the
    // frameset so the confirmed designation becomes the effective OBJECT for
    // downstream apply/session propagation. Never fails the confirm itself —
    // the durable canonical_target write above already succeeded.
    if let Some(source_group_id) = &item.source_group_id {
        // File identity (size/mtime) recorded by the last classify (spec 041
        // FR-046) — threaded through so the durable override has a baseline
        // to compare against for staleness, same as `reclassify_v2`.
        let file_identity: HashMap<String, (Option<i64>, Option<String>)> =
            repo::list_inbox_file_metadata(pool, &req.frameset_id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|m| (m.relative_file_path, (m.file_size_bytes, m.file_mtime)))
                .collect();

        if let Ok(rows) = repo::list_inbox_pointing(pool, &req.frameset_id).await {
            for row in rows {
                let (id_size, id_mtime) = file_identity
                    .get(row.relative_file_path.as_str())
                    .cloned()
                    .unwrap_or((None, None));
                let _ = repo::set_file_override(
                    pool,
                    source_group_id,
                    &row.relative_file_path,
                    "target",
                    &req.candidate.primary_designation,
                    id_size,
                    id_mtime.as_deref(),
                )
                .await;
            }
        }
    }

    Ok(ConeSearchConfirmResponse {
        canonical_target_id: id.to_string(),
        created: !was_durable_before,
        linked: true,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;
    use persistence_inbox::repositories::inbox::{InsertInboxItem, UpsertFileMetadata};
    use targeting_resolver::simbad::{ResolveCache, SimbadConfig};

    async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    #[allow(clippy::too_many_arguments)]
    async fn seed_item(
        db: &Database,
        item_id: &str,
        ra: Option<f64>,
        dec: Option<f64>,
        wcs_ra: Option<f64>,
        wcs_dec: Option<f64>,
        focal: Option<f64>,
        pixel: Option<f64>,
        naxis: Option<i64>,
    ) {
        let relative_path = format!("lights-{item_id}");
        let relative_file_path = format!("{relative_path}/light_001.fits");
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: &relative_path,
                file_count: 1,
                content_signature: Some("sig"),
                lane: "fits",
            },
        )
        .await
        .unwrap();
        repo::upsert_inbox_file_metadata(
            db.pool(),
            &UpsertFileMetadata {
                inbox_item_id: item_id,
                relative_file_path: &relative_file_path,
                ra_deg: ra,
                dec_deg: dec,
                wcs_ra_deg: wcs_ra,
                wcs_dec_deg: wcs_dec,
                focal_length_mm: focal,
                pixel_size_um: pixel,
                naxis1: naxis,
                naxis2: naxis,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    fn row(
        ra: Option<f64>,
        dec: Option<f64>,
        wcs_ra: Option<f64>,
        wcs_dec: Option<f64>,
    ) -> InboxPointingRow {
        InboxPointingRow {
            relative_file_path: "f.fits".to_owned(),
            ra_deg: ra,
            dec_deg: dec,
            focal_length_mm: None,
            pixel_size_um: None,
            naxis1: None,
            naxis2: None,
            rotator_angle_deg: None,
            sky_rotation_deg: None,
            object: None,
            wcs_ra_deg: wcs_ra,
            wcs_dec_deg: wcs_dec,
            wcs_rotation_deg: None,
        }
    }

    /// A minimal [`ConeCandidate`] for `confidence_for`/`primary_index`
    /// tests: `designation` doubles as a stand-in for both the primary
    /// designation and its sole alias (unused by these two pure functions,
    /// but required to build a well-formed identity).
    fn cone_candidate(
        designation: &str,
        object_type: simbad_resolver::ObjectType,
        otype_raw: &str,
        separation_deg: f64,
    ) -> ConeCandidate {
        ConeCandidate {
            identity: simbad_resolver::ResolvedIdentity {
                simbad_oid: None,
                primary_designation: designation.to_owned(),
                common_name: None,
                object_type,
                otype_raw: otype_raw.to_owned(),
                ra_deg: 0.0,
                dec_deg: 0.0,
                v_mag: None,
                aliases: vec![simbad_resolver::ResolvedAlias::new(
                    designation,
                    simbad_resolver::AliasKind::Designation,
                )],
                source: simbad_resolver::TargetSource::Resolved,
            },
            separation_deg,
        }
    }

    // ── confidence_for (FR-014, SC-005) ──────────────────────────────────────

    #[test]
    fn wcs_primary_non_excluded_is_high() {
        // High is the only tier `assemble_suggestion` marks `preselected`.
        assert_eq!(confidence_for(PointingSource::Wcs, true, false), ConeSearchConfidence::High);
    }

    #[test]
    fn mount_primary_is_medium_never_preselected() {
        // SC-005: a mount-only frameset must never reach High/preselected,
        // even for the nearest non-excluded (primary) candidate.
        assert_eq!(
            confidence_for(PointingSource::Mount, true, false),
            ConeSearchConfidence::Medium
        );
    }

    #[test]
    fn wcs_non_primary_is_medium() {
        assert_eq!(confidence_for(PointingSource::Wcs, false, false), ConeSearchConfidence::Medium);
    }

    #[test]
    fn mount_non_primary_is_low() {
        assert_eq!(confidence_for(PointingSource::Mount, false, false), ConeSearchConfidence::Low);
    }

    #[test]
    fn excluded_is_always_low_regardless_of_source_or_primacy() {
        assert_eq!(confidence_for(PointingSource::Wcs, true, true), ConeSearchConfidence::Low);
        assert_eq!(confidence_for(PointingSource::Mount, true, true), ConeSearchConfidence::Low);
    }

    // ── primary_index (FR-015) ────────────────────────────────────────────────

    #[test]
    fn nearest_non_excluded_wins() {
        let candidates = vec![
            cone_candidate("HD 1", simbad_resolver::ObjectType::Other, "*", 0.01), // excluded, nearest
            cone_candidate("M 31", simbad_resolver::ObjectType::Galaxy, "G", 0.5),
            cone_candidate("M 33", simbad_resolver::ObjectType::Galaxy, "G", 1.0),
        ];
        let in_field = vec![0, 1, 2];
        let primary = primary_index(&candidates, &in_field);
        assert_eq!(primary, Some(1), "the excluded nearest candidate must never become primary");
    }

    #[test]
    fn excluded_candidates_never_become_primary_even_if_only_candidate() {
        let candidates =
            vec![cone_candidate("HD 1", simbad_resolver::ObjectType::DoubleStar, "**", 0.01)];
        assert_eq!(primary_index(&candidates, &[0]), None);
    }

    #[test]
    fn prominence_breaks_separation_ties() {
        // Equal separation: the NGC-tier candidate must win over the niche one.
        let candidates = vec![
            cone_candidate("vdB 1", simbad_resolver::ObjectType::ReflectionNebula, "RNe", 0.3),
            cone_candidate("NGC 1", simbad_resolver::ObjectType::Galaxy, "G", 0.3),
        ];
        let primary = primary_index(&candidates, &[0, 1]);
        assert_eq!(primary, Some(1), "NGC-tier must win the separation tie over niche");
    }

    #[test]
    fn empty_in_field_has_no_primary() {
        let candidates =
            vec![cone_candidate("M 31", simbad_resolver::ObjectType::Galaxy, "G", 0.1)];
        assert_eq!(primary_index(&candidates, &[]), None);
    }

    // ── derive_pointing / pick_tier (FR-012) ─────────────────────────────────

    #[test]
    fn wcs_pointing_takes_precedence_over_mount() {
        let rows = vec![row(Some(1.0), Some(2.0), Some(10.68), Some(41.27))];
        let p = derive_pointing(&rows).unwrap();
        assert_eq!(p.source, PointingSource::Wcs);
        assert!((p.ra_deg - 10.68).abs() < 1e-9);
    }

    #[test]
    fn mount_pointing_used_when_no_wcs() {
        let rows = vec![row(Some(10.68), Some(41.27), None, None)];
        let p = derive_pointing(&rows).unwrap();
        assert_eq!(p.source, PointingSource::Mount);
    }

    #[test]
    fn no_pointing_when_neither_source_present() {
        let rows = vec![row(None, None, None, None)];
        assert!(derive_pointing(&rows).is_none());
    }

    #[test]
    fn subs_disagreeing_beyond_tolerance_falls_through_to_none() {
        // Two files whose WCS centres are far apart (>> tolerance) and whose
        // mount pointing also disagrees ⇒ no reliable pointing at all.
        let rows = vec![
            row(Some(10.0), Some(10.0), Some(10.68), Some(41.27)),
            row(Some(200.0), Some(-40.0), Some(150.0), Some(-30.0)),
        ];
        assert!(derive_pointing(&rows).is_none());
    }

    #[test]
    fn subs_within_tolerance_still_resolve() {
        let rows = vec![
            row(None, None, Some(10.684_708), Some(41.268_75)),
            row(None, None, Some(10.684_9), Some(41.269_0)),
        ];
        let p = derive_pointing(&rows).unwrap();
        assert_eq!(p.source, PointingSource::Wcs);
    }

    // ── suggest: no pointing / offline gating ────────────────────────────────

    #[tokio::test]
    async fn suggest_with_no_pointing_returns_empty_suggestions_no_network() {
        let db = test_db().await;
        seed_item(&db, "item-none", None, None, None, None, None, None, None).await;
        // Deliberately no resolver call is possible to observe here — the
        // function must short-circuit before ever touching the resolver.
        let cache = ResolveCache::in_memory().unwrap();
        let resolver = targeting_resolver::simbad::SimbadResolver::new(
            &SimbadConfig::default(),
            &cache,
            false,
        )
        .unwrap();
        let resp =
            suggest(db.pool(), &resolver, "item-none", ConeSearchReason::Ingest).await.unwrap();
        assert_eq!(resp.pointing.source, PointingSource::None);
        assert!(resp.suggestions.is_empty());
    }

    #[tokio::test]
    async fn suggest_is_offline_gated_when_pointing_present() {
        let db = test_db().await;
        seed_item(&db, "item-wcs", None, None, Some(10.684_708), Some(41.268_75), None, None, None)
            .await;
        let cache = ResolveCache::in_memory().unwrap();
        let resolver = targeting_resolver::simbad::SimbadResolver::new(
            &SimbadConfig::default(),
            &cache,
            false,
        )
        .unwrap();
        let err =
            suggest(db.pool(), &resolver, "item-wcs", ConeSearchReason::Ingest).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ResolveOffline);
    }

    #[tokio::test]
    async fn suggest_unknown_frameset_is_not_found() {
        let db = test_db().await;
        let cache = ResolveCache::in_memory().unwrap();
        let resolver = targeting_resolver::simbad::SimbadResolver::new(
            &SimbadConfig::default(),
            &cache,
            false,
        )
        .unwrap();
        let err =
            suggest(db.pool(), &resolver, "nope", ConeSearchReason::Ingest).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::FramesetNotFound);
    }

    // ── confirm (FR-016, SC-006) ──────────────────────────────────────────────

    fn m31_identity() -> simbad_resolver::ResolvedIdentity {
        simbad_resolver::ResolvedIdentity {
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
        }
    }

    async fn seeded_cache_with_m31() -> simbad_resolver::RedbCache {
        let store = simbad_resolver::Store::in_memory().unwrap();
        let cache = store.cache();
        let ns = simbad_resolver::identity::namespace("astro-plan.targets");
        simbad_resolver::Cache::upsert(&cache, &m31_identity(), &ns).await.unwrap();
        cache
    }

    /// Reproduces the real production path: `suggest()` cache-warms every
    /// candidate it returns (`assemble_suggestion` → `resolver.warm_cache`),
    /// so a candidate the user has NEVER separately searched/typed (unlike
    /// `seeded_cache_with_m31`'s manual pre-seed) is still confirmable
    /// afterwards — `promote_by_id` requires the id to be cache-resident.
    #[tokio::test]
    async fn confirm_succeeds_for_a_never_seeded_candidate_after_suggest_warms_the_cache() {
        let db = test_db().await;
        seed_item(&db, "item-fresh", None, None, None, None, None, None, None).await;
        let resolve_cache = ResolveCache::in_memory().unwrap();
        let resolver =
            SimbadResolver::new(&SimbadConfig::default(), &resolve_cache, false).unwrap();

        let candidate = ConeCandidate { identity: m31_identity(), separation_deg: 0.02 };
        let _ =
            assemble_suggestion(db.pool(), &resolver, &candidate, PointingSource::Wcs, true).await;

        let req = ConeSearchConfirmRequest {
            frameset_id: "item-fresh".to_owned(),
            candidate: contracts_core::cone_search::ConeSearchConfirmCandidate {
                canonical_target_id: None,
                primary_designation: "M 31".to_owned(),
                simbad_oid: Some(1_575_544),
            },
        };
        let resp = confirm(db.pool(), &resolve_cache.cache(), &req).await.unwrap();
        assert!(resp.created, "the cache-warmed candidate must be confirmable");
    }

    /// FR-007/OQ-1: when the SAME physical object was already cached under a
    /// DIFFERENT alias (a prior typeahead search for "NGC 224"), confirming a
    /// cone-search hit that resolved as "M 31" must reuse that existing id —
    /// never derive a second, mismatched id from the "M 31" designation.
    #[tokio::test]
    async fn confirm_reuses_existing_oid_cached_id_even_under_a_different_designation() {
        let db = test_db().await;
        seed_item(&db, "item-alias", None, None, None, None, None, None, None).await;
        let resolve_cache = ResolveCache::in_memory().unwrap();
        let ns = simbad_resolver::identity::namespace("astro-plan.targets");
        // Pre-cache the object under its NGC alias (as if from an earlier,
        // unrelated typeahead search) — a DIFFERENT primary_designation than
        // the cone-search hit will carry below.
        let ngc_identity = simbad_resolver::ResolvedIdentity {
            primary_designation: "NGC 224".to_owned(),
            ..m31_identity()
        };
        simbad_resolver::Cache::upsert(&resolve_cache.cache(), &ngc_identity, &ns).await.unwrap();
        let expected_id =
            simbad_resolver::Cache::get_by_simbad_oid(&resolve_cache.cache(), 1_575_544)
                .await
                .unwrap()
                .unwrap()
                .id;

        // Cone-search resolved the object as "M 31" (a different designation
        // than the cached "NGC 224"); a naive designation-derived id would
        // NOT equal `expected_id`.
        let naive_id = targeting::identity::target_id_from_designation("M 31");
        assert_ne!(naive_id, expected_id, "the test must exercise a genuine alias mismatch");

        let req = ConeSearchConfirmRequest {
            frameset_id: "item-alias".to_owned(),
            candidate: contracts_core::cone_search::ConeSearchConfirmCandidate {
                canonical_target_id: None,
                primary_designation: "M 31".to_owned(),
                simbad_oid: Some(1_575_544),
            },
        };
        let resp = confirm(db.pool(), &resolve_cache.cache(), &req).await.unwrap();
        assert_eq!(
            resp.canonical_target_id,
            expected_id.to_string(),
            "must reuse the oid-cached id, not a fresh designation-derived one"
        );
    }

    #[tokio::test]
    async fn confirm_writes_exactly_one_canonical_target_row() {
        let db = test_db().await;
        seed_item(&db, "item-confirm", None, None, None, None, None, None, None).await;
        let cache = seeded_cache_with_m31().await;

        let req = ConeSearchConfirmRequest {
            frameset_id: "item-confirm".to_owned(),
            candidate: contracts_core::cone_search::ConeSearchConfirmCandidate {
                canonical_target_id: None,
                primary_designation: "M 31".to_owned(),
                simbad_oid: Some(1_575_544),
            },
        };
        let resp = confirm(db.pool(), &cache, &req).await.unwrap();
        assert!(resp.created);
        assert!(resp.linked);

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM canonical_target")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1, "confirm must write exactly one canonical_target row");

        // Confirming again (idempotent) must not create a second row.
        let resp2 = confirm(db.pool(), &cache, &req).await.unwrap();
        assert!(!resp2.created, "second confirm reuses the existing dedup match");
        let (count2,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM canonical_target")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count2, 1);
    }

    /// #1294: confirming a cone-search match must clear the `target`
    /// mandatory-attribute gate the Inbox UI reads via
    /// `metadata::get_inbox_item_metadata` — not just write the durable
    /// `canonical_target` link. Before the fix, `get_inbox_item_metadata`
    /// read `inbox_file_metadata.object` directly and never saw the `target`
    /// override `confirm` writes at cone_search.rs:509-540, so the frameset
    /// kept reporting its target as missing until an unrelated reclassify
    /// happened to re-run the override rebuild.
    #[tokio::test]
    async fn confirm_clears_the_target_mandatory_gate_for_the_confirmed_frameset() {
        let db = test_db().await;
        let item_id = "item-1294";
        seed_item(&db, item_id, None, None, None, None, None, None, None).await;

        // Wire the item into a source group (as classify normally does) so
        // confirm's best-effort override write (cone_search.rs:509-540) has
        // somewhere to land, and classify the sole file as a light frame with
        // the other mandatory attributes present so `target` is isolated as
        // the only variable.
        sqlx::query(
            "INSERT INTO inbox_source_groups \
             (id, root_id, relative_path, discovered_at, last_scanned_at, child_count) \
             VALUES ('sg-1294', 'root-1', 'lights-item-1294', \
                     '2025-10-10T20:00:00Z', '2025-10-10T20:00:00Z', 1)",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query("UPDATE inbox_items SET source_group_id = 'sg-1294' WHERE id = ?")
            .bind(item_id)
            .execute(db.pool())
            .await
            .unwrap();
        let relative_file_path = format!("lights-{item_id}/light_001.fits");
        repo::insert_evidence(
            db.pool(),
            &repo::InsertEvidence {
                id: "ev-1294",
                inbox_item_id: item_id,
                relative_file_path: &relative_file_path,
                frame_type: Some("light"),
                evidence_source: "imagetyp_header",
                raw_value: Some("Light Frame"),
                unclassified: false,
                manual_override: None,
                is_master: false,
                master_detector: None,
            },
        )
        .await
        .unwrap();
        repo::upsert_inbox_file_metadata(
            db.pool(),
            &UpsertFileMetadata {
                inbox_item_id: item_id,
                relative_file_path: &relative_file_path,
                filter: Some("Ha"),
                exposure_s: Some(300.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let files_before =
            crate::metadata::get_inbox_item_metadata(db.pool(), item_id).await.unwrap();
        assert!(
            files_before[0].missing_mandatory.iter().any(|k| k == "target"),
            "sanity: target starts out missing"
        );

        let cache = seeded_cache_with_m31().await;
        let req = ConeSearchConfirmRequest {
            frameset_id: item_id.to_owned(),
            candidate: contracts_core::cone_search::ConeSearchConfirmCandidate {
                canonical_target_id: None,
                primary_designation: "M 31".to_owned(),
                simbad_oid: Some(1_575_544),
            },
        };
        confirm(db.pool(), &cache, &req).await.unwrap();

        let files_after =
            crate::metadata::get_inbox_item_metadata(db.pool(), item_id).await.unwrap();
        assert!(
            !files_after[0].missing_mandatory.iter().any(|k| k == "target"),
            "confirming the cone-search match must clear the target mandatory gate"
        );
        assert_eq!(
            files_after[0].object.as_deref(),
            Some("M 31"),
            "the confirmed designation must become the effective OBJECT"
        );
    }

    #[tokio::test]
    async fn confirm_unresolvable_candidate_is_candidate_invalid() {
        let db = test_db().await;
        seed_item(&db, "item-bad", None, None, None, None, None, None, None).await;
        let cache = simbad_resolver::Store::in_memory().unwrap().cache();

        let req = ConeSearchConfirmRequest {
            frameset_id: "item-bad".to_owned(),
            candidate: contracts_core::cone_search::ConeSearchConfirmCandidate {
                canonical_target_id: None,
                primary_designation: "Totally Unknown".to_owned(),
                simbad_oid: None,
            },
        };
        let err = confirm(db.pool(), &cache, &req).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::CandidateInvalid);
    }

    #[tokio::test]
    async fn confirm_unknown_frameset_is_not_found() {
        let db = test_db().await;
        let cache = seeded_cache_with_m31().await;
        let req = ConeSearchConfirmRequest {
            frameset_id: "nope".to_owned(),
            candidate: contracts_core::cone_search::ConeSearchConfirmCandidate {
                canonical_target_id: None,
                primary_designation: "M 31".to_owned(),
                simbad_oid: Some(1_575_544),
            },
        };
        let err = confirm(db.pool(), &cache, &req).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::FramesetNotFound);
    }
}
