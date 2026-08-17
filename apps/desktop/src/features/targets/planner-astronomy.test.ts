// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-astronomy.test.ts — real-ephemeris engine tests (spec 044 Track B, T014).
 *
 * Validates `computeNightObservability` against SC-001 (max-alt/transit/curve
 * internal-consistency; the rise/set/transit checks don't depend on network
 * access) and SC-002 (rise/set plausibility to ≈±1 min, circumpolar /
 * never-rising targets return null rise/set with no error). SC-009 (target↔
 * Moon separation vs an independent ephemeris) IS checked against a real,
 * hardcoded JPL Horizons (DE441) reference fetched once at write-time — see
 * the "target↔Moon separation vs an independent ephemeris" block below for
 * the exact query.
 *
 * Cross-checks used for SC-001/SC-002 instead of a live reference ephemeris:
 *   - Transit altitude must equal the sampled-grid maximum to within one grid
 *     step's worth of altitude drift (the exact transit can exceed the nearest
 *     10-min sample, never fall short of it by more than a hair).
 *   - For an object on the celestial equator (dec = 0), the hour angle at
 *     rise/set (ignoring refraction) is exactly ±90° from transit — i.e. almost
 *     exactly 12h between rise and set. Refraction lifts the true horizon
 *     slightly, extending this by a small, bounded amount (a few minutes) — so
 *     `(set - rise)` should be close to 12h, comfortably inside a 10-minute
 *     tolerance band used elsewhere in this file for grid-scale checks.
 *   - Rise must precede transit must precede set (chronological bracketing).
 */

import { describe, expect, it } from 'vitest';
import { Body, GeoVector } from 'astronomy-engine';
import {
  computeNightObservability,
  angularSeparationFromMoonDeg,
  type AltEvent,
} from './planner-astronomy';
import { targetUnitVector, angleBetweenDeg } from './astro/lunar-separation';
import type { ObserverSite } from '@/shared/observing-sites/observer-site';

/** Assert-and-narrow: fails the test with a clear message instead of `!`. */
function required<T>(value: T | null, label: string): T {
  if (value === null) throw new Error(`expected ${label} to be non-null`);
  return value;
}

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

// A fixed winter date so the night is long and well-defined at this latitude.
const WINTER_NIGHT_MS = Date.UTC(2026, 0, 15, 12, 0, 0); // 2026-01-15 noon UTC anchor

describe('computeNightObservability — SC-001 internal consistency', () => {
  it('transit altitude is >= the sampled-grid maximum (grid can only fall short of the true peak)', () => {
    // Equatorial target, well-placed for a mid-northern site.
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const transit = required<AltEvent>(night.transit, 'transit');
    const sampledMax = Math.max(...night.samples.map((s) => s.altDeg));
    expect(transit.altDeg).toBeGreaterThanOrEqual(sampledMax - 0.01);
    // And the grid should get close to the true transit altitude (10-min grid).
    expect(sampledMax).toBeGreaterThan(transit.altDeg - 2);
  });

  it('rise precedes transit precedes set for a normal (non-circumpolar) target', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const rise = required<AltEvent>(night.rise, 'rise');
    const transit = required<AltEvent>(night.transit, 'transit');
    const set = required<AltEvent>(night.set, 'set');
    expect(rise.tMs).toBeLessThan(transit.tMs);
    expect(transit.tMs).toBeLessThan(set.tMs);
  });

  it('altitude at rise and set is ~0 (near the true horizon, refraction applied)', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const rise = required<AltEvent>(night.rise, 'rise');
    const set = required<AltEvent>(night.set, 'set');
    expect(Math.abs(rise.altDeg)).toBeLessThan(1);
    expect(Math.abs(set.altDeg)).toBeLessThan(1);
  });
});

describe('computeNightObservability — SC-002 rise/set plausibility', () => {
  it('an equatorial target is above the horizon for ~12h (±10min for refraction)', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const rise = required<AltEvent>(night.rise, 'rise');
    const set = required<AltEvent>(night.set, 'set');
    const upMs = set.tMs - rise.tMs;
    const twelveHoursMs = 12 * 3_600_000;
    expect(Math.abs(upMs - twelveHoursMs)).toBeLessThan(10 * 60_000);
  });

  it('a circumpolar target (far north, mid-northern site) has no rise/set and stays visible all night', () => {
    // dec=85 at 52N is circumpolar (dec > 90 - lat).
    const night = computeNightObservability(0, 85, AMSTERDAM, WINTER_NIGHT_MS);
    expect(night.rise).toBeNull();
    expect(night.set).toBeNull();
    // Never drops near the horizon across the sampled night.
    for (const s of night.samples) {
      expect(s.altDeg).toBeGreaterThan(0);
    }
  });

  it('a never-rising target (far south, mid-northern site) has no rise/set and stays below the horizon', () => {
    // dec=-80 at 52N never rises (dec < lat - 90).
    const night = computeNightObservability(0, -80, AMSTERDAM, WINTER_NIGHT_MS);
    expect(night.rise).toBeNull();
    expect(night.set).toBeNull();
    for (const s of night.samples) {
      expect(s.altDeg).toBeLessThan(0);
    }
  });

  it('computing rise/set does not throw for either circumpolar or never-rising targets', () => {
    expect(() =>
      computeNightObservability(0, 85, AMSTERDAM, WINTER_NIGHT_MS),
    ).not.toThrow();
    expect(() =>
      computeNightObservability(0, -80, AMSTERDAM, WINTER_NIGHT_MS),
    ).not.toThrow();
  });
});

describe('computeNightObservability — dark window + grid shape', () => {
  it('produces a 10-minute grid spanning the night', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    expect(night.samples.length).toBeGreaterThan(1);
    for (let i = 1; i < night.samples.length; i++) {
      expect(night.samples[i].tMs - night.samples[i - 1].tMs).toBe(10 * 60_000);
    }
    expect(night.samples[0].tMs).toBe(night.nightStartMs);
  });

  it('the astronomical dark window sits inside (or matches) the night span', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const darkWindow = required(night.darkWindow, 'darkWindow');
    expect(darkWindow.startMs).toBeGreaterThanOrEqual(night.nightStartMs);
    expect(darkWindow.endMs).toBeLessThanOrEqual(night.nightEndMs);
  });
});

describe('computeNightObservability — US5 Moon time-series (T027/T031/T032)', () => {
  it('moonSamples is aligned 1:1 with samples (same tMs grid)', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    expect(night.moonSamples.length).toBe(night.samples.length);
    for (let i = 0; i < night.samples.length; i++) {
      expect(night.moonSamples[i].tMs).toBe(night.samples[i].tMs);
    }
  });

  it('every moonSamples separation is within [0, 180]', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    for (const s of night.moonSamples) {
      expect(s.separationDeg).toBeGreaterThanOrEqual(0);
      expect(s.separationDeg).toBeLessThanOrEqual(180);
    }
  });

  it('moonIllumination is a fraction in [0, 1]', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    expect(night.moonIllumination).toBeGreaterThanOrEqual(0);
    expect(night.moonIllumination).toBeLessThanOrEqual(1);
  });

  it('moonUpWindows only contains intervals inside the dark window', () => {
    const night = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const dark = required(night.darkWindow, 'darkWindow');
    for (const w of night.moonUpWindows) {
      expect(w.startMs).toBeGreaterThanOrEqual(dark.startMs);
      expect(w.endMs).toBeLessThanOrEqual(dark.endMs);
      expect(w.startMs).toBeLessThanOrEqual(w.endMs);
    }
  });

  it('raising minHorizonAltDeg never widens the Moon-up windows (horizon-aware, T032)', () => {
    const low = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const raised: ObserverSite = { ...AMSTERDAM, minHorizonAltDeg: 20 };
    const high = computeNightObservability(180, 0, raised, WINTER_NIGHT_MS);
    const sumMs = (windows: typeof low.moonUpWindows) =>
      windows.reduce((acc, w) => acc + (w.endMs - w.startMs), 0);
    expect(sumMs(high.moonUpWindows)).toBeLessThanOrEqual(
      sumMs(low.moonUpWindows),
    );
  });
});

describe('computeNightObservability — includeMoonGeometry=false fast path (CI perf FIX)', () => {
  it('skips the Moon time-series (empty samples/windows, zeroed reference state)', () => {
    const night = computeNightObservability(
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      false,
    );
    expect(night.moonSamples).toEqual([]);
    expect(night.moonUpWindows).toEqual([]);
    expect(night.moonIllumination).toBe(0);
    expect(night.moonAgeFromFullDays).toBe(0);
  });

  it('still computes the target-only fields identically to the full call (samples/transit/rise/set/darkWindow)', () => {
    const full = computeNightObservability(
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      true,
    );
    const fast = computeNightObservability(
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      false,
    );
    expect(fast.samples).toEqual(full.samples);
    expect(fast.transit).toEqual(full.transit);
    expect(fast.rise).toEqual(full.rise);
    expect(fast.set).toEqual(full.set);
    expect(fast.darkWindow).toEqual(full.darkWindow);
  });
});

describe('target↔Moon separation vs an independent ephemeris (SC-009, reviewer FIX item 1)', () => {
  // Reference data fetched live from the public JPL Horizons API (DE441),
  // NOT derived from astronomy-engine, at a fixed instant:
  //
  //   curl "https://ssd.jpl.nasa.gov/api/horizons.api?format=text&COMMAND='301'\
  //   &OBJ_DATA='NO'&MAKE_EPHEM='YES'&EPHEM_TYPE='OBSERVER'&CENTER='500@399'\
  //   &START_TIME='2026-01-15 00:00'&STOP_TIME='2026-01-15 00:01'&STEP_SIZE='1 m'\
  //   &QUANTITIES='1'&ANG_FORMAT='DEG'&REF_SYSTEM='ICRF'"
  //
  // Output for 2026-Jan-15 00:00 UT, "R.A._(ICRF)_DEC" (Horizons' own column
  // description: "Astrometric right ascension and declination ... in the
  // reference frame of the planetary ephemeris (ICRF). Compensated for
  // down-leg light-time delay [and stellar] aberration" — i.e. geocentric,
  // light-time + aberration corrected, ICRF/J2000-referenced, no precession
  // to a date-of-observation equinox):
  //
  //   Moon (301) geocentric from Earth (399): RA 249.44254°, Dec -27.26402°
  //
  // This is exactly the frame `computeNightObservability`'s `moonSamples`
  // separation uses: a raw (unprecessed) J2000 catalogue target vector
  // (`targetUnitVector`) against `GeoVector(Body.Moon, date, /*aberration*/
  // true)` (geocentric, light-time+aberration corrected, EQJ = ICRF/J2000) —
  // see planner-astronomy.ts's module doc (SC-013) for why this reuses Track
  // A's exact vector math rather than a second implementation. Expected
  // separations below are plane spherical-trig (law of cosines) over the two
  // independent RA/Dec pairs, not a value read out of astronomy-engine.
  const REFERENCE_INSTANT_MS = Date.UTC(2026, 0, 15, 0, 0, 0);
  const MOON_ASTROMETRIC_RA_DEG = 249.44254;
  const MOON_ASTROMETRIC_DEC_DEG = -27.26402;

  /** Plane spherical-trig separation (law of cosines) — independent of astronomy-engine. */
  function refSeparationDeg(
    ra1: number,
    dec1: number,
    ra2: number,
    dec2: number,
  ): number {
    const d2r = Math.PI / 180;
    const cosSep =
      Math.sin(dec1 * d2r) * Math.sin(dec2 * d2r) +
      Math.cos(dec1 * d2r) * Math.cos(dec2 * d2r) * Math.cos((ra1 - ra2) * d2r);
    return Math.acos(Math.max(-1, Math.min(1, cosSep))) / d2r;
  }

  it('M42 (Orion Nebula) matches the Horizons-derived separation within 0.5°', () => {
    // M42 J2000: RA 83.8221°, Dec -5.3911° (catalogue value; well outside the
    // Moon's ~0.5° own angular size, so sub-degree agreement is meaningful).
    const M42_RA = 83.8221;
    const M42_DEC = -5.3911;
    const expected = refSeparationDeg(
      M42_RA,
      M42_DEC,
      MOON_ASTROMETRIC_RA_DEG,
      MOON_ASTROMETRIC_DEC_DEG,
    );
    const actual = angularSeparationFromMoonDeg(
      M42_RA,
      M42_DEC,
      AMSTERDAM,
      REFERENCE_INSTANT_MS,
    );
    // `angularSeparationFromMoonDeg` computes topocentric (parallax-corrected
    // for the Moon's ~57' horizontal parallax) rather than geocentric, so a
    // sub-degree gap vs. the geocentric Horizons reference is expected (the
    // measured gap at this site/instant is ≈0.25°); 0.5° comfortably bounds
    // the worst-case topocentric offset while still catching a wrong
    // hemisphere/frame/gross bug.
    expect(Math.abs(actual - expected)).toBeLessThan(0.5);
  });

  it('the T027 per-sample separation formula (targetUnitVector × GeoVector, geocentric) matches Horizons to 0.1°', () => {
    // This exercises the EXACT frame/formula `computeNightObservability`'s
    // `moonSamples[].separationDeg` uses (no topocentric parallax, unlike
    // `angularSeparationFromMoonDeg` above) — see the module doc's SC-013
    // note: `targetUnitVector` (raw J2000, this file's import) against
    // `GeoVector(Body.Moon, date, true)`.
    const M31_RA = 10.6847;
    const M31_DEC = 41.269;
    const expected = refSeparationDeg(
      M31_RA,
      M31_DEC,
      MOON_ASTROMETRIC_RA_DEG,
      MOON_ASTROMETRIC_DEC_DEG,
    );
    const targetVec = targetUnitVector(M31_RA, M31_DEC);
    const moonGeoVec = GeoVector(
      Body.Moon,
      new Date(REFERENCE_INSTANT_MS),
      true,
    );
    const actual = angleBetweenDeg(targetVec, moonGeoVec);
    expect(Math.abs(actual - expected)).toBeLessThan(0.1);
  });
});

describe('computeNightObservability — US4 twilight + horizon (T031/T032/T033)', () => {
  it('nautical twilight (shallower −12° threshold) gives a window at least as wide as astronomical −18° (SC-007)', () => {
    // Sun-below−12° is a strictly looser condition than Sun-below−18°, so the
    // nautical dark window contains the astronomical one (wider, not narrower).
    const astro = computeNightObservability(180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const nautical: ObserverSite = { ...AMSTERDAM, twilight: 'nautical' };
    const naut = computeNightObservability(180, 0, nautical, WINTER_NIGHT_MS);
    const astroDark = required(astro.darkWindow, 'astro darkWindow');
    const nautDark = required(naut.darkWindow, 'naut darkWindow');
    const astroLenMs = astroDark.endMs - astroDark.startMs;
    const nautLenMs = nautDark.endMs - nautDark.startMs;
    expect(nautLenMs).toBeGreaterThanOrEqual(astroLenMs);
  });

  it('a high-latitude summer night has no dark window (SC-008, no fabrication)', () => {
    // Tromsø-like site (69.6N) at midsummer: astronomical twilight never reached.
    const highLat: ObserverSite = {
      ...AMSTERDAM,
      latitudeDeg: 69.6,
      longitudeDeg: 18.9,
    };
    const summer = Date.UTC(2026, 5, 21, 12, 0, 0);
    const night = computeNightObservability(180, 0, highLat, summer);
    expect(night.darkWindow).toBeNull();
  });
});

describe('angularSeparationFromMoonDeg', () => {
  it('returns a value in [0, 180] degrees', () => {
    const deg = angularSeparationFromMoonDeg(
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(deg).toBeGreaterThanOrEqual(0);
    expect(deg).toBeLessThanOrEqual(180);
  });

  it('is deterministic for the same inputs', () => {
    const a = angularSeparationFromMoonDeg(45, 20, AMSTERDAM, WINTER_NIGHT_MS);
    const b = angularSeparationFromMoonDeg(45, 20, AMSTERDAM, WINTER_NIGHT_MS);
    expect(a).toBe(b);
  });

  it('differs for targets in different parts of the sky (not a constant stub)', () => {
    const a = angularSeparationFromMoonDeg(0, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const b = angularSeparationFromMoonDeg(180, 60, AMSTERDAM, WINTER_NIGHT_MS);
    expect(a).not.toBe(b);
  });
});
