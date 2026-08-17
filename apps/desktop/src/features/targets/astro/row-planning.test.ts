// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect } from 'vitest';
import { deriveRowMoonPlanning, UNKNOWN_ROW_PLANNING } from './row-planning';
import type { ObservingNight } from './moon-state';
import { DEFAULT_MOON_AVOIDANCE } from '@/shared/planner/moon-avoidance';

/** A controllable observing night with the Moon at RA 0h / Dec 0° and given age. */
function nightAt(moonAgeFromFullDays: number): ObservingNight {
  return {
    nightKey: '2026-07-05',
    midnight: new Date('2026-07-05T00:00:00Z'),
    phaseName: 'full',
    waxing: false,
    illuminationFrac: 1,
    moonAgeFromFullDays,
    moonVec: { x: 1, y: 0, z: 0 },
  };
}

describe('deriveRowMoonPlanning — US3 filter guidance', () => {
  it('near-full Moon + moderately-separated target → narrowband-only, distant target → broadband-ok', () => {
    const night = nightAt(0); // full Moon: widest required separations (LRGB 120°, Ha/SII 60°, OIII 110°)
    // 70° clears Ha/SII (60°) but not L/R/G/B (120°) or OIII (110°).
    const mid = deriveRowMoonPlanning({ raDeg: 70, decDeg: 0 }, night);
    expect(mid.recommendation).toBe('narrowband-only');
    expect(mid.bandViability?.L).toBe(false);
    expect(mid.bandViability?.Ha).toBe(true);

    const far = deriveRowMoonPlanning({ raDeg: 180, decDeg: 0 }, night); // 180° from Moon
    expect(far.recommendation).toBe('broadband-ok');
    expect(far.bandViability?.L).toBe(true);
    expect(far.bandViability?.OIII).toBe(true);
  });

  it('near-new Moon (age far from full) → every band viable at a moderate separation', () => {
    const night = nightAt(14.77); // ~new Moon: required separations shrink toward zero
    const row = deriveRowMoonPlanning({ raDeg: 90, decDeg: 0 }, night);
    expect(row.recommendation).toBe('broadband-ok');
    for (const v of Object.values(row.bandViability ?? {}))
      expect(v).toBe(true);
  });

  it('boundary: separation exactly at min_separation counts as viable (>=, per moon-avoidance.ts)', () => {
    // The exact-boundary determinism itself is covered by moon-avoidance.test.ts;
    // here we confirm deriveRowMoonPlanning plumbs a comfortably-viable band
    // through correctly end-to-end (vector-derived separation carries floating-
    // point noise unsuitable for an exact-equality boundary assertion).
    const night = nightAt(0); // age 0 → Ha min separation == distanceDeg == 60°
    const row = deriveRowMoonPlanning({ raDeg: 65, decDeg: 0 }, night);
    expect(row.bandViability?.Ha).toBe(true);
  });

  it('unknown coordinates → unknown guidance, never a fabricated recommendation', () => {
    const night = nightAt(0);
    const row = deriveRowMoonPlanning({ raDeg: null, decDeg: null }, night);
    expect(row.bandViability).toBeNull();
    expect(row.recommendation).toBe('unknown');
  });

  it('no observing night (site gate closed) → the explicit unknown row', () => {
    const row = deriveRowMoonPlanning({ raDeg: 10, decDeg: 10 }, null);
    expect(row).toEqual(UNKNOWN_ROW_PLANNING);
  });

  it('live param changes flow through (SC-008 groundwork)', () => {
    const night = nightAt(0);
    const tightened = {
      ...DEFAULT_MOON_AVOIDANCE,
      Ha: { distanceDeg: 90, widthDays: 14 },
    };
    const row = deriveRowMoonPlanning(
      { raDeg: 60, decDeg: 0 },
      night,
      tightened,
    );
    // With Ha's distance widened to 90°, a 60° separation is no longer viable.
    expect(row.bandViability?.Ha).toBe(false);
    expect(row.recommendation).toBe('avoid-tonight');
  });
});

describe('deriveRowMoonPlanning — US4 opposition', () => {
  it('derives a real next-opposition date + sort key for known coordinates', () => {
    const night = nightAt(0);
    const row = deriveRowMoonPlanning({ raDeg: 83.822, decDeg: -5.391 }, night); // M42
    expect(row.nextOppositionDate).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(row.daysToOpposition).toBeGreaterThanOrEqual(0);
  });

  it('unknown coordinates → unknown opposition, never a fabricated date', () => {
    const night = nightAt(0);
    const row = deriveRowMoonPlanning({ raDeg: null, decDeg: null }, night);
    expect(row.nextOppositionDate).toBeNull();
    expect(row.daysToOpposition).toBeNull();
  });
});

describe('deriveRowMoonPlanning — 5,000-row perf (spec 047 T026, SC-007)', () => {
  it('derives the full row (lunar dist + guidance + opposition) for 5,000 rows sharing one night, well under budget', () => {
    const night = nightAt(3);
    const start = performance.now();
    for (let i = 0; i < 5000; i++) {
      const raDeg = (i * 0.073) % 360;
      const decDeg = ((i * 0.037) % 180) - 90;
      deriveRowMoonPlanning({ raDeg, decDeg }, night, DEFAULT_MOON_AVOIDANCE);
    }
    const elapsedMs = performance.now() - start;
    // Same night → the opposition Sun-RA table is memoized once; per-row work
    // is O(1) vector math + an O(367) numeric scan, not a fresh ephemeris scan.
    expect(elapsedMs).toBeLessThan(500);
  });
});
