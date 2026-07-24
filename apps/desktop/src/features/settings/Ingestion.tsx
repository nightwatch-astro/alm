// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// spec 030 (ingestion settings) — package P12: real backend persistence.
//
// Owned backend keys (IngestionSettings / UpdateIngestionSettings, its own
// dedicated `ingestion.settings.get` / `ingestion.settings.update` commands —
// NOT the generic settings.get/update scope mechanism):
//   - scanOnStartup  — "Scan on startup" toggle
//   - followSymlinks — "Follow symbolic links" toggle
//   - followJunctions — "Follow NTFS junctions" toggle
//   - hashingMode ("lazy" | "eager" | "off") — "File hashing" selector
//
// The contract also carries watcherEnabled, metadataExtraction, the
// exposure/temperature grouping tolerances, and defaultFilter — none of which
// this pane renders (matching the authoritative design,
// docs/design/screenshots/09-settings-ingestion.png). `persist()` round-trips
// those unrendered fields from loaded state so saving a rendered toggle never
// clobbers them, the same pattern CalibrationMatching.tsx uses for its
// backend-unsupported fields.
//
// CONSUMERS (issue #878):
//   - followSymlinks IS enforced: `inbox.scan.folder` resolves it through
//     `app_core::inbox_scan::resolve_scan_options`, so it decides whether the
//     Inbox scan traverses links. Covered by
//     crates/app/core/tests/ingestion_scan_settings_integration.rs.
//   - followJunctions, scanOnStartup and hashingMode have no consumer, so they
//     render disabled rather than as working controls. The scanner classes
//     junctions with symlinks under one flag
//     (`fs_pathsafe::is_link_or_junction`), no startup-scan path exists, and
//     folder signatures always use a partial hash. Giving each an independent
//     effect is a feature, not wiring; they stay persisted so enabling them
//     later needs no migration.
import { useState, useEffect, useRef } from 'react';
import { Toggle } from '@/ui';
import { m } from '@/lib/i18n';
import {
  SettingsSection,
  SettingsRow,
  RestoreDefaultsBtn,
} from './SettingsKit';
import {
  ingestionSettingsGet,
  ingestionSettingsUpdate,
  type IngestionSettings,
  type UpdateIngestionSettings,
} from './settingsIpc';
import { selectBase } from '@/styles/select.css';

interface IngestionProps {
  /** Unused in this pane — ingestion settings use their own IPC commands
   *  (settingsIpc.ingestionSettingsGet/Update), not the scope-based
   *  save(scope, values) mechanism. Kept for prop-shape consistency with
   *  sibling settings panes. */
  save: (scope: string, values: Record<string, unknown>) => void;
}

type HashingMode = 'lazy' | 'eager' | 'off';

const DEFAULTS: UpdateIngestionSettings = {
  watcherEnabled: true,
  scanOnStartup: true,
  followSymlinks: false,
  followJunctions: false,
  hashingMode: 'lazy',
  metadataExtraction: true,
  exposureGroupingToleranceS: 2,
  temperatureGroupingToleranceC: 5,
  defaultFilter: null,
};

export function Ingestion(_props: IngestionProps) {
  // Full persisted state, including fields this pane doesn't render — needed
  // so `persist()` can send a complete `UpdateIngestionSettings` without
  // clobbering them.
  const [settings, setSettings] = useState<UpdateIngestionSettings>(DEFAULTS);

  // Guards against the initial `ingestionSettingsGet()` fetch resolving
  // *after* the user has already edited a control (a real race, not just a
  // CI timing artifact: on a slower/more contended machine the mount fetch
  // can still be in flight when the user's first click fires). Without this,
  // the late `setSettings(loaded)` below stomps the user's optimistic edit
  // back to the stale fetched value.
  const editedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    ingestionSettingsGet()
      .then((loaded: IngestionSettings) => {
        if (cancelled || editedRef.current) return;
        setSettings(loaded);
      })
      .catch(() => {
        // Backend unavailable — stay with in-code defaults.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  function persist(patch: Partial<UpdateIngestionSettings>) {
    editedRef.current = true;
    const next: UpdateIngestionSettings = { ...settings, ...patch };
    setSettings(next);
    ingestionSettingsUpdate(next).catch(() => {
      // Best-effort persist; UI already reflects the change.
    });
  }

  const handleRestoreDefaults = async () => {
    editedRef.current = true;
    const persisted = await ingestionSettingsUpdate(DEFAULTS);
    setSettings(persisted);
  };

  return (
    <>
      <SettingsSection
        title={m.settings_ingestion_scan_title()}
        action={<RestoreDefaultsBtn onRestore={handleRestoreDefaults} />}
      >
        <SettingsRow
          label={m.settings_ingestion_scan_startup()}
          info={m.settings_ingestion_scan_info()}
        >
          <Toggle
            aria-label={m.settings_ingestion_scan_startup()}
            checked={settings.scanOnStartup}
            disabled
            onChange={(v) => persist({ scanOnStartup: v })}
          />
        </SettingsRow>

        <SettingsRow
          label={m.settings_ingestion_follow_symlinks()}
          info={m.settings_ingestion_symlinks_info()}
        >
          <Toggle
            aria-label={m.settings_ingestion_follow_symlinks()}
            checked={settings.followSymlinks}
            onChange={(v) => persist({ followSymlinks: v })}
          />
        </SettingsRow>

        <SettingsRow
          label={m.settings_ingestion_follow_junctions()}
          info={m.settings_ingestion_junctions_info()}
        >
          <Toggle
            aria-label={m.settings_ingestion_follow_junctions()}
            checked={settings.followJunctions}
            disabled
            onChange={(v) => persist({ followJunctions: v })}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={m.settings_ingestion_hashing_title()}>
        <SettingsRow
          label={m.settings_ingestion_hashing_mode()}
          info={m.settings_ingestion_hashing_info()}
        >
          <select
            className={selectBase}
            aria-label={m.settings_ingestion_hashing_mode()}
            disabled
            value={settings.hashingMode}
            onChange={(e) => {
              const v = e.target.value as HashingMode;
              persist({ hashingMode: v });
            }}
          >
            <option value="lazy">{m.settings_ingestion_hashing_lazy()}</option>
            <option value="eager">
              {m.settings_ingestion_hashing_eager()}
            </option>
            <option value="off">{m.settings_ingestion_hashing_off()}</option>
          </select>
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
