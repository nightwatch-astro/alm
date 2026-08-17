// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SortHeader — the single shared sortable column-header control (button-in-th).
 *
 * Every list table (Sessions / Calibration / Projects / Targets / Inbox) renders
 * the SAME header affordance: a borderless button carrying the column label and,
 * when it is the active sort column, a direction arrow. Layout, spacing, hover,
 * active color, and the arrow glyph live HERE and in the single `.pv-sorth` CSS
 * block — callers parameterise CONTENT only (`label`, `active`, `dir`, `onClick`,
 * `ariaLabel`, optional `title`). Do NOT re-implement per-feature `*-sorth`
 * classes; that is what previously drifted (and left Inbox unstyled).
 *
 * Accessibility: the active sort direction is shown visually via the arrow. NOTE
 * `aria-sort` is intentionally NOT set here — per ARIA it is only valid on the
 * `columnheader`/`th` element, not on a `button`, so setting it on this button
 * is a no-op. Announcing sort state to assistive tech lives on the enclosing
 * `<th>`: pass `ariaSort: ariaSortFor(active, dir)` (exported below) in the
 * shared `TableColumn`, and the shared `Table` emits it on the `<th>`. Tables
 * that render their own `<th>` (Targets) apply the same helper directly.
 *
 * Non-sortable columns should render their plain label node directly rather than
 * using this component.
 */

import type { ReactNode } from 'react';
import * as sh from './SortHeader.css';

/**
 * The single shared `aria-sort` mapping for sortable column headers: the
 * ACTIVE sort column announces its direction; every other column omits the
 * attribute (per ARIA, only one header should carry aria-sort at a time).
 * Feed the result to `TableColumn.ariaSort` (shared Table) or straight onto a
 * hand-rendered `<th aria-sort={…}>`.
 */
export function ariaSortFor(
  active: boolean,
  dir: 'asc' | 'desc',
): 'ascending' | 'descending' | undefined {
  if (!active) return undefined;
  return dir === 'asc' ? 'ascending' : 'descending';
}

export interface SortHeaderProps {
  /** Column label content (string or rich node). */
  label: ReactNode;
  /** Whether this column is the active sort column. */
  active: boolean;
  /** Current sort direction (only meaningful when `active`). */
  dir: 'asc' | 'desc';
  /** Invoked when the header is activated (toggle/sort by this column). */
  onClick: () => void;
  /** Accessible label, e.g. "Sort by Night". */
  ariaLabel: string;
  /** Optional native tooltip (used by columns with abbreviated labels). */
  title?: string;
}

export function SortHeader({
  label,
  active,
  dir,
  onClick,
  ariaLabel,
  title,
}: SortHeaderProps) {
  return (
    <button
      type="button"
      className={active ? `${sh.root} ${sh.active}` : sh.root}
      onClick={onClick}
      aria-label={ariaLabel}
      title={title}
    >
      {label}
      {active && (
        <span className={sh.arrow} aria-hidden="true">
          {dir === 'asc' ? '▲' : '▼'}
        </span>
      )}
    </button>
  );
}
