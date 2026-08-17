// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * `useTargetsTableRows` — data derivation + windowing for `TargetsTable`
 * (refactor sweep #976): the #634 batched Moon/opposition geometry fetch,
 * the #573 per-target-id astronomy cache, single-tier/multi-level grouping,
 * flattening into the virtual-row model, and the `@tanstack/react-virtual`
 * windowing (padding-spacer pattern — see `TargetsTable`'s module doc for
 * why). Split out of the component so the render stays render-only.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type { TargetListItem } from '@/bindings/index';
import { rowAltitudeFor } from './planner-altitude';
import type { ObservingNight } from './astro/moon-state';
import {
  UNKNOWN_ROW_PLANNING,
  type RowMoonGeometry,
} from './astro/row-planning';
import type { MoonAvoidanceParams } from '@/shared/planner/moon-avoidance';
import type { ObserverSite } from '@/shared/observing-sites/observer-site';
import {
  buildTargetAccessors,
  flattenGroups,
  getCachedRow,
  groupTargets,
  rowCacheGenKey,
  compareTargetRows,
  type FlatRow,
  type RowCacheEntry,
  type TargetGroupBy,
  type TargetSort,
} from './table-model';
import { groupByDimensions, flattenVisibleGroups } from '@/lib/grouping';

export function useTargetsTableRows({
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
  overscan,
}: {
  targets: TargetListItem[];
  sort: TargetSort;
  groupBy?: TargetGroupBy;
  dims?: string[];
  usableAltDeg: number;
  site: ObserverSite | null;
  night: ObservingNight | null;
  guidanceParams: MoonAvoidanceParams;
  dateMs: number;
  collapsed: ReadonlySet<string>;
  rowEstimate: number;
  overscan: number;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // #573 perf: per-target-id cache for the full-catalogue astronomy pass
  // below, persisted across renders for this component instance (a ref, not
  // state — it never needs to trigger a re-render itself). See
  // `getCachedRow`/`rowCacheGenKey`'s doc in `table-model.ts`.
  const rowCacheRef = useRef<Map<string, RowCacheEntry>>(new Map());
  const genKey = rowCacheGenKey(
    usableAltDeg,
    site,
    dateMs,
    night,
    guidanceParams,
  );

  // #634: real lunar-separation + opposition geometry now comes from ONE
  // batched Rust call per newly-seen target id (never per-row round trips),
  // replacing the TS `astronomy-engine` math this table used to run at
  // catalogue scale. A ref cache (mirrors the #573 `rowCacheRef` pattern)
  // means TargetsPage's incremental reveal only fetches the NEW tail of
  // ids each chunk, not the whole revealed set again; the night's `nightKey`
  // gates invalidation (the Moon's position is night-specific). A version
  // counter (not the cache map itself) triggers the re-render that picks up
  // newly-arrived entries — mutating a ref never does.
  const moonGeometryCacheRef = useRef<Map<string, RowMoonGeometry>>(new Map());
  const moonGeometryNightRef = useRef<string | null>(null);
  const [moonGeometryVersion, setMoonGeometryVersion] = useState(0);

  useEffect(() => {
    if (!night) return;
    if (moonGeometryNightRef.current !== night.nightKey) {
      moonGeometryCacheRef.current = new Map();
      moonGeometryNightRef.current = night.nightKey;
    }
    const cache = moonGeometryCacheRef.current;
    const missing = targets.filter(
      (t) =>
        typeof t.raDeg === 'number' &&
        typeof t.decDeg === 'number' &&
        !cache.has(t.id),
    );
    if (missing.length === 0) return;

    let cancelled = false;
    commands
      .targetMoonOppositionBatch({
        targets: missing.map((t) => ({
          id: t.id,
          raDeg: t.raDeg as number,
          decDeg: t.decDeg as number,
        })),
        at: night.midnight.toISOString(),
      })
      .then(unwrap)
      .then(({ results }) => {
        if (cancelled) return;
        for (const r of results) {
          cache.set(r.id, {
            lunarSeparationDeg: r.moonSeparationDeg,
            nextOppositionDate: r.opposition
              ? r.opposition.date.slice(0, 10)
              : null,
            daysToOpposition: r.opposition ? r.opposition.daysUntil : null,
          });
        }
        setMoonGeometryVersion((v) => v + 1);
      })
      .catch(() => {
        // Leave the missing ids absent from the cache — those rows render
        // the existing explicit-unknown "—" state (never a fabricated
        // value); a later target-set/night change retries the fetch.
      });
    return () => {
      cancelled = true;
    };
  }, [targets, night]);

  // Grouping + sorting + per-row altitude are all derived here so a filter
  // or sort change does one O(n) pass off the render hot path, not per-row work
  // inside the virtualized render loop. usableAltDeg + site are included in the
  // dep array so that changing the altitude threshold or the active site
  // re-derives all rows.
  //
  // When `dims` is non-empty we use the shared multi-level groupByDimensions
  // engine (with collapsible headers); when empty we fall back to the
  // legacy single-tier groupTargets (using `groupBy`).
  const useMultiGroup = dims != null && dims.length > 0;

  const flatRows = useMemo(() => {
    // `dims` re-checked here (not just via useMultiGroup) so TS narrows it to
    // `string[]` for the rest of this branch — narrowing a destructured param
    // doesn't cross the useMemo closure boundary even though useMultiGroup is
    // derived from the same null check.
    if (useMultiGroup && dims) {
      // Pre-compute altitude + moon planning for all items (needed for sort +
      // display), reusing the per-id cache (#573) — see getCachedRow's doc.
      const withAlt = targets.map((t) => {
        const { alt, moon } = getCachedRow(
          rowCacheRef.current,
          t,
          genKey,
          usableAltDeg,
          site,
          dateMs,
          guidanceParams,
          night,
          moonGeometryCacheRef.current,
        );
        return { target: t, alt, moon };
      });
      // Sort the flat list first.
      const sortedWithAlt = [...withAlt].sort((a, b) =>
        compareTargetRows(
          a.target,
          a.alt,
          a.moon,
          b.target,
          b.alt,
          b.moon,
          sort,
        ),
      );
      const sorted = sortedWithAlt.map((r) => r.target);
      const altMap = new Map(sortedWithAlt.map((r) => [r.target.id, r.alt]));
      const moonMap = new Map(sortedWithAlt.map((r) => [r.target.id, r.moon]));

      // Build the group tree using shared engine. moonMap reuse (#573):
      // avoids re-deriving moon planning already computed above.
      const tree = groupByDimensions(
        sorted,
        dims,
        buildTargetAccessors(
          night,
          guidanceParams,
          (t) => moonMap.get(t.id) ?? UNKNOWN_ROW_PLANNING,
        ),
      );
      // Flatten with collapse state, then map to our FlatRow shape.
      const visRows = flattenVisibleGroups(tree, collapsed);
      return visRows.map((vrow): FlatRow => {
        if (vrow.kind === 'header') {
          return {
            kind: 'group',
            key: vrow.path,
            label: vrow.node.label,
            count: vrow.node.count,
            path: vrow.path,
            depth: vrow.depth,
            collapsible: true,
            collapsed: vrow.collapsed,
          };
        }
        const t = vrow.item;
        return {
          kind: 'target',
          key: t.id,
          target: t,
          alt:
            altMap.get(t.id) ??
            rowAltitudeFor(
              t,
              usableAltDeg,
              site,
              dateMs,
              guidanceParams,
              false,
            ),
          moon: moonMap.get(t.id) ?? UNKNOWN_ROW_PLANNING,
          depth: vrow.depth,
        };
      });
    }
    // Single-tier legacy grouping ONLY if a caller explicitly asks for it.
    if (groupBy) {
      const groups = groupTargets(
        targets,
        sort,
        groupBy,
        usableAltDeg,
        site,
        night,
        guidanceParams,
        dateMs,
        rowCacheRef.current,
        genKey,
        moonGeometryCacheRef.current,
      );
      return flattenGroups(groups);
    }
    // Default: no grouping selected → FLAT sorted list (no group headers).
    // Full-catalogue pass (potentially ~13k rows pre-filter/pre-windowing)
    // needed only for sort, reusing the per-id cache (#573) — the real Moon
    // time-series is still computed per visible row at render time instead
    // (a real Layer-2 CI perf regression otherwise: see rowAltitudeFor's doc).
    const withAlt = targets.map((t) => {
      const { alt, moon } = getCachedRow(
        rowCacheRef.current,
        t,
        genKey,
        usableAltDeg,
        site,
        dateMs,
        guidanceParams,
        night,
        moonGeometryCacheRef.current,
      );
      return { target: t, alt, moon };
    });
    const sortedWithAlt = [...withAlt].sort((a, b) =>
      compareTargetRows(a.target, a.alt, a.moon, b.target, b.alt, b.moon, sort),
    );
    return sortedWithAlt.map(
      (r): FlatRow => ({
        kind: 'target',
        key: r.target.id,
        target: r.target,
        alt: r.alt,
        moon: r.moon,
        depth: 0,
      }),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps -- useMultiGroup is derived from dims (already listed); listing it too would be redundant
  }, [
    targets,
    sort,
    groupBy,
    usableAltDeg,
    site,
    night,
    guidanceParams,
    dateMs,
    useMultiGroup,
    dims,
    collapsed,
    genKey,
    moonGeometryVersion,
  ]);

  const virtualizer = useVirtualizer({
    count: flatRows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowEstimate,
    overscan,
  });

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();

  // Windowing fallback: in a non-layout environment (jsdom under vitest) the
  // scroll element has no measurable height, so the virtualizer yields zero
  // items. Render every row without spacers in that case — tests rely on all
  // rows being present; windowing is purely a runtime perf optimization.
  const useWindowing = virtualItems.length > 0;

  // Indices of the visible slice. In non-windowed mode we render everything.
  const renderIndices: number[] = useWindowing
    ? virtualItems.map((vi) => vi.index)
    : flatRows.map((_, i) => i);

  // Padding-spacer pattern for real CSS <table> (see module header):
  // two sentinel spacer rows bracket the visible slice. Their combined height
  // equals totalSize so the scrollbar reflects the full virtual list length,
  // but the rendered data rows are in normal table flow at their natural height.
  const paddingBefore =
    useWindowing && virtualItems.length > 0 ? virtualItems[0].start : 0;
  const paddingAfter =
    useWindowing && virtualItems.length > 0
      ? totalSize - (virtualItems[virtualItems.length - 1]?.end ?? 0)
      : 0;

  return {
    scrollRef,
    flatRows,
    renderIndices,
    paddingBefore,
    paddingAfter,
  };
}
