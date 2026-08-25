// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * ProjectDetail Prepare-edge plan wiring (astro-plan-krqge).
 *
 * `ready → prepared` is plan-gated and the backend refuses it with
 * `plan.required`. The pane answered with an info toast only — no route to the
 * source-view generation plan the edge requires — so the project stayed `ready`
 * with nothing linked. Mirrors the Archive edge: refusal → generate → review.
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
    useSearch: () => ({ selected: undefined, lifecycle: undefined }),
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

const { mockGenerateSourceView } = vi.hoisted(() => ({
  mockGenerateSourceView: vi.fn(),
}));

vi.mock('./source-views', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./source-views')>();
  return { ...actual, generateSourceView: mockGenerateSourceView };
});

vi.mock('@/features/archive/store', () => ({
  useGenerateArchivePlan: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
  }),
}));

vi.mock('@/features/plans/PlanReviewOverlay', () => ({
  PlanReviewOverlay: ({
    planId,
    open,
    title,
  }: {
    planId: string | null;
    open: boolean;
    title: string;
  }) =>
    open ? (
      <div data-testid="plan-review-stub" data-title={title}>
        {planId}
      </div>
    ) : null,
}));

import { ProjectDetailContent } from './ProjectDetail';
import * as store from './store';
import type { ProjectDetailDto } from '@/bindings/index';

const readyProject: ProjectDetailDto = {
  id: 'proj-001',
  name: 'JV Archive Prepared',
  tool: 'PixInsight',
  lifecycle: 'ready',
  path: 'projects/JV_Archive_Prepared',
  notes: null,
  channelDrift: { hasNewSources: false, suggestedAction: 'dismiss' },
  sources: [],
  channels: [],
  createdAt: '2026-06-01T00:00:00Z',
  updatedAt: '2026-06-10T00:00:00Z',
};

function renderPane() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ProjectDetailContent projectId="proj-001" />
    </QueryClientProvider>,
  );
}

function refusePlanRequired() {
  vi.mocked(store.useProjectDetail).mockReturnValue({
    data: readyProject,
    loading: false,
    error: undefined,
  });
  vi.mocked(store.callTransitionLifecycle).mockResolvedValue({
    status: 'error',
    contractVersion: '2.0.0',
    requestId: 'req-1',
    error: {
      code: 'plan.required',
      message:
        'edge (project, ready -> prepared) requires an approved FilesystemPlan',
    },
  });
}

describe('ProjectDetail Prepare plan gate', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('offers the source-view generation route when Prepare is refused with plan.required', async () => {
    refusePlanRequired();

    renderPane();
    fireEvent.click(screen.getByTestId('transition-btn-prepared'));

    expect(
      await screen.findByTestId('generate-source-view-dialog'),
    ).toBeInTheDocument();
  });

  it('opens the plan review surface with the generated source-view plan id', async () => {
    refusePlanRequired();
    mockGenerateSourceView.mockResolvedValue({
      planId: 'plan-view-prepare',
      warnings: [],
      usedCopyFallback: false,
    });

    renderPane();
    fireEvent.click(screen.getByTestId('transition-btn-prepared'));
    fireEvent.click(await screen.findByTestId('generate-source-view-submit'));

    await waitFor(() => {
      expect(mockGenerateSourceView).toHaveBeenCalledWith({
        projectId: 'proj-001',
        copyOptIn: false,
      });
    });
    const review = await screen.findByTestId('plan-review-stub');
    expect(review).toHaveTextContent('plan-view-prepare');
    expect(review).toHaveAttribute('data-title', 'Review source view plan');
  });
});
