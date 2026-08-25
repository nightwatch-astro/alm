// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useInboxPlanApplyFlow — plan approval, apply (single/batch/all), cancel,
 * and live-progress wiring for the inbox page.
 */

import { useCallback, useState } from 'react';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { usePlanApplyProgress } from '@/features/plans/usePlanApplyProgress';
import { m } from '@/lib/i18n';
import { addToast } from '@/shared/toast';
import {
  useApplySelectedInboxPlans,
  useInboxPlanApplyAll,
  useInboxPlanCancel,
} from './store';
import type { InboxOpenPlan } from './store';

/**
 * Record the user's approval for every plan in a batch gesture before any of
 * them is applied (Constitution II, `astro-plan-tykek`).
 *
 * Individual failures are swallowed: a plan already in `approved` state rejects
 * a second `plans.approve` yet still carries a usable approval, and the backend
 * refuses any plan that ends up without one, reporting it per plan.
 */
async function approvePlans(planIds: string[]): Promise<void> {
  await Promise.allSettled(planIds.map((id) => commands.plansApprove(id)));
}

export interface PlanApplyFlowResult {
  handleApplyOne: (planId: string) => Promise<void>;
  handleApplyAll: () => Promise<void>;
  handleApplySelected: (inboxItemIds: string[]) => Promise<void>;
  handleCancel: (inboxItemId: string) => Promise<void>;
  applyProgress: ReturnType<typeof usePlanApplyProgress>['progress'];
  progressPlanId: string | null;
  /** True when any plan mutation is in-flight. */
  planBusy: boolean;
}

export function useInboxPlanApplyFlow(
  refreshAll: () => void,
  viewResultAction: () => { label: string; onClick: () => void },
  openPlans: InboxOpenPlan[],
): PlanApplyFlowResult {
  const { applyAll, loading: applyAllLoading } = useInboxPlanApplyAll();
  const { applySelected, loading: applySelectedLoading } =
    useApplySelectedInboxPlans();
  const { cancel, loading: cancelLoading } = useInboxPlanCancel();
  const { progress: applyProgress, run: runPlanApply } = usePlanApplyProgress();
  const [progressPlanId, setProgressPlanId] = useState<string | null>(null);

  const handleApplyOne = useCallback(
    async (planId: string) => {
      setProgressPlanId(planId);
      let approvalToken: string | undefined;
      try {
        approvalToken = unwrap(
          await commands.plansApprove(planId),
        ).approvalToken;
      } catch {
        setProgressPlanId(null);
        addToast({
          message: m.inbox_plan_apply_failed_toast(),
          variant: 'error',
        });
        return;
      }
      const response = await runPlanApply({ id: planId, approvalToken });
      // GF-30: Clear the pre-flight busy guard once runPlanApply returns.
      setProgressPlanId(null);
      if (response) {
        addToast({
          message: m.inbox_plan_applied_toast(),
          variant: 'info',
          action: viewResultAction(),
        });
        refreshAll();
      } else {
        addToast({
          message: m.inbox_plan_apply_failed_toast(),
          variant: 'error',
        });
      }
    },
    [runPlanApply, refreshAll, viewResultAction],
  );

  const handleApplySelected = useCallback(
    async (inboxItemIds: string[]) => {
      if (inboxItemIds.length === 0) return;
      const selectedSet = new Set(inboxItemIds);
      await approvePlans(
        openPlans
          .filter((p) => selectedSet.has(p.inboxItemId))
          .map((p) => p.planId),
      );
      const result = await applySelected(inboxItemIds);
      if (result) {
        const failed = result.results.filter((r) => r.error != null).length;
        if (failed > 0) {
          addToast({
            message: m.inbox_toast_plans_partial({
              applied: String(result.results.length - failed),
              failed: String(failed),
            }),
            variant: 'warn',
          });
        } else {
          addToast({
            message: m.inbox_toast_plans_applying({
              count: String(result.results.length),
            }),
            variant: 'info',
            action: viewResultAction(),
          });
        }
        refreshAll();
      } else {
        addToast({ message: m.inbox_toast_apply_failed(), variant: 'error' });
      }
    },
    [applySelected, openPlans, refreshAll, viewResultAction],
  );

  const handleApplyAll = useCallback(async () => {
    // Only the plans the user was shown get an approval; anything that appeared
    // since this render is refused by the backend rather than applied unseen.
    await approvePlans(openPlans.map((p) => p.planId));
    const result = await applyAll();
    if (result) {
      const failed = result.results.filter((r) => r.error != null).length;
      if (failed > 0) {
        addToast({
          message: m.inbox_toast_all_plans_partial({
            applied: String(result.results.length - failed),
            failed: String(failed),
          }),
          variant: 'warn',
        });
      } else {
        addToast({
          message: m.inbox_toast_all_plans_applying({
            count: String(result.results.length),
          }),
          variant: 'info',
          action: viewResultAction(),
        });
      }
      refreshAll();
    }
  }, [applyAll, openPlans, refreshAll, viewResultAction]);

  const handleCancel = useCallback(
    async (inboxItemId: string) => {
      await cancel(inboxItemId);
      addToast({ message: m.inbox_toast_plan_discarded(), variant: 'info' });
      refreshAll();
    },
    [cancel, refreshAll],
  );

  // GF-30: Include progressPlanId in busy derivation.
  const planBusy =
    applyAllLoading ||
    applySelectedLoading ||
    cancelLoading ||
    progressPlanId != null;

  return {
    handleApplyOne,
    handleApplyAll,
    handleApplySelected,
    handleCancel,
    applyProgress,
    progressPlanId,
    planBusy,
  };
}
