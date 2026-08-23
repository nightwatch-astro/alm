# TinySpec: A sanitized value is exactly one path segment

**Branch**: fix/deriva-segment-sanitizer
**Date**: 2026-08-23
**Complexity**: 3 (TinySpec route)
**Findings**: astro-plan-3v3r.20.24, .1.22, .1.23, .8.39

## What

`safe_filename` turns an arbitrary string — in practice a FITS/XISF metadata value
— into one path segment, and `patterns` composes those segments into a rendered
destination. Four properties the pair is relied on for do not hold:

| Site | Behaviour today |
|------|-----------------|
| `crates/safe-filename/src/lib.rs:95` `is_windows_reserved_char` | `/` is absent from the substitution set, so a value of `Ha/OIII` passes through and one `{token}` renders two path segments |
| `crates/safe-filename/src/lib.rs:145` `step4_reserved_name_check` | compares the whole segment, so `CON` is refused and `CON.fits` is accepted |
| `crates/safe-filename/src/lib.rs:183` `sanitize_token_value` | returns `Ok("")` for `"..."` and `"  ..  "`, so "nothing left" is reported as success |
| `crates/safe-filename/src/lib.rs:165` `step5_confusables_check` | any mixed-script value is refused, including the Bayer designation `α Centauri` |

## The rule (decision)

**`sanitize_token_value` returns either exactly one usable path segment or an
error. It never returns a value that spans, collapses, or names something other
than one segment.**

Four sub-rules implement it.

### `/` is a reserved character, substituted in step 2

`/` joins `\`, `:`, `?`, `*`, `"`, `<`, `>`, `|` in `is_windows_reserved_char` and
maps to `_`. A sanitized value therefore cannot contain a separator, which is what
makes "one token, one segment" a property of the sanitizer rather than a hope
about metadata content.

This does not affect literal directory levels written in a pattern.
`resolve_pattern_str_with` splits the pattern on `/` *before* any sanitize call
(`resolver.rs:255`), so a literal run handed to `sanitize_literal` never contains
`/`. `resolve`'s separator parts are emitted unsanitized (`resolver.rs:172`).
Only substituted *values* are affected, which is the intent.

A consequence: a token value carrying `../` can no longer produce a traversal,
because the separators become `_` first. The assembled-path guard
`check_assembled_path` is kept — it still enforces the segment and total length
caps, and it remains the guard for any future composition that reintroduces a
separator — but the traversal arm is no longer reachable from a token value.

### The reserved-device-name check reads the stem

Windows resolves a device name from the text before the **first** dot, so
`CON.foo.json` names the console. `step4_reserved_name_check` compares
`segment.split('.').next()`, case-insensitively, against the existing list, and
stays unconditional on every platform.

`crates/fs/pathsafe/src/export_dest.rs:82` already extracts the stem before
calling `step4_reserved_name_check`, with the reasoning in a comment. Moving the
rule into the step makes that call site redundant, not wrong; removing the
duplicate split is tracked separately and is out of scope here.

### An emptied value is an error, and the token lane answers it with the fallback

`SanitizeError::EmptyAfterSanitize` is returned when step 1 and step 2 together
consume the whole value. The enum is internal to `safe_filename` and is not part
of the contract surface.

The two lanes in `patterns` answer it differently, and the difference is the
decision:

- **Token lane** (`resolve_one_token`): map it to the registry `fallback` and push
  the token onto `missing_tokens`. This is byte-for-byte the behaviour the
  `sanitized.is_empty()` branch had, so plan generation does not start failing.
- **Literal lane** (`sanitize_literal`): propagate it as a hard error, so a
  literal segment of only dots stops silently vanishing from the rendered path.

`ResolveError::EmptySegment` carries it to the boundary and maps to the existing
`ErrorCode::PatternInvalid` (`app/core`) and `ErrorCode::PathInvalid`
(`app/projects`). No `ErrorCode` is added and no TypeScript binding is
regenerated.

The token branch of `resolve` and `resolve_one_token` were already duplicate
implementations of lookup → transform → sanitize → fallback. `resolve` now calls
`resolve_one_token`, so the emptied-value rule exists once.

### A mixed Latin/Greek value is accepted; any other script mix is not

Mixed script alone is the wrong predicate: Bayer designations are Greek letters
attached to Latin constellation names, so `α Centauri` is a legitimate catalogue
name and refusing it refuses a real destination.

The rule: a non-ASCII value is accepted when it is single-script (unchanged), or
when every character's script is one of **Latin, Greek, Common, Inherited**.
Every other mix is still refused, so a Cyrillic homoglyph inside an otherwise
Latin word remains rejected.

The tradeoff Principle IV requires recorded: this admits a Latin/Greek homoglyph
pair (Greek ο against Latin o). Accepted deliberately — the alternative refuses
every Greek-letter target name, and the two scripts are both first-class in
astronomical nomenclature. No other script pair is admitted.

Per-character script lookup uses `unicode-script`, already in `Cargo.lock` as a
dependency of `unicode-security`.

## Per-finding verdict at head efaaf57a0

| Finding | Verified site at this head | Reachable | Verdict |
|---------|----------------------------|-----------|---------|
| .20.24 | `safe-filename/src/lib.rs:95-97` omits `/` | YES — a `FILTER` of `Ha/OIII` reaches `app/inbox/src/confirm.rs:463`, and the result becomes the plan destination at `:508` with no separator check between | LIVE |
| .1.22 | `safe-filename/src/lib.rs:145-151` compares the whole segment | Partly — the sanitizer returning `Ok` for `CON.fits` is proven and platform-independent. The harmful consequence, a write reaching the console device, is Windows-only and was **not** demonstrated; the sabot run was a Linux container | LIVE as a contract violation |
| .1.23 | `safe-filename/src/lib.rs:183-192` returns `Ok("")` | YES for the literal lane — `sanitize_literal` (`resolver.rs:399`) has no empty handling and `resolve_pattern_str_with:264` drops the empty result, so a dots-only literal segment disappears from the path. The bead title's "the file lands one level up" is **retracted**: the token lane already substitutes the registry fallback (`resolver.rs:387-394`), and literals join *within* a segment, so an emptied literal shortens a name rather than deleting a level | LIVE as an API-shape defect |
| .8.39 | `safe-filename/src/lib.rs:165` refuses any non-ASCII mixed-script value | YES — executed, `repro_rc=101`, refusing `α Centauri` through `pattern_path_preview` (`apps/desktop/src-tauri/src/commands/patterns.rs:89`) and source-view generation | LIVE |

Two statements in the briefing did not hold at this head and are recorded rather
than acted on:

- There is **no** comment in `resolver.rs` claiming a reserved-name check runs on
  the assembled segment. The check is genuinely absent; no false comment needed
  fixing.
- The `resolve_segment` doc "a segment never contains an internal `/`"
  (`resolver.rs:315`) was false and becomes **true** under sub-rule 1, so it is
  annotated rather than corrected.

## Context

| File | Role |
|------|------|
| `crates/safe-filename/src/lib.rs` | Modify: `/` reserved, stem-aware step 4, `EmptyAfterSanitize`, Latin/Greek confusable rule |
| `crates/safe-filename/Cargo.toml`, `Cargo.toml` | Modify: `unicode-script` workspace dependency |
| `crates/patterns/src/resolver.rs` | Modify: `resolve` delegates its token branch to `resolve_one_token`; `sanitize_literal` propagates; `ResolveError::EmptySegment`; assembled-segment reserved-name check |
| `crates/app/core/src/patterns.rs` | Modify: `map_resolve_error` gains the `EmptySegment` arm |
| `crates/app/projects/src/source_view_generate/mod.rs` | Modify: the source-view error match gains the `EmptySegment` arm |

## Requirements

1. A sanitized token value contains no `/`.
2. A single-token pattern resolves to exactly one path segment for every metadata
   value, including one containing `/`.
3. Two metadata values that differ only in a `/` do not render to the same path by
   one gaining a directory level.
4. A segment whose pre-first-dot stem is a Windows device name is refused, on
   every platform, with or without an extension.
5. `sanitize_token_value` never returns `Ok` with an empty string.
6. A token whose value sanitizes to empty resolves to the registry fallback and is
   reported in `missing_tokens`.
7. A literal pattern segment that sanitizes to empty is a hard error, not a
   dropped segment.
8. A value mixing Latin and Greek is accepted; a value mixing Latin and Cyrillic
   is refused.
9. No guard and no test for a guard is gated behind `cfg(unix)`; every fixture
   asserts the same outcome on macOS, Linux and Windows.
10. No schema change and no `ErrorCode` addition.

## Tasks

- [x] Add `/` to `is_windows_reserved_char`; update the step-2 doc and rstest table
- [x] Make `step4_reserved_name_check` compare the stem
- [x] Add `SanitizeError::EmptyAfterSanitize`; return it from `sanitize_token_value`
- [x] Relax `step5_confusables_check` to the Latin/Greek/Common/Inherited allowlist
- [x] `resolve`'s token branch delegates to `resolve_one_token`, which maps
      `EmptyAfterSanitize` to the fallback
- [x] `sanitize_literal` propagates `EmptyAfterSanitize` as `ResolveError::EmptySegment`
- [x] `check_assembled_path` also runs the reserved-name check per segment
- [x] Map `EmptySegment` in both exhaustive `ResolveError` matches
- [x] Re-target the two `.9.10` regression tests whose traversal outcome sub-rule 1
      makes unreachable, keeping the both-resolvers-agree assertion
- [x] One test per finding, each confirmed failing with its own fix reverted
- [x] Lint and test the four touched crates

## Done When

- [x] Each of the four findings has a named test that fails at base efaaf57a0 with
      that finding's fix reverted, recorded per test rather than as a blanket claim
- [x] Requirement 2 is proven at the `resolve_pattern_str` level, not only at
      `sanitize_token_value`, so the reachability claim is backed by the caller
- [x] Both confusable fixtures ship: `α Centauri` accepted, Cyrillic-in-Latin refused
- [x] `cargo clippy` and tests are green for `safe-filename`, `patterns`,
      `app_core`, `app_projects`
