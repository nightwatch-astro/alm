// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// First-run wizard: "Observing Site" step (spec 044 Track B, US6 T016).
//
// Captures a default+active observing site (name/lat/lon/timezone required,
// elevation optional) so the Targets Planner has real tonight-observability
// data (max-alt, visible-tonight, imaging time) from the very first launch,
// instead of always landing on the no-site prompt (US1/US6). Entirely
// optional — the user can leave every field blank and add a site later from
// Settings -> Target Planner (spec 044 US3, `ObservingSites.tsx`); the step
// never blocks Finish (FR-025 does not require a site to complete setup).

import { lazy, Suspense } from 'react';
import { m } from '@/lib/i18n';
import {
  localTimezone,
  ianaTimezones,
} from '@/features/targets/observing-sites/iana-timezones';
import type { Twilight } from '@/features/targets/observing-sites/observer-site';
import { selectBase } from '@/styles/select.css';

// Leaflet is ~240 KB raw. Lazy-load so it splits to its own chunk and does
// not inflate the setup-wizard bundle parsed on first run.
const SiteLocationPicker = lazy(() =>
  import('@/features/targets/observing-sites/SiteLocationPicker').then((m) => ({
    default: m.SiteLocationPicker,
  })),
);

export interface SiteStepState {
  name: string;
  latitudeDegText: string;
  longitudeDegText: string;
  elevationMText: string;
  timezone: string;
}

export const DEFAULT_SITE_STEP_STATE: SiteStepState = {
  name: '',
  latitudeDegText: '',
  longitudeDegText: '',
  elevationMText: '',
  timezone: localTimezone(),
};

/** Default twilight/horizon applied to the site created from this step (changeable later in Settings). */
export const SITE_STEP_DEFAULT_TWILIGHT: Twilight = 'astronomical';
export const SITE_STEP_DEFAULT_MIN_HORIZON_ALT_DEG = 0;

export interface StepSiteProps {
  state: SiteStepState;
  onChange: (state: SiteStepState) => void;
}

type SiteField = 'name' | 'latitude' | 'longitude' | 'elevation';
type SiteStepErrors = Partial<Record<SiteField, string>>;

/** True when enough fields are filled in to create a site from this step (name/lat/lon; matches T016's "required" fields plus timezone, which always has a value). */
export function siteStepHasSite(state: SiteStepState): boolean {
  return (
    state.name.trim() !== '' &&
    state.latitudeDegText.trim() !== '' &&
    state.longitudeDegText.trim() !== ''
  );
}

/** Validate the optional site and return localized errors keyed by field. */
export function siteStepErrors(state: SiteStepState): SiteStepErrors {
  const nameFilled = state.name.trim() !== '';
  const coordsFilled =
    state.latitudeDegText.trim() !== '' && state.longitudeDegText.trim() !== '';
  // A blank step (nothing filled in yet) is skipped, not invalid. But once
  // the user has entered coordinates, the site needs a name too — matching
  // the Settings -> Target Planner site editor, which requires it
  // (`ObservingSites.tsx`) — otherwise Continue silently accepted an
  // anonymous site that then got dropped entirely at Finish (#516).
  if (!nameFilled && !coordsFilled) return {};
  const errors: SiteStepErrors = {};
  if (!nameFilled) {
    errors.name = m.settings_observing_sites_error_name();
  }
  if (!coordsFilled) return errors;
  const lat = Number(state.latitudeDegText.trim());
  if (!Number.isFinite(lat) || lat < -90 || lat > 90) {
    errors.latitude = m.settings_observing_sites_error_latitude();
  }
  const lon = Number(state.longitudeDegText.trim());
  if (!Number.isFinite(lon) || lon < -180 || lon > 180) {
    errors.longitude = m.settings_observing_sites_error_longitude();
  }
  const elevRaw = state.elevationMText.trim();
  if (elevRaw !== '' && !Number.isFinite(Number(elevRaw))) {
    errors.elevation = m.settings_observing_sites_error_elevation();
  }
  return errors;
}

/** Validate the optional site using the first field error for the wizard gate. */
export function siteStepError(state: SiteStepState): string | null {
  return Object.values(siteStepErrors(state))[0] ?? null;
}

/**
 * Step — Observing Site.
 *
 * Same field set as the Settings -> Target Planner site editor
 * (`ObservingSites.tsx`), reused as plain controlled inputs here since the
 * wizard owns its own step-state shape rather than the settings-backed
 * site-store (the site isn't persisted until Finish).
 */
/** Parsed field value for the map, or `null` when blank/not-a-number (no pin shown). */
function parsedCoord(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === '') return null;
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : null;
}

export function StepSite({ state, onChange }: StepSiteProps) {
  const timezones = ianaTimezones();
  const errors = siteStepErrors(state);

  return (
    <div className="pv-step-site">
      <p className="pv-step-site__intro">{m.setup_site_intro()}</p>

      <div className="pv-step-site__map-section">
        <span className="pv-field-label">{m.setup_site_map_label()}</span>
        <Suspense
          fallback={
            <div
              className="pv-step-site__map"
              role="status"
              aria-label={m.setup_site_map_label()}
            />
          }
        >
          <SiteLocationPicker
            latitudeDeg={parsedCoord(state.latitudeDegText)}
            longitudeDeg={parsedCoord(state.longitudeDegText)}
            onPick={(lat, lon) =>
              onChange({
                ...state,
                latitudeDegText: lat.toFixed(5),
                longitudeDegText: lon.toFixed(5),
              })
            }
          />
        </Suspense>
      </div>

      <div className="pv-step-site__grid">
        <div className="pv-stack-1">
          <label className="pv-field-label" htmlFor="setup-site-name">
            {m.settings_observing_sites_field_name()}
          </label>
          <input
            id="setup-site-name"
            type="text"
            className="pv-input"
            aria-label={m.settings_observing_sites_field_name()}
            aria-invalid={errors.name ? 'true' : undefined}
            aria-describedby={errors.name ? 'setup-site-name-error' : undefined}
            value={state.name}
            onChange={(e) => onChange({ ...state, name: e.target.value })}
          />
          {errors.name && (
            <span id="setup-site-name-error" className="pv-field-error">
              {errors.name}
            </span>
          )}
        </div>
        <div className="pv-stack-1">
          <label className="pv-field-label" htmlFor="setup-site-lat">
            {m.settings_observing_sites_field_latitude()}
          </label>
          <input
            id="setup-site-lat"
            type="text"
            inputMode="decimal"
            className="pv-input"
            aria-label={m.settings_observing_sites_field_latitude()}
            aria-invalid={errors.latitude ? 'true' : undefined}
            aria-describedby={
              errors.latitude ? 'setup-site-lat-error' : undefined
            }
            value={state.latitudeDegText}
            onChange={(e) =>
              onChange({ ...state, latitudeDegText: e.target.value })
            }
          />
          {errors.latitude && (
            <span id="setup-site-lat-error" className="pv-field-error">
              {errors.latitude}
            </span>
          )}
        </div>
        <div className="pv-stack-1">
          <label className="pv-field-label" htmlFor="setup-site-lon">
            {m.settings_observing_sites_field_longitude()}
          </label>
          <input
            id="setup-site-lon"
            type="text"
            inputMode="decimal"
            className="pv-input"
            aria-label={m.settings_observing_sites_field_longitude()}
            aria-invalid={errors.longitude ? 'true' : undefined}
            aria-describedby={
              errors.longitude ? 'setup-site-lon-error' : undefined
            }
            value={state.longitudeDegText}
            onChange={(e) =>
              onChange({ ...state, longitudeDegText: e.target.value })
            }
          />
          {errors.longitude && (
            <span id="setup-site-lon-error" className="pv-field-error">
              {errors.longitude}
            </span>
          )}
        </div>
        <div className="pv-stack-1">
          <label className="pv-field-label" htmlFor="setup-site-elevation">
            {m.settings_observing_sites_field_elevation()}
          </label>
          <input
            id="setup-site-elevation"
            type="text"
            inputMode="decimal"
            className="pv-input"
            aria-label={m.settings_observing_sites_field_elevation()}
            aria-invalid={errors.elevation ? 'true' : undefined}
            aria-describedby={
              errors.elevation ? 'setup-site-elevation-error' : undefined
            }
            value={state.elevationMText}
            onChange={(e) =>
              onChange({ ...state, elevationMText: e.target.value })
            }
          />
          {errors.elevation && (
            <span id="setup-site-elevation-error" className="pv-field-error">
              {errors.elevation}
            </span>
          )}
        </div>
        <div className="pv-stack-1">
          <label className="pv-field-label" htmlFor="setup-site-tz">
            {m.settings_observing_sites_field_timezone()}
          </label>
          <select
            id="setup-site-tz"
            className={selectBase}
            aria-label={m.settings_observing_sites_field_timezone()}
            value={state.timezone}
            onChange={(e) => onChange({ ...state, timezone: e.target.value })}
          >
            {timezones.map((tz) => (
              <option key={tz} value={tz}>
                {tz}
              </option>
            ))}
          </select>
        </div>
      </div>

      <p className="pv-step-site__note">{m.setup_site_skip_note()}</p>
    </div>
  );
}
