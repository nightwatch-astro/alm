// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * PlannerSettings.test.tsx — Settings → Target Planner moon-avoidance table
 * (spec 047 T015).
 *
 * Mocks the generated command surface (same approach as
 * targets/guidance-settings.test.ts) so the pane's persistence round-trip,
 * validation clamps, and reset action can be exercised without a backend.
 */

import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const { settingsGet, settingsUpdate, settingsRestoreDefaults } = vi.hoisted(
  () => ({
    settingsGet: vi.fn(),
    settingsUpdate: vi.fn(),
    settingsRestoreDefaults: vi.fn(),
  }),
);
vi.mock('@/bindings/index', () => ({
  commands: { settingsGet, settingsUpdate, settingsRestoreDefaults },
}));
vi.mock('@/api/ipc', () => ({ unwrap: (v: unknown) => v }));

import { PlannerSettings } from './PlannerSettings';
import { __resetGuidanceParamsForTest } from '@/shared/planner/guidance-settings';
import { DEFAULT_MOON_AVOIDANCE } from '@/features/targets/astro/moon-avoidance';
import {
  __setObservingStateForTest,
  USABLE_ALTITUDE_KEY,
  OBSERVING_SCOPE,
} from '@/shared/observing-sites/site-store';

beforeEach(() => {
  vi.clearAllMocks();
  __resetGuidanceParamsForTest();
  __setObservingStateForTest({});
  settingsUpdate.mockResolvedValue(null);
  settingsRestoreDefaults.mockResolvedValue({
    restored: ['plannerMoonAvoidance'],
  });
  settingsGet.mockResolvedValue({ scope: 'planner', values: {} });
});
afterEach(() => {
  __resetGuidanceParamsForTest();
  __setObservingStateForTest({});
});

describe('PlannerSettings — moon avoidance (spec 047 T015)', () => {
  it('renders a distance + width input for every band, seeded with shipped defaults', () => {
    render(<PlannerSettings />);
    expect(screen.getByTestId('guidance-distance-L')).toHaveValue(
      DEFAULT_MOON_AVOIDANCE.L.distanceDeg,
    );
    expect(screen.getByTestId('guidance-width-L')).toHaveValue(
      DEFAULT_MOON_AVOIDANCE.L.widthDays,
    );
    expect(screen.getByTestId('guidance-distance-Ha')).toHaveValue(
      DEFAULT_MOON_AVOIDANCE.Ha.distanceDeg,
    );
    expect(screen.getByTestId('guidance-distance-OIII')).toHaveValue(
      DEFAULT_MOON_AVOIDANCE.OIII.distanceDeg,
    );
  });

  it('commits an edited distance on blur via settings.update', async () => {
    render(<PlannerSettings />);
    const input = screen.getByTestId(
      'guidance-distance-Ha',
    ) as HTMLInputElement;
    fireEvent.change(input, { target: { value: '90' } });
    fireEvent.blur(input);
    expect(settingsUpdate).toHaveBeenCalledWith(
      'planner',
      expect.objectContaining({
        plannerMoonAvoidance: expect.objectContaining({
          Ha: {
            distanceDeg: 90,
            widthDays: DEFAULT_MOON_AVOIDANCE.Ha.widthDays,
          },
        }),
      }),
    );
  });

  it('clamps an out-of-range width to the valid max', async () => {
    render(<PlannerSettings />);
    const input = screen.getByTestId('guidance-width-OIII') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '999' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(settingsUpdate).toHaveBeenCalledWith(
      'planner',
      expect.objectContaining({
        plannerMoonAvoidance: expect.objectContaining({
          OIII: expect.objectContaining({ widthDays: 30 }),
        }),
      }),
    );
  });

  it('reset-to-defaults calls settings.restore-defaults for the key', async () => {
    render(<PlannerSettings />);
    const resetBtn = screen.getByText('Restore defaults');
    fireEvent.click(resetBtn);
    expect(settingsRestoreDefaults).toHaveBeenCalledWith({
      keys: ['plannerMoonAvoidance'],
    });
  });
});

describe('PlannerSettings — usable altitude threshold (#823)', () => {
  function altitudeInput(): HTMLInputElement {
    return screen.getByLabelText(
      'Usable altitude threshold in degrees',
    ) as HTMLInputElement;
  }

  it('persists a valid in-range edit via settings.update under observing/usableAltitudeDeg', async () => {
    render(<PlannerSettings />);
    fireEvent.change(altitudeInput(), { target: { value: '45' } });
    fireEvent.blur(altitudeInput());

    expect(settingsUpdate).toHaveBeenCalledWith(OBSERVING_SCOPE, {
      [USABLE_ALTITUDE_KEY]: 45,
    });
  });

  it('clamps an out-of-range commit (150) to the max (90) in the displayed field', async () => {
    render(<PlannerSettings />);
    fireEvent.change(altitudeInput(), { target: { value: '150' } });
    fireEvent.blur(altitudeInput());

    expect(altitudeInput().value).toBe('90');
    expect(settingsUpdate).toHaveBeenCalledWith(OBSERVING_SCOPE, {
      [USABLE_ALTITUDE_KEY]: 90,
    });
  });
});
