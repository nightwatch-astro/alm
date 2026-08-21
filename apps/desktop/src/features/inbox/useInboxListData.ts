// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Inbox list data, root derivation, rescan targets, and the page-level search
 * filter shared by the list and every count surface.
 */

import { useCallback, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { unwrap } from '@/api/ipc';
import { commands } from '@/bindings/index';
import { queryKeys } from '@/data/queryKeys';
import {
  mergeRescanRoots,
  useInboxList,
  useInboxRescan,
  useOpenInboxPlans,
} from './store';
import type {
  InboxListItem,
  InboxOpenPlan,
  InboxSourceGroupListItem,
} from './store';

// #557: a shared, stable empty-array fallback. `listData?.items ?? []`
// allocates a NEW array every render while the query is unresolved, which
// cascades through every `useMemo` keyed on `items` (derivedStats, roots,
// etc.) and recomputed their outputs every render too — feeding an unstable
// value into `useSetPageStatus` and re-triggering its effect indefinitely.
const EMPTY_ITEMS: InboxListItem[] = [];

/**
 * Stable empty source-group array — same rationale as {@link EMPTY_ITEMS}: a
 * fresh `[]` literal per render is a new identity every time, which would make
 * every `useMemo` downstream of it recompute forever.
 */
const EMPTY_SOURCE_GROUPS: InboxSourceGroupListItem[] = [];

export interface UseInboxListDataResult {
  items: InboxListItem[];
  sourceGroups: InboxSourceGroupListItem[];
  filteredItems: InboxListItem[];
  filteredSourceGroups: InboxSourceGroupListItem[];
  listLoading: boolean;
  /** FR-006: items are bounded at 500 by the backend; surface a cap notice. */
  isCapped: boolean;
  limit: number | undefined;
  search: string;
  setSearch: (value: string) => void;
  openPlans: InboxOpenPlan[];
  totalActions: number;
  refreshAll: () => void;
  refreshOpenPlans: () => void;
  /** Destination library roots (non-inbox) for the per-detection Source picker. */
  destRoots: Array<{ id: string; path: string; category: string }>;
  rescanLoading: boolean;
  rescan: () => Promise<unknown>;
}

export function useInboxListData(): UseInboxListDataResult {
  // FR-001 / FR-002: cross-root aggregate list replaces the hardcoded scan.
  const {
    data: listData,
    loading: listLoading,
    refresh: refreshList,
  } = useInboxList();
  const items = listData?.items ?? EMPTY_ITEMS;
  // Spec 058 FR-016 / T013: scanned folders that have produced no item rows.
  const sourceGroups = listData?.sourceGroups ?? EMPTY_SOURCE_GROUPS;

  const [search, setSearch] = useState('');

  // spec 041: aggregate open-plan surface (all ingestions at once).
  const { data: openPlansData, refresh: refreshOpenPlans } =
    useOpenInboxPlans();
  const openPlans = openPlansData?.plans ?? [];
  const totalActions = openPlansData?.totalActions ?? 0;

  // Refresh both the inbox list and the aggregate plan surface after any
  // apply/cancel/confirm mutation.
  const refreshAll = useCallback(() => {
    refreshList();
    refreshOpenPlans();
  }, [refreshList, refreshOpenPlans]);

  // All registered library roots, fetched once (roots are optional UI sugar —
  // a fetch failure just leaves both derived views empty, per the prior
  // per-effect `.catch()` no-ops) and filtered client-side into the two views
  // this page needs below (was two separate `rootsList()` fetches, one per
  // filter).
  const { data: allRoots } = useQuery({
    queryKey: queryKeys.roots.all(),
    queryFn: async () => unwrap(await commands.rootsList()),
  });

  // Registered inbox-category roots (FR-005): rescan must reach every active
  // registered root, not just ones with existing items — a freshly registered
  // root has zero items until its first scan, so deriving targets from
  // `items` alone made "Rescan all roots" silently skip it.
  const registeredInboxRoots = useMemo(
    () =>
      (allRoots ?? [])
        .filter((r) => r.category === 'inbox' && r.active)
        .map((r) => ({ rootId: r.id, rootAbsolutePath: r.path })),
    [allRoots],
  );

  // Union of registered inbox roots and any root already surfaced via items
  // (covers a root whose registration briefly lags an in-flight scan).
  const roots = useMemo(
    () =>
      mergeRescanRoots(
        registeredInboxRoots,
        items.map((item) => ({
          rootId: item.rootId,
          rootAbsolutePath: item.rootAbsolutePath,
        })),
      ),
    [items, registeredInboxRoots],
  );

  const onRescanComplete = useCallback(() => refreshAll(), [refreshAll]);
  const { loading: rescanLoading, rescan } = useInboxRescan(
    roots,
    onRescanComplete,
  );

  // Destination library roots (non-inbox) for the per-detection "Source" picker.
  // When more than one exists, the user can choose where files land instead of
  // relying on backend auto-selection. "" = auto.
  const destRoots = useMemo(
    () =>
      (allRoots ?? [])
        .filter((r) => r.category !== 'inbox')
        .map((r) => ({ id: r.id, path: r.path, category: r.category })),
    [allRoots],
  );

  // Client-side text search across the relative path (the list's primary key).
  const filteredItems = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return items;
    return items.filter(
      (it) =>
        it.relativePath.toLowerCase().includes(q) ||
        (it.groupTarget ?? '').toLowerCase().includes(q),
    );
  }, [items, search]);

  // Source groups run through the SAME page-level filter as `filteredItems`
  // (search only — the lane and kind filters are applied inside `InboxList`),
  // so the two arrays are always filtered to the same degree. A source group
  // has no `groupTarget` to match on; its relative path is all there is.
  const filteredSourceGroups = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return sourceGroups;
    return sourceGroups.filter((g) => g.relativePath.toLowerCase().includes(q));
  }, [sourceGroups, search]);

  return {
    items,
    sourceGroups,
    filteredItems,
    filteredSourceGroups,
    listLoading,
    isCapped: listData?.capped ?? false,
    limit: listData?.limit,
    search,
    setSearch,
    openPlans,
    totalActions,
    refreshAll,
    refreshOpenPlans,
    destRoots,
    rescanLoading,
    rescan,
  };
}
