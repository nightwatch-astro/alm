// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Per-ingestion frame-type hint and breakdown tallies for the collapsed plan
 * summary rendered by `PlanPanel` inside the plan-approval overlay.
 */

import { useMemo } from 'react';
import { buildBreakdownFromActions } from './PlanPanel';
import { useInboxPlanBreakdowns } from './store';
import type {
  InboxBreakdownTarget,
  InboxClassifyResponse,
  InboxListItem,
  InboxOpenPlan,
} from './store';

export interface UseInboxPlanSummariesOptions {
  openPlans: InboxOpenPlan[];
  items: InboxListItem[];
  /** From the confirm flow: maps a plan action's `fromPath` to an absolute path. */
  absoluteByFromPath: Record<string, string>;
  selectedItem: InboxListItem | undefined;
  classification: InboxClassifyResponse | undefined;
}

export interface UseInboxPlanSummariesResult {
  frameTypeByItemId: Record<string, string>;
  breakdownByItemId: Record<
    string,
    ReadonlyArray<{ kind: string; count: number }>
  >;
}

export function useInboxPlanSummaries({
  openPlans,
  items,
  absoluteByFromPath,
  selectedItem,
  classification,
}: UseInboxPlanSummariesOptions): UseInboxPlanSummariesResult {
  // #75: frame-type hint per ingestion, derived from the inbox item's
  // classification/breakdown (here: the dominant `groupFrameType`, or the
  // master's `masterFrameType`). PlanPanel uses this to label each collapsed
  // group bucket by frame type (bias/dark/flat/light/master) instead of
  // degenerating to one line per catalogue action.
  const frameTypeByItemId = useMemo(() => {
    const byId: Record<string, string> = {};
    for (const it of items) {
      const ft = it.isMaster
        ? (it.masterFrameType ?? 'master')
        : it.groupFrameType;
      if (ft) byId[it.inboxItemId] = ft;
    }
    return byId;
  }, [items]);

  // #98: PRELOAD the authoritative per-type breakdown for EVERY item that has
  // an open plan — not just the selected one. Each open plan is mapped to its
  // item's registered root path (from the inbox list) so the classify query can
  // run. The hook shares `useInboxClassification`'s cache key, so the selected
  // item's classification is reused rather than re-fetched. The result is a
  // `inboxItemId → breakdown[]` map covering all unselected mixed folders, which
  // previously degraded to a dominant-type guess (e.g. "41 darks").
  const rootPathByItemId = useMemo(() => {
    const byId: Record<string, string> = {};
    for (const it of items) byId[it.inboxItemId] = it.rootAbsolutePath;
    return byId;
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
    const byId: Record<
      string,
      ReadonlyArray<{ kind: string; count: number }>
    > = {};
    for (const plan of openPlans) {
      byId[plan.inboxItemId] = buildBreakdownFromActions(
        plan.actions,
        frameTypeByItemId[plan.inboxItemId],
        absoluteByFromPath,
      );
    }
    // Overlay the preloaded authoritative breakdown for every open plan item.
    for (const [id, breakdown] of Object.entries(preloadedBreakdowns)) {
      if (breakdown.length > 0 && byId[id] != null) byId[id] = breakdown;
    }
    // Prefer the selected item's authoritative classification breakdown.
    if (
      selectedItem &&
      classification?.breakdown &&
      classification.breakdown.length > 0 &&
      byId[selectedItem.inboxItemId] != null
    ) {
      byId[selectedItem.inboxItemId] = classification.breakdown.map((b) => ({
        kind: b.kind,
        count: b.count,
      }));
    }
    return byId;
  }, [
    openPlans,
    frameTypeByItemId,
    absoluteByFromPath,
    preloadedBreakdowns,
    selectedItem,
    classification,
  ]);

  return { frameTypeByItemId, breakdownByItemId };
}
