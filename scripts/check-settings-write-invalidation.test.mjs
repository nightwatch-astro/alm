#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Self-test for the settings-write invalidation gate.
 *
 * The gate is a regex over a directory tree, so its two failure modes are
 * matching nothing (passing vacuously) and matching everything. Both are tested
 * here against a synthetic tree rather than the real repo, so the assertions do
 * not drift as `apps/desktop/src` changes.
 */

import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import assert from 'node:assert/strict';

import { directWriters } from './check-settings-write-invalidation.mjs';

const WRITE = 'await commands.settingsUpdate({ scope: "s", values: {} });';

function fixture(files) {
  const root = mkdtempSync(join(tmpdir(), 'settings-gate-'));
  for (const [rel, body] of Object.entries(files)) {
    const full = join(root, rel);
    mkdirSync(join(full, '..'), { recursive: true });
    writeFileSync(full, body);
  }
  return root;
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

check('finds a direct writer', () => {
  const root = fixture({ 'a/writer.ts': WRITE });
  assert.deepEqual(directWriters(root), ['a/writer.ts']);
});

check('ignores a module that never writes', () => {
  const root = fixture({ 'a/reader.ts': 'const x = commands.settingsGet();' });
  assert.deepEqual(directWriters(root), []);
});

check('ignores tests, which may write freely', () => {
  const root = fixture({
    'a/x.test.ts': WRITE,
    'a/y.spec.tsx': WRITE,
    'a/__tests__/z.ts': WRITE,
  });
  assert.deepEqual(directWriters(root), []);
});

check('a doc-comment mention is not a call', () => {
  // This is the case that caught a wrong allowlist entry when the gate was
  // written: settingsQueries.ts only NAMES the command in its docstring.
  const root = fixture({ 'a/doc.ts': ' * A writer calls commands.settingsUpdate afterwards.' });
  assert.deepEqual(directWriters(root), []);
});

check('tolerates whitespace before the paren', () => {
  const root = fixture({ 'a/spaced.ts': 'commands.settingsUpdate ({});' });
  assert.deepEqual(directWriters(root), ['a/spaced.ts']);
});

check('recurses and returns sorted paths', () => {
  const root = fixture({
    'z/late.ts': WRITE,
    'a/deep/early.tsx': WRITE,
    'a/plain.ts': 'nothing',
  });
  assert.deepEqual(directWriters(root), ['a/deep/early.tsx', 'z/late.ts']);
});

if (failures > 0) {
  console.error(`\n${failures} test(s) failed.`);
  process.exitCode = 1;
} else {
  console.log('\nsettings-write gate self-test: all passed.');
}
