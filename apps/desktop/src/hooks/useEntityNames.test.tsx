// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useEntityNames tests (#809) — the resolver shared by Audit Log,
 * Projects Sources, and the Calibration match panel.
 *
 * Updated for the batched `entity.names` IPC (GF-7 / DS-14): the hook now
 * calls `commands.entityNames([...])` once for all unseen refs instead of
 * one `projectsGet`/`targetsGet`/`plansGet` call per id.
 */

import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

const { mockEntityNames, mockInventoryList } = vi.hoisted(() => ({
  mockEntityNames: vi.fn(),
  mockInventoryList: vi.fn(),
}));

vi.mock('@/bindings/index', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/bindings/index')>();
  return {
    ...original,
    commands: {
      ...original.commands,
      entityNames: mockEntityNames,
      inventoryList: mockInventoryList,
    },
  };
});

import {
  useEntityNames,
  entityNameKey,
  clearEntityNameCache,
} from './useEntityNames';

/** Helper: wrap a names map as an `entity.names` ok-response. */
function namesOk(names: Record<string, string>) {
  return Promise.resolve({ status: 'ok' as const, data: { names } });
}

/** Helper: simulate a total backend failure. */
function namesErr() {
  return Promise.reject(new Error('backend error'));
}

function emptyInventory() {
  return {
    status: 'ok' as const,
    data: {
      status: 'ok',
      contractVersion: '1.0',
      requestId: 'req-inventory',
      generatedAt: '2026-01-01T00:00:00Z',
      sources: [],
    },
  };
}

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  clearEntityNameCache();
  mockEntityNames.mockImplementation(() => namesOk({}));
  mockInventoryList.mockResolvedValue(emptyInventory());
});

describe('useEntityNames', () => {
  it('resolves a known project id via a single batched entityNames call', async () => {
    mockEntityNames.mockImplementation(() =>
      namesOk({ 'project:proj-1': 'Andromeda Mosaic' }),
    );
    const ref = { entityType: 'project', entityId: 'proj-1' };
    const { result } = renderHook(() => useEntityNames([ref]), { wrapper });

    await waitFor(() => {
      expect(result.current.get(entityNameKey(ref))).toBe('Andromeda Mosaic');
    });
    expect(mockEntityNames).toHaveBeenCalledWith([
      { entityType: 'project', entityId: 'proj-1' },
    ]);
  });

  it('resolves a known session id via the inventory-sources query, not entityNames', async () => {
    mockInventoryList.mockResolvedValue({
      status: 'ok',
      data: {
        status: 'ok',
        contractVersion: '1.0',
        requestId: 'req-inventory',
        generatedAt: '2026-01-01T00:00:00Z',
        sources: [
          {
            id: 'src-1',
            path: '/library/src-1',
            kind: 'library',
            state: 'active',
            sessions: [
              {
                id: 'ses-1',
                name: 'M31 – 2026-05-20',
                sourceId: 'src-1',
                frames: 40,
                type: 'light',
                target: 'M31',
                filter: null,
                exposure: null,
              },
            ],
          },
        ],
      },
    });
    const ref = { entityType: 'session', entityId: 'ses-1' };
    const { result } = renderHook(() => useEntityNames([ref]), { wrapper });

    await waitFor(() => {
      expect(result.current.get(entityNameKey(ref))).toBe('M31 – 2026-05-20');
    });
    // Sessions skip the entityNames batch — only inventory-sources is used.
    expect(mockEntityNames).not.toHaveBeenCalled();
  });

  it('falls back (no entry in the map) for an id absent from the DB', async () => {
    // Backend returns empty names — target not found.
    mockEntityNames.mockImplementation(() => namesOk({}));
    const ref = { entityType: 'target', entityId: 'tgt-missing' };
    const { result } = renderHook(() => useEntityNames([ref]), { wrapper });

    await waitFor(() => expect(mockEntityNames).toHaveBeenCalled());
    expect(result.current.get(entityNameKey(ref))).toBeUndefined();
  });

  it('falls back for an entity type with no known lookup (e.g. settings)', () => {
    const ref = { entityType: 'settings', entityId: 'settings-1' };
    const { result } = renderHook(() => useEntityNames([ref]), { wrapper });

    // Unknown type skips the batch entirely.
    expect(result.current.get(entityNameKey(ref))).toBeUndefined();
    expect(mockEntityNames).not.toHaveBeenCalled();
  });

  it('batches multiple refs of different types into one call', async () => {
    mockEntityNames.mockImplementation(() =>
      namesOk({
        'project:proj-1': 'Project Alpha',
        'plan:plan-1': 'Plan Beta',
      }),
    );
    const refs = [
      { entityType: 'project', entityId: 'proj-1' },
      { entityType: 'plan', entityId: 'plan-1' },
    ];
    const { result } = renderHook(() => useEntityNames(refs), { wrapper });

    await waitFor(() => {
      expect(result.current.get(entityNameKey(refs[0]))).toBe('Project Alpha');
      expect(result.current.get(entityNameKey(refs[1]))).toBe('Plan Beta');
    });
    // Only one batch call, not two.
    expect(mockEntityNames).toHaveBeenCalledTimes(1);
  });

  it('does not re-request ids that are already cached', async () => {
    mockEntityNames.mockImplementation(() =>
      namesOk({ 'project:proj-cached': 'Cached Project' }),
    );
    const ref = { entityType: 'project', entityId: 'proj-cached' };

    const { result, rerender } = renderHook(() => useEntityNames([ref]), {
      wrapper,
    });
    await waitFor(() =>
      expect(result.current.get(entityNameKey(ref))).toBe('Cached Project'),
    );

    vi.clearAllMocks();
    mockEntityNames.mockImplementation(() => namesOk({}));
    rerender();

    // Cache hit — no second IPC call.
    expect(mockEntityNames).not.toHaveBeenCalled();
  });

  it('retries on backend error by removing from in-flight set', async () => {
    mockEntityNames
      .mockImplementationOnce(namesErr)
      .mockImplementation(() =>
        namesOk({ 'project:proj-retry': 'Retried Project' }),
      );
    const ref = { entityType: 'project', entityId: 'proj-retry' };
    const { result, rerender } = renderHook(() => useEntityNames([ref]), {
      wrapper,
    });

    // First call fails; ref is evicted from in-flight set.
    await waitFor(() => expect(mockEntityNames).toHaveBeenCalledTimes(1));

    // Trigger a re-render which should retry.
    rerender();
    await waitFor(() =>
      expect(result.current.get(entityNameKey(ref))).toBe('Retried Project'),
    );
  });
});
