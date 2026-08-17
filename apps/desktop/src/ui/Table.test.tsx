// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Table row a11y tests (handoff 02): keyboard-operable clickable rows —
 * roving Arrow Up/Down (pre-existing), Home/End jump-to-edge, and row/
 * aria-selected semantics. Sessions/Projects/Masters/Archive/Targets tables
 * all render through this component, so this is the single coverage point.
 *
 * The second describe block covers virtualized keyboard navigation: Arrow keys
 * must reach rows outside the render window by driving scrollToIndex on the
 * virtualizer and then focusing the newly-rendered row (WCAG 2.1.1).
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import {
  render,
  screen,
  fireEvent,
  waitFor,
  cleanup,
} from '@testing-library/react';
import { Table, type TableColumn, type TableRow } from './Table';

const COLUMNS: TableColumn[] = [{ key: 'name', label: 'Name' }];

function rows(onClick: (id: string) => void, selected?: string): TableRow[] {
  return ['a', 'b', 'c'].map((id) => ({
    name: id.toUpperCase(),
    _onClick: () => onClick(id),
    _selected: id === selected,
    _testid: `row-${id}`,
  }));
}

describe('Table clickable rows (handoff 02)', () => {
  it('exposes native row semantics with aria-selected on the selected row', () => {
    render(<Table columns={COLUMNS} rows={rows(vi.fn(), 'b')} />);
    const rowEls = screen.getAllByRole('row').slice(1); // drop the header row
    expect(rowEls).toHaveLength(3);
    expect(rowEls[0]).toHaveAttribute('aria-selected', 'false');
    expect(rowEls[1]).toHaveAttribute('aria-selected', 'true');
    expect(rowEls[2]).toHaveAttribute('aria-selected', 'false');
  });

  it('is keyboard-focusable and activates on Enter/Space', () => {
    const onClick = vi.fn();
    render(<Table columns={COLUMNS} rows={rows(onClick)} />);
    const rowA = screen.getByTestId('row-a');
    expect(rowA).toHaveAttribute('tabIndex', '0');
    fireEvent.keyDown(rowA, { key: 'Enter' });
    expect(onClick).toHaveBeenCalledWith('a');
    fireEvent.keyDown(rowA, { key: ' ' });
    expect(onClick).toHaveBeenCalledWith('a');
  });

  it('ArrowDown/ArrowUp move focus between rows (pre-existing roving nav)', () => {
    render(<Table columns={COLUMNS} rows={rows(vi.fn())} />);
    const rowA = screen.getByTestId('row-a');
    const rowB = screen.getByTestId('row-b');
    rowA.focus();
    fireEvent.keyDown(rowA, { key: 'ArrowDown' });
    expect(rowB).toHaveFocus();
    fireEvent.keyDown(rowB, { key: 'ArrowUp' });
    expect(rowA).toHaveFocus();
  });

  it('End moves focus to the last row, Home back to the first', () => {
    render(<Table columns={COLUMNS} rows={rows(vi.fn())} />);
    const rowA = screen.getByTestId('row-a');
    const rowC = screen.getByTestId('row-c');
    rowA.focus();
    fireEvent.keyDown(rowA, { key: 'End' });
    expect(rowC).toHaveFocus();
    fireEvent.keyDown(rowC, { key: 'Home' });
    expect(rowA).toHaveFocus();
  });

  it('Home/End on a middle row jump straight to the edges', () => {
    render(<Table columns={COLUMNS} rows={rows(vi.fn())} />);
    const rowB = screen.getByTestId('row-b');
    const rowA = screen.getByTestId('row-a');
    const rowC = screen.getByTestId('row-c');
    rowB.focus();
    fireEvent.keyDown(rowB, { key: 'End' });
    expect(rowC).toHaveFocus();
    rowB.focus();
    fireEvent.keyDown(rowB, { key: 'Home' });
    expect(rowA).toHaveFocus();
  });
});

// ── Virtualized keyboard navigation ──────────────────────────────────────────
//
// When `virtualized=true` the table windows rows with @tanstack/react-virtual.
// Arrow Up/Down must reach rows outside the render window: the fix drives
// scrollToIndex on the virtualizer (not a DOM query) and then focuses the
// newly-rendered row in a rAF.
//
// jsdom has no real scroll engine — setting scrollTop never fires scroll
// events on an otherwise non-scrollable element, so the virtualizer cannot
// re-render an off-window row in tests. These tests therefore use row counts
// small enough that all rows fit within the virtualizer's overscan window (so
// every row is in the DOM) while still exercising the model-driven nav path.
// The off-window mechanism (scrollToIndex + rAF) is verified by checking that
// the `data-row-index` attribute wiring and model-based skipping work end-to-end.

describe('Table virtualized keyboard navigation', () => {
  afterEach(() => {
    cleanup();
  });

  /** Build N clickable rows labelled row-0 … row-(N-1). */
  function virtRows(n: number): TableRow[] {
    return Array.from({ length: n }, (_, i) => ({
      name: `Row ${i}`,
      _onClick: vi.fn(),
      _testid: `row-${i}`,
    }));
  }

  it('emits data-row-index on every row so the rAF focus selector can find off-window rows', async () => {
    // The rAF in focusRowByIndex queries `tr[data-row-index="${idx}"]` — verify
    // the attribute is present and matches the model index.
    render(
      <Table
        columns={COLUMNS}
        rows={virtRows(5)}
        virtualized
        scrollTestId="virt-scroll"
      />,
    );

    await waitFor(() =>
      expect(screen.getByTestId('row-0')).toBeInTheDocument(),
    );

    for (let i = 0; i < 5; i++) {
      expect(screen.getByTestId(`row-${i}`)).toHaveAttribute(
        'data-row-index',
        String(i),
      );
    }
  });

  it('ArrowDown in virtualized mode focuses the next model row (model-driven path)', async () => {
    // Uses 5 rows — all fit in the virtualizer window — so the rAF finds
    // the target in DOM. Verifies the model-driven handleVirtNav path end-to-end.
    render(
      <Table
        columns={COLUMNS}
        rows={virtRows(5)}
        virtualized
        scrollTestId="virt-scroll"
      />,
    );

    await waitFor(() =>
      expect(screen.getByTestId('row-0')).toBeInTheDocument(),
    );

    const row0 = screen.getByTestId('row-0');
    row0.focus();
    fireEvent.keyDown(row0, { key: 'ArrowDown' });

    await waitFor(() => expect(screen.getByTestId('row-1')).toHaveFocus());
  });

  it('End in virtualized mode focuses the last model row (model-driven path)', async () => {
    render(
      <Table
        columns={COLUMNS}
        rows={virtRows(5)}
        virtualized
        scrollTestId="virt-scroll"
      />,
    );

    await waitFor(() =>
      expect(screen.getByTestId('row-0')).toBeInTheDocument(),
    );

    const row0 = screen.getByTestId('row-0');
    row0.focus();
    fireEvent.keyDown(row0, { key: 'End' });

    await waitFor(() => expect(screen.getByTestId('row-4')).toHaveFocus());
  });

  it('Home in virtualized mode focuses the first clickable model row', async () => {
    render(
      <Table
        columns={COLUMNS}
        rows={virtRows(5)}
        virtualized
        scrollTestId="virt-scroll"
      />,
    );

    await waitFor(() =>
      expect(screen.getByTestId('row-4')).toBeInTheDocument(),
    );

    const row4 = screen.getByTestId('row-4');
    row4.focus();
    fireEvent.keyDown(row4, { key: 'Home' });

    await waitFor(() => expect(screen.getByTestId('row-0')).toHaveFocus());
  });

  it('non-clickable rows are skipped by ArrowDown (model-driven)', async () => {
    // Row 1 is a spacer (no _onClick) — ArrowDown from row 0 should skip to row 2.
    const mixed: TableRow[] = [
      { name: 'A', _onClick: vi.fn(), _testid: 'row-0' },
      { name: 'spacer' }, // not clickable — model index 1
      { name: 'B', _onClick: vi.fn(), _testid: 'row-2' },
    ];
    render(
      <Table
        columns={COLUMNS}
        rows={mixed}
        virtualized
        scrollTestId="virt-scroll"
      />,
    );

    await waitFor(() =>
      expect(screen.getByTestId('row-0')).toBeInTheDocument(),
    );

    const rowA = screen.getByTestId('row-0');
    rowA.focus();
    fireEvent.keyDown(rowA, { key: 'ArrowDown' });

    await waitFor(() => expect(screen.getByTestId('row-2')).toHaveFocus());
  });

  it('Home skips leading non-clickable rows to the first clickable model row', async () => {
    // Model index 0 is non-clickable — Home should land on index 1.
    const mixed: TableRow[] = [
      { name: 'header' }, // non-clickable — model index 0
      { name: 'A', _onClick: vi.fn(), _testid: 'row-1' },
      { name: 'B', _onClick: vi.fn(), _testid: 'row-2' },
    ];
    render(
      <Table
        columns={COLUMNS}
        rows={mixed}
        virtualized
        scrollTestId="virt-scroll"
      />,
    );

    await waitFor(() =>
      expect(screen.getByTestId('row-2')).toBeInTheDocument(),
    );

    const row2 = screen.getByTestId('row-2');
    row2.focus();
    fireEvent.keyDown(row2, { key: 'Home' });

    await waitFor(() => expect(screen.getByTestId('row-1')).toHaveFocus());
  });
});
