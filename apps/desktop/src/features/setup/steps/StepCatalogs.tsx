// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// First-run wizard: "Configuration" step.
//
// Originally the spec-014 catalog-download step; that backend was removed (spec
// 035 — targets resolve on demand from SIMBAD + a bundled seed + local cache).
// The step slot is retained (first_run_state.last_step CHECK still includes
// 'catalogs' — no migration) and repurposed as a small first-run Configuration
// screen: a few defaults the user can set up front (all changeable later in
// Settings).

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ResolverSettingsControl } from '@/features/settings/ResolverSettingsControl';
import { usePreference } from '@/data/preferences';
import { m } from '@/lib/i18n';
import type { Density } from '@/bindings/types';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { selectBase } from '@/styles/select.css';

// ── Types ─────────────────────────────────────────────────────────────────────
//
// Kept for compatibility with SetupWizard state persistence and StepConfirm.
// `downloadAll` is an inert legacy flag — nothing is downloaded.

export interface CatalogSettings {
  /** Legacy flag retained for state-shape compatibility; no longer used. */
  downloadAll: boolean;
}

export const DEFAULT_CATALOG_SETTINGS: CatalogSettings = {
  downloadAll: true,
};

export interface StepCatalogsProps {
  settings: CatalogSettings;
  onSettingsChange: (settings: CatalogSettings) => void;
}

// 2-level model (issue #506): the third "normal" level is retired — absence
// of a per-source override already means inherit-global.
type DefaultProtection = 'protected' | 'unprotected';

// ── Default source protection (spec 018, persisted via the settings backend) ──

function DefaultProtectionControl() {
  const [value, setValue] = useState<DefaultProtection>('protected');

  // Set once the user picks a value, so the mount read below can never
  // overwrite a deliberate choice. `cancelled` only covers unmount, not the
  // still-mounted case where the user has already chosen.
  const chosenRef = useRef(false);

  // Load the persisted value on mount. This resolves asynchronously, so on a
  // slow backend it can land AFTER the user has already picked from the select
  // — and `onChange` has by then persisted their pick, so applying the read
  // would show a value the backend no longer holds. Same defect as the
  // Settings → Cleanup pane guards with `editedRef`.
  useEffect(() => {
    let cancelled = false;
    commands
      .settingsGet('cleanup')
      .then((r) => unwrap(r))
      .then((data) => {
        const vals = data?.values as Record<string, unknown> | undefined;
        const v = vals?.defaultProtection;
        if (!cancelled && !chosenRef.current && typeof v === 'string')
          setValue(v as DefaultProtection);
      })
      .catch(() => {
        // Backend unavailable — keep the default.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onChange = (v: DefaultProtection) => {
    // Claim the setting before the mount read can answer (see the effect
    // above) — from here on the user owns it for this session.
    chosenRef.current = true;
    setValue(v);
    void commands
      .settingsUpdate('cleanup', { defaultProtection: v })
      .then((r) => unwrap(r))
      .catch(() => {});
  };

  return (
    <select
      className={selectBase}
      value={value}
      aria-label={m.setup_config_default_protection_title()}
      onChange={(e) => onChange(e.target.value as DefaultProtection)}
    >
      <option value="protected">
        {m.settings_cleanup_protection_protected()}
      </option>
      <option value="unprotected">
        {m.settings_cleanup_protection_unprotected()}
      </option>
    </select>
  );
}

// ── Display density (frontend preference; applied app-wide) ───────────────────

function DensityControl() {
  const [density, setDensity] = usePreference('density');
  return (
    <select
      className={selectBase}
      value={density}
      aria-label={m.settings_density_legend()}
      onChange={(e) => setDensity(e.target.value as Density)}
    >
      <option value="compact">{m.setup_config_density_compact()}</option>
      <option value="comfortable">
        {m.setup_config_density_comfortable()}
      </option>
      <option value="spacious">{m.setup_config_density_spacious()}</option>
    </select>
  );
}

// ── A labelled config row: title + control on one line, description below ──────

function ConfigOption({
  title,
  description,
  control,
}: {
  title: string;
  description: string;
  control: ReactNode;
}) {
  return (
    <div className="pv-setup-catalogs__option">
      <div className="pv-setup-catalogs__option-header">
        <span className="pv-setup-catalogs__option-title">{title}</span>
        {control}
      </div>
      <div className="pv-settings__row-desc">{description}</div>
    </div>
  );
}

// ── StepCatalogs (Configuration) ──────────────────────────────────────────────

/**
 * Configuration step.
 *
 * A few first-run defaults: online SIMBAD resolution, display density, default
 * source protection, and default scan depth. All are changeable later in
 * Settings; the step never blocks Finish.
 */
export function StepCatalogs(_props: StepCatalogsProps) {
  return (
    <div className="pv-step-catalogs">
      {/* Online SIMBAD resolution (label + toggle on one line, desc below). */}
      <ResolverSettingsControl compact />

      <ConfigOption
        title={m.settings_density_legend()}
        description={m.setup_config_display_density_desc()}
        control={<DensityControl />}
      />

      <ConfigOption
        title={m.setup_config_default_protection_title()}
        description={m.setup_config_default_protection_desc()}
        control={<DefaultProtectionControl />}
      />
    </div>
  );
}
