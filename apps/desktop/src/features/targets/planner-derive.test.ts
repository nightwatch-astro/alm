// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-derive.test.ts — cache + pure-derivation tests (spec 044 Track B, T014).
 *
 * Validates SC-003 (instant threshold changes do not recompute positions) and
 * the never-visible edge case (T013): a target that never clears the horizon
 * degrades to zero imaging time / not-visible with no error.
 */

import { beforeEach, describe, expect, it } from 'vitest';
import {
  clearObservabilityCache,
  deriveObservability,
  getNightObservability,
  moonExcludedSpans,
} from './planner-derive';
import type { ObserverSite } from '@/shared/observing-sites/observer-site';
import { BANDS, DEFAULT_MOON_AVOIDANCE } from './astro/moon-avoidance';
import { __resetOppositionCacheForTest } from './astro/opposition';
import { assertDefined } from '@/test/assertDefined';

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

beforeEach(() => {
  clearObservabilityCache();
  __resetOppositionCacheForTest();
});

describe('getNightObservability — memoization', () => {
  it('returns the SAME cached object for repeated calls with identical (target, site, day)', () => {
    const a = getNightObservability('t1', 180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const b = getNightObservability('t1', 180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    expect(b).toBe(a); // same object reference => positions were not recomputed
  });

  it('recomputes (different object) when the target id changes', () => {
    const a = getNightObservability('t1', 180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const b = getNightObservability('t2', 180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    expect(b).not.toBe(a);
  });

  it('recomputes when the site changes (different coordinates)', () => {
    const a = getNightObservability('t1', 180, 0, AMSTERDAM, WINTER_NIGHT_MS);
    const otherSite: ObserverSite = {
      ...AMSTERDAM,
      id: 'site-2',
      latitudeDeg: 10,
    };
    const b = getNightObservability('t1', 180, 0, otherSite, WINTER_NIGHT_MS);
    expect(b).not.toBe(a);
  });
});

describe('deriveObservability — SC-003 threshold changes do not recompute positions', () => {
  it('two different usableAltitudeDeg values reuse the same cached NightObservability object', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const samplesRefBefore = night.samples;

    const low = deriveObservability(night, 5);
    const high = deriveObservability(night, 60);

    // The night object passed in is never mutated or replaced by derive calls.
    expect(night.samples).toBe(samplesRefBefore);
    // A fresh cache lookup for the same key still returns the identical object,
    // proving no recompute happened as a side effect of deriving twice.
    const cachedAgain = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(cachedAgain).toBe(night);

    // The derived scalars themselves differ (lower threshold => more usable time).
    expect(low.totalImagingMinutes).toBeGreaterThanOrEqual(
      high.totalImagingMinutes,
    );
  });

  it('maxAltDeg matches the true peak sample and is independent of the threshold', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const a = deriveObservability(night, 10);
    const b = deriveObservability(night, 80);
    expect(a.maxAltDeg).toBe(b.maxAltDeg);
    const sampledMax = Math.max(...night.samples.map((s) => s.altDeg));
    expect(a.maxAltDeg).toBeCloseTo(sampledMax, 6);
  });
});

describe('deriveObservability — never-visible edge case (T013)', () => {
  it('a target that never clears the horizon reports not-visible and zero imaging time', () => {
    // dec=-80 at 52N never rises above the horizon.
    const night = getNightObservability(
      'never',
      0,
      -80,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 30);
    expect(derived.visibleTonight).toBe(false);
    expect(derived.totalImagingMinutes).toBe(0);
    expect(derived.maxAltDeg).toBeLessThan(0);
  });

  it('a threshold of 0 still reports not-visible for a target that never clears the true horizon', () => {
    const night = getNightObservability(
      'never2',
      0,
      -80,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 0);
    expect(derived.visibleTonight).toBe(false);
    expect(derived.totalImagingMinutes).toBe(0);
  });

  it('a circumpolar target is visible all night at a low threshold', () => {
    const night = getNightObservability(
      'circumpolar',
      0,
      85,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 10);
    expect(derived.visibleTonight).toBe(true);
    expect(derived.totalImagingMinutes).toBeGreaterThan(0);
  });
});

describe('deriveObservability — #579 no-dark-window still discriminates by altitude', () => {
  // A high-latitude summer night: no astronomical dark window exists for
  // months, but visibility MUST still vary by altitude (a zenith/circumpolar
  // target is observable in twilight; a never-riser is not) instead of
  // collapsing every target to not-visible.
  const SUMMER_MS = Date.UTC(2026, 6, 15, 12, 0, 0);

  it('has no dark window on a lat-52 summer night (precondition)', () => {
    const night = getNightObservability('t', 270, 52, AMSTERDAM, SUMMER_MS);
    expect(night.darkWindow).toBeNull();
  });

  it('a zenith-transiting target reads visible even with no dark window', () => {
    const night = getNightObservability('t', 270, 52, AMSTERDAM, SUMMER_MS);
    const derived = deriveObservability(night, 30);
    expect(derived.maxAltDeg).toBeGreaterThan(80);
    expect(derived.visibleTonight).toBe(true);
    // Imaging time stays honestly zero — no astronomical darkness (FR-017).
    expect(derived.totalImagingMinutes).toBe(0);
  });

  it('a never-rising target stays not-visible on the same no-dark night', () => {
    const night = getNightObservability('never', 0, -80, AMSTERDAM, SUMMER_MS);
    expect(night.darkWindow).toBeNull();
    const derived = deriveObservability(night, 30);
    expect(derived.maxAltDeg).toBeLessThan(0);
    expect(derived.visibleTonight).toBe(false);
  });

  it('winter behaviour is unchanged: visibility follows the dark window', () => {
    const night = getNightObservability(
      't',
      90,
      52,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    expect(night.darkWindow).not.toBeNull();
    const derived = deriveObservability(night, 30);
    expect(derived.visibleTonight).toBe(true);
    expect(derived.totalImagingMinutes).toBeGreaterThan(0);
  });
});

describe('deriveObservability — US5 separation scalars (T028, SC-009)', () => {
  it('all three scalars are either a finite [0,180] degree figure or "moon-not-up"', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 30);
    for (const figure of Object.values(derived.separationScalars)) {
      if (figure === 'moon-not-up') continue;
      expect(figure).toBeGreaterThanOrEqual(0);
      expect(figure).toBeLessThanOrEqual(180);
    }
  });

  it('minOverDarkDeg never exceeds atDarkMidpointDeg when both are numeric (min ≤ any point)', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 30);
    const { minOverDarkDeg, atDarkMidpointDeg } = derived.separationScalars;
    if (
      minOverDarkDeg !== 'moon-not-up' &&
      atDarkMidpointDeg !== 'moon-not-up'
    ) {
      expect(minOverDarkDeg).toBeLessThanOrEqual(atDarkMidpointDeg + 1e-6);
    }
  });

  it('raising minHorizonAltDeg can only turn a numeric figure into "moon-not-up", never the reverse', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const low = deriveObservability(night, 30, { minHorizonAltDeg: 0 });
    const high = deriveObservability(night, 30, { minHorizonAltDeg: 89 });
    if (low.separationScalars.atDarkMidpointDeg === 'moon-not-up') {
      expect(high.separationScalars.atDarkMidpointDeg).toBe('moon-not-up');
    }
  });
});

describe('deriveObservability — US5 per-band moon-free minutes (T028, SC-010)', () => {
  it('every band is within [0, totalImagingMinutes]', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 30, {
      moonAvoidanceParams: DEFAULT_MOON_AVOIDANCE,
    });
    for (const band of BANDS) {
      expect(derived.moonFreeMinutesByBand[band]).toBeGreaterThanOrEqual(0);
      expect(derived.moonFreeMinutesByBand[band]).toBeLessThanOrEqual(
        derived.totalImagingMinutes,
      );
    }
  });

  it('a more Moon-tolerant band never reports less moon-free time than a stricter band (SC-010)', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 30, {
      moonAvoidanceParams: DEFAULT_MOON_AVOIDANCE,
    });
    // Ha (60°/7d) is strictly more tolerant than L (120°/14d) at every Moon age
    // (smaller required distance AND narrower width) — Ha must never trail L.
    expect(derived.moonFreeMinutesByBand.Ha).toBeGreaterThanOrEqual(
      derived.moonFreeMinutesByBand.L,
    );
  });

  it('no dark window => every band is zero (FR-017, no fabrication)', () => {
    const highLat: ObserverSite = {
      ...AMSTERDAM,
      latitudeDeg: 69.6,
      longitudeDeg: 18.9,
    };
    const summerMs = Date.UTC(2026, 5, 21, 12, 0, 0);
    const night = getNightObservability('t-summer', 180, 0, highLat, summerMs);
    expect(night.darkWindow).toBeNull();
    const derived = deriveObservability(night, 30);
    for (const band of BANDS) {
      expect(derived.moonFreeMinutesByBand[band]).toBe(0);
    }
  });

  it('includeMoonGeometry=false (CI perf FIX fast path) degrades to zero, never fabricating full imaging time', () => {
    // A naive "no moonSamples => treat as no interference" reading would make
    // every band equal totalImagingMinutes here (there IS a dark window and
    // imaging time) — that would be a fabricated non-zero value for data that
    // was never computed. Must be honestly zero instead.
    const night = getNightObservability(
      't-fast',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      false,
    );
    expect(night.moonSamples).toEqual([]);
    const derived = deriveObservability(night, 30, {
      moonAvoidanceParams: DEFAULT_MOON_AVOIDANCE,
    });
    expect(derived.totalImagingMinutes).toBeGreaterThan(0);
    for (const band of BANDS) {
      expect(derived.moonFreeMinutesByBand[band]).toBe(0);
    }
    expect(derived.separationScalars.atTransitDeg).toBe('moon-not-up');
    expect(derived.separationScalars.minOverDarkDeg).toBe('moon-not-up');
    expect(derived.separationScalars.atDarkMidpointDeg).toBe('moon-not-up');
  });
});

describe('deriveObservability — US2 bestDate (T025, FR-009)', () => {
  it('is null for unknown coordinates', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 30, { raDegJ2000: null });
    expect(derived.bestDate).toBeNull();
  });

  it('is a real future-or-present date with a non-negative days-until for known coordinates', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const derived = deriveObservability(night, 30, {
      raDegJ2000: 180,
      bestDateFromMs: WINTER_NIGHT_MS,
    });
    expect(derived.bestDate).not.toBeNull();
    expect(derived.bestDate?.inDays).toBeGreaterThanOrEqual(0);
    expect(derived.bestDate?.dateMs).toBeGreaterThanOrEqual(WINTER_NIGHT_MS);
  });

  it('a later anchor within the same cycle reduces the best-date days-until (US2/SC-004)', () => {
    const night = getNightObservability(
      't1',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const fromWinter = deriveObservability(night, 30, {
      raDegJ2000: 180,
      bestDateFromMs: WINTER_NIGHT_MS,
    });
    // 10 days later, still well short of the ~60-day-out best date found above —
    // moving the anchor forward within the same cycle must shrink days-until by
    // the same 10 days (same absolute best-date calendar day).
    const laterMs = WINTER_NIGHT_MS + 10 * 86_400_000;
    const fromLater = deriveObservability(night, 30, {
      raDegJ2000: 180,
      bestDateFromMs: laterMs,
    });
    expect(fromLater.bestDate?.dateMs).toBe(fromWinter.bestDate?.dateMs);
    expect(fromLater.bestDate?.inDays).toBeLessThan(
      fromWinter.bestDate?.inDays ?? -Infinity,
    );
  });
});

// ── Iteration 2026-07-15: three quantities, reason-for-zero, moon limiter, OSC ──

const HOME_BACKYARD: ObserverSite = {
  // The #817 repro site: 52.09°N on 2026-07-14 the Sun bottoms at −16.4°, so
  // astronomical darkness (−18°) is never reached — no dark window.
  id: 'site-817',
  name: 'Home Backyard',
  latitudeDeg: 52.09,
  longitudeDeg: 5.1,
  elevationM: 0,
  timezone: 'Europe/Amsterdam',
  twilight: 'astronomical',
  minHorizonAltDeg: 0,
};

const JULY_817_NIGHT_MS = Date.UTC(2026, 6, 14, 12, 0, 0);
/** M31 J2000 (the #817 repro target — transits at 73° from 52°N). */
const M31 = { raDeg: 10.684, decDeg: 41.269 };

describe('deriveObservability — reason-for-zero (FR-029) + uptime (FR-005/D1)', () => {
  it('#817 repro: no dark window → reason "darkness", zero imaging, NON-zero uptime', () => {
    const night = getNightObservability(
      'm31',
      M31.raDeg,
      M31.decDeg,
      HOME_BACKYARD,
      JULY_817_NIGHT_MS,
    );
    const d = deriveObservability(night, 30);
    expect(night.darkWindow).toBeNull();
    expect(d.totalImagingMinutes).toBe(0);
    expect(d.zeroImagingReason).toBe('darkness');
    // The three-quantity distinction: the target is well up (73° transit) even
    // though imaging time is zero — uptime must NOT read zero (D1).
    expect(d.uptimeMinutes).toBeGreaterThan(0);
    expect(d.maxAltDeg).toBeGreaterThan(60);
  });

  it('dark window exists but target never clears the threshold → reason "altitude"', () => {
    // Winter Amsterdam night has real astronomical darkness; a far-southern
    // target (dec −60°) never rises from 52°N.
    const night = getNightObservability(
      'far-south',
      180,
      -60,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const d = deriveObservability(night, 30);
    expect(night.darkWindow).not.toBeNull();
    expect(d.totalImagingMinutes).toBe(0);
    expect(d.uptimeMinutes).toBe(0);
    expect(d.zeroImagingReason).toBe('altitude');
  });

  it('simultaneous blockers: no dark window AND never-above → darkness wins (precedence)', () => {
    const night = getNightObservability(
      'far-south-summer',
      180,
      -60,
      HOME_BACKYARD,
      JULY_817_NIGHT_MS,
    );
    const d = deriveObservability(night, 30);
    expect(night.darkWindow).toBeNull();
    expect(d.zeroImagingReason).toBe('darkness');
  });

  it('non-zero imaging with some band viable → no reason (null)', () => {
    const night = getNightObservability(
      't-mid',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const d = deriveObservability(night, 5);
    expect(d.totalImagingMinutes).toBeGreaterThan(0);
    const someBandViable = BANDS.some((b) => d.moonFreeMinutesByBand[b] > 0);
    if (someBandViable) expect(d.zeroImagingReason).toBeNull();
    else expect(d.zeroImagingReason).toBe('moon');
  });
});

describe('deriveObservability — moon-limited bands (FR-031)', () => {
  it('moonLimitedBands is exactly the bands whose moon-free time is below the band-free total', () => {
    const night = getNightObservability(
      't-mid',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const d = deriveObservability(night, 5);
    const expected = BANDS.filter(
      (b) => d.moonFreeMinutesByBand[b] < d.totalImagingMinutes,
    );
    expect(d.moonLimitedBands).toEqual(expected);
  });

  it('not-computed Moon geometry never reads as "limited" or reason "moon"', () => {
    const night = getNightObservability(
      't-nomoon',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      false,
    );
    const d = deriveObservability(night, 5);
    expect(d.moonLimitedBands).toEqual([]);
    expect(d.zeroImagingReason).not.toBe('moon');
    expect(d.oscSinglePassMinutes).toBeNull();
  });
});

describe('deriveObservability — OSC single-pass (FR-036/FR-037/FR-038)', () => {
  const night = () =>
    getNightObservability('t-osc', 180, 0, AMSTERDAM, WINTER_NIGHT_MS);

  it("mono / unknown sensors keep today's model: oscSinglePassMinutes is null (SC-017 regression)", () => {
    const base = deriveObservability(night(), 30);
    const mono = deriveObservability(night(), 30, {
      sensorConfig: { sensorType: 'mono' },
    });
    expect(base.oscSinglePassMinutes).toBeNull();
    expect(mono.oscSinglePassMinutes).toBeNull();
    // Every pre-iteration output is unchanged by passing a mono config.
    expect(mono.totalImagingMinutes).toBe(base.totalImagingMinutes);
    expect(mono.moonFreeMinutesByBand).toEqual(base.moonFreeMinutesByBand);
  });

  it('OSC narrowband passband: single-pass equals the strictest band (min of the per-line windows)', () => {
    const d = deriveObservability(night(), 30, {
      sensorConfig: { sensorType: 'osc', passband: ['Ha', 'OIII'] },
      moonAvoidanceParams: DEFAULT_MOON_AVOIDANCE,
    });
    const oscSinglePassMinutes = assertDefined(
      d.oscSinglePassMinutes,
      'oscSinglePassMinutes for OSC narrowband passband',
    );
    // Interference thresholds nest (larger required sep ⊆ smaller), so the
    // strictest-band aggregation equals the minimum per-line window.
    const expected = Math.min(
      d.moonFreeMinutesByBand.Ha,
      d.moonFreeMinutesByBand.OIII,
    );
    expect(oscSinglePassMinutes).toBe(expected);
    // Per-line windows (FR-037) stay available alongside the headline.
    expect(d.moonFreeMinutesByBand.Ha).toBeGreaterThanOrEqual(
      oscSinglePassMinutes,
    );
  });

  it("OSC without a passband defaults to 'rgb' (broadband)", () => {
    const d = deriveObservability(night(), 30, {
      sensorConfig: { sensorType: 'osc' },
    });
    // LRGB share params, so the rgb single-pass equals any broadband band's window.
    expect(d.oscSinglePassMinutes).toBe(d.moonFreeMinutesByBand.R);
  });
});

describe('moonExcludedSpans — detail-graph overlay (FR-007, iteration 2026-07-15)', () => {
  it('spans are ordered, non-overlapping, and stay within the night', () => {
    const night = getNightObservability(
      't-spans',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const spans = moonExcludedSpans(night, 'L', 0, DEFAULT_MOON_AVOIDANCE);
    for (const s of spans) {
      expect(s.startMs).toBeGreaterThanOrEqual(night.nightStartMs);
      expect(s.endMs).toBeLessThanOrEqual(night.nightEndMs);
      expect(s.endMs).toBeGreaterThanOrEqual(s.startMs);
    }
    for (let i = 1; i < spans.length; i++) {
      expect(spans[i].startMs).toBeGreaterThan(spans[i - 1].endMs);
    }
  });

  it('not-computed Moon geometry yields NO spans (never a fabricated exclusion)', () => {
    const night = getNightObservability(
      't-spans-nomoon',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
      false,
    );
    expect(moonExcludedSpans(night, 'L', 0, DEFAULT_MOON_AVOIDANCE)).toEqual(
      [],
    );
  });

  it('a stricter broadband band excludes at least as much time as a narrowband line', () => {
    const night = getNightObservability(
      't-spans',
      180,
      0,
      AMSTERDAM,
      WINTER_NIGHT_MS,
    );
    const totalMs = (spans: Array<{ startMs: number; endMs: number }>) =>
      spans.reduce((acc, s) => acc + (s.endMs - s.startMs), 0);
    // Broadband L needs a LARGER Moon separation than Ha at every Moon age,
    // so its excluded time can never be smaller.
    expect(
      totalMs(moonExcludedSpans(night, 'L', 0, DEFAULT_MOON_AVOIDANCE)),
    ).toBeGreaterThanOrEqual(
      totalMs(moonExcludedSpans(night, 'Ha', 0, DEFAULT_MOON_AVOIDANCE)),
    );
  });
});
