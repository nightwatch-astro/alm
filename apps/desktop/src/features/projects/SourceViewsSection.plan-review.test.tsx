// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SourceViewsSection plan-review routing (astro-plan-krqge).
 *
 * `sourceview.generate` only persists a `prepared_view_generation` plan — the
 * links are materialised when that plan is approved and applied. The section
 * was mounted without the optional `onPlanCreated` callback, so every created
 * plan was orphaned: no review surface, no links, and an archive plan with
 * zero items forever after.
 *
 * Asserts the user-visible outcome: generating (or regenerating) opens the
 * shared plan review surface carrying the created plan id.
 */

import {
  render,
  screen,
  fireEvent,
  waitFor,
  within,
} from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const { mockList, mockGenerate, mockRegenerate } = vi.hoisted(() => ({
  mockList: vi.fn(),
  mockGenerate: vi.fn(),
  mockRegenerate: vi.fn(),
}));

vi.mock('./source-views', async () => {
  const actual =
    await vi.importActual<typeof import('./source-views')>('./source-views');
  return {
    ...actual,
    listPreparedViews: mockList,
    generateSourceView: mockGenerate,
    regeneratePreparedView: mockRegenerate,
  };
});

vi.mock('./ViewAuditHistory', () => ({
  ViewAuditHistory: () => null,
}));

vi.mock('@/shared/toast', () => ({
  addToast: vi.fn(),
}));

vi.mock('@/features/plans/PlanReviewOverlay', () => ({
  PlanReviewOverlay: ({
    planId,
    open,
  }: {
    planId: string | null;
    open: boolean;
  }) => (open ? <div data-testid="plan-review-stub">{planId}</div> : null),
}));

import { SourceViewsSection } from './SourceViewsSection';
import type { PreparedViewSummary } from './source-views';

function renderSection() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <SourceViewsSection projectId="proj-1" />
    </QueryClientProvider>,
  );
}

const removedView: PreparedViewSummary = {
  id: 'view-removed',
  projectId: 'proj-1',
  kind: 'symlink',
  state: 'removed',
  createdAt: '2026-01-01T00:00:00Z',
  itemCount: 1,
  items: [
    {
      id: 'item-1',
      inventoryItemId: 'inv-1',
      viewRelativePath: '/dest/light.fits',
      materialization: 'symlink',
      lastObservedState: 'missing',
    },
  ],
};

describe('SourceViewsSection plan review routing', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('opens the plan review surface with the generated plan id', async () => {
    mockList.mockResolvedValue({ views: [] });
    mockGenerate.mockResolvedValue({
      planId: 'plan-view-1',
      warnings: [],
      usedCopyFallback: false,
    });

    renderSection();

    fireEvent.click(await screen.findByTestId('generate-source-view-btn'));
    fireEvent.click(await screen.findByTestId('generate-source-view-submit'));

    await waitFor(() => {
      expect(mockGenerate).toHaveBeenCalledWith({
        projectId: 'proj-1',
        copyOptIn: false,
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId('plan-review-stub')).toHaveTextContent(
        'plan-view-1',
      );
    });
  });

  it('opens the plan review surface with the regeneration plan id', async () => {
    mockList.mockResolvedValue({ views: [removedView] });
    mockRegenerate.mockResolvedValue({
      planId: 'plan-regen-1',
      unresolvedItemCount: 0,
    });

    renderSection();

    fireEvent.click(await screen.findByTestId('regenerate-view-view-removed'));

    await waitFor(() => {
      expect(screen.getByTestId('plan-review-stub')).toHaveTextContent(
        'plan-regen-1',
      );
    });
  });

  it('keeps the review surface closed when generation fails', async () => {
    mockList.mockResolvedValue({ views: [] });
    mockGenerate.mockRejectedValue(new Error('no_selection'));

    renderSection();

    fireEvent.click(await screen.findByTestId('generate-source-view-btn'));
    fireEvent.click(await screen.findByTestId('generate-source-view-submit'));

    const dialog = await screen.findByTestId('generate-source-view-dialog');
    await waitFor(() => {
      expect(within(dialog).getByText(/no_selection/)).toBeInTheDocument();
    });
    expect(screen.queryByTestId('plan-review-stub')).not.toBeInTheDocument();
  });
});
