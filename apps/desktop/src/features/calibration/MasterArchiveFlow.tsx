// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * MasterArchiveFlow — in-use confirm modal + plan review overlay for the
 * single-master archive action (#886).
 *
 * Extracted from MasterDetail.tsx (refactor sweep kyo7.104). The two modals
 * are co-located here because they share the same flow: the first (confirm)
 * gates the second (review/apply), and neither makes sense without the other.
 */

import { ConfirmModal } from '@/components';
import { m } from '@/lib/i18n';
import { PlanReviewOverlay } from '@/features/plans/PlanReviewOverlay';

interface MasterArchiveFlowProps {
  inUseConfirmOpen: boolean;
  onCloseConfirm: () => void;
  onConfirmArchiveInUse: () => void;
  archivePending: boolean;
  archiveReviewPlanId: string | null;
  onCloseReview: () => void;
  onArchivePlanApplied: () => void;
  onRetryCreated: (planId: string) => void;
}

/**
 * Renders the in-use confirm gate and the plan review overlay for the
 * single-master archive flow. Mount alongside any host that drives the
 * archive action (currently MasterDetail).
 */
export function MasterArchiveFlow({
  inUseConfirmOpen,
  onCloseConfirm,
  onConfirmArchiveInUse,
  archivePending,
  archiveReviewPlanId,
  onCloseReview,
  onArchivePlanApplied,
  onRetryCreated,
}: MasterArchiveFlowProps) {
  return (
    <>
      {/* #886: in-use warn + confirm gate before archiving (decisions.md). */}
      <ConfirmModal
        open={inUseConfirmOpen}
        onClose={onCloseConfirm}
        title={m.calibration_archive_in_use_confirm_title()}
        message={m.calibration_archive_in_use_confirm_desc()}
        actionLabel={m.calibration_action_archive()}
        busy={archivePending}
        onConfirm={onConfirmArchiveInUse}
        // This dialog had a close button before it adopted ConfirmModal.
        hideClose={false}
        data-testid="master-archive-in-use-confirm"
      />

      {/* Archive plan review overlay (#886): shares the same review → approve
          → apply kit every other plan-gated flow uses. */}
      <PlanReviewOverlay
        planId={archiveReviewPlanId}
        open={archiveReviewPlanId !== null}
        onClose={onCloseReview}
        title={m.archive_generate_review_title()}
        onApplied={onArchivePlanApplied}
        onRetryCreated={onRetryCreated}
      />
    </>
  );
}
