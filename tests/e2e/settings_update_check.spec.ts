// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Journey 17 — Software update check (mock-mode validation, spec 051).
 *
 * Uses window.__PV_TEST__.updateState to seed specific update phases without
 * a real Tauri updater host. `startUpdateSubscription()` reads the hook before
 * the IS_MOCK guard, so it fires for every phase even in a VITE_USE_MOCKS env.
 *
 * Validates the Settings → Advanced Software Update section against the three
 * externally-observable phases: up-to-date, ready (update available), and
 * check-failed. The downloading/download-failed/restart-failed phases require
 * a real Tauri download step and are not covered here.
 */
import type { Page } from '@playwright/test';
import {
  test,
  expect,
  disableOnboarding,
  seedSetupComplete,
} from './support/harness';

/** Seed window.__PV_TEST__.updateState before the app boots. */
function seedUpdateState(page: Page, state: Record<string, unknown>): void {
  page.addInitScript((s) => {
    (window as Window & { __PV_TEST__?: unknown }).__PV_TEST__ = {
      updateState: s,
    };
  }, state);
}

test.beforeEach(async ({ page }) => {
  await disableOnboarding(page);
  seedSetupComplete(page);
});

test.describe('Journey 17 · Software update check (mock-mode, spec 051)', () => {
  test('shows "up to date" when update state is up-to-date', async ({
    page,
  }) => {
    seedUpdateState(page, { phase: 'up-to-date' });

    await page.goto('/#/settings/advanced');

    // Software Update section header renders.
    await expect(
      page.getByText('Software Update', { exact: true }).first(),
    ).toBeVisible();

    // Status reads the up-to-date message.
    await expect(page.getByTestId('update-status')).toHaveText(
      "You're running the latest version.",
    );

    // No action buttons visible in the up-to-date state.
    await expect(page.getByTestId('update-restart-btn')).toHaveCount(0);
    await expect(page.getByTestId('update-retry-btn')).toHaveCount(0);
  });

  test('shows "update available" with version when update state is ready', async ({
    page,
  }) => {
    seedUpdateState(page, {
      phase: 'ready',
      version: '99.0.0',
      body: 'Test release notes',
    });

    await page.goto('/#/settings/advanced');

    // Status reads the ready message with the spoofed version.
    await expect(page.getByTestId('update-status')).toHaveText(
      'Update 99.0.0 ready — restart to finish installing.',
    );

    // Restart & Install button is visible and enabled.
    await expect(page.getByTestId('update-restart-btn')).toBeVisible();
    await expect(page.getByTestId('update-restart-btn')).toBeEnabled();
  });

  test('shows "check failed" message when update state is check-failed', async ({
    page,
  }) => {
    seedUpdateState(page, {
      phase: 'check-failed',
      error: 'network unreachable',
    });

    await page.goto('/#/settings/advanced');

    // Status reads the check-failed message including the error detail.
    await expect(page.getByTestId('update-status')).toHaveText(
      "Couldn't check for updates: network unreachable",
    );

    // "Check again" retry button is visible.
    await expect(page.getByTestId('update-retry-btn')).toBeVisible();
    await expect(page.getByTestId('update-retry-btn')).toBeEnabled();

    // Restart button must NOT be shown (update not downloaded).
    await expect(page.getByTestId('update-restart-btn')).toHaveCount(0);
  });
});
