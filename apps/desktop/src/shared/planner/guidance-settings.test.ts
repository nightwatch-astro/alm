// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { DEFAULT_MOON_AVOIDANCE } from './moon-avoidance';

// Mock the generated command surface so we can drive settings I/O.
const { settingsGet, settingsUpdate, settingsRestoreDefaults } = vi.hoisted(
  () => ({
    settingsGet: vi.fn(),
    settingsUpdate: vi.fn(),
    settingsRestoreDefaults: vi.fn(),
  }),
);
vi.mock('@/bindings/index', () => ({
  commands: { settingsGet, settingsUpdate, settingsRestoreDefaults },
}));
vi.mock('@/api/ipc', () => ({ unwrap: (v: unknown) => v }));

import {
  coerceParams,
  loadGuidanceParams,
  saveGuidanceParams,
  restoreGuidanceDefaults,
  getGuidanceParams,
  __resetGuidanceParamsForTest,
  DISTANCE_MAX,
  WIDTH_MIN,
  MOON_AVOIDANCE_KEY,
  PLANNER_SCOPE,
} from './guidance-settings';

beforeEach(() => {
  vi.clearAllMocks();
  __resetGuidanceParamsForTest();
});
afterEach(() => {
  __resetGuidanceParamsForTest();
});

describe('coerceParams', () => {
  it('returns all seven bands with defaults for empty input', () => {
    const p = coerceParams(undefined);
    expect(Object.keys(p).sort()).toEqual([
      'B',
      'G',
      'Ha',
      'L',
      'OIII',
      'R',
      'SII',
    ]);
    expect(p.L).toEqual(DEFAULT_MOON_AVOIDANCE.L);
    expect(p.OIII).toEqual(DEFAULT_MOON_AVOIDANCE.OIII);
  });

  it('clamps out-of-range values into the valid ranges', () => {
    const p = coerceParams({
      L: { distanceDeg: 999, widthDays: 0.01 },
    });
    expect(p.L.distanceDeg).toBe(DISTANCE_MAX);
    expect(p.L.widthDays).toBe(WIDTH_MIN);
  });

  it('falls back per-field on non-numeric input', () => {
    const p = coerceParams({ Ha: { distanceDeg: 'x', widthDays: 5 } });
    expect(p.Ha.distanceDeg).toBe(DEFAULT_MOON_AVOIDANCE.Ha.distanceDeg);
    expect(p.Ha.widthDays).toBe(5);
  });
});

describe('loadGuidanceParams', () => {
  it('hydrates the cache from the planner scope', async () => {
    settingsGet.mockResolvedValue({
      scope: PLANNER_SCOPE,
      values: {
        [MOON_AVOIDANCE_KEY]: { L: { distanceDeg: 90, widthDays: 10 } },
      },
    });
    const p = await loadGuidanceParams();
    expect(settingsGet).toHaveBeenCalledWith(PLANNER_SCOPE);
    expect(p.L).toEqual({ distanceDeg: 90, widthDays: 10 });
    expect(getGuidanceParams().L.distanceDeg).toBe(90);
  });

  it('falls back to defaults when the backend throws', async () => {
    settingsGet.mockRejectedValue(new Error('offline'));
    const p = await loadGuidanceParams();
    expect(p).toEqual(DEFAULT_MOON_AVOIDANCE);
  });
});

describe('saveGuidanceParams (live propagation, SC-008)', () => {
  it('persists via settings.update and updates the cache immediately', async () => {
    settingsUpdate.mockResolvedValue(null);
    const next = {
      ...DEFAULT_MOON_AVOIDANCE,
      L: { distanceDeg: 100, widthDays: 12 },
    };
    await saveGuidanceParams(next);
    expect(settingsUpdate).toHaveBeenCalledWith(PLANNER_SCOPE, {
      [MOON_AVOIDANCE_KEY]: expect.objectContaining({
        L: { distanceDeg: 100, widthDays: 12 },
      }),
    });
    expect(getGuidanceParams().L).toEqual({ distanceDeg: 100, widthDays: 12 });
  });

  it('restores the pre-write value when the backend write rejects', async () => {
    settingsUpdate.mockRejectedValue(new Error('backend unavailable'));
    const original = getGuidanceParams();
    const next = {
      ...DEFAULT_MOON_AVOIDANCE,
      L: { distanceDeg: 100, widthDays: 12 },
    };

    await expect(saveGuidanceParams(next)).rejects.toThrow(
      'backend unavailable',
    );

    // The optimistic update must be rolled back.
    expect(getGuidanceParams().L).toEqual(original.L);
  });

  it('does not roll back when a newer write has committed after the failing one', async () => {
    let rejectFirst!: (e: unknown) => void;
    settingsUpdate
      .mockReturnValueOnce(
        new Promise<null>((_resolve, reject) => {
          rejectFirst = reject;
        }),
      )
      .mockResolvedValueOnce(null);

    const first = {
      ...DEFAULT_MOON_AVOIDANCE,
      L: { distanceDeg: 90, widthDays: 8 },
    };
    const second = {
      ...DEFAULT_MOON_AVOIDANCE,
      L: { distanceDeg: 120, widthDays: 14 },
    };

    const firstWrite = saveGuidanceParams(first);
    // Second write commits before the first backend call resolves.
    await saveGuidanceParams(second);
    expect(getGuidanceParams().L).toEqual({ distanceDeg: 120, widthDays: 14 });

    rejectFirst(new Error('stale failure'));
    await expect(firstWrite).rejects.toThrow('stale failure');
    // Second write's state must not be reverted.
    expect(getGuidanceParams().L).toEqual({ distanceDeg: 120, widthDays: 14 });
  });
});

describe('loadGuidanceParams vs saveGuidanceParams race (#836)', () => {
  it('does not let a load started before a save clobber the just-saved value', async () => {
    // A TargetsPage-mount `settingsGet` is in flight (started before the
    // Settings-pane edit) when `saveGuidanceParams` commits — the stale read
    // must not overwrite the newer save once it resolves.
    let resolveGet!: (v: unknown) => void;
    settingsGet.mockReturnValue(
      new Promise((resolve) => {
        resolveGet = resolve;
      }),
    );
    settingsUpdate.mockResolvedValue(null);

    const loadPromise = loadGuidanceParams();

    const next = {
      ...DEFAULT_MOON_AVOIDANCE,
      OIII: { distanceDeg: 95, widthDays: 10 },
    };
    await saveGuidanceParams(next);
    expect(getGuidanceParams().OIII.distanceDeg).toBe(95);

    // The stale load now resolves with the pre-save (empty) values.
    resolveGet({ scope: PLANNER_SCOPE, values: {} });
    await loadPromise;

    expect(getGuidanceParams().OIII.distanceDeg).toBe(95);
  });
});

describe('restoreGuidanceDefaults', () => {
  it('calls restore-defaults for the key then reloads', async () => {
    settingsRestoreDefaults.mockResolvedValue({
      restored: [MOON_AVOIDANCE_KEY],
    });
    settingsGet.mockResolvedValue({ scope: PLANNER_SCOPE, values: {} });
    await restoreGuidanceDefaults();
    expect(settingsRestoreDefaults).toHaveBeenCalledWith({
      keys: [MOON_AVOIDANCE_KEY],
    });
    expect(getGuidanceParams()).toEqual(DEFAULT_MOON_AVOIDANCE);
  });
});
