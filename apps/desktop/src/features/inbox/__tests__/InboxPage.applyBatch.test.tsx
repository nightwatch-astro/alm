// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * astro-plan-tykek — the batch Apply gestures must record the user's approval
 * before the plans are applied.
 *
 * `inbox.plan.apply_all` / `inbox.plan.apply_selected` used to reach a backend
 * that minted its own approval (`approve_plan(actor = "inbox.apply")`), so the
 * approval-token gate could never fail on the path the Inbox UI actually
 * traverses. The backend now refuses an unapproved plan, which makes these two
 * handlers responsible for approving what the user reviewed — the same shape
 * `handleApplyOne` has carried since #769/#609.
 */

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import {
  render as rtlRender,
  screen,
  fireEvent,
  waitFor,
} from '@testing-library/react';
import type { ReactElement } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PageStatusProvider } from '@/app/PageStatusContext';
import type { InboxOpenPlan } from '@/bindings/index';

function render(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return rtlRender(
    <QueryClientProvider client={queryClient}>
      <PageStatusProvider>{ui}</PageStatusProvider>
    </QueryClientProvider>,
  );
}

const {
  mockRootsList,
  mockInboxList,
  mockInboxPlanListOpen,
  mockPlansApprove,
  mockInboxPlanApplyAll,
  mockInboxPlanApplySelected,
  mockAddToast,
  mockNavigate,
} = vi.hoisted(() => ({
  mockRootsList: vi.fn(),
  mockInboxList: vi.fn(),
  mockInboxPlanListOpen: vi.fn(),
  mockPlansApprove: vi.fn(),
  mockInboxPlanApplyAll: vi.fn(),
  mockInboxPlanApplySelected: vi.fn(),
  mockAddToast: vi.fn(),
  mockNavigate: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    rootsList: mockRootsList,
    inboxList: mockInboxList,
    inboxPlanListOpen: mockInboxPlanListOpen,
    plansApprove: mockPlansApprove,
    inboxPlanApplyAll: mockInboxPlanApplyAll,
    inboxPlanApplySelected: mockInboxPlanApplySelected,
  },
}));

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
  useSearch: () => ({ selected: undefined, type: undefined }),
}));

vi.mock('@/shared/toast', () => ({
  addToast: mockAddToast,
  useToasts: () => ({ toasts: [], dismiss: vi.fn() }),
}));

const ok = <T,>(data: T) => ({ status: 'ok' as const, data });

function makePlan(n: number): InboxOpenPlan {
  return {
    inboxItemId: `item-plan-00${n}`,
    itemName: `lights/NGC700${n}`,
    planId: `plan-00${n}`,
    state: 'ready_for_review',
    stale: false,
    actions: [
      {
        index: 1,
        action: 'move',
        fromPath: `lights/NGC700${n}/frame_001.fits`,
        toPath: `M31/light/frame_00${n}.fits`,
        destinationPreview: `M31/light/frame_00${n}.fits`,
        requiresDestructiveConfirm: false,
      },
    ],
  };
}

const planA = makePlan(1);
const planB = makePlan(2);

beforeEach(() => {
  vi.clearAllMocks();
  mockRootsList.mockResolvedValue(ok([]));
  mockInboxList.mockResolvedValue(ok({ items: [], capped: false, limit: 500 }));
  mockInboxPlanListOpen.mockResolvedValue(
    ok({ plans: [planA, planB], totalActions: 2 }),
  );
  mockPlansApprove.mockImplementation((planId: string) =>
    Promise.resolve(
      ok({
        planId,
        newState: 'approved',
        approvalToken: `tok-${planId}`,
        approvedAt: '2026-08-24T00:00:00Z',
      }),
    ),
  );
  mockInboxPlanApplyAll.mockResolvedValue(ok({ results: [] }));
  mockInboxPlanApplySelected.mockResolvedValue(ok({ results: [] }));
});

import { InboxPage } from '../InboxPage';

async function openOverlay() {
  render(<InboxPage />);
  fireEvent.click(await screen.findByTestId('inbox-review-plans-btn'));
}

/** Assert every `plans.approve` call landed before the batch apply IPC. */
function expectApprovedBefore(applyMock: {
  mock: { invocationCallOrder: number[] };
}) {
  const applyOrder = applyMock.mock.invocationCallOrder[0];
  for (const order of mockPlansApprove.mock.invocationCallOrder) {
    expect(order).toBeLessThan(applyOrder);
  }
}

describe('InboxPage batch apply — approve before apply (astro-plan-tykek)', () => {
  it('approves every displayed plan before inbox.plan.apply_all', async () => {
    await openOverlay();

    fireEvent.click(await screen.findByTestId('plan-apply-all'));

    await waitFor(() => expect(mockInboxPlanApplyAll).toHaveBeenCalled());
    expect(mockPlansApprove.mock.calls.map((c) => c[0]).sort()).toEqual([
      planA.planId,
      planB.planId,
    ]);
    expectApprovedBefore(mockInboxPlanApplyAll);
  });

  it('approves the selected plans before inbox.plan.apply_selected', async () => {
    await openOverlay();

    fireEvent.click(await screen.findByTestId('plan-select-all'));
    fireEvent.click(await screen.findByTestId('plan-apply-selected'));

    await waitFor(() => expect(mockInboxPlanApplySelected).toHaveBeenCalled());
    expect(mockPlansApprove.mock.calls.map((c) => c[0]).sort()).toEqual([
      planA.planId,
      planB.planId,
    ]);
    expectApprovedBefore(mockInboxPlanApplySelected);
  });
});
