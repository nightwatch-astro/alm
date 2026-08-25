// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * ProjectDetail archive-plan wiring tests (spec 017 US2/WP-B UI-gap fix).
 *
 * `archive.plan.generate` previously had zero UI callers: the completed →
 * archived transition dead-ended on a "create or approve a plan first" info
 * toast with no way to actually create the plan (the flow only worked driven
 * over the dev bridge). Verifies that a plan.required refusal on the
 * archived edge now:
 *
 * 1. still surfaces the exact info toast (unchanged, still plan-gated —
 *    no silent lifecycle flip);
 * 2. calls `archive.plan.generate` for the project (the shared
 *    `useGenerateArchivePlan` mutation);
 * 3. opens the shared `PlanReviewOverlay` with the returned plan id;
 * 4. surfaces an error toast (and no overlay) when generation itself fails.
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tanstack/react-router', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('@tanstack/react-router')>();
  return {
    ...actual,
    // ProjectLifecycleStepper's History section (#833) falls back to this
    // route search param when its (optional) projectId prop isn't wired;
    // unrelated to this file's assertions, so a static empty selection is
    // enough.
    useSearch: () => ({ selected: undefined, lifecycle: undefined }),
    // #735: the tool-launch toast's "Configure path" action now navigates
    // through the router instead of assigning window.location.hash, and the
    // tool-not-configured hint is a real `Link` — neither works against the
    // spread-in real implementations without a router context.
    useNavigate: () => vi.fn(),
    Link: (await import('@/test/router-link-stub')).LinkStub,
  };
});

vi.mock('./store', async (importOriginal) => {
  const original = await importOriginal<typeof import('./store')>();
  return {
    ...original,
    useProjectDetail: vi.fn(),
    useSessionNames: vi.fn(() => new Map()),
    callTransitionLifecycle: vi.fn(),
    callReinferChannels: vi.fn(),
    callDismissChannelDrift: vi.fn(),
    // Avoids requiring a QueryClientProvider for this file's real-useQuery
    // History query (#833) — same reasoning as useProjectDetail above.
    useProjectHistory: vi.fn(() => ({
      data: [],
      loading: false,
      error: undefined,
    })),
  };
});

vi.mock('@/shared/toast', () => ({
  addToast: vi.fn(),
}));

const { mockGenerateArchivePlan } = vi.hoisted(() => ({
  mockGenerateArchivePlan: vi.fn(),
}));

vi.mock('@/features/archive/store', () => ({
  useGenerateArchivePlan: () => ({
    mutateAsync: mockGenerateArchivePlan,
    isPending: false,
  }),
}));

vi.mock('@/features/plans/PlanReviewOverlay', () => ({
  PlanReviewOverlay: ({
    planId,
    open,
    emptyReason,
  }: {
    planId: string | null;
    open: boolean;
    emptyReason?: string | null;
  }) =>
    open ? (
      <div
        data-testid="archive-plan-review-stub"
        data-empty-reason={emptyReason ?? ''}
      >
        {planId}
      </div>
    ) : null,
}));

import { ProjectDetailContent } from './ProjectDetail';
import * as store from './store';
import { addToast } from '@/shared/toast';
import type { ProjectDetailDto } from '@/bindings/index';

const mockProject: ProjectDetailDto = {
  id: 'proj-001',
  name: 'NGC 7000',
  tool: 'PixInsight',
  lifecycle: 'completed',
  path: 'projects/NGC7000',
  notes: null,
  channelDrift: { hasNewSources: false, suggestedAction: 'dismiss' },
  sources: [],
  channels: [],
  createdAt: '2026-06-01T00:00:00Z',
  updatedAt: '2026-06-10T00:00:00Z',
};

/** The plan-gated Prepare refusal mounts the source-view generate dialog,
 *  which reads the settings query. */
function renderPane() {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ProjectDetailContent projectId="proj-001" />
    </QueryClientProvider>,
  );
}

function setupStore(project: Partial<ProjectDetailDto> = {}) {
  vi.mocked(store.useProjectDetail).mockReturnValue({
    data: { ...mockProject, ...project },
    loading: false,
    error: undefined,
  });
}

describe('ProjectDetail archive plan generation (spec 017 US2/WP-B)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('generates and opens the review overlay when the Archive transition returns plan.required', async () => {
    setupStore({ lifecycle: 'completed' });
    vi.mocked(store.callTransitionLifecycle).mockResolvedValue({
      status: 'error',
      contractVersion: '2.0.0',
      requestId: 'req-1',
      error: { code: 'plan.required', message: 'Plan required' },
    });
    mockGenerateArchivePlan.mockResolvedValue({
      planId: 'plan-archive-1',
      itemCount: 3,
      protectedItemCount: 0,
    });

    renderPane();
    fireEvent.click(screen.getByTestId('transition-btn-archived'));

    // Still plan-gated: the exact info toast fires (no silent lifecycle flip).
    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        expect.objectContaining({ variant: 'info' }),
      );
    });

    // ...and it now actually generates the plan for this project.
    await waitFor(() => {
      expect(mockGenerateArchivePlan).toHaveBeenCalledWith('proj-001');
    });

    // ...and opens the shared review/apply overlay with the returned plan id.
    await waitFor(() => {
      expect(screen.getByTestId('archive-plan-review-stub')).toHaveTextContent(
        'plan-archive-1',
      );
    });
  });

  // #603: `archive.plan.generate`'s `emptyReason` (set only for a 0-item
  // plan) is forwarded to the overlay so it can render the diagnostic
  // instead of a bare disabled "Approve & apply".
  it('forwards emptyReason from archive.plan.generate to the review overlay (#603)', async () => {
    setupStore({ lifecycle: 'completed' });
    vi.mocked(store.callTransitionLifecycle).mockResolvedValue({
      status: 'error',
      contractVersion: '2.0.0',
      requestId: 'req-4',
      error: { code: 'plan.required', message: 'Plan required' },
    });
    mockGenerateArchivePlan.mockResolvedValue({
      planId: 'plan-archive-empty',
      itemCount: 0,
      protectedItemCount: 0,
      emptyReason:
        "No files are linked to this project's sources — nothing to archive",
    });

    renderPane();
    fireEvent.click(screen.getByTestId('transition-btn-archived'));

    await waitFor(() => {
      expect(screen.getByTestId('archive-plan-review-stub')).toHaveAttribute(
        'data-empty-reason',
        "No files are linked to this project's sources — nothing to archive",
      );
    });
  });

  it('does not generate a plan for a plan-required refusal on a different edge', async () => {
    setupStore({ lifecycle: 'ready' });
    vi.mocked(store.callTransitionLifecycle).mockResolvedValue({
      status: 'error',
      contractVersion: '2.0.0',
      requestId: 'req-2',
      error: { code: 'plan.required', message: 'Plan required' },
    });

    renderPane();
    fireEvent.click(screen.getByTestId('transition-btn-prepared'));

    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        expect.objectContaining({ variant: 'info' }),
      );
    });
    expect(mockGenerateArchivePlan).not.toHaveBeenCalled();
    expect(
      screen.queryByTestId('archive-plan-review-stub'),
    ).not.toBeInTheDocument();
  });

  it('surfaces an error toast and keeps the overlay closed when plan generation fails', async () => {
    setupStore({ lifecycle: 'completed' });
    vi.mocked(store.callTransitionLifecycle).mockResolvedValue({
      status: 'error',
      contractVersion: '2.0.0',
      requestId: 'req-3',
      error: { code: 'plan.required', message: 'Plan required' },
    });
    mockGenerateArchivePlan.mockRejectedValue(new Error('db failure'));

    renderPane();
    fireEvent.click(screen.getByTestId('transition-btn-archived'));

    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        expect.objectContaining({ variant: 'error' }),
      );
    });
    expect(
      screen.queryByTestId('archive-plan-review-stub'),
    ).not.toBeInTheDocument();
  });
});
