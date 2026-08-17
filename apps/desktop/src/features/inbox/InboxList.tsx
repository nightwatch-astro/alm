// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * InboxList — the inbox detection list, rendered with the shared `Table`
 * construct (same as Sessions / Calibration) so font, row height, headers, and
 * group sub-headers match the rest of the app. Virtualized via `Table`'s
 * opt-in padding-spacer windowing (the inbox is capped at 500 rows but can
 * still be large), so it stays windowed without a bespoke list.
 *
 * Columns: Path (relative path, primary) · Type (frame type / state) · Files ·
 * Format. When grouping is active, the chosen ordered dimensions render
 * collapsible group sub-header rows (shared `groupByDimensions` engine); leaf
 * rows indent under their group. Selection + grouping controls are owned by the
 * page / top-bar (FilterToolbar + useGrouping); this is a controlled
 * presentational list.
 *
 * Sort: column headers are <button> elements that call onSort (SessionsTable
 * convention). The page owns sort state and passes sortCol + sortDir.
 * Kind filter: the page passes a `kindFilter` string that filters by the item's
 * dominant frame type (groupFrameType / masterFrameType).
 */

import { useState, useMemo, useCallback } from 'react';
import type { InboxListItem, InboxSourceGroupListItem } from '@/bindings/index';
import {
  Btn,
  Table,
  tableIndent,
  Skeleton,
  Pill,
  type TableColumn,
  type TableRow,
} from '@/ui';
import { SortHeader, ariaSortFor } from '@/components';
import { groupByDimensions } from './grouping';
import { ACCESSORS, dimLabel } from './InboxControls';
import { m } from '@/lib/i18n';
import { useHotkeys } from '@/lib/useHotkeys';
import {
  DEFAULT_INBOX_SORT,
  classificationLabel,
  classificationMod,
  compareItems,
  compareSourceGroups,
  detectionLabel,
  flattenVisibleTree,
  formatDisplayLabel,
  itemKind,
  sourceGroupDetectionLabel,
  sourceGroupFormatTag,
  sourceGroupLane,
} from './inbox-list-model';
import type {
  InboxSort,
  InboxSortCol,
  SortDir,
  VisualRow,
  HeaderVisualRow,
  ItemVisualRow,
  SourceGroupVisualRow,
} from './inbox-list-model';

export { DEFAULT_INBOX_SORT, flattenVisibleTree };
export type {
  InboxSortCol,
  SortDir,
  InboxSort,
  HeaderVisualRow,
  ItemVisualRow,
  SourceGroupVisualRow,
  VisualRow,
};

// ── Component types ─────────────────────────────────────────────────────────────

export interface InboxListProps {
  items: InboxListItem[];
  /**
   * Spec 058 FR-016 / T013 — folders that have been scanned but have produced
   * no item rows yet.
   *
   * These render as **structurally non-confirmable** rows: a source group has
   * no `inboxItemId`, so there is no id to hand to `inbox.confirm` and no
   * selection to drive the detail pane's Confirm button. That is the point —
   * FR-016 asks for a row that *cannot* be confirmed rather than a row that is
   * offered and then refused by a guard, because a guard is a runtime promise
   * while an absent id is a structural one.
   *
   * They render as a leading block above the item rows rather than
   * interleaving: the grouping engine (`groupByDimensions`/`ACCESSORS`) keys on
   * `InboxListItem` fields that a source group does not have, and an
   * unclassified folder has no dimension value to group under in any case.
   *
   * Inert until T020: while scan still writes a folder placeholder item, every
   * scanned folder has an item row, so `inbox.list` always returns this empty.
   */
  sourceGroups?: InboxSourceGroupListItem[];
  /**
   * Spec 058 FR-017 — trigger group-scoped classification for a scanned folder.
   *
   * Deliberately separate from {@link InboxListProps.onSelect}: a source group
   * has no `inboxItemId`, and `onSelect` feeds the `?selected=` URL param,
   * which resolves against item ids only. Routing group classification through
   * `onSelect` would both break selection and delete the FR-016 invariant that
   * a source-group row never selects anything.
   *
   * When omitted the row renders its static "not yet classified" label, so
   * existing callers and fixtures keep their current behaviour.
   */
  onClassifySourceGroup?: (group: InboxSourceGroupListItem) => void;
  /** Source group whose classification is in flight — disables its action. */
  classifyingSourceGroupId?: string | null;
  /** Issue #644: selection is by item identity, not list position — an index
   * silently points at whatever item now occupies that slot after search/lane/
   * kind filters change the array shape. */
  selectedId: string | null;
  onSelect: (id: string) => void;
  /** Lane filter ('all' | 'fits' | 'video'). Owned by the page (URL state). */
  filterType: string;
  /** Active ordered grouping dimensions (owned by the page / top-bar controls). */
  dims?: string[];
  /**
   * Frame-type kind filter ('all' | 'bias' | 'dark' | 'flat' | 'light' | 'master').
   * Owned by the page.
   */
  kindFilter?: string;
  /** When true, the detection list is still loading — show skeleton rows. */
  loading?: boolean;
  /** Active sort state. Owned by the page. */
  sort?: InboxSort;
  /** Called when the user clicks a sortable column header. */
  onSort?: (col: InboxSortCol) => void;
  /** @deprecated Sort state is now owned by column headers via sort/onSort. */
  sortBy?: string;
  /** @deprecated The frame-type filter control moved to the top-bar FilterToolbar. */
  onFilterTypeChange?: (type: string | undefined) => void;
}

// ── Component ───────────────────────────────────────────────────────────────────

export function InboxList({
  items,
  sourceGroups = [],
  onClassifySourceGroup,
  classifyingSourceGroupId = null,
  selectedId,
  onSelect,
  filterType,
  dims = [],
  kindFilter,
  loading = false,
  sort = DEFAULT_INBOX_SORT,
  onSort,
}: InboxListProps) {
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());

  const filtered = useMemo(() => {
    let result = items;
    // Lane filter (fits / video).
    if (filterType !== 'all') {
      result = result.filter((item) => item.lane === filterType);
    }
    // Kind filter (bias / dark / flat / light / master).
    if (kindFilter && kindFilter !== 'all') {
      result = result.filter((item) => itemKind(item) === kindFilter);
    }
    // Sort via column headers (replaces the old name/state sort dropdown).
    const sorted = [...result].sort((a, b) => compareItems(a, b, sort));
    return sorted;
  }, [items, filterType, kindFilter, sort]);

  /**
   * Source groups surviving the active filters.
   *
   * The lane filter maps through `sourceGroupLane` (format-derived, #854). The
   * kind filter hides every source group whenever it is narrowed to a specific
   * frame type: a source group is unclassified by definition, so it matches no
   * frame-type kind — showing it under "bias" would be exactly the kind of row
   * lying about itself that this feature removes.
   */
  const filteredSourceGroups = useMemo(() => {
    let result = sourceGroups;
    if (filterType !== 'all') {
      result = result.filter((g) => sourceGroupLane(g) === filterType);
    }
    if (kindFilter && kindFilter !== 'all') {
      return [];
    }
    return [...result].sort((a, b) => compareSourceGroups(a, b, sort));
  }, [sourceGroups, filterType, kindFilter, sort]);

  const tree = useMemo(
    () => groupByDimensions(filtered, dims, ACCESSORS),
    [filtered, dims],
  );

  const originalIndexById = useMemo(() => {
    const m = new Map<string, number>();
    items.forEach((it, i) => {
      m.set(it.inboxItemId, i);
    });
    return m;
  }, [items]);

  const toggle = useCallback((path: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const grouped = dims.length > 0;

  const visualRows = useMemo<VisualRow[]>(() => {
    // Source-group rows lead, ungrouped and unindented, in both the grouped and
    // flat views — see `InboxListProps.sourceGroups` for why they do not enter
    // the grouping engine.
    const groupRows: VisualRow[] = filteredSourceGroups.map((group) => ({
      kind: 'sourceGroup' as const,
      group,
    }));
    if (grouped)
      return [
        ...groupRows,
        ...flattenVisibleTree(tree, collapsed, originalIndexById),
      ];
    return [
      ...groupRows,
      ...filtered.map((item) => ({
        kind: 'item' as const,
        item,
        originalIdx: originalIndexById.get(item.inboxItemId) ?? -1,
        indent: 0,
      })),
    ];
  }, [
    grouped,
    tree,
    collapsed,
    originalIndexById,
    filtered,
    filteredSourceGroups,
  ]);

  // ── J/K triage navigation (spec 027 FR-022, issue #747) ─────────────────────
  // Bound here rather than on the page because this component owns the visual
  // order: grouping, collapse state, sort and filters all reshape it, and J/K
  // must step through what the user actually sees.
  const navigableIds = useMemo(
    () =>
      visualRows
        .filter((r): r is ItemVisualRow => r.kind === 'item')
        .map((r) => r.item.inboxItemId),
    [visualRows],
  );

  const step = useCallback(
    (delta: number) => {
      if (navigableIds.length === 0) return;
      const cur = selectedId ? navigableIds.indexOf(selectedId) : -1;
      // Clamped, not wrapped: triage is a top-to-bottom sweep, and silently
      // jumping back to the top reads as "nothing happened" at the last row.
      // With nothing selected, J enters at the top and K at the bottom.
      const next =
        cur === -1
          ? delta > 0
            ? 0
            : navigableIds.length - 1
          : Math.min(navigableIds.length - 1, Math.max(0, cur + delta));
      const id = navigableIds[next];
      if (id && id !== selectedId) onSelect(id);
    },
    [navigableIds, selectedId, onSelect],
  );

  // Single-key bindings, matching the scheme the retired Inbox ActionSidebar
  // used. useHotkeys suppresses these while a text field has focus.
  useHotkeys(
    {
      KeyJ: (e) => {
        e.preventDefault();
        step(1);
      },
      KeyK: (e) => {
        e.preventDefault();
        step(-1);
      },
    },
    [step],
  );

  // Selection moves without moving DOM focus (the target row may not even be
  // rendered under virtualization), so nothing would reach a screen reader
  // without an explicit announcement.
  const selectedItem = useMemo(
    () =>
      selectedId
        ? visualRows.find(
            (r): r is ItemVisualRow =>
              r.kind === 'item' && r.item.inboxItemId === selectedId,
          )?.item
        : undefined,
    [visualRows, selectedId],
  );

  // ── Sortable column headers (SessionsTable convention) ──────────────────────
  const makeSortHeader = (
    col: InboxSortCol,
    label: string,
    ariaLabel: string,
  ) => (
    <SortHeader
      label={label}
      active={sort.col === col}
      dir={sort.dir}
      onClick={() => onSort?.(col)}
      ariaLabel={ariaLabel}
    />
  );
  // aria-sort lives on the <th> (shared Table), not the SortHeader button.
  const thSort = (col: InboxSortCol) => ariaSortFor(sort.col === col, sort.dir);

  // Build columns with sortable headers.
  const COLUMNS: TableColumn[] = [
    {
      key: 'detection',
      label: makeSortHeader(
        'detection',
        m.inbox_col_detection(),
        m.inbox_sort_detection_aria(),
      ),
      ariaSort: thSort('detection'),
      style: { width: '20rem' },
    },
    {
      key: 'type',
      label: makeSortHeader(
        'type',
        m.inbox_col_type(),
        m.inbox_sort_type_aria(),
      ),
      ariaSort: thSort('type'),
      style: { width: '7.5rem' },
    },
    {
      key: 'count',
      label: makeSortHeader(
        'count',
        m.inbox_col_files(),
        m.inbox_sort_files_aria(),
      ),
      ariaSort: thSort('count'),
      className: 'num',
      style: { width: '5rem' },
    },
    {
      key: 'format',
      label: makeSortHeader(
        'format',
        m.inbox_dim_format(),
        m.inbox_sort_format_aria(),
      ),
      ariaSort: thSort('format'),
      className: 'pv-inbox-cell--right',
      style: { width: '7rem' },
    },
  ];

  // Map the visual rows onto shared-Table rows (group sub-headers + item rows).
  const rows = useMemo<TableRow[]>(
    () =>
      // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: virtual-row render dispatching over header, group, and item row kinds
      visualRows.map((row) => {
        if (row.kind === 'header') {
          const { node, depth, path, collapsed: isCollapsed } = row;
          return {
            _rowClassName: 'pv-inbox-table__group',
            _indent: tableIndent(depth),
            // The collapse control is a real <button> (keyboard-accessible,
            // announces expanded state) inside the group cell — not a clickable
            // <tr>. It carries the group testid + aria-expanded.
            detection: (
              <button
                type="button"
                className="pv-inbox-table__group-cell"
                data-testid={`inbox-group-${node.dimension}-${node.key}`}
                aria-expanded={!isCollapsed}
                onClick={() => toggle(path)}
              >
                <span className="pv-inbox-list__group-caret" aria-hidden="true">
                  {isCollapsed ? '▸' : '▾'}
                </span>
                <span className="pv-inbox-list__group-label">{node.label}</span>
                <span className="pv-inbox-list__group-count">{node.count}</span>
              </button>
            ),
            type: '',
            count: '',
            format: '',
          };
        }
        if (row.kind === 'sourceGroup') {
          const { group } = row;
          const label = sourceGroupDetectionLabel(group);
          const classifying = classifyingSourceGroupId === group.sourceGroupId;
          return {
            _testid: `inbox-source-group-${group.sourceGroupId}`,
            _rowKind: 'inbox-table-row',
            _rowClassName: 'pv-inbox-table__row pv-inbox-table__row--muted',
            detection: (
              <span className="pv-inbox-cell__path-wrap">
                <span
                  className="pv-inbox-cell__path"
                  title={m.inbox_source_group_row_aria({ path: label })}
                >
                  {label}
                </span>
              </span>
            ),
            type: (
              <span
                className="pv-inbox-row__classification pv-inbox-row__classification--pending"
                data-testid="inbox-row-classification"
              >
                {classifying
                  ? m.inbox_source_group_classifying()
                  : m.inbox_state_not_yet_classified()}
                {onClassifySourceGroup ? (
                  <Btn
                    type="button"
                    variant="ghost"
                    size="sm"
                    data-testid={`inbox-source-group-classify-${group.sourceGroupId}`}
                    disabled={classifying}
                    aria-label={m.inbox_source_group_classify_aria({
                      path: label,
                    })}
                    onClick={() => onClassifySourceGroup(group)}
                  >
                    {m.inbox_source_group_classify()}
                  </Btn>
                ) : null}
              </span>
            ),
            count: m.inbox_list_file_count({ count: group.fileCount }),
            format: sourceGroupFormatTag(group),
          };
        }
        const { item, indent } = row;
        const selected = selectedId === item.inboxItemId;
        const mod = classificationMod(item);
        return {
          _testid: `inbox-item-${item.inboxItemId}`,
          _rowKind: 'inbox-table-row',
          _rowClassName: [
            'pv-inbox-table__row',
            selected ? 'pv-inbox-table__row--selected' : '',
            item.state === 'plan_open' ? 'pv-inbox-table__row--muted' : '',
          ]
            .filter(Boolean)
            .join(' '),
          _onClick: () => onSelect(item.inboxItemId),
          _selected: selected,
          _indent: indent || undefined,
          detection: (
            <span className="pv-inbox-cell__path-wrap">
              <span
                className="pv-inbox-cell__path"
                title={detectionLabel(item)}
              >
                {detectionLabel(item)}
              </span>
              {/* Issue #605: an in-context hint that Confirm already created a
                  reviewable plan — the Type column keeps showing the item's
                  dominant frame type (classificationLabel checks frameType
                  before state), so without this the row looks unchanged and
                  reads as "my confirm did nothing". Additive: no action
                  needed here, "Review plans" in the top bar remains the one
                  place to act on it. */}
              {item.state === 'plan_open' && (
                <Pill
                  variant="info"
                  data-testid={`inbox-item-plan-pending-${item.inboxItemId}`}
                >
                  {m.inbox_row_plan_pending()}
                </Pill>
              )}
            </span>
          ),
          type: (
            <span
              className={`pv-inbox-row__classification pv-inbox-row__classification--${mod}`}
              data-testid="inbox-row-classification"
            >
              {classificationLabel(item)}
            </span>
          ),
          count: m.inbox_list_file_count({ count: item.fileCount }),
          format: formatDisplayLabel(item),
        };
      }),
    [visualRows, selectedId, onSelect, toggle],
  );

  const groupingHint = grouped
    ? m.inbox_grouping_hint({ dims: dims.map((d) => dimLabel(d)).join(' › ') })
    : null;

  return (
    <div className="pv-listtable" data-testid="inbox-list">
      {visualRows.length === 0 && loading ? (
        <div className="pv-listtable__empty">
          <Skeleton variant="block" count={8} label={m.common_loading()} />
        </div>
      ) : visualRows.length === 0 ? (
        <div className="pv-listtable__empty">{m.inbox_no_detections()}</div>
      ) : (
        <Table
          className="pv-inbox-table"
          columns={COLUMNS}
          rows={rows}
          virtualized
          estimateRowHeight={40}
          scrollClassName="pv-listtable__scroll"
          scrollTestId="inbox-virtual-sizer"
        />
      )}
      {groupingHint && (
        <div className="pv-listtable__foot" data-testid="inbox-grouping-hint">
          {groupingHint}
        </div>
      )}
      {/* Discoverability for the otherwise invisible J/K/C bindings (#747). */}
      <div className="pv-listtable__foot" data-testid="inbox-hotkey-hint">
        {m.inbox_hotkey_hint()}
      </div>
      {/* Keyboard selection moves no DOM focus, so announce it explicitly. */}
      <div
        className="pv-visually-hidden"
        role="status"
        aria-live="polite"
        data-testid="inbox-selection-announcer"
      >
        {selectedItem
          ? m.inbox_row_selected_aria({ label: detectionLabel(selectedItem) })
          : ''}
      </div>
    </div>
  );
}
