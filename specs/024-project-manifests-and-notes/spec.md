# Feature Specification: Project Manifests And Notes

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `024-project-manifests-and-notes`  
**Created**: 2026-05-09  
**Updated**: 2026-06-23  
**Status**: Completed (verified 2026-06-23) — backend + UI shipped; gates green. The 5 open tasks are explicitly DEFERRED (FR-006 onboarding [spec 010], export-copy [017/025], JSON-schema contract tests). Notes display-on-load fixed during close-out (SC-002).  
**Input**: User description: "Specify project manifests, manifest checkpoints, and notes as app-owned documentation for projects and generated source views."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Generate Project Manifest (Priority: P1)

As a user, I want a project manifest that documents the sources, calibration choices, workflow resources, generated views, and project state so that project setup is reproducible.

**Why this priority**: Project creation/onboarding needs a durable documentation artifact without turning generated files into source truth.

**Independent Test**: Create a project from reviewed Inventory items and confirm a manifest record captures selected lights, flats, darks, bias, workflow, project resources, and generated source views.

**Acceptance Scenarios**:

1. **Given** a project is created, **When** resources are generated, **Then** a manifest checkpoint is recorded.
2. **Given** project sources change, **When** the project is updated, **Then** a new manifest checkpoint records the changed source map.
3. **Given** a manifest is exported, **When** the user opens it, **Then** it clearly marks itself as generated documentation.

---

### User Story 2 - Keep Project Notes (Priority: P2)

As a user, I want project notes to live with the project record so that processing context, assumptions, and cleanup decisions are preserved.

**Why this priority**: Notes were part of the story inventory and are distinct from logs and manifests.

**Independent Test**: Add notes to a project, edit them after project state changes, and confirm notes appear in project detail and manifest checkpoints where appropriate.

**Acceptance Scenarios**:

1. **Given** a project exists, **When** a note is added, **Then** it is visible in project detail.
2. **Given** a note is edited, **When** the edit saves, **Then** audit metadata records the change.
3. **Given** a manifest checkpoint is created, **When** notes are included, **Then** the checkpoint identifies which notes snapshot was used.

### Edge Cases

- Manifest export path already exists.
- Project source changes after manifest generation.
- Note contains paths or multiline processing instructions.
- User onboards a project with an existing manifest.
- Manifest export fails after project state is saved.

### Domain Questions To Resolve

- Final manifest file format and filename.
- Whether notes are included directly in exported manifests or referenced by id.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Project manifest checkpoints MUST document project source mappings, calibration choices, workflow profile, generated source views, and project lifecycle state.
- **FR-002**: Manifests MUST be generated documentation, not canonical source truth.
- **FR-003**: Project notes MUST be editable from project detail or Edit project
  on projects in any lifecycle state. Notes are read-only only when
  `lifecycle == "archived"`. (R-NotesEdit, ratified 2026-05-22.)
- **FR-004**: Note changes MUST be auditable.
- **FR-005**: Manifest exports MUST record success or failure.
- **FR-006**: Onboarding an existing project MUST detect existing manifest-like files and ask how to handle them.
- **FR-007**: Manifest checkpoints MUST reference Inventory and project records by stable ids where available.

### Key Entities

- **Project Manifest**: Generated project documentation artifact or stored checkpoint.
- **Manifest Checkpoint**: Versioned record of project source map and lifecycle state.
- **Project Note**: User-authored project documentation with audit metadata.
- **Manifest Export Event**: Log/audit event for manifest write attempt.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can inspect a project manifest checkpoint and understand which Inventory items were used.
- **SC-002**: Notes remain attached after project source mappings change.
- **SC-003**: Failed manifest exports are visible in logs and do not corrupt project state.

## Assumptions

- Project records live in the local store.
- Exported manifests may be regenerated from the database.

## Out of Scope

- Rich text note editing.
- Remote documentation publishing.

## Implementation Status

The desktop mockup (`apps/desktop/src/features/projects/ProjectsPage.tsx`) shows
the visual shape and confirms several product decisions:

- **Auto-generated manifests at lifecycle checkpoints.** Manifests are
  generated by the application (not authored by the user) at specific
  lifecycle moments: project created, sources changed/added, lifecycle
  transition, cleanup applied. The mock data records each entry with a
  `reason` ("Created", "Source added", "Lifecycle: imaging done") and a
  `timestamp`.
- **Manifests are read-only documentation.** The UI offers "Reveal in OS"
  and "Export copy" actions but no edit affordance. Manifests document
  history; they are not user-mutable.
- **Manifest files live in the project's `notes/` folder on disk.** Mock
  paths use `notes/manifest-2026-04-12.md`, `notes/m-1.md`, etc.,
  indicating manifests are real files inside the project envelope's
  `notes/` directory. A "Reveal manifest folder" action sits in the
  drawer's context menu (line 457 of `ProjectsPage.tsx`).
- **Accordion view in the project drawer.** Manifests render as an
  accordion section with each snapshot as a collapsible row showing
  timestamp + reason + path, plus an expandable body containing the
  manifest summary text and notes-of-record. Notes are a separate
  drawer section with their own free-form body.

These decisions resolve the spec's open domain questions on file format
(markdown), notes vs. manifest separation (notes are a distinct, editable
file; manifests embed a `notes` snapshot reference when relevant), and
storage location (`notes/` inside the project).
