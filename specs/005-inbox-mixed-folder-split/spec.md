# Feature Specification: Inbox Mixed-Folder Split

> **⚠ SUPERSEDED (2026-06-23) by [Spec 041 — Inbox Plan Surface](../041-inbox-plan-surface/spec.md).**
> The "detect mixed folder → warn → split into separate Inbox folders → only then move into Inventory"
> flow specified here was never implemented as written (0/51 tasks). Spec 041 is its reassigned home:
> 041's **single-type sub-items at ingest** model (mixed folders split into per-type items at ingest,
> field-agnostic reclassify, missing-mandatory gate, source-group provenance) replaces this approach.
> The single-type model is 041 iteration-2. Its source-group schema shipped:
> `inbox_source_groups` (`crates/persistence/core/migrations/0001_initial_schema.sql:1151`).
> The mixed-vs-single detection intent here remains the conceptual origin; the
> implementation lives in 041.

> **UI Revised**: The UI design in this spec has been revised by
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md).
> When implementing, follow spec 030 for layout, navigation, and component patterns.

**Feature Branch**: `005-inbox-mixed-folder-split`  
**Created**: 2026-05-09  
**Updated**: 2026-06-23  
**Status**: Superseded by spec 041 (was: Draft, mockup-only; 0/51 tasks — never implemented as written)  
**Input**: User description: "Specify how mixed folders are detected in Inbox, warned about, split into separate Inbox folders, and only then moved into Inventory."

## Implementation Status

A non-binding mockup exists in `apps/desktop/` to validate the interaction
model. It is **not** the canonical behavior and MUST NOT be treated as
authoritative once Rust/contract work begins.

Mockup-only evidence (May 2026):

- `apps/desktop/src/features/inbox/InboxPage.tsx`: drawer with file-level
  breakdown, contextual CTA toggling between "Generate split plan" (mixed)
  and "Confirm to Inventory" (single-type). Existing open plans for the same
  Inbox item are short-circuited via `useMemo` filtering of `usePlans()`.
- `apps/desktop/src/data/store.ts` (`createPlanFromInbox(item)`): builds
  per-file plan items, capped to `item.files` for sample folders and fully
  expanded otherwise; classifies items from `mixedBreakdown` or
  `sampleFiles`; sets `itemsTotal: items.length`; `simulateApply` advances
  the plan state machine.
- `apps/desktop/src/data/mock.ts`: `InboxItem` shape with `mixedBreakdown`,
  `sampleFiles`, `destinationPattern`.

The mockup does **not** read FITS headers, does **not** persist plans, does
**not** call a classifier service, and uses hand-authored seed data for both
single-type and mixed cases. The spec below defines the durable behavior the
Rust core and contracts MUST honor; the mockup will be retired or migrated
once those exist.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Detect Mixed vs Single-Type Folders (Priority: P1)

As a user scanning Inbox, I want each candidate folder classified as either
"single frame type" or "mixed", with the classification grounded in FITS
`IMAGETYP` metadata, so that mixed data cannot be silently confirmed into
Inventory and single-type folders can be confirmed without a split step.

**Why this priority**: Classification is the gate; every downstream action
(confirm, split, plan apply) branches on this single decision. Without it the
inbox is unsafe and the user cannot trust frame-type promises made to
calibration matching (spec 001) or to project lifecycle (spec 002).

**Independent Test**: Point the scanner at two fixture folders — one pure
lights, one mixed lights + darks — and confirm the Inbox row for each shows
the correct classification with at least one `IMAGETYP`-derived evidence
record per detected type.

**Acceptance Scenarios**:

1. **Given** a folder where every FITS file has a readable `IMAGETYP` header
   normalizing to `Light`, **When** the scan completes, **Then** the Inbox
   row is classified `single_type Light` and the primary CTA is "Confirm to
   Inventory". (Ref: R-IMAGETYP, A5)
2. **Given** a folder where FITS headers include both `IMAGETYP` values
   normalizing to `Light` and `Dark`, **When** the scan completes, **Then**
   the Inbox row is classified `mixed`, lists the detected types with
   per-type counts, and the primary CTA is "Generate split plan".
3. **Given** a folder where some files have no readable `IMAGETYP` header,
   **When** the scan completes, **Then** those files are marked with
   per-file unclassified markers and do not silently inherit the majority
   classification. The folder may still be `single_type` if all remaining
   files agree. (Ref: R-FileMarker)

---

### User Story 2 - Surface File-Level Breakdown (Priority: P2)

As a user reviewing a flagged folder, I want a per-type breakdown with
representative sample files and the `IMAGETYP` value that drove each
classification, so that I can verify the system's judgement before triggering
a filesystem plan.

**Why this priority**: The split CTA is destructive in intent (it eventually
moves files). Users must be able to inspect why a folder is "mixed" without
opening files in a third-party tool.

**Independent Test**: Open the detail drawer for a mixed Inbox item and
confirm the visible breakdown lists each detected frame type, the count of
files per type, at least one sample filename per type, and the destination
pattern that would be produced by a split plan.

**Acceptance Scenarios**:

1. **Given** a mixed Inbox item with 50 lights and 12 darks, **When** the
   user opens the detail drawer, **Then** the drawer shows two rows
   ("LIGHT × 50", "DARK × 12") each with a sample file and a destination
   path preview rendered through the active Naming & Structure pattern
   (spec 015).
2. **Given** the detail drawer is open and some files had no readable
   `IMAGETYP`, **Then** those files appear in a "Needs review" sub-list with
   an inline "Reclassify…" picker per file. The drawer supports multiselect
   (Shift+Click, Ctrl+Click, Select All) and a "Set type for selected" bulk
   action. (Ref: R-Unclass-1, R-Unclass-2)

---

### User Story 3 - Generate Split Plan From Mixed Folder (Priority: P3)

As a user, I want the "Generate split plan" CTA to produce a reviewable
filesystem plan — one plan item per file, grouped by frame type, with
destination paths driven by the Naming & Structure token pattern and targeting
Inventory directly — so that the actual move into Inventory passes through the
standard plan-apply pipeline (specs 017 and 025) with audit records.

**Why this priority**: This is the bridge from classification to the
existing reviewable-mutation pipeline. It MUST NOT introduce a parallel
mutation path.

**Independent Test**: From a mixed Inbox item, trigger "Generate split
plan", confirm the plan appears in the Plans list in state
`ready_for_review`, contains one item per scanned file, and that the
destination paths match the active Naming & Structure pattern when
expanded against each file's extracted metadata, pointing directly at
Inventory paths (not sibling staging folders). (Ref: R-Split-1)

**Acceptance Scenarios**:

1. **Given** a mixed Inbox item with 62 files, **When** the user triggers
   "Generate split plan", **Then** a new Plan is created with 62 plan items
   grouped by frame type, state `ready_for_review`, destination paths
   pointing directly to Inventory, and a back-reference to the originating
   Inbox item.
2. **Given** the active Naming & Structure pattern is unset or invalid,
   **When** the user triggers "Generate split plan", **Then** the operation
   is rejected with error code `pattern.unset` and no plan is created.
3. **Given** the plan reaches `applied`, **When** the user re-opens the
   Inbox, **Then** the originating Inbox item is marked resolved and its
   files no longer appear as candidates.

---

### User Story 4 - Dedupe Open Plan For Same Inbox Item (Priority: P4)

As a user who clicked "Generate split plan" earlier, I want returning to
the same Inbox item to short-circuit to the existing open plan instead of
creating a duplicate, so that I never have two parallel reviewable plans
mutating the same files.

**Why this priority**: Two concurrent plans on one Inbox item creates a
filesystem race and breaks the "one open plan per Inbox item" invariant
that the rest of the workflow relies on.

**Independent Test**: Generate a split plan for a mixed Inbox item, leave
it in `ready_for_review`, return to the Inbox item, and confirm the CTA is
"Open existing plan" rather than "Generate split plan" and that activating
it routes to the existing plan.

**Acceptance Scenarios**:

1. **Given** an Inbox item already has a Plan in state {`draft`,
   `ready_for_review`, `approved`, `applying`, `paused`}, **When** the user
   opens the detail drawer, **Then** the primary CTA is "Open existing plan"
   and references the existing `plan_id`. (Ref: E1)
2. **Given** an Inbox item's prior Plan reached state `applied`,
   `discarded`, or `failed`, **When** the user opens the detail drawer,
   **Then** a new "Generate split plan" CTA is available.
3. **Given** a user attempts the `inbox.confirm` operation while a plan is
   already open, **When** the request is processed, **Then** the operation
   is rejected with `inbox.has.open.plan` and includes the existing
   `plan_id`.

### Edge Cases

- Folder contains files whose `IMAGETYP` is unreadable or the FITS header is
  malformed or truncated. These files MUST receive a per-file unclassified
  marker and MUST NOT be silently dropped. (Ref: R-FileMarker)
- Folder mixes FITS and XISF, both with frame-type metadata. The classifier
  MUST treat them uniformly via `crates/metadata/core` regardless of
  container.
- Folder contains video files (`.ser`, `.avi`, `.mp4`, `.mov`). Video files
  are detected at scan time and routed to a separate `inbox.video.*` lane
  handled by `crates/metadata/video/`. They do NOT enter the FITS classifier
  and do NOT affect folder classification. (Ref: R-Video-1)
- All files share the same `IMAGETYP` but different `FILTER` values (e.g.,
  LRGB lights in one folder). Multi-filter folders with uniform
  `IMAGETYP=Light` are classified `single_type Light`. The `{filter}` token
  in the resolver routes files to per-filter subdirectories at
  plan-generation time. (Ref: A6)
- Folder is on a read-only mount or external drive that is currently
  detached.
- A single file fails to read mid-plan-generation.
- User edits the Naming & Structure pattern (spec 015) between
  classification and plan generation: the plan MUST be built against the
  pattern resolved at generation time, recorded with the plan.
- The folder's content changes between classification and confirm: the
  `contentSignature` in `inbox.classify` response MUST be re-verified at
  confirm time; drift returns `classification.stale`. (Ref: A8)

### Domain Questions To Resolve

All domain questions from the prior draft are resolved:

- Classification model: deterministic IMAGETYP-only (no confidence scores,
  no filename heuristics). (Ref: R-IMAGETYP, A5)
- Filter mismatches: NOT a mixed condition; single-type with `{filter}`
  routing at plan time. (Ref: A6)
- Split destination: directly to Inventory paths via spec 015 resolver.
  (Ref: R-Split-1)
- Manual reclassification: per-file inline picker + multiselect bulk-assign;
  triggers classifier re-aggregation. (Ref: R-Unclass-1, R-Unclass-2)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST classify each Inbox folder as `single_type`,
  `mixed`, or `unclassified` by reading the FITS `IMAGETYP` keyword for
  every file and normalizing via the `ImageTypNormalizationTable`. No
  confidence scores; classification is deterministic. (Ref: R-IMAGETYP, A5)
- **FR-002**: Mixed folders MUST NOT be confirmable directly to Inventory.
  The `inbox.confirm` operation MUST reject mixed items with error
  `classification.ambiguous`.
- **FR-003**: Single-type folders MUST be confirmable to Inventory via a
  single CTA without going through a split flow.
- **FR-004**: Mixed folders MUST present a file-level breakdown listing each
  detected frame type, per-type file count, at least one sample filename per
  type, and the destination path preview produced by the active Naming &
  Structure pattern (spec 015).
- **FR-005**: Files whose `IMAGETYP` is absent or unrecognized MUST receive a
  per-file unclassified marker (`InboxClassificationEvidence.unclassified =
  true`) and MUST NOT be auto-assigned to any detected frame type. A folder
  with 1000 Light files and 2 unclassified files is still `single_type
  Light`, not `mixed`. (Ref: R-FileMarker, R-IMAGETYP)
- **FR-006**: "Generate split plan" MUST produce a single Plan with one plan
  item per scanned file, grouped by frame type, in state `ready_for_review`,
  with destination paths targeting Inventory directly (not sibling staging
  folders) and a back-reference to the originating Inbox item. (Ref:
  R-Split-1)
- **FR-007**: Plan generation MUST resolve destination paths through the
  active Naming & Structure token pattern (spec 015) at generation time and
  record the resolved pattern on the plan.
- **FR-008**: An Inbox item MUST have at most one open Plan at a time, where
  "open" means any state in {`draft`, `ready_for_review`, `approved`,
  `applying`, `paused`}. Attempting to create a second open Plan MUST fail
  with `inbox.has.open.plan` and surface the existing `plan_id`. (Ref: E1)
- **FR-009**: Plan apply MUST flow through the standard reviewable-mutation
  pipeline (specs 017 and 025); this feature MUST NOT introduce a parallel
  mutation path or bypass audit records.
- **FR-010**: On successful plan apply, the originating Inbox item MUST be
  marked resolved and its files MUST no longer surface as Inbox candidates.
- **FR-011**: If the active Naming & Structure pattern is unset, invalid, or
  fails to resolve required tokens for any file, plan generation MUST fail
  with `pattern.unset` and no plan MUST be created.
- **FR-012**: Classification MUST be re-runnable per Inbox item without
  rescanning the entire Inbox root (`force_rescan` flag on
  `inbox.classify`).
- **FR-013**: The scanner walks the source root recursively. Each leaf folder
  containing FITS files becomes its own Inbox item. Folders containing only
  subfolders (no direct FITS files) are not Inbox items themselves. (Ref:
  R-Granularity-1)
- **FR-014**: Video files (`.ser`, `.avi`, `.mp4`, `.mov`) detected at scan
  time are routed to a separate `inbox.video.*` lane and MUST NOT affect FITS
  folder classification. (Ref: R-Video-1)
- **FR-015**: The `inbox.classify` response MUST include a `contentSignature`
  field. The `inbox.confirm` request MUST supply the `contentSignature`
  returned by the most recent classify call. If the folder has changed since
  classification, the operation returns `classification.stale`. (Ref: A8,
  R-Sig-1)
- **FR-016**: `InboxConfirmUseCase` MUST enumerate live files from persisted
  `InboxClassificationEvidence.relativeFilePath` rows (NOT regenerate from
  `fileCount`). Plan items carry actual source/destination paths. (Ref: A9)
- **FR-017**: Inbox confirm screen shows a destination choice toggle (Archive
  / OS trash) when the resulting plan will have destructive items. The chosen
  `destructiveDestination` is recorded on the plan. (Ref: R-DestChoice)
- **FR-018**: Users may manually reclassify unclassified files via an inline
  picker or multiselect bulk-assign. Reclassification writes
  `manualOverride` to `InboxClassificationEvidence` and triggers classifier
  re-aggregation. Once all files are either classified or overridden, the
  item transitions to `single_type` or `mixed`. (Ref: R-Unclass-1,
  R-Unclass-2)

### Key Entities

- **Inbox Item**: A candidate leaf folder under an Inbox root, with FITS
  files, classification result, and at most one open Plan.
- **Inbox Classification**: A deterministic result describing whether the
  item is `single_type`, `mixed`, or `unclassified`, with evidence records
  and content signature.
- **Inbox Breakdown Entry**: One row per detected frame type: type, count,
  destination preview, sample files.
- **Plan (cross-spec reference)**: Reviewable filesystem plan as defined by
  spec 017; this feature is a producer.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of mixed fixture folders in the test corpus are
  classified `mixed` and blocked from direct Inventory confirmation.
- **SC-002**: 100% of single-type fixture folders are confirmable in one
  click without traversing the split flow.
- **SC-003**: Re-opening an Inbox item with an open plan never creates a
  duplicate plan (verified by contract test on `inbox.confirm`).
- **SC-004**: Generating a split plan for a 500-file mixed folder completes
  in under 5 seconds on the reference dev workstation (excludes initial
  metadata scan time).
- **SC-005**: Users can resolve a representative mixed folder end-to-end
  (classify → review → generate → apply) in under 2 minutes.

## Assumptions

- The Naming & Structure token pattern (spec 015) is the single source of
  truth for destination paths. This feature consumes it; it does not
  redefine destination logic.
- The reviewable-mutation pipeline (specs 017 and 025) owns plan state
  transitions and audit records. This feature owns only plan creation.
- Frame-type classification is deterministic via FITS/XISF `IMAGETYP` only.
  No filename heuristics; no confidence scores. (Ref: R-IMAGETYP, A5)
- Inbox items represent leaf folders, not individual files. A folder with
  all lights except 2 unclassified files is `single_type Light` with 2
  per-file markers, not `mixed`. (Ref: R-FileMarker, R-IMAGETYP)
- Scanner walks recursively; each FITS-bearing leaf folder is an independent
  Inbox item. (Ref: R-Granularity-1)

## Out of Scope

- Calibration matching (spec 001 owns this).
- Project lifecycle and promotion to projects (spec 002 owns this).
- Image processing (PixInsight/WBPP boundary per constitution).
- Automatic split application without user review.
- Cross-folder deduplication or merging of split outputs.
- Editing the Naming & Structure token pattern from inside the Inbox flow
  (spec 015 owns the pattern editor).
- Video file classification details (future spec for planetary/lunar
  workflows owns the `inbox.video.*` lane).
- User-extended IMAGETYP normalization mappings (deferred to v1.x; spec 018
  follow-up). (Ref: R-IMAGETYP-Norm)
