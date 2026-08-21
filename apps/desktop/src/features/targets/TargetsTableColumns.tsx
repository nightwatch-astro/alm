// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Column model + the imaging-time cell for `TargetsTable` (refactor sweep
 * #976, iteration 2026-07-15 FR-030/FR-031/FR-032). Split out of the
 * component so the column definitions and their one stateful render helper
 * (`ImagingTimeCell`) aren't interleaved with the table's data derivation
 * and virtualization logic.
 */

import type { RowAltitude } from './planner-altitude';
import type { ZeroImagingReason } from './planner-derive';
import type { TargetSortCol } from './table-model';
import { m } from '@/lib/i18n';
import * as tt from './targets-table.css';

// ── Column model (#85 + spec 044) ──────────────────────────────────────────────
//
// Designation + Type + Sessions are kept. Constellation/Magnitude are replaced
// by planning columns. Spec 044 adds Lunar dist, Filters possible, Imaging time.
//
// Opposition: real next midnight-transit peak date (spec 047 US4,
// astro/opposition.ts); unknown coordinates render '—'.
// Sessions: real linked-session count from `TargetListItem.sessionCount`
// (#877/#622); zero renders '—'.
// All non-text columns are sortable on their mock value.

// `label`/`title` are render-time thunks (spec 046 #8b) so headers re-read the active locale.
export const COLUMNS: Array<{
  key: string;
  label: () => string;
  sort?: TargetSortCol;
  className?: string;
  title?: () => string;
}> = [
  // task #18: star column (no label — icon-only header)
  {
    key: 'star',
    label: () => '★',
    className: tt.cellCenter,
    title: () => m.targets_col_favourite(),
  },
  {
    key: 'designation',
    label: () => m.targets_col_designation(),
    sort: 'designation',
  },
  { key: 'type', label: () => m.cmp_target_search_type_label(), sort: 'type' },
  {
    key: 'maxAlt',
    label: () => m.targets_col_max_alt(),
    sort: 'maxAlt',
    className: tt.cellNum,
    title: () => m.targets_table_max_alt_title(),
  },
  {
    key: 'opposition',
    label: () => m.targets_col_opposition(),
    sort: 'opposition',
    className: tt.cellOpposition,
    title: () => m.targets_table_next_opposition(),
  },
  // task #5: abbreviated header "Lunar" fits the widened 80px column without clipping
  {
    key: 'lunarDist',
    label: () => m.targets_col_lunar(),
    sort: 'lunarDist',
    className: tt.cellNum,
    title: () => m.targets_col_lunar_title(),
  },
  {
    key: 'filters',
    label: () => m.common_filters(),
    className: tt.cellFilters,
    title: () => m.targets_col_filters_title(),
  },
  // task #5: abbreviated header "Img time" fits the widened 100px column without clipping
  {
    key: 'imagingTime',
    label: () => m.targets_col_img_time(),
    sort: 'imagingTime',
    className: tt.cellNum,
    title: () => m.targets_col_img_time_title(),
  },
  {
    key: 'sessions',
    label: () => m.common_sessions(),
    sort: 'sessions',
    className: tt.cellNum,
    title: () => m.targets_col_sessions_title(),
  },
];

// COL_COUNT is derived from COLUMNS so adding/removing a column stays in sync.
export const COL_COUNT = COLUMNS.length;

// ── Imaging-time cell (iteration 2026-07-15, FR-030/FR-031/FR-032) ──────────

/** "2h10m"-style imaging duration; whole hours drop the minute part (FR-032). */
function formatImagingHours(hours: number): string {
  const totalMin = Math.round(hours * 60);
  const h = Math.floor(totalMin / 60);
  const min = totalMin % 60;
  return min === 0
    ? m.targets_imgtime_h({ h })
    : m.targets_imgtime_hm({ h, min });
}

const ZERO_REASON_GLYPH: Record<ZeroImagingReason, string> = {
  darkness: '☀',
  altitude: '▲',
  moon: '☾',
};

/**
 * Imaging-time value + why-glyph. Zero values always carry a warning glyph
 * with the FR-029 reason (FR-030 — no bare 0 is reachable, SC-015); the
 * 'moon' reason shows the geometric window it invalidates. Non-zero values
 * carry a muted ☾ naming the affected bands only when the Moon actionably
 * shortens some band's window (FR-031); a purely darkness/altitude-capped
 * value shows no glyph.
 */
export function ImagingTimeCell({
  alt,
  threshold,
}: {
  alt: RowAltitude;
  threshold: number;
}) {
  // Degrade states (no coordinates / no site): no astronomy was attempted,
  // so there is no reason to state — matches the row's other '—' cells.
  if (alt.needsCoordinates || alt.needsSite) {
    return (
      <span className={tt.cellMuted} title={m.targets_col_img_time_title()}>
        —
      </span>
    );
  }

  // FR-036/SC-017: for an OSC camera the headline collapses to the
  // strictest-band single-pass window; null (mono/unknown/not computed)
  // keeps the geometric dark ∩ uptime value byte-identical.
  const headlineHours = alt.oscSinglePassHours ?? alt.hoursAboveUsable;

  const reason = alt.zeroImagingReason;
  if (reason !== null) {
    const title =
      reason === 'darkness'
        ? m.targets_imgtime_zero_darkness_title()
        : reason === 'altitude'
          ? m.targets_imgtime_zero_altitude_title({ threshold })
          : m.targets_imgtime_zero_moon_title();
    return (
      <span title={title}>
        {headlineHours > 0 ? formatImagingHours(headlineHours) : '—'}{' '}
        <span
          role="img"
          aria-label={title}
          className={tt.imgtimeGlyphWarn}
          data-testid="imgtime-glyph-warn"
        >
          {ZERO_REASON_GLYPH[reason]}
        </span>
      </span>
    );
  }

  // OSC single-pass zero with a non-zero geometric window: the Moon blocks
  // the strictest band of every single-pass exposure tonight — the FR-030
  // moon case as it manifests for OSC (per-band reasons can't fire because
  // some individual line may still be viable; the detail panel shows those
  // per-line windows, FR-037).
  if (alt.oscSinglePassHours !== null && alt.oscSinglePassHours <= 0) {
    const title = m.targets_imgtime_zero_moon_title();
    return (
      <span title={title}>
        {'—'}{' '}
        <span
          role="img"
          aria-label={title}
          className={tt.imgtimeGlyphWarn}
          data-testid="imgtime-glyph-warn"
        >
          ☾
        </span>
      </span>
    );
  }

  if (headlineHours <= 0) {
    // Unreachable once astronomy ran (zero always carries a reason, SC-015),
    // but never render a bare 0.
    return (
      <span className={tt.cellMuted} title={m.targets_col_img_time_title()}>
        —
      </span>
    );
  }

  const limitedTitle =
    alt.moonLimitedBands.length > 0
      ? m.targets_imgtime_moon_limited_title({
          bands: alt.moonLimitedBands.join(' · '),
        })
      : null;
  return (
    <span
      title={m.targets_table_hours_above_title({
        hours: headlineHours.toFixed(1),
        threshold,
      })}
    >
      {formatImagingHours(headlineHours)}
      {limitedTitle !== null && (
        <>
          {' '}
          <span
            role="img"
            aria-label={limitedTitle}
            title={limitedTitle}
            className={tt.imgtimeGlyphMuted}
          >
            ☾
          </span>
        </>
      )}
    </span>
  );
}
