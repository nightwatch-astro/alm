# Feature Specification: Cleanup And Archive Review Plans

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `017-cleanup-archive-review-plans`  
**Created**: 2026-05-09  
**Status**: Backend implemented (2026-06-11); UI contextual per v4 reconciliation  
**Input**: User description: "Specify what cleanup/archive plans mean, how users review them, and how destructive operations are gated."

> **v4 reconciliation (2026-06-11)**: The plan persistence + use cases
> (list/get/approve/discard/retry + archive send-to-trash / permanently-delete
> with the spec-016 protection gate) and audit events are implemented and tested.
> Design-v4 has NO standalone Plans review page (no `PlansListPage`/`PlanDetailPage`
> route); plans are generated *contextually* by consumer flows, so the plan-review
> UI tasks (T015/T016/T027/T031/T037/T048) are deferred to the specs that GENERATE
> plans and review them inline: 005 (inbox confirm), 008 (project create), 025
> (apply review), 026 (source-view removal), and the Archive page for US6. The
> archive plan GENERATOR (US2, T017–T021) is deferred pending 008 project sources
> + 015 patterns (now available). `plans.apply` remains a spec-025 stub.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Review A Cleanup Plan (Priority: P1)

As a user, I want cleanup candidates collected into a review plan so that I can
understand every proposed filesystem change before anything is applied.

**Why this priority**: The user asked what "plans" means and destructive
operations require a reviewable model.

**Independent Test**: Generate a cleanup plan for a completed project and
confirm each proposed action shows source, destination or deletion target,
reason, protection state, and review status.

**Acceptance Scenarios**:

1. **Given** cleanup candidates exist, **When** a plan is generated, **Then**
   no filesystem mutation occurs.
2. **Given** a plan item is selected, **When** detail opens, **Then** the app
   shows source path, proposed action, reason, protection, and linked
   project/Inventory records.
3. **Given** a plan contains permanent delete candidates, **When** the user
   approves the plan, **Then** those items require destructive confirmation.

---

### User Story 2 - Apply An Archive Plan (Priority: P2)

As a user, I want archive moves to be planned, reviewed, applied, and logged so
that completed projects can be moved safely.

**Why this priority**: Archive location patterns and source protection require
plan-based review.

**Independent Test**: Build an archive plan from a project, review the
generated destination pattern, approve it, and confirm applied moves and
lifecycle events.

**Acceptance Scenarios**:

1. **Given** an archive location pattern is configured, **When** a plan is
   generated, **Then** destination paths are previewed per item.
2. **Given** a destination conflict exists, **When** the plan is reviewed,
   **Then** the item is blocked until resolved.
3. **Given** applying the plan partially fails, **When** the operation ends,
   **Then** the app logs applied and failed items separately and leaves plan
   state clear.

---

### User Story 3 - Approve And Hand Off To Apply (Priority: P3)

As a user, I want a single approval gate that hands an approved plan to the
apply executor so that destructive work cannot start until I explicitly
acknowledge it.

**Why this priority**: The state machine separates *review* (this spec) from
*application* (spec 025); the handoff edge must be specified here so both
specs agree on the contract.

**Independent Test**: From a `ready_for_review` plan, exercise Approve, observe
the plan move to `approved`, then trigger Apply and confirm the plan transitions
into `applying` exactly once.

**Acceptance Scenarios**:

1. **Given** a plan in `ready_for_review`, **When** the user approves, **Then**
   state becomes `approved` and no items are mutated.
2. **Given** an `approved` plan, **When** the user clicks "Apply now", **Then**
   state becomes `applying` and control passes to the apply executor (spec 025).
3. **Given** an `approved` plan, **When** the user reopens it as draft, **Then**
   the approval is invalidated and the plan returns to `draft`.

---

### User Story 4 - Discard An Unwanted Plan (Priority: P4)

As a user, I want to discard a plan I no longer trust so that stale drafts do
not accumulate.

**Why this priority**: A growing list of stale plans creates review fatigue and
hides actionable items.

**Independent Test**: Discard a `draft` plan, confirm it disappears from the
list, and confirm the audit log retains a record.

**Acceptance Scenarios**:

1. **Given** a plan in `draft`, `ready_for_review`, or any terminal state,
   **When** the user discards it, **Then** it is removed from the active list
   and recorded in the audit log.
2. **Given** a plan in `applying`, **When** the user attempts to discard,
   **Then** the operation is refused with `plan.in_progress`.

---

### User Story 5 - Retry After Partial Or Total Failure (Priority: P5)

As a user, when a plan finishes with failures, I want to generate a new retry
plan covering the failed items so the original audit trail stays intact.

**Why this priority**: Retrying in-place mutates history; the agreed model is
"retry plan is a NEW plan" referencing its parent.

**Independent Test**: From a `failed` or `partially_applied` plan, click
"Generate retry plan", confirm a new plan is created with `parentPlanId` set,
containing only the failed items (or all items, per the chosen filter).

**Acceptance Scenarios**:

1. **Given** a `partially_applied` plan, **When** the user retries failed
   items, **Then** a new plan is created in `draft` referencing the parent.
2. **Given** a `failed` plan, **When** the user retries all items, **Then** the
   new plan contains every item from the parent, reset to `pending`.
3. **Given** any non-terminal plan, **When** retry is attempted, **Then** the
   call is refused with `parent.not_terminal`.

### User Story 6 - Manage Archive Contents After Apply (Priority: P5)

As a user, after a cleanup plan has applied and files are in the app-managed
archive folder, I want to be able to send the archive to OS trash or
permanently delete it so I can reclaim disk space when I am confident the
archived files are no longer needed.

**Why this priority**: Archiving preserves safety; cleanup is only complete
once the user confirms the files are truly unwanted.

**Independent Test**: Apply a cleanup plan with `destructiveDestination:
archive`, then use `archive.send_to_trash` to send the archive subfolder to
the OS trash. Confirm audit records the action.

**Acceptance Scenarios**:

1. **Given** a plan whose destructive items have been archived, **When** the
   user chooses "Send archive to trash", **Then** the entire
   `<library_root>/.astro-plan-archive/<planId>/` subtree is moved to the OS
   trash and an audit event is written.
2. **Given** a plan whose archive is in trash, **When** the user chooses
   "Permanently delete archive", **Then** they must type "DELETE" to confirm
   and the subtree is irreversibly removed.
3. **Given** `archive.permanently_delete` is called without `confirmText:
   "DELETE"`, **When** the request arrives, **Then** the operation is refused
   with `confirm.text.mismatch`.

---

### Edge Cases

- Destination path already exists.
- Source path is missing after plan generation.
- Protected source blocks a proposed delete.
- User edits archive pattern after a plan was generated.
- Permanent delete is disabled for reviewed plans in v1.
- A plan's parent was discarded — retry must still be possible if the parent's
  item records survive.
- A plan accumulates many items (1000+); list/detail must remain responsive.
- Concurrent attempts to approve or discard the same plan from two windows.

### Domain Questions To Resolve

- Final names for plan states. (See research.md §1.)
- Whether permanent delete is available in first release. (Deferred; v1 uses
  archive/trash only.)
- Default ordering of plans. (See research.md §5; default: failed-first.)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Cleanup/archive plans MUST be explicit review objects, not
  immediate actions.
- **FR-002**: Plan generation MUST be read-only.
- **FR-003**: Plan items MUST show proposed action, source path, destination
  path when relevant, reason, protection state, and linked entity.
- **FR-004**: Permanent delete MUST require a destructive warning popup and
  explicit confirmation. (Deferred in v1 — archive/trash only.)
- **FR-005**: Plans MUST expose the state vocabulary: `draft`,
  `ready_for_review`, `approved`, `applying`, `paused`, `applied`,
  `partially_applied`, `failed`, `cancelled`, `discarded`. `discarded` is a
  soft-delete terminal: `discarded_at` is set; the plan row is retained;
  `parentPlanId` references remain resolvable; discarded plans are excluded
  from the default `stateFilter` list (A5).
- **FR-006**: Plan apply MUST log every item outcome. (Apply itself is owned by
  spec 025; this spec only requires that the contract surfaces the per-item
  log.)
- **FR-007**: Failed plan apply MUST NOT leave ambiguous project or Inventory
  lifecycle state.
- **FR-008**: Archive destination paths MUST come from the token pattern
  builder (spec 015).
- **FR-009**: Plans list MUST default to failed-first ordering, with secondary
  sort by creation time descending.
- **FR-010**: Plans list MUST support filtering by state and by origin
  (inbox, restructure, cleanup, archive, project source-map).
- **FR-011**: The plan detail surface MUST present a state-aware action bar:
  draft → Approve & Apply; approved → Apply now; applying → Pause/Cancel;
  paused → Resume / Cancel run (R-Pause-1); applied → Back;
  partially_applied/failed → Generate retry plan; cancelled → Back.
  Per-item FS revalidation is the freshness mechanism — no time-based TTL on
  the approval token (A2).
- **FR-012**: A retry plan MUST be a new plan referencing the parent via
  `parentPlanId`. The parent plan MUST NOT be mutated by retry.
- **FR-013**: Discarding a plan MUST be refused while the plan is `applying` or
  `paused`.
- **FR-014**: Approve MUST be refused for plans with zero items.
- **FR-015**: The plans surface MUST render a three-branch empty state: no
  plans, no matches under current filter, or table with results.
- **FR-016**: Cleanup plans with destructive items MUST allow the user to
  choose between two destinations at plan-review time: `archive` (default,
  app-managed, under `<library_root>/.astro-plan-archive/<planId>/`) or
  `os_trash` (OS-native recycle bin: Windows Recycle Bin, macOS Trash, Linux
  XDG trash). The choice is per-plan, not per-item, recorded as
  `destructiveDestination` on the Plan entity (R-Trash-1). OVERRIDE: OS
  trash is available in v1; the prior "OS trash deferred" position is
  rescinded.
- **FR-017**: When all items in a plan have been archived to the
  app-managed archive folder, the user MUST be able to permanently delete
  or send to OS trash the entire plan's archive subfolder via the archive
  management operations `archive.send_to_trash` and
  `archive.permanently_delete` (R-Archive-2). `archive.permanently_delete`
  MUST require an explicit `confirmText: "DELETE"` in the request to gate
  this irreversible action.

### Key Entities

- **Plan**: Reviewable set of proposed filesystem operations, with state,
  origin, item counters, `destructiveDestination`, and optional parent
  reference.
- **Plan Item**: One proposed filesystem operation, with source, destination,
  action, reason, protection, and per-item state.
- **Plan State**: Review and apply lifecycle state, drawn from the
  ten-state vocabulary in FR-005 (including `paused` and `discarded`).
- **Plan Origin**: Where the plan came from (inbox, restructure, cleanup,
  archive, project source-map).
- **Destructive Destination**: Per-plan choice of `archive` or `os_trash`
  for destructive items (FR-016, R-Trash-1).
- **Archive Management**: Post-apply operations on the app-managed archive
  folder: `archive.send_to_trash` and `archive.permanently_delete`
  (FR-017, R-Archive-2).
- **Destructive Confirmation**: Explicit user confirmation for permanent
  delete. In v1, `archive.permanently_delete` requires `confirmText:
  "DELETE"`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can inspect every planned filesystem mutation before
  approval.
- **SC-002**: No permanent delete happens without a warning popup. (v1: no
  permanent delete is offered.)
- **SC-003**: Plan apply results are recoverable from logs and lifecycle
  events.
- **SC-004**: 100% of failed plans expose a "Generate retry plan" path that
  creates a new plan rather than mutating the parent.
- **SC-005**: Filter and sort changes on the plans list render in under 100 ms
  for up to 200 plans.

## Implementation Status

Mockup-only implementation already exists in `apps/desktop/`:

- `src/features/plans/PlansListPage.tsx` — failed-first ordering, state/origin
  filters, three-branch empty state.
- `src/features/plans/PlanDetailPage.tsx` — two-pane review (items + detail),
  applying pane, state-aware action bar covering all eight states.
- `src/data/store.ts` — `usePlans`, `getPlanById`, `updatePlanState`,
  `discardPlan`, and a `simulateApply` that mirrors the eventual apply
  executor's per-item progression and final-state computation.
- `src/data/mock.ts` — `Plan`, `PlanItem`, `PlanState`, plus seed plans
  covering every state in the vocabulary.

No backend, persistence, or filesystem mutation exists yet. The
mockup's `simulateApply` is a stand-in for spec 025's apply executor; the
review surface — list, detail, action bar, approve, discard, retry-as-new-plan
— is owned by this spec.

### Coordination With Spec 025

This spec owns the **review** surface: list, detail, approve, discard, retry.
Spec 025 owns the **apply** executor: per-item execution, pause/cancel
semantics, partial-progress accounting, and post-apply state computation.

The handoff is the `approved → applying` transition. Both specs MUST agree
that:

- The plan's state is the single source of truth across review and apply.
- The apply executor is the only writer of `applying`, `applied`,
  `partially_applied`, `failed`, and `cancelled`.
- This spec is the only writer of `draft`, `ready_for_review`, and `approved`.
- Retry creates a new plan whose `parentPlanId` points at the terminal parent.

## Assumptions

- Cleanup/archive review can apply across projects and Inventory.
- Source protection (spec 016) affects plan approval and per-item protection
  flags.
- The token pattern builder (spec 015) supplies archive destination paths.
- The lifecycle state-machine vocabulary (spec 002 §2.2) is canonical.

## Out of Scope

- Cloud archive services.
- Background scheduled deletion.
- Permanent delete of arbitrary filesystem paths outside the archive folder
  (deferred past v1; `archive.permanently_delete` covers only the
  plan-owned archive subtree).
- The apply executor itself (spec 025).
- Audit log presentation surface (spec 019).
- Per-item destructive destination override (v1 choice is per-plan only).
