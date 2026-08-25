# Data Model: Guided First Project Flow

**Feature**: 010-guided-first-project-flow
**Status**: Draft

## Entities

### GuidedFlowStep

Static definition of a single coach step. Loaded from a registry at app start;
not user-mutable.

| Field              | Type                         | Notes                                                                |
| ------------------ | ---------------------------- | -------------------------------------------------------------------- |
| `id`               | string                       | Stable id, e.g. `inbox.confirm_first`.                               |
| `route`            | string                       | Route pattern where the anchor is expected to exist.                 |
| `trigger`          | string                       | Dot-notation event topic from the lifecycle event bus (e.g. `inventory.confirmed`). |
| `completion_event` | string                       | Canonical event topic name (dot-notation lowercase).                 |
| `hint_text`        | string                       | Localized hint copy keyed by user locale.                            |
| `anchor`           | string                       | `data-guide-anchor` selector value for overlay positioning.          |

Registry (v1):

| id                       | route            | trigger (dot-notation)   | completion_event      | anchor                |
| ------------------------ | ---------------- | ------------------------ | --------------------- | --------------------- |
| `inbox.confirm_first`    | `/inbox`         | `inventory.confirmed`    | `inventory.confirmed` | `inbox.confirm-row`   |
| `project.create_first`   | `/projects`      | `project.created`        | `project.created`     | `projects.create-cta` |
| `tool.open_first`        | `/projects/:id`  | `tool.opened`            | `tool.opened`         | `project.open-in-tool`|

### GuidedFlowState

Per-install runtime state, stored in a single SQLite row.

| Field              | Type                | Notes                                                       |
| ------------------ | ------------------- | ----------------------------------------------------------- |
| `current_step`    | string \| null      | Id of the active step, or `null` when no step is active.    |
| `completed_steps` | string[]            | Ids of completed steps; order preserved.                    |
| `dismissed`       | boolean             | True when the coach was dismissed and not yet restarted.    |
| `dismissed_at`    | ISO 8601 \| null    | Set when `dismissed` becomes true; cleared on restart.      |
| `updated_at`      | ISO 8601            | Updated on every transition.                                |

## Invariants

- `current_step` is null when `dismissed` is true or when all steps in the
  registry are present in `completed_steps`.
- `completed_steps` is a subset of registry step ids; unknown ids are pruned on
  load.
- A step id appears at most once in `completed_steps`.
- `dismissed_at` is non-null if and only if `dismissed` is true.

## Transitions

| From state                | Input                                    | To state                                       |
| ------------------------- | ---------------------------------------- | ---------------------------------------------- |
| Idle                      | `setup_completed`                        | Active(first uncompleted registry step)        |
| Active(s)                 | `completion_event` for any step `t`      | Active(next uncompleted step) or Completed     |
| Active(s)                 | `dismiss`                                | Dismissed                                      |
| Dismissed                 | `restart`                                | Active(lowest uncompleted registry step)       |
| Completed                 | `restart`                                | Idle (progress reset; replay from step 1)      |

## Event Subscription Rules

The guided-flow module subscribes to the lifecycle event bus (spec 002 §6.1).
All event subscribers MUST check the `source` field on the event envelope
(see spec 002 §6 R-Source-1):

- **GuidedSubscription rule**: Ignore any event where `source == "restore"`.
  Replay events from audit-log recovery MUST NOT advance coach steps, because
  the user did not perform the action during this session.
- `source == "user"` or `source == "system"` events advance steps normally.

## Recovery Rules

When the `guided_flow_state` row fails deserialization on app start or state
read (corrupt JSON, unknown state value, schema mismatch):

1. Reset the in-memory state to Idle.
2. Persist the fresh Idle row, overwriting the corrupt row.
3. Emit a `diagnostic` audit event (`guided_flow.state.corrupted`) containing
   the raw corrupt value and the parse error detail.
4. The first `guided.state.get` call after the reset returns error code
   `STATE_CORRUPTED` (informational — the reset has already happened
   server-side; callers should present a non-blocking notice and retry, which
   will return the fresh Idle state).
5. All subsequent reads return the fresh Idle state normally.

## Storage

Single SQLite table `guided_flow_state` with one row enforced by a check
constraint or a singleton primary key. Migrations owned by
`crates/persistence/core`.

## Relationship To Other Specs

- Consumes events emitted by spec 002 (lifecycle state model) and spec 008
  (project create).
- Activates on completion signal from spec 003 (first-run setup wizard).
- Does not own any domain entity; mirrors event state only.
