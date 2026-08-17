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
//! responsible for loading the pointing and the radius (FOV-aware via
//! `field_from_optics`, or the configurable fixed fallback).
//!
//! # Why coordinates, never `OBJECT`
//!
//! R-17: the free-text `OBJECT`/`OBJCTNAME` header is set in capture software
//! (NINA etc.) and is inconsistent. Matching is done **only** by sky position;
//! `OBJECT` is carried by the caller as a display hint and never enters this
//! module.

#![allow(clippy::doc_markdown)] // domain terminology (RA/Dec, FOV) is not backtick-suited

use skymath::Equatorial;
use target_match::{Field, Optics};

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

#[cfg(test)]
mod tests {
    use super::*;

    const M31: Pointing = Pointing::new(10.684_708, 41.268_75);

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
