// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * PlanReviewOverlay — shared, parameterised focused overlay for reviewing and
 * applying ONE generated plan (spec 017 review surface; first consumer is the
 * cleanup flow, WP-E).
 *
 * Plan approval is a focused overlay per the product decisions: the overlay
 * loads the plan via `plans.get`, renders every proposed item (source path,
 * action, protection — FR-003/SC-001), gates approval behind the spec-016
 * {@link PlanProtectionGate} (protected items require explicit
 * acknowledgement), then drives the single review → approved → applying edge:
 * `plans.approve` issues the approval token and {@link usePlanApplyProgress}
 * streams the apply run with live per-item progress (D17 progress UI, absorbed
 * from spec 025).
 *
 * Reuse contract (user mandate — one parameterised component, never
 * per-feature clones): any flow that generates a plan and reviews it inline
 * renders THIS overlay with its `planId`. Feature-specific bulk surfaces (the
 * inbox multi-plan PlanApprovalOverlay) predate it and remain separate.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Modal } from '@/components';
import { Btn, Pill, Banner, Table } from '@/ui';
import { m } from '@/lib/i18n';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { errMessage } from '@/lib/errors';
import { queryKeys } from '@/data/queryKeys';
import { formatBytes } from '@/lib/format';
import { addToast } from '@/shared/toast';
import { PlanProtectionGate } from './PlanProtectionGate';
import { usePlanApplyProgress } from './usePlanApplyProgress';
import type {
  PlanDetail_Serialize,
  PlanItemDetail_Serialize,
} from '@/bindings/index';
import { bodyFill as modalBodyFill } from '@/components/modal.css';

// ── Props ────────────────────────────────────────────────────────────────────

export interface PlanReviewOverlayProps {
  /** Plan to review; the overlay is inert while null. */
  planId: string | null;
  open: boolean;
  onClose: () => void;
  /** Overlay title; defaults to the generic review title. */
  title?: string;
  /** Called once when the apply run reaches the `completed` terminal state. */
  onApplied?: () => void;
  /** Called after the plan is discarded. */
  onDiscarded?: () => void;
  /**
   * Called after `plans.retry` creates a new plan from this one's failed
   * items (US5, T037). The caller decides how to surface it — the shared
   * pattern is to re-point this same overlay's `planId` at the new plan.
   */
  onRetryCreated?: (newPlanId: string) => void;
  /**
   * Diagnostic sentence explaining a 0-item plan (#603), sourced from the
   * plan generator's own result (e.g. `GenerateArchivePlanResult.emptyReason`)
   * — the overlay only ever sees the persisted `PlanDetail`, which carries no
   * such field, so the caller must forward it from the generate call.
   */
  emptyReason?: string | null;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function actionPillVariant(action: PlanItemDetail_Serialize['action']) {
  return action === 'delete' ? ('danger' as const) : ('info' as const);
}

/**
 * #607: per-item apply outcome pill variant. `plans.get` already persists
 * `state`/`failureReason` per item (`plan_items.item_state`/`failure_reason`,
 * written by `apply_repo::item_failed`) — durable and survives reopening the
 * plan later, unlike the transient live-progress event stream. Only the
 * table never rendered it.
 */
function resultPillVariant(state: PlanItemDetail_Serialize['state']) {
  switch (state) {
    case 'succeeded':
      return 'ok' as const;
    case 'failed':
      return 'danger' as const;
    case 'applying':
      return 'info' as const;
    case 'skipped':
    case 'cancelled':
      return 'warn' as const;
    default:
      return 'ghost' as const;
  }
}

/**
 * #761 (spec 049 FR-004): the resolved per-item link kind (symlink/junction/
 * copy/hardlink), when this item carries one. Generation/regeneration plans
 * (`source_view_generate.rs`/`prepared_views.rs::regenerate_prepared_view`)
 * attach it as a `materialization` provenance entry — the only way today to
 * show the user, before apply, what capability was actually resolved for
 * each item (the contract data already existed; the review UI never read it).
 */
function linkKind(item: PlanItemDetail_Serialize): string | null {
  return (
    item.provenance?.find((p) => p.label === 'materialization')?.value ?? null
  );
}

function destinationLabel(plan: PlanDetail_Serialize): string {
  // DTO enum is `archive | os_trash` (the DB canonical vocabulary is
  // `archive | trash`; plans.get maps trash → os_trash).
  return plan.destructiveDestination === 'os_trash'
    ? m.plans_dest_trash()
    : m.plans_dest_archive();
}

// ── Component ────────────────────────────────────────────────────────────────

export function PlanReviewOverlay({
  planId,
  open,
  onClose,
  title,
  onApplied,
  onDiscarded,
  onRetryCreated,
  emptyReason,
}: PlanReviewOverlayProps) {
  const queryClient = useQueryClient();

  const {
    data: plan,
    isFetching: planLoading,
    error: planError,
  } = useQuery({
    queryKey: queryKeys.plans.detail(planId ?? ''),
    queryFn: async () => unwrap(await commands.plansGet(planId as string)),
    enabled: open && planId !== null,
  });

  // Advisory destination free-space estimate (issue #876) — surfaced at
  // review time, before approval; never gates "Approve & apply" (that
  // decision belongs to `recheck_disk_space`'s real R-Pause-1 apply-time
  // check, not this estimate). Only fetched once the plan itself has items to
  // probe a destination from.
  const { data: freeSpace } = useQuery({
    queryKey: queryKeys.plans.freeSpaceEstimate(planId ?? ''),
    queryFn: async () =>
      unwrap(await commands.plansFreeSpaceEstimate(planId as string)),
    enabled: open && planId !== null && (plan?.itemsTotal ?? 0) > 0,
  });
  const freeSpaceInsufficient =
    freeSpace?.availableBytes != null &&
    freeSpace.availableBytes < freeSpace.requiredBytes;

  // Protection gate readiness (spec 016 US3): the gate signals true when every
  // protected item is acknowledged — or immediately when none are protected.
  const [gateReady, setGateReady] = useState(false);

  const [approving, setApproving] = useState(false);
  const [discarding, setDiscarding] = useState(false);
  const [retrying, setRetrying] = useState(false);
  const [applyError, setApplyError] = useState<string | null>(null);
  // The apply run's final `PlanState` (US5/T037): distinct from
  // `progress.terminal`, which only tracks the event-stream outcome and
  // cannot tell `partially_applied` apart from `applied`. Retry needs the
  // real terminal plan state to decide whether to offer "Generate retry plan".
  const [finalState, setFinalState] = useState<string | null>(null);
  const [resuming, setResuming] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const {
    progress,
    run: runApply,
    resume: resumeApply,
    cancel: cancelApply,
    reset: resetApply,
  } = usePlanApplyProgress();

  // Destructive-confirm gate (FR-003, D9, issue #741): `delete` items are
  // permanently refused at apply time until `destructive_confirmed` is set.
  // Plan-level (not per-item — `PlanItemDetail` carries no such flag; the
  // gate mirrors the existing plan-wide protection gate above it).
  const hasDestructiveItems = (plan?.items ?? []).some(
    (item) => item.action === 'delete',
  );
  const [destructiveConfirmed, setDestructiveConfirmed] = useState(false);
  const [confirmingDestructive, setConfirmingDestructive] = useState(false);
  const [confirmDestructiveError, setConfirmDestructiveError] = useState<
    string | null
  >(null);

  const busy = approving || discarding || retrying || progress.running;
  // FR-011 (issue #733): a plan reopened from a prior session carries no
  // session-local `finalState` (it starts `null` every mount), so the
  // footer must fall back to the persisted `plan.state` from `plans.get`
  // rather than always rendering the pre-apply Discard/Approve&Apply pair
  // — the backend refuses those actions on a terminal plan with
  // `plan.invalid_state`.
  const effectiveState = finalState ?? plan?.state ?? null;
  const applied = effectiveState === 'applied';
  const retryable =
    effectiveState === 'failed' ||
    effectiveState === 'partially_applied' ||
    effectiveState === 'cancelled';

  const invalidatePlan = useCallback(() => {
    if (planId !== null) {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.plans.detail(planId),
      });
    }
  }, [planId, queryClient]);

  // #744 FR-002: `handleResume` (below) doesn't await the resumed run's
  // completion — `usePlanApplyProgress.resume` returns once the poll has
  // STARTED, not once it reaches a terminal state (unlike `runApply`, which
  // does await completion). This ref tracks "a resume is in flight" so the
  // effect below can sync `finalState`/refetch the plan exactly once the
  // NEXT terminal outcome arrives, without also firing for (and clobbering
  // the more precise `newState` set by) `handleApproveAndApply`'s own path.
  const resumeAwaitingTerminal = useRef(false);
  useEffect(() => {
    if (!resumeAwaitingTerminal.current || progress.terminal === null) return;
    resumeAwaitingTerminal.current = false;
    invalidatePlan();
    setFinalState(progress.terminal === 'completed' ? 'applied' : 'failed');
  }, [progress.terminal, invalidatePlan]);

  // The overlay must not silently disappear mid-run (constitution II — the
  // apply outcome stays on screen); ignore close requests while busy.
  const handleClose = useCallback(() => {
    if (busy) return;
    resetApply();
    resumeAwaitingTerminal.current = false;
    setApplyError(null);
    setGateReady(false);
    setFinalState(null);
    setDestructiveConfirmed(false);
    setConfirmDestructiveError(null);
    onClose();
  }, [busy, onClose, resetApply]);

  /** Persist the destructive-confirm flag for this plan (`plans.confirm.destructive`,
   * issue #741) before Approve & apply unlocks. */
  const handleConfirmDestructive = useCallback(async () => {
    if (planId === null || confirmingDestructive) return;
    setConfirmingDestructive(true);
    setConfirmDestructiveError(null);
    try {
      unwrap(await commands.plansConfirmDestructive(planId));
      setDestructiveConfirmed(true);
    } catch (e) {
      setConfirmDestructiveError(errMessage(e));
    } finally {
      setConfirmingDestructive(false);
    }
  }, [planId, confirmingDestructive]);

  const handleApproveAndApply = useCallback(async () => {
    if (planId === null || busy) return;
    setApplyError(null);
    setApproving(true);
    let token: string;
    try {
      token = unwrap(await commands.plansApprove(planId)).approvalToken;
    } catch (e) {
      setApproving(false);
      setApplyError(errMessage(e));
      return;
    }
    setApproving(false);

    const response = await runApply({ id: planId, approvalToken: token });
    invalidatePlan();
    setFinalState(response?.newState ?? 'failed');
    if (response !== null && response.newState === 'applied') {
      addToast({
        message: m.plans_review_apply_success_toast(),
        variant: 'success',
      });
      onApplied?.();
    } else {
      setApplyError(m.plans_review_apply_failed());
    }
  }, [planId, busy, runApply, invalidatePlan, onApplied]);

  /** Cancel the currently-streaming apply run (`plan.cancel`, US3/FR-009,
   * issue #743). The channel already delivers the resulting terminal event;
   * this only signals the backend and surfaces a failure to send it. */
  const handleCancelRun = useCallback(async () => {
    if (planId === null || cancelling) return;
    setCancelling(true);
    const ok = await cancelApply(planId);
    setCancelling(false);
    if (!ok) setApplyError(m.plans_review_cancel_failed());
  }, [planId, cancelling, cancelApply]);

  const handleDiscard = useCallback(async () => {
    if (planId === null || busy) return;
    setDiscarding(true);
    try {
      unwrap(await commands.plansDiscard(planId));
      addToast({ message: m.plans_review_discarded_toast(), variant: 'info' });
      onDiscarded?.();
      resetApply();
      setGateReady(false);
      setFinalState(null);
      onClose();
    } catch (e) {
      setApplyError(errMessage(e));
    } finally {
      setDiscarding(false);
    }
  }, [planId, busy, onDiscarded, onClose, resetApply]);

  /** Resume a paused apply run (R-Pause-1, T048-T050). Minimal, honest
   * surface: reflects the real `plan.resume` call outcome, nothing simulated. */
  const handleResume = useCallback(async () => {
    if (planId === null || resuming) return;
    setResuming(true);
    setApplyError(null);
    const ok = await resumeApply(planId);
    setResuming(false);
    if (ok) {
      resumeAwaitingTerminal.current = true;
    } else {
      setApplyError(m.plans_review_resume_failed());
    }
  }, [planId, resuming, resumeApply]);

  /** Generate a retry plan from this plan's failed items (US5, T037) — the
   * plan-review flow's only entry point since there is no standalone Plans
   * list to reopen a terminal plan from (T015/T016 OBSOLETE-BY-DESIGN). */
  const handleGenerateRetryPlan = useCallback(async () => {
    if (planId === null || busy) return;
    setRetrying(true);
    setApplyError(null);
    // A plan reopened in `cancelled` state has no `failed` items to retry
    // (`plan.retry`'s `failed` filter would refuse with `no.items.to.retry`)
    // — retry its cancelled items instead (`RetryItemsFilter::Cancelled`).
    const itemsFilter = effectiveState === 'cancelled' ? 'cancelled' : 'failed';
    try {
      const res = unwrap(await commands.plansRetry(planId, itemsFilter));
      addToast({
        message: m.plans_review_retry_created_toast(),
        variant: 'info',
      });
      resetApply();
      setGateReady(false);
      setFinalState(null);
      onRetryCreated?.(res.newPlanId);
    } catch (e) {
      // `errMessage` resolves a `plans.retry` `ContractError` (e.g. the
      // backend's `NoItemsToRetry`/`no.items.to.retry` code) through the
      // exhaustive spec-046 catalog (`err_no_items_to_retry`) rather than a
      // second, plan-review-specific translation of the same case.
      setApplyError(errMessage(e));
    } finally {
      setRetrying(false);
    }
  }, [planId, busy, resetApply, onRetryCreated, effectiveState]);

  // ── Items table ────────────────────────────────────────────────────────────

  const columns = [
    { key: 'name', label: m.plans_review_col_item() },
    { key: 'action', label: m.plans_review_col_action() },
    { key: 'from', label: m.plans_review_col_from() },
    { key: 'to', label: m.plans_review_col_to() },
    { key: 'protection', label: m.plans_review_col_protection() },
    { key: 'linkKind', label: m.plans_review_col_link_kind() },
    { key: 'result', label: m.plans_review_col_result() },
    { key: 'reason', label: m.plans_review_col_reason() },
    { key: 'linked', label: m.plans_review_col_linked() },
  ];

  // FR-003: every item shows its destination path or, for `delete`-action
  // items (no destination — the source is removed in place), a deletion cue.
  // `reason`/`linked` (issue #733): present on the DTO but were never
  // rendered, undermining SC-001's pre-approval inspectability.
  const rows = (plan?.items ?? []).map((item) => ({
    _testid: `plan-review-item-${item.index}`,
    _rowClassName:
      item.protection === 'protected'
        ? 'pv-plan-review__row--protected'
        : undefined,
    name: item.name,
    action: <Pill variant={actionPillVariant(item.action)}>{item.action}</Pill>,
    from: <span className="pv-mono">{item.from}</span>,
    to:
      item.action === 'delete' ? (
        <span className="pv-cell--muted">
          {m.plans_review_deletion_target()}
        </span>
      ) : (
        <span className="pv-mono">{item.to}</span>
      ),
    protection:
      item.protection === 'protected' ? (
        <Pill variant="warn">{m.settings_cleanup_protection_protected()}</Pill>
      ) : (
        <Pill variant="ghost">{m.settings_cleanup_protection_normal()}</Pill>
      ),
    // #761: per-item resolved link kind (generation/regeneration plans
    // only); blank for every other plan type — matches the `linked` column's
    // existing not-applicable convention.
    linkKind: linkKind(item) ?? (
      <span className="pv-cell--muted">{m.common_none()}</span>
    ),
    // #607: per-item apply outcome, so a partial failure is diagnosable
    // without re-running the plan. `pending` (never attempted, e.g. the plan
    // hasn't been applied yet) shows a muted dash rather than a pill.
    result:
      item.state === 'pending' ? (
        <span
          className="pv-cell--muted"
          data-testid={`plan-review-item-result-${item.index}`}
        >
          {m.common_none()}
        </span>
      ) : (
        <span
          className="pv-plan-review__result"
          data-testid={`plan-review-item-result-${item.index}`}
        >
          <Pill variant={resultPillVariant(item.state)}>{item.state}</Pill>
          {item.failureReason && (
            <span
              className="pv-plan-review__failure-reason"
              title={item.failureReason}
            >
              {item.failureReason}
            </span>
          )}
        </span>
      ),
    reason: item.reason,
    linked: item.linked ?? (
      <span className="pv-cell--muted">{m.common_none()}</span>
    ),
  }));

  // ── Footer ────────────────────────────────────────────────────────────────

  const footer = applied ? (
    <Btn onClick={handleClose}>{m.common_close()}</Btn>
  ) : retryable ? (
    <>
      <Btn variant="ghost" onClick={handleClose} disabled={busy}>
        {m.common_close()}
      </Btn>
      <Btn
        variant="danger"
        onClick={() => void handleGenerateRetryPlan()}
        disabled={busy}
        data-testid="plan-review-retry"
      >
        {retrying ? m.plans_review_retrying() : m.plans_review_retry_btn()}
      </Btn>
    </>
  ) : (
    <>
      <Btn variant="ghost" onClick={() => void handleDiscard()} disabled={busy}>
        {m.plans_review_discard_btn()}
      </Btn>
      <Btn
        variant={hasDestructiveItems ? 'destructive' : 'primary'}
        onClick={() => void handleApproveAndApply()}
        disabled={
          busy ||
          !gateReady ||
          plan == null ||
          plan.itemsTotal === 0 ||
          (hasDestructiveItems && !destructiveConfirmed)
        }
        data-testid="plan-review-approve-apply"
      >
        {approving
          ? m.plans_review_approving()
          : progress.running
            ? m.common_applying()
            : m.plans_review_approve_apply_btn()}
      </Btn>
    </>
  );

  return (
    <Modal
      open={open}
      onClose={handleClose}
      title={title ?? m.plans_review_overlay_title()}
      subtitle={
        plan
          ? `${m.plans_review_item_count({ count: plan.itemsTotal })} · ${destinationLabel(plan)}`
          : undefined
      }
      size="lg"
      ariaLabel={title ?? m.plans_review_overlay_title()}
      closeOnBackdrop={!busy}
      footer={footer}
      // The item table owns its own scroll region (below); the body itself
      // must not also scroll, or the item list would double-scroll.
      bodyClassName={modalBodyFill}
      data-testid="plan-review-overlay"
    >
      {planLoading && plan == null ? (
        <div className="pv-plan-review__status">{m.common_loading()}</div>
      ) : planError || plan == null ? (
        <Banner variant="danger">{m.plans_review_load_error()}</Banner>
      ) : (
        <div className="pv-plan-review">
          {/* Summary line: no mutation before approval (FR-002 teaching copy). */}
          <Banner variant="info" role="status">
            {m.plans_review_no_mutation_note()}
            {plan.totalBytesRequired > 0 &&
              ` ${m.plans_review_bytes_required({ size: formatBytes(plan.totalBytesRequired) })}`}
          </Banner>

          {/* #876: destination free-space estimate, before approval. Advisory
              only — a warning here never disables "Approve & apply"; the real
              gate is the apply-time `recheck_disk_space` pre-flight. */}
          {freeSpace?.availableBytes != null && (
            <Banner
              variant={freeSpaceInsufficient ? 'warn' : 'info'}
              role="status"
              data-testid="plan-review-free-space"
            >
              {freeSpaceInsufficient
                ? m.plans_review_free_space_insufficient({
                    required: formatBytes(freeSpace.requiredBytes),
                    available: formatBytes(freeSpace.availableBytes),
                  })
                : m.plans_review_free_space_available({
                    size: formatBytes(freeSpace.availableBytes),
                  })}
            </Banner>
          )}

          {/* #603: a 0-item plan otherwise dead-ends on a disabled
              "Approve & apply" with no explanation — render the generator's
              own diagnostic instead of leaving the user to guess. */}
          {plan.itemsTotal === 0 && emptyReason && (
            <Banner variant="warn" data-testid="plan-review-empty-reason">
              {emptyReason}
            </Banner>
          )}

          {/* Every proposed item, reviewable before approval (SC-001).
              Virtualized (shared `.pv-listtable` pattern, spec 017 T050):
              plans can carry hundreds of items, so the table owns its own
              bounded scroll region instead of rendering every row — the
              summary/gate/progress/footer above and below stay pinned. */}
          <div className="pv-listtable">
            <Table
              columns={columns}
              rows={rows}
              virtualized
              scrollClassName="pv-listtable__scroll"
              data-testid="plan-review-items"
            />
          </div>

          {/* Spec-016 protection gate: protected items require acknowledgement
              before Approve & apply unlocks. */}
          <PlanProtectionGate
            planId={plan.id}
            onAcknowledgedChange={setGateReady}
          />

          {/* Destructive-confirm gate (FR-003, D9, issue #741): delete items
              are refused at apply time until confirmed. Plan-level — see
              `handleConfirmDestructive`. */}
          {hasDestructiveItems && (
            <div className="pv-plan-review__destructive-gate">
              <label className="pv-plan-review__destructive-label">
                <input
                  type="checkbox"
                  checked={destructiveConfirmed}
                  disabled={confirmingDestructive || destructiveConfirmed}
                  onChange={() => void handleConfirmDestructive()}
                  aria-label={m.plans_review_confirm_destructive_label()}
                  data-testid="plan-review-confirm-destructive"
                />
                <span>
                  {confirmingDestructive
                    ? m.common_confirming()
                    : m.plans_review_confirm_destructive_label()}
                </span>
              </label>
              {confirmDestructiveError !== null && (
                <Banner variant="danger">{confirmDestructiveError}</Banner>
              )}
            </div>
          )}

          {/* Live apply progress (D17 — spec 025 progress UI, absorbed here). */}
          {(progress.running ||
            progress.terminal !== null ||
            progress.paused ||
            progress.resumeStalled) && (
            <div
              className="pv-plan-review__progress"
              role="status"
              aria-live="polite"
              data-testid="plan-review-progress"
            >
              {progress.running &&
                m.plans_review_progress_running({
                  applied: progress.applied,
                  total: progress.total ?? plan.itemsTotal,
                })}
              {/* Cancel-in-flight (US3/FR-009, issue #743): available for the
                  whole active-run window (running or paused), matching what
                  `plan.cancel` accepts server-side. */}
              {progress.running && progress.terminal === null && (
                <>
                  {' '}
                  <Btn
                    size="sm"
                    variant="ghost"
                    onClick={() => void handleCancelRun()}
                    disabled={cancelling}
                    data-testid="plan-review-cancel-run"
                  >
                    {cancelling
                      ? m.plans_review_cancelling()
                      : m.plans_review_cancel_run_btn()}
                  </Btn>
                </>
              )}
              {applied && (
                <Pill variant="ok">
                  {m.plans_review_progress_done({ count: progress.applied })}
                </Pill>
              )}
              {progress.terminal === 'failed' && (
                <Pill variant="danger">{m.plans_review_apply_failed()}</Pill>
              )}
              {progress.failed > 0 &&
                ` ${m.plans_review_progress_failed({ count: progress.failed })}`}
              {progress.paused && (
                <>
                  {' '}
                  <Pill variant="warn" data-testid="plan-review-paused-badge">
                    {m.plans_review_paused_badge({
                      reason: progress.pauseReason ?? m.common_unknown(),
                    })}
                  </Pill>{' '}
                  <Btn
                    size="sm"
                    variant="ghost"
                    onClick={() => void handleResume()}
                    disabled={resuming}
                    data-testid="plan-review-resume"
                  >
                    {resuming
                      ? m.plans_review_resuming()
                      : m.plans_review_resume_btn()}
                  </Btn>
                </>
              )}
              {progress.resumeStalled && (
                <Pill
                  variant="warn"
                  data-testid="plan-review-resume-stalled-badge"
                >
                  {m.plans_review_resume_stalled()}
                </Pill>
              )}
            </div>
          )}

          {applyError !== null && (
            <Banner variant="danger">{applyError}</Banner>
          )}
        </div>
      )}
    </Modal>
  );
}
