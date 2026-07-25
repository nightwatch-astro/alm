// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-altitude.test.ts — real per-row tonight altitude (spec 044 Track B, T011).
 *
 * Replaces the prior hash-based mock tests (tasks #84/#85) now that
 * `rowAltitudeFor`/`altitudeFor` compute against the real engine
 * (`planner-astronomy.ts` + `planner-derive.ts`) for a given site/date.
 *
 * Moon geometry (real lunar distance + filter guidance) is spec 047 Track A and
 * is tested in `astro/row-planning.test.ts` and `astro/moon-avoidance.test.ts`,
 * NOT here — this module owns tonight altitude / imaging time only.
 *
 * T013 edge cases: a target with no RA/Dec, and no active site, each degrade
 * to zero/not-visible with no thrown error.
 */

import { describe, it, expect } from 'vitest';
import type { TargetListItem } from '@/bindings/index';
import {
  rowAltitudeFor,
  altitudeFor,
  USABLE_ALT_DEG,
} from './planner-altitude';
import type { ObserverSite } from './observing-sites/observer-site';

const AMSTERDAM: ObserverSite = {
  id: 'site-ams',
  name: 'Amsterdam',
  latitudeDeg: 52.37,
  longitudeDeg: 4.9,
  elevationM: 0,
  timezone: 'Europe/Amsterdam',
  twilight: 'astronomical',
  minHorizonAltDeg: 0,
};

const WINTER_NIGHT_MS = Date.UTC(2026, 0, 15, 12, 0, 0);

function item(
  id: string,
  raDeg: number | null,
  decDeg: number | null,
  overrides: Partial<TargetListItem> = {},
): TargetListItem {
  return {
    id,
    effectiveLabel: id,
    primaryDesignation: id,
    objectType: 'other',
    raDeg,
    decDeg,
    sessionCount: 0,
    ...overrides,
  };
}

describe('rowAltitudeFor (real engine)', () => {
  it('is deterministic for the same target/site/date', () => {
    const t = item('NGC 7000', 313, 44);
    const a = rowAltitudeFor(t, USABLE_ALT_DEG, AMSTERDAM, WINTER_NIGHT_MS);
    const b = rowAltitudeFor(t, USABLE_ALT_DEG, AMSTERDAM, WINTER_NIGHT_MS);
    expect(a.maxAltDeg).toBe(b.maxAltDeg);
    expect(a.visibleTonight).toBe(b.visibleTonight);
    expect(a.points.map((p) => p.altDeg)).toEqual(
      b.points.map((p) => p.altDeg),
    );
  });

  it('produces different peak altitudes for targets in different parts of the sky', () => {
    const a = rowAltitudeFor(
      item('a', 0, 60),
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const b = rowAltitudeFor(
      item('b', 0, -80),
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(a.maxAltDeg).not.toBe(b.maxAltDeg);
  });

  it('samples a non-empty curve and a max altitude that matches the samples', () => {
    const r = rowAltitudeFor(
      item('M 42', 83.8, -5.4),
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(r.points.length).toBeGreaterThan(0);
    const sampledMax = Math.max(...r.points.map((p) => p.altDeg));
    expect(r.maxAltDeg).toBeCloseTo(sampledMax, 6);
  });

  it('visibleTonight agrees with the usable-altitude threshold', () => {
    const r = rowAltitudeFor(
      item('NGC 891', 35.6, 42.3),
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(r.visibleTonight).toBe(r.maxAltDeg >= USABLE_ALT_DEG);
  });

  it('reports hours-above-usable as non-negative and bounded by a night', () => {
    const r = rowAltitudeFor(
      item('Sh2-155', 337.2, 62.6),
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(r.hoursAboveUsable).toBeGreaterThanOrEqual(0);
    expect(r.hoursAboveUsable).toBeLessThanOrEqual(16);
  });

  it('lower threshold yields same or more imaging time', () => {
    const t = item('NGC 1234', 47.5, 25.9);
    const high = rowAltitudeFor(t, 30, AMSTERDAM, WINTER_NIGHT_MS);
    const low = rowAltitudeFor(t, 5, AMSTERDAM, WINTER_NIGHT_MS);
    expect(low.hoursAboveUsable).toBeGreaterThanOrEqual(high.hoursAboveUsable);
  });

  it('threshold 0° makes a circumpolar target visible', () => {
    const r = rowAltitudeFor(
      item('circumpolar', 0, 85),
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(r.visibleTonight).toBe(true);
  });

  it('threshold 89° makes most targets invisible', () => {
    let hiddenCount = 0;
    const decs = [60, 44, -5.4, 42.3, 62.6];
    for (const dec of decs) {
      if (
        !rowAltitudeFor(item('x', 0, dec), 89, AMSTERDAM, WINTER_NIGHT_MS)
          .visibleTonight
      ) {
        hiddenCount++;
      }
    }
    expect(hiddenCount).toBeGreaterThanOrEqual(3);
  });
});

describe('rowAltitudeFor — US4 darkWindowHours (T035)', () => {
  it('is a startHour < endHour pair inside the points range for a normal night', () => {
    const r = rowAltitudeFor(
      item('NGC 7000', 313, 44),
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(r.darkWindowHours).not.toBeNull();
    if (r.darkWindowHours) {
      expect(r.darkWindowHours.startHour).toBeLessThan(
        r.darkWindowHours.endHour,
      );
      expect(r.darkWindowHours.startHour).toBeGreaterThanOrEqual(0);
    }
  });

  it('is null when there is no dark window (US4/FR-017)', () => {
    const highLat: ObserverSite = {
      ...AMSTERDAM,
      latitudeDeg: 69.6,
      longitudeDeg: 18.9,
    };
    const summerMs = Date.UTC(2026, 5, 21, 12, 0, 0);
    const r = rowAltitudeFor(
      item('t', 180, 0),
      USABLE_ALT_DEG,
      highLat,
      summerMs,
    );
    expect(r.noDarkWindow).toBe(true);
    expect(r.darkWindowHours).toBeNull();
  });
});

describe('rowAltitudeFor — includeMoonGeometry=false fast path (CI perf FIX)', () => {
  it('leaves maxAlt/visible/imagingTime/darkWindowHours identical to the full computation', () => {
    const t = item('NGC 7000', 313, 44);
    const full = rowAltitudeFor(
      t,
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      undefined,
      true,
    );
    const fast = rowAltitudeFor(
      t,
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      undefined,
      false,
    );
    expect(fast.maxAltDeg).toBe(full.maxAltDeg);
    expect(fast.visibleTonight).toBe(full.visibleTonight);
    expect(fast.hoursAboveUsable).toBe(full.hoursAboveUsable);
    expect(fast.darkWindowHours).toEqual(full.darkWindowHours);
  });

  it('zeroes moonFreeMinutesByBand/separationScalars honestly rather than fabricating a value', () => {
    const t = item('NGC 7000', 313, 44);
    const fast = rowAltitudeFor(
      t,
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      undefined,
      false,
    );
    expect(fast.hoursAboveUsable).toBeGreaterThan(0);
    for (const minutes of Object.values(fast.moonFreeMinutesByBand)) {
      expect(minutes).toBe(0);
    }
    expect(fast.separationScalars.atTransitDeg).toBe('moon-not-up');
  });
});

// ── T013: degrade states (no throw) ──────────────────────────────────────────

describe('altitudeFor / rowAltitudeFor — T013 degrade states', () => {
  it('a target with no RA/Dec reports needsCoordinates, zero/not-visible, no throw', () => {
    expect(() =>
      altitudeFor(
        { id: 'no-coords', raDeg: null, decDeg: null },
        USABLE_ALT_DEG,
        AMSTERDAM,
        WINTER_NIGHT_MS,
      ),
    ).not.toThrow();
    const r = altitudeFor(
      { id: 'no-coords', raDeg: null, decDeg: null },
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(r.needsCoordinates).toBe(true);
    expect(r.needsSite).toBe(false);
    expect(r.visibleTonight).toBe(false);
    expect(r.hoursAboveUsable).toBe(0);
    expect(r.points).toEqual([]);
  });

  it('a target with only RA missing still degrades cleanly', () => {
    const r = altitudeFor(
      { id: 'x', raDeg: null, decDeg: 40 },
      USABLE_ALT_DEG,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(r.needsCoordinates).toBe(true);
    expect(r.visibleTonight).toBe(false);
  });

  it('no active site reports needsSite, zero/not-visible, no throw', () => {
    expect(() =>
      altitudeFor(
        { id: 'x', raDeg: 100, decDeg: 30 },
        USABLE_ALT_DEG,
        null,
        WINTER_NIGHT_MS,
      ),
    ).not.toThrow();
    const r = altitudeFor(
      { id: 'x', raDeg: 100, decDeg: 30 },
      USABLE_ALT_DEG,
      null,
      WINTER_NIGHT_MS,
    );
    expect(r.needsSite).toBe(true);
    expect(r.needsCoordinates).toBe(false);
    expect(r.visibleTonight).toBe(false);
    expect(r.hoursAboveUsable).toBe(0);
  });
});
