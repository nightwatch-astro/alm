// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * PlannerSettings — Settings → Target Planner pane (spec 044 + spec 047 T015).
 *
 * Three sections:
 *   1. Observing-site management (spec 044 US3) — add/edit/delete named sites
 *      and pick the default/active site. Persisted through the settings-backed
 *      `observing.*` store (`site-store.ts`), durable across relaunch
 *      (SC-005/SC-006) while applying instantly in the live UI (SC-003).
 *   2. Altitude threshold — the usable-altitude threshold that drives
 *      imaging-time and visible-tonight in the Planner table. Persisted via
 *      `altitude-settings.ts`; changes take effect immediately.
 *   3. Moon avoidance (per band) — the seven-band Lorentzian parameters
 *      (`plannerMoonAvoidance`, spec 047 FR-010) that drive the REAL filter
 *      guidance pills. Persisted via the spec-018 settings store
 *      (`guidance-settings.ts`); edits recompute the Planner table live
 *      (SC-008) with no Save button, matching every other settings pane's
 *      auto-persist convention. A reset-to-defaults action restores the
 *      shipped table.
 *
 * Every value the Planner table/detail pane computes from these settings is
 * real astronomy-engine output (spec 044 US1), not mock data.
 */

import { useState } from 'react';
import { m } from '@/lib/i18n';
import {
  SettingsSection,
  SettingsRow,
  RestoreDefaultsBtn,
} from './SettingsKit';
import {
  useAltitudeThreshold,
  setAltitudeThreshold,
  ALTITUDE_THRESHOLD_MIN,
  ALTITUDE_THRESHOLD_MAX,
} from '@/shared/planner/altitude-settings';
import { USABLE_ALT_DEG } from '@/features/targets/planner-altitude';
import { ObservingSites } from '@/shared/observing-sites/ObservingSites';
import {
  useGuidanceParams,
  saveGuidanceParams,
  restoreGuidanceDefaults,
  DISTANCE_MIN,
  DISTANCE_MAX,
  WIDTH_MIN,
  WIDTH_MAX,
} from '@/shared/planner/guidance-settings';
import { BANDS, type Band } from '@/features/targets/astro/moon-avoidance';

export function PlannerSettings() {
  const stored = useAltitudeThreshold();
  // Local draft so the input feels immediate; we commit to localStorage on
  // blur or Enter rather than on every keystroke.
  const [draft, setDraft] = useState<string>(String(stored));

  function commit(raw: string) {
    const n = Number(raw);
    if (Number.isFinite(n)) {
      setAltitudeThreshold(n);
      // Reflect the clamped value back into the draft so the field self-corrects.
      setDraft(
        String(
          Math.max(
            ALTITUDE_THRESHOLD_MIN,
            Math.min(ALTITUDE_THRESHOLD_MAX, Math.round(n)),
          ),
        ),
      );
    } else {
      // Non-numeric input: revert to the currently stored value.
      setDraft(String(stored));
    }
  }

  return (
    <>
      <ObservingSites />
      <SettingsSection title={m.settings_planner_altitude_title()}>
        <SettingsRow
          label={m.settings_planner_altitude_label()}
          info={m.settings_planner_altitude_info({ deg: USABLE_ALT_DEG })}
        >
          <input
            type="number"
            className="pv-input pv-input--sm pv-settings__num-input"
            value={draft}
            min={ALTITUDE_THRESHOLD_MIN}
            max={ALTITUDE_THRESHOLD_MAX}
            step={1}
            aria-label={m.settings_planner_altitude_aria()}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={(e) => commit(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter')
                commit((e.target as HTMLInputElement).value);
            }}
          />
          <span className="pv-settings__unit-label">
            {m.settings_planner_altitude_unit()}
          </span>
        </SettingsRow>
      </SettingsSection>
      <MoonAvoidanceSettings />
    </>
  );
}

// ── Moon avoidance (per band) — spec 047 T015 ──────────────────────────────────

/** Which numeric field of a band's params a draft key edits. */
type ParamField = 'distanceDeg' | 'widthDays';

function draftKey(band: Band, field: ParamField): string {
  return `${band}:${field}`;
}

function MoonAvoidanceSettings() {
  const params = useGuidanceParams();
  // Local per-cell drafts so typing feels immediate; committed on blur/Enter
  // (no Save button, matching every other settings pane's auto-persist
  // convention). Keyed by band+field so one row's edit doesn't clobber another.
  const [drafts, setDrafts] = useState<Record<string, string>>({});

  function valueFor(band: Band, field: ParamField): string {
    const key = draftKey(band, field);
    return key in drafts ? drafts[key] : String(params[band][field]);
  }

  function setDraft(band: Band, field: ParamField, raw: string) {
    setDrafts((prev) => ({ ...prev, [draftKey(band, field)]: raw }));
  }

  async function commit(band: Band, field: ParamField, raw: string) {
    const key = draftKey(band, field);
    const n = Number(raw);
    const [lo, hi] =
      field === 'distanceDeg'
        ? [DISTANCE_MIN, DISTANCE_MAX]
        : [WIDTH_MIN, WIDTH_MAX];
    if (Number.isFinite(n)) {
      const clamped = Math.max(lo, Math.min(hi, n));
      await saveGuidanceParams({
        ...params,
        [band]: { ...params[band], [field]: clamped },
      });
      setDrafts((prev) => {
        const next = { ...prev };
        delete next[key];
        return next;
      });
    } else {
      // Non-numeric input: drop the draft so the field reverts to the stored value.
      setDrafts((prev) => {
        const next = { ...prev };
        delete next[key];
        return next;
      });
    }
  }

  return (
    <SettingsSection
      title={m.settings_planner_moon_avoidance_title()}
      action={
        <RestoreDefaultsBtn
          onRestore={async () => {
            await restoreGuidanceDefaults();
            setDrafts({});
          }}
          scopeLabel={m.settings_planner_moon_avoidance_restore_scope()}
        />
      }
    >
      <SettingsRow
        label={m.settings_planner_moon_avoidance_band_col()}
        info={m.settings_planner_moon_avoidance_info()}
      >
        <span className="pv-settings__unit-label">
          {m.settings_planner_moon_avoidance_distance_col()} ·{' '}
          {m.settings_planner_moon_avoidance_width_col()}
        </span>
      </SettingsRow>
      {BANDS.map((band) => (
        <SettingsRow key={band} label={band}>
          <input
            type="number"
            className="pv-input pv-input--sm pv-settings__num-input"
            value={valueFor(band, 'distanceDeg')}
            min={DISTANCE_MIN}
            max={DISTANCE_MAX}
            step={1}
            aria-label={m.settings_planner_moon_avoidance_distance_aria({
              band,
            })}
            data-testid={`guidance-distance-${band}`}
            onChange={(e) => setDraft(band, 'distanceDeg', e.target.value)}
            onBlur={(e) => void commit(band, 'distanceDeg', e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter')
                void commit(
                  band,
                  'distanceDeg',
                  (e.target as HTMLInputElement).value,
                );
            }}
          />
          <input
            type="number"
            className="pv-input pv-input--sm pv-settings__num-input"
            value={valueFor(band, 'widthDays')}
            min={WIDTH_MIN}
            max={WIDTH_MAX}
            step={0.5}
            aria-label={m.settings_planner_moon_avoidance_width_aria({ band })}
            data-testid={`guidance-width-${band}`}
            onChange={(e) => setDraft(band, 'widthDays', e.target.value)}
            onBlur={(e) => void commit(band, 'widthDays', e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter')
                void commit(
                  band,
                  'widthDays',
                  (e.target as HTMLInputElement).value,
                );
            }}
          />
        </SettingsRow>
      ))}
    </SettingsSection>
  );
}
