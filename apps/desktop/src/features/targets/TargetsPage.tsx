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

  // search state is lifted above useTargets() so the query can be forwarded to
  // the backend (GF-11 / DS-16, lands with perf/ipc-surface #1543). Currently
  // the backend binding ignores it and client-side alias matching covers
  // this path; once #1543's bindings land the store passes it through.
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
  useStaleSelectionCleanup(
    selected,
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
