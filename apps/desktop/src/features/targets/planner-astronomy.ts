// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-astronomy.ts — real per-night observability engine (spec 044 Track B, US1 T009).
 *
 * Replaces the deterministic placeholder astronomy in the planner with a real,
 * per-site, per-date ephemeris computed entirely on the frontend via
 * `astronomy-engine` (MIT, offline, ±1′ — ADR-0001). Given a target's J2000
 * RA/Dec, an `ObserverSite`, and a date, it computes the target's altitude over
 * the night on a fixed 10-minute grid, its exact transit / rise / set, and the
 * site's astronomical/nautical dark window. All computation is offline/local —
 * no network (FR-027).
 *
 * Precession (FR-026): J2000 coordinates are precessed to of-date via
 * `Equator(..., ofdate=true)` before `Horizon()`, so the ~20′ 2000→2026 drift is
 * not dropped the way the old mock did.
 *
 * US5 (T027): also computes the Moon time-series — altitude(t) + target↔Moon
 * separation(t) aligned to the same grid, and the Moon-up windows (∩ the dark
 * window, horizon-aware — T032). Per SC-013 (no duplicate Moon-geometry
 * computation between the tracks), the per-sample separation reuses Track A's
 * exact vector math (`astro/lunar-separation.ts`'s `targetUnitVector`/
 * `angleBetweenDeg` against `GeoVector(Body.Moon, …)`, the same frame
 * `astro/moon-state.ts` uses) rather than a second implementation, and the
 * single-instant illumination/Moon-age carried on `NightObservability` is
 * Track A's own `moonStateAt` evaluated at this night's dark-window midpoint
 * (Track A only evaluates "tonight"; Track B needs an arbitrary planned date,
 * so it calls the same function at its own reference instant — still ONE
 * implementation, not a fork). The per-band Lorentzian rule itself
 * (`minSeparationDeg`) is consumed, never redefined, in `planner-derive.ts`
 * (FR-022/FR-023).
 *
 * The anti-solar best-imaging date (US2/FR-009) is likewise NOT recomputed
 * here — it is the exact same anti-solar-RA search Track A already ships as
 * `astro/opposition.ts`'s `nextOpposition` (a fixed-RA target's "transits at
 * local midnight" date IS its opposition-style date); `planner-derive.ts`
 * calls it directly instead of a second search implementation.
 *
 * Pure/offline compute; no React. Memoization is `planner-derive.ts`'s job.
 */

import {
  AngleBetween,
  Body,
  DefineStar,
  Equator,
  GeoVector,
  Horizon,
  Observer,
  SearchAltitude,
  SearchHourAngle,
  SearchRiseSet,
} from 'astronomy-engine';
import type { ObserverSite } from '@/shared/observing-sites/observer-site';
import { angleBetweenDeg, targetUnitVector } from './astro/lunar-separation';
import { moonStateAt } from './astro/moon-state';

/** Fixed grid step across the night (minutes). */
export const GRID_STEP_MINUTES = 10;
const MS_PER_MIN = 60_000;
const MS_PER_HOUR = 3_600_000;

/** Twilight Sun-depression angle for a site's night definition (degrees). */
function twilightDepressionDeg(site: ObserverSite): number {
  return site.twilight === 'nautical' ? -12 : -18;
}

/** One sampled point of the target's altitude/azimuth over the night. */
export interface AltAzSample {
  /** Epoch ms (UTC) of the sample. */
  tMs: number;
  /** Apparent altitude in degrees (refraction applied). */
  altDeg: number;
  /** Azimuth in degrees (0 = north, clockwise). */
  azDeg: number;
}

/** An instant with the target's altitude at that instant. */
export interface AltEvent {
  tMs: number;
  altDeg: number;
}

/** A time window in epoch ms. */
export interface TimeWindow {
  startMs: number;
  endMs: number;
}

/** One sampled point of the Moon's altitude + target↔Moon separation (US5, FR-019). */
export interface MoonSample {
  tMs: number;
  /** Moon's apparent altitude in degrees (refraction applied). */
  moonAltDeg: number;
  /** Target↔Moon angular separation in degrees (0…180). */
  separationDeg: number;
}

/** Per-target, per-site, per-date observability (US1 surface — data-model.md §2). */
export interface NightObservability {
  /** The night span the grid covers (sunset→sunrise, or a fallback window). */
  nightStartMs: number;
  nightEndMs: number;
  /** 10-minute altitude/az grid across the night (FR-001). */
  samples: AltAzSample[];
  /** Exact culmination via `SearchHourAngle(0)` (FR-002); null if not found. */
  transit: AltEvent | null;
  /** Exact rise/set respecting `minHorizonAltDeg` + refraction; null = circumpolar/never-rising (FR-003). */
  rise: AltEvent | null;
  set: AltEvent | null;
  /** Dark window from the site twilight depression; null when no dark exists (FR-017). */
  darkWindow: TimeWindow | null;
  /** Moon altitude + target-Moon separation, aligned 1:1 with `samples` (US5, FR-019). */
  moonSamples: MoonSample[];
  /** Contiguous Moon-above-`minHorizonAltDeg` intervals ∩ the dark window (US5, FR-021). */
  moonUpWindows: TimeWindow[];
  /** Illuminated Moon fraction [0,1] for the night, from Track A's `moonStateAt` (carried for display). */
  moonIllumination: number;
  /** Days from full Moon at the same reference instant as `moonIllumination` — the Lorentzian input (FR-022). */
  moonAgeFromFullDays: number;
}

const REFRACTION = 'normal';
const STAR_DISTANCE_LY = 1000;

/** Build an astronomy-engine observer from a site. */
function observerFor(site: ObserverSite): Observer {
  return new Observer(
    site.latitudeDeg,
    site.longitudeDeg,
    site.elevationM ?? 0,
  );
}

/** Of-date apparent horizontal coordinates of the currently-defined Star1. */
function starHorizonAt(
  date: Date,
  observer: Observer,
): { altDeg: number; azDeg: number } {
  // Precess J2000 → of-date (FR-026), then to horizontal with refraction.
  const eq = Equator(
    Body.Star1,
    date,
    observer,
    /*ofdate*/ true,
    /*aberration*/ false,
  );
  const hor = Horizon(date, observer, eq.ra, eq.dec, REFRACTION);
  return { altDeg: hor.altitude, azDeg: hor.azimuth };
}

/**
 * Resolve the night span (sunset→sunrise) bracketing the solar midnight nearest
 * the given date. Falls back to a fixed ±6h window around solar midnight when
 * the Sun does not rise/set (polar day/night) so the grid is never empty.
 */
function nightSpan(
  observer: Observer,
  dateMs: number,
): { startMs: number; endMs: number } {
  const t0 = new Date(dateMs);
  // Solar noon of the day containing `dateMs` (search from 12h before).
  const noon = SearchHourAngle(
    Body.Sun,
    observer,
    0,
    new Date(dateMs - 12 * MS_PER_HOUR),
    +1,
  );
  const sunset = SearchRiseSet(Body.Sun, observer, -1, noon.time.date, 1);
  const sunrise = sunset
    ? SearchRiseSet(Body.Sun, observer, +1, sunset.date, 1)
    : null;
  if (sunset && sunrise) {
    return { startMs: sunset.date.getTime(), endMs: sunrise.date.getTime() };
  }
  // Polar fallback: centre a fixed window on solar midnight (lower transit).
  const midnight = SearchHourAngle(Body.Sun, observer, 12, noon.time.date, +1);
  const midMs = midnight.time.date.getTime();
  // Guard against an anchor far from the requested date.
  const anchor =
    Math.abs(midMs - t0.getTime()) < 36 * MS_PER_HOUR ? midMs : t0.getTime();
  return { startMs: anchor - 6 * MS_PER_HOUR, endMs: anchor + 6 * MS_PER_HOUR };
}

/** Compute the site's dark window (Sun below twilight depression) within the night. */
function darkWindowFor(
  observer: Observer,
  site: ObserverSite,
  nightStartMs: number,
): TimeWindow | null {
  const depression = twilightDepressionDeg(site);
  const dusk = SearchAltitude(
    Body.Sun,
    observer,
    -1,
    new Date(nightStartMs),
    1,
    depression,
  );
  if (!dusk) return null;
  const dawn = SearchAltitude(Body.Sun, observer, +1, dusk.date, 1, depression);
  if (!dawn) return null;
  return { startMs: dusk.date.getTime(), endMs: dawn.date.getTime() };
}

/**
 * Find the target's rise or set relative to the night, respecting the site's
 * minimum-horizon altitude (SearchAltitude when > 0) or the true horizon with
 * refraction (SearchRiseSet when 0). Returns null for circumpolar / never-rising.
 */
function riseSetFor(
  observer: Observer,
  site: ObserverSite,
  direction: 1 | -1,
  searchStartMs: number,
): AltEvent | null {
  const start = new Date(searchStartMs);
  const event =
    site.minHorizonAltDeg > 0
      ? SearchAltitude(
          Body.Star1,
          observer,
          direction,
          start,
          1,
          site.minHorizonAltDeg,
        )
      : SearchRiseSet(Body.Star1, observer, direction, start, 1);
  if (!event) return null;
  const { altDeg } = starHorizonAt(event.date, observer);
  return { tMs: event.date.getTime(), altDeg };
}

/**
 * Compute the night's observability for a fixed target at a site and date.
 *
 * @param raDegJ2000 - ICRS/J2000 right ascension in decimal degrees.
 * @param decDegJ2000 - ICRS/J2000 declination in decimal degrees.
 * @param site - The active observing site.
 * @param dateMs - Any epoch-ms instant on the observing day ("tonight" = now).
 * @param includeMoonGeometry - Whether to also compute the Moon time-series
 *   (US5, T027). Defaults to `true`. Callers that need only the target's own
 *   altitude (e.g. a full-catalogue sort/group pass over thousands of rows —
 *   `TargetsTable.tsx`'s per-render `useMemo`) MUST pass `false`: the Moon
 *   series adds ~3 extra astronomy-engine calls per 10-min sample on top of
 *   the target's own, and multiplying that by a large catalogue is a real
 *   perf cliff (found via a Layer-2 real-UI CI timeout — the Real-UI E2E leg
 *   computes this for the full ~13k-entry bundled seed catalogue, not just
 *   the visible/windowed rows). `false` returns empty/zeroed Moon fields —
 *   never a fabricated non-zero value; see `planner-derive.ts`'s
 *   `moonFreeMinutesByBand` guard for how callers must treat that as "not
 *   computed", not "no interference".
 */
export function computeNightObservability(
  raDegJ2000: number,
  decDegJ2000: number,
  site: ObserverSite,
  dateMs: number,
  includeMoonGeometry = true,
): NightObservability {
  const observer = observerFor(site);
  // Define the fixed target as Star1: RA in sidereal hours, Dec in degrees, J2000.
  DefineStar(Body.Star1, raDegJ2000 / 15, decDegJ2000, STAR_DISTANCE_LY);
  // The target's J2000 unit vector is fixed for the whole night — computed once
  // (Track A's `lunar-separation.ts` geocentric-vector approach, ±2° tolerance
  // per its own doc; no precession needed at this tolerance for a separation
  // angle, unlike the precessed of-date altitude/azimuth above).
  const targetVec = targetUnitVector(raDegJ2000, decDegJ2000);

  const { startMs, endMs } = nightSpan(observer, dateMs);

  // 10-minute altitude/az grid across the night, plus (when requested) the
  // aligned Moon altitude + target-Moon separation series (US5, FR-019).
  const samples: AltAzSample[] = [];
  const moonSamples: MoonSample[] = [];
  const step = GRID_STEP_MINUTES * MS_PER_MIN;
  for (let tMs = startMs; tMs <= endMs; tMs += step) {
    const date = new Date(tMs);
    const { altDeg, azDeg } = starHorizonAt(date, observer);
    samples.push({ tMs, altDeg, azDeg });

    if (includeMoonGeometry) {
      // Moon apparent topocentric altitude at the site (of-date, aberration-
      // corrected — matches how a real body, as opposed to the fixed Star1
      // proxy, is normally evaluated with astronomy-engine).
      const moonEq = Equator(
        Body.Moon,
        date,
        observer,
        /*ofdate*/ true,
        /*aberration*/ true,
      );
      const moonHor = Horizon(
        date,
        observer,
        moonEq.ra,
        moonEq.dec,
        REFRACTION,
      );
      // Separation reuses Track A's exact geocentric vector math (SC-013).
      const moonGeoVec = GeoVector(Body.Moon, date, true);
      const separationDeg = angleBetweenDeg(targetVec, moonGeoVec);
      moonSamples.push({ tMs, moonAltDeg: moonHor.altitude, separationDeg });
    }
  }

  // Exact transit nearest the night (search from 1h before night start).
  let transit: AltEvent | null;
  try {
    const event = SearchHourAngle(
      Body.Star1,
      observer,
      0,
      new Date(startMs - MS_PER_HOUR),
      +1,
    );
    transit = { tMs: event.time.date.getTime(), altDeg: event.hor.altitude };
  } catch {
    transit = null;
  }

  // Exact rise/set relative to the night (search from 6h before night start).
  // IMPORTANT: `set` must be searched forward FROM the rise instant, not
  // independently from the same `searchStartMs` — searching both directions
  // from the same anchor can pair a `rise` that starts the pass containing the
  // night with a `set` that is the tail end of the PREVIOUS pass (already
  // above the horizon at `searchStartMs`), producing a `set` that precedes
  // `rise` for periodic sources. Anchoring `set`'s search on `rise` guarantees
  // both bracket the same above-horizon pass.
  const searchStartMs = startMs - 6 * MS_PER_HOUR;
  const rise = riseSetFor(observer, site, +1, searchStartMs);
  const set = riseSetFor(observer, site, -1, rise ? rise.tMs : searchStartMs);

  const darkWindow = darkWindowFor(observer, site, startMs);

  // Moon-up windows: Moon above the site's minimum-horizon altitude,
  // intersected with the dark window (US5/US4, FR-021, T032). Empty when Moon
  // geometry wasn't requested (moonSamples is empty in that case too).
  const moonUpWindows = includeMoonGeometry
    ? moonUpWindowsFor(moonSamples, darkWindow, site.minHorizonAltDeg)
    : [];

  // Single reference-instant Moon state (illumination + age) for this planned
  // night, via Track A's own `moonStateAt` (SC-013 — reuse, not a fork).
  // Track A only ever evaluates "tonight"; here the reference instant is this
  // NIGHT's own dark-window midpoint (or the night midpoint when there is no
  // dark window), so an arbitrary planned date (US2) gets a correct age/
  // illumination instead of today's. Skipped (0/0) when Moon geometry wasn't
  // requested — one extra astronomy-engine call saved per skipped target.
  const moonRef = includeMoonGeometry
    ? moonStateAt(
        new Date(
          darkWindow
            ? (darkWindow.startMs + darkWindow.endMs) / 2
            : (startMs + endMs) / 2,
        ),
      )
    : { illuminationFrac: 0, moonAgeFromFullDays: 0 };

  return {
    nightStartMs: startMs,
    nightEndMs: endMs,
    samples,
    transit,
    rise,
    set,
    darkWindow,
    moonSamples,
    moonUpWindows,
    moonIllumination: moonRef.illuminationFrac,
    moonAgeFromFullDays: moonRef.moonAgeFromFullDays,
  };
}

/**
 * Contiguous Moon-above-`minHorizonAltDeg` intervals within the dark window
 * (US5, FR-021; horizon-aware per T032). `null` dark window (no dark exists,
 * FR-017) yields no windows — there is no imaging time to protect.
 */
function moonUpWindowsFor(
  moonSamples: MoonSample[],
  darkWindow: TimeWindow | null,
  minHorizonAltDeg: number,
): TimeWindow[] {
  if (!darkWindow) return [];
  const windows: TimeWindow[] = [];
  let openStartMs: number | null = null;
  for (const s of moonSamples) {
    if (s.tMs < darkWindow.startMs || s.tMs > darkWindow.endMs) continue;
    const up = s.moonAltDeg >= minHorizonAltDeg;
    if (up && openStartMs === null) openStartMs = s.tMs;
    if (!up && openStartMs !== null) {
      windows.push({ startMs: openStartMs, endMs: s.tMs });
      openStartMs = null;
    }
  }
  if (openStartMs !== null)
    windows.push({ startMs: openStartMs, endMs: darkWindow.endMs });
  return windows;
}

/**
 * Real (non-mock) instantaneous angular separation between a fixed J2000
 * target and the Moon, at a single instant — **test-only reference
 * implementation, no production callers** (spec 033 T087c dead-code audit,
 * 2026-07-11).
 *
 * Topocentric (parallax-corrected via `Equator(..., ofdate=false)` against
 * the real observer), unlike the geocentric formula production actually uses
 * (`astro/lunar-separation.ts`'s `targetUnitVector`/`angleBetweenDeg` against
 * `GeoVector`, per this module's SC-013 note above — the ONE production
 * implementation, deliberately not duplicated). This function predates that
 * consolidation: it was the original US1 "lunar distance" placeholder
 * replacement, later superseded by the SC-013 shared vector math.
 *
 * Kept (not deleted) because `planner-astronomy.test.ts`'s
 * `angularSeparationFromMoonDeg` describe block uses it as an independent,
 * externally-anchored (JPL Horizons astrometric RA/Dec) cross-check that the
 * topocentric/geocentric gap stays within the documented tolerance
 * (`SC-002`'s ±2° planning tolerance) — the accompanying "T027 per-sample
 * separation formula" test right after it performs the equivalent
 * external-ephemeris check against the actual production formula, so between
 * the two, both the production formula AND the tolerance-of-approximation
 * claim are independently verified.
 */
export function angularSeparationFromMoonDeg(
  raDegJ2000: number,
  decDegJ2000: number,
  site: ObserverSite,
  dateMs: number,
): number {
  const observer = observerFor(site);
  const date = new Date(dateMs);
  DefineStar(Body.Star1, raDegJ2000 / 15, decDegJ2000, STAR_DISTANCE_LY);
  const targetEq = Equator(
    Body.Star1,
    date,
    observer,
    /*ofdate*/ false,
    /*aberration*/ false,
  );
  const moonEq = Equator(
    Body.Moon,
    date,
    observer,
    /*ofdate*/ false,
    /*aberration*/ false,
  );
  return AngleBetween(targetEq.vec, moonEq.vec);
}
