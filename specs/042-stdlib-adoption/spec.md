# Feature Specification: Standard-Library Adoption & Structural Modernization

**Feature Branch**: `042-stdlib-adoption`
**Created**: 2026-06-20
**Status**: Implemented (post-hoc record, verified 2026-07-28). All substantive
adoption work ships at HEAD. The remaining unchecked boxes in `tasks.md` are
commit-bookkeeping checkpoints plus one manual Windows verification, not code
gaps. That file is legacy paper; task state for this spec lives in beads.
**Input**: User description: "Discover hand-rolled code across the repo (Rust + TS/React) that has a well-established standard-library replacement, then spec, plan, task, implement, and verify the migrations — anchored on replacing the homegrown desktop state store with TanStack Query. Maximal adoption appetite; harden the Rust↔TypeScript boundary; de-duplicate copy-pasted helpers; make narrow crate-structure fixes."

## Overview

This is a **maintainer-value modernization** feature. The "users" are the project's
developers and maintainers; the value delivered is **correctness** (it fixes real
latent bugs), **maintainability** (de-duplication, single sources of truth, typed
boundaries), **performance** (list virtualization, non-blocking I/O), and
**type-safety** at the Rust↔TypeScript seam.

A comprehensive discovery pass (recorded in `research.md`) inventoried hand-rolled
code in both stacks with a verdict per finding (ADOPT a library / REFACTOR to an
idiomatic pattern / CONSOLIDATE duplication / KEEP / DEFER / REJECT). This spec
captures the **ADOPT / REFACTOR / CONSOLIDATE** decisions as independently testable
user stories. Each story replaces a hand-rolled implementation with a mature library
or idiomatic pattern **and removes the hand-rolled version**, or consolidates
duplicated code into a single shared home, without changing observable product
behavior except where a defect is explicitly being fixed.

Because library selection is the explicit subject of this feature, specific library
names appear in the requirements by design (they are decided inputs, not
implementation leakage — see Assumptions).

## Clarifications

### Session 2026-06-20

Scope, priorities, dependency appetite, and every keep/defer/reject verdict were
resolved interactively with the maintainer during the discovery walkthrough, before
this spec was written. No `[NEEDS CLARIFICATION]` markers remain. The decisions the
spec phrased as open choices were resolved as follows:

- **Adoption appetite**: *maximal* — adopt every ADOPT/REFACTOR/CONSOLIDATE finding,
  **including** items the discovery agents had marked DEFER, except the explicit
  rejections below.
- **US16 (long-operation contract)**: **adopt end-to-end** for the plan-apply reference
  flow over a Tauri channel (NOT the "retire the dead types" alternative).
- **US13 (`app/core` restructuring)**: perform the **full split into per-domain
  use-case crates**, not only internal module grouping. (Sequenced late and treated as
  the highest-risk story; it may be staged last.)
- **Forms (US5)**: adopt react-hook-form + zod; client validation is UX-only and the
  Rust backend remains the authoritative validator.
- **Caching (US12)**: `moka` with TTL for the debounce table; `dashmap` for the
  active-runs registry (with RAII removal).
- **Rust errors (US8)**: `thiserror` for library errors AND `anyhow` (+ `.context()`)
  at the application boundary, including the `.to_string()` context-loss refactor.
- **Boundary (US2)**: full retirement of the hand-written `bindings/types.ts` snake_case
  type universe; add `zod` for runtime validation at the IPC seam.
- **Explicit rejections (out of scope)**: an internal HTTP/JSON-RPC API; `walkdir`
  (no recursive `read_dir` exists); `clap` (no CLIs); a new `support`/`common`
  "junk-drawer" crate; merging the thin adapter crates; removing
  `react-resizable-panels` (retained for a deferred future use).

**Process note**: the SpecKit skill harness resolves the active feature from the
original repository checkout (branch `041-inbox-plan-surface`), not this isolated
worktree (branch `042-stdlib-adoption`). To honor worktree isolation, the SpecKit
phases are driven manually inside the worktree using the same templates the skills
use; the artifacts (`spec.md`, `research.md`, `plan.md`, `data-model.md`, `contracts/`,
`tasks.md`) are produced identically.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Replace the homegrown server-state store with a maintained data-fetching library (Priority: P1) — ANCHOR

The desktop app's server state (data fetched from the Rust core over IPC) is managed
by a ~160-line homegrown store (`apps/desktop/src/data/store.ts`) built on
`useSyncExternalStore`. It has two concrete defects: it triggers fetches **during
render** (a side-effect-in-render anti-pattern), and its parameterized-store cache is
an unbounded `Map` with no eviction (a memory leak over a long session). It is
replaced by a maintained server-state library (TanStack Query), and the homegrown
store is retired.

**Why this priority**: It is the explicit anchor of the feature, the highest-confidence
adoption, touches the most surfaces, and removes two real defects. It establishes the
query-key + invalidation conventions the rest of the frontend builds on.

**Independent Test**: With the app running against the real backend, navigate
projects/sessions/inbox, confirm lists load, mutations refresh the right views (no
stale data), no duplicate fetches occur on mount (verifiable via the network/IPC call
log), and memory does not grow unboundedly when visiting many distinct entities.
`apps/desktop/src/data/store.ts` no longer exists.

**Acceptance Scenarios**:
1. **Given** a freshly mounted list view, **When** it renders, **Then** exactly one
   fetch is issued for its data (no render-phase double-fetch).
2. **Given** a successful create/update/delete mutation, **When** it completes,
   **Then** the corresponding list and detail views refresh automatically via cache
   invalidation, with no manual store wiring.
3. **Given** a long session that opens many distinct entity detail views, **When**
   the user continues working, **Then** cached query state is bounded (old entries
   are garbage-collected), not retained forever.
4. **Given** the migration is complete, **When** the codebase is searched, **Then**
   `data/store.ts` and its `createStore`/`createParameterizedStore` APIs are gone and
   no module imports them.

### User Story 2 - Harden the Rust↔TypeScript boundary and unify error/string handling (Priority: P1)

The IPC boundary carries a parallel, hand-written `snake_case` type universe
(`bindings/types.ts`) bridged to the generated `camelCase` contract by ~26
`as unknown as` and ~91 `as` casts — the documented root cause of prior casing bugs.
Error codes are magic strings duplicated on both sides of the wire. There are two
unreconciled type generators (the live specta bindings and an orphaned JSON-Schema
surface). This story makes the generated contract the single source of truth, makes
error codes a single shared enum, and centralizes error-message handling.

**Why this priority**: The boundary is the highest-risk drift surface; the string/
message-management concern was explicitly emphasized. Aligning the frontend to the
generated contract removes a whole class of silent runtime bugs and unblocks the
other frontend stories that touch `commands.ts`.

**Independent Test**: The app talks to the real backend with no field-casing errors;
a search of the IPC layer finds zero `as unknown as` casts; error codes referenced in
the UI resolve against a single generated enum; runtime payloads are validated at the
seam and a deliberately malformed payload is caught with a clear error.

**Acceptance Scenarios**:
1. **Given** a command response from the real backend, **When** the frontend consumes
   it, **Then** field access uses the generated `camelCase` types directly with no
   intermediate cast, and the hand-written `bindings/types.ts` struct universe is gone.
2. **Given** an error returned by the core, **When** the UI branches on its code,
   **Then** it compares against a value from a single shared `ErrorCode` enum that is
   generated from one Rust definition (no duplicated string literals on either side).
3. **Given** the two type-generation outputs, **When** the contract changes, **Then**
   an automated check fails if the generated TypeScript and the language-neutral schema
   disagree (they are derived from one source).
4. **Given** any error value reaching the UI, **When** it is shown, **Then** its
   message comes from one shared normalization utility (not 15+ inline variants), and
   user-facing message strings live in a single module.

### User Story 3 - Virtualize all long lists (Priority: P2)

Several scrolling lists render every row to the DOM (log panel ~500 entries, inbox up
to 500 items, the target catalog, target-search results), and the inbox list contains
an O(n²) lookup inside its render map. They are virtualized with the already-installed
virtualization library, and the O(n²) lookup is removed.

**Why this priority**: Direct, measurable performance and responsiveness win using a
library already in the dependency tree; isolated per-component blast radius.

**Independent Test**: Load each list at its maximum size; scrolling stays smooth and
the DOM contains only the visible window of rows plus overscan; the inbox list renders
in linear time.

**Acceptance Scenarios**:
1. **Given** a list at its maximum item count, **When** the user scrolls, **Then**
   only the visible rows (plus overscan) exist in the DOM.
2. **Given** the inbox list at its cap, **When** it renders, **Then** no per-row
   linear scan of the full list occurs (no O(n²) behavior).

### User Story 4 - Replace hand-rolled overlays with the installed headless-UI primitives (Priority: P2)

Hand-rolled dropdowns/menus/popovers/comboboxes (e.g. the projects filter dropdown,
the target-search combobox) reimplement focus management, click-outside, Escape
handling and ARIA — and at least one has real bugs (no click-outside, no Escape). They
are replaced by the installed headless-UI primitives (Base UI Select/Menu/Combobox/
Popover).

**Why this priority**: Fixes real interaction bugs and deletes accessibility-critical
hand-rolled code using a library already in the tree.

**Independent Test**: Open each migrated overlay; clicking outside and pressing Escape
close it; keyboard navigation and focus return work; screen-reader roles are present.

**Acceptance Scenarios**:
1. **Given** the projects filter dropdown is open, **When** the user clicks outside or
   presses Escape, **Then** it closes (previously it did not).
2. **Given** any migrated overlay, **When** navigated by keyboard, **Then** focus
   trapping/return and option selection behave per the primitive's standard semantics.

### User Story 5 - Client-side form validation with a forms library (Priority: P2)

Forms manage state with ad-hoc `useState` per field and rely on a backend round-trip
to surface validation errors. They adopt a forms library with schema validation
(react-hook-form + zod) for immediate pre-submit feedback. **The backend remains the
authoritative validator** — client validation is a UX layer, not a trust boundary.

**Why this priority**: Improves UX on data-entry flows; introduces the shared schema-
validation capability reused at the IPC seam (Story 2).

**Independent Test**: Enter invalid input in a migrated form; an inline error appears
before submit; a valid submit still succeeds; backend rejection of a value the client
accepted is still surfaced (backend remains source of truth).

**Acceptance Scenarios**:
1. **Given** an invalid field value, **When** the user attempts to submit, **Then**
   an inline validation message appears without a backend round-trip.
2. **Given** a value the client considers valid but the backend rejects, **When**
   submitted, **Then** the backend error is still surfaced (defense in depth).

### User Story 6 - Replace reinvented frontend utilities with libraries (Priority: P3)

Hand-rolled debounce (×2), three near-identical date formatters, manual table sort/
filter state, multiple global keydown handlers, manual cross-platform path string
manipulation, and inline SVG icons duplicating an installed icon set are replaced by
established libraries (use-debounce, date-fns, TanStack Table, tinykeys, pathe,
lucide-react).

**Why this priority**: Each is a self-contained reinvented-wheel swap; low risk,
incremental, improves consistency and correctness (e.g. timezone handling).

**Independent Test**: Debounced inputs, formatted dates, sortable/filterable tables,
keyboard shortcuts, path display, and icons all behave as before (or better), with the
hand-rolled helpers removed.

**Acceptance Scenarios**:
1. **Given** a debounced input, **When** typing rapidly, **Then** the action fires
   once after the idle interval, via the library (the hand-rolled `setTimeout` ref is
   gone).
2. **Given** any date display, **When** rendered, **Then** it is produced by the date
   library through a single shared formatter (the duplicate formatters are gone).

### User Story 7 - Frontend type-safety and dead-code cleanup (Priority: P3)

Remaining type-safety holes are closed (typed mock fixtures instead of `as T`,
generated types in `useStatusSummary`, a concrete settings shape, `satisfies` for
literal unions), ~30 empty `catch {}` blocks are triaged (logged/narrowed/annotated),
dead diverged code (`lib/display.ts`) is deleted, and state-label functions are typed
against the generated state unions for exhaustiveness.

**Why this priority**: Maintainability and correctness cleanup that depends on Story 2
landing first; low individual risk.

**Independent Test**: Type-check passes; a deliberately wrong fixture shape now fails
to compile; `lib/display.ts` is gone; adding a new state value causes a compile error
in the (now exhaustive) label functions.

**Acceptance Scenarios**:
1. **Given** a mock fixture whose shape drifts from the contract, **When** the project
   type-checks, **Then** it fails (previously an `as T` cast hid the drift).
2. **Given** the dead `lib/display.ts`, **When** the cleanup lands, **Then** it is
   deleted and nothing imports it.

### User Story 8 - Idiomatic Rust error handling and logging (Priority: P2)

Library errors use `thiserror`; the application boundary uses `anyhow` with `.context()`
so error context is preserved instead of being flattened via `.to_string()`. A manual
`Display` impl becomes a `thiserror` derive, a stray production `eprintln!` becomes
structured `tracing`, and a `tracing-subscriber` configuration is established. Test
diagnostics keep using `eprintln!`/`println!`.

**Why this priority**: Improves diagnosability and error fidelity across the core;
prerequisite context for the error-code work in Story 2/Story 11.

**Independent Test**: A failing core operation surfaces a contextful error chain (not a
flattened string); production code contains no diagnostic `eprintln!`; logs are emitted
through the structured logger.

**Acceptance Scenarios**:
1. **Given** a failure deep in a use case, **When** it propagates to the boundary,
   **Then** the surfaced error carries operation context (not just a leaf message).
2. **Given** the production code paths, **When** searched, **Then** they contain no
   ad-hoc `eprintln!` diagnostics (those moved to `tracing`).

### User Story 9 - Typed string↔enum conversions in Rust (Priority: P2)

String-to-enum conversions are made into a single typed `TryFrom`/`FromStr` per type
(or via `strum`), eliminating divergent silent-default behavior. Specifically, the
calibration-kind conversion currently has two contradictory fallbacks (`_ => Dark`
vs `_ => None`) — a latent bug — which is unified; inventory-state and first-run
enum↔string pairs are likewise centralized.

**Why this priority**: Fixes a real latent correctness bug and removes silent-default
footguns.

**Independent Test**: The calibration-kind conversion has exactly one definition with
one defined fallback; an unknown value is handled identically everywhere; existing
calibration behavior is unchanged for known values.

**Acceptance Scenarios**:
1. **Given** an unknown calibration-kind string, **When** converted, **Then** the
   single defined fallback applies everywhere (no site silently defaults to a
   different variant).
2. **Given** a known stored value, **When** converted, **Then** the resulting enum is
   identical to today's behavior (no semantic change for valid data).

### User Story 10 - Replace reinvented Rust utilities with crates (Priority: P3)

Hand-rolled RFC-3986 percent-encoding, ISO-8601 date parsing with hand-written
Gregorian math, a recursive glob matcher, manual dedup helpers, manual TSV row
tokenization, hand-rolled FITS header byte parsing, lexical path normalization, and a
hand-written JSON marker are replaced by established crates (percent-encoding, `time`,
globset, itertools, csv, byteorder, path-clean, serde_json). Domain-specific
validation (e.g. RA/Dec range checks) is preserved on top of library parsing.

**Why this priority**: Self-contained correctness/robustness swaps; some touch
correctness-sensitive parsing and so are guarded by existing tests.

**Independent Test**: Each migrated utility produces identical results to the
hand-rolled version across the existing test suite (and a glob pattern matrix /
parsing fixtures); the hand-rolled code is removed.

**Acceptance Scenarios**:
1. **Given** the existing test corpus for each utility, **When** the crate replaces
   the hand-rolled code, **Then** all tests pass unchanged.
2. **Given** glob/encoding/parsing edge cases, **When** exercised, **Then** the
   library handles them at least as correctly as before (verified by an added matrix
   for globbing).

### User Story 11 - Consolidate duplicated Rust helpers into single homes (Priority: P2)

Copy-pasted helpers are de-duplicated: the RFC-3339 timestamp helper (~28 copies) and
UUID helper (×5) move onto the existing pure-domain types; the database-error and
event-bus error mappers and an object-type mapper become single internal modules; a
`From<DbError> for ContractError` collapses ~123 repetitive `.map_err` calls; the
settings schema (today three parallel match arms) becomes one descriptor table; the
duplicated SIMBAD row parser is shared. The diverged database-error mapper bug (most
sites report not-found as a fatal internal error while one site maps it to a
recoverable code) is resolved to the canonical mapping.

**Why this priority**: Largest-volume duplication; includes a correctness divergence
(error severity) and removes a maintenance hazard.

**Independent Test**: Each consolidated helper is defined exactly once; behavior for
known inputs is unchanged; a not-found database error now reports a consistent,
recoverable code everywhere it can occur.

**Acceptance Scenarios**:
1. **Given** the codebase, **When** searched, **Then** the timestamp/UUID/error-mapper
   helpers each have exactly one definition.
2. **Given** a not-found database condition, **When** it surfaces, **Then** it maps to
   the single canonical (recoverable) code, not a fatal internal error at some sites
   and a recoverable one at others.

### User Story 12 - Idiomatic Rust caching and concurrency (Priority: P3)

An unbounded debounce table is replaced by a TTL cache (moka) so it self-evicts; a
global active-runs registry guarded by a coarse mutex becomes a concurrent map
(dashmap) with RAII-based removal so a panicking run cannot leak an entry; blocking
filesystem work reached from an async task is offloaded (spawn_blocking) so it cannot
starve the runtime; small ad-hoc mutex-wrapped sets are simplified.

**Why this priority**: Removes a memory-growth source and a runtime-starvation risk;
internal-only, no semantic change.

**Independent Test**: The debounce table self-bounds over time; the active-runs entry
is removed even if a run errors; a large plan apply does not block unrelated async work.

**Acceptance Scenarios**:
1. **Given** sustained debounce activity, **When** time passes, **Then** stale entries
   are evicted (the table does not grow without bound).
2. **Given** a plan run that panics mid-apply, **When** it unwinds, **Then** its
   registry entry is removed (no leaked "active" run).

### User Story 13 - Narrow workspace/crate restructuring (Priority: P3)

Dependency hygiene and layering are corrected: the SIMBAD resolver (HTTP/DB/async
deps) is split out of the pure-domain `targeting` crate; the pure-domain base crate
becomes a true shared base so duplicated helpers (Story 11) have a home; the
oversized orchestration crate's flat modules are grouped (and split into per-domain
use-case crates); `tokio` is dropped from a pure rules crate; and the persistence→
contracts layering inversion is corrected by moving stored types to the domain layer.
The persisted on-disk/SQL representation MUST NOT change.

**Why this priority**: Restores the constitution's "narrow crates, light pure-domain
crates, independent compilation" invariants and shrinks rebuild blast radius. Larger,
so prioritized after the higher-value swaps; the persistence-inversion fix is handled
as its own carefully tested change.

**Independent Test**: A change to target/alias domain types no longer recompiles the
HTTP/DB stack; the pure-domain crate is depended on by the crates that previously
reinvented its helpers; persistence tests stay green and the database file format is
byte-identical.

**Acceptance Scenarios**:
1. **Given** an edit to a pure-domain type, **When** the workspace builds, **Then** it
   does not trigger a rebuild of network/database dependencies it should not pull.
2. **Given** the persistence layer after the inversion fix, **When** an existing
   database is opened, **Then** it reads/writes identically (no schema or
   serialization change).

### User Story 14 - UTF-8 path types across the filesystem crates (Priority: P3)

`PathBuf`/`Path` usage that crosses the IPC boundary or is serialized adopts a
UTF-8-guaranteed path type (camino's `Utf8Path`), removing lossy `to_string_lossy()`
conversions and making path serialization correct by construction.

**Why this priority**: Real cross-platform correctness benefit but broad blast radius
requiring Windows/UNC testing; sequenced late.

**Independent Test**: Paths serialize across the boundary without lossy conversion;
behavior on Windows (including long/UNC paths) is verified in the real build.

**Acceptance Scenarios**:
1. **Given** a path crossing the IPC boundary, **When** serialized, **Then** it is a
   guaranteed-UTF-8 value with no `to_string_lossy()` in the path.

### User Story 15 - Table-driven and property-based tests (Priority: P3)

Duplicated near-identical test bodies (e.g. the sanitizer's ~27 input/output cases and
per-key settings validation) are converted to parameterized tests (rstest), and
pure-transform invariants gain property tests (proptest). These are dev-dependencies
only.

**Why this priority**: Test-quality improvement with zero production blast radius;
done opportunistically alongside the crates it covers.

**Independent Test**: The converted suites cover the same (or more) cases with less
duplication and still pass.

**Acceptance Scenarios**:
1. **Given** the sanitizer test cases, **When** converted to a table-driven form,
   **Then** every prior case is still exercised and passes.

### User Story 16 - Resolve the long-operation contract (Priority: P3)

The defined-but-unused long-operation contract (operation handle + streamed operation
events) is either adopted end-to-end for one reference flow (plan apply, over a Tauri
channel) or formally retired so the contract no longer implies an unkept guarantee.

**Why this priority**: Removes a dead-but-misleading contract; lowest urgency and a
genuine either/or design decision, so sequenced last.

**Independent Test**: Either the plan-apply flow streams standard operation events
consumed by the UI, or the unused contract types are removed and nothing references
them.

**Acceptance Scenarios**:
1. **Given** the long-operation contract, **When** the story lands, **Then** it is
   either exercised by at least one real flow end-to-end or removed entirely (not left
   defined-and-unused).

### Edge Cases

- **Mock vs real backend drift**: mock-mode fixtures must match the generated contract
  shapes so casing/field drift cannot hide in mock mode (Story 2/Story 7).
- **Stored-value compatibility**: typed enum conversions (Story 9) and the persistence
  restructuring (Story 13) must accept every value already present in existing
  databases — no migration of on-disk representation.
- **Glob/parse equivalence**: library-based glob/encoding/parsing (Story 10) must not
  change classification or resolution outcomes for any in-use input.
- **Invalidation completeness**: after the Query migration (Story 1), every mutation
  that previously invalidated a homegrown store must invalidate the equivalent query
  key (no stale view).
- **Windows reality**: path-type and any platform-sensitive change is verified in the
  real Windows Tauri build, not only in unit tests.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The homegrown server-state store MUST be replaced by a maintained
  data-fetching library and deleted; server data MUST be fetched via that library with
  defined query-key and invalidation conventions.
- **FR-002**: Server data MUST NOT be fetched during render; initial fetches MUST be
  driven by the library's lifecycle, and cached query state MUST be bounded (evictable),
  not retained for the session lifetime.
- **FR-003**: The frontend MUST consume the generated `camelCase` contract types
  directly; the parallel hand-written `snake_case` type module and all
  `as unknown as` casts at the IPC boundary MUST be removed.
- **FR-004**: Error codes MUST be defined once (a Rust enum generated to TypeScript)
  and referenced from that single source on both sides; duplicated magic-string error
  codes MUST be eliminated.
- **FR-005**: The two type-generation outputs (TypeScript bindings and the
  language-neutral schema) MUST be derived from one source and guarded by an automated
  agreement check.
- **FR-006**: Error-to-message normalization MUST be a single shared utility, and
  user-facing message strings MUST live in a dedicated module; inline error-string
  idioms MUST be removed.
- **FR-007**: Runtime IPC payloads MUST be validatable at the boundary via shared
  schemas; malformed payloads MUST produce a clear, caught error.
- **FR-008**: All long scrolling lists MUST be virtualized using the installed
  virtualization library; no list may render its entire dataset to the DOM, and the
  inbox list's O(n²) per-row lookup MUST be removed.
- **FR-009**: Hand-rolled overlay widgets (dropdown/menu/popover/combobox) MUST be
  replaced by the installed headless-UI primitives, restoring click-outside, Escape,
  focus management, and ARIA semantics.
- **FR-010**: Data-entry forms MUST provide client-side pre-submit validation via a
  forms library with schema validation, while the backend remains the authoritative
  validator.
- **FR-011**: Reinvented frontend utilities (debounce, date formatting, table sort/
  filter, global keyboard shortcuts, path manipulation, icons) MUST be replaced by
  their established libraries and the hand-rolled versions removed.
- **FR-012**: Remaining frontend type-safety holes MUST be closed (typed fixtures,
  generated types, concrete settings shape, `satisfies` for literal unions, exhaustive
  state-label functions); empty `catch` blocks MUST be triaged; dead diverged modules
  MUST be deleted.
- **FR-013**: Rust library errors MUST use `thiserror` and the application boundary
  MUST use `anyhow` with preserved context; production diagnostic `eprintln!` MUST be
  replaced by structured `tracing`; a logging subscriber MUST be configured.
- **FR-014**: String↔enum conversions in Rust MUST be single typed conversions with one
  defined fallback per type; the divergent calibration-kind fallback MUST be unified
  with no behavior change for known values.
- **FR-015**: Reinvented Rust utilities (percent-encoding, date parsing, globbing,
  dedup, TSV parsing, FITS header parsing, path normalization, JSON marker) MUST be
  replaced by established crates, preserving any domain-specific validation, with the
  existing tests passing unchanged.
- **FR-016**: Duplicated Rust helpers (timestamp, id, error mappers, settings schema,
  SIMBAD row parser) MUST be consolidated to a single definition each; the diverged
  database-error mapping MUST be unified to the canonical (recoverable not-found) code.
- **FR-017**: Idiomatic concurrency/caching MUST be adopted where flagged: a TTL cache
  for the debounce table, a concurrent map with RAII removal for the active-runs
  registry, and offloading of blocking filesystem work from async tasks.
- **FR-018**: Workspace structure MUST be corrected: the resolver split out of the
  pure-domain targeting crate, the pure-domain base crate promoted as the shared base,
  the oversized orchestration crate grouped/split, `tokio` removed from a pure rules
  crate, and the persistence→contracts inversion fixed — all without changing the
  on-disk/SQL representation.
- **FR-019**: Path values crossing the IPC boundary or serialized MUST use a
  UTF-8-guaranteed path type, removing lossy conversions.
- **FR-020**: Duplicated test bodies MUST be converted to parameterized tests and pure
  transforms MUST gain property tests, as dev-dependencies only.
- **FR-021**: The long-operation contract MUST be either exercised end-to-end by at
  least one real flow or removed entirely; it MUST NOT remain defined-and-unused.
- **FR-022**: Every migration MUST remove its hand-rolled predecessor (no parallel
  old+new); DB schema, IPC command semantics, and Rust domain invariants MUST be
  unchanged except for the explicitly enumerated defect fixes.
- **FR-023**: Each user story MUST be independently shippable and independently
  verifiable, and the repository MUST remain green (lint, type-check, tests, per-crate
  clippy with warnings-as-errors) after each story.

### Key Entities

- **Query key**: the identity of a cached server-data fetch (e.g. resource + params),
  used for caching and invalidation.
- **ErrorCode**: the single enumerated set of error identifiers shared across the
  Rust core and the TypeScript UI.
- **Contract type**: a generated DTO type (one source) consumed by the UI and
  validated at the boundary.
- **Shared helper home**: the single canonical location for a previously-duplicated
  helper (timestamp, id, error mapper, etc.).
- **Crate boundary**: a workspace crate with a defined responsibility and dependency
  budget (pure-domain crates stay dependency-light).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `apps/desktop/src/data/store.ts` is deleted and no module imports it;
  server state is served entirely by the data-fetching library.
- **SC-002**: Zero `as unknown as` casts exist at the IPC boundary, and the
  hand-written `bindings/types.ts` struct universe is removed (grep count = 0).
- **SC-003**: Error codes resolve to a single generated `ErrorCode` enum; no duplicated
  error-code string literal appears on either side of the boundary.
- **SC-004**: An automated check fails when the generated TypeScript bindings and the
  language-neutral schema disagree.
- **SC-005**: Every long list renders only its visible window (verifiable by DOM node
  count staying roughly constant as item count grows), and the inbox list contains no
  O(n²) per-row lookup.
- **SC-006**: Each migrated overlay closes on outside-click and Escape and passes
  keyboard navigation (the projects filter dropdown bug is fixed).
- **SC-007**: The single-definition helpers (`now_iso`/timestamp, `new_id`, the
  database-error mapper) each appear exactly once (grep count = 1 definition).
- **SC-008**: A not-found database condition maps to one consistent recoverable code at
  every call site.
- **SC-009**: Production Rust code contains no diagnostic `eprintln!`; errors crossing
  the boundary carry preserved context.
- **SC-010**: An edit to a pure-domain type does not recompile the HTTP/DB dependency
  stack (the targeting resolver split is effective).
- **SC-011**: Existing databases open and round-trip byte-identically after the
  persistence restructuring (no on-disk/SQL change).
- **SC-012**: Every adopted library has fully replaced its hand-rolled predecessor (the
  predecessor is removed), and the full discovery inventory's KEEP/DEFER/REJECT
  decisions are recorded in `research.md`.
- **SC-013**: After each user story, `just lint`, `just typecheck`, `just test`,
  per-crate `clippy -D warnings`, `tsc --noEmit`, and the touched-feature unit tests
  are all green; the end state is verified by click-through in the real Windows Tauri
  build.

## Iterations

### Iteration 2026-06-21: SC-002/FR-021 reconcile

**Triggered by**: post-merge phantom-completion audit.

**Summary**: Tasks T190–T193 (US7, SC-002 type-safety surface) and T240 (US16,
FR-021 long-op contract) were marked `[X]` on main but their success criteria
were unmet:

- **SC-002**: `bindings/types.ts` still contained 14 hand-written `export
  interface` structs across ~19 importer files, and one `as unknown as
  OperationHandle` cast remained at `commands.ts:396`.
- **FR-021**: `applyPlan` cast `PlanApplyResponse` to `OperationHandle` instead
  of returning the generated type; no real plan-apply progress consumer existed
  in the UI.

**Actions**:
- T190, T191, T192, T193, T240 re-opened (phantom — SC/FR unmet).
- T268–T272 added to complete SC-002 and FR-021 end-to-end.
- US13 crate restructuring (T250–T254) confirmed genuinely complete; T272 added
  as a verify-only checkpoint.
- No FR/SC requirement text changed (SC-002 and FR-021 were already correctly
  specified; they were simply unmet).

## Assumptions

- **Library selection is a decided input.** The specific libraries named here
  (TanStack Query/Table/Virtual, Base UI, react-hook-form, zod, use-debounce, date-fns,
  tinykeys, pathe, lucide-react; thiserror, anyhow, tracing, strum, percent-encoding,
  globset, itertools, csv, byteorder, path-clean, serde_json, camino, moka, dashmap,
  rstest, proptest) were chosen during discovery under a maximal-adoption appetite and
  the constitution's "deliberate dependencies" principle. Naming them in this spec is
  intentional, not implementation leakage; rationale per finding is in `research.md`.
- The generated specta bindings (`bindings/index.ts`) are the authoritative IPC
  contract; `commands.ts` wrappers mirror its `camelCase` argument names exactly.
- Existing test suites are sufficient guards for behavior-preserving swaps; where they
  are thin (e.g. globbing), the story adds a focused equivalence matrix.
- "Maximal adoption" is bounded by the explicit Out-of-Scope list below.
- Each library version is pinned to the current release at implementation time (checked
  via the package-version tooling before adding).

## Dependencies

- Already in the tree and reused: TanStack Router/Virtual, Base UI, clsx, the
  `notify`/`trash`/`sqlx`/`serde`/`specta`/`time`/`tokio`/`thiserror` crates.
- The generated contract pipeline (specta → `bindings/index.ts`; schema → language-
  neutral contracts) is the backbone for Stories 2 and 7.
- Story 7 depends on Story 2 (generated types must be the source of truth first).
- Stories 11 and 13 are paired (the consolidation needs the promoted base crate).
- Verification depends on the Windows Tauri build loop (push → pull → recompile →
  click-through).

## Out of Scope

- **Client state** (TanStack Router search params, React Context, localStorage prefs)
  is NOT migrated; no zustand is introduced.
- **No internal HTTP/JSON-RPC API** is introduced; Tauri IPC remains the transport and
  the language-neutral contracts preserve future portability.
- **No `walkdir`** (no recursive `read_dir` exists to replace) and **no `clap`** (no
  CLIs exist).
- **No new `support`/`common` "junk-drawer" crate** and **no merging of the thin
  adapter crates** — both are explicitly rejected; duplication is consolidated onto the
  existing pure-domain base crate and internal modules instead.
- **No change** to DB schema, IPC command semantics, Rust domain invariants, or the
  on-disk/serialized representation — except the explicitly enumerated defect fixes
  (render-fetch, unbounded caches, casing drift, diverged not-found severity, diverged
  calibration-kind fallback, overlay interaction bugs, O(n²) inbox lookup).
- `react-resizable-panels` is retained (a deferred future use), not removed.
