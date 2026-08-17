// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Staged update flow (#888, absorbs #869/#873) + running-version display
 * (#845). Exercises the frontend-driven check/download/restart state machine
 * directly against mocked `@tauri-apps/plugin-updater`/`plugin-process`/
 * `api/app` modules — `IS_MOCK` is false in the vitest environment (see
 * vitest.config.ts), so these dynamic imports actually resolve and must be
 * mocked per test.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

const { mockCheck, mockRelaunch, mockGetVersion } = vi.hoisted(() => ({
  mockCheck: vi.fn(),
  mockRelaunch: vi.fn(),
  mockGetVersion: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: mockCheck,
}));
vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: mockRelaunch,
}));
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: mockGetVersion,
}));

import {
  checkForUpdate,
  restartPendingUpdate,
  startUpdateSubscription,
  stopUpdateSubscription,
  getUpdateSnapshot,
  getRunningVersion,
} from './updateSubscription';

function makeUpdate(
  overrides: Partial<{ version: string; body: string | null }> = {},
) {
  return {
    version: '0.6.0',
    body: 'Release notes',
    downloadAndInstall: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('checkForUpdate', () => {
  it('sets up-to-date when no update is available', async () => {
    mockCheck.mockResolvedValue(null);

    await checkForUpdate();

    expect(getUpdateSnapshot()).toEqual({ phase: 'up-to-date' });
  });

  it('downloads and verifies immediately, landing on ready without relaunching (#888)', async () => {
    const update = makeUpdate();
    mockCheck.mockResolvedValue(update);

    await checkForUpdate();

    expect(update.downloadAndInstall).toHaveBeenCalledTimes(1);
    expect(mockRelaunch).not.toHaveBeenCalled();
    expect(getUpdateSnapshot()).toEqual({
      phase: 'ready',
      version: '0.6.0',
      body: 'Release notes',
    });
  });

  it('sets check-failed (not up-to-date) when the check itself rejects (#873)', async () => {
    mockCheck.mockRejectedValue(new Error('network unreachable'));

    await checkForUpdate();

    const snapshot = getUpdateSnapshot();
    expect(snapshot.phase).toBe('check-failed');
    expect(snapshot.error).toBe('network unreachable');
  });

  it('sets download-failed (not check-failed) when downloadAndInstall rejects', async () => {
    const update = makeUpdate({
      // @ts-expect-error overriding to a rejecting fn for this case
      downloadAndInstall: vi.fn().mockRejectedValue(new Error('disk full')),
    });
    mockCheck.mockResolvedValue(update);

    await checkForUpdate();

    const snapshot = getUpdateSnapshot();
    expect(snapshot.phase).toBe('download-failed');
    expect(snapshot.error).toBe('disk full');
    expect(snapshot.version).toBe('0.6.0');
  });
});

describe('restartPendingUpdate', () => {
  it('is a no-op outside ready/restart-failed', async () => {
    mockCheck.mockResolvedValue(null);
    await checkForUpdate(); // -> up-to-date

    await restartPendingUpdate();

    expect(mockRelaunch).not.toHaveBeenCalled();
  });

  it('relaunches once an update is ready', async () => {
    mockCheck.mockResolvedValue(makeUpdate());
    await checkForUpdate(); // -> ready
    mockRelaunch.mockResolvedValue(undefined);

    await restartPendingUpdate();

    expect(mockRelaunch).toHaveBeenCalledTimes(1);
  });

  it('sets restart-failed (not a failure banner) when relaunch fails after a successful install (#869)', async () => {
    mockCheck.mockResolvedValue(makeUpdate());
    await checkForUpdate(); // -> ready (downloadAndInstall already succeeded)
    mockRelaunch.mockRejectedValue(new Error('relaunch denied'));

    await restartPendingUpdate();

    const snapshot = getUpdateSnapshot();
    expect(snapshot.phase).toBe('restart-failed');
    expect(snapshot.error).toBe('relaunch denied');
    // The version stays visible — the point of #869 is that it IS installed.
    expect(snapshot.version).toBe('0.6.0');
  });

  it('retrying restart from restart-failed relaunches again', async () => {
    mockCheck.mockResolvedValue(makeUpdate());
    await checkForUpdate();
    mockRelaunch.mockRejectedValueOnce(new Error('relaunch denied'));
    await restartPendingUpdate(); // -> restart-failed

    mockRelaunch.mockResolvedValueOnce(undefined);
    await restartPendingUpdate();

    expect(mockRelaunch).toHaveBeenCalledTimes(2);
  });
});

describe('startUpdateSubscription — __PV_TEST__ override', () => {
  beforeEach(() => {
    stopUpdateSubscription();
    delete (window as Window & { __PV_TEST__?: unknown }).__PV_TEST__;
  });

  it('seeds state from window.__PV_TEST__.updateState, skipping the real check', async () => {
    (window as Window & { __PV_TEST__?: unknown }).__PV_TEST__ = {
      updateState: { phase: 'ready', version: '99.0.0' },
    };

    await startUpdateSubscription();

    expect(getUpdateSnapshot()).toEqual({ phase: 'ready', version: '99.0.0' });
    expect(mockCheck).not.toHaveBeenCalled();
  });

  it('is idempotent: a second call with the hook set does not re-seed', async () => {
    (window as Window & { __PV_TEST__?: unknown }).__PV_TEST__ = {
      updateState: { phase: 'up-to-date' },
    };

    await startUpdateSubscription();
    // Mutate the hook after first call — should have no effect.
    (window as Window & { __PV_TEST__?: unknown }).__PV_TEST__ = {
      updateState: { phase: 'check-failed', error: 'should not appear' },
    };
    await startUpdateSubscription();

    expect(getUpdateSnapshot()).toEqual({ phase: 'up-to-date' });
  });

  it('falls through to IS_MOCK no-op when the hook is absent', async () => {
    // No __PV_TEST__ set; IS_MOCK is false in vitest (VITE_USE_MOCKS unset),
    // so it will call checkForUpdate() — mock it to resolve immediately.
    mockCheck.mockResolvedValue(null);

    await startUpdateSubscription();

    expect(mockCheck).toHaveBeenCalledTimes(1);
  });
});

describe('getRunningVersion', () => {
  it('returns the version reported by the Tauri app API', async () => {
    mockGetVersion.mockResolvedValue('0.5.0');

    await expect(getRunningVersion()).resolves.toBe('0.5.0');
  });

  it('returns null when the API call fails', async () => {
    mockGetVersion.mockRejectedValue(new Error('unavailable'));

    await expect(getRunningVersion()).resolves.toBeNull();
  });
});
