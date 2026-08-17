// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Subscription ordering: `listen('log:entry')` must attach before the
 * `log.recent` hydration call.
 *
 * The backend forwarder advances its cursor as it emits, so a row committed
 * inside a hydrate-then-listen window is emitted to nobody and skipped by every
 * later drain. The order is the fix, so the test asserts the order.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';

const calls: string[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => {
    calls.push('listen');
    return () => {};
  }),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    logRecent: vi.fn(async () => {
      calls.push('logRecent');
      return {
        status: 'ok',
        data: { truncated: false, truncatedCount: null, entries: [] },
      };
    }),
  },
}));

vi.mock('@/api/ipc', () => ({
  unwrap: (r: { data: unknown }) => r.data,
}));

describe('startLogSubscription ordering', () => {
  beforeEach(async () => {
    calls.length = 0;
    vi.resetModules();
    // The mock factories are module-scoped, so their call records outlive a
    // module reset and would otherwise carry the previous test's arguments.
    const { commands } = await import('@/bindings/index');
    vi.mocked(commands.logRecent).mockClear();
  });

  it('attaches the live listener before hydrating', async () => {
    const { startLogSubscription } = await import('./logSubscription');
    await startLogSubscription();

    expect(calls).toEqual(['listen', 'logRecent']);
  });

  it('is idempotent across repeated calls', async () => {
    const { startLogSubscription } = await import('./logSubscription');
    await startLogSubscription();
    await startLogSubscription();

    expect(calls).toEqual(['listen', 'logRecent']);
  });

  it('passes the newest aud cursor from the buffer, not a live arrival', async () => {
    // Import the store first: `logSubscription` must resolve to this same module
    // instance, so it has to be imported after the buffer is seeded.
    const store = await import('./logStore');
    store.resetLogStore();
    store.appendLog([
      {
        id: 'aud:5',
        contractVersion: '1',
        time: '2026-01-01T00:00:05Z',
        level: 'info',
        source: 'plan',
        message: 'seeded',
      },
    ]);

    const { commands } = await import('@/bindings/index');
    const { startLogSubscription } = await import('./logSubscription');
    await startLogSubscription();

    expect(vi.mocked(commands.logRecent).mock.calls[0][0]).toBe('aud:5');
  });
});
