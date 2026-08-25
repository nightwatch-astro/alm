// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Spec 011 T018 — Settings tool-enablement flow: auto-detect → save → the
 * project CTA enables.
 *
 * The companion spec (`project_tool_launch_disabled.spec.ts`) pins the
 * disabled end of this behaviour. This one drives the transition, so a
 * regression in `ProcessingTools` wiring or in profile persistence fails a
 * test instead of leaving every project spec on the never-configured path.
 *
 * What each step distinguishes:
 *
 *   1. Auto-detect fills the path input WITHOUT persisting. The pane marks the
 *      row `Auto-detected` and the project CTA is still disabled, because a
 *      discovery suggestion the user has not accepted must not enable a launch.
 *   2. Saving the filled path (Enter) persists it. The `Auto-detected` pill
 *      gives way to `Available`, which is the pane's own read of
 *      `configured && available` coming back from the backend rather than
 *      local input state.
 *   3. Only then does the CTA on an unrelated route enable and its hint
 *      disappear. Asserting this after a navigation is the point: it proves
 *      the enablement came from the persisted profile, not from React state
 *      left over in the Settings pane.
 *
 * Viewport: 1600x900, matching the companion spec — at the default 1280x820
 * the CTA hint sits below the fold with no scrollable ancestor
 * (astro-plan-ltlo).
 */
import {
  test,
  expect,
  seedSetupComplete,
  disableOnboarding,
} from './support/harness';

const DETECTED_PIXINSIGHT_PATH = '/Applications/PixInsight/PixInsight.app';

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 900 });
  await disableOnboarding(page);
  seedSetupComplete(page);
});

/** Open the project whose `tool` resolves to the `pixinsight` profile. */
async function openNarrowbandProject(
  page: import('@playwright/test').Page,
): Promise<void> {
  await page.goto('/#/projects');
  const row = page
    .locator('[data-kind="projects-table-row"]')
    .filter({ hasText: 'NGC 7000 Narrowband' })
    .first();
  await expect(row).toBeVisible({ timeout: 8_000 });
  await row.click();
  await expect(page.getByTestId('tool-launch-btn')).toBeVisible({
    timeout: 8_000,
  });
}

test.describe('spec 011 · Settings tool enablement', () => {
  test('auto-detect fills the path without enabling, and saving it enables the project CTA', async ({
    page,
  }) => {
    // Baseline: no profile is configured, so the CTA is inert and says why.
    await openNarrowbandProject(page);
    await expect(page.getByTestId('tool-launch-btn')).toBeDisabled();
    await expect(page.getByTestId('tool-launch-footer')).toContainText(
      'Tool path not configured',
    );

    await page.goto('/#/settings/tools');
    const pathInput = page.getByLabel('Executable path for PixInsight');
    await expect(pathInput).toBeVisible({ timeout: 8_000 });
    await expect(pathInput).toHaveValue('');

    // Step 1 — discovery suggests a path but persists nothing.
    await page.getByLabel('Re-run auto-detect for all tools').click();
    await expect(pathInput).toHaveValue(DETECTED_PIXINSIGHT_PATH);
    const pixinsightRow = page
      .getByTestId('settings-row')
      .filter({ hasText: 'PixInsight' })
      .first();
    await expect(pixinsightRow).toContainText('Auto-detected');
    await expect(pixinsightRow).not.toContainText('Available');

    // An unsaved suggestion must not enable a launch: navigate away and back
    // so the CTA re-reads the persisted profile rather than any local state.
    await openNarrowbandProject(page);
    await expect(page.getByTestId('tool-launch-btn')).toBeDisabled();
    await expect(page.getByTestId('tool-launch-footer')).toContainText(
      'Tool path not configured',
    );

    // Step 2 — accept the suggestion. Enter commits, per ProcessingTools'
    // onKeyDown save; the pill flipping to `Available` is the pane reading the
    // saved profile back, not echoing the input.
    await page.goto('/#/settings/tools');
    const savedInput = page.getByLabel('Executable path for PixInsight');
    await expect(savedInput).toBeVisible({ timeout: 8_000 });
    await savedInput.fill(DETECTED_PIXINSIGHT_PATH);
    await savedInput.press('Enter');
    const savedRow = page
      .getByTestId('settings-row')
      .filter({ hasText: 'PixInsight' })
      .first();
    await expect(savedRow).toContainText('Available', { timeout: 8_000 });
    await expect(savedRow).not.toContainText('Auto-detected');

    // Step 3 — the CTA is now live and the explanatory hint is gone.
    await openNarrowbandProject(page);
    const btn = page.getByTestId('tool-launch-btn');
    await expect(btn).toBeEnabled({ timeout: 8_000 });
    await expect(btn).toHaveText('Open in PixInsight');
    await expect(page.getByTestId('tool-launch-footer')).toHaveCount(0);
  });

  test('a tool the user disabled stays unlaunchable even with a saved path', async ({
    page,
  }) => {
    // #656 in the browser: the toggle is authoritative over a configured path.
    // `toolLaunchDisabledReason` checks `enabled` before `configured`, so the
    // copy must fall back to `not configured` rather than claiming the
    // executable is missing.
    await page.goto('/#/settings/tools');
    const pathInput = page.getByLabel('Executable path for PixInsight');
    await expect(pathInput).toBeVisible({ timeout: 8_000 });
    await pathInput.fill(DETECTED_PIXINSIGHT_PATH);
    await pathInput.press('Enter');
    const row = page
      .getByTestId('settings-row')
      .filter({ hasText: 'PixInsight' })
      .first();
    await expect(row).toContainText('Available', { timeout: 8_000 });

    // Click the toggle's label, not its input: `Toggle` hides the checkbox
    // behind a styled track, so the input is never visible and a click on it
    // is not the gesture a user makes.
    const toggle = row.getByTestId('toggle');
    await expect(toggle.getByLabel('Enable PixInsight')).toBeChecked();
    await toggle.click();
    await expect(toggle.getByLabel('Enable PixInsight')).not.toBeChecked();

    await openNarrowbandProject(page);
    await expect(page.getByTestId('tool-launch-btn')).toBeDisabled();
    await expect(page.getByTestId('tool-launch-footer')).toContainText(
      'Tool path not configured',
    );
  });
});
