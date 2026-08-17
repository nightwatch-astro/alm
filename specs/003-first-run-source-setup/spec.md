# Feature Specification: First-Run Source Setup

> **⚠ Partial supersession (2026-06-18)**: the "Download Catalogs" wizard step (backed by spec 014's
> `catalog.manifest.fetch` / `catalog.download`) is superseded by [Spec 035 — SIMBAD Target Resolution](../035-simbad-target-resolution/spec.md).
> Under 035 there is no catalog download at first run; if a catalog/resolver step remains it becomes a
> lightweight SIMBAD-reachability check. All other first-run setup steps are unaffected.

> **UI Revised**: The UI design in this spec has been revised by
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md).
> When implementing, follow spec 030 for layout, navigation, and component patterns.

**Feature Branch**: `003-first-run-source-setup`  
**Created**: 2026-05-09  
**Last Updated**: 2026-05-26  
**Status**: Implemented  
**Input**: User description: "Specify the one-time setup wizard for selecting initial data sources, validating selections, starting guided first steps, and restarting setup later."

## Implementation Status

The wizard is defined in
`apps/desktop/src/features/setup/SetupWizard.tsx`, with step components under
`apps/desktop/src/features/setup/steps/`. Roots and scan are backed by real
persistence, not stubs.

Shipped step sequence (`SetupWizard.tsx:86` `STEPS`, `steps/index.ts`):

1. `StepLanguage` (spec 061)
2. `StepTheme` (spec 055)
3. `StepSourceFolders`
4. `StepTools`
5. `StepCatalogs`: configuration screen: SIMBAD online-resolution toggle,
   density, default protection, scan depth, theme. No catalog download, per
   spec 035's SIMBAD pivot.
6. `StepSite`
7. `StepConfirm`
8. `StepScan`

FR-009, FR-011, FR-014, and FR-015 below describe the original step-sequence
design superseded by specs 030/035/038/044/055/061. `apps/desktop/src/api/
commands.ts` was deleted by spec 037's generated-bindings migration. Read the
step components for shipped behavior. Tracked as issue #764.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Land On The Wizard On First Launch (Priority: P1)

As a new user opening Astro Library Manager for the first time, I want the app
to take me straight to setup so that I don't accidentally land on an empty
Sessions view that has nothing to show.

**Why this priority**: Without a first-run gate, the app's primary surfaces
(Sessions, Calibration, Projects) have no source roots to scan and look broken.

**Independent Test**: Clear the library state, open the app, and confirm the
index route redirects to `/setup` and the wizard opens on the Welcome step.

**Acceptance Scenarios**:

1. **Given** a fresh install (no completion flag), **When** the user opens
   the app, **Then** the index route navigates to `/setup` and the Welcome
   step renders first.
2. **Given** a completed first-run (flag set), **When** the user opens the
   app, **Then** the index route navigates to `/sessions` and the wizard is
   not shown.
3. **Given** the wizard is open, **When** the user moves through steps,
   **Then** progress is visible via a stepper and step counter (`Step N of M`).

---

### User Story 2 - Register Source Roots By Category (Priority: P2)

As a new user, I want to register source directories grouped by category (Raw,
Calibration, Project, Inbox) so that downstream scanning, calibration matching,
and project envelopes know where to look.

**Why this priority**: Source roots are the entry point for every other
workflow. Without them the rest of the app is inert.

**Independent Test**: Walk through the four source steps, add at least one Raw
source and zero or more of each other kind, finish the wizard, and confirm the
registered sources are visible (in localStorage during the mockup; in the
library DB once the persistence task lands).

**Acceptance Scenarios**:

1. **Given** the Raw Sources step with zero entries, **When** the user
   clicks Next, **Then** the wizard blocks advancement and surfaces "No raw
   sources yet. Add at least one to continue."
2. **Given** the Raw Sources step with at least one entry, **When** the user
   clicks Next, **Then** the wizard advances to Calibration Sources.
3. **Given** the Calibration or Inbox steps with zero entries, **When** the
   user clicks Next, **Then** the wizard advances without blocking. The
   Project step REQUIRES at least one Project source before advancing:
   a project source root is mandatory because downstream project workflows
   expect one (R-Wiz-2). The wizard blocks advancement from the Project
   step with inline copy: "At least one Project source is required."
4. **Given** a source step with a directory chosen via the native picker,
   **When** the user adds it, **Then** the directory appears in the list for
   that kind and can be removed inline.
5. **Given** the Finish step, **When** the user clicks Finish, **Then** the
   completion flag is set, registered sources are persisted to the library
   store, and the app navigates to `/sessions`.

---

### User Story 3 - Restart Setup From Settings (Priority: P3)

As a user who has already completed setup, I want a clear entry point to
restart the wizard so that I can correct a wrong source or onboard a new drive
without hunting for hidden affordances.

**Why this priority**: Users explicitly asked how to reach setup again after
finishing it; without an obvious entry point they reach for app uninstall.

**Independent Test**: Complete setup, open Settings, click "Restart first-run
wizard", and confirm the wizard opens at the Welcome step.

**Acceptance Scenarios**:

1. **Given** a completed first-run, **When** the user clicks "Restart
   first-run wizard" in Settings, **Then** the completion flag is cleared
   and the app navigates to `/setup`.
2. **Given** the restarted wizard, **When** the user completes it again,
   **Then** the new source set is persisted and the completion flag is set.
3. **Given** the restart action, **When** it executes, **Then** the completion
   flag is cleared and the wizard opens with the previously registered sources
   prefilled into the working buffer for editing (A7). The existing destructive
   reset in Settings is replaced by the prefill flow. The Tauri command is
   `firstrun.restart` (dotted name, A7, R-E5).

---

### User Story 4 - Understand Each Source Category (Priority: P4)

As a new user unfamiliar with the app, I want each source step to explain what
that category is for and what I should select so that I don't conflate Raw with
Calibration or pick the wrong root.

**Why this priority**: Wrong source assignment at first-run propagates into
calibration matching errors and project envelope confusion later, which is
expensive to undo.

**Independent Test**: Open each source step and confirm the step copy explains
the category, gives an example of what to select, and clarifies whether the
step is required.

**Acceptance Scenarios**:

1. **Given** the Raw step, **When** it renders, **Then** the copy explains
   raw lights/darks/flats/bias storage and states that the step is required.
2. **Given** the Calibration step, **When** it renders, **Then** the copy
   explains calibration libraries (masters) and states that it is optional.
3. **Given** the Project step, **When** it renders, **Then** the copy
   explains per-project envelopes and notes that project creation happens
   later in the guided Projects workflow.
4. **Given** the Inbox step, **When** it renders, **Then** the copy explains
   that inbox folders are watched for newly-captured data.

### Edge Cases

- Duplicate source paths across kinds (same root listed as both Raw and
  Calibration). RESOLVED: reject with `path.already_registered.different_kind`
  error code (R-1.4). The error is surfaced inline next to the offending row
  with the conflicting kind shown.
- Duplicate source paths within a kind (mockup currently allows; the picker
  stub avoids re-suggesting, but manual entry would not).
- Path no longer exists at Finish time (drive disconnected).
- Path is a symlink or junction.
- Path requires elevated permissions to read.
- Path is on a network share that mounts lazily.
- User clicks the cancel button on the native picker dialog.
- Very large source root with millions of files (validation must not block on
  recursive enumeration).
- Mixed folders that contain both lights and calibration frames.

### Domain Questions — Resolved

- Per-row scan rule: RESOLVED — wizard shows scan-depth as advanced/collapsed
  disclosure only; default is `recursive` (R-Wiz-1).
- Finish atomicity: RESOLVED — per-source calls with row-level partial success
  via `source.register.batch` contract (R-Batch, A9).
- Skip entire wizard: RESOLVED — rejected. Raw is required; there is no global
  skip path (R-Wiz-3).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The wizard MUST be a sequential page-by-page flow accessed at
  `/setup`, not a permanent navigation entry in the main shell.
- **FR-002**: The wizard MUST treat the Raw Sources step as required (at
  least one Raw source) and the Project Sources step as required (at least
  one Project source — R-Wiz-2). Global skip of the entire wizard is
  rejected: the app is inert without a Raw source and downstream project
  workflows require a project source root (R-Wiz-3). There is no "I'll add
  sources later" escape hatch.
- **FR-003**: The wizard MUST NOT allow advancing from the Raw step until at
  least one Raw source is registered.
- **FR-004**: The wizard MUST allow advancing from Calibration and Inbox steps
  without entries (they are optional). The Project step is required (at least
  one entry) — see FR-002 and R-Wiz-2. Spec 010 cross-reference: the
  first-project flow can now assume a project source exists.
- **FR-005**: The wizard MUST validate that each chosen path exists, is a
  directory, and is readable before accepting it. [Not yet implemented; stub
  picker bypasses validation.]
- **FR-006**: The wizard MUST detect and surface duplicate source paths
  within the same kind.
- **FR-007**: The wizard MUST NOT copy, move, modify, hash, or scan files
  during the wizard run. Indexing is deferred to the Inventory workflow.
- **FR-008**: The wizard MUST use the operating system's native directory
  picker (via Tauri's `plugin-dialog`). The `DirPicker` component is already
  wired to `@tauri-apps/plugin-dialog` and is retained as-is.
- **FR-009**: First-run setup MUST include source steps for Raw Sources,
  Calibration Sources, Project Sources, and Inbox Sources in that order,
  followed by a Detect Tools step and a Download Catalogs step before Finish.
  Wizard step sequence: Welcome → Raw → Calibration → Project → Inbox →
  Detect Tools → Download Catalogs → Finish (A5, A6).
- **FR-010**: First-run setup MUST explain on the Project step that project
  creation itself happens later in the guided Projects workflow.
- **FR-011**: First-run setup MUST start with a Welcome step that explains
  setup scope, that nothing is moved or modified, and that all choices are
  changeable later.
- **FR-012**: Each source selection step MUST include explanatory copy that
  defines the category and tells the user what kind of directory to select.
- **FR-013**: The wizard MUST be restartable from Settings via a clearly
  labeled control. Restart calls the `firstrun.restart` Tauri command
  (dotted name) and prefills existing registered sources into the wizard
  working buffer (A7, R-E5).
- **FR-017**: Each source-entry row in the wizard MAY expose a scan-depth
  selector (Recursive / Single-level) as advanced/collapsed disclosure.
  Default is `recursive`. The disclosure is hidden by default; users access
  it via an "Advanced" expander on the row (R-Wiz-1).
- **FR-018**: The Detect Tools step MUST list discovered processing tools
  (PixInsight, Siril, planetary tools) read from the tool-discovery service
  (spec 011). The user confirms or edits the tool list before Finish (A5).
  Until spec 011 is implemented, this step renders with stub/placeholder UI
  and fixture data.
- **FR-019**: The Download Catalogs step MUST offer OpenNGC download by
  default (spec 014). The user can skip and download later from Settings.
  The download may run asynchronously after Finish if the user skips (A6).
  Until spec 014 is implemented, this step renders with stub/placeholder UI
  and fixture data.
- **FR-014**: On Finish, the wizard MUST set a persistent completion flag and
  navigate the user to the Sessions surface (`/sessions`).
- **FR-015**: While the wizard is in progress, the working source list MAY
  be held in volatile UI state and `localStorage` (key
  `alm-setup-wizard-state`) for resilience against refresh, but MUST be
  promoted to the library database on Finish.
- **FR-016**: The index route (`/`) MUST redirect to `/setup` when the
  completion flag is absent and to `/sessions` when present. The gate uses
  DB-first with localStorage cache for synchronous render; the localStorage
  flag is retained as a cache layer, not eliminated (A8). The route MUST
  show a loading/pending state while the DB-first reconcile resolves to
  avoid a flash of the wrong route.

### Key Entities

- **Registered Source**: Persistent record of a directory the app should
  treat as an input root, with a kind (Raw / Calibration / Project / Inbox),
  an absolute path, a `scan_depth` (`recursive` | `single`, default
  `recursive` — R-Wiz-1), and creation metadata. Multiple per kind allowed.
- **First-Run State**: Persistent flag indicating whether the wizard has
  been completed, plus the working source list buffer used during wizard
  progression.
- **Source Category**: One of Raw, Calibration, Project, Inbox. Determines
  step copy, required/optional gating, and downstream consumers.
- **Setup Session**: The current run through the wizard, including which
  step is active and which sources have been added in this run.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A new user can complete first-run setup end-to-end with valid
  source directories in under 5 minutes.
- **SC-002**: A blocked Raw step surfaces the gating reason inline within the
  step body (not in a separate toast or modal).
- **SC-003**: Finishing setup makes the index route resolve to `/sessions`
  without further prompts on the next launch.
- **SC-004**: Restarting setup from Settings opens the wizard in under 1
  second and clears the completion flag deterministically.

## Assumptions

- Project creation happens after setup in the guided first-project workflow,
  not inside the wizard.
- Source roots are directories only, not individual files or archives.
- The user will be running a desktop OS with a native directory picker.

## Clarifications

### Session 2026-05-26

- Q: Should the wizard use 8 separate steps (one per source category + Detect Tools + Download Catalogs) or the current 5-step unified design? → A: 8-step design per spec (Welcome → Raw → Calibration → Project → Inbox → Detect Tools → Download Catalogs → Finish). The current unified Sources step will be refactored.
- Q: Should routes use `/welcome` + `/inventory` (spec) or `/setup` + `/sessions` (codebase)? → A: Keep current paths — `/setup` for wizard, `/sessions` as post-setup landing.
- Q: Should commands use the spec's `source_register` / `firstrun_*` names or the existing `roots.*` dotted namespace? → A: Use existing `roots.*` namespace, add `roots.register.batch`, `firstrun.complete`, `firstrun.restart` as dotted-name Tauri commands.
- Q: Should the first-run gate be DB-backed or localStorage-only? → A: DB-backed `FirstRunState` table with localStorage as synchronous cache for the route gate.
- Q: Should Detect Tools and Download Catalogs steps be deferred or stubbed? → A: Stub both steps with placeholder UI and fixture data; wire real backends when specs 011/014 land.

## Out of Scope

- Creating the first project.
- Scanning, hashing, or indexing files.
- Moving data into Inventory.
- Applying any filesystem mutations.
- A guided post-setup coach (deferred; spec previously hinted at one but the
  mockup ends at Inventory).
