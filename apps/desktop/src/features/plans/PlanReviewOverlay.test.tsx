// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * PlanReviewOverlay tests — spec 017 WP-E (cleanup-plan review + progress UI).
 *
 * Covers the shared review overlay's contract:
 * 1. Renders every plan item (source path, action, protection) from plans.get.
 * 2. Protected items keep "Approve & apply" disabled until acknowledged via
 *    the spec-016 protection gate; acknowledging unlocks it.
 * 3. Approve & apply drives plans.approve → plans.apply (with the approval
 *    token) and reports the completed terminal state (progress UI, D17).
 * 4. Discard calls plans.discard and closes the overlay.
 * 5. A zero-item plan cannot be approved (FR-014).
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import type {
  PlanDetail_Serialize,
  PlanItemDetail_Serialize,
  PlanProtectionCheckResponse,
  OperationEvent,
} from '@/bindings/index';

const {
  mockPlansGet,
  mockPlansApprove,
  mockPlansDiscard,
  mockPlansRetry,
  mockPlansResume,
  mockPlansApplyStatus,
  mockPlansCancel,
  mockPlansConfirmDestructive,
  mockPlansFreeSpaceEstimate,
  mockProtectionCheck,
  mockAcknowledge,
  mockApplyPlan,
} = vi.hoisted(() => ({
  mockPlansGet: vi.fn(),
  mockPlansApprove: vi.fn(),
  mockPlansDiscard: vi.fn(),
  mockPlansRetry: vi.fn(),
  mockPlansResume: vi.fn(),
  mockPlansApplyStatus: vi.fn(),
  mockPlansCancel: vi.fn(),
  mockPlansConfirmDestructive: vi.fn(),
  mockPlansFreeSpaceEstimate: vi.fn(),
  mockProtectionCheck: vi.fn(),
  mockAcknowledge: vi.fn(),
  mockApplyPlan: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    plansGet: mockPlansGet,
    plansApprove: mockPlansApprove,
    plansDiscard: mockPlansDiscard,
    plansRetry: mockPlansRetry,
    plansResume: mockPlansResume,
    plansApplyStatus: mockPlansApplyStatus,
    plansCancel: mockPlansCancel,
    plansConfirmDestructive: mockPlansConfirmDestructive,
    plansFreeSpaceEstimate: mockPlansFreeSpaceEstimate,
    planProtectionCheckCmd: mockProtectionCheck,
    protectionPlanAcknowledged: mockAcknowledge,
  },
}));

// The apply IPC wrapper bridges a Tauri Channel; mock it so the test drives
// the OperationEvent stream directly (same seam usePlanApplyProgress uses).
vi.mock('./planApply', () => ({
  applyPlan: mockApplyPlan,
}));

import { PlanReviewOverlay } from './PlanReviewOverlay';

/** Wrap a value in the generated `{ status: 'ok' }` Result envelope. */
const ok = <T,>(data: T) => ({ status: 'ok' as const, data });

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

function item(
  overrides: Partial<PlanItemDetail_Serialize> = {},
): PlanItemDetail_Serialize {
  return {
    id: 'item-0',
    index: 0,
    name: 'light_001.xisf',
    action: 'archive',
    from: 'calibrated/light_001.xisf',
    to: '',
    reason: 'intermediate artifact',
    protection: 'normal',
    state: 'pending',
    ...overrides,
  };
}

function plan(
  overrides: Partial<PlanDetail_Serialize> = {},
): PlanDetail_Serialize {
  const items = overrides.items ?? [
    item(),
    item({
      id: 'item-1',
      index: 1,
      name: 'master_dark.xisf',
      from: 'masters/master_dark.xisf',
      protection: 'protected',
    }),
  ];
  return {
    id: 'plan-1',
    number: 1,
    title: 'Cleanup: M31 LRGB',
    origin: 'cleanup',
    state: 'ready_for_review',
    planType: 'cleanup',
    destructiveDestination: 'archive',
    itemsTotal: items.length,
    itemsApplied: 0,
    itemsFailed: 0,
    itemsSkipped: 0,
    itemsCancelled: 0,
    itemsPending: items.length,
    totalBytesRequired: 3000,
    createdAt: '2026-07-01T00:00:00Z',
    ...overrides,
    items,
  };
}

function protectionCheck(
  overrides: Partial<PlanProtectionCheckResponse> = {},
): PlanProtectionCheckResponse {
  return {
    planId: 'plan-1',
    hasProtectedItems: true,
    protectedItems: [
      {
        itemId: 'item-1',
        sourceId: 'project-1',
        level: 'protected',
        reason: 'masters are protected by default',
        matchedCategories: ['masters'],
        originalAction: 'archive',
        rewrittenAction: null,
        requiresAcknowledgement: true,
      },
    ],
    nonBlockingSummary: { normalCount: 1, unprotectedCount: 0 },
    ...overrides,
  };
}

function renderOverlay(
  props: Partial<Parameters<typeof PlanReviewOverlay>[0]> = {},
) {
  return render(
    <PlanReviewOverlay planId="plan-1" open onClose={vi.fn()} {...props} />,
    { wrapper },
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mockPlansGet.mockResolvedValue(ok(plan()));
  mockProtectionCheck.mockResolvedValue(ok(protectionCheck()));
  mockAcknowledge.mockResolvedValue(ok('audit-1'));
  mockPlansApprove.mockResolvedValue(
    ok({
      planId: 'plan-1',
      newState: 'approved',
      approvalToken: 'tok-plan-1',
      approvedAt: '2026-07-01T00:00:00Z',
    }),
  );
  mockPlansDiscard.mockResolvedValue(
    ok({ planId: 'plan-1', discardedAt: '2026-07-01T00:00:00Z' }),
  );
  mockPlansCancel.mockResolvedValue(
    ok({
      planId: 'plan-1',
      cancelledAt: '2026-07-01T00:00:00Z',
      itemsApplied: 0,
      itemsCancelled: 1,
    }),
  );
  mockPlansConfirmDestructive.mockResolvedValue(
    ok({ planId: 'plan-1', itemsConfirmed: 1 }),
  );
  mockPlansFreeSpaceEstimate.mockResolvedValue(
    ok({ requiredBytes: 3000, availableBytes: 5000 }),
  );
  // Default apply: streams item events then a completed terminal event.
  mockApplyPlan.mockImplementation(
    (args: { id: string; onEvent?: (e: OperationEvent) => void }) => {
      const mk = (
        sequence: number,
        eventType: OperationEvent['eventType'],
        payload: unknown,
      ): OperationEvent => ({
        contractVersion: '1.0.0',
        operationId: 'op-1',
        eventType,
        sequence,
        payload,
      });
      args.onEvent?.(mk(0, 'item_started', { itemsTotal: 2 }));
      args.onEvent?.(mk(1, 'item_applied', {}));
      args.onEvent?.(mk(2, 'item_applied', {}));
      args.onEvent?.(mk(3, 'completed', {}));
      return Promise.resolve({
        planId: args.id,
        runId: 'op-1',
        newState: 'applied',
      });
    },
  );
});

describe('PlanReviewOverlay (spec 017 WP-E)', () => {
  it('renders every plan item with source path and protection state', async () => {
    renderOverlay();
    expect(await screen.findByText('light_001.xisf')).toBeInTheDocument();
    expect(screen.getByText('master_dark.xisf')).toBeInTheDocument();
    expect(screen.getByText('calibrated/light_001.xisf')).toBeInTheDocument();
    // Protected item is clearly marked.
    const protectedRow = screen.getByTestId('plan-review-item-1');
    expect(protectedRow).toHaveClass('pv-plan-review__row--protected');
    // Read-only until approval (FR-002 note).
    expect(
      screen.getByText(/Nothing has been changed on disk/),
    ).toBeInTheDocument();
  });

  it('renders the destination for archive items and a deletion cue for delete items (FR-003)', async () => {
    mockPlansGet.mockResolvedValue(
      ok(
        plan({
          items: [
            item({ to: '.astro-plan-archive/plan-1/item-0-light_001.xisf' }),
            item({
              id: 'item-1',
              index: 1,
              name: 'raw_002.fits',
              action: 'delete',
              to: '',
            }),
          ],
        }),
      ),
    );
    renderOverlay();
    expect(
      await screen.findByText(
        '.astro-plan-archive/plan-1/item-0-light_001.xisf',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText('Deleted, not moved')).toBeInTheDocument();
    // #606: From and To must render TOGETHER for the same row — the item's
    // own row, not just somewhere in the table — so a reviewer can see both
    // sides of the move without cross-referencing.
    const row = screen.getByTestId('plan-review-item-0');
    expect(row).toHaveTextContent('calibrated/light_001.xisf');
    expect(row).toHaveTextContent(
      '.astro-plan-archive/plan-1/item-0-light_001.xisf',
    );
  });

  // #607: per-item plan-apply failures were counted but discarded — only an
  // aggregate "N failed" reached the UI, with no way to tell WHICH item
  // failed or WHY without re-running the plan. `plans.get` already persists
  // `state`/`failureReason` per item; the Result column surfaces it.
  it('#607: shows the persisted per-item failure reason, diagnosable without re-running the plan', async () => {
    mockPlansGet.mockResolvedValue(
      ok(
        plan({
          state: 'partially_applied',
          itemsApplied: 1,
          itemsFailed: 1,
          itemsPending: 0,
          items: [
            item({ state: 'succeeded' }),
            item({
              id: 'item-1',
              index: 1,
              name: 'master_dark.xisf',
              from: 'masters/master_dark.xisf',
              protection: 'protected',
              state: 'failed',
              failureReason:
                'protected.source: item is protected by source policy',
            }),
          ],
        }),
      ),
    );
    renderOverlay();

    const failedRow = await screen.findByTestId('plan-review-item-1');
    expect(failedRow).toHaveTextContent('failed');
    expect(failedRow).toHaveTextContent(
      'protected.source: item is protected by source policy',
    );

    const succeededRow = screen.getByTestId('plan-review-item-0');
    expect(succeededRow).toHaveTextContent('succeeded');
  });

  it('shows a muted placeholder in the Result column for an item never applied', async () => {
    renderOverlay();
    await screen.findByTestId('plan-review-item-0');
    expect(screen.getByTestId('plan-review-item-result-0')).toHaveTextContent(
      'None',
    );
  });

  it('keeps Approve & apply disabled until protected items are acknowledged', async () => {
    renderOverlay();
    const approveBtn = await screen.findByTestId('plan-review-approve-apply');
    expect(approveBtn).toBeDisabled();

    fireEvent.click(await screen.findByText('Acknowledge'));
    await waitFor(() => expect(approveBtn).not.toBeDisabled());
    expect(mockAcknowledge).toHaveBeenCalledTimes(1);
  });

  it('approve & apply drives plans.approve → apply with the token and reports completion', async () => {
    const onApplied = vi.fn();
    renderOverlay({ onApplied });

    fireEvent.click(await screen.findByText('Acknowledge'));
    const approveBtn = screen.getByTestId('plan-review-approve-apply');
    await waitFor(() => expect(approveBtn).not.toBeDisabled());
    fireEvent.click(approveBtn);

    await waitFor(() =>
      expect(mockPlansApprove).toHaveBeenCalledWith('plan-1'),
    );
    await waitFor(() =>
      expect(mockApplyPlan).toHaveBeenCalledWith(
        expect.objectContaining({ id: 'plan-1', approvalToken: 'tok-plan-1' }),
      ),
    );
    // Terminal completed state is surfaced (progress UI).
    expect(
      await screen.findByTestId('plan-review-progress'),
    ).toBeInTheDocument();
    expect(screen.getByText('2 items applied')).toBeInTheDocument();
    expect(onApplied).toHaveBeenCalledTimes(1);
  });

  it('discard calls plans.discard and closes the overlay', async () => {
    const onClose = vi.fn();
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    renderOverlay({ onClose });

    fireEvent.click(await screen.findByText('Discard plan'));
    await waitFor(() =>
      expect(mockPlansDiscard).toHaveBeenCalledWith('plan-1'),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('offers "Generate retry plan" after a partially_applied outcome and drives plans.retry (US5, T037)', async () => {
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    mockApplyPlan.mockImplementation(
      (args: { id: string; onEvent?: (e: OperationEvent) => void }) => {
        const mk = (
          sequence: number,
          eventType: OperationEvent['eventType'],
          payload: unknown,
        ): OperationEvent => ({
          contractVersion: '1.0.0',
          operationId: 'op-1',
          eventType,
          sequence,
          payload,
        });
        args.onEvent?.(mk(0, 'item_started', { itemsTotal: 2 }));
        args.onEvent?.(mk(1, 'item_applied', {}));
        args.onEvent?.(mk(2, 'item_failed', {}));
        args.onEvent?.(mk(3, 'failed', {}));
        return Promise.resolve({
          planId: args.id,
          runId: 'op-1',
          newState: 'partially_applied',
        });
      },
    );
    mockPlansRetry.mockResolvedValue(
      ok({ newPlanId: 'plan-2', parentPlanId: 'plan-1', itemsTotal: 1 }),
    );

    const onRetryCreated = vi.fn();
    renderOverlay({ onRetryCreated });

    const approveBtn = await screen.findByTestId('plan-review-approve-apply');
    await waitFor(() => expect(approveBtn).not.toBeDisabled());
    fireEvent.click(approveBtn);

    const retryBtn = await screen.findByTestId('plan-review-retry');
    expect(screen.getByText('1 item failed')).toBeInTheDocument();

    fireEvent.click(retryBtn);
    await waitFor(() =>
      expect(mockPlansRetry).toHaveBeenCalledWith('plan-1', 'failed'),
    );
    await waitFor(() => expect(onRetryCreated).toHaveBeenCalledWith('plan-2'));
  });

  it('shows the catalog message (not "[object Object]") when retry rejects with a ContractError', async () => {
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    mockApplyPlan.mockImplementation(
      (args: { id: string; onEvent?: (e: OperationEvent) => void }) => {
        const mk = (
          sequence: number,
          eventType: OperationEvent['eventType'],
          payload: unknown,
        ): OperationEvent => ({
          contractVersion: '1.0.0',
          operationId: 'op-1',
          eventType,
          sequence,
          payload,
        });
        args.onEvent?.(mk(0, 'item_started', { itemsTotal: 2 }));
        args.onEvent?.(mk(1, 'item_applied', {}));
        args.onEvent?.(mk(2, 'item_failed', {}));
        args.onEvent?.(mk(3, 'failed', {}));
        return Promise.resolve({
          planId: args.id,
          runId: 'op-1',
          newState: 'partially_applied',
        });
      },
    );
    // `plans.retry` rejects with a ContractError — the real shape a Tauri
    // command failure takes on the wire (not a native Error).
    mockPlansRetry.mockRejectedValue({
      code: 'no.items.to.retry',
      message: 'no failed/skipped items on plan plan-1',
    });

    renderOverlay();

    const approveBtn = await screen.findByTestId('plan-review-approve-apply');
    await waitFor(() => expect(approveBtn).not.toBeDisabled());
    fireEvent.click(approveBtn);

    const retryBtn = await screen.findByTestId('plan-review-retry');
    fireEvent.click(retryBtn);

    await waitFor(() =>
      expect(mockPlansRetry).toHaveBeenCalledWith('plan-1', 'failed'),
    );
    expect(
      await screen.findByText('There are no items to retry.'),
    ).toBeInTheDocument();
    expect(screen.queryByText('[object Object]')).not.toBeInTheDocument();
    expect(screen.queryByText('no.items.to.retry')).not.toBeInTheDocument();
  });

  it('shows a paused badge and Resume affordance on a pause condition, and calls plan.resume (R-Pause-1, T048-T050)', async () => {
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    mockApplyPlan.mockImplementation(
      (args: { id: string; onEvent?: (e: OperationEvent) => void }) => {
        const mk = (
          sequence: number,
          eventType: OperationEvent['eventType'],
          payload: unknown,
        ): OperationEvent => ({
          contractVersion: '1.0.0',
          operationId: 'op-1',
          eventType,
          sequence,
          payload,
        });
        args.onEvent?.(
          mk(0, 'item_started', { itemsTotal: 2, runId: 'run-1' }),
        );
        args.onEvent?.(mk(1, 'item_applied', {}));
        args.onEvent?.(
          mk(2, 'warning', {
            runId: 'run-1',
            pauseReason: 'item.stale',
            planId: args.id,
          }),
        );
        // No terminal event — a paused run stops streaming until resumed/cancelled.
        return Promise.resolve({
          planId: args.id,
          runId: 'run-1',
          newState: 'applying',
        });
      },
    );
    mockPlansResume.mockResolvedValue(
      ok({
        planId: 'plan-1',
        runId: 'run-1',
        resumedAt: '2026-07-09T00:00:00Z',
      }),
    );
    // Issue #575 fix + #744 FR-002: `plan.resume` re-spawns the executor but
    // returns no event channel, so this hook polls `plans.apply.status`
    // instead. First tick: still applying. Second: done.
    let statusCall = 0;
    mockPlansApplyStatus.mockImplementation(() => {
      statusCall += 1;
      return Promise.resolve(
        ok(
          statusCall === 1
            ? {
                planId: 'plan-1',
                runId: 'run-1',
                planState: 'applying',
                itemsTotal: 2,
                itemsApplied: 1,
                itemsFailed: 0,
                itemsSkipped: 0,
                itemsCancelled: 0,
                itemsPending: 1,
              }
            : {
                planId: 'plan-1',
                runId: 'run-1',
                planState: 'applied',
                itemsTotal: 2,
                itemsApplied: 2,
                itemsFailed: 0,
                itemsSkipped: 0,
                itemsCancelled: 0,
                itemsPending: 0,
              },
        ),
      );
    });

    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderOverlay();
    const approveBtn = await screen.findByTestId('plan-review-approve-apply');
    await waitFor(() => expect(approveBtn).not.toBeDisabled());
    fireEvent.click(approveBtn);

    expect(
      await screen.findByTestId('plan-review-paused-badge'),
    ).toHaveTextContent('Paused — item.stale');

    fireEvent.click(screen.getByTestId('plan-review-resume'));
    await waitFor(() =>
      expect(mockPlansResume).toHaveBeenCalledWith('plan-1', 'run-1'),
    );
    await waitFor(() =>
      expect(
        screen.queryByTestId('plan-review-paused-badge'),
      ).not.toBeInTheDocument(),
    );

    // #744: the run genuinely continues (issue #575 is fixed) — the overlay
    // now reflects that as real, busy progress instead of a permanent
    // "stalled" dead end. Discard/Approve are disabled while it runs, same
    // as any other in-flight apply; Cancel is the live escape hatch.
    expect(screen.getByTestId('plan-review-cancel-run')).toBeInTheDocument();
    expect(screen.getByText('Discard plan')).toBeDisabled();

    await waitFor(() =>
      expect(screen.getByText(/Applying 1 of 2/)).toBeInTheDocument(),
    );

    await vi.advanceTimersByTimeAsync(1000);
    // Terminal: the footer syncs to the applied state (Close-only, matching
    // the approve-and-apply path) instead of leaving a stale Discard/Approve
    // pair around a plan that already finished applying.
    await waitFor(() =>
      expect(screen.getByText('2 items applied')).toBeInTheDocument(),
    );
    expect(screen.queryByTestId('plan-review-approve-apply')).toBeNull();
    expect(screen.getByText('Close')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('cannot approve a plan with zero items (FR-014)', async () => {
    mockPlansGet.mockResolvedValue(
      ok(plan({ items: [], itemsTotal: 0, itemsPending: 0 })),
    );
    mockProtectionCheck.mockResolvedValue(
      ok(
        protectionCheck({
          hasProtectedItems: false,
          protectedItems: [],
          nonBlockingSummary: { normalCount: 0, unprotectedCount: 0 },
        }),
      ),
    );
    renderOverlay();
    const approveBtn = await screen.findByTestId('plan-review-approve-apply');
    // Gate reports ready (no protected items) but the empty plan still blocks.
    await waitFor(() =>
      expect(screen.getByText(/No protected items/)).toBeInTheDocument(),
    );
    expect(approveBtn).toBeDisabled();
  });

  // #603: a 0-item plan previously dead-ended on the disabled button above
  // with no explanation; the caller-supplied `emptyReason` now renders.
  it('#603: renders the generator-supplied diagnostic for a zero-item plan', async () => {
    mockPlansGet.mockResolvedValue(
      ok(plan({ items: [], itemsTotal: 0, itemsPending: 0 })),
    );
    mockProtectionCheck.mockResolvedValue(
      ok(
        protectionCheck({
          hasProtectedItems: false,
          protectedItems: [],
          nonBlockingSummary: { normalCount: 0, unprotectedCount: 0 },
        }),
      ),
    );
    renderOverlay({
      emptyReason:
        "No files are linked to this project's sources — nothing to archive",
    });
    expect(
      await screen.findByTestId('plan-review-empty-reason'),
    ).toHaveTextContent(
      "No files are linked to this project's sources — nothing to archive",
    );
  });

  it('does not render the empty-reason banner for a non-empty plan even if emptyReason is stale', async () => {
    renderOverlay({ emptyReason: 'stale reason from a prior generate call' });
    await screen.findByText('light_001.xisf');
    expect(
      screen.queryByTestId('plan-review-empty-reason'),
    ).not.toBeInTheDocument();
  });

  it('keeps Approve & apply disabled for a delete item until destructive-confirm succeeds (issue #741)', async () => {
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    mockPlansGet.mockResolvedValue(
      ok(
        plan({
          items: [item({ action: 'delete', to: '' })],
        }),
      ),
    );
    renderOverlay();

    const approveBtn = await screen.findByTestId('plan-review-approve-apply');
    const confirmBox = await screen.findByTestId(
      'plan-review-confirm-destructive',
    );
    // Protection gate is already satisfied (no protected items), but the
    // destructive-confirm checkbox is unchecked — Approve & apply stays
    // blocked until it is confirmed.
    await waitFor(() => expect(approveBtn).toBeDisabled());
    expect(confirmBox).not.toBeChecked();

    fireEvent.click(confirmBox);
    await waitFor(() =>
      expect(mockPlansConfirmDestructive).toHaveBeenCalledWith('plan-1'),
    );
    await waitFor(() => expect(approveBtn).not.toBeDisabled());
  });

  // handoff 06: destructive red is scoped to plans that actually remove data
  // — a move/archive-only plan must not render the destructive button.
  it('renders Approve & apply as primary (not destructive) for a plan with no delete items', async () => {
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    mockPlansGet.mockResolvedValue(
      ok(
        plan({
          items: [item({ action: 'move', to: 'archive/light_001.xisf' })],
        }),
      ),
    );
    renderOverlay();

    // Wait for the plan to load (item name renders) before reading the
    // button's variant class — the button also renders pre-load (disabled),
    // where `plan` is still null and the variant would trivially read
    // 'primary' regardless of the eventual plan content.
    await screen.findByText('light_001.xisf');
    const approveBtn = screen.getByTestId('plan-review-approve-apply');
    await waitFor(() => expect(approveBtn).toHaveAttribute('data-variant', 'primary'));
    expect(approveBtn).not.toHaveAttribute('data-variant', 'destructive');
  });

  it('renders Approve & apply as destructive for a plan with a delete item', async () => {
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    mockPlansGet.mockResolvedValue(
      ok(plan({ items: [item({ action: 'delete', to: '' })] })),
    );
    renderOverlay();

    await screen.findByText('light_001.xisf');
    const approveBtn = screen.getByTestId('plan-review-approve-apply');
    await waitFor(() => expect(approveBtn).toHaveAttribute('data-variant', 'destructive'));
    expect(approveBtn).not.toHaveAttribute('data-variant', 'primary');
  });

  it('offers Cancel apply during a running apply and calls plan.cancel (US3/FR-009, issue #743)', async () => {
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    // Never emits a terminal event — the run stays "running" so Cancel stays
    // available (mirrors the pause test's no-terminal-event pattern above).
    mockApplyPlan.mockImplementation(
      (args: { id: string; onEvent?: (e: OperationEvent) => void }) => {
        args.onEvent?.({
          contractVersion: '1.0.0',
          operationId: 'op-1',
          eventType: 'item_started',
          sequence: 0,
          payload: { itemsTotal: 2, runId: 'run-1' },
        });
        return new Promise(() => {
          /* never resolves within the test — run stays live */
        });
      },
    );

    renderOverlay();
    const approveBtn = await screen.findByTestId('plan-review-approve-apply');
    await waitFor(() => expect(approveBtn).not.toBeDisabled());
    fireEvent.click(approveBtn);

    const cancelBtn = await screen.findByTestId('plan-review-cancel-run');
    fireEvent.click(cancelBtn);
    await waitFor(() => expect(mockPlansCancel).toHaveBeenCalledWith('plan-1'));
  });

  // FR-003 (issue #733): reason/linked entity were present on the DTO but
  // never rendered in the item table.
  it("renders each item's reason and linked entity (FR-003, issue #733)", async () => {
    mockPlansGet.mockResolvedValue(
      ok(
        plan({
          items: [
            item({ reason: 'intermediate artifact', linked: 'project-42' }),
            item({ id: 'item-1', index: 1, name: 'raw_002.fits' }),
          ],
        }),
      ),
    );
    renderOverlay();
    expect(await screen.findAllByText('intermediate artifact')).toHaveLength(2);
    expect(screen.getByText('project-42')).toBeInTheDocument();
    // Second item has no linked entity — rendered as "None", not blank.
    const unlinkedRow = screen.getByTestId('plan-review-item-1');
    expect(unlinkedRow).toHaveTextContent('None');
  });

  // #761 (spec 049 FR-004): a generation/regeneration plan item's resolved
  // link kind (materialization provenance) is on the contract already —
  // the review UI never rendered it before approval.
  it("renders each item's resolved link kind from provenance (#761)", async () => {
    mockPlansGet.mockResolvedValue(
      ok(
        plan({
          items: [
            item({
              provenance: [{ label: 'materialization', value: 'copy' }],
            }),
            // No provenance at all (e.g. an archive/cleanup plan item) —
            // must render "None", not blank or crash.
            item({ id: 'item-1', index: 1, name: 'raw_002.fits' }),
          ],
        }),
      ),
    );
    renderOverlay();
    expect(await screen.findByText('copy')).toBeInTheDocument();
    const noProvenanceRow = screen.getByTestId('plan-review-item-1');
    expect(noProvenanceRow).toHaveTextContent('None');
  });

  // FR-011 (issue #733): a plan reopened from a prior session has no
  // session-local `finalState` — the footer must key off the persisted
  // `plan.state` instead of always rendering the pre-apply pair (which the
  // backend refuses with `plan.invalid_state` on a terminal plan).
  it.each([
    ['applied', 'Close'],
    ['failed', 'Generate retry plan'],
    ['partially_applied', 'Generate retry plan'],
    ['cancelled', 'Generate retry plan'],
  ] as const)(
    'renders the persisted %s plan.state footer on reopen, without a session apply (issue #733)',
    async (state, expectedAction) => {
      mockPlansGet.mockResolvedValue(ok(plan({ state })));
      renderOverlay();

      await screen.findByText('light_001.xisf');
      expect(screen.getByText(expectedAction)).toBeInTheDocument();
      expect(
        screen.queryByTestId('plan-review-approve-apply'),
      ).not.toBeInTheDocument();
      if (expectedAction !== 'Close') {
        expect(screen.getByText('Close')).toBeInTheDocument();
      }
    },
  );

  // #876: a destination free-space estimate, surfaced at review time before
  // approval, rather than only discovering insufficient space after apply.
  it('#876: shows the free-space estimate when the destination has enough room', async () => {
    mockPlansFreeSpaceEstimate.mockResolvedValue(
      ok({ requiredBytes: 3000, availableBytes: 50_000 }),
    );
    renderOverlay();
    const banner = await screen.findByTestId('plan-review-free-space');
    expect(banner).toHaveTextContent('free at the destination');
    expect(banner).not.toHaveTextContent('may not have enough free space');
  });

  it('#876: warns when the destination free-space estimate is insufficient, without disabling approval', async () => {
    mockPlansFreeSpaceEstimate.mockResolvedValue(
      ok({ requiredBytes: 3000, availableBytes: 1000 }),
    );
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    renderOverlay();

    const banner = await screen.findByTestId('plan-review-free-space');
    expect(banner).toHaveTextContent('may not have enough free space');

    // Advisory only — never gates "Approve & apply".
    const approveBtn = await screen.findByTestId('plan-review-approve-apply');
    await waitFor(() => expect(approveBtn).not.toBeDisabled());
  });

  it('#876: renders no free-space banner when the probe returns availableBytes: null (unreachable/unprobeable destination)', async () => {
    mockPlansFreeSpaceEstimate.mockResolvedValue(
      ok({ requiredBytes: 3000, availableBytes: null }),
    );
    mockProtectionCheck.mockResolvedValue(
      ok(protectionCheck({ hasProtectedItems: false, protectedItems: [] })),
    );
    renderOverlay();

    await waitFor(() =>
      expect(mockPlansFreeSpaceEstimate).toHaveBeenCalledWith('plan-1'),
    );
    // Give the item table (which always renders) time to settle so this
    // isn't just "banner hasn't appeared yet".
    await screen.findByText('light_001.xisf');
    expect(
      screen.queryByTestId('plan-review-free-space'),
    ).not.toBeInTheDocument();
  });

  it('#876: renders no free-space banner for a zero-item plan (nothing to probe a destination from)', async () => {
    mockPlansGet.mockResolvedValue(
      ok(plan({ items: [], itemsTotal: 0, itemsPending: 0 })),
    );
    mockProtectionCheck.mockResolvedValue(
      ok(
        protectionCheck({
          hasProtectedItems: false,
          protectedItems: [],
          nonBlockingSummary: { normalCount: 0, unprotectedCount: 0 },
        }),
      ),
    );
    renderOverlay();
    await screen.findByText(/No protected items/);
    expect(mockPlansFreeSpaceEstimate).not.toHaveBeenCalled();
    expect(
      screen.queryByTestId('plan-review-free-space'),
    ).not.toBeInTheDocument();
  });

  it('retries a reopened cancelled plan\'s cancelled items, not "failed" (issue #733)', async () => {
    mockPlansGet.mockResolvedValue(ok(plan({ state: 'cancelled' })));
    mockPlansRetry.mockResolvedValue(
      ok({ newPlanId: 'plan-2', parentPlanId: 'plan-1', itemsTotal: 1 }),
    );
    const onRetryCreated = vi.fn();
    renderOverlay({ onRetryCreated });

    fireEvent.click(await screen.findByTestId('plan-review-retry'));
    await waitFor(() =>
      expect(mockPlansRetry).toHaveBeenCalledWith('plan-1', 'cancelled'),
    );
    await waitFor(() => expect(onRetryCreated).toHaveBeenCalledWith('plan-2'));
  });
});
