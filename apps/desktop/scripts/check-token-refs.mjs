#!/usr/bin/env node
// check-token-refs.mjs — every `var(--pv-*)` written in TS/TSX must resolve to
// a token the design-token build actually emitted.
//
// Scope is deliberately only TS/TSX. CSS is covered by stylelint's
// no-unknown-custom-properties, which parses the stylesheets and understands
// same-file scoping — a grep cannot, and this file makes no attempt to. What
// stylelint cannot see is a token reference living inside a string literal:
//
//     style={{ fontSize: 'var(--pv-text-xs)' }}
//
// That is the surface where a bare `var(--pv-radius)` — the suffix-less form,
// which resolves to nothing and silently drops the border-radius — reached
// production once (spec 028, R-4).
//
// The valid set is read from the GENERATED stylesheets, so there is no second
// hand-maintained list to drift from the pipeline.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const APP_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO_ROOT = resolve(APP_DIR, '../..');

const TOKEN_SOURCES = [
  join(APP_DIR, 'src/styles/tokens.css'),
  join(REPO_ROOT, 'packages/tokens/tokens-docs.css'),
];
const SCAN_ROOT = join(APP_DIR, 'src');

const defined = new Set();
for (const file of TOKEN_SOURCES) {
  for (const m of readFileSync(file, 'utf8').matchAll(/--(pv-[A-Za-z0-9_-]+)\s*:/g)) {
    defined.add(m[1]);
  }
}
if (defined.size === 0) {
  console.error(`ERROR: no tokens parsed from ${TOKEN_SOURCES.join(', ')} — check would pass vacuously.`);
  process.exit(1);
}

/**
 * Every .ts/.tsx under src/, excluding declaration files, generated bindings
 * and tests — the same exclusions Check 4 of check-tokens.sh applies. Tests
 * legitimately carry undefined token strings as fixtures: the R-4 regression
 * pin asserts on the literal `var(--pv-radius)` it exists to forbid.
 */
function* sourceFiles(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      if (entry === 'bindings') continue;
      yield* sourceFiles(path);
    } else if (
      /\.tsx?$/.test(entry) &&
      !entry.endsWith('.d.ts') &&
      !entry.includes('.test.') &&
      // .css.ts files are VE stylesheets; their component-local custom
      // properties are same-file-scoped (stylelint covers these, not this
      // script).
      !entry.endsWith('.css.ts')
    ) {
      yield path;
    }
  }
}

const problems = [];
let referenceCount = 0;

for (const file of sourceFiles(SCAN_ROOT)) {
  const text = readFileSync(file, 'utf8');
  for (const m of text.matchAll(/var\(\s*--(pv-[A-Za-z0-9_-]*)\s*[,)]/g)) {
    referenceCount += 1;
    const name = m[1];
    if (defined.has(name)) continue;
    const line = text.slice(0, m.index).split('\n').length;
    problems.push(`${relative(REPO_ROOT, file)}:${line}  --${name}`);
  }
}

// A scan that matches nothing reports success indistinguishably from a scan
// that matched everything and found no problems.
if (referenceCount === 0) {
  console.error('ERROR: no var(--pv-*) references found in TS/TSX — the scan is not looking where it thinks.');
  process.exit(1);
}

if (problems.length > 0) {
  console.error(`Undefined design tokens referenced in TS/TSX (${problems.length}):`);
  for (const p of problems) console.error(`  ${p}`);
  console.error(`\nValid tokens come from the generated stylesheets; run \`pnpm tokens:build\` if one is missing.`);
  process.exit(1);
}

console.log(`  ${referenceCount} var(--pv-*) references checked against ${defined.size} tokens.`);
