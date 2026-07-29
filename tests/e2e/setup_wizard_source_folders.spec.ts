// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { Locator, Page } from '@playwright/test';
import { assertDefined, expect, test } from './support/harness';

const LOCALES = [
  {
    locale: 'en-GB',
    required: 'Required',
    optional: 'Optional',
    lightFrames: 'Light frames',
    calibration: 'Calibration frames',
    requiredStatus: 'required ✓',
    optionalStatus: 'optional',
  },
  {
    locale: 'pt-BR',
    required: 'Obrigatório',
    optional: 'Opcional',
    lightFrames: 'Quadros de luz',
    calibration: 'Quadros de calibração',
    requiredStatus: 'obrigatório ✓',
    optionalStatus: 'opcional',
  },
] as const;

function seedWizardAtSources(page: Page, locale: string): void {
  page.addInitScript((selectedLocale) => {
    window.localStorage.removeItem('alm-preferences');
    window.localStorage.setItem('pv.locale', selectedLocale);
    window.localStorage.setItem(
      'alm-setup-wizard-state',
      JSON.stringify({
        version: 2,
        currentStep: 2,
        sources: [
          {
            kind: 'light_frames',
            path: '/astro/a-very-long-light-frames-folder-name',
            organizationState: 'organized',
          },
        ],
        catalogSettings: { selectedCatalogIds: [] },
        tools: {
          pixinsight: { enabled: false, path: null },
          siril: { enabled: false, path: null },
        },
      }),
    );
  }, locale);
}

async function expectSourceControlsContained(root: Locator): Promise<void> {
  const overflowing = await root
    .locator(
      '[data-testid="step-sources"], [data-testid^="source-group-"], [data-testid="step-sources-group-header"], [data-testid="step-sources-add-actions"], [data-testid^="manual-add-by-path-"], [data-testid="step-sources-row-main"]',
    )
    .evaluateAll((elements) =>
      elements
        .filter((element) => element.scrollWidth > element.clientWidth + 1)
        .map((element) => element.className),
    );
  expect(overflowing).toEqual([]);

  for (const group of await root
    .locator('[data-testid^="source-group-"]')
    .all()) {
    const groupBox = assertDefined(
      await group.boundingBox(),
      'source group bounding box',
    );
    for (const control of await group.locator('button, input, select').all()) {
      const controlBox = assertDefined(
        await control.boundingBox(),
        'source control bounding box',
      );
      expect(controlBox.x).toBeGreaterThanOrEqual(groupBox.x - 1);
      expect(controlBox.x + controlBox.width).toBeLessThanOrEqual(
        groupBox.x + groupBox.width + 1,
      );
    }
  }
}

test.describe('setup wizard · source-folder primitives at 320 CSS px', () => {
  test.use({ viewport: { width: 320, height: 900 } });

  for (const labels of LOCALES) {
    test(`${labels.locale} preserves semantic groups, keyboard order, and horizontal reflow`, async ({
      page,
    }) => {
      seedWizardAtSources(page, labels.locale);
      await page.goto('/#/setup');

      const required = page.getByRole('region', {
        name: labels.required,
        exact: true,
      });
      const optional = page.getByRole('region', {
        name: labels.optional,
        exact: true,
      });
      await expect(
        page.getByRole('heading', { level: 2, name: labels.required }),
      ).toBeVisible({ timeout: 10_000 });
      await expect(
        page.getByRole('heading', { level: 2, name: labels.optional }),
      ).toBeVisible();
      await expect(
        required.getByRole('heading', {
          level: 3,
          name: labels.lightFrames,
        }),
      ).toBeVisible();
      await expect(
        optional.getByRole('heading', {
          level: 3,
          name: labels.calibration,
        }),
      ).toBeVisible();

      const lightFrames = page.getByRole('region', {
        name: labels.lightFrames,
        exact: true,
      });
      const calibration = page.getByRole('region', {
        name: labels.calibration,
        exact: true,
      });
      await expect(
        lightFrames.getByTestId('requirement-status-light_frames'),
      ).toHaveClass(/pv-pill--ok/);
      await expect(
        lightFrames.getByTestId('requirement-status-light_frames'),
      ).toHaveText(labels.requiredStatus);
      await expect(
        calibration.getByTestId('requirement-status-calibration'),
      ).toHaveClass(/pv-pill--ghost/);
      await expect(
        calibration.getByTestId('requirement-status-calibration'),
      ).toHaveText(labels.optionalStatus);

      const organization = lightFrames.getByTestId('org-select-light_frames-0');
      const manualPath = lightFrames.getByTestId(
        'manual-path-input-light_frames',
      );
      // The org-state select is keyed on its per-row test id; its vanilla-extract
      // class name is hashed in production builds and is not assertable. The
      // manual-path input still uses the un-migrated global `.pv-input`.
      await expect(organization).toHaveValue('organized');
      await expect(manualPath).toHaveClass(/pv-input/);

      const info = lightFrames.locator('[data-testid="info-tip"]').first();
      const choose = lightFrames.locator('[data-testid="btn-primary"]');
      const addByPath = lightFrames.getByTestId(
        'manual-add-path-btn-light_frames',
      );
      await info.focus();
      await page.keyboard.press('Tab');
      await expect(choose).toBeFocused();
      await page.keyboard.press('Tab');
      await expect(manualPath).toBeFocused();
      await manualPath.fill('/astro/new-lights');
      await expect(addByPath).toBeEnabled();
      await page.keyboard.press('Tab');
      await expect(addByPath).toBeFocused();

      await expectSourceControlsContained(
        page.locator('[data-testid="step-sources"]'),
      );
    });
  }
});

test.describe('setup wizard · source-folder primitives at 200% zoom', () => {
  test.use({ viewport: { width: 640, height: 900 } });

  test('pt-BR controls reflow without horizontal clipping', async ({
    page,
  }) => {
    seedWizardAtSources(page, 'pt-BR');
    await page.goto('/#/setup');
    await expect(
      page.getByRole('heading', { level: 2, name: 'Obrigatório' }),
    ).toBeVisible({ timeout: 10_000 });

    await page.evaluate(() => {
      document.documentElement.style.zoom = '200%';
    });
    await page.evaluate(
      () =>
        new Promise<void>((resolve) => {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
        }),
    );

    await expectSourceControlsContained(
      page.locator('[data-testid="step-sources"]'),
    );
  });
});
