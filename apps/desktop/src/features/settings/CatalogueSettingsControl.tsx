// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Default planner catalogues control (task #82).
//
// Lets the user pick which catalogues are enabled by default in the Targets →
// Planner multi-select. Persisted via the generic settings backend under the
// `'catalogues'` scope (see features/targets/catalogue-settings.ts). The Planner
// initializes its catalogue filter from this setting on load.

import { useCallback, useEffect, useRef, useState } from 'react';
import { Toggle } from '@/ui';
import {
  PLANNER_CATALOGS,
  type CatalogueId,
} from '@/features/targets/planner-catalog';
import {
  DEFAULT_ENABLED_CATALOGUES,
  loadDefaultCatalogues,
  saveDefaultCatalogues,
} from '@/shared/planner/catalogue-settings';
import { m } from '@/lib/i18n';
import { SettingsRow, RestoreDefaultsBtn } from './SettingsKit';

export function CatalogueSettingsControl() {
  const [enabled, setEnabled] = useState<Set<CatalogueId>>(
    () => new Set(DEFAULT_ENABLED_CATALOGUES),
  );
  const [saveError, setSaveError] = useState<string | null>(null);

  // Set once the user toggles a catalogue, so the mount read below can never
  // overwrite a deliberate choice. `cancelled` only covers unmount, not the
  // case where the component is still mounted and the user has already acted.
  const toggledRef = useRef(false);

  // Load the persisted defaults on mount. This resolves asynchronously, so on a
  // slow backend it can land AFTER the user has toggled a catalogue — and by
  // then `persist` has already written their choice to the settings DB, so
  // applying the read would leave the UI showing a value the backend no longer
  // holds. The toggle is the more recent intent and must win.
  useEffect(() => {
    let cancelled = false;
    loadDefaultCatalogues()
      .then((ids) => {
        if (!cancelled && !toggledRef.current) setEnabled(new Set(ids));
      })
      .catch(() => {
        // Backend unavailable — keep in-code defaults.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const persist = useCallback(async (next: Set<CatalogueId>) => {
    setEnabled(next);
    setSaveError(null);
    try {
      await saveDefaultCatalogues(
        PLANNER_CATALOGS.map((c) => c.id).filter((id) => next.has(id)),
      );
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const toggle = (id: CatalogueId, on: boolean): void => {
    // Claim the setting before the mount read can answer (see the effect
    // above) — from here on the user owns it for this session.
    toggledRef.current = true;
    const next = new Set(enabled);
    if (on) next.add(id);
    else next.delete(id);
    void persist(next);
  };

  // #802: same custom-onRestore shape as ResolverSettingsControl — this pane's
  // defaults live in DEFAULT_ENABLED_CATALOGUES, not the generic settings-scope
  // key list, so the button persists that constant directly instead of going
  // through settings.restore-defaults.
  const handleRestoreDefaults = useCallback(async () => {
    toggledRef.current = true;
    await persist(new Set(DEFAULT_ENABLED_CATALOGUES));
  }, [persist]);

  return (
    <>
      {PLANNER_CATALOGS.map((c) => (
        <SettingsRow
          key={c.id}
          label={c.label()}
          info={m.settings_catalogue_default_info({ label: c.label() })}
        >
          <Toggle
            checked={enabled.has(c.id)}
            aria-label={m.settings_catalogue_enable_default_aria({
              label: c.label(),
            })}
            onChange={(v) => toggle(c.id, v)}
          />
        </SettingsRow>
      ))}

      <SettingsRow
        label={m.settings_action_restore_defaults()}
        info={m.settings_catalogue_restore_scope()}
      >
        <RestoreDefaultsBtn
          onRestore={handleRestoreDefaults}
          scopeLabel={m.settings_catalogue_restore_scope()}
        />
      </SettingsRow>

      {saveError && (
        <div className="pv-settings__error" role="alert">
          {m.settings_catalogue_save_error({ error: saveError })}
        </div>
      )}
    </>
  );
}
