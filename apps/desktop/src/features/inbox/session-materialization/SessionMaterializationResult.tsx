// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionMaterializationResult — immutable terminal outcome display
 * (spec 062, US1).
 *
 * Shown after the operation reaches `applied`, `cancelled`, or `failed`.
 * The operation result is immutable — no retry or edit affordances here;
 * callers handle navigation.
 */

import { m } from '@/lib/i18n';
import { Banner } from '@/ui';
import type { SessionMaterializationOperation } from './types';
import type { FlowPhase } from './useSessionMaterializationFlow';

export interface SessionMaterializationResultProps {
  phase: FlowPhase;
  operation: SessionMaterializationOperation | null;
  errorCode: string | null;
}

function localizeErrorCode(code: string | null): string {
  switch (code) {
    case 'inbox.plan_stale':
      return m.session_mat_err_stale();
    case 'inbox.plan_digest_mismatch':
      return m.session_mat_err_digest_mismatch();
    case 'inbox.plan_not_open':
      return m.session_mat_err_not_open();
    case 'inbox.site_selection_required':
    case 'inbox.timestamp_conflict':
      return m.session_mat_err_site_unresolved();
    default:
      return m.session_mat_err_unknown();
  }
}

export function SessionMaterializationResult({
  phase,
  operation,
  errorCode,
}: SessionMaterializationResultProps) {
  if (phase === 'applied') {
    return (
      <Banner variant="info" data-testid="session-mat-result-applied">
        <div>
          <strong>{m.session_mat_applied_title()}</strong>
          <p>
            {m.session_mat_applied_body({
              count: String(operation?.sessionCount ?? 0),
              frames: String(operation?.frameMembershipCount ?? 0),
            })}
          </p>
          {(operation?.blockedFrameCount ?? 0) > 0 && (
            <p data-testid="session-mat-blocked-frames">
              {m.session_mat_blocked_frames_badge({
                count: String(operation!.blockedFrameCount),
              })}
            </p>
          )}
        </div>
      </Banner>
    );
  }

  if (phase === 'cancelled') {
    return (
      <Banner variant="info" data-testid="session-mat-result-cancelled">
        <div>
          <strong>{m.session_mat_cancelled_title()}</strong>
          <p>{m.session_mat_cancelled_body()}</p>
        </div>
      </Banner>
    );
  }

  if (phase === 'failed') {
    return (
      <Banner variant="danger" data-testid="session-mat-result-failed">
        <div>
          <strong>{m.session_mat_failed_title()}</strong>
          <p>{m.session_mat_failed_body()}</p>
          {errorCode && (
            <p
              className="pv-session-mat-result__error-code"
              data-testid="session-mat-error-code"
            >
              {localizeErrorCode(errorCode)}
            </p>
          )}
        </div>
      </Banner>
    );
  }

  return null;
}
