# Feature Specification: Filesystem Plan Application

> **See Spec 030**: UI implementation of this feature must follow
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md)
> for layout, navigation, and component patterns.

**Feature Branch**: `025-filesystem-plan-application`  
**Created**: 2026-05-09  
**Updated**: 2026-06-23  
**Status**: Implemented; 6 items open (see Implementation Status)  
**Input**: User description: "Specify applying reviewed filesystem plans, including per-item outcomes, rollback where possible, progress, failures, and audit records."

## Implementation Status

The real plan-apply path shipped through
[Spec 041 — Inbox Plan Surface](../041-inbox-plan-surface/spec.md):
`crates/app/core/src/plan_apply.rs`,
`crates/persistence/plans/src/repositories/plan_apply.rs`,
`crates/app/inbox/src/plan_listener.rs`, with audit coverage in
`crates/app/core/tests/plan_apply_audit_integration.rs`.

Live progress reporting and the apply UI shipped as
`apps/desktop/src/features/plans/usePlanApplyProgress.ts` and
`apps/desktop/src/features/plans/PlanReviewOverlay.tsx`, which also carries the
paused/`resumeStalled` state.

Open items, tracked in beads and GitHub:

- `resume_plan` does not re-spawn the executor to continue pending items
  (issue #575).
- Rollback-audit coverage needs a real cross-device `EXDEV` error, not
  producible in an ordinary tempdir (issue #577).
- Three race/device-unavailable tests need non-deterministic OS conditions.
- Docs cross-links and the 10k-item perf benchmark.

### Boundary with spec 017

Spec **017 (Cleanup And Archive Review Plans)** owns plan generation, item
preview, review, **approve**, and **discard**. It also owns `plan.retry`,
which materialises a **new plan** from failed (or all) items of a terminal
parent plan; that new plan returns to draft and follows the 017 review flow
before reaching 025 again.

Spec **025 (this feature)** owns:

- The **apply executor** (the long-running operation that mutates the
  filesystem).
- **Item-level progression** semantics (`pending → applying → succeeded |
  failed | skipped | cancelled`).
- **Terminal state computation** for the plan (`applied`,
  `partially_applied`, `failed`, `cancelled`).
- **Cancellation** of an in-flight apply.
- **Per-item skip** (only while still pending) and **per-item retry within a
  running apply** (re-attempt a failed item without creating a new plan).
- The **audit records** written per item attempt.

`plan_id` and `item_id` shapes, the plan state machine pre-apply, and the
retry-via-new-plan flow are shared with 017; this spec MUST NOT redefine them.

### Item-progression semantics (from mockup)

- A plan must be in `approved` before apply begins. The executor transitions
  the plan to `applying` and processes items in plan order. Parallelism is a
  research question; the mockup is strictly sequential.
- Each item enters `applying`, then resolves to `succeeded`, `failed`, or
  `skipped`. Items the user did not reach when cancelling resolve to
  `cancelled`.
- Counters (`itemsApplied`, `itemsFailed`, `itemsPending`) update after each
  item resolves so the UI can render progress without polling per item.
- Re-applying an `approved` plan re-runs items still in `pending`. Items in
  `failed` are preserved across re-apply unless the user explicitly retries
  them (item-level retry) or routes them through 017's `plan.retry` to a new
  plan.

### Cancellation behaviour

- Cancellation is a user-initiated transition while the plan is `applying`.
- The executor MUST finish the item that is currently `applying` (no torn
  writes mid-rename or mid-copy) and MUST NOT start any further items.
- Items already `succeeded`/`failed` keep their state. Untouched items move
  from `pending` to `cancelled`.
- The plan terminal state after cancellation is `cancelled` even if some
  items succeeded; the audit log preserves which items applied so the user
  can reason about partial progress.

### Terminal-state computation

After the last item resolves (or after cancellation halts progress) the
executor computes the plan terminal state:

- `applied`: every item is `succeeded` (and at least one item ran).
- `partially_applied`: at least one `succeeded` and at least one `failed`,
  with no `cancelled` items.
- `failed`: at least one `failed` and zero `succeeded`.
- `cancelled`: the run halted via cancellation; failures and successes
  before cancellation are preserved on individual items.

### Retry handoff to spec 017

There are **two** retry pathways and they are distinct:

1. **Per-item retry within a running apply (025)**: a failed item is
   re-attempted in the same `applying` run. No new plan is created.
2. **Plan-level retry after terminal (017)**: a terminal plan (`failed`,
   `partially_applied`, `cancelled`) is the input to `plan.retry` in 017,
   which materialises a fresh `draft` plan with the chosen items.

Spec 025 MUST refuse to "retry" a terminal plan in place — that flow
belongs to 017.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Apply A Reviewed Plan (Priority: P1)

As a user, I want to apply a reviewed filesystem plan only after approval so that moves, archive actions, generated-resource cleanup, and deletes are deliberate.

**Why this priority**: Plan review is only useful if apply behavior is explicit and auditable.

**Independent Test**: Approve a plan with move, archive, remove generated source view, and skipped items; apply it and confirm per-item outcomes and lifecycle state.

**Acceptance Scenarios**:

1. **Given** a plan is not approved, **When** the user tries to apply it, **Then** the app blocks the operation.
2. **Given** a plan is approved, **When** the user applies it, **Then** each item shows progress and final outcome.
3. **Given** an item fails, **When** apply completes, **Then** the plan state is `partially_applied` (if any item succeeded) or `failed` (if none did) and successful items remain recorded.

---

### User Story 2 - Handle Failure Safely (Priority: P2)

As a user, I want failures during plan application to be logged and recoverable so that I can retry or adjust without losing track of what happened.

**Why this priority**: Filesystem operations can fail due to permissions, missing files, destination conflicts, or removable drives.

**Independent Test**: Apply a plan where one destination is blocked and confirm the app logs success/failure per item and keeps retry context.

**Acceptance Scenarios**:

1. **Given** a destination conflict appears during apply, **When** that item runs, **Then** the item fails with a structured reason and later items follow the plan policy.
2. **Given** rollback is possible for a failed operation, **When** rollback runs, **Then** rollback outcome is logged separately.
3. **Given** rollback is not possible, **When** apply ends, **Then** the app clearly marks manual recovery steps.

---

### User Story 3 - Cancel An In-Flight Apply (Priority: P2)

As a user, I want to stop an apply that is running so that I can intervene before more changes happen.

**Independent Test**: Start applying a plan with many items, cancel while it is running, and confirm the plan terminal state is `cancelled`, finished items keep their outcomes, and remaining items move to `cancelled`.

**Acceptance Scenarios**:

1. **Given** a plan is `applying`, **When** the user cancels, **Then** the current item finishes and no further items start.
2. **Given** the run was cancelled, **When** apply ends, **Then** the plan state is `cancelled` and per-item audit records preserve which items applied.

---

### User Story 4 - Per-Item Skip And Retry Within Apply (Priority: P3)

As a user, I want to skip a pending item or retry a failed item without restarting the whole plan, so I can resolve issues incrementally.

**Independent Test**: While a plan is `applying`, mark a pending item skipped (it transitions to `skipped` and is not executed) and retry a failed item (it transitions back to `applying`).

**Acceptance Scenarios**:

1. **Given** an item is `pending` during an active apply, **When** the user skips it, **Then** the item becomes `skipped` and is not executed.
2. **Given** an item is `failed` during an active apply, **When** the user retries it, **Then** the item becomes `applying` again and the executor re-attempts it once.
3. **Given** an item is not in the eligible state, **When** skip/retry is requested, **Then** the operation returns a structured error.

### Edge Cases

- Source path disappears between approval and apply.
- Destination path appears between approval and apply.
- User cancels while apply is running mid-item (current item must finish or fail cleanly; no torn write).
- Permanent delete is included and requires destructive confirmation.
- Some items are protected by source policy.
- Re-apply of a plan that already has terminal failed items: only `pending` items run; `failed` items are preserved.
- Approval token issued by 017 has been invalidated by a later plan edit.

### Domain Questions To Resolve

- Default failure policy: stop on first failure or continue safe independent items. *(Default selected: continue; see research.md.)*
- Which operations support automatic rollback. *(See research.md failure-mode taxonomy.)*
- Whether per-volume parallelism is acceptable or whether v1 stays sequential.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Filesystem plans MUST require approval (an approval token issued by spec 017) before apply.
- **FR-002**: Apply MUST show progress and per-item outcomes incrementally, not only at completion.
- **FR-003**: Apply MUST log request id, entity metadata, operation, prior state, new state, and item outcome for every item state transition.
- **FR-004**: Permanent delete items MUST require destructive confirmation immediately before apply.
- **FR-005**: Failed items MUST retain a structured failure record (`code`, `message`, `recoverable`) and remain available for per-item retry within the current apply or for plan-level retry via 017.
- **FR-006**: Partial success MUST result in `partially_applied`; total failure with zero successes MUST result in `failed`; cancellation MUST result in `cancelled` regardless of partial successes.
- **FR-007**: Rollback attempts MUST be logged as separate audit events and MUST NOT be assumed to succeed.
- **FR-008**: Source protection MUST be enforced during apply, not only during plan generation.
- **FR-009**: Cancellation MUST halt forward progress; the currently `applying` item MUST be allowed to complete or fail; remaining `pending` items MUST become `cancelled`.
- **FR-010**: Re-applying an `approved` plan MUST resume from items still in `pending` and MUST NOT silently re-run `succeeded` items.
- **FR-011**: Apply MUST refuse if the approval token issued by 017 has been
  invalidated (`plan.approval.stale`). No time-based TTL; freshness is
  enforced by per-item FS revalidation (A2).
- **FR-012**: Plan generation MUST compute `totalBytesRequired` and verify
  each destination volume has sufficient free space (with a safety margin).
  Plan generation MUST fail (plan does not enter `draftable` state) if the
  pre-flight space check fails. Mid-apply `disk.full` is a recoverable
  per-item failure that pauses the run (A4, R-Pause-1).
- **FR-013**: Before each item mutation the executor MUST check (a) source
  path's current `(mtime, sizeBytes)` matches the `approvedMtime` /
  `approvedSizeBytes` snapshot stored by `plan.approve`, AND (b) the
  destination path is empty (no name conflict). On mismatch: item state →
  `stale`; run pauses (R-Pause-1); UI shows "Plan is stale" dialog with
  regenerate option. Error code: `item.stale` (`recoverable: true`;
  non-skippable; requires re-approval via regenerated plan) (R-FS-1).
- **FR-014**: Before any filesystem mutation the executor MUST canonicalize
  source and destination paths and verify each resolves inside a registered
  library root. Paths resolving outside a root MUST fail with
  `path.invalid`. Symlinks in resolved paths MUST be treated as scope
  violations unless the root has explicit symlink-follow enabled
  (constitution §V, A6).
- **FR-015**: When the executor encounters `volume.unavailable`, `disk.full`,
  or `item.stale` mid-apply, the run MUST transition to `paused`. The UI
  surfaces a dialog with actions: "Remount drive" / "Free space" / "Resume"
  / "Cancel run". "Cancel run" calls `plan.cancel`; "Resume" calls
  `plan.resume` (R-Pause-1).
- **FR-016**: The apply executor MUST perform an atomic compare-and-swap on
  `plans.state = 'approved' → 'applying'` before starting. If the CAS fails
  (state changed between read and write), apply MUST return
  `plan.invalid_state` and not start the run (R-CAS-1).
- **FR-017**: Multiple plans MAY apply concurrently only if their
  (source ∪ destination) path sets are disjoint at subtree-prefix
  granularity. Overlapping plans are rejected with `plan.conflict.overlap`
  (R-Concur-1).

### Key Entities

- **Approved Filesystem Plan**: Review object authorized for apply (owned by 017; consumed here).
- **Plan Apply Run**: One execution attempt against a plan.
- **Plan Item Outcome**: `succeeded`, `failed`, `skipped`, `cancelled`, or `pending`.
- **Plan Item Failure**: `{ code, message, recoverable }`.
- **Plan Apply Event**: `{ plan_id, item_id, prior_state, new_state, at }` audit record.
- **Rollback Outcome**: Result of any attempted reversal.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can reconstruct every applied filesystem operation from the plan apply record.
- **SC-002**: Partial failures never leave the plan with an ambiguous final state (terminal state is always one of `applied`, `partially_applied`, `failed`, `cancelled`).
- **SC-003**: Protected-source delete attempts are blocked or confirmed according to source policy.
- **SC-004**: Cancelling an in-flight apply halts forward progress within one item, never starts a new item after cancel is observed.

## Assumptions

- Plan generation, review, approval, discard, and plan-level retry are owned by spec 017.
- The app can record audit events even if filesystem mutation fails.
- v1 apply is sequential within a single plan; parallelism is a research question.

## Out of Scope

- Background scheduling.
- Remote filesystem operations.
- Plan-level retry (covered in spec 017 via `plan.retry`).
- Plan generation, review, approve, and discard (spec 017).
