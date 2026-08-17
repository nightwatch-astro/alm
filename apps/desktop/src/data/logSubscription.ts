// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Log stream subscription (spec 019, T020).
 *
 * Subscribes to the backend log stream in two ways:
 * 1. Live updates: listens for `log:entry` Tauri events forwarded by the
 *    backend bus→Tauri forwarder.
 * 2. Initial hydration: calls the `logRecent` binding (`log_recent`) to populate
 *    the ring buffer with the most recent 500 entries.
 *
 * The listener attaches BEFORE hydration. Reversing that order leaves a window
 * in which a committed row is past the hydration query and ahead of the
 * listener: the forwarder emits it to nobody and advances its cursor past it, so
 * no later drain re-sends it. The overlap the listener-first order creates
 * instead is absorbed by `appendLog`'s id dedup and ordering.
 *
 * Deduplication is handled by `appendLog` in `logStore.ts`.
 *
 * In mock/test mode (VITE_USE_MOCKS=true) the Tauri listen API is unavailable;
 * the subscription is a no-op and the mock invoker seeds the initial entries.
 */

import {
  appendLog,
  getLogSnapshot,
  markTruncated,
  type LogEntry,
} from './logStore';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';

const IS_MOCK = import.meta.env.VITE_USE_MOCKS === 'true';

let unlisten: (() => void) | null = null;
let subscribed = false;

// ── Recent entries pull ───────────────────────────────────────────────────────

async function fetchRecentEntries(cursor?: string): Promise<void> {
  try {
    // Conditionally import based on mock mode.
    if (IS_MOCK) {
      // In mock mode, seed with a few placeholder entries.
      const { MOCK_LOG_ENTRIES } = await import('./mockLogEntries');
      appendLog(MOCK_LOG_ENTRIES);
      return;
    }

    // Use the generated `logRecent` binding (registered command `log_recent`)
    // via the shared switcher — NOT a raw dotted `invoke('log.recent')`, which
    // the real backend rejects as "command not found" (silently losing the
    // on-open history backfill; see spec 019 closeout).
    const response = unwrap(
      await commands.logRecent(cursor ?? null, null, true, null, 500),
    );

    if (response.truncated) {
      markTruncated(response.truncatedCount ?? undefined);
    }
    // Runtime shape is identical to the local LogEntry (the wrapper documents
    // this); cast to satisfy the ring-buffer's local type.
    const entries = response.entries as unknown as LogEntry[];
    if (entries.length > 0) {
      appendLog(entries);
    }
  } catch (err) {
    // Non-fatal: the log panel shows whatever is in the buffer.
    console.warn('[logSubscription] fetchRecentEntries failed:', err);
  }
}

// ── Live event listener ───────────────────────────────────────────────────────

async function startLiveListener(): Promise<void> {
  if (IS_MOCK) return;

  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlistenFn = await listen<LogEntry>('log:entry', (event) => {
      appendLog([event.payload]);
    });
    unlisten = unlistenFn;
  } catch (err) {
    console.warn('[logSubscription] listen failed:', err);
  }
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Start the log subscription (idempotent).
 *
 * On first call: starts the live listener, then fetches the initial window.
 * On subsequent calls: no-op (dedup handles reconnect replay).
 */
export async function startLogSubscription(): Promise<void> {
  if (subscribed) return;
  subscribed = true;

  // Read the resume cursor before the listener can widen the buffer: a live
  // entry arriving first would otherwise become the cursor and suppress the
  // older history this hydration exists to fetch.
  //
  // Entries are newest-first; we prefer the latest aud: entry as cursor because
  // dia: (diagnostic) entries are in-memory only and have no DB row to resume from.
  // Without this, a dia: entry as the last seen causes a full replay (T062 FR-025).
  const snapshot = getLogSnapshot();
  const audEntry = snapshot.entries.find((e) => e.id.startsWith('aud:'));
  const cursor = audEntry?.id;

  await startLiveListener();
  await fetchRecentEntries(cursor);
}

/**
 * Stop the live listener and reset subscription state.
 *
 * Does NOT clear the ring buffer — entries remain visible after reconnect
 * until they are naturally evicted.
 */
export function stopLogSubscription(): void {
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
  subscribed = false;
}
