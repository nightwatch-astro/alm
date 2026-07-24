// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * PageTopBar — shared pinned top band for list/detail pages (spec 043, task #62).
 *
 * Renders inside the always-visible page header region (follows the page-layout
 * convention: it carries `.pv-page__bar`, so it NEVER scrolls — only the page
 * body below it scrolls). Four optional slots laid out on a consistent,
 * token-driven row:
 *
 *   title    — heading / breadcrumb node (left, primary)
 *   summary  — context stats node, e.g. counts (left, secondary, muted)
 *   filters  — controls node (FilterToolbar goes here); fills the middle
 *   actions  — right-aligned action buttons node
 *
 * ── Action-button convention (single source of truth) ──────────────────────
 * Buttons placed in the `actions` slot MUST use the shared `Btn` from `@/ui`
 * with `size="sm"`, and they sit RIGHT-ALIGNED at the end of the bar. Variant
 * encodes intent, not color: the single primary CTA uses `variant="primary"`,
 * destructive actions use `variant="danger"`, and everything else is the
 * default (neutral) variant. Keep at most one primary CTA visible. Every page
 * adopting PageTopBar follows this so action affordances read identically
 * across Sessions / Calibration / Projects / Targets / Inbox.
 */

import type { ReactNode } from 'react';
import { pageBar } from '@/ui/page-layout.css';

export interface PageTopBarProps {
  /** Heading / breadcrumb node (primary, left). */
  title?: ReactNode;
  /** Context stats node, e.g. counts (secondary, muted, left). */
  summary?: ReactNode;
  /** Controls node — typically a `FilterToolbar`. Fills the middle of the row. */
  filters?: ReactNode;
  /** Right-aligned action buttons (see action-button convention above). */
  actions?: ReactNode;
}

export function PageTopBar({
  title,
  summary,
  filters,
  actions,
}: PageTopBarProps) {
  return (
    <div className={`${pageBar} pv-topbar`} data-testid="topbar">
      {(title != null || summary != null) && (
        <div className="pv-topbar__lead">
          {title != null && <div className="pv-topbar__title">{title}</div>}
          {summary != null && (
            <div className="pv-topbar__summary">{summary}</div>
          )}
        </div>
      )}
      {filters != null && <div className="pv-topbar__filters">{filters}</div>}
      {actions != null && <div className="pv-topbar__actions">{actions}</div>}
    </div>
  );
}
