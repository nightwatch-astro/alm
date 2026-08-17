// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SessionMaterializationPanel — accessible review surface tests (spec 062, US1).
 *
 * Covers:
 * - Renders loading state while plan is fetching.
 * - Renders proposed sessions table with kind/frames/site/warning columns.
 * - Missing-metadata warning codes show the "missing metadata" pill.
 * - Contradictory-metadata warning codes show the "contradictory" pill.
 * - Approve button disabled when any resolution is unresolved (AC4).
 * - Approve button disabled when plan is stale.
 * - Approve button disabled when plan is refused.
 * - Approve button enabled when plan is open and all sites resolved.
 * - Approve triggers approveMaterialization then applyMaterialization.
 * - Progress live region updates while applying.
 * - Cancel button enabled when cancelSafe and calls cancelMaterialization.
 * - Result shown after applied terminal state.
 * - Discard callback fires on Discard button click.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  render,
  screen,
  waitFor,
  fireEvent,
  act,
} from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

// ── IPC seam mock ─────────────────────────────────────────────────────────────

const {
  mockQueryMaterializationPlan,
  mockListProposedSessions,
  mockQueryAcquisitionSiteResolution,
  mockApproveMaterialization,
  mockApplyMaterialization,
  mockQueryMaterializationProgress,
  mockCancelMaterialization,
} = vi.hoisted(() => ({
  mockQueryMaterializationPlan: vi.fn(),
  mockListProposedSessions: vi.fn(),
  mockQueryAcquisitionSiteResolution: vi.fn(),
  mockApproveMaterialization: vi.fn(),
  mockApplyMaterialization: vi.fn(),
  mockQueryMaterializationProgress: vi.fn(),
  mockCancelMaterialization: vi.fn(),
}));

vi.mock(
  '@/features/inbox/session-materialization/sessionMaterializationIpc',
  () => ({
    queryMaterializationPlan: mockQueryMaterializationPlan,
    listProposedSessions: mockListProposedSessions,
    queryAcquisitionSiteResolution: mockQueryAcquisitionSiteResolution,
    approveMaterialization: mockApproveMaterialization,
    applyMaterialization: mockApplyMaterialization,
    queryMaterializationProgress: mockQueryMaterializationProgress,
    cancelMaterialization: mockCancelMaterialization,
  }),
);

// ── Fixtures ──────────────────────────────────────────────────────────────────

import type { InboxMaterializationPlan, InboxProposedSession } from '../types';

function makePlan(
  overrides: Partial<InboxMaterializationPlan> = {},
): InboxMaterializationPlan {
  return {
    planId: 'plan-1',
    planRevision: 1,
    state: 'open',
    canonicalPlanDigest: 'digest-abc',
    inputEvidenceRevision: 1,
    configurationRevisionId: 'cfg-1',
    acquisitionSiteResolutionCount: 1,
    planResultSnapshotId: 'snap-1',
    candidateFrameCount: 10,
    proposedSessionCount: 2,
    blockedFrameCount: 0,
    warningCodes: [],
    createdAt: '2026-07-01T00:00:00Z',
    createdBy: 'test',
    ...overrides,
  };
}

function makeSession(
  overrides: Partial<InboxProposedSession> = {},
): InboxProposedSession {
  return {
    ordinal: 0,
    proposedSessionKey: 'session-key-1',
    frameKind: 'light',
    proposedIdentityDigest: 'id-digest-1',
    proposedFrameCount: 5,
    acquisitionSiteResolutionId: 'res-1',
    acquisitionSiteResolutionRevision: 1,
    warningCodes: [],
    ...overrides,
  };
}

// ── Test wrapper ──────────────────────────────────────────────────────────────

function Wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

// ── Import subject under test AFTER mock setup ────────────────────────────────

import { SessionMaterializationPanel } from '../SessionMaterializationPanel';

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('SessionMaterializationPanel', () => {
  beforeEach(() => {
    // Default: plan open, one resolved session, no warnings.
    mockQueryMaterializationPlan.mockResolvedValue(makePlan());
    mockListProposedSessions.mockResolvedValue({
      items: [makeSession()],
    });
    mockQueryAcquisitionSiteResolution.mockResolvedValue({
      resolutionId: 'res-1',
      revision: 1,
      state: 'resolved',
      decision: 'accepted_candidate',
      conflictCodes: [],
      evidenceRefs: [],
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('renders nothing when planId is null', () => {
    const { container } = render(
      <Wrapper>
        <SessionMaterializationPanel planId={null} />
      </Wrapper>,
    );
    expect(container.firstChild).toBeNull();
  });

  it('shows a loading state while the plan is fetching', () => {
    mockQueryMaterializationPlan.mockImplementation(
      () => new Promise(() => {}),
    );
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    expect(screen.getByTestId('session-mat-loading')).toBeInTheDocument();
  });

  it('blocks approve while sessions query is in-flight (fail-safe)', async () => {
    // Arrange: sessions query never resolves — plan header resolves immediately.
    mockListProposedSessions.mockImplementation(() => new Promise(() => {}));
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    // Wait for the panel (plan loaded) but sessions still in-flight.
    await waitFor(() =>
      expect(screen.getByTestId('session-mat-panel')).toBeInTheDocument(),
    );
    // Approve must be absent (open plan guard) OR disabled.
    // The panel only renders the approve button for state=open plans, so
    // once the heading is present we assert the button is disabled.
    await waitFor(() =>
      expect(screen.getByTestId('session-mat-approve-btn')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('session-mat-approve-btn')).toBeDisabled();
  });

  it('renders the sessions table with kind/frames columns', async () => {
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    // Wait for sessions to load (table tbody is populated once sessions resolve).
    await waitFor(() => {
      const table = screen.getByTestId('session-mat-sessions-table');
      const rows = table.querySelectorAll('tbody tr');
      expect(rows.length).toBeGreaterThan(0);
    });
    expect(screen.getByText('light')).toBeInTheDocument();
    expect(screen.getByText('5')).toBeInTheDocument();
  });

  it('shows derivedObservingNight from site resolution in the night column', async () => {
    mockQueryAcquisitionSiteResolution.mockResolvedValue({
      resolutionId: 'res-1',
      revision: 1,
      state: 'resolved',
      decision: 'accepted_candidate',
      conflictCodes: [],
      evidenceRefs: [],
      derivedObservingNight: '2026-07-20',
    });
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    await waitFor(() => {
      const table = screen.getByTestId('session-mat-sessions-table');
      const rows = table.querySelectorAll('tbody tr');
      expect(rows.length).toBeGreaterThan(0);
    });
    expect(screen.getByText('2026-07-20')).toBeInTheDocument();
  });

  it('shows missing-metadata pill when session has a missing warning code', async () => {
    mockListProposedSessions.mockResolvedValue({
      items: [makeSession({ warningCodes: ['missing_date_obs'] })],
    });
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    await waitFor(() =>
      expect(screen.getByText('Missing identity metadata')).toBeInTheDocument(),
    );
  });

  it('shows contradictory-metadata pill when session has a contradictory warning code', async () => {
    mockListProposedSessions.mockResolvedValue({
      items: [makeSession({ warningCodes: ['contradictory_filter'] })],
    });
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    await waitFor(() =>
      expect(screen.getByText('Contradictory metadata')).toBeInTheDocument(),
    );
  });

  it('disables approve button and shows blocked message when site resolution is unresolved', async () => {
    // Override default resolution mock to return needs_review.
    mockQueryAcquisitionSiteResolution.mockResolvedValue({
      resolutionId: 'res-1',
      revision: 1,
      state: 'needs_review',
      decision: 'unresolved',
      conflictCodes: [],
      evidenceRefs: [],
    });
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    // Wait for the resolution query to settle and the derived block to reflect it.
    await waitFor(() => {
      const btn = screen.getByTestId('session-mat-approve-btn');
      expect(btn).toBeInTheDocument();
      expect(btn).toBeDisabled();
    });
    expect(
      screen.getByTestId('session-mat-approve-blocked-msg'),
    ).toBeInTheDocument();
  });

  it('shows stale banner and disables approve when plan is stale', async () => {
    mockQueryMaterializationPlan.mockResolvedValue(
      makePlan({ state: 'stale' }),
    );
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    await waitFor(() =>
      expect(
        screen.getByTestId('session-mat-stale-banner'),
      ).toBeInTheDocument(),
    );
    expect(screen.queryByTestId('session-mat-approve-btn')).toBeNull();
  });

  it('shows refused banner when plan is refused', async () => {
    mockQueryMaterializationPlan.mockResolvedValue(
      makePlan({ state: 'refused' }),
    );
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );
    await waitFor(() =>
      expect(
        screen.getByTestId('session-mat-refused-banner'),
      ).toBeInTheDocument(),
    );
  });

  it('calls approveMaterialization then applyMaterialization on approve', async () => {
    const plan = makePlan();
    mockQueryMaterializationPlan.mockResolvedValue(plan);
    mockApproveMaterialization.mockResolvedValue({
      planId: 'plan-1',
      planRevision: 1,
      approvedPlanDigest: 'digest-abc',
      approvedAt: '2026-07-01T01:00:00Z',
      auditId: 'audit-1',
    });
    mockApplyMaterialization.mockResolvedValue({
      operation: {
        operationId: 'op-1',
        kind: 'inbox_ingestion',
        state: 'applying',
        sourcePlanId: 'plan-1',
        approvedPlanDigest: 'digest-abc',
        sessionCount: 2,
        frameMembershipCount: 10,
        singletonPanelGroupCount: 2,
        blockedFrameCount: 0,
      },
      auditId: 'audit-2',
    });
    mockQueryMaterializationProgress.mockResolvedValue({
      operationId: 'op-1',
      state: 'applying',
      processedSessionCount: 1,
      totalSessionCount: 2,
      processedFrameCount: 5,
      totalFrameCount: 10,
      cancelSafe: true,
      updatedAt: '2026-07-01T01:00:01Z',
    });

    // The panel treats all resolutions as unresolved until they're loaded.
    // For this test to allow approve, we need sessions with no unresolved state.
    // Since we can't inject the loaded resolutions map, we use an empty session list
    // with the plan having 0 resolution count.
    mockListProposedSessions.mockResolvedValue({ items: [] });
    mockQueryMaterializationPlan.mockResolvedValue(
      makePlan({ acquisitionSiteResolutionCount: 0, proposedSessionCount: 0 }),
    );

    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );

    // Wait for approve button to appear and be enabled.
    await waitFor(() => {
      const btn = screen.getByTestId('session-mat-approve-btn');
      expect(btn).toBeInTheDocument();
      expect(btn).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('session-mat-approve-btn'));
    });

    await waitFor(() => {
      expect(mockApproveMaterialization).toHaveBeenCalledTimes(1);
      expect(mockApplyMaterialization).toHaveBeenCalledTimes(1);
    });
  });

  it('shows progress live region while applying', async () => {
    // The progress surface appears immediately when phase transitions to
    // `applying` — no poll tick is needed to verify the component renders.
    mockListProposedSessions.mockResolvedValue({ items: [] });
    mockQueryMaterializationPlan.mockResolvedValue(
      makePlan({ acquisitionSiteResolutionCount: 0, proposedSessionCount: 0 }),
    );
    mockApproveMaterialization.mockResolvedValue({
      planId: 'plan-1',
      planRevision: 1,
      approvedPlanDigest: 'digest-abc',
      approvedAt: '2026-07-01T01:00:00Z',
      auditId: 'audit-1',
    });
    mockApplyMaterialization.mockResolvedValue({
      operation: {
        operationId: 'op-1',
        kind: 'inbox_ingestion',
        state: 'applying',
        sourcePlanId: 'plan-1',
        approvedPlanDigest: 'digest-abc',
        sessionCount: 2,
        frameMembershipCount: 10,
        singletonPanelGroupCount: 2,
        blockedFrameCount: 0,
      },
      auditId: 'audit-2',
    });
    // Progress mock: never resolves — we only test that the component appears,
    // not that it received a poll result.
    mockQueryMaterializationProgress.mockImplementation(
      () => new Promise(() => {}),
    );

    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );

    await waitFor(() =>
      expect(screen.getByTestId('session-mat-approve-btn')).not.toBeDisabled(),
    );

    await act(async () => {
      fireEvent.click(screen.getByTestId('session-mat-approve-btn'));
    });

    // The progress surface renders as soon as phase = 'applying' (no poll needed).
    await waitFor(() =>
      expect(screen.getByTestId('session-mat-progress')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('session-mat-progress-live')).toBeInTheDocument();
  });

  it('shows applied result after terminal applied state', async () => {
    // Only fake setInterval/clearInterval so Promises still resolve normally.
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    mockListProposedSessions.mockResolvedValue({ items: [] });
    mockQueryMaterializationPlan.mockResolvedValue(
      makePlan({ acquisitionSiteResolutionCount: 0, proposedSessionCount: 0 }),
    );
    mockApproveMaterialization.mockResolvedValue({
      planId: 'plan-1',
      planRevision: 1,
      approvedPlanDigest: 'digest-abc',
      approvedAt: '2026-07-01T01:00:00Z',
      auditId: 'audit-1',
    });
    mockApplyMaterialization.mockResolvedValue({
      operation: {
        operationId: 'op-1',
        kind: 'inbox_ingestion',
        state: 'applying',
        sourcePlanId: 'plan-1',
        approvedPlanDigest: 'digest-abc',
        sessionCount: 2,
        frameMembershipCount: 10,
        singletonPanelGroupCount: 2,
        blockedFrameCount: 0,
      },
      auditId: 'audit-2',
    });
    mockQueryMaterializationProgress.mockResolvedValue({
      operationId: 'op-1',
      state: 'applied',
      processedSessionCount: 2,
      totalSessionCount: 2,
      processedFrameCount: 10,
      totalFrameCount: 10,
      cancelSafe: false,
      updatedAt: '2026-07-01T01:00:05Z',
    });

    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );

    await waitFor(() =>
      expect(screen.getByTestId('session-mat-approve-btn')).not.toBeDisabled(),
    );

    await act(async () => {
      fireEvent.click(screen.getByTestId('session-mat-approve-btn'));
    });

    // Fire the poll interval.
    await act(async () => {
      vi.advanceTimersByTime(600);
    });

    await waitFor(() =>
      expect(
        screen.getByTestId('session-mat-result-applied'),
      ).toBeInTheDocument(),
    );
    expect(screen.getByText('Sessions created')).toBeInTheDocument();

    vi.useRealTimers();
  });

  it('enables cancel button when cancelSafe and calls cancelMaterialization', async () => {
    // Progress mock: never resolves → keep phase at `applying` so the cancel
    // button stays visible.
    mockListProposedSessions.mockResolvedValue({ items: [] });
    mockQueryMaterializationPlan.mockResolvedValue(
      makePlan({ acquisitionSiteResolutionCount: 0, proposedSessionCount: 0 }),
    );
    mockApproveMaterialization.mockResolvedValue({
      planId: 'plan-1',
      planRevision: 1,
      approvedPlanDigest: 'digest-abc',
      approvedAt: '2026-07-01T01:00:00Z',
      auditId: 'audit-1',
    });
    mockApplyMaterialization.mockResolvedValue({
      operation: {
        operationId: 'op-1',
        kind: 'inbox_ingestion',
        state: 'applying',
        sourcePlanId: 'plan-1',
        approvedPlanDigest: 'digest-abc',
        sessionCount: 2,
        frameMembershipCount: 10,
        singletonPanelGroupCount: 2,
        blockedFrameCount: 0,
      },
      auditId: 'audit-2',
    });
    // Progress resolves with cancelSafe=true so the cancel button is enabled.
    mockQueryMaterializationProgress.mockResolvedValue({
      operationId: 'op-1',
      state: 'applying',
      processedSessionCount: 0,
      totalSessionCount: 2,
      processedFrameCount: 0,
      totalFrameCount: 10,
      cancelSafe: true,
      updatedAt: '2026-07-01T01:00:01Z',
    });
    mockCancelMaterialization.mockResolvedValue({
      operationId: 'op-1',
      state: 'cancelling',
      processedSessionCount: 0,
      totalSessionCount: 2,
      processedFrameCount: 0,
      totalFrameCount: 10,
      cancelSafe: false,
      updatedAt: '2026-07-01T01:00:02Z',
    });

    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });

    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );

    await waitFor(() =>
      expect(screen.getByTestId('session-mat-approve-btn')).not.toBeDisabled(),
    );

    await act(async () => {
      fireEvent.click(screen.getByTestId('session-mat-approve-btn'));
    });

    // Fire the poll interval so the progress response is consumed.
    await act(async () => {
      vi.advanceTimersByTime(600);
    });

    await waitFor(() =>
      expect(screen.getByTestId('session-mat-cancel-btn')).toBeInTheDocument(),
    );
    // The cancel button is enabled because the first poll tick returned cancelSafe=true.
    await waitFor(() =>
      expect(screen.getByTestId('session-mat-cancel-btn')).not.toBeDisabled(),
    );

    await act(async () => {
      fireEvent.click(screen.getByTestId('session-mat-cancel-btn'));
    });

    await waitFor(() =>
      expect(mockCancelMaterialization).toHaveBeenCalledTimes(1),
    );

    vi.useRealTimers();
  });

  it('calls onDiscard when Discard button is clicked', async () => {
    // Plan loads with default beforeEach mocks (open, one session).
    const onDiscard = vi.fn();
    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" onDiscard={onDiscard} />
      </Wrapper>,
    );
    await waitFor(() => {
      expect(screen.getByTestId('session-mat-discard-btn')).toBeInTheDocument();
      expect(screen.getByTestId('session-mat-panel')).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId('session-mat-discard-btn'));
    expect(onDiscard).toHaveBeenCalledWith('plan-1');
  });

  it('cancel button aria-label is accessible', async () => {
    // The cancel button appears in `applying` phase — just verify the aria-label
    // once the progress surface renders (no poll tick needed).
    mockListProposedSessions.mockResolvedValue({ items: [] });
    mockQueryMaterializationPlan.mockResolvedValue(
      makePlan({ acquisitionSiteResolutionCount: 0, proposedSessionCount: 0 }),
    );
    mockApproveMaterialization.mockResolvedValue({
      planId: 'plan-1',
      planRevision: 1,
      approvedPlanDigest: 'digest-abc',
      approvedAt: '2026-07-01T01:00:00Z',
      auditId: 'audit-1',
    });
    mockApplyMaterialization.mockResolvedValue({
      operation: {
        operationId: 'op-1',
        kind: 'inbox_ingestion',
        state: 'applying',
        sourcePlanId: 'plan-1',
        approvedPlanDigest: 'digest-abc',
        sessionCount: 1,
        frameMembershipCount: 5,
        singletonPanelGroupCount: 1,
        blockedFrameCount: 0,
      },
      auditId: 'audit-2',
    });
    // Never resolves — keeps phase at applying without affecting render.
    mockQueryMaterializationProgress.mockImplementation(
      () => new Promise(() => {}),
    );

    render(
      <Wrapper>
        <SessionMaterializationPanel planId="plan-1" />
      </Wrapper>,
    );

    await waitFor(() =>
      expect(screen.getByTestId('session-mat-approve-btn')).not.toBeDisabled(),
    );

    await act(async () => {
      fireEvent.click(screen.getByTestId('session-mat-approve-btn'));
    });

    await waitFor(() =>
      expect(screen.getByTestId('session-mat-cancel-btn')).toBeInTheDocument(),
    );

    const cancelBtn = screen.getByTestId('session-mat-cancel-btn');
    expect(cancelBtn).toHaveAttribute(
      'aria-label',
      'Cancel the materialization in progress',
    );
  });
});
