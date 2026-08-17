// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * FilterToolbar — generic, prop-driven filter row (spec 043, task #63).
 *
 * Renders, in a single consistent row: an optional search box, any number of
 * labeled select fields, an optional "Group by" select, an optional sort
 * control (select + direction toggle), and trailing action node. Every slot is
 * optional — pass only what the page needs.
 *
 * This is what goes in `PageTopBar`'s `filters` slot. It owns NO state: all
 * values + change handlers are supplied by the host page (URL state, store,
 * local state — the toolbar does not care). Token-only styling via the shared
 * `.pv-filterbar*` classes; no inline styles.
 */

import type { ReactNode } from 'react';
import { Btn } from '@/ui';
import { m } from '@/lib/i18n';
import * as ft from './FilterToolbar.css';

export interface FilterOption {
  value: string;
  label: string;
}

export interface FilterField {
  /** Stable key (used for React key + ids). */
  key: string;
  /** Visible label rendered before the select. */
  label: string;
  /** Current selected value (`''` = the leading "all/any" option). */
  value: string;
  /** Selectable options (excluding the implicit leading option). */
  options: FilterOption[];
  onChange: (value: string) => void;
  /** Text of the leading option that clears the field. Default "All". */
  allLabel?: string;
}

/**
 * A multi-select filter field (task #82): a labeled control whose value is a
 * SET of selected option values, rendered as a compact popover of checkboxes.
 * Distinct from `FilterField` (single-select). Selecting zero options means
 * "none" — the host decides what an empty selection shows.
 */
export interface MultiFilterField {
  /** Stable key (used for React key + ids). */
  key: string;
  /** Visible label rendered before the control. */
  label: string;
  /** Currently selected option values. */
  value: string[];
  /** Selectable options. */
  options: FilterOption[];
  onChange: (value: string[]) => void;
}

export interface GroupByControl {
  value: string;
  options: FilterOption[];
  onChange: (value: string) => void;
  /** Visible label. Default "Group by". */
  label?: string;
}

/**
 * Ordered MULTI-LEVEL grouping control ("Group by X, then by Y, then by Z").
 * The shared gold-standard grouping every list page gets — pair with the
 * `useGrouping` hook (which owns `dims` + persistence) and feed `dims` to the
 * page table's `groupByDimensions`. `dimensions` is the page-specific set of
 * selectable grouping dimensions.
 */
export interface GroupingControl {
  /** Selectable grouping dimensions for this page. */
  dimensions: FilterOption[];
  /** Active ordered dimension ids. */
  dims: string[];
  /** Set the dimension at a slot ("" clears it and deeper slots). */
  setSlot: (slot: number, value: string) => void;
  /** Number of ordered slots. Default 3. */
  maxLevels?: number;
}

export interface SortControl {
  value: string;
  options: FilterOption[];
  onChange: (value: string) => void;
  dir: 'asc' | 'desc';
  onDirToggle: () => void;
  /** Visible label. Default "Sort". */
  label?: string;
}

export interface SearchControl {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  /** Accessible label. Default "Search". */
  ariaLabel?: string;
}

export interface FilterToolbarProps {
  search?: SearchControl;
  fields?: FilterField[];
  /** Multi-select fields (set-valued), rendered after the single-select fields. */
  multiFields?: MultiFilterField[];
  /** Single-tier group-by (legacy). Prefer `grouping` for multi-level. */
  groupBy?: GroupByControl;
  /** Ordered multi-level grouping (the shared gold standard). */
  grouping?: GroupingControl;
  sort?: SortControl;
  /** Trailing node rendered at the row's end (e.g. a secondary control). */
  actions?: ReactNode;
}

/**
 * The ordered multi-level grouping configurator: N `<select>` slots, each
 * offering this page's dimensions. A slot is enabled only once the previous
 * slot has a dimension; dimensions chosen earlier are excluded from later
 * slots. Selecting "—" clears the slot and all deeper ones.
 */
function GroupingSelects({ grouping }: { grouping: GroupingControl }) {
  const { dimensions, dims, setSlot, maxLevels = 3 } = grouping;
  return (
    <div className={ft.group}>
      {Array.from({ length: maxLevels }).map((_, slot) => {
        const value = dims[slot] ?? '';
        const disabled = slot > 0 && !dims[slot - 1];
        const usedEarlier = new Set(dims.slice(0, slot));
        return (
          <select
            key={slot}
            className={ft.select}
            value={value}
            disabled={disabled}
            onChange={(e) => setSlot(slot, e.target.value)}
            aria-label={
              slot === 0
                ? m.inbox_group_by_aria()
                : m.inbox_group_by_level_aria({ level: slot + 1 })
            }
          >
            <option value="">
              {slot === 0
                ? m.inbox_controls_group_none()
                : m.inbox_controls_then_none()}
            </option>
            {dimensions
              .filter((d) => d.value === value || !usedEarlier.has(d.value))
              .map((d) => (
                <option key={d.value} value={d.value}>
                  {slot === 0
                    ? m.inbox_groupby_chip_primary({ label: d.label })
                    : m.inbox_groupby_chip_secondary({ label: d.label })}
                </option>
              ))}
          </select>
        );
      })}
    </div>
  );
}

function LabeledSelect({
  id,
  label,
  value,
  options,
  onChange,
  leadingOption,
}: {
  id: string;
  label: string;
  value: string;
  options: FilterOption[];
  onChange: (value: string) => void;
  leadingOption?: string;
}) {
  return (
    <label className={ft.field} htmlFor={id}>
      <span className={ft.fieldLabel}>{label}</span>
      <select
        className={ft.select}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-label={label}
        id={id}
      >
        {leadingOption != null && <option value="">{leadingOption}</option>}
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

/**
 * Multi-select filter control: a `<details>` popover whose summary shows the
 * selection count and whose body is a list of option checkboxes. Native
 * `<details>` keeps it dependency-free, keyboard-accessible, and click-to-toggle
 * without any open-state wiring in the host. Token-only `.pv-filterbar__multi*`
 * styling; no inline styles.
 */
function MultiSelect({
  id,
  label,
  value,
  options,
  onChange,
}: {
  id: string;
  label: string;
  value: string[];
  options: FilterOption[];
  onChange: (value: string[]) => void;
}) {
  const selected = new Set(value);
  const toggle = (v: string): void => {
    const next = new Set(selected);
    if (next.has(v)) next.delete(v);
    else next.add(v);
    // Preserve option order in the emitted array for stable downstream behavior.
    onChange(options.map((o) => o.value).filter((ov) => next.has(ov)));
  };
  const summary =
    selected.size === 0
      ? m.common_none()
      : selected.size === options.length
        ? m.common_all()
        : m.filter_multiselect_count({ count: selected.size });

  return (
    // A `<details>` is not a labelable control, so this field is a labelled
    // group (heading + aria-label) rather than a `<label>` wrapper.
    <div className={ft.field}>
      <span className={ft.fieldLabel}>{label}</span>
      <details className={ft.multi} id={id}>
        <summary
          className={ft.multiSummary}
          aria-label={`${label}: ${summary}`}
        >
          {summary}
        </summary>
        <div className={ft.multiMenu} role="group" aria-label={label}>
          {options.map((o) => (
            <label
              key={o.value}
              className={ft.multiOption}
              htmlFor={`${id}-${o.value}`}
            >
              <input
                type="checkbox"
                id={`${id}-${o.value}`}
                className={ft.multiCheck}
                checked={selected.has(o.value)}
                onChange={() => toggle(o.value)}
                aria-label={o.label}
              />
              <span>{o.label}</span>
            </label>
          ))}
        </div>
      </details>
    </div>
  );
}

export function FilterToolbar({
  search,
  fields,
  multiFields,
  groupBy,
  grouping,
  sort,
  actions,
}: FilterToolbarProps) {
  return (
    <div className={ft.root}>
      {search && (
        <input
          type="search"
          className={ft.search}
          placeholder={search.placeholder ?? m.common_search_placeholder()}
          value={search.value}
          onChange={(e) => search.onChange(e.target.value)}
          aria-label={search.ariaLabel ?? m.common_search_aria()}
        />
      )}

      {fields?.map((f) => (
        <LabeledSelect
          key={f.key}
          id={`filterbar-${f.key}`}
          label={f.label}
          value={f.value}
          options={f.options}
          onChange={f.onChange}
          leadingOption={f.allLabel ?? m.common_all()}
        />
      ))}

      {multiFields?.map((f) => (
        <MultiSelect
          key={f.key}
          id={`filterbar-${f.key}`}
          label={f.label}
          value={f.value}
          options={f.options}
          onChange={f.onChange}
        />
      ))}

      {grouping && <GroupingSelects grouping={grouping} />}

      {groupBy && (
        <LabeledSelect
          id="filterbar-groupby"
          label={groupBy.label ?? m.filter_group_by_label()}
          value={groupBy.value}
          options={groupBy.options}
          onChange={groupBy.onChange}
        />
      )}

      {sort && (
        <div className={ft.sort}>
          <LabeledSelect
            id="filterbar-sort"
            label={sort.label ?? m.filter_sort_label()}
            value={sort.value}
            options={sort.options}
            onChange={sort.onChange}
          />
          <Btn
            size="sm"
            variant="ghost"
            className={ft.sortDir}
            onClick={sort.onDirToggle}
            aria-label={m.filter_sort_dir_aria({
              dir:
                sort.dir === 'asc'
                  ? m.filter_sort_dir_ascending()
                  : m.filter_sort_dir_descending(),
            })}
            title={
              sort.dir === 'asc'
                ? m.filter_sort_dir_ascending()
                : m.filter_sort_dir_descending()
            }
          >
            <span aria-hidden="true">{sort.dir === 'asc' ? '▲' : '▼'}</span>
          </Btn>
        </div>
      )}

      {actions != null && <div className={ft.actions}>{actions}</div>}
    </div>
  );
}
