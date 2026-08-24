# Feature Specification: IPC wrapper removal (adopt generated bindings)

**Feature Branch**: `037-ipc-wrapper-removal`

**Created**: 2026-06-19

**Status**: Draft — **remains open**, but rescoped 2026-08-24 against `origin/main`
a52c637f2. `apps/desktop/src/api/commands.ts` was deleted (1673 lines) at
`54b56d284` (2026-07-05, PR #439) and the switcher this spec proposed now exists
as `apps/desktop/src/api/ipc.ts:61`. The remaining subject is **24** hand-written
wrappers in two feature modules, not ~90 in `commands.ts`: see the rescoped
Background below.

**Input**: Remove the hand-written `apps/desktop/src/api/commands.ts` `invoke()` wrappers and migrate the desktop app to call the generated tauri-specta bindings directly, eliminating the IPC name/payload drift bug class while preserving mock mode and the dev-tools recording proxy.

## Background

### As written (2026-06-19)

The desktop app talked to the Rust backend through ~90 hand-written
`invoke('<name>', <payload>)` wrappers in `apps/desktop/src/api/commands.ts`. The generated tauri-specta bindings (`bindings/index.ts`,
`export const commands`) already describe every command's exact name and camelCase payload
shape, but the wrappers re-declare both by hand and silently drift. Because mock mode
(`VITE_USE_MOCKS`) short-circuits `invoke()`, drift is invisible on Linux/CI and only surfaces
on a real Windows build. Three drift incidents shipped this cycle:

- Dotted command names registered as snake_case (`target.get` → "command not found"). (Fixed.)
- snake_case payload keys the camelCase backend rejects (`scan_depth`, `session_id`, `root_id`,
  …) making whole command args fail to deserialize. (Fixed, 8 commands, #263/#264.)
- A CI guard now blocks both classes (`commands.bindings-guard.test.ts`, #265).

The guard prevents regressions but the wrappers remain the root cause. This feature removes
them so the generated bindings are the single source of truth.

### Rescoped (2026-08-24, verified against `origin/main` a52c637f2)

`commands.ts` no longer exists — it was deleted, 1673 lines, at `54b56d284`
(2026-07-05, PR #439), sixteen days after this spec was created. The switcher this
spec proposed shipped as `apps/desktop/src/api/ipc.ts:61`.

The remaining subject is **24** hand-written `invoke()` wrappers in two feature
IPC modules, both importing `invoke` from the switcher (`@/api/ipc`):

- `apps/desktop/src/features/sessions/sessionsGroupsIpc.ts` — 15
- `apps/desktop/src/features/calibration/calibrationHandoffIpc.ts` — 9

So US1's Independent Test does **not** yet pass. Two further `invoke(` sites are
**not** in scope and must not be counted as wrappers:

- `apps/desktop/src/dev/ContractsPage.tsx:142` — a deliberate *dynamic*
  `invoke(cmd, …)` replaying a recorded call, which is the spec-021 dev-tools
  surface this migration is required to preserve (and is `dev-tools`-gated).
- `apps/desktop/src/data/logSubscription.ts:52` — a comment, not a call; that
  module already routes through the generated binding.

### Constraint that makes this non-trivial

`bindings/index.ts` is generated and imports `invoke` **directly** from `@tauri-apps/api/core`.
The hand-written `invoke()` in `commands.ts` is a **switcher** the whole app depends on:

- routes to `mocks.ts` `mockInvoke` when `VITE_USE_MOCKS` (mock-driven dev + most tests),
- routes through `_invokeOverride` for the spec-021 dev-tools recording proxy (release-safe, gated).

Calling generated `commands.*` directly would bypass both. The migration MUST preserve mock
mode and the recording proxy.

## User Scenarios & Testing *(mandatory)*

("Users" here are the maintainers of this codebase; the value is reliability and lower
maintenance cost.)

### User Story 1 - Single source of truth for IPC calls (Priority: P1)

A developer adds or changes a backend command and the frontend call cannot drift from the
backend contract: there is exactly one place (the generated bindings) that defines each
command name and payload shape, and the app calls it.

**Why this priority**: This is the whole point — it eliminates the recurring class of
mock-hidden, Windows-only IPC failures that have cost real debugging time.

**Independent Test**: Grep the app for `invoke('` string literals outside the generated
bindings / the switcher module — there are none. Every feature call goes through a typed
generated function.

**Acceptance Scenarios**:

1. **Given** the migration is complete, **When** a backend command is renamed and bindings are
   regenerated, **Then** the frontend fails to typecheck at the call site (compile-time safety),
   rather than failing at runtime only on Windows.
2. **Given** a command's payload field is camelCase in the contract, **When** a developer calls
   it, **Then** the payload key is supplied by the generated typed signature, not hand-written.

### User Story 2 - Mock mode and dev recorder keep working (Priority: P1)

A developer runs the app with `VITE_USE_MOCKS=true` and every screen still works against the
mock layer; in a dev-tools build the recording proxy still captures every IPC call.

**Why this priority**: Mock mode is how the app is developed and tested without a backend;
the recording proxy (spec 021) is an existing capability. Breaking either is unacceptable.

**Independent Test**: Run the full vitest suite (mock-backed) — all pass. Boot a dev-tools
build, exercise a few screens, confirm `dev.calls.list` shows the calls.

**Acceptance Scenarios**:

1. **Given** `VITE_USE_MOCKS=true`, **When** any migrated call runs, **Then** it is served by
   `mockInvoke`, not a real `__TAURI_INVOKE`.
2. **Given** a dev-tools build with the recorder installed, **When** a migrated call runs,
   **Then** the recording proxy observes it.

### User Story 3 - Stable call sites during migration (Priority: P2)

The migration lands incrementally without a single mega-PR that rewrites all 56 caller files
and their test mocks at once.

**Why this priority**: A 56-file big-bang is high-risk, especially mid-Windows-validation.
Incremental delivery keeps `main` releasable at every step.

**Independent Test**: Each PR in the series keeps `just lint`, `just test`, `just typecheck`,
and the full vitest suite green.

**Acceptance Scenarios**:

1. **Given** a partially migrated app, **When** the suite runs, **Then** it is green (mixed
   wrapper + generated calls coexist).
2. **Given** the `@/api/commands` public surface, **When** it is retired, **Then** no remaining
   imports reference it.

### Edge Cases

- Generated functions return a `Result`-style `{ status: 'ok' | 'error' }`; callers today expect
  throw-on-error. The migration MUST translate (unwrap → return data / throw error) so caller
  semantics are unchanged.
- Some wrappers post-process responses (mapping, defaulting). That logic must be preserved or
  moved, not dropped.
- Test files that `vi.mock('@/api/commands', …)` must be migrated to mock the new call path.
- Dead plumbing surfaced by the audit MUST be removed, not carried forward: `approvePlan`
  (no caller), and the unused `filters`/`sort`/`group_by` args on `listSessions` /
  `listCalibrationMasters` (no caller; `sessions_list` is a fixture stub — real filtering is
  spec-029, out of scope here).

## Requirements *(mandatory)*

- **FR-001**: The generated tauri-specta bindings MUST be the only definition of command names
  and payload shapes used by the app.
- **FR-002**: The generated bindings' invoke dispatch MUST route through the existing switcher
  so `VITE_USE_MOCKS` mock mode and the spec-021 recording proxy continue to function
  unchanged. (e.g. configure tauri-specta to import its `__TAURI_INVOKE` from an app module
  that delegates to mock/override/real, instead of `@tauri-apps/api/core`.)
- **FR-003**: Calling a migrated command MUST preserve the current throw-on-error contract
  (the generated `Result` is unwrapped: data returned on ok, error thrown on error).
- **FR-004**: Response post-processing currently done in wrappers MUST be preserved.
- **FR-005**: The migration MUST be delivered in incremental PRs, each keeping all gates green.
- **FR-006**: Dead wrappers/args identified in the audit MUST be removed during the migration.
- **FR-007**: When the wrapper layer is retired, no source or test file may import
  `@/api/commands`, and no `invoke('...')` string literal may exist outside the switcher module.
- **FR-008**: The release build MUST NOT include the dev-tools recorder by default (the
  `dev-tools` feature gate from spec 021 is preserved).
- **FR-009**: The existing `commands.bindings-guard.test.ts` MUST be removed or repurposed only
  once the wrappers it guards no longer exist (it is obsolete when there are no hand-written
  invoke strings to drift).

## Success Criteria *(mandatory)*

- **SC-001**: Zero `invoke('...')` string literals outside the dispatch switcher module after
  completion (grep-verifiable).
- **SC-002**: Renaming a backend command and regenerating bindings produces a **compile-time**
  failure at every affected call site (no silent runtime-only failure).
- **SC-003**: Full vitest suite passes in mock mode at every PR in the series and at completion.
- **SC-004**: A dev-tools build records migrated IPC calls via the recording proxy.
- **SC-005**: No `@/api/commands` imports remain at completion.

## Scope

**In scope**: removing the hand-written wrappers; redirecting generated-bindings dispatch
through the mock/override switcher; unwrapping Result→throw; migrating callers + test mocks;
removing audit-identified dead plumbing.

**Out of scope**: wiring real `sessions_list` persistence and list filtering/sort/group-by
(spec-029); any backend command behavior change; new commands.

## Assumptions

- tauri-specta can be configured (template/runtime-import) to emit a custom invoke import, or a
  thin generated-bindings wrapper module can re-export `commands.*` bound to the switcher.
- The generated `Result`/`typedError` shape is stable across regeneration.
- Greenfield: no external consumers of `@/api/commands`.
