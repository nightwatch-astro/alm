// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-catalog.ts — Allowed-catalog filter for the Target Planner (task #40, spec 043 §4).
 *
 * The Planner must list ONLY catalog objects a user would realistically plan a
 * session around — Messier, NGC, IC, Sharpless, LBN, LDN, Caldwell, Barnard —
 * NOT the ~13k SIMBAD double stars the raw `target.list` endpoint returns.
 *
 * STUB: this is a CLIENT-SIDE designation-prefix filter. The real fix is a
 * backend catalog filter on the list endpoint (task #57 — "Targets list-endpoint
 * enrichment"). Until that lands, we filter the full list in the browser by the
 * `primaryDesignation` prefix. This is a heuristic on the designation string,
 * not a catalog-membership lookup, so it can over- or under-match unusual
 * designations.
 *
 * Task #82 adds per-catalogue classification (`catalogueOf`) so the Planner top
 * bar can offer a multi-select catalogue filter and a "group by catalogue"
 * grouping, and so Settings can persist a default-enabled catalogue subset.
 */

import type { TargetListItem } from '@/bindings/index';
import { m } from '@/lib/i18n';

/** Stable id for each planner catalogue (persisted in Settings + used as group key). */
export type CatalogueId =
  | 'M'
  | 'NGC'
  | 'IC'
  | 'Sh2'
  | 'LBN'
  | 'LDN'
  | 'C'
  | 'B';

/**
 * Allowed planner catalogs, keyed by the designation prefix that identifies
 * each. Matching is case-insensitive and anchored at the start of the
 * `primaryDesignation`, with the prefix followed by whitespace or a digit so
 * that e.g. "IC" does not also match "ICRS" and "B" (Barnard) does not match
 * arbitrary "B..." designations that are not "B <number>".
 *
 * Order matters for classification: longer / more-specific prefixes ("NGC",
 * "Sh2", "IC", "LBN", "LDN") MUST be tested before the single-letter prefixes
 * ("M", "C", "B") so "NGC 7000" classifies as NGC, not as a stray "N…".
 */
// `label` is a render-time thunk so it re-reads the active locale (spec 046 #8).
export const PLANNER_CATALOGS: ReadonlyArray<{
  id: CatalogueId;
  prefix: string;
  label: () => string;
}> = [
  { id: 'NGC', prefix: 'NGC', label: () => m.targets_catalog_ngc() },
  { id: 'Sh2', prefix: 'Sh2', label: () => m.targets_catalog_sharpless() },
  { id: 'LBN', prefix: 'LBN', label: () => m.targets_catalog_lbn() },
  { id: 'LDN', prefix: 'LDN', label: () => m.targets_catalog_ldn() },
  { id: 'IC', prefix: 'IC', label: () => m.targets_catalog_ic() },
  { id: 'M', prefix: 'M', label: () => m.targets_catalog_messier() },
  { id: 'C', prefix: 'C', label: () => m.targets_catalog_caldwell() },
  { id: 'B', prefix: 'B', label: () => m.targets_catalog_barnard() },
];

/** Human label for a catalogue id (e.g. "M" → "Messier"). */
export function catalogueLabel(id: CatalogueId): string {
  return PLANNER_CATALOGS.find((c) => c.id === id)?.label() ?? id;
}

/**
 * Per-catalogue matcher. Each prefix must be followed by optional whitespace or
 * a hyphen and then a digit (catalog designations are "<prefix> <number>"),
 * e.g. "M 31", "NGC 7000", "Sh2-155", "C 14". The trailing `[\s-]?\d` guard
 * prevents matching unrelated tokens that merely start with the same letters
 * (e.g. "Cygnus", "ICRS", "Barnard's Star" spelled out).
 */
const CATALOG_MATCHERS: ReadonlyArray<{ id: CatalogueId; re: RegExp }> =
  PLANNER_CATALOGS.map((c) => ({
    id: c.id,
    re: new RegExp(`^${c.prefix}[\\s-]?\\d`, 'i'),
  }));

/** Build a single regex that matches any allowed-catalog designation. */
const PLANNER_DESIGNATION_RE = new RegExp(
  `^(?:${PLANNER_CATALOGS.map((c) => c.prefix).join('|')})[\\s-]?\\d`,
  'i',
);

/**
 * Classify a target into its planner catalogue, or `null` when its primary
 * designation does not belong to any allowed catalogue. Tested in
 * PLANNER_CATALOGS order so multi-letter prefixes win over single-letter ones.
 */
export function catalogueOf(t: TargetListItem): CatalogueId | null {
  const d = t.primaryDesignation.trim();
  for (const m of CATALOG_MATCHERS) {
    if (m.re.test(d)) return m.id;
  }
  return null;
}

/** True when the target's primary designation belongs to an allowed planner catalog. */
export function isPlannerCatalogTarget(t: TargetListItem): boolean {
  return PLANNER_DESIGNATION_RE.test(t.primaryDesignation.trim());
}

/**
 * STUB: client-side catalog filter. Returns only targets whose designation
 * belongs to an allowed planner catalog. Replace with a backend catalog filter
 * (task #57) once the list endpoint can filter server-side.
 */
export function filterPlannerCatalog(
  targets: TargetListItem[],
): TargetListItem[] {
  return targets.filter(isPlannerCatalogTarget);
}

/**
 * Restrict targets to the enabled catalogue subset (task #82 multi-select). A
 * target is kept when its catalogue is in `enabled`; targets not in any planner
 * catalogue are always dropped. When `enabled` is empty the result is empty
 * (no catalogue selected → nothing to show).
 */
export function filterByCatalogues(
  targets: TargetListItem[],
  enabled: ReadonlySet<CatalogueId>,
): TargetListItem[] {
  return targets.filter((t) => {
    const cat = catalogueOf(t);
    return cat != null && enabled.has(cat);
  });
}
