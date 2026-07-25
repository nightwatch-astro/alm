// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * usePlanApplyProgress — live plan-apply progress consumer (spec 042 US16 /
 * FR-021).
 *
 * Drives the `applyPlan` wrapper with an `onEvent` subscriber and reduces the
 * streamed `OperationEvent`s (Started → per-item → terminal) into a small,
 * render-friendly progress state. This is the end-to-end consumer of the
 * `tauri::ipc::Channel<OperationEvent>` long-operation contract: the backend
 * (or the mock IPC) emits per-item events as the filesystem plan is applied and
 * this hook surfaces a live item counter + terminal outcome to the UI.
 *
 * DS-16 throttle: high-frequency `item_applied` / `item_failed` / `progress`
 * events are accumulated in a ref and flushed on rAF (or a 200 ms fallback
 * when rAF is unavailable) to keep React re-renders below one per animation
 * frame during large plan applies.  Terminal and warning events always flush
 * immediately.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { applyPlan } from './planApply';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type {
  OperationEvent,
  PlanApplyResponse,
  PlanApplyStatus_Serialize as PlanApplyStatus,
} from '@/bindings/index';

/** `PlanApplyStatus.planState` values that mean the run is no longer active. */
const TERMINAL_PLAN_STATES = new Set([
  'applied',
  'partially_applied',
  'failed',
  'cancelled',
  'discarded',
]);

/** Poll interval for `plans.apply.status` while a resumed run has no live
 * event channel (see `resume` below). Matches the Inbox review overlay's
 * own open-plan poll cadence (InboxPage.tsx) for one consistent rhythm. */
const RESUME_POLL_MS = 1000;

/** Flush interval cap when rAF is unavailable (e.g. background tabs). */
const RAF_FALLBACK_MS = 200;

export interface PlanApplyProgress {
  /** Whether an apply is currently streaming. */
  running: boolean;
  /** Items applied so far (counts `item_applied` events). */
  applied: number;
  /** Items failed so far (counts `item_failed` events). */
  failed: number;
  /** Total items, once the `item_started` lead event reports it (else null). */
  total: number | null;
  /** Terminal outcome once the stream completes, else null. */
  terminal: 'completed' | 'failed' | null;
  /** The most recent streamed event type, for fine-grained UI. */
  lastEventType: OperationEvent['eventType'] | null;
  /**
   * Whether the run has halted on a pause condition (R-Pause-1: volume
   * unavailable, disk full, or a stale source file). Derived from the
   * backend's `warning` long-op event, which carries `pauseReason`/`runId`
   * (spec 042 US16). `plan.resume` re-validates the condition server-side;
   * this hook only reflects what the backend reports.
   */
  paused: boolean;
  /** Human-readable pause reason as reported by the backend, else null. */
  pauseReason: string | null;
  /** Run id needed to call `plan.resume`; set once a pause or the initial
   * apply response reports one. */
  runId: string | null;
  /**
   * `true` immediately after a successful `plan.resume` call, until the
   * first `plans.apply.status` poll (below) lands. Issue #575 (backend
   * `spawn_executor_run` shared by apply and resume) is fixed — the run DOES
   * continue — but `plan.resume`'s response carries no event channel, so
   * there is nothing to await for a "first real update" the way `run()` has.
   * This is a brief, honest "reconnecting" placeholder, not a permanent
   * dead-end: `running` flips true and polling starts in the same tick (see
   * `resume` below).
   */
  resumeStalled: boolean;
}

const IDLE: PlanApplyProgress = {
  running: false,
  applied: 0,
  failed: 0,
  total: null,
  terminal: null,
  lastEventType: null,
  paused: false,
  pauseReason: null,
  runId: null,
  resumeStalled: false,
};

/** Extract a numeric `itemsTotal` from an event payload when present. */
function readItemsTotal(payload: unknown): number | null {
  if (payload && typeof payload === 'object' && 'itemsTotal' in payload) {
    const v = (payload as { itemsTotal?: unknown }).itemsTotal;
    return typeof v === 'number' ? v : null;
  }
  return null;
}

/** Extract a string field from an event payload when present. */
function readStringField(payload: unknown, field: string): string | null {
  if (payload && typeof payload === 'object' && field in payload) {
    const v = (payload as Record<string, unknown>)[field];
    return typeof v === 'string' ? v : null;
  }
  return null;
}

/**
 * Pending delta accumulated between rAF flushes (DS-16).
 * Carries ONLY the counters that can batch safely; flags that need immediate
 * attention (paused, terminal) always trigger a synchronous flush.
 */
interface PendingDelta {
  appliedDelta: number;
  failedDelta: number;
  lastEventType: OperationEvent['eventType'] | null;
}

export function usePlanApplyProgress() {
  const [progress, setProgress] = useState<PlanApplyProgress>(IDLE);
  // Issue #744 (FR-002): `plan.resume` returns no event channel, so a
  // resumed run's ongoing progress (which, since #575, genuinely continues)
  // is only observable by polling `plans.apply.status`. Ref (not state) so
  // `stopResumePolling` is stable across renders and effect cleanup always
  // sees the current timer.
  const pollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  // DS-16: accumulated counter delta between rAF flushes.
  const pendingRef = useRef<PendingDelta | null>(null);
  // rAF / fallback timer handle for the throttle flush.
  const flushHandleRef = useRef<number | ReturnType<typeof setTimeout> | null>(
    null,
  );

  const stopResumePolling = useCallback(() => {
    if (pollTimerRef.current !== null) {
      clearInterval(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }, []);

  // Cancel any pending rAF/timeout flush and clear the delta on unmount or
  // reset so stale deltas never land after a new run starts.
  const cancelPendingFlush = useCallback(() => {
    if (flushHandleRef.current !== null) {
      if (typeof flushHandleRef.current === 'number') {
        cancelAnimationFrame(flushHandleRef.current);
      } else {
        clearTimeout(flushHandleRef.current);
      }
      flushHandleRef.current = null;
    }
    pendingRef.current = null;
  }, []);

  // Stop polling if the component unmounts mid-resume (e.g. the overlay
  // closes) — never let an interval outlive its hook instance.
  useEffect(
    () => () => {
      stopResumePolling();
      cancelPendingFlush();
    },
    [stopResumePolling, cancelPendingFlush],
  );

  const reset = useCallback(() => {
    stopResumePolling();
    cancelPendingFlush();
    setProgress(IDLE);
  }, [stopResumePolling, cancelPendingFlush]);

  /** Flush the pending delta into React state. */
  const flushPending = useCallback(() => {
    flushHandleRef.current = null;
    const delta = pendingRef.current;
    pendingRef.current = null;
    if (!delta) return;
    setProgress((prev) => ({
      ...prev,
      applied: prev.applied + delta.appliedDelta,
      failed: prev.failed + delta.failedDelta,
      lastEventType: delta.lastEventType ?? prev.lastEventType,
    }));
  }, []);

  /** Schedule a rAF flush if one is not already queued. */
  const scheduleFlush = useCallback(() => {
    if (flushHandleRef.current !== null) return;
    if (typeof requestAnimationFrame !== 'undefined') {
      flushHandleRef.current = requestAnimationFrame(flushPending);
    } else {
      flushHandleRef.current = setTimeout(flushPending, RAF_FALLBACK_MS);
    }
  }, [flushPending]);

  const run = useCallback(
    async (args: {
      id: string;
      approvalToken?: string;
    }): Promise<PlanApplyResponse | null> => {
      stopResumePolling();
      cancelPendingFlush();
      setProgress({ ...IDLE, running: true });
      try {
        const response = await applyPlan({
          id: args.id,
          approvalToken: args.approvalToken,
          onEvent: (event: OperationEvent) => {
            switch (event.eventType) {
              case 'item_started': {
                // item_started is infrequent and carries the total count —
                // flush any pending delta immediately, then apply.
                flushPending();
                const total = readItemsTotal(event.payload);
                const runId = readStringField(event.payload, 'runId');
                setProgress((prev) => ({
                  ...prev,
                  lastEventType: event.eventType,
                  total: total ?? prev.total,
                  runId: runId ?? prev.runId,
                }));
                break;
              }

              case 'progress': {
                // Batch progress tick from the group-commit flush (kyo7.52).
                // Accumulate in the delta ref; flush on next rAF.
                const p = event.payload as Record<string, unknown>;
                const delta = pendingRef.current ?? {
                  appliedDelta: 0,
                  failedDelta: 0,
                  lastEventType: null,
                };
                if (typeof p.itemsApplied === 'number')
                  delta.appliedDelta += p.itemsApplied;
                delta.lastEventType = event.eventType;
                pendingRef.current = delta;
                scheduleFlush();
                break;
              }

              case 'item_applied': {
                const delta = pendingRef.current ?? {
                  appliedDelta: 0,
                  failedDelta: 0,
                  lastEventType: null,
                };
                delta.appliedDelta += 1;
                delta.lastEventType = event.eventType;
                pendingRef.current = delta;
                scheduleFlush();
                break;
              }

              case 'item_failed': {
                const delta = pendingRef.current ?? {
                  appliedDelta: 0,
                  failedDelta: 0,
                  lastEventType: null,
                };
                delta.failedDelta += 1;
                delta.lastEventType = event.eventType;
                pendingRef.current = delta;
                scheduleFlush();
                break;
              }

              case 'warning': {
                // Pause condition (R-Pause-1): always flush immediately so the
                // UI reflects the pause before the next animation frame.
                flushPending();
                const pauseReason = readStringField(
                  event.payload,
                  'pauseReason',
                );
                const runId = readStringField(event.payload, 'runId');
                setProgress((prev) => ({
                  ...prev,
                  lastEventType: event.eventType,
                  paused: true,
                  pauseReason,
                  runId: runId ?? prev.runId,
                }));
                break;
              }

              case 'completed':
              case 'failed': {
                // Terminal: flush pending counts, then set terminal state.
                flushPending();
                setProgress((prev) => ({
                  ...prev,
                  lastEventType: event.eventType,
                  running: false,
                  terminal:
                    event.eventType === 'completed' ? 'completed' : 'failed',
                }));
                break;
              }

              default:
                break;
            }
          },
        });
        // Flush any remaining delta and ensure running clears even when no
        // terminal event arrived.
        flushPending();
        setProgress((prev) =>
          prev.running ? { ...prev, running: false } : prev,
        );
        return response;
      } catch {
        flushPending();
        setProgress((prev) => ({
          ...prev,
          running: false,
          terminal: 'failed',
        }));
        return null;
      }
    },
    [stopResumePolling, cancelPendingFlush, flushPending, scheduleFlush],
  );

  /**
   * Reduce one `plans.apply.status` poll tick into `progress`. Returns
   * whether the plan has reached a terminal state (poll should stop).
   */
  const applyStatusTick = useCallback((status: PlanApplyStatus) => {
    const isTerminal = TERMINAL_PLAN_STATES.has(status.planState);
    setProgress((prev) => ({
      ...prev,
      running: !isTerminal,
      applied: status.itemsApplied,
      failed: status.itemsFailed,
      total: status.itemsTotal,
      runId: status.runId ?? prev.runId,
      resumeStalled: false,
      paused: status.planState === 'paused',
      pauseReason:
        status.planState === 'paused' ? (status.pauseReason ?? null) : null,
      terminal: isTerminal
        ? status.planState === 'applied'
          ? 'completed'
          : 'failed'
        : null,
    }));
    return isTerminal;
  }, []);

  /**
   * Resume a paused run (`plan.resume`, R-Pause-1). Calls the real backend
   * command; the plan's DB state moves `paused -> applying` on success, and
   * (since issue #575's `spawn_executor_run` fix) the executor genuinely
   * continues applying the run's remaining items.
   *
   * `plan.resume`'s response carries no event channel (unlike the initial
   * `run()`), so there is nothing to await for live `item_*` events. Instead
   * poll `plans.apply.status` (issue #744 FR-002) — a DB-backed snapshot
   * independent of any channel — until the plan reaches a terminal state.
   */
  const resume = useCallback(
    async (planId: string): Promise<boolean> => {
      if (progress.runId === null) return false;
      try {
        unwrap(await commands.plansResume(planId, progress.runId));
        setProgress((prev) => ({
          ...prev,
          running: true,
          paused: false,
          pauseReason: null,
          resumeStalled: true,
        }));
        stopResumePolling();
        pollTimerRef.current = setInterval(() => {
          void commands
            .plansApplyStatus(planId)
            .then(unwrap)
            .then((status) => {
              if (applyStatusTick(status)) stopResumePolling();
            })
            .catch(() => {
              // A transient status-poll failure is not a terminal outcome —
              // keep polling rather than freezing the UI on a dropped tick.
            });
        }, RESUME_POLL_MS);
        return true;
      } catch {
        return false;
      }
    },
    [progress.runId, applyStatusTick, stopResumePolling],
  );

  /**
   * Cancel an in-flight apply run (`plan.cancel`, US3/FR-009). Only needs the
   * plan id — the backend signals the shared `CancellationToken` for the
   * plan's currently-registered run, no `run_id` required. The channel
   * already streaming this run's progress will still deliver the resulting
   * terminal `completed` event (backend-emitted for a cancelled run too), so
   * this function does not itself flip `progress` — the reducer above
   * self-updates from that event.
   */
  const cancel = useCallback(async (planId: string): Promise<boolean> => {
    try {
      unwrap(await commands.plansCancel(planId));
      return true;
    } catch {
      return false;
    }
  }, []);

  return { progress, run, resume, cancel, reset };
}
