// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionsTable — spec 043 §4 Sessions redesign (task #36) + multi-level
 * grouping (spec 043 gold-standard, shared groupByDimensions engine).
 *
 * The table receives an ordered `dims` prop from SessionsPage (fed by
 * `useGrouping`) and produces collapsible multi-level group headers via the
 * shared `groupByDimensions` + `flattenVisibleGroups` + `useCollapsibleGroups`
 * machinery. When `dims` is empty the table renders a flat sorted list (no
 * synthetic "All" header).
 *
 * Columns: Target · Filter · Frames · Integration · Night · Camera · Projects.
 * Sortable column headers call `onSort`; the active column's `<th>` announces
 * `aria-sort` via the shared Table + `ariaSortFor`. Selecting a row opens the
 * existing SessionDetail in the bottom detail panel on SessionsPage.
 *
 * Inbox-parity (spec 043 §4): the table renders inside the shared
 * `.pv-listtable` viewport, windows its rows via the shared Table's
 * `virtualized` padding-spacer mode (sticky header included), carries a
 * per-row `sessions-row-<id>` testid, shows the row's target identity in the
 * Target cell even when flat (the default), and pins a grouping-hint footer
 * when grouping is active.
 *
 * Spec 041 FR-051 (T076, Phase 13): the State column and the "needs review"
 * warning icon were removed along with the session review-state machine —
 * sessions are derived, already-confirmed inventory.
 */

import { useMemo, type ReactNode } from 'react';
import type { InventorySource, InventorySession } from '@/bindings/index';
import { Table, Pill, Skeleton, tableIndent } from '@/ui';
import { SortHeader, ariaSortFor } from '@/components';
import { m } from '@/lib/i18n';
import type { TableColumn, TableRow } from '@/ui';
import {
  groupByDimensions,
  flattenVisibleGroups,
  type DimensionAccessor,
} from '@/lib/grouping';
import { useCollapsibleGroups } from '@/lib/use-grouping';
import { sessionDisplayName } from './displayName';
import { integrationSeconds, integrationLabel } from './integration';
import { connectivityLabel, connectivityVariant } from './connectivity';

// ── Sort model ────────────────────────────────────────────────────────────────

export type SessionSortCol =
  | 'target'
  | 'filter'
  | 'frames'
  | 'exposure'
  | 'night'
  | 'camera';
export type SortDir = 'asc' | 'desc';

export interface SessionSort {
  col: SessionSortCol;
  dir: SortDir;
}

export const DEFAULT_SESSION_SORT: SessionSort = { col: 'night', dir: 'desc' };

// ── Grouping dimensions ────────────────────────────────────────────────────────

export const SESSION_ACCESSORS: Readonly<
  Record<string, DimensionAccessor<InventorySession>>
> = {
  target: (s) => s.target ?? s.name,
  filter: (s) => s.filter,
  night: (s) => s.capturedOn,
  camera: (s) => s.camera,
  month: (s) => s.capturedOn?.slice(0, 7),
};

/**
 * Locale-aware label per grouping dimension — the single source for the page's
 * Group-by options AND the table's grouping-hint footer (render-time thunks so
 * labels re-read the active locale, spec 046 #8).
 */
export const SESSION_DIM_LABELS: Readonly<Record<string, () => string>> = {
  target: () => m.projects_create_target_label(),
  filter: () => m.common_filter(),
  night: () => m.sessions_col_night(),
  camera: () => m.settings_calmatch_camera(),
  month: () => m.sessions_dim_month(),
};

// ── Sort helpers ────────────────────────────────────────────────────────────────

function compareStr(
  a: string | null | undefined,
  b: string | null | undefined,
): number {
  return (a ?? '').localeCompare(b ?? '');
}

function compareSessions(
  a: InventorySession,
  b: InventorySession,
  sort: SessionSort,
): number {
  let cmp = 0;
  switch (sort.col) {
    case 'target':
      cmp = compareStr(a.target ?? a.name, b.target ?? b.name);
      break;
    case 'filter':
      cmp = compareStr(a.filter, b.filter);
      break;
    case 'frames':
      cmp = a.frames - b.frames;
      break;
    case 'exposure':
      // Sorts the "Integration" column by TOTAL integration time (#798),
      // matching what the column now displays — not the raw per-frame
      // exposure string.
      cmp = (integrationSeconds(a) ?? 0) - (integrationSeconds(b) ?? 0);
      break;
    case 'night':
      cmp = compareStr(a.capturedOn, b.capturedOn);
      break;
    case 'camera':
      cmp = compareStr(a.camera, b.camera);
      break;
  }
  return sort.dir === 'asc' ? cmp : -cmp;
}

// ── Column model ────────────────────────────────────────────────────────────────

// `label` is a render-time thunk so headers re-read the active locale (spec 046 #8).
// spec 041 FR-051 (T076): the `state` column is dropped along with the session
// review-state machine — sessions no longer have a review state to display.
const COLUMNS: Array<{
  key: string;
  label: () => string;
  sort?: SessionSortCol;
  className?: string;
}> = [
  {
    key: 'target',
    label: () => m.projects_create_target_label(),
    sort: 'target',
  },
  { key: 'filter', label: () => m.common_filter(), sort: 'filter' },
  {
    key: 'frames',
    label: () => m.projects_wizard_col_frames(),
    sort: 'frames',
    className: 'pv-cell--num',
  },
  {
    key: 'integration',
    label: () => m.projects_wizard_col_integration(),
    sort: 'exposure',
    className: 'pv-cell--mono',
  },
  {
    key: 'night',
    label: () => m.sessions_col_night(),
    sort: 'night',
    className: 'pv-cell--mono',
  },
  {
    key: 'camera',
    label: () => m.settings_calmatch_camera(),
    sort: 'camera',
    className: 'pv-cell--muted',
  },
  { key: 'projects', label: () => m.common_projects() },
];

const EMPTY_CELLS = {
  filter: '' as string | ReactNode,
  frames: '' as string | ReactNode | number,
  integration: '' as string | ReactNode,
  night: '' as string | ReactNode,
  camera: '' as string | ReactNode,
  projects: '' as string | ReactNode,
};

// ── Component ────────────────────────────────────────────────────────────────

interface Props {
  sources: InventorySource[];
  selected: string | null;
  onSelect: (id: string) => void;
  loading?: boolean;
  sort: SessionSort;
  onSort: (col: SessionSortCol) => void;
  /**
   * Active ordered grouping dimension ids. Supplied by SessionsPage via
   * `useGrouping`. When empty the table renders a flat sorted list.
   */
  dims?: string[];
}

export function SessionsTable({
  sources,
  selected,
  onSelect,
  loading,
  sort,
  onSort,
  dims = [],
}: Props) {
  const { collapsed, toggle } = useCollapsibleGroups();

  // Backing-source connectivity per session (#889) — a session's row-level
  // health is its owning source's state, looked up by `sourceId`.
  const sourceStateById = useMemo(
    () => new Map(sources.map((src) => [src.id, src.state])),
    [sources],
  );

  // Flatten all sessions across sources, then sort.
  const allSessions = useMemo<InventorySession[]>(() => {
    const flat: InventorySession[] = [];
    for (const src of sources) flat.push(...src.sessions);
    return [...flat].sort((a, b) => compareSessions(a, b, sort));
  }, [sources, sort]);

  const grouped = dims.length > 0;

  // Build the group tree when grouping is active. When dims is empty the
  // table is a TRUE flat list (Inbox-parity): plain item rows, no synthetic
  // "All" group header (groupByDimensions would emit an `__all__` node).
  const tree = useMemo(
    () =>
      grouped ? groupByDimensions(allSessions, dims, SESSION_ACCESSORS) : [],
    [grouped, allSessions, dims],
  );

  const visualRows = useMemo(
    () =>
      grouped
        ? flattenVisibleGroups(tree, collapsed)
        : allSessions.map((s) => ({
            kind: 'item' as const,
            item: s,
            depth: 0,
          })),
    [grouped, tree, collapsed, allSessions],
  );

  // Build sortable column headers. aria-sort is emitted on the <th> by the
  // shared Table (ariaSortFor) — the SortHeader button only shows the arrow.
  const columns: TableColumn[] = COLUMNS.map((c) => ({
    key: c.key,
    className: c.className,
    ariaSort: c.sort ? ariaSortFor(sort.col === c.sort, sort.dir) : undefined,
    label: c.sort ? (
      <SortHeader
        label={c.label()}
        active={sort.col === c.sort}
        dir={sort.dir}
        onClick={() => onSort(c.sort as SessionSortCol)}
        ariaLabel={m.common_sort_by_aria({ col: c.label() })}
      />
    ) : (
      c.label()
    ),
  }));

  const rows: TableRow[] = useMemo(() => {
    const firstItemIndex = visualRows.findIndex((row) => row.kind === 'item');
    // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: table row render with per-column conditional formatting
    return visualRows.map((row, rowIndex) => {
      if (row.kind === 'header') {
        const { node, depth, path, collapsed: isCollapsed } = row;
        return {
          _rowClassName: 'pv-listgroup',
          _indent: tableIndent(depth),
          target: (
            <button
              type="button"
              className="pv-listgroup__cell"
              data-testid={`sessions-group-${node.dimension}-${node.key}`}
              aria-expanded={!isCollapsed}
              onClick={() => toggle(path)}
            >
              <span className="pv-listgroup__caret" aria-hidden="true">
                {isCollapsed ? '▸' : '▾'}
              </span>
              <span className="pv-listgroup__label">{node.label}</span>
              <span className="pv-listgroup__count">{node.count}</span>
            </button>
          ),
          ...EMPTY_CELLS,
        };
      }

      // Flat item or grouped leaf. The session's TARGET IDENTITY is the row
      // headline (spec 043 §4) — rendered in the Target cell whether the
      // table is flat (the default) or a grouped leaf (indented under its
      // header). Inbox-parity: rows carry a stable per-row testid.
      const s = row.item;
      const indentPx = grouped ? tableIndent(row.depth) : 0;
      const projects = s.linked?.projects ?? [];
      const sourceState = sourceStateById.get(s.sourceId);
      const connLabel = sourceState ? connectivityLabel(sourceState) : null;
      return {
        _testid: `sessions-row-${s.id}`,
        _rowKind: 'sessions-table-row',
        _guideAnchor:
          rowIndex === firstItemIndex ? 'sessions.review-row' : undefined,
        _rowClassName:
          'pv-sessions-table__row' +
          (selected === s.id ? ' pv-sessions-table__row--selected' : ''),
        _onClick: () => onSelect(s.id),
        _selected: selected === s.id,
        _indent: indentPx || undefined,
        target: (
          <span className="pv-sessions-cell--target">
            {sessionDisplayName(s)}
            {connLabel && sourceState && (
              <Pill
                variant={connectivityVariant(sourceState)}
                data-testid={`sessions-row-connectivity-${s.id}`}
              >
                {connLabel}
              </Pill>
            )}
          </span>
        ),
        filter: s.filter ?? '—',
        frames: s.frames,
        integration: integrationLabel(s),
        night: s.capturedOn ?? '—',
        camera: s.camera ?? '—',
        projects:
          projects.length > 0 ? (
            <span className="pv-sessions-cell__projects">
              {projects.map((p) => (
                <Pill key={p.id} variant="info">
                  {p.name}
                </Pill>
              ))}
            </span>
          ) : (
            <span className="pv-cell--muted">—</span>
          ),
      };
    });
  }, [visualRows, grouped, sourceStateById, selected, toggle, onSelect]);

  // Inbox-parity: grouping-state hint footer under the table when grouped.
  const groupingHint = grouped
    ? m.sessions_grouping_hint({
        dims: dims.map((d) => SESSION_DIM_LABELS[d]?.() ?? d).join(' › '),
      })
    : null;

  return (
    <div className="pv-listtable" data-testid="sessions-list">
      {visualRows.length === 0 && loading ? (
        <div className="pv-listtable__empty">
          <Skeleton variant="block" count={8} label={m.common_loading()} />
        </div>
      ) : visualRows.length === 0 && !loading ? (
        <div className="pv-listtable__empty">{m.sessions_no_match()}</div>
      ) : (
        <Table
          className="pv-sessions-table"
          columns={columns}
          rows={rows}
          virtualized
          estimateRowHeight={36}
          scrollClassName="pv-listtable__scroll"
          scrollTestId="sessions-virtual-sizer"
        />
      )}
      {groupingHint && (
        <div
          className="pv-listtable__foot"
          data-testid="sessions-grouping-hint"
        >
          {groupingHint}
        </div>
      )}
    </div>
  );
}
