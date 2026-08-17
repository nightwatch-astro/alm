// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Sources-store corrupted-storage fallback.
 *
 * `loadSources` validates each persisted entry on its own, so one malformed
 * entry cannot discard the user's other registered folders.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('./registerSources', () => ({ registerRootBatch: vi.fn() }));

import { loadSources, saveSources } from './sources-store';

const STORAGE_KEY = 'alm-setup-wizard-state';

beforeEach(() => {
  window.localStorage.clear();
});

describe('loadSources with a corrupted stored value', () => {
  it('returns an empty list for unparseable JSON', () => {
    window.localStorage.setItem(STORAGE_KEY, '{not json');
    expect(loadSources()).toEqual([]);
  });

  it('returns an empty list when sources is not an array', () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ sources: 'lights' }),
    );
    expect(loadSources()).toEqual([]);
  });

  it('keeps the valid entries and drops only the malformed ones', () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        sources: [
          { path: '/astro/lights', kind: 'light_frames' },
          { path: '/astro/bogus', kind: 'not_a_kind' },
          { kind: 'project' },
          'nonsense',
          { path: '/astro/projects', kind: 'project' },
        ],
      }),
    );
    expect(loadSources()).toEqual([
      {
        path: '/astro/lights',
        kind: 'light_frames',
        organizationState: 'organized',
      },
      {
        path: '/astro/projects',
        kind: 'project',
        organizationState: 'organized',
      },
    ]);
  });

  it('forces inbox entries to unorganized regardless of what was persisted', () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        sources: [
          {
            path: '/astro/inbox',
            kind: 'inbox',
            organizationState: 'organized',
          },
        ],
      }),
    );
    expect(loadSources()[0]?.organizationState).toBe('unorganized');
  });

  it('backfills organizationState for entries written before the field existed', () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        sources: [{ path: '/astro/cal', kind: 'calibration' }],
      }),
    );
    expect(loadSources()[0]?.organizationState).toBe('organized');
  });
});

describe('saveSources', () => {
  it('preserves the sibling wizard-state keys it does not own', () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ currentStep: 4, version: 2 }),
    );
    saveSources([
      {
        path: '/astro/lights',
        kind: 'light_frames',
        organizationState: 'organized',
      },
    ]);
    const stored = JSON.parse(
      window.localStorage.getItem(STORAGE_KEY) ?? '{}',
    ) as Record<string, unknown>;
    expect(stored.currentStep).toBe(4);
    expect(stored.version).toBe(2);
    expect(loadSources()).toHaveLength(1);
  });
});
