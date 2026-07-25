// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useState, type ReactNode } from 'react';
import { Btn, InfoTip } from '@/ui';
import { settingsRestoreDefaults, getSettings } from './settingsIpc';
import { m } from '@/lib/i18n';

/**
 * Settings layout kit — the consistent section + form-row primitives every
 * settings pane composes (authoritative: platevault-settings-menu.html).
 *
 * - `SettingsSection`: uppercase title + optional right-aligned action, then a
 *   hairline rule, then the rows/table (mock `.sec` / `.sec__t` / `.sec__r`).
 * - `SettingsRow`: 200px label column (with an optional ⓘ InfoTip carrying the
 *   help text) + control column. Help prose lives in the tooltip, never as
 *   always-on text under the control (mock de-vibe step).
 */

export interface SettingsSectionProps {
  title: string;
  /** Optional right-aligned action (e.g. a primary "+ Add…" button). */
  action?: ReactNode;
  children: ReactNode;
}

export function SettingsSection({
  title,
  action,
  children,
}: SettingsSectionProps) {
  return (
    <div className="pv-settings__group">
      {action ? (
        <div className="pv-settings__group-header pv-settings__group-header--ruled">
          <div
            className="pv-settings__group-title"
            data-testid="settings-group-title"
          >
            {title}
          </div>
          {action}
        </div>
      ) : (
        <div
          className="pv-settings__group-title"
          data-testid="settings-group-title"
        >
          {title}
        </div>
      )}
      {children}
    </div>
  );
}

export interface SettingsRowProps {
  label: ReactNode;
  /** Help text shown in an ⓘ tooltip beside the label (replaces help prose). */
  info?: string;
  children: ReactNode;
}

export function SettingsRow({ label, info, children }: SettingsRowProps) {
  return (
    <div className="pv-settings__row" data-testid="settings-row">
      <div className="pv-settings__row-label">
        <span>{label}</span>
        {info ? <InfoTip tip={info} /> : null}
      </div>
      <div className="pv-settings__row-content">{children}</div>
    </div>
  );
}

// ── Shared add/edit form shell ────────────────────────────────────────────────

export interface SettingsFormShellProps {
  /** Form-level error (wrapped save-error frame or validation message). */
  error: string | null;
  /** Disables the actions and switches the save label while a call is in flight. */
  saving: boolean;
  onCancel: () => void;
  onSave: () => void | Promise<void>;
  /** The entity-specific field controls (each wrapped in `.pv-stack-1`). */
  children: ReactNode;
}

/**
 * The one parameterised add/edit form frame every settings pane's CRUD list
 * shares: field grid + error line + cancel/save actions. Only the fields
 * differ per entity type (shared-component mandate — no per-pane clones).
 * Originally lived only in `Equipment.tsx`; promoted here so other panes
 * (e.g. observing-site management, spec 044 US3) reuse it verbatim, and to
 * keep the `.pv-equipment__form*` markup as one generic settings-form frame
 * rather than a per-feature copy.
 */
export function SettingsFormShell({
  error,
  saving,
  onCancel,
  onSave,
  children,
}: SettingsFormShellProps) {
  return (
    <div className="pv-equipment__form">
      <div className="pv-equipment__form-grid">{children}</div>
      {error && <span className="pv-field-error">{error}</span>}
      <div className="pv-equipment__form-actions">
        <Btn size="sm" variant="ghost" onClick={onCancel} disabled={saving}>
          {m.common_cancel()}
        </Btn>
        <Btn size="sm" onClick={() => void onSave()} disabled={saving}>
          {saving ? m.common_saving() : m.common_save()}
        </Btn>
      </div>
    </div>
  );
}

// ── Restore-defaults action (spec 018 T028) ───────────────────────────────────

export interface RestoreDefaultsBtnProps {
  /**
   * Keys belonging to this pane (settings-scope mode). Pass an empty array to
   * restore all keys. The set mirrors the backend `scope_keys` in settings.rs.
   * Ignored when `onRestore` is supplied.
   */
  keys?: string[];
  /** Called after defaults are restored so the pane can re-hydrate. */
  onRestored?: (values: Record<string, unknown>) => void;
  /** Settings scope used to refetch after restore (e.g. 'advanced'). */
  scope?: string;
  /**
   * Custom restore action for panes whose values are NOT in the settings table
   * (e.g. Calibration, which owns its own `calibrationTolerances` IPC). When
   * provided, the button runs this instead of `settings.restore-defaults`, and
   * `keys`/`scope`/`onRestored` are unused.
   */
  onRestore?: () => Promise<void>;
  /**
   * What this button resets, e.g. "moon-avoidance defaults" (#837) — several
   * panes scope a "Restore defaults" button to only PART of the pane (a
   * sub-table, not the whole settings section), and the bare label gives no
   * indication of that. Rendered as the button's `aria-label`/`title` so
   * screen readers and hover both disambiguate scope; the visible label
   * stays the short "Restore defaults" text.
   */
  scopeLabel?: string;
}

type RestoreState = 'idle' | 'restoring' | 'done';

/**
 * Small "Restore defaults" button for a settings section header.
 *
 * Default mode calls `settings.restore-defaults` with this pane's key list,
 * then re-fetches the scope so controls reflect the restored values. Panes
 * backed by a different store pass `onRestore` to run their own reset instead.
 */
const FEEDBACK_MS = 1500;

export function RestoreDefaultsBtn({
  keys,
  onRestored,
  scope,
  onRestore,
  scopeLabel,
}: RestoreDefaultsBtnProps) {
  const [state, setState] = useState<RestoreState>('idle');

  const handleClick = async () => {
    if (state === 'restoring') return;
    setState('restoring');
    try {
      if (onRestore) {
        await onRestore();
      } else {
        await settingsRestoreDefaults(keys ?? []);
        // Re-fetch so the pane receives the actual persisted defaults.
        const fresh = await getSettings({ scope: scope ?? '' });
        onRestored?.(fresh.values as Record<string, unknown>);
      }
      setState('done');
      setTimeout(() => setState('idle'), FEEDBACK_MS);
    } catch {
      setState('idle');
    }
  };

  const label =
    state === 'restoring'
      ? m.settings_action_restore_defaults_restoring()
      : state === 'done'
        ? m.settings_action_restore_defaults_done()
        : m.settings_action_restore_defaults();

  return (
    <Btn
      size="sm"
      variant="ghost"
      disabled={state === 'restoring'}
      onClick={() => void handleClick()}
      title={scopeLabel}
      aria-label={scopeLabel ? `${label} — ${scopeLabel}` : undefined}
    >
      {label}
    </Btn>
  );
}
