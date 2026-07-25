// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TargetsTable — spec 043 shared list-page adoption (task #73), refined #82,
 * VIRTUALIZED + planning columns (#84/#85), spec 044 mock columns.
 *
 * A DENSE, FULL-WIDTH sortable table (shared `Table` look) that is the primary
 * content of the Targets page's `ListPageLayout`; TargetDetail lives in the
 * detail pane.
 *
 * Columns (spec 044, iteration 2026-07-15 FR-007): Designation · Type ·
 * Max alt · Opposition · Lunar dist · Filters · Imaging time · Sessions.
 * The per-row altitude sparkline and visible-tonight columns are removed —
 * the detail panel's altitude graph is the canonical altitude view, and
 * visibility is folded into the imaging-time glyph (FR-030/FR-031).
 *
 * Task #84 — VIRTUALIZATION (padding-spacer pattern):
 *   The Planner catalogue is large; rendering every row synchronously blocks the
 *   main thread. Rows (group-header rows AND target rows) are flattened into one
 *   list and windowed with `@tanstack/react-virtual`.
 *
 *   IMPORTANT — mixing a real CSS `<table>` with TanStack Virtual requires the
 *   PADDING-SPACER pattern, NOT the tbody-height + transform pattern:
 *
 *   Problem: setting `height: totalSize` on `<tbody>` and leaving rows in normal
 *   table flow (display:table-row) causes the browser to distribute the total
 *   height across the rendered rows — each row stretches to totalSize/rowCount
 *   (~1.7 M px with 13 rows rendered from 11 k). If `measureElement` is also
 *   attached it reads the stretched height and re-inflates totalSize → runaway
 *   feedback loop → effectively invisible table.
 *
 *   Fix: keep `<tbody>` height unstyled (natural). Render two sentinel spacer
 *   `<tr>` rows — one before the windowed slice (height = start of first virtual
 *   row) and one after (height = totalSize − end of last virtual row). Real data
 *   rows render between them in normal table flow at their natural ~36 px height.
 *   No transforms, no absolutepositioning, no tbody height, no measureElement.
 *   Row heights are uniform so fixed `estimateSize` is exact.
 *
 *   jsdom fallback: when the scroll element has no measurable height the
 *   virtualizer yields zero virtual items. In that case all rows render with no
 *   spacers — windowing is a runtime perf optimization, not a behavior change.
 *
 * Task #85 — PLANNING COLUMNS:
 *   The low-value Constellation/Magnitude columns are replaced with
 *   planning-relevant ones driven by the real per-site ephemeris
 *   (`planner-altitude.ts`/`planner-astronomy.ts`, spec 044 Track B,
 *   astronomy-engine): max altitude tonight, a tiny inline altitude SPARKLINE
 *   per row, and a visible-tonight indicator, all computed from the target's
 *   real J2000 coordinates and the active observing site — never derived from
 *   the designation.
 *
 * Spec 047 real Track-A astronomy columns (date/time + catalogued RA/Dec only;
 * replaces the former spec 044 §3 mock columns):
 *   - Lunar dist: real target↔Moon separation from the shared `ObservingNight`
 *     (US2); unknown coordinates/no site → explicit "—", never a number.
 *   - Filters: real per-band Moon-avoidance viability pills + derived
 *     recommendation (US3, `GuidanceCell`/`astro/moon-avoidance.ts`), with a
 *     hover/focus explanation popover.
 *   - Opposition: real next-opposition date (US4, `astro/opposition.ts`),
 *     date-level + relative "in N days/months"; unknown → "—".
 *   - Imaging time: real hours above the user-configured altitude threshold
 *     tonight (spec 044 Track B, dark-window-gated).
 *   All are SORTABLE; the usable-altitude threshold is configurable via Settings.
 *
 * Task #82:
 *   Row density follows the GLOBAL density setting (`density-*` on <html>).
 *   GROUP-BY: rows render under spanning group-header rows, grouped by Catalogue
 *   (default) or Object type.
 *
 * Search + the catalogue / group-by controls live in the page top bar; this
 * surface owns no toolbar state. Selecting a row opens TargetDetail (selection
 * is driven by the host page via `?selected`).
 *
 * Split by responsibility (refactor sweep #976): `table-model.ts` is the pure
 * sort/group/row-cache model; `TargetsTableColumns.tsx` is the column
 * definitions + `ImagingTimeCell`; `useTargetsTableRows.ts` is the data
 * derivation + virtualizer/windowing. This file is Props + the render.
 */

import { Link } from '@tanstack/react-router';
import type { TargetListItem } from '@/bindings/index';
import { Pill, Banner, Skeleton, tableIndent } from '@/ui';
import { SortHeader, ariaSortFor } from '@/components';
import { UnresolvedChip } from '@/components/RenderValue';
import { USABLE_ALT_DEG, rowAltitudeFor } from './planner-altitude';
import type { ObservingNight } from './astro/moon-state';
import {
  DEFAULT_MOON_AVOIDANCE,
  type MoonAvoidanceParams,
} from './astro/moon-avoidance';
import type { SensorConfig } from './planner-derive';
import { formatOppositionDate, oppositionRelative } from './astro/opposition';
import { GuidanceCell } from './GuidanceCell';
import { m } from '@/lib/i18n';
import { useFavourites } from './useFavourites';
import { useActiveSite } from './observing-sites/site-store';
import { usePlannerDateMs } from './planner-date-store';
import { usePreference } from '@/data/preferences';
import { ROW_HEIGHT_PX } from '@/data/theme';
import { MoonSummary } from './MoonSummary';
import { useCollapsibleGroups } from '@/lib/use-grouping';
import {
  formatType,
  getCachedRow,
  rowCacheGenKey,
  type TargetGroupBy,
  type TargetSort,
  type TargetSortCol,
} from './table-model';
import { COLUMNS, COL_COUNT, ImagingTimeCell } from './TargetsTableColumns';
import { useTargetsTableRows } from './useTargetsTableRows';

export type {
  TargetSortCol,
  SortDir,
  TargetSort,
  TargetGroupBy,
} from './table-model';
export {
  DEFAULT_TARGET_SORT,
  DEFAULT_TARGET_GROUP_BY,
  formatType,
} from './table-model';

/** Extra rows rendered above/below the viewport to avoid blank flashes on scroll. */
const OVERSCAN = 12;

interface Props {
  targets: TargetListItem[];
  selected: string | null;
  onSelect: (id: string) => void;
  loading?: boolean;
  sort: TargetSort;
  onSort: (col: TargetSortCol) => void;
  /**
   * Legacy single-tier group-by (kept for back-compat with tests).
   * When `dims` is provided it takes precedence; `groupBy` is only used as
   * the fallback when `dims` is empty.
   * @deprecated Prefer `dims` from `useGrouping`.
   */
  groupBy?: TargetGroupBy;
  /**
   * Active ordered grouping dimension ids from `useGrouping`.
   * When non-empty, drives multi-level collapsible grouping; overrides `groupBy`.
   * When empty, falls back to `groupBy` (single-tier, legacy).
   */
  dims?: string[];
  /** Message shown when the list is empty (tab-specific). */
  emptyMessage?: string;
  /**
   * User-configured usable-altitude threshold in degrees (default USABLE_ALT_DEG).
   * Drives hoursAboveUsable, visibleTonight, and the Imaging time column.
   * Pass `useAltitudeThreshold()` from the host page.
   */
  usableAltDeg?: number;
  /**
   * The memoized observing night (spec 047), or `null` when no observing site
   * exists (site gate) so real astronomy is suppressed. Drives real per-row
   * lunar distance (US2), filter guidance (US3), and opposition (US4). One
   * `ObservingNight` per `nightKey` is computed by the host page and passed
   * down so per-row work stays O(1) and the Moon state is shared (SC-007).
   */
  night?: ObservingNight | null;
  /**
   * Active per-band Moon-avoidance parameters (spec 047 US3, Settings →
   * Target Planner). Defaults to the shipped table; pass `useGuidanceParams()`
   * from the host page so live edits recompute pills/recommendation (SC-008).
   */
  guidanceParams?: MoonAvoidanceParams;
  /**
   * Set of currently-favourited target ids (task #18, spec 051 US2).
   * When provided the star column renders filled for matched ids.
   */
  favouriteIds?: ReadonlySet<string>;
  /**
   * Called when the user clicks the star button in a row (task #18).
   */
  onToggleFavourite?: (targetId: string) => void;
  /**
   * Camera sensor configuration (FR-035/FR-036, T046): when OSC, the
   * imaging-time headline collapses to the strictest-band single-pass
   * window; `null`/absent keeps the per-filter model unchanged (FR-038).
   * Pass `usePlannerSensorConfig()` from the host page.
   */
  sensorConfig?: SensorConfig | null;
}

export function TargetsTable({
  targets,
  selected,
  onSelect,
  loading,
  sort,
  onSort,
  // No default: when neither `dims` nor an explicit `groupBy` is supplied the
  // table renders a FLAT sorted list (previously this defaulted to
  // 'catalogue', which grouped Targets by catalogue even with nothing selected).
  groupBy,
  dims,
  emptyMessage = m.targets_table_no_match(),
  usableAltDeg = USABLE_ALT_DEG,
  night = null,
  guidanceParams = DEFAULT_MOON_AVOIDANCE,
  favouriteIds,
  onToggleFavourite,
  sensorConfig = null,
}: Props) {
  // task #18: subscribe to the local favourites store.  The host page (TargetsPage)
  // passes its own useFavourites result down; if it doesn't (e.g. tests that don't
  // need favourites), we fall back to an internal subscription.  This keeps the
  // table self-contained when used standalone.
  const internalFavourites = useFavourites();
  const resolvedFavouriteIds = favouriteIds ?? internalFavourites.favouriteIds;
  const resolvedToggle = onToggleFavourite ?? internalFavourites.toggle;
  // Row height follows the GLOBAL density setting (task #82, `--pv-row-height`
  // in tokens.css); the virtualizer has no measureElement (see file docstring
  // — rows are uniform so estimateSize must be exact), so this must track the
  // active density rather than a fixed guess.
  const [density] = usePreference('density');
  const rowEstimate = ROW_HEIGHT_PX[density] ?? ROW_HEIGHT_PX.comfortable;
  const { collapsed, toggle: toggleCollapsed } = useCollapsibleGroups();

  // US6/T015: the active observing site drives every row's real astronomy.
  // Self-contained subscription (mirrors useFavourites above) so callers don't
  // need to thread a `site` prop through — when there is no active site every
  // row degrades to the "needs a site" zero/not-visible state (T013) and the
  // banner below prompts the user to add one.
  const site = useActiveSite();

  // US2/T024: the Planner's chosen date (defaults to "tonight", never
  // persisted — FR-008). Every rowAltitudeFor call below reads it so choosing
  // a different date recomputes the whole table (SC-004).
  const dateMs = usePlannerDateMs();

  const { scrollRef, flatRows, renderIndices, paddingBefore, paddingAfter } =
    useTargetsTableRows({
      targets,
      sort,
      groupBy,
      dims,
      usableAltDeg,
      site,
      night,
      guidanceParams,
      dateMs,
      collapsed,
      rowEstimate,
      overscan: OVERSCAN,
    });

  const columns = COLUMNS.map((c) => ({
    key: c.key,
    className: c.className,
    title: c.title?.(),
    // aria-sort belongs on the <th> (this table renders its own header row).
    ariaSort: c.sort ? ariaSortFor(sort.col === c.sort, sort.dir) : undefined,
    header: c.sort ? (
      <SortHeader
        label={c.label()}
        active={sort.col === c.sort}
        dir={sort.dir}
        onClick={() => onSort(c.sort as TargetSortCol)}
        ariaLabel={m.common_sort_by_aria({ col: c.label() })}
        title={c.title?.()}
      />
    ) : (
      c.label()
    ),
  }));

  const count = targets.length;

  // #618: the Moon-phase widget (and its no-site fallback prompt) moved here
  // from the Targets page's pinned top bar (which was wrapping to a 3rd
  // stacked band before any table data appeared) — this is the "table header
  // zone" alternative from the design-review recommendation, chosen over the
  // detail rail because the #450 dead-gate regression guard requires the
  // prompt/summary to be visible without selecting any row.
  const moonHeader = night ? (
    <MoonSummary night={night} />
  ) : (
    <div className="pv-planner-site-prompt" data-testid="planner-site-prompt">
      <span className="pv-planner-site-prompt__title">
        {m.targets_planner_site_prompt_title()}
      </span>
      <span className="pv-planner-site-prompt__desc">
        {m.targets_planner_site_prompt_desc()}
      </span>
    </div>
  );

  if (count === 0 && loading) {
    return (
      <div className="pv-targets-table__wrap" data-testid="targets-table-wrap">
        {moonHeader}
        <div className="pv-targets-table__empty">
          <Skeleton variant="block" count={8} label={m.common_loading()} />
        </div>
      </div>
    );
  }

  if (count === 0 && !loading) {
    return (
      <div className="pv-targets-table__wrap" data-testid="targets-table-wrap">
        {moonHeader}
        <div className="pv-targets-table__empty">{emptyMessage}</div>
      </div>
    );
  }

  return (
    <div className="pv-targets-table__wrap" data-testid="targets-table-wrap">
      {moonHeader}
      {!site && (
        <Banner
          variant="info"
          className="pv-targets-table__no-site-banner"
          data-testid="targets-table-no-site-banner"
        >
          {m.targets_planner_no_site_banner()}{' '}
          <Link
            to="/settings/$pane"
            params={{ pane: 'planner' }}
            className="pv-banner__action-link"
          >
            {m.targets_planner_no_site_banner_action()}
          </Link>
        </Banner>
      )}
      <div
        ref={scrollRef}
        className="pv-targets-table__scroll"
        data-testid="targets-table-scroll"
      >
        <table className="pv-table pv-targets-table">
          {/* Fixed-layout colgroup: column widths are pinned so the table
              does NOT recompute widths per windowed page as pill text varies
              (e.g. "galaxy" vs "open cluster" would shift all columns).
              Designation is auto (fills remaining width); fixed widths on
              the right prevent the per-page column-shift bug.
              task #18: star col added first (28 px, wave2 CSS block). */}
          <colgroup>
            <col className="pv-targets-col--star" />
            <col className="pv-targets-col--designation" />
            <col className="pv-targets-col--type" />
            <col className="pv-targets-col--maxalt" />
            <col className="pv-targets-col--opposition" />
            {/* task #5: lunardist widened to 80px (wave2 CSS block). */}
            <col className="pv-targets-col--lunardist" />
            <col className="pv-targets-col--filters" />
            {/* task #5: imagingtime widened to 100px (wave2 CSS block). */}
            <col className="pv-targets-col--imagingtime" />
            <col className="pv-targets-col--sessions" />
          </colgroup>
          <thead>
            <tr>
              {columns.map((c) => (
                <th
                  key={c.key}
                  className={c.className}
                  title={c.title}
                  aria-sort={c.ariaSort}
                >
                  {c.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="pv-targets-table__body">
            {/* Before-spacer: reserve height for virtual rows above the window.
                Height is dynamic (virtualizer offset), allowed by convention. */}
            {paddingBefore > 0 && (
              <tr aria-hidden="true" className="pv-targets-table__spacer">
                {/* eslint-disable no-restricted-syntax, jsx-a11y/control-has-associated-label -- dynamic: virtualizer before-spacer height; empty presentational cell inside an aria-hidden spacer row (no label needed) */}
                <td
                  colSpan={COL_COUNT}
                  style={{ height: `${paddingBefore}px` }}
                />
                {/* eslint-enable no-restricted-syntax, jsx-a11y/control-has-associated-label */}
              </tr>
            )}

            {renderIndices.map((index) => {
              const row = flatRows[index];

              if (row.kind === 'group') {
                if (row.collapsible && row.path != null) {
                  // Captured as a local so the onClick closure keeps the
                  // narrowing above — a nested arrow function re-reads
                  // `row.path` from scratch and TS would widen it back to
                  // `string | undefined`.
                  const { path } = row;
                  // Multi-level collapsible group header.
                  return (
                    <tr
                      key={row.key}
                      data-index={index}
                      className="pv-listgroup"
                    >
                      <td colSpan={COL_COUNT}>
                        <button
                          type="button"
                          className="pv-listgroup__cell"
                          data-testid={`targets-group-${row.key}`}
                          aria-expanded={!row.collapsed}
                          aria-label={row.label}
                          onClick={() => toggleCollapsed(path)}
                          // eslint-disable-next-line no-restricted-syntax -- dynamic: depth-based group-header indent
                          style={{ paddingLeft: tableIndent(row.depth ?? 0) }}
                        >
                          <span
                            className="pv-listgroup__caret"
                            aria-hidden="true"
                          >
                            {row.collapsed ? '▸' : '▾'}
                          </span>
                          <span className="pv-listgroup__label">
                            {row.label}
                          </span>
                          <span className="pv-listgroup__count">
                            {m.targets_table_target_count({ count: row.count })}
                          </span>
                        </button>
                      </td>
                    </tr>
                  );
                }
                // Legacy non-collapsible group header.
                return (
                  <tr key={row.key} data-index={index} className="pv-listgroup">
                    <td colSpan={COL_COUNT}>
                      {row.label}
                      <span className="pv-listgroup__count">
                        {m.targets_table_target_count({ count: row.count })}
                      </span>
                    </td>
                  </tr>
                );
              }

              const t = row.target;
              const alt = row.alt;
              const moon = row.moon;
              // Real per-band moon-free hours (US5/T029), recomputed HERE at
              // render time — only for the ~20-30 rows actually windowed on
              // screen, not the full (possibly ~13k-entry) catalogue `alt`
              // came from (that pass used includeMoonGeometry=false for
              // performance; see rowAltitudeFor's doc).
              const altMoon = rowAltitudeFor(
                t,
                usableAltDeg,
                site,
                dateMs,
                guidanceParams,
                true,
                sensorConfig,
              );
              const showAltDesig = t.effectiveLabel !== t.primaryDesignation;
              const isSelected = selected === t.id;

              const isFav = resolvedFavouriteIds.has(t.id);

              return (
                <tr
                  key={row.key}
                  data-testid="targets-table-row"
                  data-index={index}
                  className={
                    'pv-targets-table__row pv-table__row--clickable' +
                    (isSelected ? ' pv-targets-table__row--selected' : '')
                  }
                  onClick={() => onSelect(t.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onSelect(t.id);
                    }
                  }}
                  // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- selectable row: focusable, activated by Enter/Space
                  tabIndex={0}
                  aria-selected={isSelected}
                >
                  {/* task #18: favourite star toggle.
                      STUB: stored in localStorage only until task #54 (backend linkage) lands.
                      stopPropagation prevents the row-select click from firing. */}
                  <td className="pv-targets-cell--center">
                    <button
                      type="button"
                      className={
                        'pv-targets-star' +
                        (isFav ? ' pv-targets-star--active' : '')
                      }
                      data-testid="targets-star"
                      aria-label={
                        isFav
                          ? m.targets_star_unfavourite_aria({
                              label: t.effectiveLabel,
                            })
                          : m.targets_star_favourite_aria({
                              label: t.effectiveLabel,
                            })
                      }
                      aria-pressed={isFav}
                      title={
                        isFav
                          ? m.targets_star_remove_title()
                          : m.targets_star_add_title()
                      }
                      // Onboarding find-it spotlight anchor (spec 056 FR-026).
                      data-guide-anchor="targets.favourite-toggle"
                      onClick={(e) => {
                        e.stopPropagation();
                        resolvedToggle(t.id);
                      }}
                    >
                      {isFav ? '★' : '☆'}
                    </button>
                  </td>
                  <td>
                    <span className="pv-targets-cell__desig">
                      <span className="pv-targets-cell__label">
                        {t.effectiveLabel}
                      </span>
                      {showAltDesig && (
                        <span className="pv-targets-cell__alt">
                          {t.primaryDesignation}
                        </span>
                      )}
                    </span>
                  </td>
                  <td>
                    <Pill variant="ghost">{formatType(t.objectType)}</Pill>
                  </td>
                  {/* Real peak altitude tonight (spec 044 Track B ephemeris).
                      #757: no catalogued coordinates → the shared unresolved
                      chip, never a fabricated 0°. */}
                  <td className="pv-targets-cell--num">
                    {alt.needsCoordinates ? (
                      <span title={m.targets_table_needs_coordinates_title()}>
                        <UnresolvedChip />
                      </span>
                    ) : (
                      <span title={m.targets_table_max_alt_title()}>
                        {Math.round(alt.maxAltDeg)}°
                      </span>
                    )}
                  </td>
                  {/* Real next-opposition date (spec 047 US4). Unknown
                      coordinates / no site → explicit "—", never a date. */}
                  <td className="pv-targets-cell--opposition">
                    {moon.nextOppositionDate === null ||
                    moon.daysToOpposition === null ? (
                      <span
                        className="pv-targets-cell--muted"
                        title={m.targets_opposition_unknown_title()}
                      >
                        —
                      </span>
                    ) : (
                      (() => {
                        const rel = oppositionRelative(moon.daysToOpposition);
                        const relText =
                          rel.unit === 'days'
                            ? m.targets_opposition_in_days({ count: rel.count })
                            : m.targets_opposition_in_months({
                                count: rel.count,
                              });
                        const oppositionText = `${formatOppositionDate(
                          new Date(`${moon.nextOppositionDate}T00:00:00Z`),
                        )} · ${relText}`;
                        // #792: the cell can clip at narrow widths; the title
                        // must carry the actual (recoverable) value, not the
                        // static generic column label.
                        return (
                          <span title={oppositionText}>{oppositionText}</span>
                        );
                      })()
                    )}
                  </td>
                  {/* Real lunar angular separation (spec 047 US2). Unknown
                      coordinates / no site → explicit "—", never a number. */}
                  <td className="pv-targets-cell--num">
                    {moon.lunarSeparationDeg === null ? (
                      <span
                        className="pv-targets-cell--muted"
                        title={m.targets_lunar_unknown_title()}
                      >
                        —
                      </span>
                    ) : (
                      <span
                        className="pv-targets-cell--lunardist"
                        data-testid="targets-cell-lunardist"
                        title={m.targets_table_lunar_dist_title({
                          deg: Math.round(moon.lunarSeparationDeg),
                        })}
                      >
                        {Math.round(moon.lunarSeparationDeg)}°
                      </span>
                    )}
                  </td>
                  {/* Real per-band filter guidance from the Moon-avoidance rule
                      (spec 047 US3): pills + explanation popover. */}
                  <td className="pv-targets-cell--filters">
                    <GuidanceCell
                      night={night}
                      moon={moon}
                      params={guidanceParams}
                      targetLabel={t.effectiveLabel}
                      moonFreeMinutesByBand={
                        altMoon.needsCoordinates || altMoon.needsSite
                          ? null
                          : altMoon.moonFreeMinutesByBand
                      }
                    />
                  </td>
                  {/* Imaging time (dark ∩ uptime) + why-glyph (iteration
                      2026-07-15). Zero values carry a ☀/▲/☾ warning glyph
                      with the FR-029 reason (FR-030, SC-015); non-zero values
                      carry a muted ☾ only when the Moon actionably shortens
                      some band's window (FR-031). Reason facts come from
                      `altMoon` — the per-rendered-row pass that includes Moon
                      geometry — so 'moon' is never inferred from the cheap
                      geometry-free catalogue pass. */}
                  <td className="pv-targets-cell--num">
                    <ImagingTimeCell alt={altMoon} threshold={usableAltDeg} />
                  </td>
                  {/* #622: real linked-session count (`sessionCount`, #877).
                      Zero renders '—' rather than a bare 0 — a target nothing
                      has been shot on has no coverage to report, matching the
                      muted placeholder the row's other unknown cells use. */}
                  <td className="pv-targets-cell--num">
                    {(t.sessionCount ?? 0) > 0 ? (
                      t.sessionCount
                    ) : (
                      <span className="pv-targets-cell--muted">—</span>
                    )}
                  </td>
                </tr>
              );
            })}

            {/* After-spacer: reserve height for virtual rows below the window.
                Height is dynamic (virtualizer remainder), allowed by convention. */}
            {paddingAfter > 0 && (
              <tr aria-hidden="true" className="pv-targets-table__spacer">
                {/* eslint-disable no-restricted-syntax, jsx-a11y/control-has-associated-label -- dynamic: virtualizer after-spacer height; empty presentational cell inside an aria-hidden spacer row (no label needed) */}
                <td
                  colSpan={COL_COUNT}
                  style={{ height: `${paddingAfter}px` }}
                />
                {/* eslint-enable no-restricted-syntax, jsx-a11y/control-has-associated-label */}
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="pv-targets-table__footer">
        {loading ? m.common_loading() : m.targets_table_target_count({ count })}
      </div>
    </div>
  );
}

// ── Test-only exports (#573) ────────────────────────────────────────────────
//
// Direct unit-testable access to the row cache contract (getCachedRow +
// rowCacheGenKey), mirroring the `__setObservingStateForTest` convention in
// site-store.ts. Not used by any non-test caller.
export const __testExports = { getCachedRow, rowCacheGenKey };
