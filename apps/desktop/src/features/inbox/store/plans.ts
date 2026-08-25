// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Inbox store — plan lifecycle hooks (fetch, apply, apply-all, cancel,
 * open-plans aggregate, stats).
 */

import { useCallback, useEffect, useState } from 'react';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type {
  InboxPlanView,
  InboxOpenPlan,
  InboxOpenPlansResponse,
  InboxPlanAction,
  PlanApplyResponse,
  InboxApplyAllResponse,
  InboxPlanCancelResponse,
  InboxStatsResponse,
  InboxStatsPerType,
  InboxStatsTotals,
} from '@/bindings/index';

export type {
  InboxPlanView,
  InboxOpenPlan,
  InboxOpenPlansResponse,
  InboxPlanAction,
  PlanApplyResponse,
  InboxApplyAllResponse,
  InboxPlanCancelResponse,
  InboxStatsResponse,
  InboxStatsPerType,
  InboxStatsTotals,
};

// ── Inbox plan surface (spec 041) ─────────────────────────────────────────────

interface PlanState {
  plan: InboxPlanView | null;
  loading: boolean;
  error: string | null;
}

/**
 * Fetch + hold the open plan for the currently selected inbox item.
 * Pass an empty string to skip the fetch (no item selected / no plan).
 */
export function useInboxPlan(inboxItemId: string) {
  const [state, setState] = useState<PlanState>({
    plan: null,
    loading: false,
    error: null,
  });

  const fetchPlan = useCallback(async () => {
    if (!inboxItemId) {
      setState({ plan: null, loading: false, error: null });
      return;
    }
    setState((s) => ({ ...s, loading: true, error: null }));
    try {
      const plan = unwrap(await commands.inboxPlan(inboxItemId));
      setState({ plan, loading: false, error: null });
    } catch (e: unknown) {
      const msg = String(e);
      // 'no_plan' is expected when the item was just confirmed and listener
      // hasn't fired yet, or when the item is not in plan_open state.
      if (
        msg.includes('inbox.item.no_plan') ||
        msg.includes('inbox.item.not_found')
      ) {
        setState({ plan: null, loading: false, error: null });
      } else {
        setState({ plan: null, loading: false, error: msg });
      }
    }
  }, [inboxItemId]);

  return { ...state, fetchPlan };
}

interface PlanApplyState {
  loading: boolean;
  error: string | null;
}

/** Apply all plans currently in `plan_open` state. */
export function useInboxPlanApplyAll() {
  const [state, setState] = useState<PlanApplyState>({
    loading: false,
    error: null,
  });

  const applyAll =
    useCallback(async (): Promise<InboxApplyAllResponse | null> => {
      setState({ loading: true, error: null });
      try {
        const result = unwrap(await commands.inboxPlanApplyAll());
        setState({ loading: false, error: null });
        return result;
      } catch (e: unknown) {
        setState({ loading: false, error: String(e) });
        return null;
      }
    }, []);

  return { ...state, applyAll };
}

/** Cancel the open plan for a single inbox item, resetting it to `classified`. */
export function useInboxPlanCancel() {
  const [state, setState] = useState<PlanApplyState>({
    loading: false,
    error: null,
  });

  const cancel = useCallback(
    async (inboxItemId: string): Promise<InboxPlanCancelResponse | null> => {
      setState({ loading: true, error: null });
      try {
        const result = unwrap(await commands.inboxPlanCancel(inboxItemId));
        setState({ loading: false, error: null });
        return result;
      } catch (e: unknown) {
        setState({ loading: false, error: String(e) });
        return null;
      }
    },
    [],
  );

  return { ...state, cancel };
}

// ── Aggregate open-plans surface (spec 041, US2) ──────────────────────────────

export interface OpenPlansState {
  data: InboxOpenPlansResponse | null;
  loading: boolean;
  error: string | null;
}

/**
 * Load and cache the cross-root aggregate of every open inbox plan.
 *
 * Mirrors `useInboxList`: a useState + useEffect + cancelled-flag pattern keyed
 * by a monotonic `epoch`. Call `refresh()` to re-fetch (e.g. after an
 * apply/cancel/confirm mutation).
 */
export function useOpenInboxPlans() {
  const [epoch, setEpoch] = useState(0);
  const [state, setState] = useState<OpenPlansState>({
    data: null,
    loading: true,
    error: null,
  });

  useEffect(() => {
    let cancelled = false;
    setState((s) => ({ ...s, loading: true, error: null }));
    commands
      .inboxPlanListOpen()
      .then(unwrap)
      .then((resp) => {
        if (!cancelled) setState({ data: resp, loading: false, error: null });
      })
      .catch((e: unknown) => {
        // Issue #767: keep the last-known plans on a transient refetch error
        // instead of clobbering `data` to null. The 1s poll while the review
        // overlay is open (InboxPage) treats a null/empty `data` exactly like
        // "every plan applied" and auto-closes — a single dropped poll tick
        // must not be mistaken for that.
        if (!cancelled)
          setState((s) => ({ data: s.data, loading: false, error: String(e) }));
      });
    return () => {
      cancelled = true;
    };
  }, [epoch]);

  const refresh = useCallback(() => setEpoch((n) => n + 1), []);

  return { ...state, refresh };
}

/**
 * Apply a caller-chosen subset of open inbox plans (selection is per-ingestion
 * group / plan-level). Mirrors `useInboxPlanApplyAll`.
 */
export function useApplySelectedInboxPlans() {
  const [state, setState] = useState<PlanApplyState>({
    loading: false,
    error: null,
  });

  const applySelected = useCallback(
    async (inboxItemIds: string[]): Promise<InboxApplyAllResponse | null> => {
      setState({ loading: true, error: null });
      try {
        const result = unwrap(
          await commands.inboxPlanApplySelected({ inboxItemIds }),
        );
        setState({ loading: false, error: null });
        return result;
      } catch (e: unknown) {
        setState({ loading: false, error: String(e) });
        return null;
      }
    },
    [],
  );

  return { ...state, applySelected };
}

// ── Inbox stats hook (spec 041, US6 T039) ────────────────────────────────────

interface InboxStatsState {
  data: InboxStatsResponse | null;
  loading: boolean;
  error: string | null;
}

/**
 * Load aggregate per-type frame counts across all active inbox items.
 * Mirrors `useOpenInboxPlans`: a useState + useEffect + cancelled-flag pattern
 * keyed by a monotonic `epoch`. Call `refresh()` to re-fetch.
 */
export function useInboxStats() {
  const [epoch, setEpoch] = useState(0);
  const [state, setState] = useState<InboxStatsState>({
    data: null,
    loading: true,
    error: null,
  });

  useEffect(() => {
    let cancelled = false;
    setState((s) => ({ ...s, loading: true, error: null }));
    commands
      .inboxStats()
      .then(unwrap)
      .then((resp) => {
        if (!cancelled) setState({ data: resp, loading: false, error: null });
      })
      .catch((e: unknown) => {
        if (!cancelled)
          setState({ data: null, loading: false, error: String(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [epoch]);

  const refresh = useCallback(() => setEpoch((n) => n + 1), []);

  return { ...state, refresh };
}
