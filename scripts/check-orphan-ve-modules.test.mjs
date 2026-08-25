#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
//
// Tests for check-orphan-ve-modules.mjs (astro-plan-ibk5).
//
// The gate reports zero orphans on a clean repo, which is indistinguishable
// from a gate that cannot find any. Every way it could break is silent: a
// specifier pattern that stops matching reports the whole tree as orphaned
// (loud), but one that over-matches, an alias that stops resolving, or a test
// file that leaks into the production graph all pass green over a real orphan.
// So the detector is exercised against fixture trees where the answer is known.
//
// It also mirrors check-mock-baseline.mjs's entry-point guard, which has its own
// history of no-opping green on Node 20.
//
// Pure Node -- no Vitest (matches check-mock-baseline.test.mjs). Run via
// `node scripts/check-orphan-ve-modules.test.mjs`.

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { orphanVeModules, specifiers } from './check-orphan-ve-modules.mjs';

const SCRIPT_PATH = resolve(
  dirname(fileURLToPath(import.meta.url)),
  'check-orphan-ve-modules.mjs',
);

const failures = [];

function assertEqual(actual, expected, msg) {
  if (actual !== expected) {
    failures.push(`${msg}\n  expected: ${expected}\n  actual:   ${actual}`);
  }
}

/** Write `files` into a throwaway tree and return its orphan list. */
function orphansOf(files, entries = ['main.tsx']) {
  const root = mkdtempSync(join(tmpdir(), 've-orphan-'));
  try {
    for (const [path, source] of Object.entries(files)) {
      const full = join(root, path);
      mkdirSync(dirname(full), { recursive: true });
      writeFileSync(full, source, 'utf8');
    }
    return orphanVeModules(root, entries).orphans;
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

// ── specifiers ─────────────────────────────────────────────────────────────
// A comment naming a module has no specifier, which is the whole reason this
// moved off a substring search: the stylesheets carry migration comments naming
// `*.css.ts` modules, and a fixture of one orphan plus one comment mentioning it
// used to report zero orphans.
const found = specifiers(`
// ./commented.css is only mentioned here
// import './commented-out.css';
/* import './block-commented.css'; */
const quoted = "import './quoted.css'";
const re = /['"]/;
import './live.css';
import { x } from '@/ui/aliased.css';
export { y } from './reexported.css';
const lazy = await import('./dynamic.css');
import react from 'react';
`);
assertEqual(found.has('./commented.css'), false, 'a comment mention yields no specifier');
assertEqual(
  found.has('./commented-out.css'),
  false,
  'a commented-out import yields no specifier',
);
assertEqual(
  found.has('./block-commented.css'),
  false,
  'a block-commented import yields no specifier',
);
assertEqual(
  found.has('./quoted.css'),
  false,
  'an import written inside a string literal yields no specifier',
);
assertEqual(found.has('./live.css'), true, 'side-effect import is a specifier');
assertEqual(found.has('@/ui/aliased.css'), true, 'aliased import is a specifier');
assertEqual(found.has('./reexported.css'), true, 're-export is a specifier');
assertEqual(found.has('./dynamic.css'), true, 'dynamic import is a specifier');
assertEqual(found.has('react'), false, 'a package specifier is not a local module');

// ── comment mentions do not keep a module alive ─────────────────────────────
assertEqual(
  orphansOf({
    'main.tsx': `// orphan.css.ts is documented here\nexport const app = 1;\n`,
    'orphan.css.ts': 'export const cls = "x";\n',
  }).join(','),
  'orphan.css.ts',
  'a module named only in a comment is an orphan',
);

assertEqual(
  orphansOf({
    'main.tsx': `// import './orphan.css';\nexport const app = 1;\n`,
    'orphan.css.ts': 'export const cls = "x";\n',
  }).join(','),
  'orphan.css.ts',
  'a module whose only importer is commented out is an orphan',
);

assertEqual(
  orphansOf({
    'main.tsx': `import './theme.css';\n`,
    'theme.css': `/* @import './orphan.css'; */\n`,
    'orphan.css.ts': 'export const cls = "x";\n',
  }).join(','),
  'orphan.css.ts',
  'a commented-out CSS @import does not keep a module alive',
);

// ── a longer sibling does not mask a shorter module ─────────────────────────
// `PropertyTable.css` contains `Table.css`, so the substring rule kept
// `ui/Table.css.ts` alive on `ui/PropertyTable.css.ts`'s importer alone.
assertEqual(
  orphansOf({
    'main.tsx': `import './PropertyTable.css';\n`,
    'PropertyTable.css.ts': 'export const a = 1;\n',
    'Table.css.ts': 'export const b = 1;\n',
  }).join(','),
  'Table.css.ts',
  'a longer sibling name does not keep the shorter module alive',
);

// ── a test-only importer does not count ────────────────────────────────────
assertEqual(
  orphansOf({
    'main.tsx': 'export const app = 1;\n',
    've-class-application.test.tsx': `import './styled.css';\n`,
    'styled.css.ts': 'export const c = 1;\n',
  }).join(','),
  'styled.css.ts',
  'a stylesheet imported only by a test is an orphan',
);

// ── an orphan does not keep its own imports alive ───────────────────────────
assertEqual(
  orphansOf({
    'main.tsx': 'export const app = 1;\n',
    'dead.css.ts': `import './alsoDead.css';\nexport const d = 1;\n`,
    'alsoDead.css.ts': 'export const e = 1;\n',
  }).join(','),
  'alsoDead.css.ts,dead.css.ts',
  'a module imported only by an orphan is an orphan too',
);

// ── the live cases pass ────────────────────────────────────────────────────
assertEqual(
  orphansOf({
    'main.tsx': `import './Panel';\n`,
    'Panel.tsx': `import './panel.css';\nexport const P = 1;\n`,
    'panel.css.ts': 'export const f = 1;\n',
  }).length,
  0,
  'a stylesheet reached through an intermediate component is live',
);
assertEqual(
  orphansOf({
    'main.tsx': `import '@/ui/aliased.css';\n`,
    'ui/aliased.css.ts': 'export const g = 1;\n',
  }).length,
  0,
  'an aliased import resolves against the tree root',
);
assertEqual(
  orphansOf(
    {
      'main.tsx': 'export const app = 1;\n',
      'splash/main.ts': `import './splash.css';\n`,
      'splash/splash.css.ts': 'export const h = 1;\n',
    },
    ['main.tsx', 'splash/main.ts'],
  ).length,
  0,
  'the second HTML entry point contributes to reachability',
);

// ── entry-point guard ──────────────────────────────────────────────────────
// Same class of bug as check-eslint-baseline.mjs's `import.meta.main` no-op: an
// exit-0-with-no-output run reads as "step passed" to `pnpm lint`.
const run = spawnSync(process.execPath, [SCRIPT_PATH], { encoding: 'utf8' });
const stdout = run.stdout ?? '';
if (stdout.trim().length === 0) {
  failures.push(
    `check-orphan-ve-modules.mjs produced NO stdout when run directly (node ${SCRIPT_PATH}) — the entry-point guard did not fire, so main() never ran. status=${run.status} stderr=${run.stderr}`,
  );
} else if (!/^OK: all \d+ vanilla-extract modules/.test(stdout)) {
  failures.push(
    `check-orphan-ve-modules.mjs produced unexpected stdout (entry-point guard may not have fired as expected):\n${stdout}`,
  );
}

if (failures.length > 0) {
  console.error(`orphan-ve-modules self-test FAILED (${failures.length}):`);
  for (const f of failures) console.error(`\n  ${f}`);
  process.exitCode = 1;
} else {
  console.log('orphan-ve-modules self-test: PASS');
}
