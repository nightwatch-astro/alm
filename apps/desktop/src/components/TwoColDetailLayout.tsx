// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import {
  wrapper,
  col as colCls,
  linked as linkedCls,
  head,
  muted as mutedCls,
} from './two-col-detail-layout.css';

export interface TwoColDetailLayoutProps {
  /** Left property column (typically a `PropertyTable` half). */
  colA: ReactNode;
  /**
   * Right property column (typically a `PropertyTable` half). Pass `null` to
   * omit the slot entirely — an empty `__col` is not free, it still claims
   * `min-width: 340px` of the flex row and reads as a gap. For callers that
   * spread a variable-length fact list across the two columns and may end up
   * with nothing for the second.
   */
  colB?: ReactNode;
  /**
   * Further full-width property columns rendered after `colB`, each in its own
   * `__col`. `null`/`undefined` entries are skipped, so a caller can pass
   * conditional slots positionally. Use this — not `linked` — for anything
   * table-shaped: `__col` is `flex: 0 1 400px; min-width: 340px` whereas
   * `__linked` is `flex: 0 0 auto; min-width: 160px`, which squeezes real
   * content.
   */
  extraCols?: ReactNode[];
  /**
   * Trailing slot — a linked-entity list, a stacked pair of popovers, or any
   * other narrow right-aligned block. Omitted entirely when there is nothing
   * to show.
   */
  linked?: ReactNode;
  /** Extra class appended to the linked slot (e.g. the `--stack` modifier
   * used when it holds more than one stacked block). */
  linkedClassName?: string;
}

/**
 * Renders a shared detail layout with property columns and an optional linked section.
 *
 * @param colB - Optional second property column.
 * @param extraCols - Additional property columns; nullish entries are skipped.
 * @param linked - Optional content for the trailing linked section.
 * @param linkedClassName - Additional class name for the linked section.
 */
export function TwoColDetailLayout({
  colA,
  colB,
  extraCols,
  linked,
  linkedClassName,
}: TwoColDetailLayoutProps) {
  const finalLinkedCls = [linkedCls, linkedClassName].filter(Boolean).join(' ');
  return (
    <div className={wrapper} data-testid="two-col-detail">
      <div className={colCls} data-testid="detail-col">
        {colA}
      </div>
      {colB != null && (
        <div className={colCls} data-testid="detail-col">
          {colB}
        </div>
      )}
      {(extraCols ?? []).map((item, i) =>
        item == null ? null : (
          // Positional slots: a caller passes a fixed-length array whose
          // entries may be null, so the index IS the stable identity here.
          <div className={colCls} data-testid="detail-col" key={i}>
            {item}
          </div>
        ),
      )}
      {linked != null && (
        <div className={finalLinkedCls} data-testid="detail-linked">
          {linked}
        </div>
      )}
    </div>
  );
}

export interface DetailLinkedGroupProps {
  /** Heading rendered above the content (`pv-session-detail2__head`). */
  label: ReactNode;
  /** Renders `emptyLabel` instead of `children` — e.g. a zero-count list. */
  empty?: boolean;
  /** Muted placeholder shown when `empty` (`pv-session-detail2__muted`). */
  emptyLabel?: ReactNode;
  children?: ReactNode;
}

/**
 * Renders a labelled content block with an optional empty-state placeholder.
 *
 * @param empty - Whether to display the empty-state content instead of the children.
 * @param emptyLabel - Content displayed when the block is empty.
 */
export function DetailLinkedGroup({
  label,
  empty,
  emptyLabel,
  children,
}: DetailLinkedGroupProps) {
  return (
    <div>
      <div className={head} data-testid="detail-group-head">
        {label}
      </div>
      {empty ? <span className={mutedCls}>{emptyLabel}</span> : children}
    </div>
  );
}
