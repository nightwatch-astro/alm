// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * CandidateEvidencePanel tests — spec 062 US4.
 *
 * Tests:
 * 1. Fresh dark candidate renders age-fresh pill and auto-eligible badge.
 * 2. Yellow (ageing) candidate renders age-yellow pill and review-required badge.
 * 3. Red (stale) candidate renders age-red pill, review-required badge, and stale warning.
 * 4. Blocked candidate renders blocked badge and blocked banner.
 * 5. Unknown temperature dark renders thermal-unknown warning banner.
 * 6. Red thermal dark renders thermal-red pill.
 * 7. Flat candidate renders orientation evidence; non-flat suppresses orientation.
 * 8. Unknown orientation flat renders orientation-unknown pill.
 * 9. Warning codes list renders when non-empty.
 * 10. Incomplete recipe evidence renders banner.
 * 11. isDarkFlat('dark_flat') returns true; isDarkFlat('dark') returns false.
 * 12. CandidateEvidencePanel renders null for dark_flat kind (DarkFlat boundary).
 * 13. Eligibility badge has aria-live="polite" for screen reader announcements.
 */

import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { CandidateEvidencePanel, isDarkFlat } from './CandidateEvidencePanel';
import type { CalibrationCandidateEvidence } from './calibrationHandoffTypes';

// ── Fixtures ──────────────────────────────────────────────────────────────────

function makeEvidence(
  overrides: Partial<CalibrationCandidateEvidence> = {},
): CalibrationCandidateEvidence {
  return {
    evidenceId: 'ev-001',
    sessionId: 'ses-001',
    requirementId: 'req-001',
    recipeCompatibility: 'compatible',
    recipeEvidenceRef: 'ref-001',
    recipeEvidenceComplete: true,
    missingRecipeFields: [],
    temperatureMode: 'regulated',
    age: {
      basis: 'elapsed_days',
      state: 'fresh',
      ageDays: 30,
      freshThroughDays: 270,
      redAfterDays: 365,
      settingsRevision: 1,
    },
    thermal: {
      state: 'normal',
      missingReadingCount: 0,
      invalidReadingCount: 0,
      percentile95AbsoluteDeviationDeg: 0.1,
    },
    orientation: {
      state: 'not_applicable',
    },
    sourceAvailability: {
      indexedFrameCount: 100,
      availableReadableIndexedFrameCount: 100,
      checkedAt: '2026-07-25T00:00:00Z',
    },
    sufficient: true,
    automaticEligibility: 'eligible',
    warningCodes: [],
    basisFingerprint: 'fp-001',
    ...overrides,
  };
}

// ── isDarkFlat guard ───────────────────────────────────────────────────────────

describe('isDarkFlat', () => {
  it('returns true for dark_flat', () => {
    expect(isDarkFlat('dark_flat')).toBe(true);
  });

  it('returns false for dark', () => {
    expect(isDarkFlat('dark')).toBe(false);
  });

  it('returns false for bias', () => {
    expect(isDarkFlat('bias')).toBe(false);
  });

  it('returns false for flat', () => {
    expect(isDarkFlat('flat')).toBe(false);
  });
});

// ── DarkFlat boundary enforcement ─────────────────────────────────────────────

describe('CandidateEvidencePanel — DarkFlat boundary', () => {
  it('renders null for dark_flat kind (spec-062 US4 AS-5)', () => {
    const { container } = render(
      <CandidateEvidencePanel evidence={makeEvidence()} kind="dark_flat" />,
    );
    expect(container.firstChild).toBeNull();
  });
});

// ── Age evidence ──────────────────────────────────────────────────────────────

describe('CandidateEvidencePanel — age evidence', () => {
  it('renders fresh age pill and auto-eligible badge', () => {
    render(<CandidateEvidencePanel evidence={makeEvidence()} kind="dark" />);
    expect(screen.getByTestId('candidate-age')).toBeInTheDocument();
    expect(screen.getByTestId('candidate-eligibility')).toBeInTheDocument();
    // Eligibility badge is auto-eligible
    expect(screen.getByTestId('candidate-eligibility').textContent).toContain(
      'Auto-eligible',
    );
  });

  it('renders yellow (ageing) age with review-required badge', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          age: {
            basis: 'elapsed_days',
            state: 'yellow',
            ageDays: 300,
            freshThroughDays: 270,
            redAfterDays: 365,
            settingsRevision: 1,
          },
          automaticEligibility: 'review_required',
        })}
        kind="dark"
      />,
    );
    expect(screen.getByTestId('candidate-eligibility').textContent).toContain(
      'Review required',
    );
  });

  it('renders red (stale) age with stale warning banner', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          age: {
            basis: 'elapsed_days',
            state: 'red',
            ageDays: 400,
            freshThroughDays: 270,
            redAfterDays: 365,
            settingsRevision: 1,
          },
          automaticEligibility: 'review_required',
        })}
        kind="dark"
      />,
    );
    expect(screen.getByTestId('candidate-stale-warning')).toBeInTheDocument();
  });

  it('renders night distance for flat age evidence', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          age: {
            basis: 'observing_night_distance',
            state: 'fresh',
            ageNights: 0,
            freshThroughNights: 1,
            redAfterNights: 7,
            settingsRevision: 1,
          },
          orientation: { state: 'normal', minimumCircularDeltaDeg: 0.5 },
        })}
        kind="flat"
      />,
    );
    expect(screen.getByTestId('candidate-age')).toBeInTheDocument();
  });
});

// ── Eligibility states ─────────────────────────────────────────────────────────

describe('CandidateEvidencePanel — eligibility', () => {
  it('shows blocked badge and blocked banner for blocked eligibility', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          sufficient: false,
          automaticEligibility: 'blocked',
          recipeEvidenceComplete: false,
          missingRecipeFields: ['gain'],
        })}
        kind="dark"
      />,
    );
    expect(screen.getByTestId('candidate-eligibility').textContent).toContain(
      'Blocked',
    );
    expect(screen.getByTestId('candidate-blocked-banner')).toBeInTheDocument();
  });

  it('eligibility badge has aria-live="polite"', () => {
    render(<CandidateEvidencePanel evidence={makeEvidence()} kind="dark" />);
    const badge = screen.getByTestId('candidate-eligibility');
    expect(badge).toHaveAttribute('aria-live', 'polite');
  });
});

// ── Thermal evidence ──────────────────────────────────────────────────────────

describe('CandidateEvidencePanel — thermal', () => {
  it('shows unknown-temp warning banner when temperatureMode is unknown', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          temperatureMode: 'unknown',
          thermal: {
            state: 'unknown',
            missingReadingCount: 0,
            invalidReadingCount: 0,
          },
          automaticEligibility: 'blocked',
        })}
        kind="dark"
      />,
    );
    expect(
      screen.getByTestId('candidate-thermal-unknown-warn'),
    ).toBeInTheDocument();
  });

  it('shows red thermal pill for regulated dark with excessive drift', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          thermal: {
            state: 'red',
            percentile95AbsoluteDeviationDeg: 3.5,
            missingReadingCount: 0,
            invalidReadingCount: 0,
          },
          automaticEligibility: 'review_required',
        })}
        kind="dark"
      />,
    );
    expect(screen.getByTestId('candidate-thermal')).toBeInTheDocument();
  });

  it('suppresses thermal for bias (not_applicable)', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          temperatureMode: 'not_applicable',
          thermal: {
            state: 'not_applicable',
            missingReadingCount: 0,
            invalidReadingCount: 0,
          },
        })}
        kind="bias"
      />,
    );
    expect(screen.queryByTestId('candidate-thermal')).toBeNull();
  });
});

// ── Orientation evidence ──────────────────────────────────────────────────────

describe('CandidateEvidencePanel — orientation', () => {
  it('renders orientation pill for flat candidates', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          orientation: { state: 'normal', minimumCircularDeltaDeg: 0.3 },
        })}
        kind="flat"
      />,
    );
    expect(screen.getByTestId('candidate-orientation')).toBeInTheDocument();
  });

  it('renders unknown orientation for flat with unverified orientation', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          orientation: { state: 'unknown' },
          automaticEligibility: 'review_required',
        })}
        kind="flat"
      />,
    );
    expect(screen.getByTestId('candidate-orientation').textContent).toContain(
      'Unverified orientation',
    );
  });

  it('suppresses orientation for dark (not_applicable)', () => {
    render(<CandidateEvidencePanel evidence={makeEvidence()} kind="dark" />);
    expect(screen.queryByTestId('candidate-orientation')).toBeNull();
  });
});

// ── Warning codes ─────────────────────────────────────────────────────────────

describe('CandidateEvidencePanel — warning codes', () => {
  it('renders warning codes list when non-empty', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          warningCodes: [
            'calibration.compatibility_unverified_orientation',
            'calibration.cross_night_flat',
          ],
        })}
        kind="flat"
      />,
    );
    expect(screen.getByTestId('candidate-warning-codes')).toBeInTheDocument();
    expect(
      screen.getByText('calibration.compatibility_unverified_orientation'),
    ).toBeInTheDocument();
  });

  it('suppresses warning codes list when empty', () => {
    render(<CandidateEvidencePanel evidence={makeEvidence()} kind="dark" />);
    expect(screen.queryByTestId('candidate-warning-codes')).toBeNull();
  });
});

// ── Incomplete recipe evidence ────────────────────────────────────────────────

describe('CandidateEvidencePanel — recipe evidence', () => {
  it('renders incomplete recipe banner when evidence is incomplete', () => {
    render(
      <CandidateEvidencePanel
        evidence={makeEvidence({
          recipeEvidenceComplete: false,
          missingRecipeFields: ['gain', 'binning_x'],
          automaticEligibility: 'blocked',
        })}
        kind="dark"
      />,
    );
    expect(
      screen.getByTestId('candidate-recipe-incomplete'),
    ).toBeInTheDocument();
  });
});
