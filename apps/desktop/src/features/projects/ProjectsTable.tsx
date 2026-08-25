// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ProjectsTable — spec 043 shared-layout adoption (tasks #73 / #43).
 *
 * Replaces the old narrow `ProjectsList` sidebar with a DENSE, FULL-WIDTH
 * sortable table — the same surface pattern as the Sessions page (shared
 * `Table` from `@/ui`). One row per project; selecting a row opens the existing
 * ProjectDetail in the right-side detail pane on ProjectsPage.
 *
 * Columns: Name · Tool · Target · State (Pill) · Sources · Updated. These reuse
 * the rich-row fields introduced for the list rows (#43). `Target` is a STUB:
 * ProjectSummaryDto carries no target linkage yet (needs FITS OBJECT →
 * target_id, spec task #54), so it renders an em-dash placeholder — never a
 * fabricated value.
 *
 * Search + the State filter + the sort control live in the persistent page top
 * bar (shared PageTopBar + FilterToolbar), NOT inside this surface. Sorting is
 * driven by the `sort` prop; clicking a sortable header calls `onSort`.
 */

import { useMemo } from 'react';
import { AlertTriangle } from 'lucide-react';
import { m } from '@/lib/i18n';
import { Table, tableIndent, Skeleton } from '@/ui';
import { SortHeader, ariaSortFor, StatusTag } from '@/components';
import type { TableColumn, TableRow } from '@/ui';
import { projectStateLabel, projectStateVariant } from '@/lib/lifecycle';
import { blockedReasonMessage, deriveBlockedReason } from './BlockedBanner';
import * as pt from './projects-table.css';
import { compareDateDesc, formatDateTime } from '@/lib/datetime';
import type { ProjectSummaryDto } from '@/bindings/index';
import {
  groupByDimensions,
  flattenVisibleGroups,
  type DimensionAccessor,
} from '@/lib/grouping';
import { useCollapsibleGroups } from '@/lib/use-grouping';

// ── Sort model ────────────────────────────────────────────────────────────────

export type ProjectSortCol = 'name' | 'tool' | 'state' | 'sources' | 'updated';
export type SortDir = 'asc' | 'desc';

export interface ProjectSort {
  col: ProjectSortCol;
  dir: SortDir;
}

export const DEFAULT_PROJECT_SORT: ProjectSort = {
  col: 'updated',
  dir: 'desc',
};

function compareStr(
  a: string | null | undefined,
  b: string | null | undefined,
): number {
  return (a ?? '').localeCompare(b ?? '');
}

function compareProjects(
  a: ProjectSummaryDto,
  b: ProjectSummaryDto,
  sort: ProjectSort,
): number {
  let cmp = 0;
  switch (sort.col) {
    case 'name':
      cmp = compareStr(a.name, b.name);
      break;
    case 'tool':
      cmp = compareStr(a.tool, b.tool);
      break;
    case 'state':
      cmp = compareStr(a.lifecycle, b.lifecycle);
      break;
    case 'sources':
      cmp = a.sourceCount - b.sourceCount;
      break;
    case 'updated':
      // compareDateDesc returns most-recent-first; invert so the asc/desc flip
      // below keeps "desc" meaning newest-first.
      cmp = -compareDateDesc(a.updatedAt, b.updatedAt);
      break;
  }
  return sort.dir === 'asc' ? cmp : -cmp;
}

// ── Multi-level grouping accessors ────────────────────────────────────────────

export const PROJECT_ACCESSORS: Readonly<
  Record<string, DimensionAccessor<ProjectSummaryDto>>
> = {
  state: (p) => p.lifecycle,
  tool: (p) => p.tool,
  target: (_p) => null, // STUB: target linkage blocked on task #54
};

// ── Column model ────────────────────────────────────────────────────────────────

// `label` is a render-time thunk so headers re-read the active locale (spec 046 #8).
const COLUMNS: Array<{
  key: string;
  label: () => string;
  sort?: ProjectSortCol;
  className?: string;
}> = [
  { key: 'name', label: () => m.projects_col_name(), sort: 'name' },
  {
    key: 'tool',
    label: () => m.projects_col_tool(),
    sort: 'tool',
    className: pt.cellMuted,
  },
  {
    key: 'target',
    label: () => m.projects_create_target_label(),
    className: pt.cellMuted,
  },
  { key: 'state', label: () => m.sessions_col_state(), sort: 'state' },
  {
    key: 'sources',
    label: () => m.common_sources(),
    sort: 'sources',
    className: pt.cellNum,
  },
  {
    key: 'updated',
    label: () => m.projects_stepper_updated(),
    sort: 'updated',
    className: pt.cellMono,
  },
];

// ── Props ───────────────────────────────────────────────────────────────────

export interface ProjectsTableProps {
  projects: ProjectSummaryDto[];
  selectedId: string | undefined;
  onSelect: (id: string) => void;
  loading?: boolean;
  sort: ProjectSort;
  onSort: (col: ProjectSortCol) => void;
  /**
   * Active ordered grouping dimension ids from `useGrouping`.
   * When empty the table renders a flat sorted list.
   */
  dims?: string[];
  /**
   * True when a search/lifecycle filter is active, so an empty `projects`
   * list means "no matches" rather than "no projects exist yet" — the two
   * cases need different empty-state copy (empty-state canon).
   */
  isFiltered?: boolean;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function ProjectsTable({
  projects,
  selectedId,
  onSelect,
  loading = false,
  sort,
  onSort,
  dims = [],
  isFiltered = false,
}: ProjectsTableProps) {
  const { collapsed, toggle } = useCollapsibleGroups();

  const sorted = useMemo(
    () => [...projects].sort((a, b) => compareProjects(a, b, sort)),
    [projects, sort],
  );

  const useMultiGroup = dims.length > 0;

  const tree = useMemo(
    () =>
      useMultiGroup ? groupByDimensions(sorted, dims, PROJECT_ACCESSORS) : [],
    [sorted, dims, useMultiGroup],
  );

  const visualRows = useMemo(
    () => (useMultiGroup ? flattenVisibleGroups(tree, collapsed) : []),
    [tree, collapsed, useMultiGroup],
  );

  // Sortable header buttons (button-in-th) — mirrors the Sessions table.
  const columns: TableColumn[] = COLUMNS.map((c) => ({
    key: c.key,
    className: c.className,
    ariaSort: c.sort ? ariaSortFor(sort.col === c.sort, sort.dir) : undefined,
    label: c.sort ? (
      <SortHeader
        label={c.label()}
        active={sort.col === c.sort}
        dir={sort.dir}
        onClick={() => onSort(c.sort as ProjectSortCol)}
        ariaLabel={m.common_sort_by_aria({ col: c.label() })}
      />
    ) : (
      c.label()
    ),
  }));

  const EMPTY_PROJECT_CELLS = {
    tool: '' as string,
    target: '' as string,
    state: '' as string,
    sources: '' as string,
    updated: '' as string,
  };

  function projectItemRow(project: ProjectSummaryDto, indentPx = 0): TableRow {
    return {
      _rowClassName:
        pt.row + (project.id === selectedId ? ` ${pt.rowSelected}` : ''),
      _onClick: () => onSelect(project.id),
      _selected: project.id === selectedId,
      _indent: indentPx || undefined,
      _rowKind: 'projects-table-row',
      _testid: `project-row-${project.id}`,
      name: (
        <span className={pt.name}>
          {project.lifecycle === 'blocked' &&
            (() => {
              const reason = deriveBlockedReason(
                project.blockedReasonKind,
                project.blockedReasonNote,
              );
              const reasonText = blockedReasonMessage(reason);
              return (
                <span title={reasonText}>
                  <AlertTriangle
                    size={13}
                    role="img"
                    aria-label={m.projects_table_blocked_aria({
                      reason: reasonText,
                    })}
                    className={pt.blockedIcon}
                  />
                </span>
              );
            })()}
          {project.name}
          {project.channelDrift && (
            <span
              className={pt.driftBadge}
              title={m.projects_table_channel_drift_title()}
            >
              <AlertTriangle size={11} aria-hidden="true" />{' '}
              {m.projects_table_channel_drift_label()}
            </span>
          )}
        </span>
      ),
      tool: <span className={pt.cellMuted}>{project.tool}</span>,
      // STUB: target — omitted until FITS OBJECT → target_id linkage lands (#54).
      target: <span className={pt.dash}>—</span>,
      state: (
        <StatusTag variant={projectStateVariant(project.lifecycle)}>
          {projectStateLabel(project.lifecycle)}
        </StatusTag>
      ),
      sources: (
        <span className={pt.cellNum}>
          {project.sourceCount > 0 ? project.sourceCount : '—'}
        </span>
      ),
      updated: (
        <span className={pt.cellMono}>{formatDateTime(project.updatedAt)}</span>
      ),
    };
  }

  if (loading && projects.length === 0) {
    return (
      <div className={pt.empty}>
        <Skeleton
          variant="block"
          count={6}
          label={m.projects_table_loading()}
        />
      </div>
    );
  }

  if (projects.length === 0) {
    return (
      <div className={pt.empty}>
        {isFiltered
          ? m.projects_table_empty_filtered()
          : m.projects_table_empty()}
      </div>
    );
  }

  // Build rows: multi-level grouping path or flat sorted path.
  const rows: TableRow[] = [];

  if (useMultiGroup) {
    for (const vrow of visualRows) {
      if (vrow.kind === 'header') {
        const { node, depth, path, collapsed: isCollapsed } = vrow;
        rows.push({
          _rowClassName: 'pv-listgroup',
          _indent: tableIndent(depth),
          name: (
            <button
              type="button"
              className="pv-listgroup__cell"
              data-testid={`projects-group-${node.dimension}-${node.key}`}
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
          ...EMPTY_PROJECT_CELLS,
        });
      } else {
        rows.push(projectItemRow(vrow.item, tableIndent(vrow.depth)));
      }
    }
  } else {
    for (const project of sorted) {
      rows.push(projectItemRow(project));
    }
  }

  // The project count moved to the bottom status bar (top-bar convention,
  // task #80) — no in-table footer count line.
  return (
    <div className="pv-listtable" data-testid="projects-list">
      <Table
        className={pt.table}
        columns={columns}
        rows={rows}
        virtualized
        estimateRowHeight={36}
        scrollClassName="pv-listtable__scroll"
        scrollTestId="projects-virtual-sizer"
      />
    </div>
  );
}
