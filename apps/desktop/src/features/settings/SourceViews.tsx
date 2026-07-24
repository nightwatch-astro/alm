// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SourceViews — Settings → Source Views pane (spec 049 T030).
 *
 * Owned keys: `sourceViewLinkKindIntraDrive`, `sourceViewLinkKindCrossDrive`
 * (backend scope `"sourceViews"`, `apps/desktop/src-tauri/src/commands/settings.rs`).
 * Loads from `settings.get('sourceViews')` on mount and auto-saves via the
 * shared `save()` prop, matching every other settings pane's convention.
 *
 * Capability-constrained per FR-004a/FR-004c: cross-drive never offers
 * `hardlink` (hardlinks cannot cross volumes — the resolver treats it as
 * `symlink` defensively even if written directly). There is no live
 * per-drive-scope filesystem-capability *preview* command yet (a real probe
 * needs a new contract/command — tracked as a T029/T030 follow-up, same gap
 * already documented in `GenerateSourceViewDialog.tsx`), so this pane surfaces
 * a Developer Mode note about capability-drift fallback instead of fabricating
 * a pre-select achievability check.
 */
import { useState, useEffect, useRef } from 'react';
import { getSettings } from './settingsIpc';
import { m } from '@/lib/i18n';
import {
  SettingsSection,
  SettingsRow,
  RestoreDefaultsBtn,
} from './SettingsKit';
import { selectBase } from '@/styles/select.css';

const SOURCE_VIEWS_KEYS = [
  'sourceViewLinkKindIntraDrive',
  'sourceViewLinkKindCrossDrive',
];

type LinkKind = 'symlink' | 'hardlink' | 'junction';

const INTRA_DRIVE_KINDS: LinkKind[] = ['hardlink', 'symlink', 'junction'];
const CROSS_DRIVE_KINDS: LinkKind[] = ['symlink', 'junction'];

function linkKindLabel(kind: LinkKind): string {
  switch (kind) {
    case 'hardlink':
      return m.settings_source_views_kind_hardlink();
    case 'symlink':
      return m.settings_source_views_kind_symlink();
    case 'junction':
      return m.settings_source_views_kind_junction();
  }
}

interface SourceViewsProps {
  save: (scope: string, values: Record<string, unknown>) => void;
}

export function SourceViews({ save }: SourceViewsProps) {
  const [intraDrive, setIntraDrive] = useState<LinkKind>('hardlink');
  const [crossDrive, setCrossDrive] = useState<LinkKind>('symlink');

  // Guards the mount-time fetch against clobbering an in-flight user edit
  // (same convention as Cleanup.tsx/DataSources.tsx).
  const editedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    getSettings({ scope: 'sourceViews' })
      .then((data) => {
        if (cancelled || editedRef.current) return;
        const values = data.values as Record<string, unknown>;
        if (typeof values?.sourceViewLinkKindIntraDrive === 'string') {
          setIntraDrive(values.sourceViewLinkKindIntraDrive as LinkKind);
        }
        if (typeof values?.sourceViewLinkKindCrossDrive === 'string') {
          setCrossDrive(values.sourceViewLinkKindCrossDrive as LinkKind);
        }
      })
      .catch(() => {
        // Backend unavailable — stay with in-code defaults.
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <SettingsSection
      title={m.settings_source_views_title()}
      action={
        <RestoreDefaultsBtn
          scope="sourceViews"
          keys={SOURCE_VIEWS_KEYS}
          scopeLabel={m.settings_source_views_restore_scope()}
          onRestored={(values) => {
            editedRef.current = false;
            if (typeof values.sourceViewLinkKindIntraDrive === 'string') {
              setIntraDrive(values.sourceViewLinkKindIntraDrive as LinkKind);
            }
            if (typeof values.sourceViewLinkKindCrossDrive === 'string') {
              setCrossDrive(values.sourceViewLinkKindCrossDrive as LinkKind);
            }
          }}
        />
      }
    >
      <SettingsRow
        label={m.settings_source_views_intra_drive_label()}
        info={m.settings_source_views_intra_drive_info()}
      >
        <select
          className={selectBase}
          value={intraDrive}
          aria-label={m.settings_source_views_intra_drive_label()}
          data-testid="source-views-intra-drive-select"
          onChange={(e) => {
            editedRef.current = true;
            const v = e.target.value as LinkKind;
            setIntraDrive(v);
            save('sourceViews', { sourceViewLinkKindIntraDrive: v });
          }}
        >
          {INTRA_DRIVE_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {linkKindLabel(kind)}
            </option>
          ))}
        </select>
      </SettingsRow>

      <SettingsRow
        label={m.settings_source_views_cross_drive_label()}
        info={m.settings_source_views_cross_drive_info()}
      >
        <select
          className={selectBase}
          value={crossDrive}
          aria-label={m.settings_source_views_cross_drive_label()}
          data-testid="source-views-cross-drive-select"
          onChange={(e) => {
            editedRef.current = true;
            const v = e.target.value as LinkKind;
            setCrossDrive(v);
            save('sourceViews', { sourceViewLinkKindCrossDrive: v });
          }}
        >
          {CROSS_DRIVE_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {linkKindLabel(kind)}
            </option>
          ))}
        </select>
      </SettingsRow>

      <p className="text-muted text-xs">
        {m.settings_source_views_drift_note()}
      </p>
    </SettingsSection>
  );
}
