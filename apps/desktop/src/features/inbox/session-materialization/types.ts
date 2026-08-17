// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TypeScript mirrors of the inbox materialization contract DTOs
 * (specs/062-session-heterogeneity/contracts/inbox-materialization.md).
 *
 * Kept local to this feature until tauri-specta generates bindings for the
 * new session materialization commands (ic9h.20 handover).
 */

export interface InboxMaterializationPlan {
  planId: string;
  planRevision: number;
  state: 'open' | 'approved' | 'applied' | 'discarded' | 'stale' | 'refused';
  canonicalPlanDigest: string;
  inputEvidenceRevision: number;
  configurationRevisionId: string;
  acquisitionSiteResolutionCount: number;
  planResultSnapshotId: string;
  candidateFrameCount: number;
  proposedSessionCount: number;
  blockedFrameCount: number;
  warningCodes: string[];
  createdAt: string;
  createdBy: string;
  approvedAt?: string;
  approvedBy?: string;
}

export interface AcquisitionSiteResolution {
  resolutionId: string;
  revision: number;
  state: 'needs_review' | 'resolved' | 'conflict';
  selectedSiteId?: string;
  selectedTimezone?: string;
  decision:
    | 'unresolved'
    | 'accepted_candidate'
    | 'corrected'
    | 'reviewed_local_fallback';
  timestampDecision?: 'canonical_instant_confirmed' | 'reviewed_local_fallback';
  canonicalExposureInstant?: string;
  localExposureTimestamp?: string;
  derivedObservingNight?: string;
  conflictCodes: string[];
  evidenceRefs: string[];
  decidedAt?: string;
  decidedBy?: string;
}

export interface AcquisitionSiteCandidate {
  siteId: string;
  label: string;
  timezone: string;
  confidence: 'exact' | 'review';
  basisCodes: string[];
  evidenceRefs: string[];
  derivedObservingNight: string;
  conflictCodes: string[];
}

export interface InboxProposedSession {
  ordinal: number;
  proposedSessionKey: string;
  frameKind: 'light' | 'dark' | 'bias' | 'flat';
  proposedIdentityDigest: string;
  proposedFrameCount: number;
  acquisitionSiteResolutionId: string;
  acquisitionSiteResolutionRevision: number;
  warningCodes: string[];
}

export interface SessionMaterializationOperation {
  operationId: string;
  kind: 'inbox_ingestion' | 'metadata_reclassification';
  state:
    | 'ready'
    | 'applying'
    | 'cancelling'
    | 'cancelled'
    | 'applied'
    | 'failed';
  sourcePlanId: string;
  approvedPlanDigest: string;
  resultSnapshotId?: string;
  sessionCount: number;
  frameMembershipCount: number;
  singletonPanelGroupCount: number;
  blockedFrameCount: number;
  startedAt?: string;
  finishedAt?: string;
  failureCode?: string;
}

export interface SessionMaterializationProgress {
  operationId: string;
  state:
    | 'ready'
    | 'applying'
    | 'cancelling'
    | 'cancelled'
    | 'applied'
    | 'failed';
  processedSessionCount: number;
  totalSessionCount: number;
  processedFrameCount: number;
  totalFrameCount: number;
  cancelSafe: boolean;
  updatedAt: string;
}

export interface MaterializationResultSession {
  ordinal: number;
  sessionId: string;
  frameKind: 'light' | 'dark' | 'bias' | 'flat';
  frameCount: number;
  singletonPanelGroupId?: string;
  singletonPanelRevisionId?: string;
}

/** Paged response envelope used for all list queries. */
export interface Page<T> {
  items: T[];
  nextPage?: number;
  totalCount?: number;
}

/** Backend ContractError envelope. */
export interface ContractError {
  code: string;
  message: string;
}
