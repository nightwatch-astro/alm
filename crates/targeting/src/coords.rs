// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Coordinate-based nearest-neighbour target resolution (spec 041 R-17/R-18).
//!
//! This module is the **pure** kernel behind `inbox.target_recommendations`: it
//! takes a light sub-group's pointing, the in-memory target catalog, and a
//! search radius, and returns the catalog entries ranked ascending by
//! great-circle (haversine) angular separation, keeping only those within the
//! radius.
//!
//! No DB, no I/O, no spatial-index dependency — a bounded linear scan over the
//! (small) target catalog is sufficient (Constitution: keep dependencies
//! deliberate; the target DB is small). The caller (`app_core_inbox`) is
//! responsible for loading the pointing + catalog and for choosing the radius
//! (FOV-aware via [`fov_radius_deg`], or the configurable fixed fallback).
//!
//! # Why coordinates, never `OBJECT`
//!
//! R-17: the free-text `OBJECT`/`OBJCTNAME` header is set in capture software
//! (NINA etc.) and is inconsistent. Matching is done **only** by sky position;
//! `OBJECT` is carried by the caller as a display hint and never enters this
//! module. There is deliberately no name parameter on [`TargetCoord`] beyond the
//! display `name`, and nothing here compares names.

#![allow(clippy::doc_markdown)] // domain terminology (RA/Dec, FOV) is not backtick-suited

use skymath::Equatorial;
use target_match::{Field, Optics, RadiusPolicy};

/// A sky pointing in ICRS J2000 decimal degrees.
///
/// `ra_deg` is right ascension in `[0, 360)`; `dec_deg` is declination in
/// `[-90, 90]`. Inputs are not re-validated here (the caller extracts them from
/// already-validated metadata); out-of-domain values still produce a finite
/// separation via the haversine form (RA is wrapped into `[0, 360)` and Dec is
/// clamped into `[-90, 90]` before the underlying `skymath::Equatorial` is
/// built, since that type rejects out-of-domain input outright).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointing {
    /// Right ascension, decimal degrees.
    pub ra_deg: f64,
    /// Declination, decimal degrees.
    pub dec_deg: f64,
}

impl Pointing {
    /// Construct a pointing from decimal-degree RA/Dec.
    #[must_use]
    pub const fn new(ra_deg: f64, dec_deg: f64) -> Self {
        Self { ra_deg, dec_deg }
    }
}

/// One catalog entry to rank against a pointing.
///
/// `target_id` is the persisted `canonical_target.id` (a UUID string here so the
/// pure crate stays free of the `uuid` dependency for ranking); `name` is the
/// display designation. `ra_deg`/`dec_deg` are the catalog coordinates in
/// decimal degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetCoord {
    /// Persisted canonical-target id (opaque to this module).
    pub target_id: String,
    /// Display designation / effective label.
    pub name: String,
    /// Catalog right ascension, decimal degrees.
    pub ra_deg: f64,
    /// Catalog declination, decimal degrees.
    pub dec_deg: f64,
}

/// A ranked recommendation: a catalog entry plus its angular separation (deg)
/// from the queried pointing.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// Persisted canonical-target id.
    pub target_id: String,
    /// Display designation / effective label.
    pub name: String,
    /// Great-circle angular separation from the pointing, in decimal degrees.
    pub separation_deg: f64,
}

/// Great-circle angular separation between two pointings, in decimal degrees.
///
/// Delegates to `skymath::separation` (numerically-stable haversine form,
/// robust for the small separations that dominate target matching, where the
/// law-of-cosines form loses precision). The result is in `[0, 180]`.
///
/// A non-finite input on either pointing yields `NaN` (matching the previous
/// permissive behaviour), rather than the domain-validation error
/// `skymath::Equatorial::j2000_lenient` would otherwise raise.
#[must_use]
pub fn angular_separation_deg(a: Pointing, b: Pointing) -> f64 {
    let (Ok(ea), Ok(eb)) = (
        Equatorial::j2000_lenient(a.ra_deg, a.dec_deg),
        Equatorial::j2000_lenient(b.ra_deg, b.dec_deg),
    ) else {
        return f64::NAN;
    };
    skymath::separation(ea, eb).degrees()
}

/// Build a `skymath::Equatorial` from a [`Pointing`], wrapping RA into
/// `[0, 360)` and clamping Dec into `[-90, 90]` so out-of-domain-but-finite
/// inputs still produce a position rather than an error (see the [`Pointing`]
/// docs).
///
/// # Panics
/// Panics if `p.ra_deg` or `p.dec_deg` is non-finite (NaN/±inf) — callers with
/// possibly-non-finite input (e.g. an unvalidated catalog row) MUST filter
/// first; [`angular_separation_deg`] does this internally.
#[must_use]
pub fn to_equatorial(p: Pointing) -> Equatorial {
    Equatorial::j2000_lenient(p.ra_deg, p.dec_deg)
        .expect("Pointing must be finite; callers filter non-finite before calling")
}

/// Build a `target_match::Field` from optics + sensor pixel counts
/// (best-effort), for exact rectangular (optionally rotated) frame membership
/// via `target_match::Constraint::frame`/`frame_rotated`.
///
/// Pixels are assumed square (`pixel_size_um` applies to both axes) and
/// binning is fixed at `(1, 1)`: neither per-axis pixel size nor a binning
/// factor is tracked by the caller's per-file metadata. Delegates to
/// `target_match::Field::from_optics`, which uses the exact arcsec-per-radian
/// constant (`206_264.806…`) rather than a rounded approximation.
///
/// Returns `None` when any input is missing or non-positive, or when
/// `naxis1`/`naxis2` overflow `u32`. `focal_length_mm` and `pixel_size_um`
/// must be `> 0`; `naxis1`/`naxis2` must be `> 0`.
#[must_use]
pub fn field_from_optics(
    focal_length_mm: Option<f64>,
    pixel_size_um: Option<f64>,
    naxis1: Option<i64>,
    naxis2: Option<i64>,
) -> Option<Field> {
    let focal = focal_length_mm.filter(|v| v.is_finite() && *v > 0.0)?;
    let pixel = pixel_size_um.filter(|v| v.is_finite() && *v > 0.0)?;
    let nx = naxis1.filter(|v| *v > 0).and_then(|v| u32::try_from(v).ok())?;
    let ny = naxis2.filter(|v| *v > 0).and_then(|v| u32::try_from(v).ok())?;

    Field::from_optics(Optics {
        focal_mm: focal,
        pixel_um: (pixel, pixel),
        binning: (1, 1),
        pixels: (nx, ny),
    })
    .ok()
}

/// Compute a FOV-aware search radius (decimal degrees) from optics + sensor.
///
/// The radius is **half the sensor diagonal field of view** (`target_match`'s
/// [`RadiusPolicy::Circumscribed`]), so any catalog target whose true position
/// lies anywhere on the frame is inside the radius regardless of where in the
/// frame it sits. See [`field_from_optics`] for the underlying geometry.
///
/// Returns `None` when [`field_from_optics`] does (missing/non-positive input,
/// or `naxis1`/`naxis2` overflowing `u32`), so the caller can fall back to the
/// configurable fixed radius (R-17 C5).
#[must_use]
pub fn fov_radius_deg(
    focal_length_mm: Option<f64>,
    pixel_size_um: Option<f64>,
    naxis1: Option<i64>,
    naxis2: Option<i64>,
) -> Option<f64> {
    field_from_optics(focal_length_mm, pixel_size_um, naxis1, naxis2)
        .map(|f| f.radius(RadiusPolicy::Circumscribed).degrees())
}

/// Rank catalog targets by angular separation from `pointing`, keeping only
/// those within `radius_deg` (inclusive), ascending (nearest first).
///
/// Ties on separation break deterministically by `name` then `target_id` so the
/// ordering is stable across runs. A non-finite or negative `radius_deg` yields
/// an empty list. This is a bounded linear scan — no spatial index.
#[must_use]
pub fn rank_candidates(
    pointing: Pointing,
    targets: &[TargetCoord],
    radius_deg: f64,
) -> Vec<Candidate> {
    if !radius_deg.is_finite() || radius_deg < 0.0 {
        return Vec::new();
    }

    let mut hits: Vec<Candidate> = targets
        .iter()
        .filter_map(|t| {
            let sep = angular_separation_deg(pointing, Pointing::new(t.ra_deg, t.dec_deg));
            (sep <= radius_deg).then(|| Candidate {
                target_id: t.target_id.clone(),
                name: t.name.clone(),
                separation_deg: sep,
            })
        })
        .collect();

    hits.sort_by(|a, b| {
        a.separation_deg
            .partial_cmp(&b.separation_deg)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.target_id.cmp(&b.target_id))
    });
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    const M31: Pointing = Pointing::new(10.684_708, 41.268_75);

    fn target(id: &str, name: &str, ra: f64, dec: f64) -> TargetCoord {
        TargetCoord { target_id: id.to_owned(), name: name.to_owned(), ra_deg: ra, dec_deg: dec }
    }

    // ── angular_separation_deg ────────────────────────────────────────────────

    #[test]
    fn separation_to_self_is_zero() {
        assert!(angular_separation_deg(M31, M31).abs() < 1e-9);
    }

    #[test]
    fn separation_is_symmetric() {
        let a = Pointing::new(83.822_08, -5.391_11); // M42
        let ab = angular_separation_deg(M31, a);
        let ba = angular_separation_deg(a, M31);
        assert!((ab - ba).abs() < 1e-12);
    }

    #[test]
    fn separation_one_degree_along_equator() {
        // Two points on the celestial equator 1° apart in RA are exactly 1° apart.
        let a = Pointing::new(100.0, 0.0);
        let b = Pointing::new(101.0, 0.0);
        let sep = angular_separation_deg(a, b);
        assert!((sep - 1.0).abs() < 1e-9, "expected ~1.0°, got {sep}");
    }

    #[test]
    fn separation_ra_at_high_dec_is_compressed() {
        // 1° of RA at dec=60° subtends only ~0.5° on the sky (cos 60° = 0.5).
        let a = Pointing::new(100.0, 60.0);
        let b = Pointing::new(101.0, 60.0);
        let sep = angular_separation_deg(a, b);
        assert!((sep - 0.5).abs() < 1e-3, "expected ~0.5°, got {sep}");
    }

    #[test]
    fn separation_known_pair_m31_m110() {
        // M110 (NGC 205) sits ~0.62° from M31 — a real close pair.
        let m110 = Pointing::new(10.092_08, 41.685_28);
        let sep = angular_separation_deg(M31, m110);
        assert!((0.4..0.9).contains(&sep), "M31↔M110 expected ~0.62°, got {sep}");
    }

    #[test]
    fn separation_antipodal_is_180() {
        let a = Pointing::new(0.0, 0.0);
        let b = Pointing::new(180.0, 0.0);
        let sep = angular_separation_deg(a, b);
        assert!((sep - 180.0).abs() < 1e-6, "expected 180°, got {sep}");
    }

    // ── fov_radius_deg ────────────────────────────────────────────────────────

    #[test]
    fn fov_radius_from_optics() {
        // ASI2600 (3.76µm, 6248×4176) on an 800mm scope.
        // pixel scale = 206.265*3.76/800 ≈ 0.9694"/px
        // width ≈ 0.9694*6248/3600 ≈ 1.683°, height ≈ 0.9694*4176/3600 ≈ 1.125°
        // radius = hypot(1.683,1.125)/2 ≈ 1.012°
        let r = fov_radius_deg(Some(800.0), Some(3.76), Some(6248), Some(4176)).unwrap();
        assert!((0.95..1.07).contains(&r), "expected ~1.01°, got {r}");
    }

    #[test]
    fn fov_radius_none_when_pixel_size_absent() {
        // R-17: pixel size unavailable ⇒ no FOV radius ⇒ caller uses fixed fallback.
        assert!(fov_radius_deg(Some(800.0), None, Some(6248), Some(4176)).is_none());
    }

    #[test]
    fn fov_radius_none_when_any_input_missing_or_nonpositive() {
        assert!(fov_radius_deg(None, Some(3.76), Some(6248), Some(4176)).is_none());
        assert!(fov_radius_deg(Some(800.0), Some(3.76), None, Some(4176)).is_none());
        assert!(fov_radius_deg(Some(800.0), Some(3.76), Some(6248), None).is_none());
        assert!(fov_radius_deg(Some(0.0), Some(3.76), Some(6248), Some(4176)).is_none());
        assert!(fov_radius_deg(Some(800.0), Some(-1.0), Some(6248), Some(4176)).is_none());
        assert!(fov_radius_deg(Some(800.0), Some(3.76), Some(0), Some(4176)).is_none());
    }

    // ── rank_candidates ───────────────────────────────────────────────────────

    fn sample_catalog() -> Vec<TargetCoord> {
        vec![
            target("id-m31", "M 31", 10.684_708, 41.268_75),
            target("id-m110", "M 110", 10.092_08, 41.685_28),
            target("id-m33", "M 33", 23.462_1, 30.659_9), // ~14.7° away
            target("id-m42", "M 42", 83.822_08, -5.391_11), // far side of sky
        ]
    }

    #[test]
    fn ranks_ascending_within_radius() {
        let cat = sample_catalog();
        // 2° radius keeps M31 (self, 0°) and M110 (~0.62°); excludes M33 & M42.
        let out = rank_candidates(M31, &cat, 2.0);
        assert_eq!(out.len(), 2, "only M31 + M110 within 2°, got {out:?}");
        assert_eq!(out[0].target_id, "id-m31");
        assert!(out[0].separation_deg < 1e-6, "nearest is the exact match");
        assert_eq!(out[1].target_id, "id-m110");
        assert!(out[0].separation_deg <= out[1].separation_deg, "ascending order");
    }

    #[test]
    fn excludes_targets_outside_radius() {
        let cat = sample_catalog();
        // A tight 0.1° radius keeps only the exact-match M31.
        let out = rank_candidates(M31, &cat, 0.1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_id, "id-m31");
    }

    #[test]
    fn radius_boundary_is_inclusive() {
        let cat = vec![target("id-a", "A", 101.0, 0.0)];
        let probe = Pointing::new(100.0, 0.0); // ~1° away
                                               // Use the exact computed separation as the radius: sep <= radius must
                                               // include it (boundary is inclusive). Comparing against a hand-written
                                               // 1.0 would be brittle since haversine(1°) lands a few ULPs off 1.0.
        let sep = angular_separation_deg(probe, Pointing::new(101.0, 0.0));
        assert_eq!(rank_candidates(probe, &cat, sep).len(), 1, "sep == radius is included");
        assert_eq!(
            rank_candidates(probe, &cat, sep - 1e-9).len(),
            0,
            "just inside the radius excludes a target fractionally outside it"
        );
    }

    #[test]
    fn empty_when_radius_negative_or_nan() {
        let cat = sample_catalog();
        assert!(rank_candidates(M31, &cat, -1.0).is_empty());
        assert!(rank_candidates(M31, &cat, f64::NAN).is_empty());
    }

    #[test]
    fn empty_catalog_yields_no_candidates() {
        assert!(rank_candidates(M31, &[], 10.0).is_empty());
    }

    #[test]
    fn equal_separation_ties_break_by_name_then_id() {
        // Two targets symmetric about the pointing → identical separation.
        let probe = Pointing::new(50.0, 0.0);
        let cat = vec![target("id-z", "Zeta", 51.0, 0.0), target("id-a", "Alpha", 49.0, 0.0)];
        let out = rank_candidates(probe, &cat, 2.0);
        assert_eq!(out.len(), 2);
        // Equal separation → "Alpha" (id-a) sorts before "Zeta" (id-z).
        assert_eq!(out[0].name, "Alpha");
        assert_eq!(out[1].name, "Zeta");
    }

    /// OBJECT/name is NEVER a matching input: a catalog entry whose `name`
    /// equals the (irrelevant) display string but whose coordinates are far
    /// away is excluded by radius — only position decides membership.
    #[test]
    fn name_never_drives_matching_only_coordinates_do() {
        // Catalog entry literally named "M 31" but parked at the wrong coords.
        let mislabelled = target("id-wrong", "M 31", 200.0, -40.0);
        let correct = target("id-right", "Some Galaxy", M31.ra_deg, M31.dec_deg);
        let cat = vec![mislabelled, correct];

        let out = rank_candidates(M31, &cat, 2.0);
        assert_eq!(out.len(), 1, "the far 'M 31'-named entry must NOT match by name");
        assert_eq!(out[0].target_id, "id-right", "only the coordinate match is returned");
    }

    // ── angular_separation_deg boundary / equivalence ────────────────────────

    #[test]
    fn separation_nan_on_non_finite_inputs() {
        // Non-finite RA or Dec on either side must propagate NaN, never panic.
        let good = Pointing::new(10.0, 20.0);
        assert!(angular_separation_deg(Pointing::new(f64::NAN, 0.0), good).is_nan());
        assert!(angular_separation_deg(Pointing::new(0.0, f64::INFINITY), good).is_nan());
        assert!(angular_separation_deg(good, Pointing::new(f64::NAN, 0.0)).is_nan());
        assert!(angular_separation_deg(good, Pointing::new(0.0, f64::NEG_INFINITY)).is_nan());
    }

    #[test]
    fn separation_out_of_domain_finite_ra_is_normalized() {
        // RA=370 wraps to 10; points are identical so separation is 0.
        let a = Pointing::new(10.0, 20.0);
        let b = Pointing::new(370.0, 20.0);
        assert!(angular_separation_deg(a, b) < 1e-9, "RA 370 wraps to 10, same point");
    }

    #[test]
    fn separation_near_south_pole_with_large_ra_gap() {
        // At dec=-89.9, 180° apart in RA is only ~0.2° on the sphere.
        let a = Pointing::new(0.0, -89.9);
        let b = Pointing::new(180.0, -89.9);
        let sep = angular_separation_deg(a, b);
        assert!(sep < 0.3, "expected small south-polar separation, got {sep}");
    }
}
