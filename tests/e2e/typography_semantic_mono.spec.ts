// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Spec 055 Phase 3 (T020–T022, T024) — semantic base layer + mono restoration.
 *
 * Pins the Phase-3 guarantees (SC-005/SC-006):
 *   1. No element computes `font-weight: 700` (the bundled Inter statics load
 *      400/500/600 only — a 700 would be a synthetic-bold browser fallback).
 *   2. Every italic-rendering element resolves against a loaded Inter italic
 *      face (no synthetic oblique).
 *   3. Mono-restoration surfaces (code/pre, a representative filesystem path,
 *      an RA/Dec coordinate value) compute the monospace stack, not Inter.
 *   4. Regression net for the removed `* { font-family !important }` blanket
 *      (reset.css): every rendered element's computed font-family either
 *      starts with Inter, or is an intentional mono surface
 *      (code/pre/kbd or `.pv-mono`).
 *
 * See apps/desktop/src/styles/reset.css (semantic base layer),
 * apps/desktop/src/styles/components/primitives.css (`.pv-mono`), and
 * specs/055-typography-rework/tasks.md Phase 3.
 */
import {
  test,
  expect,
  seedSetupComplete,
  disableOnboarding,
} from './support/harness';
import type { Page } from '@playwright/test';

/** Seed an active observing site (mirrors targets_planner.spec.ts) so the M 31
 * seed target's detail view renders its resolved RA/Dec property row. */
function seedObservingSite(page: Page): void {
  page.addInitScript(() => {
    window.localStorage.setItem(
      'pv-e2e-observing',
      JSON.stringify({
        observingSites: [
          {
            id: 'site-e2e-1',
            name: 'Backyard (Amsterdam)',
            latitudeDeg: 52.37,
            longitudeDeg: 4.9,
            elevationM: 5,
            timezone: 'Europe/Amsterdam',
            twilight: 'astronomical',
            minHorizonAltDeg: 0,
          },
        ],
        observingActiveSiteId: 'site-e2e-1',
        observingDefaultSiteId: 'site-e2e-1',
        usableAltitudeDeg: 30,
      }),
    );
  });
}

/** Locate a target row by its designation text (mirrors targets_planner.spec.ts). */
function targetRow(page: Page, designation: string) {
  return page.locator('[data-testid="targets-table-row"]', {
    hasText: designation,
  });
}

test.describe('Spec 055 · semantic base layer + mono restoration (Phase 3)', () => {
  test('no rendered element computes synthetic-bold font-weight 700', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/sessions');
    await page.waitForLoadState('networkidle');

    const violations = await page.evaluate(() => {
      const bad: string[] = [];
      document.querySelectorAll('body *').forEach((el) => {
        const cs = getComputedStyle(el);
        if (cs.fontWeight === '700') {
          bad.push(`${el.tagName}.${(el as HTMLElement).className}`);
        }
      });
      return bad;
    });

    expect(violations).toEqual([]);
  });

  test('italic-rendering elements resolve against a loaded Inter italic face', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/sessions');
    await page.waitForLoadState('networkidle');

    const result = await page.evaluate(async () => {
      await document.fonts.ready;
      const italics: { weight: string }[] = [];
      document.querySelectorAll('body *').forEach((el) => {
        const cs = getComputedStyle(el);
        if (cs.fontStyle === 'italic' && cs.fontFamily.includes('Inter')) {
          italics.push({ weight: cs.fontWeight });
        }
      });
      const loadedItalicWeights = new Set(
        Array.from(document.fonts)
          .filter((f) => f.family === 'Inter' && f.style === 'italic')
          .map((f) => f.weight),
      );
      const unmatched = italics.filter(
        (i) => !loadedItalicWeights.has(i.weight),
      );
      return { count: italics.length, unmatched };
    });

    // If the page has no italic Inter text at all this is vacuously fine —
    // the point is that whichever italic text exists never falls back to a
    // synthetic oblique of a face that was never loaded.
    expect(result.unmatched).toEqual([]);
  });

  test('code/pre content and a representative filesystem path render in the monospace stack', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/settings/sources');
    await page.waitForLoadState('networkidle');

    // A registered data-source root's path (real, mock-fixture-backed data —
    // apps/desktop/src/api/mocks.ts `mockRoots`) — not the fabricated Advanced
    // pane db-path #601/#602 removed. Same `<code class="pv-mono">` mechanism.
    const rootPath = page.locator('[data-testid="data-sources-root-path"]', {
      hasText: '/astro/raw',
    });
    await expect(rootPath).toBeVisible();
    await expect(rootPath).toHaveText('/astro/raw');

    const family = await rootPath.evaluate(
      (el) => getComputedStyle(el).fontFamily,
    );
    expect(family).not.toContain('Inter');
    expect(family.toLowerCase()).toContain('mono');

    // `code, pre, kbd` base-layer rule (reset.css): the root-path element itself
    // is a <code>, which is the same mechanism as every other code/pre surface.
    const tag = await rootPath.evaluate((el) => el.tagName.toLowerCase());
    expect(tag).toBe('code');
  });

  test('RA/Dec coordinate value renders in the monospace stack', async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible({ timeout: 8_000 });
    await m31.click();

    const radecValue = page.locator(
      '[data-testid="property-table-cell-value"][data-mono]',
    );
    await expect(radecValue.first()).toBeVisible({ timeout: 8_000 });

    const family = await radecValue
      .first()
      .evaluate((el) => getComputedStyle(el).fontFamily);
    expect(family).not.toContain('Inter');
    expect(family.toLowerCase()).toContain('mono');
  });

  test('regression net: every rendered element is Inter or an intentional mono surface', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/settings/advanced');
    await page.waitForLoadState('networkidle');

    const stragglers = await page.evaluate(() => {
      const bad: { tag: string; cls: string; family: string }[] = [];
      document.querySelectorAll('body *').forEach((el) => {
        // SVG <text> declares font-family explicitly per-surface (spec 055
        // T021) rather than through inheritance — out of scope for this sweep.
        if (el.closest('svg')) return;
        const cs = getComputedStyle(el);
        const family = cs.fontFamily;
        if (!family) return;
        const tag = el.tagName.toLowerCase();
        const isMonoIntent =
          (el as HTMLElement).classList?.contains('pv-mono') ||
          tag === 'code' ||
          tag === 'pre' ||
          tag === 'kbd' ||
          !!el.closest('code, pre, kbd, .pv-mono');
        const isInter = family.includes('Inter');
        const isMono = family.toLowerCase().includes('mono');
        if (isMonoIntent ? !isMono : !isInter) {
          bad.push({ tag, cls: (el as HTMLElement).className, family });
        }
      });
      return bad;
    });

    expect(stragglers).toEqual([]);
  });
});
