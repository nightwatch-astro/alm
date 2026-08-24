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
 *
 * The page is composition only. Data and derivation live in
 * `useInboxListData`, selection and its two mid-flight handoffs in
 * `useInboxSelection`, plan tallies in `useInboxPlanSummaries`, the status-bar
 * slot in `useInboxPageStatus`, and the top bar in `InboxTopBar`.
 */

import { useSearch, useNavigate } from '@tanstack/react-router';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ListPageLayout } from '@/components';
import { useInboxConfirmFlow } from './useInboxConfirmFlow';
import { useInboxListData } from './useInboxListData';
import { useInboxPageStatus } from './useInboxPageStatus';
import { useInboxPlanApplyFlow } from './useInboxPlanApplyFlow';
import { useInboxPlanSummaries } from './useInboxPlanSummaries';
import { useInboxSelection } from './useInboxSelection';
import { m } from '@/lib/i18n';
import { useGrouping } from '@/lib/use-grouping';
import { GROUPING_DIMENSIONS, GROUPING_STORAGE_KEY } from './InboxControls';
import { AttributionPicker } from './AttributionPicker';
import { InboxDetail } from './InboxDetail';
import { InboxList, DEFAULT_INBOX_SORT } from './InboxList';
import type { InboxSortCol, InboxSort } from './InboxList';
import { InboxTopBar } from './InboxTopBar';
import { PlanApprovalOverlay } from './PlanApprovalOverlay';
import { useInboxClassification, useInboxItemMetadata } from './store';

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

export function InboxPage() {
  const { selected, type } = useSearch({ from: '/shell/inbox' });
  const navigate = useNavigate({ from: '/inbox' });

  const {
    items,
    filteredItems,
    filteredSourceGroups,
    listLoading,
    isCapped,
    limit,
    search,
    setSearch,
    openPlans,
    totalActions,
    refreshAll,
    refreshOpenPlans,
    destRoots,
    rescanLoading,
    rescan,
  } = useInboxListData();

  // Search + grouping / sort / frame-type controls now live in the top bar
  // (spec 043 #73/#31). `useGrouping` owns the persisted ordered grouping
  // dimensions; sort is local column-header state; lane + kind filters are
  // URL-backed (`type`) and local state respectively.
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

  // URL-backed selection is by item id (issue #644), not list position — an
  // index silently points at whatever item now occupies that slot once
  // search/lane/kind filters reshape the array.
  const selectedItem = selected
    ? filteredItems.find((it) => it.inboxItemId === selected)
    : undefined;

  const {
    onSelect,
    clearSelection,
    handleReclassified,
    handleClassifySourceGroup,
    classifyingSourceGroupId,
  } = useInboxSelection({
    selected,
    selectedItem,
    items,
    filteredItems,
    listLoading,
  });

  // Each item carries its own root path — use it for classify / confirm calls.
  const selectedRootPath = selectedItem?.rootAbsolutePath ?? '';

  // Load classification for the selected item (no-op when nothing selected).
  const { data: classification } = useInboxClassification(
    selectedItem?.inboxItemId ?? '',
    selectedRootPath,
  );

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
  } = useInboxPlanApplyFlow(refreshAll, viewResultAction, openPlans);

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

  const { frameTypeByItemId, breakdownByItemId } = useInboxPlanSummaries({
    openPlans,
    items,
    absoluteByFromPath,
    selectedItem,
    classification,
  });

  useInboxPageStatus({
    filteredItems,
    filteredSourceGroups,
    listLoading,
    isCapped,
    limit,
  });

  // ── Standardised list-page layout (Sessions/Calibration reference) ──
  //   primary: detection LIST (full width)
  //   detail:  InboxDetail docked in the BOTTOM panel (auto-size, own scroll)
  //            with the per-detection "Confirm to inventory" inline in its
  //            header. Plan review remains the focused PlanApprovalOverlay.
  return (
    <>
      <ListPageLayout
        topBar={
          <InboxTopBar
            search={search}
            onSearchChange={setSearch}
            fileType={type}
            onFileTypeChange={(v) =>
              navigate({ search: (prev) => ({ ...prev, type: v }) })
            }
            kindFilter={kindFilter}
            onKindFilterChange={setKindFilter}
            dims={dims}
            setSlot={setSlot}
            // Shown when ≥1 open plan exists OR a destination-root pick is
            // pending — the latter can occur with zero open plans, when the
            // plan was not generated yet.
            showPlans={openPlans.length > 0 || pendingRootPick != null}
            planCount={openPlans.length}
            onOpenPlans={() => setPlanOverlayOpen(true)}
            bulkEligibleCount={bulkEligibleItems.length}
            canBulkConfirm={canBulkConfirm}
            bulkConfirmLoading={bulkConfirmLoading}
            onBulkConfirm={() => void handleBulkConfirm()}
            rescanLoading={rescanLoading}
            onRescan={() => void rescan()}
          />
        }
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
          classifyingSourceGroupId={classifyingSourceGroupId}
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
