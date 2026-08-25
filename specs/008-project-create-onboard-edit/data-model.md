# Data Model: Project Create, Onboard, And Edit

**Spec**: 008-project-create-onboard-edit | **Date**: 2026-05-09

This data model is shared with spec 009 (lifecycle), spec 010 (guided first
project), and spec 011 (tool launch). 008 owns the **identity, source link,
channel, and notes** facets; 009 owns `lifecycle`, `blockedReason`, and
`lastAction`; the storage shape is unified.

## Project

```
Project {
  id:                Uuid                       // stable identifier
  name:              String                     // user-facing label, ≤ 120 chars, unique within library
  tool:              ProcessingTool             // selected by user at create (R3)
  lifecycle:         ProjectLifecycle           // owned by spec 009; new projects start "setup_incomplete"
  path:              String                     // relative-to-library path of the project folder
  sources:           ProjectSource[]            // inventory links, see below
  calibrationSets:   CalibrationSetRef[]        // owned by spec 007
  channels:          ProjectChannel[]           // inferred + manual, see R4
  plans:             ProjectPlanRef[]           // owned by spec 025
  manifests:         ProjectManifest[]          // owned by spec 024
  framings:          Framing[]                  // co-registerable integration units, see below (Q27)
  isMosaic:          Boolean                    // mosaic-mode flag, default false (Q27)
  notes?:            String                     // free-text, ≤ 4 KB
  lastAction?:       LastAction                 // owned by spec 009
  blockedReason?:    BlockedReason              // owned by spec 009
  createdAt:         Timestamp
  updatedAt:         Timestamp
}
```

`isMosaic` defaults to `false` (backward-compatible with existing rows). When
`true`, the project may hold multiple framings that all inherit the project's
declared target, and per-frame OBJECT/coordinate resolution is suppressed
(Q27, FR-017). There is no panel entity — panels are the framings of a mosaic
project.

```
ProcessingTool = "PixInsight" | "Siril" | "Planetary Suite"
```

The `ProcessingTool` enum is the canonical list; new tools require a
research decision and a versioned contract bump. The mockup currently uses
the same three values at `apps/desktop/src/data/mock.ts:323`.

## ProjectSource

```
ProjectSource {
  inventoryId:  Uuid           // hard reference to the Inventory row
  name:         String         // snapshot at link time
  frames:       u32            // snapshot at link time
  filter:       String         // snapshot at link time, drives channel inference
  exposure:     String         // snapshot at link time (display string, e.g. "120s")
  linkedAt:     Timestamp
}
```

Snapshot fields (`name`, `frames`, `filter`, `exposure`) are denormalized
from the Inventory row at link time. The Inventory row remains the
authoritative source; if the row mutates (rename, additional frames
appended), the project drawer reads the live values via join. The snapshot
is retained for two reasons:

1. Fast list-render without an Inventory join.
2. Audit truth: the user agreed to *these* numbers at link time. Later
   divergence is surfaced as a `prepared_source_stale` blocked reason
   (spec 009 R3).

## ProjectChannel

```
ProjectChannel {
  label:   String              // e.g. "Ha", "OIII", "L"
  source:  "inferred" | "manual"
  addedAt: Timestamp
}
```

Inference rule (R4): the inferred channel set is the deduplicated,
sorted-ascending list of `ProjectSource.filter` values across linked
sources. Manual additions persist regardless of source coverage. Manual
removals of inferred channels are recorded as user-intent and not
auto-restored even when a matching source is added later — except when
the user explicitly re-runs "Re-infer channels" from the edit pane.

## Framing (Q27)

A **framing** is the co-registerable integration unit within a project — the
light sessions sharing target + optic-train + pointing + rotation within a
tunable tolerance, spanning all filters and nights of one pointing. It sits
between the project and its sessions: `project → framing → session → frames`.

```
Framing {
  id:           Uuid
  projectId:    Uuid                 // a framing belongs to exactly one project
  targetId:     Uuid | null          // the framing's target; == the project's declared target for mosaic panels
  opticTrainKey: String              // optic-train identity (Q12/Q17 grouping key)
  pointing:     { ra: f64, dec: f64 } // representative FOV-relative pointing
  rotation:     f32                   // representative rotation, degrees
  tolerance:    { pointing: f64, rotation: f32 } // snapshot of the tunable tolerance the clustering used
  sessionIds:   Uuid[]               // member light sessions (spec 006 sessions)
  clustering:   "suggested" | "user_adjusted"     // provenance — never authoritative
  sourceViewId?: Uuid                // Q20 per-framing source view (reproducible projection)
  manifest?:    FramingManifestRef   // Q10 per-framing manifest (rendered on demand)
}
```

```
FramingManifestRef { id: Uuid, path: String }   // Q10 projection, not a stored dump
```

- `clustering` starts as `"suggested"` (the app's tolerance-based grouping) and
  flips to `"user_adjusted"` on any user merge/split/reassign (FR-015). The app
  never treats its clustering as authoritative.
- `tolerance` is a snapshot of the parameters used, documenting *why* these
  sessions grouped; it is a tunable parameter, never an exact-match key (FR-014).
- Membership is derived from physical acquisition attributes only — never from
  OBJECT/panel-name strings (FR-018).
- A framing is a lights-only unit; calibration sessions are matched by the
  Q18/Q26 rules and are not framing members.
- **Durable clustering key (session-level geometry)**: the clustering inputs
  MUST be durably persisted on the acquisition session — **nullable**
  `pointing (ra/dec)`, `rotation`, and `optic_train_key` columns populated at
  confirm time (today pointing/rotation exist only on non-durable inbox staging;
  `acquisition_session` has no geometry/optic-train columns). **Legacy rows keep
  NULL geometry** (consistent with the merged Q16 null semantics — missing is
  missing, never fabricated); NULL-geometry sessions are **excluded from
  clustering** until backfilled via rescan (the Q28 onboarding path) and surface
  as unassigned for manual reassign.
- **Clustering trigger + protection**: clustering runs **incrementally at
  confirm** (the attribution pass) and as an **explicit bulk derive**
  (onboarding/rescan, Q28). Re-derivation MUST NEVER modify a framing with
  `clustering == "user_adjusted"` — it may only surface **new** suggestions
  (this is what makes US5 AS2's "the adjustment persists" hold).

## Cross-Spec References

```
CalibrationSetRef { id: Uuid, label: String }      // owned by spec 007
ProjectPlanRef    { id: Uuid, title: String, state: PlanState }   // owned by spec 025; PlanState defined by spec 002 (includes paused, discarded per spec 017+025 amendment — E6)
ProjectManifest   { id, reason, timestamp, path, body? }          // owned by spec 024
LastAction        { label, when }                  // owned by spec 009
BlockedReason     { kind, ...payload }             // owned by spec 009
ProjectLifecycle  = "setup_incomplete"             // owned by spec 009
                  | "ready"
                  | "prepared"
                  | "processing"
                  | "completed"
                  | "archived"
                  | "blocked"
```

The `ProjectLifecycle` enum is owned by spec 009; this spec consumes it
without redefinition. New project creation deterministically yields
`lifecycle = "setup_incomplete"`; the first transition to `ready` is a
spec-009 operation that runs after the create plan is applied.

## Invariants

- `name` is non-empty, ≤ 120 chars, unique within library scope.
- `path` is unique within library scope; two projects MUST NOT share a
  folder.
- `tool` is non-null on all projects after create (R-Tool-Req). `tool` is
  immutable when `lifecycle in {"prepared", "processing", "completed",
  "blocked"}` (R-Tool-Lock, GRILL 2026-05-22). The `blocked` state is
  included because it may be entered from any of the locked states.
  Changing tool requires lifecycle to be `setup_incomplete` or `ready`.
  Recovery from a locked-tool project uses manual re-creation via
  `project.create` (no `project.duplicate` in v1 — R-NoDup).
- `setup_incomplete` is ONLY for projects with missing/unconfirmed sources.
  A project without a tool MUST NOT be created (tool is required at create).
- `sources[]` MUST contain unique `inventoryId` values; duplicate add
  attempts return `source.already.linked`. Each linked source MUST reference
  a confirmed Inventory session (`state == "confirmed"` — R-Inventory-Confirmed).
- `channels[]` MUST contain unique `label` values.
- `lifecycle == "archived"` ⇒ all edit operations refuse with
  `lifecycle.read_only` (research R7).
- `notes` is plain text, ≤ 4 KB; markdown is not rendered in v1.
- A `Framing` belongs to exactly one project; a light session belongs to at most
  one framing.
- `isMosaic == false` ⇒ all framings share the single project target
  (`Framing.targetId == Project.canonical_target_id`) and the project has **one
  active framing** — extra derived framings surface for user reconciliation
  (reassign, retune tolerance, or enable mosaic mode), never as silent parallel
  integration units (FR-016).
- Re-derivation never modifies a `user_adjusted` framing; it only surfaces new
  suggestions.
- Sessions with NULL geometry (legacy/pre-Q12 rows) are excluded from clustering
  until backfilled via rescan (Q28); they surface as unassigned.
- `isMosaic == true` ⇒ every framing inherits the project's declared target, and
  per-frame OBJECT/coordinate resolution is suppressed. There is no panel entity.
- Framing membership is derived from physical clustering (target + optic-train +
  pointing + rotation within tolerance), never from OBJECT/panel-name strings.
- Framing tolerance is a tunable parameter, not an exact-match key.

## Storage Notes

- Persisted in the SQLite store managed by the `crates/persistence/` crates.
- `name`, `path`, and `tool` are indexed for filter and uniqueness.
- `sources` is a child table keyed by `(project_id, inventory_id)`.
- `channels` is a child table keyed by `(project_id, label)`.
- `manifests`, `plans`, and `calibrationSets` are owned by their
  respective specs; this spec only carries foreign keys.
- `framing` is a child table keyed by `id`, FK `project_id`, with a
  `framing_session` join keyed by `(framing_id, session_id)`. `Project.is_mosaic`
  is a new column defaulting to `false` (backward-compatible migration).
  Session-level nullable geometry columns (`pointing`, `rotation`,
  `optic_train_key`) are added to the acquisition-session table (see Durable
  clustering key above).
- **Migration numbering**: no version is pinned in spec text — claim the next
  free version at implementation merge against origin/main, and the reviewer
  MUST dup-check `crates/persistence/core/migrations/` for version collisions
  (duplicate-0046 precedent, PR #317). Touch the persistence crate's `lib.rs`
  to force the sqlx macro to re-embed new migration files.

## Derived Views

### ProjectCreateResult (returned by `project.create`)

```
ProjectCreateResult {
  project_id:    Uuid
  lifecycle:     "setup_incomplete"   // always, per invariant above
  plan_id?:      Uuid                 // FilesystemPlan id; present when folder/marker write was planned
  channels:      ProjectChannel[]     // inferred from initial_sources at create time
}
```

### ProjectUpdateResult (returned by `project.update`)

```
ProjectUpdateResult {
  project_id:     Uuid
  fields_updated: String[]            // subset of ["name", "tool", "notes"]
  audit_id:       Uuid
}
```

### ProjectSourceAddResult (returned by `project.source.add`)

```
ProjectSourceAddResult {
  project_id:    Uuid
  source_added:  ProjectSource
  channels:      ProjectChannel[]     // recomputed after link; the caller may diff against prior list
  audit_id:      Uuid
  new_lifecycle? ProjectLifecycle     // present if source.add triggered a setup_incomplete → ready auto-transition (R-Ready-Trigger)
}
```

### ProjectSourceRemoveResult (returned by `project.source.remove`)

```
ProjectSourceRemoveResult {
  projectId:       Uuid
  removedSourceId: Uuid
  auditId:         Uuid
  newLifecycle?:   ProjectLifecycle   // present if removal triggered a ready → setup_incomplete transition
}
```

Uses camelCase convention (A7 exception for new contracts).

### ChannelDrift (embedded in `project.get` response)

```
ChannelDrift {
  hasNewSources:   Boolean   // true when sources were added after last channel review
  suggestedAction: "re_infer" | "dismiss"
}
```

Present on the `project.get` response. Reset to `hasNewSources = false` by
calling `project.channels.reinfer` or `project.channels.dismiss_drift`
(R-ChannelDrift, GRILL 2026-05-22).

### IngestionAttributionCandidate (Q27 — Inbox confirm response extension)

The Inbox confirm gate's attribution pass (the first pre-ingest pass; the Q22
duplicate sweep joins the same pass when its iterate lands)
returns a ranked list of candidates per incoming light session. It is a
**suggestion surface**: it never writes a merge (FR-019/FR-020).

```
IngestionAttributionCandidate {
  kind:         "add_to_framing" | "new_framing" | "flag_optic_difference" | "new_project"
  projectId?:   Uuid            // present for the first three kinds
  framingId?:   Uuid            // present for add_to_framing
  targetId?:    Uuid            // matched target
  matchScore:   f32             // ranking key: framing-match strength (target+optic-train+pointing+rotation)
  reopen?:      Boolean         // true when projectId is a completed project (add + reopen, Q25 revoke/warn)
  opticMismatch?: Boolean       // true for flag_optic_difference
}
```

Candidates are **ranked by `matchScore`**; the user picks (recommend-then-
override). A `reopen == true` candidate targets a completed project and, on
selection, uses the spec-009 `completed → processing` reopen edge with its
raw-subs-archived warning (Q25).

**Apply-path (FR-022)** — the pick is persisted at confirm time as an
**additive field on the confirm request** (per session/item):

```
chosenAttribution? {
  kind:       "add_to_framing" | "new_framing" | "flag_optic_difference" | "new_project" | "unassigned"
  projectId?: Uuid
  framingId?: Uuid            // required for add_to_framing
}
```

Confirm writes the framing membership (and creates the framing/project when the
kind requires it) as **database metadata** — not a filesystem mutation, so no
§II plan is generated for it, and the write applies identically on
catalogue-in-place and queued-move confirms. Omitting `chosenAttribution`
leaves the session unassigned (attributable later via reassign). This is what
makes SC-008 testable end-to-end.

### Framing adjustment results (Q27)

`framing.merge`, `framing.split`, and `framing.reassign` are user-driven
adjustments. Each returns the affected framing ids and sets `clustering =
"user_adjusted"`; an audit event is emitted. `framing.list` returns the
project's `Framing[]`. These mutate framing membership only — never image files
(§III).
