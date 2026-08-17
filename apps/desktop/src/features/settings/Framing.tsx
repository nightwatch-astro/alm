// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Framing — Settings → Framing pane (spec 008 Q27 F-Framing-11, R11a).
 *
 * Surfaces the four clustering tolerance tunables that group a project's
 * light sessions into framings (`crates/sessions::ToleranceParams`) and the
 * inbox-confirm attribution pass's mosaic candidate envelope
 * (`crates/app/inbox::attribution`, FR-019). Persisted through the generic
 * `"framing"` settings scope; R11a's shipped defaults are the backend
 * defaults, so this pane never needs to duplicate them beyond the displayed
 * fallback values below.
 */
import { useState, useEffect, useRef } from 'react';
import { m } from '@/lib/i18n';
import { getSettingsTyped, parseSettingsValues } from './settingsIpc';
import { FramingSettingsSchema, type FramingSettings } from './settingsSchemas';
import {
  SettingsSection,
  SettingsRow,
  RestoreDefaultsBtn,
} from './SettingsKit';

const FRAMING_SCOPE = 'framing';
const FRAMING_KEYS = [
  'framingPointingFractionOfFov',
  'framingPointingFallbackDeg',
  'framingRotationToleranceDeg',
  'framingMosaicEnvelopeFractionOfFov',
];

// R11a shipped defaults (`domain_core::settings::SettingsState::default()`)
// — used only as the initial render before the backend fetch resolves.
const DEFAULTS = {
  pointingFractionOfFov: 0.1,
  pointingFallbackDeg: 0.2,
  rotationToleranceDeg: 3.0,
  mosaicEnvelopeFractionOfFov: 1.0,
};

// Mirrors the backend descriptor bounds (`crates/app/settings/src/descriptors.rs`).
const POINTING_FRACTION_MIN = 0.01;
const POINTING_FRACTION_MAX = 2.0;
const POINTING_FALLBACK_MIN = 0.01;
const POINTING_FALLBACK_MAX = 10.0;
const ROTATION_MIN = 0.1;
const ROTATION_MAX = 45.0;
const MOSAIC_ENVELOPE_MIN = 0.1;
const MOSAIC_ENVELOPE_MAX = 5.0;

interface FramingProps {
  save: (scope: string, values: Record<string, unknown>) => void;
}

export function Framing({ save }: FramingProps) {
  const [pointingFraction, setPointingFraction] = useState(
    DEFAULTS.pointingFractionOfFov,
  );
  const [pointingFallback, setPointingFallback] = useState(
    DEFAULTS.pointingFallbackDeg,
  );
  const [rotationTolerance, setRotationTolerance] = useState(
    DEFAULTS.rotationToleranceDeg,
  );
  const [mosaicEnvelope, setMosaicEnvelope] = useState(
    DEFAULTS.mosaicEnvelopeFractionOfFov,
  );

  // Guards the mount-time fetch against clobbering an in-flight user edit
  // (same convention as Cleanup.tsx/SourceViews.tsx).
  const editedRef = useRef(false);

  function applyTyped(values: FramingSettings) {
    if (values.framingPointingFractionOfFov !== undefined) {
      setPointingFraction(values.framingPointingFractionOfFov);
    }
    if (values.framingPointingFallbackDeg !== undefined) {
      setPointingFallback(values.framingPointingFallbackDeg);
    }
    if (values.framingRotationToleranceDeg !== undefined) {
      setRotationTolerance(values.framingRotationToleranceDeg);
    }
    if (values.framingMosaicEnvelopeFractionOfFov !== undefined) {
      setMosaicEnvelope(values.framingMosaicEnvelopeFractionOfFov);
    }
  }

  // `RestoreDefaultsBtn.onRestored` hands back a raw value bag, so it needs the
  // same validation the mount-time read gets.
  function applyValues(values: Record<string, unknown>) {
    applyTyped(parseSettingsValues(values, FramingSettingsSchema));
  }

  useEffect(() => {
    let cancelled = false;
    getSettingsTyped(FRAMING_SCOPE, FramingSettingsSchema)
      .then((values) => {
        if (cancelled || editedRef.current) return;
        applyTyped(values);
      })
      .catch(() => {
        // Backend unavailable — stay with in-code R11a defaults.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Marks the mount-time fetch stale as soon as the user starts typing, not
  // only once they blur/Enter to commit. Without this, the fetch can resolve
  // in the gap between an onChange (uncommitted local state) and the later
  // commit, clobbering the typed value back to the fetched one before
  // `editedRef` was ever set — and the subsequent blur then persists that
  // clobbered value, since it reads the (now-reset) DOM value.
  function onFieldChange(raw: string, setter: (n: number) => void) {
    editedRef.current = true;
    setter(Number(raw));
  }

  function commitNumber(
    raw: string,
    min: number,
    max: number,
    key: string,
    setter: (n: number) => void,
  ) {
    const n = Number(raw);
    if (!Number.isFinite(n)) return;
    const clamped = Math.max(min, Math.min(max, n));
    editedRef.current = true;
    setter(clamped);
    save(FRAMING_SCOPE, { [key]: clamped });
  }

  return (
    <SettingsSection
      title={m.settings_framing_title()}
      action={
        <RestoreDefaultsBtn
          scope={FRAMING_SCOPE}
          keys={FRAMING_KEYS}
          onRestored={applyValues}
        />
      }
    >
      <SettingsRow
        label={m.settings_framing_pointing_fraction_label()}
        info={m.settings_framing_pointing_fraction_info()}
      >
        <input
          type="number"
          className="pv-input pv-input--sm pv-settings__num-input"
          value={pointingFraction}
          min={POINTING_FRACTION_MIN}
          max={POINTING_FRACTION_MAX}
          step={0.01}
          aria-label={m.settings_framing_pointing_fraction_label()}
          data-testid="framing-pointing-fraction-input"
          onChange={(e) => onFieldChange(e.target.value, setPointingFraction)}
          onBlur={(e) =>
            commitNumber(
              e.target.value,
              POINTING_FRACTION_MIN,
              POINTING_FRACTION_MAX,
              'framingPointingFractionOfFov',
              setPointingFraction,
            )
          }
          onKeyDown={(e) => {
            if (e.key === 'Enter')
              commitNumber(
                (e.target as HTMLInputElement).value,
                POINTING_FRACTION_MIN,
                POINTING_FRACTION_MAX,
                'framingPointingFractionOfFov',
                setPointingFraction,
              );
          }}
        />
        <span className="pv-settings__unit-label">
          {m.settings_framing_unit_fraction()}
        </span>
      </SettingsRow>

      <SettingsRow
        label={m.settings_framing_pointing_fallback_label()}
        info={m.settings_framing_pointing_fallback_info()}
      >
        <input
          type="number"
          className="pv-input pv-input--sm pv-settings__num-input"
          value={pointingFallback}
          min={POINTING_FALLBACK_MIN}
          max={POINTING_FALLBACK_MAX}
          step={0.1}
          aria-label={m.settings_framing_pointing_fallback_label()}
          data-testid="framing-pointing-fallback-input"
          onChange={(e) => onFieldChange(e.target.value, setPointingFallback)}
          onBlur={(e) =>
            commitNumber(
              e.target.value,
              POINTING_FALLBACK_MIN,
              POINTING_FALLBACK_MAX,
              'framingPointingFallbackDeg',
              setPointingFallback,
            )
          }
          onKeyDown={(e) => {
            if (e.key === 'Enter')
              commitNumber(
                (e.target as HTMLInputElement).value,
                POINTING_FALLBACK_MIN,
                POINTING_FALLBACK_MAX,
                'framingPointingFallbackDeg',
                setPointingFallback,
              );
          }}
        />
        <span className="pv-settings__unit-label">
          {m.settings_framing_unit_deg()}
        </span>
      </SettingsRow>

      <SettingsRow
        label={m.settings_framing_rotation_label()}
        info={m.settings_framing_rotation_info()}
      >
        <input
          type="number"
          className="pv-input pv-input--sm pv-settings__num-input"
          value={rotationTolerance}
          min={ROTATION_MIN}
          max={ROTATION_MAX}
          step={0.5}
          aria-label={m.settings_framing_rotation_label()}
          data-testid="framing-rotation-tolerance-input"
          onChange={(e) => onFieldChange(e.target.value, setRotationTolerance)}
          onBlur={(e) =>
            commitNumber(
              e.target.value,
              ROTATION_MIN,
              ROTATION_MAX,
              'framingRotationToleranceDeg',
              setRotationTolerance,
            )
          }
          onKeyDown={(e) => {
            if (e.key === 'Enter')
              commitNumber(
                (e.target as HTMLInputElement).value,
                ROTATION_MIN,
                ROTATION_MAX,
                'framingRotationToleranceDeg',
                setRotationTolerance,
              );
          }}
        />
        <span className="pv-settings__unit-label">
          {m.settings_framing_unit_deg()}
        </span>
      </SettingsRow>

      <SettingsRow
        label={m.settings_framing_mosaic_envelope_label()}
        info={m.settings_framing_mosaic_envelope_info()}
      >
        <input
          type="number"
          className="pv-input pv-input--sm pv-settings__num-input"
          value={mosaicEnvelope}
          min={MOSAIC_ENVELOPE_MIN}
          max={MOSAIC_ENVELOPE_MAX}
          step={0.1}
          aria-label={m.settings_framing_mosaic_envelope_label()}
          data-testid="framing-mosaic-envelope-input"
          onChange={(e) => onFieldChange(e.target.value, setMosaicEnvelope)}
          onBlur={(e) =>
            commitNumber(
              e.target.value,
              MOSAIC_ENVELOPE_MIN,
              MOSAIC_ENVELOPE_MAX,
              'framingMosaicEnvelopeFractionOfFov',
              setMosaicEnvelope,
            )
          }
          onKeyDown={(e) => {
            if (e.key === 'Enter')
              commitNumber(
                (e.target as HTMLInputElement).value,
                MOSAIC_ENVELOPE_MIN,
                MOSAIC_ENVELOPE_MAX,
                'framingMosaicEnvelopeFractionOfFov',
                setMosaicEnvelope,
              );
          }}
        />
        <span className="pv-settings__unit-label">
          {m.settings_framing_unit_fraction()}
        </span>
      </SettingsRow>

      <p className="pv-calmatch__help">{m.settings_framing_help()}</p>
    </SettingsSection>
  );
}
