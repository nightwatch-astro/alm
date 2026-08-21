// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * theme.persistence.test.ts — theme-settings-db.
 *
 * The settings DB (`general` scope, `theme` key) is the durable source of
 * truth for the theme choice; localStorage (`pv.theme`) is kept only as a
 * synchronous boot cache so `initAppearance()` can paint before first render
 * without waiting on IPC (avoiding a flash of the wrong theme). Covers:
 * `setThemeChoice` writing both localStorage and the settings DB, and
 * `hydrateThemeFromSettings` reconciling the cache from the DB at boot.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

type IpcOutcome =
  | { status: 'ok'; data: unknown }
  | { status: 'error'; error: unknown };

const isTauriMock = vi.fn<() => boolean>();
const settingsGetMock = vi.fn<(scope: string) => Promise<IpcOutcome>>();
const settingsUpdateMock =
  vi.fn<(scope: string, values: unknown) => Promise<IpcOutcome>>();

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => isTauriMock(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme: vi.fn().mockResolvedValue(undefined) }),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    settingsGet: (scope: string) => settingsGetMock(scope),
    settingsUpdate: (scope: string, values: unknown) =>
      settingsUpdateMock(scope, values),
  },
}));

/** Poll briefly for an async mock to have been called (mirrors theme.test.ts). */
async function waitForCall(fn: ReturnType<typeof vi.fn>): Promise<void> {
  for (let i = 0; i < 50; i++) {
    if (fn.mock.calls.length > 0) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

describe('setThemeChoice — write-through to the settings DB', () => {
  beforeEach(() => {
    vi.resetModules();
    isTauriMock.mockReset();
    settingsGetMock.mockReset();
    settingsUpdateMock.mockReset();
    settingsUpdateMock.mockResolvedValue({ status: 'ok', data: null });
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('persists the choice to localStorage synchronously and to settings.update when inside Tauri', async () => {
    isTauriMock.mockReturnValue(true);
    const { setThemeChoice } = await import('./theme');

    setThemeChoice('espresso-dark');

    // The localStorage cache write is synchronous — no await needed.
    expect(localStorage.getItem('pv.theme')).toBe('espresso-dark');

    await waitForCall(settingsUpdateMock);
    expect(settingsUpdateMock).toHaveBeenCalledWith('general', {
      theme: 'espresso-dark',
    });
  });

  it('still writes localStorage but skips settings.update outside Tauri (no-op)', async () => {
    isTauriMock.mockReturnValue(false);
    const { setThemeChoice } = await import('./theme');

    setThemeChoice('warm-clay');

    expect(localStorage.getItem('pv.theme')).toBe('warm-clay');
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(settingsUpdateMock).not.toHaveBeenCalled();
  });

  it('degrades silently when settings.update rejects — never throws out of setThemeChoice', async () => {
    isTauriMock.mockReturnValue(true);
    settingsUpdateMock.mockRejectedValue(new Error('db unavailable'));
    const { setThemeChoice } = await import('./theme');

    expect(() => setThemeChoice('warm-slate')).not.toThrow();
    await waitForCall(settingsUpdateMock);
    // localStorage still reflects the choice even though the DB write failed.
    expect(localStorage.getItem('pv.theme')).toBe('warm-slate');
  });
});

describe('hydrateThemeFromSettings — reconcile the boot cache from the settings DB', () => {
  beforeEach(() => {
    vi.resetModules();
    isTauriMock.mockReset();
    settingsGetMock.mockReset();
    settingsUpdateMock.mockReset();
    settingsUpdateMock.mockResolvedValue({ status: 'ok', data: null });
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('overwrites a stale localStorage cache with the DB value (survives a WebView2 force-kill)', async () => {
    isTauriMock.mockReturnValue(true);
    localStorage.setItem('pv.theme', 'warm-clay');
    settingsGetMock.mockResolvedValue({
      status: 'ok',
      data: { scope: 'general', values: { theme: 'espresso-dark' } },
    });

    const { hydrateThemeFromSettings } = await import('./theme');
    await hydrateThemeFromSettings();

    expect(localStorage.getItem('pv.theme')).toBe('espresso-dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe(
      'espresso-dark',
    );

    // Adopting the DB value calls `setThemeChoice`, whose write-through is
    // fire-and-forget. Settle it here: left in flight it lands in the next
    // test, after its `mockReset`, and fails that test's assertion instead.
    await waitForCall(settingsUpdateMock);
    expect(settingsUpdateMock).toHaveBeenCalledWith('general', {
      theme: 'espresso-dark',
    });
  });

  it('leaves the cache untouched when the DB already agrees (no redundant write-back)', async () => {
    isTauriMock.mockReturnValue(true);
    localStorage.setItem('pv.theme', 'observatory-dark');
    settingsGetMock.mockResolvedValue({
      status: 'ok',
      data: { scope: 'general', values: { theme: 'observatory-dark' } },
    });

    const { hydrateThemeFromSettings } = await import('./theme');
    await hydrateThemeFromSettings();

    expect(settingsUpdateMock).not.toHaveBeenCalled();
  });

  it('ignores a malformed/unknown DB value and keeps the localStorage cache', async () => {
    isTauriMock.mockReturnValue(true);
    localStorage.setItem('pv.theme', 'warm-slate');
    settingsGetMock.mockResolvedValue({
      status: 'ok',
      data: { scope: 'general', values: { theme: 'not-a-real-theme' } },
    });

    const { hydrateThemeFromSettings, getThemeChoice } = await import(
      './theme'
    );
    await hydrateThemeFromSettings();

    expect(getThemeChoice()).toBe('warm-slate');
  });

  it('is a no-op outside Tauri (dev server / vitest)', async () => {
    isTauriMock.mockReturnValue(false);
    localStorage.setItem('pv.theme', 'warm-clay');

    const { hydrateThemeFromSettings } = await import('./theme');
    await hydrateThemeFromSettings();

    expect(settingsGetMock).not.toHaveBeenCalled();
    expect(localStorage.getItem('pv.theme')).toBe('warm-clay');
  });

  it('degrades silently when settings.get rejects — never throws', async () => {
    isTauriMock.mockReturnValue(true);
    localStorage.setItem('pv.theme', 'warm-clay');
    settingsGetMock.mockRejectedValue(new Error('db unavailable'));

    const { hydrateThemeFromSettings } = await import('./theme');
    await expect(hydrateThemeFromSettings()).resolves.toBeUndefined();
    expect(localStorage.getItem('pv.theme')).toBe('warm-clay');
  });
});

describe('THEMES registry — canonical vs. variant (handoff 03)', () => {
  beforeEach(() => {
    vi.resetModules();
    isTauriMock.mockReset();
    settingsUpdateMock.mockReset();
    settingsUpdateMock.mockResolvedValue({ status: 'ok', data: null });
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('marks exactly the four canonical themes enabled, keeping the two variants in the registry', async () => {
    const { THEMES } = await import('./theme');

    const enabled = THEMES.filter((t) => t.enabled)
      .map((t) => t.id)
      .sort();
    const disabled = THEMES.filter((t) => !t.enabled)
      .map((t) => t.id)
      .sort();

    expect(enabled).toEqual(
      [
        'observatory-cool',
        'observatory-cool-light',
        'observatory-dark',
        'warm-slate',
      ].sort(),
    );
    expect(disabled).toEqual(['espresso-dark', 'warm-clay'].sort());
  });

  it('groups the two new cool themes under the cool family, light before dark', async () => {
    const { THEMES } = await import('./theme');
    const cool = THEMES.filter((t) => t.family === 'cool');

    expect(cool.map((t) => t.id)).toEqual(
      expect.arrayContaining(['observatory-cool', 'observatory-cool-light']),
    );
    const light = cool.find((t) => t.id === 'observatory-cool-light');
    const dark = cool.find((t) => t.id === 'observatory-cool');
    expect(light?.mode).toBe('light');
    expect(dark?.mode).toBe('dark');
  });

  it('a disabled (picker-hidden) variant already chosen still resolves, applies, and persists', async () => {
    isTauriMock.mockReturnValue(true);
    const { setThemeChoice, getThemeChoice, resolveTheme } = await import(
      './theme'
    );

    setThemeChoice('espresso-dark');

    expect(getThemeChoice()).toBe('espresso-dark');
    expect(resolveTheme('espresso-dark')).toBe('espresso-dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe(
      'espresso-dark',
    );
    await waitForCall(settingsUpdateMock);
    expect(settingsUpdateMock).toHaveBeenCalledWith('general', {
      theme: 'espresso-dark',
    });
  });
});
