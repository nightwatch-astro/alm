// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Spec 011 T018 — browser smoke for the disabled "Open in {tool}" CTA and its
 * explanatory hint.
 *
 * Scope is deliberately narrow. The copy matrix (which string for which
 * `configured`/`available`/`enabled` combination) is already covered by
 * `apps/desktop/src/features/projects/tool-launch.test.ts`, and
 * `project_lifecycle_surfaces.spec.ts` already asserts the CTA renders
 * disabled with the hint present. This file asserts only what jsdom and
 * `toBeVisible()` cannot decide:
 *
 *   1. The disabled CTA dispatches NO click event even under a forced click.
 *      jsdom's `fireEvent.click` fires on a disabled button regardless, so a
 *      unit test cannot distinguish "genuinely inert" from "styled as
 *      disabled".
 *   2. Keyboard traversal skips it — a disabled button is not a tab stop.
 *   3. The hint has real layout (non-zero box) and is the top-most element at
 *      its own coordinates, i.e. not painted under a sticky header or another
 *      overlay. `toBeVisible()` passes for an element covered by an overlay.
 *
 * Viewport: 1600x900. At the app's default 1280x820 window the hint sits below
 * the fold with no scrollable ancestor, so it cannot be brought into view at
 * all — a layout defect tracked as astro-plan-ltlo, not something this spec
 * locks in.
 *
 * Mock wiring: `tools_list` has no mock handler, so `useToolProfiles()`
 * degrades to no profile and `toolLaunchDisabledReason()` returns
 * `not_configured` — the same path `project_lifecycle_surfaces.spec.ts`
 * documents.
 */
import {
  test,
  expect,
  seedSetupComplete,
  disableOnboarding,
  assertDefined,
} from './support/harness';

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 900 });
  await disableOnboarding(page);
  seedSetupComplete(page);
});

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

test.describe('spec 011 · disabled tool-launch CTA (browser-only behaviour)', () => {
  test('a forced click on the disabled CTA dispatches no click event and does not start a launch', async ({
    page,
  }) => {
    await openNarrowbandProject(page);
    const btn = page.getByTestId('tool-launch-btn');
    await expect(btn).toBeDisabled();

    // Count real click dispatches at the button itself. `force: true` skips
    // Playwright's own actionability wait, so this is the browser's own
    // disabled-button event suppression under test, not Playwright's guard.
    await page.evaluate(() => {
      const w = window as unknown as { __pvLaunchClicks: number };
      w.__pvLaunchClicks = 0;
      document
        .querySelector('[data-testid="tool-launch-btn"]')
        ?.addEventListener('click', () => {
          w.__pvLaunchClicks += 1;
        });
    });
    await btn.click({ force: true });

    expect(
      await page.evaluate(
        () =>
          (window as unknown as { __pvLaunchClicks: number }).__pvLaunchClicks,
      ),
    ).toBe(0);
    // No launch started: the label never flips to its in-flight form.
    await expect(btn).toHaveText('Open in PixInsight');
  });

  test('keyboard traversal skips the disabled CTA', async ({ page }) => {
    await openNarrowbandProject(page);

    // Reveal is the action-bar control immediately before the CTA.
    await page.getByTestId('action-reveal').focus();
    await page.keyboard.press('Tab');

    // Focus lands on the next enabled control (the lifecycle transition),
    // never on the disabled CTA in between.
    await expect(page.getByTestId('transition-btn-completed')).toBeFocused();
  });

  test('the explanatory hint has real layout and is not covered by another element', async ({
    page,
  }) => {
    await openNarrowbandProject(page);
    const footer = page.getByTestId('tool-launch-footer');
    await expect(footer).toContainText('Tool path not configured');

    // Non-zero box: rules out a collapsed/zero-height container that
    // `toBeVisible()` would still report as visible in some layouts.
    const box = assertDefined(
      await footer.boundingBox(),
      'tool-launch hint has no layout box',
    );
    expect(box.height).toBeGreaterThan(0);
    expect(box.width).toBeGreaterThan(0);

    // Hit-test the rendered GLYPHS, not the container: `toBeVisible()` passes
    // for text painted underneath a sticky header or any overlay. The probe
    // targets the text node's own rect (via Range) and requires the top-most
    // element there to be exactly the element that owns the text — so an
    // overlay covering the copy fails even when it is a descendant of the
    // hint container and therefore invisible to a `contains()` check.
    const glyphsOnTop = await page.evaluate(() => {
      const f = document.querySelector('[data-testid="tool-launch-footer"]');
      if (!f) return 'no footer';
      const walker = document.createTreeWalker(f, NodeFilter.SHOW_TEXT);
      for (
        let node = walker.nextNode();
        node !== null;
        node = walker.nextNode()
      ) {
        if (!node.textContent?.includes('Tool path not configured')) continue;
        const range = document.createRange();
        range.selectNodeContents(node);
        const r = range.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return 'text has no rect';
        const hit = document.elementFromPoint(r.x + 2, r.y + r.height / 2);
        return hit === node.parentElement
          ? 'text on top'
          : `covered by ${(hit as Element | null)?.tagName ?? 'nothing'}`;
      }
      return 'text node not found';
    });
    expect(glyphsOnTop).toBe('text on top');

    // The hint and the control it explains are in the same detail pane and
    // both within the viewport at this size, so the user sees the disabled
    // CTA and its reason together.
    const btnBox = assertDefined(
      await page.getByTestId('tool-launch-btn').boundingBox(),
      'tool-launch CTA has no layout box',
    );
    const viewport = assertDefined(page.viewportSize(), 'no viewport size');
    expect(btnBox.y).toBeGreaterThanOrEqual(0);
    expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
    const detail = page.getByTestId('detail');
    await expect(detail.getByTestId('tool-launch-btn')).toHaveCount(1);
    await expect(detail.getByTestId('tool-launch-footer')).toHaveCount(1);

    // The "Configure" link is genuinely clickable and routes to the tools
    // pane. A plain (non-forced) click runs Playwright's hit-target check, so
    // anything intercepting pointer events over the link fails here. Last in
    // the test: it navigates away from the project detail.
    await footer.getByRole('link', { name: 'Configure' }).click();
    await expect(page).toHaveURL(/#\/settings\/tools$/);
  });
});
