// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * Framing settings pane — clustering tolerance mount-race regression (spec
 * 008 Q27 F-Framing-11).
 *
 * Covers the flaky `tests/e2e/settings_framing.spec.ts` failure mode: a
 * two-phase input (onChange updates local state; blur commits + persists)
 * has a race window the Cleanup-style "edit before mount fetch resolves"
 * guard did not cover — the mount fetch can resolve in the gap between
 * onChange and blur, before `editedRef` was set, clobbering the typed value
 * back to the fetched one. The subsequent blur then reads that clobbered DOM
 * value and persists it, permanently losing the edit.
 */
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { ZodType } from 'zod';

const { mockGetSettings } = vi.hoisted(() => ({
  mockGetSettings: vi.fn().mockResolvedValue({ values: {} }),
}));
// Only the transport is stubbed; the real per-scope schema validation runs.
// `getSettingsTyped` is re-composed over the stub because its internal
// `getSettings` call resolves through the module's own binding, which replacing
// the export does not intercept.
vi.mock(import('./settingsIpc'), async (importOriginal) => {
  const actual = await importOriginal();
  return {
    ...actual,
    getSettings: mockGetSettings,
    getSettingsTyped: async <T extends Record<string, unknown>>(
      scope: string,
      schema: ZodType<T>,
    ): Promise<T> =>
      actual.parseSettingsValues(
        (await mockGetSettings({ scope })).values,
        schema,
      ),
  };
});

import { Framing } from './Framing';

beforeEach(() => {
  vi.clearAllMocks();
  mockGetSettings.mockResolvedValue({ values: {} });
});

describe('Framing — mount-race clobber regression (spec 008 Q27 F-Framing-11)', () => {
  it('an edit typed before the mount fetch resolves is not clobbered, and blur persists the typed value', async () => {
    // Mount-time `getSettings('framing')` is left unresolved until after the
    // user has already typed a new value (onChange only — not yet blurred),
    // reproducing the real race: React StrictMode's live effect invocation
    // can resolve after the keystroke but before the commit.
    let resolveGet:
      | ((value: { values: Record<string, unknown> }) => void)
      | undefined;
    mockGetSettings.mockReturnValue(
      new Promise((resolve) => {
        resolveGet = resolve;
      }),
    );

    const save = vi.fn();
    render(<Framing save={save} />);

    const input = await screen.findByTestId('framing-pointing-fraction-input');
    fireEvent.change(input, { target: { value: '0.25' } });
    expect(input).toHaveValue(0.25);

    // The stale fetch now resolves with the pre-edit default — it must be
    // ignored, not applied on top of the uncommitted edit.
    resolveGet?.({
      values: { framingPointingFractionOfFov: 0.1 },
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(input).toHaveValue(0.25);

    // Commit (blur) must persist the typed value, not a clobbered default.
    fireEvent.blur(input);
    await waitFor(() => {
      expect(save).toHaveBeenCalledWith('framing', {
        framingPointingFractionOfFov: 0.25,
      });
    });
    expect(input).toHaveValue(0.25);
  });

  it('loads the persisted value on mount when the user has not edited', async () => {
    mockGetSettings.mockResolvedValue({
      values: { framingPointingFractionOfFov: 0.25 },
    });
    render(<Framing save={vi.fn()} />);

    const input = await screen.findByTestId('framing-pointing-fraction-input');
    await waitFor(() => {
      expect(input).toHaveValue(0.25);
    });
  });
});
