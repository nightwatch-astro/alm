// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * CommandPalette devMode gate tests (spec 021 T009).
 *
 * These import the real `PAGES`, `DEV_PAGES`, and `visiblePagesFor` from
 * `CommandPalette.tsx`. An earlier version of this file declared local copies of
 * all three and tested those, which is why it kept passing while production
 * gated `DEV_PAGES` on the runtime `devMode` setting alone and shipped
 * `/dev/contracts` into release bundles. The copies had also drifted: they
 * listed `/review`, `/plans`, and `/audit`, none of which are palette pages.
 *
 * The test run bakes `VITE_DEV_TOOLS="false"` into `vitest.config.ts`, so
 * `DEV_TOOLS_ENABLED` is false here and `DEV_PAGES` is empty — the shape a
 * release build has. The developer-build shape is covered by passing explicit
 * dev pages to `visiblePagesFor`.
 *
 * The rendered Dialog needs ResizeObserver and is deferred to Playwright.
 */

import { describe, it, expect } from 'vitest';
import { PAGES, DEV_PAGES, visiblePagesFor } from '@/app/CommandPalette';
import { DEV_TOOLS_ENABLED } from './devToolsEnabled';

const DEV_CONTRACTS_ROUTE = '/dev/contracts';

/** Stand-in for the dev entry a `VITE_DEV_TOOLS="true"` build would compile in. */
const DEV_PAGES_ENABLED = [
  { label: () => 'Developer / Contracts', route: DEV_CONTRACTS_ROUTE },
];

describe('CommandPalette dev gate — release-shaped build (T009)', () => {
  it('DEV_TOOLS_ENABLED is false under the test config', () => {
    expect(DEV_TOOLS_ENABLED).toBe(false);
  });

  it('DEV_PAGES is empty, so no dev route exists to leak into the bundle', () => {
    expect(DEV_PAGES).toEqual([]);
  });

  it('devMode = true cannot surface a dev entry when the build gate is off', () => {
    const pages = visiblePagesFor(true);
    expect(pages.find((p) => p.route === DEV_CONTRACTS_ROUTE)).toBeUndefined();
    expect(pages.map((p) => p.route)).toEqual(PAGES.map((p) => p.route));
  });
});

describe('CommandPalette devMode gate — developer build (T009)', () => {
  it('dev entry is absent when devMode = false', () => {
    const pages = visiblePagesFor(false, DEV_PAGES_ENABLED);
    expect(pages.find((p) => p.route === DEV_CONTRACTS_ROUTE)).toBeUndefined();
  });

  it('dev entry is present and routed when devMode = true', () => {
    const pages = visiblePagesFor(true, DEV_PAGES_ENABLED);
    const devEntry = pages.find((p) => p.route === DEV_CONTRACTS_ROUTE);
    expect(devEntry).toBeDefined();
    expect(devEntry?.label()).toBe('Developer / Contracts');
  });

  it('standard pages are unchanged regardless of devMode', () => {
    const off = visiblePagesFor(false, DEV_PAGES_ENABLED).map((p) => p.route);
    const on = visiblePagesFor(true, DEV_PAGES_ENABLED).map((p) => p.route);
    for (const p of PAGES) {
      expect(off).toContain(p.route);
      expect(on).toContain(p.route);
    }
  });

  it('no standard page uses a dev route', () => {
    const devRoutes = new Set(DEV_PAGES_ENABLED.map((p) => p.route));
    for (const p of PAGES) {
      expect(devRoutes.has(p.route)).toBe(false);
    }
  });
});
