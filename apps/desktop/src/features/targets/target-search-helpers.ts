// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * target-search-helpers.ts — pure helpers for client-side planner target
 * search (#103b, #29, spec-043).
 *
 * Extracted from TargetsPage.tsx (refactor sweep kyo7.104) so they can be
 * imported by useTargetsPageFilters without a circular dependency. TargetsPage
 * re-exports them so target-search.test.ts import paths stay stable.
 */

import type { TargetListItem } from '@/bindings/index';

/**
 * Normalize a designation or label for alias-aware matching (#103b).
 *
 * Collapses internal whitespace so "M31" and "M 31" become identical tokens
 * ("m31"). Case is folded to lower. This means "M31", "M 31", and "m 31" all
 * normalize to "m31" and match each other — the key astrophotography UX need
 * where catalog designations appear both spaced ("M 31") and compact ("M31").
 */
export function normalizeDesig(s: string): string {
  return s.toLowerCase().replace(/\s+/g, '');
}

/**
 * Designation- and label-aware match for the client-side filter.
 *
 * Matching strategy:
 *  1. Normalized exact/prefix/substring match on the collapsed designation and
 *     label (so "M31" matches "M 31" and vice versa).
 *  2. Unnormalized substring on effectiveLabel for proper names
 *     ("Andromeda" substring of "Andromeda Galaxy").
 *
 * Alias matching moved server-side via `target.list(search)` (GF-11 / DS-16):
 * `aliases` is no longer serialized on `TargetListItem`, so the backend owns
 * the alias-aware pass over the ~13k-alias catalog.
 */
export function matchesSearch(t: TargetListItem, query: string): boolean {
  const qNorm = normalizeDesig(query);
  const qLower = query.toLowerCase();
  if (normalizeDesig(t.primaryDesignation).includes(qNorm)) return true;
  if (normalizeDesig(t.effectiveLabel).includes(qNorm)) return true;
  // Plain lowercase substring on effectiveLabel for proper names
  // ("andromeda" in "Andromeda Galaxy") without whitespace collapsing.
  if (t.effectiveLabel.toLowerCase().includes(qLower)) return true;
  return false;
}
