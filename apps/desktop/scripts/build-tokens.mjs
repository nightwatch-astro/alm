#!/usr/bin/env node
// build-tokens.mjs — generate tokens.css from the DTCG sources in tokens/.
//
// Architecture (the standard three-tier model):
//   tier 1  tokens/themes/<id>.json  raw palette, one file per theme, each
//                                    emitted as its own [data-theme] block
//   tier 2  tokens/semantic.json     aliases + theme-invariant metrics, :root
//   tier 3  tokens/component/*.json  per-component geometry, also :root. These
//                                    are literals, not aliases: a semantic token
//                                    resolves against the active theme, whereas
//                                    a component's px metrics do not vary by
//                                    theme at all. No component COLOUR tokens
//                                    exist -- nothing varies per-component
//                                    per-theme, so there is none to express.
//
// Theme-invariant type/space/radius live in packages/tokens/foundation.css,
// which is shared with the docs repo and is NOT generated here — tokens.css
// @imports it via prelude.css.
//
// `outputReferences: true` is load-bearing: the semantic tier must emit
// `var(--pv-ink)` rather than a resolved literal, or every theme would inherit
// Warm Slate's palette instead of overriding it.
//
// Usage: node apps/desktop/scripts/build-tokens.mjs [--check]
//   --check  regenerate into memory and diff against the committed file,
//            exiting 1 on drift (used by CI)

import StyleDictionary from 'style-dictionary';
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, basename, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
// Reused from the standalone type generator rather than reimplemented, so the
// emitted tokens.d.ts stays byte-identical and its drift test keeps passing.
import { extractTokenNames, renderTokenTypesDts } from './gen-token-types.mjs';

// Paths resolve from this file, not the cwd: CI invokes it via
// `pnpm --filter @astro-plan/desktop run lint`, which runs from apps/desktop,
// while a developer runs it from the repo root. Both must work.
const APP_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const TOKENS_DIR = join(APP_DIR, 'tokens');
const OUT = join(APP_DIR, 'src/styles/tokens.css');
const PRELUDE = join(TOKENS_DIR, 'prelude.css');
const TYPES_OUT = join(APP_DIR, 'src/styles/tokens.d.ts');

// The docs site shares this app's non-color foundation but keeps its own
// palette (#1153), so it gets a purpose-built output rather than a slice of
// the app's stylesheet. The docs repo vendors this file's CONTENT into its own
// src/styles/theme.css, so renaming it here costs that repo nothing.
const DOCS_OUT = resolve(APP_DIR, '../../packages/tokens/tokens-docs.css');

// Warm Slate doubles as the no-data-theme :root default so the default light
// palette (pre-hydration, vitest) has a single source of truth without
// competing in the cascade when another named theme is set on <html>.
const DEFAULT_THEME = 'warm-slate';

const themeIds = readdirSync(join(TOKENS_DIR, 'themes'))
  .filter((f) => f.endsWith('.json'))
  .map((f) => basename(f, '.json'))
  .sort();

/**
 * Which source file a token came from — used to split the tiers apart.
 * Paths are normalised to forward slashes first: on Windows `filePath` is
 * `...\density\compact.json`, so matching a literal `density/compact.json`
 * selects NOTHING and the block emits empty — valid CSS, silently missing
 * every density and theme token.
 */
const posix = (p) => (p ?? '').split('\\').join('/');
const fromFile = (suffix) => (token) => posix(token.filePath).endsWith(suffix);
const inDir = (dir) => (token) => posix(token.filePath).includes(`/${dir}/`);

// Uses SD's built-in `css/variables` format rather than a hand-written one.
// A custom format that reads `token.$value` directly bypasses outputReferences
// and emits resolved literals — which flattens the semantic tier to Warm
// Slate's palette, so every other theme silently stops applying.
async function buildBlock({ sources, selector, include, outputReferences = true }) {
  const sd = new StyleDictionary({
    source: sources,
    platforms: {
      css: {
        transformGroup: 'css',
        prefix: 'pv',
        files: [
          {
            destination: 'block.css',
            format: 'css/variables',
            filter: include,
            options: { selector, outputReferences },
          },
        ],
      },
    },
    log: { verbosity: 'silent', warnings: 'disabled' },
  });
  const built = await sd.formatPlatform('css');
  const output = built[0].output;

  // An empty block is syntactically valid CSS, so a filter that selects nothing
  // produces a file that looks fine and silently drops a whole tier or theme.
  // That is exactly how the Windows path-separator bug got through review: the
  // filters matched nothing there, every theme block emitted empty, and only
  // the byte-level drift gate on a Windows runner noticed.
  if (!/--pv-[a-z0-9-]+\s*:/.test(output)) {
    console.error(
      `ERROR: block "${selector}" emitted no declarations — its source filter ` +
        `matched none of:\n  ${sources.join('\n  ')}`,
    );
    process.exit(1);
  }
  return output;
}

// The semantic tier references raw palette tokens, so each build must also
// load a theme for those references to resolve — the `include` filter then
// keeps only the tier being emitted.
const semanticSrc = join(TOKENS_DIR, 'semantic.json');
const foundationSrc = join(TOKENS_DIR, 'foundation.json');
const themeSrc = (id) => join(TOKENS_DIR, 'themes', `${id}.json`);

// #1153 guardrail, now executable instead of conventional: the foundation is
// what the app and the docs site SHARE, and they keep deliberately divergent
// palettes. A colour smuggled in here would ship the app's palette into the
// docs build. Previously this was guaranteed only by the file boundary.
{
  const foundation = JSON.parse(readFileSync(foundationSrc, 'utf8'));
  const colours = Object.entries(foundation)
    .filter(([, t]) => t.$type === 'color' || /^#|^rgba?\(/i.test(String(t.$value)))
    .map(([k]) => k);
  if (colours.length > 0) {
    console.error(
      `ERROR: colour tokens in the shared foundation: ${colours.join(', ')}.\n` +
        'The docs site vendors this tier and keeps its own palette (#1153).',
    );
    process.exit(1);
  }
}

const blocks = [];

// Tier 0 (foundation) + tier 2 + tier 3 all land in :root — they differ in
// ownership, not scope. Foundation is inlined rather than @imported so the
// stylesheet is self-contained and the docs output can be emitted separately.
const componentSrcs = readdirSync(join(TOKENS_DIR, 'component'))
  .filter((f) => f.endsWith('.json'))
  .sort()
  .map((f) => join(TOKENS_DIR, 'component', f));

blocks.push(
  await buildBlock({
    sources: [foundationSrc, semanticSrc, ...componentSrcs, themeSrc(DEFAULT_THEME)],
    selector: ':root',
    include: (token) =>
      fromFile('foundation.json')(token) ||
      fromFile('semantic.json')(token) ||
      inDir('component')(token),
  }),
);

for (const id of themeIds) {
  const selector =
    id === DEFAULT_THEME
      ? `:root:not([data-theme]),\n[data-theme="${id}"]`
      : `[data-theme="${id}"]`;
  blocks.push(
    await buildBlock({
      sources: [semanticSrc, themeSrc(id)],
      selector,
      include: fromFile(`${id}.json`),
    }),
  );
}

const densityIds = readdirSync(join(TOKENS_DIR, 'density'))
  .filter((f) => f.endsWith('.json'))
  .map((f) => basename(f, '.json'))
  .sort();

// Density is a third axis, orthogonal to theme — it overrides row metrics only.
for (const id of densityIds) {
  blocks.push(
    await buildBlock({
      sources: [semanticSrc, themeSrc(DEFAULT_THEME), join(TOKENS_DIR, 'density', `${id}.json`)],
      selector: `.density-${id}`,
      include: fromFile(`density/${id}.json`),
    }),
  );
}

// Component parameters: emitted as @property registrations rather than plain
// declarations. Two things this buys that a fallback cannot:
//
//   - `initial-value` IS the default, so `var(--pv-skel-h)` needs no fallback.
//     A fallback hides a broken reference behind a value that renders fine; an
//     unregistered property with no fallback fails visibly instead.
//   - The name becomes KNOWN to stylelint's no-unknown-custom-properties, which
//     otherwise cannot see it — that rule treats any reference with a
//     resolvable fallback as fine.
//
// `initial-value` cannot contain var(), so it must be a literal. That is why
// this block is generated with outputReferences:false: Style Dictionary
// resolves `{sp-3}` to its value here, so the default cannot drift from the
// token it is defined against.
const propertiesSrc = join(TOKENS_DIR, 'properties.json');
const propertyMeta = JSON.parse(readFileSync(propertiesSrc, 'utf8'));
const resolvedParams = await buildBlock({
  sources: [foundationSrc, semanticSrc, themeSrc(DEFAULT_THEME), propertiesSrc],
  selector: ':root',
  include: fromFile('properties.json'),
  outputReferences: false,
});

const propertyRules = [...resolvedParams.matchAll(/--pv-([a-z0-9-]+):\s*([^;]+);/g)].map(
  ([, name, value]) => {
    const meta = propertyMeta[name]?.$extensions?.['pv.property'];
    if (!meta) {
      console.error(`ERROR: no pv.property metadata for --pv-${name} in properties.json`);
      process.exit(1);
    }
    return (
      `@property --pv-${name} {\n` +
      `  syntax: "${meta.syntax}";\n` +
      `  inherits: ${meta.inherits};\n` +
      `  initial-value: ${value.trim()};\n` +
      `}`
    );
  },
);
if (propertyRules.length !== Object.keys(propertyMeta).filter((k) => !k.startsWith('$')).length) {
  console.error('ERROR: @property count does not match properties.json — a parameter was dropped.');
  process.exit(1);
}
// Trailing newline: this block is emitted last, and without it the file ends
// unterminated. The end-of-file pre-commit hook then "fixes" the committed
// artifact, which no longer matches what the generator produces — the drift
// gate fails on a file nobody edited by hand.
blocks.push(`${propertyRules.join('\n')}\n`);

const banner =
  '/* AUTO-GENERATED by scripts/build-tokens.mjs from apps/desktop/tokens/ —\n' +
  '   do not edit by hand. Run `pnpm tokens:build` after editing the DTCG\n' +
  '   sources. CI fails if this file drifts from them. */\n';

const output = `${banner}${readFileSync(PRELUDE, 'utf8')}\n${blocks.join('\n')}`;

// The docs site's share: the foundation tier alone, no palette.
const docsBanner =
  '/* AUTO-GENERATED by the app\'s scripts/build-tokens.mjs — do not edit by hand.\n' +
  '   The non-color `--pv-*` foundation shared with the app. The docs repo\n' +
  '   vendors this content into src/styles/theme.css; colors are deliberately\n' +
  '   absent so each surface keeps its own palette (#1153). */\n';

const docsOutput = `${docsBanner}${await buildBlock({
  sources: [foundationSrc],
  selector: ':root',
  include: fromFile('foundation.json'),
})}`;

// tokens.d.ts is derived from the CSS this same run produced, so the type and
// the stylesheet cannot disagree — they are written from one in-memory build
// rather than by two scripts that might run at different times.
const typesOutput = renderTokenTypesDts(extractTokenNames([output, docsOutput]));

// Every token that ships in the CSS must exist in the type. Today this holds by
// construction, because the names are extracted FROM the emitted CSS — which
// also means a `--pv-*` hand-added to a non-generated import (prelude.css)
// is picked up rather than missed. The assertion is here so that stays true if
// anyone later switches to enumerating the token tree instead, where the union
// across every tier and theme becomes an assumption rather than a consequence.
{
  const inCss = new Set(extractTokenNames([output, docsOutput]));
  const inTypes = new Set([...typesOutput.matchAll(/'(pv-[A-Za-z0-9_-]+)'/g)].map((m) => m[1]));
  const missing = [...inCss].filter((n) => !inTypes.has(n));
  if (missing.length > 0) {
    console.error(`ERROR: tokens in the CSS but absent from tokens.d.ts: ${missing.join(', ')}`);
    process.exit(1);
  }
}

const artifacts = [
  [OUT, output],
  [DOCS_OUT, docsOutput],
  [TYPES_OUT, typesOutput],
];

// Compare content, not line endings. `.gitattributes` pins these files to LF,
// but a checkout predating that rule — or any autocrlf setting — otherwise makes
// this gate fail on Windows ONLY, reporting a file as "out of date" and telling
// you to rebuild something already byte-correct. A gate that lies gets ignored.
const sameContent = (a, b) => a.split('\r\n').join('\n') === b.split('\r\n').join('\n');

if (process.argv.includes('--check')) {
  const stale = artifacts.filter(([path, want]) => !sameContent(readFileSync(path, 'utf8'), want));
  if (stale.length > 0) {
    console.error(
      `ERROR: out of date with apps/desktop/tokens/:\n  ${stale.map(([p]) => p).join('\n  ')}\n` +
        'Run `pnpm tokens:build` and commit the result.',
    );
    process.exit(1);
  }
  console.log(
    `OK: ${artifacts.length} generated artifacts match their DTCG sources ` +
      `(${themeIds.length} themes).`,
  );
} else {
  for (const [path, contents] of artifacts) writeFileSync(path, contents);
  console.log(
    `wrote ${artifacts.length} artifacts — ${themeIds.length} themes, ` +
      `foundation + semantic + component tiers`,
  );
}
