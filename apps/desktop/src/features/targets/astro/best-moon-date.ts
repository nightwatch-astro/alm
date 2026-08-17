// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * best-moon-date.ts — nearest Moon-viable night to opposition (spec 044,
 * FR-009 amendment, iteration 2026-07-17). DETAIL-PANE ONLY: the planner
 * list's "Opposition" column stays the pure geometric transit-at-midnight
 * date from `opposition.ts` (spec 047 FR-014).
 *
 * Search: take `nextOpposition` unchanged, then score the 31 nights at
 * opposition ±15 using the existing shared primitives verbatim —
 * `moonStateAt` (age-from-full + illumination + geocentric Moon vector),
 * `lunarSeparationDeg`, and spec 047's Lorentzian `minSeparationDeg`. A
 * night is viable when separation ≥ the scoring band's minimum for that
 * night's Moon age. Scoring band v1 is broadband L; it is an explicit
 * parameter so a passband-aware upgrade (equipment sensor/passband, FR-035/
 * FR-036) is a parameter change, not a rework.
 *
 * Selection: opposition night viable → opposition (`'coincides'`); else the
 * nearest viable night, ties preferring the earlier night; nights before the
 * search start are never recommended (a past date is not a plan); no viable
 * night in the window → the opposition date with the distinct `'none-viable'`
 * state — never a silent fallback.
 *
 * Performance (the `sunRaTable` memoization pattern): the 31-night Moon table
 * is target-independent — it depends only on the window start — so it is
 * computed once per distinct window start (single-entry cache; every
 * re-render of one detail pane shares it) and the per-target marginal cost is
 * one unit vector + 31 dot products.
 *
 * Known limitation (documented in the spec amendment): each candidate night
 * is scored from a single geocentric Moon snapshot at the candidate instant,
 * so a close Moon counts as interfering even when it is below the local
 * horizon that night — the same simplification as the shipped Track-A
 * tonight guidance.
 */

import { moonStateAt, type Vec3 } from './moon-state';
import { lunarSeparationDeg } from './lunar-separation';
import {
  minSeparationDeg,
  type Band,
  type MoonAvoidanceParams,
} from '@/shared/planner/moon-avoidance';
import { nextOpposition } from './opposition';

const MS_PER_DAY = 86_400_000;

/** Search radius around the opposition night (±15 nights → 31 candidates). */
export const BEST_DATE_RADIUS_NIGHTS = 15;

/** Per-night Moon facts needed for scoring (target-independent). */
interface MoonNightEntry {
  ageDays: number;
  illumFrac: number;
  moonVec: Vec3;
}

// ── Memoized 31-night Moon table (sunRaTable pattern) ────────────────────────

let cachedWindowStartMs: number | null = null;
let cachedMoonTable: MoonNightEntry[] | null = null;

function moonNightTable(windowStartMs: number): MoonNightEntry[] {
  if (cachedWindowStartMs === windowStartMs && cachedMoonTable) {
    return cachedMoonTable;
  }
  const table = new Array<MoonNightEntry>(2 * BEST_DATE_RADIUS_NIGHTS + 1);
  for (let i = 0; i < table.length; i++) {
    const s = moonStateAt(new Date(windowStartMs + i * MS_PER_DAY));
    table[i] = {
      ageDays: s.moonAgeFromFullDays,
      illumFrac: s.illuminationFrac,
      moonVec: s.moonVec,
    };
  }
  cachedWindowStartMs = windowStartMs;
  cachedMoonTable = table;
  return table;
}

/** Test-only: clear the Moon-table cache (avoid cross-test leakage). */
export function __resetBestMoonDateCacheForTest(): void {
  cachedWindowStartMs = null;
  cachedMoonTable = null;
}

// ── Result shape ─────────────────────────────────────────────────────────────

/** Moon facts for one scored night, UI-ready. */
export interface MoonNightFacts {
  /** Illuminated fraction of the lunar disk, whole percent (0…100). */
  illumPct: number;
  /** Target↔Moon separation in degrees (0…180, unrounded). */
  sepDeg: number;
}

/**
 * How the best date relates to the opposition date:
 * - `'coincides'`: the opposition night itself is Moon-viable.
 * - `'diverged'`: a different (nearest viable) night was chosen.
 * - `'none-viable'`: no night in the ±15 window qualifies — `dateMs` falls
 *   back to the opposition date, disclosed distinctly (never silently).
 */
export type BestMoonDateState = 'coincides' | 'diverged' | 'none-viable';

/** Moon-aware best imaging date (spec 044 FR-009 amendment). */
export interface BestMoonDate {
  /** The recommended date (whole-day resolution, same instant grid as `from`). */
  dateMs: number;
  /** Whole days from `from` to `dateMs` (≥ 0). */
  inDays: number;
  state: BestMoonDateState;
  /** The unadjusted opposition date (`nextOpposition`, list-column value). */
  oppositionDateMs: number;
  /** Moon facts on the recommended night. */
  moonAtBest: MoonNightFacts;
  /** Moon facts on the opposition night (equals `moonAtBest` when not diverged). */
  moonAtOpposition: MoonNightFacts;
}

/**
 * Find the nearest Moon-viable night to the target's next opposition.
 *
 * @param raDeg - Target J2000 right ascension in degrees, or `null`/`undefined`
 *   for unknown coordinates (returns `null`).
 * @param decDeg - Target J2000 declination in degrees, or `null`/`undefined`.
 * @param from - Search start instant (the planner date). Passed unchanged to
 *   `nextOpposition`, so the opposition date here is byte-identical to the
 *   list column's. Candidate nights are scored at `opposition ± N days` on
 *   this instant's time-of-day grid (whole-day tolerance, matching
 *   `nextOpposition`'s one-day resolution).
 * @param params - Live per-band Moon-avoidance parameters (spec 047's shared
 *   settings) — a tuning edit recomputes the result.
 * @param scoringBand - Band whose Lorentzian minimum defines viability.
 *   v1 default `'L'` (broadband); a passband-aware upgrade passes a
 *   different band here.
 * @returns The moon-aware best date, or `null` when coordinates are unknown.
 */
export function bestMoonDate(
  raDeg: number | null | undefined,
  decDeg: number | null | undefined,
  from: Date,
  params: MoonAvoidanceParams,
  scoringBand: Band = 'L',
): BestMoonDate | null {
  if (decDeg == null || !Number.isFinite(decDeg)) return null;
  const opposition = nextOpposition(raDeg, from);
  if (!opposition) return null;

  const fromMs = from.getTime();
  const oppositionMs = opposition.date.getTime();
  const windowStartMs = oppositionMs - BEST_DATE_RADIUS_NIGHTS * MS_PER_DAY;
  const table = moonNightTable(windowStartMs);

  // Per-target marginal work: 31 separations + 31 Lorentzian thresholds.
  const sepDeg = table.map(
    (entry) => lunarSeparationDeg(raDeg, decDeg, entry.moonVec) as number,
  );
  const viable = table.map(
    (entry, i) =>
      sepDeg[i] >= minSeparationDeg(scoringBand, entry.ageDays, params),
  );
  /** A night before the search start is never a recommendation. */
  const inPast = (i: number) => windowStartMs + i * MS_PER_DAY < fromMs;

  const OPP = BEST_DATE_RADIUS_NIGHTS; // index of the opposition night
  let chosen: number | null = null;
  if (viable[OPP]) {
    chosen = OPP;
  } else {
    for (let d = 1; d <= BEST_DATE_RADIUS_NIGHTS && chosen === null; d++) {
      // Nearest first; at equal distance the earlier night wins the tie.
      if (viable[OPP - d] && !inPast(OPP - d)) chosen = OPP - d;
      else if (viable[OPP + d]) chosen = OPP + d;
    }
  }

  const state: BestMoonDateState =
    chosen === null ? 'none-viable' : chosen === OPP ? 'coincides' : 'diverged';
  const index = chosen ?? OPP;
  const dateMs = windowStartMs + index * MS_PER_DAY;
  const factsAt = (i: number): MoonNightFacts => ({
    illumPct: Math.round(table[i].illumFrac * 100),
    sepDeg: sepDeg[i],
  });

  return {
    dateMs,
    inDays: Math.round((dateMs - fromMs) / MS_PER_DAY),
    state,
    oppositionDateMs: oppositionMs,
    moonAtBest: factsAt(index),
    moonAtOpposition: factsAt(OPP),
  };
}
