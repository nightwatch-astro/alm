#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
//
// Regression tests for two run-jc0717 bugs in check-eslint-baseline.mjs:
//
// 1. Windows path-separator bug: path.relative() returns backslash-separated
//    paths on Windows, which never matched the forward-slash entries in the
//    checked-in (cross-platform) eslint-alm-baseline.txt — every alm/*
//    violation looked "new" on Windows CI even when already baselined.
//
// 2. Silent-no-op entry-point bug: the script previously guarded its
//    `main()` call with `import.meta.main`, which is `undefined` (falsy) on
//    Node <22.18/24.2 — and CI pins node-version: 20 (ci.yml:170,:309) — so
//    `main()` never ran and the ENTIRE alm/* lint gate exited 0 with zero
//    output on every CI leg. A unit test importing `baselineKey` alone
//    cannot catch this class of bug (the module still imports fine); only
//    actually spawning the script as CI does and asserting it produced
//    output proves the entry-point guard fired.
//
// Pure Node — no Vitest (matches packages/contracts/tests/*.test.mjs). Run
// via `node scripts/check-eslint-baseline.test.mjs`.

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { baselineKey, lintedFileFloorError } from './check-eslint-baseline.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const SCRIPT_PATH = path.join(here, 'check-eslint-baseline.mjs');

const failures = [];
function assertEqual(actual, expected, msg) {
  if (actual !== expected) {
    failures.push(`${msg}\n  expected: ${expected}\n  actual:   ${actual}`);
  }
}

const message = { line: 101, ruleId: 'alm/no-user-string' };

// Forward-slash input (POSIX path.relative() output, and what's checked into
// the baseline file) must produce a stable key.
assertEqual(
  baselineKey('src/features/calibration/MastersTable.tsx', message),
  'src/features/calibration/MastersTable.tsx\t101\talm/no-user-string',
  'forward-slash relPath',
);

// Backslash input (what path.relative() returns on win32) must normalize to
// the SAME key as the forward-slash form above — this is the bug.
assertEqual(
  baselineKey('src\\features\\calibration\\MastersTable.tsx', message),
  'src/features/calibration/MastersTable.tsx\t101\talm/no-user-string',
  'backslash relPath (win32 path.relative() output) must match the forward-slash baseline entry',
);

// A relPath built with the CURRENT platform's path.sep must also normalize
// (covers the actual call site: path.relative(DESKTOP_ROOT, filePath)).
const platformRelPath = ['src', 'features', 'calibration', 'MastersTable.tsx'].join(
  path.sep,
);
assertEqual(
  baselineKey(platformRelPath, message),
  'src/features/calibration/MastersTable.tsx\t101\talm/no-user-string',
  'relPath joined with the current platform path.sep',
);

// A lint run that covered no files must be refused, not reported as clean.
assertEqual(
  lintedFileFloorError(0, 'src/') === null,
  false,
  'a zero-file lint run must produce a floor error',
);
assertEqual(
  lintedFileFloorError(1, 'src/'),
  null,
  'a lint run covering at least one file must pass the floor',
);

// Entry-point execution proof: spawn the script exactly as `pnpm lint` /
// CI does (`node <path>`, no extra args) and assert it actually produced
// output. This is the assertion that would have caught the
// `import.meta.main` bug — that version exited 0 with EMPTY stdout, which
// every other check in this repo's lint pipeline would have read as "step
// passed."
const run = spawnSync(process.execPath, [SCRIPT_PATH], { encoding: 'utf8' });
const stdout = run.stdout ?? '';
if (stdout.trim().length === 0) {
  failures.push(
    `check-eslint-baseline.mjs produced NO stdout when run directly (node ${SCRIPT_PATH}) — the entry-point guard did not fire, so main() never ran. status=${run.status} stderr=${run.stderr}`,
  );
} else if (!/^eslint: /.test(stdout) && !/^eslint FAILED/.test(stdout)) {
  failures.push(
    `check-eslint-baseline.mjs produced unexpected stdout (entry-point guard may not have fired as expected):\n${stdout}`,
  );
}

if (failures.length > 0) {
  console.error(`check-eslint-baseline.test.mjs FAILED:\n\n${failures.join('\n\n')}`);
  process.exitCode = 1;
} else {
  console.log('check-eslint-baseline.test.mjs: OK (6 assertions).');
}
