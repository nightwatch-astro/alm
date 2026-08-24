# Feature Specification: Project Lifecycle Model

> **AMENDED (2026-07-14) by the Q27 framing-layer iteration on
> [Spec 008 — Project Create, Onboard, And Edit](../008-project-create-onboard-edit/spec.md).**
> Spec 008 introduces a **framing layer** (`project → framing → session →
> frames`). Two lifecycle interactions follow: (1) the Q27 incremental
> ingestion-attribution pass, when a new session matches a **completed**
> project, offers **add + reopen** using the existing `completed → processing`
> edge (with the Q25 raw-subs-archived reopen warning) — no new edge is added;
> (2) framing is **orthogonal to lifecycle** (a project's framings are a
> sub-structure of its sources, not a lifecycle state), and the `ready →
> prepared` PreparedSource creation is now **per-framing** (a Q20 source view per
> framing). See the Iterations log below. Spec 008 owns the framing data model.

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `009-project-lifecycle-model`  
**Created**: 2026-05-09  
**Status**: Implemented; FR-003 partial (More menu, issue #764)  
**Input**: User description: "Specify project lifecycle state, project detail structure, actions by phase, and cleanup/archive readiness without ambiguous Plan columns or unexplained top action strips."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See Project State And Next Actions (Priority: P1)

As a user, I want each project to show its lifecycle state and available actions in a consistent way so that I know whether to review, prepare, open, process, archive, or clean up.

**Why this priority**: The project list must communicate useful workflow state without unclear columns such as "Plan".

**Independent Test**: Create projects in draft, ready, prepared, active processing, completed, archived, and blocked states and confirm each shows a plain state label, source summary, and phase-appropriate actions.

**Acceptance Scenarios**:

1. **Given** a project is missing required sources, **When** it appears in the project list, **Then** its state indicates that setup is incomplete and actions focus on editing the project.
2. **Given** a project has reviewed sources, **When** it appears in the project list, **Then** its state indicates it is ready to open or prepare.
3. **Given** a project has cleanup/archive candidates, **When** it is selected, **Then** the detail pane shows lifecycle information and cleanup/archive plan links.

---

### User Story 2 - Inspect Project Detail (Priority: P2)

As a user, I want the project side panel and opened project view to show structured details, sources, channels, lifecycle events, and actions so that I can manage the project without duplicated row text.

**Why this priority**: The user called the previous side panel chaotic and overlapping.

**Independent Test**: Select a project with multiple light sessions, flats, darks, bias, and generated outputs; confirm the detail pane uses structured rows and expandable sections.

**Acceptance Scenarios**:

1. **Given** a project is selected, **When** the detail pane opens, **Then** it shows project fields in a structured table-like layout.
2. **Given** sources are listed under a project, **When** the user clicks a source, **Then** the app opens the linked Inventory item.
3. **Given** channels or generated views exist, **When** the user expands the relevant section, **Then** details are shown without layout overlap.

### Edge Cases

- Project source becomes stale or missing.
- Generated prepared source is out of date.
- Processing-tool path is not configured.
- Cleanup/archive plan exists but is not reviewed.
- Project was onboarded from an existing folder with partial markers.

### Domain Questions To Resolve

- Final project lifecycle states and transitions.
- Which actions are available in each lifecycle phase.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Project lifecycle state MUST be a domain field, not a decorative badge-only UI treatment.
- **FR-002**: Project rows MUST NOT use an ambiguous "Plan" column unless it is explicitly a cleanup/archive plan count or link.
- **FR-003**: Project actions MUST be phase-aware and use the shared primary action plus More menu pattern.
  *(Partial: `ProjectDetail.tsx` has no More-menu/overflow pattern. Issue #764.)*
- **FR-004**: The project surface MUST NOT show unexplained top action strips such as Candidate, Source mapping, Prepared, or Processing when those states are already represented by project data.
- **FR-005**: Project details MUST list sources directly.
- **FR-006**: Clicking a source in a project MUST navigate to or open the linked Inventory item.
- **FR-007**: Project detail layout MUST use structured rows and expandable sections for channels, generated views, lifecycle events, and plans.
- **FR-008**: Lifecycle transitions MUST be logged and auditable.
- **FR-009**: Blocked states MUST identify the blocking reason and the action that resolves it.

### Key Entities

- **Project Lifecycle State**: Setup incomplete, ready, prepared, processing, completed, archived, blocked, or other final planned state.
- **Project Action**: Open, edit, open in tool, prepare, archive review, cleanup review, or delete/remove where allowed.
- **Project Source**: Inventory link used by the project.
- **Generated View**: Prepared source or tool-specific projection derived from Inventory.
- **Project Lifecycle Event**: State transition, source change, processing artifact observation, plan creation, or failure.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can identify why a project is blocked from the row and detail pane.
- **SC-002**: Users can open any project source directly from project details.
- **SC-003**: The project detail pane has no duplicated row description content.
- **SC-004**: State filtering supports multiselect for lifecycle states.

## Assumptions

- The exact final lifecycle vocabulary may be refined during implementation planning.
- Project source links reference Inventory items.

## Out of Scope

- Running processing tools.
- Full cleanup/archive execution.
- Cloud synchronization.

## Implementation Status

Shipped surfaces:

- Lifecycle states and the stepper: `ProjectLifecycleStepper.tsx`,
  `lifecycle-actions.ts`, `lifecycleTransition.ts` (all under
  `apps/desktop/src/features/projects/`).
- Backend transitions: `lifecycle_transition_preview`,
  `lifecycle_transition_apply`, `lifecycle_ledger_list`, `lifecycle_manifest`
  (`apps/desktop/src-tauri/src/commands/`).
- Blocked-state surface: `BlockedBanner.tsx`.

FR-003 is partial: the shared primary-action pattern shipped, the More-menu half
did not. Tracked as issue #764.

**Transition graph (canonical edges):**

| From               | Allowed `to`                                                        |
| ------------------ | ------------------------------------------------------------------- |
| `setup_incomplete` | `ready`, `blocked`                                                  |
| `ready`            | `prepared`, `processing`, `blocked`                                 |
| `prepared`         | `ready`, `processing`, `blocked`                                    |
| `processing`       | `completed`, `blocked`                                              |
| `completed`        | `archived`, `processing` (re-open)                                  |
| `archived`         | `processing` (Unarchive — resume work), `ready` (Unarchive to ready — R-Unarchive, GRILL 2026-05-22) |
| `blocked`          | `ready`, `prepared`, `processing`, `setup_incomplete`, `archived`   |

Seventeen allowed edges total. `blocked → archived` is an escape hatch for
permanently-blocked projects; it requires explicit user confirmation and always
requires a plan (same requirements as `completed → archived`). `blocked →
completed` remains forbidden (A3, GRILL 2026-05-22).

Forbidden: `processing → ready` (would lie about progress); `archived →
completed` (no implicit re-completion); any direct skip past `setup_incomplete`
into `prepared` or later without passing `ready`.

**Unarchive edge added (R-Unarchive, GRILL 2026-05-22)**: `archived → ready`
is now a valid transition. It is the primary unarchive path for users who want
to revisit a finished project without immediately resuming processing. Conditions
and plan requirements mirror `archived → processing`: plan required when (a)
sources are mapped to different paths OR (b) any source content needs to move;
plan NOT required when only metadata transitions. Actor: user only. Audit event
`project.unarchived` is emitted. The previous `archived → processing` edge
remains as an alternate unarchive path (resume work directly).

Eighteen allowed edges total (seventeen + `archived → ready`).

**Surface behavior (FR alignment):**

- FR-001/FR-002: Lifecycle is a domain field rendered as a labeled stepper;
  rows display state via `StateLabel`/`lifecycleTone` rather than a
  decorative-only badge. No ambiguous "Plan" column appears in the project
  table; plan counts/links are scoped to detail expandable rows.
- FR-003: Footer uses the shared primary + secondary pattern. The More menu is
  not built (issue #764).
- FR-004: Phase-only top-action strips are removed; the stepper plus footer
  carries all phase context.
- FR-005/FR-006: Project detail enumerates sources with inventory deep-links.
- FR-007: Channels, plans, manifests, lifecycle events render as expandable
  sections.
- FR-008: Every transition appends an audit log entry; full auditing routes
  through spec 002's `lifecycle.transition` contract.
- FR-009: `Project.blockedReason?` and a resolve-action label identify the
  unblock path.

**Deferred to implementation (post-mockup):**

- Auto-`blocked` detection (missing inventory link, stale prepared source,
  unconfigured tool path). The mockup permits only user-triggered transitions.
- Filesystem mutation on `ready → prepared` and on cleanup generation flows
  through spec 017 (filesystem plan) / spec 025; `setProjectLifecycle` is a
  pure state write in the mockup.
- Cross-spec audit fan-out via the persistence boundary
  (`crates/persistence/db/`).

## Iterations

### Iteration 2026-07-14: Framing layer interactions (Q27, cross-spec delta)

**Change**: Spec 008's Q27 framing layer adds two lifecycle-adjacent
interactions recorded here (no lifecycle vocabulary or edge changes):
- The Q27 Inbox-confirm attribution pass, on matching a **completed** project,
  suggests **add + reopen** via the existing `completed → processing` edge
  (transition table above) with the Q25 raw-subs-archived reopen warning. This
  is a new **consumer** of that edge, not a new edge.
- Framing is **orthogonal to lifecycle** — a project's framings are a
  sub-structure of its sources, unaffected by lifecycle state. The `ready →
  prepared` PreparedSource creation becomes **per-framing** (one Q20 source view
  per framing); the plan-required semantics of that edge are unchanged.
**Scope**: Cross-spec delta (no FR/edge added; framing data model owned by spec 008).
**Artifacts updated**: spec.md (this amendment note + log). No transition-table
or data-model edges added.

