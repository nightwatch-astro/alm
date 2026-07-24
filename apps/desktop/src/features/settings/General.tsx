// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Appearance settings — theme, font size, display density.
// Theme is applied live via the appearance runtime (data/theme.ts): swatch
// cards re-scope the token layer with `data-theme` so each preview shows its
// own palette without any element-level color injection.
import { usePreference } from '@/data/preferences';
import {
  useThemeChoice,
  useFontSizeChoice,
  useZoomChoice,
  ZOOM_STEPS,
} from '@/data/theme';
import type { FontSizeChoice, ZoomPercent } from '@/data/theme';
import type { Density } from '@/bindings/types';
import { m } from '@/lib/i18n';
import { useLocale, BASE_LOCALE, SHIPPED_LOCALES } from '@/data/locale';
import type { Locale } from '@/data/locale';
import { LOCALE_META } from '@/data/locale-meta';
import { SegControl } from '@/ui';
import type { SegControlOption } from '@/ui/SegControl';
import { ThemePicker } from '@/components/ThemePicker';
import { SettingsSection, RestoreDefaultsBtn } from './SettingsKit';
import { selectBase } from '@/styles/select.css';

/** In-code defaults (data/theme.ts + preferences.ts) — none of these are
 *  settings-DB-backed panes, so restore is a local reset, not a backend call
 *  (#802: Appearance was one of 3 default-backed panes missing the shared
 *  RestoreDefaultsBtn control). */
const DEFAULT_DENSITY: Density = 'comfortable';

// FR-007 (flag + native name per option) and research D6 (accessible name is
// the native name alone, never the flag) at once: `icon` supplies the
// visible content (flag + name together), while `label` — read as
// `aria-label`/`title` whenever `icon` is set (see SegControl) — carries
// only the native name, so a screen reader never announces the flag emoji.
const LANGUAGE_OPTIONS: SegControlOption[] = SHIPPED_LOCALES.map((id) => {
  const meta = LOCALE_META[id];
  return {
    value: id,
    label: meta.nativeName,
    icon: (
      <>
        <span aria-hidden="true">{meta.flag}</span> {meta.nativeName}
      </>
    ),
  };
});

export function General() {
  const [, setChoice] = useThemeChoice();
  const [fontSize, setFontSize] = useFontSizeChoice();
  const [zoom, setZoom] = useZoomChoice();
  const [density, setDensity] = usePreference('density');
  const { locale, changeLocale } = useLocale();

  const handleRestoreDefaults = async () => {
    setChoice('system');
    setFontSize('default');
    setZoom(100);
    setDensity(DEFAULT_DENSITY);
    changeLocale(BASE_LOCALE);
  };

  return (
    <>
      <SettingsSection
        title={m.settings_general_theme()}
        action={<RestoreDefaultsBtn onRestore={handleRestoreDefaults} />}
      >
        <ThemePicker />
      </SettingsSection>

      <div className="pv-settings__group">
        <div
          className="pv-settings__group-title"
          data-testid="settings-group-title"
        >
          {m.settings_language_title()}
        </div>
        <div className="pv-settings__row" data-testid="settings-row">
          <div className="pv-settings__row-label">
            {m.settings_language_label()}
          </div>
          <div className="pv-settings__row-content">
            <SegControl
              options={LANGUAGE_OPTIONS}
              value={locale}
              onChange={(next) => changeLocale(next as Locale)}
              aria-label={m.settings_language_label()}
            />
          </div>
        </div>
      </div>

      <div className="pv-settings__group">
        <div
          className="pv-settings__group-title"
          data-testid="settings-group-title"
        >
          {m.settings_general_fontsize_title()}
        </div>
        <div className="pv-settings__row" data-testid="settings-row">
          <div className="pv-settings__row-label">
            {m.settings_general_fontsize_title()}
          </div>
          <div className="pv-settings__row-content">
            <select
              className={selectBase}
              value={fontSize}
              onChange={(e) => setFontSize(e.target.value as FontSizeChoice)}
            >
              <option value="small">
                {m.settings_general_fontsize_small()}
              </option>
              <option value="default">
                {m.settings_general_fontsize_default()}
              </option>
              <option value="large">
                {m.settings_general_fontsize_large()}
              </option>
            </select>
          </div>
        </div>
      </div>

      <div className="pv-settings__group">
        <div
          className="pv-settings__group-title"
          data-testid="settings-group-title"
        >
          {m.settings_general_zoom_title()}
        </div>
        <div className="pv-settings__row" data-testid="settings-row">
          <div className="pv-settings__row-label">
            {m.settings_general_zoom_label()}
          </div>
          <div className="pv-settings__row-content">
            <select
              className={selectBase}
              value={zoom}
              onChange={(e) => setZoom(Number(e.target.value) as ZoomPercent)}
            >
              {ZOOM_STEPS.map((step) => (
                <option key={step} value={step}>
                  {step}%
                  {step === 100
                    ? ` (${m.settings_general_zoom_default_suffix()})`
                    : ''}
                </option>
              ))}
            </select>
          </div>
        </div>
        <p className="pv-calmatch__help">{m.settings_general_zoom_hint()}</p>
      </div>

      <div className="pv-settings__group">
        <div
          className="pv-settings__group-title"
          data-testid="settings-group-title"
        >
          {m.settings_general_density_title()}
        </div>
        <div className="pv-settings__row" data-testid="settings-row">
          <div className="pv-settings__row-label">
            {m.settings_general_density_label()}
          </div>
          <div className="pv-settings__row-content">
            <select
              className={selectBase}
              value={density}
              onChange={(e) => setDensity(e.target.value as Density)}
            >
              <option value="compact">
                {m.settings_general_density_compact()}
              </option>
              <option value="comfortable">
                {m.settings_general_density_comfortable()}
              </option>
              <option value="spacious">
                {m.settings_general_density_spacious()}
              </option>
            </select>
          </div>
        </div>
      </div>
    </>
  );
}
