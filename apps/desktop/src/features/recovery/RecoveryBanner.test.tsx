// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * RecoveryBanner tests (astro-plan-kyo7.48).
 *
 * 1. No banner on a clean shutdown.
 * 2. No banner when unclean but no plans were interrupted (self-healed).
 * 3. Banner shown when unclean AND plans interrupted; Review opens the overlay
 *    for the first interrupted plan; Dismiss hides the banner.
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { it, expect, vi, beforeEach } from 'vitest';

const { mockRecoveryStatus } = vi.hoisted(() => ({
  mockRecoveryStatus: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: { recoveryStatus: mockRecoveryStatus },
}));

// Stub the overlay so the test asserts on wiring (planId + open) without
// pulling in its data-fetching dependencies.
const { mockOverlay } = vi.hoisted(() => ({ mockOverlay: vi.fn() }));
vi.mock('@/features/plans/PlanReviewOverlay', () => ({
  PlanReviewOverlay: (props: { planId: string | null; open: boolean }) => {
    mockOverlay(props);
    return props.open ? <div data-testid="overlay">{props.planId}</div> : null;
  },
}));

import { RecoveryBanner } from './RecoveryBanner';

const ok = <T,>(data: T) => ({ status: 'ok' as const, data });

beforeEach(() => {
  mockRecoveryStatus.mockReset();
  mockOverlay.mockReset();
});

it('shows no banner after a clean shutdown', async () => {
  mockRecoveryStatus.mockResolvedValue(
    ok({ uncleanShutdown: false, interruptedPlanIds: ['p1'] }),
  );
  render(<RecoveryBanner />);
  await waitFor(() => expect(mockRecoveryStatus).toHaveBeenCalled());
  expect(screen.queryByTestId('recovery-banner')).toBeNull();
});

it('shows no banner when unclean but nothing was interrupted', async () => {
  mockRecoveryStatus.mockResolvedValue(
    ok({ uncleanShutdown: true, interruptedPlanIds: [] }),
  );
  render(<RecoveryBanner />);
  await waitFor(() => expect(mockRecoveryStatus).toHaveBeenCalled());
  expect(screen.queryByTestId('recovery-banner')).toBeNull();
});

it('surfaces the banner and opens review for the first interrupted plan', async () => {
  mockRecoveryStatus.mockResolvedValue(
    ok({ uncleanShutdown: true, interruptedPlanIds: ['plan-a', 'plan-b'] }),
  );
  render(<RecoveryBanner />);

  const banner = await screen.findByTestId('recovery-banner');
  expect(banner).toBeInTheDocument();

  fireEvent.click(screen.getByText('Review & resume'));
  const overlay = await screen.findByTestId('overlay');
  expect(overlay).toHaveTextContent('plan-a');
});

it('dismisses the banner', async () => {
  mockRecoveryStatus.mockResolvedValue(
    ok({ uncleanShutdown: true, interruptedPlanIds: ['plan-a'] }),
  );
  render(<RecoveryBanner />);
  await screen.findByTestId('recovery-banner');

  fireEvent.click(screen.getByText('Dismiss'));
  await waitFor(() =>
    expect(screen.queryByTestId('recovery-banner')).toBeNull(),
  );
});
