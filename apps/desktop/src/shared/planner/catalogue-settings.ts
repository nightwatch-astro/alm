// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * catalogue-settings.ts — default-enabled planner catalogues (task #82).
 *
 * Which catalogues are enabled by default in the Planner is a user setting,
 * persisted through the generic settings backend under the `'catalogues'`
 * scope (`commands.settingsGet` / `commands.settingsUpdate`), value shape
 * `{ enabled: string[] }`.
 *
 * The Planner top bar initializes its catalogue multi-select from this setting;
 * the Settings → Target Resolution pane edits it. Sensible default ON subset is
 * Messier + NGC + IC + Sharpless; the rest (LBN/LDN/Caldwell/Barnard) are off.
 */

import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { updateSettings } from '@/data/settingsWrite';
import {
  PLANNER_CATALOGS,
  type CatalogueId,
} from '@/shared/planner/planner-catalog';

/** Settings scope for the default-enabled planner catalogues. */
export const CATALOGUES_SCOPE = 'catalogues';

/** Every selectable catalogue id, in display order. */
export const ALL_CATALOGUE_IDS: readonly CatalogueId[] = PLANNER_CATALOGS.map(
  (c) => c.id,
);

/** Sensible default ON subset when no setting is persisted yet. */
export const DEFAULT_ENABLED_CATALOGUES: readonly CatalogueId[] = [
  'M',
  'NGC',
  'IC',
  'Sh2',
];

const VALID = new Set<string>(ALL_CATALOGUE_IDS);

/** Coerce an unknown persisted value into a clean, ordered catalogue id list. */
function coerce(value: unknown): CatalogueId[] {
  const raw =
    value &&
    typeof value === 'object' &&
    Array.isArray((value as { enabled?: unknown }).enabled)
      ? (value as { enabled: unknown[] }).enabled
      : null;
  if (!raw) return [...DEFAULT_ENABLED_CATALOGUES];
  const set = new Set(
    raw.filter((v): v is string => typeof v === 'string' && VALID.has(v)),
  );
  // Preserve canonical display order regardless of stored order.
  return ALL_CATALOGUE_IDS.filter((id) => set.has(id));
}

/**
 * Load the default-enabled catalogues. Falls back to DEFAULT_ENABLED_CATALOGUES
 * when the backend is unavailable or nothing is persisted yet.
 */
export async function loadDefaultCatalogues(): Promise<CatalogueId[]> {
  try {
    const data = unwrap(await commands.settingsGet(CATALOGUES_SCOPE));
    return coerce(data.values);
  } catch {
    return [...DEFAULT_ENABLED_CATALOGUES];
  }
}

/** Persist the default-enabled catalogues. */
export async function saveDefaultCatalogues(
  enabled: readonly CatalogueId[],
): Promise<void> {
  await updateSettings({
    scope: CATALOGUES_SCOPE,
    values: { enabled: [...enabled] },
  });
}
