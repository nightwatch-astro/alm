// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TanStack Query hooks for the spec-062 sessions/groups/proposals surface.
 *
 * Each hook wraps the corresponding IPC adapter from sessionsGroupsIpc.ts.
 * Mutation hooks return the raw mutation object so callers can inspect status,
 * trigger calls with `.mutateAsync`, and handle errors at the call site.
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import type { UseQueryResult, UseMutationResult } from '@tanstack/react-query';
import { queryKeys } from '@/data/queryKeys';
import {
  panelGroupQuery,
  panelGroupList,
  mosaicQuery,
  mosaicEdgeList,
  mosaicObjectEvidenceList,
  relationProposalList,
  relationProposalQuery,
  relationProposalManualCreate,
  relationProposalAccept,
  relationProposalReject,
  matchingSettingsGet,
  matchingSettingsValidate,
  matchingSettingsUpdate,
} from './sessionsGroupsIpc';
import type {
  PanelGroupRevision,
  MosaicRevision,
  MosaicEdge,
  MosaicObjectEvidenceItem,
  RelationProposal,
  ProposalState,
  ProposalKind,
  MatchingSettings,
  SettingsValidation,
  Page,
} from './groupsTypes';

// ── Panel group hooks ──────────────────────────────────────────────────────────

export function usePanelGroup(
  panelGroupId: string | undefined,
): UseQueryResult<{
  acceptedHead: PanelGroupRevision;
  requestedRevision?: PanelGroupRevision;
}> {
  return useQuery({
    queryKey: queryKeys.sessions.panelGroup(panelGroupId ?? ''),
    queryFn: () => panelGroupQuery({ panelGroupId: panelGroupId! }),
    enabled: panelGroupId != null && panelGroupId !== '',
  });
}

export function usePanelGroupList(filters?: {
  targetId?: string;
  sessionId?: string;
  activeOnly?: boolean;
}): UseQueryResult<Page<PanelGroupRevision>> {
  return useQuery({
    queryKey: queryKeys.sessions.panelGroupList(filters),
    queryFn: () => panelGroupList(filters ?? {}),
  });
}

// ── Mosaic hooks ───────────────────────────────────────────────────────────────

export function useMosaic(mosaicId: string | undefined): UseQueryResult<{
  acceptedHead: MosaicRevision;
  requestedRevision?: MosaicRevision;
}> {
  return useQuery({
    queryKey: queryKeys.sessions.mosaic(mosaicId ?? ''),
    queryFn: () => mosaicQuery({ mosaicId: mosaicId! }),
    enabled: mosaicId != null && mosaicId !== '',
  });
}

export function useMosaicEdges(
  mosaicId: string | undefined,
  revisionId: string | undefined,
): UseQueryResult<Page<{ edge: MosaicEdge; ordinal: number }>> {
  return useQuery({
    queryKey: [
      ...queryKeys.sessions.mosaic(mosaicId ?? ''),
      'edges',
      revisionId,
    ],
    queryFn: () =>
      mosaicEdgeList({ mosaicId: mosaicId!, revisionId: revisionId! }),
    enabled: mosaicId != null && revisionId != null,
  });
}

export function useMosaicObjectEvidence(
  mosaicId: string | undefined,
  revisionId: string | undefined,
): UseQueryResult<Page<MosaicObjectEvidenceItem>> {
  return useQuery({
    queryKey: [
      ...queryKeys.sessions.mosaic(mosaicId ?? ''),
      'objects',
      revisionId,
    ],
    queryFn: () =>
      mosaicObjectEvidenceList({
        mosaicId: mosaicId!,
        revisionId: revisionId!,
      }),
    enabled: mosaicId != null && revisionId != null,
  });
}

// ── Relation proposal hooks ────────────────────────────────────────────────────

export function useRelationProposals(filters?: {
  state?: ProposalState;
  kind?: ProposalKind;
  targetId?: string;
}): UseQueryResult<Page<RelationProposal>> {
  return useQuery({
    queryKey: queryKeys.sessions.proposals(filters),
    queryFn: () => relationProposalList(filters ?? {}),
  });
}

export function useRelationProposal(
  proposalId: string | undefined,
): UseQueryResult<RelationProposal> {
  return useQuery({
    queryKey: queryKeys.sessions.proposal(proposalId ?? ''),
    queryFn: () => relationProposalQuery({ proposalId: proposalId! }),
    enabled: proposalId != null && proposalId !== '',
  });
}

export function useRelationProposalManualCreate(): UseMutationResult<
  { proposal: RelationProposal; auditId: string },
  unknown,
  Parameters<typeof relationProposalManualCreate>[0]
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: relationProposalManualCreate,
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.sessions.proposals(),
      });
    },
  });
}

export function useRelationProposalAccept(): UseMutationResult<
  {
    proposal: RelationProposal;
    decisionSnapshotId: string;
    crossTargetAssociationId?: string;
    auditId: string;
  },
  unknown,
  Parameters<typeof relationProposalAccept>[0]
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: relationProposalAccept,
    onSuccess: (_data, vars) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.sessions.proposals(),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.sessions.proposal(vars.proposalId),
      });
      // Panel group and mosaic heads may have changed
      void queryClient.invalidateQueries({
        queryKey: queryKeys.sessions.panelGroupList(),
      });
    },
  });
}

export function useRelationProposalReject(): UseMutationResult<
  {
    proposal: RelationProposal;
    suppressionFingerprint: string;
    auditId: string;
  },
  unknown,
  Parameters<typeof relationProposalReject>[0]
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: relationProposalReject,
    onSuccess: (_data, vars) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.sessions.proposals(),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.sessions.proposal(vars.proposalId),
      });
    },
  });
}

// ── Matching settings hooks ────────────────────────────────────────────────────

export function useMatchingSettings(
  revision?: number,
): UseQueryResult<MatchingSettings> {
  return useQuery({
    queryKey: queryKeys.sessions.matchingSettings(revision),
    queryFn: () =>
      matchingSettingsGet(revision != null ? { revision } : undefined),
  });
}

/**
 * Validate a settings patch. Returns a stale result while re-validating; the
 * caller can use `isFetching` to show a spinner.
 */
export function useMatchingSettingsValidate(
  baseRevision: number | undefined,
  patch: Partial<MatchingSettings> | undefined,
): UseQueryResult<SettingsValidation> {
  return useQuery({
    queryKey: ['sessions', 'matchingSettings', 'validate', baseRevision, patch],
    queryFn: () =>
      matchingSettingsValidate({ baseRevision: baseRevision!, patch: patch! }),
    enabled: baseRevision != null && patch != null,
  });
}

export function useMatchingSettingsUpdate(): UseMutationResult<
  {
    settings: MatchingSettings;
    warnings: SettingsValidation['issues'];
    auditId: string;
  },
  unknown,
  Parameters<typeof matchingSettingsUpdate>[0]
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: matchingSettingsUpdate,
    onSuccess: (data) => {
      // Populate the cache for the new revision
      void queryClient.setQueryData(
        queryKeys.sessions.matchingSettings(data.settings.revision),
        data.settings,
      );
      // Invalidate the "current" (no revision) entry
      void queryClient.invalidateQueries({
        queryKey: queryKeys.sessions.matchingSettings(),
      });
    },
  });
}

// ── Re-exports so callers only need this file ──────────────────────────────────

export type {
  PanelGroupRevision,
  MosaicRevision,
  MosaicEdge,
  MosaicObjectEvidenceItem,
  RelationProposal,
  ProposalState,
  ProposalKind,
  MatchingSettings,
  SettingsValidation,
  SettingsIssue,
  Page,
  ManualRelationReview,
  RelationEvidence,
  ThresholdMeasurement,
} from './groupsTypes';

export {
  MATCHING_SETTINGS_BOUNDS,
  CALIBRATION_AGE_DEFAULTS,
} from './groupsTypes';

// ── Validation helpers ─────────────────────────────────────────────────────────

/**
 * Validate a single numeric field against its hard bounds. Returns 'ok',
 * 'yellow', or 'red'.
 *
 * Used by MatchingSettingsPanel to gate save and show inline pill feedback
 * before sending to the server. Server-side validation is authoritative;
 * this is client-side fast-feedback only.
 */
export function validateFieldSeverity(
  value: number,
  bounds: {
    min: number;
    max: number;
    yellowBelow?: number;
    yellowAbove?: number;
  },
): 'ok' | 'yellow' | 'red' {
  if (value < bounds.min || value > bounds.max) return 'red';
  if (bounds.yellowBelow != null && value < bounds.yellowBelow) return 'yellow';
  if (bounds.yellowAbove != null && value > bounds.yellowAbove) return 'yellow';
  return 'ok';
}
