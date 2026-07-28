// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * moon-avoidance.ts — SHARED Lorentzian Moon-avoidance rule (spec 047, plan D4).
 *
 * This is the ONE authoritative module for per-band Moon-avoidance guidance.
 * Spec 047 (Track A) owns it; spec 044 (Track B) imports these exact types,
 * defaults, and pure functions for its per-band moon-free-time integration —
 * it MUST NOT fork the tolerances, pills, or recommendation, and MUST NOT
 * define a second parameter store or a `min_lunar_separation_deg` scalar
 * (that knob is dead per the Track B handover).
 *
 * The rule: a filter band is viable tonight when the target's angular
 * separation from the Moon is at least the band's Lorentzian minimum
 * separation, which shrinks as the Moon ages away from full:
 *
 *   minSeparationDeg(band, ageDays) =
 *     distanceDeg[band] / (1 + (ageDays / widthDays[band])^2)
 *
 * where `ageDays` is days from full Moon (0 = full, ~14.77 = new). At full
 * Moon the required separation is the full `distanceDeg`; it falls off with a
 * Lorentzian (Cauchy) profile as the Moon darkens. Boundary is `>=` (a
 * separation exactly equal to the minimum counts as viable) — deterministic
 * per the spec edge case (FR-009/009a).
 *
 * Pure functions only — no React, no astronomy-engine import.
 */

/** Fixed filter-band set (v1). Broadband: L/R/G/B. Narrowband: Ha/SII/OIII. */
export type Band = 'L' | 'R' | 'G' | 'B' | 'Ha' | 'SII' | 'OIII';

/** Canonical band display order (LRGB, then Ha/SII/OIII). */
export const BANDS: readonly Band[] = ['L', 'R', 'G', 'B', 'Ha', 'SII', 'OIII'];

/** Broadband bands (share params — broadband viability is all-or-none). */
export const BROADBAND_BANDS: readonly Band[] = ['L', 'R', 'G', 'B'];

/** Narrowband bands. */
export const NARROWBAND_BANDS: readonly Band[] = ['Ha', 'SII', 'OIII'];

/** Per-band Lorentzian parameters. */
export interface BandParams {
  /** Required separation at full Moon (deg), in [0, 180]. */
  distanceDeg: number;
  /** Lorentzian half-width in Moon-age days, in [0.5, 30]. */
  widthDays: number;
}

/** Full per-band parameter set (mirrors the `plannerMoonAvoidance` settings key). */
export type MoonAvoidanceParams = Record<Band, BandParams>;

/**
 * Shipped defaults (data-model.md / plan D5):
 *   LRGB 120°/14d · Ha/SII 60°/7d · OIII 110°/10d.
 * Broadband is more Moon-sensitive (wide required distance, wide width);
 * narrowband tolerates a much closer Moon.
 */
export const DEFAULT_MOON_AVOIDANCE: MoonAvoidanceParams = {
  L: { distanceDeg: 120, widthDays: 14 },
  R: { distanceDeg: 120, widthDays: 14 },
  G: { distanceDeg: 120, widthDays: 14 },
  B: { distanceDeg: 120, widthDays: 14 },
  Ha: { distanceDeg: 60, widthDays: 7 },
  SII: { distanceDeg: 60, widthDays: 7 },
  OIII: { distanceDeg: 110, widthDays: 10 },
};

/**
 * Derived per-target recommendation category.
 * - `broadband-ok`: every band viable (LRGB share params → all-or-none).
 * - `narrowband-only`: no broadband band viable, but ≥1 narrowband viable.
 * - `avoid-tonight`: no band viable.
 * - `unknown`: no coordinates (viability could not be computed).
 */
export type Recommendation =
  | 'broadband-ok'
  | 'narrowband-only'
  | 'avoid-tonight'
  | 'unknown';

/**
 * Lorentzian minimum separation for one band at a given Moon age (days from full).
 *
 * @param band - Filter band.
 * @param ageDays - Days from full Moon (0 = full … ~14.77 = new). Clamped to ≥0.
 * @param params - Per-band parameter set.
 * @returns Required minimum separation in degrees (0…distanceDeg].
 */
export function minSeparationDeg(
  band: Band,
  ageDays: number,
  params: MoonAvoidanceParams = DEFAULT_MOON_AVOIDANCE,
): number {
  const { distanceDeg, widthDays } = params[band];
  const age = Math.max(0, ageDays);
  const ratio = age / widthDays;
  return distanceDeg / (1 + ratio * ratio);
}

/**
 * Per-band viability given a target's Moon separation and the Moon's age.
 *
 * Boundary: separation exactly equal to the required minimum counts as viable
 * (`>=`). Deterministic — no floating tie-break ambiguity.
 *
 * @param separationDeg - Target↔Moon angular separation (0…180°).
 * @param ageDays - Days from full Moon.
 * @param params - Per-band parameter set.
 * @returns A record of every band → boolean viability.
 */
export function bandViability(
  separationDeg: number,
  ageDays: number,
  params: MoonAvoidanceParams = DEFAULT_MOON_AVOIDANCE,
): Record<Band, boolean> {
  const out = {} as Record<Band, boolean>;
  for (const band of BANDS) {
    out[band] = separationDeg >= minSeparationDeg(band, ageDays, params);
  }
  return out;
}

/**
 * Derive the summary recommendation from a per-band viability record.
 *
 * `null` viability (unknown coordinates) → `'unknown'`.
 *
 * @param viability - Per-band viability, or `null` when coordinates are unknown.
 * @returns The derived recommendation category.
 */
export function deriveRecommendation(
  viability: Record<Band, boolean> | null,
): Recommendation {
  if (viability === null) return 'unknown';
  const anyBroadband = BROADBAND_BANDS.some((b) => viability[b]);
  const anyNarrowband = NARROWBAND_BANDS.some((b) => viability[b]);
  if (anyBroadband) return 'broadband-ok';
  if (anyNarrowband) return 'narrowband-only';
  return 'avoid-tonight';
}

/** Tier of a band for display grouping. */
export function bandTier(band: Band): 'broadband' | 'narrowband' {
  return BROADBAND_BANDS.includes(band) ? 'broadband' : 'narrowband';
}
