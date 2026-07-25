// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * RecoveryBanner — unclean-shutdown recovery prompt (astro-plan-kyo7.48).
 *
 * On mount it asks the backend whether the previous process exited cleanly and
 * which plans were left mid-apply. When the shutdown was unclean AND at least
 * one plan is interrupted, it renders a non-blocking banner offering to review
 * and resume. "Review & resume" opens the existing PlanReviewOverlay for the
 * first interrupted plan (the resume/repair machinery already lives there);
 * "Dismiss" hides the banner for the session. Read-only detection — nothing is
 * mutated or resumed without the user's explicit action in the overlay.
 */

import { useEffect, useState } from 'react';
import { Banner, Btn } from '@/ui';
import { m } from '@/lib/i18n';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { PlanReviewOverlay } from '@/features/plans/PlanReviewOverlay';

export function RecoveryBanner() {
  const [interruptedPlanIds, setInterruptedPlanIds] = useState<string[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [reviewPlanId, setReviewPlanId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const status = unwrap(await commands.recoveryStatus());
        if (!cancelled && status.uncleanShutdown) {
          setInterruptedPlanIds(status.interruptedPlanIds);
        }
      } catch {
        // Detection is best-effort; a failed probe simply shows no banner.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const show = !dismissed && interruptedPlanIds.length > 0;

  return (
    <>
      {show && (
        <Banner
          variant="warn"
          className="pv-recovery-banner"
          data-testid="recovery-banner"
        >
          <span>
            {m.recovery_banner_message({ count: interruptedPlanIds.length })}
          </span>
          <span className="pv-recovery-banner__actions">
            <Btn onClick={() => setReviewPlanId(interruptedPlanIds[0] ?? null)}>
              {m.recovery_banner_review()}
            </Btn>
            <Btn variant="ghost" onClick={() => setDismissed(true)}>
              {m.recovery_banner_dismiss()}
            </Btn>
          </span>
        </Banner>
      )}
      <PlanReviewOverlay
        planId={reviewPlanId}
        open={reviewPlanId !== null}
        onClose={() => setReviewPlanId(null)}
        onApplied={() => {
          // Drop the resumed plan from the pending set; hide the banner when
          // none remain.
          setInterruptedPlanIds((ids) =>
            ids.filter((id) => id !== reviewPlanId),
          );
          setReviewPlanId(null);
        }}
      />
    </>
  );
}
