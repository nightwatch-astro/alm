#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Self-test for the settings-write invalidation gate.
 *
 * The gate reads a directory tree with regexes, so its two failure modes are
 * matching nothing (passing vacuously) and matching everything. Both are tested
 * against a synthetic tree rather than the real repo, so the assertions do not
 * drift as `apps/desktop/src` changes.
 *
 * The chained-call, restore-defaults and cached-reader cases each shipped as a
 * hole in the first version of this gate: `app/LogPanelContext.tsx` and
 * `features/setup/steps/StepCatalogs.tsx` bypassed it unseen because prettier had
 * broken their calls across lines, and both wrote a scope a `useQuery` reads.
 *
 * Pure Node -- no Vitest (matches check-mock-baseline.test.mjs). Run via
 * `node scripts/check-settings-write-invalidation.test.mjs`.
 */

import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

import {
  cachedReaderScopes,
  directWriters,
  resolveScope,
  toPosix,
  violations,
} from './check-settings-write-invalidation.mjs';

const SCRIPT_PATH = resolve(
  dirname(fileURLToPath(import.meta.url)),
  'check-settings-write-invalidation.mjs',
);

const WRITE = "await commands.settingsUpdate('s', {});";

function fixture(files) {
  const root = mkdtempSync(join(tmpdir(), 'settings-gate-'));
  for (const [rel, body] of Object.entries(files)) {
    const full = join(root, rel);
    mkdirSync(join(full, '..'), { recursive: true });
    writeFileSync(full, body);
  }
  return root;
}

/** Writer paths only, which is what most cases assert on. */
function writerPaths(root) {
  return [...directWriters(root).keys()];
}

let failures = 0;
function check(name, fn) {
  try {
    fn();
    console.log(`ok   ${name}`);
  } catch (e) {
    failures += 1;
    console.error(`FAIL ${name}\n  ${e.message}`);
  }
}

// ── call-site matching ─────────────────────────────────────────────────────────

check('finds a direct writer', () => {
  assert.deepEqual(writerPaths(fixture({ 'a/writer.ts': WRITE })), ['a/writer.ts']);
});

check('ignores a module that never writes', () => {
  const root = fixture({ 'a/reader.ts': 'const x = commands.settingsGet();' });
  assert.deepEqual(writerPaths(root), []);
});

check('ignores tests, which may write freely', () => {
  const root = fixture({
    'a/x.test.ts': WRITE,
    'a/y.spec.tsx': WRITE,
    'a/__tests__/z.ts': WRITE,
  });
  assert.deepEqual(writerPaths(root), []);
});

check('a doc-comment mention is not a call', () => {
  // This caught a wrong allowlist entry when the gate was written:
  // settingsQueries.ts only NAMES the command in its docstring.
  const root = fixture({ 'a/doc.ts': ' * A writer calls commands.settingsUpdate afterwards.' });
  assert.deepEqual(writerPaths(root), []);
});

check('tolerates whitespace before the paren', () => {
  const root = fixture({ 'a/spaced.ts': "commands.settingsUpdate ('s', {});" });
  assert.deepEqual(writerPaths(root), ['a/spaced.ts']);
});

check('matches a call prettier broke across lines', () => {
  // The real miss: `void commands\n  .settingsUpdate('advanced', ...)`.
  const root = fixture({
    'a/chained.ts': "void commands\n  .settingsUpdate('advanced', { v: 1 })\n  .then(unwrap);",
  });
  assert.deepEqual(writerPaths(root), ['a/chained.ts']);
});

check('matches settingsRestoreDefaults as a write', () => {
  const root = fixture({
    'a/restore.ts': "await commands.settingsRestoreDefaults({ keys: ['k'] });",
  });
  assert.deepEqual(directWriters(root).get('a/restore.ts').commands, [
    'settingsRestoreDefaults',
  ]);
});

check('recurses and returns sorted paths', () => {
  const root = fixture({
    'z/late.ts': WRITE,
    'a/deep/early.tsx': WRITE,
    'a/plain.ts': 'nothing',
  });
  assert.deepEqual(writerPaths(root), ['a/deep/early.tsx', 'z/late.ts']);
});

// ── scope resolution ──────────────────────────────────────────────────────────

check('resolves a literal and a local constant scope', () => {
  const source = "const OBSERVING_SCOPE = 'observing';";
  assert.equal(resolveScope(source, "'cleanup'"), 'cleanup');
  assert.equal(resolveScope(source, 'OBSERVING_SCOPE'), 'observing');
  assert.equal(resolveScope(source, 'runtimeScope'), null);
});

check('derives the written scope from the call site', () => {
  const root = fixture({
    'a/w.ts': "const S = 'planner';\nawait commands.settingsUpdate(S, {});",
  });
  assert.deepEqual(directWriters(root).get('a/w.ts').scopes, ['planner']);
});

check('marks an unresolvable scope rather than dropping it', () => {
  const root = fixture({ 'a/w.ts': 'await commands.settingsUpdate(scope, {});' });
  assert.equal(directWriters(root).get('a/w.ts').unresolved, true);
});

check('collects reader scopes and skips the query definition', () => {
  const root = fixture({
    'features/settings/settingsQueries.ts':
      'export function settingsQueryOptions(scope: string) {}',
    'features/settings/Cleanup.tsx': "useQuery(settingsQueryOptions('cleanup'));",
    'features/settings/Framing.tsx':
      "const FRAMING_SCOPE = 'framing';\nuseQuery(settingsQueryOptions(FRAMING_SCOPE));",
  });
  assert.deepEqual([...cachedReaderScopes(root).keys()].sort(), ['cleanup', 'framing']);
});

// ── violation kinds ───────────────────────────────────────────────────────────

check('an unlisted direct writer is a bypass', () => {
  const root = fixture({ 'a/rogue.ts': WRITE });
  const found = violations(directWriters(root), new Map(), new Map());
  assert.deepEqual(found.map((v) => v.kind), ['bypass']);
});

check('a baselined bypass whose scope gains a reader fails', () => {
  const root = fixture({ 'a/listed.ts': "await commands.settingsUpdate('general', {});" });
  const readers = new Map([['general', ['features/settings/General.tsx']]]);
  const allowlist = new Map([['a/listed.ts', { scopes: ['general'] }]]);
  const found = violations(directWriters(root), readers, allowlist);
  assert.deepEqual(found.map((v) => v.kind), ['cached-reader']);
});

check('a baselined bypass with no reader passes', () => {
  const root = fixture({ 'a/listed.ts': "await commands.settingsUpdate('general', {});" });
  const readers = new Map([['cleanup', ['features/settings/Cleanup.tsx']]]);
  const allowlist = new Map([['a/listed.ts', { scopes: ['general'] }]]);
  assert.deepEqual(violations(directWriters(root), readers, allowlist), []);
});

check('a scope the allowlist does not declare is drift', () => {
  const root = fixture({ 'a/listed.ts': "await commands.settingsUpdate('surprise', {});" });
  const allowlist = new Map([['a/listed.ts', { scopes: ['general'] }]]);
  const found = violations(directWriters(root), new Map(), allowlist);
  assert.deepEqual(found.map((v) => v.kind), ['scope-drift']);
});

check('a non-derivable entry is exempt from the drift check', () => {
  const root = fixture({ 'a/listed.ts': 'await commands.settingsUpdate(scope, {});' });
  const allowlist = new Map([
    ['a/listed.ts', { scopes: ['ui_state'], derivable: false }],
  ]);
  assert.deepEqual(violations(directWriters(root), new Map(), allowlist), []);
});

check('a stale allowlist entry is an error, not a note', () => {
  // A warning left the path permanently authorized, so a direct write added back
  // to the same file passed the gate unseen.
  const allowlist = new Map([['a/gone.ts', { scopes: ['general'] }]]);
  const found = violations(new Map(), new Map(), allowlist);
  assert.deepEqual(found.map((v) => v.kind), ['stale']);
});

// ── path handling ─────────────────────────────────────────────────────────────

check('windows separators are normalized before comparison', () => {
  // `path.relative` returns `features\settings\settingsIpc.ts` on Windows, which
  // matched no forward-slash constant and made the gate fail there always.
  assert.equal(toPosix('features\\settings\\settingsIpc.ts'), 'features/settings/settingsIpc.ts');
  assert.equal(toPosix('features/settings/settingsIpc.ts'), 'features/settings/settingsIpc.ts');
});

// ── entry-point guard ─────────────────────────────────────────────────────────
// Same class of bug as check-eslint-baseline.mjs's `import.meta.main` no-op: an
// exit-0-with-no-output run reads as "step passed" to `pnpm lint`.
check('running the gate directly produces output', () => {
  const run = spawnSync(process.execPath, [SCRIPT_PATH], { encoding: 'utf8' });
  const stdout = run.stdout ?? '';
  assert.ok(
    stdout.trim().length > 0,
    `no stdout from \`node ${SCRIPT_PATH}\` — the entry-point guard did not fire, so main() never ran. status=${run.status} stderr=${run.stderr}`,
  );
  assert.match(stdout, /^OK: settings writes go through /);
});

if (failures > 0) {
  console.error(`\n${failures} test(s) failed.`);
  process.exitCode = 1;
} else {
  console.log('\nsettings-write gate self-test: all passed.');
}
