# Implementation Plan: Retire Legacy Target Tables

**Branch**: `036-retire-legacy-targets` | **Date**: 2026-06-19 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/036-retire-legacy-targets/spec.md`

## Summary

Delete the two legacy target generations (gen-1 `target`; gen-2 spec-013/023 `targets`,
`target_aliases`, `target_catalog_refs`, `catalog_equivalences` + their FK columns) and
all code that touches them, then rebuild the live Targets management page on the spec-035
`canonical_target` / `target_alias` model. Add a presentation-only **display alias** and
**user-added aliases** to the gen-3 model. Greenfield: legacy schema is removed by
editing the source migrations directly (no drop migration, no data backfill).

## Technical Context

**Language/Version**: Rust 1.75+ (workspace crates), TypeScript 5 / React 19 (desktop).

**Primary Dependencies**: sqlx (SQLite), tauri 2 + tauri-specta, TanStack Router, Base UI.

**Storage**: SQLite (single local DB). Target store after this change: `canonical_target`
+ `target_alias` only (spec-035, migration 0031), plus `projects.canonical_target_id`.

**Testing**: `cargo test --workspace`, `vitest` (desktop), contract bindings test.

**Target Platform**: Local desktop (Tauri) on Windows/macOS/Linux.

**Project Type**: Desktop app — Rust core crates + React frontend via Tauri commands.

**Performance Goals**: Target detail/alias actions feel instant (local SQLite, < 50 ms).

**Constraints**: Greenfield — no migration/backfill; legacy columns are simply never
created (source migrations edited), so no SQLite table-rebuild dance is needed.

**Scale/Scope**: ~single-digit thousands of canonical targets (seed + resolved); the
Targets list is a local query.

## Constitution Check

*GATE: must pass before Phase 0 and re-checked after Phase 1.*

- **I. Local-First File Custody** — PASS. Target identity is metadata only; no image files
  touched. Deleting legacy tables removes metadata rows only.
- **II. Reviewable Filesystem Mutation** — N/A (no filesystem plans). Display alias /
  alias edits are explicit user actions with clear outcomes; no inference, no confidence.
- **III. PixInsight Boundary** — PASS. No processing behaviour.
- **IV. Research-Led Domain Modeling** — PASS. Display-alias storage + user-alias `kind`
  are decided in research.md; the SIMBAD-canonical rule is the documented default.
- **V. Portable Contracts & Durable Records** — PASS. New target-management operations are
  language-neutral JSON-schema contracts; SQLite stays the durable record; display alias
  is a user-owned column (durable), SIMBAD responses remain reproducible projections.

**Post-Phase-1 re-check**: still PASS — design adds only metadata columns/commands within
the existing spec-035 model; no new external surface beyond documented contracts.

## Architecture & Approach

### A. Schema deletion (greenfield — edit source migrations)

- **Gen-1 (0002) is DEFERRED** (user decision 2026-06-19): removing the singular `target`
  table is entangled with the dormant original schema generation (singular `project`,
  `catalog_equivalence`, NOT-NULL FK chains). Leave the gen-1 objects untouched; this
  spec only ensures gen-1 `target` has no live reader (drop the inventory join, §B). The
  original-generation cleanup is a separate future spec.

  Reconciled 2026-08-24: the numbered migration chain named throughout this plan no longer
  exists. Every schema object now lives in the single editable baseline
  `crates/persistence/core/migrations/0001_initial_schema.sql`, per decision bead `adr-1`; gen-1
  `target` is at `:2914` and `acquisition_session.target_id` at `:28`. The gen-2 tables this
  spec removed (`targets`, `target_aliases`, `target_catalog_refs`, `catalog_equivalences`)
  are absent from the baseline. Editing the baseline in place changes the version-1 checksum,
  so every existing development database fails to open and must be recreated — see
  `docs/development/database-baseline-and-migrations.md`.
- Delete `0017_targets.sql` entirely (whole file is gen-2 target schema).
- Delete `0027_target_identity.sql` entirely (whole file is gen-2 target extensions: the
  `targets.notes`/`updated_at` ALTERs, `target_aliases`, and the FK columns
  `acquisition_session.acq_target_id`, `projects.target_id`, `project_sources.target_id`
  + indexes).
- Verify no later migration references the removed objects (0031/0033 use only gen-3).
- **Display alias + user alias** added to gen-3 by editing `0031_target_resolution.sql`
  (greenfield): add nullable `canonical_target.display_alias TEXT`; extend
  `target_alias.kind` CHECK to include `'user'` so user-added aliases are distinguishable
  from SIMBAD-derived ones (only `'user'` aliases are removable).

### B. Dead-code deletion

- `crates/persistence/db/src/repositories/targets.rs` — delete (gen-2 repo).
- `crates/targeting/src/load.rs` — delete (gen-2 catalog load) + its module wiring.
- `crates/app/core/src/target_identity.rs` — delete/replace (gen-2 use-cases).
- Spec-013 commands `target.lookup` / `target.resolve.fits` in
  `apps/desktop/src-tauri/src/commands/target_lookup.rs` — delete (spec-035
  `target.search`/`target.resolve` supersede them).
- `crates/persistence/db/src/repositories/inventory.rs` — remove the
  `LEFT JOIN target ...` for `target_name` (always empty today); keep the projection
  shape, emitting `NULL`/absent target name.
- Remove gen-2 contract DTOs (`crates/contracts/core/src/targets.rs` gen-2 shapes) and
  any frontend types/fixtures referencing them.

### C. Gen-3 target-management surface (rebuild)

New app_core use-case module (e.g. `crates/app/core/src/target_management.rs`) + repo
functions on `canonical_target`/`target_alias`, exposed via Tauri commands that **reuse
the existing command names** (so the frontend invoke targets are unchanged) repointed to
gen-3:

- `target.get` → canonical target detail: id, primary designation, display alias (if
  set), effective display label, object type, ra/dec, alias list (with `kind`).
- `target.list` → list canonical targets for the Targets list pane (id + effective label
  + object type), ordered by primary designation; simple local query.
- `target.alias.add` → insert a `kind='user'` alias (normalized; rejects duplicates).
- `target.alias.remove` → delete a `kind='user'` alias (only user aliases removable).
- `target.display_alias.set` → set `canonical_target.display_alias`.
- `target.display_alias.clear` → null `canonical_target.display_alias`.

Removed commands: `target.note.update`, `target.primary.rename`.

Repo additions (in `crates/targeting/src/resolver/cache.rs` or a sibling): `list_all`,
`insert_user_alias`, `delete_user_alias`, `set_display_alias`, `clear_display_alias`,
and ensure `upsert_resolved` **preserves** `display_alias` on conflict (never overwrites
the user value on re-resolution — FR-012).

### D. Frontend rebuild

- `apps/desktop/src/features/targets/TargetDetailV2.tsx` — repoint to the gen-3
  `target.get`; show effective display label + canonical designation, object type,
  coordinates, alias list; alias add/remove; **display-alias set/clear control**; remove
  the note box and the primary-rename control.
- `TargetList.tsx` / `TargetsPage.tsx` — list from `target.list`.
- Regenerate tauri-specta bindings; update commands wrapper + types.
- Cmd+K target search and `/targets`, `/targets/$id` routing stay; only the backing
  data/commands change.

### E. Tests

Replace gen-2 tests (target_identity, targets repo, load) with gen-3 equivalents; update
contract/bindings tests; update e2e/component tests that referenced removed commands.
All on a fresh DB.

## Project Structure

### Documentation (this feature)

```text
specs/036-retire-legacy-targets/
├── plan.md          # this file
├── research.md      # Phase 0 — display-alias storage, user-alias kind, deletion order
├── data-model.md    # Phase 1 — gen-3 additions + removed objects
├── quickstart.md    # Phase 1 — manual verification walkthrough
├── contracts/       # Phase 1 — target.get/list/alias.*/display_alias.* schemas
└── tasks.md         # Phase 2 (/speckit-tasks)
```

### Source code (impacted)

```text
crates/persistence/db/migrations/   # edit 0002, 0031; delete 0017, 0027
crates/persistence/db/src/repositories/{targets.rs(del),inventory.rs,...}
crates/targeting/src/resolver/cache.rs        # + list/alias/display-alias fns
crates/targeting/src/load.rs                  # delete
crates/app/core/src/{target_identity.rs(del)→target_management.rs}
crates/contracts/core/src/targets.rs          # gen-2 DTOs out, gen-3 mgmt DTOs in
apps/desktop/src-tauri/src/commands/{target_identity.rs(del)→target_management.rs, target_lookup.rs}
apps/desktop/src/features/targets/{TargetDetailV2,TargetList,TargetsPage}.tsx
apps/desktop/src/bindings/index.ts            # regenerated
```

## Phasing (implementation order — dependency-safe)

1. **Schema**: edit migrations (0002/0031 edits, delete 0017/0027); fresh-DB migrate +
   `cargo build -p persistence_db`.
2. **Repo + use-cases**: add gen-3 management repo fns + `target_management` use-cases;
   delete gen-2 repo/load/use-cases.
3. **Contracts + commands**: gen-3 DTOs; repoint/rename commands; regenerate bindings.
4. **Inventory join** removal.
5. **Frontend**: rebuild Targets page on gen-3 + display-alias control.
6. **Tests**: replace gen-2 tests; full gate (fmt/clippy/test/typecheck/vitest/bindings).

Each phase builds + tests before the next.

## Complexity Tracking

No constitution deviations. The only non-trivial design choices (display-alias storage,
user-alias `kind`, migration-edit vs drop-migration) are resolved in research.md.
