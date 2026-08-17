// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * site-store race tests.
 *
 * `Shell.tsx` kicks off `loadObservingState()` at boot. On a slow backend that
 * read can still be in flight by the time the user reaches Targets and saves a
 * site or moves the usable-altitude slider — a read that started before the
 * write is stale by the time it resolves and must not clobber the newer value.
 *
 * This mirrors the `writeGen` guard added to the sibling `guidance-settings.ts`
 * in #836; the same store shape here was left unguarded.
 */

import { beforeEach, describe, it, expect, vi } from 'vitest';

const { settingsGet, settingsUpdate } = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  settingsUpdate: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: { settingsGet, settingsUpdate },
}));

vi.mock('@/api/ipc', () => ({
  unwrap: (r: unknown) => r,
}));

import {
  __setObservingStateForTest,
  getObservingState,
  getUsableAltitude,
  loadObservingState,
  saveSites,
  saveUsableAltitude,
  OBSERVING_SCOPE,
  SITES_KEY,
  DEFAULT_SITE_ID_KEY,
  ACTIVE_SITE_ID_KEY,
} from './site-store';
import type { ObserverSite } from './observer-site';

const SITE: ObserverSite = {
  id: 'site-1',
  name: 'Backyard',
  latitudeDeg: 52.1,
  longitudeDeg: 5.1,
  elevationM: 10,
  timezone: 'Europe/Amsterdam',
  twilight: 'astronomical',
  minHorizonAltDeg: 0,
};

beforeEach(() => {
  settingsGet.mockReset();
  settingsUpdate.mockReset();
  settingsUpdate.mockResolvedValue(null);
  __setObservingStateForTest({});
});

describe('loadObservingState vs saveSites race', () => {
  it('does not let a load started before a save clobber the just-saved sites', async () => {
    let resolveGet!: (v: unknown) => void;
    settingsGet.mockReturnValue(
      new Promise((resolve) => {
        resolveGet = resolve;
      }),
    );

    // Boot read starts...
    const loadPromise = loadObservingState();

    // ...then the user saves a site and it commits.
    await saveSites([SITE], SITE.id, SITE.id);
    expect(getObservingState().sites).toHaveLength(1);

    // The stale boot read now resolves with the pre-save (empty) values.
    resolveGet({
      scope: OBSERVING_SCOPE,
      values: {
        [SITES_KEY]: [],
        [DEFAULT_SITE_ID_KEY]: null,
        [ACTIVE_SITE_ID_KEY]: null,
      },
    });
    await loadPromise;

    expect(getObservingState().sites).toHaveLength(1);
    expect(getObservingState().activeSiteId).toBe(SITE.id);
  });

  it('does not let a failing load reset state saved while it was in flight', async () => {
    // The catch path writes EMPTY_STATE unconditionally before the fix.
    let rejectGet!: (e: unknown) => void;
    settingsGet.mockReturnValue(
      new Promise((_resolve, reject) => {
        rejectGet = reject;
      }),
    );

    const loadPromise = loadObservingState();
    await saveSites([SITE], SITE.id, SITE.id);

    rejectGet(new Error('backend unavailable'));
    await loadPromise;

    expect(getObservingState().sites).toHaveLength(1);
  });

  it('does not let a load clobber a usable-altitude change made while it was in flight', async () => {
    let resolveGet!: (v: unknown) => void;
    settingsGet.mockReturnValue(
      new Promise((resolve) => {
        resolveGet = resolve;
      }),
    );

    const loadPromise = loadObservingState();
    await saveUsableAltitude(42);
    expect(getUsableAltitude()).toBe(42);

    resolveGet({ scope: OBSERVING_SCOPE, values: {} });
    await loadPromise;

    expect(getUsableAltitude()).toBe(42);
  });

  it('still applies a load when no write raced it', async () => {
    // The guard must not disable loading outright.
    settingsGet.mockResolvedValue({
      scope: OBSERVING_SCOPE,
      values: {
        [SITES_KEY]: [SITE],
        [DEFAULT_SITE_ID_KEY]: SITE.id,
        [ACTIVE_SITE_ID_KEY]: SITE.id,
      },
    });

    await loadObservingState();

    expect(getObservingState().sites).toHaveLength(1);
    expect(getObservingState().activeSiteId).toBe(SITE.id);
  });
});

describe('saveUsableAltitude backend-failure rollback', () => {
  it('restores the pre-write value when the backend write rejects', async () => {
    __setObservingStateForTest({ usableAltitudeDeg: 20 });
    settingsUpdate.mockRejectedValue(new Error('backend unavailable'));

    await expect(saveUsableAltitude(55)).rejects.toThrow('backend unavailable');

    // The optimistic update must be rolled back.
    expect(getUsableAltitude()).toBe(20);
  });

  it('does not roll back when a newer write has committed after the failing one', async () => {
    __setObservingStateForTest({ usableAltitudeDeg: 20 });

    let rejectFirst!: (e: unknown) => void;
    settingsUpdate
      .mockReturnValueOnce(
        new Promise<null>((_resolve, reject) => {
          rejectFirst = reject;
        }),
      )
      .mockResolvedValueOnce(null);

    // First write (will fail) starts optimistically at 55...
    const firstWrite = saveUsableAltitude(55);
    // ...then a second write commits at 60.
    await saveUsableAltitude(60);
    expect(getUsableAltitude()).toBe(60);

    // The first write's backend call now rejects — must not roll back 60.
    rejectFirst(new Error('stale failure'));
    await expect(firstWrite).rejects.toThrow('stale failure');
    expect(getUsableAltitude()).toBe(60);
  });
});
