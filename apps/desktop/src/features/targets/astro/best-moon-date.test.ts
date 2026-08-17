// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * best-moon-date.test.ts — spec 044 FR-009 amendment (iteration 2026-07-17).
 *
 * Two layers:
 *  - real-ephemeris cases (astronomy-engine is deterministic for a fixed
 *    instant) pin the end-to-end behavior on scouted 2026 dates, with the
 *    scenario precondition (full/new Moon at opposition) asserted from the
 *    same result so a constants drift fails loudly;
 *  - controlled-Moon cases (switchable `moonStateAt` override) pin the
 *    selection rules that real geometry can't isolate: the earlier-night
 *    tie-break, the none-viable fallback, and band/param sensitivity.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Switchable moonStateAt override: `null` = real astronomy-engine ephemeris.
const moonMock = vi.hoisted(() => ({
  override: null as
    | null
    | ((at: Date) => {
        phaseName: string;
        waxing: boolean;
        illuminationFrac: number;
        moonAgeFromFullDays: number;
        moonVec: { x: number; y: number; z: number };
      }),
}));

vi.mock('./moon-state', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./moon-state')>();
  return {
    ...actual,
    moonStateAt: (at: Date) =>
      moonMock.override ? moonMock.override(at) : actual.moonStateAt(at),
  };
});

import {
  BEST_DATE_RADIUS_NIGHTS,
  bestMoonDate,
  __resetBestMoonDateCacheForTest,
} from './best-moon-date';
import { nextOpposition, __resetOppositionCacheForTest } from './opposition';
import { targetUnitVector } from './lunar-separation';
import {
  DEFAULT_MOON_AVOIDANCE,
  type MoonAvoidanceParams,
} from '@/shared/planner/moon-avoidance';
import { assertDefined } from '@/test/assertDefined';

const MS_PER_DAY = 86_400_000;
const FROM = new Date('2026-01-01T00:00:00Z');
const DEC = 20;

beforeEach(() => {
  moonMock.override = null;
  __resetBestMoonDateCacheForTest();
  __resetOppositionCacheForTest();
});

describe('bestMoonDate — real ephemeris', () => {
  it('returns null for unknown coordinates', () => {
    expect(bestMoonDate(null, DEC, FROM, DEFAULT_MOON_AVOIDANCE)).toBeNull();
    expect(bestMoonDate(120, null, FROM, DEFAULT_MOON_AVOIDANCE)).toBeNull();
    expect(
      bestMoonDate(120, Number.NaN, FROM, DEFAULT_MOON_AVOIDANCE),
    ).toBeNull();
  });

  it('viable opposition night coincides — date and Moon facts unchanged', () => {
    // RA 120° from 2026-01-01 → opposition 2026-01-18, a ~1%-lit new Moon
    // ~169° away (scouted constants; preconditions asserted below).
    const r = assertDefined(
      bestMoonDate(120, DEC, FROM, DEFAULT_MOON_AVOIDANCE),
      'bestMoonDate for RA 120 (viable opposition night)',
    );
    expect(r.moonAtOpposition.illumPct).toBeLessThanOrEqual(5); // precondition
    expect(r.state).toBe('coincides');
    expect(r.dateMs).toBe(r.oppositionDateMs);
    expect(r.dateMs).toBe(Date.UTC(2026, 0, 18));
    expect(r.inDays).toBe(17);
    expect(r.moonAtBest).toEqual(r.moonAtOpposition);
  });

  it('full-Moon opposition diverges to a nearby darker, farther night', () => {
    // RA 135° → opposition 2026-02-02 lands on a full Moon ~2° away; the
    // nearest viable night is 7 nights earlier (2026-01-26, ~48% lit, ~98°).
    const r = assertDefined(
      bestMoonDate(135, DEC, FROM, DEFAULT_MOON_AVOIDANCE),
      'bestMoonDate for RA 135 (full-Moon opposition diverges)',
    );
    expect(r.moonAtOpposition.illumPct).toBeGreaterThanOrEqual(95); // precondition
    expect(r.state).toBe('diverged');
    expect(r.oppositionDateMs).toBe(Date.UTC(2026, 1, 2));
    expect(r.dateMs).toBe(Date.UTC(2026, 0, 26));
    expect(r.moonAtBest.illumPct).toBeLessThan(r.moonAtOpposition.illumPct);
    expect(r.moonAtBest.sepDeg).toBeGreaterThan(r.moonAtOpposition.sepDeg);
  });

  it('never recommends a night before the search start', () => {
    // RA 105° → opposition 2026-01-05, only 4 nights after `from`: every
    // earlier candidate that would win the tie-break is in the past and is
    // skipped — the pick lands after the opposition instead.
    const r = assertDefined(
      bestMoonDate(105, DEC, FROM, DEFAULT_MOON_AVOIDANCE),
      'bestMoonDate for RA 105 (never before search start)',
    );
    expect(r.state).toBe('diverged');
    expect(r.oppositionDateMs).toBe(Date.UTC(2026, 0, 5));
    expect(r.dateMs).toBeGreaterThanOrEqual(FROM.getTime());
    expect(r.dateMs).toBe(Date.UTC(2026, 0, 11));
    expect(r.inDays).toBe(10);
  });
});

describe('bestMoonDate — controlled Moon (selection rules)', () => {
  const RA = 210;
  /** The real (unmocked) opposition anchoring the candidate window. */
  const opposition = () =>
    assertDefined(nextOpposition(RA, FROM), 'nextOpposition for RA 210');
  const windowStartMs = () =>
    opposition().date.getTime() - BEST_DATE_RADIUS_NIGHTS * MS_PER_DAY;

  /** Candidate index 0…30 for the instant the module scores. */
  const indexOf = (at: Date) =>
    Math.round((at.getTime() - windowStartMs()) / MS_PER_DAY);

  const AWAY = { ...targetUnitVector(RA + 180, -DEC) };
  const ON_TARGET = targetUnitVector(RA, DEC);

  /** Moon 7 age-days from full (L requirement 120/(1+(7/14)²) = 96°). */
  const controlledMoon =
    (viableIndices: number[]) =>
    (at: Date): ReturnType<NonNullable<typeof moonMock.override>> => {
      const viable = viableIndices.includes(indexOf(at));
      return {
        phaseName: 'first-quarter',
        waxing: true,
        illuminationFrac: viable ? 0.4 : 0.99,
        moonAgeFromFullDays: 7,
        // Viable nights: Moon opposite the target (sep 180° ≥ 96°).
        // Non-viable: Moon on the target (sep 0°).
        moonVec: viable ? AWAY : ON_TARGET,
      };
    };

  it('equidistant viable nights tie-break to the earlier one', () => {
    const OPP = BEST_DATE_RADIUS_NIGHTS;
    moonMock.override = controlledMoon([OPP - 3, OPP + 3]);
    const r = assertDefined(
      bestMoonDate(RA, DEC, FROM, DEFAULT_MOON_AVOIDANCE),
      'bestMoonDate for equidistant viable nights',
    );
    expect(r.state).toBe('diverged');
    expect(r.dateMs).toBe(opposition().date.getTime() - 3 * MS_PER_DAY);
  });

  it('nearest viable night wins over a viable night farther out', () => {
    const OPP = BEST_DATE_RADIUS_NIGHTS;
    moonMock.override = controlledMoon([OPP - 9, OPP + 2]);
    const r = assertDefined(
      bestMoonDate(RA, DEC, FROM, DEFAULT_MOON_AVOIDANCE),
      'bestMoonDate for nearest-viable-wins',
    );
    expect(r.state).toBe('diverged');
    expect(r.dateMs).toBe(opposition().date.getTime() + 2 * MS_PER_DAY);
  });

  it('no viable night in ±15 falls back to the opposition with a distinct state', () => {
    moonMock.override = controlledMoon([]);
    const r = assertDefined(
      bestMoonDate(RA, DEC, FROM, DEFAULT_MOON_AVOIDANCE),
      'bestMoonDate for no-viable-night fallback',
    );
    expect(r.state).toBe('none-viable');
    expect(r.dateMs).toBe(r.oppositionDateMs);
    expect(r.moonAtBest).toEqual(r.moonAtOpposition);
    expect(r.moonAtBest.illumPct).toBe(99);
  });

  it('viability follows the live band parameters and the scoring band', () => {
    // Moon everywhere ~92° from the target (ΔRA 100° at dec 20°), full
    // (age 0): L default needs 120° → nothing viable; loosening L to 90° at
    // full → the opposition night qualifies; likewise scoring against the
    // more tolerant Ha band (60°).
    moonMock.override = () => ({
      phaseName: 'full',
      waxing: false,
      illuminationFrac: 1,
      moonAgeFromFullDays: 0,
      moonVec: targetUnitVector(RA + 100, DEC),
    });

    const strict = assertDefined(
      bestMoonDate(RA, DEC, FROM, DEFAULT_MOON_AVOIDANCE),
      'bestMoonDate under strict default params',
    );
    expect(strict.state).toBe('none-viable');

    __resetBestMoonDateCacheForTest();
    const loosened: MoonAvoidanceParams = {
      ...DEFAULT_MOON_AVOIDANCE,
      L: { distanceDeg: 90, widthDays: 14 },
    };
    expect(
      assertDefined(
        bestMoonDate(RA, DEC, FROM, loosened),
        'bestMoonDate under loosened L param',
      ).state,
    ).toBe('coincides');

    __resetBestMoonDateCacheForTest();
    expect(
      assertDefined(
        bestMoonDate(RA, DEC, FROM, DEFAULT_MOON_AVOIDANCE, 'Ha'),
        'bestMoonDate scored against the Ha band',
      ).state,
    ).toBe('coincides');
  });
});
