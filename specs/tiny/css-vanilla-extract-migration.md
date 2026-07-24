# TinySpec: Component CSS migration to vanilla-extract

**Branch**: phased — `css/ve-foundation` (PR #1545) first, then per-sheet branches
**Date**: 2026-07-24
**Status**: ratified (user decision 2026-07-24; bake-off evidence: bead astro-plan-kyo7.108, branch `pilot/css-vanilla-extract`)
**Complexity**: medium (phased; each PR individually small)

## What

Replace the ~10.4k lines of hand-written global BEM CSS in
`apps/desktop/src/styles/components/` with typed, build-time-extracted
vanilla-extract styles. The DTCG JSON in `apps/desktop/tokens/` stays the
single human-edited token source; a generator emits the vanilla-extract theme
contract instead of (eventually, as well as) `tokens.css`. Chosen over Panda
CSS in a two-branch bake-off on LogPanel: simpler mental model, 1:1 devtools
class names, one Vite plugin vs codegen+aliases, `styleVariants` beats recipe
ceremony for modifier classes.

## Context

| File | Role |
|------|------|
| `apps/desktop/tokens/` (DTCG JSON) | Source of truth — unchanged |
| `apps/desktop/scripts/gen-ve-themes.mjs` | Generator (PR #1545) — DTCG → `themes.css.ts`, `--check` drift mode in CI |
| `apps/desktop/src/styles/themes.css.ts` | Generated: `createThemeContract` + 6× `createGlobalTheme('[data-theme=…]')` |
| `apps/desktop/scripts/build-tokens.mjs` | Coexists during Phase 2; retired in Phase 3 |
| `apps/desktop/src/styles/tokens.css` | Coexists during Phase 2; deleted in Phase 3 |
| `apps/desktop/src/styles/components/*.css` (18 remaining sheets) | Dissolved per-sheet to co-located `.css.ts`, then deleted |
| `apps/desktop/src/styles/components.css` | Barrel with load-bearing import order — shrinks per sheet, deleted in Phase 3 |
| `docs/development/css-vanilla-extract.md` | Authoring conventions (PR #1545) |

## Requirements

1. All 6 themes remain runtime-switchable via the `data-theme` attribute on
   `<html>` at every commit — no build-time-only theming.
2. `themes.css.ts` is generated, never hand-edited; CI fails on drift from the
   DTCG sources (`--check` mode, same pattern as `build-tokens.mjs`).
3. New `.css.ts` files reference tokens via the imported `vars` contract —
   no raw `var(--pv-*)` strings (typo-safe by construction).
4. Modifier classes use `styleVariants` keyed on the relevant union type.
5. **Three-layer architecture — one consumer per file** (ratified 2026-07-24,
   prevents god style files):
   - **Tokens**: the generated `themes.css.ts`. Machine-owned, size-exempt.
   - **Primitives**: one `.css.ts` per UI primitive, co-located
     (`src/ui/Btn.tsx` + `btn.css.ts`).
   - **Components**: one `.css.ts` per component, co-located; the feature
     directory expresses the domain grouping.
   A hand-written `.css.ts` has **exactly one consumer**. A style needed by
   two components moves *up* to a primitive or *down* to a token — no
   shared-styles third bucket. Migrating a multi-component sheet means
   dissolving it across its consumers in the same PR, never porting it 1:1.
   Line-count ratchet: warn > 200, fail > 400 lines per hand-written
   `.css.ts` (generated file exempt) — wired in Phase 3.
6. Each migrated sheet's old CSS file is deleted in the same PR; unmigrated
   dead classes die with their sheet (completes the deferred ~119 dead-class
   sweep).
7. Both systems coexist during Phase 2; the ~8 KB theme-var duplication is
   accepted temporarily and MUST be eliminated in Phase 3.
8. e2e tests select by `data-testid` only (PR #1542); component tests MUST
   NOT assert on generated class names.
9. No rc/beta deps (`@vanilla-extract/css` and the Vite plugin are stable).

## Plan

**Phase 1 — foundation (DONE, PR #1545, approved)**: generator + drift CI +
LogPanel reference migration + testid test fixes + conventions doc.

**Phase 2 — per-sheet dissolution (~17 PRs, three waves; beads kyo7.109.1-3)**
- Wave 1 (kyo7.109.1, in flight): wizard-base, modals, skeleton, dev,
  app-shell, sessions.
- Wave 2 (kyo7.109.2): projects, targets, target-search, plan-panel,
  tables-lists, detail-panes, primitives (primitives split per-widget).
- Wave 3 (kyo7.109.3, coordinated): settings + settings-detail (ride kyo7.25
  RHF), inbox (after kyo7.43), feature-lists, redesign-detail, wizard-steps
  (2,008L, last).
- Per sheet: rules → co-located `.css.ts` files per req. 5, className
  strings → imports, delete sheet + barrel import, surface tests green.

**Phase 3 — retirement (closing PR, part of kyo7.109.3)**
1. Delete `tokens.css` emission; drift CI keeps guarding `themes.css.ts`.
2. Delete the `components.css` barrel; cascade order ceases to exist.
3. CSS entry in `.size-limit.json`; verify net bundle ≤ pre-migration.
4. Ratchet script: no raw `var(--pv-*)` in `.css.ts` + the req. 5 line-count
   check (warn 200 / fail 400, generated exempt).

## Done When

- [ ] `src/styles/components/` is empty and deleted
- [ ] `tokens.css` gone; `themes.css.ts` sole token surface; drift CI green
- [ ] Every hand-written `.css.ts` has exactly one consumer and ≤ 400 lines
- [ ] All 6 themes verified switchable (Playwright theme toggle)
- [ ] Bundle CSS ≤ pre-migration size; size-limit entries ratcheted
- [ ] No `var(--pv-*)` string literals in any `.css.ts`
