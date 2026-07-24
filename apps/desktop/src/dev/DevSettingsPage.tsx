// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * /dev/settings — hidden developer-mode toggle (spec 021, T032).
 *
 * Reachable ONLY by typing the full URL. Deliberately NOT added to
 * `DEV_PAGES` in `apps/desktop/src/app/CommandPalette.tsx` (unlike
 * `/dev/contracts`) and NOT linked from the normal Settings › Advanced pane
 * — the whole point of this page is to be the one place that can turn
 * `devMode` on before the rest of the developer surface becomes reachable.
 *
 * Compile-time gated: this file is only imported (and its route only
 * registered) when `VITE_DEV_TOOLS === 'true'` — see
 * `apps/desktop/src/app/router.tsx`'s `DEV_TOOLS_ENABLED` constant, which
 * mirrors the Rust `dev-tools` Cargo feature. Release builds never bundle
 * this component (FR-031, SC-009).
 *
 * Toggling `devMode` off here still requires an app restart before the
 * recording proxy is fully uninstalled (FR-008) — this page surfaces that
 * with a restart hint rather than attempting a live uninstall.
 */

import { useState, useEffect, useCallback } from 'react';
import { PageShell } from '@/components';
import { getSettings, updateSettings } from '@/features/settings/settingsIpc';
import { SettingsSection, SettingsRow } from '@/features/settings/SettingsKit';
import { Toggle } from '@/ui';
import { m } from '@/lib/i18n';
import {
  pageBody,
  pageTitle,
  pageExportResult,
  pageError,
  pageLoading,
} from './dev.css';

export function DevSettingsPage() {
  const [devMode, setDevModeState] = useState<boolean | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    let cancelled = false;
    getSettings({ scope: 'advanced' })
      .then((data) => {
        if (cancelled) return;
        const vals = data.values as Record<string, unknown>;
        setDevModeState(vals?.devMode === true);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => load(), [load]);

  const handleToggle = useCallback(async (next: boolean) => {
    setSaving(true);
    setError(null);
    try {
      await updateSettings({ scope: 'advanced', values: { devMode: next } });
      setDevModeState(next);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }, []);

  return (
    <PageShell>
      {/* Reuses the dev/ContractsPage body/title/error/loading classes (shared
          component / no cloned CSS — see dev.css "DEV CONTRACTS PAGE"). */}
      <div className={`pv-page__scroll ${pageBody}`}>
        <h1 className={pageTitle}>{m.dev_settings_title()}</h1>
        <p className={pageExportResult}>{m.dev_settings_intro()}</p>

        {error && (
          <div role="alert" className={pageError}>
            {m.dev_settings_error({ message: error })}
          </div>
        )}

        <SettingsSection title={m.dev_settings_section_title()}>
          <SettingsRow
            label={m.dev_settings_toggle_label()}
            info={m.dev_settings_toggle_info()}
          >
            {devMode === null ? (
              <span className={pageLoading}>{m.common_loading()}</span>
            ) : (
              <Toggle
                aria-label={m.dev_settings_toggle_label()}
                checked={devMode}
                onChange={(v) => void handleToggle(v)}
                data-testid="dev-mode-toggle"
              />
            )}
          </SettingsRow>
          {devMode === true && (
            <p className={pageExportResult}>{m.dev_settings_restart_hint()}</p>
          )}
        </SettingsSection>

        {saving && <p className={pageExportResult}>{m.common_saving()}</p>}
      </div>
    </PageShell>
  );
}
