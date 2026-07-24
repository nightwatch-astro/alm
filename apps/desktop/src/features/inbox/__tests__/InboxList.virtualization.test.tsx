// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * Spec 041 — InboxList virtualization.
 *
 * The grouped/flat inbox list is flattened into a single visual-row array and
 * windowed with `@tanstack/react-virtual` so a large inbox mounts only the rows
 * in view. These tests force a measured viewport (jsdom reports 0 height by
 * default, which makes the component fall back to rendering every row — the path
 * the other inbox tests exercise) and then assert true windowing:
 *
 *  1. A 500-item flat list mounts far fewer than 500 rows, while the spacer's
 *     height reflects all 500 (so scroll extent is correct).
 *  2. Grouping + collapse still drive the (windowed) flattened model on a long
 *     list: collapsing a group shrinks the total size and reveals the next
 *     group's header.
 *
 * No shared `vitest.setup.ts` change is needed — `@tanstack/virtual-core`
 * degrades gracefully when `ResizeObserver` is absent (it still takes one
 * `getBoundingClientRect` measurement on mount), so stubbing that rect here is
 * enough to switch the component into its windowed path.
 */

import {
  render,
  screen,
  fireEvent,
  waitFor,
  cleanup,
} from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { InboxList } from '../InboxList';
import { FilterToolbar } from '@/components';
import {
  useGrouping,
  __resetGroupingRegistryForTest,
} from '@/lib/use-grouping';
import { GROUPING_DIMENSIONS, GROUPING_STORAGE_KEY } from '../InboxControls';
import type { InboxListItem } from '@/bindings/index';
import { __resetScopeRegistryForTest } from '@/data/persisted-state';

// Harness mirroring InboxPage: useGrouping + FilterToolbar.grouping feed InboxList.
function Harness({ items }: { items: InboxListItem[] }) {
  const { dims, setSlot } = useGrouping({
    storageKey: GROUPING_STORAGE_KEY,
    validIds: GROUPING_DIMENSIONS.map((d) => d.id),
    defaultDims: [],
  });
  return (
    <>
      <FilterToolbar
        grouping={{
          dimensions: GROUPING_DIMENSIONS.map((d) => ({
            value: d.id,
            label: d.label(),
          })),
          dims,
          setSlot,
        }}
      />
      <InboxList
        items={items}
        selectedId={null}
        onSelect={vi.fn()}
        filterType="all"
        dims={dims}
      />
    </>
  );
}

// ── Fixtures ────────────────────────────────────────────────────────────────────

function makeItem(
  over: Partial<InboxListItem> & { inboxItemId: string },
): InboxListItem {
  return {
    relativePath: over.inboxItemId,
    fileCount: 1,
    lane: 'fits',
    format: 'fits',
    state: 'classified',
    contentSignature: `sig-${over.inboxItemId}`,
    isMaster: false,
    masterFrameType: null,
    masterFilter: null,
    masterExposureS: null,
    rootAbsolutePath: '/lib/root',
    organizationState: 'unorganized',
    groupTarget: null,
    groupFrameType: null,
    groupDate: null,
    groupFilter: null,
    groupExposure: null,
    groupInstrument: null,
    ...over,
  } as InboxListItem;
}

// ── getBoundingClientRect stub: gives every element a real height so the ────────
// virtualizer sees outerSize > 0 and windows instead of falling back. ───────────
const ROW_PX = 40;
let gbcrSpy: ReturnType<typeof vi.spyOn> | undefined;

function rect(height: number): DOMRect {
  return {
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: 300,
    bottom: height,
    width: 300,
    height,
    toJSON: () => ({}),
  };
}

beforeEach(() => {
  localStorage.clear();
  __resetGroupingRegistryForTest();
  __resetScopeRegistryForTest();
  gbcrSpy = vi
    .spyOn(HTMLElement.prototype, 'getBoundingClientRect')
    .mockImplementation(() => rect(ROW_PX));
});

afterEach(() => {
  gbcrSpy?.mockRestore();
  cleanup();
});

// Off-window scroll extent. The shared Table windows with the padding-spacer
// pattern (two sentinel <tr class="pv-table__spacer"> bracket the mounted
// slice), so the extent the window is NOT showing is the sum of the spacer
// heights — the windowed-pattern stand-in for the old single-sizer height.
function sizerHeight(): number {
  let h = 0;
  document.querySelectorAll('[data-testid="table-spacer"] td').forEach((td) => {
    h += parseFloat((td as HTMLElement).style.height || '0');
  });
  return h;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('InboxList — virtualization', () => {
  it('(1) windows a large flat list: mounts << total while the spacer spans all rows', async () => {
    const items = Array.from({ length: 500 }, (_, i) =>
      makeItem({ inboxItemId: `f${String(i).padStart(3, '0')}` }),
    );

    render(
      <InboxList
        items={items}
        selectedId={null}
        onSelect={vi.fn()}
        filterType="all"
        onFilterTypeChange={vi.fn()}
      />,
    );

    // Once the viewport is measured, only a small window of rows is mounted.
    await waitFor(() => {
      const mounted = screen.getAllByTestId(/^inbox-item-/).length;
      expect(mounted).toBeGreaterThan(0);
      expect(mounted).toBeLessThan(150);
    });

    // The spacer height reflects ALL 500 rows (scroll extent), not just the
    // mounted window — comfortably larger than what a handful of rows occupy.
    expect(sizerHeight()).toBeGreaterThan(500 * (ROW_PX / 2));
  });

  it('(2) grouping + collapse drive the windowed model on a long list', async () => {
    // Two frame-type groups, 250 items each. Sorted by label → "dark" first.
    const items: InboxListItem[] = [
      ...Array.from({ length: 250 }, (_, i) =>
        makeItem({
          inboxItemId: `d${String(i).padStart(3, '0')}`,
          groupFrameType: 'dark',
        }),
      ),
      ...Array.from({ length: 250 }, (_, i) =>
        makeItem({
          inboxItemId: `l${String(i).padStart(3, '0')}`,
          groupFrameType: 'light',
        }),
      ),
    ];

    render(<Harness items={items} />);

    fireEvent.change(screen.getByLabelText('Group by'), {
      target: { value: 'frameType' },
    });

    // First group ("dark") header is in the window; rendering is windowed so far
    // fewer than 500 item rows are mounted. The far-away "light" header is below
    // the window, so it is NOT mounted yet.
    await waitFor(() => {
      expect(
        screen.getByTestId('inbox-group-frameType-dark'),
      ).toBeInTheDocument();
    });
    expect(screen.getAllByTestId(/^inbox-item-/).length).toBeLessThan(150);
    expect(
      screen.queryByTestId('inbox-group-frameType-light'),
    ).not.toBeInTheDocument();

    const heightExpanded = sizerHeight();

    // Collapse the first group → its 250 leaves leave the flattened model. Total
    // size shrinks and the second group's header moves up into the window.
    fireEvent.click(screen.getByTestId('inbox-group-frameType-dark'));

    await waitFor(() => {
      expect(
        screen.getByTestId('inbox-group-frameType-light'),
      ).toBeInTheDocument();
    });
    expect(sizerHeight()).toBeLessThan(heightExpanded);
  });
});
