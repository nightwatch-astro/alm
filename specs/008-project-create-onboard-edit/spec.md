# Feature Specification: Project Create, Onboard, And Edit

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `008-project-create-onboard-edit`  
**Created**: 2026-05-09  
**Status**: Implemented  
**Input**: User description: "Specify project creation and onboarding as a single project setup/edit flow that creates required resources, sources, folder structure, and project markers without separate envelope/source-generation actions."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Create A Project (Priority: P1)

As a user, I want to create a project from a single dialog that collects the required fields — name, tool, optional initial sources, and optional notes — so that I do not need separate technical actions like creating an envelope or retrying marker writes.

**Why this priority**: Project setup is a core workflow and must use simple functional naming.

**Independent Test**: Open "New project" dialog, provide name, select tool (required), optionally pick initial sources from Inventory, optionally add notes, then confirm the app creates the project record, folder structure, project marker, and source mappings in one operation.

**Acceptance Scenarios**:

1. **Given** name and tool are supplied (tool is mandatory), **When** the user confirms the dialog, **Then** folder structure, source mappings, workflow resources, and project marker are created as one operation.
2. **Given** any creation step fails, **When** the operation stops, **Then** the app rolls back created resources where possible, logs an error, and notifies the user.
3. **Given** project creation succeeds, **When** the project opens, **Then** sources are listed directly and can be opened or inspected.
4. **Given** no initial sources are supplied, **When** the project is created, **Then** the project lands in `setup_incomplete`. The `setup_incomplete` state is ONLY for missing/unconfirmed sources, never for missing tool (tool is required at create). The system auto-transitions to `ready` once the first confirmed source is added and mapped.

**Note on `project.duplicate`**: The recovery path for tool-locked projects (lifecycle in `{prepared, processing, completed, blocked}`) is manual re-creation via `project.create`. There is no `project.duplicate` contract in v1. See plan.md for the deferred follow-up note.

**Note on source removal**: `project.source.remove` is available in v1. Removal from lifecycle states `{prepared, processing, completed, archived}` is refused with `lifecycle.read_only`.

---

### User Story 2 - Onboard An Existing Project (Priority: P2)

As a user, I want to onboard an existing project folder by identifying its project information and source locations so that existing work can be tracked without recreating it.

**Why this priority**: Users already have PixInsight/Siril project structures and local folders.

**Independent Test**: Select an existing folder, provide required metadata and source mappings, and confirm the app links existing resources without duplicating them.

**Acceptance Scenarios**:

1. **Given** an existing project folder, **When** the user onboards it, **Then** the app detects or asks for source locations and creates missing app-owned tracking records.
2. **Given** a project marker already exists, **When** onboarding runs, **Then** the app reuses it or asks for confirmation if it conflicts.
3. **Given** existing source paths are missing, **When** onboarding is reviewed, **Then** the app blocks completion until required mappings are resolved or skipped intentionally.

---

### User Story 3 - Edit Project Settings (Priority: P3)

As a user, I want all project setup fields to be editable from one project settings pane so that I do not hunt for separate actions.

**Why this priority**: The user explicitly rejected separate project envelope, prepared source, marker retry, and source mapping actions.

**Independent Test**: Open Edit project and update name, path, workflow, source mappings, light sessions, flats, darks, bias, and tool settings from one pane.

**Acceptance Scenarios**:

1. **Given** a project exists, **When** the user opens Edit project, **Then** all setup fields are visible in one structured pane.
2. **Given** a user changes source mapping, **When** the edit is saved automatically or confirmed, **Then** dependent generated resources update through a single operation.

---

> **US4 is reserved.** There is no "User Story 4" section in this spec; US4
> (channel inference) exists as a task-level story in `tasks.md` (§US 4). The
> number is reserved to keep task↔story traceability stable — do not reuse it.

### User Story 5 - Group A Project Into Framings (Priority: P2)

As a user, I want my project's light sessions grouped into **framings** — each framing being the sessions that share target, optic-train, pointing, and rotation within a tolerance (all filters and nights of one co-registerable integration unit) — so that a normal project's multi-night, multi-filter data reads as one integration unit and a mosaic's panels read as separate ones.

**Why this priority**: The framing is the unit a per-framing source view (Q20) and per-framing manifest (Q10) are built against; without it, multi-night data and mosaic panels cannot be distinguished for processing prep.

**Independent Test**: Create a project with L/R/G/B light sessions captured across two nights on one target and optic-train with the same pointing; confirm they collapse into a single framing. Then merge, split, or reassign a framing and confirm the change persists and is marked user-adjusted.

**Acceptance Scenarios**:

1. **Given** a project's light sessions share target + optic-train + pointing + rotation within tolerance, **When** the framings are derived, **Then** they collapse into a single framing spanning all their filters and nights.
2. **Given** the app has suggested a framing clustering, **When** the user merges, splits, or reassigns sessions between framings, **Then** the adjustment persists and the framing is recorded as user-adjusted, not authoritative clustering.
3. **Given** two light sessions differ in pointing beyond tolerance, **When** framings are derived, **Then** they land in distinct framings.

---

### User Story 6 - Mosaic Mode (Priority: P3)

As a user, I want to mark a project as a **mosaic** so that its multiple framings (panels) are all understood to belong to my one declared target and the app does not try to resolve a separate target per frame.

**Why this priority**: Mosaic panels point away from target center, so per-frame target resolution would mis-resolve; the flag is the minimal mechanism that keeps a multi-panel project coherent.

**Independent Test**: Mark a project mosaic, ingest subs for two panels with different pointing/rotation, and confirm the project shows two framings that both inherit the declared target with no per-frame OBJECT/coordinate resolution and no panel entity created.

**Acceptance Scenarios**:

1. **Given** a project is marked mosaic, **When** its framings are derived, **Then** each framing inherits the project's declared target and per-frame OBJECT/coordinate resolution is suppressed.
2. **Given** a mosaic project, **When** a second panel's subs are ingested, **Then** they form (or match) a distinct framing by pointing+rotation clustering, without any OBJECT/panel-name string parsing.
3. **Given** a mosaic project with existing framings, **When** subs for a **first new panel** are ingested (pointing matches no existing framing, per-frame target resolution suppressed), **Then** attribution still suggests the mosaic project with **add-as-new-framing**, via the FR-019 mosaic relaxation (optic-train match + pointing within the envelope of the project's existing framings).

---

### User Story 7 - Incremental Ingestion Attribution (Priority: P2)

As a user, when I confirm new light sessions at the Inbox gate, I want the app to **suggest** where they belong — add to an existing framing, add as a new framing, add to a project but flag an optic-train difference, or start a new project — ranked by framing match, so multi-night and multi-panel data flows into the right integration unit without me hunting for it.

**Why this priority**: Incremental attribution is the payoff of the framing model — it routes tonight's subs into last month's project/panel automatically-suggested, while keeping every merge user-approved.

**Independent Test**: With an existing project holding a framing, ingest a new session matching that framing's target + optic-train + pointing + rotation; confirm the existing framing is the top-ranked suggestion and nothing is merged until the user picks it. Repeat against a completed project and confirm the suggestion offers add + reopen.

**Acceptance Scenarios**:

1. **Given** an existing framing matches a new session's target + optic-train + pointing + rotation within tolerance, **When** the session is confirmed at the Inbox gate, **Then** that framing is surfaced as the top-ranked attribution suggestion and is applied only on the user's pick.
2. **Given** a new session matches a project's target but a different optic-train, **When** attribution runs, **Then** the project is suggested with an optic-difference flag.
3. **Given** a new session matches a **completed** project, **When** attribution runs, **Then** the suggestion offers add + reopen (with the raw-subs-archived reopen warning) and never auto-merges.
4. **Given** no framing/project matches, **When** attribution runs, **Then** the suggestion is new-project / unassigned.

### Edge Cases

- Project path already contains files.
- Project marker write fails.
- Folder structure creation partially succeeds.
- Source mapping points to a missing Inventory item.
- User adds multiple light sessions with different optional flats.
- Two light sessions share a target but differ in optic-train (framing must not merge them; attribution flags the difference).
- Rotation drifts a few degrees between nights (must stay within tolerance and remain one framing).
- A mosaic project's panels point away from the declared target center (per-frame resolution must stay suppressed).
- A new session matches a completed project (attribution must offer add + reopen, never auto-merge).

### Domain Questions To Resolve

- Which project types/workflows are available at first release?
- Which generated resources are required for PixInsight versus Siril?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Project creation MUST use functional labels such as Add project, Edit project, Open, and Open in PixInsight/Siril.
- **FR-002**: Project creation MUST use a single-form dialog collecting: name (required), tool (required), optional initial sources, optional notes. There is no multi-step wizard for create (GRILL A1).
- **FR-003**: Tool MUST be selected at project creation time; it is a required field. `setup_incomplete` state is ONLY for projects missing confirmed sources, never for missing tool (R-Tool-Req).
- **FR-004**: Initial sources are optional at create; omitting them is valid and results in `setup_incomplete`. The system auto-transitions to `ready` once the tool is set (always true post-create) and at least one confirmed source is mapped.
- **FR-005**: Project creation MUST create required folder structure, project marker, and workflow resources as part of the operation.
- **FR-006**: Project creation MUST roll back, log, and notify on failure.
  *(Reconciliation note, 2026-07-19, issue #764: "roll back" describes the
  original all-or-nothing design; per the 2026-07-04 user decision
  (`crates/app/core/src/project_create.rs` module doc), a failed or
  non-starting scaffolding apply never fails project creation or unwinds
  partial writes — the plan stays reviewable and retryable through the
  normal plan surfaces, exactly like a failed manual apply. Treat "roll
  back" as historical intent, not the shipped failure-recovery model.)*
- **FR-007**: Onboarding MUST support existing project folders.
- **FR-008**: Project edit MUST be a single pane for project fields and source mappings.
- **FR-009**: Technical actions named Create project envelope, Generate/update prepared sources, Project label, or Retry marker write MUST NOT appear as normal user actions.
- **FR-010**: After source additions, projects with manually-overridden channels MUST surface `channelDrift.hasNewSources = true` on `project.get` until the user re-infers (calls `project.channels.reinfer`) or dismisses (calls `project.channels.dismiss_drift`).
- **FR-011**: `project.source.remove` MUST be permitted when `lifecycle in {setup_incomplete, ready, blocked}` and refused with `lifecycle.read_only` when `lifecycle in {prepared, processing, completed, archived}`.
- **FR-012**: `project.source.add` use case MUST verify the referenced Inventory session has `state == "confirmed"`. Unconfirmed sessions are rejected with `source.not_confirmed`.
  *(Reconciliation note, 2026-07-19, issue #764: this gate was never
  implemented and was formally descoped by decision D9, 2026-07-03 —
  `docs/development/orchestrator-handover-2026-07-03.md` — superseded by
  spec 041's universal Inbox confirm gate, which makes every session
  reaching this code path already-confirmed by construction; D9's verdict
  found all production session-creation paths safe without an explicit
  runtime check. `contracts/project.source.add.json` still declares
  `source.not_confirmed` + `actualState` for this dead gate — kept in the
  schema for now but not enforced.)*
- **FR-013**: A project's light sessions MUST be groupable into **framings**, where a framing is the set of light sessions sharing target + optic-train + pointing + rotation within a configured **tolerance** (never an exact key). A framing is the co-registerable integration unit spanning all filters and nights of one pointing.
- **FR-014**: Framing tolerance (FOV-relative pointing offset; rotation drift in degrees) MUST be a tunable parameter with a sensible default; it MUST NOT be an exact-match key.
- **FR-015**: Framing clustering MUST be presented as a **suggestion** the user can adjust — **merge**, **split**, and **reassign** sessions between framings — and MUST NOT be treated as authoritative.
- **FR-016**: The one-target-per-project rule MUST hold: every framing of a non-mosaic project MUST share the single project target. A non-mosaic project has **one active framing** (one framing, many filters/nights); when clustering yields additional framings (pointing/rotation beyond tolerance), they MUST be surfaced for user reconciliation (reassign, retune tolerance, or enable mosaic mode), never silently accepted as parallel integration units.
- **FR-017**: A project MUST carry a minimal **mosaic-mode flag**. A mosaic project MAY hold multiple framings that all **inherit the project's declared target**, and per-frame OBJECT/coordinate resolution MUST be **suppressed** for mosaic projects.
- **FR-018**: The system MUST NOT parse `OBJECT` values or panel-name strings for attribution anywhere; panel identity is derived only from the physical pointing+rotation clustering. There is **no panel entity** — panels are simply the framings of a mosaic project.
- **FR-019**: At the Inbox confirm gate, the system MUST run an **attribution pass** that matches each new light session against existing framings/projects by target + optic-train + pointing+rotation (tolerance) and **suggests** one of: add-to-existing-framing, add-as-new-framing, add-to-project-but-flag-optic-difference, or new-project/unassigned. Multiple candidates MUST be **ranked by framing match** and the user picks (recommend-then-override). Attribution is the **first** pre-ingest pass at the confirm gate; the Q22 duplicate-detection sweep joins the **same** pass when its iterate lands (composition point, not a prerequisite). **Mosaic relaxation**: for `isMosaic` candidate projects, target equality (unresolvable for panels — per-frame resolution is suppressed and panels point away from the declared center) is replaced by **optic-train match + pointing within an envelope of the project's existing framings** (FOV-relative; default pinned in research R11a, tunable) — this is what lets a first NEW panel suggest add-as-new-framing.
- **FR-020**: Attribution suggestions MUST NOT auto-merge (reviewable-mutation principle). A suggestion that matches a **completed** project MUST offer add + reopen and honor the reopen revoke/warn (raw-subs-archived warning).
- **FR-021**: The app MUST support a **per-framing source view** (Q20 — one processing-tool-ready folder per framing) and a **per-framing manifest** (Q10). It MUST NOT stitch, register, or integrate framings (PixInsight-boundary principle). These projections are **consumers** of the framing model, delivered when the Q20 (spec-026/049) and Q10 (spec-024) iterations land — not prerequisites for it.
- **FR-022**: The user's attribution pick MUST be **persisted at confirm time as part of the confirm request** (an additive extension of the existing confirm contract). Framing membership is database metadata, not a filesystem mutation, so no reviewable filesystem plan is required for the write; it applies identically on catalogue-in-place and queued-move confirms. Later membership changes go through the merge/split/reassign adjustment surface (FR-015).

**Note — `{target}` path token on mosaic panels**: path generation currently fills the `{target}` token from the OBJECT header, so a mosaic panel confirmed as a move lands under an `…Panel N/` folder while inheriting the declared target. This is legal — FR-018 is **attribution-scoped**, not naming-scoped — and is the documented current behavior; `{target}` token semantics for `isMosaic` projects are deferred to the Q23 naming iterate (spec-015/025, Wave 1).

### Key Entities

- **Project**: App-owned work unit with name, path, workflow, lifecycle state, sources, and a mosaic-mode flag.
- **Project Source Mapping**: Link from project role to one or more Inventory items or source folders.
- **Framing**: The co-registerable integration unit within a project — the light sessions sharing target + optic-train + pointing + rotation within a tunable tolerance, across all filters and nights of one pointing. Carries clustering provenance (suggested vs user-adjusted); the app never treats its clustering as authoritative. Owns a per-framing source view (Q20) and manifest (Q10).
- **Mosaic-mode flag**: A project-level boolean. When set, the project may hold multiple framings (panels) that all inherit the project's declared target, and per-frame OBJECT/coordinate resolution is suppressed. There is no panel entity.
- **Light Session**: Light frames plus optional flats for that session; a member of at most one framing.
- **Project Marker**: App-owned file/record identifying the project folder.
- **Project Setup Operation**: Atomic create or onboard operation with rollback metadata.
- **Ingestion Attribution Suggestion**: A ranked, user-selectable suggestion (add-to-framing / new-framing / flag-optic-difference / new-project) produced at the Inbox confirm gate; never an auto-merge.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A first project can be created from confirmed sample Inventory items without invoking any separate technical actions.
- **SC-002**: Project creation failures produce a visible notification and log entry.
- **SC-003**: Editing a project never requires leaving the project settings pane to update source mappings.
- **SC-004**: Users can add at least two light sessions with different optional flats.
- **SC-005**: A project's light sessions across at least two nights and two filters collapse into one framing when target + optic-train + pointing + rotation match within tolerance.
- **SC-006**: A user can merge, split, or reassign a framing and the change persists as user-adjusted.
- **SC-007**: A mosaic project surfaces at least two framings all inheriting the declared target, with no OBJECT/panel-name parsing and no panel entity.
- **SC-008**: An Inbox confirm of a new session matching an existing framing surfaces that framing as the top-ranked suggestion and applies only on the user's pick.
- **SC-009**: A new session matching a completed project surfaces an add + reopen suggestion (never an auto-merge).

## Iterations

### Iteration 2026-07-14: Framing layer (Q27)

**Change**: Introduce a framing layer (`project → framing → session → frames`) — a physically-clustered co-registerable integration unit (target + optic-train + pointing + rotation within a tunable tolerance), suggested-and-adjustable, with a minimal mosaic-mode project flag (no panel entity, no OBJECT-string parsing) and an Inbox-confirm incremental attribution pass.
**Scope**: Feature-wide (new Framing entity + hierarchy layer; additive).
**Artifacts updated**: spec.md, data-model.md, plan.md, tasks.md, quickstart.md, research.md.
**Tasks added**: F-Framing-1..F-Framing-9 (Phase F).
**Tasks removed**: none.
**Tasks marked complete**: none.
**Cross-spec deltas**: spec-009 (reopen-on-attribution-match; framing orthogonal to lifecycle; per-framing prepared source) and spec-006 (session→framing membership; clustering suggestion surfaced; attribution shares the Q22 pre-ingest sweep) recorded in those specs' iteration logs.

### Iteration 2026-07-14 (b): Framing-layer gate fixes (critique 2026-07-14-q27-gate)

**Change**: Pre-implementation gate fixes — no model rethink. Durable session-level geometry persistence + NULL-legacy exclusion (F-Framing-1); attribution apply-path persisted in the confirm request (FR-022, F-Framing-10); mosaic first-new-panel relaxation (FR-019, US6 AS3); blocked-by/composition-fallback annotations for Q22/Q20/Q10/Q12 (attribution is the first pass); `framing.list` folded into F-Framing-3; clustering trigger + `user_adjusted` protection invariant; clustering semantics pinned in research R11a + tunables settings task (F-Framing-11); migration claim-next-free rule; FR-016 one-active-framing normative; US4 reservation note; `{target}` mosaic token behavior documented (deferred to Q23 iterate); attribution prefilter note.
**Scope**: Fix round (spec/tasks granularity).
**Artifacts updated**: spec.md, data-model.md, plan.md, tasks.md, research.md; cross-spec amendment note added to spec-041.
**Tasks added**: F-Framing-10, F-Framing-11. **Tasks modified**: F-Framing-1/2/3/5/7/8.

## Assumptions

- Initial project creation happens in the guided first-project flow after first-run source setup.
- Workflow-specific generated files are app-owned projections.
- Framing membership is derived from physical acquisition attributes (target + optic-train + pointing + rotation), never from OBJECT/panel-name strings.
- Pointing/rotation/optic-train are persisted at session level at confirm time (F-Framing-1). Once the Q12 strict-gate iterate is applied (spec-006/033/041 — **not yet applied**), those attributes are guaranteed present on new ingests; until then, and for legacy rows, geometry is nullable — NULL-geometry sessions are excluded from clustering until backfilled via rescan (Q28 path).

## Out of Scope

- Actual image processing.
- Remote project sync.
- Full processing-tool automation.

## Implementation Status

| Capability | Shipped surface |
| --- | --- |
| Create project | `projects_create` (`apps/desktop/src-tauri/src/commands/projects.rs`), contract `ProjectCreateRequest` (`crates/contracts/core/src/projects_v2.rs:232`) |
| Add / remove source | `projects_source_add`, `projects_source_remove` |
| Edit metadata | `projects_update` |
| Channel inference | `projects_channels_reinfer`, `projects_channels_dismiss_drift` |
| Marker write | `project_marker_to_disk_with_audit` (`crates/project/structure/src/lib.rs`) |
| Folder structure write | plan-backed via `projects_create_plan` (spec 025) |
| Framing layer (Q27) | `framing` / `framing_session` tables (`crates/persistence/core/migrations/0001_initial_schema.sql:914`), `crates/app/core/src/framing.rs`, `projects_framing_list` / `_merge` / `_split` / `_reassign` |

Task state lives in beads; this spec's `tasks.md` was retired (2026-08-17).
