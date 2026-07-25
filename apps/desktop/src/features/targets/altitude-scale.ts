// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * altitude-scale.ts — shared altitude/night-hour scale helpers (spec 044
 * Track B, T035). ONE domain definition + `@visx/scale` factory used by both
 * `AltitudeSparkline.tsx` (per-row inline sparkline) and `TargetDetail.tsx`'s
 * detail-pane altitude graph, so the two charts never drift out of sync and
 * neither hand-rolls its own linear-interpolation math.
 */

import { scaleLinear } from '@visx/scale';

/** Altitude domain both charts plot (degrees): a touch below the horizon to zenith-ish. */
export const ALT_DOMAIN: [number, number] = [-10, 90];

/**
 * Default night-hour domain: 0 = night start … 12 = night end. Used as the
 * fallback upper bound when a caller doesn't know the real sunset→sunrise
 * span; `hourScale`'s `maxHour` should be given the actual span (`nightSpan`)
 * where available so long nights (e.g. ~16.5 h at 52°N in December) don't
 * collapse onto the 12 h mark (#759).
 */
export const HOUR_DOMAIN: [number, number] = [0, 12];

/**
 * Altitude → pixel-Y scale. `rangeBottomPx`/`rangeTopPx` are the pixel
 * coordinates for the domain's min/max (SVG y grows downward, so the low
 * altitude maps to the larger pixel value).
 */
export function altitudeScale(rangeBottomPx: number, rangeTopPx: number) {
  // clamp: true — mirrors the prior hand-rolled Math.max/min clamping so an
  // out-of-domain altitude (rare, near ±90°) still plots at the chart edge
  // instead of extrapolating off-canvas.
  return scaleLinear<number>({
    domain: ALT_DOMAIN,
    range: [rangeBottomPx, rangeTopPx],
    clamp: true,
  });
}

/**
 * Night-hour → pixel-X scale. `maxHour` defaults to `HOUR_DOMAIN[1]` (12) for
 * callers that don't know the real night span; pass the actual sunset→sunrise
 * span (`nightSpan()`) to avoid flattening the tail of a long night onto the
 * rightmost pixel (#759).
 */
export function hourScale(
  rangeLeftPx: number,
  rangeRightPx: number,
  maxHour: number = HOUR_DOMAIN[1],
) {
  return scaleLinear<number>({
    domain: [0, maxHour],
    range: [rangeLeftPx, rangeRightPx],
    clamp: true,
  });
}

/**
 * The true sunset→sunrise span in hours for a sampled altitude curve, i.e.
 * the same axis `AltPoint.tHour` is measured on. Falls back to the default
 * 12 h domain for an empty curve (nothing to plot).
 */
export function nightSpan(points: readonly { tHour: number }[]): number {
  if (points.length === 0) return HOUR_DOMAIN[1];
  return points.reduce((max, p) => Math.max(max, p.tHour), 0);
}
