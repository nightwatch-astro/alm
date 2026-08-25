// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * LogPanel model layer — pure filter, severity, and navigation helpers.
 * No React, no JSX.
 */

import { m } from '@/lib/i18n';
import { entityPath } from '@/lib/entity-path';
import type { LogLevel, LogEntrySource } from '@/data/logStore';
import type { LevelFilter } from './LogPanelContext';

// ── Level chip display helpers ────────────────────────────────────────────────

// `label` is a render-time thunk so it re-reads the active locale (spec 046 #8).
export const LEVEL_CHIPS: { value: LevelFilter; label: () => string }[] = [
  { value: 'all', label: () => m.common_all() },
  { value: 'error', label: () => m.settings_advanced_log_error() },
  { value: 'warn', label: () => m.settings_advanced_log_warn() },
  { value: 'info', label: () => m.settings_advanced_log_info() },
  { value: 'debug', label: () => m.settings_advanced_log_debug() },
];

// Severity order (ascending). A level-chip selection is a floor: choosing
// e.g. "warn" shows warn AND error, matching conventional log-viewer
// semantics rather than an exact-level match (#582).
export const LEVEL_SEVERITY: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

export function passesLevelFilter(
  entryLevel: LogLevel,
  filter: LevelFilter,
): boolean {
  if (filter === 'all') return true;
  return LEVEL_SEVERITY[entryLevel] >= LEVEL_SEVERITY[filter];
}

export function passesSourceFilter(
  entrySource: LogEntrySource,
  filter: LogEntrySource[],
): boolean {
  if (filter.length === 0) return true;
  return filter.includes(entrySource);
}

// All known log-entry sources, for the category/source filter chips (#666).
// Kept local (not exported from `data/logStore`) — this is a UI concern only.
export const ALL_LOG_SOURCES: LogEntrySource[] = [
  'audit',
  'diagnostic',
  'catalog',
  'plan',
  'workflow',
  'lifecycle',
  'inventory',
  'settings',
  'project',
  'target',
  'tool',
];

/**
 * Names the filters currently narrowing the list, or `null` when none of the
 * user-selectable filters is active.
 *
 * #669 / Journey 13: a filtered-to-empty log must never render the same copy
 * as a log that recorded nothing, so the empty state names what is excluding
 * the rows. Returns `null` when only the non-user-selectable diagnostics gate
 * is doing the excluding — there is no filter name to show the user then.
 */
export function activeFilterLabel(
  levelFilter: LevelFilter,
  sourceFilter: LogEntrySource[],
): string | null {
  const parts: string[] = [];
  if (levelFilter !== 'all') {
    const chip = LEVEL_CHIPS.find((c) => c.value === levelFilter);
    if (chip) parts.push(chip.label());
  }
  parts.push(...sourceFilter);
  return parts.length > 0 ? parts.join(', ') : null;
}

// ── Entity navigation helpers ─────────────────────────────────────────────────

export type EntityNavigateFn = (entityType: string, entityId: string) => void;
export type AuditNavigateFn = (requestId: string) => void;

/**
 * Resolve an entity link's destination path, or `null` when the entity type
 * has no deep-linkable destination yet (row still shows subject-context text,
 * just without click affordance).
 *
 * Unlike the AuditLog's use of `entityPath`, the LogPanel links every remaining
 * type: `catalog` and the fallback point at the real Settings panes
 * (`/settings/$pane` is a plain path segment, so a literal string is fine here
 * — no route for `/audit` or `/settings?tab=catalogs` ever existed).
 */
export function buildEntityPath(
  entityType: string,
  entityId: string,
): string | null {
  return entityPath(entityType, entityId, {
    fallback: (type, id) =>
      type === 'catalog'
        ? `/settings/catalogs`
        : `/settings/audit?entityType=${type}&entityId=${id}`,
  });
}
