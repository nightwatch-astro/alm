# Feature Specification: Processing Tool Launch

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `011-processing-tool-launch`
**Created**: 2026-05-09
**Last Amended**: 2026-07-03
**Status**: **Implemented** (closed 2026-07-03). Launch pipeline, per-tool profiles, cwd-containment guard, detach + pid tracking, and UI (launch CTA + relaunch modal in `ProjectDetail.tsx`, launch-history `ToolLaunchesAccordion`) shipped and tested (FakeSpawner unit + integration; migration `0024_tool_launches.sql`). FR-001…FR-011 satisfied. T021 (one-time cwd-anchored first-launch hint) implemented in closeout; doc cross-links (X-1/X-2) done. Remaining open tasks are DEFERRED — env-blocked test infra only: T018 Playwright settings-flow smoke and T022 stub-binary real-spawn tests (both blocked on WSL/sandbox, not missing logic). Satisfies spec 012's dependencies (`launch_id` handle, `tool_launches.completed_at`, accordion surface).
**Input**: User description: "Specify direct tool launch for project workflows,
including configured paths for PixInsight, Siril, and future tools, with
project-level actions."

## Implementation Status: Implemented (2026-07-03)

Superseding the original "CTA visual only" note: tool launch is fully wired and
spawns a real OS process. The launch CTA and relaunch modal live in
`apps/desktop/src/features/projects/ProjectDetail.tsx` → `useToolLaunch`
(`tool-launch.ts`); the Tauri command `tools_launch`
(`apps/desktop/src-tauri/src/commands/tools.rs`) uses the production
**`RealSpawner`** (`crates/workflow/profiles/src/launch.rs`), which calls
`std::process::Command::new(executable).spawn()` with platform detach handling
(Windows `DETACHED_PROCESS`, unix `process_group`), reads the resolved tool
path, sets the project working directory (cwd-containment guarded), tracks the
pid, and writes an audit record. FR-001…FR-011 are satisfied and tested
(FakeSpawner unit + integration; migration `0024_tool_launches.sql`).

**E5 (satisfied)**: `setProjectLifecycle('processing')` was removed — tool
launch does not mutate lifecycle state (zero references remain in
`features/projects/`); the `ready → processing` transition stays a separate
explicit user action governed by spec 009.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Launch The Configured Tool With Project Context (Priority: P1)

As a user, I want one click on `Open in {tool}` to actually launch the
configured tool (PixInsight, Siril, or Planetary Suite) with my project's
working folder set so I can resume processing where I left off instead of
re-navigating from the tool's file dialog.

**Why this priority**: Without an actual launch, the CTA is decorative; this
is the core promise of the feature and the unblock for spec 012 (Processing
Artifact Observation), which only fires when a launch happened.

**Independent Test**: With PixInsight path configured in Settings and a
PixInsight-bound project selected, click `Open in PixInsight`. PixInsight
opens and a `tool_launch` audit record appears. On Linux and Windows the
process starts in the project's generated source-view folder and the record
carries a non-null PID. On macOS PixInsight launches through its bundle id,
which carries neither a working directory nor a PID, so the record's PID is
null and no working-directory observable applies.

**Acceptance Scenarios**:

1. **Given** PixInsight is configured and the project has a generated source
   view folder, **When** the user clicks `Open in PixInsight`, **Then** the
   tool starts as a detached child process, the source-view folder is passed
   per the tool's launch profile, and the project shows a transient
   "Launched PixInsight" toast.
2. **Given** Siril is configured but the project has no generated source view
   yet, **When** the user clicks `Open in Siril`, **Then** the tool launches
   with the project root as working directory (no file argument), and the
   audit record notes `source_view: absent`.
3. **Given** a tool launch succeeded once, **When** the user clicks the CTA
   again while the prior process is still alive, **Then** the app presents a
   modal warning with two buttons: **"Open another instance"** (sends
   `force=true` to the contract) and **"Cancel"** (aborts the launch).

---

### User Story 2 - Configure Tool Paths Without Editing Files (Priority: P2)

As a user, I want to configure executable paths for supported processing
tools in Settings (and accept sensible auto-discovered defaults) so the
launch CTA is enabled exactly when it can actually do something.

**Why this priority**: The P1 launch path is gated on having a valid
executable; without configuration the CTA must degrade to a disabled state
with a clear "Configure path" affordance.

**Independent Test**: Open Settings → Tool Workflows. Confirm each supported
tool shows either an auto-discovered path or an empty field. Set a path, save,
return to a project — the corresponding launch action becomes enabled.

**Acceptance Scenarios**:

1. **Given** PixInsight is installed in a standard location, **When** the
   user opens Settings → Tool Workflows for the first time, **Then** the
   PixInsight path field is pre-filled with the discovered executable and
   marked "auto-detected".
2. **Given** the user pastes a path that does not exist, **When** they save,
   **Then** the field shows an inline error and the corresponding launch CTA
   stays disabled with tooltip "Tool path missing".
3. **Given** a configured path used to be valid but the file was deleted,
   **When** the user clicks `Open in {tool}`, **Then** launch fails with
   `tool.executable.not_found`, the audit log records the attempt, and a
   notification offers a "Re-configure path" action.

---

### User Story 3 - Pass Project Context On Launch (Priority: P3)

As a user, I want the launched tool to start positioned at my project's
source-view folder (or root folder when no source view exists yet) so I do
not have to re-navigate or remember absolute paths across drives.

**Why this priority**: This is where the product earns its keep relative to
"the user could double-click the .exe themselves". Per-tool launch profiles
own this mapping.

**Independent Test**: Inspect the spawned process's argument vector and
working directory after each tool launch. The values match the per-tool
launch profile defined in `crates/workflow/profiles/`.

**Acceptance Scenarios**:

1. **Given** a project has a generated source-view folder, **When** the user
   launches PixInsight, **Then** PixInsight is spawned with the project
   source-view folder as `cwd` and (per the launch profile) any tool-specific
   argument that opens that folder.
2. **Given** a project has no generated source-view folder, **When** the
   user launches Siril, **Then** Siril is spawned with the project root
   folder as `cwd` and no file argument.
3. **Given** the launch profile declares `supports_open_folder: false`,
   **When** the user launches that tool, **Then** the app spawns the tool
   without a folder argument and shows a one-time note that "this tool does
   not accept a folder; the working directory is set instead".

### Edge Cases

- Executable path missing, inaccessible, points to the wrong app, or is on a
  drive that is currently unmounted.
- Tool supports project folder but not direct project-file opening; vice
  versa for Siril `.ssf` scripts.
- Multiple installed versions of the same tool (e.g. PixInsight 1.8 / 1.9).
- Launch command contains spaces, non-ASCII characters, or platform-specific
  path conventions (UNC, `\\?\`, `~`, etc.).
- User disables direct launch globally.
- A previous launch process for the same project is still alive.
- The project's source-view folder was promised but never generated (spec 026
  removed it).

### Domain Questions Resolved

- **Q1 (Tool list)**: First-class support for PixInsight, Siril, Planetary
  Suite (Firecapture/AutoStakkert as a paired profile). New tools added
  via launch profiles, not core changes. → `research.md` R1.
- **Q2 (Argument shape)**: Per-tool `args_template` with a small substitution
  vocabulary (`{folder}`, `{file}`). The CTA never invents arguments outside
  the template. → `research.md` R3.
- **Q3 (Path discovery)**: Settings is authoritative; auto-discovery is a
  pre-fill convenience that the user can override or clear. → `research.md`
  R2.

### Domain Questions Open

- **O2 (Tool-emitted "back-to-app" signal)**: Whether to ship a small drop
  folder convention (or a process-watch heuristic) so spec 012 can detect
  artifacts without polling. Deferred from this spec but a launch-time
  decision (we record `launched_at` and project id whether or not the tool
  cooperates).

### Domain Questions Deferred to v2

- **O1 (Multi-version handling)**: Should `ToolProfile` allow a list of
  candidate executables (PixInsight 1.8 vs 1.9), and if so should we expose
  a per-project "preferred version" override? Deferred: v1 has one executable
  per tool profile.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Settings MUST allow configuring an executable path per supported
  processing tool, with inline validation against existence and execute
  permission.
- **FR-002**: Project actions (row overflow, footer, detail header) MUST
  include `Open in {tool}` whenever the project's workflow profile resolves
  to a configured tool, and MUST be disabled with a clear reason otherwise.
  *(Reconciliation note, 2026-07-19, issue #764: only the detail-pane action
  bar carries this CTA — `ProjectsPage.tsx`'s own doc comment confirms
  per-project actions live solely in the detail pane, which mounts on
  selection; no row-overflow or footer surface exists.)*
- **FR-003**: Tool launch MUST be unavailable or clearly blocked when no
  valid executable path is configured for the project's workflow.
- **FR-004**: Every launch attempt (success or failure) MUST be recorded via
  the audit envelope with project id, tool id, executable path hash, and
  resolved argument vector hash. *(Reconciliation note, 2026-07-19, issue
  #764: `tool_launch.rs::compute_args_hash` produces a single SHA-256 hash
  over the executable path + argv together (`args_hash`), not two separate
  hashes.)*
- **FR-005**: Tool launch MUST NOT mutate project lifecycle state. The
  `ready → processing` transition remains a separate, explicit user action.
- **FR-006**: Workflow profiles MUST remain tool-agnostic and extensible
  beyond PixInsight; adding a tool means adding a `ToolProfile` row, not
  changing core code.
- **FR-007**: The app MUST use native desktop process APIs (Tauri shell /
  Rust `std::process::Command` behind the use case) for launch, with the
  child detached from the app process so closing the app does not kill the
  tool.
- **FR-008**: A successful launch MUST return a `launch_id` that spec 012 can
  observe; the launch use case MUST persist `ToolLaunch` rows.
- **FR-009**: When the launch profile declares `supports_open_folder: true`
  AND the project has a generated source-view folder, the app MUST pass that
  folder per the profile's argument template; otherwise it MUST fall back to
  the project root as working directory only.
- **FR-010**: Tool launch MUST canonicalize the working directory and verify
  it resolves inside a registered library root. If it does not, the launch
  MUST be refused with error code `cwd.outside_library_root`.
- **FR-011**: Executable existence is NOT validated before launch. If the
  executable path is absent or inaccessible, the OS spawn error is propagated
  as `launch.failed` with a descriptive `os_error_kind`; the use case does
  not perform a pre-spawn file-existence check.

### Key Entities

- **ToolProfile**: Definition of a supported tool — id, display name,
  executable path, args template, and a `supports_open_folder` capability
  flag. Persisted in settings, not hardcoded.
- **ToolLaunch**: Auditable launch record (project, tool, timestamps, PID).
  Observed by spec 012.
- **WorkflowBinding**: A project's mapping from project type / channel set
  to the `ToolProfile` whose CTA is offered.
- **Launch Attempt Outcome**: Audit-visible outcome with success or one of
  the typed error codes (`tool.not_configured`, `tool.executable.not_found`,
  `project.not_found`, `launch.failed`).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can configure PixInsight, Siril, and Planetary Suite
  paths without editing files or restarting the app.
- **SC-002**: For every supported tool, clicking the CTA results in the
  tool process starting within 2s on a developer-class machine, with the
  working directory pointed at the project's source view (or root).
- **SC-003**: Failed launches surface a notification within 1s, are logged
  with a stable error code, and never silently no-op the CTA.
- **SC-004**: Spec 012 receives a `launch_id` for every successful launch
  and can correlate emitted artifacts back to a project without polling
  arbitrary filesystem locations.

## Assumptions

- Tauri desktop is the first launch adapter.
- Processing remains outside the app boundary (constitution III).
- Generated source-view folders are produced by spec 017 / 026 plans; this
  spec only reads their resolved path.

## Out of Scope

- Automating image processing or scripting PixInsight / Siril.
- Installing or updating external tools.
- Building tool-specific script editors or process-state inspectors beyond
  "is the PID still alive".
- Detecting artifacts emitted by the tool — that is spec 012.
