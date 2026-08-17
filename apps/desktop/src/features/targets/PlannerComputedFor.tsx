// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * PlannerComputedFor — the always-visible computation-context label (spec 044
 * iteration 2026-07-15, FR-033/T043): one compact single line disclosing the
 * inputs every planner number is computed from — the active site (FR-012),
 * its twilight definition (FR-015), and the usable-altitude threshold
 * (FR-018) — "Computed for: <site> <lat>°N · <twilight> · ≥<N>° · change".
 * "change" opens the existing Settings → Target Planner surface (the
 * site-switch/settings surface; no new switching UI).
 *
 * Renders nothing when no site is active: the toolbar already shows the
 * set-up-your-site prompt in that state, so there is no context to disclose.
 */

import { Link } from '@tanstack/react-router';
import { m } from '@/lib/i18n';
import { useActiveSite } from '@/shared/observing-sites/site-store';

export function PlannerComputedFor({ usableAltDeg }: { usableAltDeg: number }) {
  const site = useActiveSite();
  if (!site) return null;

  const lat = `${Math.abs(site.latitudeDeg).toFixed(1)}°${site.latitudeDeg < 0 ? 'S' : 'N'}`;
  const twilight =
    site.twilight === 'nautical'
      ? m.settings_observing_sites_twilight_nautical()
      : m.settings_observing_sites_twilight_astronomical();

  return (
    <div className="pv-planner-computed-for" data-testid="planner-computed-for">
      <span className="pv-planner-computed-for__text">
        {m.targets_computed_for({
          site: site.name,
          lat,
          twilight,
          threshold: usableAltDeg,
        })}
      </span>
      <Link
        to="/settings/$pane"
        params={{ pane: 'planner' }}
        className="pv-planner-computed-for__change"
      >
        {m.targets_computed_for_change()}
      </Link>
    </div>
  );
}
