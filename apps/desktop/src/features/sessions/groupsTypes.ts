// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Feature-local TypeScript types for the spec-062 sessions/groups/proposals
 * surface. These mirror the DTOs in
 * `specs/062-session-heterogeneity/contracts/sessions-groups-proposals.md`
 * and `contracts/matching-settings.md`.
 *
 * When node .20 wires the Tauri commands and generates bindings, the types
 * below should be replaced by the generated `@/bindings/index` imports in
 * `sessionsGroupsIpc.ts`. Keep the shapes in sync with the contract until then.
 */

// ── Shared primitives ──────────────────────────────────────────────────────────

export type ValueState<T> =
  | { state: 'known'; value: T }
  | { state: 'absent' }
  | { state: 'unknown' }
  | { state: 'contradictory'; evidenceRefs: string[] };

export interface ObservingNightDerivationAcquisitionTimezone {
  kind: 'acquisition_timezone';
  timezone: string;
  localBoundaryTime: '12:00:00';
}

export interface ObservingNightDerivationLocalFallback {
  kind: 'reviewed_local_fallback';
  localBoundaryTime: '12:00:00';
  reviewEvidenceId: string;
  reviewedAt: string;
  reviewedBy: string;
  reason: string;
}

export type ObservingNightDerivation =
  | ObservingNightDerivationAcquisitionTimezone
  | ObservingNightDerivationLocalFallback;

// ── Session DTOs ───────────────────────────────────────────────────────────────

export interface SessionSummary {
  sessionId: string;
  materializationOperationId: string;
  materializationKind: 'inbox_ingestion' | 'metadata_reclassification';
  frameKind: 'light' | 'dark' | 'bias' | 'flat';
  observingNight: string; // date ISO
  acquisitionTimezone?: string;
  nightDerivation: ObservingNightDerivation;
  canonicalTargetId?: string;
  cameraId?: string;
  opticalProfileId?: string;
  frameCount: number;
  createdAt: string;
  supersededBySessionCount: number;
  activePanelMembership?: {
    panelGroupId: string;
    panelRevisionId: string;
  };
  warningCodes: string[];
}

export interface SessionDetail {
  summary: SessionSummary;
  identity: {
    filter: ValueState<string>;
    exposureMs: ValueState<number>;
    gain: ValueState<number>;
    offset: ValueState<number>;
    binningX: ValueState<number>;
    binningY: ValueState<number>;
    readoutMode: ValueState<string>;
    rasterWidth: number;
    rasterHeight: number;
    cropEvidence: ValueState<string>;
    geometryEvidenceId?: string;
  };
  provenance: {
    sourceGroupId: string;
    acquisitionSiteId?: string;
    approvedAt: string;
    approvedBy: string;
  };
  predecessorSessionCount: number;
  metadataResolutionRevision: number;
}

// ── Panel group DTOs ────────────────────────────────────────────────────────────

export interface PanelGroupRevision {
  panelGroupId: string;
  revisionId: string;
  revisionNumber: number;
  parentRevisionId?: string;
  acceptedHead: boolean;
  retired: boolean;
  canonicalTargetId?: string;
  crossTargetAssociationId?: string;
  sessionCount: number;
  representativeSessionId: string;
  representativeEvidenceId: string;
  matchingSettingsRevision: number;
  acceptedAt: string;
  acceptedBy: string;
  decisionReason?: string;
  predecessorGroupCount: number;
  successorGroupCount: number;
}

// ── Mosaic DTOs ────────────────────────────────────────────────────────────────

export interface MosaicRevision {
  mosaicId: string;
  revisionId: string;
  revisionNumber: number;
  parentRevisionId?: string;
  acceptedHead: boolean;
  retired: boolean;
  intendedTargetId?: string;
  crossTargetAssociationId?: string;
  panelCount: number;
  edgeCount: number;
  capturedUnionEvidenceId: string;
  matchingSettingsRevision: number;
  acceptedAt: string;
  acceptedBy: string;
  decisionReason?: string;
  predecessorMosaicCount: number;
  successorMosaicCount: number;
}

export interface MosaicEdge {
  edgeId: string;
  leftPanelRevisionId: string;
  rightPanelRevisionId: string;
  overlapPercent: number;
  residualSkyRotationDeg: number;
  allowedResidualRotationRangesDeg: Array<{ min: number; max: number }>;
  parityMatch: boolean;
  acquisitionGeometryCompatible: boolean;
  evidenceId: string;
  stale: boolean;
  invalidationReasonCode?: string;
  appliedReclassificationPlanRevisionId?: string;
}

export interface MosaicObjectEvidenceItem {
  canonicalObjectId: string;
  panelContainmentRefs: string[];
  sessionContainmentRefs: string[];
  coverageState: 'full' | 'partial';
}

// ── Relation proposal DTOs ─────────────────────────────────────────────────────

export type ProposalKind =
  | 'panel_add'
  | 'panel_replace'
  | 'panel_split'
  | 'panel_merge'
  | 'mosaic_create'
  | 'mosaic_edge'
  | 'mosaic_split'
  | 'mosaic_merge'
  | 'manual_relation';

export type ProposalState =
  | 'pending'
  | 'accepted'
  | 'rejected'
  | 'superseded'
  | 'stale';

export interface ThresholdMeasurement {
  key: string;
  measuredValue: number;
  unit: string;
  comparison: 'lt' | 'lte' | 'eq' | 'gte' | 'gt';
  thresholdValue: number;
  outcome: 'pass' | 'fail';
}

export interface RelationEvidence {
  evidenceId: string;
  targetCompatibility: 'same_target' | 'reviewed_cross_target' | 'incompatible';
  footprintCoveragePercent?: number;
  centerSeparationPercent?: number;
  residualSkyRotationDeg?: number;
  allowedResidualRotationRangesDeg: Array<{ min: number; max: number }>;
  parity: 'match' | 'mismatch' | 'unknown';
  acquisitionGeometry: 'compatible' | 'incompatible' | 'unknown';
  equipment: 'compatible' | 'incompatible' | 'unknown';
  missingEvidenceCodes: string[];
  thresholdSnapshot: ThresholdMeasurement[];
}

export interface ProposalDecision {
  decision: 'accepted' | 'rejected' | 'corrected';
  decidedAt: string;
  reason: string;
  auditId: string;
}

export interface ManualRelationReview {
  relationKind: Exclude<ProposalKind, 'manual_relation'>;
  reviewReason: string;
  targetScope:
    | { kind: 'same_target'; canonicalTargetId: string }
    | { kind: 'existing_cross_target'; crossTargetAssociationId: string }
    | {
        kind: 'new_reviewed_cross_target';
        canonicalTargetIds: string[];
        purpose: string;
      };
  missingEvidenceCodes: string[];
}

export interface RelationProposal {
  proposalId: string;
  proposalRevision: number;
  kind: ProposalKind;
  state: ProposalState;
  sourceRevisionCount: number;
  subjectCount: number;
  proposedMembershipCount: number;
  proposedEdgeCount: number;
  proposedLineageCount: number;
  evidence: RelationEvidence;
  matchingSettingsRevision: number;
  basisFingerprint: string;
  createdAt: string;
  createdBy: string;
  manualRelation?: ManualRelationReview;
  decision?: ProposalDecision;
  supersededByProposalId?: string;
}

// ── Pagination ─────────────────────────────────────────────────────────────────

export interface Page<T> {
  items: T[];
  nextCursor?: string;
  readWatermark: number;
}

// ── Matching settings DTOs ─────────────────────────────────────────────────────

export interface GeometryThresholds {
  coverageMinPercent: number;
  centerSeparationMaxPercent: number;
  rotationMaxDeg: number;
}

export interface MatchingSettings {
  revision: number;
  sameSession: GeometryThresholds;
  sibling: GeometryThresholds;
  mosaic: {
    overlapMinPercent: number;
    overlapMaxPercent: number;
    residualSkyRotationCapDeg: number;
  };
  darkThermal: {
    moderateDeg: number;
    severeDeg: number;
  };
  calibrationAge: Array<{
    cameraId: string;
    kind: 'dark' | 'bias';
    freshThroughDays: number;
    redAfterDays: number;
  }>;
  flatOrientation: {
    normalThroughDeg: number;
    redAboveDeg: number;
  };
  flatAge: {
    redAfterNights: number;
  };
  updatedAt: string;
  updatedBy: string;
}

export type SettingsIssueSeverity = 'yellow' | 'red';

export interface SettingsIssue {
  code: string;
  severity: SettingsIssueSeverity;
  fieldPaths: string[];
  values: Array<{ fieldPath: string; value: number }>;
  messageKey: string;
}

export interface SettingsValidation {
  valid: boolean;
  issues: SettingsIssue[];
  effective: MatchingSettings;
}

// ── Hard bounds and defaults (FR-027 through FR-034) ──────────────────────────

export const MATCHING_SETTINGS_BOUNDS = {
  sameSession: {
    coverageMinPercent: { min: 90, max: 99.5, default: 95, yellowBelow: 93 },
    centerSeparationMaxPercent: {
      min: 0.5,
      max: 5,
      default: 2,
      yellowAbove: 3,
    },
    rotationMaxDeg: { min: 0.25, max: 3, default: 1, yellowAbove: 2 },
  },
  sibling: {
    coverageMinPercent: { min: 80, max: 95, default: 90, yellowBelow: 85 },
    centerSeparationMaxPercent: {
      min: 2,
      max: 15,
      default: 5,
      yellowAbove: 10,
    },
    rotationMaxDeg: { min: 1, max: 15, default: 5, yellowAbove: 10 },
  },
  mosaic: {
    overlapMinPercent: { min: 1, max: 20, default: 5, yellowBelow: 3 },
    overlapMaxPercent: { min: 20, max: 60, default: 40, yellowAbove: 50 },
  },
  darkThermal: {
    moderateDeg: { min: 0.1, max: 2, default: 0.5, yellowAbove: 1 },
    severeDeg: { min: 0.5, max: 5, default: 2, yellowAbove: 3 },
  },
  flatOrientation: {
    normalThroughDeg: { min: 0.5, max: 5, default: 2, yellowAbove: 3 },
    redAboveDeg: { min: 0, max: 15, default: 5, yellowAbove: 8 },
  },
  flatAge: {
    redAfterNights: { min: 7, max: 365, default: 7, yellowAbove: 90 },
  },
} as const;

export const CALIBRATION_AGE_DEFAULTS = {
  freshThroughDays: { min: 0, max: 1795, default: 270 },
  redAfterDays: { min: 30, max: 1825, default: 365, yellowAbove: 730 },
} as const;
