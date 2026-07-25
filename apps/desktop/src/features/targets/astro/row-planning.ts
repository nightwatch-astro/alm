// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * row-planning.ts — per-target planner astronomy derivation (spec 047).
 *
 * Combines a target's catalogued J2000 coordinates with the shared observing
 * night to produce the per-row `RowMoonPlanning` decorations (data-model.md):
 * real lunar separation (US2), per-band filter viability + derived
 * recommendation (US3), and the next opposition date (US4).
 *
 * `night === null` means the site gate is closed (no observing site yet); every
 * derived value is then the explicit unknown state, never a fabricated number.
 *
 * `geometryOverride` (#634): `TargetsTable`'s full-catalogue pass no longer
 * computes lunar separation / opposition via the TS `astronomy-engine` calls
 * below — it fetches them once per batch from the real Rust
 * `target.moon_opposition.batch` command (never per-row round trips) and
 * passes the looked-up result in explicitly (`null` = fetch not resolved yet
 * / unknown coordinates, same "never fabricate" contract as everything else
 * here). Every OTHER caller (`TargetDetail`'s single-target planner,
 * `TargetsPage`'s synchronous full-catalogue filter-count pass, and this
 * file's own tests) omits the 4th argument and keeps the original synchronous
 * TS-ephemeris path unchanged — this is a purely additive, backward-compatible
 * parameter, not a signature break.
 */

import type { ObservingNight } from './moon-state';
import { lunarSeparationDeg } from './lunar-separation';
import {
  bandViability,
  deriveRecommendation,
  DEFAULT_MOON_AVOIDANCE,
  type Band,
  type MoonAvoidanceParams,
  type Recommendation,
} from './moon-avoidance';
import { nextOpposition } from './opposition';

/** Minimal catalogued coordinates a row exposes for planning. */
export interface RowCoords {
  raDeg: number | null;
  decDeg: number | null;
}

/**
 * Real Moon geometry for one target (#634): lunar separation + next
 * opposition, sourced from the batched Rust command instead of computed
 * locally. `lunarSeparationDeg: null` mirrors the existing "unknown
 * coordinates" convention (never a fabricated number).
 */
export interface RowMoonGeometry {
  lunarSeparationDeg: number | null;
  nextOppositionDate: string | null;
  daysToOpposition: number | null;
}

/** Per-target planner astronomy (data-model.md §RowMoonPlanning). */
export interface RowMoonPlanning {
  /** Target↔Moon separation (0…180°); `null` = unknown coordinates or no site. */
  lunarSeparationDeg: number | null;
  /** Per-band viability for tonight; `null` = unknown coordinates or no site. */
  bandViability: Record<Band, boolean> | null;
  /** Derived summary recommendation ('unknown' when `bandViability` is null). */
  recommendation: Recommendation;
  /** Next opposition date (ISO `YYYY-MM-DD`), or `null` = unknown coordinates. */
  nextOppositionDate: string | null;
  /** Whole days from tonight to `nextOppositionDate`; sort key (unknowns last). */
  daysToOpposition: number | null;
}

/**
 * Derive the per-row planner astronomy for a target under a given night.
 *
 * @param coords - The row's catalogued RA/Dec (either may be `null`).
 * @param night - The shared observing night, or `null` when no site exists.
 * @param params - Active per-band Moon-avoidance parameters (Settings →
 *   Target Planner); defaults to the shipped table.
 * @param geometryOverride - #634: when provided (even `null`), lunar
 *   separation + opposition come from this pre-fetched batch result instead
 *   of the local TS ephemeris — `TargetsTable`'s only call site. Omitted
 *   (`undefined`, every other caller) preserves the original synchronous,
 *   no-fetch TS computation.
 */
export function deriveRowMoonPlanning(
  coords: RowCoords,
  night: ObservingNight | null,
  params: MoonAvoidanceParams = DEFAULT_MOON_AVOIDANCE,
  geometryOverride?: RowMoonGeometry | null,
): RowMoonPlanning {
  if (!night) return { ...UNKNOWN_ROW_PLANNING };

  const geometry: RowMoonGeometry | null =
    geometryOverride !== undefined
      ? geometryOverride
      : localGeometry(coords, night);
  if (!geometry) return { ...UNKNOWN_ROW_PLANNING };

  const separation = geometry.lunarSeparationDeg;
  const viability =
    separation === null
      ? null
      : bandViability(separation, night.moonAgeFromFullDays, params);
  const recommendation = deriveRecommendation(viability);

  return {
    lunarSeparationDeg: separation,
    bandViability: viability,
    recommendation,
    nextOppositionDate: geometry.nextOppositionDate,
    daysToOpposition: geometry.daysToOpposition,
  };
}

/** The original synchronous TS-ephemeris geometry (pre-#634 behavior). */
function localGeometry(
  coords: RowCoords,
  night: ObservingNight,
): RowMoonGeometry {
  const separation = lunarSeparationDeg(
    coords.raDeg,
    coords.decDeg,
    night.moonVec,
  );
  const opposition = nextOpposition(coords.raDeg, night.midnight);
  return {
    lunarSeparationDeg: separation,
    nextOppositionDate: opposition
      ? opposition.date.toISOString().slice(0, 10)
      : null,
    daysToOpposition: opposition ? opposition.daysUntil : null,
  };
}

/** The explicit unknown planning row (no site / no coordinates). */
export const UNKNOWN_ROW_PLANNING: RowMoonPlanning = {
  lunarSeparationDeg: null,
  bandViability: null,
  recommendation: 'unknown',
  nextOppositionDate: null,
  daysToOpposition: null,
};
