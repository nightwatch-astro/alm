// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * HandoffProgress — spec 062 US4 (calibration handoff operation progress).
 *
 * Renders the verification progress for a CalibrationHandoffOperation:
 *   - Progress bar (verifiedFrameCount / totalFrameCount)
 *   - State label (verifying / cancelling / cancelled / applied / failed)
 *   - Cancel button (only when cancelSafe is true and state is verifying)
 *   - Failure display with failure code mapped to a user-facing message
 *
 * The cancel button is gated on `cancelSafe` — the contract specifies that
 * cancellation is safe only when the operation has not yet entered its final
 * bounded commit window.
 *
 * Progress uses `aria-valuenow` / `aria-valuemax` / `aria-valuetext` for
 * screen reader compatibility.
 */

import { Btn, Banner } from '@/ui';
import { m } from '@/lib/i18n';
import type {
  CalibrationHandoffOperation,
  HandoffFailureCode,
} from './calibrationHandoffTypes';

// ── Failure code mapping ───────────────────────────────────────────────────────

function failureLabel(code: HandoffFailureCode): string {
  switch (code) {
    case 'calibration.source_unavailable':
      return m.calibration_handoff_fail_unavailable();
    case 'calibration.source_identity_changed':
      return m.calibration_handoff_fail_identity_changed();
    case 'calibration.handoff_too_large':
      return m.calibration_handoff_fail_too_large();
    case 'calibration.cancel_deadline_exceeded':
      return m.calibration_handoff_fail_cancel_deadline();
    case 'calibration.verification_failed':
      return m.calibration_handoff_fail_verification_failed();
  }
}

function stateLabel(op: CalibrationHandoffOperation): string {
  switch (op.state) {
    case 'verifying':
      return m.calibration_handoff_state_verifying();
    case 'cancelling':
      return m.calibration_handoff_state_cancelling();
    case 'cancelled':
      return m.calibration_handoff_state_cancelled();
    case 'applied':
      return m.calibration_handoff_state_applied();
    case 'failed':
      return m.calibration_handoff_state_failed();
  }
}

// ── Progress bar ──────────────────────────────────────────────────────────────

function ProgressBar({ verified, total }: { verified: number; total: number }) {
  const pct = total > 0 ? Math.round((verified / total) * 100) : 0;
  const progressText = m.calibration_handoff_progress_label({
    verified: String(verified),
    total: String(total),
  });

  return (
    <div
      role="progressbar"
      aria-valuenow={verified}
      aria-valuemax={total}
      aria-valuetext={progressText}
      className="pv-handoff-progress__bar-track"
      data-testid="handoff-progress-bar"
    >
      <div
        className="pv-handoff-progress__bar-fill"
        // eslint-disable-next-line no-restricted-syntax -- dynamic: progress bar width %
        style={{ width: `${pct}%` }}
        data-testid="handoff-progress-fill"
      />
      <span className="pv-handoff-progress__bar-label">{progressText}</span>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export interface HandoffProgressProps {
  operation: CalibrationHandoffOperation;
  onCancel: (operationId: string) => void;
  cancelling?: boolean;
}

export function HandoffProgress({
  operation,
  onCancel,
  cancelling = false,
}: HandoffProgressProps) {
  const isActive =
    operation.state === 'verifying' || operation.state === 'cancelling';
  const canCancel =
    operation.state === 'verifying' && operation.cancelSafe && !cancelling;

  return (
    <div
      className="pv-handoff-progress"
      data-testid={`handoff-operation-${operation.operationId}`}
    >
      <div className="pv-handoff-progress__header">
        <span
          className="pv-handoff-progress__state"
          data-testid="handoff-state-label"
          aria-live="polite"
        >
          {stateLabel(operation)}
        </span>

        {canCancel && (
          <Btn
            size="sm"
            variant="ghost"
            onClick={() => onCancel(operation.operationId)}
            data-testid="handoff-cancel-btn"
          >
            {m.calibration_handoff_cancel_btn()}
          </Btn>
        )}
      </div>

      {isActive && (
        <ProgressBar
          verified={operation.verifiedFrameCount}
          total={operation.totalFrameCount}
        />
      )}

      {operation.state === 'failed' && operation.failureCode && (
        <Banner variant="danger" data-testid="handoff-failure-banner">
          {failureLabel(operation.failureCode)}
        </Banner>
      )}
    </div>
  );
}
