// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * InboxDetail — two-col layout migration (#813, final call site).
 *
 * InboxDetail was the last of the four hand-copied `.pv-session-detail2`
 * consumers. Unlike Sessions/Calibration it needs FOUR `__col` slots
 * (detection facts A/B, Files, Needs-review), so the migration rides on
 * `TwoColDetailLayout`'s `extraCols` rather than its `linked` slot.
 *
 * These assertions are the regression net for that: `__col` is
 * `flex: 0 1 400px; min-width: 340px` while `__linked` is
 * `flex: 0 0 auto; min-width: 160px`, so routing the table-shaped Files and
 * Needs-review blocks through `linked` would silently squeeze them. Nothing
 * else in the suite pins this structure.
 */

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render as rtlRender, screen } from '@testing-library/react';
import type { ReactElement } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type {
  InboxItemSummary,
  InboxFileMetadata_Serialize as InboxFileMetadata,
} from '@/bindings/index';
import type { InboxClassifyResponse } from '@/bindings/aliases';
import { InboxDetail } from '../InboxDetail';

function render(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return rtlRender(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

vi.mock('@/bindings/index', () => ({
  commands: {
    inboxReclassify: vi.fn(),
    inboxReclassifyV2: vi.fn(),
    inboxPropertyRegistry: vi
      .fn()
      .mockResolvedValue({ status: 'ok', data: [] }),
  },
}));

const item: InboxItemSummary & { organizationState?: string } = {
  inboxItemId: 'item-1',
  relativePath: 'M51/LUM/2025-05-03',
  fileCount: 2,
  lane: 'fits',
  format: 'fits',
  state: 'classified',
  contentSignature: 'sig-1',
  isMaster: false,
  masterFrameType: null,
  masterFilter: null,
  masterExposureS: null,
};

function classification(
  unclassifiedFiles: string[] = [],
): InboxClassifyResponse {
  return {
    inboxItemId: 'item-1',
    type: 'single_type',
    frameType: 'light',
    contentSignature: 'sig-1',
    breakdown: [
      {
        kind: 'light',
        count: 2,
        destinationPreview: 'M51/LUM/2025-05-03/light/',
        sampleFiles: ['frame_001.fits'],
      },
    ],
    unclassifiedFiles,
    sampleFiles: ['frame_001.fits'],
    computedAt: '2025-05-03T00:00:00Z',
  };
}

const fileMetadata = [
  {
    relativeFilePath: 'frame_001.fits',
    frameTypeEffective: 'light',
    filter: 'LUM',
    exposureS: 300,
    missingPathAttributes: [],
  },
] as unknown as InboxFileMetadata[];

describe('InboxDetail — two-col layout (#813)', () => {
  it('renders the shared .pv-session-detail2 structure', () => {
    render(
      <InboxDetail
        item={item}
        rootAbsolutePath="/astro/inbox"
        classification={classification()}
        fileMetadata={fileMetadata}
      />,
    );
    expect(screen.getByTestId('two-col-detail')).toBeInTheDocument();
  });

  it('gives Files its own __col, never the narrow __linked slot', () => {
    render(
      <InboxDetail
        item={item}
        rootAbsolutePath="/astro/inbox"
        classification={classification()}
        fileMetadata={fileMetadata}
      />,
    );
    const wrapper = screen.getByTestId('two-col-detail');

    // detection A + detection B + Files. No needs-review column: nothing
    // is unclassified here.
    expect(wrapper.querySelectorAll('[data-testid="detail-col"]')).toHaveLength(
      3,
    );
    expect(
      wrapper.querySelector('[data-testid="detail-linked"]'),
    ).not.toBeInTheDocument();

    // The Files trigger really is inside a __col, not floating elsewhere.
    const trigger = screen.getByTestId('inbox-files-popover-trigger');
    expect(trigger.closest('[data-testid="detail-col"]')).toBeInTheDocument();
  });

  it('adds a fourth __col for Needs-review when files are unclassified', () => {
    render(
      <InboxDetail
        item={item}
        rootAbsolutePath="/astro/inbox"
        classification={classification(['frame_002.fits'])}
        fileMetadata={fileMetadata}
      />,
    );
    const wrapper = screen.getByTestId('two-col-detail');
    expect(wrapper.querySelectorAll('[data-testid="detail-col"]')).toHaveLength(
      4,
    );
    expect(
      wrapper.querySelector('[data-testid="detail-linked"]'),
    ).not.toBeInTheDocument();

    // And it is the Needs-review block that occupies the extra column.
    const selectAll = screen.getByTestId('reclassify-select-all');
    expect(selectAll.closest('[data-testid="detail-col"]')).toBeInTheDocument();
  });
});
