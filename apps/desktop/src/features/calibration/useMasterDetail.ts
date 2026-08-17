// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useMasterDetail — stateful data + action layer for MasterDetail (spec 007,
 * spec 043, #642, #886).
 *
 * Extracted from MasterDetail.tsx (refactor sweep kyo7.104) so the component
 * stays render-only. Owns: matching context, suggest/assign flows, archive
 * generation + confirm gate, detail fetch + session-name resolution, and the
 * reveal path derivation.
 */

import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '@/data/queryKeys';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type {
  CalibrationMaster_Serialize as CalibrationMaster,
  CalibrationMatchMissingFlag,
} from '@/bindings/index';
import { m } from '@/lib/i18n';
import { errMessage } from '@/lib/errors';
import { addToast } from '@/shared/toast';
import { useInventorySources } from '@/features/sessions/store';
import { isSourceActionable } from '@/features/sessions/connectivity';
import {
  resolveRevealPath,
  revealInventoryPath,
} from '@/features/sessions/revealInventory';
import {
  useCalibrationAssign,
  useCalibrationSuggest,
  useGenerateMasterArchivePlan,
  useInvalidateCalibrationMaster,
} from './useCalibration';

// ── DetailState ───────────────────────────────────────────────────────────────

/** spec 048 US5 (FR-024/025): distinct wording per trigger path. */
function missingFlagLabel(flag: CalibrationMatchMissingFlag): string {
  switch (flag) {
    case 'master_missing':
      return m.calibration_flag_master_missing();
    case 'source_subs_missing':
      return m.calibration_flag_source_subs_missing();
  }
}

export interface DetailState {
  confirmedNames: string[];
  compatibleNames: string[];
  loading: boolean;
  /** spec 048 US5 (FR-024/025): derived "missing" flag from `calibrationMastersGet`. */
  missingFlag: CalibrationMatchMissingFlag | null;
  /** Pre-formatted label for `missingFlag`; null when missingFlag is null. */
  missingFlagLabel: string | null;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export interface MasterDetailState {
  detail: DetailState;
  /** Matching context for MatchCandidatesPanel (first usedBySessionId). */
  matchSessionId: string | undefined;
  suggestResponse: ReturnType<typeof useCalibrationSuggest>['response'];
  suggestLoading: ReturnType<typeof useCalibrationSuggest>['loading'];
  suggestError: ReturnType<typeof useCalibrationSuggest>['error'];
  assigning: boolean;
  /** Archive plan modal state: planId when the overlay is open, null otherwise. */
  archiveReviewPlanId: string | null;
  setArchiveReviewPlanId: (id: string | null) => void;
  inUseConfirmOpen: boolean;
  setInUseConfirmOpen: (open: boolean) => void;
  /** Archive-plan mutation pending (disable trigger button). */
  archivePending: boolean;
  /** Reveal target absolute path, or `undefined` when unresolvable (disabled). */
  revealTarget: string | undefined;
  revealActionable: boolean;
  handleAssign: (
    masterId: string,
    override: boolean,
  ) => Promise<{
    status: string;
    error?: {
      code: string;
      message: string;
      details?: { dimensions: string[] };
    };
  }>;
  handleReveal: () => Promise<void>;
  handleArchive: () => void;
  handleConfirmArchiveInUse: () => void;
  handleArchivePlanApplied: () => void;
}

export function useMasterDetail(
  master: CalibrationMaster | null,
): MasterDetailState {
  const matchSessionId = master?.usedBySessionIds[0];
  const {
    response: suggestResponse,
    loading: suggestLoading,
    error: suggestError,
    refresh: refreshSuggest,
  } = useCalibrationSuggest(matchSessionId);
  const { assigning, assign } = useCalibrationAssign();

  const handleAssign = async (masterId: string, override: boolean) => {
    if (!matchSessionId) {
      return {
        status: 'error',
        error: {
          code: 'no_session',
          message: m.calibration_compatible_sessions_no_anchor_desc(),
        },
      };
    }
    const res = await assign(matchSessionId, masterId, override);
    if (res.status === 'success') refreshSuggest();
    // Normalize: the real response types `error`/`details` as `| null`, while
    // the panel's prop type only allows the object or `undefined`.
    return {
      status: res.status,
      error: res.error
        ? {
            code: res.error.code,
            message: res.error.message,
            details: res.error.details ?? undefined,
          }
        : undefined,
    };
  };

  const masterId = master?.id;
  const masterDetailQuery = useQuery({
    queryKey: queryKeys.calibration.master(masterId ?? '__none__'),
    queryFn: async () =>
      unwrap(await commands.calibrationMastersGet(masterId as string)),
    enabled: !!masterId,
  });
  const sessionsQuery = useQuery({
    queryKey: queryKeys.sessions.all(),
    queryFn: async () => unwrap(await commands.sessionsList(null, null)),
  });
  // #642: shares the same inventory-sources query SessionsPage's Reveal
  // action reads (`queryKeys.inventory.all`) — no private fetch.
  const sourcesQuery = useInventorySources();

  const generateArchivePlan = useGenerateMasterArchivePlan();
  const invalidateMaster = useInvalidateCalibrationMaster();
  const [archiveReviewPlanId, setArchiveReviewPlanId] = useState<string | null>(
    null,
  );
  const [inUseConfirmOpen, setInUseConfirmOpen] = useState(false);

  const detail: DetailState = useMemo(() => {
    const empty: DetailState = {
      confirmedNames: [],
      compatibleNames: [],
      loading: false,
      missingFlag: null,
      missingFlagLabel: null,
    };
    if (!masterId) return empty;
    if (masterDetailQuery.isFetching || sessionsQuery.isFetching) {
      return { ...empty, loading: true };
    }
    if (
      masterDetailQuery.error ||
      sessionsQuery.error ||
      !masterDetailQuery.data
    ) {
      return empty;
    }
    const masterDetail = masterDetailQuery.data;
    const idToName = new Map<string, string>();
    const sessionsList = Array.isArray(sessionsQuery.data)
      ? sessionsQuery.data
      : [];
    for (const s of sessionsList) {
      const k = s.sessionKey;
      idToName.set(s.id, `${k.target} · ${k.filter} · ${k.night}`);
    }
    const flag = masterDetail.missingFlag ?? null;
    return {
      confirmedNames: masterDetail.usedBySessionIds
        .map((id) => idToName.get(id) ?? id)
        .filter(Boolean),
      compatibleNames: masterDetail.compatibleSessions
        .map((e) => idToName.get(e.sessionId) ?? e.sessionId)
        .filter(Boolean),
      loading: false,
      missingFlag: flag,
      missingFlagLabel: flag ? missingFlagLabel(flag) : null,
    };
  }, [
    masterId,
    masterDetailQuery.data,
    masterDetailQuery.isFetching,
    masterDetailQuery.error,
    sessionsQuery.data,
    sessionsQuery.isFetching,
    sessionsQuery.error,
  ]);

  // #642: resolve the master's absolute file location.
  const revealSource = sourcesQuery.data?.sources.find(
    (s) => s.id === master?.rootId,
  );
  const revealTarget =
    master?.rootId != null &&
    master.relativePath != null &&
    revealSource != null
      ? resolveRevealPath(revealSource.path, master.relativePath)
      : undefined;
  const revealActionable =
    revealTarget != null &&
    revealSource != null &&
    isSourceActionable(revealSource.state);

  const handleReveal = async () => {
    if (!revealTarget || !master) return;
    try {
      await revealInventoryPath({ path: revealTarget, sessionId: master.id });
    } catch {
      addToast({
        message: m.common_reveal_error(),
        variant: 'error',
      });
    }
  };

  const handleArchiveSuccess = (res: { planId: string; itemCount: number }) => {
    addToast({
      message: m.calibration_archive_plan_created_toast({
        count: res.itemCount,
      }),
      variant: 'info',
    });
    setArchiveReviewPlanId(res.planId);
  };

  const handleArchive = () => {
    if (!master) return;
    generateArchivePlan.mutate(
      { masterId: master.id },
      {
        onSuccess: handleArchiveSuccess,
        onError: (err) => {
          if (err.code === 'calibration.master_in_use') {
            setInUseConfirmOpen(true);
            return;
          }
          addToast({
            message: errMessage(err),
            variant: 'error',
          });
        },
      },
    );
  };

  const handleConfirmArchiveInUse = () => {
    if (!master) return;
    setInUseConfirmOpen(false);
    generateArchivePlan.mutate(
      { masterId: master.id, confirmInUse: true },
      {
        onSuccess: handleArchiveSuccess,
        onError: (err) => {
          addToast({
            message: errMessage(err),
            variant: 'error',
          });
        },
      },
    );
  };

  const handleArchivePlanApplied = () => {
    if (!master) return;
    invalidateMaster(master.id);
  };

  return {
    detail,
    matchSessionId,
    suggestResponse,
    suggestLoading,
    suggestError,
    assigning,
    archiveReviewPlanId,
    setArchiveReviewPlanId,
    inUseConfirmOpen,
    setInUseConfirmOpen,
    archivePending: generateArchivePlan.isPending,
    revealTarget,
    revealActionable,
    handleAssign,
    handleReveal,
    handleArchive,
    handleConfirmArchiveInUse,
    handleArchivePlanApplied,
  };
}
