// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * RelationProposalList — paginated, ordered list of relation proposals.
 *
 * Spec 062 US2: the user sees pending proposals organised by kind, with the
 * evidence severity surfaced per row. Selecting a row opens the detail view
 * (RelationProposalDetail) in the same panel. The list shows pending proposals
 * by default; a state filter lets the user browse historical decisions.
 *
 * FR-094, FR-095: severity conveyed by text + icon, keyboard operable.
 * FR-096: loading and error states announced to screen readers.
 */

import { useState, type ReactNode } from 'react';
import { EmptyState, Skeleton, Pill } from '@/ui';
import type { PillVariant } from '@/ui';
import { m } from '@/lib/i18n';
import { useRelationProposals } from './useGroupsStore';
import { EvidenceSeverityPill } from './EvidenceSeverityPill';
import type {
  RelationProposal,
  ProposalState,
  ProposalKind,
} from './groupsTypes';

// ── Severity derivation ────────────────────────────────────────────────────────

function proposalEvidenceSeverity(
  p: RelationProposal,
): 'ok' | 'yellow' | 'red' | 'missing' {
  if (p.evidence.missingEvidenceCodes.length > 0) return 'missing';
  const failed = p.evidence.thresholdSnapshot.filter(
    (t) => t.outcome === 'fail',
  );
  if (failed.length > 0) return 'red';
  if (
    p.evidence.parity === 'unknown' ||
    p.evidence.acquisitionGeometry === 'unknown' ||
    p.evidence.equipment === 'unknown'
  ) {
    return 'yellow';
  }
  return 'ok';
}

// ── State filter options ───────────────────────────────────────────────────────

const STATE_OPTS: Array<{ value: ProposalState | ''; label: () => string }> = [
  { value: 'pending', label: () => m.proposal_state_pending() },
  { value: 'accepted', label: () => m.proposal_state_accepted() },
  { value: 'rejected', label: () => m.proposal_state_rejected() },
  { value: 'superseded', label: () => m.proposal_state_superseded() },
  { value: 'stale', label: () => m.proposal_state_stale() },
  { value: '', label: () => m.common_all() },
];

function statePillVariant(state: ProposalState): PillVariant {
  if (state === 'accepted') return 'ok';
  if (state === 'rejected') return 'danger';
  if (state === 'stale') return 'warn';
  return 'neutral';
}

function kindLabel(kind: ProposalKind): string {
  switch (kind) {
    case 'panel_add':
      return m.proposal_kind_panel_add();
    case 'panel_replace':
      return m.proposal_kind_panel_replace();
    case 'panel_split':
      return m.proposal_kind_panel_split();
    case 'panel_merge':
      return m.proposal_kind_panel_merge();
    case 'mosaic_create':
      return m.proposal_kind_mosaic_create();
    case 'mosaic_edge':
      return m.proposal_kind_mosaic_edge();
    case 'mosaic_split':
      return m.proposal_kind_mosaic_split();
    case 'mosaic_merge':
      return m.proposal_kind_mosaic_merge();
    case 'manual_relation':
      return m.proposal_kind_manual_relation();
  }
}

// ── Proposal row ──────────────────────────────────────────────────────────────

function ProposalRow({
  proposal,
  selected,
  onClick,
}: {
  proposal: RelationProposal;
  selected: boolean;
  onClick: () => void;
}) {
  const evidenceSeverity = proposalEvidenceSeverity(proposal);
  const isManual = proposal.kind === 'manual_relation';

  return (
    <button
      type="button"
      role="row"
      className={`pv-proposal-row${selected ? ' pv-proposal-row--selected' : ''}`}
      aria-selected={selected}
      aria-label={m.proposal_row_aria({
        kind: kindLabel(proposal.kind),
        state: proposal.state,
        subjects: proposal.subjectCount,
      })}
      onClick={onClick}
      data-testid={`proposal-row-${proposal.proposalId}`}
    >
      <span className="pv-proposal-row__kind">
        {kindLabel(proposal.kind)}
        {isManual && (
          <Pill variant="info" aria-label={m.proposal_manual_aria()}>
            {m.proposal_manual_label()}
          </Pill>
        )}
      </span>
      <span className="pv-proposal-row__subjects">
        {m.proposal_row_subjects({ count: proposal.subjectCount })}
      </span>
      <span className="pv-proposal-row__evidence">
        <EvidenceSeverityPill severity={evidenceSeverity} />
      </span>
      <span className="pv-proposal-row__state">
        <Pill variant={statePillVariant(proposal.state)}>{proposal.state}</Pill>
      </span>
    </button>
  );
}

// ── Main component ─────────────────────────────────────────────────────────────

export interface RelationProposalListProps {
  /** Controlled selected proposal ID. */
  selectedId: string | undefined;
  onSelect: (proposalId: string) => void;
  /** Slot for any action button (e.g. "Create manual relation"). */
  headerAction?: ReactNode;
}

export function RelationProposalList({
  selectedId,
  onSelect,
  headerAction,
}: RelationProposalListProps) {
  const [stateFilter, setStateFilter] = useState<ProposalState | ''>('pending');

  const { data, isLoading, isError } = useRelationProposals(
    stateFilter ? { state: stateFilter } : undefined,
  );

  const proposals = data?.items ?? [];

  return (
    <section
      aria-label={m.proposal_list_heading()}
      className="pv-proposal-list-section"
      data-testid="relation-proposal-list"
    >
      <div className="pv-proposal-list__header">
        <h3 className="pv-section__heading">{m.proposal_list_heading()}</h3>
        {headerAction}
      </div>

      <div
        className="pv-proposal-list__filter"
        role="group"
        aria-label={m.proposal_filter_aria()}
      >
        {STATE_OPTS.map((opt) => (
          <button
            key={opt.value}
            type="button"
            className={`pv-seg-btn${stateFilter === opt.value ? ' pv-seg-btn--active' : ''}`}
            aria-pressed={stateFilter === opt.value}
            onClick={() => setStateFilter(opt.value)}
          >
            {opt.label()}
          </button>
        ))}
      </div>

      {isLoading && (
        <div
          role="status"
          aria-live="polite"
          aria-label={m.proposal_list_loading()}
        >
          <Skeleton variant="block" />
        </div>
      )}

      {isError && !isLoading && (
        <div role="alert" aria-live="assertive" className="pv-error-text">
          {m.proposal_list_error()}
        </div>
      )}

      {!isLoading && !isError && proposals.length === 0 && (
        <EmptyState
          title={m.proposal_list_empty_title()}
          description={m.proposal_list_empty_desc()}
        />
      )}

      {!isLoading && proposals.length > 0 && (
        <div
          role="grid"
          aria-label={m.proposal_list_aria({ count: proposals.length })}
          className="pv-proposal-list"
        >
          {proposals.map((p) => (
            <ProposalRow
              key={p.proposalId}
              proposal={p}
              selected={p.proposalId === selectedId}
              onClick={() => onSelect(p.proposalId)}
            />
          ))}
        </div>
      )}
    </section>
  );
}
