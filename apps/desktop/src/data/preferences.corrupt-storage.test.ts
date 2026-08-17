// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Preferences corrupted-storage fallback.
 *
 * `getPreferences` validates the persisted bag, so a garbage or wrong-shaped
 * stored value falls back to the in-code default for the affected key instead
 * of reaching the UI as a wrong shape.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';

const STORAGE_KEY = 'alm-preferences';

async function freshGetPreferences() {
  // The module caches its first read, so each case needs a fresh instance.
  // `resetPreferences()` cannot be used here: it also clears the stored key
  // each case has just seeded.
  vi.resetModules();
  const mod = await import('./preferences');
  return mod.getPreferences;
}

beforeEach(() => {
  window.localStorage.clear();
});

describe('getPreferences with a corrupted stored value', () => {
  it('falls back to defaults for unparseable JSON', async () => {
    window.localStorage.setItem(STORAGE_KEY, '{not json');
    const getPreferences = await freshGetPreferences();
    expect(getPreferences().density).toBe('comfortable');
    expect(getPreferences().sidebarCollapsed).toBe(false);
  });

  it('drops a field of the wrong type and keeps valid siblings', async () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ density: 42, sidebarCollapsed: true }),
    );
    const getPreferences = await freshGetPreferences();
    expect(getPreferences().density).toBe('comfortable');
    expect(getPreferences().sidebarCollapsed).toBe(true);
  });

  it('drops a retired enum member instead of applying it', async () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ sessionsView: 'grid', sessionsGroupBy: 'target' }),
    );
    const getPreferences = await freshGetPreferences();
    expect(getPreferences().sessionsView).toBe('list');
    expect(getPreferences().sessionsGroupBy).toBe('target');
  });

  it('loads a valid stored bag unchanged', async () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ density: 'compact', defaultProjectView: 'pipeline' }),
    );
    const getPreferences = await freshGetPreferences();
    expect(getPreferences().density).toBe('compact');
    expect(getPreferences().defaultProjectView).toBe('pipeline');
  });
});
