// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * CandidateEvidencePanel — spec 062 US4 (calibration candidate review).
 *
 * Renders the measured evidence for one CalibrationCandidateEvidence:
 *   - Age state (fresh / yellow / red / unknown) with day or night distance
 *   - Thermal state (dark only: normal / yellow / red / unknown)
 *   - Orientation state (flat only: normal / yellow / red / unknown)
 *   - Automatic eligibility badge (eligible / review_required / blocked)
 *   - Warning codes list
 *   - Unknown-temperature guard for darks without a cooling set point
 *
 * OWNER CONSTRAINTS (spec 062 epic bead, do not relax):
 *   - 365-day aging applies only to automatic reuse eligibility; beyond-ceiling
 *     candidates remain visible as historical and selectable with stale warning.
 *   - Observing night is a hard discriminator for flats.
 *   - dark_flat is excluded at classification — this component never renders
 *     for dark_flat frames (see `isDarkFlat` guard in calibrationHandoffTypes).
 *   - Temperature unknown → blocked until camera marked unregulated.
 *   - Do not invent values: display only what the backend supplied.
 *
 * Accessible: eligibility state uses `aria-live="polite"` so screen readers
 * announce changes when the candidate list is refreshed.
 */

import { AlertTriangle, CheckCircle, XCircle, HelpCircle } from 'lucide-react';
import { Pill, Banner, Section } from '@/ui';
import type { PillVariant } from '@/ui';
import { m } from '@/lib/i18n';
import type {
  CalibrationCandidateEvidence,
  AutomaticEligibility,
  CalibrationKind,
} from './calibrationHandoffTypes';

// ── DarkFlat boundary ─────────────────────────────────────────────────────────

/**
 * Returns true for the dark_flat frame kind.
 *
 * Dark-flat detection terminates before an Inbox item, session, match, plan,
 * or event in this contract (spec-062 US4 AS-5). This guard enforces the
 * unreachable DarkFlat product boundary: any surface rendering calibration
 * candidates must call this and render nothing when it returns true.
 * No reclassification or exclusion control is offered.
 */
export function isDarkFlat(kind: string): boolean {
  return kind === 'dark_flat';
}

// ── Evidence state helpers ────────────────────────────────────────────────────

type TrafficState =
  | 'fresh'
  | 'normal'
  | 'yellow'
  | 'red'
  | 'unknown'
  | 'not_applicable';

function stateVariant(state: TrafficState): PillVariant {
  switch (state) {
    case 'fresh':
    case 'normal':
      return 'ok';
    case 'yellow':
      return 'warn';
    case 'red':
      return 'danger';
    case 'unknown':
      return 'neutral';
    case 'not_applicable':
      return 'neutral';
  }
}

/** Inline icon element for an evidence state — avoids storing a component in a
 *  render variable (react-hooks/static-components). */
function StateIcon({ state, label }: { state: TrafficState; label: string }) {
  const props = {
    size: 12 as const,
    'aria-hidden': true as const,
    className: 'pv-candidate-evidence__state-icon',
    'aria-label': label,
  };
  if (state === 'fresh' || state === 'normal')
    return <CheckCircle {...props} />;
  if (state === 'red') return <XCircle {...props} />;
  if (state === 'not_applicable') return <HelpCircle {...props} />;
  return <AlertTriangle {...props} />;
}

// ── Age evidence ──────────────────────────────────────────────────────────────

function AgeLabel({
  evidence,
  kind,
}: {
  evidence: CalibrationCandidateEvidence;
  kind: string;
}) {
  const { age } = evidence;
  const state = age.state;

  const distanceLabel =
    age.basis === 'elapsed_days'
      ? age.ageDays !== undefined
        ? m.calibration_candidate_age_days({ days: String(age.ageDays) })
        : null
      : age.ageNights !== undefined
        ? m.calibration_candidate_age_nights({ nights: String(age.ageNights) })
        : null;

  const label =
    state === 'fresh'
      ? m.calibration_candidate_age_fresh()
      : state === 'yellow'
        ? m.calibration_candidate_age_yellow()
        : state === 'red'
          ? m.calibration_candidate_age_red()
          : m.calibration_candidate_age_unknown();

  return (
    <span className="pv-candidate-evidence__age" data-testid="candidate-age">
      <Pill variant={stateVariant(state)}>
        <StateIcon state={state} label={label} /> {label}
        {distanceLabel && (
          <span className="pv-candidate-evidence__age-distance">
            {' '}
            {distanceLabel}
          </span>
        )}
      </Pill>
    </span>
  );
}

// ── Thermal evidence (dark only) ──────────────────────────────────────────────

function ThermalLabel({
  evidence,
}: {
  evidence: CalibrationCandidateEvidence;
}) {
  const { thermal, temperatureMode } = evidence;

  if (
    temperatureMode === 'not_applicable' ||
    thermal.state === 'not_applicable'
  ) {
    return null;
  }

  if (temperatureMode === 'unknown') {
    return (
      <div
        className="pv-candidate-evidence__thermal"
        data-testid="candidate-thermal"
      >
        <Banner variant="warn" data-testid="candidate-thermal-unknown-warn">
          {m.calibration_candidate_unknown_temp_desc()}
        </Banner>
      </div>
    );
  }

  const state = thermal.state;
  const label =
    state === 'normal'
      ? m.calibration_candidate_thermal_normal()
      : state === 'yellow'
        ? m.calibration_candidate_thermal_yellow()
        : state === 'red'
          ? m.calibration_candidate_thermal_red()
          : m.calibration_candidate_thermal_unknown();

  return (
    <span
      className="pv-candidate-evidence__thermal"
      data-testid="candidate-thermal"
    >
      <Pill variant={stateVariant(state)}>
        <StateIcon state={state} label={label} /> {label}
        {thermal.percentile95AbsoluteDeviationDeg !== undefined && (
          <span className="pv-mono pv-candidate-evidence__thermal-val">
            {' '}
            {/* eslint-disable alm/no-user-string -- p95 deviation measurement, not translatable */}
            p95 Δ{thermal.percentile95AbsoluteDeviationDeg.toFixed(2)}°C
            {/* eslint-enable alm/no-user-string */}
          </span>
        )}
      </Pill>
    </span>
  );
}

// ── Orientation evidence (flat only) ─────────────────────────────────────────

function OrientationLabel({
  evidence,
}: {
  evidence: CalibrationCandidateEvidence;
}) {
  const { orientation } = evidence;

  if (orientation.state === 'not_applicable') {
    return null;
  }

  const state = orientation.state;
  const label =
    state === 'normal'
      ? m.calibration_candidate_orientation_normal()
      : state === 'yellow'
        ? m.calibration_candidate_orientation_yellow()
        : state === 'red'
          ? m.calibration_candidate_orientation_red()
          : m.calibration_candidate_orientation_unknown();

  return (
    <span
      className="pv-candidate-evidence__orientation"
      data-testid="candidate-orientation"
    >
      <Pill variant={stateVariant(state)}>
        <StateIcon state={state} label={label} /> {label}
        {orientation.minimumCircularDeltaDeg !== undefined && (
          <span className="pv-mono pv-candidate-evidence__orientation-val">
            {' '}
            {/* eslint-disable alm/no-user-string -- rotation delta measurement, not translatable */}
            Δ{orientation.minimumCircularDeltaDeg.toFixed(2)}°
            {/* eslint-enable alm/no-user-string */}
          </span>
        )}
      </Pill>
    </span>
  );
}

// ── Eligibility badge ─────────────────────────────────────────────────────────

function eligibilityVariant(eligibility: AutomaticEligibility): PillVariant {
  switch (eligibility) {
    case 'eligible':
      return 'ok';
    case 'review_required':
      return 'warn';
    case 'blocked':
      return 'danger';
  }
}

function eligibilityLabel(eligibility: AutomaticEligibility): string {
  switch (eligibility) {
    case 'eligible':
      return m.calibration_candidate_eligibility_eligible();
    case 'review_required':
      return m.calibration_candidate_eligibility_review();
    case 'blocked':
      return m.calibration_candidate_eligibility_blocked();
  }
}

// ── Warning codes ─────────────────────────────────────────────────────────────

function WarningCodes({ codes }: { codes: string[] }) {
  if (codes.length === 0) return null;
  return (
    <div
      className="pv-candidate-evidence__warnings"
      data-testid="candidate-warning-codes"
    >
      <span className="pv-candidate-evidence__warnings-label">
        {m.calibration_candidate_warning_codes_label()}
      </span>
      <ul className="pv-candidate-evidence__warnings-list">
        {codes.map((code) => (
          <li
            key={code}
            className="pv-candidate-evidence__warning-code pv-mono"
          >
            {code}
          </li>
        ))}
      </ul>
    </div>
  );
}

// ── Incomplete recipe evidence banner ────────────────────────────────────────

function RecipeIncompleteBanner({
  evidence,
}: {
  evidence: CalibrationCandidateEvidence;
}) {
  if (evidence.recipeEvidenceComplete) return null;
  if (evidence.missingRecipeFields.length === 0) return null;

  return (
    <Banner variant="warn" data-testid="candidate-recipe-incomplete">
      {m.calibration_candidate_recipe_incomplete({
        fields: evidence.missingRecipeFields.join(', '),
      })}
    </Banner>
  );
}

// ── Stale warning banner (yellow/red age, review_required eligibility) ────────

function StaleBanner({ evidence }: { evidence: CalibrationCandidateEvidence }) {
  const isStale =
    (evidence.age.state === 'yellow' || evidence.age.state === 'red') &&
    evidence.automaticEligibility === 'review_required';
  if (!isStale) return null;
  return (
    <Banner variant="warn" data-testid="candidate-stale-warning">
      {m.calibration_candidate_stale_warning()}
    </Banner>
  );
}

// ── Blocked banner ────────────────────────────────────────────────────────────

function BlockedBanner({
  evidence,
}: {
  evidence: CalibrationCandidateEvidence;
}) {
  if (evidence.automaticEligibility !== 'blocked') return null;
  const codes = [
    ...(!evidence.sufficient ? ['source.not_sufficient'] : []),
    ...(!evidence.recipeEvidenceComplete ? ['recipe.evidence_incomplete'] : []),
    ...(evidence.age.state === 'unknown' ? ['age.unknown'] : []),
    ...(evidence.temperatureMode === 'unknown' ? ['temperature.unknown'] : []),
    ...evidence.warningCodes.filter((c) => c.startsWith('calibration.')),
  ];
  return (
    <Banner variant="danger" data-testid="candidate-blocked-banner">
      {m.calibration_candidate_blocked_desc({
        codes: codes.length > 0 ? codes.join(', ') : 'unknown',
      })}
    </Banner>
  );
}

// ── Main panel ────────────────────────────────────────────────────────────────

export interface CandidateEvidencePanelProps {
  evidence: CalibrationCandidateEvidence;
  /**
   * The calibration kind for the requirement this candidate satisfies.
   * Accepts `string` so the DarkFlat boundary guard (`isDarkFlat`) can fire
   * even when the caller receives an unexpected kind from the backend.
   */
  kind: string;
}

/**
 * Render the candidate evidence summary for one calibration session.
 *
 * Returns null when `kind` is `dark_flat` — the DarkFlat product boundary
 * is enforced here and at the callers supplying the requirement kind.
 */
export function CandidateEvidencePanel({
  evidence,
  kind,
}: CandidateEvidencePanelProps) {
  // DarkFlat boundary — unreachable by product design (spec-062 US4 AS-5).
  if (isDarkFlat(kind)) return null;

  const eligibility = evidence.automaticEligibility;

  return (
    <Section
      title={evidence.sessionId}
      data-testid={`candidate-evidence-${evidence.evidenceId}`}
    >
      {/* Eligibility badge — live region so screen readers announce changes */}
      <div
        className="pv-candidate-evidence__eligibility"
        aria-live="polite"
        aria-label={eligibilityLabel(eligibility)}
        data-testid="candidate-eligibility"
      >
        <Pill variant={eligibilityVariant(eligibility)}>
          {eligibilityLabel(eligibility)}
        </Pill>
      </div>

      {/* Evidence row: age + thermal + orientation */}
      <div className="pv-candidate-evidence__evidence-row">
        <AgeLabel evidence={evidence} kind={kind} />
        <ThermalLabel evidence={evidence} />
        <OrientationLabel evidence={evidence} />
      </div>

      {/* Banners: stale, blocked, recipe-incomplete, unknown-temp */}
      <StaleBanner evidence={evidence} />
      <BlockedBanner evidence={evidence} />
      <RecipeIncompleteBanner evidence={evidence} />

      {/* Warning codes */}
      <WarningCodes codes={evidence.warningCodes} />
    </Section>
  );
}
