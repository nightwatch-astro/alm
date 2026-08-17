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
 * Deliberately narrow. It answers one question — is this module imported
 * anywhere — and does not attempt the harder one of whether a `.pv-*` selector
 * in a plain stylesheet still matches a rendered element. Absolute orphan
 * counts are unusable as a gate (418 of 1426 `pv-` classes are unreferenced on
 * main, most legitimately: token names, utility classes, state hooks), which is
 * why that half stays manual.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Anchored to this file, not cwd: the gate is invoked both from the repo root
// and from apps/desktop, and a cwd-relative path breaks one of the two.
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(REPO_ROOT, 'apps/desktop/src');

/**
 * VE modules with no importer, allowed to stay.
 *
 * Empty on purpose. If an entry is ever needed, say why the module exists
 * without an importer -- a genuine side-effect-only global sheet is the only
 * case I would expect, and those are usually imported for their side effect
 * anyway (see `ui/Tooltip.css.ts`, which IS imported that way).
 */
const ALLOWED_ORPHANS = new Set([]);

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

/** VE modules under `root` that no other file references by name. */
function orphanVeModules(root = SRC) {
  const files = walk(root);
  const veModules = files.filter((f) => f.endsWith('.css.ts'));
  if (veModules.length === 0) return { veModules, orphans: [] };

  // Import specifiers drop the `.ts`, so match on the `<name>.css` stem. Read
  // every other file once rather than per module: this runs in a lint chain.
  const corpus = files
    .filter((f) => !f.endsWith('.css.ts'))
    .map((f) => readFileSync(f, 'utf8'))
    .join('\n');

  const orphans = veModules
    .filter((f) => {
      const stem = basename(f, '.ts'); // e.g. `modal.css`
      // Also check sibling VE modules: one may re-export another.
      const siblings = veModules
        .filter((o) => o !== f)
        .map((o) => readFileSync(o, 'utf8'))
        .join('\n');
      return !corpus.includes(stem) && !siblings.includes(stem);
    })
    .map((f) => relative(root, f))
    .sort();

  return { veModules, orphans };
}

function main() {
  const { veModules, orphans } = orphanVeModules();

  // A gate that finds nothing looks identical to a clean repo. This one walks a
  // tree and matches on a stem, either of which could break silently, so refuse
  // to pass vacuously: there must be VE modules to check in the first place.
  if (veModules.length === 0) {
    console.error(
      'FAIL: found no *.css.ts modules under apps/desktop/src.\n' +
        'Either the tree moved or the walk is broken -- this gate is not ' +
        'measuring anything until that is fixed.',
    );
    process.exitCode = 1;
    return;
  }

  const unexpected = orphans.filter((f) => !ALLOWED_ORPHANS.has(f));
  if (unexpected.length === 0) {
    console.log(`OK: all ${veModules.length} vanilla-extract modules are imported.`);
    return;
  }

  console.error('FAIL: these vanilla-extract modules are imported by nothing:');
  for (const f of unexpected) console.error(`  ${f}`);
  console.error(
    '\nAn unimported *.css.ts emits no CSS, so whatever it was written to style' +
      '\nis silently unstyled -- no build error, no failing test.' +
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
if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}

export { ALLOWED_ORPHANS, orphanVeModules };
