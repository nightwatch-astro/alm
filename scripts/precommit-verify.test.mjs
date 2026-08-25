#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
//
// Tests for precommit-verify.sh.
//
// The wrapper exists to separate "every hook skipped" from "every hook passed",
// which pre-commit itself reports with the same exit 0. The discrimination lives
// entirely in how it reads hook status lines, so the tests drive it against a
// stubbed `pre-commit` on PATH rather than the real one: a real run depends on
// installed hook environments and on the repo's own exclude list, neither of
// which this behaviour should be coupled to.
//
// Pure Node — no Vitest (matches check-mock-baseline.test.mjs). Run via
// `node scripts/precommit-verify.test.mjs`.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const here = path.dirname(fileURLToPath(import.meta.url));
const SCRIPT_PATH = path.join(here, 'precommit-verify.sh');

const failures = [];

// Real `pre-commit run --files` output, one status line per hook.
const ALL_SKIPPED = [
  'check for added large files.............................(no files to check)Skipped',
  'check json.............................................(no files to check)Skipped',
  'typos..................................................(no files to check)Skipped',
].join('\n');

const SOME_PASSED = [
  'check for added large files..............................................Passed',
  'check json.............................................(no files to check)Skipped',
  'typos....................................................................Passed',
].join('\n');

const ONE_FAILED = [
  'check for added large files..............................................Passed',
  'typos....................................................................Failed',
].join('\n');

/** Run the wrapper with a stub `pre-commit` that prints `output` and exits `code`. */
function runWithStub(output, code, args = ['some/file.md']) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'precommit-verify-'));
  try {
    const stub = path.join(dir, 'pre-commit');
    fs.writeFileSync(stub, `#!/bin/sh\ncat <<'EOF'\n${output}\nEOF\nexit ${code}\n`);
    fs.chmodSync(stub, 0o755);
    return spawnSync('bash', [SCRIPT_PATH, ...args], {
      encoding: 'utf8',
      env: { ...process.env, PATH: `${dir}:${process.env.PATH}` },
    });
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function check(label, run, expectedStatus, expectedStdoutPattern) {
  if (run.status !== expectedStatus) {
    failures.push(
      `${label}: expected exit ${expectedStatus}, got ${run.status}\n  stdout: ${run.stdout}\n  stderr: ${run.stderr}`,
    );
    return;
  }
  if (expectedStdoutPattern && !expectedStdoutPattern.test(run.stdout ?? '')) {
    failures.push(`${label}: stdout did not match ${expectedStdoutPattern}\n  stdout: ${run.stdout}`);
  }
}

// A run where every hook skipped is the vacuous case pre-commit reports as 0.
check('all hooks skipped', runWithStub(ALL_SKIPPED, 0), 1, /actually ran on these 1 path\(s\): 0/);

// The same output, but pre-commit's own exit status must not be what decides it.
check('all hooks skipped, non-zero pre-commit', runWithStub(ALL_SKIPPED, 1), 1, /: 0/);

// A run where hooks did look at the files passes through unchanged.
check('some hooks passed', runWithStub(SOME_PASSED, 0), 0, /actually ran on these 1 path\(s\): 2/);

// A real hook failure keeps pre-commit's status; the wrapper must not mask it.
check('a hook failed', runWithStub(ONE_FAILED, 1), 1, /actually ran on these 1 path\(s\): 2/);

// No paths is a caller error, not a vacuous run.
const noArgs = spawnSync('bash', [SCRIPT_PATH], { encoding: 'utf8' });
if (noArgs.status !== 2) {
  failures.push(`no arguments: expected exit 2, got ${noArgs.status}`);
}

if (failures.length > 0) {
  console.error(`precommit-verify.test.mjs FAILED:\n\n${failures.join('\n\n')}`);
  process.exitCode = 1;
} else {
  console.log('precommit-verify.test.mjs: OK (5 assertions).');
}
