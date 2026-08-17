// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * PlannerComputedFor tests (spec 044 iteration 2026-07-15, FR-033/T043):
 *  - with an active site: one single-line label discloses site name, latitude
 *    with hemisphere, twilight definition, and the ≥threshold value, plus a
 *    "change" link to the existing Settings → Target Planner surface;
 *  - without an active site: renders nothing (the toolbar's site prompt owns
 *    that state).
 */

import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Same router stub as TargetsTable.test.tsx: Link needs a router context.
vi.mock('@tanstack/react-router', () => ({
  Link: ({
    children,
    to,
    ...rest
  }: {
    children?: import('react').ReactNode;
    to: string;
  }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}));

import { PlannerComputedFor } from './PlannerComputedFor';
import { __setObservingStateForTest } from '@/shared/observing-sites/site-store';
import type { ObserverSite } from '@/shared/observing-sites/observer-site';

const SITE: ObserverSite = {
  id: 'site-test',
  name: 'Backyard North',
  latitudeDeg: 52.37,
  longitudeDeg: 4.9,
  elevationM: 0,
  timezone: 'Europe/Amsterdam',
  twilight: 'astronomical',
  minHorizonAltDeg: 0,
};

describe('PlannerComputedFor (FR-033)', () => {
  beforeEach(() => {
    __setObservingStateForTest({});
  });

  it('renders nothing when no site is active', () => {
    render(<PlannerComputedFor usableAltDeg={30} />);
    expect(
      screen.queryByTestId('planner-computed-for'),
    ).not.toBeInTheDocument();
  });

  it('discloses site, latitude, twilight, and threshold on one line', () => {
    __setObservingStateForTest({
      sites: [SITE],
      activeSiteId: SITE.id,
      defaultSiteId: SITE.id,
    });
    render(<PlannerComputedFor usableAltDeg={42} />);
    const label = screen.getByTestId('planner-computed-for');
    expect(label).toHaveTextContent('Computed for:');
    expect(label).toHaveTextContent('Backyard North');
    expect(label).toHaveTextContent('52.4°N');
    expect(label).toHaveTextContent(/Astronomical/);
    expect(label).toHaveTextContent('≥42°');
  });

  it('southern-hemisphere latitude renders with °S', () => {
    __setObservingStateForTest({
      sites: [{ ...SITE, latitudeDeg: -33.9 }],
      activeSiteId: SITE.id,
      defaultSiteId: SITE.id,
    });
    render(<PlannerComputedFor usableAltDeg={30} />);
    expect(screen.getByTestId('planner-computed-for')).toHaveTextContent(
      '33.9°S',
    );
  });

  it('"change" opens the existing Settings → Target Planner surface', () => {
    __setObservingStateForTest({
      sites: [SITE],
      activeSiteId: SITE.id,
      defaultSiteId: SITE.id,
    });
    render(<PlannerComputedFor usableAltDeg={30} />);
    const change = screen.getByRole('link', { name: 'change' });
    expect(change).toHaveAttribute('href', '/settings/$pane');
  });
});
