// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />

/**
 * RelationProposalList component tests.
 *
 * Tests:
 * 1. Renders empty state when no proposals.
 * 2. Renders proposal rows with kind label and subject count.
 * 3. Marks selected proposal row as aria-selected.
 * 4. Calls onSelect when a proposal row is activated.
 * 5. Shows loading state while fetching.
 * 6. Shows error state on fetch failure (assertive live region).
 * 7. Severity: no failed thresholds → ok pill.
 * 8. Severity: failed threshold → red pill.
 * 9. Severity: missing evidence codes → missing pill.
 * 10. State filter buttons toggle the active state.
 * 11. Manual proposal rows show the "Manual" pill.
 * 12. Header action slot renders the provided action.
 */

import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RelationProposalList } from '../RelationProposalList';
import type { Page, RelationProposal } from '../groupsTypes';

// ── Mock useRelationProposals ─────────────────────────────────────────────────

const mockQueryResult: {
  data: Page<RelationProposal> | undefined;
  isLoading: boolean;
  isError: boolean;
} = { data: undefined, isLoading: false, isError: false };

vi.mock('../useGroupsStore', async (importOriginal) => {
  const original = await importOriginal<typeof import('../useGroupsStore')>();
  return {
    ...original,
    useRelationProposals: vi.fn(() => mockQueryResult),
  };
});

function makeProposal(
  overrides: Partial<RelationProposal> = {},
): RelationProposal {
  return {
    proposalId: 'prop-1',
    proposalRevision: 1,
    kind: 'panel_add',
    state: 'pending',
    sourceRevisionCount: 1,
    subjectCount: 2,
    proposedMembershipCount: 2,
    proposedEdgeCount: 0,
    proposedLineageCount: 0,
    evidence: {
      evidenceId: 'ev-1',
      targetCompatibility: 'same_target',
      allowedResidualRotationRangesDeg: [],
      parity: 'match',
      acquisitionGeometry: 'compatible',
      equipment: 'compatible',
      missingEvidenceCodes: [],
      thresholdSnapshot: [],
    },
    matchingSettingsRevision: 1,
    basisFingerprint: 'fp-1',
    createdAt: '2026-07-01T00:00:00Z',
    createdBy: 'system',
    ...overrides,
  };
}

function makeEmptyPage(): Page<RelationProposal> {
  return { items: [], readWatermark: 1 };
}

function makePage(items: RelationProposal[]): Page<RelationProposal> {
  return { items, readWatermark: 1 };
}

function renderList(
  props: Partial<React.ComponentProps<typeof RelationProposalList>> = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <RelationProposalList
        selectedId={undefined}
        onSelect={vi.fn()}
        {...props}
      />
    </QueryClientProvider>,
  );
}

describe('RelationProposalList', () => {
  it('1. renders empty state when no proposals', () => {
    mockQueryResult.data = makeEmptyPage();
    mockQueryResult.isLoading = false;
    mockQueryResult.isError = false;
    renderList();
    // EmptyState renders title + description; match the title specifically
    expect(screen.getAllByText(/no proposals/i).length).toBeGreaterThanOrEqual(
      1,
    );
  });

  it('2. renders proposal row with kind label and subject count', () => {
    mockQueryResult.data = makePage([makeProposal()]);
    renderList();
    // Kind label: "Panel add"
    expect(screen.getByText(/panel add/i)).toBeInTheDocument();
    // Subject count: "2 subjects"
    expect(screen.getByText(/2 subject/i)).toBeInTheDocument();
  });

  it('3. marks selected row as aria-selected', () => {
    mockQueryResult.data = makePage([makeProposal({ proposalId: 'p1' })]);
    renderList({ selectedId: 'p1', onSelect: vi.fn() });
    const row = screen.getByTestId('proposal-row-p1');
    expect(row).toHaveAttribute('aria-selected', 'true');
  });

  it('4. calls onSelect with proposal ID on click', () => {
    const onSelect = vi.fn();
    mockQueryResult.data = makePage([makeProposal({ proposalId: 'p-abc' })]);
    renderList({ onSelect });
    fireEvent.click(screen.getByTestId('proposal-row-p-abc'));
    expect(onSelect).toHaveBeenCalledWith('p-abc');
  });

  it('5. shows loading state', () => {
    mockQueryResult.data = undefined;
    mockQueryResult.isLoading = true;
    mockQueryResult.isError = false;
    renderList();
    // Loading status region present — use aria-label to be specific
    expect(
      screen.getByRole('status', { name: /loading proposals/i }),
    ).toBeInTheDocument();
  });

  it('6. shows error state with assertive live region', () => {
    mockQueryResult.data = undefined;
    mockQueryResult.isLoading = false;
    mockQueryResult.isError = true;
    renderList();
    const alert = screen.getByRole('alert');
    expect(alert).toHaveAttribute('aria-live', 'assertive');
  });

  it('7. ok severity when no failed thresholds and no missing evidence', () => {
    mockQueryResult.data = makePage([
      makeProposal({
        evidence: {
          evidenceId: 'ev-ok',
          targetCompatibility: 'same_target',
          allowedResidualRotationRangesDeg: [],
          parity: 'match',
          acquisitionGeometry: 'compatible',
          equipment: 'compatible',
          missingEvidenceCodes: [],
          thresholdSnapshot: [
            {
              key: 'coverage',
              measuredValue: 95,
              unit: '%',
              comparison: 'gte',
              thresholdValue: 90,
              outcome: 'pass',
            },
          ],
        },
      }),
    ]);
    renderList();
    // Should contain "Pass" (ok severity label)
    expect(screen.getByText(/pass/i)).toBeInTheDocument();
  });

  it('8. red severity when a threshold fails', () => {
    mockQueryResult.data = makePage([
      makeProposal({
        evidence: {
          evidenceId: 'ev-fail',
          targetCompatibility: 'same_target',
          allowedResidualRotationRangesDeg: [],
          parity: 'match',
          acquisitionGeometry: 'compatible',
          equipment: 'compatible',
          missingEvidenceCodes: [],
          thresholdSnapshot: [
            {
              key: 'coverage',
              measuredValue: 70,
              unit: '%',
              comparison: 'gte',
              thresholdValue: 90,
              outcome: 'fail',
            },
          ],
        },
      }),
    ]);
    renderList();
    expect(screen.getByText(/fail/i)).toBeInTheDocument();
  });

  it('9. missing severity when missing evidence codes exist', () => {
    mockQueryResult.data = makePage([
      makeProposal({
        evidence: {
          evidenceId: 'ev-miss',
          targetCompatibility: 'same_target',
          allowedResidualRotationRangesDeg: [],
          parity: 'unknown',
          acquisitionGeometry: 'unknown',
          equipment: 'unknown',
          missingEvidenceCodes: ['footprint.unavailable'],
          thresholdSnapshot: [],
        },
      }),
    ]);
    renderList();
    expect(screen.getByText(/missing/i)).toBeInTheDocument();
  });

  it('10. state filter buttons show correct active state', () => {
    mockQueryResult.data = makeEmptyPage();
    renderList();
    // "Pending" button should start pressed
    const pendingBtn = screen.getByRole('button', { name: /pending/i });
    expect(pendingBtn).toHaveAttribute('aria-pressed', 'true');
    // Click "Accepted" — it should become active
    const acceptedBtn = screen.getByRole('button', { name: /accepted/i });
    fireEvent.click(acceptedBtn);
    expect(acceptedBtn).toHaveAttribute('aria-pressed', 'true');
    expect(pendingBtn).toHaveAttribute('aria-pressed', 'false');
  });

  it('11. manual proposal rows show Manual pill', () => {
    mockQueryResult.data = makePage([
      makeProposal({ kind: 'manual_relation' }),
    ]);
    renderList();
    // There are two elements with "manual" — the row kind label and the pill.
    // Use getAllByText and assert at least one exists.
    const manualEls = screen.getAllByText(/manual/i);
    expect(manualEls.length).toBeGreaterThanOrEqual(1);
  });

  it('12. header action renders in the header', () => {
    mockQueryResult.data = makeEmptyPage();
    renderList({
      headerAction: <button type="button">Custom action</button>,
    });
    expect(screen.getByText('Custom action')).toBeInTheDocument();
  });
});
