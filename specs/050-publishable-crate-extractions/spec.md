# Feature Specification: Publishable Crate Extraction Program

**Feature Branch**: `050-publishable-crate-extractions`

**Created**: 2026-07-04

**Status**: Implemented (post-hoc record, verified 2026-07-28). All four crates
are extracted and published, and the workspace consumes them as versioned
external dependencies: `fits-header 0.4.2` (`crates/metadata/fits/Cargo.toml`),
`xisf-header 0.4.3` (`crates/metadata/xisf/Cargo.toml`), `skymath 0.7.1` and
`target-match 0.5.1` (`crates/targeting/Cargo.toml`). This header still read
"no implementation yet" until the 2026-07-28 spec audit corrected it.

**Input**: User description: "Record the approved publishable-crate extraction
program — four crates identified by the crate-audit (2026-07-04) as
zero/near-zero-dep, non-app-shaped, and differentiated enough to justify
publishing outside this monorepo."

## Why This Is A Mini Spec, Not A Full SDD Run

This is a **plan-of-record**, not a feature to implement yet. It exists so the
decisions below (crate boundaries, what's in/out, gating) don't have to be
re-litigated when each extraction is actually picked up. No code changes ship
under this spec — each extraction gets its own follow-up work (new repo setup,
CI, release-please) when it starts, referencing this document. `specs/tiny/`
was considered and rejected as the home for this: tinyspec is scoped to single
code changes under ~1 hour touching a handful of files, whereas this spans four
future extractions across four future repos with no code change here at all.

## Shared Plan Facts (apply to all four)

- Each extraction becomes a **separate new GitHub repo**, not a workspace member
  kept in-tree.
- Each new repo gets its **own CI** and its **own release-please** config,
  using plain `v{version}` tags (matching this repo's tag convention — no
  component-prefixed tags).
- **Gating**: all four are blocked on PR #349 (`feat(spec-043): UI redesign +
  CSS split + shared SortHeader`) landing on `main`. Work for any extraction
  branches off `main` only after #349 merges, not before.
- Source analysis: the campaign's crate-audit (2026-07-04) surveyed all crates
  in this workspace for publishability; the four below are what it approved.
- The extensible prefix-table API for `astro-target-id` (user-supplied catalog
  prefixes) is a **v1 API requirement**, not deferred polish.

## Extraction 1 — `fits-header` (new repo)

**What**: Complete typed FITS header extraction — every card as a typed value
(logical/int/real/string), `COMMENT`/`HISTORY`, `CONTINUE` long-string
handling, per-HDU. Not the current cherry-picked keyword subset that
`crates/metadata/fits` extracts today.

**API shape** (clarified 2026-07-04): the crate MUST support all three access
patterns, not just one:
1. **Extract all headers** for an HDU as a clear typed object (a map/struct
   of every card, typed).
2. **Extract one specific header** by keyword, without paying for or
   materializing the rest.
3. **List all header keywords** present (with enough info — keyword, HDU
   index, type — to iterate and let a caller select which ones to fetch),
   so a caller can enumerate before deciding what to pull.

These three are complementary views over the same parsed representation, not
three separate parsers — the "list" and "get one" operations are cheap
lookups/iterators over the same typed-card structure "extract all" returns.

**Why publishable**: Ecosystem gap — existing FITS crates (e.g. `fitsrs`) focus
on array/image data access; there's no lightweight, zero-dependency,
panic-free, *header-only* typed extraction crate. README should position this
crate explicitly against `fitsrs` (what it does instead of, not on top of).

**Boundary**:
- New crate is zero-dep and panic-free; vendor the small parsing helpers it
  needs (`parse_f64`, sexagesimal parsing) instead of taking a dependency, so
  the published crate stays zero-dep.
- Header-only — no image/pixel data handling.
- `crates/metadata/fits` in this monorepo becomes a thin internal adapter over
  the new published crate: it stays responsible for mapping typed headers to
  this app's `FrameType`/`RawFileMetadata` domain types, which remain app-side
  and do not move to the published crate.

**Effort**: Moderate — the full typed-card model, the three access patterns
above, and `CONTINUE`/long-string handling are new work beyond today's subset;
vendoring the two helper functions is small.

**Gating**: PR #349 on `main`, then its own new-repo bootstrap.

## Extraction 2 — `xisf-header` (new repo)

**What**: Same pattern as `fits-header`, for XISF: full XML header parsed into
typed properties, not a keyword subset.

**API shape**: mirrors `fits-header` — extract-all-as-typed-object,
extract-one-property-by-name, and list-all-properties-for-iteration/selection,
over one shared parsed representation.

**Why publishable**: No crates.io prior art for XISF header parsing at all —
this is the highest-differentiation extraction of the four.

**Boundary**: Header/property-only, zero-dep, panic-free, mirrors
`fits-header`'s shape so the two crates read as a matched pair. This repo's
`crates/metadata/xisf` becomes the thin internal adapter, same division of
responsibility as `fits-header`/`crates/metadata/fits`.

**Effort**: Moderate-to-large — no existing subset to start from; full XML
property model plus the three access patterns is new work.

**Gating**: PR #349 on `main`, then its own new-repo bootstrap.

## Extraction 3 — `astro-target-id` (from `crates/targeting`)

**What**: Astronomy-designation normalizer (NFKC/casefold/strip/collapse) with
an **extensible** prefix table (Messier, NGC, IC, Sh2, B, vdB, LDN, LBN, Mel,
Caldwell, Arp), deterministic UUIDv5 target identity, and a pure coordinate kit
(`angular_separation_deg`, `fov_radius_deg` from optics, `rank_candidates`).

**Why publishable**: Already has zero internal (in-workspace) dependencies.
Deterministic cross-tool target identity plus a pure coordinate-matching kit
is generically useful outside this app.

**Boundary**:
- Work is doc scrub + making the prefix table's API explicitly extensible
  (callers can register additional catalog prefixes, not just the ones listed
  above), not a functional rewrite.
- README should note the input synergy with `fits-header`: the FITS header
  crate's typed output (`RA`, `DEC`, `FOCALLEN`, `XPIXSZ`, `NAXIS`) is exactly
  the input shape the coordinate kit consumes — the two crates are designed to
  compose.

**Catalog supply boundary — the three-layer stack** (user directive
2026-07-04, corrected same day):

The published offering is a **three-layer stack** (user correction 2026-07-04;
supersedes an earlier recorded default of not publishing the seed):

- **(a) `astro-target-id` — the kernel.** Pure math/identity: normalization,
  UUIDv5 identity, coordinate matching. No data, no I/O. Its matching APIs
  take `&[TargetCoord]` — the kernel never acquires a catalog and never
  resolves over the network.
- **(b) `astro-seed-catalog` — the data layer.** The bundled seed (`seed.json`,
  built by `crates/tools/seed-builder` from SIMBAD TAP + OpenNGC: Messier,
  Caldwell, and an NGC/IC slice, with aliases and coordinates) IS published as
  a data crate, **gated only on license verification**: SIMBAD/CDS attribution
  requirements and the OpenNGC license MUST be verified before any publish.
  If licensing forbids redistribution, the fallback is publishing the
  seed-**builder** tooling instead, so consumers generate their own seed.
  - **New work item — seed ingestion API**: today's `seed.json` shape is
    PlateVault-internal (it's loaded into the app's SQLite cache via
    `targeting_resolver::seed::load` → `cache::upsert_resolved`, which a pure
    published crate cannot reuse). Publishing requires a stable, documented
    format plus a loader API that yields (1) `TargetCoord`-compatible
    coordinate rows for the matching kit and (2) designation/alias rows for
    name matching through the normalizer. This is how consumers ingest the
    catalog into `astro-target-id`.
  - Also decide staleness/versioning policy (seed-builder rerun cadence,
    data-crate version bumps) and check bundled-data size.
- **(c) `simbad-resolver` — the live layer.** Resolve-on-demand against SIMBAD
  TAP/Sesame plus local-cache population. Stays in the Later tier (blocked on
  `domain_core` id-kernel work), but it is the stack's third layer, not an
  unrelated candidate.

**Composition model** (user design decision, 2026-07-04 — how the layers tie
together):

- The kernel (`astro-target-id`) defines the integration **contract**: a
  `CatalogSource` trait (lookup of a normalized query → candidates; coordinate
  rows for ranking) and a `LiveResolver` trait (async-capable,
  resolve-on-miss), plus a `ChainedSource` combinator implementing the
  canonical pipeline: local catalog first → live resolver on miss → the
  resolved result is handed back for the **consumer** to persist into their
  catalog. The kernel keeps **zero I/O** — it owns the interfaces and the
  chaining logic only; every implementation lives in a higher layer or in
  consumer code.
- `astro-seed-catalog` implements `CatalogSource` over the published seed
  format. `simbad-resolver` implements `LiveResolver`.
- **Facade via dependency direction** (tight coupling for consumers, achieved
  safely): `simbad-resolver` depends on BOTH lower layers and **re-exports**
  them (a prelude), so one dependency gives consumers the fully-wired stack —
  kernel + seed + live resolve. Kernel-only consumers add just the kernel.
  The dependency direction is unchanged: kernel ← seed ← resolver.
- **Explicitly rejected: a single crate with feature flags.** Rationale on
  record: (a) the seed DATA's licensing (SIMBAD/CDS/OpenNGC) must not
  contaminate the pure-MIT kernel; (b) the resolver's HTTP/async dependencies
  must never reach the kernel; (c) seed-data refresh releases must not bump
  kernel versions.

**Resolution story** (explicit, so nobody mistakes the kernel for a resolver):
`astro-target-id` alone does NOT resolve names. Full resolution =
normalize (kernel) → look up in a `CatalogSource` (seed or consumer-supplied)
→ on miss, live resolve via a `LiveResolver` (`simbad-resolver`) → resolved
result handed back for the consumer to persist. `ChainedSource` is the
canonical composition of that pipeline, mirroring PlateVault's own
seed → cache → SIMBAD → persist flow: in-tree, `targeting_resolver` supplies
rows from seed + SIMBAD + user overrides into the SQLite cache, and the app
feeds them to `coords::rank_candidates` as `TargetCoord`s. Consumers pick
layers: kernel-only (bring your own rows), kernel + seed (offline resolution
of common objects), or all three via the resolver facade
(full PlateVault-equivalent resolution).

**Dependency direction** (unchanged, restated): kernel ← seed-catalog ←
resolver — `astro-target-id` depends on nothing in the stack;
`astro-seed-catalog` may depend on the kernel's types; `simbad-resolver`
depends on both (and re-exports them as the facade). Never inverted (this is
already the in-tree direction: `targeting_resolver` depends on `targeting`).

**Effort**: Small — mostly documentation and API-surface polish on
already-decoupled code.

**Gating**: PR #349 on `main`, then its own new-repo bootstrap.

## Extraction 4 — `calibration-match` (from `crates/calibration/core`)

**What**: Pure calibration matching/ranking engine — temperature, exposure,
gain, binning, offset tolerances, and reuse policy — with no image I/O.

**Why publishable**: Already has zero internal dependencies; calibration
frame matching/reuse logic is a generically useful, narrow domain problem for
other astrophotography tooling.

**Boundary**: Work is stripping spec cross-references (`specs/007-...` style
comments) that only make sense inside this monorepo, and stabilizing the
public `SessionInfo`/`MasterInfo` API so it reads as an intentional external
contract rather than an internal implementation detail.

**Effort**: Small — no functional change, API stabilization + doc cleanup.

**Gating**: PR #349 on `main`, then its own new-repo bootstrap.

## Deprioritized / Rejected Candidates (for the record — do not re-propose without new information)

- **`simbad-resolver`** — later; blocked on `domain_core` id-kernel work.
  Now framed as layer (c) of the `astro-target-id` catalog stack (see
  Extraction 3), not an unrelated candidate — the Later-tier timing is
  unchanged.
- **`observing-night`** — later; also blocked on `domain_core` id-kernel work.
- **`path-template-resolver`** — niche; revisit on demand, no current
  pull.
- **`reviewable-fs-plan`** — niche; revisit on demand, no current pull.
- **Anything app-shaped** (depends on this app's domain types, DB schema, or
  Tauri/IPC boundary) — never publishable as-is; would need a redesign this
  program does not propose.

## Done When

- [ ] This spec is merged as the plan-of-record (docs only — this task).
- [ ] Each extraction, when picked up, links back to this spec in its own
      repo's initial README/CHANGELOG instead of re-deriving these decisions.
- [ ] No extraction starts before PR #349 is merged to `main`.
