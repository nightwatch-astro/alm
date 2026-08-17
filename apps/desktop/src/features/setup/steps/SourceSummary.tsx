// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SourceSummary — per-source scan result card for the "Scan" step (spec 038).
 *
 * Shows the source path, scan phase pill, expandable detection breakdown
 * (kind-count chips + metadata table), and — for frame sources with a real
 * backend root — the RootDetectionConfig inline (spec 048 US4 T035).
 *
 * Extracted from StepScan.tsx (refactor sweep kyo7.104) so the #513
 * reconciliation logic lives with its component rather than embedded in the
 * orchestration.
 */

import { useState } from 'react';
import { Pill } from '@/ui/Pill';
import type { PillVariant } from '@/ui/Pill';
import { Table } from '@/ui';
import type { TableColumn, TableRow } from '@/ui';
import { m } from '@/lib/i18n';
import { frameTypeCountLabel, masterLabel } from '@/lib/master-label';
import type { InboxItemSummary } from '@/bindings/index';
import type { InboxClassifyResponse } from '@/bindings/aliases';
import type { SourceEntry } from '../sources-store';
import { sourceKindLabel } from '../sources-store';
import { RootDetectionConfig } from '@/features/inventory/RootDetectionConfig';
import type { ScanPhase } from './step-scan-model';

// ── SourceScanState (shared with StepScan) ────────────────────────────────────

export interface SourceScanState {
  source: SourceEntry;
  /** rootId returned by roots_register_batch for this source. */
  rootId: string;
  phase: ScanPhase;
  items: InboxItemSummary[];
  /** Per-item classify results keyed by inboxItemId. */
  classifications: Map<string, InboxClassifyResponse>;
  /**
   * Items whose classify call FAILED, as opposed to succeeding with nothing to
   * report. Without this the two are indistinguishable downstream: a missing
   * map entry renders exactly like an empty classification, so a crashed
   * folder showed an em dash and silently dropped out of the per-kind chips
   * while still counting toward the header's file total — the same
   * reconciliation gap issue #513 closed for unmapped IMAGETYP.
   */
  classifyFailures: Set<string>;
  error?: string;
}

// ── Label helpers ─────────────────────────────────────────────────────────────

function phaseLabel(phase: ScanPhase): string {
  switch (phase) {
    case 'pending':
      return m.setup_scan_phase_pending();
    case 'scanning':
      return m.setup_scan_scanning();
    case 'done':
      return m.setup_scan_phase_done();
    case 'error':
      return m.settings_advanced_log_error();
  }
}

function phasePillVariant(phase: ScanPhase): PillVariant {
  switch (phase) {
    case 'pending':
      return 'neutral';
    case 'scanning':
      return 'warn';
    case 'done':
      return 'ok';
    case 'error':
      return 'danger';
  }
}

function scanKindCountLabel(kind: string, count: number): string {
  switch (kind) {
    case 'master':
      return m.setup_scan_master_count({ count });
    case 'unclassified':
      return m.setup_scan_unclassified_count({ count });
    case 'failed':
      return m.setup_scan_classify_failed_count({ count });
    default:
      return frameTypeCountLabel(kind, count);
  }
}

// ── SourceSummary ─────────────────────────────────────────────────────────────

interface SourceSummaryProps {
  state: SourceScanState;
}

export function SourceSummary({ state }: SourceSummaryProps) {
  const {
    source,
    rootId,
    phase,
    items,
    classifications,
    classifyFailures,
    error,
  } = state;
  const [expanded, setExpanded] = useState(false);

  // Spec 048 US4 T035: the wizard's Scan step is the earliest point a newly
  // added raw/calibration root has a real backend id (roots.register.batch
  // already ran via flushToDB before this step mounts) — a dedicated new
  // wizard step was rejected as unwarranted churn for one control (see
  // RootDetectionConfig's doc comment). getRootId() falls back to the raw
  // path string when registration failed; skip rendering in that case since
  // it is not a valid root id.
  const hasRegisteredRoot = rootId !== source.path;
  const isFrameSource =
    source.kind === 'light_frames' || source.kind === 'calibration';

  const totalItems = items.length;
  const totalFiles = items.reduce((acc, it) => acc + it.fileCount, 0);

  // Aggregate breakdown counts across all items, reconciled against the file
  // count (issue #513): masters and unclassified files were previously
  // dropped from this summary because they don't appear in a classify
  // breakdown entry, so the chip counts silently undercounted totalFiles.
  const kindCounts = new Map<string, number>();
  for (const item of items) {
    if (item.isMaster) {
      kindCounts.set(
        'master',
        (kindCounts.get('master') ?? 0) + item.fileCount,
      );
      continue;
    }
    if (classifyFailures.has(item.inboxItemId)) {
      // Count them so the chips still add up to the header total; a folder that
      // failed must not simply vanish from the breakdown.
      kindCounts.set(
        'failed',
        (kindCounts.get('failed') ?? 0) + item.fileCount,
      );
      continue;
    }
    const cls = classifications.get(item.inboxItemId);
    for (const entry of cls?.breakdown ?? []) {
      kindCounts.set(
        entry.kind,
        (kindCounts.get(entry.kind) ?? 0) + entry.count,
      );
    }
    const unclassifiedCount = cls?.unclassifiedFiles.length ?? 0;
    if (unclassifiedCount > 0) {
      kindCounts.set(
        'unclassified',
        (kindCounts.get('unclassified') ?? 0) + unclassifiedCount,
      );
    }
  }

  // Stable sort for the table: by lane, then folder path.
  const sortedItems = [...items].sort(
    (a, b) =>
      a.lane.localeCompare(b.lane) ||
      a.relativePath.localeCompare(b.relativePath),
  );

  // Built per render rather than hoisted to module scope: the `m.*` accessors
  // resolve against the active locale at call time, so hoisting would freeze
  // the header labels to whichever locale was active at import.
  const columns: TableColumn[] = [
    { key: 'folder', label: m.setup_scan_col_folder() },
    { key: 'files', label: m.setup_scan_col_files() },
    { key: 'format', label: m.setup_scan_col_format() },
    { key: 'types', label: m.setup_scan_col_types() },
  ];

  const rows: TableRow[] = sortedItems.map((item) => {
    const cls = classifications.get(item.inboxItemId);
    const typeParts = (cls?.breakdown ?? []).map((b) =>
      frameTypeCountLabel(b.kind, b.count),
    );
    // Reconcile against fileCount (issue #513): files with no IMAGETYP (or an
    // unmapped one) don't appear in the classify breakdown at all, so the type
    // list previously undercounted the row's file total with no indication why.
    const unclassifiedCount = cls?.unclassifiedFiles.length ?? 0;
    if (unclassifiedCount > 0) {
      typeParts.push(
        m.setup_scan_unclassified_count({ count: unclassifiedCount }),
      );
    }
    const types = typeParts.join(', ');
    return {
      _testid: `scan-item-${item.inboxItemId}`,
      folder: (
        <span className="pv-table__cell-inline">
          {item.isMaster && <Pill variant="info">{m.setup_scan_master()}</Pill>}
          {/* The root/top-level row of an expanded folder carries an empty
              relativePath; label it instead of leaving it blank (issue #513). */}
          {item.relativePath || m.setup_scan_root_label()}
        </span>
      ),
      files: item.fileCount,
      format: (item.format ?? item.lane).toUpperCase(),
      // Individual calibration masters carry their frame type on the item
      // itself (spec 040 FR-006); grouped sub-frame folders rely on the
      // classify breakdown instead.
      types: item.isMaster
        ? masterLabel(item)
        : classifyFailures.has(item.inboxItemId)
          ? m.setup_scan_classify_failed()
          : types || '—',
    };
  });

  // The detail panel (chips + table) is expandable only when scan is done and
  // there are items to show.
  const isExpandable = phase === 'done' && totalItems > 0;

  // Compact count summary for the always-visible header line.
  const countSummary =
    phase === 'done' && totalItems > 0
      ? `${m.setup_scan_folder_count({ count: totalItems })} · ${m.setup_scan_file_count({ count: totalFiles })}`
      : null;

  return (
    <div
      data-testid={`scan-source-${source.path}`}
      className="pv-setup-scan__card"
    >
      {/* ── Always-visible header row ─────────────────────────────────────── */}
      {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions -- conditionally interactive: role=button + keyboard (Enter/Space) handler applied together only when isExpandable; inert div otherwise */}
      <div
        role={isExpandable ? 'button' : undefined}
        // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- tabIndex is paired with role=button (above) only when isExpandable, making the element a focusable button
        tabIndex={isExpandable ? 0 : undefined}
        aria-expanded={isExpandable ? expanded : undefined}
        onClick={isExpandable ? () => setExpanded((v) => !v) : undefined}
        onKeyDown={
          isExpandable
            ? (e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  setExpanded((v) => !v);
                }
              }
            : undefined
        }
        className={
          'pv-setup-scan__header' +
          (isExpandable ? ' pv-setup-scan__header--expandable' : '')
        }
      >
        {/* Chevron — only shown when expandable */}
        {isExpandable && (
          <span aria-hidden="true" className="pv-setup-scan__chevron">
            {expanded ? '▾' : '▸'}
          </span>
        )}

        {/* Source path */}
        <span className="pv-setup-scan__path">{source.path}</span>

        {/* Kind label (muted, small) */}
        <span className="pv-setup-scan__kind">
          {sourceKindLabel(source.kind)}
        </span>

        {/* Compact count summary */}
        {countSummary && (
          <span className="pv-setup-scan__count">{countSummary}</span>
        )}

        {/* Phase pill */}
        <span aria-hidden="true">
          <Pill variant={phasePillVariant(phase)}>{phaseLabel(phase)}</Pill>
        </span>
      </div>

      {phase !== 'error' && (
        <span
          className="pv-visually-hidden"
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          {m.setup_scan_source_status({
            path: source.path,
            status: phaseLabel(phase),
          })}
        </span>
      )}

      {/* Spec 048 US4 T035: per-root reconcile mode + detection triggers,
          set up at add-time with documented defaults pre-selected. */}
      {isFrameSource && hasRegisteredRoot && (
        <RootDetectionConfig rootId={rootId} />
      )}

      {/* ── Always-visible transient states (below header, outside collapse) ── */}
      {/* These are small/short and don't benefit from collapse. */}
      {phase === 'error' && error && (
        <p
          className="pv-setup-scan__msg pv-setup-scan__msg--error"
          role="alert"
        >
          {m.setup_scan_source_error({ path: source.path, error })}
        </p>
      )}

      {phase === 'done' && totalItems === 0 && (
        <p
          data-testid="scan-empty"
          className="pv-setup-scan__msg pv-setup-scan__msg--empty"
        >
          {m.setup_scan_nothing_detected()}
        </p>
      )}

      {/* ── Collapsible detail panel (chips + table) ─────────────────────── */}
      {isExpandable && expanded && (
        <div className="pv-setup-scan__detail">
          {/* Kind-count chips */}
          {kindCounts.size > 0 && (
            <div className="pv-setup-scan__chips">
              {Array.from(kindCounts.entries()).map(([kind, count]) => (
                <span key={kind} className="pv-setup-scan__chip">
                  {scanKindCountLabel(kind, count)}
                </span>
              ))}
            </div>
          )}

          {/* Scrollable metadata table of detected ingestion groups.
              `virtualized` is used for its scroll container + sticky header
              (primitives.css `.pv-table__scroll`), not for windowing — a
              per-source detection list is small. */}
          <Table
            columns={columns}
            rows={rows}
            virtualized
            scrollClassName="pv-setup-scan__table-wrap"
          />
        </div>
      )}
    </div>
  );
}
