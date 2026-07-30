// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * InboxControls — grouping dimension registry for the Inbox list.
 *
 * spec 043 (tasks #73/#75/#76 + #31/#63): grouping, sort, and lane-filter
 * controls moved into the shared FilterToolbar (InboxPage). This module now
 * exports only the dimension registry consumed by InboxPage (grouping prop)
 * and InboxList (ACCESSORS / DIM_LABELS). The bespoke configurator JSX and
 * `useInboxControls` hook have been retired; InboxPage uses `useGrouping`
 * from `@/lib/use-grouping` instead.
 *
 * `InboxControls` is kept as a thin shim that renders ONLY the grouping
 * selects (the same selects the shared FilterToolbar.grouping renders) so
 * existing tests that render InboxControls standalone continue to pass.
 *
 * Frame-type filter is now a FilterToolbar `fields` entry on InboxPage.
 */

import type { DimensionAccessor } from './grouping';
import type { InboxListItem } from '@/bindings/index';
import { m } from '@/lib/i18n';
import { selectBase } from '@/styles/select.css';

// ── Grouping dimension registry ─────────────────────────────────────────────────

/** A user-selectable grouping dimension. */
export interface Dimension {
  id: string;
  /** Render-time thunk (spec 046 #8b) so the label re-reads the active locale. */
  label: () => string;
  accessor: DimensionAccessor<InboxListItem>;
}

/** Basename (last path segment) of an absolute path, for the "source" dimension. */
function basename(p: string | null | undefined): string | null {
  if (!p) return null;
  const trimmed = p.replace(/[/\\]+$/, '');
  const parts = trimmed.split(/[/\\]/);
  return parts[parts.length - 1] || null;
}

/**
 * Ordered registry of dimensions the user can group by. Accessors return the
 * string value or null/undefined; the engine buckets null/empty under its
 * NONE_KEY → "(none)" label.
 */
export const GROUPING_DIMENSIONS: readonly Dimension[] = [
  {
    /**
     * Spec 058 T034 / FR-025 / SC-010: group siblings under their folder.
     *
     * A folder that split into several frame types produces N rows sharing one
     * `relativePath`. Without this the list reads as N near-identical
     * detections rather than "one folder, N types". `relativePath` IS the
     * folder identity for an item — a materialized sub-item inherits its source
     * folder's path — so grouping on it puts exactly the siblings of one folder
     * under one header, which is what FR-025 asks for.
     *
     * Listed first because it explains the list's post-058 shape; every other
     * dimension describes what is inside a folder rather than which folder a
     * row came from.
     */
    id: 'folder',
    label: () => m.inbox_dim_folder(),
    accessor: (i) => i.relativePath,
  },
  {
    id: 'target',
    label: () => m.inbox_dim_target(),
    accessor: (i) => i.groupTarget,
  },
  {
    id: 'frameType',
    label: () => m.inbox_frame_type_label(),
    accessor: (i) => i.groupFrameType,
  },
  {
    id: 'date',
    label: () => m.archive_prop_date(),
    accessor: (i) => i.groupDate,
  },
  {
    id: 'filter',
    label: () => m.common_filter(),
    accessor: (i) => i.groupFilter,
  },
  {
    id: 'exposure',
    label: () => m.inbox_dim_exposure(),
    accessor: (i) => i.groupExposure,
  },
  {
    id: 'instrument',
    label: () => m.inbox_dim_instrument(),
    accessor: (i) => i.groupInstrument,
  },
  {
    id: 'source',
    label: () => m.inbox_dim_source(),
    accessor: (i) => basename(i.rootAbsolutePath),
  },
  {
    id: 'format',
    label: () => m.inbox_dim_format(),
    accessor: (i) => i.format,
  },
  {
    id: 'orgState',
    label: () => m.inbox_dim_org_state(),
    accessor: (i) => i.organizationState,
  },
];

/** Accessor map keyed by dimension id, consumed by `groupByDimensions`. */
export const ACCESSORS: Record<
  string,
  DimensionAccessor<InboxListItem>
> = Object.fromEntries(GROUPING_DIMENSIONS.map((d) => [d.id, d.accessor]));

/**
 * Resolve a dimension's label in the active locale by id (spec 046 #8b). Reads
 * the thunk at call time, replacing the former frozen `DIM_LABELS` map so the
 * label re-evaluates when the locale changes.
 */
export function dimLabel(id: string): string {
  return GROUPING_DIMENSIONS.find((d) => d.id === id)?.label() ?? id;
}

/** Number of ordered grouping slots offered in the configurator. */
const MAX_GROUP_LEVELS = 3;

/** localStorage key for the persisted ordered grouping dimensions. */
export const GROUPING_STORAGE_KEY = 'inbox.grouping.dims.v1';

// ── Thin shim (backward-compat for existing tests) ──────────────────────────────
// InboxPage no longer mounts InboxControls; this shim lets affordance tests
// render the grouping selects in isolation without changing their imports.

export interface InboxControlsProps {
  dims: string[];
  setSlot: (slot: number, value: string) => void;
  /** @deprecated Sort state is now owned by InboxList column headers. Ignored. */
  sortBy?: string;
  /** @deprecated Sort state is now owned by InboxList column headers. Ignored. */
  onSortByChange?: (v: string) => void;
  /** @deprecated Lane filter is now a FilterToolbar `fields` entry. Ignored. */
  filterType?: string;
  /** @deprecated Lane filter is now a FilterToolbar `fields` entry. Ignored. */
  onFilterTypeChange?: (type: string | undefined) => void;
}

/**
 * Thin shim: renders only the ordered grouping selects. The sort dropdown and
 * lane-filter dropdown have been retired to FilterToolbar on InboxPage.
 */
export function InboxControls({ dims, setSlot }: InboxControlsProps) {
  return (
    <div className="pv-inbox-list__controls pv-inbox-list__controls--toolbar">
      <div className="pv-inbox-list__group-row">
        {Array.from({ length: MAX_GROUP_LEVELS }).map((_, slot) => {
          const value = dims[slot] ?? '';
          const disabled = slot > 0 && !dims[slot - 1];
          const usedEarlier = new Set(dims.slice(0, slot));
          return (
            <select
              key={slot}
              className={selectBase}
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
              {GROUPING_DIMENSIONS.filter(
                (d) => d.id === value || !usedEarlier.has(d.id),
              ).map((d) => (
                <option key={d.id} value={d.id}>
                  {slot === 0
                    ? m.inbox_groupby_chip_primary({ label: d.label() })
                    : m.inbox_groupby_chip_secondary({ label: d.label() })}
                </option>
              ))}
            </select>
          );
        })}
      </div>
    </div>
  );
}

/**
 * @deprecated Use `useGrouping` from `@/lib/use-grouping` instead.
 * Kept as a re-export shim so import sites compile without changes.
 */
export { useGrouping as useInboxControls } from '@/lib/use-grouping';
