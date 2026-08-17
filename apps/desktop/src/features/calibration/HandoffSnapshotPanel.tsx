// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * HandoffSnapshotPanel — spec 062 US4 (calibration handoff snapshot summary).
 *
 * Renders the summary for a CalibrationHandoffSnapshot:
 *   - Counts: requirementCount, selectionCount, frameCount
 *   - Warning codes as pills (each code is opaque to the UI; the backend owns
 *     well-known codes like `calibration.no_automatic_candidate`)
 *   - "Add reviewed session" entry point per requirement (disabled when snapshot
 *     is not the handoff head — stale snapshot guard)
 *
 * The add-reviewed path opens a form collecting:
 *   - sessionId (caller supplies)
 *   - decisionReason (non-empty, per contract)
 *   - acknowledgedWarningCodes (every warning code on the candidate evidence
 *     must be acknowledged before the form can submit)
 *
 * This component renders the summary and the add-reviewed prompt only.
 * The actual mutation is driven by the parent via `onReviewedAdd`.
 */

import { Pill, Section, Banner } from '@/ui';
import { m } from '@/lib/i18n';
import type { CalibrationHandoffSnapshot } from './calibrationHandoffTypes';

// ── Warning code pills ─────────────────────────────────────────────────────────

function WarningCodePills({ codes }: { codes: string[] }) {
  if (codes.length === 0) return null;

  return (
    <div
      className="pv-handoff-snapshot__warnings"
      data-testid="handoff-warning-codes"
    >
      <span className="pv-handoff-snapshot__warnings-label">
        {m.calibration_handoff_warning_codes_label()}
      </span>
      <div className="pv-handoff-snapshot__warning-pills">
        {codes.map((code) => (
          <Pill
            key={code}
            variant="warn"
            className="pv-mono pv-handoff-snapshot__warning-pill"
            data-testid={`handoff-warning-${code}`}
          >
            {code}
          </Pill>
        ))}
      </div>
    </div>
  );
}

// ── Counts row ────────────────────────────────────────────────────────────────

function CountsRow({ snapshot }: { snapshot: CalibrationHandoffSnapshot }) {
  return (
    <dl className="pv-handoff-snapshot__counts" data-testid="handoff-counts">
      <div className="pv-handoff-snapshot__count">
        <dt className="pv-handoff-snapshot__count-label">
          {m.calibration_handoff_summary_requirements()}
        </dt>
        <dd
          className="pv-handoff-snapshot__count-value pv-mono"
          data-testid="handoff-requirement-count"
        >
          {snapshot.requirementCount}
        </dd>
      </div>
      <div className="pv-handoff-snapshot__count">
        <dt className="pv-handoff-snapshot__count-label">
          {m.calibration_handoff_summary_selections()}
        </dt>
        <dd
          className="pv-handoff-snapshot__count-value pv-mono"
          data-testid="handoff-selection-count"
        >
          {snapshot.selectionCount}
        </dd>
      </div>
      <div className="pv-handoff-snapshot__count">
        <dt className="pv-handoff-snapshot__count-label">
          {m.calibration_handoff_summary_frames()}
        </dt>
        <dd
          className="pv-handoff-snapshot__count-value pv-mono"
          data-testid="handoff-frame-count"
        >
          {snapshot.frameCount}
        </dd>
      </div>
    </dl>
  );
}

// ── No-automatic-candidate banner ──────────────────────────────────────────────

/** Surfaces the well-known `calibration.no_automatic_candidate` warning. */
function NoAutoCandidateBanner({
  snapshot,
}: {
  snapshot: CalibrationHandoffSnapshot;
}) {
  if (!snapshot.warningCodes.includes('calibration.no_automatic_candidate')) {
    return null;
  }
  // Add-reviewed path is surfaced via the parent; this banner is informational.
  return (
    <Banner variant="warn" data-testid="handoff-no-auto-candidate">
      {m.calibration_handoff_no_auto_candidate()}
    </Banner>
  );
}

// ── Main panel ────────────────────────────────────────────────────────────────

export interface HandoffSnapshotPanelProps {
  snapshot: CalibrationHandoffSnapshot;
  /**
   * Called when the user opens the "add reviewed session" flow.
   * The parent is responsible for collecting sessionId / decisionReason /
   * acknowledgedWarningCodes and invoking `calibration.handoff.reviewed_add`.
   */
  onAddReviewed?: () => void;
  /** Whether this snapshot is the current handoff head — disables add when false. */
  isHead: boolean;
}

export function HandoffSnapshotPanel({
  snapshot,
  onAddReviewed,
  isHead,
}: HandoffSnapshotPanelProps) {
  return (
    <Section
      title={snapshot.snapshotId}
      data-testid={`handoff-snapshot-${snapshot.snapshotId}`}
    >
      <CountsRow snapshot={snapshot} />
      <NoAutoCandidateBanner snapshot={snapshot} />
      <WarningCodePills
        codes={snapshot.warningCodes.filter(
          (c) => c !== 'calibration.no_automatic_candidate',
        )}
      />
      {onAddReviewed && (
        <div
          className="pv-handoff-snapshot__add-reviewed"
          data-testid="handoff-add-reviewed-row"
        >
          <button
            type="button"
            className="pv-btn pv-btn--sm pv-btn--ghost"
            onClick={onAddReviewed}
            disabled={!isHead}
            title={m.calibration_handoff_add_reviewed_title()}
            aria-disabled={!isHead}
            data-testid="handoff-add-reviewed-btn"
          >
            {m.calibration_handoff_add_reviewed_btn()}
          </button>
        </div>
      )}
    </Section>
  );
}
