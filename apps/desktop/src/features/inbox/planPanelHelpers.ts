// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Pure helpers for `PlanPanel`'s plan-summary rendering: frame-type
 * inference from action paths, destination shortening, and the collapsed
 * summary/breakdown line builders. No React — colocated `.ts` per the
 * `inboxDetailHelpers.ts` / `inboxStatsFromItems.ts` convention in this
 * directory.
 */

import type { InboxPlanAction } from './store';
import { m } from '@/lib/i18n';
import { basename } from '@/lib/path';
export { basename };

function actionLabel(kind: string): string {
  switch (kind) {
    case 'move':
      return m.inbox_action_move();
    case 'catalogue':
      return m.inbox_action_catalogue();
    case 'archive':
      return m.inbox_action_archive();
    case 'trash':
      return m.inbox_action_trash();
    default:
      return kind;
  }
}

/**
 * Frame-type keywords we look for in a destination path to label a summary
 * line. `InboxPlanAction` carries no frame type, so we infer it from the path
 * the file is headed to (e.g. `masters/darks/…`, `IC1396/Ha/…` → no frame
 * keyword, falls back to the action kind). Keys are matched case-insensitively
 * against each path segment; the value is the singular label used for a count
 * of one (pluralised with a trailing "s").
 */
const FRAME_TYPE_KEYWORDS: Array<[token: string, singular: string]> = [
  ['lights', 'light'],
  ['light', 'light'],
  ['darkflats', 'dark flat'],
  ['dark_flats', 'dark flat'],
  ['darks', 'dark'],
  ['dark', 'dark'],
  ['flats', 'flat'],
  ['flat', 'flat'],
  ['biases', 'bias'],
  ['bias', 'bias'],
  ['masters', 'master'],
  ['master', 'master'],
];

/**
 * Normalise a raw frame-type hint (from the inbox item's classification /
 * breakdown — e.g. `groupFrameType` / `masterFrameType`) into the singular
 * label vocabulary used on the summary lines. Unknown values pass through
 * lower-cased so they still aggregate sensibly.
 */
function normalizeFrameTypeHint(raw: string): string {
  const v = raw.trim().toLowerCase();
  switch (v) {
    case 'lights':
      return 'light';
    case 'darks':
      return 'dark';
    case 'flats':
      return 'flat';
    case 'biases':
      return 'bias';
    case 'dark_flat':
    case 'darkflat':
    case 'dark_flats':
    case 'darkflats':
      return 'dark flat';
    case 'masters':
      return 'master';
    default:
      return v;
  }
}

/** Normalise a path's separators and split into lowercase segments. */
function pathSegments(path: string): string[] {
  return path
    .replace(/\\/g, '/')
    .split('/')
    .filter(Boolean)
    .map((s) => s.toLowerCase());
}

/** Directory portion of a destination (drops the trailing file name). */
function destinationDir(path: string): string {
  const norm = path.replace(/\\/g, '/');
  const idx = norm.lastIndexOf('/');
  return idx > 0 ? norm.slice(0, idx) : norm;
}

/**
 * Shorten a long destination to a readable tail — keep the last two path
 * segments so the summary shows where files land without overflowing the row.
 */
function shortDestination(path: string): string {
  const norm = path.replace(/\\/g, '/');
  const parts = norm.split('/').filter(Boolean);
  if (parts.length <= 2) return norm;
  return `…/${parts.slice(-2).join('/')}`;
}

/**
 * Resolve the frame-type label for one action (spec 043 #75). Priority:
 *   1. `action.frameType` from the backend (set at plan creation since kyo7.85),
 *   2. a frame keyword present in the destination OR source path (handles split
 *      plans where each type lands in its own typed folder — legacy fallback),
 *   3. the per-ingestion hint from the inbox item's classification/breakdown
 *      (`itemFrameType` — e.g. a single-type catalogue folder of darks whose
 *      in-place destination carries no frame keyword), then
 *   4. the action kind (e.g. "catalogue") as a last resort.
 */
export function frameTypeLabel(
  action: InboxPlanAction,
  destination: string,
  itemFrameType?: string,
): string {
  // Prefer the backend-supplied frame type (no path inference needed).
  if (action.frameType) return normalizeFrameTypeHint(action.frameType);

  // Legacy path: infer from destination/source path keywords (pre-kyo7.85 plans).
  const segments = [
    ...pathSegments(destination),
    ...pathSegments(action.fromPath),
  ];
  for (const [token, singular] of FRAME_TYPE_KEYWORDS) {
    if (segments.includes(token)) return singular;
  }
  // Match a frame keyword INSIDE the file name (e.g. "bias_001.fits",
  // "master_dark_300s.fits") so per-file rows of an in-place mixed folder get
  // their real type instead of the folder's dominant hint.
  const base = basename(action.fromPath).toLowerCase();
  for (const [token, singular] of FRAME_TYPE_KEYWORDS) {
    if (base.includes(token)) return singular;
  }
  if (itemFrameType) return normalizeFrameTypeHint(itemFrameType);
  return actionLabel(action.action).toLowerCase();
}

/** Pluralise a singular frame/action label for a count. */
export function pluralLabel(singular: string, count: number): string {
  if (count === 1) return singular;
  return singular.endsWith('s') ? singular : `${singular}s`;
}

/**
 * Localized "{count} <frame type>" count-variant thunks for the KNOWN canonical
 * frame-type bucket keys (see `FRAME_TYPE_KEYWORDS`/`normalizeFrameTypeHint`:
 * light/dark/flat/bias/dark flat/master). Render-time thunks (spec 046 #8) so
 * they re-read the active locale on each call rather than resolving once at
 * module scope.
 */
const FRAMETYPE_COUNT_LABEL_FNS: Record<string, (count: number) => string> = {
  light: (count) => m.inbox_frametype_count_light({ count }),
  dark: (count) => m.inbox_frametype_count_dark({ count }),
  flat: (count) => m.inbox_frametype_count_flat({ count }),
  bias: (count) => m.inbox_frametype_count_bias({ count }),
  'dark flat': (count) => m.inbox_frametype_count_dark_flat({ count }),
  master: (count) => m.inbox_frametype_count_master({ count }),
};

/**
 * Localized "{count} <frame type>" label for a plan summary bucket. Returns
 * the count-variant message for a KNOWN canonical frame-type key; returns
 * `null` for unknown values (e.g. the `actionLabel(...).toLowerCase()`
 * fallback for an unrecognised frame type) so the caller falls back to the
 * existing `pluralLabel` behaviour, which is already localized at its source.
 */
export function frameTypeCountLabel(
  frameType: string,
  count: number,
): string | null {
  return FRAMETYPE_COUNT_LABEL_FNS[frameType]?.(count) ?? null;
}

/** One collapsed summary line: "N <frametype> → <destination tail>". */
export interface PlanGroupSummaryLine {
  /** Stable key for the row. */
  key: string;
  count: number;
  /** Singular frame/action label (e.g. "light", "dark", "catalogue"). */
  frameType: string;
  /** Shortened destination directory shown after the arrow. */
  destinationShort: string;
  /** Full destination directory for the title/tooltip. */
  destinationFull: string;
}

/**
 * Group a plan's actions by (frame type → destination directory) and produce
 * one summary line per bucket, sorted by frame type then destination. The
 * destination comes from the captured absolute path when present, else the
 * action's relative preview (mirrors the per-row fallback).
 */
export function buildGroupSummary(
  actions: InboxPlanAction[],
  absoluteByFromPath?: Record<string, string>,
  itemFrameType?: string,
): PlanGroupSummaryLine[] {
  const buckets = new Map<
    string,
    { count: number; frameType: string; destinationFull: string }
  >();
  for (const a of actions) {
    const dest = absoluteByFromPath?.[a.fromPath] ?? a.destinationPreview;
    const frameType = frameTypeLabel(a, dest, itemFrameType);
    const destDir = destinationDir(dest);
    const key = `${frameType}${destDir}`;
    const existing = buckets.get(key);
    if (existing) existing.count += 1;
    else buckets.set(key, { count: 1, frameType, destinationFull: destDir });
  }
  return [...buckets.entries()]
    .sort(([ka], [kb]) => ka.localeCompare(kb))
    .map(([key, b]) => ({
      key,
      count: b.count,
      // Kept SINGULAR (not pluralised here) so the JSX consumer can key off
      // the canonical value to pick a localized count-variant message for a
      // known frame type, falling back to `pluralLabel` for unknown ones.
      frameType: b.frameType,
      destinationShort: shortDestination(b.destinationFull),
      destinationFull: b.destinationFull,
    }));
}

/**
 * Derive a per-type frame breakdown from a plan's ACTIONS, by classifying each
 * action with {@link frameTypeLabel} (frame keyword in the destination/source
 * path, then the per-ingestion `itemFrameType` hint). Returns `[{ kind, count }]`
 * buckets — the same shape the inbox item's classification breakdown carries —
 * so a MOVE/SPLIT plan whose files land in typed folders yields a true
 * multi-type tally (e.g. `light` + `dark`). A single-type catalogue ingestion
 * folds onto its hint; a mixed in-place catalogue (no keyword, no per-file
 * type) can only report the hint, which is the irreducible backend-data limit.
 *
 * InboxPage merges this with the selected item's real classification breakdown
 * (which DOES resolve a mixed catalogue) when building `breakdownByItemId`.
 */
export function buildBreakdownFromActions(
  actions: InboxPlanAction[],
  itemFrameType?: string,
  absoluteByFromPath?: Record<string, string>,
): Array<{ kind: string; count: number }> {
  const buckets = new Map<string, number>();
  for (const a of actions) {
    const dest = absoluteByFromPath?.[a.fromPath] ?? a.destinationPreview;
    const frameType = frameTypeLabel(a, dest, itemFrameType);
    buckets.set(frameType, (buckets.get(frameType) ?? 0) + 1);
  }
  return [...buckets.entries()].map(([kind, count]) => ({ kind, count }));
}

/** One aggregated frame-type entry for the per-type breakdown summary line. */
export interface PlanGroupBreakdownEntry {
  /** Stable key for the entry. */
  key: string;
  count: number;
  /** Singular frame-type label (e.g. "bias", "dark", "light", "master"). */
  frameType: string;
}

/**
 * Build the per-type breakdown for one group from the ingestion's frame-type
 * tally (the SAME shape `InboxStatsSummary` derives from the inbox item — see
 * `buildBreakdownByItemId` in InboxPage). Each `{ kind, count }` becomes one
 * aggregated entry, normalised to the singular label vocabulary and merged so a
 * folder reporting e.g. both `lights` and `light` collapses to one bucket.
 * Sorted by frame type for a stable, readable order. Returns `[]` when there is
 * no usable breakdown, signalling the caller to fall back to the per-action
 * keyword/hint summary.
 */
export function buildGroupBreakdown(
  breakdown: ReadonlyArray<{ kind: string; count: number }> | undefined,
): PlanGroupBreakdownEntry[] {
  if (!breakdown || breakdown.length === 0) return [];
  const buckets = new Map<string, number>();
  for (const { kind, count } of breakdown) {
    if (!kind || count <= 0) continue;
    const frameType = normalizeFrameTypeHint(kind);
    buckets.set(frameType, (buckets.get(frameType) ?? 0) + count);
  }
  return [...buckets.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([frameType, count]) => ({ key: frameType, count, frameType }));
}

/**
 * Resolve the single destination shown after the breakdown summary's arrow.
 * For an in-place catalogue ingestion this is the ingestion folder (`(root)`
 * when empty); otherwise the common destination directory tail of the actions.
 */
export function breakdownDestination(
  actions: InboxPlanAction[],
  itemName: string,
  absoluteByFromPath?: Record<string, string>,
): { short: string; full: string } {
  const dirs = new Set<string>();
  for (const a of actions) {
    const dest = absoluteByFromPath?.[a.fromPath] ?? a.destinationPreview;
    dirs.add(destinationDir(dest));
  }
  if (dirs.size === 1) {
    const full = [...dirs][0] || itemName || '(root)';
    return { short: shortDestination(full), full };
  }
  // Files land in several typed folders — name the ingestion instead.
  const full = itemName || '(root)';
  return { short: full, full };
}
