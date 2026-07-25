// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-derive.ts — pure derivations over cached night observability (spec 044 US1 T010).
 *
 * Splits the planner's astronomy into two layers so the interactive controls are
 * cheap (SC-003): the expensive positions (`NightObservability`) are computed
 * once per `(target, activeSite, date)` and memoized here; the threshold-
 * dependent scalars (`DerivedObservability`) are re-derived from the cached
 * positions **without** recomputing positions whenever the usable-altitude
 * threshold changes.
 *
 * US1: band-free total imaging time, visible-tonight, max altitude.
 *
 * US5 (T028): the three Moon separation scalars (transit / min-over-dark /
 * dark-midpoint) and per-band moon-free minutes, importing Track A's shared
 * Lorentzian rule (`astro/moon-avoidance.ts`'s `minSeparationDeg`) — Track B
 * integrates the rule over its own geometry but MUST NOT redefine the
 * per-band tolerances (FR-022/FR-023).
 *
 * US2 (T025): `bestDate` (FR-009) is the exact same anti-solar-RA search
 * Track A already ships as `nextOpposition` (a fixed-RA "transits at local
 * midnight" date is that target's opposition-style date) — reused directly,
 * not reimplemented.
 *
 * Pure functions only — no React, no astronomy-engine import beyond the memoized
 * `computeNightObservability` call (Moon-band integration below reads only the
 * already-computed `moonSamples`/`moonAgeFromFullDays`, no fresh engine calls).
 */

import {
  GRID_STEP_MINUTES,
  type MoonSample,
  type NightObservability,
  type TimeWindow,
  computeNightObservability,
} from './planner-astronomy';
import type { ObserverSite } from '@/shared/observing-sites/observer-site';
import {
  BANDS,
  BROADBAND_BANDS,
  DEFAULT_MOON_AVOIDANCE,
  minSeparationDeg,
  type Band,
  type MoonAvoidanceParams,
} from './astro/moon-avoidance';
import { nextOpposition } from './astro/opposition';

/** A separation figure, or the explicit "Moon not up" state at the reference time/window (FR-020). */
export type SeparationFigure = number | 'moon-not-up';

/** The three target↔Moon separation reference figures (US5, FR-020). */
export interface SeparationScalars {
  atTransitDeg: SeparationFigure;
  minOverDarkDeg: SeparationFigure;
  atDarkMidpointDeg: SeparationFigure;
}

/** Best-imaging date (US2, FR-009): local-midnight transit date + days-until. */
export interface BestImagingDate {
  dateMs: number;
  inDays: number;
}

/**
 * Why imaging time is zero (iteration 2026-07-15, FR-029). Precedence when
 * several blockers hold at once: darkness > altitude > moon — the most
 * upstream structural blocker is reported.
 *   - 'darkness': no qualifying dark window tonight (FR-017).
 *   - 'altitude': a dark window exists but the target never clears the
 *     usable-altitude threshold inside it.
 *   - 'moon': dark ∩ uptime is non-empty but every band's moon-viable window
 *     is empty (only reported when Moon geometry was actually computed —
 *     never inferred from the not-computed zero degrade).
 */
export type ZeroImagingReason = 'darkness' | 'altitude' | 'moon';

/**
 * OSC passband (iteration 2026-07-15, FR-035): `'rgb'` = plain color camera;
 * a band subset (e.g. `['Ha','OIII']`) = dual/tri-band narrowband filter.
 */
export type OscPassband = 'rgb' | Band[];

/** Camera sensor configuration driving FR-036/FR-038; `null`/absent = unknown → mono behavior. */
export interface SensorConfig {
  sensorType: 'mono' | 'osc';
  /** Only meaningful for `'osc'`; defaults to `'rgb'` when unset. */
  passband?: OscPassband | null;
}

/** Threshold-derived, per-target observability scalars. */
export interface DerivedObservability {
  /** Peak altitude across the night (= transit altitude), degrees. */
  maxAltDeg: number;
  /**
   * True when the target reaches the usable altitude anywhere in the
   * observable window — the astronomical dark window when one exists, else the
   * whole night (#579: discriminate by altitude even when there is no
   * astronomical darkness, rather than collapsing every target to not-visible).
   */
  visibleTonight: boolean;
  /** Minutes of dark window with altitude ≥ usable (band-free — FR-005). */
  totalImagingMinutes: number;
  /** Date the target transits at local midnight (US2, FR-009); `null` = unknown coordinates. */
  bestDate: BestImagingDate | null;
  /** Three real target↔Moon separation reference figures (US5, FR-020). */
  separationScalars: SeparationScalars;
  /** Per-band moon-free imaging minutes (US5, FR-022); zero for every band with no dark window. */
  moonFreeMinutesByBand: Record<Band, number>;
  /**
   * Minutes the target sits above the usable altitude across the WHOLE night
   * (the D1 "target uptime window" length — NOT gated by the dark window).
   * Distinguishable from `totalImagingMinutes` (= dark ∩ uptime) per FR-005.
   */
  uptimeMinutes: number;
  /** The stated blocker when imaging time is (effectively) zero (FR-029); `null` otherwise. */
  zeroImagingReason: ZeroImagingReason | null;
  /**
   * Bands whose moon-viable window is strictly smaller than dark ∩ uptime
   * (FR-031's "Moon is the actionable limiter" facts). Empty when Moon
   * geometry wasn't computed — "not computed" never reads as "limited".
   */
  moonLimitedBands: Band[];
  /**
   * OSC single-pass imaging minutes (FR-036): dark ∩ uptime ∩ moon-viable
   * under the strictest required separation across the passband's bands.
   * `null` when the sensor is mono/unknown OR Moon geometry wasn't computed
   * (never fabricated).
   */
  oscSinglePassMinutes: number | null;
}

/** Options for the Moon/best-date integration layered onto `deriveObservability`. */
export interface DeriveOptions {
  /** The target's J2000 RA (for `bestDate`); `null`/undefined = unknown coordinates. */
  raDegJ2000?: number | null;
  /** The site's minimum-horizon altitude (Moon-up determination, T032); default 0. */
  minHorizonAltDeg?: number;
  /** Active per-band Moon-avoidance parameters (Track A, Settings → Target Planner). */
  moonAvoidanceParams?: MoonAvoidanceParams;
  /** Search-start instant for `bestDate` (typically the planned night's start); default `night.nightStartMs`. */
  bestDateFromMs?: number;
  /**
   * Camera sensor configuration (FR-035/FR-038). `'osc'` computes
   * `oscSinglePassMinutes` (FR-036); mono/unknown/absent keeps today's
   * per-filter model unchanged.
   */
  sensorConfig?: SensorConfig | null;
}

const ZERO_BY_BAND: Record<Band, number> = Object.fromEntries(
  BANDS.map((b) => [b, 0]),
) as Record<Band, number>;

/** The explicit "no astronomy possible" separation-scalars state. */
export const UNKNOWN_SEPARATION_SCALARS: SeparationScalars = {
  atTransitDeg: 'moon-not-up',
  minOverDarkDeg: 'moon-not-up',
  atDarkMidpointDeg: 'moon-not-up',
};

/** Nearest `moonSamples` entry to `tMs` (grid is 10-min resolution; linear scan is cheap — ≤ ~72 samples/night). */
function nearestMoonSample(
  moonSamples: MoonSample[],
  tMs: number,
): MoonSample | null {
  let best: MoonSample | null = null;
  let bestDiffMs = Infinity;
  for (const s of moonSamples) {
    const diff = Math.abs(s.tMs - tMs);
    if (diff < bestDiffMs) {
      best = s;
      bestDiffMs = diff;
    }
  }
  return best;
}

/** The separation figure at a single reference instant, or "moon-not-up" (FR-020). */
function separationAt(
  moonSamples: MoonSample[],
  tMs: number,
  minHorizonAltDeg: number,
): SeparationFigure {
  const nearest = nearestMoonSample(moonSamples, tMs);
  if (!nearest || nearest.moonAltDeg < minHorizonAltDeg) return 'moon-not-up';
  return nearest.separationDeg;
}

/** The minimum separation over the dark window while the Moon is up, or "moon-not-up" if it never is. */
function minSeparationOverDark(
  moonSamples: MoonSample[],
  darkWindow: TimeWindow | null,
  minHorizonAltDeg: number,
): SeparationFigure {
  if (!darkWindow) return 'moon-not-up';
  let min = Infinity;
  for (const s of moonSamples) {
    if (s.tMs < darkWindow.startMs || s.tMs > darkWindow.endMs) continue;
    if (s.moonAltDeg < minHorizonAltDeg) continue;
    if (s.separationDeg < min) min = s.separationDeg;
  }
  return Number.isFinite(min) ? min : 'moon-not-up';
}

/**
 * Per-band moon-free imaging minutes (US5, FR-022): Σ dark-window intervals
 * where the target clears the usable altitude AND NOT (Moon above the
 * horizon AND separation below that band's Lorentzian minimum for the
 * night's Moon age). Zero for every band when there is no dark window
 * (FR-017), OR when Moon geometry wasn't computed for this `night`
 * (`computeNightObservability`'s `includeMoonGeometry=false` fast path,
 * `night.moonSamples` empty) — "not computed" must degrade to zero, NOT to
 * "no interference found" (which `nearestMoonSample` returning `null` would
 * otherwise silently imply): never fabricated.
 */
function moonFreeMinutesByBand(
  night: NightObservability,
  usableAltitudeDeg: number,
  minHorizonAltDeg: number,
  params: MoonAvoidanceParams,
): Record<Band, number> {
  const dark = night.darkWindow;
  if (!dark || night.moonSamples.length === 0) return { ...ZERO_BY_BAND };

  const out: Record<Band, number> = { ...ZERO_BY_BAND };
  for (const s of night.samples) {
    if (s.tMs < dark.startMs || s.tMs > dark.endMs) continue;
    if (s.altDeg < usableAltitudeDeg) continue;
    const moon = nearestMoonSample(night.moonSamples, s.tMs);
    const moonUp = moon !== null && moon.moonAltDeg >= minHorizonAltDeg;
    for (const band of BANDS) {
      const interfering =
        moonUp &&
        moon !== null &&
        moon.separationDeg <
          minSeparationDeg(band, night.moonAgeFromFullDays, params);
      if (!interfering) out[band] += GRID_STEP_MINUTES;
    }
  }
  return out;
}

/**
 * OSC single-pass imaging minutes (iteration 2026-07-15, FR-036): one
 * exposure captures every band in the passband simultaneously, so viability
 * must satisfy the strictest band — the required separation is
 * `max over band in passband of minSeparationDeg(band, age, params)`
 * (Track A's rule consumed verbatim, aggregated on this Track-B side; spec
 * 047 FR-020). Returns `null` (unknown, never fabricated) when Moon geometry
 * wasn't computed; 0 when there's no dark window.
 */
function oscSinglePassMinutesFor(
  night: NightObservability,
  usableAltitudeDeg: number,
  minHorizonAltDeg: number,
  params: MoonAvoidanceParams,
  passband: OscPassband,
): number | null {
  if (night.moonSamples.length === 0) return null;
  const dark = night.darkWindow;
  if (!dark) return 0;

  // A plain color camera captures broadband; LRGB share params so the max is
  // well-defined either way (FR-035's 'rgb' passband).
  const bands: readonly Band[] =
    passband === 'rgb' ? BROADBAND_BANDS : passband;
  if (bands.length === 0) return null;
  let effectiveMinSepDeg = 0;
  for (const band of bands) {
    const sep = minSeparationDeg(band, night.moonAgeFromFullDays, params);
    if (sep > effectiveMinSepDeg) effectiveMinSepDeg = sep;
  }

  let minutes = 0;
  for (const s of night.samples) {
    if (s.tMs < dark.startMs || s.tMs > dark.endMs) continue;
    if (s.altDeg < usableAltitudeDeg) continue;
    const moon = nearestMoonSample(night.moonSamples, s.tMs);
    const interfering =
      moon !== null &&
      moon.moonAltDeg >= minHorizonAltDeg &&
      moon.separationDeg < effectiveMinSepDeg;
    if (!interfering) minutes += GRID_STEP_MINUTES;
  }
  return minutes;
}

/**
 * Moon-excluded spans for ONE band across the whole night (iteration
 * 2026-07-15, FR-007's detail-graph overlay): contiguous sample runs where
 * the Moon is up (≥ `minHorizonAltDeg`) AND its separation is below the
 * band's Lorentzian minimum for the night's Moon age. Empty when Moon
 * geometry wasn't computed (`night.moonSamples` empty) — "not computed"
 * must render as no overlay, never as a fabricated exclusion.
 */
export function moonExcludedSpans(
  night: NightObservability,
  band: Band,
  minHorizonAltDeg: number,
  params: MoonAvoidanceParams,
): Array<{ startMs: number; endMs: number }> {
  if (night.moonSamples.length === 0) return [];
  const spans: Array<{ startMs: number; endMs: number }> = [];
  let open: { startMs: number; endMs: number } | null = null;
  for (const s of night.samples) {
    const moon = nearestMoonSample(night.moonSamples, s.tMs);
    const excluded =
      moon !== null &&
      moon.moonAltDeg >= minHorizonAltDeg &&
      moon.separationDeg <
        minSeparationDeg(band, night.moonAgeFromFullDays, params);
    if (excluded) {
      if (open) open.endMs = s.tMs;
      else open = { startMs: s.tMs, endMs: s.tMs };
    } else if (open) {
      spans.push(open);
      open = null;
    }
  }
  if (open) spans.push(open);
  return spans;
}

/** Best-imaging date (US2, FR-009) — thin wrapper reusing Track A's `nextOpposition` (no second search). */
function deriveBestDate(
  raDegJ2000: number | null | undefined,
  fromMs: number,
): BestImagingDate | null {
  const result = nextOpposition(raDegJ2000, new Date(fromMs));
  return result
    ? { dateMs: result.date.getTime(), inDays: result.daysUntil }
    : null;
}

// ── Phase helpers for deriveObservability ────────────────────────────────────

interface AltitudeStats {
  maxAltDeg: number;
  usableSamples: number;
  uptimeSamples: number;
  visibleTonight: boolean;
}

/**
 * Phase 1: scan all night samples to derive pure altitude stats
 * (peak altitude, dark-gated imaging sample count, whole-night uptime count,
 * and whether the target clears `usableAltitudeDeg` in the observable window).
 *
 * Observable window is the astronomical dark window when one exists, else the
 * whole night (#579): at high latitude (e.g. lat 52 in summer) there is no
 * astronomical darkness for months, but a high/circumpolar target is still
 * observable in twilight and MUST NOT read identically to a target that never
 * rises. Imaging time stays dark-gated (FR-017); only the visibility flag falls
 * back. Uptime (FR-005/D1) counts the whole night's above-threshold samples.
 */
export function sampleAltitudeStats(
  night: NightObservability,
  usableAltitudeDeg: number,
): AltitudeStats {
  let maxAltDeg = Number.NEGATIVE_INFINITY;
  for (const s of night.samples) {
    if (s.altDeg > maxAltDeg) maxAltDeg = s.altDeg;
  }
  if (!Number.isFinite(maxAltDeg)) maxAltDeg = 0;

  const dark = night.darkWindow;
  const observable = dark ?? {
    startMs: night.nightStartMs,
    endMs: night.nightEndMs,
  };
  let usableSamples = 0;
  let uptimeSamples = 0;
  let visibleTonight = false;
  for (const s of night.samples) {
    if (s.altDeg < usableAltitudeDeg) continue;
    uptimeSamples += 1;
    if (dark && s.tMs >= dark.startMs && s.tMs <= dark.endMs) {
      usableSamples += 1;
    }
    if (s.tMs >= observable.startMs && s.tMs <= observable.endMs) {
      visibleTonight = true;
    }
  }
  return { maxAltDeg, usableSamples, uptimeSamples, visibleTonight };
}

/**
 * Phase 2: build the three target↔Moon separation reference figures (US5,
 * FR-020) from already-computed `NightObservability` moon samples — no fresh
 * astronomy-engine calls.
 */
export function separationScalarsFor(
  night: NightObservability,
  minHorizonAltDeg: number,
): SeparationScalars {
  return {
    atTransitDeg: night.transit
      ? separationAt(night.moonSamples, night.transit.tMs, minHorizonAltDeg)
      : 'moon-not-up',
    minOverDarkDeg: minSeparationOverDark(
      night.moonSamples,
      night.darkWindow,
      minHorizonAltDeg,
    ),
    atDarkMidpointDeg: night.darkWindow
      ? separationAt(
          night.moonSamples,
          (night.darkWindow.startMs + night.darkWindow.endMs) / 2,
          minHorizonAltDeg,
        )
      : 'moon-not-up',
  };
}

/**
 * Phase 3: derive the FR-029 reason-for-zero with precedence
 * darkness > altitude > moon. Returns `null` when imaging time is non-zero
 * and at least one band is moon-viable.
 *
 * `moonComputed` must be `night.moonSamples.length > 0` — "not computed"
 * never reads as "moon-limited" (fabrication guard).
 */
export function zeroImagingReasonFor(
  dark: TimeWindow | null,
  totalImagingMinutes: number,
  moonComputed: boolean,
  moonFree: Record<Band, number>,
): ZeroImagingReason | null {
  if (!dark) return 'darkness';
  if (totalImagingMinutes === 0) return 'altitude';
  if (moonComputed && BANDS.every((b) => moonFree[b] === 0)) return 'moon';
  return null;
}

/**
 * Derive the threshold-dependent scalars from cached positions. Pure and cheap —
 * safe to call on every threshold change (SC-003). Moon/best-date integration
 * (`options`) reads only already-computed `NightObservability` fields — no
 * fresh astronomy-engine calls happen here.
 */
export function deriveObservability(
  night: NightObservability,
  usableAltitudeDeg: number,
  options: DeriveOptions = {},
): DerivedObservability {
  const { maxAltDeg, usableSamples, uptimeSamples, visibleTonight } =
    sampleAltitudeStats(night, usableAltitudeDeg);

  const minHorizonAltDeg = options.minHorizonAltDeg ?? 0;
  const params = options.moonAvoidanceParams ?? DEFAULT_MOON_AVOIDANCE;

  const separationScalars = separationScalarsFor(night, minHorizonAltDeg);

  const totalImagingMinutes = usableSamples * GRID_STEP_MINUTES;
  const moonFree = moonFreeMinutesByBand(
    night,
    usableAltitudeDeg,
    minHorizonAltDeg,
    params,
  );

  // FR-031: bands whose moon-viable window is strictly smaller than
  // dark ∩ uptime. Guarded on Moon geometry actually having been computed —
  // the not-computed all-zero degrade must not read as "every band limited".
  const moonComputed = night.moonSamples.length > 0;
  const moonLimitedBands = moonComputed
    ? BANDS.filter((b) => moonFree[b] < totalImagingMinutes)
    : [];

  const zeroImagingReason = zeroImagingReasonFor(
    night.darkWindow,
    totalImagingMinutes,
    moonComputed,
    moonFree,
  );

  const sensor = options.sensorConfig;
  const oscSinglePassMinutes =
    sensor?.sensorType === 'osc'
      ? oscSinglePassMinutesFor(
          night,
          usableAltitudeDeg,
          minHorizonAltDeg,
          params,
          sensor.passband ?? 'rgb',
        )
      : null;

  return {
    maxAltDeg,
    visibleTonight,
    totalImagingMinutes,
    bestDate: deriveBestDate(
      options.raDegJ2000,
      options.bestDateFromMs ?? night.nightStartMs,
    ),
    separationScalars,
    moonFreeMinutesByBand: moonFree,
    uptimeMinutes: uptimeSamples * GRID_STEP_MINUTES,
    zeroImagingReason,
    moonLimitedBands,
    oscSinglePassMinutes,
  };
}

// ── Position memoization per (target, site, date) ────────────────────────────

interface CacheEntry {
  key: string;
  night: NightObservability;
}

const CACHE_LIMIT = 2000;
const cache = new Map<string, CacheEntry>();

/** The night is keyed by the day of the date, not the exact instant. */
function dayKey(dateMs: number): string {
  return new Date(dateMs).toISOString().slice(0, 10);
}

function siteKey(site: ObserverSite): string {
  // Everything that changes the positions: id + coordinates + horizon/twilight.
  return [
    site.id,
    site.latitudeDeg,
    site.longitudeDeg,
    site.elevationM ?? 'n',
    site.twilight,
    site.minHorizonAltDeg,
  ].join('|');
}

/**
 * Cached night observability for a target at a site/date. Positions are computed
 * once per `(targetId, site, day, includeMoonGeometry)`; repeated calls (e.g.
 * threshold drags) reuse the cached result so only `deriveObservability`
 * re-runs (SC-003).
 *
 * @param includeMoonGeometry - See `computeNightObservability`'s doc — pass
 *   `false` for a full-catalogue sort/group pass (cheap: target altitude
 *   only) and `true` only for the rows actually being displayed (list-row
 *   GuidanceCell, detail pane). Part of the cache key: a `false` request
 *   never returns a stale/incomplete `true`-shaped entry or vice versa.
 */
export function getNightObservability(
  targetId: string,
  raDegJ2000: number,
  decDegJ2000: number,
  site: ObserverSite,
  dateMs: number,
  includeMoonGeometry = true,
): NightObservability {
  const key = `${targetId}@${siteKey(site)}@${dayKey(dateMs)}@${includeMoonGeometry ? 'moon' : 'nomoon'}`;
  const hit = cache.get(key);
  if (hit) return hit.night;

  const night = computeNightObservability(
    raDegJ2000,
    decDegJ2000,
    site,
    dateMs,
    includeMoonGeometry,
  );
  if (cache.size >= CACHE_LIMIT) {
    // Simple bound: drop the oldest insertion.
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(key, { key, night });
  return night;
}

/** Clear the position cache (tests, or when the target catalog changes). */
export function clearObservabilityCache(): void {
  cache.clear();
}
