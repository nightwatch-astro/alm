// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Playwright mock-e2e: adaptive detail-panel dock (spec 054 / #936).
 *
 * `ListPageLayout`'s `detailPlacement` DEFAULT is now `'adaptive'` (see
 * `apps/desktop/src/components/ListPageLayout.tsx` + `useAdaptiveDock` in
 * `apps/desktop/src/ui`): the detail panel docks to the SIDE when the window
 * is wide enough and falls back to the BOTTOM strip when narrow, with a
 * per-page pinned override + resizable side width persisted in
 * localStorage. Exercised against the Calibration page (an existing
 * ListPageLayout consumer that takes the default placement).
 *
 * Existing pins this spec must NOT break: `.pv-listpage__detail` stays the
 * outer detail-panel class in BOTH placements (only the `--side` modifier
 * differs) — `calibration_masters_matching.spec.ts:157` and
 * `inbox_ingest_confirm.spec.ts` select on that base class at the default
 * 1280x720 viewport, which resolves to bottom placement (below the 1400px
 * adaptive threshold) and is unaffected by this change.
 */
import {
  test,
  expect,
  seedSetupComplete,
  disableOnboarding,
} from './support/harness';

test.describe('adaptive detail-panel dock (spec 054 / #936)', () => {
  test('docks to the side at a wide viewport and falls back to the bottom at 1100x720', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.goto('/#/calibration');
    await disableOnboarding(page);
    await expect(page.locator('[data-testid="calib-table"]')).toBeVisible({
      timeout: 8_000,
    });

    const darkRow = page
      .locator('[data-kind="calib-table-row"]')
      .filter({ hasText: 'Master Dark · 120s' });
    await darkRow.click();
    const detail = page.locator('[data-testid="listpage-detail"]');
    await expect(detail).toBeVisible({ timeout: 5_000 });
    await expect(detail).toHaveAttribute('data-dock', 'side');
    await expect(page.getByTestId('dock-resize-handle')).toBeVisible();

    // Shrink to the shell's enforced minimum — bottom is the universal
    // narrow fallback (decision #8); the resize handle disappears with it.
    await page.setViewportSize({ width: 1100, height: 720 });
    await expect(detail).toHaveAttribute('data-dock', 'bottom');
    await expect(page.getByTestId('dock-resize-handle')).not.toBeVisible();
    // The panel itself, and its content, remain intact through the switch.
    await expect(detail).toContainText('Poseidon-C PRO');
  });

  test('pinning to side persists the placement across a reload', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.goto('/#/calibration');
    await disableOnboarding(page);
    await expect(page.locator('[data-testid="calib-table"]')).toBeVisible({
      timeout: 8_000,
    });

    await page
      .locator('[data-kind="calib-table-row"]')
      .filter({ hasText: 'Master Dark · 120s' })
      .click();
    const detail = page.locator('[data-testid="listpage-detail"]');
    await expect(detail).toBeVisible({ timeout: 5_000 });
    await expect(detail).toHaveAttribute('data-dock', 'bottom');

    await page.getByRole('radio', { name: 'Right' }).click();
    await expect(detail).toHaveAttribute('data-dock', 'side');

    await page.reload();
    await disableOnboarding(page);
    await page
      .locator('[data-kind="calib-table-row"]')
      .filter({ hasText: 'Master Dark · 120s' })
      .click();
    const detailAfterReload = page.locator('[data-testid="listpage-detail"]');
    await expect(detailAfterReload).toBeVisible({ timeout: 5_000 });
    // The pin survives the reload even though 1024 is below the auto threshold.
    await expect(detailAfterReload).toHaveAttribute('data-dock', 'side');
  });
});
