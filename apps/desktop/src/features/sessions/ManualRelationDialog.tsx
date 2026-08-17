// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ManualRelationDialog — form for creating a manual relation proposal.
 *
 * Spec 062 US2: "The application allows an explicit manual relation with
 * missing evidence disclosed." FR-026 specifies that missing reliable
 * footprint/orientation must be disclosed via missingEvidenceCodes.
 *
 * Guard from contract:
 *  - reviewReason must contain non-whitespace text
 *  - new_reviewed_cross_target scope requires ≥2 distinct canonical target IDs
 *  - missingEvidenceCodes must enumerate every unavailable measurement
 *  - at least one of proposedMembership / edges / lineage is non-empty
 *
 * FR-095: modal focus enters at the review-reason heading.
 * FR-094: missing evidence shown with text + icon.
 */

import { useState, useRef, type FormEvent } from 'react';
import { Modal } from '@/components';
import { Btn, Banner } from '@/ui';
import { m } from '@/lib/i18n';
import { useRelationProposalManualCreate } from './useGroupsStore';
import type { ProposalKind, ManualRelationReview } from './groupsTypes';

// Missing evidence codes as defined in the spec contracts. The frontend
// presents these as labelled checkboxes so users explicitly acknowledge each
// missing measurement.
const KNOWN_MISSING_EVIDENCE_CODES: Array<{
  code: string;
  label: () => string;
}> = [
  {
    code: 'footprint.unavailable',
    label: () => m.missing_evidence_footprint(),
  },
  {
    code: 'orientation.unavailable',
    label: () => m.missing_evidence_orientation(),
  },
  { code: 'parity.unknown', label: () => m.missing_evidence_parity() },
  { code: 'geometry.unresolved', label: () => m.missing_evidence_geometry() },
];

const RELATION_KINDS: Array<{
  value: Exclude<ProposalKind, 'manual_relation'>;
  label: () => string;
}> = [
  { value: 'panel_add', label: () => m.proposal_kind_panel_add() },
  { value: 'panel_replace', label: () => m.proposal_kind_panel_replace() },
  { value: 'panel_split', label: () => m.proposal_kind_panel_split() },
  { value: 'panel_merge', label: () => m.proposal_kind_panel_merge() },
  { value: 'mosaic_create', label: () => m.proposal_kind_mosaic_create() },
  { value: 'mosaic_edge', label: () => m.proposal_kind_mosaic_edge() },
  { value: 'mosaic_split', label: () => m.proposal_kind_mosaic_split() },
  { value: 'mosaic_merge', label: () => m.proposal_kind_mosaic_merge() },
];

export interface ManualRelationDialogProps {
  open: boolean;
  onClose: () => void;
  /** Panel group or session IDs to use as subjects. */
  defaultSubjectIds?: string[];
}

export function ManualRelationDialog({
  open,
  onClose,
  defaultSubjectIds,
}: ManualRelationDialogProps) {
  const create = useRelationProposalManualCreate();

  const [relationKind, setRelationKind] =
    useState<Exclude<ProposalKind, 'manual_relation'>>('panel_add');
  const [reviewReason, setReviewReason] = useState('');
  const [targetScopeKind, setTargetScopeKind] =
    useState<ManualRelationReview['targetScope']['kind']>('same_target');
  const [canonicalTargetId, setCanonicalTargetId] = useState('');
  const [crossTargetIds, setCrossTargetIds] = useState('');
  const [crossTargetPurpose, setCrossTargetPurpose] = useState('');
  const [missingCodes, setMissingCodes] = useState<Set<string>>(new Set());
  const [submitError, setSubmitError] = useState<string | null>(null);

  const headingRef = useRef<HTMLHeadingElement | null>(null);
  const reasonRef = useRef<HTMLTextAreaElement | null>(null);

  const trimmedReason = reviewReason.trim();

  // Cross-target validation: ≥2 distinct IDs required
  const parsedCrossTargetIds = crossTargetIds
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
  const uniqueCrossTargetIds = [...new Set(parsedCrossTargetIds)];

  const crossTargetValid =
    targetScopeKind !== 'new_reviewed_cross_target' ||
    uniqueCrossTargetIds.length >= 2;

  const canSubmit = trimmedReason.length > 0 && crossTargetValid;

  function buildTargetScope(): ManualRelationReview['targetScope'] {
    if (targetScopeKind === 'same_target') {
      return {
        kind: 'same_target',
        canonicalTargetId: canonicalTargetId.trim(),
      };
    }
    if (targetScopeKind === 'existing_cross_target') {
      return {
        kind: 'existing_cross_target',
        crossTargetAssociationId: canonicalTargetId.trim(),
      };
    }
    return {
      kind: 'new_reviewed_cross_target',
      canonicalTargetIds: uniqueCrossTargetIds,
      purpose: crossTargetPurpose.trim(),
    };
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setSubmitError(null);

    const subjectRefs = (defaultSubjectIds ?? []).map((id) => ({
      entityType: 'session',
      entityId: id,
    }));

    try {
      await create.mutateAsync({
        relationKind,
        sourceRevisionRefs: [],
        subjectRefs,
        proposedMembershipRefs: subjectRefs,
        targetScope: buildTargetScope(),
        evidence: {
          evidenceId: crypto.randomUUID(),
          targetCompatibility:
            targetScopeKind === 'same_target'
              ? 'same_target'
              : 'reviewed_cross_target',
          allowedResidualRotationRangesDeg: [],
          parity: missingCodes.has('parity.unknown') ? 'unknown' : 'unknown',
          acquisitionGeometry: missingCodes.has('geometry.unresolved')
            ? 'unknown'
            : 'unknown',
          equipment: 'unknown',
          missingEvidenceCodes: [...missingCodes],
          thresholdSnapshot: [],
        },
        reviewReason: trimmedReason,
        mutationContext: { commandId: crypto.randomUUID() },
      });
      onClose();
      // Reset form
      setReviewReason('');
      setMissingCodes(new Set());
      setCanonicalTargetId('');
      setCrossTargetIds('');
      setCrossTargetPurpose('');
    } catch {
      setSubmitError(m.manual_relation_submit_error());
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={m.manual_relation_open_btn()}
      initialFocus={headingRef}
      size="md"
      data-testid="manual-relation-dialog"
      footer={
        <div className="pv-modal__actions">
          <Btn variant="ghost" onClick={onClose} disabled={create.isPending}>
            {m.common_cancel()}
          </Btn>
          <Btn
            variant="primary"
            type="submit"
            form="manual-relation-form"
            disabled={!canSubmit || create.isPending}
            data-testid="manual-relation-submit"
          >
            {create.isPending
              ? m.common_saving()
              : m.manual_relation_submit_btn()}
          </Btn>
        </div>
      }
    >
      <h2 ref={headingRef} className="pv-modal__section-heading" tabIndex={-1}>
        {m.manual_relation_open_btn()}
      </h2>

      {submitError && (
        <Banner variant="danger" role="alert" aria-live="assertive">
          {submitError}
        </Banner>
      )}

      <form id="manual-relation-form" onSubmit={(e) => void handleSubmit(e)}>
        {/* Relation kind */}
        <div className="pv-form-section">
          <label htmlFor="manual-relation-kind" className="pv-field-label">
            {m.manual_relation_kind_label()}
          </label>
          <select
            id="manual-relation-kind"
            className="pv-input"
            value={relationKind}
            onChange={(e) =>
              setRelationKind(
                e.target.value as Exclude<ProposalKind, 'manual_relation'>,
              )
            }
          >
            {RELATION_KINDS.map((k) => (
              <option key={k.value} value={k.value}>
                {k.label()}
              </option>
            ))}
          </select>
        </div>

        {/* Review reason — required, non-whitespace */}
        <div className="pv-form-section">
          <label htmlFor="review-reason" className="pv-field-label">
            {m.proposal_manual_reason_label()}
            <span className="pv-field-required" aria-hidden="true">
              {' '}
              *
            </span>
          </label>
          <textarea
            id="review-reason"
            ref={reasonRef}
            className="pv-input pv-input--textarea"
            aria-label={m.proposal_manual_reason_label()}
            value={reviewReason}
            onChange={(e) => setReviewReason(e.target.value)}
            rows={3}
            required
            aria-required="true"
            placeholder={m.manual_relation_reason_placeholder()}
            data-testid="manual-relation-reason"
          />
        </div>

        {/* Target scope */}
        <div className="pv-form-section">
          <fieldset className="pv-fieldset">
            <legend className="pv-field-label">
              {m.proposal_manual_target_scope_label()}
            </legend>
            {(
              [
                'same_target',
                'existing_cross_target',
                'new_reviewed_cross_target',
              ] as const
            ).map((kind) => (
              <label key={kind} className="pv-radio-label">
                <input
                  type="radio"
                  name="target-scope"
                  value={kind}
                  checked={targetScopeKind === kind}
                  onChange={() => setTargetScopeKind(kind)}
                  aria-label={
                    kind === 'same_target'
                      ? m.manual_relation_scope_same()
                      : kind === 'existing_cross_target'
                        ? m.manual_relation_scope_existing_cross()
                        : m.manual_relation_scope_new_cross()
                  }
                />
                {kind === 'same_target' && m.manual_relation_scope_same()}
                {kind === 'existing_cross_target' &&
                  m.manual_relation_scope_existing_cross()}
                {kind === 'new_reviewed_cross_target' &&
                  m.manual_relation_scope_new_cross()}
              </label>
            ))}
          </fieldset>

          {(targetScopeKind === 'same_target' ||
            targetScopeKind === 'existing_cross_target') && (
            <div>
              <label htmlFor="target-id" className="pv-field-label">
                {targetScopeKind === 'same_target'
                  ? m.manual_relation_target_id_label()
                  : m.manual_relation_association_id_label()}
              </label>
              <input
                id="target-id"
                type="text"
                className="pv-input"
                value={canonicalTargetId}
                onChange={(e) => setCanonicalTargetId(e.target.value)}
                placeholder={m.manual_relation_id_placeholder()}
                aria-label={
                  targetScopeKind === 'same_target'
                    ? m.manual_relation_target_id_label()
                    : m.manual_relation_association_id_label()
                }
              />
            </div>
          )}

          {targetScopeKind === 'new_reviewed_cross_target' && (
            <div className="pv-stack-2">
              <div>
                <label htmlFor="cross-target-ids" className="pv-field-label">
                  {m.manual_relation_cross_target_ids_label()}
                  {!crossTargetValid && (
                    <span
                      id="cross-target-ids-error"
                      className="pv-field-error"
                      role="alert"
                    >
                      {' '}
                      {m.manual_relation_cross_target_min_error()}
                    </span>
                  )}
                </label>
                <textarea
                  id="cross-target-ids"
                  className="pv-input pv-input--textarea"
                  aria-label={m.manual_relation_cross_target_ids_label()}
                  value={crossTargetIds}
                  onChange={(e) => setCrossTargetIds(e.target.value)}
                  rows={3}
                  placeholder={m.manual_relation_cross_target_ids_placeholder()}
                  aria-invalid={!crossTargetValid}
                  aria-describedby={
                    !crossTargetValid ? 'cross-target-ids-error' : undefined
                  }
                  data-testid="cross-target-ids-input"
                />
                <span className="pv-field-hint">
                  {m.manual_relation_cross_target_count({
                    count: uniqueCrossTargetIds.length,
                  })}
                </span>
              </div>
              <div>
                <label
                  htmlFor="cross-target-purpose"
                  className="pv-field-label"
                >
                  {m.manual_relation_purpose_label()}
                </label>
                <input
                  id="cross-target-purpose"
                  type="text"
                  className="pv-input"
                  value={crossTargetPurpose}
                  onChange={(e) => setCrossTargetPurpose(e.target.value)}
                  placeholder={m.manual_relation_purpose_placeholder()}
                  aria-label={m.manual_relation_purpose_label()}
                />
              </div>
            </div>
          )}
        </div>

        {/* Missing evidence disclosure — FR-026 */}
        <div className="pv-form-section">
          <fieldset className="pv-fieldset">
            <legend className="pv-field-label">
              {m.evidence_missing_codes_label()}
            </legend>
            <p className="pv-field-hint">
              {m.manual_relation_missing_evidence_desc()}
            </p>
            {KNOWN_MISSING_EVIDENCE_CODES.map(({ code, label }) => (
              <label key={code} className="pv-check-label">
                <input
                  type="checkbox"
                  checked={missingCodes.has(code)}
                  aria-label={label()}
                  onChange={(e) => {
                    const next = new Set(missingCodes);
                    if (e.target.checked) next.add(code);
                    else next.delete(code);
                    setMissingCodes(next);
                  }}
                  data-testid={`missing-evidence-${code}`}
                />{' '}
                <code>{code}</code>: {label()}
              </label>
            ))}
          </fieldset>
        </div>
      </form>
    </Modal>
  );
}
