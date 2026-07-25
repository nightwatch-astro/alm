// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Ring buffer eviction tests (spec 019, T022).
 *
 * Verifies that:
 * - Oldest entries are evicted first when capacity is exceeded.
 * - `dropped` counter increments correctly.
 * - Deduplication by `id` prevents double-appending.
 *
 * The listener-notification test runs in an isolated module instance
 * (vi.resetModules + dynamic import) because logStore is a process singleton
 * and orphan requestAnimationFrame callbacks from other test files in the same
 * vitest worker can fire during this test, inflating the listener call count.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  appendLog,
  getLogSnapshot,
  resetLogStore,
  type subscribeLog,
  LOG_BUFFER_SIZE,
  type LogEntry,
} from './logStore';

function makeEntry(n: number): LogEntry {
  return {
    id: `aud:${n}`,
    contractVersion: '1',
    time: `2026-01-01T00:${String(n).padStart(2, '0')}:00Z`,
    level: 'info',
    source: 'plan',
    message: `Entry ${n}`,
  };
}

describe('logStore ring buffer', () => {
  beforeEach(() => {
    resetLogStore();
  });

  it('appends entries and returns them newest-first', () => {
    appendLog([makeEntry(1), makeEntry(2), makeEntry(3)]);
    const { entries } = getLogSnapshot();
    // Newest first: entry 3 is at index 0.
    expect(entries[0].id).toBe('aud:3');
    expect(entries[1].id).toBe('aud:2');
    expect(entries[2].id).toBe('aud:1');
  });

  it('starts with dropped=0', () => {
    const { dropped } = getLogSnapshot();
    expect(dropped).toBe(0);
  });

  it('evicts oldest entries when capacity is exceeded', () => {
    // Fill beyond capacity.
    const batch: LogEntry[] = [];
    for (let i = 1; i <= LOG_BUFFER_SIZE + 10; i++) {
      batch.push(makeEntry(i));
    }
    appendLog(batch);

    const { entries, dropped } = getLogSnapshot();
    expect(entries.length).toBe(LOG_BUFFER_SIZE);
    expect(dropped).toBe(10);
    // Newest entry (index LOG_BUFFER_SIZE+10) should be at position 0.
    expect(entries[0].id).toBe(`aud:${LOG_BUFFER_SIZE + 10}`);
    // The oldest evicted entries (1–10) must not be present.
    expect(entries.find((e) => e.id === 'aud:1')).toBeUndefined();
    expect(entries.find((e) => e.id === 'aud:10')).toBeUndefined();
    // Entry 11 should be the oldest remaining.
    expect(entries[entries.length - 1].id).toBe('aud:11');
  });

  it('deduplicates entries by id', () => {
    appendLog([makeEntry(1), makeEntry(2)]);
    appendLog([makeEntry(2), makeEntry(3)]); // entry 2 is a duplicate

    const { entries } = getLogSnapshot();
    const ids = entries.map((e) => e.id);
    expect(ids).toHaveLength(3);
    expect(ids.filter((id) => id === 'aud:2')).toHaveLength(1);
  });

  it('accumulates dropped across multiple append calls', () => {
    // First batch fills the buffer exactly.
    const first: LogEntry[] = [];
    for (let i = 1; i <= LOG_BUFFER_SIZE; i++) first.push(makeEntry(i));
    appendLog(first);

    // Second batch pushes 5 more: should evict 5.
    appendLog([
      makeEntry(LOG_BUFFER_SIZE + 1),
      makeEntry(LOG_BUFFER_SIZE + 2),
      makeEntry(LOG_BUFFER_SIZE + 3),
      makeEntry(LOG_BUFFER_SIZE + 4),
      makeEntry(LOG_BUFFER_SIZE + 5),
    ]);

    const { entries, dropped } = getLogSnapshot();
    expect(entries.length).toBe(LOG_BUFFER_SIZE);
    expect(dropped).toBe(5);
  });

  it('handles empty append gracefully', () => {
    appendLog([makeEntry(1)]);
    appendLog([]);
    const { entries } = getLogSnapshot();
    expect(entries.length).toBe(1);
  });

  // Listener notification test is in a nested describe with its own fresh
  // module instance so orphan rAF callbacks from other test files cannot
  // inflate the call count (logStore singleton shared across vitest worker).
  describe('listener notification (isolated module)', () => {
    let isolatedAppendLog: typeof appendLog;
    let isolatedSubscribeLog: typeof subscribeLog;

    beforeEach(async () => {
      vi.resetModules();
      const mod = await import('./logStore');
      isolatedAppendLog = mod.appendLog;
      isolatedSubscribeLog = mod.subscribeLog;
    });

    it('notifies listeners on append (async via rAF batch)', async () => {
      vi.useFakeTimers();

      let callCount = 0;
      const unsub = isolatedSubscribeLog(() => {
        callCount++;
      });

      isolatedAppendLog([makeEntry(1)]);
      // notify() schedules a requestAnimationFrame — flush it.
      await vi.runAllTimersAsync();
      expect(callCount).toBe(1);

      isolatedAppendLog([makeEntry(2)]);
      await vi.runAllTimersAsync();
      expect(callCount).toBe(2);

      unsub();
      vi.useRealTimers();
    });
  });
});
