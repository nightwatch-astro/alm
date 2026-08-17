// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionMaterializationPanel — accessible Inbox review surface for
 * proposed session partitions (spec 062, US1).
 *
 * Displays:
 *  - Each proposed session with its identity metadata (frame kind, frame
 *    count, observing night, acquisition site state).
 *  - Per-session warning evidence (missing or contradictory metadata).
 *  - Site resolution state badge — degraded for `needs_review`, blocked for
 *    `conflict`.  Values are never invented (AC4).
 *  - Approve + apply action (blocked when any resolution is `conflict` or
 *    `needs_review`, or when the plan is `stale`/`refused`).
 *  - Discard action.
 *  - Asynchronous progress with cancellation (composed from
 *    SessionMaterializationProgress).
 *  - Immutable result display (composed from SessionMaterializationResult).
 *
 * Accessible: ARIA roles, keyboard navigation, live regions for async progress.
 */

import { useMemo } from 'react';
import { useQueries } from '@tanstack/react-query';
import { m } from '@/lib/i18n';
import { Banner, Btn, Pill, Section, Table } from '@/ui';
import type { TableColumn, TableRow } from '@/ui';
import { SessionMaterializationProgress } from './SessionMaterializationProgress';
import { SessionMaterializationResult } from './SessionMaterializationResult';
import {
  useInboxMaterializationPlan,
  useProposedSessions,
  materializationQueryKeys,
} from './useSessionMaterializationPlan';
import { queryAcquisitionSiteResolution } from './sessionMaterializationIpc';
import { useSessionMaterializationFlow } from './useSessionMaterializationFlow';
import type { InboxProposedSession, AcquisitionSiteResolution } from './types';

// ── Warning codes that indicate missing or contradictory metadata ─────────────

/** Warning codes that degrade a session row (missing identity metadata). */
const MISSING_METADATA_CODES = new Set([
  'missing_date_obs',
  'missing_filter',
  'missing_exposure',
  'missing_binning',
  'missing_gain',
  'missing_readout_mode',
]);

/** Warning codes that make a session row contradictory (blocked). */
const CONTRADICTORY_METADATA_CODES = new Set([
  'contradictory_filter',
  'contradictory_exposure',
  'contradictory_binning',
  'contradictory_readout_mode',
  'observing_night_conflict',
]);

function classifyWarnings(codes: string[]): {
  missing: boolean;
  contradictory: boolean;
} {
  return {
    missing: codes.some((c) => MISSING_METADATA_CODES.has(c)),
    contradictory: codes.some((c) => CONTRADICTORY_METADATA_CODES.has(c)),
  };
}

// ── Site resolution badge ─────────────────────────────────────────────────────

interface SiteBadgeProps {
  resolutionId: string;
  /** Map of resolutionId → resolution loaded by the panel. */
  resolutionMap: Map<string, AcquisitionSiteResolution>;
}

function SiteBadge({ resolutionId, resolutionMap }: SiteBadgeProps) {
  const res = resolutionMap.get(resolutionId);
  if (!res) {
    return (
      <Pill variant="warn" aria-label={m.session_mat_site_unresolved_badge()}>
        {m.inbox_state_needs_review()}
      </Pill>
    );
  }
  if (res.state === 'conflict') {
    return (
      <Pill variant="danger" aria-label={m.session_mat_site_conflict_badge()}>
        {m.session_mat_site_conflict()}
      </Pill>
    );
  }
  if (res.state === 'needs_review') {
    return (
      <Pill variant="warn" aria-label={m.session_mat_site_unresolved_badge()}>
        {m.inbox_state_needs_review()}
      </Pill>
    );
  }
  return (
    <Pill variant="ok" aria-label={m.session_mat_site_resolved()}>
      {m.session_mat_site_resolved()}
    </Pill>
  );
}

// ── Warning badges for a session row ─────────────────────────────────────────

interface WarningBadgesProps {
  codes: string[];
}

function WarningBadges({ codes }: WarningBadgesProps) {
  const { missing, contradictory } = classifyWarnings(codes);
  if (!missing && !contradictory) return null;
  return (
    <span className="pv-session-mat-panel__warnings">
      {missing && (
        <Pill variant="warn">{m.session_mat_warning_missing_metadata()}</Pill>
      )}
      {contradictory && (
        <Pill variant="danger">
          {m.session_mat_warning_contradictory_metadata()}
        </Pill>
      )}
    </span>
  );
}

// ── Main panel ────────────────────────────────────────────────────────────────

export interface SessionMaterializationPanelProps {
  /** The plan to display. When null, renders nothing (not yet loaded). */
  planId: string | null;
  /** Called after a successful apply, so the parent can refresh. */
  onApplied?: () => void;
  /** Called when the user requests to discard this plan. */
  onDiscard?: (planId: string) => void;
}

export function SessionMaterializationPanel({
  planId,
  onApplied,
  onDiscard,
}: SessionMaterializationPanelProps) {
  const { data: plan, isLoading: planLoading } =
    useInboxMaterializationPlan(planId);

  const { data: sessionsPage, isLoading: sessionsLoading } =
    useProposedSessions(planId, plan?.planResultSnapshotId ?? null);

  const {
    state: flowState,
    handleApprove,
    handleCancel,
  } = useSessionMaterializationFlow(planId, onApplied);

  const sessions = sessionsPage?.items ?? [];

  // Load site resolutions for each proposed session in parallel.
  // Each unique resolutionId is loaded once; results are indexed by resolutionId.
  const uniqueResolutions = useMemo(() => {
    const seen = new Map<string, { resolutionId: string; revision: number }>();
    for (const s of sessions) {
      if (!seen.has(s.acquisitionSiteResolutionId)) {
        seen.set(s.acquisitionSiteResolutionId, {
          resolutionId: s.acquisitionSiteResolutionId,
          revision: s.acquisitionSiteResolutionRevision,
        });
      }
    }
    return [...seen.values()];
  }, [sessions]);

  const resolutionResults = useQueries({
    queries: uniqueResolutions.map((r) => ({
      queryKey: materializationQueryKeys.siteResolution(
        planId ?? '',
        r.resolutionId,
      ),
      queryFn: () =>
        queryAcquisitionSiteResolution({
          planId: planId!,
          resolutionId: r.resolutionId,
          resolutionRevision: r.revision,
        }),
      enabled: planId != null,
      staleTime: 5_000,
    })),
  });

  const resolutionMap = useMemo(() => {
    const map = new Map<string, AcquisitionSiteResolution>();
    for (const result of resolutionResults) {
      if (result.data) {
        map.set(result.data.resolutionId, result.data);
      }
    }
    return map;
  }, [resolutionResults]);

  // Fail-safe: any in-flight resolution query means we don't yet have full
  // data — treat as blocked until all queries settle.
  const anyResolutionLoading = resolutionResults.some((r) => r.isLoading);

  // Determine whether approval is blocked:
  //  - sessions query still loading → blocked (no data yet)
  //  - any site-resolution query still loading → blocked (no data yet)
  //  - plan is stale or refused → blocked
  //  - any proposed session has contradictory warning codes → blocked
  //  - any site resolution is not resolved (or not yet loaded) → blocked
  const planIsTerminallyBlocked =
    plan?.state === 'stale' || plan?.state === 'refused';

  const hasContradictorySession = sessions.some(
    (s) => classifyWarnings(s.warningCodes).contradictory,
  );

  // If a resolution is absent from the map (not yet loaded) treat it as
  // unresolved — fail-safe, never approve blind.
  const hasUnresolvedSite = sessions.some((s) => {
    const res = resolutionMap.get(s.acquisitionSiteResolutionId);
    if (!res) return true;
    return res.state !== 'resolved';
  });

  const approveBlocked =
    sessionsLoading ||
    anyResolutionLoading ||
    planIsTerminallyBlocked ||
    hasContradictorySession ||
    hasUnresolvedSite;

  const isActive = flowState.phase !== 'idle';
  const showResult =
    flowState.phase === 'applied' ||
    flowState.phase === 'cancelled' ||
    flowState.phase === 'failed';

  const showProgress =
    flowState.phase === 'approving' ||
    flowState.phase === 'applying' ||
    flowState.phase === 'cancelling';

  if (!planId) return null;
  if (planLoading && !plan) {
    return (
      <Section
        title={m.session_mat_review_title()}
        data-testid="session-mat-panel"
      >
        <div aria-busy="true" data-testid="session-mat-loading" />
      </Section>
    );
  }
  if (!plan) return null;

  // Columns for the proposed sessions table.
  const columns: TableColumn[] = [
    { key: 'kind', label: m.session_mat_col_kind() },
    { key: 'frames', label: m.session_mat_col_frames() },
    { key: 'night', label: m.session_mat_col_observing_night() },
    { key: 'site', label: m.session_mat_col_site() },
    { key: 'warnings', label: m.session_mat_col_warnings() },
  ];

  const rows: TableRow[] = sessions.map((s) =>
    buildSessionRow(s, resolutionMap),
  );

  return (
    <Section
      title={m.session_mat_review_title()}
      data-testid="session-mat-panel"
    >
      {/* Stale / refused banners */}
      {plan.state === 'stale' && (
        <Banner variant="warn" data-testid="session-mat-stale-banner">
          {m.session_mat_stale_banner()}
        </Banner>
      )}
      {plan.state === 'refused' && (
        <Banner variant="danger" data-testid="session-mat-refused-banner">
          {m.session_mat_refused_banner()}
        </Banner>
      )}

      {/* Proposed sessions table */}
      {!showResult && (
        <>
          <h3
            className="pv-session-mat-panel__sessions-heading"
            id="session-mat-sessions-label"
          >
            {m.session_mat_proposed_sessions_heading({
              count: sessionsLoading ? '…' : String(sessions.length),
            })}
          </h3>
          <Table
            aria-labelledby="session-mat-sessions-label"
            columns={columns}
            rows={rows}
            data-testid="session-mat-sessions-table"
          />
          {plan.blockedFrameCount > 0 && (
            <p
              className="pv-session-mat-panel__blocked-badge"
              data-testid="session-mat-blocked-frames-badge"
            >
              {m.session_mat_blocked_frames_badge({
                count: String(plan.blockedFrameCount),
              })}
            </p>
          )}
        </>
      )}

      {/* Async progress */}
      {showProgress && (
        <SessionMaterializationProgress
          phase={flowState.phase}
          progress={flowState.progress}
          onCancel={() => void handleCancel()}
        />
      )}

      {/* Terminal result */}
      {showResult && (
        <SessionMaterializationResult
          phase={flowState.phase}
          operation={flowState.operation}
          errorCode={flowState.errorCode}
        />
      )}

      {/* Action row — hidden while an operation is active or complete */}
      {!isActive && !showResult && plan.state === 'open' && (
        <div
          className="pv-session-mat-panel__actions"
          role="group"
          aria-label={m.session_mat_review_title()}
        >
          {approveBlocked && (
            <p
              className="pv-session-mat-panel__blocked-msg"
              role="alert"
              aria-live="polite"
              data-testid="session-mat-approve-blocked-msg"
            >
              {m.session_mat_approve_blocked_unresolved()}
            </p>
          )}
          <Btn
            variant="primary"
            disabled={approveBlocked}
            onClick={() => void handleApprove(plan)}
            aria-label={m.session_mat_approve_btn_aria({
              count: String(plan.proposedSessionCount),
            })}
            data-testid="session-mat-approve-btn"
          >
            {m.session_mat_approve_btn()}
          </Btn>
          {onDiscard && (
            <Btn
              variant="ghost"
              onClick={() => onDiscard(plan.planId)}
              aria-label={m.session_mat_discard_btn_aria()}
              data-testid="session-mat-discard-btn"
            >
              {m.plans_review_discard_btn()}
            </Btn>
          )}
        </div>
      )}
    </Section>
  );
}

// ── Row builder ───────────────────────────────────────────────────────────────

function buildSessionRow(
  session: InboxProposedSession,
  resolutionMap: Map<string, AcquisitionSiteResolution>,
): TableRow {
  return {
    kind: session.frameKind,
    frames: String(session.proposedFrameCount),
    night: session.warningCodes.includes('missing_date_obs')
      ? '—'
      : (resolutionMap.get(session.acquisitionSiteResolutionId)
          ?.derivedObservingNight ?? '…'),
    site: (
      <SiteBadge
        resolutionId={session.acquisitionSiteResolutionId}
        resolutionMap={resolutionMap}
      />
    ),
    warnings: <WarningBadges codes={session.warningCodes} />,
  };
}
