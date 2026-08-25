# Feature Specification: Guided First Project Flow

> **UI Revised**: The UI design in this spec was revised by
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md), which is
> superseded as of 2026-06-11. UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns.

**Feature Branch**: `010-guided-first-project-flow`
**Created**: 2026-05-09
**Last Updated**: 2026-07-18
**Status**: Superseded — replaced by
[Spec 056 — Onboarding Redesign](../056-onboarding-redesign/spec.md). The
sequential coach specified here is deleted, not iterated; only FR-009 (no demo
data) carries forward.
**Input**: User description: "Specify the guided in-app coach that activates after first-run setup and walks a new user through Inbox confirmation, first-project creation, and first tool open using contextual overlay hints anchored to real UI elements."

## Implementation Status: NOT IMPLEMENTED

No code in `apps/desktop`, `crates/`, or `packages/contracts` implements the
guided flow described here. This spec is paired with spec 003 (first-run setup
wizard) and spec 008 (project create) and must not begin implementation before
both upstream features land and the plan, research, data model, contracts, and
tasks artifacts in this directory pass review.

## Product Intent

The guided first project flow is an in-app coach, not a tutorial mode. It
observes real user actions inside the production UI and surfaces overlay hints
that point at the next real control. Hints never replace the real workflow with
a stand-in: every step is completed by performing the action in the actual
Inbox, Inventory, Projects, and tool-open surfaces. The coach exists to remove
"where do I click" friction for a user who finished setup but has no muscle
memory for the four-stage Inbox → Inventory → Project → Tool lifecycle.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Confirm First Inventory Items From Inbox (Priority: P1)

As a new user who just finished setup, I want a contextual overlay hint to tell
me which Inbox row to confirm into Inventory first, so I learn the promotion
gesture by performing it on my own files.

**Why this priority**: Inbox-to-Inventory confirmation is the entry point of the
lifecycle and the first place a new user gets lost. Until the user has at least
one confirmed Inventory item they cannot reach any later coach step.

**Independent Test**: Launch the app immediately after first-run setup
completes, observe an overlay hint anchored to the Inbox list, perform a real
confirm action on a real Inbox candidate, and verify the coach advances on the
inventory confirmation event without any modal interrupting the action.

**Acceptance Scenarios**:

1. **Given** first-run setup has completed and the coach is active, **When**
   the user opens Inbox, **Then** an overlay hint is anchored to the first
   confirmable row and the rest of the UI remains fully interactive.
2. **Given** an Inbox candidate is highlighted by the coach, **When** the user
   confirms it into Inventory through the normal confirmation control, **Then**
   the coach records step P1 complete and advances to the next step without
   blocking the user's flow.
3. **Given** the user navigates away from Inbox mid-step, **When** the user
   returns to Inbox, **Then** the same overlay hint is re-anchored to a still
   valid candidate or to the empty-state guidance.

---

### User Story 2 - Create First Project From Confirmed Inventory (Priority: P2)

As a new user with at least one confirmed Inventory item, I want a coach hint
to point me at "Create project" and walk me through the real project create
form so I produce a real first project, not a sample.

**Why this priority**: The first project proves that Inventory confirmation
enables downstream work. Without it the user has no anchor for tool prep.

**Independent Test**: With at least one confirmed Inventory item present, the
coach surfaces an overlay hint over the Create project entry point, the user
fills the real form and submits, and the coach advances on the project created
event.

**Acceptance Scenarios**:

1. **Given** P1 is complete, **When** the user is on a route that exposes
   project creation, **Then** an overlay hint is anchored to the Create project
   control.
2. **Given** the project create form is open, **When** the user submits a
   valid project, **Then** the coach advances to P3 without showing any modal
   confirmation of its own.

---

### User Story 3 - Open First Project In A Tool (Priority: P3)

As a new user who has created a first project, I want the coach to point me at
the "Open in tool" action so I see how the app prepares inputs for an external
processing tool.

**Why this priority**: This step closes the loop between Inventory
confirmation, project assembly, and external processing without crossing the
PixInsight boundary.

**Independent Test**: With a first project present, the coach surfaces a hint
on the open-in-tool affordance; performing the action records step P3 complete.

**Acceptance Scenarios**:

1. **Given** P2 is complete, **When** the user opens the new project, **Then**
   an overlay hint is anchored to the open-in-tool control.
2. **Given** the user invokes open-in-tool, **Then** the coach records step P3
   complete and shows a non-blocking completion hint.

---

### User Story 4 - Dismiss And Restart Coach (Priority: P4)

As a user who prefers to learn on my own, I want to dismiss the coach from any
hint and restart it later from Settings without losing prior progress.

**Why this priority**: The coach is optional and must never trap the user.

**Independent Test**: Dismiss the coach from any hint, verify all overlay hints
are removed, restart it from Settings, and verify it resumes at the lowest
uncompleted step.

**Acceptance Scenarios**:

1. **Given** any overlay hint is shown, **When** the user dismisses the coach,
   **Then** no overlay hints are shown on any route and a dismissed timestamp
   is recorded.
2. **Given** the coach is dismissed, **When** the user restarts it from
   Settings, **Then** it resumes at the lowest uncompleted step and previously
   completed steps remain completed.

### Edge Cases

- Inventory is empty when the coach activates after setup.
- A coach completion event arrives for a step the user already completed before
  the coach was started.
- The user dismisses the coach mid-step then triggers the completion event
  through normal use.
- A user uninstalls the sample/demo content provided by spec 003 before
  starting the coach.
- The user is on a route where the current step's anchor element does not
  exist.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The coach MUST activate automatically the first time the app
  opens after first-run setup completes successfully, unless previously
  dismissed.
- **FR-002**: The coach MUST surface progress as overlay hints anchored to real
  UI elements and MUST NOT use modal dialogs or replace real controls with
  stand-ins.
- **FR-003**: The coach MUST advance steps only when the corresponding domain
  event is observed (inventory confirmation, project created, tool opened).
- **FR-004**: The coach MUST be dismissable from any hint with a single
  action and MUST hide all hints when dismissed.
- **FR-005**: The coach MUST be restartable from Settings. When dismissed
  without completing all steps, restart resumes at the lowest uncompleted step
  (previously completed steps remain completed). When restarted after the flow
  has reached `Completed`, restart MUST reset all progress and replay from step
  1 (Idle).
- **FR-010**: When the guided-flow state row is corrupt (deserialization fails,
  invalid state value, or any unrecoverable parse error), the system MUST reset
  the state to Idle, emit a diagnostic audit event containing the corruption
  detail, and continue normally. The `STATE_CORRUPTED` error code is returned on
  the first `guided.state.get` call after the reset (informational — signals the
  reset happened); subsequent reads return the fresh Idle state.
- **FR-006**: The coach MUST persist completed steps and dismissed state across
  app restarts.
- **FR-007**: The coach MUST tolerate the anchor element being absent on the
  current route by deferring the hint until the user navigates to a route that
  exposes the anchor.
- **FR-008**: The coach MUST treat events from normal use as valid completion
  signals, even when the user did not follow the hint.
- **FR-009**: The coach MUST NOT inject sample records into the user's library;
  it operates on whatever real inventory the user has.

### Key Entities

- **GuidedFlowStep**: Static definition of a single coach step (route, anchor,
  completion event, hint text).
- **GuidedFlowState**: Per-install runtime state (current step, completed
  steps, dismissed-at).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user who has not seen the coach before can reach project
  creation without opening external docs in at least 80 percent of internal
  walkthroughs.
- **SC-002**: Every step advances strictly on its domain completion event;
  there is no time-based or click-based fallback.
- **SC-003**: The coach can be dismissed and restarted without leaving any
  stale overlay artifacts or losing prior progress.
- **SC-004**: When the user completes a step through normal use before the
  coach reaches it, the coach records the step complete on next activation.

## Assumptions

- Spec 003 first-run setup completes (or is skipped) before this coach can
  activate.
- Spec 008 project create is available when P2 is reached.
- The lifecycle event bus surfaces inventory confirmation, project created,
  and tool opened events with enough fidelity to drive completion.

## Out of Scope

- Full tutorial content, video walkthroughs, or branching tours.
- Injecting demo inventory, projects, or sample files.
- Coaching for any step beyond first inventory confirm, first project create,
  and first tool open.
- Multi-user or shared coach state.
