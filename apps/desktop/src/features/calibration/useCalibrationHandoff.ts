// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Hooks for the spec-062 US4 calibration candidate / handoff surfaces.
 *
 * All data goes through `calibrationHandoffIpc` (the feature-local seam) until
 * node ic9h.20 generates typed bindings. At that point, replace the seam calls
 * with `commands.*` and remove the ipc-boundary.guard.test.ts exemption.
 *
 * Hook summary:
 *   useCandidateList        — paginated CalibrationCandidateEvidence for one requirement
 *   useCalibrationHandoff   — CalibrationHandoffSnapshot for a handoff
 *   useHandoffOperation     — polls one CalibrationHandoffOperation (2-second interval
 *                             while verifying / cancelling; stops on terminal state)
 *   useCancelHandoff        — mutation: calibration.handoff.cancel
 *   useEquipmentResolution  — EquipmentResolution for a session
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '@/data/queryKeys';
import { errMessage } from '@/lib/errors';
import {
  calibrationCandidateList,
  calibrationHandoffQuery,
  calibrationHandoffOperationQuery,
  calibrationHandoffCancel,
  calibrationHandoffReviewedAdd,
  equipmentResolutionQuery,
} from './calibrationHandoffIpc';
import type {
  CalibrationRequirementDto,
  CalibrationCandidateEvidence,
  CalibrationHandoffSnapshot,
  CalibrationHandoffOperation,
  EquipmentResolution,
  Page,
} from './calibrationHandoffTypes';

// ── Candidate list ─────────────────────────────────────────────────────────────

export interface UseCandidateListState {
  page: Page<CalibrationCandidateEvidence> | undefined;
  loading: boolean;
  error: string | undefined;
  refetch: () => void;
}

/**
 * Load the first page of candidate evidence for one requirement.
 *
 * The query is disabled when no requirement is provided or when
 * `requiredRecipeEvidenceComplete` is false (blocked requirement —
 * no candidates can be returned).
 */
export function useCandidateList(
  requirement: CalibrationRequirementDto | undefined,
): UseCandidateListState {
  const enabled = requirement?.requiredRecipeEvidenceComplete;

  const { data, isFetching, error, refetch } = useQuery({
    queryKey: queryKeys.calibration.candidates(
      requirement?.requirementId ?? '__none__',
    ),
    queryFn: async () =>
      calibrationCandidateList({
        requirement: requirement as CalibrationRequirementDto,
        asOf: new Date().toISOString(),
        page: { limit: 50 },
      }),
    enabled,
  });

  return {
    page: data,
    loading: isFetching,
    error: error ? errMessage(error) : undefined,
    refetch: () => void refetch(),
  };
}

// ── Handoff snapshot ───────────────────────────────────────────────────────────

export interface UseCalibrationHandoffState {
  snapshot: CalibrationHandoffSnapshot | undefined;
  loading: boolean;
  error: string | undefined;
}

export function useCalibrationHandoff(
  handoffId: string | undefined,
  snapshotId?: string,
): UseCalibrationHandoffState {
  const { data, isFetching, error } = useQuery({
    queryKey: queryKeys.calibration.handoff(handoffId ?? '__none__'),
    queryFn: async () =>
      calibrationHandoffQuery({
        handoffId: handoffId as string,
        snapshotId,
      }),
    enabled: !!handoffId,
  });

  return {
    snapshot: data,
    loading: isFetching,
    error: error ? errMessage(error) : undefined,
  };
}

// ── Operation polling ─────────────────────────────────────────────────────────

const POLL_INTERVAL_MS = 2_000;

/** Terminal states — polling stops when reached. */
function isTerminal(state: CalibrationHandoffOperation['state']): boolean {
  return state === 'applied' || state === 'cancelled' || state === 'failed';
}

export interface UseHandoffOperationState {
  operation: CalibrationHandoffOperation | undefined;
  loading: boolean;
  error: string | undefined;
}

/**
 * Poll a CalibrationHandoffOperation at 2-second intervals while it is in a
 * non-terminal state (verifying or cancelling). Polling stops automatically
 * when the operation reaches applied / cancelled / failed.
 */
export function useHandoffOperation(
  operationId: string | undefined,
): UseHandoffOperationState {
  const { data, isFetching, error } = useQuery({
    queryKey: queryKeys.calibration.handoffOperation(operationId ?? '__none__'),
    queryFn: async () =>
      calibrationHandoffOperationQuery({
        operationId: operationId as string,
      }),
    enabled: !!operationId,
    refetchInterval: (query) => {
      const op = query.state.data;
      if (!op || isTerminal(op.state)) return false;
      return POLL_INTERVAL_MS;
    },
  });

  return {
    operation: data,
    loading: isFetching,
    error: error ? errMessage(error) : undefined,
  };
}

// ── Cancel mutation ────────────────────────────────────────────────────────────

export interface UseCancelHandoffState {
  cancelling: boolean;
  cancel: (
    operationId: string,
    commandId: string,
  ) => Promise<CalibrationHandoffOperation>;
}

export function useCancelHandoff(): UseCancelHandoffState {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: async ({
      operationId,
      commandId,
    }: {
      operationId: string;
      commandId: string;
    }) =>
      calibrationHandoffCancel({
        operationId,
        mutationContext: { commandId },
      }),
    onSuccess: (op) => {
      // Update the operation cache so polling picks up the cancelled state.
      void queryClient.setQueryData(
        queryKeys.calibration.handoffOperation(op.operationId),
        op,
      );
    },
  });

  const cancel = async (operationId: string, commandId: string) =>
    mutation.mutateAsync({ operationId, commandId });

  return {
    cancelling: mutation.isPending,
    cancel,
  };
}

// ── Reviewed add mutation ──────────────────────────────────────────────────────

export interface ReviewedAddArgs {
  handoffId: string;
  snapshotId: string;
  expectedHandoffHeadGeneration: number;
  sessionId: string;
  requirementId: string;
  expectedSnapshotBasisFingerprint: string;
  evidenceId: string;
  decisionReason: string;
  acknowledgedWarningCodes: string[];
  commandId: string;
}

export interface UseReviewedAddState {
  submitting: boolean;
  reviewedAdd: (
    args: ReviewedAddArgs,
  ) => Promise<{ operation: CalibrationHandoffOperation }>;
}

export function useReviewedAdd(): UseReviewedAddState {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: async (args: ReviewedAddArgs) => {
      const { commandId, ...rest } = args;
      return calibrationHandoffReviewedAdd({
        ...rest,
        mutationContext: { commandId },
      });
    },
    onSuccess: (_res, args) => {
      // Invalidate the handoff snapshot so callers reload the new head.
      void queryClient.invalidateQueries({
        queryKey: queryKeys.calibration.handoff(args.handoffId),
      });
    },
  });

  return {
    submitting: mutation.isPending,
    reviewedAdd: (args) => mutation.mutateAsync(args),
  };
}

// ── Equipment resolution ───────────────────────────────────────────────────────

export interface UseEquipmentResolutionState {
  resolution: EquipmentResolution | undefined;
  loading: boolean;
  error: string | undefined;
}

export function useEquipmentResolution(
  sessionId: string | undefined,
): UseEquipmentResolutionState {
  const { data, isFetching, error } = useQuery({
    queryKey: queryKeys.calibration.equipmentResolution(
      sessionId ?? '__none__',
    ),
    queryFn: async () =>
      equipmentResolutionQuery({ sessionId: sessionId as string }),
    enabled: !!sessionId,
  });

  return {
    resolution: data,
    loading: isFetching,
    error: error ? errMessage(error) : undefined,
  };
}
