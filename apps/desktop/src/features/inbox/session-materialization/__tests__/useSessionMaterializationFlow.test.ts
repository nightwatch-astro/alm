// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * useSessionMaterializationFlow — hook unit tests (spec 062, US1).
 *
 * Covers:
 * - Initial state is idle.
 * - handleApprove calls approveMaterialization then applyMaterialization.
 * - handleApprove sets phase to `failed` on approval error.
 * - handleApprove sets phase to `failed` on apply error.
 * - Progress polling transitions phase from applying → applied on terminal.
 * - Progress polling transitions phase → cancelled on cancel terminal.
 * - handleCancel calls cancelMaterialization.
 * - reset returns to idle state.
 * - Error code extraction from ContractError shape.
 */

import { renderHook, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { ReactNode } from 'react';
import React from 'react';

// ── IPC seam mock ─────────────────────────────────────────────────────────────

const {
  mockApproveMaterialization,
  mockApplyMaterialization,
  mockQueryMaterializationProgress,
  mockCancelMaterialization,
} = vi.hoisted(() => ({
  mockApproveMaterialization: vi.fn(),
  mockApplyMaterialization: vi.fn(),
  mockQueryMaterializationProgress: vi.fn(),
  mockCancelMaterialization: vi.fn(),
}));

vi.mock(
  '@/features/inbox/session-materialization/sessionMaterializationIpc',
  () => ({
    queryMaterializationPlan: vi.fn(),
    listProposedSessions: vi.fn(),
    queryAcquisitionSiteResolution: vi.fn(),
    approveMaterialization: mockApproveMaterialization,
    applyMaterialization: mockApplyMaterialization,
    queryMaterializationProgress: mockQueryMaterializationProgress,
    cancelMaterialization: mockCancelMaterialization,
    discardMaterialization: vi.fn(),
  }),
);

import { useSessionMaterializationFlow } from '../useSessionMaterializationFlow';
import type { InboxMaterializationPlan } from '../types';

// ── Fixtures ──────────────────────────────────────────────────────────────────

const TEST_PLAN: InboxMaterializationPlan = {
  planId: 'plan-1',
  planRevision: 1,
  state: 'open',
  canonicalPlanDigest: 'digest-abc',
  inputEvidenceRevision: 1,
  configurationRevisionId: 'cfg-1',
  acquisitionSiteResolutionCount: 0,
  planResultSnapshotId: 'snap-1',
  candidateFrameCount: 5,
  proposedSessionCount: 1,
  blockedFrameCount: 0,
  warningCodes: [],
  createdAt: '2026-07-01T00:00:00Z',
  createdBy: 'test',
};

const APPROVE_RESP = {
  planId: 'plan-1',
  planRevision: 1,
  approvedPlanDigest: 'digest-abc',
  approvedAt: '2026-07-01T01:00:00Z',
  auditId: 'audit-1',
};

const APPLY_RESP = {
  operation: {
    operationId: 'op-1',
    kind: 'inbox_ingestion' as const,
    state: 'applying' as const,
    sourcePlanId: 'plan-1',
    approvedPlanDigest: 'digest-abc',
    sessionCount: 1,
    frameMembershipCount: 5,
    singletonPanelGroupCount: 1,
    blockedFrameCount: 0,
  },
  auditId: 'audit-2',
};

// ── Wrapper ───────────────────────────────────────────────────────────────────

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return React.createElement(
    QueryClientProvider,
    { client: queryClient },
    children,
  );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('useSessionMaterializationFlow', () => {
  beforeEach(() => {
    // Only fake setInterval/clearInterval so Promises still resolve naturally.
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('starts in idle phase', () => {
    const { result } = renderHook(
      () => useSessionMaterializationFlow('plan-1'),
      { wrapper },
    );
    expect(result.current.state.phase).toBe('idle');
    expect(result.current.state.operation).toBeNull();
  });

  it('calls approveMaterialization then applyMaterialization on handleApprove', async () => {
    mockApproveMaterialization.mockResolvedValue(APPROVE_RESP);
    mockApplyMaterialization.mockResolvedValue(APPLY_RESP);
    mockQueryMaterializationProgress.mockResolvedValue({
      operationId: 'op-1',
      state: 'applying',
      processedSessionCount: 0,
      totalSessionCount: 1,
      processedFrameCount: 0,
      totalFrameCount: 5,
      cancelSafe: true,
      updatedAt: '2026-07-01T01:00:01Z',
    });

    const { result } = renderHook(
      () => useSessionMaterializationFlow('plan-1'),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleApprove(TEST_PLAN);
    });

    expect(mockApproveMaterialization).toHaveBeenCalledOnce();
    expect(mockApplyMaterialization).toHaveBeenCalledOnce();
    expect(result.current.state.phase).toBe('applying');
    expect(result.current.state.operation?.operationId).toBe('op-1');
  });

  it('transitions to failed when approval throws a ContractError', async () => {
    mockApproveMaterialization.mockRejectedValue({
      code: 'inbox.plan_stale',
      message: 'Plan is stale',
    });

    const { result } = renderHook(
      () => useSessionMaterializationFlow('plan-1'),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleApprove(TEST_PLAN);
    });

    expect(result.current.state.phase).toBe('failed');
    expect(result.current.state.errorCode).toBe('inbox.plan_stale');
    expect(mockApplyMaterialization).not.toHaveBeenCalled();
  });

  it('transitions to failed when apply throws', async () => {
    mockApproveMaterialization.mockResolvedValue(APPROVE_RESP);
    mockApplyMaterialization.mockRejectedValue({
      code: 'inbox.plan_not_approved',
      message: 'Plan is not approved',
    });

    const { result } = renderHook(
      () => useSessionMaterializationFlow('plan-1'),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleApprove(TEST_PLAN);
    });

    expect(result.current.state.phase).toBe('failed');
    expect(result.current.state.errorCode).toBe('inbox.plan_not_approved');
  });

  it('transitions to applied after terminal applied progress', async () => {
    mockApproveMaterialization.mockResolvedValue(APPROVE_RESP);
    mockApplyMaterialization.mockResolvedValue(APPLY_RESP);
    mockQueryMaterializationProgress.mockResolvedValue({
      operationId: 'op-1',
      state: 'applied',
      processedSessionCount: 1,
      totalSessionCount: 1,
      processedFrameCount: 5,
      totalFrameCount: 5,
      cancelSafe: false,
      updatedAt: '2026-07-01T01:00:05Z',
    });

    const onApplied = vi.fn();
    const { result } = renderHook(
      () => useSessionMaterializationFlow('plan-1', onApplied),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleApprove(TEST_PLAN);
    });

    await act(async () => {
      vi.advanceTimersByTime(600);
    });

    expect(result.current.state.phase).toBe('applied');
    expect(onApplied).toHaveBeenCalledOnce();
  });

  it('transitions to cancelled after terminal cancelled progress', async () => {
    mockApproveMaterialization.mockResolvedValue(APPROVE_RESP);
    mockApplyMaterialization.mockResolvedValue(APPLY_RESP);
    mockQueryMaterializationProgress.mockResolvedValue({
      operationId: 'op-1',
      state: 'cancelled',
      processedSessionCount: 0,
      totalSessionCount: 1,
      processedFrameCount: 0,
      totalFrameCount: 5,
      cancelSafe: false,
      updatedAt: '2026-07-01T01:00:03Z',
    });

    const { result } = renderHook(
      () => useSessionMaterializationFlow('plan-1'),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleApprove(TEST_PLAN);
    });

    await act(async () => {
      vi.advanceTimersByTime(600);
    });

    expect(result.current.state.phase).toBe('cancelled');
  });

  it('calls cancelMaterialization on handleCancel when operationId is known', async () => {
    mockApproveMaterialization.mockResolvedValue(APPROVE_RESP);
    mockApplyMaterialization.mockResolvedValue(APPLY_RESP);
    mockQueryMaterializationProgress.mockResolvedValue({
      operationId: 'op-1',
      state: 'applying',
      processedSessionCount: 0,
      totalSessionCount: 1,
      processedFrameCount: 0,
      totalFrameCount: 5,
      cancelSafe: true,
      updatedAt: '2026-07-01T01:00:01Z',
    });
    mockCancelMaterialization.mockResolvedValue({
      operationId: 'op-1',
      state: 'cancelling',
      processedSessionCount: 0,
      totalSessionCount: 1,
      processedFrameCount: 0,
      totalFrameCount: 5,
      cancelSafe: false,
      updatedAt: '2026-07-01T01:00:02Z',
    });

    const { result } = renderHook(
      () => useSessionMaterializationFlow('plan-1'),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleApprove(TEST_PLAN);
    });

    await act(async () => {
      await result.current.handleCancel();
    });

    expect(mockCancelMaterialization).toHaveBeenCalledOnce();
    expect(mockCancelMaterialization.mock.calls[0][0]).toMatchObject({
      operationId: 'op-1',
    });
  });

  it('reset returns to idle and clears state', async () => {
    mockApproveMaterialization.mockResolvedValue(APPROVE_RESP);
    mockApplyMaterialization.mockResolvedValue(APPLY_RESP);
    mockQueryMaterializationProgress.mockResolvedValue({
      operationId: 'op-1',
      state: 'applied',
      processedSessionCount: 1,
      totalSessionCount: 1,
      processedFrameCount: 5,
      totalFrameCount: 5,
      cancelSafe: false,
      updatedAt: '2026-07-01T01:00:05Z',
    });

    const { result } = renderHook(
      () => useSessionMaterializationFlow('plan-1'),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleApprove(TEST_PLAN);
    });

    await act(async () => {
      vi.advanceTimersByTime(600);
    });

    expect(result.current.state.phase).toBe('applied');

    act(() => {
      result.current.reset();
    });

    expect(result.current.state.phase).toBe('idle');
    expect(result.current.state.operation).toBeNull();
    expect(result.current.state.progress).toBeNull();
  });

  it('does nothing on handleApprove when planId is null', async () => {
    const { result } = renderHook(() => useSessionMaterializationFlow(null), {
      wrapper,
    });

    await act(async () => {
      await result.current.handleApprove(TEST_PLAN);
    });

    expect(mockApproveMaterialization).not.toHaveBeenCalled();
    expect(result.current.state.phase).toBe('idle');
  });
});
