// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * URL-backed inbox selection plus the two selection handoffs that survive an
 * item id disappearing mid-flight (`reclassify_v2` re-split, classify-split
 * materialization).
 *
 * Selection is by item id (`?selected=<id>`), not list position: an index
 * silently points at whatever item now occupies that slot once search / lane /
 * kind filters reshape the array (issue #644).
 */

import { useNavigate } from '@tanstack/react-router';
import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import type { InboxReclassifyV2Response_Serialize as InboxReclassifyV2Response } from '@/bindings/index';
import { queryKeys } from '@/data/queryKeys';
import { m } from '@/lib/i18n';
import { useStaleSelectionCleanup } from '@/lib/use-stale-selection';
import { addToast } from '@/shared/toast';
import {
  pickReclassifyTarget,
  resolveClassifiedGroupSelection,
  resolveReclassifyHandoff,
} from './inboxSelectionModel';
import { inboxClassifyQueryKey, useInboxClassifySourceGroup } from './store';
import type { InboxListItem, InboxSourceGroupListItem } from './store';

export interface UseInboxSelectionOptions {
  /** Current `?selected=` value. */
  selected: string | undefined;
  /** The item `selected` resolves to in the filtered list, if still present. */
  selectedItem: InboxListItem | undefined;
  /** Unfiltered item list. */
  items: InboxListItem[];
  /** Page-filtered item list (what the list renders). */
  filteredItems: InboxListItem[];
  listLoading: boolean;
}

export interface UseInboxSelectionResult {
  onSelect: (id: string) => void;
  clearSelection: () => void;
  handleReclassified: (response: InboxReclassifyV2Response) => void;
  handleClassifySourceGroup: (group: InboxSourceGroupListItem) => void;
  classifyingSourceGroupId: string | null;
}

export function useInboxSelection({
  selected,
  selectedItem,
  items,
  filteredItems,
  listLoading,
}: UseInboxSelectionOptions): UseInboxSelectionResult {
  const navigate = useNavigate({ from: '/inbox' });
  const queryClient = useQueryClient();

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

  const onSelect = useCallback(
    (id: string) => {
      void navigate({ search: (prev) => ({ ...prev, selected: id }) });
    },
    [navigate],
  );

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

  // Group-scoped classification for scanned-but-unclassified folders
  // (spec 058 FR-017). Unlike the item-scoped classification query this does
  // NOT fire on selection — a source-group row is not selectable — so it is
  // driven by an explicit button in the row.
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

  return {
    onSelect,
    clearSelection,
    handleReclassified,
    handleClassifySourceGroup,
    classifyingSourceGroupId: pendingSourceGroupId,
  };
}
