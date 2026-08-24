# Feature Specification: End-to-End & Integration Testing (Full App Coverage)

**Feature Branch**: `037-e2e-integration-testing`

**Created**: 2026-06-19

**Status**: Implemented (post-hoc record, verified 2026-08-24 against `origin/main`
a52c637f2). Both layers and all five user stories ship:

Three distinct test populations exist; they are **not** interchangeable, and the
CI tier names do not match this document's layer names (see the note at the head
of `contracts/coverage-matrix.md`).

- **Layer 1, real-backend integration**: the `integration` matrix job in
  `.github/workflows/ci.yml` ("Unit + integration (L1+L2)"), exercising core logic
  against real SQLite and a real temporary filesystem with the network mocked at
  its outermost boundary.
- **Layer 2, full-stack end-to-end**: the **`thirtyfour` Rust journey suite** —
  **30** tests across 14 files under `crates/e2e-tests/tests/` (plus `common/`),
  driving the built `desktop_shell` under the `e2e` feature via
  `tauri-plugin-webdriver`, with no per-OS external WebDriver. All 30 are
  `#[ignore]`d in-source, so `cargo test --workspace` never runs them; they run only
  through `.github/workflows/e2e.yml`.
- **Mock-mode UI regression** (neither spec layer): **38** Playwright `.spec.ts`
  files under `tests/e2e/`, plus `tests/e2e/support/`. Run by the `ui-e2e` job in
  `ci.yml` ("UI mock-mode (Playwright) — run") on `ubuntu-latest`, frontend-gated,
  with `ui-e2e-gate` publishing the required context. These drive the UI against
  in-process mocks, **not** a real backend, so they do not satisfy Layer 2.
- **US2, cross-platform automation** — what runs **by default** versus what is
  opt-in (`.github/workflows/e2e.yml:14-35`):
  - Every `pull_request` and `merge_group` entry: a Tier-1 real-UI **smoke subset**
    on `ubuntu-latest` only. Required context "Real-UI smoke (L3) — ubuntu-latest".
  - Every push to `main`: the Tier-2 full matrix, `ubuntu-latest` (4 shards) +
    `windows-latest` (3 shards). Windows is deliberately off PRs as the slow and
    historically flaky leg.
  - `macos-latest`: **opt-in only.** It runs on an explicit `workflow_dispatch` with
    `run_macos=true`, which defaults to `false` (`:163-168`, gated at `:303`). It is
    skipped on ordinary PR and main-push runs, so the spec's "across Windows, Linux,
    and macOS" is met on Linux and Windows automatically and on macOS on request.
- **US4, one-command local execution**: `apps/desktop/package.json`
  `test:e2e` (`playwright test`, mock mode) and `test:e2e:real`
  (`scripts/run-e2e-real.sh`); `just test-e2e`.
- **US5, standing convention**: `.claude/rules/22-astro-build-run.md:32` documents
  the two layers as the project's default way of working and points at
  `contracts/coverage-matrix.md`, which is maintained per feature area.

Correction of record: an earlier audit concluded Layer 2 had not shipped by
checking `apps/desktop/e2e`. That path **did** exist — added 2026-06-19/20 across
`329c6d11d`, `db56e34ee`, `695f668b6` and `e3793a598`, holding
`playwright.real-backend.config.ts`, `real-backend/*.spec.ts`, `helpers/` and a
`README.md` — and was removed across `e3793a598`, `7a4f70446` (2026-06-20, which
standardized real-UI E2E on `tauri-plugin-webdriver` and retired wdio) and
`a504a7fce` (2026-07-11). So the audit cited a deleted path, not one that never
existed. Its `research.md` citations of `apps/desktop/e2e/helpers/*.ts` are stale
records of that tree, not planned paths.

**Input**: User description: "Full integration testing of our app: exercise the real application (real data store, real command/IPC dispatch, real side effects) rather than mocked stubs, covering all currently implemented features, across Windows, Linux, and macOS, in both CI/CD and during local development. Update the project instructions so this is the standing convention."

## Overview

When this spec was written, the automated tests proved only that individual pieces
work in isolation: pure domain logic was unit-tested, and the desktop UI was
tested against an in-process mock that faked every backend response. Nothing
exercised the **real** running application — real data store, real command
dispatch and serialization, real filesystem and network side effects — as a user
actually experiences it. That gap meant a whole class of failures (command wiring, payload shape mismatches,
data-store migrations, state injection, UI↔backend contract drift) can pass all
existing checks and still break the shipped product. Several such failures have
already reached real builds and were only caught by hand on one developer's
machine.

This feature establishes a durable, two-layer "real-stack" testing capability
covering every implemented feature, runs it automatically on every change across
all three supported desktop operating systems, makes it runnable on demand by
any developer on their own machine, and writes the strategy into the project's
standing instructions so it is the default way the team works — not a one-off.

The two layers are:

- **Real-backend integration**: the application's core logic exercised against
  its real dependencies (real local data store, real temporary filesystem, the
  network mocked only at its outermost boundary). Fast, deterministic, carries
  the bulk of the assertions.
- **Full-stack end-to-end**: the actual built application, driven through its
  real user interface, firing real commands that run the real backend and
  produce real side effects. A thin set of smoke journeys that prove the UI,
  the command/IPC layer, and the backend are genuinely wired together.

This feature adds test coverage, supporting test fixtures, automation, and
documentation only. It does **not** change any product behavior, and it stays
within the product boundary: it never invokes image-processing tools.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Real-stack regression safety for every implemented feature (Priority: P1)

A developer changes backend logic, a command handler, a data-store query, or a
contract shape. Before the change can merge, an automated suite exercises that
area against the **real** data store and real dependencies and fails if the
end-to-end behavior regressed — catching command-wiring, payload-shape,
migration, and state-injection breakage that mock-based tests cannot see.

**Why this priority**: This is the core value and the cheapest, most reliable
layer. It closes the exact gap that has already let real defects reach builds.
Implemented alone it already delivers most of the protection, independently of
any UI automation.

**Independent Test**: Run the real-backend integration suite locally; confirm it
spins up the real data store, applies real migrations, executes representative
read/write/transition flows for each implemented feature area, and reports
pass/fail deterministically without any mocked backend responses.

**Acceptance Scenarios**:

1. **Given** a clean environment, **When** the integration suite runs, **Then**
   it provisions a real, isolated data store with real schema migrations and
   requires no pre-seeded shared state.
2. **Given** a write-then-read flow for an implemented feature (e.g. create a
   project, record a session, register a source, assign a calibration match),
   **When** the operation runs against the real store, **Then** the persisted
   result read back matches what was written.
3. **Given** a backend regression is introduced (e.g. a command stops persisting
   a field, or a payload field is renamed), **When** the suite runs, **Then** at
   least one test fails and names the affected feature area.
4. **Given** logic that calls an external network service, **When** the suite
   runs, **Then** the service is stubbed only at the network boundary while the
   application's own code path executes for real.
5. **Given** the suite completes, **When** results are reported, **Then** every
   implemented feature area is represented by at least one real-stack test and
   gaps are reported explicitly rather than silently skipped.

---

### User Story 2 - Automated cross-platform verification on every change (Priority: P1)

When a change is proposed, the full real-stack test suite runs automatically on
Windows, Linux, and macOS without anyone manually kicking it off, and the change
is blocked from merging until the suite passes on every supported platform.

**Why this priority**: The product ships on three operating systems with
genuinely different path, filesystem, and runtime behavior. Manual,
single-machine verification is how defects have slipped through. Automation
across all three is what makes the protection trustworthy and repeatable.

**Delivery note**: The fast integration layer is the P1 increment — it is wired
into CI on all three operating systems alongside User Story 1 and gates merges
immediately. The slower end-to-end CI stage depends on the end-to-end suite from
User Story 3 (P2) and lands with it; until then this story's merge gate is the
integration layer.

**Independent Test**: Open a change; observe that the automated pipeline launches
the suite on all three operating systems, reports per-platform results, and marks
the change blocked until all required platforms pass.

**Acceptance Scenarios**:

1. **Given** a proposed change, **When** the pipeline triggers, **Then** the
   real-backend integration suite runs on Windows, Linux, and macOS.
2. **Given** a platform-specific failure (e.g. a path or filesystem assumption
   that only breaks on one operating system), **When** the suite runs, **Then**
   the failing platform is reported distinctly from the passing ones.
3. **Given** the suite fails on any required platform, **When** results are
   collected, **Then** the change is reported as not mergeable.
4. **Given** the fast integration layer fails, **When** the pipeline runs,
   **Then** it surfaces that failure before spending time on the slower
   end-to-end layer.
5. **Given** the pipeline finishes, **When** a developer inspects it, **Then**
   per-platform, per-layer results are visible without re-running anything.

---

### User Story 3 - Full end-to-end smoke journeys through the real application (Priority: P2)

A small set of automated journeys drives the **actual built application** through
its real interface — real clicks and inputs that fire real commands, run the real
backend, and produce real side effects — proving that the UI, the command layer,
and the backend are connected end to end for the primary user workflows.

**Why this priority**: Integration tests (US1) cover backend behavior but cannot
prove the shipped UI actually reaches the backend. A thin end-to-end layer is the
only thing that catches "the button is wired to nothing" and contract drift
between the generated UI bindings and the backend. It is high-value but slower
and more fragile, so it stays a smoke layer, not the bulk of assertions.

**Independent Test**: Launch the built application under UI automation, perform a
primary journey (e.g. complete first-run setup, search and resolve a target,
create a project, review and apply a filesystem plan), and assert the on-screen
result reflects a value that round-tripped through the real backend.

**Acceptance Scenarios**:

1. **Given** the built application, **When** a primary journey is driven through
   the real UI, **Then** an action taken in the UI produces a visible result that
   came back through the real backend (not a fixed mock value).
2. **Given** an end-to-end journey that performs a reviewable filesystem
   operation, **When** the plan is applied through the UI, **Then** the real side
   effect occurs and a corresponding durable audit record exists.
3. **Given** each implemented top-level feature area has a navigable screen,
   **When** the smoke suite runs, **Then** every such screen loads in the real
   application without error.
4. **Given** a UI binding no longer matches the backend command it targets,
   **When** the smoke suite runs, **Then** at least one journey fails.
5. **Given** an operating system where the real interface cannot be driven by the
   available automation, **When** the suite runs there, **Then** the end-to-end
   layer is reported as not-applicable for that platform (explicitly, not as a
   false pass) while that platform's integration layer still runs in full.

---

### User Story 4 - One-command local execution for developers (Priority: P2)

A developer on Windows, Linux, or macOS can run the integration suite and the
end-to-end smoke suite on their own machine with a single, documented command per
layer, getting the same real-stack coverage locally that the pipeline runs —
before they ever push.

**Why this priority**: Coverage that only runs in the pipeline pushes the
feedback loop to the end. Local runnability shortens it and is what makes the
suites part of everyday work rather than a gate developers resent.

**Independent Test**: On each operating system, run the documented command for
each layer from a clean checkout and confirm it executes the real-stack suite and
reports results without bespoke per-machine setup beyond documented prerequisites.

**Acceptance Scenarios**:

1. **Given** a clean checkout, **When** a developer runs the documented
   integration command, **Then** the real-backend suite runs locally.
2. **Given** a clean checkout with documented prerequisites installed, **When** a
   developer runs the documented end-to-end command, **Then** the smoke suite
   drives the real application locally.
3. **Given** a required prerequisite is missing, **When** the command runs,
   **Then** it fails with a clear message naming the missing prerequisite rather
   than failing obscurely.
4. **Given** the same suite, **When** it is run locally and in the pipeline,
   **Then** it exercises the same layers and journeys.

---

### User Story 5 - Testing strategy is the documented standing convention (Priority: P3)

The two-layer real-stack strategy, what each layer covers, how to run each layer
on each operating system, the per-platform expectations and caveats, and the rule
that new features ship with real-stack coverage are written into the project's
standing instructions and contributor documentation.

**Why this priority**: Without documentation the capability decays — new features
ship without coverage and the per-platform caveats get rediscovered painfully.
Documentation is what turns a one-time build-out into a durable convention. It is
P3 because it depends on the layers it describes existing first.

**Independent Test**: A contributor unfamiliar with the suite reads the project
instructions and contributor testing doc and can state which layer to add a test
to for a given change, and how to run it on their operating system.

**Acceptance Scenarios**:

1. **Given** the project instructions, **When** a contributor reads them, **Then**
   the two-layer strategy and the expectation that new features include
   real-stack coverage are stated.
2. **Given** the contributor testing doc, **When** a contributor reads it, **Then**
   it explains each layer, the command to run it per operating system, and the
   per-platform caveats (including where the end-to-end layer does not apply).
3. **Given** a developer command list, **When** a contributor inspects it, **Then**
   the documented commands for each layer are present and named consistently with
   the documentation.

---

### Edge Cases

- **Network unavailable during tests**: integration tests that touch an external
  service must remain deterministic by stubbing at the network boundary; they
  must not fail merely because the machine is offline.
- **Stale build driving end-to-end tests**: the end-to-end layer must run against
  a freshly built application, never a previously built one, so a passing result
  cannot reflect outdated code.
- **Shared/dirty data store between tests**: each test must run against isolated
  data so order and leftover state from a prior test cannot cause false passes or
  flaky failures.
- **Platform that cannot drive the real UI**: the end-to-end layer must report
  not-applicable on such a platform rather than silently passing, and integration
  coverage on that platform must remain complete.
- **Destructive/irreversible operations under test**: tests covering archive,
  delete, move, or other mutations must operate only inside disposable test
  locations and must assert the audit record, never touch real user libraries.
- **A feature with no test**: an implemented feature lacking real-stack coverage
  must surface as an explicit reported gap, not an absence that looks like
  success.
- **Long-running operations**: a journey that triggers background or
  long-running work must be able to wait for and assert the eventual outcome
  rather than racing it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The test capability MUST exercise the application against its real
  dependencies (real local data store, real schema migrations, real temporary
  filesystem), without mocked backend responses, for the integration layer.
- **FR-002**: Each implemented feature area MUST be covered by at least one
  real-stack test, and any uncovered implemented feature area MUST be reported as
  an explicit gap.
- **FR-003**: External network dependencies MUST be isolated only at the network
  boundary, so the application's own logic executes for real and tests stay
  deterministic offline.
- **FR-004**: The integration layer MUST provision isolated, disposable test data
  per run (or per test) so results do not depend on shared or leftover state.
- **FR-005**: The end-to-end layer MUST drive the actual built application through
  its real interface so that real user actions fire real commands, run the real
  backend, and produce real side effects.
- **FR-006**: The end-to-end layer MUST run against a freshly built application
  and MUST NOT accept a stale prior build.
- **FR-007**: Every implemented top-level feature screen MUST be reachable and
  load without error in at least one end-to-end smoke journey.
- **FR-008**: At least one end-to-end journey MUST assert a value that
  round-trips from the UI through the real backend and back, proving the layers
  are connected (i.e. not a fixed mock value).
- **FR-009**: At least one end-to-end journey MUST cover a reviewable filesystem
  mutation applied through the UI and MUST assert both the real side effect and a
  durable audit record.
- **FR-010**: The full real-stack suite MUST run automatically on every proposed
  change on Windows, Linux, and macOS, and MUST block merge until it passes on
  every **required platform**. *Required platform* is defined per layer: the
  integration layer is required on all three operating systems; the end-to-end
  layer is required on Windows and Linux and is **best-effort (non-merge-blocking)
  on macOS** (see FR-013 and the Assumptions). A macOS end-to-end failure therefore
  does not block merge, while a macOS integration failure does.
- **FR-011**: Per-platform and per-layer results MUST be reported distinctly so a
  platform-specific failure is attributable without re-running.
- **FR-012**: The fast integration layer MUST run before the slower end-to-end
  layer so backend failures surface first.
- **FR-013**: Where a platform cannot drive the real UI with the available
  automation, the end-to-end layer MUST report not-applicable for that platform
  explicitly (never a false pass) while that platform's integration layer runs in
  full.
- **FR-014**: Each layer MUST be runnable locally on Windows, Linux, and macOS via
  a single documented command, matching what the pipeline runs.
- **FR-015**: A missing local prerequisite MUST cause a clear, named failure
  rather than an obscure one.
- **FR-016**: Tests covering destructive or irreversible operations MUST operate
  only within disposable test locations and MUST NOT touch real user libraries.
  (FR-016 is the sandboxing guarantee — *where* mutations may happen; FR-009
  separately requires *asserting the audit record* a mutation produces.)
- **FR-017**: The two-layer strategy, per-layer coverage, per-platform run
  instructions and caveats, and the expectation that new features ship with
  real-stack coverage MUST be documented in the project's standing instructions
  and a contributor testing document.
- **FR-018**: The capability MUST NOT alter product behavior and MUST NOT invoke
  external image-processing tools, preserving the product boundary.
- **FR-019**: A coverage mapping MUST exist that ties each implemented feature
  area to the real-stack test(s) that cover it, so coverage gaps are auditable.

### Key Entities *(include if feature involves data)*

- **Implemented Feature Area**: a user-facing capability already shipped (e.g.
  first-run setup, target lookup/resolution, inbox split, calibration matching,
  sessions, projects & manifests, filesystem plans, archive/cleanup, processing
  tool launch/observation, settings, log viewer) that requires real-stack
  coverage.
- **Integration Test**: a real-stack check of backend behavior against real
  dependencies for one feature area, with isolated test data.
- **End-to-End Journey**: a smoke path through the real built application proving
  UI↔command↔backend connectivity for a primary workflow.
- **Supported Platform**: one of Windows, Linux, macOS, each with its own run
  expectations and caveats.
- **Test Fixture / Disposable Test Data**: isolated data store and temporary
  filesystem content created for a run and discarded after.
- **Coverage Mapping**: the auditable record linking each implemented feature
  area to its covering real-stack test(s).
- **Audit Record**: the durable record a mutating operation must produce, asserted
  by tests covering side effects.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of implemented feature areas have at least one real-stack
  (integration or end-to-end) test, and any gap is reported explicitly.
- **SC-002**: The full real-stack suite runs automatically on Windows, Linux, and
  macOS for every proposed change, with zero changes able to merge while a
  required platform's suite is failing.
- **SC-003**: 100% of implemented top-level feature screens load without error in
  the end-to-end smoke suite on every platform where the end-to-end layer applies.
- **SC-004**: At least one end-to-end journey proves a UI-to-backend round trip
  and at least one proves a real filesystem mutation plus its audit record.
- **SC-005**: A developer on any supported operating system can run each layer
  from a clean checkout with a single documented command and documented
  prerequisites, with no undocumented per-machine setup.
- **SC-006**: The integration suite is deterministic — it passes offline and does
  not exhibit order-dependent or shared-state flakiness across repeated runs.
- **SC-007**: Every regression deliberately introduced into a covered backend
  behavior is caught by at least one failing real-stack test (validated with
  representative seeded regressions).
- **SC-008**: The two-layer strategy and per-platform run instructions are present
  in the standing project instructions and contributor testing document, and a
  contributor can determine which layer to extend for a given change from the docs
  alone.

## Assumptions

- The application targets desktop on Windows, Linux, and macOS, and these three
  are the platforms that must be covered. Windows is treated as a first-class
  primary platform.
- "All currently implemented features" means the feature areas delivered by the
  merged specifications (roughly 001–035) plus any merged at the time of
  implementation; the in-progress feature on the current working branch is
  included once merged.
- Real-stack coverage is achievable for the integration layer on all three
  operating systems, so the integration layer is **required** everywhere. The
  **end-to-end** (real-UI) layer is required on Windows and Linux; on macOS it is
  **best-effort / non-merge-blocking** because the platform's available UI
  automation is newer and less stable (resolved in research, decision D4). FR-013
  governs the explicit not-applicable reporting if the macOS end-to-end path is not
  yet adopted. The spec does not mandate a specific automation tool.
- Existing fast unit tests and mock-based UI tests remain valuable and are kept;
  this feature adds the real-stack layers rather than replacing existing tests.
- Existing scaffolding for a real-backend end-to-end harness already present in
  the repository may be completed and reused rather than rebuilt from scratch.
- Test data and fixtures live in disposable locations; no real user astrophotography
  libraries are touched by any test.
- The project's standing instruction files are generated from a source of truth;
  documentation updates are made at that source so they survive regeneration.

## Out of Scope

- Performance, load, or stress testing, and benchmarking of the application.
- Accessibility/WCAG conformance testing (covered by the prior UI/accessibility
  work).
- Validation of image-processing correctness or any behavior owned by external
  processing tools (outside the product boundary).
- Visual-regression / pixel-diff testing of the UI.
- Changing or adding product features; this feature is test, automation, and
  documentation only.
- Release/packaging/signing pipelines beyond what is needed to build the
  application for end-to-end runs.
- Mobile or web deployments of the application.
