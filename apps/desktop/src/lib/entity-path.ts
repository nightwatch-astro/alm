// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Shared entity deep-link resolution for audit/log surfaces.
 *
 * Only entity types with a reachable detail page resolve to a path; everything
 * else routes through `fallback` so a row without a destination stays unlinked
 * rather than navigating to a dead end.
 *
 * `plan` resolves to `null` even though `ENTITY_TYPE_VALUES` includes it — no
 * `/plans/:id` route exists (#626) — and it must stay ahead of `fallback` so a
 * caller with a non-null fallback does not link it by accident.
 */

/** Resolves entity types without a dedicated detail page. */
export type EntityPathFallback = (
  entityType: string,
  entityId: string,
) => string | null;

export interface EntityPathOptions {
  /** Destination for entity types with no detail page. Defaults to unlinked. */
  fallback?: EntityPathFallback;
}

export function entityPath(
  entityType: string,
  entityId: string,
  options: EntityPathOptions = {},
): string | null {
  switch (entityType) {
    case 'project':
      return `/projects/${entityId}`;
    case 'session':
      return `/sessions/${entityId}`;
    case 'target':
      return `/targets/${entityId}`;
    case 'plan':
      return null;
    default:
      return options.fallback?.(entityType, entityId) ?? null;
  }
}
