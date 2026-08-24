# Data Model: Data Lifecycle State Model

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md) | **Date**: 2026-05-20

This document defines the canonical entity tables, invariants, and lifecycle
transition graphs that anchor the Rust port in `crates/domain/core/`,
`crates/audit/`, and the `crates/persistence/` crates. State family names and
transition graphs are frozen from `research.md` §1–§2; entities are anchored
on `Data Asset` per spec.md FR-007.

Conventions:

- `id` fields are UUIDv4 unless otherwise noted.
- Timestamps are RFC 3339 UTC.
- Provenance is carried via `ProvenancedValue<T>` (see §ProvenancedValue),
  not via columns on the entity table.
- Ledger rows omit `confidence | evidence | provenance` columns (FR-006);
  those are available in detail views via `ProvenancedValue.history`.

---

## LibraryRoot

A user-configured filesystem mount that anchors relative paths so external
drives and remapped roots can be recovered without rewriting history.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `label` | string | yes | Human-readable name (e.g. "External NAS"). |
| `current_path` | string | yes | Absolute OS path at the current moment. |
| `kind` | enum(`local`,`external`,`network`) | yes | Mount class. |
| `state` | enum(`active`,`missing`,`disabled`,`reconnect_required`) | yes | See research.md §1. |
| `last_seen_at` | datetime | no | Last successful scan touch. |
| `created_at` | datetime | yes | Initial registration timestamp. |

### Invariants

- `current_path` is mutable; `id` is not. Path remapping MUST NOT rewrite child `FileRecord` rows.
- A `LibraryRoot` in `missing` or `reconnect_required` MUST NOT be auto-promoted to `active` without a user-triggered rescan.
- `disabled` roots remain queryable but are excluded from default scan sweeps.

### Lifecycle

| From | To | Trigger | Side effects |
|---|---|---|---|
| `active` | `missing` | Scan probe fails | Children flagged `unverified` (read-only projection). |
| `missing` | `active` | Successful rescan at remembered path | Children re-verified; `last_seen_at` updated. |
| `missing` | `reconnect_required` | User changes the stored path | Awaiting user re-validation. |
| `reconnect_required` | `active` | User confirms new path + scan | Path remap recorded in audit. |
| `active` | `disabled` | User disables root | Excluded from sweeps; no child mutation. |
| `disabled` | `active` | User re-enables | Resume sweeps. |

---

## FileRecord

A scanned filesystem entry under a `LibraryRoot`. Source-of-truth for
"observed" facts.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `root_id` | uuid | yes | FK → `LibraryRoot.id`. |
| `relative_path` | string | yes | POSIX-normalised path under root. |
| `size_bytes` | u64 | yes | Observed size at last scan. |
| `mtime` | datetime | yes | Observed modification time. |
| `content_hash` | string | no | Lazy; populated only when a workflow demands it. |
| `state` | enum(`observed`,`missing`,`changed`,`classified`,`rejected`,`protected`) | yes | See spec.md §State Families (Inventory Record). |
| `first_seen_at` | datetime | yes | First scan that observed this path. |
| `last_seen_at` | datetime | yes | Most recent scan touch. |

### Invariants

- `(root_id, relative_path)` is unique.
- `content_hash` MUST remain optional; computing it MUST NOT be a side effect of plain enumeration.
- Symlinks/junctions are not followed unless the root explicitly opted in.
- A `FileRecord` MUST NOT be hard-deleted by the app; transition to `missing` instead.

### Lifecycle

| From | To | Trigger | Side effects |
|---|---|---|---|
| `observed` | `classified` | Inventory pipeline assigns frame kind | `ProvenancedValue` inferred entries written. |
| `observed` | `changed` | Rescan detects size/mtime drift | Dependent projections marked `stale`. |
| `observed` | `missing` | Rescan no longer sees the path | Dependent projections marked `stale`. |
| `changed` | `observed` | New scan succeeds and metadata re-parses | Projections regenerated. |
| `classified` | `rejected` | User rejects | Removed from default ledgers; preserved for audit. |
| `rejected` | `classified` | User reinstates | Re-enters ledgers. |
| `*` | `protected` | User marks protected category | Excluded from cleanup plans. |

---

## AcquisitionSession

A grouping of light frames sharing a metadata-derived session key
(FR-011/FR-012). Folder layout is provenance, not identity.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `session_key` | string | yes | Derived from FITS/XISF/video metadata. |
| `target_id` | uuid | no | FK → `Target.id` after confirmation. |
| `frame_ids` | uuid[] | yes | FK list → `FileRecord.id`. |
| `state` | enum(`discovered`,`candidate`,`needs_review`,`confirmed`,`rejected`,`ignored`) | yes | See research.md §2.3. |
| `observer_location` | `ProvenancedValue<ObserverLocation>` \| null | no | Observer location at capture time. See §ObserverLocation below (R-Obs). |
| `review_snapshot_id` | uuid | no | FK to the immutable snapshot captured at last review (FR-005). |
| `last_action` | object | no | `{label, at, actor}` for UI projection. |
| `created_at` | datetime | yes | First derivation timestamp. |

### Invariants

- A session's `session_key` MUST be reproducible from its members' metadata.
- Members with divergent session keys MUST be split into separate sessions (FR-012).
- `confirmed` and `rejected` are soft-terminal; both are re-openable to `needs_review`.
- Each transition into a review state MUST snapshot the contributing observed/inferred/reviewed context (FR-005).
- When `observer_location` extraction fails or yields null for a session where location is action-critical, `AcquisitionSession.state` transitions to `needs_review` with `observer_location` as a blocking field. The `provenance.unreviewed` error code is used (R2.2). User review resolves the block.

### Lifecycle

Source: research.md §2.3.

| From | To | Trigger | Side effects |
|---|---|---|---|
| `discovered` | `candidate` | Session key stabilises across all members | None. |
| `discovered` | `ignored` | User dismisses noise | None. |
| `candidate` | `needs_review` | Action-critical fields are unresolved (FR-010) | UI surfaces blocking fields. |
| `candidate` | `confirmed` | Action-critical fields are all `reviewed` | Snapshot written; `last_action` updated. |
| `candidate` | `rejected` | User rejects | Snapshot written. |
| `needs_review` | `confirmed` | User confirms required fields | Snapshot written. |
| `needs_review` | `rejected` | User rejects | Snapshot written. |
| `confirmed` | `needs_review` | Re-open review | Prior snapshot retained; new pending review. |
| `confirmed` | `rejected` | User rejects after confirm | New snapshot. |
| `rejected` | `needs_review` | User reopens | New snapshot. |
| `ignored` | `candidate` | User un-ignores | None. |

---

## CalibrationSession

A grouping of calibration frames (darks/flats/bias) sharing equipment +
exposure metadata. Same state family as `AcquisitionSession`.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `session_key` | string | yes | Equipment + exposure + temperature signature. |
| `frame_ids` | uuid[] | yes | FK list → `FileRecord.id`. |
| `kind` | enum(`dark`,`flat`,`bias`,`flat_dark`) | yes | Calibration frame kind. |
| `state` | enum(`discovered`,`candidate`,`needs_review`,`confirmed`,`rejected`,`ignored`) | yes | Same family as acquisition. |
| `review_snapshot_id` | uuid | no | FK to immutable snapshot. |
| `last_action` | object | no | `{label, at, actor}`. |
| `created_at` | datetime | yes | First derivation timestamp. |

### Invariants

- Same group invariants as `AcquisitionSession`.
- Frame kind heterogeneity within one session MUST be rejected at candidate formation.

### Lifecycle

Identical transition table to `AcquisitionSession` (see above). State family
is shared per spec.md §State Families.

---

## CalibrationMaster

A reusable master frame derived from a confirmed `CalibrationSession`. Stored
as a `ProcessingArtifact` reference plus reuse policy data.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `source_session_id` | uuid | yes | FK → `CalibrationSession.id`. |
| `artifact_id` | uuid | yes | FK → `ProcessingArtifact.id`. |
| `kind` | enum(`master_dark`,`master_flat`,`master_bias`,`master_flat_dark`) | yes | Master frame kind. |
| `reuse_match_key` | string | yes | Match signature for calibration reuse policy. |
| `expires_at` | datetime | no | Optional reuse cutoff. |
| `created_at` | datetime | yes | Master creation timestamp. |

### Invariants

- A master MUST link back to exactly one `CalibrationSession`.
- The app MUST NOT generate masters itself (PixInsight boundary); it tracks
  masters produced externally.
- `reuse_match_key` MUST be deterministic from the source session metadata.

### Lifecycle

No lifecycle of its own — bound to the source session. If the source session
transitions to `rejected`, dependent `ProcessingArtifact` references MUST be
flagged `stale`.

---

## Target

A celestial target (DSO, planet, lunar feature) referenced by sessions and
projects.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. UUIDv5 from canonical designation per spec 013 R6. |
| `primary_designation` | string | yes | Catalog primary display name (e.g. `M 31`). Chosen by catalog precedence per spec 013. |
| `aliases` | string[] | yes | Other names users may have used in folders/filenames. |
| `catalog_refs` | object[] | no | Structured catalog identifiers (`{catalog_id, catalog_display, designation}`). |
| `created_at` | datetime | yes | Initial registration timestamp. |

### Invariants

- `primary_designation` MUST be unique per library.
- Aliases MUST be matched case-insensitively but stored as-entered.
- Targets MUST NOT be deleted while a `Project` or `AcquisitionSession` references them.

### Lifecycle

None — `Target` is a reference entity.

---

## Project

A user-facing organisational envelope grouping sessions for shared
processing intent.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `name` | string | yes | User-supplied label. |
| `target_id` | uuid | yes | FK → `Target.id`. |
| `session_ids` | uuid[] | yes | FK list → `AcquisitionSession.id`. |
| `state` | enum(`setup_incomplete`,`ready`,`prepared`,`processing`,`completed`,`archived`,`blocked`) | yes | See research.md §2.1. |
| `last_action` | object | no | `{label, at, actor}` projection of audit. |
| `block_reason` | string | no | Required when `state == blocked`. |
| `created_at` | datetime | yes | First creation timestamp. |

### Invariants

- A project in `ready` or later MUST have at least one linked `AcquisitionSession`.
- `state == blocked` MUST carry `block_reason`.
- `state` transitions are gated by the table below; refused transitions MUST audit-log without mutation.
- `processing → ready` is explicitly disallowed (research.md §2.1).

### Lifecycle

Verbatim from research.md §2.1 `PROJECT_TRANSITIONS`.

| From | To | Trigger | Side effects |
|---|---|---|---|
| `setup_incomplete` | `ready` | Required fields populated (target, ≥1 session) | Audit emitted; `last_action` set. |
| `setup_incomplete` | `blocked` | User flags blocker | `block_reason` required. |
| `ready` | `prepared` | Prepared source view generated | `PreparedSource` rows linked; may require `FilesystemPlan`. |
| `ready` | `processing` | User starts processing without prepared view | Audit emitted. |
| `ready` | `blocked` | User flags blocker | `block_reason` required. |
| `prepared` | `ready` | Prepared view discarded | `PreparedSource` rows flagged `retired`. |
| `prepared` | `processing` | User starts processing | Audit emitted. |
| `prepared` | `blocked` | User flags blocker | `block_reason` required. |
| `processing` | `completed` | User marks completed | Final outputs recorded. |
| `processing` | `blocked` | User flags blocker | `block_reason` required. |
| `completed` | `archived` | User archives | Excluded from default surfaces. |
| `completed` | `processing` | Re-open completed project | Audit logs "Re-opened". |
| `archived` | `processing` | Unarchive | Audit logs "Unarchived → resumed processing". |
| `archived` | `ready` | Unarchive to ready | Audit logs "Unarchived". R-Unarchive (GRILL 2026-05-22): primary unarchive path for revisiting a project without immediately resuming processing. |
| `blocked` | `setup_incomplete` | Recovery to setup | Resume context preserved. |
| `blocked` | `ready` | Recovery to ready | Resume context preserved. |
| `blocked` | `prepared` | Recovery to prepared | Resume context preserved. |
| `blocked` | `processing` | Recovery to processing | Resume context preserved. |
| `blocked` | `archived` | Escape hatch: archive a permanently-blocked project | Audit logs "Archived from blocked"; `blocked → completed` remains forbidden (GRILL spec 009). |

---

## ProcessingArtifact

An externally-produced output (stack, master, integration, manifest) tracked
by the app but not produced by it.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `project_id` | uuid | no | FK → `Project.id` (nullable for stand-alone masters). |
| `file_record_id` | uuid | yes | FK → `FileRecord.id` of the artifact on disk. |
| `kind` | enum(`master`,`integration`,`drizzle`,`manifest`,`other`) | yes | Artifact category. |
| `tool` | string | no | Producing tool (e.g. "PixInsight WBPP"). |
| `staleness` | enum(`current`,`stale`,`regenerating`) | yes | Projection-state per research.md §6. |
| `created_at` | datetime | yes | First record timestamp. |

### Invariants

- The app MUST NOT modify artifact bytes; only records and reuse policy.
- `staleness` transitions to `stale` when any linked source session, project, or `FileRecord` mutates.
- Manifest artifacts are projections, not canonical (constitution §V).

### Lifecycle

| From | To | Trigger | Side effects |
|---|---|---|---|
| `current` | `stale` | Source asset transition | Dependents recomputed. |
| `stale` | `regenerating` | User requests regeneration | If filesystem write implied, `FilesystemPlan` created. |
| `regenerating` | `current` | Regeneration succeeds | New `file_record_id` linked if regenerated artifact replaced the prior file. |
| `regenerating` | `stale` | Regeneration fails or is cancelled | Prior file remains; user must retry. |

---

## FilesystemPlan

A reviewable, auditable set of filesystem mutations awaiting approval and
execution. Canonical home is `crates/fs/planner/`.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `kind` | enum(`organize`,`prepare_source`,`cleanup`,`archive`,`regenerate_artifact`) | yes | Plan category. |
| `items` | object[] | yes | Per-item mutation records `{source, target, op, item_state}`. |
| `state` | enum(`draft`,`ready_for_review`,`approved`,`applying`,`paused`,`applied`,`partially_applied`,`failed`,`cancelled`,`discarded`) | yes | See research.md §2.2. `discarded` is the soft-delete terminal (pairs with spec 017's retry-chain semantics). `paused` is a mid-apply suspension state entered on `volume.unavailable`, `disk.full`, or `item.stale` (R-Pause-1). |
| `parent_plan_id` | uuid | no | Set when this plan is a retry of a failed plan. |
| `created_by` | enum(`user`,`system`) | yes | Plan origin. |
| `created_at` | datetime | yes | Initial draft timestamp. |
| `applied_at` | datetime | no | Set on transition to a terminal state. |

### Invariants

- `applied`, `partially_applied`, `failed`, `cancelled` are terminal — retry produces a NEW `FilesystemPlan` with `parent_plan_id` set.
- A plan in `approved` MUST refuse `applying` unless the entity transition that triggered it still requires it (no stale auto-apply).
- Item-level state MUST be preserved across terminal outcomes so partial failures are traceable (FR-004).
- A plan MUST NOT overwrite existing files silently; destructive ops MUST prefer archive/trash.

### Lifecycle

Verbatim from research.md §2.2.

| From | To | Trigger | Side effects |
|---|---|---|---|
| `draft` | `ready_for_review` | User submits | Audit emitted. |
| `draft` | `discarded` | User discards | Audit emitted; terminal. |
| `ready_for_review` | `approved` | Reviewer approves | Audit emitted. |
| `ready_for_review` | `draft` | Reviewer requests changes | Audit emitted. |
| `ready_for_review` | `discarded` | Reviewer discards | Audit emitted; terminal. |
| `approved` | `applying` | Apply begins | Per-item walk starts. |
| `approved` | `draft` | Re-open invalidates approval | Audit emitted. |
| `applying` | `applied` | All items succeed | Terminal. |
| `applying` | `partially_applied` | Mixed item outcomes | Terminal; item state preserved. |
| `applying` | `failed` | All items fail or fatal abort | Terminal; item state preserved. |
| `applying` | `cancelled` | User cancels mid-apply | Terminal; in-flight item state preserved. |
| `applying` | `paused` | Mid-apply fault: `volume.unavailable`, `disk.full`, or `item.stale` | Non-terminal; user must resume or cancel (R-Pause-1). |
| `paused` | `applying` | User resumes after resolving condition | Run continues from next pending item. |
| `paused` | `cancelled` | User cancels from paused state | Terminal. |

---

## AuditLogEntry

The durable record of lifecycle transitions, refused transitions, and
no-ops-that-the-caller-should-have-known-about. Canonical home is
`crates/audit/`.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `entity_type` | string | yes | Entity family (e.g. `project`, `plan`, `session`). |
| `entity_id` | uuid | yes | Subject entity id. |
| `from_state` | string | no | Null for creation events. |
| `to_state` | string | no | Null for refused-no-transition events. |
| `trigger` | string | yes | Action label (e.g. "Unarchived"). |
| `actor` | enum(`user`,`system`) | yes | Who initiated. |
| `outcome` | enum(`applied`,`refused`,`failed`) | yes | Result class. No-op transitions (`status: "noop"` on the contract) write no row, so `unchanged` is intentionally absent. |
| `severity` | enum(`workflow`,`diagnostic`) | yes | Default-visible vs. log-only (FR-008). |
| `request_id` | uuid | yes | Operation correlation. |
| `at` | datetime | yes | Event timestamp. |
| `payload` | object | no | Optional structured detail (errors, item counts). |

### Invariants

- Append-only. Never updated or deleted.
- Written transactionally with the entity mutation (or with no entity mutation, for refused outcomes).
- `outcome == refused` MUST have `to_state == null` and MUST NOT have mutated the entity.
- No-op transitions (caller-requested next_state equals current_state) MUST NOT write a row; the contract response is `status: "noop"` (see `contracts/lifecycle.transition.json`).
- `actor == system` is permitted ONLY on edges entering or leaving `blocked` (per GRILL spec 009 ratification). The use case MUST reject any other edge with `actor == system` with `transition.refused`; the rejection itself is audit-logged as `actor == system, outcome == refused`. See `tasks.md` T038/T044 for the enforcement rule.
- Default UI timelines (and spec 019's log panel) filter `severity = workflow`; diagnostic events stay queryable but hidden by default (FR-008).
- Ledger rows MUST omit `confidence | evidence | provenance` columns (FR-006); those live in `payload` only.

### Lifecycle

None — entries are immutable.

---

## Plan-Requirement Edge Table

Canonical server-side table mapping `(entity_type, from, to) → requires_plan`.
The use case derives the plan gate from this table on every transition
attempt; callers MUST NOT pass `requires_plan` on the contract request
(`contracts/lifecycle.transition.json` does not accept it). See research.md
§2 and tasks.md T044.

| entity_type | from | to | requires_plan | Notes |
|---|---|---|---|---|
| `project` | `ready` | `prepared` | `true` | Generating prepared source view writes to disk. |
| `project` | `prepared` | `ready` | `true` | Retiring prepared source rows may delete views (constitution §archive-over-delete). |
| `project` | `completed` | `archived` | `true` **(always — R-Archived-Plan, GRILL 2026-05-22)** | Plan always required even when no files move; plan has at least the manifest-write structural item. **NOTE: this row previously said "when archiving moves files"; that condition is REMOVED — plan is unconditionally required per spec 009 R-Archived-Plan.** |
| `project` | `blocked` | `archived` | `true` **(always — A3, GRILL 2026-05-22)** | Escape-hatch edge added in spec 009 A3. Plan always required (same as `completed → archived`). |
| `project` | `archived` | `processing` | `true` (when unarchive restores files, C7) | Plan required when (a) sources mapped to different paths OR (b) any source content needs to move to active project root. Plan NOT required when only metadata changes. |
| `project` | `archived` | `ready` | `true` (when unarchive restores files, C7) | **R-Unarchive (GRILL 2026-05-22)**: New edge. Same C7 criterion as `archived → processing`. Plan required when (a) sources mapped to different paths OR (b) any source content needs to move. Plan NOT required when only metadata (notes, lifecycle) transitions. Actor: user only. Audit event `project.unarchived` emitted. |
| `prepared_source` | `*` | `retired` | `true` | Removes/cleans up generated files (spec 026). |
| `processing_artifact` | `*` | `regenerating` | `true` (when materialization implies a write) | Generating regeneration outputs to disk requires a plan. |
| `file_record` | `*` | `protected` | `false` | Pin-only mutation; no filesystem effect. |
| `inventory_session` | `*` | `*` | `false` | Pure metadata transitions. |
| `calibration_session` | `*` | `*` | `false` | Pure metadata transitions. |
| `data_source` | `*` | `*` | `false` | Connection/config transitions only. |
| `plan` | `*` | `*` | `false` | Plan lifecycle itself is the plan; no nested plan needed. |
| `projection` | `*` | `*` | `false` | Read-side staleness flag; regeneration is owned by the source entity edge. |

Edges not listed default to `requires_plan = false`. The full table is
authored in `crates/domain/core/src/lifecycle/plan_requirement.rs` (canonical
home; tasks.md T044 wires it).

## Action-Bound Review

Canonical server-side table mapping `(entity_type, from, to) →
action_critical_fields[]`. The transition use case enforces FR-009/FR-010
by reading `ProvenancedValue.origin` for each listed field via
`LifecycleRepository::field_origins` and refusing the edge with
`provenance.unreviewed` (populating `error.details.blockingFields`) when
any required field is not yet `reviewed`. Review state is derived
field-level from provenance — it is NOT a per-entity column.

| entity_type | from | to | action_critical_fields | Notes |
|---|---|---|---|---|
| `acquisition_session` | `candidate` | `confirmed` | `observer_location` | Confirming a session requires the observer location to have been reviewed (FR-009/FR-010). |
| `acquisition_session` | `needs_review` | `confirmed` | `observer_location` | Same gate applies when promoting from `needs_review` to `confirmed`. |

The auto-transition `candidate → needs_review` described in §AcquisitionSession
invariants (when extraction fails or yields null) is a **pipeline-driven
auto-transition**, NOT a user action this gate refuses. It is therefore not a
cell in this table; the gate fires on the user-initiated confirmation edges.
(Clarified 2026-05-23 — narrow clarify pass over §Action-Bound Review.)

Further cells require additional fields to be promoted to `ProvenancedValue<T>`
first. Today only `AcquisitionSession.observer_location` is wrapped; promoting
other extracted fields (e.g. `FileRecord` exposure metadata, `AcquisitionSession.target_id`,
`Project.target_id`) is out of scope for spec 002 and would be a separate spec
amendment or a new "Provenance Wrapper Coverage" spec.

The canonical table lives in
`crates/domain/core/src/lifecycle/action_review_requirement.rs`. The
contract dispatcher tags acquisition-session requests as the
`inventory_session` variant; the Rust table mirrors the cell under that
alias so the gate fires on either path. The refusal envelope shape is
defined by `contracts/lifecycle.transition.json` (`provenance.unreviewed`,
`details.blockingFields`) and exercised by `tests/transition_apply.rs`.

## ProvenanceHistoryArchive

Append-only archive for `ProvenancedValue.history` entries that age out of
the inline retention window. Decision: keep most recent N entries per
`(asset_id, field_path, origin)` inline on `ProvenancedValue`; spill older
entries here (GRILL 2026-05-21).

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | uuid | yes | Stable identifier. |
| `asset_type` | string | yes | Entity family (matches `provenance.read` `asset_type`). |
| `asset_id` | uuid | yes | Owning entity. |
| `field_path` | string | yes | Dotted field path on the asset. |
| `origin` | enum(`observed`,`inferred`,`reviewed`,`generated`,`planned`,`applied`) | yes | Tag of the archived entry. |
| `value` | json | yes | Prior value at the time of capture. |
| `captured_at` | datetime | yes | Original capture timestamp. |
| `source_id` | string | no | Opaque reference (file id, plan id, reviewer id). |
| `replaced_by` | string | no | Pointer to the entry that superseded this one. |
| `archived_at` | datetime | yes | When this entry left the inline window. |

### Invariants

- Append-only. Never updated or deleted.
- The `provenance.read` contract returns inline entries plus a `history_truncated: true` flag when archive rows exist for a `(asset_id, field_path)` pair. The archive table is the destination for follow-on `provenance.history.list` reads (deferred to v1.x; not part of this spec's contract surface).
- Retention budget per `(asset_id, field_path, origin)` inline window is implementation-tunable; tasks.md T010 names the SQLite migration that creates this table.

## ProvenancedValue

Wrapper that carries observed/inferred/reviewed (and downstream
generated/planned/applied) history for any field on a Data Asset. Defined in
research.md §4. Read access for UI/agents goes through the
`provenance.read` contract (`contracts/provenance.read.json`).

| Field | Type | Required | Description |
|---|---|---|---|
| `current` | T | yes | The effective value (priority `reviewed > inferred > observed`). |
| `origin` | enum(`observed`,`inferred`,`reviewed`,`generated`,`planned`,`applied`) | yes | Which tag won. |
| `history` | object[] | yes | Append-only entries `{value, tag, at, actor, source_ref}`. |

### Invariants

- `history` is append-only. User corrections produce a new `reviewed` entry without erasing prior `inferred` or `observed` entries (constitution §archive-over-delete).
- A single field MAY carry simultaneous `observed`, `inferred`, and `reviewed` entries; `current` reflects priority.
- `generated` entries are recomputed on any source change; they MUST NOT be authored by the user.
- `planned` entries are cleared when the originating `FilesystemPlan` resolves; on `applied`, they become `applied` entries.
- Action-bound review (FR-009/FR-010) inspects `origin` per field; missing `reviewed` for an action-critical field MUST block the action.

### Lifecycle

None — the wrapper itself has no state; its `history` records the lifecycle
of the underlying value.

---

## ObserverLocation

The observer's geographic location and timezone at acquisition time.
Carried on `AcquisitionSession.observer_location` as a
`ProvenancedValue<ObserverLocation>` (R-Obs).

| Field | Type | Required | Description |
|---|---|---|---|
| `tz` | string (IANA timezone) | yes | e.g. `"Europe/Amsterdam"`. |
| `lat` | float (degrees, −90..90) | no | Latitude. Extracted from FITS `OBSGEO-B` or `SITELAT`. |
| `lon` | float (degrees, −180..180) | no | Longitude. Extracted from FITS `OBSGEO-L` or `SITELONG`. |

### Provenance

Auto-extracted from FITS keywords during metadata parsing. If both
`OBSGEO-B`/`OBSGEO-L` and `SITELAT`/`SITELONG` are present, prefer
`OBSGEO-B`/`OBSGEO-L` (IAU standard). If timezone cannot be derived from
coordinates, the value is partial (lat/lon present, tz absent).

`ProvenancedValue.origin` is:
- `observed` when extracted directly from FITS keywords.
- `inferred` when derived from coordinates using a timezone lookup.
- `reviewed` when the user has confirmed or corrected the value.

### Notes

`observer_location` is NOT collected at first-run setup (spec 003). It is
resolved at session-formation time, per-import. It is NOT a global settings
key in spec 018 (R-Obs; GRILL_DECISIONS amendment 2026-05-22). It is used
by downstream features including spec 007 (calibration night matching) and
spec 023 (`captured_on` date derivation).
