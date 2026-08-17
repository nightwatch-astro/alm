// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * All reclassify UI state for the Inbox detail pane (#994, extracted from
 * InboxDetail.tsx): the per-file override flow, the multi-select + bulk
 * override flow, the #611 heterogeneous-selection acknowledgement gate, and
 * the undo of a bulk frame-type override.
 *
 * Wraps `useInboxReclassifyV2` (the IPC call) and owns everything around it,
 * so the component keeps only rendering concerns.
 */

import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { commands } from '@/bindings/index';
import type {
  InboxFileMetadata_Serialize as InboxFileMetadata,
  InboxReclassifyV2Response_Serialize as InboxReclassifyV2Response,
  PropertyRegistryEntry_Serialize as PropertyRegistryEntry,
} from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { errMessage } from '@/lib/errors';
import type { InboxClassifyResponse } from './store';
import { useInboxReclassifyV2 } from './useInboxReclassifyV2';

export interface UseInboxReclassifyStateArgs {
  inboxItemId: string;
  rootAbsolutePath: string;
  sourceGroupId?: string | null;
  classification: InboxClassifyResponse | null;
  fileMetadata?: InboxFileMetadata[];
  onReclassified?: (response: InboxReclassifyV2Response) => void;
}

export function useInboxReclassifyState({
  inboxItemId,
  rootAbsolutePath,
  sourceGroupId,
  classification,
  fileMetadata,
  onReclassified,
}: UseInboxReclassifyStateArgs) {
  const { reclassifyV2, loading: reclassifyLoading } = useInboxReclassifyV2(
    inboxItemId,
    rootAbsolutePath,
    sourceGroupId,
  );

  // Per-file overrides pending submission (single-file flow).
  const [pendingOverrides, setPendingOverrides] = useState<
    Record<string, string>
  >({});
  const [applyError, setApplyError] = useState<string | null>(null);

  // T027: multi-select + bulk override state.
  const [selectedFiles, setSelectedFiles] = useState<Set<string>>(new Set());
  const [bulkFrameType, setBulkFrameType] = useState('');
  // Field-agnostic bulk values (spec 041 R-13/US11, issue #755): any
  // overridable property from `inbox.property_registry` keyed by its
  // registry `key` (e.g. "filter", "exposureS", "gain", "temperatureC").
  const [bulkPropValues, setBulkPropValues] = useState<Record<string, string>>(
    {},
  );
  const [bulkError, setBulkError] = useState<string | null>(null);

  // #611: acknowledgement gate for a HETEROGENEOUS bulk frame-type override
  // (the selection spans more than one currently-detected frame type).
  // Keyed by a signature of (selected files, chosen type) so the checkbox
  // un-acknowledges itself the instant either changes — an acknowledgement
  // must never silently carry over to a DIFFERENT selection/value.
  const [heterogeneousAckKey, setHeterogeneousAckKey] = useState<string | null>(
    null,
  );
  // #611: last bulk frame-type override applied, so the user can undo it —
  // restores each file's PRE-OVERRIDE detected frame type via a per-file
  // `overrides` call (never a bulk one — the prior values are heterogeneous
  // by construction here). Files that had no prior detected type (genuinely
  // unclassified) are omitted: there is nothing valid to restore them to.
  const [lastFrameTypeUndo, setLastFrameTypeUndo] = useState<{
    count: number;
    overrides: Array<{ filePath: string; properties: { frameType: string } }>;
  } | null>(null);
  const [undoLoading, setUndoLoading] = useState(false);
  const [undoError, setUndoError] = useState<string | null>(null);

  // Property registry (FR-044) — drives the generic bulk editor below.
  // Static per app session, so fetched lazily (only once the bulk editor can
  // actually be shown) and cached indefinitely.
  const { data: propertyRegistry } = useQuery<PropertyRegistryEntry[]>({
    queryKey: ['inbox', 'propertyRegistry'],
    queryFn: async () => unwrap(await commands.inboxPropertyRegistry()),
    enabled: selectedFiles.size > 0,
    staleTime: Number.POSITIVE_INFINITY,
  });

  const handleOverrideChange = (filePath: string, frameType: string) => {
    setPendingOverrides((prev) => ({ ...prev, [filePath]: frameType }));
  };

  const handleApplyOverrides = async () => {
    const overrides = Object.entries(pendingOverrides).map(
      ([filePath, frameType]) => ({
        filePath,
        properties: { frameType },
      }),
    );
    if (overrides.length === 0) return;
    setApplyError(null);
    try {
      const result = await reclassifyV2({ overrides });
      setPendingOverrides({});
      onReclassified?.(result);
    } catch (err) {
      setApplyError(errMessage(err));
    }
  };

  // T027 selection helpers.
  //
  // #1114: the needs-review set is every file that still blocks promotion, NOT
  // just the ones with no frame type. `classification.unclassifiedFiles` drops
  // a file the instant a frame-type override is written (classify.rs: the
  // `unclassified != 0 && manual_override.is_none()` filter), which used to
  // unmount this whole section — including the generic property editor — while
  // the file was still blocked on a mandatory attribute like exposureS. The
  // per-file `missingMandatory` set (metadata.rs `compute_missing_mandatory`,
  // override-applied and re-fetched after every reclassify) is the truthful
  // gate. Unioned rather than swapped because per-file metadata can be absent
  // or still loading, and losing the frame-type affordance then would be the
  // same class of bug in the other direction.
  const filesNeedingReview = useMemo(() => {
    const paths = new Set(classification?.unclassifiedFiles ?? []);
    for (const f of fileMetadata ?? []) {
      if ((f.missingMandatory?.length ?? 0) > 0) paths.add(f.relativeFilePath);
    }
    return Array.from(paths);
  }, [classification?.unclassifiedFiles, fileMetadata]);

  const handleToggleFile = (filePath: string) => {
    setSelectedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(filePath)) next.delete(filePath);
      else next.add(filePath);
      return next;
    });
  };

  const handleSelectAll = () => {
    if (selectedFiles.size === filesNeedingReview.length)
      setSelectedFiles(new Set());
    else setSelectedFiles(new Set(filesNeedingReview));
  };

  const handleBulkPropChange = (key: string, value: string) => {
    setBulkPropValues((prev) => ({ ...prev, [key]: value }));
  };

  // #611: the currently-detected frame type for each selected file, keyed by
  // path, sourced from the per-file metadata table (not the classification
  // response — that only lists WHICH files are unclassified, not what each
  // one's own already-detected type is). Used to warn before a bulk override
  // silently overwrites a heterogeneous selection.
  const selectedDetectedTypes = new Map<string, string | null>();
  for (const fp of selectedFiles) {
    const meta = fileMetadata?.find((f) => f.relativeFilePath === fp);
    selectedDetectedTypes.set(fp, meta?.frameTypeEffective ?? null);
  }
  const distinctSelectedTypes = new Set(
    Array.from(selectedDetectedTypes.values()).filter(
      (t): t is string => t != null,
    ),
  );
  const isHeterogeneousFrameTypeBulk =
    bulkFrameType !== '' && distinctSelectedTypes.size > 1;
  const heterogeneousSignature = isHeterogeneousFrameTypeBulk
    ? `${bulkFrameType}::${Array.from(selectedFiles).sort().join(',')}`
    : null;
  const heterogeneousAcked =
    !isHeterogeneousFrameTypeBulk ||
    heterogeneousAckKey === heterogeneousSignature;

  // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: bulk apply guard chain then per-selection reclassify dispatch
  const handleBulkApply = async () => {
    if (selectedFiles.size === 0) return;
    if (isHeterogeneousFrameTypeBulk && !heterogeneousAcked) return;
    const filePaths = Array.from(selectedFiles);
    const bulk: Array<{
      property: string;
      value: unknown;
      filePaths: string[];
    }> = [];
    if (bulkFrameType !== '') {
      bulk.push({ property: 'frameType', value: bulkFrameType, filePaths });
    }
    for (const [key, raw] of Object.entries(bulkPropValues)) {
      if (raw === '') continue;
      const entry = propertyRegistry?.find((e) => e.key === key);
      const isNumeric = entry?.kind === 'number' || entry?.kind === 'integer';
      const value = isNumeric ? Number(raw) : raw;
      if (isNumeric && Number.isNaN(value)) continue;
      bulk.push({ property: key, value, filePaths });
    }
    if (bulk.length === 0) return;
    setBulkError(null);
    setUndoError(null);
    // #611: snapshot each selected file's PRE-OVERRIDE detected frame type
    // before applying, so a bad bulk override is recoverable. Only captured
    // when this call actually changes frameType, and only for files that had
    // a known prior type (nothing valid to restore an unclassified file to).
    const undoOverrides =
      bulkFrameType !== ''
        ? filePaths
            .map((fp) => {
              const prev = selectedDetectedTypes.get(fp);
              return prev
                ? { filePath: fp, properties: { frameType: prev } }
                : null;
            })
            .filter(
              (
                o,
              ): o is { filePath: string; properties: { frameType: string } } =>
                o != null,
            )
        : [];
    try {
      const result = await reclassifyV2({ bulk });
      setSelectedFiles(new Set());
      setBulkFrameType('');
      setBulkPropValues({});
      setHeterogeneousAckKey(null);
      if (undoOverrides.length > 0) {
        setLastFrameTypeUndo({
          count: undoOverrides.length,
          overrides: undoOverrides,
        });
      }
      onReclassified?.(result);
    } catch (err) {
      setBulkError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleUndoBulkFrameType = async () => {
    if (!lastFrameTypeUndo) return;
    setUndoLoading(true);
    setUndoError(null);
    try {
      const result = await reclassifyV2({
        overrides: lastFrameTypeUndo.overrides,
      });
      setLastFrameTypeUndo(null);
      onReclassified?.(result);
    } catch (err) {
      setUndoError(errMessage(err));
    } finally {
      setUndoLoading(false);
    }
  };

  return {
    reclassifyLoading,
    filesNeedingReview,
    propertyRegistry,
    // per-file override flow
    pendingOverrides,
    applyError,
    handleOverrideChange,
    handleApplyOverrides,
    // selection + bulk flow
    selectedFiles,
    bulkFrameType,
    setBulkFrameType,
    bulkPropValues,
    bulkError,
    handleToggleFile,
    handleSelectAll,
    handleBulkPropChange,
    handleBulkApply,
    // #611 heterogeneous gate + undo
    isHeterogeneousFrameTypeBulk,
    heterogeneousSignature,
    heterogeneousAcked,
    setHeterogeneousAckKey,
    lastFrameTypeUndo,
    undoLoading,
    undoError,
    handleUndoBulkFrameType,
  };
}

export type UseInboxReclassifyState = ReturnType<
  typeof useInboxReclassifyState
>;
