// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/** Per-root card: path/pill/meta row + kebab (⋯) action menu (issue #562). */
import { useRef, useState } from 'react';
import { Btn, Pill } from '@/ui';
import { formatDistanceToNow } from 'date-fns';
import type { LibraryRoot } from '@/bindings/types';
import { m } from '@/lib/i18n';
import { useDismiss } from '@/hooks/useDismiss';
import { SourceProtectionOverride } from './SourceProtectionOverride';
import { RootDetectionConfig } from '@/features/inventory/RootDetectionConfig';
import { revealInOs } from '@/shared/native/reveal';
import { revealLabel } from '@/lib/reveal-label';
import { addToast } from '@/shared/toast';
import { RECONCILABLE_CATEGORIES } from './datasources-model';

interface RootCardProps {
  root: LibraryRoot;
  onRescan: (root: LibraryRoot) => void;
  rescanning: boolean;
  onReconcile: (root: LibraryRoot) => void;
  reconciling: boolean;
  onRemap: (root: LibraryRoot) => void;
  onToggleActive: (root: LibraryRoot) => void;
  togglingActive: boolean;
  onDelete: (root: LibraryRoot) => void;
  deleting: boolean;
}

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: root card render with per-root-state action and status branches
export function RootCard({
  root,
  onRescan,
  rescanning,
  onReconcile,
  reconciling,
  onRemap,
  onToggleActive,
  togglingActive,
  onDelete,
  deleting,
}: RootCardProps) {
  const isOffline = !root.online;

  const [menuOpen, setMenuOpen] = useState(false);
  const [editProtectionOpen, setEditProtectionOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useDismiss([menuRef], () => setMenuOpen(false), menuOpen);

  const metaParts: string[] = [];
  if (root.fileCount != null && root.fileCount > 0) {
    metaParts.push(
      m.data_sources_file_count({
        count: root.fileCount,
        formatted: root.fileCount.toLocaleString(),
      }),
    );
  }
  if (root.lastScanned) {
    // Issue #562: humanize the raw ISO timestamp ("2 days ago" instead of
    // "2026-07-11T09:42:02.2555817Z"). Falls back to the raw value if the
    // stored timestamp somehow fails to parse.
    let humanized = root.lastScanned;
    try {
      humanized = formatDistanceToNow(new Date(root.lastScanned), {
        addSuffix: true,
      });
    } catch {
      // keep raw fallback
    }
    metaParts.push(m.settings_datasources_scanned({ date: humanized }));
  }
  const meta = metaParts.join(' · ');

  const handleReveal = async () => {
    setMenuOpen(false);
    try {
      await revealInOs(root.path, {
        entityKind: 'registered_source',
        entityId: root.id,
      });
    } catch (err: unknown) {
      addToast({
        message: typeof err === 'string' ? err : m.common_reveal_error(),
        variant: 'error',
      });
    }
  };

  return (
    <div
      className={
        'pv-data-sources__root-card' +
        (isOffline ? ' pv-data-sources__root-card--offline' : '') +
        (root.active ? '' : ' pv-data-sources__root-card--disabled')
      }
    >
      {/* Row 1: path + compact protection pill + offline/disabled pills.
          Row 2: humanized meta line. */}
      <div className="pv-data-sources__root-info">
        <div className="pv-data-sources__root-path-row">
          <code
            className="pv-mono pv-data-sources__root-path"
            data-testid="data-sources-root-path"
          >
            {root.path}
          </code>
          <SourceProtectionOverride
            sourceId={root.id}
            open={editProtectionOpen}
            onOpenChange={setEditProtectionOpen}
          />
          {isOffline && (
            <Pill variant="warn" className="pv-data-sources__offline-pill">
              {m.nav_roots_offline_suffix()}
            </Pill>
          )}
          {!root.active && (
            <Pill variant="neutral" className="pv-data-sources__disabled-pill">
              {m.settings_datasources_disabled_pill()}
            </Pill>
          )}
        </div>
        {meta && <div className="pv-data-sources__root-meta">{meta}</div>}
        {/* spec 048 US4: per-root detection config only applies to roots that
            carry `file_record` rows (raw/calibration). */}
        {RECONCILABLE_CATEGORIES.includes(root.category) && (
          <RootDetectionConfig rootId={root.id} />
        )}
      </div>

      {/* Right: kebab (⋯) menu — issue #562 consolidates every per-source
          action here instead of a scattered button row. */}
      <div className="pv-data-sources__root-actions" ref={menuRef}>
        <Btn
          size="sm"
          variant="ghost"
          className="pv-data-sources__kebab-btn"
          data-testid="data-sources-kebab-btn"
          aria-label={m.settings_datasources_actions_aria()}
          aria-haspopup="menu"
          aria-expanded={menuOpen}
          onClick={() => setMenuOpen((v) => !v)}
        >
          ⋯
        </Btn>
        {menuOpen && (
          <div className="pv-data-sources__kebab-menu" role="menu">
            {!isOffline && (
              <button
                type="button"
                role="menuitem"
                className="pv-data-sources__kebab-item"
                disabled={rescanning}
                // Stays open while in flight (unlike the other items) so the
                // disabled/relabeled "Rescanning…" state remains visible —
                // a background action, not a navigation to a dialog/panel.
                onClick={() => onRescan(root)}
              >
                {rescanning ? m.common_rescanning() : m.common_rescan()}
              </button>
            )}
            {!isOffline && RECONCILABLE_CATEGORIES.includes(root.category) && (
              <button
                type="button"
                role="menuitem"
                className="pv-data-sources__kebab-item"
                data-testid={`reconcile-now-${root.id}`}
                disabled={reconciling}
                onClick={() => onReconcile(root)}
              >
                {reconciling ? m.common_reconciling() : m.common_reconcile()}
              </button>
            )}
            <button
              type="button"
              role="menuitem"
              className="pv-data-sources__kebab-item"
              onClick={() => {
                setMenuOpen(false);
                onRemap(root);
              }}
            >
              {m.settings_datasources_remap()}
            </button>
            <button
              type="button"
              role="menuitem"
              className="pv-data-sources__kebab-item"
              onClick={() => {
                setMenuOpen(false);
                setEditProtectionOpen(true);
              }}
            >
              {m.settings_datasources_edit_protection()}
            </button>
            {!isOffline && (
              <button
                type="button"
                role="menuitem"
                className="pv-data-sources__kebab-item"
                disabled={togglingActive}
                onClick={() => {
                  setMenuOpen(false);
                  onToggleActive(root);
                }}
              >
                {root.active
                  ? togglingActive
                    ? m.common_disabling()
                    : m.settings_datasources_disable()
                  : togglingActive
                    ? m.common_enabling()
                    : m.settings_datasources_enable()}
              </button>
            )}
            <button
              type="button"
              role="menuitem"
              className="pv-data-sources__kebab-item"
              onClick={() => void handleReveal()}
            >
              {revealLabel()}
            </button>
            {/* #559: Delete was only reachable for offline roots before this
                fix — the backend already blocks it server-side when the
                source has dependents (has_dependents), surfaced in the
                confirm dialog below. */}
            <button
              type="button"
              role="menuitem"
              className="pv-data-sources__kebab-item pv-data-sources__kebab-item--danger"
              disabled={deleting}
              onClick={() => {
                setMenuOpen(false);
                onDelete(root);
              }}
            >
              {deleting ? m.common_deleting() : m.settings_datasources_delete()}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
