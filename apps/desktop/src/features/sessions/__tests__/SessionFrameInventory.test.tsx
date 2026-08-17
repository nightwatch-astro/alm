// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SessionFrameInventory tests (spec 048 T014/T025 frontend).
 *
 * `inventory.frame.list` and `inventory.frame.relink` had real, tested
 * backends but zero frontend callers before this component — see the
 * documented gap in `crates/e2e-tests/tests/inventory_journeys.rs`. Verifies:
 * 1. Pre-scan: no fabricated frames; scan is on-demand.
 * 2. Scanned frames render present count/disk total and a per-frame table.
 * 3. A `missing` frame gets a Relink action; a successful relink toasts and
 *    re-runs the scan.
 * 4. A `hash.mismatch` relink error renders inline — never silently treated
 *    as success.
 */

import { render, screen, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { InventoryFrameListResponse } from '@/bindings/index';

const {
  mockScanMutate,
  mockRelinkMutate,
  mockUseRootFrameWatcher,
  scanState,
  relinkState,
} = vi.hoisted(() => ({
  mockScanMutate: vi.fn(),
  mockRelinkMutate: vi.fn(),
  mockUseRootFrameWatcher: vi.fn(),
  scanState: {
    data: undefined as InventoryFrameListResponse | undefined,
    isPending: false,
    isError: false,
  },
  relinkState: {
    isPending: false,
    isError: false,
    error: undefined as Error | undefined,
  },
}));

vi.mock('@/features/inventory/store', () => ({
  useFrameListScan: () => ({ ...scanState, mutate: mockScanMutate }),
  useRelinkFrame: () => ({
    ...relinkState,
    mutate: mockRelinkMutate,
    reset: vi.fn(),
  }),
  useRootFrameWatcher: (rootId: string | null) => {
    mockUseRootFrameWatcher(rootId);
  },
}));

const mockAddToast = vi.fn();
vi.mock('@/shared/toast', () => ({
  addToast: (...args: unknown[]) => {
    mockAddToast(...args);
  },
  useToasts: () => ({ toasts: [], dismiss: vi.fn() }),
}));

import { SessionFrameInventory } from '../SessionFrameInventory';

function listResult(
  overrides: Partial<InventoryFrameListResponse> = {},
): InventoryFrameListResponse {
  return {
    frames: [
      {
        frameId: 'frame-1',
        rootId: 'root-1',
        relativePath: 'lights/frame_001.fits',
        frameType: 'light',
        sizeBytes: 1024,
        state: 'present',
        sessionId: 'session-1',
      },
      {
        frameId: 'frame-2',
        rootId: 'root-1',
        relativePath: 'lights/frame_002.fits',
        frameType: 'light',
        sizeBytes: 2048,
        state: 'missing',
        sessionId: 'session-1',
      },
    ],
    presentCount: 1,
    presentSizeBytes: 1024,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  scanState.data = undefined;
  scanState.isPending = false;
  scanState.isError = false;
  relinkState.isPending = false;
  relinkState.isError = false;
  relinkState.error = undefined;
});

describe('SessionFrameInventory (spec 048 T014/T025)', () => {
  it('renders the panel and calls inventory.frame.list on demand (no fabricated frames)', () => {
    render(<SessionFrameInventory sessionId="session-1" rootId="root-1" />);
    expect(screen.getByTestId('session-frame-inventory')).toBeInTheDocument();
    expect(
      screen.queryByTestId('frame-inventory-row-frame-1'),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('frame-inventory-scan-btn'));
    expect(mockScanMutate).toHaveBeenCalledWith({
      sessionId: 'session-1',
      rootId: null,
    });
  });

  it('binds the session root live detection triggers to the panel lifetime (T023/T026)', () => {
    render(<SessionFrameInventory sessionId="session-1" rootId="root-1" />);
    expect(mockUseRootFrameWatcher).toHaveBeenCalledWith('root-1');
  });

  it('shows the real present count + disk total and a Missing pill for missing frames', () => {
    scanState.data = listResult();
    render(<SessionFrameInventory sessionId="session-1" rootId="root-1" />);

    expect(screen.getByTestId('frame-inventory-summary')).toHaveTextContent(
      '1 present',
    );
    expect(screen.getByTestId('frame-inventory-summary')).toHaveTextContent(
      '1.0 KB',
    );

    const missingRow = screen.getByTestId('frame-inventory-row-frame-2');
    expect(within(missingRow).getByText('Missing')).toBeInTheDocument();
    expect(
      within(missingRow).getByTestId('relink-open-frame-2'),
    ).toBeInTheDocument();

    const presentRow = screen.getByTestId('frame-inventory-row-frame-1');
    expect(within(presentRow).queryByText('Missing')).not.toBeInTheDocument();
  });

  it('relinking a missing frame calls inventory.frame.relink and toasts on success', () => {
    scanState.data = listResult();
    mockRelinkMutate.mockImplementation(
      (_vars: unknown, opts?: { onSuccess?: () => void }) => {
        opts?.onSuccess?.();
      },
    );
    render(<SessionFrameInventory sessionId="session-1" rootId="root-1" />);

    fireEvent.click(screen.getByTestId('relink-open-frame-2'));
    fireEvent.change(screen.getByTestId('relink-input-frame-2'), {
      target: { value: 'lights/relocated.fits' },
    });
    fireEvent.click(screen.getByTestId('relink-confirm-frame-2'));

    expect(mockRelinkMutate).toHaveBeenCalledWith(
      { frameId: 'frame-2', candidateRelativePath: 'lights/relocated.fits' },
      expect.anything(),
    );
    expect(mockAddToast).toHaveBeenCalledWith(
      expect.objectContaining({ message: 'Frame relinked' }),
    );
    // Re-scans to reflect the real post-relink state.
    expect(mockScanMutate).toHaveBeenCalledWith({
      sessionId: 'session-1',
      rootId: null,
    });
  });

  it('a hash.mismatch relink error renders inline, never as a fake success', () => {
    scanState.data = listResult();
    relinkState.isError = true;
    relinkState.error = {
      name: 'ContractError',
      message: 'hash.mismatch',
    } as Error;
    render(<SessionFrameInventory sessionId="session-1" rootId="root-1" />);

    fireEvent.click(screen.getByTestId('relink-open-frame-2'));
    expect(screen.getByTestId('relink-error-frame-2')).toBeInTheDocument();
    expect(mockAddToast).not.toHaveBeenCalled();
  });
});
