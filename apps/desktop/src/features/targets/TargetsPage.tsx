// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TargetsPage — spec 043 shared list-page adoption (task #73), spec 044 mock
 * columns + filter-by-filter + altitude threshold setting.
 *
 * Standardized on the Sessions layout system: a pinned `PageTopBar` (title +
 * summary counts + `FilterToolbar` + right-aligned actions) over a
 * `ListPageLayout` body — a dense FULL-WIDTH sortable `TargetsTable` as primary
 * content and the existing planner detail (`TargetDetailV2`) in the right-side
 * detail pane that mounts on selection.
 *
 * Filter/search/sort state lives in `useTargetsPageFilters` (refactor sweep
 * kyo7.104); this file owns the layout, navigation, and page-level side effects.
 *
 * #103b: Text search is whitespace/case-insensitive across the designation and
 * label — "M31", "M 31", and "m31" all resolve to the same target (the
 * normalizer collapses internal whitespace).
 *
 * Spec 044:
 *   - Filter-by-filter: a "Filters" multi-select in the top bar (spec 047
 *     US3, FR-011 — now filter-by-recommendation, using REAL derived categories).
 *   - Usable altitude threshold: loaded from localStorage via `useAltitudeThreshold`.
 */

import { useEffect, useCallback, useMemo, useState } from 'react';
import { useNavigate, useSearch } from '@tanstack/react-router';
import type { TargetListItem } from '@/bindings/index';
import { PageTopBar, FilterToolbar, ListPageLayout } from '@/components';
import type { FilterOption } from '@/components';
import { m } from '@/lib/i18n';
import { Btn, EmptyState } from '@/ui';
import { useGrouping } from '@/lib/use-grouping';
import { useStaleSelectionCleanup } from '@/lib/use-stale-selection';
import { AddTargetDialog } from './AddTargetDialog';
import { TargetDetailV2 } from './TargetDetailV2';
import { useTargets } from './store';
import { PLANNER_CATALOGS, type CatalogueId } from './planner-catalog';
import { TargetsTable } from './TargetsTable';
import { useAltitudeThreshold } from './altitude-settings';
import { useObservingNight } from './astro/observing-night';
import { computeObservingNight, type ObservingNight } from './astro/moon-state';
import { useObserverSiteExists } from './site-gate';
import { PlannerDatePicker } from './PlannerDatePicker';
import { PlannerComputedFor } from './PlannerComputedFor';
import { useGuidanceParams, loadGuidanceParams } from './guidance-settings';
import { usePlannerSensorConfig } from './planner-sensor';
import { recommendationLabel } from './FilterBadges';
import type { Recommendation } from './astro/moon-avoidance';
import {
  useTargetsPageFilters,
  MY_TARGETS_VALUE,
} from './useTargetsPageFilters';

// Re-export search helpers so target-search.test.ts import paths stay stable.
export { normalizeDesig, matchesSearch } from './target-search-helpers';

type ListState =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'loaded'; items: TargetListItem[] };

/** Multi-level grouping dimensions for the Planner top bar. */
// Render-time factory (spec 046 #8b) so labels re-read the active locale.
const TARGETS_DIMENSIONS = (): FilterOption[] => [
  { value: 'catalogue', label: m.cmp_target_search_catalogue_label() },
  { value: 'type', label: m.targets_groupby_object_type() },
  { value: 'constellation', label: m.targets_dim_constellation() },
  { value: 'filters', label: m.targets_dim_applicable_filters() },
];

/** Catalogue multi-select options, in canonical display order. */
const CATALOGUE_OPTIONS: FilterOption[] = PLANNER_CATALOGS.map((c) => ({
  value: c.id,
  label: c.label(),
}));

/**
 * Normalize a designation or label for alias-aware matching (#103b).
 *
 * Collapses internal whitespace so "M31" and "M 31" become identical tokens
 * ("m31"). Case is folded to lower. This means "M31", "M 31", and "m 31" all
 * normalize to "m31" and match each other — the key astrophotography UX need
 * where catalog designations appear both spaced ("M 31") and compact ("M31").
 */
export function normalizeDesig(s: string): string {
  return s.toLowerCase().replace(/\s+/g, '');
}

/**
 * Designation- and label-aware match for the CommandPalette client-side
 * filter.  Alias matching moved to backend via `target.list(search)` (GF-11).
 */
export function matchesSearch(t: TargetListItem, query: string): boolean {
  const qNorm = normalizeDesig(query);
  const qLower = query.toLowerCase();
  if (normalizeDesig(t.primaryDesignation).includes(qNorm)) return true;
  if (normalizeDesig(t.effectiveLabel).includes(qNorm)) return true;
  // Plain lowercase substring on effectiveLabel for proper names
  // ("andromeda" in "Andromeda Galaxy") without whitespace collapsing.
  if (t.effectiveLabel.toLowerCase().includes(qLower)) return true;
  return false;
}

/** My Targets filter options for the FilterToolbar single-select (#91). */
// Render-time factory (spec 046 #8b) so the label re-reads the active locale.
const MY_TARGETS_FILTER_OPTIONS = (): FilterOption[] => [
  { value: MY_TARGETS_VALUE, label: m.nav_my_targets() },
];

/**
 * Filter-by-recommendation options (spec 047 US3, FR-011).
 */
// Render-time factory (spec 046 #8b) so labels re-read the active locale.
const RECOMMENDATION_FILTER_OPTIONS = (): FilterOption[] => [
  { value: 'broadband-ok', label: recommendationLabel('broadband-ok') },
  { value: 'narrowband-only', label: recommendationLabel('narrowband-only') },
  { value: 'avoid-tonight', label: recommendationLabel('avoid-tonight') },
  { value: 'unknown', label: recommendationLabel('unknown') },
];

export function TargetsPage() {
  const { selected } = useSearch({ from: '/shell/targets' });
  const navigate = useNavigate({ from: '/targets' });
  const [addOpen, setAddOpen] = useState(false);
  const { dims, setSlot } = useGrouping({
    storageKey: 'targets.grouping.dims.v1',
    validIds: ['catalogue', 'type', 'constellation', 'filters'],
    defaultDims: [],
  });
  /**
   * User-configured usable-altitude threshold from Settings → Target Planner.
   */
  const usableAltDeg = useAltitudeThreshold();

  /**
   * Site gate (spec 047 D7): the planner renders no astronomy until a default
   * observing site exists.
   */
  const siteExists = useObserverSiteExists();
  /**
   * The observing-night anchor, re-checked on focus / hourly (spec 047 D1).
   */
  const nightAnchor = useObservingNight();
  const night = useMemo<ObservingNight | null>(
    () => (siteExists ? computeObservingNight(nightAnchor) : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [nightAnchor.nightKey, siteExists],
  );
  /**
   * Live per-band Moon-avoidance params (spec 047 US3, Settings → Target
   * Planner).
   */
  const guidanceParams = useGuidanceParams();
  const sensorConfig = usePlannerSensorConfig();

  // spec 047 T017: hydrate the live per-band Moon-avoidance params cache from
  // the backend on mount.
  useEffect(() => {
    void loadGuidanceParams();
  }, []);

  // search state is lifted above useTargets() so the query is forwarded to the
  // backend target.list endpoint (GF-11 / DS-16) for server-side alias-aware
  // filtering.
  const [search, setSearch] = useState('');

  const targetsQuery = useTargets(search);
  const load = targetsQuery.refetch;
  const listState: ListState = targetsQuery.error
    ? { status: 'error', message: m.targets_page_error_load() }
    : targetsQuery.loading || !targetsQuery.data
      ? { status: 'loading' }
      : { status: 'loaded', items: targetsQuery.data };

  const {
    myTargetsFilter,
    setMyTargetsFilter,
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
  } = useTargetsPageFilters(
    listState,
    night,
    guidanceParams,
    search,
    setSearch,
  );

  const onSelect = (id: string) =>
    navigate({ search: (prev) => ({ ...prev, selected: id }) });

  const clearSelection = useCallback(
    () =>
      navigate({
        search: (prev) => ({ ...prev, selected: undefined }),
        replace: true,
      }),
    [navigate],
  );

  // #735: stale-id cleanup — matched against the FULL query data rather than
  // the progressively-revealed slice.
  //
  // #1584: also treat any in-flight fetch as "not yet settled". The list uses
  // `keepPreviousData`, so during the post-"Add target" refetch the status is
  // still 'loaded' with the OLD rows — the just-added id is absent until the
  // refetch lands. Without this guard the cleanup would clear the `?selected=`
  // the add flow just navigated to, hanging the add-target E2E journey.
  useStaleSelectionCleanup(
    selected,
    targetsQuery.fetching ||
      listState.status !== 'loaded' ||
      listState.items.some((t) => t.id === selected),
    clearSelection,
  );

  const handleAdded = useCallback(
    (targetId: string) => {
      load();
      void navigate({ search: (prev) => ({ ...prev, selected: targetId }) });
    },
    [load, navigate],
  );

  const handleSort = useCallback((col: TargetSortCol) => {
    setSort((prev) =>
      prev.col === col
        ? { col, dir: prev.dir === 'asc' ? 'desc' : 'asc' }
        : { col, dir: 'asc' },
    );
  }, []);

  // STUB: client-side catalog filter. Replace with a backend catalog filter on
  // the list endpoint (task #57) once `target.list` can filter server-side. The
  // catalogue multi-select (task #82) restricts to the user-selected subset.
  const plannerTargets = useMemo(
    () =>
      listState.status === 'loaded'
        ? filterByCatalogues(listState.items, new Set(enabledCatalogues))
        : [],
    [listState, enabledCatalogues],
  );

  /**
   * #573: the "All targets" view is what the progressive reveal caps — a
   * prefix slice of the full filtered catalogue, growing via `revealCount`.
   * Order is the raw fetch order (not the user's chosen sort — TargetsTable
   * sorts whatever it's given), so this only affects which rows exist during
   * the brief initial-load window, not final correctness once fully loaded.
   */
  const revealedPlannerTargets = useMemo(
    () => plannerTargets.slice(0, revealCount),
    [plannerTargets, revealCount],
  );

  /**
   * task #18: when "My Targets" is active, filter the Planner catalog to only
   * the targets the user has starred (stored client-side via useFavourites).
   * STUB: favouriteIds comes from localStorage only until task #54 lands and
   * provides real backend "has linked sessions/projects" data.
   *
   * Uses the FULL `plannerTargets` (not the progressive-reveal slice, #573):
   * favourites are a small set regardless of catalogue size, so there's no
   * perf reason to cap it, and capping it would make a starred target
   * transiently vanish from "My Targets" until reveal catches up to it.
   */
  const tabTargets = useMemo(() => {
    if (myTargetsFilter !== MY_TARGETS_VALUE) return revealedPlannerTargets;
    if (favouriteIds.size === 0) return MY_TARGETS_EMPTY;
    return plannerTargets.filter((t) => favouriteIds.has(t.id));
  }, [myTargetsFilter, revealedPlannerTargets, plannerTargets, favouriteIds]);

  const visibleTargets = useMemo(() => {
    // Backend filters by search (alias-aware, GF-11 / DS-16) — the returned
    // list is already scoped to the query, so no client-side alias filter
    // needed.  My Targets still bypasses the backend filter and uses the
    // local favourite set.
    let result = tabTargets;

    // Filter-by-recommendation (spec 047 US3, FR-011): keep only targets whose
    // REAL derived recommendation is one of the selected categories.
    // deriveRowMoonPlanning is O(1) per target (pure, no fetch); night/params
    // are included in deps so a settings change or the nightly Moon state
    // re-filters too.
    if (filterRecommendations.length > 0) {
      const selected = new Set(filterRecommendations);
      result = result.filter((t) => {
        const { recommendation } = deriveRowMoonPlanning(
          t,
          night,
          guidanceParams,
        );
        return selected.has(recommendation);
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

  // Per the top-bar convention (task #80/#91): no title/summary in the bar —
  // the left nav names the page and per-page counts move to the status bar.
  // The My Targets filter, search, catalogue multi-select, and group-by ALL
  // remain visible on both the "All targets" and "My Targets" views so the
  // bar is consistent regardless of which view is active (#91 correction:
  // the bar must not collapse on tab switch).
  const isMyTargets = myTargetsFilter === MY_TARGETS_VALUE;

  const topBar = (
    <PageTopBar
      filters={
        <FilterToolbar
          search={{
            value: search,
            onChange: setSearch,
            placeholder: m.targets_page_search_placeholder(),
            ariaLabel: m.targets_page_search_aria(),
          }}
          fields={[
            {
              key: 'myTargets',
              label: m.targets_filter_show_label(),
              value: myTargetsFilter,
              options: MY_TARGETS_FILTER_OPTIONS(),
              onChange: setMyTargetsFilter,
              allLabel: m.targets_page_filter_all_targets(),
            },
          ]}
          multiFields={[
            {
              key: 'catalogues',
              label: m.targets_filter_catalogues_label(),
              value: enabledCatalogues,
              options: CATALOGUE_OPTIONS,
              onChange: (v) => setEnabledCatalogues(v as CatalogueId[]),
            },
            {
              key: 'filterRecommendations',
              label: m.common_filters(),
              value: filterRecommendations,
              options: RECOMMENDATION_FILTER_OPTIONS(),
              onChange: (v) => setFilterRecommendations(v as Recommendation[]),
            },
          ]}
          grouping={{
            dimensions: TARGETS_DIMENSIONS(),
            dims,
            setSlot,
          }}
        />
      }
      actions={
        <>
          <PlannerComputedFor usableAltDeg={usableAltDeg} />
          <PlannerDatePicker />
          <Btn
            size="sm"
            variant="primary"
            data-guide-anchor="targets.resolve-cta"
            onClick={() => setAddOpen(true)}
          >
            {m.targets_add_target()}
          </Btn>
        </>
      }
    />
  );

  return (
    <div className="pv-targets-page" data-testid="targets-page">
      <AddTargetDialog
        open={addOpen}
        onClose={() => setAddOpen(false)}
        onAdded={handleAdded}
      />
      <ListPageLayout
        topBar={topBar}
        detail={
          selected ? (
            <TargetDetailV2
              targetId={selected}
              item={plannerTargets.find((t) => t.id === selected) ?? null}
              usableAltDeg={usableAltDeg}
              night={night}
              sensorConfig={sensorConfig}
              onMutated={load}
            />
          ) : undefined
        }
        onCloseDetail={selected ? clearSelection : undefined}
        detailLabel={m.targets_detail_label()}
        dockId="targets"
      >
        {listState.status === 'error' ? (
          <EmptyState
            title={m.settings_advanced_log_error()}
            desc={listState.message}
          />
        ) : (
          <TargetsTable
            targets={visibleTargets}
            selected={selected ?? null}
            onSelect={onSelect}
            loading={listState.status === 'loading'}
            sort={sort}
            onSort={handleSort}
            dims={dims}
            usableAltDeg={usableAltDeg}
            night={night}
            guidanceParams={guidanceParams}
            sensorConfig={sensorConfig}
            favouriteIds={favouriteIds}
            onToggleFavourite={toggleFavourite}
            emptyMessage={
              isMyTargets
                ? favouriteIds.size === 0
                  ? m.targets_page_my_targets_no_favs()
                  : m.targets_page_my_targets_no_match()
                : m.targets_page_no_match()
            }
          />
        )}
      </ListPageLayout>
    </div>
  );
}
