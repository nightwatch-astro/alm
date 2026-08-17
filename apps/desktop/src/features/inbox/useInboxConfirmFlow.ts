// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useInboxConfirmFlow — confirm lifecycle (single + bulk), attribution
 * prefetch/pick, destination-root pick, and all associated transient state.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { queryKeys } from '@/data/queryKeys';
import type {
  ChosenAttributionDto_Deserialize as ChosenAttributionRequest,
  IngestionAttributionCandidateDto,
  InboxConfirmDestination,
} from '@/bindings/index';
import { m } from '@/lib/i18n';
import { useHotkeys } from '@/lib/useHotkeys';
import { addToast } from '@/shared/toast';
import type { DestructiveDestination, PendingRootPick } from './PlanPanel';
import type { InboxListItem } from './store';
import { normalizeConfirmError, useInboxConfirm } from './store';

/** Shape of `inbox.destination_root_required` error details (spec 041 US8/FR-029). */
interface DestinationRootRequiredDetails {
  category: string;
  candidates: Array<{ rootId: string; path: string; kind: string }>;
}

function asRootRequiredDetails(
  d: unknown,
): DestinationRootRequiredDetails | null {
  if (
    d &&
    typeof d === 'object' &&
    'candidates' in d &&
    Array.isArray(d.candidates)
  ) {
    return d as DestinationRootRequiredDetails;
  }
  return null;
}

export interface ConfirmFlowDeps {
  selectedItem: InboxListItem | undefined;
  selectedRootPath: string;
  classification:
    | {
        contentSignature: string;
        type: string;
        breakdown: Array<{ kind: string; count: number }>;
      }
    | null
    | undefined;
  fileMetadataLoading: boolean;
  fileMetadataError: unknown;
  hasMissingRequiredMeta: boolean;
  items: InboxListItem[];
  refreshAll: () => void;
}

export interface ConfirmFlowResult {
  handleConfirm: () => Promise<void>;
  handlePickAttribution: (chosen: ChosenAttributionRequest) => Promise<void>;
  handlePickDestinationRoot: (rootId: string) => Promise<void>;
  handleBulkConfirm: () => Promise<void>;
  canConfirm: boolean;
  confirmLoading: boolean;
  confirmFlowBusy: boolean;
  bulkConfirmLoading: boolean;
  canBulkConfirm: boolean;
  bulkEligibleItems: InboxListItem[];
  destructiveDestination: DestructiveDestination;
  setDestructiveDestination: (d: DestructiveDestination) => void;
  selectedDestRootId: string;
  setSelectedDestRootId: (id: string) => void;
  pendingRootPick: PendingRootPick | null;
  pendingAttribution: {
    itemId: string;
    rootAbsolutePath: string;
    contentSignature: string;
    rootId?: string;
    candidates: IngestionAttributionCandidateDto[];
  } | null;
  clearPendingAttribution: () => void;
  attributionProjectNames: Record<string, string>;
  absoluteByFromPath: Record<string, string>;
}

export function useInboxConfirmFlow(deps: ConfirmFlowDeps): ConfirmFlowResult {
  const {
    selectedItem,
    selectedRootPath,
    classification,
    fileMetadataLoading,
    fileMetadataError,
    hasMissingRequiredMeta,
    items,
    refreshAll,
  } = deps;

  const queryClient = useQueryClient();
  const { confirm, loading: confirmLoading } = useInboxConfirm();

  const [destructiveDestination, setDestructiveDestination] =
    useState<DestructiveDestination>('archive');
  const [confirmFlowBusy, setConfirmFlowBusy] = useState(false);
  const [pendingRootPick, setPendingRootPick] =
    useState<PendingRootPick | null>(null);
  const [rootPickItemId, setRootPickItemId] = useState<string | null>(null);
  const [pendingAttribution, setPendingAttribution] = useState<{
    itemId: string;
    rootAbsolutePath: string;
    contentSignature: string;
    rootId?: string;
    candidates: IngestionAttributionCandidateDto[];
  } | null>(null);
  const [absoluteByFromPath, setAbsoluteByFromPath] = useState<
    Record<string, string>
  >({});
  const [selectedDestRootId, setSelectedDestRootId] = useState('');
  const [bulkConfirmLoading, setBulkConfirmLoading] = useState(false);

  const selectedItemId = selectedItem?.inboxItemId ?? null;

  // Drop a pending root pick when the user navigates away from its item.
  useEffect(() => {
    if (rootPickItemId && rootPickItemId !== selectedItemId) {
      setPendingRootPick(null);
      setRootPickItemId(null);
    }
  }, [rootPickItemId, selectedItemId]);

  // #648: Reset dest root on selection change.
  useEffect(() => {
    setSelectedDestRootId('');
  }, [selectedItemId]);

  const mergeDestinations = useCallback(
    (destinations?: InboxConfirmDestination[] | null) => {
      if (!destinations || destinations.length === 0) return;
      setAbsoluteByFromPath((prev) => {
        const next = { ...prev };
        for (const d of destinations) {
          next[d.fromPath] = d.toAbsolutePath;
        }
        return next;
      });
    },
    [],
  );

  const runConfirm = useCallback(
    async (
      item: { inboxItemId: string; rootAbsolutePath: string },
      contentSignature: string,
      rootId?: string,
      chosenAttribution?: ChosenAttributionRequest,
      // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: confirm submit handler with per-error-code recovery branches
    ) => {
      try {
        const result = await confirm({
          inboxItemId: item.inboxItemId,
          contentSignature,
          rootAbsolutePath: item.rootAbsolutePath,
          destructiveDestination,
          rootId: rootId ?? null,
          chosenAttribution,
        });
        setPendingRootPick(null);
        setRootPickItemId(null);
        setPendingAttribution(null);
        mergeDestinations(result.destinations);
        addToast({
          message: m.inbox_toast_plan_created({
            count: String(result.itemsTotal),
          }),
          variant: 'info',
        });
        refreshAll();
      } catch (e) {
        const { code, message, details } = normalizeConfirmError(e);
        if (code === 'inbox.destination_root_required') {
          const parsed = asRootRequiredDetails(details);
          if (parsed) {
            setPendingRootPick({
              category: parsed.category,
              candidates: parsed.candidates,
            });
            setRootPickItemId(item.inboxItemId);
            addToast({
              message: m.inbox_toast_choose_dest_root(),
              variant: 'warn',
            });
            return;
          }
        }
        if (code === 'inbox.invalid_destination_root') {
          addToast({
            message: message || m.inbox_toast_invalid_destination_root(),
            variant: 'error',
          });
          return;
        }
        if (code === 'inbox.no_destination_root') {
          addToast({
            message: message || m.inbox_toast_no_destination_root(),
            variant: 'error',
          });
          return;
        }
        if (code === 'inbox.missing_path_attributes') {
          addToast({
            message: m.inbox_toast_missing_path_attrs(),
            variant: 'warn',
          });
          return;
        }
        if (message.includes('inbox.has.open.plan')) {
          addToast({ message: m.inbox_toast_has_open_plan(), variant: 'warn' });
        } else if (message.includes('classification.stale')) {
          addToast({
            message: m.inbox_toast_stale_classification(),
            variant: 'warn',
          });
        } else {
          addToast({
            message: m.inbox_toast_confirm_failed({ message }),
            variant: 'error',
          });
        }
      }
    },
    [confirm, destructiveDestination, mergeDestinations, refreshAll],
  );

  // GFD-1: prefetch attribution candidates alongside classify.
  const selectedItemIdForPrefetch = selectedItem?.inboxItemId ?? null;
  useEffect(() => {
    if (!selectedItemIdForPrefetch) return;
    void queryClient.prefetchQuery({
      queryKey: queryKeys.inbox.attributionSuggest(selectedItemIdForPrefetch),
      queryFn: async () =>
        unwrap(
          await commands.inboxAttributionSuggest(selectedItemIdForPrefetch),
        ),
    });
  }, [selectedItemIdForPrefetch, queryClient]);

  const handleConfirm = useCallback(async () => {
    if (!selectedItem || !classification) return;
    if (confirmFlowBusy) return;
    setConfirmFlowBusy(true);
    try {
      const rootId = selectedDestRootId || undefined;
      let candidates: IngestionAttributionCandidateDto[] = [];
      try {
        const cached = queryClient.getQueryData<
          IngestionAttributionCandidateDto[]
        >(queryKeys.inbox.attributionSuggest(selectedItem.inboxItemId));
        if (cached !== undefined) {
          candidates = cached;
        } else {
          candidates = unwrap(
            await commands.inboxAttributionSuggest(selectedItem.inboxItemId),
          );
        }
      } catch (err) {
        console.error(
          `inbox.attribution.suggest failed for item ${selectedItem.inboxItemId}; confirming without an attribution pick`,
          err,
        );
        candidates = [];
      }
      if (candidates.length > 0) {
        setPendingAttribution({
          itemId: selectedItem.inboxItemId,
          rootAbsolutePath: selectedRootPath,
          contentSignature: classification.contentSignature,
          rootId,
          candidates,
        });
        return;
      }
      await runConfirm(
        {
          inboxItemId: selectedItem.inboxItemId,
          rootAbsolutePath: selectedRootPath,
        },
        classification.contentSignature,
        rootId,
      );
    } finally {
      setConfirmFlowBusy(false);
    }
  }, [
    selectedItem,
    classification,
    confirmFlowBusy,
    selectedDestRootId,
    selectedRootPath,
    queryClient,
    runConfirm,
  ]);

  // Attribution project names for the picker.
  const { data: attributionProjects } = useQuery({
    queryKey: queryKeys.projects.all(),
    queryFn: async () => unwrap(await commands.projectsList(null)),
    enabled: pendingAttribution != null,
  });
  const attributionProjectNames = useMemo(() => {
    const out: Record<string, string> = {};
    for (const p of attributionProjects ?? []) out[p.id] = p.name;
    return out;
  }, [attributionProjects]);

  const handlePickAttribution = useCallback(
    async (chosen: ChosenAttributionRequest) => {
      const pending = pendingAttribution;
      if (!pending) return;
      await runConfirm(
        {
          inboxItemId: pending.itemId,
          rootAbsolutePath: pending.rootAbsolutePath,
        },
        pending.contentSignature,
        pending.rootId,
        chosen,
      );
    },
    [pendingAttribution, runConfirm],
  );

  const bulkEligibleItems = useMemo(
    () => items.filter((it) => it.state === 'classified'),
    [items],
  );
  const canBulkConfirm = bulkEligibleItems.length > 0 && !bulkConfirmLoading;

  const handleBulkConfirm = useCallback(async () => {
    if (bulkEligibleItems.length === 0) return;
    setBulkConfirmLoading(true);
    let successCount = 0;
    let failCount = 0;
    for (const it of bulkEligibleItems) {
      try {
        await confirm({
          inboxItemId: it.inboxItemId,
          contentSignature: it.contentSignature,
          rootAbsolutePath: it.rootAbsolutePath,
          destructiveDestination,
          rootId: null,
        });
        successCount += 1;
      } catch {
        failCount += 1;
      }
    }
    setBulkConfirmLoading(false);
    if (failCount > 0 && successCount > 0) {
      addToast({
        message: m.inbox_toast_bulk_partial({
          success: String(successCount),
          fail: String(failCount),
        }),
        variant: 'warn',
      });
    } else if (failCount > 0 && successCount === 0) {
      addToast({
        message: m.inbox_toast_bulk_all_need_review(),
        variant: 'warn',
      });
    } else {
      addToast({
        message: m.inbox_toast_bulk_confirmed({
          count: successCount,
        }),
        variant: 'info',
      });
    }
    refreshAll();
  }, [bulkEligibleItems, confirm, destructiveDestination, refreshAll]);

  const handlePickDestinationRoot = useCallback(
    async (rootId: string) => {
      if (!rootPickItemId || !selectedItem || !classification) return;
      if (selectedItem.inboxItemId !== rootPickItemId) return;
      await runConfirm(
        {
          inboxItemId: selectedItem.inboxItemId,
          rootAbsolutePath: selectedRootPath,
        },
        classification.contentSignature,
        rootId,
      );
    },
    [
      rootPickItemId,
      selectedItem,
      classification,
      selectedRootPath,
      runConfirm,
    ],
  );

  const hasOpenPlan = selectedItem?.state === 'plan_open';

  const canConfirm =
    !!selectedItem &&
    !!classification &&
    classification.type === 'single_type' &&
    !fileMetadataLoading &&
    !fileMetadataError &&
    !hasMissingRequiredMeta &&
    !hasOpenPlan;

  // Spec 027 FR-022: confirm from keyboard.
  useHotkeys(
    {
      KeyC: (e) => {
        if (!canConfirm || confirmLoading || confirmFlowBusy) return;
        e.preventDefault();
        void handleConfirm();
      },
    },
    [canConfirm, confirmLoading, confirmFlowBusy, handleConfirm],
  );

  return {
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
    clearPendingAttribution: () => setPendingAttribution(null),
    attributionProjectNames,
    absoluteByFromPath,
  };
}
