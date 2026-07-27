#!/usr/bin/env node
// Circular-dependency gate (DS-22).
//
// Runs madge --circular over src/ and fails if any cycle is found that is not
// in the known-allowlist below.  The allowlist contains only type-import cycles
// that are safe at runtime (TypeScript `import type` creates no JS module edge)
// but that madge cannot distinguish from value imports.  It MUST NOT grow: new
// cycles fail the check.  After a refactor removes an allowlisted cycle, delete
// its entry so the list actually shrinks.
//
// Usage:
//   node scripts/check-circular.mjs              # CI / lint gate
//   node scripts/check-circular.mjs --list       # print all current cycles

import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

// Allowlisted cycles — type-import-only; safe at runtime.
// Format: canonical string produced by madge, e.g.
//   "a/b.tsx > a/c.ts"
// Remove an entry once the cycle is fixed.
const ALLOWLIST = new Set([
  // TargetSearch/TargetSearch.tsx imports `useTargetSearch` (value), and
  // useTargetSearch imports `type { TargetSearchProps }` from TargetSearch.
  // The type half is erased at compile time; the value import direction is
  // TargetSearch→useTargetSearch, not a real cycle at runtime.
  'components/TargetSearch/TargetSearch.tsx > components/TargetSearch/useTargetSearch.ts',

  // PlanPanel imports PlanDestructiveControl and PlanRootPicker (value), both
  // of which `import type { … }` from PlanPanel.  Type-only reverse edges.
  'features/inbox/PlanPanel.tsx > features/inbox/PlanDestructiveControl.tsx',
  'features/inbox/PlanPanel.tsx > features/inbox/PlanRootPicker.tsx',
]);

const listMode = process.argv.includes('--list');

let madgeOut;
try {
  madgeOut = execSync(
    'node_modules/.bin/madge --circular --json --no-spinner --extensions ts,tsx src',
    { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] },
  );
} catch (err) {
  // madge exits 1 when cycles are found — capture stdout anyway.
  madgeOut = /** @type {any} */ (err).stdout ?? '';
}

// `--json` is the only stable machine format: madge's human output carries ANSI
// styling even under `--no-color`, which defeats string comparison against the
// allowlist.  Unparseable output means madge did not run (e.g. dependencies not
// installed) — fail rather than report a vacuous pass.
/** @type {string[][]} */
let cycles;
try {
  cycles = JSON.parse(madgeOut);
  if (!Array.isArray(cycles)) throw new Error('expected an array');
} catch (err) {
  process.stderr.write(
    `CIRCULAR DEP GATE FAILED — could not parse madge --json output: ${
      /** @type {Error} */ (err).message
    }\nRun \`pnpm install\` in apps/desktop and retry.\n`,
  );
  process.exit(1);
}

const cycleLines = cycles.map((c) => c.join(' > '));

if (listMode) {
  for (const c of cycleLines) process.stdout.write(`${c}\n`);
  process.exit(0);
}

const newCycles = cycleLines.filter((c) => !ALLOWLIST.has(c));

if (newCycles.length > 0) {
  process.stderr.write(
    `CIRCULAR DEP GATE FAILED — ${newCycles.length} new cycle(s) found:\n`,
  );
  for (const c of newCycles) process.stderr.write(`  ${c}\n`);
  process.stderr.write(
    '\nFix the cycle or, for type-only imports, add it to the ALLOWLIST in\n' +
    'apps/desktop/scripts/check-circular.mjs with a comment explaining why\n' +
    'it is safe.\n',
  );
  process.exit(1);
}

const knownRemaining = cycleLines.filter((c) => ALLOWLIST.has(c));
process.stdout.write(
  `Circular dep gate OK — 0 new cycles (${knownRemaining.length} allowlisted type-import cycle(s)).\n`,
);
