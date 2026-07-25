// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * AuditLog tests — spec-043 P2 (wire Audit Log settings screen to real
 * backend).
 *
 * Covers:
 *   1. Loads and renders audit entries via `commands.auditList`.
 *   2. The search box is debounced (no per-keystroke IPC), then maps to
 *      `filters.search` on the next `auditList` call.
 *   3. Date-range inputs map to `filters.from` / `filters.to`.
 *   4. Pagination (Next) advances `pagination.offset` by the page size.
 *   5. A load failure surfaces via the load-error banner (errMessage).
 *   6. Export calls `commands.auditExport` with the current filters and
 *      triggers a file download.
 *   7. Detail localization (D23 upgrade): entries carrying a stable
 *      `detailCode` + `detailParams` render a catalog message in the entity
 *      tooltip; entries without a code (or without the params the template
 *      needs) fall back to the stored English `detail`.
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

const { mockList, mockExport, mockNavigate } = vi.hoisted(() => ({
  mockList: vi.fn(),
  mockExport: vi.fn(),
  mockNavigate: vi.fn(),
}));

// The shared useEntityNames hook (#809) reads session names via
// useInventorySources, a TanStack Query hook — AuditLog now needs a
// QueryClientProvider ancestor, same pattern as CalibrationMatchPanel.test.tsx.
function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
}));

// #803 entity-name resolution calls these per-row; default to a "not found"
// error so unrelated tests keep seeing the raw `entityType · entityId` text.
// #809: the shared useEntityNames hook (apps/desktop/src/hooks/useEntityNames.ts)
// also reads session names off `inventoryList` (no per-id session lookup
// exists) — an empty source list keeps existing raw-fallback assertions for
// `entityType: 'session'` entries unchanged.
vi.mock('@/bindings/index', () => ({
  commands: {
    auditList: mockList,
    auditExport: mockExport,
    // #809 (GF-7): entity.names batch — default to empty map so unresolved
    // refs fall back to the raw `entityType · entityId` text.
    entityNames: vi.fn().mockResolvedValue({
      status: 'ok',
      data: { names: {} },
    }),
    inventoryList: vi.fn().mockResolvedValue({
      status: 'ok',
      data: {
        status: 'ok',
        contractVersion: '1.0',
        requestId: 'req-inventory',
        generatedAt: '2026-01-01T00:00:00Z',
        sources: [],
      },
    }),
  },
}));

import { AuditLog } from './AuditLog';

const ENTRIES = [
  {
    id: 'audit-001',
    timestamp: '2026-05-20T22:15:00Z',
    eventType: 'session.confirmed',
    entityType: 'session',
    entityId: 'ses-001',
    fromState: 'needs_review',
    toState: 'confirmed',
    actor: 'user',
    outcome: 'applied',
    detail: 'User confirmed session',
  },
  {
    id: 'audit-002',
    timestamp: '2026-05-19T21:00:00Z',
    eventType: 'plan.approved',
    entityType: 'plan',
    entityId: 'plan-001',
    fromState: 'ready_for_review',
    toState: 'approved',
    actor: 'user',
    outcome: 'applied',
    detail: 'Plan approved',
  },
];

beforeEach(() => {
  mockList.mockReset();
  mockExport.mockReset();
  mockNavigate.mockReset();
  mockList.mockResolvedValue({
    status: 'ok',
    data: { entries: ENTRIES, total: ENTRIES.length },
  });
  mockExport.mockResolvedValue({
    status: 'ok',
    data: ENTRIES.map((e) => JSON.stringify(e)).join('\n'),
  });
});

describe('AuditLog', () => {
  it('loads audit entries via auditList and renders them', async () => {
    render(<AuditLog />, { wrapper });
    await waitFor(() => expect(mockList).toHaveBeenCalled());

    // findBy, not getBy: rows render behind `{!loading && …}` (AuditLog.tsx)
    // and `loading` only clears once auditList's promise RESOLVES — the
    // waitFor above only proves the call was made. A sync getBy therefore
    // races the "Loading…" row on a contended runner (#1083).
    expect(await screen.findByText('session.confirmed')).toBeInTheDocument();
    expect(screen.getByText('plan.approved')).toBeInTheDocument();
    // First call has no filters (nothing typed yet).
    expect(mockList).toHaveBeenCalledWith(null, { limit: 8, offset: 0 });
  });

  it('debounces the search box, then maps it to filters.search', async () => {
    render(<AuditLog />, { wrapper });
    // Start from settled load state before interacting.
    await screen.findByText('session.confirmed');
    const callsBeforeTyping = mockList.mock.calls.length;

    fireEvent.change(screen.getByLabelText('Search audit events'), {
      target: { value: 'plan.approved' },
    });

    // Debounced: the keystroke does not fire an immediate IPC round-trip.
    expect(mockList).toHaveBeenCalledTimes(callsBeforeTyping);

    // After the 300ms debounce the query lands as filters.search.
    await waitFor(() =>
      expect(mockList).toHaveBeenLastCalledWith(
        { search: 'plan.approved' },
        { limit: 8, offset: 0 },
      ),
    );
  });

  it('maps the date-range inputs to filters.from / filters.to', async () => {
    render(<AuditLog />, { wrapper });
    await screen.findByText('session.confirmed');

    fireEvent.change(screen.getByLabelText('From'), {
      target: { value: '2026-05-01' },
    });
    await waitFor(() =>
      expect(mockList).toHaveBeenLastCalledWith(
        { from: new Date('2026-05-01').toISOString() },
        { limit: 8, offset: 0 },
      ),
    );

    fireEvent.change(screen.getByLabelText('To'), {
      target: { value: '2026-05-10' },
    });
    await waitFor(() =>
      expect(mockList).toHaveBeenLastCalledWith(
        {
          from: new Date('2026-05-01').toISOString(),
          to: new Date(
            new Date('2026-05-10').getTime() + 86400000,
          ).toISOString(),
        },
        { limit: 8, offset: 0 },
      ),
    );
  });

  it('advances pagination.offset on Next', async () => {
    mockList.mockResolvedValue({
      status: 'ok',
      data: { entries: ENTRIES, total: 20 },
    });
    render(<AuditLog />, { wrapper });
    // Wait for the initial load to RESOLVE, not merely for auditList to have been
    // called: the Next button is disabled until `total` is set (totalPages > 1),
    // so clicking while the load promise is still pending is a no-op that leaves
    // offset at 0 (the source of the parallel-run flake). Rows only render after
    // loading flips false, so finding an entry proves total=20 landed and Next
    // is enabled.
    await screen.findByText('session.confirmed');
    const nextBtn = screen.getByText('Next');
    expect(nextBtn).toBeEnabled();

    fireEvent.click(nextBtn);

    await waitFor(() =>
      expect(mockList).toHaveBeenLastCalledWith(null, { limit: 8, offset: 8 }),
    );
  });

  it('shows the load-error banner when auditList fails', async () => {
    mockList.mockResolvedValue({
      status: 'error',
      error: {
        code: 'internal.database',
        message: 'db down',
        severity: 'fatal',
        retryable: true,
      },
    });
    render(<AuditLog />, { wrapper });

    await waitFor(() =>
      expect(
        screen.getByText(/Could not load audit events/),
      ).toBeInTheDocument(),
    );
  });

  it('localizes detail tooltips from detailCode + detailParams, with English fallback', async () => {
    mockList.mockResolvedValue({
      status: 'ok',
      data: {
        entries: [
          {
            id: 'audit-101',
            timestamp: '2026-07-01T10:00:00Z',
            eventType: 'project: ready -> prepared',
            entityType: 'project',
            entityId: 'proj-001',
            fromState: 'ready',
            toState: null,
            actor: 'user',
            outcome: 'refused',
            detail:
              'edge (project, ready -> prepared) requires an approved FilesystemPlan',
            detailCode: 'plan.required',
            detailParams: {
              entityType: 'project',
              fromState: 'ready',
              toState: 'prepared',
            },
          },
          {
            id: 'audit-102',
            timestamp: '2026-07-01T11:00:00Z',
            eventType: 'project: prepared -> processing',
            entityType: 'project',
            entityId: 'proj-002',
            fromState: 'prepared',
            toState: null,
            actor: 'user',
            outcome: 'refused',
            detail:
              'edge (project, prepared -> processing) requires reviewed provenance on 2 field(s)',
            detailCode: 'provenance.unreviewed',
            detailParams: { count: '2' },
          },
          {
            id: 'audit-103',
            timestamp: '2026-07-01T12:00:00Z',
            eventType: 'target.resolved',
            entityType: 'canonical_target',
            entityId: 'tgt-001',
            fromState: null,
            toState: null,
            actor: 'user',
            outcome: 'applied',
            detail: 'target.resolved (M 31)',
            detailCode: 'target.resolved',
            detailParams: { query: 'M 31' },
          },
          {
            // Old row (pre-D23-upgrade): ambiguous code, no params → the
            // stored English message must render unchanged.
            id: 'audit-104',
            timestamp: '2026-07-01T13:00:00Z',
            eventType: 'project: processing -> completed',
            entityType: 'project',
            entityId: 'proj-003',
            fromState: 'processing',
            toState: null,
            actor: 'user',
            outcome: 'refused',
            detail: 'some legacy free-form refusal reason',
            detailCode: 'transition.refused',
            detailParams: null,
          },
        ],
        total: 4,
      },
    });

    render(<AuditLog />, { wrapper });

    // waitFor on rendered content (not just "mockList was called") — the
    // list call resolves asynchronously, so asserting immediately after only
    // the call-count race with the re-render that actually paints the rows
    // (observed flaky on a loaded CI runner: assertion ran while the table
    // still showed "Loading…"). Detail is now a visible column (#749), not
    // a hover-only tooltip.
    await waitFor(() =>
      expect(
        screen.getByText(
          'Transition ready → prepared for project requires an approved filesystem plan',
        ),
      ).toBeInTheDocument(),
    );
    expect(
      screen.getByText('2 fields require review before this transition'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('Target resolved from query “M 31”'),
    ).toBeInTheDocument();
    // Fallback: no usable template → stored English detail, byte-identical.
    expect(
      screen.getByText('some legacy free-form refusal reason'),
    ).toBeInTheDocument();
  });

  it('exports via auditExport with a save dialog and the current filters', async () => {
    // Mock @tauri-apps/plugin-dialog save function.
    vi.mock('@tauri-apps/plugin-dialog', () => ({
      save: vi.fn().mockResolvedValue('/tmp/audit-export.ndjson'),
    }));

    render(<AuditLog />, { wrapper });
    await waitFor(() => expect(mockList).toHaveBeenCalled());

    // Wait for the button to be enabled (loading clears after auditList resolves).
    const exportBtn = await screen.findByRole('button', {
      name: 'Export audit events to a file',
    });
    await waitFor(() => expect(exportBtn).toBeEnabled());
    fireEvent.click(exportBtn);

    await waitFor(() =>
      expect(mockExport).toHaveBeenCalledWith('/tmp/audit-export.ndjson', null),
    );
  });

  it('renders the state-change column (#749)', async () => {
    render(<AuditLog />, { wrapper });
    await waitFor(() => expect(mockList).toHaveBeenCalled());

    // findBy: rows are gated on `loading` clearing, which the waitFor above
    // does not prove (#1083).
    expect(
      await screen.findByText('needs_review → confirmed'),
    ).toBeInTheDocument();
    expect(screen.getByText('ready_for_review → approved')).toBeInTheDocument();
  });

  it('maps the outcome and entity-type selects to structured filters (#749)', async () => {
    render(<AuditLog />, { wrapper });
    await screen.findByText('session.confirmed');

    fireEvent.change(screen.getByLabelText('Outcome'), {
      target: { value: 'refused' },
    });
    await waitFor(() =>
      expect(mockList).toHaveBeenLastCalledWith(
        { outcome: 'refused' },
        { limit: 8, offset: 0 },
      ),
    );

    fireEvent.change(screen.getByLabelText('Entity'), {
      target: { value: 'project' },
    });
    await waitFor(() =>
      expect(mockList).toHaveBeenLastCalledWith(
        { outcome: 'refused', entityType: 'project' },
        { limit: 8, offset: 0 },
      ),
    );
  });

  it('navigates to the entity page when a linked row is clicked (#831)', async () => {
    render(<AuditLog />, { wrapper });
    await waitFor(() => expect(mockList).toHaveBeenCalled());

    // ENTRIES[0] is entityType 'session', which has a real /sessions/:id route.
    // findBy: the row only exists once `loading` clears (#1083); clicking a
    // row that hasn't rendered would throw rather than navigate.
    fireEvent.click(await screen.findByText('session.confirmed'));

    expect(mockNavigate).toHaveBeenCalledWith({ to: '/sessions/ses-001' });
  });

  it('does not link entity types without a destination page (#626 reasoning, #831)', async () => {
    render(<AuditLog />, { wrapper });
    await waitFor(() => expect(mockList).toHaveBeenCalled());

    // ENTRIES[1] is entityType 'plan' — no /plans/:id route exists yet.
    // findBy: without it a still-loading table makes this pass vacuously —
    // the click never lands, so "navigate was not called" proves nothing
    // about the no-route behaviour (#1083).
    fireEvent.click(await screen.findByText('plan.approved'));

    expect(mockNavigate).not.toHaveBeenCalled();
  });

  it('resolves a project entity to its display name (#803)', async () => {
    const { commands } = await import('@/bindings/index');
    vi.mocked(commands.entityNames).mockResolvedValueOnce({
      status: 'ok',
      data: { names: { 'project:proj-abc': 'J5 Lifecycle Test' } },
    });
    mockList.mockResolvedValue({
      status: 'ok',
      data: {
        entries: [
          {
            id: 'audit-201',
            timestamp: '2026-07-14T10:00:00Z',
            eventType: 'project.archive_refused',
            entityType: 'project',
            entityId: 'proj-abc',
            fromState: null,
            toState: null,
            actor: 'system',
            outcome: 'refused',
            detail: 'Archive refused',
          },
        ],
        total: 1,
      },
    });

    render(<AuditLog />, { wrapper });

    await waitFor(() => {
      expect(screen.getByText('J5 Lifecycle Test')).toBeInTheDocument();
    });
    expect(screen.queryByText('project · proj-abc')).not.toBeInTheDocument();
  });
});
