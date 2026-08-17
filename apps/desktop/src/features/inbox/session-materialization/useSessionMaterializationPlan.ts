// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * React Query hooks for inbox session materialization plan data.
 *
 * Provides the plan header, proposed session list, and per-session
 * site-resolution status needed to render the review surface.
 */

import { useQuery } from '@tanstack/react-query';
import {
  queryMaterializationPlan,
  listProposedSessions,
  queryAcquisitionSiteResolution,
} from './sessionMaterializationIpc';
import type { InboxProposedSession } from './types';

// ── Query key factory (local — not in shared queryKeys.ts until bindings land)

export const materializationQueryKeys = {
  plan: (planId: string) =>
    ['inbox', 'materialization', 'plan', planId] as const,
  proposedSessions: (planId: string, snapshotId: string) =>
    [
      'inbox',
      'materialization',
      'proposedSessions',
      planId,
      snapshotId,
    ] as const,
  siteResolution: (planId: string, resolutionId: string) =>
    [
      'inbox',
      'materialization',
      'siteResolution',
      planId,
      resolutionId,
    ] as const,
};

// ── Plan header query ─────────────────────────────────────────────────────────

export function useInboxMaterializationPlan(planId: string | null) {
  return useQuery({
    queryKey: materializationQueryKeys.plan(planId ?? ''),
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    queryFn: () => queryMaterializationPlan({ planId: planId! }),
    enabled: planId != null,
    staleTime: 5_000,
  });
}

// ── Proposed sessions (page 0 — the UI fetches all on first mount) ────────────

/** All proposed sessions for one plan snapshot. Fetches page 0 only for now;
 *  a paginated surface can be added when session counts justify it (FR spec
 *  note: bounded result snapshots, no capped list in response). */
export function useProposedSessions(
  planId: string | null,
  planResultSnapshotId: string | null,
) {
  return useQuery({
    queryKey: materializationQueryKeys.proposedSessions(
      planId ?? '',
      planResultSnapshotId ?? '',
    ),
    queryFn: () =>
      listProposedSessions({
        planId: planId!,
        planResultSnapshotId: planResultSnapshotId!,
        page: 0,
      }),
    enabled: planId != null && planResultSnapshotId != null,
    staleTime: 10_000,
  });
}

// ── Site resolution for one proposed session ──────────────────────────────────

export function useSiteResolution(
  planId: string | null,
  session: InboxProposedSession | null,
) {
  return useQuery({
    queryKey: materializationQueryKeys.siteResolution(
      planId ?? '',
      session?.acquisitionSiteResolutionId ?? '',
    ),
    queryFn: () =>
      queryAcquisitionSiteResolution({
        planId: planId!,
        resolutionId: session!.acquisitionSiteResolutionId,
        resolutionRevision: session!.acquisitionSiteResolutionRevision,
      }),
    enabled: planId != null && session != null,
    staleTime: 5_000,
  });
}
