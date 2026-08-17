// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ProposalEvidencePanel — ordered evidence disclosure for one relation proposal.
 *
 * SC-007: every automatic suggestion must expose its measured evidence and the
 * active thresholds that caused the suggestion. This panel renders:
 *   - footprint coverage, centre separation, residual rotation with pass/fail
 *   - parity, acquisition geometry, equipment compatibility
 *   - missing evidence codes (with text + icon per FR-094)
 *   - threshold measurements table
 *
 * Accessibility (FR-094, FR-095): severity is conveyed by text + icon, not
 * colour alone. The panel is a section landmark with a heading so keyboard
 * users can jump to it.
 */

import { m } from '@/lib/i18n';
import { KV } from '@/ui';
import { EvidenceSeverityPill } from './EvidenceSeverityPill';
import type { RelationEvidence, ThresholdMeasurement } from './groupsTypes';

function formatPercent(v: number | undefined): string {
  return v != null ? `${v.toFixed(2)}%` : m.evidence_missing_value();
}

function formatDeg(v: number | undefined): string {
  return v != null ? `${v.toFixed(3)}°` : m.evidence_missing_value();
}

type TernaryInput =
  | 'match'
  | 'mismatch'
  | 'unknown'
  | 'compatible'
  | 'incompatible';

function ternaryLabel(v: TernaryInput): {
  label: string;
  severity: 'ok' | 'yellow' | 'red' | 'missing';
} {
  switch (v) {
    case 'match':
    case 'compatible':
      return { label: m.evidence_compatible(), severity: 'ok' };
    case 'mismatch':
    case 'incompatible':
      return { label: m.evidence_incompatible(), severity: 'red' };
    case 'unknown':
      return { label: m.evidence_unknown(), severity: 'missing' };
  }
}

function ThresholdRow({ t }: { t: ThresholdMeasurement }) {
  const pass = t.outcome === 'pass';
  return (
    <tr
      className={`pv-evidence-row pv-evidence-row--${pass ? 'pass' : 'fail'}`}
    >
      <td className="pv-evidence-row__key">{t.key}</td>
      <td className="pv-evidence-row__measured">
        {t.measuredValue.toFixed(3)} {t.unit}
      </td>
      <td className="pv-evidence-row__threshold">
        {t.comparison} {t.thresholdValue.toFixed(3)} {t.unit}
      </td>
      <td className="pv-evidence-row__outcome">
        <EvidenceSeverityPill severity={pass ? 'ok' : 'red'} />
      </td>
    </tr>
  );
}

export interface ProposalEvidencePanelProps {
  evidence: RelationEvidence;
}

export function ProposalEvidencePanel({
  evidence,
}: ProposalEvidencePanelProps) {
  const parityInfo = ternaryLabel(evidence.parity);
  const geometryInfo = ternaryLabel(evidence.acquisitionGeometry);
  const equipmentInfo = ternaryLabel(evidence.equipment);

  return (
    <section
      aria-label={m.proposal_evidence_heading()}
      className="pv-evidence-panel"
      data-testid="proposal-evidence-panel"
    >
      <h3 className="pv-section__heading">{m.proposal_evidence_heading()}</h3>

      <dl className="pv-kv-list">
        <KV
          label={m.evidence_coverage_label()}
          value={formatPercent(evidence.footprintCoveragePercent)}
        />
        <KV
          label={m.evidence_center_sep_label()}
          value={formatPercent(evidence.centerSeparationPercent)}
        />
        <KV
          label={m.evidence_rotation_label()}
          value={formatDeg(evidence.residualSkyRotationDeg)}
        />
        <KV
          label={m.evidence_parity_label()}
          value={
            <EvidenceSeverityPill
              severity={parityInfo.severity}
              detail={parityInfo.label}
            />
          }
        />
        <KV
          label={m.evidence_geometry_label()}
          value={
            <EvidenceSeverityPill
              severity={geometryInfo.severity}
              detail={geometryInfo.label}
            />
          }
        />
        <KV
          label={m.evidence_equipment_label()}
          value={
            <EvidenceSeverityPill
              severity={equipmentInfo.severity}
              detail={equipmentInfo.label}
            />
          }
        />
      </dl>

      {evidence.missingEvidenceCodes.length > 0 && (
        <div
          className="pv-evidence-missing"
          role="status"
          aria-live="polite"
          aria-label={m.evidence_missing_codes_aria({
            count: evidence.missingEvidenceCodes.length,
          })}
        >
          <EvidenceSeverityPill severity="missing" />
          <ul
            className="pv-evidence-missing__list"
            aria-label={m.evidence_missing_codes_list_aria()}
          >
            {evidence.missingEvidenceCodes.map((code) => (
              <li key={code} className="pv-evidence-missing__item">
                <code>{code}</code>
              </li>
            ))}
          </ul>
        </div>
      )}

      {evidence.thresholdSnapshot.length > 0 && (
        <div className="pv-evidence-thresholds">
          <h4 className="pv-section__subheading">
            {m.evidence_thresholds_heading()}
          </h4>
          <table
            className="pv-evidence-table"
            aria-label={m.evidence_thresholds_aria()}
          >
            <thead>
              <tr>
                <th scope="col">{m.evidence_col_key()}</th>
                <th scope="col">{m.evidence_col_measured()}</th>
                <th scope="col">{m.evidence_col_threshold()}</th>
                <th scope="col">{m.evidence_col_outcome()}</th>
              </tr>
            </thead>
            <tbody>
              {evidence.thresholdSnapshot.map((t) => (
                <ThresholdRow key={t.key} t={t} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
