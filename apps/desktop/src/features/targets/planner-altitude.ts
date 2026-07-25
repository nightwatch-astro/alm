// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-altitude.ts — per-row tonight observability for the Planner table +
 * detail pane (spec 044 Track B, T011).
 *
 * Prior versions of this module (tasks #84/#85, mock spec 044 §3) derived a
 * deterministic pseudo-curve from a hash of the target's designation at a
 * fixed placeholder latitude. That mock is now replaced with real per-site,
 * per-date computation via `planner-astronomy.ts` (astronomy-engine, offline) +
 * `planner-derive.ts` (cached positions, pure threshold derivation — SC-003).
 *
 * `RowAltitude` gains two degrade flags for the edge cases in T013:
 *   - `needsCoordinates`: the target has no RA/Dec (never resolved / manual
 *     entry without coordinates) — no astronomy is possible.
 *   - `needsSite`: there is no active observing site (US6 no-site state) — no
 *     astronomy is possible until the user adds/activates a site.
 * In either degrade state the row reports zero imaging time / not visible,
 * with NO thrown error (FR-024/SC-011, T013).
 *
 * Moon geometry — real lunar distance, per-band filter guidance, and next
 * opposition — is spec 047 Track A and lives in `astro/row-planning.ts`
 * (`RowMoonPlanning`, computed from the shared `ObservingNight` + catalogued
 * RA/Dec), NOT in this module. This module owns tonight altitude / imaging time.
 *
 * US2/US5 (T024/T027-T029): also exposes `bestDate` (FR-009), the three real
 * target↔Moon separation scalars, and per-band moon-free minutes for the
 * chosen `(site, date)` — thin pass-through of `planner-derive.ts`'s
 * `DerivedObservability`, not a second computation.
 */

import type { TargetListItem } from '@/bindings/index';
import type { ObserverSite } from './observing-sites/observer-site';
import { activeSite } from './observing-sites/site-store';
import {
  deriveObservability,
  getNightObservability,
  moonExcludedSpans,
  UNKNOWN_SEPARATION_SCALARS,
  type BestImagingDate,
  type SensorConfig,
  type SeparationScalars,
  type ZeroImagingReason,
} from './planner-derive';
import {
  BANDS,
  DEFAULT_MOON_AVOIDANCE,
  type Band,
  type MoonAvoidanceParams,
} from './astro/moon-avoidance';

/**
 * Default usable-altitude threshold (degrees above horizon for imaging).
 * Overridable via Settings → Target Planner; callers should prefer the
 * user-configured value from `observing-sites/site-store.ts` (settings-backed,
 * T012b) over this constant.
 */
export const USABLE_ALT_DEG = 30;

/** One sampled point of the night's altitude curve. */
export interface AltPoint {
  /** Hours into the night (0 = night start … night end). */
  tHour: number;
  /** Altitude in degrees (−90…+90), refraction-corrected. */
  altDeg: number;
}

/** Summary of a row's tonight visibility, derived from the real ephemeris. */
export interface RowAltitude {
  points: AltPoint[];
  /** Peak altitude across the night (deg). 0 in the degrade states. */
  maxAltDeg: number;
  /**
   * Hours of the (astronomically) dark window the target sits above the
   * caller's usable-altitude threshold (default USABLE_ALT_DEG = 30°;
   * overridable via Settings).
   */
  hoursAboveUsable: number;
  /** True when the target reaches usable altitude at any dark-window sample. */
  visibleTonight: boolean;
  /** T013: true when the target has no RA/Dec — no astronomy is possible. */
  needsCoordinates: boolean;
  /** T013/US6: true when there is no active observing site. */
  needsSite: boolean;
  /** Date the target transits at local midnight (US2, FR-009); `null` = unknown coordinates. */
  bestDate: BestImagingDate | null;
  /** Three real target↔Moon separation reference figures (US5, FR-020). */
  separationScalars: SeparationScalars;
  /** Per-band moon-free imaging minutes for the chosen night (US5, FR-022). */
  moonFreeMinutesByBand: Record<Band, number>;
  /**
   * US4/T033: true when the site/date has no qualifying dark window under the
   * chosen twilight (e.g. high-latitude summer) — total/per-band imaging time
   * is correctly zero, but the UI MUST disclose the reason rather than
   * implying the target is simply too low (FR-017). Always `false` in the
   * degrade states (no astronomy is attempted there at all).
   */
  noDarkWindow: boolean;
  /**
   * The dark window's `[startHour, endHour]` on the SAME `tHour` axis as
   * `points` (T035: lets the detail-pane graph shade twilight vs dark);
   * `null` when there is no dark window (`noDarkWindow`) or in the degrade
   * states.
   */
  darkWindowHours: { startHour: number; endHour: number } | null;
  /**
   * Hours the target sits above the usable altitude across the WHOLE night
   * (the D1 uptime window — NOT dark-gated; distinguishable from
   * `hoursAboveUsable` per FR-005). 0 in the degrade states.
   */
  uptimeHours: number;
  /** Stated blocker when imaging time is (effectively) zero (FR-029); `null` otherwise / degrade. */
  zeroImagingReason: ZeroImagingReason | null;
  /** Bands the Moon actionably limits tonight (FR-031); empty when not computed / degrade. */
  moonLimitedBands: Band[];
  /**
   * OSC single-pass imaging hours (FR-036); `null` for mono/unknown sensors,
   * when Moon geometry wasn't computed, and in the degrade states.
   */
  oscSinglePassHours: number | null;
}

const ZERO_BY_BAND: Record<Band, number> = Object.fromEntries(
  BANDS.map((b) => [b, 0]),
) as Record<Band, number>;

const DEGRADE_ROW: Omit<RowAltitude, 'needsCoordinates' | 'needsSite'> = {
  points: [],
  maxAltDeg: 0,
  hoursAboveUsable: 0,
  visibleTonight: false,
  bestDate: null,
  separationScalars: UNKNOWN_SEPARATION_SCALARS,
  moonFreeMinutesByBand: ZERO_BY_BAND,
  noDarkWindow: false,
  darkWindowHours: null,
  uptimeHours: 0,
  zeroImagingReason: null,
  moonLimitedBands: [],
  oscSinglePassHours: null,
};

/** A minimal shape sufficient to compute tonight observability (T012 fallback reuse). */
export interface AltitudeSubject {
  id: string;
  raDeg: number | null;
  decDeg: number | null;
}

/**
 * Compute the real tonight-altitude summary for a target at a site/date.
 *
 * Degrades cleanly (T013, no throw) when coordinates or a site are missing:
 * returns the zero/not-visible `DEGRADE_ROW` shape with the appropriate
 * `needsCoordinates`/`needsSite` flag set.
 *
 * @param subject - Anything with an id + RA/Dec (a `TargetListItem`, a
 *   `TargetDetailV3`, or a synthesized minimal object).
 * @param usableAltDeg - Altitude threshold for "usable for imaging" in
 *   degrees. Prefer the settings-backed value from `site-store.ts`
 *   (`useUsableAltitude()` / `getUsableAltitude()`) over the USABLE_ALT_DEG
 *   default.
 * @param site - The observer site to compute against. Defaults to the
 *   currently active site (`site-store.ts`); pass `null` explicitly to force
 *   the no-site degrade state.
 * @param dateMs - Any epoch-ms instant on the observing night. Defaults to now
 *   ("tonight"); pass the Planner's chosen date (`planner-date-store.ts`,
 *   US2/T024) to plan an arbitrary future night.
 * @param moonAvoidanceParams - Active per-band Moon-avoidance parameters
 *   (Track A, Settings → Target Planner) for `moonFreeMinutesByBand` (US5).
 *   Defaults to the shipped table; prefer `useGuidanceParams()` from the host
 *   page so a live settings edit recomputes moon-free hours (SC-008).
 * @param includeMoonGeometry - Pass `false` for a full-catalogue sort/group
 *   pass (`TargetsTable.tsx`'s pre-virtualization `useMemo`, potentially
 *   thousands of rows) — computing the Moon time-series for every row there
 *   is a real perf cliff (found via a Layer-2 CI timeout against the full
 *   ~13k-entry bundled seed catalogue). Defaults to `true` (single-target
 *   callers: `TargetDetail`, and the per-visible-row recompute for
 *   `GuidanceCell`). `false` zeroes `separationScalars`/`moonFreeMinutesByBand`
 *   (never fabricating a non-zero value) — `bestDate` is unaffected, it's
 *   cheap (Sun-RA-table based) and always computed regardless of this flag.
 */
export function altitudeFor(
  subject: AltitudeSubject,
  usableAltDeg: number = USABLE_ALT_DEG,
  site: ObserverSite | null | undefined = activeSite(),
  dateMs: number = Date.now(),
  moonAvoidanceParams: MoonAvoidanceParams = DEFAULT_MOON_AVOIDANCE,
  includeMoonGeometry = true,
  sensorConfig: SensorConfig | null = null,
): RowAltitude {
  const needsCoordinates = subject.raDeg === null || subject.decDeg === null;
  const needsSite = !site;
  if (
    needsCoordinates ||
    needsSite ||
    subject.raDeg === null ||
    subject.decDeg === null ||
    !site
  ) {
    return { ...DEGRADE_ROW, needsCoordinates, needsSite };
  }

  const night = getNightObservability(
    subject.id,
    subject.raDeg,
    subject.decDeg,
    site,
    dateMs,
    includeMoonGeometry,
  );
  const derived = deriveObservability(night, usableAltDeg, {
    raDegJ2000: subject.raDeg,
    minHorizonAltDeg: site.minHorizonAltDeg,
    moonAvoidanceParams,
    bestDateFromMs: dateMs,
    sensorConfig,
  });
  const points: AltPoint[] = night.samples.map((s) => ({
    tHour: (s.tMs - night.nightStartMs) / 3_600_000,
    altDeg: s.altDeg,
  }));
  return {
    points,
    maxAltDeg: derived.maxAltDeg,
    hoursAboveUsable: derived.totalImagingMinutes / 60,
    visibleTonight: derived.visibleTonight,
    needsCoordinates: false,
    needsSite: false,
    bestDate: derived.bestDate,
    separationScalars: derived.separationScalars,
    moonFreeMinutesByBand: derived.moonFreeMinutesByBand,
    noDarkWindow: night.darkWindow === null,
    darkWindowHours: night.darkWindow
      ? {
          startHour:
            (night.darkWindow.startMs - night.nightStartMs) / 3_600_000,
          endHour: (night.darkWindow.endMs - night.nightStartMs) / 3_600_000,
        }
      : null,
    uptimeHours: derived.uptimeMinutes / 60,
    zeroImagingReason: derived.zeroImagingReason,
    moonLimitedBands: derived.moonLimitedBands,
    oscSinglePassHours:
      derived.oscSinglePassMinutes === null
        ? null
        : derived.oscSinglePassMinutes / 60,
  };
}

/**
 * Compute a real per-row tonight-altitude summary for a Planner list target.
 *
 * @param t - The target list item (carries raDeg/decDeg — gen-3 list rows
 *   always populate them per the binding doc comment).
 * @param usableAltDeg - See {@link altitudeFor}.
 * @param site - See {@link altitudeFor}. Defaults to the active site.
 * @param dateMs - See {@link altitudeFor}. Defaults to now ("tonight").
 * @param moonAvoidanceParams - See {@link altitudeFor}.
 * @param includeMoonGeometry - See {@link altitudeFor}. Defaults to `true`;
 *   callers doing a full-catalogue pass MUST pass `false`.
 *
 * Memoization of the underlying positions is `planner-derive.ts`'s job
 * (per target/site/day/`includeMoonGeometry`); this function itself is cheap
 * enough to call inline from render/sort/group code without an extra memo
 * layer — PROVIDED `includeMoonGeometry` is `false` for a full-catalogue pass
 * (see {@link altitudeFor}'s doc for why: the Moon time-series alone is not
 * cheap at catalogue scale).
 */
export function rowAltitudeFor(
  t: TargetListItem,
  usableAltDeg: number = USABLE_ALT_DEG,
  site: ObserverSite | null | undefined = activeSite(),
  dateMs: number = Date.now(),
  moonAvoidanceParams: MoonAvoidanceParams = DEFAULT_MOON_AVOIDANCE,
  includeMoonGeometry = true,
  sensorConfig: SensorConfig | null = null,
): RowAltitude {
  return altitudeFor(
    { id: t.id, raDeg: t.raDeg, decDeg: t.decDeg },
    usableAltDeg,
    site,
    dateMs,
    moonAvoidanceParams,
    includeMoonGeometry,
    sensorConfig,
  );
}

/**
 * Moon-excluded spans for one band on the SAME `tHour` axis as
 * `RowAltitude.points` (iteration 2026-07-15, FR-007's detail-graph
 * overlay). Reads the same cached `NightObservability` as `altitudeFor` for
 * the (target, site, date) key — no second astronomy pass. Empty in the
 * degrade states and when Moon geometry wasn't computed.
 */
export function moonExcludedSpanHours(
  subject: AltitudeSubject,
  band: Band,
  site: ObserverSite | null | undefined = activeSite(),
  dateMs: number = Date.now(),
  moonAvoidanceParams: MoonAvoidanceParams = DEFAULT_MOON_AVOIDANCE,
): Array<{ startHour: number; endHour: number }> {
  if (subject.raDeg === null || subject.decDeg === null || !site) return [];
  const night = getNightObservability(
    subject.id,
    subject.raDeg,
    subject.decDeg,
    site,
    dateMs,
    true,
  );
  return moonExcludedSpans(
    night,
    band,
    site.minHorizonAltDeg,
    moonAvoidanceParams,
  ).map((s) => ({
    startHour: (s.startMs - night.nightStartMs) / 3_600_000,
    endHour: (s.endMs - night.nightStartMs) / 3_600_000,
  }));
}
