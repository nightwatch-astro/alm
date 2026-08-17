// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * Cleanup settings pane — cleanup policy control (issue #804).
 *
 * The pane used to render a 15-row `CLEANUP_TYPES` fixture table writing a
 * `cleanupTypeOverrides` settings blob that no scan path read. It now edits the
 * real `cleanup.policy.get`/`cleanup.policy.update` model — the one
 * `cleanup_scan`/`cleanup_plan_generate` consult — keyed
 * `intermediate`/`master`/`final`.
 *
 * Covers: load, per-type round-trip, auto-on-completion round-trip, the
 * protected-category warning, restore-defaults, and the stale-fetch race.
 */
import { screen, fireEvent, waitFor, within } from '@testing-library/react';
import { renderWithQuery as render } from '@/test/renderWithQuery';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { ZodType } from 'zod';

const { mockGetSettings, mockPolicyGet, mockPolicyUpdate } = vi.hoisted(() => ({
  mockGetSettings: vi.fn(),
  mockPolicyGet: vi.fn(),
  mockPolicyUpdate: vi.fn(),
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
    cleanupPolicyGet: mockPolicyGet,
    cleanupPolicyUpdate: mockPolicyUpdate,
  };
});

import { Cleanup } from './Cleanup';

const ALL_KEEP = {
  entries: [
    { dataType: 'intermediate', action: 'keep' },
    { dataType: 'master', action: 'keep' },
    { dataType: 'final', action: 'keep' },
  ],
  autoOnCompletion: false,
};

/** The pane's row for one policy data type, located by its visible label. */
function policyRow(label: string) {
  return screen.getByText(label).closest('.pv-settings__row') as HTMLElement;
}

beforeEach(() => {
  vi.clearAllMocks();
  mockGetSettings.mockResolvedValue({ values: {} });
  mockPolicyGet.mockResolvedValue(ALL_KEEP);
  mockPolicyUpdate.mockImplementation((req) => Promise.resolve(req));
});

describe('Cleanup — cleanup policy control (issue #804)', () => {
  it('reflects the persisted policy rather than an all-Keep assumption', async () => {
    mockPolicyGet.mockResolvedValue({
      entries: [
        { dataType: 'intermediate', action: 'delete' },
        { dataType: 'master', action: 'keep' },
        { dataType: 'final', action: 'keep' },
      ],
      autoOnCompletion: true,
    });
    render(<Cleanup save={vi.fn()} />);

    await waitFor(() => {
      expect(
        policyRow('Intermediate files').querySelector('.pv-seg__btn--active'),
      ).toHaveTextContent('Delete');
    });
    expect(
      screen.getByRole('checkbox', { name: 'Scan on project completion' }),
    ).toBeChecked();
  });

  it('defaults every data type to Keep when the backend has no stored policy', async () => {
    render(<Cleanup save={vi.fn()} />);

    for (const label of [
      'Intermediate files',
      'Calibration masters',
      'Final images',
    ]) {
      await waitFor(() => {
        expect(
          policyRow(label).querySelector('.pv-seg__btn--active'),
        ).toHaveTextContent('Keep');
      });
    }
  });

  it('changing a data type persists the whole policy via cleanup_policy_update', async () => {
    render(<Cleanup save={vi.fn()} />);
    await waitFor(() => expect(mockPolicyGet).toHaveBeenCalled());

    const row = policyRow('Intermediate files');
    fireEvent.click(within(row).getByRole('radio', { name: 'Archive' }));

    await waitFor(() => {
      expect(mockPolicyUpdate).toHaveBeenCalledWith({
        entries: [
          { dataType: 'intermediate', action: 'archive' },
          { dataType: 'master', action: 'keep' },
          { dataType: 'final', action: 'keep' },
        ],
        autoOnCompletion: false,
      });
    });
    expect(row.querySelector('.pv-seg__btn--active')).toHaveTextContent(
      'Archive',
    );
  });

  it('toggling scan-on-completion persists it alongside the current actions', async () => {
    render(<Cleanup save={vi.fn()} />);
    await waitFor(() => expect(mockPolicyGet).toHaveBeenCalled());

    fireEvent.click(
      screen.getByRole('checkbox', { name: 'Scan on project completion' }),
    );

    await waitFor(() => {
      expect(mockPolicyUpdate).toHaveBeenCalledWith(
        expect.objectContaining({ autoOnCompletion: true }),
      );
    });
  });

  it('warns when a protected data type is opted into a destructive action', async () => {
    render(<Cleanup save={vi.fn()} />);
    await waitFor(() => expect(mockPolicyGet).toHaveBeenCalled());

    expect(screen.queryByText(/Protected categories are set/)).toBeNull();

    fireEvent.click(
      within(policyRow('Calibration masters')).getByRole('radio', {
        name: 'Delete',
      }),
    );

    await waitFor(() => {
      expect(
        screen.getByText(/Protected categories are set.*Calibration masters/),
      ).toBeInTheDocument();
    });
  });

  it('restore defaults writes the all-Keep policy back and re-hydrates from the response', async () => {
    mockPolicyGet.mockResolvedValue({
      entries: [
        { dataType: 'intermediate', action: 'delete' },
        { dataType: 'master', action: 'keep' },
        { dataType: 'final', action: 'keep' },
      ],
      autoOnCompletion: true,
    });
    mockPolicyUpdate.mockResolvedValue(ALL_KEEP);
    render(<Cleanup save={vi.fn()} />);

    await waitFor(() => {
      expect(
        policyRow('Intermediate files').querySelector('.pv-seg__btn--active'),
      ).toHaveTextContent('Delete');
    });

    fireEvent.click(
      screen.getByRole('button', { name: /cleanup policy defaults/ }),
    );

    await waitFor(() => {
      expect(mockPolicyUpdate).toHaveBeenCalledWith(ALL_KEEP);
    });
    await waitFor(() => {
      expect(
        policyRow('Intermediate files').querySelector('.pv-seg__btn--active'),
      ).toHaveTextContent('Keep');
    });
    expect(
      screen.getByRole('checkbox', { name: 'Scan on project completion' }),
    ).not.toBeChecked();
  });

  it('a policy edit before the mount fetch resolves is not clobbered by the stale response', async () => {
    let resolveGet: ((value: typeof ALL_KEEP) => void) | undefined;
    mockPolicyGet.mockReturnValue(
      new Promise((resolve) => {
        resolveGet = resolve;
      }),
    );

    render(<Cleanup save={vi.fn()} />);

    const row = policyRow('Final images');
    fireEvent.click(within(row).getByRole('radio', { name: 'Archive' }));
    await waitFor(() => {
      expect(row.querySelector('.pv-seg__btn--active')).toHaveTextContent(
        'Archive',
      );
    });

    resolveGet?.(ALL_KEEP);
    await new Promise((r) => setTimeout(r, 0));

    expect(row.querySelector('.pv-seg__btn--active')).toHaveTextContent(
      'Archive',
    );
  });
});
