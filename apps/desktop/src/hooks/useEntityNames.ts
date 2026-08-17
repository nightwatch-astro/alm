// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useEntityNames -- resolves `(entityType, entityId)` refs to display names.
 *
 * Extracted from AuditLog's per-row resolver (#803) so Audit Log, Projects
 * Sources, and the Calibration match panel share one implementation instead
 * of three ad hoc treatments (#809).
 *
 * - `project` / `target` / `plan`: one batched `entity.names` IPC call for
 *   all unseen refs, cached module-wide since ids repeat across pages within
 *   a session.  Previously one round-trip per unseen id (GF-7 / DS-14 fix).
 * - `session`: sessions have no cheap per-id name lookup (`sessions.get`
 *   returns session detail, not a display name), so names are read from the
 *   already-fetched inventory-sources query instead of adding a new IPC
 *   command -- the same batch data the Sessions page and Projects Sources
 *   view already warm (#663).
 *
 * Callers decide the fallback text for a ref that never resolves (unknown id,
 * lookup failed, or still pending) -- this hook only returns names it
 * actually resolved.
 */

import { useEffect, useRef, useState } from 'react';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { useInventorySources } from '@/features/sessions/store';

export interface EntityNameRef {
  entityType: string;
  entityId: string;
}

/** Module-scope cache for the IPC batch lookups (project/target/plan). */
const entityNameCache = new Map<string, string | null>();

/** Clear the entity-name cache — for use in tests to prevent cross-test pollution. */
export function clearEntityNameCache(): void {
  entityNameCache.clear();
}

export function entityNameKey(ref: EntityNameRef): string {
  return `${ref.entityType}:${ref.entityId}`;
}

export function useEntityNames(
  refs: readonly EntityNameRef[],
): Map<string, string> {
  const [, forceRender] = useState(0);
  const requested = useRef(new Set<string>());
  const { data: inventoryData } = useInventorySources();

  useEffect(() => {
    let cancelled = false;

    // Collect refs not yet cached or in-flight.  Session names come from the
    // inventory-sources store; unknown types are skipped (backend ignores them
    // anyway but filtering here avoids a wasted IPC call).
    const BATCH_TYPES = new Set(['project', 'plan', 'target']);
    const unseen = refs.filter((ref) => {
      if (!BATCH_TYPES.has(ref.entityType)) return false;
      const key = entityNameKey(ref);
      return !entityNameCache.has(key) && !requested.current.has(key);
    });

    if (unseen.length === 0) return;

    // Mark all unseen as in-flight before the async call to prevent
    // duplicate dispatches from concurrent renders.
    for (const ref of unseen) {
      requested.current.add(entityNameKey(ref));
    }

    // Single batch IPC call instead of N sequential round-trips (GF-7).
    void commands
      .entityNames(
        unseen.map((r) => ({ entityType: r.entityType, entityId: r.entityId })),
      )
      .then(unwrap)
      .then(({ names }) => {
        // Populate cache for all refs in the batch — present entries get their
        // name, absent entries (not in DB) get null so we don't re-request.
        for (const ref of unseen) {
          const key = entityNameKey(ref);
          entityNameCache.set(key, names[key] ?? null);
        }
        if (!cancelled) forceRender((n) => n + 1);
      })
      .catch(() => {
        // On failure leave the cache empty — the hook will retry on the next
        // render cycle when the refs change (e.g. page navigation).
        for (const ref of unseen) {
          requested.current.delete(entityNameKey(ref));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [refs]);

  const sessionNames = new Map<string, string>();
  for (const source of inventoryData?.sources ?? []) {
    for (const session of source.sessions) {
      sessionNames.set(session.id, session.name);
    }
  }

  const names = new Map<string, string>();
  for (const ref of refs) {
    const cacheKey = entityNameKey(ref);
    if (ref.entityType === 'session') {
      const name = sessionNames.get(ref.entityId);
      if (name) names.set(cacheKey, name);
      continue;
    }
    const cached = entityNameCache.get(cacheKey);
    if (cached) names.set(cacheKey, cached);
  }
  return names;
}
