// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Pure presentation helpers for the Inbox detail pane.
 *
 * Extracted from `InboxDetail.tsx` (#994) — no React, no IPC, so each one is
 * unit-testable on its own and the detail component stays about composition.
 */

import type { PillVariant } from '@/ui';
// Canonical path/format utilities used locally and re-exported so existing
// callers of this module keep working without changes. resolveRevealPath
// replaces the former resolveInboxRevealPath; basename, parentSegment, and
// formatExposureSeconds move to lib/ as the single source of truth.
import { resolveRevealPath, basename, parentSegment } from '@/lib/path';
import { formatExposureSeconds } from '@/lib/format';
export { resolveRevealPath as resolveInboxRevealPath, basename, parentSegment };
export { formatExposureSeconds };

/** "exposureS" → "exposure S" (best-effort label for a registry key with no i18n entry). */
export function humanizeKey(key: string): string {
  const spaced = key.replace(/([a-z0-9])([A-Z])/g, '$1 $2');
  return spaced.charAt(0).toUpperCase() + spaced.slice(1).toLowerCase();
}

export function classificationVariant(type: string): PillVariant {
  switch (type) {
    case 'single_type':
      return 'info';
    case 'mixed':
      return 'warn';
    case 'unclassified':
      return 'neutral';
    default:
      return 'neutral';
  }
}

export const FRAME_TYPE_OPTIONS = [
  'light',
  'dark',
  'bias',
  'flat',
  'dark_flat',
] as const;

/**
 * Applicable destination-root category for a frame type (point 1: only show
 * libraries that can actually receive this image type). Light frames go to a
 * "raw" root; calibration frames (bias/dark/flat) + their masters go to a
 * "calibration" root. Returns null when we can't narrow (e.g. mixed) — then all
 * roots are shown. NOTE: this is a pragmatic frontend mapping; the spec-045
 * iterate (single-type sub-items) will make this authoritative per item.
 */
export function applicableRootCategory(
  frameType?: string | null,
): string | null {
  if (!frameType) return null;
  const ft = frameType.toLowerCase();
  if (ft.includes('light')) return 'raw';
  if (ft.includes('bias') || ft.includes('dark') || ft.includes('flat'))
    return 'calibration';
  return null;
}

/**
 * Build destination-root option labels, disambiguating roots that share a
 * basename (issue #866): two registered roots at different locations but the
 * same folder name (e.g. two "Lights" folders) rendered identically as
 * "Lights · raw" with no way to tell which one a pick actually targets.
 * Duplicates get their parent directory appended; unique basenames are
 * unaffected.
 */
export function buildRootLabels(
  roots: Array<{ id: string; path: string; category: string }>,
): Map<string, string> {
  const counts = new Map<string, number>();
  for (const r of roots) {
    const base = basename(r.path);
    counts.set(base, (counts.get(base) ?? 0) + 1);
  }
  const labels = new Map<string, string>();
  for (const r of roots) {
    const base = basename(r.path);
    const parent = parentSegment(r.path);
    const disambiguated =
      (counts.get(base) ?? 0) > 1 && parent ? `${base} (${parent})` : base;
    labels.set(r.id, `${disambiguated} · ${r.category}`);
  }
  return labels;
}
