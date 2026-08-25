# Data Model: Cleanup And Archive Review Plans

**Feature**: 017-cleanup-archive-review-plans  
**Date**: 2026-05-20

## Entities

### Plan

A reviewable proposed set of filesystem operations.

| Field           | Type                                      | Notes |
|-----------------|-------------------------------------------|-------|
| `id`            | string (uuid)                             | stable identifier |
| `number`        | integer                                   | display-friendly monotonically increasing number |
| `title`         | string                                    | e.g. "Split /raw/2026-04" |
| `origin`        | enum: `inbox` \| `restructure` \| `cleanup` \| `archive` \| `project` | source generator |
| `originPath`    | string (optional)                         | filesystem context when applicable |
| `state`         | `PlanState`                               | see lifecycle below |
| `createdAt`     | timestamp (ISO 8601)                      | creation time |
| `items`         | `PlanItem[]`                              | proposed operations (may be paged in detail load) |
| `itemsTotal`              | integer                                   | item count |
| `itemsApplied`            | integer                                   | succeeded so far |
| `itemsFailed`             | integer                                   | failed so far |
| `itemsSkipped`            | integer                                   | skipped so far (A3) |
| `itemsCancelled`          | integer                                   | cancelled so far (A3) |
| `itemsPending`            | integer                                   | still pending |
| `totalBytesRequired`      | integer                                   | pre-flight space estimate in bytes (A4) |
| `destructiveDestination`  | enum: `archive` \| `trash`                | per-plan destination for destructive items, default `archive` (R-Trash-1). Canonical vocab per spec 033 / migration 0040 (`os_trash` was the pre-0040 token) |
| `type`                    | enum: `split` \| `restructure` \| `cleanup` \| `archive` \| `source_map` | execution shape |
| `parentPlanId`            | string (uuid, optional)                   | set on retry plans; references the terminal parent |
| `discardedAt`             | timestamp (ISO 8601, optional)            | set when state transitions to `discarded` (A5) |

Invariants:

- `itemsTotal == itemsApplied + itemsFailed + itemsSkipped + itemsCancelled +
  itemsPending` MUST hold at every observation point (A3).
- `parentPlanId` MUST reference a plan in a terminal state at the time the
  retry plan was created.
- A plan with `itemsTotal == 0` MUST NOT be approvable.
- `totalBytesRequired` is computed at plan generation time; plan generation
  MUST fail (plan does not enter `draftable` state) if pre-flight space check
  fails for any destination volume (A4).
- `discardedAt` is set and immutable once `state == discarded`; discarded plans
  are soft-deleted (row retained, `parentPlanId` references resolvable) and
  excluded from default list results (A5).

### PlanItem

One proposed filesystem operation.

| Field           | Type                                      | Notes |
|-----------------|-------------------------------------------|-------|
| `id`            | string                                    | stable per-item id |
| `index`         | integer                                   | per-plan ordinal (1-based) |
| `name`          | string                                    | display name (filename or summary) |
| `action`        | enum: `move` \| `archive` \| `delete` \| `link` \| `write` | proposed action |
| `from`          | string                                    | source absolute path |
| `to`            | string                                    | destination absolute path (may be empty for `delete`) |
| `reason`        | string                                    | human-readable rationale |
| `protection`    | enum: `normal` \| `protected`             | from spec 016 |
| `linked`        | string (optional)                         | linked Inventory session, project, or calibration set |
| `state`               | enum: `pending` \| `applying` \| `succeeded` \| `failed` \| `skipped` \| `cancelled` | per-item progression (E5) |
| `failureReason`       | string (optional)                         | populated by apply executor on failure |
| `provenance`          | `{ label, value }[]` (optional)           | how the item was inferred |
| `approvedMtime`       | timestamp (ISO 8601, optional)            | source mtime snapshot taken at approve time (R-FS-1) |
| `approvedSizeBytes`   | integer (optional)                        | source size snapshot taken at approve time (R-FS-1) |
| `archivePath`         | string (optional)                         | computed destination under `<library_root>/.astro-plan-archive/<planId>/<relative_source_path>` for destructive items (R-Archive-1) |

### PlanState

The ten-state lifecycle vocabulary, drawn from spec 002 §2.2 and reproduced
here for completeness. Added `paused` (R-Pause-1) and `discarded` (A5).

```
draft            → { ready_for_review, discarded }
ready_for_review → { approved, draft, discarded }
approved         → { applying, draft }            # reopen invalidates
applying         → { applied, partially_applied, failed, cancelled, paused }
paused           → { applying, cancelled }        # resume or cancel only (R-Pause-1)
applied          → ∅ (terminal)
partially_applied→ ∅ (terminal — retry plan is a NEW plan)
failed           → ∅ (terminal — retry plan is a NEW plan)
cancelled        → ∅ (terminal)
discarded        → ∅ (terminal — soft-delete; row retained; parentPlanId references resolvable)
```

Lifecycle table:

| State              | Writer       | Terminal? | Allowed transitions                                             | Notes |
|--------------------|--------------|-----------|-----------------------------------------------------------------|-------|
| `draft`            | review (017) | no        | → `ready_for_review`, → `discarded`                            | initial state from any generator |
| `ready_for_review` | review (017) | no        | → `approved`, → `draft`, → `discarded`                        | items materialised and shown in detail |
| `approved`         | review (017) | no        | → `applying`, → `draft`                                       | reopen as draft invalidates approval |
| `applying`         | apply  (025) | no        | → `applied`, → `partially_applied`, → `failed`, → `cancelled`, → `paused` | only executor writes; paused on mid-apply fault (R-Pause-1) |
| `paused`           | apply  (025) | no        | → `applying`, → `cancelled`                                   | resume via `plan.resume`; cancel via `plan.cancel` (R-Pause-1) |
| `applied`          | apply  (025) | yes       | ∅                                                             | every item succeeded |
| `partially_applied`| apply  (025) | yes       | ∅                                                             | retry creates a new plan |
| `failed`           | apply  (025) | yes       | ∅                                                             | retry creates a new plan |
| `cancelled`        | apply  (025) | yes       | ∅                                                             | forward progress halted by user; no rollback |
| `discarded`        | review (017) | yes       | ∅                                                             | soft-delete; `discardedAt` set; excluded from default list filter (A5) |

Key constraint (carried from spec 002 §2.2): **a retry plan is a new plan**
with its own audit trail, referencing the failed plan by id via
`parentPlanId`. This was confirmed in the mockup by the "Generate retry plan
for failures" CTA, which calls `createPlan(retry, parent_id)` rather than
mutating the failed plan in place.

## Relationships

- `Plan` 1—n `PlanItem` (composition).
- `Plan` 0..1—1 `Plan` (`parentPlanId` references parent for retry chains).
- `Plan` n—1 audit-event stream (`crates/audit/`).
- `PlanItem.linked` is a soft reference to an Inventory session, project, or
  calibration set; review surface does not enforce referential integrity but
  warns if the referent is missing.

## Storage Notes

- Plans and items live in SQLite tables owned by `crates/persistence/plans/`.
- Item counters (`itemsApplied`, `itemsFailed`, `itemsSkipped`, `itemsCancelled`,
  `itemsPending`) are persisted on the plan row to avoid recomputing on list
  render; the apply executor is responsible for keeping them coherent (A3).
- Paths are stored as `(root_id, relative_path)` pairs internally; the review
  contracts return resolved absolute paths for display.
- `totalBytesRequired` is computed at plan generation by summing source file
  sizes for copy/archive operations; stored on the plan row and surfaced in
  the review surface for user visibility (A4).
- `destructiveDestination` defaults to `archive`; switching to `trash`
  before approval directs the apply executor to use the OS-native recycle
  bin API (R-Trash-1).
- Destructive `PlanItem` rows include `archivePath` (computed as
  `<library_root>/.astro-plan-archive/<planId>/<relative_source_path>`).
  Conflict naming: if destination exists, append `.<n>` before the extension.
  The archive folder is filesystem-visible and appears in spec 016 protected
  categories by default. Per-plan subfolders enable bulk operations
  (R-Archive-1, R-Archive-2).
- `approvedMtime` and `approvedSizeBytes` are populated by `plan.approve` at
  approval time. The apply executor re-checks these before each item mutation
  (per-item FS revalidation). Any mismatch causes that item to enter `stale`
  state, the run pauses, and the user must regenerate the plan (R-FS-1).
