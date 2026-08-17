// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * MasterDetail — two-col layout migration (#813).
 *
 * Proves the fingerprint columns + stacked session popovers still render
 * through the shared `.pv-session-detail2` structure now that MasterDetail
 * builds it via `TwoColDetailLayout` instead of hand-copied divs.
 */

import { render as rtlRender, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';
import { MasterDetail } from './MasterDetail';
import type { CalibrationMaster_Serialize as CalibrationMaster } from '@/bindings/index';

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

vi.mock('@/bindings/index', () => ({
  commands: {
    calibrationMastersGet: vi.fn().mockResolvedValue({
      status: 'ok',
      data: { usedBySessionIds: [], compatibleSessions: [] },
    }),
    sessionsList: vi.fn().mockResolvedValue({ status: 'ok', data: [] }),
  },
}));

function makeMaster(): CalibrationMaster {
  return {
    id: 'm-1',
    kind: 'dark',
    fingerprint: {
      camera: 'ASI2600MM',
      exposureS: 300,
      tempC: -10,
      gain: 100,
      binning: '1x1',
    },
    sourceSessionId: 'cal-ses-001',
    createdAt: '2026-01-01T00:00:00Z',
    ageDays: 30,
    sizeBytes: 128 * 1024 * 1024,
    usedBySessionIds: [],
    usedByProjectIds: [],
  };
}

describe('MasterDetail — two-col layout (#813)', () => {
  it('renders the shared two-col-properties + stacked-linked structure', () => {
    render(
      <MasterDetail
        master={makeMaster()}
        prefillSuggestion={false}
        agingThresholdDays={90}
      />,
    );

    const wrapper = screen.getByTestId('two-col-detail');
    expect(wrapper).toBeInTheDocument();
    expect(wrapper.querySelectorAll('[data-testid="detail-col"]')).toHaveLength(
      2,
    );

    const linked = wrapper?.querySelector('[data-testid="detail-linked"]');
    expect(linked).toBeInTheDocument();

    // The two SessionListPopover instances (Used by / Compatible) stack
    // inside the single linked slot.
    expect(screen.getByText('Used by')).toBeInTheDocument();
    expect(screen.getByText('Compatible')).toBeInTheDocument();
  });
});
