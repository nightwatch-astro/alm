// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Log ring buffer and subscription store (spec 019).
 *
 * Manages a 500-entry FIFO ring buffer of LogEntry items fed by:
 * 1. `logSubscription.ts` — live backend stream via `log:entry` Tauri events.
 * 2. `log.recent` command — initial hydration on first subscribe.
 *
 * Architecture:
 * - `appendLog(entries)` dedupes by `id` and evicts oldest when over capacity.
 * - `useLog()` hook returns the current buffer snapshot.
 * - `dropped` counts total evicted entries since session start (diagnostics only).
 * - Ring buffer ordering is newest-first for render (reverse of wire order).
 *
 * Notification contract:
 * `notify()` coalesces rapid `appendLog` calls via `requestAnimationFrame`.
 * Listeners fire asynchronously (next paint frame), not synchronously on
 * `appendLog`. `useSyncExternalStore` in LogPanel handles this correctly —
 * React re-renders on the next frame rather than within the same microtask.
 * Tests that assert listener call counts must advance fake timers to flush
 * the pending rAF (see logStore.ringBuffer.test.ts).
 */

type Listener = () => void;

export const LOG_BUFFER_SIZE = 500;

/** Severity level (matches spec 019 LogEntry schema). */
export type LogLevel = 'error' | 'warn' | 'info' | 'debug';

/** Source tag (matches spec 019 LogEntry schema). */
export type LogEntrySource =
  | 'audit'
  | 'diagnostic'
  | 'catalog'
  | 'plan'
  | 'workflow'
  | 'lifecycle'
  | 'inventory'
  | 'settings'
  | 'project'
  | 'target'
  | 'tool';

/** A projected log entry from the backend (matches spec 019 data-model.md). */
export interface LogEntry {
  id: string;
  contractVersion: string;
  time: string;
  level: LogLevel;
  source: LogEntrySource;
  message: string;
  requestId?: string;
  entityType?: string;
  entityId?: string;
}

interface LogBufferState {
  /** Entries in newest-first order for render. */
  entries: LogEntry[];
  /** Total entries evicted since session start. */
  dropped: number;
  /** True when the stream reported a history gap (truncated cursor). */
  truncated: boolean;
  truncatedCount?: number;
}

// ── Internal state ────────────────────────────────────────────────────────────

let state: LogBufferState = {
  entries: [],
  dropped: 0,
  truncated: false,
};

const listeners = new Set<Listener>();
// Fast dedup set on entry ids.
const seenIds = new Set<string>();

let notifyScheduled = false;
function notify() {
  if (notifyScheduled) return;
  notifyScheduled = true;
  requestAnimationFrame(() => {
    notifyScheduled = false;
    for (const listener of listeners) {
      listener();
    }
  });
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Ordering key for newest-first render: `[time, tiebreak]`, compared
 * lexicographically.
 *
 * `time` is whole-second ISO-8601, so same-second entries need the tiebreak.
 * `aud:<n>` uses the backing `event_id`, which is monotonic. `dia:<seq>` rows
 * are in-memory diagnostics emitted from the live path, so they sort after every
 * `aud` row sharing their second.
 */
function orderKey(entry: LogEntry): [string, number] {
  const seq = Number.parseInt(entry.id.slice(entry.id.indexOf(':') + 1), 10);
  const rank = Number.isNaN(seq) ? 0 : seq;
  return [
    entry.time,
    entry.id.startsWith('dia:') ? DIA_RANK_BASE + rank : rank,
  ];
}

/** Above any plausible `event_id`, so diagnostics sort after audit rows. */
const DIA_RANK_BASE = Number.MAX_SAFE_INTEGER / 2;

/**
 * Append one or more log entries to the ring buffer.
 *
 * - Dedupes by `id` so reconnect replay does not produce duplicate rows.
 * - Sorts the merged buffer newest-first rather than trusting arrival order: a
 *   live `log:entry` can land before the `log.recent` hydration it postdates.
 * - Evicts oldest entries (from the tail of the array, i.e. the oldest)
 *   when `capacity` is exceeded.
 */
export function appendLog(newEntries: LogEntry[]): void {
  if (newEntries.length === 0) return;

  const toAdd = newEntries.filter((e) => !seenIds.has(e.id));
  if (toAdd.length === 0) return;

  for (const e of toAdd) seenIds.add(e.id);

  const combined = [...toAdd, ...state.entries].sort((a, b) => {
    const [aTime, aRank] = orderKey(a);
    const [bTime, bRank] = orderKey(b);
    if (aTime !== bTime) return aTime < bTime ? 1 : -1;
    return bRank - aRank;
  });

  // Evict from tail (oldest) when over capacity.
  let dropped = state.dropped;
  let trimmed = combined;
  if (combined.length > LOG_BUFFER_SIZE) {
    const excess = combined.length - LOG_BUFFER_SIZE;
    dropped += excess;
    trimmed = combined.slice(0, LOG_BUFFER_SIZE);
    // Remove evicted ids from dedup set.
    for (const evicted of combined.slice(LOG_BUFFER_SIZE)) {
      seenIds.delete(evicted.id);
    }
  }

  state = { ...state, entries: trimmed, dropped };
  notify();
}

/** Mark the stream as truncated (history gap). */
export function markTruncated(count?: number): void {
  state = { ...state, truncated: true, truncatedCount: count };
  notify();
}

/** Return the current buffer snapshot. */
export function getLogSnapshot(): LogBufferState {
  return state;
}

/** Subscribe to buffer changes. Returns an unsubscribe function. */
export function subscribeLog(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Reset the buffer (used in tests). */
export function resetLogStore(): void {
  state = { entries: [], dropped: 0, truncated: false };
  seenIds.clear();
  listeners.clear();
  notifyScheduled = false;
}
