// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * T074a — mixed frame-type derived dynamically; per-item inventory refs
 * render in SourceViewsSection (FR-033).
 *
 * Tests:
 * 1. InboxList groupBy options include "type" and "date" (not legacy "lane").
 * 2. ActionSidebar derives "mixed" label dynamically from classification.type.
 * 3. SourceViewsSection renders per-item inventory refs when items are present.
 * 4. InboxList filter label shows "image / video" (user-meaningful, not "lane").
 *
 * The former "Show ignored items" Cmd+K entry test is gone: that action was
 * removed entirely in spec 041 FR-051/T076 (CommandPalette.tsx no longer
 * builds a showIgnoredAction at all — see the comment above its ALL_ACTIONS).
 * The test here only compared a hardcoded string to itself ("tautology to
 * document intent") and never exercised CommandPalette, so it provided no
 * coverage of the removal either.
 */

import { screen } from '@testing-library/react';
import { renderWithQuery as render } from '@/test/renderWithQuery';
import { describe, it, expect, vi } from 'vitest';
import { InboxControls } from '../InboxControls';

// Hoisted so this mock actually applies to SourceViewsSection's import of
// listPreparedViews below — a vi.mock() called inside an `it()` body (the
// prior shape here) runs too late to intercept the module graph and Vitest
// silently falls through to the real implementation (mirrors the
// module-scope pattern SourceViewsSection.test.tsx already uses).
const { mockListPreparedViews } = vi.hoisted(() => ({
  mockListPreparedViews: vi.fn().mockResolvedValue({
    views: [
      {
        id: 'view-aabbcc',
        projectId: 'proj-1',
        kind: 'symlink',
        state: 'current',
        createdAt: '2026-06-01T00:00:00Z',
        itemCount: 2,
        items: [
          {
            id: 'item-1',
            inventoryItemId: 'inv-001',
            viewRelativePath: 'lights/Ha_001.fit',
            materialization: 'symlink',
            lastObservedState: 'present',
          },
          {
            id: 'item-2',
            inventoryItemId: 'inv-002',
            viewRelativePath: 'lights/Ha_002.fit',
            materialization: 'symlink',
            lastObservedState: 'present',
          },
        ],
      },
    ],
  }),
}));

vi.mock('@/features/projects/source-views', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('@/features/projects/source-views')>();
  return {
    ...actual,
    listPreparedViews: mockListPreparedViews,
  };
});

import { SourceViewsSection } from '@/features/projects/SourceViewsSection';

// The grouping controls moved from InboxList into the shared FilterToolbar on
// InboxPage (spec 043 #73/#31). InboxControls is now a thin shim that renders
// the grouping selects standalone for these option assertions.
function renderControls(dims: string[] = []) {
  return render(<InboxControls dims={dims} setSlot={vi.fn()} />);
}

// ── T074a test 2: InboxList groupBy options ────────────────────────────────────

describe('T074a: InboxList group-by options are user-meaningful', () => {
  it('renders dimension options (Target / Frame type) and no legacy "lane"', () => {
    renderControls();
    // The configurable grouping control offers user-meaningful dimensions,
    // not the legacy "lane" label.
    expect(
      screen.getByRole('option', { name: /group: target/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('option', { name: /group: frame type/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('option', { name: /group: lane/i }),
    ).not.toBeInTheDocument();
  });

  it('renders "Group: Date" option', () => {
    renderControls();
    expect(
      screen.getByRole('option', { name: /group: date/i }),
    ).toBeInTheDocument();
  });
});

// ── T074a test 4: per-item inventory refs in SourceViewsSection ──────────────

describe('T074a: SourceViewsSection per-item inventory refs', () => {
  it('renders inventory ref paths when items are present', async () => {
    const { findByText } = render(<SourceViewsSection projectId="proj-1" />);

    // The summary line "2 inventory refs" should appear
    await findByText(/2 inventory refs/i);
    expect(mockListPreparedViews).toHaveBeenCalled();
  });
});
