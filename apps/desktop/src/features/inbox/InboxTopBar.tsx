// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Inbox top bar: NO page title and NO summary (top-bar convention matches other
 * pages). Search + group/sort/filter live in `FilterToolbar`; the right side
 * carries only page-level actions — review plans, bulk confirm, rescan. The
 * per-detection "Confirm to inventory" lives in the detail header (Sessions
 * convention).
 */

import { FilterToolbar, PageTopBar } from '@/components';
import { m } from '@/lib/i18n';
import type { FrameType } from '@/lib/route-contract';
import type { UseGroupingResult } from '@/lib/use-grouping';
import { Btn } from '@/ui';
import { GROUPING_DIMENSIONS } from './InboxControls';

export interface InboxTopBarProps {
  search: string;
  onSearchChange: (value: string) => void;
  /** URL-backed file-type lane filter (`?type=`). */
  fileType: FrameType | undefined;
  onFileTypeChange: (value: FrameType | undefined) => void;
  kindFilter: string;
  onKindFilterChange: (value: string) => void;
  dims: UseGroupingResult['dims'];
  setSlot: UseGroupingResult['setSlot'];
  /** Number of open plans; the trigger label shows it when non-zero. */
  planCount: number;
  /**
   * Shown when ≥1 open plan exists OR a destination-root pick is pending — the
   * latter can occur with zero open plans, when the plan was not generated yet.
   */
  showPlans: boolean;
  onOpenPlans: () => void;
  bulkEligibleCount: number;
  canBulkConfirm: boolean;
  bulkConfirmLoading: boolean;
  onBulkConfirm: () => void;
  rescanLoading: boolean;
  onRescan: () => void;
}

export function InboxTopBar({
  search,
  onSearchChange,
  fileType,
  onFileTypeChange,
  kindFilter,
  onKindFilterChange,
  dims,
  setSlot,
  planCount,
  showPlans,
  onOpenPlans,
  bulkEligibleCount,
  canBulkConfirm,
  bulkConfirmLoading,
  onBulkConfirm,
  rescanLoading,
  onRescan,
}: InboxTopBarProps) {
  return (
    <PageTopBar
      filters={
        <FilterToolbar
          search={{
            value: search,
            onChange: onSearchChange,
            placeholder: m.inbox_search_placeholder(),
            ariaLabel: m.inbox_search_aria_label(),
          }}
          fields={[
            {
              key: 'fileType',
              label: m.inbox_filter_file_type_label(),
              value: fileType ?? '',
              options: [
                { value: 'fits', label: m.inbox_filter_fits() },
                { value: 'video', label: m.inbox_filter_video() },
              ],
              allLabel: m.inbox_filter_all_file_types(),
              onChange: (v) =>
                onFileTypeChange((v || undefined) as FrameType | undefined),
            },
            {
              key: 'kind',
              label: m.inbox_filter_kind_label(),
              value: kindFilter,
              options: [
                { value: 'bias', label: m.inbox_kind_bias() },
                { value: 'dark', label: m.inbox_kind_dark() },
                { value: 'flat', label: m.inbox_kind_flat() },
                { value: 'light', label: m.inbox_kind_light() },
                { value: 'master', label: m.inbox_kind_master() },
              ],
              allLabel: m.inbox_filter_kind_all(),
              onChange: onKindFilterChange,
            },
          ]}
          grouping={{
            dimensions: GROUPING_DIMENSIONS.map((d) => ({
              value: d.id,
              label: d.label(),
            })),
            dims,
            setSlot,
          }}
        />
      }
      actions={
        <>
          {showPlans && (
            <Btn
              size="sm"
              variant="ghost"
              onClick={onOpenPlans}
              aria-label={m.inbox_review_plans_with_count({ count: planCount })}
              data-testid="inbox-review-plans-btn"
            >
              {planCount > 0
                ? m.inbox_review_plans_with_count({ count: planCount })
                : m.inbox_review_plans()}
            </Btn>
          )}
          {/* task 35: bulk-confirm all cleanly-classified items in one action */}
          {bulkEligibleCount > 0 && (
            <Btn
              size="sm"
              variant="primary"
              disabled={!canBulkConfirm}
              onClick={onBulkConfirm}
              aria-label={m.inbox_confirm_all_classified_aria({
                count: bulkEligibleCount,
              })}
              data-testid="inbox-bulk-confirm-btn"
              // Onboarding find-it spotlight anchor (spec 056 FR-026). The
              // canonical `inbox.confirm-row` anchor lives on this page-level
              // bulk-confirm action so the spotlight resolves it.
              data-guide-anchor="inbox.confirm-row"
            >
              {bulkConfirmLoading
                ? m.common_confirming()
                : m.inbox_confirm_all({ count: bulkEligibleCount })}
            </Btn>
          )}
          <Btn
            size="sm"
            disabled={rescanLoading}
            onClick={onRescan}
            aria-label={m.inbox_rescan_all_roots_aria()}
          >
            {rescanLoading ? m.common_rescanning() : m.common_rescan()}
          </Btn>
        </>
      }
    />
  );
}
