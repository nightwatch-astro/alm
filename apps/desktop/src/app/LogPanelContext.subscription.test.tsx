// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The log subscription must not be gated on the drawer being open.
 *
 * `Shell` renders `{expanded && <LogPanel />}` and `expanded` defaults to the
 * persisted `false`, so a subscription started from `LogPanel` may never start at
 * all. The backend forwarder advances its cursor as it emits, so entries emitted
 * while nothing is listening are unreachable to every later drain.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/react';
import { LogPanelProvider } from '@/app/LogPanelContext';
import { startLogSubscription } from '@/data/logSubscription';

vi.mock('@/data/logSubscription', () => ({
  startLogSubscription: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    settingsGet: vi
      .fn()
      .mockResolvedValue({ status: 'ok', data: { values: {} } }),
    settingsUpdate: vi.fn().mockResolvedValue({ status: 'ok', data: {} }),
  },
}));

vi.mock('@/api/ipc', () => ({
  unwrap: (r: { data: unknown }) => r.data,
}));

describe('LogPanelProvider log subscription', () => {
  beforeEach(() => {
    vi.mocked(startLogSubscription).mockClear();
  });

  it('starts the subscription without rendering the panel', async () => {
    render(
      <LogPanelProvider>
        <div data-testid="child" />
      </LogPanelProvider>,
    );

    await waitFor(() => {
      expect(startLogSubscription).toHaveBeenCalledTimes(1);
    });
  });
});
