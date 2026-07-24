// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { clsx } from 'clsx';
import * as tp from './ThemePicker.css';
import {
  resolveTheme,
  THEMES,
  useThemeChoice,
  type ThemeChoice,
  type ThemeId,
  type ThemeMeta,
} from '@/data/theme';
import { m } from '@/lib/i18n';

interface ThemePickerChoice {
  id: ThemeChoice;
  label: () => string;
  mode: 'light' | 'dark' | 'auto';
}

export interface ThemePickerProps {
  /**
   * The setup wizard exposes every shipped theme so existing first-run
   * choices remain available. Settings keeps its intentionally smaller set
   * of canonical themes.
   */
  includeVariants?: boolean;
  className?: string;
}

const MODE_ORDER: Record<ThemeMeta['mode'], number> = { light: 0, dark: 1 };

function toChoice(theme: ThemeMeta): ThemePickerChoice {
  return {
    id: theme.id,
    label: () => theme.label,
    mode: theme.mode,
  };
}

function themesForFamily(
  family: ThemeMeta['family'],
  includeVariants: boolean,
): ThemePickerChoice[] {
  return THEMES.filter(
    (theme) => theme.family === family && (includeVariants || theme.enabled),
  )
    .sort((a, b) => MODE_ORDER[a.mode] - MODE_ORDER[b.mode])
    .map(toChoice);
}

function themeModeLabel(choice: ThemePickerChoice, resolved: ThemeId): string {
  if (choice.id === 'system') {
    return THEMES.find((theme) => theme.id === resolved)?.mode === 'dark'
      ? m.settings_theme_mode_auto_dark()
      : m.settings_theme_mode_auto_light();
  }
  return choice.mode === 'dark'
    ? m.settings_theme_mode_dark()
    : m.settings_theme_mode_light();
}

/** Replaces any registry-label mode suffix with the localized mode label. */
function themeAccessibleLabel(
  choice: ThemePickerChoice,
  modeLabel: string,
): string {
  const label = choice.label();
  const modeSuffix = ` · ${choice.mode}`;
  const baseLabel = label.toLowerCase().endsWith(modeSuffix)
    ? label.slice(0, -modeSuffix.length)
    : label;
  return `${baseLabel} · ${modeLabel}`;
}

/**
 * Shared live theme picker used by Settings and first-run setup.
 *
 * Each preview scopes the existing token layer with `data-theme`; selecting a
 * card updates the app-wide theme source of truth immediately. Native buttons
 * and `aria-pressed` preserve keyboard and screen-reader behaviour.
 */
export function ThemePicker({
  includeVariants = false,
  className,
}: ThemePickerProps) {
  const [choice, setChoice] = useThemeChoice();
  const resolved = resolveTheme(choice);
  const systemChoice: ThemePickerChoice = {
    id: 'system',
    label: () => m.settings_general_theme_system(),
    mode: 'auto',
  };
  const warmChoices = themesForFamily('warm', includeVariants);
  const coolChoices = themesForFamily('cool', includeVariants);

  const renderChoice = (theme: ThemePickerChoice) => {
    const isActive = choice === theme.id;
    const previewTheme = theme.id === 'system' ? resolved : theme.id;
    const modeLabel = themeModeLabel(theme, resolved);
    return (
      <button
        key={theme.id}
        type="button"
        className={clsx(tp.swatch, isActive && tp.swatchActive)}
        data-testid="theme-swatch"
        onClick={() => setChoice(theme.id)}
        aria-pressed={isActive}
        aria-label={themeAccessibleLabel(theme, modeLabel)}
      >
        {isActive && (
          <span className={tp.swatchSelected} aria-hidden="true">
            ✓
          </span>
        )}
        <span className={tp.swatchPrev} data-theme={previewTheme}>
          <i className="pv-theme-swatch__bg" />
          <i className="pv-theme-swatch__surface" />
          <i className="pv-theme-swatch__accent" />
        </span>
        <span className={tp.swatchName}>{theme.label()}</span>
        <span className={tp.swatchMode}>{modeLabel}</span>
      </button>
    );
  };

  return (
    <div
      className={clsx(tp.picker, className)}
      data-testid="theme-picker"
      role="group"
      aria-label={m.settings_general_theme()}
    >
      <div className={tp.swatches}>{renderChoice(systemChoice)}</div>

      <section
        className={tp.pickerGroup}
        aria-labelledby="theme-picker-warm-heading"
      >
        <div
          id="theme-picker-warm-heading"
          className="pv-settings__group-title"
          data-testid="settings-group-title"
        >
          {m.settings_general_theme_group_warm()}
        </div>
        <div className={tp.swatches}>{warmChoices.map(renderChoice)}</div>
      </section>

      <section
        className={tp.pickerGroup}
        aria-labelledby="theme-picker-cool-heading"
      >
        <div
          id="theme-picker-cool-heading"
          className="pv-settings__group-title"
          data-testid="settings-group-title"
        >
          {m.settings_general_theme_group_cool()}
        </div>
        <div className={tp.swatches}>{coolChoices.map(renderChoice)}</div>
      </section>
    </div>
  );
}
