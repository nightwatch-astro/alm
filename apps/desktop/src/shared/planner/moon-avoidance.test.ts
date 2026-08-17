// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect } from 'vitest';
import {
  BANDS,
  DEFAULT_MOON_AVOIDANCE,
  minSeparationDeg,
  bandViability,
  deriveRecommendation,
  bandTier,
  type MoonAvoidanceParams,
} from './moon-avoidance';

describe('minSeparationDeg (Lorentzian)', () => {
  it('returns the full distanceDeg at full Moon (age 0)', () => {
    expect(minSeparationDeg('L', 0)).toBeCloseTo(120, 10);
    expect(minSeparationDeg('Ha', 0)).toBeCloseTo(60, 10);
    expect(minSeparationDeg('OIII', 0)).toBeCloseTo(110, 10);
  });

  it('falls to half the distance at age == widthDays', () => {
    // 1 / (1 + 1^2) = 0.5
    expect(minSeparationDeg('L', 14)).toBeCloseTo(60, 10);
    expect(minSeparationDeg('Ha', 7)).toBeCloseTo(30, 10);
  });

  it('is monotonically decreasing with Moon age', () => {
    let prev = Infinity;
    for (let age = 0; age <= 14.77; age += 0.5) {
      const v = minSeparationDeg('L', age);
      expect(v).toBeLessThanOrEqual(prev);
      prev = v;
    }
  });

  it('clamps negative ages to 0 (full-Moon value)', () => {
    expect(minSeparationDeg('L', -5)).toBeCloseTo(minSeparationDeg('L', 0), 10);
  });

  it('handles extreme params: distance 180° and minimum width', () => {
    const params: MoonAvoidanceParams = {
      ...DEFAULT_MOON_AVOIDANCE,
      L: { distanceDeg: 180, widthDays: 0.5 },
    };
    expect(minSeparationDeg('L', 0, params)).toBeCloseTo(180, 10);
    // At age 0.5 (== width): half.
    expect(minSeparationDeg('L', 0.5, params)).toBeCloseTo(90, 10);
    // Far past full: very small required separation.
    expect(minSeparationDeg('L', 14.77, params)).toBeLessThan(1);
  });
});

describe('bandViability', () => {
  it('marks all bands viable when the target is far from a new Moon', () => {
    const v = bandViability(180, 14.77); // new Moon, opposite sky
    for (const b of BANDS) expect(v[b]).toBe(true);
  });

  it('marks all bands not viable at full Moon right next to the Moon', () => {
    const v = bandViability(0, 0); // full Moon, 0° separation
    for (const b of BANDS) expect(v[b]).toBe(false);
  });

  it('boundary: separation exactly equal to the minimum counts as viable', () => {
    const min = minSeparationDeg('Ha', 3);
    const v = bandViability(min, 3);
    expect(v.Ha).toBe(true);
    // Just below the boundary is not viable.
    const v2 = bandViability(min - 1e-9, 3);
    expect(v2.Ha).toBe(false);
  });

  it('is deterministic (same inputs → same record)', () => {
    const a = bandViability(75, 4);
    const b = bandViability(75, 4);
    expect(a).toEqual(b);
  });

  it('near-full Moon: narrowband viable while broadband is not for a mid target', () => {
    // Full Moon (age 0). Broadband needs 120°, Ha needs 60°, OIII needs 110°.
    const v = bandViability(70, 0);
    expect(v.L).toBe(false); // 70 < 120
    expect(v.Ha).toBe(true); // 70 >= 60
    expect(v.SII).toBe(true);
    expect(v.OIII).toBe(false); // 70 < 110
  });
});

describe('deriveRecommendation', () => {
  it('unknown when viability is null', () => {
    expect(deriveRecommendation(null)).toBe('unknown');
  });

  it('broadband-ok when any broadband band is viable', () => {
    expect(deriveRecommendation(bandViability(180, 14.77))).toBe(
      'broadband-ok',
    );
  });

  it('narrowband-only when only narrowband bands are viable', () => {
    expect(deriveRecommendation(bandViability(70, 0))).toBe('narrowband-only');
  });

  it('avoid-tonight when no band is viable', () => {
    expect(deriveRecommendation(bandViability(0, 0))).toBe('avoid-tonight');
  });
});

describe('bandTier', () => {
  it('classifies LRGB as broadband and Ha/SII/OIII as narrowband', () => {
    expect(bandTier('L')).toBe('broadband');
    expect(bandTier('R')).toBe('broadband');
    expect(bandTier('G')).toBe('broadband');
    expect(bandTier('B')).toBe('broadband');
    expect(bandTier('Ha')).toBe('narrowband');
    expect(bandTier('SII')).toBe('narrowband');
    expect(bandTier('OIII')).toBe('narrowband');
  });
});
