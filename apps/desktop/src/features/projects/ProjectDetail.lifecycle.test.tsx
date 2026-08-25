// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * ProjectDetail lifecycle wiring tests — spec 009 US3-3 / US4.
 *
 * Uses mocked Tauri invoke. Tests:
 * 1. Lifecycle footer actions render for a 'ready' project.
 * 2. Clicking a transition button dispatches the correct invoke call.
 * 3. A plan.required error response surfaces an info toast, not an error toast.
 * 4. A transition.refused error surfaces an error toast.
 * 5. Blocked banner renders when lifecycle='blocked'.
 * 6. Clicking resolve on BlockedBanner dispatches a transition to the correct recovery edge.
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock Tauri invoke
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

// Mock the project detail store
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

// Mock toast
vi.mock('@/shared/toast', () => ({
  addToast: vi.fn(),
}));

// Archive plan generation + review overlay have dedicated coverage in
// ProjectDetail.archive-plan.test.tsx; stub them here so this file's
// unrelated lifecycle assertions don't need a QueryClientProvider.
vi.mock('@/features/archive/store', () => ({
  useGenerateArchivePlan: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));
vi.mock('@/features/plans/PlanReviewOverlay', () => ({
  PlanReviewOverlay: () => null,
}));

import { ProjectDetailContent } from './ProjectDetail';
import * as store from './store';
import { addToast } from '@/shared/toast';
import type { ProjectDetailDto } from '@/bindings/index';

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

const mockProject: ProjectDetailDto = {
  id: 'proj-001',
  name: 'NGC 7000',
  tool: 'PixInsight',
  lifecycle: 'ready',
  path: 'projects/NGC7000',
  notes: null,
  channelDrift: { hasNewSources: false, suggestedAction: 'dismiss' },
  sources: [],
  channels: [],
  createdAt: '2026-06-01T00:00:00Z',
  updatedAt: '2026-06-10T00:00:00Z',
};

function setupStore(project: Partial<ProjectDetailDto> = {}) {
  vi.mocked(store.useProjectDetail).mockReturnValue({
    data: { ...mockProject, ...project },
    loading: false,
    error: undefined,
  });
}

describe('ProjectDetail lifecycle transitions (spec 009 US3-3)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders lifecycle actions for ready state', () => {
    setupStore({ lifecycle: 'ready' });
    renderPane();
    // Per-project actions live in the detail action bar (single source of truth).
    expect(screen.getByTestId('lifecycle-actions')).toBeInTheDocument();
    // Should have a "Prepare" button (ready → prepared)
    expect(screen.getByTestId('transition-btn-prepared')).toBeInTheDocument();
    // Should have a "Mark as Processing" button (ready → processing)
    expect(screen.getByTestId('transition-btn-processing')).toBeInTheDocument();
  });

  it('dispatches transition when action button is clicked', async () => {
    setupStore({ lifecycle: 'processing' });
    vi.mocked(store.callTransitionLifecycle).mockResolvedValue({
      status: 'success',
      contractVersion: '2.0.0',
      requestId: 'req-1',
      newState: 'completed',
    });

    renderPane();
    fireEvent.click(screen.getByTestId('transition-btn-completed'));

    await waitFor(() => {
      expect(store.callTransitionLifecycle).toHaveBeenCalledWith(
        'proj-001',
        'processing',
        'completed',
        'Mark as Completed',
      );
    });
  });

  it('shows info toast when plan.required is returned (plan-gated edge)', async () => {
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
  });

  it('shows error toast when transition is refused', async () => {
    setupStore({ lifecycle: 'ready' });
    vi.mocked(store.callTransitionLifecycle).mockResolvedValue({
      status: 'error',
      contractVersion: '2.0.0',
      requestId: 'req-3',
      error: {
        code: 'transition.refused',
        message: 'Transition refused: edge not allowed',
      },
    });

    renderPane();
    fireEvent.click(screen.getByTestId('transition-btn-processing'));

    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        expect.objectContaining({ variant: 'error' }),
      );
    });
  });

  it('renders the blocked banner when lifecycle=blocked', () => {
    setupStore({ lifecycle: 'blocked' });
    renderPane();
    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(screen.getByTestId('blocked-resolve-btn')).toBeInTheDocument();
  });

  it('dispatches blocked → ready transition on resolve click (user reason)', async () => {
    setupStore({ lifecycle: 'blocked' });
    vi.mocked(store.callTransitionLifecycle).mockResolvedValue({
      status: 'success',
      contractVersion: '2.0.0',
      requestId: 'req-4',
      newState: 'ready',
    });

    renderPane();
    fireEvent.click(screen.getByTestId('blocked-resolve-btn'));

    await waitFor(() => {
      expect(store.callTransitionLifecycle).toHaveBeenCalledWith(
        'proj-001',
        'blocked',
        'ready',
        'Resolved blocker',
      );
    });
  });

  it('does not render lifecycle transition buttons when lifecycle=setup_incomplete', () => {
    setupStore({ lifecycle: 'setup_incomplete' });
    renderPane();
    // The action bar still hosts always-present actions (Reveal / Open in tool),
    // but no lifecycle transition buttons exist for setup_incomplete.
    expect(screen.queryByTestId(/^transition-btn-/)).not.toBeInTheDocument();
    // The Reveal action carries the shared platform-native revealLabel()
    // (jsdom reports no platform → the Linux-generic label).
    expect(screen.getByTestId('action-reveal')).toHaveTextContent(
      'Show in file manager',
    );
  });

  it('renders unarchive actions for archived state', () => {
    setupStore({ lifecycle: 'archived' });
    renderPane();
    expect(screen.getByTestId('transition-btn-ready')).toBeInTheDocument();
    expect(screen.getByTestId('transition-btn-processing')).toBeInTheDocument();
  });
});
