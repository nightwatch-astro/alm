// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Regression: the star and designation columns stay pinned while the Targets
 * table scrolls horizontally.
 *
 * `position: sticky` with an `auto` inset does not stick. When the vanilla-extract
 * rewrite assigned `left` to `thead th` only, the two body cells kept
 * `position: sticky` and lost their offsets, so they scrolled out from under
 * their own still-pinned headers. Both halves render as "sticky" to any
 * computed-style or class-name check, which is why this asserts measured
 * geometry after a real scroll.
 *
 * Viewport is deliberately narrower than the table's 1000px `min-width`, since
 * that is the only condition under which the table scrolls horizontally at all.
 */
import {
  test,
  expect,
  seedSetupComplete,
  disableOnboarding,
  assertDefined,
} from './support/harness';

const SCROLL_BY = 300;

test.beforeEach(async ({ page }) => {
  // 820px < the table's 1000px min-width, so the scroll container overflows.
  await page.setViewportSize({ width: 820, height: 900 });
  await disableOnboarding(page);
  seedSetupComplete(page);
});

test.describe('targets table · pinned columns', () => {
  test('the star and designation cells hold their position when scrolled horizontally', async ({
    page,
  }) => {
    await page.goto('/#/targets');

    const firstRow = page
      .locator('tbody tr')
      .filter({ has: page.locator('td') })
      .first();
    await expect(firstRow).toBeVisible({ timeout: 8_000 });

    const starCell = firstRow.locator('td').nth(0);
    const desigCell = firstRow.locator('td').nth(1);
    const typeCell = firstRow.locator('td').nth(2);
    const starHeader = page.locator('thead th').nth(0);
    const desigHeader = page.locator('thead th').nth(1);

    const before = {
      star: assertDefined(await starCell.boundingBox(), 'star cell has no box'),
      desig: assertDefined(
        await desigCell.boundingBox(),
        'designation cell has no box',
      ),
      type: assertDefined(await typeCell.boundingBox(), 'type cell has no box'),
      starHeader: assertDefined(
        await starHeader.boundingBox(),
        'star header has no box',
      ),
      desigHeader: assertDefined(
        await desigHeader.boundingBox(),
        'designation header has no box',
      ),
    };

    // Scroll the container that actually overflows, and confirm it moved —
    // otherwise the assertions below would pass on a table that never scrolled.
    const scrolled = await page.evaluate((by) => {
      const containers = Array.from(document.querySelectorAll('div')).filter(
        (el) =>
          el.scrollWidth > el.clientWidth + 10 && el.querySelector('table'),
      );
      const el = containers[0];
      if (!el) return -1;
      el.scrollLeft = by;
      return el.scrollLeft;
    }, SCROLL_BY);
    expect(scrolled).toBeGreaterThan(0);

    const after = {
      star: assertDefined(await starCell.boundingBox(), 'star cell has no box'),
      desig: assertDefined(
        await desigCell.boundingBox(),
        'designation cell has no box',
      ),
      type: assertDefined(await typeCell.boundingBox(), 'type cell has no box'),
      starHeader: assertDefined(
        await starHeader.boundingBox(),
        'star header has no box',
      ),
      desigHeader: assertDefined(
        await desigHeader.boundingBox(),
        'designation header has no box',
      ),
    };

    // The unpinned third column is the control: it must have moved left by the
    // scroll amount, proving the scroll took effect on the table itself.
    expect(before.type.x - after.type.x).toBeGreaterThan(scrolled - 2);

    // The pinned pair holds. 1px of slack absorbs sub-pixel layout rounding.
    expect(Math.abs(after.star.x - before.star.x)).toBeLessThanOrEqual(1);
    expect(Math.abs(after.desig.x - before.desig.x)).toBeLessThanOrEqual(1);

    // And each body cell stays aligned with its own header, which is the
    // specific breakage: headers pinned, cells adrift.
    expect(Math.abs(after.star.x - after.starHeader.x)).toBeLessThanOrEqual(1);
    expect(Math.abs(after.desig.x - after.desigHeader.x)).toBeLessThanOrEqual(
      1,
    );
  });
});
