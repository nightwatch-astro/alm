// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Feature-local IPC adapter for the spec-062 calibration candidate and handoff
 * surface (US4 — find compatible calibration sessions).
 *
 * All calls go through `invoke` from `@/api/ipc` because the Tauri commands
 * in this spec (calibration.candidate.list, calibration.handoff.*,
 * equipment.resolution.*) are not yet wired — node ic9h.20 will generate
 * the typed bindings. Once those bindings exist, replace each `invoke` call
 * with the corresponding `commands.*` call and re-type from the generated
 * shapes, then remove the entry from ipc-boundary.guard.test.ts.
 *
 * Until then, this file is the authoritative seam: callers in this feature
 * import from here, never from `@/api/ipc` directly.
 */

import { invoke } from '@/api/ipc';
import type {
  CalibrationRequirementDto,
  CalibrationCandidateEvidence,
  CalibrationHandoffSnapshot,
  CalibrationHandoffOperation,
  CalibrationSelection,
  EquipmentResolution,
  Page,
  EquipmentResolutionDecision,
} from './calibrationHandoffTypes';

export type {
  CalibrationRequirementDto,
  CalibrationCandidateEvidence,
  CalibrationHandoffSnapshot,
  CalibrationHandoffOperation,
  CalibrationSelection,
  EquipmentResolution,
  Page,
};

// ── Candidate list ─────────────────────────────────────────────────────────────

/**
 * `calibration.candidate.list` — evaluate calibration candidate evidence for
 * one requirement. Establishes a watermark on the first call; continuations
 * must carry the same watermark via `cursor`.
 */
export async function calibrationCandidateList(args: {
  requirement: CalibrationRequirementDto;
  asOf: string;
  automaticEligibility?: 'eligible' | 'review_required' | 'blocked';
  page?: { limit?: number; cursor?: string };
}): Promise<Page<CalibrationCandidateEvidence>> {
  return invoke('calibration_candidate_list', { request: args });
}

// ── Handoff queries ────────────────────────────────────────────────────────────

/**
 * `calibration.handoff.query` — fetch a handoff snapshot (latest head or a
 * specific snapshot by ID).
 */
export async function calibrationHandoffQuery(args: {
  handoffId: string;
  snapshotId?: string;
}): Promise<CalibrationHandoffSnapshot> {
  return invoke('calibration_handoff_query', { request: args });
}

/**
 * `calibration.handoff.operation.query` — fetch one verifying/terminal
 * operation.
 */
export async function calibrationHandoffOperationQuery(args: {
  operationId: string;
}): Promise<CalibrationHandoffOperation> {
  return invoke('calibration_handoff_operation_query', { request: args });
}

/**
 * `calibration.handoff.requirement.list` — paginated requirements for a
 * snapshot.
 */
export async function calibrationHandoffRequirementList(args: {
  snapshotId: string;
  page?: { limit?: number; cursor?: string };
}): Promise<Page<CalibrationRequirementDto>> {
  return invoke('calibration_handoff_requirement_list', { request: args });
}

/**
 * `calibration.handoff.selection.list` — paginated selections for a snapshot,
 * optionally scoped to one requirement.
 */
export async function calibrationHandoffSelectionList(args: {
  snapshotId: string;
  requirementId?: string;
  page?: { limit?: number; cursor?: string };
}): Promise<Page<CalibrationSelection>> {
  return invoke('calibration_handoff_selection_list', { request: args });
}

// ── Handoff commands ──────────────────────────────────────────────────────────

/**
 * `calibration.handoff.cancel` — request cancellation of a running
 * verification operation.
 */
export async function calibrationHandoffCancel(args: {
  operationId: string;
  mutationContext: { commandId: string };
}): Promise<CalibrationHandoffOperation> {
  return invoke('calibration_handoff_cancel', { request: args });
}

/**
 * `calibration.handoff.reviewed_add` — add a manually-reviewed session to
 * an existing snapshot, producing a successor.
 */
export async function calibrationHandoffReviewedAdd(args: {
  handoffId: string;
  snapshotId: string;
  expectedHandoffHeadGeneration: number;
  sessionId: string;
  requirementId: string;
  expectedSnapshotBasisFingerprint: string;
  evidenceId: string;
  decisionReason: string;
  acknowledgedWarningCodes: string[];
  mutationContext: { commandId: string };
}): Promise<{ operation: CalibrationHandoffOperation }> {
  return invoke('calibration_handoff_reviewed_add', { request: args });
}

// ── Equipment resolution ───────────────────────────────────────────────────────

/**
 * `equipment.resolution.query` — fetch the current (or a historical) equipment
 * resolution for a session.
 */
export async function equipmentResolutionQuery(args: {
  sessionId: string;
  resolutionRevision?: number;
}): Promise<EquipmentResolution> {
  return invoke('equipment_resolution_query', { request: args });
}

/**
 * `equipment.resolution.decide` — accept, correct, or override the equipment
 * resolution for a session.
 */
export async function equipmentResolutionDecide(args: {
  sessionId: string;
  expectedResolutionRevision: number;
  decision: EquipmentResolutionDecision;
  mutationContext: { commandId: string };
}): Promise<{ resolution: EquipmentResolution; auditId: string }> {
  return invoke('equipment_resolution_decide', { request: args });
}
