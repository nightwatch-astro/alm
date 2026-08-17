// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The six orientation-walk stops (spec 056, T012; FR-002/FR-006).
 *
 * Five whole-page workflow spotlights in the sidebar's workflow order, then a
 * final stop anchored on the sidebar Getting started section — the L1→L2 bridge
 * that introduces the checklists (FR-002). Data only; `OrientationWalk` maps
 * these onto the library-agnostic joyride adapter.
 *
 * Copy is read through Paraglide thunks so each stop re-reads the active locale
 * on render (the sidebar-label convention, spec 046 #8).
 *
 * INTER-NODE CONTRACT: the final stop's target
 * `[data-guide-anchor="onboarding.getting-started"]` MUST be present on the
 * Getting started section root built by T018 — a mismatch is silent
 * (TARGET_NOT_FOUND at runtime, no compile error).
 */

import { m } from '@/lib/i18n';

export interface OrientationStop {
  /** CSS selector for the whole-page / section spotlight target. */
  target: string;
  /** Route to navigate to before showing the stop; absent = stay put. */
  route?: string;
  /** Localized stop title (announced + heading). */
  title: () => string;
  /** Localized one/two-sentence "this is where X happens" body. */
  body: () => string;
  /**
   * Tooltip placement. REQUIRED and never undefined: react-joyride's
   * `getFallbackPlacements` calls `placement.startsWith(...)` and the adapter
   * forwards the key verbatim, so an undefined placement throws in a render
   * loop. Whole-page stops use `center`: the target (the frame `<main>`) fills
   * the viewport, so an anchored placement (`auto`/`bottom`) has no room and
   * the floater fails to render — `center` positions the copy on the viewport
   * and keeps the full-page dim overlay (its own whole-page spotlight, FR-002;
   * a per-element cutout of the whole page would be pointless). The section
   * stop anchors to its `right` with a real cutout on the small target.
   */
  placement: 'center' | 'right';
}

/**
 * Selector for the app's whole-page content region (route-independent).
 *
 * Targets the frame `<main>` by data-testid, not its class: the app-shell
 * migration to vanilla-extract hashes the `frameMain` class, so a literal
 * `.pv-frame__main` selector no longer resolves and react-joyride would fail to
 * spotlight (tooltip never renders).
 */
const PAGE_SPOTLIGHT = '[data-testid="frame-main"]';

export const ORIENTATION_STOPS: OrientationStop[] = [
  {
    route: '/inbox',
    target: PAGE_SPOTLIGHT,
    title: () => m.onboarding_walk_stop_inbox_title(),
    body: () => m.onboarding_walk_stop_inbox_body(),
    placement: 'center',
  },
  {
    route: '/sessions',
    target: PAGE_SPOTLIGHT,
    title: () => m.onboarding_walk_stop_sessions_title(),
    body: () => m.onboarding_walk_stop_sessions_body(),
    placement: 'center',
  },
  {
    route: '/calibration',
    target: PAGE_SPOTLIGHT,
    title: () => m.onboarding_walk_stop_calibration_title(),
    body: () => m.onboarding_walk_stop_calibration_body(),
    placement: 'center',
  },
  {
    route: '/targets',
    target: PAGE_SPOTLIGHT,
    title: () => m.onboarding_walk_stop_targets_title(),
    body: () => m.onboarding_walk_stop_targets_body(),
    placement: 'center',
  },
  {
    route: '/projects',
    target: PAGE_SPOTLIGHT,
    title: () => m.onboarding_walk_stop_projects_title(),
    body: () => m.onboarding_walk_stop_projects_body(),
    placement: 'center',
  },
  {
    // No route: the final stop stays on /projects and spotlights the sidebar
    // Getting started section (FR-002 bridge). See INTER-NODE CONTRACT above.
    target: '[data-guide-anchor="onboarding.getting-started"]',
    title: () => m.onboarding_walk_stop_getting_started_title(),
    body: () => m.onboarding_walk_stop_getting_started_body(),
    placement: 'right',
  },
];
