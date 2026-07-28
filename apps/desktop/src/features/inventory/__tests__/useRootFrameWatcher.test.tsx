// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * `useRootFrameWatcher` tests (spec 048 T023/T026 frontend).
 *
 * The hook is the surface-lifetime half of research R2's "detach when the
 * relevant surface closes; do not hold live watches on idle roots": unmount
 * MUST detach, and a null root MUST attach nothing.
 */

import { render } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const { mockAttach, mockDetach } = vi.hoisted(() => ({
  mockAttach: vi.fn(() => Promise.resolve()),
  mockDetach: vi.fn(() => Promise.resolve()),
}));

vi.mock('../inventoryIpc', () => ({
  inventoryWatcherAttach: (req: { rootId: string }) => mockAttach(req),
  inventoryWatcherDetach: (req: { rootId: string }) => mockDetach(req),
}));

import { useRootFrameWatcher } from '../store';

function Probe({ rootId }: { rootId: string | null }) {
  useRootFrameWatcher(rootId);
  return null;
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('useRootFrameWatcher', () => {
  it('attaches on mount and detaches on unmount', () => {
    const view = render(<Probe rootId="root-1" />);
    expect(mockAttach).toHaveBeenCalledWith({ rootId: 'root-1' });
    expect(mockDetach).not.toHaveBeenCalled();

    view.unmount();
    expect(mockDetach).toHaveBeenCalledWith({ rootId: 'root-1' });
  });

  it('attaches nothing when there is no root', () => {
    const view = render(<Probe rootId={null} />);
    view.unmount();
    expect(mockAttach).not.toHaveBeenCalled();
    expect(mockDetach).not.toHaveBeenCalled();
  });

  it('detaches the old root and attaches the new one when the root changes', () => {
    const view = render(<Probe rootId="root-1" />);
    view.rerender(<Probe rootId="root-2" />);

    expect(mockDetach).toHaveBeenCalledWith({ rootId: 'root-1' });
    expect(mockAttach).toHaveBeenCalledWith({ rootId: 'root-2' });
  });
});
