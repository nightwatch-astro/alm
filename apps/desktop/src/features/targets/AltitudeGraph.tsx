// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Tonight Altitude graph (spec 044 Track B, T035 — @visx), split out of
 * `TargetDetail` (refactor sweep #982): a self-contained, purely
 * props-driven SVG chart with no dependency on the detail pane's own state.
 *
 * Built on `@visx/scale`/`shape`/`group`/`threshold` (replacing the prior
 * hand-rolled polyline): the usable-altitude shading follows the CURVE
 * itself (Threshold clips the shaded area to where the curve is actually
 * above the threshold, not a static full-width band), and twilight-vs-dark
 * shading marks the evening/morning twilight either side of the real dark
 * window. The x/y scales are `altitude-scale.ts`, shared with the per-row
 * `AltitudeSparkline`.
 */

import { Group } from '@visx/group';
import { LinePath } from '@visx/shape';
import { Threshold } from '@visx/threshold';
import { altitudeScale, hourScale, nightSpan } from './altitude-scale';
import { m } from '@/lib/i18n';

/** One sampled point of a per-night altitude curve. */
export interface AltPoint {
  /** Hours into the night (0 = night start … night end). */
  tHour: number;
  /** Altitude in degrees (-90..+90), refraction-corrected. */
  altDeg: number;
}

export interface AltitudeGraphProps {
  /** Pre-sampled altitude curve (shared with the list's max-alt computation). */
  points: AltPoint[];
  /** Usable-altitude threshold (Settings → Target Planner); shades the curve above it. */
  usableAltDeg: number;
  /** The dark window's `[startHour, endHour]` on the same axis as `points`; `null` = no dark window (US4). */
  darkWindowHours: { startHour: number; endHour: number } | null;
  /**
   * Moon-excluded spans for the displayed band on the same axis as `points`
   * (iteration 2026-07-15, FR-007 overlay); empty when the Moon never
   * interferes or its geometry wasn't computed.
   */
  moonSpans?: Array<{ startHour: number; endHour: number }>;
}

const SVG_W = 400;
const SVG_H = 140;
const PAD_L = 32;
const PAD_R = 12;
const PAD_T = 10;
const PAD_B = 28;
const PLOT_W = SVG_W - PAD_L - PAD_R;
const PLOT_H = SVG_H - PAD_T - PAD_B;

export function AltitudeGraph({
  points,
  usableAltDeg,
  darkWindowHours,
  moonSpans = [],
}: AltitudeGraphProps) {
  // #757 defense-in-depth: the caller (TargetDetail) already gates this
  // component out of the render tree for the degrade states where `points`
  // is `[]` (DEGRADE_ROW — no coordinates / no site). Guard here too so a
  // future caller passing an empty curve degrades to nothing rather than
  // crashing on `points.reduce(..., points[0])` → `undefined`.
  if (points.length === 0) return null;

  const yScale = altitudeScale(PLOT_H, 0);
  // #759: the X-axis domain follows the curve's real sunset→sunrise span
  // instead of a fixed 12 h, so long nights don't flatten their last third
  // onto the rightmost pixel (data values were always correct; only the
  // axis/guide geometry was wrong).
  const maxHour = nightSpan(points);
  const xScale = hourScale(0, PLOT_W, maxHour);

  const usableYPx = yScale(usableAltDeg);

  // FR-034 (#817): with no dark window the graph must AGREE with the 0-hour
  // imaging stat — the whole plot is shaded non-dark and the above-threshold
  // fill renders grey instead of the usable green.
  const noDark = darkWindowHours === null;

  const clampHour = (h: number) => Math.min(maxHour, Math.max(0, h));

  // Transit marker: find point closest to peak altitude.
  const peak = points.reduce(
    (best, p) => (p.altDeg > best.altDeg ? p : best),
    points[0],
  );
  const transitXPx = xScale(peak.tHour);

  // X-axis tick labels, every 2 h from night start (clock = 18:00 + h,
  // wrapping past midnight), through the real span rather than a fixed 12 h.
  const xTicks: Array<{ tHour: number; label: string }> = [];
  for (let h = 0; h <= maxHour; h += 2) {
    const clock = (18 + h) % 24;
    xTicks.push({ tHour: h, label: String(clock).padStart(2, '0') });
  }

  // Y-axis tick labels.
  const yTicks = [0, 30, 60, 90];

  return (
    <div className="pv-planner__graph-wrap">
      {/* viewBox and width/height are geometry — inline SVG attributes */}
      <svg
        className="pv-planner__graph-svg"
        viewBox={`0 0 ${SVG_W} ${SVG_H}`}
        aria-label={m.targets_detail_alt_graph_aria()}
        role="img"
      >
        <Group left={PAD_L} top={PAD_T}>
          {/* US4/T031: twilight (not-dark) shading either side of the real
              dark window. FR-034: with NO dark window the ENTIRE plot is
              shaded non-dark (it used to be omitted, which read as an
              all-dark night and contradicted the 0-hour stat — #817). */}
          {noDark && (
            <rect
              x={0}
              y={0}
              width={PLOT_W}
              height={PLOT_H}
              className="pv-planner__graph-twilight"
              data-testid="planner-graph-twilight"
            />
          )}
          {darkWindowHours && darkWindowHours.startHour > 0 && (
            <rect
              x={0}
              y={0}
              width={Math.max(0, xScale(darkWindowHours.startHour))}
              height={PLOT_H}
              className="pv-planner__graph-twilight"
              data-testid="planner-graph-twilight"
            />
          )}
          {darkWindowHours && darkWindowHours.endHour < 12 && (
            <rect
              x={xScale(darkWindowHours.endHour)}
              y={0}
              width={Math.max(0, PLOT_W - xScale(darkWindowHours.endHour))}
              height={PLOT_H}
              className="pv-planner__graph-twilight"
              data-testid="planner-graph-twilight"
            />
          )}

          {/* Usable-altitude shading — clipped to where the CURVE is actually
              above the threshold (T035), not a static full-width band. */}
          <Threshold<AltPoint>
            id="tonight-usable-threshold"
            data={points}
            x={(p) => xScale(p.tHour)}
            y0={() => usableYPx}
            y1={(p) => yScale(p.altDeg)}
            clipAboveTo={0}
            clipBelowTo={PLOT_H}
            // FR-034: no dark window → grey the fill; green would claim
            // usable imaging time the stats correctly report as zero.
            aboveAreaProps={
              noDark
                ? { fill: 'var(--pv-text-faint)', opacity: 0.25 }
                : { fill: 'var(--pv-ok-bg)', opacity: 0.6 }
            }
            belowAreaProps={{ fill: 'none' }}
          />

          {/* Moon-excluded spans for the displayed band (FR-007 overlay):
              bottom-anchored band so it reads as a time-range exclusion
              without hiding the curve. */}
          {moonSpans.map((s) => {
            const x0 = xScale(clampHour(s.startHour));
            const x1 = xScale(clampHour(s.endHour));
            return (
              <rect
                key={`moon-${s.startHour}`}
                x={x0}
                y={PLOT_H - 6}
                width={Math.max(1, x1 - x0)}
                height={6}
                className="pv-planner__graph-moon"
              />
            );
          })}

          {/* Usable-altitude guide line. */}
          <line
            x1={0}
            y1={usableYPx}
            x2={PLOT_W}
            y2={usableYPx}
            stroke="var(--pv-ok-border)"
            strokeWidth={1}
            strokeDasharray="3 3"
          />

          {/* Altitude curve. */}
          <LinePath<AltPoint>
            data={points}
            x={(p) => xScale(p.tHour)}
            y={(p) => yScale(p.altDeg)}
            stroke="var(--pv-accent)"
            strokeWidth={1.5}
            strokeLinejoin="round"
            strokeLinecap="round"
          />

          {/* Transit vertical marker. */}
          <line
            x1={transitXPx}
            y1={0}
            x2={transitXPx}
            y2={PLOT_H}
            stroke="var(--pv-accent)"
            strokeWidth={1}
            strokeDasharray="2 2"
            opacity={0.6}
          />
          <text
            x={transitXPx + 3}
            y={9}
            className="pv-planner__graph-label-text"
          >
            {m.targets_detail_transit()}
          </text>

          {/* Y-axis */}
          <line
            x1={0}
            y1={0}
            x2={0}
            y2={PLOT_H}
            stroke="var(--pv-border)"
            strokeWidth={1}
          />
          {/* X-axis */}
          <line
            x1={0}
            y1={PLOT_H}
            x2={PLOT_W}
            y2={PLOT_H}
            stroke="var(--pv-border)"
            strokeWidth={1}
          />

          {/* Y-axis ticks + labels */}
          {yTicks.map((alt) => (
            <g key={alt}>
              <line
                x1={-3}
                y1={yScale(alt)}
                x2={0}
                y2={yScale(alt)}
                stroke="var(--pv-border)"
                strokeWidth={1}
              />
              <text
                x={-5}
                y={yScale(alt) + 3}
                textAnchor="end"
                className="pv-planner__graph-axis-text"
              >
                {alt}°
              </text>
            </g>
          ))}

          {/* X-axis ticks + labels */}
          {xTicks.map(({ tHour, label }) => (
            <g key={tHour}>
              <line
                x1={xScale(tHour)}
                y1={PLOT_H}
                x2={xScale(tHour)}
                y2={PLOT_H + 3}
                stroke="var(--pv-border)"
                strokeWidth={1}
              />
              <text
                x={xScale(tHour)}
                y={PLOT_H + 12}
                textAnchor="middle"
                className="pv-planner__graph-axis-text"
              >
                {label}
              </text>
            </g>
          ))}
        </Group>
      </svg>
    </div>
  );
}
