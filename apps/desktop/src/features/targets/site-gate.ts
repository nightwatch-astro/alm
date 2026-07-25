// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * site-gate.ts — observing-site existence gate for the planner (spec 047, D7).
 *
 * The planner renders no astronomy (Moon summary, lunar distance, filter
 * guidance, opposition) until a default `ObserverSite` exists — prompt for the
 * site first, with no location-independent fallback rendering.
 *
 * Spec 047 only *consumes* a site-existence signal; the ObserverSite model,
 * wizard step, and site CRUD are Track B (spec 044 / 048) scope. Track B has
 * now landed the `observing`-scope settings store
 * (`observing-sites/site-store`), so the gate reads the real active site: it
 * opens exactly when an observing site exists (persisted by the first-run Site
 * step or Settings → Observing Sites, both of which set the first site active),
 * and the planner then computes altitude/rise-set/imaging-time (044) and Moon
 * phase/lunar separation/filter guidance/opposition (047) against it.
 */

import { activeSite, useActiveSite } from '@/shared/observing-sites/site-store';

/** Test-only override; `null` = use the real binding. */
let testOverride: boolean | null = null;

/**
 * Whether a default observing site exists — true when the observing-site store
 * has an active site. Non-reactive read for comparators; the hook below
 * subscribes for live updates.
 */
export function readSiteExists(): boolean {
  return activeSite() !== null;
}

/** Non-hook read (comparators, tests). Honours the test override. */
export function siteExists(): boolean {
  return testOverride ?? readSiteExists();
}

/**
 * Test seam: force the gate to a fixed value (or `null` to restore the real
 * binding). Lets US1/US2 component tests exercise both the gated-off prompt
 * and the real astronomy rendering before Track B's site key exists.
 */
export function __setSiteExistsForTest(value: boolean | null): void {
  testOverride = value;
}

/**
 * React hook: whether a default observing site exists. Subscribes to the
 * observing-site store so the planner opens the moment a site is created
 * (first-run Finish or Settings → Observing Sites) without a reload. The test
 * override still wins when set.
 */
export function useObserverSiteExists(): boolean {
  const site = useActiveSite();
  return testOverride ?? site !== null;
}
