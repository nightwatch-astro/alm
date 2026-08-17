// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * step-scan-model.ts — pure data-layer for the "Scan" step (spec 038).
 *
 * Extracted from StepScan.tsx (refactor sweep kyo7.104) so the async scan
 * logic and discriminated-union target types can be tested and reasoned about
 * independently of rendering.
 */

import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type { InboxItemSummary } from '@/bindings/index';
import type { InboxClassifyResponse } from '@/bindings/aliases';
import type { SourceEntry } from '../sources-store';

// ── ScanPhase ─────────────────────────────────────────────────────────────────

export type ScanPhase = 'pending' | 'scanning' | 'done' | 'error';

// ── RenderTarget discriminated union ─────────────────────────────────────────

/** A source resolved to a real backend root, ready to scan+classify. */
export interface ScanRenderTarget {
  source: SourceEntry;
  rootId: string;
  kind: 'scan';
}
/** A source whose already-scanned status couldn't be resolved (roots.list
 * failed or returned no match) — surfaced as a static error, never scanned. */
export interface ResolutionErrorRenderTarget {
  source: SourceEntry;
  rootId: string;
  kind: 'resolution-error';
}
export type RenderTarget = ScanRenderTarget | ResolutionErrorRenderTarget;

export function isScanTarget(t: RenderTarget): t is ScanRenderTarget {
  return t.kind === 'scan';
}

// ── queryPhase ────────────────────────────────────────────────────────────────

/** Map a `useQueries` result onto the same phase vocabulary the display states use. */
export function queryPhase(q: {
  status: 'pending' | 'error' | 'success';
  fetchStatus: 'fetching' | 'paused' | 'idle';
}): ScanPhase {
  if (q.status === 'success') return 'done';
  if (q.status === 'error') return 'error';
  return q.fetchStatus === 'fetching' ? 'scanning' : 'pending';
}

// ── scanAndClassify ───────────────────────────────────────────────────────────

/** One scan+classify pass result for a single source. */
export interface ScanResult {
  items: InboxItemSummary[];
  classifications: Map<string, InboxClassifyResponse>;
  classifyFailures: Set<string>;
}

/**
 * One scan+classify pass for a single source. Classify failures are captured
 * per-item (never thrown) so one bad folder doesn't fail the whole query —
 * only a `scanFolder` failure itself surfaces as the query's error.
 */
export async function scanAndClassify(
  rootId: string,
  rootAbsolutePath: string,
): Promise<ScanResult> {
  const scanResponse = unwrap(
    await commands.inboxScanFolder({ rootId, rootAbsolutePath }),
  );
  const items = scanResponse.items ?? [];

  const classifications = new Map<string, InboxClassifyResponse>();
  const classifyFailures = new Set<string>();
  await Promise.allSettled(
    items.map(async (item) => {
      try {
        const cls = unwrap(
          await commands.inboxClassify({
            inboxItemId: item.inboxItemId,
            rootAbsolutePath,
          }),
        );
        classifications.set(item.inboxItemId, cls);
      } catch {
        // Don't abort the whole scan for one item — but do record it.
        // Swallowing it entirely made a crash look like a clean "nothing
        // detected" result.
        classifyFailures.add(item.inboxItemId);
      }
    }),
  );
  return { items, classifications, classifyFailures };
}
