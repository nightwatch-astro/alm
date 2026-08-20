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
 * `commands.settingsUpdate` or `commands.settingsRestoreDefaults` directly. The
 * allowlist below is the set of modules permitted to do so, and a new bypass
 * fails until it is either routed through the chokepoint or added here
 * deliberately.
 *
 * A bypass is only latent while no `useQuery` reads the scope it writes, so this
 * gate checks that too: each allowlist entry declares the scopes its writes
 * affect, and a cached reader appearing for one of them fails. That is the event
 * that turns a baselined bypass into a pane showing a value the user just
 * changed, with nothing in the diff to suggest why.
 *
 * Deliberately a baseline rather than a prohibition. The existing bypasses have
 * reasons — theme and locale write during boot, before a QueryClient
 * necessarily exists — and rewriting them is a separate change from stopping the
 * set from growing.
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
 * The two cache-invalidating commands. `settingsRestoreDefaults` matters as much
 * as `settingsUpdate`: its wrapper invalidates `queryKeys.settings.all()`, so a
 * direct call leaves every cached scope stale rather than one.
 */
const WRITE_COMMANDS = ['settingsUpdate', 'settingsRestoreDefaults'];

/**
 * Modules allowed to call a settings-write command directly, and the scopes each
 * one's writes affect.
 *
 * `scopes` is what makes the bypass safe to baseline: it is checked against the
 * scopes `settingsQueryOptions` is called with, and a reader appearing for one of
 * them fails the gate. If you add an entry, say why it cannot use
 * `updateSettings` — "it runs before the QueryClient exists" is a real reason,
 * "it was easier" is not.
 */
const ALLOWED_BYPASS = new Map([
  // Writes during boot, before the QueryClient is mounted.
  ['data/theme.ts', { scopes: ['general'] }],
  ['data/locale.tsx', { scopes: ['general'] }],
  // Write-behind per constitution V; the in-memory value is authoritative, so a
  // stale cache read is not the failure mode here. The scope is a parameter, so
  // it cannot be derived from the call site — `ui_state` is what its callers pass.
  ['data/persisted-state.ts', { scopes: ['ui_state'], derivable: false }],
  ['shared/observing-sites/site-store.ts', { scopes: ['observing'] }],
  // Also the one direct `settingsRestoreDefaults` caller. Restore is key-scoped,
  // and the keys it restores belong to `planner`.
  ['shared/planner/guidance-settings.ts', { scopes: ['planner'] }],
  ['shared/planner/catalogue-settings.ts', { scopes: ['catalogues'] }],
]);

/** Windows `path.relative` returns `a\b`; every path in this file uses `/`. */
function toPosix(path) {
  return path.split('\\').join('/');
}

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
    if (/\.test\.|\.spec\.|__tests__/.test(toPosix(full))) continue;
    out.push(full);
  }
  return out;
}

/**
 * A call-site matcher for `commands.<command>(`.
 *
 * `\s*` around the property access is load-bearing: prettier breaks a chained
 * call as `commands\n  .settingsUpdate(...)`, which the fixed-token spelling
 * missed. `app/LogPanelContext.tsx` and `features/setup/steps/StepCatalogs.tsx`
 * were both written that way and both bypassed this gate unseen.
 */
function callPattern(command) {
  return new RegExp(String.raw`commands\s*\.\s*${command}\s*\(`, 'g');
}

/**
 * First argument of each `commands.<command>(` call in `source`, as written.
 *
 * Returns the raw argument text, which is a string literal or an identifier.
 */
function firstArguments(source, command) {
  const out = [];
  const pattern = new RegExp(
    String.raw`commands\s*\.\s*${command}\s*\(\s*([^,)\s]+)`,
    'g',
  );
  let match;
  while ((match = pattern.exec(source)) !== null) out.push(match[1]);
  return out;
}

/**
 * The string a scope argument denotes, or null when it cannot be resolved.
 *
 * Scopes are written as module-local constants at every call site here
 * (`SETTINGS_SCOPE`, `OBSERVING_SCOPE`, `FRAMING_SCOPE`), so a literal-only
 * reader would resolve nothing at all.
 */
function resolveScope(source, argument) {
  const literal = /^['"]([^'"]+)['"]$/.exec(argument);
  if (literal !== null) return literal[1];
  if (!/^[A-Za-z_$][\w$]*$/.test(argument)) return null;
  const declaration = new RegExp(
    String.raw`\b${argument}\s*(?::\s*[^=]+)?=\s*['"]([^'"]+)['"]`,
  ).exec(source);
  return declaration === null ? null : declaration[1];
}

/**
 * Modules calling a settings-write command directly.
 *
 * Returns a Map of SRC-relative path to `{ commands, scopes, unresolved }`,
 * where `scopes` holds the resolvable `settingsUpdate` scopes at that call site.
 */
function directWriters(root = SRC) {
  const out = new Map();
  for (const file of sourceFiles(root)) {
    const source = readFileSync(file, 'utf8');
    const found = WRITE_COMMANDS.filter((c) => callPattern(c).test(source));
    if (found.length === 0) continue;

    const scopes = new Set();
    let unresolved = false;
    for (const argument of firstArguments(source, 'settingsUpdate')) {
      const scope = resolveScope(source, argument);
      if (scope === null) unresolved = true;
      else scopes.add(scope);
    }
    out.set(toPosix(relative(root, file)), {
      commands: found,
      scopes: [...scopes].sort(),
      unresolved,
    });
  }
  return new Map([...out].sort(([a], [b]) => (a < b ? -1 : 1)));
}

/**
 * Scopes some component reads through `settingsQueryOptions`, with the module
 * that reads each.
 *
 * The definition in `settingsQueries.ts` is skipped: its own `scope` parameter is
 * not a reader.
 */
function cachedReaderScopes(root = SRC) {
  const out = new Map();
  for (const file of sourceFiles(root)) {
    const rel = toPosix(relative(root, file));
    if (rel === 'features/settings/settingsQueries.ts') continue;
    const source = readFileSync(file, 'utf8');
    const pattern = /settingsQueryOptions\s*\(\s*([^,)\s]+)/g;
    let match;
    while ((match = pattern.exec(source)) !== null) {
      const scope = resolveScope(source, match[1]);
      if (scope === null) continue;
      if (!out.has(scope)) out.set(scope, []);
      out.get(scope).push(rel);
    }
  }
  return out;
}

/**
 * Every problem with the current tree, as `{ kind, detail }` records.
 *
 * `allowlist` is a parameter so the self-test can drive each violation kind from
 * a fixture tree instead of the repo's own (passing) allowlist.
 */
function violations(
  writers = directWriters(),
  readers = cachedReaderScopes(),
  allowlist = ALLOWED_BYPASS,
) {
  const out = [];

  for (const [path, info] of writers) {
    if (path === CHOKEPOINT || allowlist.has(path)) continue;
    out.push({
      kind: 'bypass',
      detail: `${path} calls ${info.commands.map((c) => `commands.${c}`).join(' and ')} directly`,
    });
  }

  for (const [path, entry] of allowlist) {
    const info = writers.get(path);

    // A stale entry is an error, not a note: left in place it permanently
    // authorizes the path, so a direct write added back later passes unseen and
    // the set can regrow without the gate ever failing.
    if (info === undefined) {
      out.push({
        kind: 'stale',
        detail: `${path} no longer calls a settings-write command; drop it from ALLOWED_BYPASS`,
      });
      continue;
    }

    // The declared scopes are what the reader check is applied to, so a bypass
    // quietly changing scope would otherwise dodge it.
    if (entry.derivable !== false) {
      const undeclared = info.scopes.filter((s) => !entry.scopes.includes(s));
      if (undeclared.length > 0) {
        out.push({
          kind: 'scope-drift',
          detail: `${path} writes ${undeclared.join(', ')}, which ALLOWED_BYPASS does not declare`,
        });
      }
    }

    for (const scope of entry.scopes) {
      const readerModules = readers.get(scope);
      if (readerModules === undefined) continue;
      out.push({
        kind: 'cached-reader',
        detail: `scope \`${scope}\` is bypassed by ${path} and cached by ${readerModules.join(', ')}`,
      });
    }
  }

  return out;
}

function main() {
  const writers = directWriters();
  const readers = cachedReaderScopes();

  // A gate that finds nothing is indistinguishable from a clean repo, and this
  // one depends on a path, a call spelling and a scope resolver that could each
  // change. Refuse to pass vacuously: the chokepoint must be found calling both
  // commands, and at least one cached reader must resolve.
  const chokepoint = writers.get(CHOKEPOINT);
  const missing = WRITE_COMMANDS.filter(
    (c) => !(chokepoint?.commands ?? []).includes(c),
  );
  if (missing.length > 0) {
    console.error(
      `settings-write gate FAILED: expected ${CHOKEPOINT} to call ` +
        `${missing.map((c) => `commands.${c}`).join(' and ')}, and it does not.\n` +
        'Either the chokepoint moved or the call spelling changed — this gate ' +
        'is not measuring anything until that is fixed.',
    );
    process.exitCode = 1;
    return;
  }
  if (readers.size === 0) {
    console.error(
      'settings-write gate FAILED: resolved no settingsQueryOptions reader ' +
        'scopes.\nThe scope resolver or the reader spelling changed — the ' +
        'cached-reader half of this gate is not measuring anything until that ' +
        'is fixed.',
    );
    process.exitCode = 1;
    return;
  }

  const found = violations(writers, readers);
  if (found.length === 0) {
    console.log(
      `OK: settings writes go through ${CHOKEPOINT} ` +
        `(${ALLOWED_BYPASS.size} baselined bypasses, ${readers.size} cached scopes).`,
    );
    return;
  }

  console.error('settings-write gate FAILED:');
  for (const { kind, detail } of found) console.error(`  [${kind}] ${detail}`);
  console.error(
    '\n`settingsQueryOptions` caches with `staleTime: Infinity`, so a write' +
      '\nthat skips the chokepoint leaves any cached reader showing the old' +
      '\nvalue forever — and the symptom is a pane ignoring a change the user' +
      '\njust made.' +
      '\n\n[bypass]        route the write through `updateSettings` /' +
      `\n                \`settingsRestoreDefaults\` in ${CHOKEPOINT}, or add` +
      '\n                it to ALLOWED_BYPASS with the scopes it writes and why.' +
      '\n[cached-reader] the latent bypass is now live: either invalidate in the' +
      '\n                bypassing module, or move it onto the chokepoint.' +
      '\n[scope-drift]   the module writes a scope ALLOWED_BYPASS does not list;' +
      '\n                declare it so the reader check covers it.' +
      '\n[stale]         the bypass is gone; drop the entry so it cannot' +
      '\n                authorize a future one.',
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

export {
  ALLOWED_BYPASS,
  CHOKEPOINT,
  WRITE_COMMANDS,
  cachedReaderScopes,
  callPattern,
  directWriters,
  firstArguments,
  resolveScope,
  toPosix,
  violations,
};
