// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * MatchingSettingsPanel — guarded matching geometry and calibration settings.
 *
 * Spec 062 contracts/matching-settings.md. Key constraints:
 *
 * FR-027/028: hard ranges per field; yellow-warning thresholds.
 * FR-029: sibling config must not be stricter than same-session config.
 * FR-030: risky-but-valid = yellow pill; out-of-bounds = red + unsaveable.
 * FR-031: saves affect future suggestions only.
 *
 * The save flow:
 *   1. User edits a field → immediate client-side severity feedback.
 *   2. On "Save", validate against server (matching_settings.validate).
 *   3. If yellow issues exist, show them with acknowledgement checkboxes.
 *   4. User acknowledges all yellow issues, then confirms.
 *   5. Call matching_settings.update with acknowledgedWarningCodes.
 *
 * FR-095: modal focus management for the acknowledgement dialog.
 * FR-094: severity via text + icon, not colour alone.
 */

import { useState, useEffect, useRef } from 'react';
import { Modal } from '@/components';
import { Btn, Section, Banner, NumberField } from '@/ui';
import { m } from '@/lib/i18n';
import {
  useMatchingSettings,
  useMatchingSettingsUpdate,
  validateFieldSeverity,
  MATCHING_SETTINGS_BOUNDS,
} from './useGroupsStore';
import { matchingSettingsValidate } from './sessionsGroupsIpc';
import { EvidenceSeverityPill } from './EvidenceSeverityPill';
import type {
  MatchingSettings,
  SettingsIssue,
  SettingsValidation,
} from './groupsTypes';

// ── Field state helpers ────────────────────────────────────────────────────────

interface FieldState {
  raw: string;
  value: number | null;
}

function parseField(raw: string): number | null {
  const n = parseFloat(raw);
  return Number.isNaN(n) ? null : n;
}

function fieldState(initial: number): FieldState {
  return { raw: String(initial), value: initial };
}

// ── Single settings row ────────────────────────────────────────────────────────

function SettingsRow({
  id,
  label,
  state,
  onChange,
  bounds,
  min,
  step,
}: {
  id: string;
  label: string;
  state: FieldState;
  onChange: (s: FieldState) => void;
  bounds: {
    min: number;
    max: number;
    yellowBelow?: number;
    yellowAbove?: number;
  };
  min?: number;
  step?: number | 'any';
}) {
  const severity =
    state.value != null ? validateFieldSeverity(state.value, bounds) : 'red';

  return (
    <div className="pv-settings-row" data-testid={`settings-row-${id}`}>
      <NumberField
        id={id}
        label={label}
        value={state.raw}
        onChange={(raw) => onChange({ raw, value: parseField(raw) })}
        min={min ?? bounds.min}
        step={step ?? 'any'}
        hint={`${bounds.min}–${bounds.max}`}
      />
      {severity !== 'ok' && (
        <EvidenceSeverityPill
          severity={severity}
          detail={
            severity === 'red'
              ? m.settings_out_of_bounds()
              : m.settings_yellow_warning()
          }
        />
      )}
    </div>
  );
}

// ── Acknowledge modal ─────────────────────────────────────────────────────────

interface AcknowledgeModalProps {
  open: boolean;
  issues: SettingsIssue[];
  onConfirm: (codes: string[]) => void;
  onClose: () => void;
  busy: boolean;
}

function AcknowledgeModal({
  open,
  issues,
  onConfirm,
  onClose,
  busy,
}: AcknowledgeModalProps) {
  const [acknowledged, setAcknowledged] = useState<Set<string>>(new Set());
  const headingRef = useRef<HTMLHeadingElement | null>(null);

  const yellowIssues = issues.filter((i) => i.severity === 'yellow');
  const allAcknowledged = yellowIssues.every((i) => acknowledged.has(i.code));

  const toggle = (code: string, checked: boolean) => {
    const next = new Set(acknowledged);
    if (checked) next.add(code);
    else next.delete(code);
    setAcknowledged(next);
  };

  const handleConfirm = () => {
    onConfirm([...acknowledged]);
    setAcknowledged(new Set());
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={m.settings_acknowledge_title()}
      initialFocus={headingRef}
      size="md"
      data-testid="settings-acknowledge-modal"
      footer={
        <div className="pv-modal__actions">
          <Btn variant="ghost" onClick={onClose} disabled={busy}>
            {m.common_cancel()}
          </Btn>
          <Btn
            variant="primary"
            onClick={handleConfirm}
            disabled={!allAcknowledged || busy}
            data-testid="settings-acknowledge-confirm"
          >
            {busy ? m.common_saving() : m.settings_acknowledge_btn()}
          </Btn>
        </div>
      }
    >
      <h2 ref={headingRef} className="pv-modal__section-heading" tabIndex={-1}>
        {m.settings_acknowledge_title()}
      </h2>
      <p>{m.settings_acknowledge_desc()}</p>
      <ul className="pv-settings-issues" aria-label={m.settings_issues_aria()}>
        {yellowIssues.map((issue) => (
          <li key={issue.code} className="pv-settings-issue">
            <label className="pv-check-label">
              <input
                type="checkbox"
                checked={acknowledged.has(issue.code)}
                onChange={(e) => toggle(issue.code, e.target.checked)}
                data-testid={`ack-${issue.code}`}
                aria-label={issue.code}
              />{' '}
              <EvidenceSeverityPill
                severity="yellow"
                detail={issue.messageKey}
              />
              <span className="pv-settings-issue__fields">
                {issue.fieldPaths.join(', ')}
              </span>
            </label>
          </li>
        ))}
      </ul>
    </Modal>
  );
}

// ── Main component ─────────────────────────────────────────────────────────────

export function MatchingSettingsPanel() {
  const { data: settings, isLoading, isError } = useMatchingSettings();
  const updateMutation = useMatchingSettingsUpdate();

  // ── Form state ───────────────────────────────────────────────────────────────
  const [ssCoverage, setSsCoverage] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [ssCenter, setSsCenter] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [ssRotation, setSsRotation] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [sibCoverage, setSibCoverage] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [sibCenter, setSibCenter] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [sibRotation, setSibRotation] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [mosaicMin, setMosaicMin] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [mosaicMax, setMosaicMax] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [darkModerate, setDarkModerate] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [darkSevere, setDarkSevere] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [flatOrientNormal, setFlatOrientNormal] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [flatOrientRed, setFlatOrientRed] = useState<FieldState>({
    raw: '',
    value: null,
  });
  const [flatAgeRed, setFlatAgeRed] = useState<FieldState>({
    raw: '',
    value: null,
  });

  const [showAcknowledge, setShowAcknowledge] = useState(false);
  const [pendingIssues, setPendingIssues] = useState<SettingsIssue[]>([]);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState(false);

  // Populate form when settings load
  useEffect(() => {
    if (!settings) return;
    setSsCoverage(fieldState(settings.sameSession.coverageMinPercent));
    setSsCenter(fieldState(settings.sameSession.centerSeparationMaxPercent));
    setSsRotation(fieldState(settings.sameSession.rotationMaxDeg));
    setSibCoverage(fieldState(settings.sibling.coverageMinPercent));
    setSibCenter(fieldState(settings.sibling.centerSeparationMaxPercent));
    setSibRotation(fieldState(settings.sibling.rotationMaxDeg));
    setMosaicMin(fieldState(settings.mosaic.overlapMinPercent));
    setMosaicMax(fieldState(settings.mosaic.overlapMaxPercent));
    setDarkModerate(fieldState(settings.darkThermal.moderateDeg));
    setDarkSevere(fieldState(settings.darkThermal.severeDeg));
    setFlatOrientNormal(fieldState(settings.flatOrientation.normalThroughDeg));
    setFlatOrientRed(fieldState(settings.flatOrientation.redAboveDeg));
    setFlatAgeRed(fieldState(settings.flatAge.redAfterNights));
  }, [settings]);

  function buildPatch(): Partial<MatchingSettings> {
    return {
      sameSession: {
        coverageMinPercent: ssCoverage.value!,
        centerSeparationMaxPercent: ssCenter.value!,
        rotationMaxDeg: ssRotation.value!,
      },
      sibling: {
        coverageMinPercent: sibCoverage.value!,
        centerSeparationMaxPercent: sibCenter.value!,
        rotationMaxDeg: sibRotation.value!,
      },
      mosaic: {
        overlapMinPercent: mosaicMin.value!,
        overlapMaxPercent: mosaicMax.value!,
        residualSkyRotationCapDeg:
          settings?.mosaic.residualSkyRotationCapDeg ?? 10,
      },
      darkThermal: {
        moderateDeg: darkModerate.value!,
        severeDeg: darkSevere.value!,
      },
      flatOrientation: {
        normalThroughDeg: flatOrientNormal.value!,
        redAboveDeg: flatOrientRed.value!,
      },
      flatAge: {
        redAfterNights: flatAgeRed.value!,
      },
    };
  }

  const allFieldsValid = [
    ssCoverage,
    ssCenter,
    ssRotation,
    sibCoverage,
    sibCenter,
    sibRotation,
    mosaicMin,
    mosaicMax,
    darkModerate,
    darkSevere,
    flatOrientNormal,
    flatOrientRed,
    flatAgeRed,
  ].every((f) => f.value != null);

  const hasOutOfBounds =
    (ssCoverage.value != null &&
      validateFieldSeverity(
        ssCoverage.value,
        MATCHING_SETTINGS_BOUNDS.sameSession.coverageMinPercent,
      ) === 'red') ||
    (ssCenter.value != null &&
      validateFieldSeverity(
        ssCenter.value,
        MATCHING_SETTINGS_BOUNDS.sameSession.centerSeparationMaxPercent,
      ) === 'red') ||
    (ssRotation.value != null &&
      validateFieldSeverity(
        ssRotation.value,
        MATCHING_SETTINGS_BOUNDS.sameSession.rotationMaxDeg,
      ) === 'red') ||
    (sibCoverage.value != null &&
      validateFieldSeverity(
        sibCoverage.value,
        MATCHING_SETTINGS_BOUNDS.sibling.coverageMinPercent,
      ) === 'red') ||
    (sibCenter.value != null &&
      validateFieldSeverity(
        sibCenter.value,
        MATCHING_SETTINGS_BOUNDS.sibling.centerSeparationMaxPercent,
      ) === 'red') ||
    (sibRotation.value != null &&
      validateFieldSeverity(
        sibRotation.value,
        MATCHING_SETTINGS_BOUNDS.sibling.rotationMaxDeg,
      ) === 'red') ||
    (mosaicMin.value != null &&
      validateFieldSeverity(
        mosaicMin.value,
        MATCHING_SETTINGS_BOUNDS.mosaic.overlapMinPercent,
      ) === 'red') ||
    (mosaicMax.value != null &&
      validateFieldSeverity(
        mosaicMax.value,
        MATCHING_SETTINGS_BOUNDS.mosaic.overlapMaxPercent,
      ) === 'red') ||
    (darkModerate.value != null &&
      validateFieldSeverity(
        darkModerate.value,
        MATCHING_SETTINGS_BOUNDS.darkThermal.moderateDeg,
      ) === 'red') ||
    (darkSevere.value != null &&
      validateFieldSeverity(
        darkSevere.value,
        MATCHING_SETTINGS_BOUNDS.darkThermal.severeDeg,
      ) === 'red');

  const canSave = allFieldsValid && !hasOutOfBounds;

  const handleSave = async () => {
    if (!settings || !canSave) return;
    setSaveError(null);
    setSaveSuccess(false);

    let validation: SettingsValidation;
    try {
      validation = await matchingSettingsValidate({
        baseRevision: settings.revision,
        patch: buildPatch(),
      });
    } catch {
      setSaveError(m.settings_validate_error());
      return;
    }

    if (!validation.valid) {
      setSaveError(m.settings_has_red_issues());
      return;
    }

    const yellowIssues = validation.issues.filter(
      (i) => i.severity === 'yellow',
    );

    if (yellowIssues.length > 0) {
      setPendingIssues(validation.issues);
      setShowAcknowledge(true);
      return;
    }

    await doSave([]);
  };

  const doSave = async (acknowledgedWarningCodes: string[]) => {
    if (!settings) return;
    try {
      await updateMutation.mutateAsync({
        expectedRevision: settings.revision,
        patch: buildPatch(),
        acknowledgedWarningCodes,
        mutationContext: { commandId: crypto.randomUUID() },
      });
      setSaveSuccess(true);
      setShowAcknowledge(false);
    } catch {
      setSaveError(m.settings_save_error());
    }
  };

  if (isLoading) {
    return (
      <div
        role="status"
        aria-live="polite"
        data-testid="matching-settings-panel"
      >
        {m.settings_loading()}
      </div>
    );
  }

  if (isError || !settings) {
    return (
      <div role="alert" aria-live="assertive" className="pv-error-text">
        {m.settings_load_error()}
      </div>
    );
  }

  const b = MATCHING_SETTINGS_BOUNDS;

  return (
    <article
      className="pv-matching-settings"
      aria-label={m.settings_matching_heading()}
      data-testid="matching-settings-panel"
    >
      <h2 className="pv-section__heading">{m.settings_matching_heading()}</h2>

      <Banner variant="info" aria-live="polite">
        {m.settings_future_only_notice()}
      </Banner>

      {saveError && (
        <div role="alert" aria-live="assertive" className="pv-error-text">
          {saveError}
        </div>
      )}

      {saveSuccess && (
        <div role="status" aria-live="polite" className="pv-success-text">
          {m.settings_save_success()}
        </div>
      )}

      {/* Same-session geometry (FR-027) */}
      <Section title={m.settings_same_session_heading()}>
        <SettingsRow
          id="ss-coverage"
          label={m.settings_coverage_label()}
          state={ssCoverage}
          onChange={setSsCoverage}
          bounds={b.sameSession.coverageMinPercent}
        />
        <SettingsRow
          id="ss-center"
          label={m.settings_center_sep_label()}
          state={ssCenter}
          onChange={setSsCenter}
          bounds={b.sameSession.centerSeparationMaxPercent}
        />
        <SettingsRow
          id="ss-rotation"
          label={m.settings_rotation_label()}
          state={ssRotation}
          onChange={setSsRotation}
          bounds={b.sameSession.rotationMaxDeg}
        />
      </Section>

      {/* Sibling geometry (FR-028/029) */}
      <Section title={m.settings_sibling_heading()}>
        <SettingsRow
          id="sib-coverage"
          label={m.settings_coverage_label()}
          state={sibCoverage}
          onChange={setSibCoverage}
          bounds={b.sibling.coverageMinPercent}
        />
        <SettingsRow
          id="sib-center"
          label={m.settings_center_sep_label()}
          state={sibCenter}
          onChange={setSibCenter}
          bounds={b.sibling.centerSeparationMaxPercent}
        />
        <SettingsRow
          id="sib-rotation"
          label={m.settings_rotation_label()}
          state={sibRotation}
          onChange={setSibRotation}
          bounds={b.sibling.rotationMaxDeg}
        />
      </Section>

      {/* Mosaic overlap (FR-034) */}
      <Section title={m.settings_mosaic_heading()}>
        <SettingsRow
          id="mosaic-min"
          label={m.settings_mosaic_min_label()}
          state={mosaicMin}
          onChange={setMosaicMin}
          bounds={b.mosaic.overlapMinPercent}
        />
        <SettingsRow
          id="mosaic-max"
          label={m.settings_mosaic_max_label()}
          state={mosaicMax}
          onChange={setMosaicMax}
          bounds={b.mosaic.overlapMaxPercent}
        />
      </Section>

      {/* Dark thermal (FR-088) */}
      <Section title={m.settings_dark_thermal_heading()}>
        <SettingsRow
          id="dark-moderate"
          label={m.settings_dark_moderate_label()}
          state={darkModerate}
          onChange={setDarkModerate}
          bounds={b.darkThermal.moderateDeg}
        />
        <SettingsRow
          id="dark-severe"
          label={m.settings_dark_severe_label()}
          state={darkSevere}
          onChange={setDarkSevere}
          bounds={b.darkThermal.severeDeg}
        />
      </Section>

      {/* Flat orientation (FR-073) */}
      <Section title={m.settings_flat_orient_heading()}>
        <SettingsRow
          id="flat-orient-normal"
          label={m.settings_flat_orient_normal_label()}
          state={flatOrientNormal}
          onChange={setFlatOrientNormal}
          bounds={b.flatOrientation.normalThroughDeg}
        />
        <SettingsRow
          id="flat-orient-red"
          label={m.settings_flat_orient_red_label()}
          state={flatOrientRed}
          onChange={setFlatOrientRed}
          bounds={b.flatOrientation.redAboveDeg}
        />
      </Section>

      {/* Flat age (FR-074) */}
      <Section title={m.settings_flat_age_heading()}>
        <SettingsRow
          id="flat-age-red"
          label={m.settings_flat_age_red_label()}
          state={flatAgeRed}
          onChange={setFlatAgeRed}
          bounds={b.flatAge.redAfterNights}
          step={1}
        />
      </Section>

      <div className="pv-settings-actions">
        <Btn
          variant="primary"
          onClick={() => void handleSave()}
          disabled={!canSave || updateMutation.isPending}
          aria-disabled={!canSave}
          data-testid="settings-save-btn"
        >
          {updateMutation.isPending ? m.common_saving() : m.common_save()}
        </Btn>
      </div>

      {/* Acknowledgement modal — FR-095 focus management */}
      <AcknowledgeModal
        open={showAcknowledge}
        issues={pendingIssues}
        onConfirm={(codes) => void doSave(codes)}
        onClose={() => setShowAcknowledge(false)}
        busy={updateMutation.isPending}
      />
    </article>
  );
}
