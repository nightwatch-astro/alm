// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useRef, useCallback, useState, useEffect } from 'react';
import { updateSettings } from './settingsIpc';

/**
 * Auto-save hook with 300ms debounce.
 * Each pane calls `save(scope, values)` on change.
 * Exposes a `saved` flag that resets after 1.5s for UI feedback.
 *
 * DS-7: a single shared timer drops the previous pending write when a second
 * edit within 300ms targets a DIFFERENT scope. We now accumulate pending
 * writes per scope in a Map, and flush ALL pending scopes when the debounce
 * fires — so no write is silently dropped across a scope boundary.
 *
 * Unmount / route-change flush: the pending Map is drained synchronously
 * (without the debounce delay) when the component unmounts.
 */
const FEEDBACK_MS = 1500;

export function useAutoSave() {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const feedbackRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Map<scope, latestValues> — accumulates the most-recent values per scope
  // within the debounce window so the flush writes all affected scopes.
  const pendingRef = useRef<Map<string, Record<string, unknown>>>(new Map());
  const [saved, setSaved] = useState(false);

  // Flush all pending scopes immediately (used on debounce fire and on unmount).
  const flushAll = useCallback(async () => {
    const pending = pendingRef.current;
    if (pending.size === 0) return;
    // Drain before the async loop so concurrent flushAll calls don't double-write.
    const snapshot = new Map(pending);
    pending.clear();
    for (const [scope, values] of snapshot) {
      // Errors are caught per-scope so one bad save cannot prevent flushing
      // the remaining scopes. On unmount the component is already gone — log
      // but do not surface as an unhandled rejection.
      try {
        await updateSettings({ scope, values });
      } catch (err) {
        console.error('useAutoSave: flush failed for scope', scope, err);
      }
    }
    setSaved(true);
    if (feedbackRef.current) clearTimeout(feedbackRef.current);
    feedbackRef.current = setTimeout(() => setSaved(false), FEEDBACK_MS);
  }, []);

  const save = useCallback(
    (scope: string, values: Record<string, unknown>) => {
      // Merge latest values for this scope; other scopes' pending writes survive.
      pendingRef.current.set(scope, values);

      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        void flushAll();
      }, 300);
    },
    [flushAll],
  );

  // Flush any pending writes when the component unmounts (route change / close).
  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      if (pendingRef.current.size > 0) void flushAll();
    },
    [flushAll],
  );

  return { save, saved };
}
