// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useTargetTonight — derives tonight planner data (altitude curve, imaging
 * time, moon separation, best-date) for a single target.
 *
 * Extracted from TargetDetail.tsx to isolate the computation from rendering.
 */

import type { TargetListItem } from '@/bindings/index';
import {
  altitudeFor,
  moonExcludedSpanHours,
  rowAltitudeFor,
  USABLE_ALT_DEG,
} from './planner-altitude';
import { BANDS } from '@/shared/planner/moon-avoidance';
import type { SensorConfig } from './planner-derive';
import { useActiveSite } from '@/shared/observing-sites/site-store';
import { usePlannerDateMs } from './planner-date-store';
import { deriveRowMoonPlanning } from './astro/row-planning';
import type { ObservingNight } from './astro/moon-state';
import { useGuidanceParams } from '@/shared/planner/guidance-settings';
import { bestMoonDate } from './astro/best-moon-date';
import type { AltPoint } from './AltitudeGraph';

export interface UseTargetTonightOptions {
  targetId: string;
  raDeg: number | null;
  decDeg: number | null;
  /** The selected list row — supplies precomputed tonight stats. */
  item?: TargetListItem | null;
  usableAltDeg?: number;
  night?: ObservingNight | null;
  sensorConfig?: SensorConfig | null;
}

export interface UseTargetTonightResult {
  rowAlt: ReturnType<typeof altitudeFor>;
  tonightPoints: AltPoint[];
  tonightAvailable: boolean;
  displayBand: string;
  moonSpans: ReturnType<typeof moonExcludedSpanHours>;
  moon: ReturnType<typeof deriveRowMoonPlanning>;
  bestMoon: ReturnType<typeof bestMoonDate>;
  guidanceParams: ReturnType<typeof useGuidanceParams>;
  site: ReturnType<typeof useActiveSite>;
  night: ObservingNight | null;
  dateMs: number;
}

export function useTargetTonight({
  targetId,
  raDeg,
  decDeg,
  item = null,
  usableAltDeg = USABLE_ALT_DEG,
  night = null,
  sensorConfig = null,
}: UseTargetTonightOptions): UseTargetTonightResult {
  const guidanceParams = useGuidanceParams();
  const site = useActiveSite();
  const dateMs = usePlannerDateMs();

  // Tonight planner data — shared with the list row (same rowAltitudeFor source
  // so the graph peak and the "Max alt" stat agree). Falls back to a direct
  // real computation from the detail's own RA/Dec when the list item isn't
  // available (e.g. direct navigation to a target's detail page).
  // Null coords propagate to altitudeFor which sets needsCoordinates=true.
  const rowAlt = item
    ? rowAltitudeFor(
        item,
        usableAltDeg,
        site,
        dateMs,
        guidanceParams,
        true,
        sensorConfig,
      )
    : altitudeFor(
        { id: targetId, raDeg, decDeg },
        usableAltDeg,
        site,
        dateMs,
        guidanceParams,
        true,
        sensorConfig,
      );
  const tonightPoints: AltPoint[] = rowAlt.points;

  // FR-007 overlay: Moon-excluded spans for the DISPLAYED band — default
  // unchanged: the band with the most moon-free time.
  const displayBand = BANDS.reduce((best, b) =>
    rowAlt.moonFreeMinutesByBand[b] > rowAlt.moonFreeMinutesByBand[best]
      ? b
      : best,
  );
  const moonSpans =
    !rowAlt.needsCoordinates && !rowAlt.needsSite
      ? moonExcludedSpanHours(
          { id: targetId, raDeg, decDeg },
          displayBand,
          site,
          dateMs,
          guidanceParams,
        )
      : [];

  const moon = deriveRowMoonPlanning({ raDeg, decDeg }, night, guidanceParams);

  const tonightAvailable = !rowAlt.needsCoordinates && !rowAlt.needsSite;

  const bestMoonResult = tonightAvailable
    ? bestMoonDate(
        raDeg,
        decDeg,
        night?.midnight ?? new Date(dateMs),
        guidanceParams,
      )
    : null;

  return {
    rowAlt,
    tonightPoints,
    tonightAvailable,
    displayBand,
    moonSpans,
    moon,
    bestMoon: bestMoonResult,
    guidanceParams,
    site,
    night,
    dateMs,
  };
}
