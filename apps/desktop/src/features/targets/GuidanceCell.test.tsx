// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { GuidanceCell } from './GuidanceCell';
import {
  deriveRowMoonPlanning,
  UNKNOWN_ROW_PLANNING,
} from './astro/row-planning';
import { DEFAULT_MOON_AVOIDANCE } from '@/shared/planner/moon-avoidance';
import type { ObservingNight } from './astro/moon-state';

function night(): ObservingNight {
  return {
    nightKey: '2026-07-05',
    midnight: new Date('2026-07-05T00:00:00Z'),
    phaseName: 'full',
    waxing: false,
    illuminationFrac: 1,
    moonAgeFromFullDays: 0,
    moonVec: { x: 1, y: 0, z: 0 },
  };
}

describe('GuidanceCell — explanation popover (spec 047 T018, FR-012)', () => {
  it('opens the explanation on click/focus-activate, showing Moon state + separation + per-band thresholds', async () => {
    const n = night();
    const moon = deriveRowMoonPlanning(
      { raDeg: 70, decDeg: 0 },
      n,
      DEFAULT_MOON_AVOIDANCE,
    );
    render(
      <GuidanceCell
        night={n}
        moon={moon}
        params={DEFAULT_MOON_AVOIDANCE}
        targetLabel="M 42"
      />,
    );

    const trigger = screen.getByRole('button');
    fireEvent.click(trigger);

    const popup = await screen.findByTestId('guidance-explain-popup');
    expect(popup.textContent).toMatch(/Full Moon/);
    expect(popup.textContent).toMatch(/100%/);
    expect(popup.textContent).toMatch(/70°/);
    expect(popup.textContent).toMatch(/Ha/);
  });

  it('shows the explicit unknown explanation for unknown coordinates', async () => {
    render(
      <GuidanceCell
        night={night()}
        moon={UNKNOWN_ROW_PLANNING}
        params={DEFAULT_MOON_AVOIDANCE}
        targetLabel="Unresolved"
      />,
    );
    const trigger = screen.getByRole('button');
    fireEvent.click(trigger);
    const popup = await screen.findByTestId('guidance-explain-popup');
    expect(popup.textContent).toMatch(/No catalogued coordinates/);
  });

  it('the trigger button stops row-select click propagation', async () => {
    const n = night();
    const moon = deriveRowMoonPlanning(
      { raDeg: 70, decDeg: 0 },
      n,
      DEFAULT_MOON_AVOIDANCE,
    );
    let rowClicked = false;
    render(
      // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- test harness only
      <div
        onClick={() => {
          rowClicked = true;
        }}
      >
        <GuidanceCell
          night={n}
          moon={moon}
          params={DEFAULT_MOON_AVOIDANCE}
          targetLabel="M 42"
        />
      </div>,
    );
    fireEvent.click(screen.getByRole('button'));
    expect(rowClicked).toBe(false);
  });
});
