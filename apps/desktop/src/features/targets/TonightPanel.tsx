// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TonightPanel — the tonight altitude graph + planner stats + filter guidance.
 *
 * Extracted from TargetDetail.tsx for isolation.
 */

import { Link } from '@tanstack/react-router';
import { PropertyTable, type PropertyDef } from '@/components';
import { Banner } from '@/ui';
import { m } from '@/lib/i18n';
import { USABLE_ALT_DEG } from './planner-altitude';
import type { SensorConfig } from './planner-derive';
import { formatOppositionDate, oppositionRelative } from './astro/opposition';
import { AltitudeGraph } from './AltitudeGraph';
import { formatSeparationFigure } from './target-detail-format';
import { GuidanceCell } from './GuidanceCell';
import type { UseTargetTonightResult } from './useTargetTonight';

export interface TonightPanelProps {
  tonight: UseTargetTonightResult;
  effectiveLabel: string;
  usableAltDeg?: number;
  sensorConfig?: SensorConfig | null;
}

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: panel render over visibility windows and per-condition labels
export function TonightPanel({
  tonight,
  effectiveLabel,
  usableAltDeg = USABLE_ALT_DEG,
  sensorConfig = null,
}: TonightPanelProps) {
  const {
    rowAlt,
    tonightPoints,
    tonightAvailable,
    moonSpans,
    moon,
    bestMoon,
    guidanceParams,
    site,
    night,
  } = tonight;

  // Tonight stats (numeric).
  const relativeText = (inDays: number): string => {
    const rel = oppositionRelative(inDays);
    return rel.unit === 'days'
      ? m.targets_opposition_in_days({ count: rel.count })
      : m.targets_opposition_in_months({ count: rel.count });
  };
  const bestDateValue = bestMoon
    ? `${formatOppositionDate(new Date(bestMoon.dateMs))} · ${relativeText(bestMoon.inDays)}`
    : null;
  const bestDateTooltip = bestMoon
    ? bestMoon.state === 'diverged'
      ? m.targets_best_date_tooltip_diverged({
          oppDate: formatOppositionDate(new Date(bestMoon.oppositionDateMs)),
          oppIllum: bestMoon.moonAtOpposition.illumPct,
          oppSep: Math.round(bestMoon.moonAtOpposition.sepDeg),
          bestDate: formatOppositionDate(new Date(bestMoon.dateMs)),
          bestIllum: bestMoon.moonAtBest.illumPct,
          bestSep: Math.round(bestMoon.moonAtBest.sepDeg),
        })
      : bestMoon.state === 'coincides'
        ? m.targets_best_date_tooltip_coincides({
            illum: bestMoon.moonAtBest.illumPct,
            sep: Math.round(bestMoon.moonAtBest.sepDeg),
          })
        : m.targets_best_date_tooltip_none()
    : undefined;

  const darkWindowValue = rowAlt.darkWindowHours
    ? `${(rowAlt.darkWindowHours.endHour - rowAlt.darkWindowHours.startHour).toFixed(1)} h`
    : m.targets_detail_dark_window_none();
  const imagingHeadlineHours =
    rowAlt.oscSinglePassHours ?? rowAlt.hoursAboveUsable;
  const oscPassband = sensorConfig?.passband;
  const oscLineValue =
    sensorConfig?.sensorType === 'osc' &&
    Array.isArray(oscPassband) &&
    oscPassband.length > 0 &&
    rowAlt.oscSinglePassHours !== null
      ? oscPassband
          .map(
            (b) => `${b} ${(rowAlt.moonFreeMinutesByBand[b] / 60).toFixed(1)}h`,
          )
          .join(' · ')
      : null;

  const tonightStats: PropertyDef[] = tonightAvailable
    ? [
        {
          key: 'maxalt',
          label: m.targets_col_max_alt(),
          value: `${Math.round(rowAlt.maxAltDeg)}°`,
        },
        {
          key: 'darkwindow',
          label: m.targets_detail_dark_window(),
          value: darkWindowValue,
        },
        {
          key: 'uptime',
          label: m.targets_detail_uptime({ threshold: usableAltDeg }),
          value: `${rowAlt.uptimeHours.toFixed(1)} h`,
        },
        {
          key: 'imgtime',
          label: m.targets_col_img_time(),
          value: `${imagingHeadlineHours.toFixed(1)} h`,
        },
        ...(oscLineValue !== null
          ? [
              {
                key: 'osclines',
                label: m.targets_detail_osc_lines(),
                value: oscLineValue,
              },
            ]
          : []),
        {
          key: 'lunar',
          label: m.targets_col_lunar(),
          value:
            moon.lunarSeparationDeg != null
              ? `${Math.round(moon.lunarSeparationDeg)}°`
              : null,
        },
        {
          key: 'bestdate',
          label: m.targets_col_best_date(),
          value: bestDateValue,
          tooltip: bestDateTooltip,
        },
      ]
    : [];

  const moonTrio: PropertyDef[] = tonightAvailable
    ? [
        {
          key: 'moon-transit',
          label: m.targets_detail_moon_at_transit(),
          value: formatSeparationFigure(rowAlt.separationScalars.atTransitDeg),
        },
        {
          key: 'moon-min-dark',
          label: m.targets_detail_moon_min_over_dark(),
          value: formatSeparationFigure(
            rowAlt.separationScalars.minOverDarkDeg,
          ),
        },
        {
          key: 'moon-dark-mid',
          label: m.targets_detail_moon_at_dark_midpoint(),
          value: formatSeparationFigure(
            rowAlt.separationScalars.atDarkMidpointDeg,
          ),
        },
      ]
    : [];

  return (
    <div className="pv-planner__tonight">
      <div className="pv-planner__graph-title">
        {site
          ? m.targets_detail_tonight_title({
              lat: Math.round(site.latitudeDeg),
            })
          : m.targets_detail_tonight_title_no_site()}
      </div>
      {rowAlt.needsSite ? (
        <Banner variant="info">
          {m.targets_planner_no_site_banner()}{' '}
          <Link
            to="/settings/$pane"
            params={{ pane: 'planner' }}
            className="pv-banner__action-link"
          >
            {m.targets_planner_no_site_banner_action()}
          </Link>
        </Banner>
      ) : rowAlt.needsCoordinates ? (
        <Banner variant="info">
          {m.targets_detail_needs_coordinates_banner()}
        </Banner>
      ) : (
        <>
          <AltitudeGraph
            points={tonightPoints}
            usableAltDeg={usableAltDeg}
            darkWindowHours={rowAlt.darkWindowHours}
            moonSpans={moonSpans}
          />
          {tonightAvailable && rowAlt.zeroImagingReason !== null && (
            <Banner variant="info">
              {rowAlt.zeroImagingReason === 'darkness'
                ? m.targets_imgtime_zero_darkness_title()
                : rowAlt.zeroImagingReason === 'altitude'
                  ? m.targets_imgtime_zero_altitude_title({
                      threshold: usableAltDeg,
                    })
                  : m.targets_imgtime_zero_moon_title()}
            </Banner>
          )}
          {tonightAvailable && (
            <>
              <PropertyTable mode="view" properties={tonightStats} />
              <div className="pv-planner__tonight-filters">
                <span className="pv-planner__tonight-filters-label">
                  {m.targets_detail_moon_trio_title()}
                </span>
                <PropertyTable mode="view" properties={moonTrio} />
              </div>
              <div className="pv-planner__tonight-filters">
                <span className="pv-planner__tonight-filters-label">
                  {m.common_filters()}
                </span>
                <GuidanceCell
                  night={night}
                  moon={moon}
                  params={guidanceParams}
                  targetLabel={effectiveLabel}
                  moonFreeMinutesByBand={rowAlt.moonFreeMinutesByBand}
                />
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
