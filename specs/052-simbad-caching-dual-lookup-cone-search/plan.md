# Implementation Plan: SIMBAD Resolver Caching, Dual-Lookup, and Cone-Search

**Branch**: `052-simbad-caching-dual-lookup-cone-search` | **Date**: 2026-07-12 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/052-simbad-caching-dual-lookup-cone-search/spec.md`

## Summary

Three phased, independently-shippable deliverables over the existing SIMBAD resolution stack (spec-035):

- **P1 (US1)** — Adopt the `simbad-resolver` **`SimbadResolver` facade** with a persistent redb **`CacheBackend::File`** (one global `<app_data>/simbad-cache.redb`, no TTL, manual clear); move durable persistence from persist-on-resolve to **persist-on-in-use** (supersedes spec-035 FR-006); warm the cache from the bundled seed + existing durable rows; replace the hand-rolled `cache.rs` search with `facade.search()`; enrich adopted targets with `magnitude` (from `ResolvedIdentity.v_mag`) and `constellation` (skymath 0.3); route every identity string through one **normalization choke-point**. Bump `simbad-resolver` 0.1.3 → 0.2.0 and fix the seed-builder V-magnitude gap.
- **P2 (US2)** — **Dual lookup**: TAP-first, Sesame name-resolver fallback only on a TAP miss, only on explicit resolve; oid recovery (TAP re-enrich → UUIDv5-from-designation); gated by the online-resolve setting.
- **P3 (US3)** — **Cone-search** at Inbox ingest, per light-frameset: derive pointing (WCS → mount → none, never filename), rotation-aware footprint + FOV via target-match 0.3, radius from optics (~1° default), top-N candidates, explicit confidence, nearest-to-centre primary with a niche-otype exclusion set. New contract + Tauri command + Inbox suggestion UI. **Suggested only** — confirm creates the durable link → in-use → persists `canonical_target`.

The approach reuses existing rails — the resolver crate and its `SimbadResolver` wrapper, `canonical_target` (with the already-present `magnitude`/`constellation` columns), the online-resolve gate, the Inbox confirm pipeline, and the pinned-but-unused `skymath`/`target-match` — rather than adding new crates or SQLite tables. No schema migration is required.

## Technical Context

**Language/Version**: Rust (workspace edition per `Cargo.toml`), TypeScript/React for the desktop shell.

**Primary Dependencies**: `simbad-resolver` **0.2.0** (facade + `CacheBackend::File` redb + `ResolvedIdentity.v_mag` + Sesame fallback), `skymath` 0.3 (IAU constellation-from-coords), `target-match` 0.3 (optics→FOV, rotation-aware footprint), `sqlx` (SQLite, canonical store), Tauri + tauri-specta (contract boundary).

**Storage**: SQLite `canonical_target` remains canonical (no migration — `magnitude`/`constellation` exist since `0047`). Resolve cache is redb, one global file in the app-data dir (outside SQLite).

**Testing**: per-crate `cargo test` (workspace baseline is known-red — validate per crate), integration tests under `tests/`, TS typecheck; real-app verification via `verify-on-windows` + a tauri-driver Layer-2 journey for the P3 Inbox suggestion.

**Target Platform**: Desktop (Windows primary dev target, plus macOS/Linux) via Tauri.

**Project Type**: Desktop app over granular Rust crates with a language-neutral contract boundary.

**Performance Goals**: Repeat search of a cached object issues zero network calls (SC-001); typeahead served locally; cone-search bounded to top-N.

**Constraints**: PixInsight boundary (no image processing); reviewable mutation (no silent target auto-apply — suggestions carry explicit confidence and require confirm); portable contracts + durable SQLite record; the redb cache is a reproducible projection, never canonical.

**Scale/Scope**: Resolution of individual objects and per-frameset cone-search; the seed catalogue is bundled.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|-----------|------------|
| I. Local-First File Custody | **PASS.** No image files touched; resolution and cone-search operate on metadata (coordinates, designations) only. The redb cache lives in the app-data dir and holds no raw/processed image data. |
| II. Reviewable Filesystem Mutation | **PASS (identity-linking analogue).** No filesystem mutation. Cone-search never silently assigns a target: every suggestion carries an explicit confidence (per §II's confidence requirement for inference) and becomes a durable link only on explicit user confirm. |
| III. PixInsight Boundary | **PASS.** No calibrate/debayer/register/integrate/edit; the feature resolves identities and suggests target links from existing plate-solve/mount metadata. Plate-solving itself is not performed — WCS is read, not computed. |
| IV. Research-Led Domain Modeling | **PASS.** Facade vs direct-TAP, cache backend, §V reconciliation, dual-lookup ordering, and normalization are decided with alternatives in `research.md`. The two genuinely-open P3 questions (catalogue-prominence ranking, default otype exclusion set) are documented as OQ-1/OQ-2 with proposed defaults, to be confirmed in P3 design before P3 implementation. |
| V. Portable Contracts & Durable Records | **PASS.** The new cone-search operation is a language-neutral contract (`crates/contracts/core` + `packages/contracts` + generated bindings). SQLite `canonical_target` is the durable record; the redb resolve cache is an explicitly reproducible projection (§V). |

**Result**: PASS (initial). Re-checked after Phase 1 design — still PASS. One item to close before P3 *implementation* (not before design): resolve OQ-1/OQ-2 (Principle IV gate for the cone-search modeling). P1/P2 have no open modeling questions. See Complexity Tracking (empty).

## Project Structure

### Documentation (this feature)

```text
specs/052-simbad-caching-dual-lookup-cone-search/
├── plan.md              # This file
├── research.md          # Phase 0 output (decisions + OQ-1/OQ-2)
├── data-model.md        # Phase 1 output
└── contracts/           # Phase 1 output (operations.md — P3 cone-search)
```

### Source Code (repository root)

```text
crates/
├── targeting/resolver/
│   ├── Cargo.toml            # simbad-resolver 0.1.3 → 0.2.0 (P1)
│   ├── simbad.rs             # refactor: direct TapResolver → SimbadResolver facade + CacheBackend::File (P1); Sesame fallback wiring (P2)
│   ├── cache.rs              # replace hand-rolled search_by_normalized/search_fuzzy with facade.search() (P1)
│   ├── seed.rs               # warm redb cache from bundled seed (P1)
│   └── lib.rs                # normalization choke-point routing; v_mag/constellation on ResolvedIdentity mapping (P1)
├── targeting/                # target-match 0.3 (FOV/footprint), skymath 0.3 (constellation) — already pinned (P1 constellation, P3 cone)
├── app/targets/
│   ├── target_resolve.rs     # in-use-gated canonical_target write (P1, supersede 035 FR-006); enrichment on adopt (P1)
│   └── (cone-search orchestration for P3: pointing derivation + candidate ranking)
├── app/inbox/                # per-light-frameset cone-search hook at confirm; suggestion carried to UI (P3)
├── contracts/core/           # cone-search + Inbox target-suggestion DTOs (P3 only)
└── tools/seed-builder/src/main.rs  # parse_basic_row 5→6-tuple + f.V + LEFT OUTER JOIN allfluxes (P1)

apps/desktop/
├── src-tauri/src/lib.rs                 # build SimbadResolver facade w/ File backend at startup (P1); register cone-search command (P3)
├── src-tauri/src/commands/target_lookup.rs  # facade construction; "clear resolve cache" command (P1)
├── src-tauri/src/commands/               # new cone-search command (P3)
└── src/features/inbox/                    # target-suggestion UI in the confirm gate (P3)
packages/contracts/                        # generated TS surface for the P3 cone-search operation
tests/                                     # cache-persist-across-restart, in-use-gate, dual-lookup, cone-search suggestion
```

**Structure Decision**: Extend existing crates; add **no** new crate and **no** SQLite table. P1/P2 are internal to the resolver + app-targets + seed-builder and add **no new contract**. P3 adds exactly one new contract/command/UI surface. This follows the crate-split-by-domain rule and keeps pure-domain crates free of new cross-crate deps.

## Phase 0 — Research

See [research.md](./research.md). Decisions: D1 facade vs direct-TAP; D2 `CacheBackend::File` (redb) vs InMemory vs moka (moka dropped); D3 §V reconciliation (redb projection, SQLite canonical); D4 in-use-gated persistence (supersedes 035 FR-006); D5 TAP-first/Sesame-fallback + oid recovery; D6 normalization choke-point; D7 0.2.0 bump + seed-builder V-mag fix; D8 constellation via skymath; D9 cone-search building blocks. Open: OQ-1 catalogue-prominence ranking, OQ-2 default otype exclusion set (proposed defaults given; confirm in P3 design).

## Phase 1 — Design

See [data-model.md](./data-model.md) and [contracts/operations.md](./contracts/operations.md). P1/P2 add no new contract (documented in contracts). P3 adds the cone-search command + Inbox target-suggestion DTO.

## Phased delivery

| Phase | User Story | Ships | New contract? |
|-------|-----------|-------|---------------|
| P1 | US1 | facade + persistent redb cache + in-use persistence + 0.2.0 bump + seed-builder fix + magnitude/constellation enrichment + normalization choke-point | No |
| P2 | US2 | TAP-first / Sesame-fallback dual lookup + oid recovery | No |
| P3 | US3 | cone-search suggestion at Inbox ingest (contract + command + UI) | Yes |

## Complexity Tracking

> No constitution violations. Table intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
