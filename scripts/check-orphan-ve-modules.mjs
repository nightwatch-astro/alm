#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Orphan vanilla-extract module gate (astro-plan-ibk5).
 *
 * A `*.css.ts` module that nothing imports emits no CSS, so the styling it was
 * written to provide silently does not apply. There is no build error and no
 * failing test: the component keeps rendering, unstyled or falling back to a
 * stylesheet rule that may itself be on its way out. Two manual audits (#1572
 * and chore/mv02-dead-css) found seven of these.
 *
 * Why this script and not `knip`: knip does not report them. Verified by
 * planting an orphan `*.css.ts` and an orphan plain `.ts` under
 * `apps/desktop/src` and running both `npx knip` and `npx knip --include files`
 * — neither named either file. So the earlier suggestion that knip already
 * covers this was wrong; it does not, for VE modules or for ordinary ones.
 *
 * Liveness is reachability from the production entry point, not the presence of
 * the module's name in some file. Two weaker rules were tried and both fail
 * open:
 *
 *   - A substring search for the `<name>.css` stem passes on a mention in a
 *     migration comment, and `PropertyTable.css` contains `Table.css`, so a
 *     longer sibling's import keeps a shorter module alive.
 *   - Counting any importer treats a stylesheet imported only by a unit test as
 *     live. `components/ve-class-application.test.tsx` imports stylesheets
 *     directly, so deleting the production import would leave this gate green
 *     while the bundle lost the CSS.
 *
 * Reachability catches a third case neither reaches: a module imported only by
 * another orphan is itself dead, and the import graph says so.
 *
 * Deliberately narrow. It answers one question — does the production bundle
 * reach this module — and does not attempt the harder one of whether a `.pv-*`
 * selector in a plain stylesheet still matches a rendered element. Absolute
 * orphan counts are unusable as a gate (418 of 1426 `pv-` classes are
 * unreferenced on main, most legitimately: token names, utility classes, state
 * hooks), which is why that half stays manual.
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import ts from 'typescript';

// Anchored to this file, not cwd: the gate is invoked both from the repo root
// and from apps/desktop, and a cwd-relative path breaks one of the two.
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(REPO_ROOT, 'apps/desktop/src');

// The two HTML entries in `vite.config.ts`'s `rollupOptions.input`: `index.html`
// loads `/src/main.tsx`, `splash.html` loads `/src/splash/main.ts`. Everything
// the bundle emits is reached from one of the two, and leaving the splash entry
// out would report its stylesheets as orphans.
const ENTRIES = ['main.tsx', 'splash/main.ts'];

// `@/x` is `src/x`, declared in both `tsconfig.json` (`compilerOptions.paths`)
// and `vite.config.ts` (`resolve.alias`). Most imports here use it, so a
// relative-only resolver reaches almost nothing and reports the tree as orphaned.
const ALIAS_PREFIX = '@/';

/**
 * VE modules the production graph does not reach, allowed to stay.
 *
 * Empty on purpose. If an entry is ever needed, say why the module exists
 * without a production importer -- a genuine side-effect-only global sheet is
 * the only case I would expect, and those are usually imported for their side
 * effect anyway (see `ui/Tooltip.css.ts`, which IS imported that way).
 */
const ALLOWED_ORPHANS = new Set([]);

/** True for a file the production bundle never includes. */
function isTestFile(path) {
  return (
    /\.(test|spec)\.(ts|tsx)$/.test(path) ||
    path.includes(`${'/'}__tests__${'/'}`) ||
    path.includes(`${'/'}__mocks__${'/'}`) ||
    /(^|\/)__smoke__\.ts$/.test(path)
  );
}

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) {
      out.push(...walk(full));
      continue;
    }
    if (/\.(ts|tsx|css)$/.test(name)) out.push(full);
  }
  return out;
}

// CSS `@import`, with block comments removed first so a commented-out import is
// not a live specifier. CSS has no line comments and no template literals, so
// the two together are the whole grammar that matters here.
const CSS_IMPORT = /@import\s+(?:url\()?\s*['"]([^'"]+)['"]/g;
const CSS_COMMENT = /\/\*[\s\S]*?\*\//g;

/**
 * Specifiers in `source` that name a file in this tree, as written.
 *
 * TypeScript's own preprocessor supplies the JS/TS side. A regex over raw source
 * cannot: it reads `// import './x.css'` as a live specifier, which leaves this
 * gate green when a refactor comments an import out. Stripping comments by regex
 * trades that for a worse bug, because a regex literal holding an unbalanced
 * quote (`/['"]/`) desynchronises the string tracking.
 *
 * A bare specifier is a package and never a local module, so it is dropped here
 * rather than resolved and discarded later.
 */
function specifiers(source, isCss = false) {
  const found = isCss
    ? [...source.replace(CSS_COMMENT, ' ').matchAll(CSS_IMPORT)].map((m) => m[1])
    : ts.preProcessFile(source, true, true).importedFiles.map((f) => f.fileName);

  return new Set(
    found.filter((s) => s.startsWith('.') || s.startsWith(ALIAS_PREFIX)),
  );
}

/**
 * Absolute path a specifier names, or null when it leaves the walked tree.
 *
 * A TypeScript specifier drops the extension, so `./modal.css` is the file
 * `modal.css.ts` and `./Panel` is `Panel.tsx`. The plain-CSS candidate comes
 * first for an exact hit, which keeps an `@import './theme.css'` targeting a
 * real `theme.css` from resolving to `theme.css.ts`.
 */
function resolveSpecifier(fromFile, specifier, root = SRC) {
  const base = specifier.startsWith(ALIAS_PREFIX)
    ? join(root, specifier.slice(ALIAS_PREFIX.length))
    : resolve(dirname(fromFile), specifier);
  for (const candidate of [
    base,
    `${base}.ts`,
    `${base}.tsx`,
    join(base, 'index.ts'),
    join(base, 'index.tsx'),
  ]) {
    if (existsSync(candidate) && statSync(candidate).isFile()) return candidate;
  }
  return null;
}

/**
 * VE modules under `root` that no production entry point reaches.
 *
 * `entries` is relative to `root` so the self-test can point at a fixture tree.
 */
function orphanVeModules(root = SRC, entries = ENTRIES) {
  const files = walk(root);
  const veModules = files.filter((f) => f.endsWith('.css.ts'));
  if (veModules.length === 0) return { veModules, orphans: [], reached: new Set() };

  const production = new Set(files.filter((f) => !isTestFile(f)));

  // Breadth-first over production imports only. A test file is never enqueued,
  // so a stylesheet it alone imports stays unreached.
  const reached = new Set();
  const queue = entries.map((e) => join(root, e)).filter((p) => production.has(p));
  while (queue.length > 0) {
    const current = queue.pop();
    if (reached.has(current)) continue;
    reached.add(current);
    for (const specifier of specifiers(readFileSync(current, 'utf8'), current.endsWith('.css'))) {
      const target = resolveSpecifier(current, specifier, root);
      if (target !== null && production.has(target) && !reached.has(target)) {
        queue.push(target);
      }
    }
  }

  const orphans = veModules
    .filter((f) => !reached.has(f))
    .map((f) => relative(root, f))
    .sort();

  return { veModules, orphans, reached };
}

function main() {
  const { veModules, orphans, reached } = orphanVeModules();

  // A gate that finds nothing looks identical to a clean repo. This one walks a
  // tree and traverses an import graph, either of which could break silently, so
  // refuse to pass vacuously: there must be VE modules to check, and the entry
  // point must actually reach something.
  if (veModules.length === 0) {
    console.error(
      'FAIL: found no *.css.ts modules under apps/desktop/src.\n' +
        'Either the tree moved or the walk is broken -- this gate is not ' +
        'measuring anything until that is fixed.',
    );
    process.exitCode = 1;
    return;
  }

  // Without this, a renamed or moved entry point makes every module unreachable
  // and the gate reports the whole tree as orphaned, or -- worse, if the list
  // were ever allowlisted -- reports nothing at all.
  if (reached.size <= 1) {
    console.error(
      `FAIL: the production entry points reached ${reached.size} file(s).\n` +
        'Either it moved or specifier resolution is broken -- the reachability ' +
        'result is not trustworthy until that is fixed.',
    );
    process.exitCode = 1;
    return;
  }

  const unexpected = orphans.filter((f) => !ALLOWED_ORPHANS.has(f));
  if (unexpected.length === 0) {
    console.log(
      `OK: all ${veModules.length} vanilla-extract modules are reachable from the production entry points.`,
    );
    return;
  }

  console.error(
    'FAIL: the production bundle does not reach these vanilla-extract modules:',
  );
  for (const f of unexpected) console.error(`  ${f}`);
  console.error(
    '\nAn unimported *.css.ts emits no CSS, so whatever it was written to style' +
      '\nis silently unstyled -- no build error, no failing test. A module' +
      '\nimported only by a test, or only by another orphan, is unreached too.' +
      '\n\nEither wire it into the component it belongs to, or delete it. Do not' +
      '\nadd it to ALLOWED_ORPHANS unless it is genuinely side-effect-only, and' +
      '\nsay why in a comment there.',
  );
  process.exitCode = 1;
}

// Guarded so the self-test can import the walker without running the gate.
// `import.meta.main` is NOT usable: it needs Node >=22.18/24.2 and CI pins node
// 20, where it is undefined -- main() would never run and this gate would no-op
// green. Same note as check-mock-baseline.mjs.
// `argv[1]` is undefined under `node -e`, where pathToFileURL throws.
if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}

export { ALLOWED_ORPHANS, ENTRIES, isTestFile, orphanVeModules, resolveSpecifier, specifiers };
