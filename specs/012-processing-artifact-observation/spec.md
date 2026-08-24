# Feature Specification: Processing Artifact Observation

> **See Spec 030**: UI implementation of this feature must follow
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md)
> for layout, navigation, and component patterns.

**Feature Branch**: `012-processing-artifact-observation`  
**Created**: 2026-05-09  
**Last Updated**: 2026-05-22  
**Status**: Implemented (post-hoc record, verified 2026-07-28). Artifact
observation ships at HEAD via `crates/workflow/artifacts` and the app-layer
artifact use-cases. The two remaining unchecked boxes in `tasks.md` are a missing
e2e mock fixture and a missing research doc, tracked as a bead; the underlying
behaviour already has unit coverage. That file is legacy paper and task state
lives in beads.  
**Input**: User description: "Specify how the app observes outputs from PixInsight, Siril, planetary/lunar tools, and future workflow profiles without becoming the processing tool."

## Implementation Status: Substantially implemented (2026-07-03)

Superseding the original "NOT IMPLEMENTED" note: the functional core is built.
Real crate `crates/workflow/artifacts/` (watcher, classifier, reconciler,
rules), the artifact tables in
`crates/persistence/core/migrations/0001_initial_schema.sql` (originally
migration `0025_artifacts.sql`, since folded into the pre-1.0 baseline),
repository `crates/persistence/plans/src/repositories/artifacts.rs`, and app
use-cases in `crates/app/lifecycle/src/artifact/` (`detect`/`list`/`classify_override`/
`mark_resolved`/`mark_missing`/`reattribute`/`complete_run`) are in place, with
Tauri commands (`artifact_list`/`classify`/`mark_resolved`) + generated
bindings, a live notify-rs OS watcher (`crates/fs/inventory/src/artifact_watcher.rs`),
and the `ToolLaunchesAccordion` drawer component.
Detection, classification, override, missing-state, attribution, persistence,
and contracts all exist.

**Correction (2026-07-04, WP-012-A)**: a prior revision of this note claimed
"T008's notify-rs watcher, marked deferred in tasks.md, is in fact done" —
that was inaccurate. What existed was a single always-on watcher spawned once
at app startup over every registered library root (never per-project,
never attached/detached, and it stamped the *library root* id onto
`ProcessingArtifact.project_id`, not the real project). T008 (drawer
attach/detach lifecycle) and T007b (`watch_extensions` setting) were
genuinely open; both are now implemented: a per-project
`ArtifactWatcherRegistry` attached via `artifact.watcher.attach`/detached via
`artifact.watcher.detach` from `ProjectDetailContent`'s own mount lifecycle
(runs the T005 on-attach reconciliation pass first), and a
`tools.<tool_id>.watch_extensions` Settings key applied by both the live
watcher and reconciliation. Remaining open items are still peripheral:
integration/e2e/contract tests, a research doc, cross-spec sign-off
(011/017/024), and mounting `ToolLaunchesAccordion` into the drawer UI (T023
is checked in tasks.md but the component is not actually rendered anywhere —
see WP-012-A handover) — not core build.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Detect New Files In A Project's Output Folder (Priority: P1)

As a user, I want the app to notice files that appear in a project's
expected output folder so that processing results show up alongside the
project without manual import.

**Why this priority**: detection is the foundation. Until the app sees
the file, classification and surfacing are moot. P1 also satisfies the
constitution's PixInsight Boundary — the app *observes* what PixInsight
or Siril wrote, it does not write the file itself.

**Independent Test**: launch the desktop app with a project pointing at
an output folder. Drop a representative `.xisf` or `.fits` file into the
folder from outside the app. Within the configured debounce window the
file appears in the project drawer's Tool Launches accordion as an
unclassified artifact with a detection timestamp.

**Acceptance Scenarios**:

1. **Given** a project has an output folder configured, **When** a new
   file with a recognized processing extension is written to that
   folder, **Then** a `ProcessingArtifact` row is recorded with the
   project id, absolute path (resolved through library root), and a
   detection timestamp.
2. **Given** a file is renamed or replaced in the output folder,
   **When** the watcher fires, **Then** the existing artifact's
   `detected_at` is updated and an audit event records the change
   rather than creating a duplicate row.
3. **Given** a file is deleted outside the app, **When** the project is
   rescanned, **Then** the artifact transitions to `missing` rather
   than disappearing silently.

---

### User Story 2 - Classify Artifacts As Intermediate / Master / Final (Priority: P2)

As a user, I want detected files to be classified so that I can tell at
a glance which files are throwaway intermediates and which are the
final result of a processing session.

**Why this priority**: detection alone produces a noisy list. Coarse
classification (intermediate / master / final) gives the user enough
signal to prioritise review. Manual override is part of the user
story — heuristics are never silently authoritative.

**Independent Test**: drop a set of representative files into the
output folder (`integration_*.xisf`, `MasterDark_*.xisf`,
`<target>_final.tif`). Each artifact appears classified per the
workflow profile's rules with a confidence value. The user changes one
artifact's kind via the contract and the override is recorded.

**Acceptance Scenarios**:

1. **Given** a detected file matches a workflow-profile naming
   heuristic, **When** the classifier runs, **Then** the artifact's
   `kind` is set to one of `intermediate`, `master`, or `final` with a
   `classification_confidence` between 0 and 1.
2. **Given** a user manually overrides an artifact's kind, **When** the
   override is applied, **Then** the new kind, the source
   (`manual_override`), and an audit event are recorded; subsequent
   re-classifications do not silently revert the user choice.
3. **Given** a file is unknown to all loaded workflow profiles,
   **When** the classifier runs, **Then** the artifact is recorded
   with `kind = intermediate` and `classification_confidence < 0.2`,
   surfaced as "needs review" rather than dropped.

---

### User Story 3 - Surface Artifacts In The Project Drawer (Priority: P3)

As a user, I want observed artifacts to appear in the project drawer's
Tool Launches accordion so that processing results are visible next to
the launch that produced them.

**Why this priority**: surfacing depends on detection (P1) and is
clearer with classification (P2). Grouping under Tool Launches ties
each artifact to a launch when one exists (feature 011 cross-spec).

**Independent Test**: open a project drawer; the Tool Launches
accordion section lists each tool launch followed by the artifacts
that appeared in the output folder after the launch's start time,
grouped by `kind` with a count badge.

**Acceptance Scenarios**:

1. **Given** the project has artifacts and tool launches, **When** the
   drawer renders, **Then** artifacts are grouped under their nearest
   preceding launch by detection timestamp.
2. **Given** an artifact has no preceding launch (manual processing),
   **When** the drawer renders, **Then** it appears under an
   "Unattributed" group.
3. **Given** an artifact is `missing`, **When** the drawer renders,
   **Then** the row visibly distinguishes the missing state and offers
   a "Mark resolved" affordance via the classify contract.

---

### Edge Cases

- Output folder is on a removed external drive: watcher unavailable;
  detection pauses without losing existing rows.
- Same artifact appears under multiple names (symlinks, hardlinks): the
  app records the first observed canonical path and treats links as
  references (constitution forbids following symlinks unless enabled).
- A user runs PixInsight and writes the file before the watcher has
  attached: the on-attach reconciliation scan must catch the file.
- Workflow profile rules change after artifacts were observed: existing
  classifications stay; the user can request a reclassify per project.
- Very large output folders: bounded debounce window (default 2s) and
  paged listing keep UI responsive.

### Domain Questions To Resolve

- Exact extension allow-list per workflow profile (see research.md).
- Default debounce window and watch-vs-poll selection per platform.
- Whether `final` requires a manifest cross-reference (feature 024).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The app MUST observe a project's configured output folder
  and detect newly written files matching the active workflow profile.
- **FR-002**: Each observed file MUST become a `ProcessingArtifact` row
  with `project_id`, `path`, `tool`, and `detected_at` recorded.
- **FR-003**: Each artifact MUST carry a `kind` of `intermediate`,
  `master`, or `final`, plus a `classification_confidence` value.
- **FR-004**: Classification MUST be driven by workflow-profile rules,
  not hardcoded per UI surface.
- **FR-005**: Users MUST be able to manually override classification;
  overrides MUST persist across re-detection and re-classification.
- **FR-006**: Missing files MUST transition to a `missing` state rather
  than being deleted from the index silently.
- **FR-007**: The app MUST NOT write to, modify, or process the
  observed files. Observation is read-only.
- **FR-008**: Artifact detection and classification MUST emit audit
  events (`artifact.detected`, `artifact.classified`,
  `artifact.classify.override`, `artifact.missing`).
- **FR-009**: Artifacts MUST be surfaceable in the project drawer's
  Tool Launches accordion, grouped by nearest preceding launch.
- **FR-010**: Spec 012 MUST emit a `workflow.run_completed` event on the
  spec 002 event bus when a tool launch reaches a terminal state
  (specifically: when `ToolLaunch.completed_at` is set by this spec's
  attribution pass). The event payload MUST include `projectId`,
  `toolId`, `toolLaunchId`, `completedAt`, and the list of `artifactIds`
  detected under that launch. Spec 024 subscribes to this event to write
  `workflow_run` manifests (R-Event-Light).

### Key Entities

- **Processing Artifact**: Output file observed under a project,
  classified as intermediate / master / final, optionally associated
  with a tool launch.
- **Artifact Classifier Rule**: Workflow-profile-scoped naming or
  extension pattern that maps an observed file to a kind and tool.
- **Artifact State**: Present, missing, or user-resolved.
- **Workflow Profile**: Tool-specific configuration (feature 011 cross-spec).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Dropping a known PixInsight master file into a project's
  output folder results in a classified artifact in the drawer within
  the debounce window.
- **SC-002**: PixInsight and Siril rules coexist without UI hardcoding.
- **SC-003**: Missing artifacts remain auditable after rescan.
- **SC-004**: Manual classification overrides survive a re-scan.

## Cross-References

- Feature 011 (Processing Tool Launch): supplies the `tool_launch_id`
  used to attribute artifacts to a launch.
- Feature 024 (Project Manifests And Notes): manifests snapshot the
  set of `final` artifacts at lifecycle checkpoints.
- Feature 001 (Library Manager): library-root abstraction resolves
  artifact paths across removed/remapped drives.

## Assumptions

- Processing tools own actual processing.
- Project artifact observation is local-filesystem based.
- Each project has at most one active output folder per workflow.

## Out of Scope

- Running workflow scripts or invoking PixInsight/Siril.
- Editing, transcoding, or compressing artifact files.
- Uploading artifacts to remote services.
- Diffing or comparing artifact contents.
