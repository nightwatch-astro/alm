// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// spec 007 — Calibration Matching settings pane.
//
// Authoritative design: platevault-settings-menu.html § [data-pane="calmatch"]
//
// Owned backend keys (CalibrationTolerances / UpdateCalibrationTolerances),
// persisted to the `calibration_tolerances` singleton table (migration 0008 +
// 0051) via `calibration.tolerances.get`/`update`:
//   - requireSameBinning  (boolean) — Binning "Match required" toggle
//   - requireSameGain     (boolean) — Gain "Match required" toggle
//   - requireSameOffset   (boolean) — Offset "Match required" toggle. Also
//     feeds `calibration_core::ranking::MatchingRuleConfig::require_same_offset`
//     (the dark/bias hard-rule the matching engine already enforces).
//   - temperatureToleranceC (number | null) — Sensor temp tolerance in °C
//   - agingLimitDays      (number) — Dark / bias age tolerance in days
import { useState, useEffect, useRef } from 'react';
import { Toggle, Pill } from '@/ui';
import { m } from '@/lib/i18n';
import { SettingsSection, RestoreDefaultsBtn } from './SettingsKit';

import {
  calibrationTolerancesGet,
  calibrationTolerancesUpdate,
} from './settingsIpc';
import type { UpdateCalibrationTolerances } from './settingsIpc';

interface CalibrationMatchingProps {
  /** Unused in this pane — tolerances use their own IPC commands. Kept for
   *  prop-shape consistency with sibling settings panes. */
  save: (scope: string, values: Record<string, unknown>) => void;
}

// Defaults per authoritative mock (platevault-settings-menu.html § calmatch).
const DEFAULTS = {
  requireSameBinning: true,
  requireSameGain: true,
  requireSameOffset: true,
  temperatureToleranceC: 5,
  agingLimitDays: 365,
  // Not surfaced in this pane's UI, but exposureToleranceS is a non-optional
  // f64 on the backend DTO (crates/contracts/core/calibration_tolerances.rs)
  // — matches migration 0008's column default. #639.
  exposureToleranceS: 2.0,
};

export function CalibrationMatching(_props: CalibrationMatchingProps) {
  // ── Hard-required field toggles ────────────────────────────────────────────
  const [requireBinning, setRequireBinning] = useState(
    DEFAULTS.requireSameBinning,
  );
  const [requireGain, setRequireGain] = useState(DEFAULTS.requireSameGain);
  const [requireOffset, setRequireOffset] = useState(
    DEFAULTS.requireSameOffset,
  );

  // ── Soft-tolerance inputs ──────────────────────────────────────────────────
  const [tempTolerance, setTempTolerance] = useState<number>(
    DEFAULTS.temperatureToleranceC,
  );
  const [agingLimit, setAgingLimit] = useState<number>(DEFAULTS.agingLimitDays);
  // Round-tripped but not editable in this pane (#639) — persist() must echo
  // back the real value; the backend rejects null for this non-optional field.
  const [exposureTolerance, setExposureTolerance] = useState<number>(
    DEFAULTS.exposureToleranceS,
  );

  // Guards against the initial calibrationTolerancesGet() fetch resolving
  // *after* the user has already edited a control — a real race (not just
  // CI timing): on a slower/more contended machine the mount fetch can still
  // be in flight when the user's first click/keystroke fires. Without this,
  // the late setState calls below stomp the user's optimistic edit back to
  // the stale fetched value (same class of bug as the Ingestion pane's
  // startup-fetch race).
  const editedRef = useRef(false);

  // ── Load persisted values from backend on mount ────────────────────────────
  useEffect(() => {
    // `cancelled` is a separate concern from `editedRef`: that guard protects a
    // user edit from being clobbered by a late fetch, this one stops the fetch
    // touching React state at all once the pane has unmounted.
    let cancelled = false;
    calibrationTolerancesGet()
      .then((tol) => {
        if (cancelled || editedRef.current) return;
        setRequireBinning(tol.requireSameBinning);
        setRequireGain(tol.requireSameGain);
        setRequireOffset(tol.requireSameOffset);
        if (tol.temperatureToleranceC !== null) {
          setTempTolerance(tol.temperatureToleranceC);
        }
        setAgingLimit(tol.agingLimitDays);
        if (tol.exposureToleranceS !== null) {
          setExposureTolerance(tol.exposureToleranceS);
        }
      })
      .catch(() => {
        // Backend unavailable in mock / dev mode — stay with in-code defaults.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // ── Persist a partial update; callers pass only the changed field ──────────
  function persist(patch: Partial<UpdateCalibrationTolerances>) {
    editedRef.current = true;
    const req: UpdateCalibrationTolerances = {
      requireSameBinning: requireBinning,
      requireSameGain: requireGain,
      requireSameOffset: requireOffset,
      temperatureToleranceC: tempTolerance,
      agingLimitDays: agingLimit,
      // Backend field is non-optional f64 — sending null fails deserialization
      // and every update in this pane was silently rejected (#639).
      exposureToleranceS: exposureTolerance,
      ...patch,
    };
    calibrationTolerancesUpdate(req).catch(() => {
      // Best-effort persist; UI already reflects the change.
    });
  }

  // ── Toggle handlers ────────────────────────────────────────────────────────
  function handleBinningToggle(val: boolean) {
    setRequireBinning(val);
    persist({ requireSameBinning: val });
  }
  function handleGainToggle(val: boolean) {
    setRequireGain(val);
    persist({ requireSameGain: val });
  }
  function handleOffsetToggle(val: boolean) {
    setRequireOffset(val);
    persist({ requireSameOffset: val });
  }

  // ── Tolerance input handlers ───────────────────────────────────────────────
  function handleTempChange(e: React.ChangeEvent<HTMLInputElement>) {
    const n = parseFloat(e.target.value);
    if (!Number.isFinite(n) || n < 0) return;
    setTempTolerance(n);
    persist({ temperatureToleranceC: n });
  }
  function handleAgingChange(e: React.ChangeEvent<HTMLInputElement>) {
    const n = parseInt(e.target.value, 10);
    if (!Number.isFinite(n) || n < 1) return;
    setAgingLimit(n);
    persist({ agingLimitDays: n });
  }

  // Restore defaults for THIS pane. Calibration tolerances live in their own
  // `calibrationTolerances` IPC store (not the settings table), so we reset the
  // visible fields to DEFAULTS and persist via that store — not
  // `settings.restore-defaults`, which would touch unrelated settings keys.
  const handleRestoreCalibration = async () => {
    editedRef.current = true;
    setRequireBinning(DEFAULTS.requireSameBinning);
    setRequireGain(DEFAULTS.requireSameGain);
    setRequireOffset(DEFAULTS.requireSameOffset);
    setTempTolerance(DEFAULTS.temperatureToleranceC);
    setAgingLimit(DEFAULTS.agingLimitDays);
    setExposureTolerance(DEFAULTS.exposureToleranceS);
    await calibrationTolerancesUpdate({
      requireSameBinning: DEFAULTS.requireSameBinning,
      requireSameGain: DEFAULTS.requireSameGain,
      requireSameOffset: DEFAULTS.requireSameOffset,
      temperatureToleranceC: DEFAULTS.temperatureToleranceC,
      agingLimitDays: DEFAULTS.agingLimitDays,
      exposureToleranceS: DEFAULTS.exposureToleranceS,
    });
  };

  return (
    <SettingsSection
      title={m.settings_calmatch_title()}
      action={
        <RestoreDefaultsBtn
          onRestore={handleRestoreCalibration}
          scopeLabel={m.settings_calmatch_restore_scope()}
        />
      }
    >
      <table className="pv-table pv-calmatch__table">
        <thead>
          <tr>
            <th>{m.settings_calmatch_field()}</th>
            <th className="pv-calmatch__col-required">
              {m.settings_calmatch_required()}
            </th>
            <th className="pv-calmatch__col-tolerance">
              {m.settings_calmatch_tolerance()}
            </th>
          </tr>
        </thead>
        <tbody>
          {/* Binning — hard toggle, persists to requireSameBinning */}
          <tr>
            <td>{m.settings_calmatch_binning()}</td>
            <td>
              <Toggle checked={requireBinning} onChange={handleBinningToggle} />
            </td>
            <td className="pv-cell--mono">{m.settings_calmatch_exact()}</td>
          </tr>

          {/* Gain — hard toggle, persists to requireSameGain */}
          <tr>
            <td>{m.settings_calmatch_gain()}</td>
            <td>
              <Toggle checked={requireGain} onChange={handleGainToggle} />
            </td>
            <td className="pv-cell--mono">{m.settings_calmatch_exact()}</td>
          </tr>

          {/* Offset — hard toggle, persists to requireSameOffset and feeds
              MatchingRuleConfig::require_same_offset in the matching engine */}
          <tr>
            <td>{m.settings_calmatch_offset()}</td>
            <td>
              <Toggle checked={requireOffset} onChange={handleOffsetToggle} />
            </td>
            <td className="pv-cell--mono">{m.settings_calmatch_exact()}</td>
          </tr>

          {/* Sensor temp — soft field: pill label + number input */}
          <tr>
            <td>{m.settings_calmatch_sensor_temp()}</td>
            <td>
              <Pill variant="neutral">{m.settings_calmatch_soft()}</Pill>
            </td>
            <td>
              <span className="pv-calmatch__tol-input-row">
                <input
                  type="number"
                  className="pv-input pv-calmatch__num-input"
                  value={tempTolerance}
                  min={0}
                  step={0.5}
                  onChange={handleTempChange}
                  aria-label={m.settings_calmatch_sensor_temp_aria()}
                />
                <span className="pv-calmatch__unit">
                  {m.settings_calmatch_unit_c()}
                </span>
              </span>
            </td>
          </tr>

          {/* Dark / bias age — warn-level soft field */}
          <tr>
            <td>{m.settings_calmatch_dark_bias_age()}</td>
            <td>
              <Pill variant="warn">{m.settings_calmatch_warn()}</Pill>
            </td>
            <td>
              <span className="pv-calmatch__tol-input-row">
                <input
                  type="number"
                  className="pv-input pv-calmatch__num-input"
                  value={agingLimit}
                  min={1}
                  max={3650}
                  onChange={handleAgingChange}
                  aria-label={m.settings_calmatch_dark_bias_age_aria()}
                />
                <span className="pv-calmatch__unit">
                  {m.settings_calmatch_unit_d()}
                </span>
              </span>
            </td>
          </tr>
        </tbody>
      </table>

      <p className="pv-calmatch__help">{m.settings_calmatch_help()}</p>
    </SettingsSection>
  );
}
