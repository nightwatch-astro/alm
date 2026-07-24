// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { clsx } from 'clsx';
import { m } from '@/lib/i18n';
import { useLogPanel } from './LogPanelContext';
import { useOperationStatus } from './OperationStatusContext';
import { usePageStatus } from './PageStatusContext';
import { useStatusSummary } from './useStatusSummary';
import {
  statusBar,
  sep as statusBarSep,
  right as statusBarRight,
  op as statusBarOp,
  idle as statusBarIdle,
  spinner as statusBarSpinner,
  vol as statusBarVol,
  volWarn as statusBarVolWarn,
  meter as statusBarMeter,
  logToggle as statusBarLogToggle,
} from './statusbar.css';

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function formatCount(n: number): string {
  return n.toLocaleString();
}

/**
 * Status bar (design v4): three zones. LEFT = the current operation (transient),
 * CENTER = GLOBAL library inventory totals (task #87 — stable across routes,
 * NOT per-page counts), RIGHT = persistent storage/cleanup health + log toggle.
 *
 * The global totals come from `useStatusSummary()` → `commands.statusSummary()`
 * (the real `LibraryStats` DTO: sessions / projects / calibration sets /
 * targets). There is no library-wide image/file count in the contract yet (the
 * Advanced settings "Records" line is hardcoded); rather than show a fabricated
 * or dev-status-leaking placeholder (fix ef364dfc), the image total is simply
 * omitted from this bar until the metadata-ingest pipeline exposes a real count.
 */
export function StatusBar() {
  const { toggle } = useLogPanel();
  const { status: opStatus, statusLabel } = useOperationStatus();
  const status = useStatusSummary();
  const pageStatus = usePageStatus();
  // Compare the stable `status` enum, not `statusLabel` (a translated,
  // locale-dependent string — spec 046 #8).
  const isActive = opStatus !== 'idle';

  return (
    <div className={statusBar}>
      {/* LEFT — current operation */}
      <div className={statusBarOp}>
        {isActive ? (
          <>
            <span className={statusBarSpinner} />
            <span>{statusLabel}</span>
          </>
        ) : (
          <span className={statusBarIdle}>{m.status_ready()}</span>
        )}
      </div>

      {/* PAGE-CONTEXTUAL — shown only when the active page populates the slot
          (currently: Inbox folder/master count + per-frame-type breakdown).
          Sits left-of-center and is cleared automatically on route change. */}
      {pageStatus != null && (
        <div className="pv-statusbar__page" data-testid="statusbar-page-status">
          {pageStatus}
        </div>
      )}

      {/* CENTER — GLOBAL library inventory (stable across routes, task #87).
          Sessions / projects / calibration masters / targets are real totals
          from the status_summary contract. No images/file total is shown here —
          there is no library-wide count in the contract yet, and a fabricated
          or dev-status placeholder was deliberately removed (fix ef364dfc). */}
      <div className="pv-statusbar__lib" data-testid="statusbar-library-stats">
        <span title={m.status_sessions_title()}>
          {formatCount(status.sessionCount)}{' '}
          {m.status_sessions_label({ count: status.sessionCount })}
        </span>
        <span className={statusBarSep}>·</span>
        <span title={m.status_projects_title()}>
          {formatCount(status.projectCount)}{' '}
          {m.status_projects_label({ count: status.projectCount })}
        </span>
        <span className={statusBarSep}>·</span>
        <span title={m.status_masters_title()}>
          {formatCount(status.calibrationCount)}{' '}
          {m.status_masters_label({ count: status.calibrationCount })}
        </span>
      </div>

      {/* RIGHT — persistent storage / cleanup health + log */}
      <div className={statusBarRight}>
        {status.cleanupReclaimableBytes > 0 && (
          <span>
            {formatBytes(status.cleanupReclaimableBytes)}{' '}
            {m.status_reclaimable()}
          </span>
        )}
        {status.volumes.map((vol) => {
          const usedPct =
            vol.totalBytes > 0
              ? Math.round(
                  ((vol.totalBytes - vol.freeBytes) / vol.totalBytes) * 100,
                )
              : 0;
          const label = vol.path.split(/[\\/]/).pop() || vol.path;
          return (
            <span
              key={vol.path}
              className={clsx(statusBarVol, vol.warning && statusBarVolWarn)}
              title={m.statusbar_vol_title({
                path: vol.path,
                free: formatBytes(vol.freeBytes),
                total: formatBytes(vol.totalBytes),
              })}
            >
              <span>{label}</span>
              <span className={statusBarMeter}>
                {/* eslint-disable-next-line no-restricted-syntax -- dynamic: disk usage meter width % */}
                <i style={{ width: `${usedPct}%` }} />
              </span>
              <span>{usedPct}%</span>
            </span>
          );
        })}
        <button
          type="button"
          className={statusBarLogToggle}
          onClick={toggle}
          aria-label={m.status_log_toggle_aria()}
        >
          {m.status_log_label()}
        </button>
      </div>
    </div>
  );
}
