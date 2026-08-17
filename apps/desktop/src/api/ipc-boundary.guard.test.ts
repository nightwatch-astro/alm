// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Spec 037 IPC-boundary teardown guard (SC-006).
 *
 * SC-001 and SC-005 are Biome GritQL plugins instead — single-file literal
 * patterns need no module graph:
 * `apps/desktop/biome-plugins/no-raw-invoke.grit` and
 * `apps/desktop/biome-plugins/no-retired-commands-wrapper.grit`.
 *
 * This scans the whole `src` tree so the drift-bug class the migration removed
 * cannot silently return:
 *
 * - SC-006: no new file may call a `settingsIpc` wrapper function directly
 *   inside a `useEffect` (or a `useCallback` invoked from one). The correct
 *   pattern is React Query (`useQuery`/`useMutation`). Existing sites are
 *   allowlisted and expected to drain as they are migrated (GF-24 / C-27).
 *
 *   Detection: a file is an offender when it (a) imports from the `settingsIpc`
 *   module AND (b) a `useEffect(` block in its source calls one of the async
 *   IPC fetch functions exported by that module. The allowlist below shrinks
 *   to [] as files migrate to React Query; removing a file from the list is
 *   the only allowed change.
 *
 *   A vitest guard is used here rather than an ESLint rule because (1) the
 *   detection crosses two language features (import graph + useEffect body
 *   inspection) that ESLint cannot scope to "within a useEffect callback" without
 *   a custom rule, and (2) the allowlist-drain pattern is already established by
 *   the ESLint baseline mechanism — this mirrors it for the fetch-in-effect class.
 */

import { describe, it, expect } from 'vitest';

// Vite reads every source file at build time (`?raw`, eager) so the scan needs
// no Node fs/path APIs and works under the desktop tsconfig.
const raw = import.meta.glob('/src/**/*.{ts,tsx}', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** Strip block + line comments so prose mentioning `invoke('…')` doesn't trip
 *  the scanners. (`[^:]` guards the `://` in URLs from the line-comment rule.) */
function stripComments(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[^:])\/\/.*$/gm, '$1');
}

const files = Object.entries(raw)
  .filter(
    ([path]) =>
      !path.includes('/paraglide/') &&
      !path.endsWith('/api/ipc-boundary.guard.test.ts') &&
      // vanilla-extract CSS-in-TS files; ?raw import returns undefined when the
      // VE plugin intercepts them — and they can never be IPC boundary offenders.
      !path.endsWith('.css.ts'),
  )
  .map(([path, src]) => ({ path, src: stripComments(src) }));

describe('spec 037 — IPC boundary guards', () => {
  it('has files to scan', () => {
    expect(files.length).toBeGreaterThan(100);
  });

  it('SC-006: no new file calls a settingsIpc wrapper inside useEffect (migrate to React Query)', () => {
    // Detection (two-step):
    //   Step 1 — file imports from the settingsIpc wrapper module.
    //   Step 2 — a useEffect( block in the file calls one of the async IPC
    //            fetch functions from that module.
    //
    // "Calls inside useEffect" is approximated by: the source contains both
    // `useEffect(` and a call to any of the known IPC fetch functions. This
    // cannot be fooled by imports alone because the detected functions are
    // async IPC calls, not React hooks or sync utilities. The comment-stripped
    // source (shared `files` array) prevents prose mentions from matching.
    //
    // Allowlist — existing sites to drain by migrating to useQuery/useMutation.
    // Remove entries as migrations land; adding new entries fails the review.
    const ALLOWLIST = new Set([
      '/src/dev/DevSettingsPage.tsx',
      '/src/features/projects/GenerateSourceViewDialog.tsx',
      '/src/features/settings/Advanced.tsx',
      '/src/features/settings/AuditLog.tsx',
      '/src/features/settings/CalibrationMatching.tsx',
      '/src/features/settings/Cleanup.tsx',
      '/src/features/settings/Framing.tsx',
      '/src/features/settings/Ingestion.tsx',
      '/src/features/settings/PerTypeDestinationPatterns.tsx',
      '/src/features/settings/ProcessingTools.tsx',
      '/src/features/settings/RemapRootDialog.tsx',
      '/src/features/settings/ResolverSettingsControl.tsx',
      '/src/features/settings/SourceProtectionOverride.tsx',
      '/src/features/settings/SourceViews.tsx',
      '/src/features/settings/useEquipment.ts',
      '/src/features/settings/useNamingPattern.ts',
      '/src/features/targets/planner-sensor.ts',
    ]);

    // Pattern: imports the settingsIpc module (path alias or relative).
    const importsSettingsIpc =
      /from\s*['"](@\/features\/settings\/settingsIpc|\.\/settingsIpc)['"]/;

    // Pattern: calls one of the known IPC fetch functions (async, data-loading).
    // These are all the async functions exported from settingsIpc that are
    // called inside useEffect or useCallback-from-effect across the codebase.
    const ipcFetchCall =
      /\b(getSettings|sourceProtectionGet|equipmentCamerasList|equipmentTelescopesList|equipmentTrainsList|equipmentFiltersList|auditList|toolProfileList|calibrationTolerancesGet|cleanupPolicyGet|ingestionSettingsGet|getResolverSettings|listRoots)\s*\(/;

    // A file is an offender when it imports settingsIpc AND a useEffect block
    // in its source calls one of those functions.
    const offenders = files
      .filter(
        (f) =>
          importsSettingsIpc.test(f.src) &&
          f.src.includes('useEffect(') &&
          ipcFetchCall.test(f.src),
      )
      .map((f) => f.path)
      .filter((p) => !ALLOWLIST.has(p));

    expect(offenders).toEqual([]);
  });
});
