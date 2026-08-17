// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * RelationProposalDetail — full evidence disclosure + accept/reject flow.
 *
 * Spec 062 US2: the user reviews measured evidence before accepting any change
 * to an existing group. Acceptance requires:
 *   - the current proposal revision (stale-head guard)
 *   - the expected source revision set digest
 *
 * FR-094: severity visible as text + icon, not colour alone.
 * FR-095: modal focus at heading, remains within modal, returns to invoker.
 * FR-096: blocking failures use assertive announcement.
 *
 * When the proposal is stale (server returns `relation_proposal.stale`), the
 * component shows a stale-conflict banner and disables accept/reject until the
 * user refreshes.
 */

import { useState, useRef, useCallback } from 'react';
import { Modal, DetailPanel, FactsKV } from '@/components';
import { Btn, Pill, EmptyState, Skeleton, Banner } from '@/ui';
import { m } from '@/lib/i18n';
import {
  useRelationProposal,
  useRelationProposalAccept,
  useRelationProposalReject,
} from './useGroupsStore';
import { ProposalEvidencePanel } from './ProposalEvidencePanel';
import type { RelationProposal } from './groupsTypes';

// ── Helpers ────────────────────────────────────────────────────────────────────

function isStaleError(err: unknown): boolean {
  if (err == null || typeof err !== 'object') return false;
  const e = err as { code?: unknown };
  return e.code === 'relation_proposal.stale';
}

// ── Accept confirmation modal ──────────────────────────────────────────────────

interface AcceptConfirmModalProps {
  proposal: RelationProposal;
  open: boolean;
  onConfirm: () => Promise<void>;
  onClose: () => void;
  busy: boolean;
}

function AcceptConfirmModal({
  proposal,
  open,
  onConfirm,
  onClose,
  busy,
}: AcceptConfirmModalProps) {
  const headingRef = useRef<HTMLHeadingElement | null>(null);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={m.proposal_accept_confirm_title()}
      initialFocus={headingRef}
      size="sm"
      data-testid="proposal-accept-confirm"
      footer={
        <div className="pv-modal__actions">
          <Btn variant="ghost" onClick={onClose} disabled={busy}>
            {m.common_cancel()}
          </Btn>
          <Btn
            variant="primary"
            onClick={() => void onConfirm()}
            disabled={busy}
            aria-label={m.proposal_accept_btn_aria()}
            data-testid="proposal-accept-confirm-btn"
          >
            {busy ? m.common_saving() : m.proposal_accept_confirm_btn()}
          </Btn>
        </div>
      }
    >
      <h2 ref={headingRef} className="pv-modal__section-heading" tabIndex={-1}>
        {m.proposal_accept_confirm_title()}
      </h2>
      <p>
        {m.proposal_accept_confirm_desc({ subjects: proposal.subjectCount })}
      </p>
      {proposal.evidence.missingEvidenceCodes.length > 0 && (
        <Banner variant="warn" aria-live="polite">
          {m.proposal_accept_missing_evidence_warning({
            count: proposal.evidence.missingEvidenceCodes.length,
          })}
        </Banner>
      )}
    </Modal>
  );
}

// ── Reject confirmation modal ──────────────────────────────────────────────────

interface RejectConfirmModalProps {
  open: boolean;
  onConfirm: (reason: string) => Promise<void>;
  onClose: () => void;
  busy: boolean;
}

function RejectConfirmModal({
  open,
  onConfirm,
  onClose,
  busy,
}: RejectConfirmModalProps) {
  const [reason, setReason] = useState('');
  const headingRef = useRef<HTMLHeadingElement | null>(null);
  const reasonRef = useRef<HTMLTextAreaElement | null>(null);

  const trimmedReason = reason.trim();
  const canSubmit = trimmedReason.length > 0;

  const handleConfirm = async () => {
    if (!canSubmit) return;
    await onConfirm(trimmedReason);
    setReason('');
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={m.proposal_reject_confirm_title()}
      initialFocus={reasonRef}
      size="sm"
      data-testid="proposal-reject-confirm"
      footer={
        <div className="pv-modal__actions">
          <Btn variant="ghost" onClick={onClose} disabled={busy}>
            {m.common_cancel()}
          </Btn>
          <Btn
            variant="danger"
            onClick={() => void handleConfirm()}
            disabled={!canSubmit || busy}
            aria-label={m.proposal_reject_btn_aria()}
            data-testid="proposal-reject-confirm-btn"
          >
            {busy ? m.common_saving() : m.proposal_reject_confirm_btn()}
          </Btn>
        </div>
      }
    >
      <h2 ref={headingRef} className="pv-modal__section-heading" tabIndex={-1}>
        {m.proposal_reject_confirm_title()}
      </h2>
      <p>{m.proposal_reject_desc()}</p>
      <label htmlFor="reject-reason" className="pv-field-label">
        {m.proposal_reject_reason_label()}
      </label>
      <textarea
        id="reject-reason"
        ref={reasonRef}
        className="pv-input pv-input--textarea"
        aria-label={m.proposal_reject_reason_label()}
        value={reason}
        onChange={(e) => setReason(e.target.value)}
        rows={3}
        aria-required="true"
        aria-describedby={
          !canSubmit && reason.length > 0 ? 'reject-reason-error' : undefined
        }
        data-testid="reject-reason-input"
      />
      {!canSubmit && reason.length > 0 && (
        <div id="reject-reason-error" role="alert" className="pv-field-error">
          {m.proposal_reject_reason_required()}
        </div>
      )}
    </Modal>
  );
}

// ── Main component ─────────────────────────────────────────────────────────────

export interface RelationProposalDetailProps {
  proposalId: string;
  /** Returns focus to this element after an action modal closes (FR-095). */
  returnFocusRef?: React.RefObject<HTMLElement | null>;
}

export function RelationProposalDetail({
  proposalId,
  returnFocusRef,
}: RelationProposalDetailProps) {
  const {
    data: proposal,
    isLoading,
    isError,
    refetch,
  } = useRelationProposal(proposalId);
  const acceptMutation = useRelationProposalAccept();
  const rejectMutation = useRelationProposalReject();

  const [showAcceptConfirm, setShowAcceptConfirm] = useState(false);
  const [showRejectConfirm, setShowRejectConfirm] = useState(false);
  const [staleConflict, setStaleConflict] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const acceptBtnRef = useRef<HTMLButtonElement | null>(null);
  const rejectBtnRef = useRef<HTMLButtonElement | null>(null);

  const clearErrors = useCallback(() => {
    setStaleConflict(false);
    setActionError(null);
  }, []);

  const handleAcceptConfirm = useCallback(async () => {
    if (!proposal) return;
    clearErrors();
    try {
      await acceptMutation.mutateAsync({
        proposalId: proposal.proposalId,
        expectedProposalRevision: proposal.proposalRevision,
        expectedSourceRevisionSetDigest: proposal.evidence.evidenceId,
        mutationContext: { commandId: crypto.randomUUID() },
      });
      setShowAcceptConfirm(false);
      returnFocusRef?.current?.focus();
    } catch (err) {
      setShowAcceptConfirm(false);
      if (isStaleError(err)) {
        setStaleConflict(true);
      } else {
        setActionError(m.proposal_accept_error());
      }
    }
  }, [proposal, acceptMutation, clearErrors, returnFocusRef]);

  const handleRejectConfirm = useCallback(
    async (reason: string) => {
      if (!proposal) return;
      clearErrors();
      try {
        await rejectMutation.mutateAsync({
          proposalId: proposal.proposalId,
          expectedProposalRevision: proposal.proposalRevision,
          rejectionReason: reason,
          mutationContext: { commandId: crypto.randomUUID() },
        });
        setShowRejectConfirm(false);
        returnFocusRef?.current?.focus();
      } catch (err) {
        setShowRejectConfirm(false);
        if (isStaleError(err)) {
          setStaleConflict(true);
        } else {
          setActionError(m.proposal_reject_error());
        }
      }
    },
    [proposal, rejectMutation, clearErrors, returnFocusRef],
  );

  if (isLoading) {
    return (
      <div
        role="status"
        aria-live="polite"
        aria-label={m.proposal_detail_loading()}
        data-testid="relation-proposal-detail"
      >
        <Skeleton variant="block" />
      </div>
    );
  }

  if (isError || !proposal) {
    return (
      <EmptyState
        title={m.proposal_detail_error_title()}
        description={m.proposal_detail_error_desc()}
      />
    );
  }

  const isPending = proposal.state === 'pending';
  const isManual = proposal.kind === 'manual_relation';

  return (
    <DetailPanel
      title={m.proposal_kind_label()}
      subtitle={proposal.kind}
      data-testid="proposal-detail"
    >
      {/* Stale conflict banner — FR-096: assertive live region */}
      {staleConflict && (
        <Banner variant="warn" role="alert" aria-live="assertive">
          {m.proposal_stale_conflict()}
          <Btn
            variant="ghost"
            size="sm"
            onClick={() => {
              clearErrors();
              void refetch();
            }}
          >
            {m.common_refresh()}
          </Btn>
        </Banner>
      )}

      {/* Action error — assertive live region */}
      {actionError && (
        <div role="alert" aria-live="assertive" className="pv-error-text">
          {actionError}
        </div>
      )}

      {/* Proposal facts */}
      <div className="pv-detailpanel__kv-grid">
        <FactsKV label={m.proposal_state_label()} value={proposal.state} />
        <FactsKV
          label={m.proposal_subjects_label()}
          value={String(proposal.subjectCount)}
        />
        <FactsKV
          label={m.proposal_membership_label()}
          value={String(proposal.proposedMembershipCount)}
        />
        <FactsKV
          label={m.proposal_edges_label()}
          value={String(proposal.proposedEdgeCount)}
        />
        <FactsKV
          label={m.proposal_created_label()}
          value={proposal.createdAt}
        />
        <FactsKV
          label={m.proposal_settings_revision_label()}
          value={String(proposal.matchingSettingsRevision)}
        />
      </div>

      {isManual && proposal.manualRelation && (
        <section
          aria-label={m.proposal_manual_review_heading()}
          className="pv-proposal-manual-section"
        >
          <h4 className="pv-section__subheading">
            {m.proposal_manual_review_heading()}
          </h4>
          <div className="pv-detailpanel__kv-grid">
            <FactsKV
              label={m.proposal_manual_reason_label()}
              value={proposal.manualRelation.reviewReason}
            />
            <FactsKV
              label={m.proposal_manual_target_scope_label()}
              value={proposal.manualRelation.targetScope.kind}
            />
          </div>
          {proposal.manualRelation.missingEvidenceCodes.length > 0 && (
            <div
              aria-label={m.evidence_missing_codes_aria({
                count: proposal.manualRelation.missingEvidenceCodes.length,
              })}
            >
              <span className="pv-field-label">
                {m.evidence_missing_codes_label()}
              </span>
              <ul>
                {proposal.manualRelation.missingEvidenceCodes.map((code) => (
                  <li key={code}>
                    <code>{code}</code>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </section>
      )}

      {/* Evidence disclosure — required before acceptance (SC-007) */}
      <ProposalEvidencePanel evidence={proposal.evidence} />

      {/* Decision record — shown after a decision was made */}
      {proposal.decision && (
        <section
          aria-label={m.proposal_decision_heading()}
          className="pv-proposal-decision-section"
        >
          <h4 className="pv-section__subheading">
            {m.proposal_decision_heading()}
          </h4>
          <div className="pv-detailpanel__kv-grid">
            <FactsKV
              label={m.proposal_decision_outcome_label()}
              value={proposal.decision.decision}
            />
            <FactsKV
              label={m.proposal_decision_reason_label()}
              value={proposal.decision.reason}
            />
            <FactsKV
              label={m.proposal_decision_at_label()}
              value={proposal.decision.decidedAt}
            />
          </div>
        </section>
      )}

      {/* Action bar — only for pending proposals */}
      {isPending && (
        <div
          className="pv-proposal-detail__actions"
          role="group"
          aria-label={m.proposal_actions_aria()}
        >
          <Btn
            ref={rejectBtnRef}
            variant="ghost"
            onClick={() => {
              clearErrors();
              setShowRejectConfirm(true);
            }}
            aria-label={m.proposal_reject_btn_aria()}
            data-testid="proposal-reject-btn"
          >
            {m.proposal_reject_btn()}
          </Btn>
          <Btn
            ref={acceptBtnRef}
            variant="primary"
            onClick={() => {
              clearErrors();
              setShowAcceptConfirm(true);
            }}
            aria-label={m.proposal_accept_btn_aria()}
            data-testid="proposal-accept-btn"
          >
            {m.proposal_accept_btn()}
          </Btn>
        </div>
      )}

      {proposal.state === 'stale' && (
        <Pill variant="warn" role="status">
          {m.proposal_stale_label()}
        </Pill>
      )}

      {/* Confirmation modals — FR-095: focus enters at heading, stays within modal */}
      <AcceptConfirmModal
        proposal={proposal}
        open={showAcceptConfirm}
        onConfirm={handleAcceptConfirm}
        onClose={() => {
          setShowAcceptConfirm(false);
          acceptBtnRef.current?.focus();
        }}
        busy={acceptMutation.isPending}
      />
      <RejectConfirmModal
        open={showRejectConfirm}
        onConfirm={handleRejectConfirm}
        onClose={() => {
          setShowRejectConfirm(false);
          rejectBtnRef.current?.focus();
        }}
        busy={rejectMutation.isPending}
      />
    </DetailPanel>
  );
}
