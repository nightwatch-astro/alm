// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useSessionMaterializationFlow — approve → apply → async progress → cancel
 * lifecycle for the spec 062 inbox session materialization surface.
 *
 * Approval and apply are two distinct backend commands separated by digest
 * confirmation (contract: inbox-materialization.md). Progress is polled via
 * `session.materialization.progress.query` at a 500 ms cadence while an
 * operation is applying; the UI renders a live region from the returned state.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import {
  approveMaterialization,
  applyMaterialization,
  cancelMaterialization,
  queryMaterializationProgress,
} from './sessionMaterializationIpc';
import { materializationQueryKeys } from './useSessionMaterializationPlan';
import type {
  InboxMaterializationPlan,
  SessionMaterializationOperation,
  SessionMaterializationProgress,
} from './types';

/** Milliseconds between progress polls while an operation is running. */
const PROGRESS_POLL_MS = 500;

/** Operation states that are no longer live. */
const TERMINAL_STATES = new Set<SessionMaterializationProgress['state']>([
  'applied',
  'cancelled',
  'failed',
]);

export type FlowPhase =
  | 'idle'
  | 'approving'
  | 'applying'
  | 'cancelling'
  | 'applied'
  | 'cancelled'
  | 'failed';

export interface SessionMaterializationFlowState {
  phase: FlowPhase;
  /** Set once an operation is started. */
  operation: SessionMaterializationOperation | null;
  /** Live progress while applying or cancelling. */
  progress: SessionMaterializationProgress | null;
  /** Human-readable error for the most recent failure, if any. */
  errorCode: string | null;
}

export interface UseSessionMaterializationFlowResult {
  state: SessionMaterializationFlowState;
  /** Approve the plan and start applying it. `plan` must be `open`. */
  handleApprove: (plan: InboxMaterializationPlan) => Promise<void>;
  /** Request cancellation of the current operation. Safe to call at any time. */
  handleCancel: () => Promise<void>;
  /** Reset to idle (e.g. after dismissing the result view). */
  reset: () => void;
}

const IDLE_STATE: SessionMaterializationFlowState = {
  phase: 'idle',
  operation: null,
  progress: null,
  errorCode: null,
};

export function useSessionMaterializationFlow(
  planId: string | null,
  onApplied?: () => void,
): UseSessionMaterializationFlowResult {
  const queryClient = useQueryClient();
  const [state, setState] =
    useState<SessionMaterializationFlowState>(IDLE_STATE);
  const pollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const operationIdRef = useRef<string | null>(null);

  const stopPolling = useCallback(() => {
    if (pollTimerRef.current !== null) {
      clearInterval(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }, []);

  // Stop polling on unmount.
  useEffect(() => stopPolling, [stopPolling]);

  const startProgressPolling = useCallback(
    (operationId: string) => {
      stopPolling();
      pollTimerRef.current = setInterval(() => {
        void queryMaterializationProgress({ operationId })
          .then((progress) => {
            setState((prev) => {
              const isTerminal = TERMINAL_STATES.has(progress.state);
              if (isTerminal) {
                stopPolling();
                const phase: FlowPhase =
                  progress.state === 'applied'
                    ? 'applied'
                    : progress.state === 'cancelled'
                      ? 'cancelled'
                      : 'failed';
                if (phase === 'applied' && planId) {
                  void queryClient.invalidateQueries({
                    queryKey: materializationQueryKeys.plan(planId),
                  });
                  onApplied?.();
                }
                return { ...prev, phase, progress };
              }
              return {
                ...prev,
                phase:
                  progress.state === 'cancelling' ? 'cancelling' : 'applying',
                progress,
              };
            });
          })
          .catch(() => {
            // Transient poll failure — keep polling, do not freeze the UI.
          });
      }, PROGRESS_POLL_MS);
    },
    [stopPolling, queryClient, planId, onApplied],
  );

  const handleApprove = useCallback(
    async (plan: InboxMaterializationPlan) => {
      if (!planId || plan.state !== 'open') return;

      setState({ ...IDLE_STATE, phase: 'approving' });
      stopPolling();

      // Build a stable commandId for idempotency.
      const commandId = `approve-${plan.planId}-rev${plan.planRevision}-${Date.now()}`;

      let approvedDigest: string;
      try {
        const approveResp = await approveMaterialization({
          planId: plan.planId,
          expectedPlanRevision: plan.planRevision,
          expectedInputEvidenceRevision: plan.inputEvidenceRevision,
          // The digest of all resolution revisions is carried by the plan itself.
          expectedSiteResolutionRevisionsDigest: plan.canonicalPlanDigest,
          mutationContext: {
            commandId,
            approvalDigest: plan.canonicalPlanDigest,
          },
        });
        approvedDigest = approveResp.approvedPlanDigest;
      } catch (err) {
        const code = extractErrorCode(err);
        setState({ ...IDLE_STATE, phase: 'failed', errorCode: code });
        return;
      }

      // Approve succeeded — now apply.
      setState((prev) => ({ ...prev, phase: 'applying' }));
      const applyCommandId = `apply-${plan.planId}-rev${plan.planRevision}-${Date.now()}`;

      try {
        const applyResp = await applyMaterialization({
          planId: plan.planId,
          expectedPlanRevision: plan.planRevision,
          mutationContext: {
            commandId: applyCommandId,
            approvalDigest: approvedDigest,
          },
        });
        operationIdRef.current = applyResp.operation.operationId;
        setState((prev) => ({
          ...prev,
          phase: 'applying',
          operation: applyResp.operation,
        }));
        startProgressPolling(applyResp.operation.operationId);
      } catch (err) {
        const code = extractErrorCode(err);
        setState((prev) => ({ ...prev, phase: 'failed', errorCode: code }));
      }
    },
    [planId, stopPolling, startProgressPolling],
  );

  const handleCancel = useCallback(async () => {
    const operationId = operationIdRef.current;
    if (!operationId) return;
    const commandId = `cancel-${operationId}-${Date.now()}`;
    try {
      await cancelMaterialization({
        operationId,
        mutationContext: { commandId },
      });
      // The progress poll will pick up `cancelling → cancelled` from the backend.
    } catch {
      // Cancel is best-effort; the poll continues and will surface the final state.
    }
  }, []);

  const reset = useCallback(() => {
    stopPolling();
    operationIdRef.current = null;
    setState(IDLE_STATE);
  }, [stopPolling]);

  return { state, handleApprove, handleCancel, reset };
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function extractErrorCode(err: unknown): string {
  if (
    err != null &&
    typeof err === 'object' &&
    'code' in err &&
    typeof (err as { code: unknown }).code === 'string'
  ) {
    return (err as { code: string }).code;
  }
  return 'unknown';
}
