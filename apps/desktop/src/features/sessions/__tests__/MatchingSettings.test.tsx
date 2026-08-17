// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />

/**
 * MatchingSettingsPanel component tests.
 *
 * Spec 062 FR-030: risky-but-valid = yellow pill; out-of-bounds = red + unsaveable.
 * FR-031: saves affect future suggestions only (notice shown).
 * FR-027/028: hard bounds enforced per field.
 *
 * Tests:
 * 1. Shows "future only" notice.
 * 2. Save button disabled when a field is out of bounds.
 * 3. Shows out-of-bounds pill when value exceeds hard max.
 * 4. Shows yellow-warning pill when value is in yellow range.
 * 5. Save button enabled when all fields are in valid range.
 * 6. Loading state shown while fetching.
 * 7. Error state shown when fetch fails.
 * 8. Acknowledge modal appears when yellow issues exist on save attempt.
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MatchingSettingsPanel } from '../MatchingSettingsPanel';
import type { MatchingSettings } from '../groupsTypes';

// ── Mocks ─────────────────────────────────────────────────────────────────────

const mockSettings: MatchingSettings = {
  revision: 1,
  sameSession: {
    coverageMinPercent: 95,
    centerSeparationMaxPercent: 2,
    rotationMaxDeg: 1,
  },
  sibling: {
    coverageMinPercent: 90,
    centerSeparationMaxPercent: 5,
    rotationMaxDeg: 5,
  },
  mosaic: {
    overlapMinPercent: 5,
    overlapMaxPercent: 40,
    residualSkyRotationCapDeg: 10,
  },
  darkThermal: { moderateDeg: 0.5, severeDeg: 2 },
  calibrationAge: [],
  flatOrientation: { normalThroughDeg: 2, redAboveDeg: 5 },
  flatAge: { redAfterNights: 7 },
  updatedAt: '2026-07-01T00:00:00Z',
  updatedBy: 'system',
};

const mockSettingsQuery = {
  data: mockSettings as MatchingSettings | undefined,
  isLoading: false,
  isError: false,
};

const mockValidate = vi.fn();
const mockUpdateMutation = { mutateAsync: vi.fn(), isPending: false };

vi.mock('../useGroupsStore', async (importOriginal) => {
  const original = await importOriginal<typeof import('../useGroupsStore')>();
  return {
    ...original,
    useMatchingSettings: vi.fn(() => mockSettingsQuery),
    useMatchingSettingsUpdate: vi.fn(() => mockUpdateMutation),
    // Re-export the real validateFieldSeverity and bounds
    validateFieldSeverity: original.validateFieldSeverity,
    MATCHING_SETTINGS_BOUNDS: original.MATCHING_SETTINGS_BOUNDS,
  };
});

vi.mock('../sessionsGroupsIpc', async (importOriginal) => {
  const original =
    await importOriginal<typeof import('../sessionsGroupsIpc')>();
  return {
    ...original,
    matchingSettingsValidate: (...args: unknown[]) => mockValidate(...args),
  };
});

vi.stubEnv('VITE_USE_MOCKS', 'true');

function renderPanel() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MatchingSettingsPanel />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  mockSettingsQuery.data = { ...mockSettings };
  mockSettingsQuery.isLoading = false;
  mockSettingsQuery.isError = false;
  mockValidate.mockResolvedValue({
    valid: true,
    issues: [],
    effective: mockSettings,
  });
  mockUpdateMutation.mutateAsync.mockResolvedValue({
    settings: { ...mockSettings, revision: 2 },
    warnings: [],
    auditId: 'a1',
  });
});

describe('MatchingSettingsPanel', () => {
  it('1. shows future-only notice', () => {
    renderPanel();
    expect(screen.getByText(/future suggestions only/i)).toBeInTheDocument();
  });

  it('2. save button disabled when a field is out of bounds', async () => {
    renderPanel();
    // Wait for settings to populate the form
    await waitFor(() =>
      expect(
        screen.getByTestId('settings-row-ss-coverage'),
      ).toBeInTheDocument(),
    );
    // Set coverage to 89.9 (below hard min 90)
    const input = screen
      .getByTestId('settings-row-ss-coverage')
      .querySelector('input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '89.9' } });
    const saveBtn = screen.getByTestId('settings-save-btn');
    expect(saveBtn).toBeDisabled();
  });

  it('3. shows out-of-bounds pill when value exceeds hard max', async () => {
    renderPanel();
    await waitFor(() =>
      expect(
        screen.getByTestId('settings-row-ss-coverage'),
      ).toBeInTheDocument(),
    );
    const input = screen
      .getByTestId('settings-row-ss-coverage')
      .querySelector('input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '100' } }); // above 99.5
    expect(screen.getByText(/out of bounds/i)).toBeInTheDocument();
  });

  it('4. shows yellow-warning pill when value is in yellow range', async () => {
    renderPanel();
    await waitFor(() =>
      expect(
        screen.getByTestId('settings-row-ss-coverage'),
      ).toBeInTheDocument(),
    );
    const input = screen
      .getByTestId('settings-row-ss-coverage')
      .querySelector('input') as HTMLInputElement;
    // Set to 91 (below yellowBelow: 93)
    fireEvent.change(input, { target: { value: '91' } });
    expect(screen.getByText(/risky but valid/i)).toBeInTheDocument();
  });

  it('5. save button enabled when all fields are valid', async () => {
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('settings-save-btn')).not.toBeDisabled(),
    );
  });

  it('6. loading state shown while fetching', () => {
    mockSettingsQuery.data = undefined;
    mockSettingsQuery.isLoading = true;
    renderPanel();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('7. error state shown when fetch fails', () => {
    mockSettingsQuery.data = undefined;
    mockSettingsQuery.isLoading = false;
    mockSettingsQuery.isError = true;
    renderPanel();
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent(/could not load/i);
  });

  it('8. acknowledge modal appears when yellow issues returned from validate', async () => {
    mockValidate.mockResolvedValue({
      valid: true,
      issues: [
        {
          code: 'ss.coverage.yellow',
          severity: 'yellow',
          fieldPaths: ['sameSession.coverageMinPercent'],
          values: [{ fieldPath: 'sameSession.coverageMinPercent', value: 91 }],
          messageKey: 'settings_yellow_warning',
        },
      ],
      effective: mockSettings,
    });

    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('settings-save-btn')).not.toBeDisabled(),
    );

    fireEvent.click(screen.getByTestId('settings-save-btn'));

    await waitFor(() =>
      expect(
        screen.getByTestId('settings-acknowledge-modal'),
      ).toBeInTheDocument(),
    );
  });
});
