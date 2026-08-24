# Implementation Plan: Token Pattern Builder

**Feature**: `015-token-pattern-builder`
**Status**: Implemented (post-hoc record, verified 2026-08-24) — agrees with
`spec.md`'s Implemented header, which was previously contradicted here. Resolver
`crates/patterns/src/resolver.rs`, validator `crates/patterns/src/validator.rs`,
builder UI shipped renamed as
`apps/desktop/src/features/settings/NamingStructure.tsx`.
**Constitution check**: passes — local-first metadata, reviewable mutation (resolver feeds plans, never applies directly), PixInsight boundary preserved (no image processing), research-led vocabulary, portable contracts via JSON Schema.

## Goal

Replace freeform path templates with a structured pattern model. The pattern is
a list of typed parts. A resolver turns a pattern plus a FITS metadata bundle
into a relative path; a validator catches OS-incompatible output and structural
errors before any plan is generated.

## Architecture

### Pattern as Data, Not a String

A pattern is `PatternPart[]`, where each part is `{ id, kind, value }`. This
matches the existing UI shape in `apps/desktop/src/ui/TokenPattern.tsx` and the
persisted `SettingsState.pattern` in `apps/desktop/src/data/settings.ts`. Tokens
remain symbolic until resolved; separators carry their literal value.

Rationale: storing structure (not a `{token}` string) avoids parsing
ambiguity, preserves token identity across vocabulary renames, and lets the UI
render chips without re-parsing.

### Resolver Boundary

The canonical resolver lives in **`crates/patterns/`** (previously proposed
as `crates/project/structure/`; split per R-CratePatterns, 2026-05-22). It accepts:

- a `Pattern` (Rust mirror of the JSON shape),
- a `MetadataBundle` keyed by token name,
- a `ResolverConfig` describing fallbacks, date format, and value transforms.

It returns `{ relative_path, missing_tokens }` or a typed error
(`pattern.invalid`, `token.unknown`, `pattern.empty`).

The resolver is **pure**: no filesystem, no clock, no database. This keeps it
unit-testable in milliseconds and reusable from CLI, desktop, and a future
remote backend.

### Validator Boundary

`crates/patterns/` also hosts a structural validator that operates on a
`Pattern` without metadata. It surfaces:

- empty pattern → `pattern.empty`,
- unknown token names → `token.unknown` (compared against the resolver's
  token registry),
- structural warnings (consecutive separators, trailing/leading separator,
  duplicate `/` segments).

OS path-character validation runs after resolution because invalid characters
can only appear in resolved values, not in tokens or whitelisted separators.

### Contracts

Two operation contracts, both under `specs/015-token-pattern-builder/contracts/`:

- `pattern.resolve` — resolves a pattern against a metadata bundle.
- `pattern.validate` — structural-only validation of a pattern.

Both are JSON Schema; the Tauri adapter and any future remote backend honor
the same shapes.

### UI Integration

The desktop mockup is already complete for the builder, preview, and the
Naming & Structure settings surface. The real implementation replaces the
hand-curated preview rows with live calls to `pattern.resolve` over the most
recent N inventory sessions per source, grouped by destination.

Per-source override edits write to `crates/persistence/db/` and are read by
the Inbox confirm pipeline (spec 018).

## Boundaries

- This feature owns: pattern model, resolver, validator, token registry,
  fallback policy, sanitization pipeline (Unicode + OS + path safety).
- This feature does **not** own: Inbox confirm pipeline (spec 018), plan
  generation (spec on filesystem plans), per-source override storage schema
  (lives in `crates/persistence/db/` and the Naming & Structure spec).
- No new heavy dependencies. The resolver is plain Rust; the validator uses
  a small inline helper. The `unicode-security` crate is the only new
  dependency (Unicode confusables detection, Ref: A1).

## Phases

### Phase 0 — Research

See `research.md`. Decisions cover vocabulary scope, fallback labels, date
formats, structural validation rules, and unknown-token handling.

### Phase 1 — Contracts & Data Model

- `contracts/pattern.resolve.json`
- `contracts/pattern.validate.json`
- `data-model.md` describes `PatternPart`, `Pattern`, `TokenDefinition`, and
  the fallback table.

Constitution re-check after Phase 1: still passes; resolver is pure, contracts
are portable, no implicit filesystem mutation.

### Phase 2 — Tasks

See `tasks.md`. Tasks are grouped by user story (P1–P4) and mark the mockup
work already complete.

## Crate Boundary

**Updated 2026-05-22 (Ref: R-CratePatterns)**: The pattern parser and
resolver are split into a dedicated crate **`crates/patterns/`**, separate
from `crates/project/structure/`.

`crates/patterns/` is the home for:

- `Pattern`, `PatternPart` Rust mirrors of the contract DTOs,
- `TokenRegistry` and `TokenDefinition`,
- `resolve()` and `validate()` functions,
- fallback configuration loader,
- sanitization pipeline (Unicode NFC, strip controls/bidi/format, OS char
  substitution, `.`/`..` traversal check, reserved name check, length caps),
- error types matching the contract error codes (`pattern.invalid`,
  `pattern.invalid.unicode`, `path.traversal`, `path.reserved_name`,
  `token.unknown`, `pattern.empty`).

The crate has zero runtime dependencies on Tauri, the database, or the
metadata extraction crates. Metadata is passed in as a plain bundle.

**Consumers of `crates/patterns/`**:

- `crates/app/core` (spec 005 Inbox confirm + preview)
- `crates/fs/planner` (spec 017 plan generation)
- `crates/project/structure/` (spec 008/024 project manifests)

`crates/project/structure/` retains project-envelope rules but delegates
pattern resolution to `crates/patterns/`.

**CLAUDE.md note**: The `crates/patterns/` crate path must be added to the
Monorepo Structure section of `CLAUDE.md`. This edit is **deferred** — do not
edit CLAUDE.md in this session. Flag for next CLAUDE.md revision. (Ref: R-CratePatterns)

## Risks & Mitigations

- **Token vocabulary drift**: persisted patterns may reference tokens removed
  in a future version. Mitigation: explicit `token.unknown` error rather than
  silent empty segments; migration notes documented when vocabulary changes.
- **Path collisions across sources**: two sources can resolve to the same
  relative path with different content. Mitigation: out of scope here;
  belongs to spec 018 / the plan generator, which sees the absolute path
  including the source root.
- **Date format ambiguity**: locale-sensitive formatting would break audits.
  Mitigation: fixed ISO-like format documented in `research.md`; never
  locale-derived.
