// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * PlanPanel — aggregate plan surface for the Inbox review screen.
 *
 * spec 041 (#1/#2 + US7 T041/T042): renders the bottom region of the inbox
 * centre column and shows EVERY open plan across all ingestions at once,
 * grouped by ingestion (one group per `InboxOpenPlan`). Each group has a
 * plan-level selection checkbox; the header has a select-all/none checkbox and
 * the apply controls. The destructive-destination control (default Archive)
 * now lives here (relocated out of the deleted ActionSidebar) and feeds the
 * confirm/apply data flow via the parent.
 *
 * Pure presentational component apart from local selection state — all data
 * fetching + mutations are owned by the parent (InboxPage). The per-plan
 * group row markup lives in `PlanGroupRow`, the destination-root prompt in
 * `PlanRootPicker`, the destructive-destination control in
 * `PlanDestructiveControl`, and the pure summary/frame-type helpers in
 * `planPanelHelpers` — split out of this file to keep each module focused.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { Btn } from '@/ui';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { errMessage } from '@/lib/errors';
import type { PlanApplyProgress } from '@/features/plans/usePlanApplyProgress';
import type { InboxOpenPlan } from './store';
import { m } from '@/lib/i18n';
import { PlanRootPicker } from './PlanRootPicker';
import { PlanGroupRow } from './PlanGroupRow';
import { PlanDestructiveControl } from './PlanDestructiveControl';
import * as pp from './plan-panel.css';

export { buildBreakdownFromActions } from './planPanelHelpers';

// ── Types ─────────────────────────────────────────────────────────────────────

/**
 * Destructive-destination choice. The literal values are the strings the
 * backend `inbox.confirm` command accepts (`"archive"` keeps a recoverable
 * copy, `"trash"` uses the OS trash). Defaults to `"archive"` per
 * Constitution §II.
 */
export type DestructiveDestination = 'archive' | 'trash';

/**
 * One candidate destination root surfaced when a confirm fails with
 * `inbox.destination_root_required` (spec 041 US8/FR-029). Mirrors the
 * structured-error `details.candidates[]` shape emitted by the backend.
 */
export interface DestinationRootCandidate {
  rootId: string;
  path: string;
  kind: string;
}

/**
 * Pending destination-root selection: the user must pick one of `candidates`
 * before the plan for this item can be generated (FR-029). `category` is the
 * frame-type category the roots host (e.g. `light_frames`).
 */
export interface PendingRootPick {
  category: string;
  candidates: DestinationRootCandidate[];
}

export interface PlanPanelProps {
  /** Every open plan across all roots (already fetched by the parent). */
  plans: InboxOpenPlan[];
  /** Sum of actions across all plans, for the header count. */
  totalActions: number;
  destructiveDestination: DestructiveDestination;
  onDestructiveDestinationChange: (d: DestructiveDestination) => void;
  /** Apply only the currently-selected (checked) ingestion groups. */
  onApplySelected: (inboxItemIds: string[]) => void;
  /** Apply every open plan. */
  onApplyAll: () => void;
  /**
   * Apply a single ingestion group's plan with live per-item progress
   * streamed over the long-operation `OperationEvent` channel (spec 042
   * US16 / FR-021). Receives the group's `planId`.
   */
  onApplyOne?: (planId: string) => void;
  /**
   * Live progress for the plan currently streaming (the one whose `planId`
   * matches `progressPlanId`). Null when no live apply is in flight.
   */
  progress?: PlanApplyProgress | null;
  /** `planId` of the group whose live `progress` is being shown. */
  progressPlanId?: string | null;
  /** Discard a single ingestion group's plan. */
  onCancel: (inboxItemId: string) => void;
  busy?: boolean;
  /**
   * Destination-root prompt (spec 041 US8/FR-029). Non-null when the last
   * confirm needs the user to choose among multiple candidate roots. The plan
   * cannot be generated/applied until a root is chosen.
   */
  pendingRootPick?: PendingRootPick | null;
  /** Re-invoke confirm with the chosen destination root. */
  onPickDestinationRoot?: (rootId: string) => void;
  /** Busy flag specific to the (re-)confirm triggered by a root pick. */
  rootPickBusy?: boolean;
  /**
   * Absolute destination paths keyed by source `fromPath`, populated from the
   * latest `inbox.confirm` response's `destinations[]` (spec 041 US8/FR-031).
   * Action rows show the absolute path when present, else the relative preview.
   */
  absoluteByFromPath?: Record<string, string>;
  /**
   * Frame-type hint per ingestion (`inboxItemId` → "bias" | "dark" | "flat" |
   * "light" | "master" | …), derived by the parent from the inbox item's
   * classification / breakdown (spec 043 #75). Used to label each collapsed
   * group bucket by frame type so catalogue actions (whose destination path
   * carries no frame keyword) aggregate to "N <frametype>" instead of
   * degenerating into one line per file.
   */
  frameTypeByItemId?: Record<string, string>;
  /**
   * Per-ingestion frame-type BREAKDOWN (`inboxItemId` → [{kind, count}, …]),
   * derived by the parent from the SAME data `InboxStatsSummary` computes from
   * the inbox item (the per-type bias/dark/flat/light/master tallies — see
   * `buildBreakdownByItemId` in InboxPage).
   *
   * spec 043 #75: this is the authoritative fix for the degenerate summary. The
   * plan ACTIONS carry no per-file frame type, and the single `frameTypeHint`
   * collapses a MIXED ingestion to one wrong label. When a breakdown is present
   * the collapsed summary renders ONE line listing every type with its count —
   * `"10 bias · 21 dark · 12 light → (root)"` — instead of per-file rows or a
   * single mislabelled type. Absent (no breakdown for the id) the panel falls
   * back to the per-action keyword/hint aggregation.
   */
  breakdownByItemId?: Record<
    string,
    ReadonlyArray<{ kind: string; count: number }>
  >;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function PlanPanel({
  plans,
  totalActions,
  destructiveDestination,
  onDestructiveDestinationChange,
  onApplySelected,
  onApplyAll,
  onApplyOne,
  progress = null,
  progressPlanId = null,
  onCancel,
  busy = false,
  pendingRootPick = null,
  onPickDestinationRoot,
  rootPickBusy = false,
  absoluteByFromPath,
  frameTypeByItemId,
  breakdownByItemId,
}: PlanPanelProps) {
  // Plan-level selection set, keyed by inboxItemId. Stale plans cannot be
  // selected (and are pruned from the set if they become stale).
  const [selected, setSelected] = useState<Set<string>>(new Set());

  // Per-group file-row visibility, keyed by inboxItemId. Groups are COLLAPSED
  // by default — only the summary lines show until a group is expanded.
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const toggleExpanded = useCallback((inboxItemId: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(inboxItemId)) next.delete(inboxItemId);
      else next.add(inboxItemId);
      return next;
    });
  }, []);

  // Selectable = not stale. Keep the selection set in sync as the open-plans
  // list changes (e.g. after an apply removes a plan, or a plan goes stale).
  const selectableIds = useMemo(
    () => plans.filter((p) => !p.stale).map((p) => p.inboxItemId),
    [plans],
  );

  useEffect(() => {
    setSelected((prev) => {
      const allowed = new Set(selectableIds);
      let changed = false;
      const next = new Set<string>();
      for (const id of prev) {
        if (allowed.has(id)) next.add(id);
        else changed = true;
      }
      return changed ? next : prev;
    });
  }, [selectableIds]);

  const hasDestructive = useMemo(
    () =>
      plans.some((p) => p.actions.some((a) => a.requiresDestructiveConfirm)),
    [plans],
  );

  // Destructive-confirm gate (FR-003, D9, issue #741): `destructive_confirmed`
  // had no writer anywhere, so every plan with a trash/delete item was
  // permanently refused at apply time. Plan-level (not per-item —
  // `InboxPlanAction` carries no item id), matching the destructive-destination
  // control above. Tracked by plan id rather than a single boolean so newly
  // arrived destructive plans are not mistaken for already-confirmed ones.
  const [confirmedPlanIds, setConfirmedPlanIds] = useState<Set<string>>(
    new Set(),
  );
  const [confirmingDestructive, setConfirmingDestructive] = useState(false);
  const [confirmDestructiveError, setConfirmDestructiveError] = useState<
    string | null
  >(null);

  const destructivePlanIds = useMemo(
    () =>
      plans
        .filter((p) => p.actions.some((a) => a.requiresDestructiveConfirm))
        .map((p) => p.planId),
    [plans],
  );
  const allDestructiveConfirmed = destructivePlanIds.every((id) =>
    confirmedPlanIds.has(id),
  );

  const handleConfirmDestructive = useCallback(async () => {
    if (confirmingDestructive) return;
    const pending = destructivePlanIds.filter(
      (id) => !confirmedPlanIds.has(id),
    );
    if (pending.length === 0) return;
    setConfirmingDestructive(true);
    setConfirmDestructiveError(null);
    try {
      await Promise.all(
        pending.map(async (id) =>
          unwrap(await commands.plansConfirmDestructive(id)),
        ),
      );
      setConfirmedPlanIds((prev) => new Set([...prev, ...pending]));
    } catch (e) {
      setConfirmDestructiveError(errMessage(e));
    } finally {
      setConfirmingDestructive(false);
    }
  }, [confirmingDestructive, destructivePlanIds, confirmedPlanIds]);

  const selectedArray = useMemo(() => [...selected], [selected]);
  const anySelectedStale = false; // selection set never contains stale plans by construction
  const allSelectableSelected =
    selectableIds.length > 0 && selectableIds.every((id) => selected.has(id));

  const toggleGroup = (inboxItemId: string, stale: boolean) => {
    if (stale) return;
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(inboxItemId)) next.delete(inboxItemId);
      else next.add(inboxItemId);
      return next;
    });
  };

  const toggleAll = () => {
    setSelected((prev) =>
      prev.size === selectableIds.length && selectableIds.length > 0
        ? new Set()
        : new Set(selectableIds),
    );
  };

  // Starting global action index per plan, so each absolute-path cell gets a
  // stable, unique `inbox-dest-absolute-<idx>` testid (computed during render
  // instead of mutating a counter, which the immutability lint forbids).
  const planRowOffsets = useMemo(() => {
    const offsets: number[] = [];
    let running = 0;
    for (const p of plans) {
      offsets.push(running);
      running += p.actions.length;
    }
    return offsets;
  }, [plans]);

  // ── Empty state ──
  // Nothing to show unless there is at least one open plan OR a pending root
  // pick (the latter can occur with zero open plans — the plan wasn't created).
  if (plans.length === 0) {
    return pendingRootPick ? (
      <div className={pp.root} data-testid="plan-panel">
        <PlanRootPicker
          pendingRootPick={pendingRootPick}
          onPickDestinationRoot={onPickDestinationRoot}
          rootPickBusy={rootPickBusy}
        />
      </div>
    ) : null;
  }

  const applySelectedDisabled =
    busy ||
    selectedArray.length === 0 ||
    anySelectedStale ||
    !allDestructiveConfirmed;
  const applyAllDisabled =
    busy || plans.length === 0 || !allDestructiveConfirmed;

  return (
    <div className={pp.root} data-testid="plan-panel">
      {/* ── Destination-root picker (FR-029): blocks apply until chosen ── */}
      {pendingRootPick && (
        <PlanRootPicker
          pendingRootPick={pendingRootPick}
          onPickDestinationRoot={onPickDestinationRoot}
          rootPickBusy={rootPickBusy}
        />
      )}

      {/* ── Pinned header: counts + select-all + apply controls ── */}
      <div className={pp.bar} data-testid="plan-panel-bar">
        <div className={pp.barLeft}>
          {}
          <label className={pp.selectAll}>
            <input
              type="checkbox"
              checked={allSelectableSelected}
              onChange={toggleAll}
              disabled={selectableIds.length === 0}
              aria-label={m.inbox_select_all_plans_aria()}
              data-testid="plan-select-all"
            />
            <span className={pp.selectAllLabel}>{m.common_select_all()}</span>
          </label>
          <span className={pp.countSummary} data-testid="plan-total-count">
            {m.plan_count_label({ count: plans.length })} ·{' '}
            {m.action_count_label({ count: totalActions })}
          </span>
        </div>
        <div className={pp.barActions}>
          <Btn
            variant="primary"
            data-guide-anchor="inbox.apply-plan-cta"
            onClick={() => onApplySelected(selectedArray)}
            disabled={applySelectedDisabled}
            data-testid="plan-apply-selected"
            aria-label={m.inbox_apply_selected_plans_aria()}
          >
            {busy
              ? m.common_applying()
              : m.inbox_apply_selected_plans({ count: selectedArray.length })}
          </Btn>
          <Btn
            variant="ghost"
            onClick={onApplyAll}
            disabled={applyAllDisabled}
            data-testid="plan-apply-all"
            aria-label={m.inbox_apply_all_plans_aria()}
          >
            {m.inbox_apply_all()}
          </Btn>
        </div>
      </div>

      {/* ── Scrollable group list ── */}
      <div className={pp.scroll} data-testid="plan-panel-scroll">
        {/* Column header — aligns with each plan's group-header grid. */}
        <div className={pp.listHead} aria-hidden="true">
          <span className={pp.groupLead} />
          <span>{m.inbox_plan_col_plan()}</span>
          <span>{m.inbox_plan_col_composition()}</span>
          <span>{m.inbox_col_destination()}</span>
          <span>{m.inbox_col_files()}</span>
          <span />
        </div>
        {plans.map((plan, planIdx) => (
          <PlanGroupRow
            key={plan.inboxItemId}
            plan={plan}
            rowOffset={planRowOffsets[planIdx]}
            checked={selected.has(plan.inboxItemId)}
            isExpanded={expanded.has(plan.inboxItemId)}
            onToggleExpanded={() => toggleExpanded(plan.inboxItemId)}
            onToggleGroup={() => toggleGroup(plan.inboxItemId, plan.stale)}
            onApplyOne={onApplyOne}
            onCancel={onCancel}
            busy={busy}
            confirmedPlanIds={confirmedPlanIds}
            progress={progress}
            progressPlanId={progressPlanId}
            absoluteByFromPath={absoluteByFromPath}
            frameTypeByItemId={frameTypeByItemId}
            breakdownByItemId={breakdownByItemId}
          />
        ))}
      </div>

      {/* ── Destructive destination control (relocated from ActionSidebar) ── */}
      {hasDestructive && (
        <PlanDestructiveControl
          destructiveDestination={destructiveDestination}
          onDestructiveDestinationChange={onDestructiveDestinationChange}
          allDestructiveConfirmed={allDestructiveConfirmed}
          confirmingDestructive={confirmingDestructive}
          confirmDestructiveError={confirmDestructiveError}
          onConfirmDestructive={() => void handleConfirmDestructive()}
        />
      )}
    </div>
  );
}
