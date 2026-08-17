// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SourceViews settings pane (spec 049 T030).
 *
 * Covers:
 *   1. Loads persisted `sourceViewLinkKindIntraDrive`/`sourceViewLinkKindCrossDrive`
 *      values on mount and reflects them in the selects (not the in-code default).
 *   2. Defaults to hardlink (intra) / symlink (cross) when nothing is persisted.
 *   3. Changing a select calls `save('sourceViews', { ... })` with the new value.
 *   4. The cross-drive select never offers `hardlink` (FR-004a).
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { renderWithQuery as render } from '@/test/renderWithQuery';
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

import { SourceViews } from './SourceViews';

beforeEach(() => {
  vi.clearAllMocks();
  mockGetSettings.mockResolvedValue({ values: {} });
});

describe('SourceViews settings pane (spec 049 T030)', () => {
  it('defaults to hardlink (intra) / symlink (cross) when nothing is persisted', async () => {
    render(<SourceViews save={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByTestId('source-views-intra-drive-select')).toHaveValue(
        'hardlink',
      );
    });
    expect(screen.getByTestId('source-views-cross-drive-select')).toHaveValue(
      'symlink',
    );
  });

  it('loads persisted values and reflects them, not the in-code default', async () => {
    mockGetSettings.mockResolvedValue({
      values: {
        sourceViewLinkKindIntraDrive: 'symlink',
        sourceViewLinkKindCrossDrive: 'junction',
      },
    });
    render(<SourceViews save={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByTestId('source-views-intra-drive-select')).toHaveValue(
        'symlink',
      );
    });
    expect(screen.getByTestId('source-views-cross-drive-select')).toHaveValue(
      'junction',
    );
  });

  it('changing the intra-drive select calls save with the new value', async () => {
    const save = vi.fn();
    render(<SourceViews save={save} />);
    await waitFor(() => {
      expect(screen.getByTestId('source-views-intra-drive-select')).toHaveValue(
        'hardlink',
      );
    });

    fireEvent.change(screen.getByTestId('source-views-intra-drive-select'), {
      target: { value: 'symlink' },
    });

    expect(save).toHaveBeenCalledWith('sourceViews', {
      sourceViewLinkKindIntraDrive: 'symlink',
    });
  });

  it('never offers hardlink as a cross-drive option (FR-004a)', async () => {
    render(<SourceViews save={vi.fn()} />);
    await waitFor(() => {
      expect(
        screen.getByTestId('source-views-cross-drive-select'),
      ).toBeInTheDocument();
    });

    const options = Array.from(
      screen
        .getByTestId('source-views-cross-drive-select')
        .querySelectorAll('option'),
    ).map((o) => o.getAttribute('value'));
    expect(options).not.toContain('hardlink');
    expect(options).toEqual(['symlink', 'junction']);
  });
});
