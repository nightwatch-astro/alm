// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Feature-local IPC adapter for the spec-062 sessions/groups/proposals surface.
 *
 * All calls go through `invoke` from `@/api/ipc` because the Tauri commands
 * in this spec (panel_group.*, relation_proposal.*, matching_settings.*) are
 * not yet wired — node ic9h.20 will generate the typed bindings. Once those
 * bindings exist, replace each `invoke` call with the corresponding
 * `commands.*` call and re-type the return value from the generated shapes.
 *
 * Until then, every public function acts as the authoritative seam: callers
 * in the rest of this feature import from here, never from `@/api/ipc`
 * directly.
 */

import { invoke } from '@/api/ipc';
import type {
  PanelGroupRevision,
  RelationProposal,
  ProposalKind,
  ProposalState,
  MosaicRevision,
  MosaicEdge,
  MosaicObjectEvidenceItem,
  MatchingSettings,
  SettingsValidation,
  Page,
  ManualRelationReview,
  RelationEvidence,
} from './groupsTypes';

// ── Panel group queries ────────────────────────────────────────────────────────

/**
 * `panel_group.query` — fetch the accepted head (and optional historical
 * revision) for one panel group.
 */
export async function panelGroupQuery(args: {
  panelGroupId: string;
  revisionId?: string;
}): Promise<{
  acceptedHead: PanelGroupRevision;
  requestedRevision?: PanelGroupRevision;
}> {
  return invoke('panel_group_query', { request: args });
}

/**
 * `panel_group.list` — list accepted panel group heads, optionally scoped to
 * a target or session.
 */
export async function panelGroupList(args: {
  targetId?: string;
  sessionId?: string;
  activeOnly?: boolean;
  page?: { cursor?: string; limit?: number };
}): Promise<Page<PanelGroupRevision>> {
  return invoke('panel_group_list', { request: args });
}

/**
 * `panel_group.membership.list` — immutable ordered session membership for
 * one revision.
 */
export async function panelGroupMembershipList(args: {
  panelGroupId: string;
  revisionId: string;
  page?: { cursor?: string; limit?: number };
}): Promise<Page<{ sessionId: string; ordinal: number }>> {
  return invoke('panel_group_membership_list', { request: args });
}

// ── Mosaic queries ─────────────────────────────────────────────────────────────

/**
 * `mosaic.query` — fetch the accepted mosaic head.
 */
export async function mosaicQuery(args: {
  mosaicId: string;
  revisionId?: string;
}): Promise<{
  acceptedHead: MosaicRevision;
  requestedRevision?: MosaicRevision;
}> {
  return invoke('mosaic_query', { request: args });
}

/**
 * `mosaic.edge.list` — edges for one mosaic revision.
 */
export async function mosaicEdgeList(args: {
  mosaicId: string;
  revisionId: string;
  page?: { cursor?: string; limit?: number };
}): Promise<Page<{ edge: MosaicEdge; ordinal: number }>> {
  return invoke('mosaic_edge_list', { request: args });
}

/**
 * `mosaic.panel.list` — panel group membership for one mosaic revision.
 */
export async function mosaicPanelList(args: {
  mosaicId: string;
  revisionId: string;
  page?: { cursor?: string; limit?: number };
}): Promise<
  Page<{ panelGroupId: string; panelRevisionId: string; ordinal: number }>
> {
  return invoke('mosaic_panel_list', { request: args });
}

/**
 * `mosaic.object_evidence.list` — object containment evidence for a mosaic
 * revision (filtered: point objects in gaps excluded, zero-intersection
 * extended objects excluded).
 */
export async function mosaicObjectEvidenceList(args: {
  mosaicId: string;
  revisionId: string;
  page?: { cursor?: string; limit?: number };
}): Promise<Page<MosaicObjectEvidenceItem>> {
  return invoke('mosaic_object_evidence_list', { request: args });
}

// ── Relation proposal queries ──────────────────────────────────────────────────

/**
 * `relation_proposal.list` — paginated ordered list of proposals, optionally
 * filtered by state, kind, target, or subject reference.
 */
export async function relationProposalList(args: {
  state?: ProposalState;
  kind?: ProposalKind;
  targetId?: string;
  page?: { cursor?: string; limit?: number };
}): Promise<Page<RelationProposal>> {
  return invoke('relation_proposal_list', { request: args });
}

/**
 * `relation_proposal.query` — fetch one proposal by ID.
 */
export async function relationProposalQuery(args: {
  proposalId: string;
}): Promise<RelationProposal> {
  return invoke('relation_proposal_query', { request: args });
}

// ── Relation proposal commands ─────────────────────────────────────────────────

/**
 * `relation_proposal.manual.create` — create a manual relation proposal.
 * Missing-evidence codes must enumerate every unavailable geometry or
 * orientation measurement.
 */
export async function relationProposalManualCreate(args: {
  relationKind: Exclude<RelationProposal['kind'], 'manual_relation'>;
  sourceRevisionRefs: Array<{
    entityType: string;
    entityId: string;
    revisionId: string;
  }>;
  subjectRefs: Array<{ entityType: string; entityId: string }>;
  proposedMembershipRefs?: Array<{ entityType: string; entityId: string }>;
  proposedEdges?: MosaicEdge[];
  proposedLineage?: Array<{
    predecessorGroupId: string;
    successorGroupId: string;
  }>;
  targetScope: ManualRelationReview['targetScope'];
  evidence: RelationEvidence;
  reviewReason: string;
  mutationContext: { commandId: string };
}): Promise<{ proposal: RelationProposal; auditId: string }> {
  return invoke('relation_proposal_manual_create', { request: args });
}

/**
 * `relation_proposal.accept` — accept a pending proposal. Supply the
 * current proposal revision and source-revision-set digest to guard against
 * concurrent changes.
 */
export async function relationProposalAccept(args: {
  proposalId: string;
  expectedProposalRevision: number;
  expectedSourceRevisionSetDigest: string;
  mutationContext: { commandId: string };
}): Promise<{
  proposal: RelationProposal;
  decisionSnapshotId: string;
  crossTargetAssociationId?: string;
  auditId: string;
}> {
  return invoke('relation_proposal_accept', { request: args });
}

/**
 * `relation_proposal.reject` — reject a pending proposal. Rejection reason
 * must be non-whitespace. Equivalent automatic proposals stay suppressed
 * while their basis fingerprint, evidence revision, and settings revision
 * remain unchanged.
 */
export async function relationProposalReject(args: {
  proposalId: string;
  expectedProposalRevision: number;
  rejectionReason: string;
  mutationContext: { commandId: string };
}): Promise<{
  proposal: RelationProposal;
  suppressionFingerprint: string;
  auditId: string;
}> {
  return invoke('relation_proposal_reject', { request: args });
}

// ── Matching settings ──────────────────────────────────────────────────────────

/**
 * `matching_settings.get` — read current (or historical) matching settings.
 */
export async function matchingSettingsGet(args?: {
  revision?: number;
}): Promise<MatchingSettings> {
  return invoke('matching_settings_get', { request: args ?? {} });
}

/**
 * `matching_settings.validate` — validate a settings patch against a base
 * revision. Returns all red and yellow issues in field-path order. Does not
 * write to the database.
 */
export async function matchingSettingsValidate(args: {
  baseRevision: number;
  patch: Partial<MatchingSettings>;
}): Promise<SettingsValidation> {
  return invoke('matching_settings_validate', { request: args });
}

/**
 * `matching_settings.update` — save a validated settings patch. Requires:
 * - `expectedRevision` must match the current accepted revision.
 * - No red issues.
 * - Every yellow issue code must appear in `acknowledgedWarningCodes`.
 */
export async function matchingSettingsUpdate(args: {
  expectedRevision: number;
  patch: Partial<MatchingSettings>;
  acknowledgedWarningCodes: string[];
  mutationContext: { commandId: string };
}): Promise<{
  settings: MatchingSettings;
  warnings: SettingsValidation['issues'];
  auditId: string;
}> {
  return invoke('matching_settings_update', { request: args });
}
