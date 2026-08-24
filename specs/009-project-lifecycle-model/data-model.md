# Data Model: Project Lifecycle Model

**Spec**: 009-project-lifecycle-model | **Date**: 2026-05-09

## Project

```
Project {
  id:                Uuid                       // stable identifier
  name:              String                     // user-facing label
  lifecycle:         ProjectLifecycle           // current state (R1)
  tool:              ProcessingTool             // PixInsight | Siril | Planetary Suite
  sources:           ProjectSource[]            // inventory links
  calibrationSets:   CalibrationSetRef[]        // matched calibration sets
  channels:          String[]                   // e.g. ["Ha","OIII","L"]
  plans:             ProjectPlanRef[]           // related filesystem plans
  manifests:         ProjectManifest[]          // historical manifests
  notes?:            String                     // free-text user notes
  lastAction?:       LastAction                 // denormalized R3 marker
  blockedReason?:    BlockedReason              // present iff lifecycle == "blocked"
}
```

```
ProjectLifecycle = "setup_incomplete"
                 | "ready"
                 | "prepared"
                 | "processing"
                 | "completed"
                 | "archived"
                 | "blocked"
```

```
ProjectSource {
  inventoryId:  Uuid
  name:         String
  frames:       u32
  filter:       String
  exposure:     String
}
```

```
ProjectManifest {
  id:         Uuid
  reason:     ManifestReason   // R-Manifest-Reason: closed enum, canonical owner is spec 024
  timestamp:  Timestamp
  path:       String           // relative-to-library path
  body?: {
    sources:      String[]
    calibration:  String[]
    lifecycle:    String       // snapshot label
    notes?:       String
  }
}
```

```
ManifestReason = "created"
               | "source_change"
               | "lifecycle_transition"
               | "cleanup_applied"
               | "workflow_run"
```

`ManifestReason` is a closed enum. Spec 024 is the canonical owner; this spec
references the enum. Do not add new values without a spec 024 amendment
(R-Manifest-Reason, GRILL 2026-05-22, E3).

Cross-spec dependency: spec 024 `data-model.md §Trigger Taxonomy` owns the
values. This spec uses them but does not redefine them.

```
ProjectPlanRef {
  id:     Uuid
  title:  String
  state:  PlanState           // spec 002 PlanState
}
```

```
LastAction {
  label:  String              // R2-derived or caller-supplied
  when:   Timestamp           // truncated to minute for stable rendering
}
```

```
BlockedReason =
  | { kind: "source_missing",       inventoryId: Uuid }
  | { kind: "prepared_source_stale", preparedId:  Uuid }
  | { kind: "tool_unconfigured",    tool: ProcessingTool }
  | { kind: "calibration_unmatched", calibrationSetId: Uuid }
  | { kind: "user",                 note: String }
```

## Transition Table

Eighteen allowed edges (sixteen original + `blocked → archived` escape hatch
per A3 + `archived → ready` unarchive per R-Unarchive, both GRILL 2026-05-22).
The `requires_plan` column marks edges whose use case sets `requires_plan = true`
on the spec 002 envelope (server-derived from the spec 002 plan-requirement edge
table — callers do NOT pass `requires_plan`; see A6). The `trigger` column
describes who normally drives the edge.

**`actor=system` authorization (A4)**: The server enforces that `actor=system`
is only allowed on edges entering or leaving `blocked` (`* → blocked` and
`blocked → *`), PLUS the deterministic invariant-driven `setup_incomplete →
ready` transition (R-Ready-Trigger). Any other system-actor edge is rejected
with `transition.refused`.

| From               | To                  | Default action label     | Trigger                     | Side effect / requires_plan                        |
| ------------------ | ------------------- | ------------------------ | --------------------------- | -------------------------------------------------- |
| `setup_incomplete` | `ready`             | `Marked ready`           | user OR system (invariant)  | none; system path is invariant-driven (R-Ready-Trigger) |
| `setup_incomplete` | `blocked`           | `Marked blocked`         | user or system              | none                                               |
| `ready`            | `prepared`          | `Marked prepared`        | user                        | PreparedSource creation, **plan required**         |
| `ready`            | `processing`        | `Marked processing`      | user                        | none (Open in tool is launch-only)                 |
| `ready`            | `blocked`           | `Marked blocked`         | user or system              | none                                               |
| `prepared`         | `ready`             | `Reverted to ready`      | user                        | PreparedSource retire, **plan required**           |
| `prepared`         | `processing`        | `Marked processing`      | user                        | none                                               |
| `prepared`         | `blocked`           | `Marked blocked`         | user or system              | none                                               |
| `processing`       | `completed`         | `Marked completed`       | user                        | none                                               |
| `processing`       | `blocked`           | `Marked blocked`         | user or system              | none                                               |
| `completed`        | `archived`          | `Marked archived`        | user                        | Archive manifest snapshot, **plan always required** (R-Archived-Plan) |
| `completed`        | `processing`        | `Re-opened`              | user                        | none                                               |
| `archived`         | `processing`        | `Unarchived`             | user                        | **Plan required when** (a) sources mapped to different paths OR (b) any source content needs to move to active project root. Plan NOT required when only metadata (notes, lifecycle) changes (C7). |
| `archived`         | `ready`             | `Unarchived`             | user                        | **R-Unarchive (GRILL 2026-05-22)**: Unarchive to ready state for users who want to revisit a project without immediately resuming processing. Plan requirements mirror `archived → processing` (C7 criterion). Audit event `project.unarchived` emitted. Actor: user only. This edge enables view operations on previously-archived projects after unarchiving (cross-reference: spec 026 R-026-Lifecycle). |
| `blocked`          | `setup_incomplete`  | `Resolved blocker`       | user or system              | none                                               |
| `blocked`          | `ready`             | `Resolved blocker`       | user or system              | none                                               |
| `blocked`          | `prepared`          | `Resolved blocker`       | user or system              | none                                               |
| `blocked`          | `processing`        | `Resolved blocker`       | user or system              | none                                               |
| `blocked`          | `archived`          | `Archived from blocked`  | user (explicit confirmation required) | Escape hatch: archive a permanently-blocked project. **Plan always required** (same as `completed → archived`). `blocked → completed` remains forbidden (A3, GRILL 2026-05-22). |

All other `(from, to)` combinations are rejected with error code
`transition.refused` and a `details.allowed_next_states` array enumerating
the legal successors for the current state.

**Note on `archived → ready` (R-Unarchive, GRILL 2026-05-22)**: This edge was
added after the initial seventeen-edge ratification. It is the primary unarchive
path for users who do not need to resume processing immediately. The `archived →
processing` edge remains valid as an alternate direct-resume path. The mockup
implementation (`archived` footer showing "Unarchive" as primary action) should
surface both target states with appropriate labels: "Unarchive to Ready" and
"Unarchive and Resume" (or equivalent UX per the design pass).

**`completed → archived` plan note (R-Archived-Plan)**: A plan is always
required even when no files move. When no physical files move, plan generation
produces a Plan with at least the manifest-write item (a structural item, not a
filesystem mutation). The plan still writes the manifest snapshot per spec 024.
This preserves the audit trail for every archive action.

**`setup_incomplete → ready` auto-transition (R-Ready-Trigger)**: After every
`project.update` or `project.source.add`, the use case fires an invariant check.
If `tool != null AND ≥1 confirmed source mapped`, the lifecycle service
auto-transitions from `setup_incomplete` to `ready` using `actor=system`. This
is classified as an "automatic invariant transition" — a sub-classification of
system-actor usage permitted alongside the `* → blocked` / `blocked → *` family
(see A4 reconciliation). The auto-transition emits a `project.lifecycle.ready`
event on the event bus.

**Blocked-flag debounce (D5)**: The detector layer (watchers / health-check
tasks that trigger `* → blocked`) MUST debounce on the same
`(entity_id, blocking_condition)` for at least 60 seconds before re-emitting
a block signal. The lifecycle layer itself does NOT debounce — it executes every
transition it receives. This separation keeps the lifecycle use case simple and
puts debounce responsibility where it belongs: the event producer.

## Derived Views

### ProjectSummary (returned by `project.list`)

```
ProjectSummary {
  id:           Uuid
  name:         String
  lifecycle:    ProjectLifecycle
  tool:         ProcessingTool
  sourceCount:  u32
  channels:     String[]
  blockedReason?: BlockedReason
  lastAction?:  LastAction
}
```

Full `Project` is fetched by detail-view contracts; the list endpoint
deliberately omits `manifests`, `notes`, and full `sources` to keep paged
responses small.

## Invariants

- `lifecycle == "blocked"` ⇔ `blockedReason` is present.
- `lifecycle == "setup_incomplete"` ⇒ either `sources.length == 0` or at
  least one source is unmapped/unconfirmed. This state is NEVER entered due
  to a missing tool (tool is required at project creation — R-Tool-Req).
- The auto-invariant `tool != null AND ≥1 confirmed source mapped` ⇒ system
  fires `setup_incomplete → ready` automatically (R-Ready-Trigger). This is an
  "automatic invariant transition" sub-classification of `actor=system` use.
- `lastAction` MAY be absent only for never-transitioned projects; once any
  transition has been recorded it is present and monotonically forward in
  time.
- `manifests[*].body.lifecycle` is a snapshot label and MUST NOT be used as
  an authoritative state read.
- `manifests[*].reason` is a `ManifestReason` closed enum value (spec 024
  canonical owner — R-Manifest-Reason, E3).
- `plans[*].id` references spec 017 plan records; if the referenced plan is
  garbage-collected, the reference is hidden from the UI but the manifest
  trail is preserved.
- `plans[*].state` uses the `PlanState` enum defined by spec 002, which
  includes `paused` and `discarded` per the spec 017+025 amendment (E6).

## Storage Notes

- Persisted in the SQLite store managed by the `crates/persistence/` crates.
- `lifecycle`, `blockedReason.kind`, and `lastAction.when` are indexed for
  filter and sort.
- `lastAction` is denormalized; `crates/audit/` retains the durable record.
