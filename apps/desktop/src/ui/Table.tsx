// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { forwardRef, useCallback, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import type {
  ReactNode,
  CSSProperties,
  TableHTMLAttributes,
  MouseEvent,
  KeyboardEvent,
} from 'react';
import * as tbl from './Table.css';

/** Base left padding (px) + per-depth step for indented (grouped) table rows. */
const INDENT_BASE = 8;
const INDENT_STEP = 12;

/**
 * Left indent (px) for a grouped row at `depth`. Shared by every list-page
 * table so the depth→padding mapping lives in one place; pass the result as a
 * row's `_indent`.
 */
export function tableIndent(depth: number): number {
  return INDENT_BASE + depth * INDENT_STEP;
}

/**
 * Move keyboard focus to the adjacent focusable (selectable) row, skipping
 * spacer rows and non-clickable rows. Wired to Arrow Up/Down on clickable rows.
 *
 * Non-virtualized path: DOM query within the same tbody. This only sees
 * rendered rows, which is fine when all rows are in the DOM.
 */
function focusAdjacentRow(current: HTMLTableRowElement, dir: 1 | -1) {
  const scope = current.closest('tbody') ?? current.parentElement;
  if (!scope) return;
  const clickable = Array.from(
    scope.querySelectorAll<HTMLTableRowElement>(
      'tr[data-row-clickable="true"]',
    ),
  );
  const idx = clickable.indexOf(current);
  clickable[idx + dir]?.focus();
}

/**
 * Move keyboard focus to the first or last focusable (selectable) row in the
 * same scope as `current`. Wired to Home/End on clickable rows.
 *
 * Non-virtualized path only — see virtualizer-aware counterparts below.
 */
function focusEdgeRow(current: HTMLTableRowElement, edge: 'first' | 'last') {
  const scope = current.closest('tbody') ?? current.parentElement;
  if (!scope) return;
  const clickable = scope.querySelectorAll<HTMLTableRowElement>(
    'tr[data-row-clickable="true"]',
  );
  const target =
    edge === 'first' ? clickable[0] : clickable[clickable.length - 1];
  target?.focus();
}

/**
 * Focus a row by its model index after the virtualizer has scrolled it into
 * view. Uses a single rAF so the DOM settles (virtualizer flushes its state
 * synchronously via scrollToIndex, but the React re-render painting the new
 * rows happens on the next frame).
 *
 * `scrollContainer` is the element with `data-virtual-scroll="true"` — the
 * same element the virtualizer measures against.
 */
function focusRowByIndex(
  scrollContainer: Element,
  targetIdx: number,
  scrollToIndex: (idx: number, opts?: { behavior?: ScrollBehavior }) => void,
) {
  scrollToIndex(targetIdx);
  requestAnimationFrame(() => {
    const el = scrollContainer.querySelector<HTMLTableRowElement>(
      `tr[data-row-index="${targetIdx}"][data-row-clickable="true"]`,
    );
    el?.focus();
  });
}

export interface TableColumn {
  key: string;
  /** Header content. Accepts a plain string or rich nodes (e.g. sortable header buttons). */
  label: ReactNode;
  className?: string;
  style?: CSSProperties;
  cellStyle?: CSSProperties;
  /**
   * ARIA sort state emitted on the `<th>` (NOT on the inner SortHeader button,
   * where it would be invalid). Sortable tables pass
   * `ariaSortFor(active, dir)` from `@/components` so only the active sort
   * column announces its direction.
   */
  ariaSort?: 'ascending' | 'descending' | 'none' | 'other';
}

export type TableRow = {
  [key: string]:
    | ReactNode
    | CSSProperties
    | ((evt: MouseEvent) => void)
    | string
    | undefined;
  /** Optional per-row CSS applied to the <tr> element. Not rendered as a cell. */
  _rowStyle?: CSSProperties;
  /** Optional per-row className applied to the <tr> element. Not rendered as a cell. */
  _rowClassName?: string;
  /**
   * Optional activation handler for the row. When set, the row becomes a
   * keyboard-operable option: focusable (`tabIndex=0`), activated by
   * Enter/Space, and navigable with Arrow Up/Down. Named `_onClick` for
   * backward compatibility; it also fires on keyboard activation.
   */
  _onClick?: (evt: MouseEvent) => void;
  /**
   * Selection state for a clickable row. When provided, emitted as
   * `aria-selected` on the `<tr>`. Omit for clickable rows that navigate
   * rather than select.
   */
  _selected?: boolean;
  /**
   * Left indent (px) for a grouped/nested row, applied to the first cell's
   * content via a CSS variable. Use `tableIndent(depth)` to compute it.
   */
  _indent?: number;
  /** Optional `data-testid` applied to the <tr> element. Not rendered as a cell. */
  _testid?: string;
  /** Optional `data-kind` applied to the <tr> element for row-type-based queries. Not rendered as a cell. */
  _rowKind?: string;
  /** Optional onboarding spotlight anchor applied to the interactive row. */
  _guideAnchor?: string;
};

export interface TableProps extends TableHTMLAttributes<HTMLTableElement> {
  columns: TableColumn[];
  rows: TableRow[];
  /**
   * Window the rows for large lists using the padding-spacer pattern (a real
   * CSS `<table>` bracketed by two sentinel spacer `<tr>`s). The table is
   * wrapped in its OWN scroll container — give that container a bounded height
   * (e.g. a flex child of a column). Default false (plain table, all rows).
   *
   * When the scroll element has no measurable height (jsdom / first paint) the
   * virtualizer yields zero items and every row renders without spacers, so
   * windowing is a runtime perf optimisation, never a behaviour change.
   */
  virtualized?: boolean;
  /** Estimated row height (px) seeding the virtualizer. Default 36. */
  estimateRowHeight?: number;
  /** className on the scroll wrapper (virtualized mode only). */
  scrollClassName?: string;
  /** `data-testid` on the scroll wrapper (virtualized mode only). */
  scrollTestId?: string;
}

export const Table = forwardRef<HTMLTableElement, TableProps>(function Table(
  {
    columns,
    rows,
    className,
    virtualized = false,
    estimateRowHeight = 36,
    scrollClassName,
    scrollTestId,
    ...rest
  },
  ref,
) {
  const cls = [tbl.root, className].filter(Boolean).join(' ');

  // The scroll viewport the virtualizer measures against (virtualized mode).
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null);
  const scrollRef = useCallback((node: HTMLDivElement | null) => {
    setScrollEl(node);
  }, []);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollEl,
    estimateSize: () => estimateRowHeight,
    overscan: 8,
    enabled: virtualized,
  });

  const renderRow = (
    row: TableRow,
    ri: number,
    onNav?: (currentIdx: number, dir: 1 | -1 | 'first' | 'last') => void,
  ) => {
    const onClick = row._onClick;
    const clickable = typeof onClick === 'function';
    const style: CSSProperties | undefined =
      row._indent != null
        ? // The custom property carries the per-row indent; consumed by the
          // `.pv-table__row--indented` rule (primitives.css).
          {
            ...row._rowStyle,
            ['--pv-row-indent' as string]: `${row._indent}px`,
          }
        : row._rowStyle;
    const className =
      [
        row._rowClassName,
        clickable ? tbl.rowClickable : null,
        row._indent != null ? tbl.rowIndented : null,
      ]
        .filter(Boolean)
        .join(' ') || undefined;
    return (
      <tr
        key={ri}
        // eslint-disable-next-line no-restricted-syntax -- dynamic: Table row style passthrough (_rowStyle) + per-row indent CSS var
        style={style}
        className={className}
        onClick={onClick}
        onKeyDown={
          clickable
            ? (e: KeyboardEvent<HTMLTableRowElement>) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onClick(e as unknown as MouseEvent);
                } else if (e.key === 'ArrowDown') {
                  e.preventDefault();
                  if (onNav) {
                    onNav(ri, 1);
                  } else {
                    focusAdjacentRow(e.currentTarget, 1);
                  }
                } else if (e.key === 'ArrowUp') {
                  e.preventDefault();
                  if (onNav) {
                    onNav(ri, -1);
                  } else {
                    focusAdjacentRow(e.currentTarget, -1);
                  }
                } else if (e.key === 'Home') {
                  e.preventDefault();
                  if (onNav) {
                    onNav(ri, 'first');
                  } else {
                    focusEdgeRow(e.currentTarget, 'first');
                  }
                } else if (e.key === 'End') {
                  e.preventDefault();
                  if (onNav) {
                    onNav(ri, 'last');
                  } else {
                    focusEdgeRow(e.currentTarget, 'last');
                  }
                }
              }
            : undefined
        }
        // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- selectable row: focusable only when it carries an activation handler
        tabIndex={clickable ? 0 : undefined}
        aria-selected={row._selected}
        data-row-clickable={clickable ? 'true' : undefined}
        data-row-index={ri}
        data-testid={row._testid}
        data-kind={row._rowKind}
        data-guide-anchor={row._guideAnchor}
      >
        {columns.map((c, ci) => (
          // eslint-disable-next-line no-restricted-syntax -- dynamic: Table cell style passthrough from caller (cellStyle)
          <td key={ci} className={c.className} style={c.cellStyle}>
            {row[c.key] as ReactNode}
          </td>
        ))}
      </tr>
    );
  };

  const head = (
    <thead>
      <tr>
        {columns.map((c, i) => (
          <th
            key={i}
            className={c.className}
            // eslint-disable-next-line no-restricted-syntax -- dynamic: caller-provided column header style passthrough
            style={c.style}
            aria-sort={c.ariaSort}
          >
            {c.label}
          </th>
        ))}
      </tr>
    </thead>
  );

  if (!virtualized) {
    return (
      <table ref={ref} className={cls} {...rest}>
        {head}
        <tbody>{rows.map((row, ri) => renderRow(row, ri))}</tbody>
      </table>
    );
  }

  // ── Virtualized (padding-spacer pattern) ──────────────────────────────────
  const virtualItems = rowVirtualizer.getVirtualItems();
  const windowed = virtualItems.length > 0;
  const totalSize = rowVirtualizer.getTotalSize();
  const paddingBefore = windowed ? virtualItems[0].start : 0;
  const paddingAfter = windowed
    ? totalSize - virtualItems[virtualItems.length - 1].end
    : 0;
  const colCount = columns.length;

  // Model-aware navigation for virtualized mode: navigate by row-model index
  // so Arrow keys can reach rows outside the render window. When the target
  // index is off-screen, scrollToIndex brings it into the window; a rAF then
  // focuses the newly-rendered <tr>. Home/End scan the rows model array (not
  // the DOM) so they always land on the true first/last clickable row.
  const handleVirtNav = (
    currentIdx: number,
    dir: 1 | -1 | 'first' | 'last',
  ) => {
    if (!scrollEl) return;
    let targetIdx: number;
    if (dir === 'first') {
      targetIdx = rows.findIndex((r) => typeof r._onClick === 'function');
    } else if (dir === 'last') {
      // findLastIndex is ES2023; scan manually to stay within the ES2022 lib target.
      targetIdx = -1;
      for (let i = rows.length - 1; i >= 0; i--) {
        if (typeof rows[i]._onClick === 'function') {
          targetIdx = i;
          break;
        }
      }
    } else {
      // Walk the model in the given direction, skipping non-clickable rows.
      let next = currentIdx + dir;
      while (next >= 0 && next < rows.length) {
        if (typeof rows[next]._onClick === 'function') break;
        next += dir;
      }
      if (next < 0 || next >= rows.length) return;
      targetIdx = next;
    }
    if (targetIdx < 0) return;
    focusRowByIndex(scrollEl, targetIdx, (idx, opts) =>
      rowVirtualizer.scrollToIndex(idx, opts),
    );
  };

  return (
    <div
      ref={scrollRef}
      className={[tbl.scroll, scrollClassName].filter(Boolean).join(' ')}
      data-testid={scrollTestId}
      data-virtual-scroll="true"
    >
      <table ref={ref} className={cls} {...rest}>
        {head}
        <tbody>
          {windowed ? (
            <>
              {paddingBefore > 0 && (
                <tr aria-hidden="true" className={tbl.spacerRow} data-testid="table-spacer">
                  {/* eslint-disable-next-line jsx-a11y/control-has-associated-label -- decorative spacer in aria-hidden row, no label needed */}
                  <td
                    colSpan={colCount}
                    // eslint-disable-next-line no-restricted-syntax -- dynamic: virtualizer before-spacer height
                    style={{ height: `${paddingBefore}px` }}
                  />
                </tr>
              )}
              {virtualItems.map((vi) =>
                renderRow(rows[vi.index], vi.index, handleVirtNav),
              )}
              {paddingAfter > 0 && (
                <tr aria-hidden="true" className={tbl.spacerRow} data-testid="table-spacer">
                  {/* eslint-disable-next-line jsx-a11y/control-has-associated-label -- decorative spacer in aria-hidden row, no label needed */}
                  <td
                    colSpan={colCount}
                    // eslint-disable-next-line no-restricted-syntax -- dynamic: virtualizer after-spacer height
                    style={{ height: `${paddingAfter}px` }}
                  />
                </tr>
              )}
            </>
          ) : (
            rows.map((row, ri) => renderRow(row, ri, handleVirtNav))
          )}
        </tbody>
      </table>
    </div>
  );
});
Table.displayName = 'Table';
