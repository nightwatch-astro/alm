// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
import { render, screen, within } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { FilterBadges, recommendationLabel } from './FilterBadges';
import {
  bandViability,
  DEFAULT_MOON_AVOIDANCE,
  BANDS,
} from '@/shared/planner/moon-avoidance';

describe('FilterBadges — per-band viability pills (spec 047 T016)', () => {
  it('renders all seven bands with a viable/not-viable aria-label each', () => {
    const viability = bandViability(70, 0, DEFAULT_MOON_AVOIDANCE); // full Moon, 70° separation
    render(
      <FilterBadges viability={viability} recommendation="narrowband-only" />,
    );
    for (const band of BANDS) {
      const state = viability[band] ? 'viable' : 'not viable';
      expect(
        screen.getByLabelText(`${band}: ${state} tonight`),
      ).toBeInTheDocument();
    }
  });

  it('renders a single unknown pill instead of fabricating per-band viability', () => {
    render(<FilterBadges viability={null} recommendation="unknown" />);
    expect(
      screen.getByText(recommendationLabel('unknown')),
    ).toBeInTheDocument();
    // No band letters rendered as separate pills.
    for (const band of BANDS) {
      expect(
        screen.queryByLabelText(new RegExp(`^${band}:`)),
      ).not.toBeInTheDocument();
    }
  });

  it('every band pill carries the tier class (broadband vs narrowband)', () => {
    const viability = bandViability(180, 0, DEFAULT_MOON_AVOIDANCE); // trivially all-viable
    const { container } = render(
      <FilterBadges viability={viability} recommendation="broadband-ok" />,
    );
    const broadband = within(container).getByLabelText('L: viable tonight');
    expect(broadband.className).toContain('pv-filter-badge--broadband');
    const narrowband = within(container).getByLabelText('Ha: viable tonight');
    expect(narrowband.className).toContain('pv-filter-badge--narrowband');
  });

  it('recommendationLabel covers all four categories', () => {
    expect(recommendationLabel('broadband-ok')).toBeTruthy();
    expect(recommendationLabel('narrowband-only')).toBeTruthy();
    expect(recommendationLabel('avoid-tonight')).toBeTruthy();
    expect(recommendationLabel('unknown')).toBeTruthy();
  });
});
