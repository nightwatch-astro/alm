// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { z } from 'zod';
import { registerRootBatch } from './registerSources';
import { readLocalStorage, writeLocalStorage } from '@/lib/local-storage';
import { m } from '@/lib/i18n';
import { errMessage } from '@/lib/errors';

const STORAGE_KEY = 'alm-setup-wizard-state';

// Per-image frame type (light/dark/flat/bias) is detected from image metadata
// (FITS IMAGETYP header) during scan/ingest — NOT inferred from which source
// folder the file is in. 'calibration' here is only a user-facing folder
// category that covers darks, flats, and bias frames together.
export const ALL_SOURCE_KINDS = [
  'light_frames',
  'calibration',
  'project',
  'inbox',
] as const;

export type SourceKind = (typeof ALL_SOURCE_KINDS)[number];
export type OrganizationState = 'organized' | 'unorganized';

// Values are render-time thunks so labels re-read the active locale (spec 046 #8).
export const SOURCE_KIND_LABELS: Record<SourceKind, () => string> = {
  light_frames: () => m.setup_kind_light_frames(),
  calibration: () => m.setup_kind_calibration(),
  project: () => m.common_projects(),
  inbox: () => m.settings_datasources_category_inbox(),
};

export function sourceKindLabel(kind: string): string {
  if (Object.hasOwn(SOURCE_KIND_LABELS, kind)) {
    return SOURCE_KIND_LABELS[kind as SourceKind]();
  }
  return m.setup_scan_source_unknown({
    value: kind.trim() || m.common_unknown(),
  });
}

// spec 039: inbox is now optional — users do not need a dedicated drop folder
// to use the Inbox (which aggregates unacknowledged items across all roots).
export const REQUIRED_KINDS: SourceKind[] = ['light_frames', 'project'];

export interface SourceEntry {
  path: string;
  kind: SourceKind;
  /** Organization state for this source (spec 041 R-7).
   *  Inbox kind is always 'unorganized'. Non-inbox defaults to 'organized' (local-first safe default).
   */
  organizationState: OrganizationState;
}

/** Flat array of source entries — replaces the old per-kind object shape. */
export type SourcesState = SourceEntry[];

export interface DeduplicationResult {
  ok: boolean;
  /** Warning: same path already exists within this kind. */
  sameKindDuplicate?: boolean;
  /** Error: same path registered under a different kind. */
  crossKindConflict?: SourceKind;
}

export interface FlushRowResult {
  kind: SourceKind;
  path: string;
  success: boolean;
  /** The registered root's id (present on successful rows); used to scan the source. */
  rootId?: string;
  /**
   * True when the row "failed" only because the path was already registered
   * (issue #704). The restart flow pre-fills already-registered folders, so
   * re-confirming them is the desired end state, not a user-facing failure —
   * the wizard treats these as benign no-ops rather than blocking failures.
   */
  alreadyRegistered?: boolean;
  error?: string;
}

export interface FlushResult {
  results: FlushRowResult[];
  allSucceeded: boolean;
}

export interface ValidationError {
  code: string;
  message: string;
}

/**
 * One persisted source entry. `organizationState` is optional because entries
 * written before the field existed still load; `loadSources` supplies it.
 */
const persistedEntrySchema = z.object({
  path: z.string(),
  kind: z.enum(ALL_SOURCE_KINDS),
  organizationState: z.enum(['organized', 'unorganized']).optional(),
});

/**
 * The wizard-state envelope, read for its `sources` key alone. Elements stay
 * `unknown` here and are validated one at a time in `loadSources`: a single
 * malformed entry must not discard the user's other registered folders.
 */
const persistedSourcesSchema = z
  .object({ sources: z.array(z.unknown()) })
  .partial();

/** Load sources state from localStorage. */
export function loadSources(): SourcesState {
  const stored = readLocalStorage(STORAGE_KEY, persistedSourcesSchema, {});
  return (stored.sources ?? []).flatMap((raw) => {
    const entry = persistedEntrySchema.safeParse(raw);
    if (!entry.success) return [];
    return [
      {
        path: entry.data.path,
        kind: entry.data.kind,
        // spec 041 R-7: inbox is always unorganized regardless of what was
        // persisted; other kinds default to the local-first-safe 'organized'.
        organizationState:
          entry.data.kind === 'inbox'
            ? 'unorganized'
            : (entry.data.organizationState ?? 'organized'),
      },
    ];
  });
}

/** Persist sources state to localStorage under the wizard state key. */
export function saveSources(sources: SourcesState): void {
  // Preserves the sibling wizard-state keys this module does not own.
  const existing = readLocalStorage(STORAGE_KEY, z.looseObject({}), {});
  writeLocalStorage(STORAGE_KEY, { ...existing, sources });
}

/**
 * Replace the entire persisted wizard state with a fresh run prefilled with
 * `sources` (spec 003 US3 restart flow, A7).
 *
 * Used after a successful `firstrun.restart` call so the wizard opens at
 * step 0 with the previously registered sources loaded into the working
 * buffer for editing, rather than resuming a stale in-progress run.
 * Intentionally overwrites (not merges) `currentStep`/`catalogSettings`/
 * `tools` — omitting them here makes `SetupWizard`'s `loadWizardState` fall
 * back to its own defaults for those fields.
 */
export function resetWizardStateWithSources(sources: SourcesState): void {
  // Storage full / unavailable is best-effort: the wizard then starts from
  // loadSources()'s empty-array fallback instead of the prefilled sources.
  writeLocalStorage(STORAGE_KEY, { sources });
}

/** Check whether adding a path to a kind would create duplicates. */
export function checkDeduplication(
  sources: SourcesState,
  kind: SourceKind,
  path: string,
): DeduplicationResult {
  const normalizedPath = path.toLowerCase();

  const sameKindDuplicate = sources.some(
    (entry) =>
      entry.kind === kind && entry.path.toLowerCase() === normalizedPath,
  );

  let crossKindConflict: SourceKind | undefined;
  for (const entry of sources) {
    if (entry.kind !== kind && entry.path.toLowerCase() === normalizedPath) {
      crossKindConflict = entry.kind;
      break;
    }
  }

  return {
    ok: !sameKindDuplicate && !crossKindConflict,
    sameKindDuplicate: sameKindDuplicate || undefined,
    crossKindConflict,
  };
}

/** Add a source entry, returning updated state.
 *  Inbox kind is always forced to 'unorganized'; non-inbox defaults to 'organized'.
 */
export function addSource(
  sources: SourcesState,
  kind: SourceKind,
  path: string,
  organizationState?: OrganizationState,
): SourcesState {
  const state: OrganizationState =
    kind === 'inbox' ? 'unorganized' : (organizationState ?? 'organized');
  return [...sources, { path, kind, organizationState: state }];
}

/** Remove a source entry by index, returning updated state. */
export function removeSource(
  sources: SourcesState,
  _kind: SourceKind,
  index: number,
): SourcesState {
  return sources.filter((_, i) => i !== index);
}

/** Get sources for a specific kind. */
export function getSourcesByKind(
  sources: SourcesState,
  kind: SourceKind,
): SourceEntry[] {
  return sources.filter((e) => e.kind === kind);
}

/** Check which required kinds are missing from the current sources. */
export function getMissingRequiredKinds(sources: SourcesState): SourceKind[] {
  return REQUIRED_KINDS.filter((kind) => !sources.some((e) => e.kind === kind));
}

/**
 * Validate a single path client-side. Does NOT register the source —
 * registration only happens at flush time via flushToDB().
 * Returns null on success, or a ValidationError on failure.
 */
export function validatePath(
  sources: SourcesState,
  path: string,
  kind: SourceKind,
): ValidationError | null {
  if (!path?.trim()) {
    return { code: 'path.empty', message: m.setup_validate_path_empty() };
  }

  const dedup = checkDeduplication(sources, kind, path);
  if (dedup.sameKindDuplicate) {
    return {
      code: 'path.already_registered',
      message: m.err_path_already_registered(),
    };
  }
  if (dedup.crossKindConflict) {
    return {
      code: 'path.already_registered.different_kind',
      message: `${m.err_path_already_registered_different_kind()} (${dedup.crossKindConflict})`,
    };
  }

  return null;
}

/**
 * Flush all sources to the database via roots.register.batch.
 * Returns per-row success/failure results.
 */
export async function flushToDB(sources: SourcesState): Promise<FlushResult> {
  const isMockMode = import.meta.env.VITE_USE_MOCKS === 'true';
  const validSources = sources.filter((s) => s.path);

  if (isMockMode) {
    return {
      results: validSources.map((s) => ({
        kind: s.kind,
        path: s.path,
        success: true,
        rootId: s.path,
      })),
      allSucceeded: true,
    };
  }

  try {
    const batchResult = await registerRootBatch({
      sources: validSources.map((s) => ({
        kind: s.kind,
        path: s.path,
        // scanDepth is a required backend contract field (camelCase) with no
        // frontend-configurable state left — 'single' was never implemented,
        // every scan is recursive (#509), and the UI plumbing was retired
        // end-to-end (#913). Hardcode the only value the backend ever acts on.
        scanDepth: 'recursive',
        // organizationState is required by the backend contract (spec 041 R-7).
        // Inbox is always unorganized; non-inbox carries the user's explicit choice.
        organizationState:
          s.kind === 'inbox'
            ? 'unorganized'
            : (s.organizationState ?? 'organized'),
      })),
    });

    const results: FlushRowResult[] = batchResult.results.map((item) => {
      if (item.success) {
        // Carry the assigned root id so the wizard scan step can scan this source.
        return {
          kind: item.kind as SourceKind,
          path: item.path,
          success: true,
          rootId: item.rootId,
        };
      }
      // Resolve through the single translation point so the user sees a friendly
      // catalog message, never the raw backend code (spec 046 FR-008/FR-009).
      const message = errMessage({
        code: item.error ?? 'unknown',
        message: '',
      });
      return {
        kind: item.kind as SourceKind,
        path: item.path,
        success: false,
        // Issue #704: an exact-path duplicate is not a genuine failure in the
        // restart flow — flag it so the wizard advances instead of blocking.
        alreadyRegistered:
          item.error === 'path.already_registered' || undefined,
        error: message,
      };
    });

    return { results, allSucceeded: results.every((r) => r.success) };
  } catch (err: unknown) {
    return {
      results: validSources.map((s) => ({
        kind: s.kind,
        path: s.path,
        success: false,
        error: m.setup_sources_error_batch_registration_failed({
          message: errMessage(err),
        }),
      })),
      allSucceeded: false,
    };
  }
}
