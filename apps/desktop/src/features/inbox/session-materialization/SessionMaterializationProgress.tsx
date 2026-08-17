// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionMaterializationProgress — accessible progress surface for an
 * in-flight inbox session materialization operation (spec 062, US1).
 *
 * Renders:
 *  - A live region (`aria-live="polite"`) announcing session progress.
 *  - A text-based progress indicator (processed / total sessions).
 *  - A cancel control, enabled when the backend reports `cancelSafe`.
 *  - A cancelling state that disables the cancel control and announces it.
 */

import { m } from '@/lib/i18n';
import { Btn } from '@/ui';
import type { SessionMaterializationProgress as ProgressDto } from './types';
import type { FlowPhase } from './useSessionMaterializationFlow';

export interface SessionMaterializationProgressProps {
  phase: FlowPhase;
  progress: ProgressDto | null;
  onCancel: () => void;
}

export function SessionMaterializationProgress({
  phase,
  progress,
  onCancel,
}: SessionMaterializationProgressProps) {
  const isApplying = phase === 'applying';
  const isCancelling = phase === 'cancelling';
  const isBusy = isApplying || isCancelling;

  const processed = progress?.processedSessionCount ?? 0;
  const total = progress?.totalSessionCount ?? 0;
  const cancelSafe = progress?.cancelSafe ?? false;

  const isApproving = phase === 'approving';

  const progressText =
    total > 0
      ? m.session_mat_progress_label({
          processed: String(processed),
          total: String(total),
        })
      : isCancelling
        ? m.session_mat_cancelling_label()
        : isApproving
          ? m.session_mat_approving_label()
          : m.session_mat_applying_label();

  return (
    <div
      className="pv-session-mat-progress"
      aria-busy={isBusy}
      data-testid="session-mat-progress"
    >
      {/* Live region — screen readers announce progress updates. */}
      <div
        aria-live="polite"
        aria-atomic="true"
        className="pv-session-mat-progress__live"
        data-testid="session-mat-progress-live"
      >
        {progressText}
      </div>

      <div className="pv-session-mat-progress__bar-row">
        <progress
          className="pv-session-mat-progress__bar"
          value={total > 0 ? processed : undefined}
          max={total > 0 ? total : undefined}
          aria-label={progressText}
          aria-valuemin={0}
          aria-valuemax={total > 0 ? total : undefined}
          aria-valuenow={total > 0 ? processed : undefined}
          data-testid="session-mat-progress-bar"
        />
        <span className="pv-session-mat-progress__label" aria-hidden="true">
          {progressText}
        </span>
      </div>

      {isApplying && (
        <div className="pv-session-mat-progress__cancel-row">
          <Btn
            variant="ghost"
            size="sm"
            disabled={!cancelSafe || isCancelling}
            onClick={onCancel}
            aria-label={m.session_mat_cancel_btn_aria()}
            data-testid="session-mat-cancel-btn"
          >
            {isCancelling
              ? m.session_mat_cancelling_label()
              : m.session_mat_cancel_btn()}
          </Btn>
        </div>
      )}
    </div>
  );
}
