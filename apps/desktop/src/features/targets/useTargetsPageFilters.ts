// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useTargetsPageFilters — filter/search state for TargetsPage (spec 043/044/047).
 *
 * Extracted from TargetsPage.tsx (refactor sweep kyo7.104) so filter logic
 * can be reasoned about independently of rendering. Owns: catalogue selection,
 * My Targets toggle, search query, recommendation filter, sort, progressive
 * reveal, and the derived visible target list.
 *
 * Progressive reveal note (#573): the full catalogue loads at once from IPC
 * and TargetsTable's per-row astronomy was synchronous on first render. The
 * virtualizer in TargetsTable/useTargetsTableRows now windows rows, so the
 * reveal logic only controls which rows ARE visible (and thus computed) during
 * the brief initial-load window — astronomy for unrevealed rows is not
 * precomputed.
 */

import { useState, useEffect, useCallback, useMemo } from 'react';
import type { TargetListItem } from '@/bindings/index';
import { filterByCatalogues, type CatalogueId } from './planner-catalog';
import {
  DEFAULT_ENABLED_CATALOGUES,
  loadDefaultCatalogues,
} from './catalogue-settings';
import { DEFAULT_TARGET_SORT } from './TargetsTable';
import type { TargetSort, TargetSortCol } from './TargetsTable';
import { useFavourites } from './useFavourites';
import { deriveRowMoonPlanning } from './astro/row-planning';
import { matchesSearch } from './target-search-helpers';
import type { ObservingNight } from './astro/moon-state';
import type {
  MoonAvoidanceParams,
  Recommendation,
} from './astro/moon-avoidance';

// ── Constants ─────────────────────────────────────────────────────────────────

/** "My Targets" filter value (#91): '' = all, 'my' = starred targets. */
export const MY_TARGETS_VALUE = 'my';

/** Sentinel: stub empty result for My Targets with no favourites. */
const MY_TARGETS_EMPTY: TargetListItem[] = [];

/**
 * Progressive-reveal chunk size (#573): limits the rows fed to TargetsTable
 * on the first render, growing in the background via setTimeout so the page
 * is interactive immediately.
 */
const REVEAL_CHUNK = 300;

// ── Hook ──────────────────────────────────────────────────────────────────────

type ListState =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'loaded'; items: TargetListItem[] };

export interface TargetsPageFilters {
  myTargetsFilter: string;
  setMyTargetsFilter: (v: string) => void;
  search: string;
  setSearch: (v: string) => void;
  sort: TargetSort;
  handleSort: (col: TargetSortCol) => void;
  enabledCatalogues: CatalogueId[];
  setEnabledCatalogues: (ids: CatalogueId[]) => void;
  filterRecommendations: Recommendation[];
  setFilterRecommendations: (v: Recommendation[]) => void;
  /** The full catalogue (pre-reveal cap), filtered by catalogue selection. */
  plannerTargets: TargetListItem[];
  /** The currently visible targets after all filters + progressive reveal. */
  visibleTargets: TargetListItem[];
  favouriteIds: ReadonlySet<string>;
  toggleFavourite: (id: string) => void;
  isMyTargets: boolean;
}

export function useTargetsPageFilters(
  listState: ListState,
  night: ObservingNight | null,
  guidanceParams: MoonAvoidanceParams,
  // search/setSearch are lifted to the TargetsPage call site so the value is
  // available before useTargets() is called (needed to forward the query to
  // the backend on perf/ipc-surface landing). The hook still owns all other
  // filter state and returns search/setSearch in its result for the JSX bindings.
  search: string,
  setSearch: (v: string) => void,
): TargetsPageFilters {
  const [myTargetsFilter, setMyTargetsFilter] = useState('');
  const [sort, setSort] = useState<TargetSort>(DEFAULT_TARGET_SORT);
  const [enabledCatalogues, setEnabledCatalogues] = useState<CatalogueId[]>(
    () => [...DEFAULT_ENABLED_CATALOGUES],
  );
  const [filterRecommendations, setFilterRecommendations] = useState<
    Recommendation[]
  >([]);
  const [revealCount, setRevealCount] = useState(REVEAL_CHUNK);

  const { favouriteIds, toggle: toggleFavourite } = useFavourites();

  useEffect(() => {
    let cancelled = false;
    loadDefaultCatalogues()
      .then((ids) => {
        if (!cancelled) setEnabledCatalogues(ids);
      })
      .catch(() => {
        // Keep the in-code default subset.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (listState.status === 'loading') setRevealCount(REVEAL_CHUNK);
  }, [listState.status]);

  useEffect(() => {
    if (listState.status !== 'loaded') return;
    const total = listState.items.length;
    if (revealCount >= total) return;
    const handle = setTimeout(() => {
      setRevealCount((n) => Math.min(n + REVEAL_CHUNK, total));
    }, 0);
    return () => clearTimeout(handle);
  }, [listState, revealCount]);

  const handleSort = useCallback((col: TargetSortCol) => {
    setSort((prev) =>
      prev.col === col
        ? { col, dir: prev.dir === 'asc' ? 'desc' : 'asc' }
        : { col, dir: 'asc' },
    );
  }, []);

  // STUB: client-side catalog filter. Replace with a backend catalog filter on
  // the list endpoint (task #57) once `target.list` can filter server-side.
  const plannerTargets = useMemo(
    () =>
      listState.status === 'loaded'
        ? filterByCatalogues(listState.items, new Set(enabledCatalogues))
        : [],
    [listState, enabledCatalogues],
  );

  // #573: the "All targets" view is what the progressive reveal caps.
  const revealedPlannerTargets = useMemo(
    () => plannerTargets.slice(0, revealCount),
    [plannerTargets, revealCount],
  );

  // task #18: "My Targets" uses the FULL plannerTargets (not the reveal slice)
  // because favourites are a small set and capping them would make a starred
  // target transiently vanish from "My Targets" until reveal catches up.
  const tabTargets = useMemo(() => {
    if (myTargetsFilter !== MY_TARGETS_VALUE) return revealedPlannerTargets;
    if (favouriteIds.size === 0) return MY_TARGETS_EMPTY;
    return plannerTargets.filter((t) => favouriteIds.has(t.id));
  }, [myTargetsFilter, revealedPlannerTargets, plannerTargets, favouriteIds]);

  const visibleTargets = useMemo(() => {
    const q = search.trim();
    // #919: search must find any matching target immediately, even one not
    // yet revealed by the progressive-reveal loader — same carve-out
    // "My Targets" already gets.
    const searchBase = q
      ? myTargetsFilter === MY_TARGETS_VALUE
        ? tabTargets
        : plannerTargets
      : tabTargets;
    let result = q ? searchBase.filter((t) => matchesSearch(t, q)) : tabTargets;

    // Filter-by-recommendation (spec 047 US3, FR-011): keep only targets whose
    // REAL derived recommendation is one of the selected categories.
    if (filterRecommendations.length > 0) {
      const sel = new Set(filterRecommendations);
      result = result.filter((t) => {
        const { recommendation } = deriveRowMoonPlanning(
          t,
          night,
          guidanceParams,
        );
        return sel.has(recommendation);
      });
    }

    return result;
  }, [
    tabTargets,
    plannerTargets,
    myTargetsFilter,
    search,
    filterRecommendations,
    night,
    guidanceParams,
  ]);

  const isMyTargets = myTargetsFilter === MY_TARGETS_VALUE;

  return {
    myTargetsFilter,
    setMyTargetsFilter,
    search,
    setSearch,
    sort,
    handleSort,
    enabledCatalogues,
    setEnabledCatalogues,
    filterRecommendations,
    setFilterRecommendations,
    plannerTargets,
    visibleTargets,
    favouriteIds,
    toggleFavourite,
    isMyTargets,
  };
}
