// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SessionDetail calibration-linkage tests (#772).
 *
 * 1. A session with no calibration matches renders the explicit
 *    "no calibration match" empty state.
 * 2. A session with matches renders one row per assigned master.
 */

import {
  render as rtlRender,
  screen,
  fireEvent,
  waitFor,
} from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';

vi.mock('@/bindings/index', () => ({
  commands: {
    calibrationMatchUnassign: vi.fn().mockResolvedValue({
      status: 'ok',
      data: { status: 'success', contractVersion: '1.0.0', requestId: 'r1' },
    }),
  },
}));

// The frame-inventory / cleanup / notes children hit IPC or the query cache —
// stub them so this test isolates the calibration-linkage rendering.
vi.mock('./SessionFrameInventory', () => ({
  SessionFrameInventory: () => null,
}));
vi.mock('./RawFrameCleanupSection', () => ({
  RawFrameCleanupSection: () => null,
}));
vi.mock('./SessionNotesSection', () => ({ SessionNotesSection: () => null }));

import { SessionDetail } from './SessionDetail';
import type { InventorySession } from '@/bindings/index';

// #875: CalibrationLinkage's un-assign action reads useQueryClient() (via
// useCalibrationUnassign), so every render needs a QueryClientProvider
// ancestor now — mirrors MasterDetail.actions.test.tsx's render() wrapper.
function render(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return rtlRender(ui, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  });
}

function makeSession(
  overrides: Partial<InventorySession> = {},
): InventorySession {
  return {
    id: 'acq-1',
    name: 'M 51 · L — 2025-05-03',
    sourceId: 'root-1',
    frames: 2,
    type: 'light',
    target: 'M 51',
    filter: 'L',
    exposure: null,
    ...overrides,
  } as InventorySession;
}

describe('SessionDetail — display-name discriminator (#654)', () => {
  it('titles a metadata-less session with a disambiguated fallback, not the bare generic name', () => {
    render(
      <SessionDetail
        session={makeSession({
          target: null,
          name: 'Session — 2026-07-11',
          frames: 4,
          id: 'sess-a',
        })}
      />,
    );
    // Not the bare generic fallback — carries a discriminator suffix.
    expect(screen.queryByText('Session — 2026-07-11')).toBeNull();
    expect(screen.getByText(/Session — 2026-07-11 ·/)).toBeInTheDocument();
  });
});

describe('SessionDetail — backing-source connectivity (#889)', () => {
  it('renders a connectivity chip and hides Reveal for a non-active source', () => {
    render(
      <SessionDetail
        session={makeSession({})}
        sourceState="reconnect_required"
        revealVisible={false}
      />,
    );
    expect(screen.getByTestId('session-detail-connectivity')).toHaveTextContent(
      'Reconnect required',
    );
    expect(
      screen.queryByRole('button', { name: /show in file manager/i }),
    ).toBeNull();
  });

  it('renders no chip for an active source', () => {
    render(<SessionDetail session={makeSession({})} sourceState="active" />);
    expect(screen.queryByTestId('session-detail-connectivity')).toBeNull();
  });
});

describe('SessionDetail calibration linkage', () => {
  it('renders the no-calibration-match state when there are no matches', () => {
    render(<SessionDetail session={makeSession({ calibrationMatches: [] })} />);
    expect(screen.getByTestId('session-calib-empty')).toBeInTheDocument();
  });

  it('renders a row per assigned calibration master', () => {
    render(
      <SessionDetail
        session={makeSession({
          calibrationMatches: [
            {
              masterId: 'master-dark-1',
              kind: 'dark',
              score: 0.9,
              softMismatches: ['gain'],
              wasOverride: false,
            },
          ],
        })}
      />,
    );
    const list = screen.getByTestId('session-calib-list');
    expect(list).toHaveTextContent('master-dark-1');
    expect(list).toHaveTextContent('90%');
    expect(list).toHaveTextContent('gain');
  });
});

describe('SessionDetail calibration linkage — un-assign (#875)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('opens a confirm dialog and does NOT call unassign until confirmed', async () => {
    const { commands } = await import('@/bindings/index');
    render(
      <SessionDetail
        session={makeSession({
          id: 'sess-unassign-1',
          calibrationMatches: [
            {
              masterId: 'master-dark-1',
              kind: 'dark',
              score: 0.9,
              softMismatches: [],
              wasOverride: false,
            },
          ],
        })}
      />,
    );

    fireEvent.click(screen.getByTestId('session-calib-unassign-dark'));
    await screen.findByText('Remove this assignment?');
    expect(commands.calibrationMatchUnassign).not.toHaveBeenCalled();

    fireEvent.click(
      screen.getByTestId('session-calib-unassign-confirm-confirm-btn'),
    );

    await waitFor(() =>
      expect(commands.calibrationMatchUnassign).toHaveBeenCalledWith(
        expect.objectContaining({
          sessionId: 'sess-unassign-1',
          calibrationType: 'dark',
        }),
      ),
    );
  });

  it('cancel closes the dialog without calling unassign', async () => {
    const { commands } = await import('@/bindings/index');
    render(
      <SessionDetail
        session={makeSession({
          calibrationMatches: [
            {
              masterId: 'master-flat-1',
              kind: 'flat',
              score: 0.8,
              softMismatches: [],
              wasOverride: false,
            },
          ],
        })}
      />,
    );

    fireEvent.click(screen.getByTestId('session-calib-unassign-flat'));
    await screen.findByText('Remove this assignment?');
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(screen.queryByText('Remove this assignment?')).toBeNull();
    expect(commands.calibrationMatchUnassign).not.toHaveBeenCalled();
  });
});
