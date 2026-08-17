// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * altitude-settings.test.ts — unit tests for the usable-altitude threshold
 * adapter (spec 044 Track B, T012b).
 *
 * As of T012b the threshold is settings-backed (`observing-sites/site-store.ts`,
 * `usableAltitudeDeg` key) rather than localStorage — this durability change is
 * FR-004/SC-006 (the value now survives relaunch, not just page reload). These
 * tests reset the site-store's live cache directly (`__setObservingStateForTest`)
 * rather than touching localStorage, and exercise `setAltitudeThreshold` via its
 * real (optimistic, then backend-persisted) `saveUsableAltitude` path.
 */

import { beforeEach, describe, it, expect, vi } from 'vitest';

// `setAltitudeThreshold` persists through `site-store.ts`'s `saveUsableAltitude`,
// which fires `commands.settingsUpdate` (a real tauri IPC). Under jsdom that
// invoke has no `__TAURI_INTERNALS__` and rejects AFTER the test completes,
// producing an unhandled rejection that intermittently reds CI. Mock the binding
// so the backend write resolves cleanly here — belt-and-suspenders with the
// `.catch()` guard now at the fire-and-forget call site in `altitude-settings.ts`.
vi.mock('@/bindings/index', () => ({
  commands: {
    settingsUpdate: vi.fn().mockResolvedValue({ status: 'ok', data: null }),
    settingsGet: vi.fn().mockResolvedValue({ status: 'ok', data: {} }),
  },
}));

import {
  getAltitudeThreshold,
  setAltitudeThreshold,
  ALTITUDE_THRESHOLD_MIN,
  ALTITUDE_THRESHOLD_MAX,
} from './altitude-settings';
import {
  __setObservingStateForTest,
  DEFAULT_USABLE_ALTITUDE_DEG,
  loadObservingState,
  USABLE_ALTITUDE_KEY,
  OBSERVING_SCOPE,
} from '@/shared/observing-sites/site-store';
import { commands } from '@/bindings/index';

beforeEach(() => {
  __setObservingStateForTest({});
});

describe('getAltitudeThreshold', () => {
  it('returns DEFAULT_USABLE_ALTITUDE_DEG when nothing is stored', () => {
    expect(getAltitudeThreshold()).toBe(DEFAULT_USABLE_ALTITUDE_DEG);
  });

  it('returns the stored value after setAltitudeThreshold', () => {
    setAltitudeThreshold(25);
    expect(getAltitudeThreshold()).toBe(25);
  });
});

describe('setAltitudeThreshold', () => {
  it('clamps values below ALTITUDE_THRESHOLD_MIN to the minimum', () => {
    setAltitudeThreshold(ALTITUDE_THRESHOLD_MIN - 10);
    expect(getAltitudeThreshold()).toBe(ALTITUDE_THRESHOLD_MIN);
  });

  it('clamps values above ALTITUDE_THRESHOLD_MAX to the maximum', () => {
    setAltitudeThreshold(ALTITUDE_THRESHOLD_MAX + 10);
    expect(getAltitudeThreshold()).toBe(ALTITUDE_THRESHOLD_MAX);
  });

  it('accepts ALTITUDE_THRESHOLD_MIN as a valid boundary', () => {
    setAltitudeThreshold(ALTITUDE_THRESHOLD_MIN);
    expect(getAltitudeThreshold()).toBe(ALTITUDE_THRESHOLD_MIN);
  });

  it('accepts ALTITUDE_THRESHOLD_MAX as a valid boundary', () => {
    setAltitudeThreshold(ALTITUDE_THRESHOLD_MAX);
    expect(getAltitudeThreshold()).toBe(ALTITUDE_THRESHOLD_MAX);
  });

  it('updates the shared observing state cache the planner reads from', () => {
    setAltitudeThreshold(42);
    expect(getAltitudeThreshold()).toBe(42);
  });
});

describe('usable-altitude backend round-trip (#823)', () => {
  it('sends the clamped value to settings.update under usableAltitudeDeg/observing', async () => {
    setAltitudeThreshold(45);
    // saveUsableAltitude awaits the write; flush a microtask so it lands.
    await Promise.resolve();
    await Promise.resolve();
    expect(commands.settingsUpdate).toHaveBeenCalledWith(OBSERVING_SCOPE, {
      [USABLE_ALTITUDE_KEY]: 45,
    });
  });

  it('a reload (loadObservingState) after a persisted write returns the saved value, not the default', async () => {
    setAltitudeThreshold(45);
    await Promise.resolve();
    await Promise.resolve();

    // Simulate what a fresh app session's settings.get would now return.
    vi.mocked(commands.settingsGet).mockResolvedValueOnce({
      status: 'ok',
      data: { scope: OBSERVING_SCOPE, values: { [USABLE_ALTITUDE_KEY]: 45 } },
    });
    await loadObservingState();

    expect(getAltitudeThreshold()).toBe(45);
  });
});
