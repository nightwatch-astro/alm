// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TableStateGate — shared three-state loading/error/empty gate for list pages.
 *
 * Renders a Skeleton while loading (with no data yet), an EmptyState on error,
 * an EmptyState when the list is empty, or the `children` when there is data.
 * Extracts a pattern that was hand-rolled in MastersTable, ProjectsTable,
 * SessionsTable, TargetsTable, and ArchivePage (C-28).
 *
 * The "filtered empty" case (list has records, but the current filter matches
 * none) is a separate variant that callers can opt into via `filteredEmpty`.
 * When both `empty` and `filteredEmpty` nodes are given, `filteredEmpty` is
 * shown only when `isEmpty` is true after loading.
 */

import type { ReactNode } from 'react';
import { Skeleton, EmptyState } from '@/ui';

export interface TableStateGateProps {
  /** Whether the initial data load is in progress. */
  loading: boolean;
  /** Error from the data load. Pass a string or null/undefined. */
  error?: string | null;
  /** Whether the (loaded) result list is empty. */
  isEmpty: boolean;
  /** Number of skeleton rows to render while loading. DEFAULT 6. */
  skeletonCount?: number;
  /** Accessible label for the skeleton loader. */
  skeletonLabel?: string;
  /** EmptyState to render when the loaded list is empty (no filter applied). */
  empty: ReactNode;
  /**
   * Alternative EmptyState to render when the list is empty due to a filter.
   * When omitted, `empty` is shown for both cases.
   */
  filteredEmpty?: ReactNode;
  /** Whether the current empty state is a filter-miss (uses filteredEmpty). */
  isFilteredEmpty?: boolean;
  /** EmptyState to render when there is a load error. Required when error is possible. */
  errorEmpty?: ReactNode;
  /**
   * Extra wrapper class applied around the content area (Skeleton/EmptyState).
   * Matches the per-feature wrapper element pattern (e.g. `pv-calib-table__status`).
   */
  wrapperClassName?: string;
  /** data-testid for the wrapper element (set only when wrapperClassName is used). */
  'data-testid'?: string;
  /** The table content to render when loading is done and data is present. */
  children: ReactNode;
}

/**
 * Renders one of four states:
 * 1. loading (and no data yet) → Skeleton
 * 2. error → errorEmpty or a generic EmptyState
 * 3. isEmpty (filtered miss) → filteredEmpty (if provided) or empty
 * 4. isEmpty (truly empty) → empty
 * 5. has data → children
 */
export function TableStateGate({
  loading,
  error,
  isEmpty,
  skeletonCount = 6,
  skeletonLabel,
  empty,
  filteredEmpty,
  isFilteredEmpty = false,
  errorEmpty,
  wrapperClassName,
  'data-testid': testId,
  children,
}: TableStateGateProps) {
  let content: ReactNode;

  if (loading && isEmpty) {
    content = (
      <Skeleton variant="block" count={skeletonCount} label={skeletonLabel} />
    );
  } else if (error) {
    content = errorEmpty ?? <EmptyState title={error} />;
  } else if (isEmpty) {
    content = isFilteredEmpty && filteredEmpty ? filteredEmpty : empty;
  } else {
    content = children;
  }

  if (wrapperClassName) {
    return (
      <div className={wrapperClassName} data-testid={testId}>
        {content}
      </div>
    );
  }

  return <>{content}</>;
}
