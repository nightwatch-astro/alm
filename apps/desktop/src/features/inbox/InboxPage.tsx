// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * InboxPage — classify / confirm workflow on the shared list-page layout.
 *
 * The Inbox once composed its own bespoke 3-zone body (list + fixed side panel
 * + docked bottom plan panel) and deliberately avoided `ListPageLayout`. It no
 * longer does. It renders through the shared `ListPageLayout` like every other
 * list page, passing no `detailPlacement`, so it takes the `'adaptive'` default
 * (spec 054 / #936): the detail docks to the SIDE on a wide window and to the
 * BOTTOM on a narrow one, and the user can pin that per page via the
 * Auto/Bottom/Right control. There is no Inbox-specific placement — the
 * permanent narrow split once designed for it (spec 054 FR-014/FR-015) was
 * never built and was withdrawn in #1068.
 *
 *   - `detail` is the selected detection's `InboxDetail`: classification +
 *     breakdown + per-file metadata. Its body is its own scroll region, so a
 *     long FILES list is reachable rather than clipped (PR #939, fixes #553).
 *   - `children` is the `InboxList` detection table.
 *   - The aggregate `PlanPanel` is NOT a docked zone any more. It lives in the
 *     plan-approval overlay below, opened from a top-bar trigger. #75:
 *     per-group summaries collapse per-file rows and aggregate by ACTUAL frame
 *     type from the item's breakdown.
 *   - #83: ONE search only (top-bar FilterToolbar). The list no longer wraps in
 *     ListSidebar (which carried a 2nd search box + a 3rd folder/master count).
 *     The triplicate counts collapse to a single compact per-frame-type
 *     breakdown in the top-bar summary; global totals live in the status bar.
 *
 * spec 039: the list is a cross-root aggregate of all unacknowledged items
 * (inbox.list), grouped/labelled by their registered root.
 */

import { useNavigate, useSearch } from '@tanstack/react-router';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { queryKeys } from '@/data/queryKeys';
import type { InboxReclassifyV2Response_Serialize as InboxReclassifyV2Response } from '@/bindings/index';
import { useSetPageStatus } from '@/app/PageStatusContext';
import { FilterToolbar, ListPageLayout, PageTopBar } from '@/components';
import {
  pickReclassifyTarget,
  resolveClassifiedGroupSelection,
  resolveReclassifyHandoff,
} from './inboxSelectionModel';
import { useInboxConfirmFlow } from './useInboxConfirmFlow';
import { useInboxPlanApplyFlow } from './useInboxPlanApplyFlow';
import { m } from '@/lib/i18n';
import type { FrameType } from '@/lib/route-contract';
import { useGrouping } from '@/lib/use-grouping';
import { useStaleSelectionCleanup } from '@/lib/use-stale-selection';
import { addToast } from '@/shared/toast';
import { Btn } from '@/ui';
import { GROUPING_DIMENSIONS, GROUPING_STORAGE_KEY } from './InboxControls';
import { AttributionPicker } from './AttributionPicker';
import { InboxDetail } from './InboxDetail';
import { InboxList, DEFAULT_INBOX_SORT } from './InboxList';
import type { InboxSortCol, InboxSort } from './InboxList';
import { InboxStatsSummary } from './InboxStatsSummary';
import { deriveInboxStats } from './inboxStatsFromItems';
import { PlanApprovalOverlay } from './PlanApprovalOverlay';
import { buildBreakdownFromActions } from './PlanPanel';
import type {
  InboxBreakdownTarget,
  InboxListItem,
  InboxSourceGroupListItem,
} from './store';
import {
  inboxClassifyQueryKey,
  mergeRescanRoots,
  useInboxClassification,
  useInboxClassifySourceGroup,
  useInboxItemMetadata,
  useInboxList,
  useInboxPlanBreakdowns,
  useInboxRescan,
  useOpenInboxPlans,
} from './store';

// Re-export pure selection-handoff functions (tested via this path).
export {
  pickReclassifyTarget,
  resolveReclassifyHandoff,
  resolveClassifiedGroupSelection,
} from './inboxSelectionModel';
export type {
  ReclassifyHandoffDecision,
  ClassifiedGroupSelection,
} from './inboxSelectionModel';

const PLANS_POLL_MS = 1000;

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

export function InboxPage() {
  const { selected, type } = useSearch({ from: '/shell/inbox' });
  const navigate = useNavigate({ from: '/inbox' });
  const queryClient = useQueryClient();

  // FR-001 / FR-002: cross-root aggregate list replaces the hardcoded scan.
  const {
    data: listData,
    loading: listLoading,
    refresh: refreshList,
  } = useInboxList();
  const items = listData?.items ?? EMPTY_ITEMS;
  // Spec 058 FR-016 / T013: scanned folders that have produced no item rows.
  // Empty until T020 removes the scan-time placeholder.
  const sourceGroups = listData?.sourceGroups ?? EMPTY_SOURCE_GROUPS;

  // Search + grouping / sort / frame-type controls now live in the top bar
  // (spec 043 #73/#31). `useGrouping` owns the persisted ordered grouping
  // dimensions; sort is local column-header state; lane + kind filters are
  // URL-backed (`type`) and local state respectively.
  const [search, setSearch] = useState('');
  const { dims, setSlot } = useGrouping({
    storageKey: GROUPING_STORAGE_KEY,
    validIds: GROUPING_DIMENSIONS.map((d) => d.id),
    defaultDims: [],
  });

  // Column-header sort state (replaces the old sort dropdown).
  const [inboxSort, setInboxSort] = useState<InboxSort>(DEFAULT_INBOX_SORT);
  const handleSort = useCallback((col: InboxSortCol) => {
    setInboxSort((prev) =>
      prev.col === col
        ? { col, dir: prev.dir === 'asc' ? 'desc' : 'asc' }
        : { col, dir: 'asc' },
    );
  }, []);

  // Kind filter: frame type of the detection (bias/dark/flat/light/master).
  const [kindFilter, setKindFilter] = useState('');

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

  // #871: after a plan apply completes, offer a direct link to the updated
  // inventory instead of leaving the user to find the moved items manually.
  // Sessions is the library's browsable inventory view; apply has no
  // per-plan destination id to deep-link further than that.
  const viewResultAction = useCallback(
    () => ({
      label: m.inbox_view_result_action(),
      onClick: () => void navigate({ to: '/sessions' }),
    }),
    [navigate],
  );

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

  // FR-006: items are already bounded at 500 by the backend; surface a notice
  // when the cap is hit.
  const isCapped = listData?.capped ?? false;

  // Client-side text search across the relative path (the list's primary key).
  // task 33: also apply the breakdown-row frame-type filter when active.
  const filteredItems = useMemo(() => {
    let result = items;
    const q = search.trim().toLowerCase();
    if (q) {
      result = result.filter(
        (it) =>
          it.relativePath.toLowerCase().includes(q) ||
          (it.groupTarget ?? '').toLowerCase().includes(q),
      );
    }
    return result;
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

  // URL-backed selection is by item id (issue #644), not list position — an
  // index silently points at whatever item now occupies that slot once
  // search/lane/kind filters reshape the array. Sessions and Calibration
  // already use this id-based `?selected=<id>` pattern.
  const selectedItem = selected
    ? filteredItems.find((it) => it.inboxItemId === selected)
    : undefined;

  // Tracks the sourceGroupId of the last item that was actively selected.
  // Updated via React's derived-state-from-render pattern (setState during render
  // is permitted for local state driven by current props/state — the React docs'
  // alternative to getDerivedStateFromProps). Written only when `selectedItem`
  // is defined, so it always holds the most-recently-selected item's identity.
  // When `selectedItem` disappears (classify-split purged the placeholder), this
  // state still carries the sourceGroupId needed to find the successor — without
  // touching a ref during render, which the react-hooks lint rule forbids.
  const [prevSelectedInfo, setPrevSelectedInfo] = useState<{
    inboxItemId: string;
    sourceGroupId: string | null;
  } | null>(null);
  if (
    selectedItem !== undefined &&
    (prevSelectedInfo === null ||
      prevSelectedInfo.inboxItemId !== selectedItem.inboxItemId)
  ) {
    setPrevSelectedInfo({
      inboxItemId: selectedItem.inboxItemId,
      sourceGroupId: selectedItem.sourceGroupId ?? null,
    });
  }

  // `reclassify_v2` operates at source-group scope and re-splits the group
  // into new single-type sub-items (R-14, issue #755) — the currently
  // selected item's id can stop existing mid-flight. Holds the post-split
  // target id until the (already-invalidated, auto-refetching) item list
  // contains it, at which point the effect below moves `selected` to its
  // new index. `useStaleSelectionCleanup` must NOT treat the old index as
  // stale while this handoff is in flight, or it races the handoff and
  // clears the selection first (both fire from the same commit).
  const [pendingReclassifySelectionId, setPendingReclassifySelectionId] =
    useState<string | null>(null);

  // Classify-split handoff (issue #1038 / astro-plan-srz6): when
  // `inbox.classify` materializes sub-items from a placeholder, the placeholder
  // row disappears from the list without any `reclassify_v2` call.
  // `pendingReclassifySelectionId` is never set, so `useStaleSelectionCleanup`
  // would clear the selection instead of following the successor.
  //
  // Rule (CHK011 N=1 case, mirroring `resolveClassifiedGroupSelection`): if
  // EXACTLY ONE item in the settled list shares the missing item's
  // `sourceGroupId`, that is the unambiguous successor — navigate to it.
  // Computed synchronously during render (pure state/props derivation) so its
  // non-null value can gate `useStaleSelectionCleanup` on the SAME render that
  // would otherwise clear the selection. Placed before the cleanup call.
  const classifySplitSibling = useMemo(() => {
    if (
      listLoading ||
      pendingReclassifySelectionId !== null ||
      selected === undefined ||
      selectedItem !== undefined
    ) {
      return null;
    }
    if (
      !prevSelectedInfo ||
      prevSelectedInfo.inboxItemId !== selected ||
      !prevSelectedInfo.sourceGroupId
    ) {
      return null;
    }
    const decision = resolveClassifiedGroupSelection(
      prevSelectedInfo.sourceGroupId,
      items,
      false,
    );
    return decision.action === 'select' ? decision.id : null;
  }, [
    listLoading,
    pendingReclassifySelectionId,
    selected,
    selectedItem,
    prevSelectedInfo,
    items,
  ]);

  // #735: `listLoading` joins the gate because on a cold reload the list cache
  // is empty and an unguarded `selectedItem === undefined` wipes a valid
  // `?selected=` before the list IPC resolves. This does NOT reopen the
  // unbounded-gate hazard the reclassify handoff guards against: `listLoading`
  // settles on its own, whereas `pendingReclassifySelectionId` needed
  // `resolveReclassifyHandoff`'s explicit give-up path.
  useStaleSelectionCleanup(
    selected,
    listLoading ||
      selectedItem !== undefined ||
      pendingReclassifySelectionId !== null ||
      classifySplitSibling !== null,
    () =>
      navigate({
        search: (prev) => ({ ...prev, selected: undefined }),
        replace: true,
      }),
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

  /**
   * `InboxDetail`'s reclassify_v2 callback: queue the post-split handoff, OR
   * — when there is nothing to hand off to — force-refetch the CURRENTLY
   * selected item's own classification.
   *
   * `reclassify_v2` only emits `subItems` when it re-splits a source group
   * into separate materialized rows (R-14); a group that resolves to exactly
   * the item already selected (single-type, no missing attrs — nothing to
   * split) can report an empty/unusable `subItems` list. Relying SOLELY on
   * the handoff left the confirm gate + "frame types required" banner stuck
   * on the pre-reclassify state forever in that case — nothing ever asked
   * the CURRENT selection to re-derive (CI-red,
   * `inbox_ui_unclassified_gate_bulk_reclassify_unblocks_confirm`). The
   * force-refetch is safe unconditionally: if a handoff ALSO starts and
   * later moves selection to a new id, this just refetches a query for an
   * item that's about to fall out of view anyway.
   */
  const handleReclassified = useCallback(
    (response: InboxReclassifyV2Response) => {
      const targetId = pickReclassifyTarget(response.subItems);
      if (targetId) {
        setPendingReclassifySelectionId(targetId);
        return;
      }
      if (selectedItem) {
        void queryClient.invalidateQueries({
          queryKey: inboxClassifyQueryKey(
            selectedItem.rootAbsolutePath,
            selectedItem.inboxItemId,
          ),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.inbox.metadata(selectedItem.inboxItemId),
        });
      }
    },
    [selectedItem, queryClient],
  );

  // Completes (or abandons) the handoff once the invalidated list query has
  // settled (list.type invalidation is fired by InboxDetail's reclassify
  // hook). Bounded via `resolveReclassifyHandoff` — see its doc comment for
  // why the give-up path is required (an active search filter must not be
  // able to gate `useStaleSelectionCleanup` open forever).
  useEffect(() => {
    if (!pendingReclassifySelectionId) return;
    const decision = resolveReclassifyHandoff(
      pendingReclassifySelectionId,
      items,
      filteredItems,
      listLoading,
    );
    if (decision.action === 'wait') return;
    if (decision.action === 'navigate' && decision.id !== selected) {
      // Hold the handoff OPEN across the navigate. `navigate` is async, so
      // clearing the pending id here (as this did) drops
      // `useStaleSelectionCleanup`'s guard one commit BEFORE `?selected=`
      // carries the new id. In that commit the old id is already gone from
      // the list, so `selectedItem` is undefined and the gate opens — the
      // cleanup's `selected: undefined` then lands AFTER this navigate and
      // clobbers it, leaving the page with no selection at all.
      // (T051; measured on `..._bulk_reclassify_unblocks_confirm`, where the
      // URL went old-id → undefined instead of old-id → new-id.)
      // Re-running with `selected === decision.id` falls through to the
      // clear below, so the guard is still bounded.
      void navigate({
        search: (prev) => ({ ...prev, selected: decision.id }),
      });
      return;
    }
    setPendingReclassifySelectionId(null);
  }, [
    pendingReclassifySelectionId,
    items,
    filteredItems,
    listLoading,
    selected,
    navigate,
  ]);

  useEffect(() => {
    if (!classifySplitSibling) return;
    void navigate({
      search: (prev) => ({ ...prev, selected: classifySplitSibling }),
    });
  }, [classifySplitSibling, navigate]);

  // Each item carries its own root path — use it for classify / confirm calls.
  const selectedRootPath = selectedItem?.rootAbsolutePath ?? '';

  // Load classification for the selected item (no-op when nothing selected).
  const { data: classification } = useInboxClassification(
    selectedItem?.inboxItemId ?? '',
    selectedRootPath,
  );

  // Group-scoped classification for scanned-but-unclassified folders
  // (spec 058 FR-017). Unlike the item-scoped hook above this does NOT fire on
  // selection — a source-group row is not selectable — so it is driven by an
  // explicit button in the row.
  const { pendingSourceGroupId, classifySourceGroup } =
    useInboxClassifySourceGroup();

  // CHK011 handoff: set once a classify succeeds, cleared when the refetched
  // list settles and `resolveClassifiedGroupSelection` decides.
  const [pendingClassifiedGroupId, setPendingClassifiedGroupId] = useState<
    string | null
  >(null);

  const handleClassifySourceGroup = useCallback(
    (group: InboxSourceGroupListItem) => {
      void classifySourceGroup({
        sourceGroupId: group.sourceGroupId,
        rootAbsolutePath: group.rootAbsolutePath,
      })
        .then(() => {
          setPendingClassifiedGroupId(group.sourceGroupId);
        })
        .catch((e: unknown) => {
          // The row erases itself on success, so a silent failure would look
          // like nothing happened at all. Surface it.
          addToast({
            variant: 'error',
            message: m.inbox_toast_classify_group_failed({
              message: e instanceof Error ? e.message : String(e),
            }),
          });
        });
    },
    [classifySourceGroup],
  );

  // Completes the CHK011 handoff once the invalidated list has settled. Mirrors
  // the reclassify handoff above, including its bounded give-up: a decision is
  // only taken on a settled list, and the pending id is always cleared so it
  // cannot gate `useStaleSelectionCleanup` open indefinitely.
  useEffect(() => {
    if (!pendingClassifiedGroupId) return;
    const decision = resolveClassifiedGroupSelection(
      pendingClassifiedGroupId,
      items,
      listLoading,
    );
    if (decision.action === 'wait') return;
    setPendingClassifiedGroupId(null);
    if (decision.action === 'select' && decision.id !== selected) {
      void navigate({ search: (prev) => ({ ...prev, selected: decision.id }) });
    }
  }, [pendingClassifiedGroupId, items, listLoading, selected, navigate]);

  // Load per-file extracted metadata for the selected item (spec 041 US2/FR-010).
  // Issue #643: `loading`/`error` used to be discarded here, so a metadata
  // fetch that never lands (or errors) left `fileMetadata` at its `[]`
  // default — `hasMissingRequiredMeta` below then saw no files at all and
  // silently left Confirm ENABLED on an item the backend would still refuse.
  const {
    data: fileMetadata,
    loading: fileMetadataLoading,
    error: fileMetadataError,
  } = useInboxItemMetadata(selectedItem?.inboxItemId ?? null);

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
  const hasMissingRequiredMeta = useMemo(
    () =>
      (fileMetadata ?? []).some(
        (f) => (f.missingPathAttributes?.length ?? 0) > 0,
      ),
    [fileMetadata],
  );

  const {
    handleConfirm,
    handlePickAttribution,
    handlePickDestinationRoot,
    handleBulkConfirm,
    canConfirm,
    confirmLoading,
    confirmFlowBusy,
    bulkConfirmLoading,
    canBulkConfirm,
    bulkEligibleItems,
    destructiveDestination,
    setDestructiveDestination,
    selectedDestRootId,
    setSelectedDestRootId,
    pendingRootPick,
    pendingAttribution,
    clearPendingAttribution,
    attributionProjectNames,
    absoluteByFromPath,
  } = useInboxConfirmFlow({
    selectedItem,
    selectedRootPath,
    classification,
    fileMetadataLoading,
    fileMetadataError,
    hasMissingRequiredMeta,
    items,
    refreshAll,
  });

  const {
    handleApplyOne,
    handleApplyAll,
    handleApplySelected,
    handleCancel,
    applyProgress,
    progressPlanId,
    planBusy,
  } = useInboxPlanApplyFlow(refreshAll, viewResultAction);

  // Stage B: plan review overlay open/close state.
  const [planOverlayOpen, setPlanOverlayOpen] = useState(false);

  // Auto-close the overlay once all plans have been applied/cancelled.
  useEffect(() => {
    if (planOverlayOpen && openPlans.length === 0 && pendingRootPick == null) {
      setPlanOverlayOpen(false);
    }
  }, [planOverlayOpen, openPlans.length, pendingRootPick]);

  // While the overlay is open, poll the open-plan surface: the backend's
  // plan-applied LISTENER transitions items to resolved asynchronously
  // AFTER `inbox.plan.apply*` returns (`plan_listener.rs`), so the single
  // post-apply `refreshAll()` can race it and read the plan as still open —
  // after which nothing would ever refresh again and the overlay could
  // never auto-close (deterministic on CI runners, spec 037 Layer-2
  // catalogue journey, PR #457). Polling only while open keeps the page
  // quiescent otherwise.
  useEffect(() => {
    if (!planOverlayOpen) return undefined;
    const timer = setInterval(() => refreshOpenPlans(), PLANS_POLL_MS);
    return () => clearInterval(timer);
  }, [planOverlayOpen, refreshOpenPlans]);

  // spec 041 T072: "Generate split plan" is retired along with the backend
  // "split" action (FR-050) — a mixed row is disabled via `canConfirm`
  // above, so the label is always the plain confirm label now.
  const confirmLabel = m.inbox_confirm_to_inventory();

  // spec 041 US6: aggregate inbox queue stats. Derived from the SAME item list
  // the header/footer count from (distinct-folder counting) so the stats strip,
  // header, and footer always reconcile — a mixed folder counts once overall.
  //
  // spec 058 T022 / SC-004 (owner decision, 2026-07-20): derived from
  // `filteredItems`, NOT `items`. `InboxList` renders `filteredItems`, so
  // deriving the summary from the unfiltered array made the strip report more
  // rows than the list showed whenever a lane or kind filter was active. A
  // summary sitting above a filtered list and disagreeing with it is the same
  // class of lie this feature exists to remove, so the counts now describe what
  // the user is actually looking at.
  // T022 / CHK010: source-group rows are counted, and from the same filtered
  // arrays `InboxList` renders — otherwise the strip and the list disagree the
  // moment a scanned-but-unclassified folder exists.
  const derivedStats = useMemo(
    () => deriveInboxStats(filteredItems, filteredSourceGroups),
    [filteredItems, filteredSourceGroups],
  );

  // #75: frame-type hint per ingestion, derived from the inbox item's
  // classification/breakdown (here: the dominant `groupFrameType`, or the
  // master's `masterFrameType`). PlanPanel uses this to label each collapsed
  // group bucket by frame type (bias/dark/flat/light/master) instead of
  // degenerating to one line per catalogue action.
  const frameTypeByItemId = useMemo(() => {
    const m: Record<string, string> = {};
    for (const it of items) {
      const ft = it.isMaster
        ? (it.masterFrameType ?? 'master')
        : it.groupFrameType;
      if (ft) m[it.inboxItemId] = ft;
    }
    return m;
  }, [items]);

  // #98: PRELOAD the authoritative per-type breakdown for EVERY item that has
  // an open plan — not just the selected one. Each open plan is mapped to its
  // item's registered root path (from the inbox list) so the classify query can
  // run. The hook shares `useInboxClassification`'s cache key, so the selected
  // item's classification is reused rather than re-fetched. The result is a
  // `inboxItemId → breakdown[]` map covering all unselected mixed folders, which
  // previously degraded to a dominant-type guess (e.g. "41 darks").
  const rootPathByItemId = useMemo(() => {
    const m: Record<string, string> = {};
    for (const it of items) m[it.inboxItemId] = it.rootAbsolutePath;
    return m;
  }, [items]);

  const breakdownTargets = useMemo<InboxBreakdownTarget[]>(() => {
    const seen = new Set<string>();
    const out: InboxBreakdownTarget[] = [];
    for (const plan of openPlans) {
      const rootAbsolutePath = rootPathByItemId[plan.inboxItemId];
      if (!rootAbsolutePath || seen.has(plan.inboxItemId)) continue;
      seen.add(plan.inboxItemId);
      out.push({ inboxItemId: plan.inboxItemId, rootAbsolutePath });
    }
    return out;
  }, [openPlans, rootPathByItemId]);

  const preloadedBreakdowns = useInboxPlanBreakdowns(breakdownTargets);

  // #75/#98: per-ingestion frame-type BREAKDOWN for the collapsed plan summary —
  // the per-type bias/dark/flat/light/master counts (same shape the classify
  // breakdown / InboxStatsSummary use). Sourced + merged per item, preferring
  // the most authoritative source available:
  //   1. From each open plan's ACTIONS, classified by destination-path keyword
  //      + the per-item hint (`buildBreakdownFromActions`) — the always-present
  //      fallback. A MOVE/SPLIT plan whose files land in typed folders yields a
  //      TRUE multi-type tally even before classify resolves.
  //   2. The PRELOADED real classification `breakdown[]` for the plan's item
  //      (#98) — resolves a MIXED in-place catalogue the action paths cannot,
  //      for EVERY open plan regardless of selection.
  //   3. The SELECTED item's freshly-loaded classification breakdown — same
  //      data as (2) but guaranteed current for the active selection.
  // The result keys each plan to its tally so PlanPanel renders one summary
  // line ("10 bias · 21 dark · 12 light → (root)") instead of per-file rows.
  const breakdownByItemId = useMemo(() => {
    const m: Record<
      string,
      ReadonlyArray<{ kind: string; count: number }>
    > = {};
    for (const plan of openPlans) {
      m[plan.inboxItemId] = buildBreakdownFromActions(
        plan.actions,
        frameTypeByItemId[plan.inboxItemId],
        absoluteByFromPath,
      );
    }
    // Overlay the preloaded authoritative breakdown for every open plan item.
    for (const [id, breakdown] of Object.entries(preloadedBreakdowns)) {
      if (breakdown.length > 0 && m[id] != null) m[id] = breakdown;
    }
    // Prefer the selected item's authoritative classification breakdown.
    if (
      selectedItem &&
      classification?.breakdown &&
      classification.breakdown.length > 0 &&
      m[selectedItem.inboxItemId] != null
    ) {
      m[selectedItem.inboxItemId] = classification.breakdown.map((b) => ({
        kind: b.kind,
        count: b.count,
      }));
    }
    return m;
  }, [
    openPlans,
    frameTypeByItemId,
    absoluteByFromPath,
    preloadedBreakdowns,
    selectedItem,
    classification,
  ]);

  // Summary count (no page title — top-bar convention): folders / masters.
  //
  // spec 058 SC-004: counted off the SAME filtered arrays as `derivedStats`
  // above, and off the same ones `InboxList` renders. These two surfaces sit
  // beside each other and had drifted apart in two independent ways:
  //
  //   - the stats strip moved onto `filteredItems` for SC-004 while the header
  //     stayed on the unfiltered `items`, so with any lane/kind/search filter
  //     active the two adjacent numbers disagreed;
  //   - `deriveInboxStats` then began counting source-group rows (CHK010) while
  //     the header still ignored them, so a scanned-but-unclassified folder was
  //     counted by one surface and not the other.
  //
  // Both are the same defect SC-004 names — a summary that disagrees with the
  // list under it — so both are fixed at the one site rather than the header
  // being taught the source-group rule separately.
  const folderCount =
    filteredItems.filter((it) => !it.isMaster).length +
    filteredSourceGroups.length;
  const masterCount = filteredItems.filter((it) => it.isMaster).length;
  const summary = useMemo(() => {
    if (listLoading) return m.common_loading();
    const parts: string[] = [];
    if (folderCount > 0)
      parts.push(m.inbox_count_folders({ count: folderCount }));
    if (masterCount > 0)
      parts.push(m.inbox_count_masters({ count: masterCount }));
    const base =
      parts.length > 0 ? parts.join(' · ') : m.inbox_summary_zero_detections();
    return isCapped
      ? m.inbox_summary_capped({ base, limit: String(listData?.limit ?? 500) })
      : base;
  }, [listLoading, folderCount, masterCount, isCapped, listData?.limit]);

  // ── Status bar: push the inbox-specific folder/master count + per-frame-type
  // breakdown into the global status bar's page-contextual slot. The top bar
  // reverts to filters + actions only (matching all other pages). The slot is
  // automatically cleared when this page unmounts (route change). ──
  //
  // #557: this JSX MUST be memoised. `useSetPageStatus` re-runs its effect
  // whenever the node's identity changes; a bare JSX literal gets a fresh
  // identity on every render, so the effect fired every render, called
  // `setNode` on the shell-level `PageStatusProvider`, which re-rendered this
  // page and created another new-identity node — an infinite render loop
  // ("Maximum update depth exceeded") for as long as the Inbox was open.
  const pageStatusNode = useMemo(
    () => (
      <span className="pv-inbox-summary" data-testid="statusbar-inbox-summary">
        <span className="pv-inbox-summary__count">{summary}</span>
        {!listLoading && <InboxStatsSummary stats={derivedStats} />}
      </span>
    ),
    [summary, listLoading, derivedStats],
  );
  useSetPageStatus(pageStatusNode);

  // Shown when ≥1 open plan exists OR a destination-root pick is pending (the
  // latter can occur with zero open plans — the plan wasn't generated yet).
  // Declared BEFORE topBar: the topBar JSX evaluates these eagerly at createElement.
  const showPlans = openPlans.length > 0 || pendingRootPick != null;
  const planCount = openPlans.length;

  // ── Top bar: NO page title, NO summary (top-bar convention matches other pages).
  // Search + group/sort/filter in FilterToolbar; Confirm + Rescan on the right. ──
  const topBar = (
    <PageTopBar
      filters={
        <FilterToolbar
          search={{
            value: search,
            onChange: setSearch,
            placeholder: m.inbox_search_placeholder(),
            ariaLabel: m.inbox_search_aria_label(),
          }}
          fields={[
            {
              key: 'fileType',
              label: m.inbox_filter_file_type_label(),
              value: type ?? '',
              options: [
                { value: 'fits', label: m.inbox_filter_fits() },
                { value: 'video', label: m.inbox_filter_video() },
              ],
              allLabel: m.inbox_filter_all_file_types(),
              onChange: (v) =>
                navigate({
                  search: (prev) => ({
                    ...prev,
                    type: (v || undefined) as FrameType | undefined,
                  }),
                }),
            },
            {
              key: 'kind',
              label: m.inbox_filter_kind_label(),
              value: kindFilter,
              options: [
                { value: 'bias', label: m.inbox_kind_bias() },
                { value: 'dark', label: m.inbox_kind_dark() },
                { value: 'flat', label: m.inbox_kind_flat() },
                { value: 'light', label: m.inbox_kind_light() },
                { value: 'master', label: m.inbox_kind_master() },
              ],
              allLabel: m.inbox_filter_kind_all(),
              onChange: setKindFilter,
            },
          ]}
          grouping={{
            dimensions: GROUPING_DIMENSIONS.map((d) => ({
              value: d.id,
              label: d.label(),
            })),
            dims,
            setSlot,
          }}
        />
      }
      actions={
        <>
          {/* Stage B: trigger to open the plan-approval overlay. Shown
						    whenever open plans OR a pending root pick exist. */}
          {showPlans && (
            <Btn
              size="sm"
              variant="ghost"
              onClick={() => setPlanOverlayOpen(true)}
              aria-label={m.inbox_review_plans_with_count({ count: planCount })}
              data-testid="inbox-review-plans-btn"
            >
              {planCount > 0
                ? m.inbox_review_plans_with_count({ count: planCount })
                : m.inbox_review_plans()}
            </Btn>
          )}
          {/* task 35: bulk-confirm all cleanly-classified items in one action */}
          {bulkEligibleItems.length > 0 && (
            <Btn
              size="sm"
              variant="primary"
              disabled={!canBulkConfirm}
              onClick={() => void handleBulkConfirm()}
              aria-label={m.inbox_confirm_all_classified_aria({
                count: bulkEligibleItems.length,
              })}
              data-testid="inbox-bulk-confirm-btn"
              // Onboarding find-it spotlight anchor (spec 056 FR-026). The
              // canonical `inbox.confirm-row` anchor lives on this page-level
              // bulk-confirm action so the spotlight resolves it.
              data-guide-anchor="inbox.confirm-row"
            >
              {bulkConfirmLoading
                ? m.common_confirming()
                : m.inbox_confirm_all({ count: bulkEligibleItems.length })}
            </Btn>
          )}
          {/* Per-detection "Confirm to inventory" lives in the bottom detail
						header (Sessions convention). The top bar keeps only page-level
						actions: review plans, bulk confirm, rescan. */}
          <Btn
            size="sm"
            disabled={rescanLoading}
            onClick={() => void rescan()}
            aria-label={m.inbox_rescan_all_roots_aria()}
          >
            {rescanLoading ? m.common_rescanning() : m.common_rescan()}
          </Btn>
        </>
      }
    />
  );

  // ── Standardised list-page layout (Sessions/Calibration reference) ──
  //   primary: detection LIST (full width)
  //   detail:  InboxDetail docked in the BOTTOM panel (auto-size, own scroll)
  //            with the per-detection "Confirm to inventory" inline in its
  //            header. Plan review remains the focused PlanApprovalOverlay.
  return (
    <>
      <ListPageLayout
        topBar={topBar}
        dockId="inbox"
        detailLabel={m.inbox_detection_details()}
        detail={
          selectedItem != null ? (
            <InboxDetail
              // Remount per SOURCE GROUP (not per raw item id) so per-item
              // state (pending overrides, the "Needs review" bulk-select /
              // frame-type / exposure fields) never leaks across a genuinely
              // different selection, but SURVIVES the involuntary id churn
              // classify()'s own materialize_sub_items performs on the very
              // FIRST classify of a freshly scanned item (placeholder row
              // purged, replaced by a fresh-UUID needs-review sub-item —
              // `useInboxReclassifyV2`'s docstring above, `classify.rs`'s
              // `materialize_sub_items`). Keying on `inboxItemId` remounted
              // InboxDetail — wiping `selectedFiles`/`bulkFrameType` — the
              // instant that churn landed, mid-sequence, silently no-opping
              // the bulk-reclassify Apply click (CI-red,
              // `inbox_ui_unclassified_gate_bulk_reclassify_unblocks_confirm`
              // — `allReclassifyV2CallCount` in the CI dump proved the click
              // never reached a real command). The materialized sub-item
              // always carries the SAME `sourceGroupId` as the placeholder it
              // replaced (`classify.rs`'s `sg_id_for_split`), so this key is
              // stable across exactly that transition while still changing
              // for an unrelated row (a different source group, or a legacy
              // pre-source-group item, where it falls back to the item id).
              key={selectedItem.sourceGroupId ?? selectedItem.inboxItemId}
              item={selectedItem}
              rootAbsolutePath={selectedRootPath}
              classification={classification ?? null}
              fileMetadata={fileMetadata}
              // Confirm runs the same flow the old top-bar button did.
              // Disabled for any row that is not single-type — see canConfirm.
              onConfirm={() => void handleConfirm()}
              confirmLabel={confirmLabel}
              confirmDisabled={!canConfirm}
              confirmBusy={confirmLoading || confirmFlowBusy}
              destinationRoots={destRoots}
              selectedRootId={selectedDestRootId}
              onSelectRoot={setSelectedDestRootId}
              onReclassified={handleReclassified}
              // Stable reclassify scope: sub-item ids are purged/recreated by
              // re-splits; the source-group id survives them (see
              // useInboxReclassifyV2 in InboxDetail).
              sourceGroupId={selectedItem.sourceGroupId}
            />
          ) : undefined
        }
        onCloseDetail={selectedItem != null ? clearSelection : undefined}
      >
        <InboxList
          items={filteredItems}
          sourceGroups={filteredSourceGroups}
          onClassifySourceGroup={handleClassifySourceGroup}
          classifyingSourceGroupId={pendingSourceGroupId}
          selectedId={selected ?? null}
          onSelect={onSelect}
          filterType={type ?? 'all'}
          dims={dims}
          kindFilter={kindFilter}
          loading={listLoading}
          sort={inboxSort}
          onSort={handleSort}
        />
      </ListPageLayout>

      {/* spec 008 US7/FR-022 (#943): the pre-confirm attribution pick. */}
      {pendingAttribution && (
        <AttributionPicker
          candidates={pendingAttribution.candidates}
          projectNames={attributionProjectNames}
          busy={confirmLoading}
          onPick={(chosen) => void handlePickAttribution(chosen)}
          onCancel={clearPendingAttribution}
        />
      )}

      {/* Plan-approval overlay — opens via top-bar trigger.
			    Wraps the existing PlanPanel; all apply/cancel/root-pick
			    handlers are passed through unchanged. The per-plan live-apply
			    progress stream (spec 042 US16 / FR-021) threads through here so
			    the overlay's PlanPanel can show per-group apply progress. */}
      <PlanApprovalOverlay
        open={planOverlayOpen}
        onClose={() => setPlanOverlayOpen(false)}
        plans={openPlans}
        totalActions={totalActions}
        destructiveDestination={destructiveDestination}
        onDestructiveDestinationChange={setDestructiveDestination}
        onApplySelected={(ids) => void handleApplySelected(ids)}
        onApplyAll={() => void handleApplyAll()}
        onApplyOne={(planId) => void handleApplyOne(planId)}
        progress={applyProgress}
        progressPlanId={progressPlanId}
        onCancel={(id) => void handleCancel(id)}
        busy={planBusy || applyProgress.running}
        pendingRootPick={pendingRootPick}
        onPickDestinationRoot={(rootId) =>
          void handlePickDestinationRoot(rootId)
        }
        rootPickBusy={confirmLoading}
        absoluteByFromPath={absoluteByFromPath}
        frameTypeByItemId={frameTypeByItemId}
        breakdownByItemId={breakdownByItemId}
      />
    </>
  );
}
