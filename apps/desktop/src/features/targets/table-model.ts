// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Pure sort/group/row-cache model for `TargetsTable` (refactor sweep #976):
 * no JSX, no React — sort types, comparators, the per-target-id astronomy
 * cache (#573), single-tier grouping, and the flattened virtual-row shape.
 * Split out of the component so the model can be unit-tested and reasoned
 * about independently of rendering.
 */

import type { TargetListItem, TargetObjectType } from '@/bindings/index';
import { objectTypeLabel } from '@/components/TargetSearch/objectType';
import { catalogueOf, catalogueLabel } from '@/shared/planner/planner-catalog';
import { rowAltitudeFor, type RowAltitude } from './planner-altitude';
import type { ObservingNight } from './astro/moon-state';
import {
  deriveRowMoonPlanning,
  type RowMoonGeometry,
  type RowMoonPlanning,
} from './astro/row-planning';
import type { MoonAvoidanceParams } from '@/shared/planner/moon-avoidance';
import { recommendationLabel } from './FilterBadges';
import { m } from '@/lib/i18n';
import type { DimensionAccessor } from '@/lib/grouping';
import type { ObserverSite } from '@/shared/observing-sites/observer-site';

// ── Sort model ────────────────────────────────────────────────────────────────

/**
 * Columns the table can sort by.
 *
 * Spec 044/047 additions, sortable:
 *   - maxAlt: sorts by peak altitude tonight (Track B placeholder)
 *   - opposition: sorts by real days-to-next-opposition, soonest first;
 *     unknown coordinates always sort last regardless of direction (spec 047 US4)
 *   - lunarDist: sorts by real target↔Moon separation; unknowns sort last
 *     regardless of direction (spec 047 US2)
 *   - imagingTime: sorts by hoursAboveUsable (Track B placeholder)
 *   - sessions: sorts by real linked-session count (#622), ties by designation
 */
export type TargetSortCol =
  | 'designation'
  | 'type'
  | 'maxAlt'
  | 'opposition'
  | 'lunarDist'
  | 'imagingTime'
  | 'sessions';
export type SortDir = 'asc' | 'desc';

export interface TargetSort {
  col: TargetSortCol;
  dir: SortDir;
}

export const DEFAULT_TARGET_SORT: TargetSort = {
  col: 'designation',
  dir: 'asc',
};

// ── Grouping model (task #82) ───────────────────────────────────────────────────

/** What the table groups rows by (Planner top-bar Group-by control). */
export type TargetGroupBy = 'catalogue' | 'type';
export const DEFAULT_TARGET_GROUP_BY: TargetGroupBy = 'catalogue';

/** Formats the objectType string into a readable, localized label. */
export function formatType(objectType: string): string {
  return objectTypeLabel(objectType as TargetObjectType);
}

/** Resolve the group key + display headline for a target under `groupBy`. */
function groupHeadlineOf(t: TargetListItem, groupBy: TargetGroupBy): string {
  if (groupBy === 'type') {
    return t.objectType
      ? formatType(t.objectType)
      : m.targets_table_unknown_type();
  }
  const cat = catalogueOf(t);
  return cat ? catalogueLabel(cat) : m.targets_objtype_other();
}

function compareStr(a: string, b: string): number {
  return a.localeCompare(b);
}

/**
 * Compare two target+altitude pairs for sorting. Altitude-derived columns
 * require the pre-computed `RowAltitude` values (which encode the user's
 * threshold), so the comparator receives them alongside the list items.
 */
export function compareTargetRows(
  a: TargetListItem,
  altA: RowAltitude,
  moonA: RowMoonPlanning,
  b: TargetListItem,
  altB: RowAltitude,
  moonB: RowMoonPlanning,
  sort: TargetSort,
): number {
  let cmp = 0;
  switch (sort.col) {
    case 'designation':
      cmp = compareStr(a.effectiveLabel, b.effectiveLabel);
      break;
    case 'type':
      cmp = compareStr(a.objectType, b.objectType);
      break;
    case 'maxAlt':
      cmp = altA.maxAltDeg - altB.maxAltDeg;
      break;
    case 'opposition': {
      // Real next-opposition date (US4). Unknowns (no coordinates / no site)
      // always sort AFTER known values regardless of direction, mirroring the
      // lunarDist convention (FR-014), soonest-next first when ascending.
      const da = moonA.daysToOpposition;
      const db = moonB.daysToOpposition;
      if (da === null && db === null)
        return compareStr(a.effectiveLabel, b.effectiveLabel);
      if (da === null) return 1;
      if (db === null) return -1;
      cmp = da - db || compareStr(a.effectiveLabel, b.effectiveLabel);
      break;
    }
    case 'lunarDist': {
      // Real lunar separation (US2). Unknowns (no coordinates / no site) always
      // sort AFTER known values regardless of direction, with a deterministic
      // designation tie-break (FR-007).
      const sa = moonA.lunarSeparationDeg;
      const sb = moonB.lunarSeparationDeg;
      if (sa === null && sb === null)
        return compareStr(a.effectiveLabel, b.effectiveLabel);
      if (sa === null) return 1;
      if (sb === null) return -1;
      cmp = sa - sb || compareStr(a.effectiveLabel, b.effectiveLabel);
      break;
    }
    case 'imagingTime':
      cmp = altA.hoursAboveUsable - altB.hoursAboveUsable;
      break;
    case 'sessions':
      // #622: real `sessionCount` (#877). Absent on older clients → 0, which
      // sorts alongside genuinely-unshot targets; designation breaks ties so
      // the order stays deterministic across the many zero-count rows.
      cmp =
        (a.sessionCount ?? 0) - (b.sessionCount ?? 0) ||
        compareStr(a.effectiveLabel, b.effectiveLabel);
      break;
  }
  return sort.dir === 'asc' ? cmp : -cmp;
}

export interface TargetGroup {
  label: string;
  /** Each entry holds the item, its altitude row, and its moon-planning row. */
  rows: Array<{
    target: TargetListItem;
    alt: RowAltitude;
    moon: RowMoonPlanning;
  }>;
}

// ── Per-target row cache (#573) ─────────────────────────────────────────────
//
// The full-catalogue sort/group pass below (up to ~13k rows) is the
// unbackgrounded synchronous work that froze the app on open (#573): each
// row runs astronomy-engine altitude sampling (rowAltitudeFor) + Moon
// planning (deriveRowMoonPlanning), which is not free at catalogue scale
// (see rowAltitudeFor's doc — "a real perf cliff... a Layer-2 CI timeout
// against the full ~13k-entry bundled seed catalogue"). Re-running that for
// every target on every render (every sort/filter/group/reveal-chunk change)
// is wasted work when the astronomy INPUTS (site/date/night/threshold/
// guidance params) haven't changed for a given target. `rowCache` memoizes
// per target id, gated by `genKey` (all astronomy inputs); TargetsPage's
// incremental reveal (growing the `targets` array in chunks) then only pays
// for the newly-revealed delta each step instead of recomputing the whole
// set, since previously-seen ids hit the cache.
export interface RowCacheEntry {
  genKey: string;
  alt: RowAltitude;
}

/**
 * Site geometry component of the gen key. `rowAltitudeFor` reads
 * lat/lon/elevation off the `ObserverSite` object directly (not just its id),
 * so editing an existing site's coordinates (same id) must still invalidate
 * `rowCache` — keying on `site.id` alone let a geometry edit serve stale
 * altitude/moon values for every already-cached target.
 */
function siteGeometryKey(site: ObserverSite | null): string {
  if (!site) return '';
  return `${site.latitudeDeg}|${site.longitudeDeg}|${site.elevationM ?? ''}`;
}

/** Generation key gating `rowCache`: changes whenever any astronomy input does. */
export function rowCacheGenKey(
  usableAltDeg: number,
  site: ObserverSite | null,
  dateMs: number,
  night: ObservingNight | null,
  guidanceParams: MoonAvoidanceParams,
): string {
  return `${usableAltDeg}|${site?.id ?? ''}|${siteGeometryKey(site)}|${dateMs}|${night?.nightKey ?? ''}|${JSON.stringify(guidanceParams)}`;
}

/**
 * Look up (or compute + cache) a target's full-catalogue-pass altitude, and
 * derive its Moon planning fresh from the pre-fetched batch geometry map
 * (#634). `includeMoonGeometry` is always `false` here — same contract as
 * the call sites this replaces.
 *
 * Only `alt` (real astronomy-engine altitude sampling) is cached by `genKey`
 * — `moon` is no longer astronomy-engine-derived here (real geometry now
 * comes from `moonGeometry`, an O(1) map lookup), so recomputing it on every
 * call is cheap and always reflects the latest fetched batch without needing
 * its own cache-invalidation key.
 */
export function getCachedRow(
  cache: Map<string, RowCacheEntry>,
  t: TargetListItem,
  genKey: string,
  usableAltDeg: number,
  site: ObserverSite | null,
  dateMs: number,
  guidanceParams: MoonAvoidanceParams,
  night: ObservingNight | null,
  moonGeometry: ReadonlyMap<string, RowMoonGeometry>,
): { alt: RowAltitude; moon: RowMoonPlanning } {
  const cached = cache.get(t.id);
  const alt =
    cached && cached.genKey === genKey
      ? cached.alt
      : rowAltitudeFor(t, usableAltDeg, site, dateMs, guidanceParams, false);
  if (!cached || cached.genKey !== genKey) {
    cache.set(t.id, { genKey, alt });
  }
  const moon = deriveRowMoonPlanning(
    t,
    night,
    guidanceParams,
    night ? (moonGeometry.get(t.id) ?? null) : null,
  );
  return { alt, moon };
}

/**
 * Group targets by the selected key, compute altitude + moon planning for each,
 * sort within groups, then order groups by their first (sorted) row.
 */
export function groupTargets(
  targets: TargetListItem[],
  sort: TargetSort,
  groupBy: TargetGroupBy,
  usableAltDeg: number,
  site: ObserverSite | null,
  night: ObservingNight | null,
  guidanceParams: MoonAvoidanceParams,
  dateMs: number,
  cache: Map<string, RowCacheEntry>,
  genKey: string,
  moonGeometry: ReadonlyMap<string, RowMoonGeometry>,
): TargetGroup[] {
  const byKey = new Map<
    string,
    Array<{ target: TargetListItem; alt: RowAltitude; moon: RowMoonPlanning }>
  >();
  for (const t of targets) {
    const key = groupHeadlineOf(t, groupBy);
    const { alt, moon } = getCachedRow(
      cache,
      t,
      genKey,
      usableAltDeg,
      site,
      dateMs,
      guidanceParams,
      night,
      moonGeometry,
    );
    const bucket = byKey.get(key);
    if (bucket) bucket.push({ target: t, alt, moon });
    else byKey.set(key, [{ target: t, alt, moon }]);
  }

  const groups: TargetGroup[] = [];
  for (const [label, rows] of byKey) {
    groups.push({
      label,
      rows: [...rows].sort((ra, rb) =>
        compareTargetRows(
          ra.target,
          ra.alt,
          ra.moon,
          rb.target,
          rb.alt,
          rb.moon,
          sort,
        ),
      ),
    });
  }

  groups.sort((ga, gb) => {
    const cmp = compareTargetRows(
      ga.rows[0].target,
      ga.rows[0].alt,
      ga.rows[0].moon,
      gb.rows[0].target,
      gb.rows[0].alt,
      gb.rows[0].moon,
      sort,
    );
    return cmp !== 0 ? cmp : compareStr(ga.label, gb.label);
  });
  return groups;
}

// ── Flattened virtual-row model ────────────────────────────────────────────────
//
// The virtualizer windows a single flat list, so groups + targets are flattened
// into a discriminated union. Group-header rows and target rows estimate to the
// same row height (keyed off --pv-row-height in CSS).

export type FlatRow =
  | {
      kind: 'group';
      key: string;
      label: string;
      count: number;
      path?: string;
      depth?: number;
      collapsible?: boolean;
      collapsed?: boolean;
    }
  | {
      kind: 'target';
      key: string;
      target: TargetListItem;
      alt: RowAltitude;
      moon: RowMoonPlanning;
      depth?: number;
    };

// ── Multi-level grouping accessors ────────────────────────────────────────────

/**
 * Build the multi-level grouping accessors for the current night + guidance
 * params (US3, FR-011): the `filters` dimension groups by the REAL derived
 * recommendation category, so it must be rebuilt whenever the night or the
 * live per-band params change rather than being a static module export.
 */
export function buildTargetAccessors(
  night: ObservingNight | null,
  guidanceParams: MoonAvoidanceParams,
  // #573 perf: reuse the moon planning already computed by the cached
  // full-catalogue pass (moonMap) instead of re-deriving it per target.
  moonLookup?: (t: TargetListItem) => RowMoonPlanning,
): Readonly<Record<string, DimensionAccessor<TargetListItem>>> {
  return {
    constellation: (t) =>
      (t as TargetListItem & { constellation?: string }).constellation ?? null,
    type: (t) => (t.objectType ? formatType(t.objectType) : null),
    catalogue: (t) => {
      const cat = catalogueOf(t);
      return cat ? catalogueLabel(cat) : m.targets_objtype_other();
    },
    // Applicable filters: group by the target's REAL derived recommendation
    // category (broadband-ok / narrowband-only / avoid-tonight / unknown).
    filters: (t) => {
      const { recommendation } = moonLookup
        ? moonLookup(t)
        : deriveRowMoonPlanning(t, night, guidanceParams);
      return recommendationLabel(recommendation);
    },
  };
}

export function flattenGroups(groups: TargetGroup[]): FlatRow[] {
  const rows: FlatRow[] = [];
  for (const group of groups) {
    rows.push({
      kind: 'group',
      key: `g:${group.label}`,
      label: group.label,
      count: group.rows.length,
    });
    for (const { target, alt, moon } of group.rows) {
      rows.push({ kind: 'target', key: target.id, target, alt, moon });
    }
  }
  return rows;
}
