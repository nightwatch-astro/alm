// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Local TypeScript mirrors of the spec-062 calibration candidate / handoff
 * DTOs (contracts/calibration-handoff.md and
 * contracts/metadata-equipment-reclassification.md).
 *
 * These are feature-local types declared here because node ic9h.20 will
 * generate the Tauri-specta bindings — at that point, replace these with
 * imports from `@/bindings/index` and delete this file.
 */

// ── Pagination ─────────────────────────────────────────────────────────────────

export interface Page<T> {
  items: T[];
  nextCursor?: string;
}

// ── CalibrationRequirement ─────────────────────────────────────────────────────

export type CalibrationKind = 'dark' | 'bias' | 'flat';

export interface CalibrationRequirementDto {
  requirementId: string;
  kind: CalibrationKind;
  cameraId?: string;
  opticalProfileId?: string;
  filterIdentity?:
    | { state: 'known'; normalizedCapturedLabelId: string }
    | { state: 'absent' };
  targetLightSessionId?: string;
  targetObservingNight?: string;
  recipeId: string;
  recipeRevisionId: string;
  requiredRecipeEvidenceRef: string;
  requiredRecipeEvidenceComplete: boolean;
  missingRequiredFields: string[];
}

// ── CalibrationCandidateEvidence ───────────────────────────────────────────────

export type TemperatureMode =
  | 'regulated'
  | 'unregulated'
  | 'unknown'
  | 'not_applicable';

export type EvidenceState =
  | 'fresh'
  | 'yellow'
  | 'red'
  | 'unknown'
  | 'normal'
  | 'not_applicable';

export interface AgeEvidence {
  basis: 'elapsed_days' | 'observing_night_distance';
  state: 'fresh' | 'yellow' | 'red' | 'unknown';
  ageDays?: number;
  ageNights?: number;
  freshThroughDays?: number;
  redAfterDays?: number;
  freshThroughNights?: number;
  redAfterNights?: number;
  settingsRevision: number;
}

export interface ThermalEvidence {
  state: 'normal' | 'yellow' | 'red' | 'unknown' | 'not_applicable';
  validReadingPercent?: number;
  minimumAbsoluteDeviationDeg?: number;
  medianAbsoluteDeviationDeg?: number;
  maximumAbsoluteDeviationDeg?: number;
  percentile95AbsoluteDeviationDeg?: number;
  missingReadingCount: number;
  invalidReadingCount: number;
  settingsRevision?: number;
}

export interface OrientationEvidence {
  state: 'normal' | 'yellow' | 'red' | 'unknown' | 'not_applicable';
  minimumCircularDeltaDeg?: number;
  normalThroughDeg?: number;
  redAboveDeg?: number;
  settingsRevision?: number;
}

export type AutomaticEligibility = 'eligible' | 'review_required' | 'blocked';

export interface SourceAvailability {
  indexedFrameCount: number;
  availableReadableIndexedFrameCount: number;
  checkedAt: string;
}

export interface CalibrationCandidateEvidence {
  evidenceId: string;
  sessionId: string;
  requirementId: string;
  recipeCompatibility: 'compatible' | 'incompatible' | 'unknown';
  recipeEvidenceRef: string;
  recipeEvidenceComplete: boolean;
  missingRecipeFields: string[];
  temperatureMode: TemperatureMode;
  age: AgeEvidence;
  thermal: ThermalEvidence;
  orientation: OrientationEvidence;
  sourceAvailability: SourceAvailability;
  sufficient: boolean;
  automaticEligibility: AutomaticEligibility;
  warningCodes: string[];
  basisFingerprint: string;
}

// ── CalibrationSelection ───────────────────────────────────────────────────────

export interface CalibrationSelection {
  selectionId: string;
  requirementId: string;
  sessionId: string;
  evidenceId: string;
  source: 'automatic' | 'reviewed';
  selectedAt: string;
  review?: {
    reviewId: string;
    reviewedAt: string;
    decisionReason: string;
    acknowledgedWarningCodes: string[];
  };
}

// ── CalibrationHandoffSnapshot ────────────────────────────────────────────────

export interface CalibrationHandoffSnapshot {
  handoffId: string;
  handoffHeadGeneration: number;
  snapshotId: string;
  predecessorSnapshotId?: string;
  projectId: string;
  externalProcessor: 'pixinsight_wbpp' | 'siril';
  requirementCount: number;
  selectionCount: number;
  frameCount: number;
  sourceByteCount: number;
  maximumSourceBytes: number;
  matchingSettingsRevision: number;
  evaluationAt: string;
  createdAt: string;
  createdBy: string;
  basisFingerprint: string;
  warningCodes: string[];
}

// ── CalibrationHandoffOperation ───────────────────────────────────────────────

export type HandoffOperationState =
  | 'verifying'
  | 'cancelling'
  | 'cancelled'
  | 'applied'
  | 'failed';

export type HandoffFailureCode =
  | 'calibration.source_unavailable'
  | 'calibration.source_identity_changed'
  | 'calibration.handoff_too_large'
  | 'calibration.cancel_deadline_exceeded'
  | 'calibration.verification_failed';

export interface CalibrationHandoffOperation {
  operationId: string;
  handoffId: string;
  state: HandoffOperationState;
  verifiedFrameCount: number;
  totalFrameCount: number;
  verifiedSourceBytes: number;
  totalSourceBytes: number;
  cancelSafe: boolean;
  snapshotId?: string;
  reviewId?: string;
  failureCode?: HandoffFailureCode;
  failureDetail?: string;
  updatedAt: string;
}

// ── EquipmentResolution ────────────────────────────────────────────────────────

export interface CameraCandidate {
  cameraId: string;
  displayName: string;
  matchedAliases: string[];
  geometryCompatible: boolean;
  confidence: 'exact' | 'review';
}

export interface OpticalProfileCandidate {
  opticalProfileId: string;
  displayName: string;
  reportedFocalLengthMm?: number;
  calculatedFocalLengthMm?: number;
  representativeFocalLengthMm: number;
  representativeDifferencePercent?: number;
  reportedCalculatedDifferencePercent?: number;
  classification: 'same' | 'review' | 'different';
}

export interface ResolutionWarning {
  code: string;
  severity: 'yellow' | 'red';
  field?: string;
  evidenceRefs: string[];
}

export interface ResolutionChoice<T> {
  selectedId?: string;
  candidates: T[];
  basis: string[];
  decision: 'automatic' | 'accepted' | 'corrected' | 'unresolved';
}

export interface EquipmentResolution {
  resolutionId: string;
  sessionId: string;
  revision: number;
  state: 'resolved' | 'needs_review' | 'blocked';
  camera: ResolutionChoice<CameraCandidate>;
  opticalProfile: ResolutionChoice<OpticalProfileCandidate>;
  warnings: ResolutionWarning[];
  evidenceRevision: number;
  decidedAt?: string;
  decidedBy?: string;
}

export interface EquipmentResolutionDecision {
  cameraId?: string;
  opticalProfileId?: string;
  markCameraUnregulated?: boolean;
  note: string;
}
