// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { expect, landOnMockRoute, test } from './support/harness';

const THEMES = [
  'espresso-dark',
  'observatory-cool-light',
  'observatory-cool',
  'observatory-dark',
  'warm-clay',
  'warm-slate',
] as const;

test('shared form controls use the dedicated boundary token in every theme', async ({
  page,
}) => {
  // Land on a page that renders a real shared select so the probe can reuse its
  // live class: the select primitive is a vanilla-extract style whose class is
  // hashed at build time and cannot be hard-coded here (Settings → General
  // renders one). Only a non-empty class matters -- the token itself is not a
  // contract.
  await landOnMockRoute(page, '/#/settings/general');
  const selectClass = await page
    .locator('[data-testid="settings-row"] select')
    .first()
    .getAttribute('class');
  expect(
    selectClass?.trim(),
    'shared select carries a style class',
  ).toBeTruthy();

  const samples = await page.evaluate(
    ([themes, selectClass]) => {
      const host = document.createElement('div');
      host.innerHTML = `
      <input data-control="input" class="pv-input" />
      <select data-control="select" class="${selectClass}"><option>Test</option></select>
      <label class="pv-toggle pv-toggle--disabled" data-testid="toggle">
        <input type="checkbox" disabled />
        <span data-control="toggle" class="pv-toggle__track"></span>
        <span class="pv-toggle__thumb"></span>
      </label>
      <span data-token="control"></span>
      <span data-token="border"></span>
      <span data-token="subtle"></span>
    `;
      for (const control of host.querySelectorAll<HTMLElement>(
        '[data-control]',
      )) {
        control.style.transition = 'none';
      }
      const controlProbe = host.querySelector<HTMLElement>(
        '[data-token="control"]',
      );
      const borderProbe = host.querySelector<HTMLElement>(
        '[data-token="border"]',
      );
      const subtleProbe = host.querySelector<HTMLElement>(
        '[data-token="subtle"]',
      );
      if (!controlProbe || !borderProbe || !subtleProbe) {
        throw new Error('missing token probe');
      }
      controlProbe.style.color = 'var(--pv-control-border)';
      borderProbe.style.color = 'var(--pv-border)';
      subtleProbe.style.color = 'var(--pv-border-subtle)';
      document.body.append(host);

      const previousTheme = document.documentElement.dataset.theme;
      try {
        return themes.map((theme) => {
          document.documentElement.dataset.theme = theme;
          const input = host.querySelector<HTMLElement>(
            '[data-control="input"]',
          );
          const select = host.querySelector<HTMLElement>(
            '[data-control="select"]',
          );
          const toggle = host.querySelector<HTMLElement>(
            '[data-control="toggle"]',
          );
          const disabledToggle = host.querySelector<HTMLElement>(
            '[data-testid="toggle"]',
          );
          if (!input || !select || !toggle || !disabledToggle) {
            throw new Error('missing control probe');
          }
          return {
            theme,
            control: getComputedStyle(controlProbe).color,
            input: getComputedStyle(input).borderTopColor,
            select: getComputedStyle(select).borderTopColor,
            toggle: getComputedStyle(toggle).backgroundColor,
            border: getComputedStyle(borderProbe).color,
            subtle: getComputedStyle(subtleProbe).color,
            disabledOpacity: getComputedStyle(disabledToggle).opacity,
          };
        });
      } finally {
        if (previousTheme)
          document.documentElement.dataset.theme = previousTheme;
        else delete document.documentElement.dataset.theme;
        host.remove();
      }
    },
    [THEMES, selectClass] as const,
  );

  for (const sample of samples) {
    expect(sample.input, `${sample.theme} input boundary`).toBe(sample.control);
    expect(sample.select, `${sample.theme} select boundary`).toBe(
      sample.control,
    );
    expect(sample.toggle, `${sample.theme} toggle boundary`).toBe(
      sample.control,
    );
    expect(sample.border, `${sample.theme} legacy border`).not.toBe(
      sample.control,
    );
    expect(sample.subtle, `${sample.theme} subtle divider`).not.toBe(
      sample.control,
    );
    expect(sample.disabledOpacity, `${sample.theme} disabled toggle`).toBe(
      '0.5',
    );
  }
});
