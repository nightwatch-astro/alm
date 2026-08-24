# Feature Specification: Token Pattern Builder

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `015-token-pattern-builder`  
**Created**: 2026-05-09  
**Status**: **Implemented** (2026-07-03) — builder UI + resolver/validator/preview shipped; see below.
**Input**: User description: "Specify the token-based pattern builder for project folders and archive locations, without freeform path text."

## Implementation Status: Implemented (2026-07-03)

Superseding the original "mockup only" note: the builder is shipped end to end.
The chip-based editor is live in
`apps/desktop/src/features/settings/NamingStructure.tsx` (`PatternChipsEditor` —
token + separator chips with add/remove, live validation via `pattern_validate`,
live preview via `pattern_preview`, save-blocking when invalid). The
resolver/validator/sanitizer is the `crates/patterns/` crate
(registry/resolver/validator/sanitize/per_type, ~64 passing tests), exposed
through `crates/contracts/core/src/patterns.rs` + app orchestration
(`crates/app/core/src/patterns.rs`) + Tauri commands `pattern_validate` /
`pattern_resolve` / `pattern_preview`. A full SpecKit artifact set exists
(plan/research/data-model/contracts/tasks, ~30 tasks — not "0/0"). Deferred
downstream scope (per-source overrides, session-backed preview aggregation,
Inbox-confirm consumption) was handed to spec 018.

> Note: the earlier "Mockup Evidence" file references (`ui/TokenPattern.tsx`,
> `data/mock.ts`, `data/settings.ts`) are obsolete — those files were replaced
> by the real implementation above during the 027 frontend rewrite.

### Token Vocabulary (v1)

Sourced from `availableTokens` in `apps/desktop/src/data/mock.ts`:

- `target` — primary acquisition target (catalog name or alias).
- `filter` — optical filter used for the frame.
- `date` — capture date (default `YYYY-MM-DD`).
- `frame_type` — `light`, `dark`, `flat`, `bias`, or `mixed`.
- `camera` — camera model.
- `exposure` — exposure duration (formatted, e.g. `120s`).
- `gain` — gain/ISO value.
- `binning` — binning notation (e.g. `1x1`).
- `set_temp` — set/sensor temperature (e.g. `-10C`).

Telescope, project, and workflow tokens listed in FR-003 are **deferred** to a
later spec; the v1 builder uses the vocabulary above.

### Separator Vocabulary

Sourced from `TokenPatternBuilder` defaults: `/`, `-`, `_`, and space. `/` is
the only separator that introduces a new path segment; the others are
treated as in-segment literals.

### Preview Behavior

`PatternPreview` accepts an array of `{ path, count }` rows. The mockup uses
hand-curated sample destinations representative of the user's recent FITS;
the live implementation MUST derive preview rows by resolving the current
pattern against the most recent N inventory sessions per source, grouped by
unique destination path, with a frame count per row.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Build Project Folder Pattern (Priority: P1)

As a user configuring naming, I want to build project folder patterns by selecting metadata tokens and separators so that folder names are valid and predictable.

**Why this priority**: Freeform pattern text is unclear and error-prone.

**Independent Test**: Build a pattern using target, filter, date, frame_type, camera, and separators and confirm the preview updates.

**Acceptance Scenarios**:

1. **Given** the pattern builder is open, **When** the user adds a metadata token, **Then** the token appears in the pattern preview.
2. **Given** the user adds a separator, **When** it is `/`, `-`, `_`, or space, **Then** it appears between tokens in the rendered pattern.
3. **Given** a pattern with `{target}/{filter}/`, **When** the user removes the `{filter}` chip, **Then** the preview reflects the shorter pattern immediately.

---

### User Story 2 - Live Preview Against Recent Metadata (Priority: P2)

As a user editing the pattern, I want to see how it resolves against my recent FITS so I can spot regressions before saving.

**Independent Test**: Edit the pattern and confirm the preview rows recompute with current sample frame counts.

**Acceptance Scenarios**:

1. **Given** a pattern resolving to a path with a missing token, **When** the source metadata lacks a value, **Then** the preview row shows the configured fallback (e.g. `—` or `unclassified`).
2. **Given** the pattern is empty, **When** the preview is requested, **Then** the preview displays a clear empty-state message and saving is blocked.

---

### User Story 3 - Resolve Pattern at Inbox Confirm (Priority: P3)

As the application, when Inbox items are confirmed, I want to resolve the active pattern against each item's metadata to produce a destination relative path.

**Independent Test**: Resolve a known pattern against a fixture metadata bundle and assert the produced relative path matches expectations.

**Acceptance Scenarios**:

1. **Given** a pattern `{target}/{filter}/{date}/{frame_type}/`, **When** resolved against `{target: M101, filter: Ha, date: 2026-04-12, frame_type: light}`, **Then** the relative path is `M101/Ha/2026-04-12/light/`.
2. **Given** a token has no value, **When** resolved, **Then** the result includes the fallback and the missing-token list reports the absent token.
3. **Given** a pattern produces a path containing OS-invalid characters, **When** validated, **Then** validation fails with `pattern.invalid` and the offending characters are reported.

---

### User Story 4 - Per-Source Override (Priority: P4)

As a user with a legacy source whose existing layout differs, I want to override the library default pattern for that source only.

**Independent Test**: Set an override pattern for one source and confirm Inbox items from other sources still resolve via the library default.

**Acceptance Scenarios**:

1. **Given** source A has an override pattern, **When** an Inbox item from source A is confirmed, **Then** its destination is resolved against the override.
2. **Given** source B has no override, **When** an Inbox item from source B is confirmed, **Then** its destination is resolved against the library default.

---

### Edge Cases

- Missing metadata values for one or more tokens.
- Consecutive separators (e.g. `//`) produced by the user.
- OS-invalid characters in resolved metadata values (`:`, `\`, `?`, `*`, etc.).
- Extremely long generated paths (Windows MAX_PATH, POSIX 4096).
- Empty pattern.
- Trailing separator vs no trailing separator.
- Unknown token name in a persisted pattern (vocabulary drift).

### Domain Questions To Resolve

See `research.md` for resolved decisions.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Project folder patterns MUST NOT be edited as unrestricted freeform text.
- **FR-002**: Archive location patterns MUST NOT be edited as unrestricted freeform text and MUST reuse the same builder component (deferred to a later spec; vocabulary remains shared).
- **FR-003**: Builder MUST offer metadata tokens from the v1 vocabulary: `target`, `filter`, `date`, `frame_type`, `camera`, `exposure`, `gain`, `binning`, `set_temp`.
- **FR-004**: Builder MUST allow separators from the v1 set: `/`, `-`, `_`, space.
- **FR-005**: Builder MUST show a preview of representative destination paths derived from recent metadata.
- **FR-006**: Resolver MUST validate generated paths against OS path rules before any plan generation that consumes them.
- **FR-007**: Resolver MUST substitute a documented fallback when a token has no value, and MUST report the missing tokens to the caller.
- **FR-008**: Pattern MUST persist as an ordered list of `PatternPart` records, each tagged as `token` or `separator`.
- **FR-009**: Builder MUST allow removing individual chips and reordering is **deferred** — v1 supports only append + remove.
- **FR-010**: Library default pattern and per-source overrides MUST be persisted and editable in the Naming & Structure settings surface.
- **FR-011**: When a persisted pattern references an unknown token name, the resolver MUST surface a `token.unknown` error rather than silently producing an empty segment.

### Key Entities

- **PatternPart**: `{ id, kind: token|separator, value }` — the atomic unit of a pattern.
- **Pattern**: ordered `PatternPart[]`.
- **Token Definition**: name, source metadata field, optional fallback, optional value transform (e.g. date formatting).
- **Pattern Preview Row**: `{ path, count? }` — a sampled resolution against representative metadata.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can create valid project patterns without typing raw template syntax.
- **SC-002**: Invalid pattern outputs (OS character violations, empty pattern, unknown tokens) are caught before they affect Inbox confirmation, plan generation, or archive plans.
- **SC-003**: Users can identify all tokens available for folder naming from the UI.
- **SC-004**: Preview reflects the current pattern within one frame of any edit (no save round-trip).

## Assumptions

- Metadata tokens map to extracted FITS/XISF fields or user-entered confirmations.
- Missing metadata is represented by configurable fallback labels (see `research.md`).
- The builder is not responsible for renaming existing files — only for resolving destination paths for new operations.

## Out of Scope

- Arbitrary scripting or expression evaluation in patterns.
- Automatic renaming of existing project folders.
- Drag-to-reorder chips (deferred; v1 is append + remove).
- Telescope / project / workflow tokens (deferred to a follow-up vocabulary spec).
- Conditional pattern branches (e.g. "if frame_type is calibration, use a different sub-pattern") — deferred.

## Dependencies

- Spec 018 (Naming & Structure) consumes the persisted pattern and per-source overrides.
- Spec 001 metadata extraction provides the field values resolved tokens read from.
- `crates/project/structure/` will host the resolver and validator.
