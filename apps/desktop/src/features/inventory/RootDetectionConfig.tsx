// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Per-root reconcile mode + detection-trigger controls (spec 048 US4,
 * T034-T036 frontend). `inventory.root_config.{get,set}` (T034, backend
 * already real) had no UI surface at all before this component.
 *
 * Mounted in two places, sharing this one component rather than duplicating
 * the markup (code economy):
 *   - `features/settings/DataSources.tsx` — existing root settings (T036).
 *   - `features/setup/steps/StepScan.tsx` — the first-run wizard's Scan step,
 *     the earliest point a newly-added root has a real backend id from
 *     `roots.register.batch` (T035). A dedicated new wizard STEP was
 *     considered and rejected: it would require step-count/indicator/test
 *     churn across the whole wizard shell for a control that fits naturally
 *     next to each source's existing per-root detail card.
 *
 * Reuses the existing `pv-settings__row*` classes (Data Sources' own
 * per-source override panel) instead of introducing new CSS.
 *
 * Split into an outer toggle (`RootDetectionConfig`, no query hooks) and an
 * inner panel (`RootDetectionConfigPanel`, mounted only once expanded) so a
 * page that renders a root list — several existing `DataSources` tests, for
 * example — does not require a `QueryClientProvider` ancestor merely because
 * a per-root detection control exists somewhere on the page; the hooks only
 * run once a user actually opens the panel.
 */

import { useState } from 'react';
import { Btn, Toggle } from '@/ui';
import { m } from '@/lib/i18n';
import { errMessage } from '@/lib/errors';
import { useRootConfig, useSetRootConfig } from './store';
import type { ReconcileMode } from '@/bindings/index';
import { selectBase } from '@/styles/select.css';

export interface RootDetectionConfigProps {
  rootId: string;
}

export function RootDetectionConfig({ rootId }: RootDetectionConfigProps) {
  const [expanded, setExpanded] = useState(false);

  if (!expanded) {
    return (
      <Btn
        size="sm"
        variant="ghost"
        onClick={() => setExpanded(true)}
        data-testid={`root-detection-toggle-${rootId}`}
      >
        {m.inventory_detection_toggle_btn()}
      </Btn>
    );
  }

  return (
    <RootDetectionConfigPanel
      rootId={rootId}
      onClose={() => setExpanded(false)}
    />
  );
}

function RootDetectionConfigPanel({
  rootId,
  onClose,
}: {
  rootId: string;
  onClose: () => void;
}) {
  const { data, isLoading, error } = useRootConfig(rootId);
  const setConfig = useSetRootConfig(rootId);

  return (
    <div
      className="pv-settings__group"
      data-testid={`root-detection-${rootId}`}
    >
      <div
        className="pv-settings__group-title"
        data-testid="settings-group-title"
      >
        {m.inventory_detection_title()}
        <Btn size="sm" variant="ghost" onClick={onClose}>
          {m.common_close()}
        </Btn>
      </div>

      {isLoading && (
        <div className="pv-data-sources__status">{m.common_loading()}</div>
      )}
      {error && (
        <div className="pv-data-sources__load-error">{errMessage(error)}</div>
      )}

      {data && (
        <>
          <div className="pv-settings__row" data-testid="settings-row">
            <div className="pv-settings__row-label">
              {m.inventory_detection_mode_label()}
            </div>
            <div className="pv-settings__row-content">
              <select
                className={selectBase}
                value={data.reconcileMode}
                aria-label={m.inventory_detection_mode_label()}
                onChange={(e) =>
                  setConfig.mutate({
                    reconcileMode: e.target.value as ReconcileMode,
                  })
                }
              >
                <option value="flag_missing">
                  {m.inventory_detection_mode_flag()}
                </option>
                <option value="auto_reconcile">
                  {m.inventory_detection_mode_auto()}
                </option>
              </select>
            </div>
          </div>

          <div className="pv-settings__row" data-testid="settings-row">
            <div className="pv-settings__row-label">
              {m.inventory_detection_live_label()}
            </div>
            <div className="pv-settings__row-content">
              <Toggle
                aria-label={m.inventory_detection_live_label()}
                checked={data.detection.live}
                onChange={(v) =>
                  setConfig.mutate({
                    detection: {
                      live: v,
                      scheduled: null,
                      onOpen: null,
                      followSymlinks: null,
                    },
                  })
                }
              />
            </div>
          </div>

          <div className="pv-settings__row" data-testid="settings-row">
            <div className="pv-settings__row-label">
              {m.inventory_detection_scheduled_label()}
            </div>
            <div className="pv-settings__row-content">
              <Toggle
                aria-label={m.inventory_detection_scheduled_label()}
                checked={data.detection.scheduled}
                onChange={(v) =>
                  setConfig.mutate({
                    detection: {
                      live: null,
                      scheduled: v,
                      onOpen: null,
                      followSymlinks: null,
                    },
                  })
                }
              />
            </div>
          </div>

          <div className="pv-settings__row" data-testid="settings-row">
            <div className="pv-settings__row-label">
              {m.inventory_detection_on_open_label()}
            </div>
            <div className="pv-settings__row-content">
              <Toggle
                aria-label={m.inventory_detection_on_open_label()}
                checked={data.detection.onOpen}
                onChange={(v) =>
                  setConfig.mutate({
                    detection: {
                      live: null,
                      scheduled: null,
                      onOpen: v,
                      followSymlinks: null,
                    },
                  })
                }
              />
            </div>
          </div>

          <div className="pv-settings__row" data-testid="settings-row">
            <div className="pv-settings__row-label">
              {m.inventory_detection_follow_symlinks_label()}
            </div>
            <div className="pv-settings__row-content">
              <Toggle
                aria-label={m.inventory_detection_follow_symlinks_label()}
                checked={data.detection.followSymlinks}
                onChange={(v) =>
                  setConfig.mutate({
                    detection: {
                      live: null,
                      scheduled: null,
                      onOpen: null,
                      followSymlinks: v,
                    },
                  })
                }
              />
            </div>
          </div>

          {setConfig.isError && (
            <div className="pv-data-sources__add-error">
              {errMessage(setConfig.error)}
            </div>
          )}
        </>
      )}
    </div>
  );
}
