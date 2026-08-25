#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Mock-coverage baseline gate (#1221).
 *
 * `apps/desktop/src/api/mocks.ts` mocks a subset of the generated Tauri
 * commands; the rest throw "Unknown mock command" at dispatch. That is safe
 * (loud), but a NEWLY generated command silently joins the unmocked set, and
 * mock-mode Playwright specs are the only coverage for several surfaces — so
 * the gap needs to be visible rather than discovered by a failing spec.
 *
 * `tsc` already enforces the *shapes* (mockHandlers is `satisfies
 * MockRegistry`, a mapped type over the generated `commands` object) and
 * already rejects a mock for a command that no longer exists. This gate adds
 * the axis the type system cannot express: the unmocked COUNT, pinned to a
 * checked-in baseline so it can only shrink deliberately.
 *
 * Repo precedent: scripts/check-eslint-baseline.mjs + eslint-alm-baseline.txt,
 * and scripts/check-db-boundary.sh. Unlike the db-boundary guard (sealed at
 * zero), this baseline intentionally starts non-empty and drains as mocks are
 * added.
 *
 * Deliberately regex-based over the two source files rather than type-aware:
 * it must stay runnable in `pnpm lint` without a TypeScript program, and the
 * authoritative shape checking already happens in `pnpm typecheck`.
 *
 * Usage:
 *   node scripts/check-mock-baseline.mjs              # enforce (CI mode)
 *   node scripts/check-mock-baseline.mjs --generate   # rewrite the baseline
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(here, '..');
const DESKTOP_ROOT = path.join(REPO_ROOT, 'apps/desktop');
const BINDINGS_PATH = path.join(DESKTOP_ROOT, 'src/bindings/index.ts');
const MOCKS_PATH = path.join(DESKTOP_ROOT, 'src/api/mocks.ts');
const BASELINE_PATH = path.join(here, 'mock-unmocked-baseline.txt');

/**
 * Wire command names registered in the generated bindings.
 *
 * An empty set means the invoke wrapper was renamed, not that the app has no
 * commands: without this throw every command reads as mocked and --generate
 * drains the baseline.
 */
function bindingCommands(src) {
  const names = new Set(
    [...src.matchAll(/__TAURI_INVOKE\("([a-z0-9_]+)"/g)].map((m) => m[1]),
  );
  if (names.size === 0) {
    throw new Error(
      'bindings/index.ts: no __TAURI_INVOKE("command") call found — the generated invoke wrapper was renamed; update scripts/check-mock-baseline.mjs.',
    );
  }
  return names;
}

/**
 * Wire command names handled in the `mockHandlers` registry.
 *
 * Anchored to the registry block so unrelated top-level-indented object literals
 * elsewhere in the file cannot be mistaken for handlers.
 */
function mockedCommands(src) {
  const start = src.indexOf('const mockHandlers = {');
  if (start === -1) {
    throw new Error(
      'mocks.ts: `const mockHandlers = {` not found — the registry was renamed; update scripts/check-mock-baseline.mjs.',
    );
  }
  const end = src.indexOf('\n} satisfies MockRegistry;', start);
  if (end === -1) {
    throw new Error(
      'mocks.ts: `} satisfies MockRegistry;` terminator not found — the registry lost its type anchor, which would silently disable ALL mock payload checking.',
    );
  }
  const block = src.slice(start, end);
  return new Set(
    [...block.matchAll(/^ {2}([a-z0-9_]+): async \(/gm)].map((m) => m[1]),
  );
}

function loadBaseline() {
  let raw = '';
  try {
    raw = readFileSync(BASELINE_PATH, 'utf8');
  } catch {
    return new Set();
  }
  return new Set(
    raw
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith('#')),
  );
}

function writeBaseline(names) {
  const lines = [
    '# Tauri commands with NO mock in apps/desktop/src/api/mocks.ts.',
    '# These throw "Unknown mock command" in mock mode — loud, not silent, so',
    '# they are safe; this file exists so a NEWLY generated command surfaces',
    '# here as a deliberate decision instead of silently joining the list.',
    '#',
    '# scripts/check-mock-baseline.mjs fails CI on any unmocked command NOT',
    '# listed here. Drain entries by adding a mock (tsc then checks its shape',
    '# against the generated binding); regenerate with:',
    '#   node scripts/check-mock-baseline.mjs --generate',
    ...[...names].sort(),
    '',
  ];
  writeFileSync(BASELINE_PATH, lines.join('\n'));
}

function main() {
  const commands = bindingCommands(readFileSync(BINDINGS_PATH, 'utf8'));
  const mocked = mockedCommands(readFileSync(MOCKS_PATH, 'utf8'));

  // Dead mocks are already a compile error (mockHandlers is `satisfies
  // MockRegistry`), but this gate runs in `pnpm lint` — ahead of typecheck —
  // so report them here too rather than emitting a confusing count.
  const dead = [...mocked].filter((c) => !commands.has(c)).sort();
  const unmocked = new Set([...commands].filter((c) => !mocked.has(c)));

  if (process.argv.includes('--generate')) {
    writeBaseline(unmocked);
    console.log(
      `mock-unmocked-baseline.txt regenerated: ${unmocked.size} unmocked command(s).`,
    );
    return;
  }

  const baseline = loadBaseline();
  const newlyUnmocked = [...unmocked].filter((c) => !baseline.has(c)).sort();
  const stale = [...baseline].filter((c) => !unmocked.has(c)).sort();

  const problems = [];
  for (const c of dead) {
    problems.push(
      `${c}  [mock for a command that does not exist in the generated bindings]`,
    );
  }
  for (const c of newlyUnmocked) {
    problems.push(`${c}  [new command with no mock, not in baseline]`);
  }
  for (const c of stale) {
    problems.push(`${c}  [baselined as unmocked but now mocked — stale entry]`);
  }

  if (problems.length === 0) {
    console.log(
      `mock baseline: OK (${commands.size} commands; ${mocked.size} mocked; ${unmocked.size} unmocked and baselined).`,
    );
    return;
  }

  for (const p of problems) console.error(p);
  console.error(
    `\nmock baseline FAILED: ${problems.length} problem(s).` +
      '\nAdd a mock in apps/desktop/src/api/mocks.ts (preferred — tsc then checks' +
      '\nits payload against the generated binding), or, if leaving the command' +
      '\nunmocked is deliberate, run:' +
      '\n  node scripts/check-mock-baseline.mjs --generate',
  );
  process.exitCode = 1;
}

// Guarded so scripts/check-mock-baseline.test.mjs can import the parsers
// without running the gate. `import.meta.main` is NOT usable: it needs Node
// >=22.18/24.2 and CI pins node 20, where it is undefined — main() would
// silently never run and this gate would no-op green. See the same note in
// check-eslint-baseline.mjs.
if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}

export { bindingCommands, mockedCommands };
