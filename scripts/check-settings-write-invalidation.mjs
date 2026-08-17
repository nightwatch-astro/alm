#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Settings-write invalidation gate (astro-plan-cia8).
 *
 * `settingsQueryOptions` caches each settings scope with `staleTime: Infinity`,
 * which is only safe because the writer invalidates the key afterwards. That
 * invalidation lives at the two chokepoints in
 * `apps/desktop/src/features/settings/settingsIpc.ts` — `updateSettings` and
 * `settingsRestoreDefaults` — so a caller that goes through them cannot forget.
 *
 * The risk is the caller that does NOT go through them: several modules invoke
 * `commands.settingsUpdate` directly. Today none of their scopes has a
 * `useQuery` reader, so nothing goes stale — the bug is latent, not live. It
 * becomes real the moment someone adds a reader for one of those scopes, and at
 * that point the failure is a pane showing a value the user just changed, with
 * nothing in the diff to suggest why.
 *
 * A comment cannot hold that line. This gate does: the allowlist below is the
 * set of modules permitted to bypass the chokepoint, and a new bypass fails
 * until it is either routed through `updateSettings` or added here deliberately.
 *
 * Deliberately a baseline rather than a prohibition. The existing bypasses have
 * reasons — theme and locale write during boot, before a QueryClient
 * necessarily exists — and rewriting them is a separate change from stopping
 * the set from growing.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Anchored to this file, not to cwd: the gate runs both from the repo root and
// from apps/desktop (via that package's `lint` chain), and a cwd-relative path
// crashes with ENOENT in one of the two.
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(REPO_ROOT, 'apps/desktop/src');

/** The single sanctioned writer. Bypasses are measured against this. */
const CHOKEPOINT = 'features/settings/settingsIpc.ts';

/**
 * Modules allowed to call `commands.settingsUpdate` directly.
 *
 * Each entry is a module that writes a settings scope no `useQuery` currently
 * reads. If you add one, say which scope it writes and why it cannot use
 * `updateSettings` — "it runs before the QueryClient exists" is a real reason;
 * "it was easier" is not.
 */
const ALLOWED_BYPASS = new Set([
  // Scope `general`. Writes during boot, before the QueryClient is mounted.
  'data/theme.ts',
  'data/locale.tsx',
  // Scope `ui_state`. Write-behind per constitution V; the in-memory value is
  // authoritative, so a stale cache read is not the failure mode here.
  'data/persisted-state.ts',
  // Scope `observing`.
  'shared/observing-sites/site-store.ts',
  // Scopes `planner` and `catalogues`.
  'shared/planner/guidance-settings.ts',
  'shared/planner/catalogue-settings.ts',
]);

/** Every `.ts`/`.tsx` file under a directory, excluding tests. */
function sourceFiles(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) {
      out.push(...sourceFiles(full));
      continue;
    }
    if (!/\.tsx?$/.test(name)) continue;
    if (/\.test\.|\.spec\.|__tests__/.test(full)) continue;
    out.push(full);
  }
  return out;
}

/** Modules calling `commands.settingsUpdate`, as SRC-relative paths. */
function directWriters(root = SRC) {
  return sourceFiles(root)
    .filter((f) => /commands\.settingsUpdate\s*\(/.test(readFileSync(f, 'utf8')))
    .map((f) => relative(root, f))
    .sort();
}

function main() {
  const writers = directWriters();

  // A gate that finds nothing is indistinguishable from a clean repo, and this
  // one depends on a path and a call spelling that could both change. Refuse to
  // pass vacuously: the chokepoint itself must always be found.
  if (!writers.includes(CHOKEPOINT)) {
    console.error(
      `settings-write gate FAILED: expected ${CHOKEPOINT} to call ` +
        'commands.settingsUpdate, and it does not.\n' +
        'Either the chokepoint moved or the call spelling changed — this gate ' +
        'is not measuring anything until that is fixed.',
    );
    process.exitCode = 1;
    return;
  }

  const unexpected = writers.filter(
    (f) => f !== CHOKEPOINT && !ALLOWED_BYPASS.has(f),
  );
  const stale = [...ALLOWED_BYPASS].filter((f) => !writers.includes(f)).sort();

  if (stale.length > 0) {
    console.warn(
      'NOTE: these no longer call commands.settingsUpdate; drop them from ' +
        `ALLOWED_BYPASS in ${relative('.', 'scripts/check-settings-write-invalidation.mjs')}:`,
    );
    for (const f of stale) console.warn(`  ${f}`);
  }

  if (unexpected.length === 0) {
    console.log(
      `OK: settings writes go through ${CHOKEPOINT} ` +
        `(${ALLOWED_BYPASS.size} baselined bypasses).`,
    );
    return;
  }

  console.error(
    'settings-write gate FAILED: these call commands.settingsUpdate directly, ' +
      'bypassing the invalidation chokepoint:',
  );
  for (const f of unexpected) console.error(`  ${f}`);
  console.error(
    `\n\`settingsQueryOptions\` caches with \`staleTime: Infinity\`, so a write` +
      '\nthat skips `updateSettings` leaves any cached reader showing the old' +
      '\nvalue forever — and the symptom is a pane ignoring a change the user' +
      '\njust made.' +
      '\n\nPrefer routing the write through `updateSettings` in' +
      `\n${CHOKEPOINT}. If it genuinely cannot (it runs before the QueryClient` +
      '\nexists, say), add it to ALLOWED_BYPASS with the scope it writes and' +
      '\nwhy.',
  );
  process.exitCode = 1;
}

// Guarded so the co-located test can import the parser without running the
// gate. `import.meta.main` is NOT usable: it needs Node >=22.18/24.2 and CI
// pins node 20, where it is undefined — main() would silently never run and
// this gate would no-op green. Same note as check-mock-baseline.mjs.
if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}

export { ALLOWED_BYPASS, CHOKEPOINT, directWriters };
