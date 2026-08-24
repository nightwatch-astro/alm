# Feature Specification: Inbox Confirmation & Reviewable Plan Surface

> **AMENDED (2026-07-14) by the Q27 framing-layer iteration on
> [Spec 008 — Project Create, Onboard, And Edit](../008-project-create-onboard-edit/spec.md).**
> Q27 extends this spec's **confirm contract additively** in both directions:
> the confirm **response** gains a ranked `IngestionAttributionCandidate[]`
> (framing/project attribution suggestions) and the confirm **request** gains a
> per-item `chosenAttribution` field persisting the user's pick (spec-008
> FR-019/FR-022; shapes in spec-008 `data-model.md`). The **framing-membership
> write happens at confirm time in ALL paths** — catalogue-in-place or queued
> move — because membership is database metadata, independent of filesystem
> plan application; it MUST NOT be deferred to plan-apply completion. The
> **pending Q7 iteration** (`pending-iteration.md`), which reworks this same
> confirm/queue surface (T090 confirm-time catalogue registration, US18
> Apply-now/Add-to-plan), **must preserve that confirm-time membership write**.
> Residual Q7×Q27 shape questions — flagged, not resolved here: (a) both
> iterations extend the confirm request/response, so whichever lands second
> reconciles the contract shape; (b) whether attribution suggestions also
> surface inside the Q7 global queue UI (vs confirm-gate-only) is a Q7-side
> design call.

**Feature Branch**: `041-inbox-plan-surface`

**Created**: 2026-06-20

**Updated**: 2026-06-23

**Status**: Implemented — confirm pipeline + reviewable plan surface + apply (59/59 tasks); iteration-1 added single-type sub-items at ingest + destination model (PR #315). Supersedes spec 005; absorbed parts of specs 025/045. FR-006a/b/c (the explicit
approval gate on apply) were **added to this document 2026-08-24**, after the
59/59 count: the behaviour shipped in PR #1740 (`c0f7e10ea`) but the spec was
silent on it, so the count does not cover them. The gate is live at
`crates/app/core/src/inbox_plan.rs:207` (refuses with `plan.approval_required`),
documented at `:162-168`/`:177`/`:224`/`:334`, asserted at `:1138`, with token
verification in `crates/app/core/src/plan_apply/paths.rs:122-149` and both codes in
`crates/contracts/core/src/error_code.rs:169`/`:171`.

**Input**: User description: "Rework the inbox confirmation experience and the file-organization plan model so confirming an item produces a visible, reviewable plan; the review surface is structured (no overflowing pills, multi-level grouping) and shows per-file metadata; users can override header-derived values beyond frame type and apply overrides to a multi-selection; and whether files are moved or catalogued in place depends on where they came from (inbox vs already-organized library root)."

## Context & Motivation

Hands-on use of the Inbox surfaced a cluster of problems that, together, break two constitutional principles and make the confirmation surface hard to use:

- **Reviewable Filesystem Mutation (Principle II)** — confirming an item silently generates a plan the user cannot see. The item disappears from the queue, the only "View plan" affordance navigates away to the unrelated Archive page, and there is no in-context way to review the scheduled actions before they run.
- **Local-First File Custody (Principle I)** — the move-vs-leave-in-place decision is inconsistent (light frames generate a move plan, calibration masters are "added directly"), and there is no notion of "this folder is already organized, catalogue it in place rather than moving it."

### Observed problems (from hands-on review)

1. The review list uses status **pills that overflow outside the visible sidebar**; the list is not structured and does not follow the standard sidebar layout.
2. The list cannot be **grouped by more than one dimension** (e.g. by target, then by frame type).
3. The central panel exposes **only the frame type** for override and **hides the rest of the FITS metadata** (filter, exposure, binning, gain, temperature, object, date).
4. There is **no multi-select**: a user cannot set a value once and apply it to many files/items.
5. The **destination preview is always empty** during review.
6. Folders with more than one frame type show a **"mixed" label with no explanation** of what the mix is.
7. **Calibration masters hang on "Loading classification"** because they are a single file, not a folder.
8. The right action sidebar holds essentially **one button plus two radios** ("Archive folder" / "System Trash") whose **purpose is unclear** in context.
9. The queue footer shows a **bare "N folders"** with no per-type breakdown.
10. After confirm, **items (especially lights) vanish** with no feedback; the generated plan is effectively invisible.

This feature reworks the Inbox confirmation surface and plan model to make every filesystem mutation reviewable and visible in-context, to surface and let users correct per-file metadata, to organize the review list with multi-level grouping, and to make the move-vs-catalogue decision predictable and consistent. It extends and folds in prior work: spec 005 (mixed-folder split), spec 039 (cross-root inbox), and spec 040 (calibration master detection).

### Relationship to recent bug fixes

Several defects encountered during this review were already repaired (PR #298): masters now classify (file-not-folder), the per-item override state no longer leaks across selections, the breakdown no longer blanks after applying overrides, the empty destination cell now reads "computed on confirm", and the Calibration page no longer crashes. Those fixes restored basic function; **this spec defines the redesigned behavior** that supersedes those stopgaps (e.g. the destination is shown for real during review, not "computed on confirm").

## Clarifications

### Session 2026-06-20

- Q: For an item whose files are partly already-organized-in-place and partly not, how is move-vs-catalogue decided? → A: Per file by provenance — already-organized files are catalogued in place, the rest get move actions (one item's plan may contain both).
- Q: How is the move-vs-catalogue decision determined for a source? → A: By an explicit per-source **organization state** (organized → catalogue-in-place; unorganized → propose move plan) that is **orthogonal to content kind** (`light_frames`/`calibration`/`project`/`inbox`). It is the field the decision keys on, not `kind`.
- Q: Can the same content kind have both organized and unorganized sources? → A: Yes — e.g. an organized `light_frames` library and an unorganized `light_frames` capture dump can coexist. This is the reason `kind` alone cannot drive the decision.
- Q: How is organization state set, and what is the default for non-inbox sources? → A: The user must **explicitly choose** organized vs unorganized **when adding** each non-inbox source (no silent default — a forced choice). `inbox`-kind sources are unorganized by definition (no choice needed). The state is changeable later and affects only future confirms.
- Q: How is the organization-state choice communicated? → A: The source-add / setup wizard flow must **explain** the choice and its consequence, ideally with a small flow diagram, so the user understands organized → leave-in-place vs unorganized → propose-moves.
- Q: Which dimensions can the multi-level grouping use? → A: The full common set — **target, frame type, filter, exposure, date, and source** — freely orderable into nested levels.
- Q: Do manual metadata overrides persist across a rescan? → A: Yes — overrides **persist keyed to the file's content** and re-apply automatically across rescans; they are invalidated only when the file's content changes.
- Q: Can the user apply more than one plan at once? → A: Yes — the user can apply a single item's plan **or batch-apply all pending planned items** in one action; each action is still individually audited.

## Iterations

### Iteration 2026-06-21: Inbox destination model

**Change**: Per-type configurable destination patterns (light/flat/master-flat/bias/master-bias/dark/master-dark with sensible defaults), explicit destination-root selection (in-place default; inbox must target a root; multi-root requires user choice), full absolute-path preview, and a mandatory gate on missing path-load-bearing attributes.
**Scope**: Feature-wide (additive requirements + behavioral change to the merged move-destination computation).
**Artifacts updated**: spec.md (US8, US9, FR-025–FR-033), data-model.md, plan.md, tasks.md, research.md, quickstart.md.
**Tasks added**: T048–T060.
**Context**: Follows the merged spec 041 (apply executor now resolves root_id via registered_sources; breakdown layout stable; move-preview double-slash fixed). Found during Windows real-app E2E (T046).

### Iteration 2026-06-23: Single-type inbox sub-items + field-agnostic metadata + lifecycle drop

**Change**: Make the inbox unit of work a single-type sub-item materialized at classify (mixed folder → N homogeneous items; item↔plan 1:1), reclassify field-agnostic over a typed property registry, generalize the missing-metadata gate, add source-group provenance, resolve light targets by coordinates, extend header extraction, and drop the session review lifecycle in favour of derived, already-confirmed sessions with persisted editable metadata.
**Scope**: Pivot (changes the core inbox granularity invariant; folds in the lifecycle drop; cross-spec impact on 045/006/035).
**Artifacts updated**: spec.md, research.md, data-model.md, plan.md, contracts/operations.md, tasks.md, quickstart.md.
**Tasks added**: T061–T079. **Tasks retired**: T036, T037.

### Iteration 2026-07-14: Q27 confirm-contract extension (cross-spec delta, owned by spec 008)

**Change**: The Q27 framing layer (spec 008) additively extends the confirm
contract — response: ranked `IngestionAttributionCandidate[]`; request:
per-item `chosenAttribution` persisting the user's attribution pick at confirm
time. Membership is DB metadata written at confirm in all paths (catalogue or
queued move), independent of plan application. The pending Q7 iteration must
preserve that write when it reworks the confirm/queue surface; contract-shape
reconciliation falls to whichever of Q7/Q27 lands second (flagged in the
amendment note above).
**Scope**: Cross-spec delta (documentation only; contracts/tasks live in
spec-008 Phase F — F-Framing-5/10).
**Artifacts updated**: spec.md (amendment note + this log entry).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Reviewable, visible plan on confirm (Priority: P1)

When a user confirms an inbox item, the app produces a **reviewable plan** describing exactly which files will be moved (or catalogued) and where. The confirmed item does **not** vanish: it stays in the queue, distinctly marked as **"planned"** (e.g. greyed with a planned badge), and remains selectable for review. The scheduled filesystem actions are listed in an **in-context plan panel** (at the bottom of the central detail area — **not** by navigating to the Archive page). The user takes an **explicit Apply** step to execute them, and can **review the full plan and cancel** it before applying.

**Why this priority**: This is the core constitutional fix (Reviewable Filesystem Mutation). Without it the product's central promise — making large library changes safe and reversible — is broken. Highest-value slice.

**Independent Test**: Confirm one inbox item; verify the item stays visible as "planned", a plan with one or more actions appears in the in-context plan panel (no navigation away), the files have not moved on disk, Apply executes the actions and writes an audit record, and Cancel discards the plan leaving files untouched.

**Acceptance Scenarios**:

1. **Given** an unconfirmed inbox item, **When** the user confirms it, **Then** a plan listing the intended file actions appears in an in-context plan panel, the item is shown as "planned" (still visible and selectable, not removed), and no files have moved yet.
2. **Given** a planned item, **When** the user opens "View plan", **Then** the plan is shown in-context within the inbox surface (the user is NOT taken to the Archive page or any unrelated page).
3. **Given** a planned item, **When** the user clicks Apply, **Then** the planned filesystem actions execute, each action's outcome is recorded in the audit log, no existing file is overwritten silently, and the item leaves the queue only after the plan is applied.
4. **Given** a planned item, **When** the user cancels the plan, **Then** the plan is discarded, no files are moved, and the item returns to the unconfirmed state.
5. **Given** a planned item whose source files changed on disk since planning, **When** the user clicks Apply, **Then** the app refuses to apply silently and surfaces that the plan is stale and must be regenerated.
6. **Given** several planned items, **When** the user chooses "apply all", **Then** every pending plan is applied in one action and each action is individually recorded in the audit log.

---

### User Story 2 - Structured, groupable review list and per-file metadata detail (Priority: P1)

The inbox review list (left) presents each item in a **structured, tabular layout** that fits the sidebar — item type(s), file count, format, and a clear master indicator — using **no pills** and following the standard page/sidebar layout (pinned controls, scrolling content, nothing overflowing). The user can **group the list by one or more dimensions in a chosen order** — e.g. group by target, then by frame type, then by filter — with collapsible groups. Selecting an item shows a **restructured central detail panel** that lists the item's files with their extracted **FITS/XISF metadata** (image type, filter, exposure, binning, gain, temperature, object, observation date). When a folder holds more than one frame type, the panel makes the **composition explicit** (which types and how many of each) rather than showing a bare "mixed" label.

**Why this priority**: The review surface is currently unusable for real decisions (overflowing pills, single-level grouping, only frame type visible, opaque "mixed"). A structured, groupable, metadata-rich surface is a prerequisite for trustworthy confirmation and for the override flow (US3). Independently valuable even without US1.

**Independent Test**: Open an item with a folder of sub-frames; verify the list row shows type/count/format/master indicator with no pills and no overflow; apply a two-level grouping (e.g. target → frame type) and verify the list nests accordingly; verify the detail panel lists each file with its metadata fields populated, and that a multi-type folder shows its explicit per-type composition.

**Acceptance Scenarios**:

1. **Given** the inbox has items, **When** the user views the list, **Then** each row shows the item's type(s), file count, format, and master indicator in a structured (non-pill) layout that fits the sidebar width without clipped or overflowing content.
2. **Given** the list, **When** the user chooses to group by dimension X and then by dimension Y, **Then** the list is organized into nested, collapsible groups (X at the top level, Y within each X group), and the chosen grouping persists while navigating.
3. **Given** an item is selected, **When** the detail panel renders, **Then** it shows per-file metadata (image type, filter, exposure, binning, gain, temperature, object, date), displaying each field as unavailable when absent.
4. **Given** a folder containing more than one frame type, **When** it is selected, **Then** the detail panel shows the explicit composition (each frame type and its count), not just a "mixed" label.
5. **Given** a calibration master (a single file), **When** it is selected, **Then** the detail panel resolves and shows its metadata and master type (it does not hang on a loading state).

---

### User Story 3 - Override header values beyond type, with multi-select apply-to-all (Priority: P2)

In the detail panel the user can correct header-derived values the app inferred — frame type **and other header fields** (at minimum filter, exposure, binning; ideally the full surfaced set including gain, object, temperature) — for one or more files before confirming. The user can **select multiple files** (within an item and/or across items, including via the grouped list) and **apply an override to the entire selection at once**. The "Apply N overrides" action applies exactly the overrides for the selected scope, reports an **accurate count**, and the classification breakdown **stays visible and updates** afterwards. Overrides are reflected in the destination/plan before applying.

**Why this priority**: Real libraries have missing or wrong headers; correcting only the frame type is insufficient. Multi-select apply-to-all is a major time-saver for large folders. Depends on US2's metadata surface but delivers standalone value.

**Independent Test**: Select several files with a wrong/blank filter (spanning more than one item via the grouped list), apply a filter override to the whole selection, and verify every selected file reflects the new value, the reported count equals the selection size, the breakdown stays visible, and the resulting destinations use the corrected values.

**Acceptance Scenarios**:

1. **Given** files with a blank or wrong inferred value, **When** the user overrides frame type / filter / exposure / binning (or another surfaced header) on a file, **Then** the override is recorded for that file and reflected in its metadata and destination.
2. **Given** multiple selected files (within or across items), **When** the user applies an override to the selection, **Then** every selected file receives the override and the reported applied-count equals the number of selected files.
3. **Given** the user has applied overrides, **When** the detail re-renders, **Then** the type/breakdown stays visible and reflects the overrides (it does not blank out), and only the current scope's overrides are applied (no leakage from previously-viewed items).

---

### User Story 4 - Move vs catalogue-in-place based on a source's organization state (Priority: P2)

Whether confirming an item generates a **move plan** or **catalogues the files in place** depends on the **organization state of the source the files came from**, not on their frame type. Each source carries an explicit **organization state** — *organized* (already-sorted library; catalogue in place, no move) or *unorganized* (capture dump / inbox dropzone; propose a reviewable move plan) — set by the user when adding the source and orthogonal to its content `kind`. So an organized `light_frames` library and an unorganized `light_frames` dump can coexist, treated differently. This applies uniformly to calibration and light frames alike — removing today's inconsistency where lights get a plan but masters are added directly. The user explicitly chooses the organization state when adding any non-inbox source (inbox sources are unorganized by definition), the choice is explained in the add-source/wizard flow (ideally with a flow diagram), and it can be changed later (affecting only future confirms).

**Why this priority**: This is the Local-First File Custody fix and removes the calibration-vs-light inconsistency. Essential for users with existing organized libraries who must not have their files moved.

**Independent Test**: Confirm an item from an inbox source and verify a move plan is produced; confirm an item from an already-organized library root and verify it is catalogued in place (recorded in the database, no move plan, no file movement) — for both a light item and a calibration master.

**Acceptance Scenarios**:

1. **Given** an item from an inbox-designated source, **When** the user confirms it, **Then** a reviewable move plan is generated (US1 flow).
2. **Given** an item from an already-organized library root, **When** the user confirms it, **Then** the files are catalogued in place — recorded in the database with no move plan and no file movement.
3. **Given** a calibration master from an unorganized source, **When** confirmed, **Then** it both gets a reviewable move plan and is registered as a master (consistent with light frames getting a plan).
4. **Given** a calibration master from an organized source, **When** confirmed, **Then** it is registered as a master in place with no move plan.
5. **Given** the user is adding a non-inbox source, **When** they reach the organization-state step, **Then** they are required to explicitly choose organized vs unorganized and are shown an explanation of the consequence (no silent default).
6. **Given** an organized and an unorganized source of the same content kind, **When** items from each are confirmed, **Then** the organized source's item is catalogued in place and the unorganized source's item gets a move plan.

---

### User Story 5 - Confirm auto-splits mixed folders (Priority: P3) — [SUPERSEDED by US10]

RETIRED (superseded by US10 single-type items): with single-type sub-items materialized at ingest, a mixed folder yields N homogeneous items directly, so there is nothing left to auto-split at confirm — see US10. The original story is preserved below for history.

When the user confirms a folder containing more than one frame type, the app **automatically** separates it into per-category plan actions (each type routed to its correct destination). There is **no separate manual "Split" step** — splitting is implicit in confirm.

**Why this priority**: Removes a confusing extra step (folds spec 005). Valuable but secondary to the core plan surface and override flow.

**Independent Test**: Confirm a folder containing both light and dark frames and verify the resulting plan contains separate per-type move actions routing each frame type to its own destination, with no manual split required.

**Acceptance Scenarios**:

1. **Given** a folder with multiple frame types, **When** the user confirms it, **Then** the generated plan contains a distinct action per frame type, each with its own destination, without the user invoking a separate Split action.
2. **Given** a folder with a single frame type, **When** confirmed, **Then** a single move action is generated.

---

### User Story 6 - Richer inbox queue statistics (Priority: P3)

The inbox queue summary replaces the bare "N folders" figure with a **breakdown**: counts of folders per type, masters per type, and images per type, so the user can gauge what is waiting at a glance.

**Why this priority**: Improves situational awareness; lowest priority as it is informational and does not block any task.

**Independent Test**: With a known mix of items in the queue, verify the summary shows folder, master, and image counts broken down by type that match the actual queue contents.

**Acceptance Scenarios**:

1. **Given** a populated inbox, **When** the user views the queue summary, **Then** it shows counts broken down by type (folders per type, masters per type, images per type) consistent with the items present.

---

### User Story 7 - Understandable destructive destination choice (Priority: P3)

When a plan includes a **destructive action** (e.g. archiving or trashing rejected, duplicate, or superseded files), the user can choose the destructive destination — **Archive folder** (app-managed archive, the safe default) vs **System Trash** — from a **clearly labelled, well-placed control within the plan/review surface** (not an unexplained pair of radios in a sidebar that is being removed). The choice and its meaning are explained at the point of use, and the safe archive option is the default.

**Why this priority**: The control already exists but is unclear and orphaned by the removal of the right sidebar. Correctly placing and explaining it preserves the constitution's preference for archive/trash over permanent deletion.

**Independent Test**: Generate a plan that includes a destructive action; verify the destructive-destination control appears in the plan/review surface with clear labels, defaults to Archive, and that switching to System Trash is reflected in the plan actions before Apply.

**Acceptance Scenarios**:

1. **Given** a plan with a destructive action, **When** the user reviews it, **Then** a clearly labelled Archive-vs-System-Trash control is shown in context, defaulting to Archive.
2. **Given** the user selects System Trash, **When** the plan is applied, **Then** the destructive files are routed to the system trash (never permanently deleted without a recoverable step), recorded in the audit log.

### Edge Cases

- A folder is confirmed, then its files change on disk before the plan is applied → Apply must detect staleness and refuse rather than move the wrong/changed files (US1 scenario 5).
- A file has no readable header / unreadable metadata → the detail panel shows it as unclassified and allows manual override rather than hanging or crashing.
- A destination collision (target file already exists) at Apply time → the plan/apply must not overwrite silently; it surfaces the conflict for the user to resolve.
- An item spans files of mixed source provenance (some already organized in place, some not) → the plan is split **per file by provenance**: files already under an organized root are catalogued in place, while the rest receive move actions (so one item's plan may contain both catalogue and move actions). **(Iteration 2026-06-23: no longer reachable at the sub-item level — a source group is one leaf folder under one source, so a single-type sub-item has uniform provenance; retained for the legacy folder-level path.)**
- The active naming pattern is unset/incomplete on a fresh setup → destination preview must degrade gracefully (clear "destination unavailable until a pattern is configured" rather than a blank field).
- Multi-level grouping where some items lack a grouping dimension (e.g. no target) → such items are gathered under a clear "unknown/none" group rather than dropped.
- A multi-select override spans files across different source designations (inbox vs in-place) → the override applies to metadata uniformly, but the move-vs-catalogue decision still follows each item's source (US4).
- A continuous dimension (exposure, temperature, pointing, rotation) varies slightly across files of one acquisition run → values are bucketed/within-tolerance so near-identical frames stay in one item rather than forking into many (US10, US16).
- Files lack a mandatory grouping or path attribute (or an unreadable frame type) → they collect in a per-source-group "needs-review" bucket that blocks plan creation until resolved, then re-split into proper single-type items (US12).
- An item carries an open plan when the single-type migration runs → it is kept as a single legacy sub-item until its plan resolves/discards, and re-derivation into sub-items happens on the next classify (US10, FR-054).
- A user override changes a file's grouping value → the file re-partitions between sub-items (its old and new sub-group signatures change); this is correct churn surfaced as override-stale (US11).
- A manual rotator does not report drift (`ROTATANG` stays static; only `OBJCTROT` moves) → intra-session rotator drift is not detectable, so only the flat-group-vs-light-group `ROTATANG` deviation is determinable and is warned on (US15/US16).

### User Story 8 - Destination-root selection for moves (Priority: P2)

When a move plan is generated, the destination **root** is resolved explicitly. For a non-inbox source the default is to reorganize **in place** within the source's own root. Inbox sources are never a destination, so an inbox item **must** be moved into a chosen library root. When more than one registered root is a valid destination for the item's frame type, the user is **required to pick** the destination root during plan review; when exactly one valid root exists, it is selected automatically with no prompt.

**Why this priority**: Makes consolidation predictable and prevents inbox items from having no home; required for libraries with multiple roots of the same type.

**Independent Test**: With two light roots registered, confirm an inbox light item and verify a root-selection prompt appears and apply is blocked until a root is chosen; with a single calibration root, confirm a calibration item and verify the destination root is chosen automatically (no prompt); confirm a non-inbox unorganized item and verify it defaults to in-place.

#### Acceptance Scenarios

1. **Given** an inbox item and >1 valid destination root for its type, **When** confirming, **Then** the user must select the destination root before the plan can be applied.
2. **Given** exactly one valid destination root for the type, **When** confirming, **Then** the root is selected automatically with no prompt.
3. **Given** a non-inbox unorganized source, **When** confirming, **Then** the default destination is the source's own root (in place).

### User Story 9 - Mandatory capture of missing path attributes (Priority: P2)

A plan cannot be generated or applied while any attribute used to build a file's destination path is missing. Such a file routes through the same needs-review/unclassified gate as a missing image type, and the user must supply the value before the plan proceeds.

**Why this priority**: Prevents files landing in placeholder paths like "undated"/"nofilter"; ensures every moved file has a meaningful, complete destination.

**Independent Test**: Confirm a light frame missing its observation date and verify the plan is blocked and the file is surfaced for input; supply the date and verify the gate clears and the destination updates.

#### Acceptance Scenarios

1. **Given** a file missing a path-load-bearing attribute (e.g. a light with no date), **When** the user attempts to confirm, **Then** plan generation is blocked and the file is surfaced in the needs-review flow.
2. **Given** that file, **When** the user supplies the missing value, **Then** the gate clears and the resolved destination updates accordingly.

### User Story 10 - Single-type sub-items at ingest (Priority: P1)

As a user ingesting a leaf folder that mixes frame types (or mixes exposures, filters, or targets within a type), I want each homogeneous group to become its own inbox item so that every item maps cleanly to one destination and one reviewable plan, with no "mixed" branch to reason about.

**Why this priority**: This is the structural pivot the rest of the iteration builds on. Without single-type items the 1:1 item↔plan invariant cannot hold, the destination-root choice stays ambiguous per item, and the "mixed" affordances persist. Highest-value slice.

**Independent Test**: Classify a leaf folder containing light frames at two exposures plus dark frames; verify the inbox shows one homogeneous item per group (e.g. `(root) · light · Ha · 300s`, `(root) · light · Ha · 120s`, `(root) · dark · -10°C · 300s`), zero items labelled "mixed", each item exposes exactly one destination root, and each item produces exactly one plan on confirm.

**Acceptance Scenarios**:

1. **Given** a leaf folder mixing more than one frame type, **When** it is classified, **Then** the inbox materializes one single-type item per homogeneous group and no item is labelled "mixed".
2. **Given** a single-type item, **When** the user confirms it, **Then** it produces exactly one plan (item↔plan is 1:1) with a single chosen destination root.
3. **Given** an unchanged folder, **When** it is rescanned, **Then** the same group keys and the same items are produced (no item churn).

---

### User Story 11 - Field-agnostic reclassify over a typed property registry (Priority: P1)

As a user correcting incomplete headers, I want to fill any missing metadata property (temperature, gain, target, offset, or any future field) through a generic editor and apply a value across many files at once, so that the app is not limited to a fixed set of overridable fields and never rewrites my files.

**Why this priority**: Real libraries have missing or non-standard headers across many fields; a fixed override list (filter/exposure/binning) cannot keep pace. A typed, future-proof property map with bulk apply is required for the gate (US12) and grouping (US10) to be usable. Highest-value slice.

**Independent Test**: Open a source group whose files lack temperature and gain; via the generic metadata table set temperature and gain for the whole selection ("set all"); verify each file receives the values, header-present fields remain read-only, the file bytes on disk are unchanged, and the items re-partition to reflect the new values.

**Acceptance Scenarios**:

1. **Given** any property defined in the registry (including temperature, gain, target), **When** the user supplies a value for one or more files, **Then** the value is recorded and reflected in classification, grouping, and destination — with no code change required to support that property.
2. **Given** a value already present in the header, **When** the editor renders it, **Then** it is shown read-only (gap-filling, not rewriting), and the only exception is the explicit "correct classification" frame-type action.
3. **Given** a user supplies a value via "set all", **When** the override is applied, **Then** every targeted file receives it and the user's files on disk are never modified (index-only).

---

### User Story 12 - Generalized missing-mandatory gate with a needs-review bucket (Priority: P1)

As a user, I want files missing any mandatory grouping or path attribute to be collected into a clear "needs-review" bucket that blocks plan creation until I resolve it, so that no item is confirmed into a plan with incomplete metadata.

**Why this priority**: Generalizes the existing missing-path-attribute gate to all mandatory grouping attributes (including an unclassifiable frame type). Without it, single-type items could form on incomplete metadata and route files to placeholder paths. Highest-value slice.

**Independent Test**: Classify a folder where some lights lack a date and some files lack a readable frame type; verify those files land in a per-source-group "needs-review" item that cannot be confirmed; supply the missing values via the generic editor; verify the system re-runs classification and re-splits the needs-review bucket into proper single-type items, which then become confirmable.

**Acceptance Scenarios**:

1. **Given** files missing a mandatory grouping or path attribute (or an unclassifiable frame type), **When** classification runs, **Then** they are collected into a per-source-group needs-review item that blocks plan creation.
2. **Given** a needs-review item, **When** the user supplies the missing values, **Then** the system re-runs classification + grouping and re-splits the bucket into fully-resolved single-type items (splitting happens before confirm, never inside plan creation).
3. **Given** an item still carrying a missing mandatory attribute, **When** the user attempts to confirm it, **Then** confirm is rejected until the attribute is supplied.

---

### User Story 13 - Source-group provenance (Priority: P2)

As a user, I want each single-type item to show the leaf folder it came from and its sibling items, so that I can see what was "ingested together" and trust where files originated.

**Why this priority**: Single-type items fragment a folder into several rows; without provenance the user loses the sense of which items share an origin. Improves trust and review ergonomics; secondary to forming the items themselves.

**Independent Test**: Classify a mixed folder; verify each resulting item carries a source-group id and a display label of the form `(root) · <type> · <discriminating dims>`, and the UI groups the children under their shared parent leaf folder with a sibling count.

**Acceptance Scenarios**:

1. **Given** a classified leaf folder, **When** its items are listed, **Then** each item exposes a source-group id and a human-readable label, and the children are presented under their shared parent folder.
2. **Given** a source group with several children, **When** the user views it, **Then** the sibling count and the "ingested together" relationship are visible.

---

### User Story 14 - Sessions as derived, already-confirmed inventory (Priority: P2)

As a user, I want acquisition and calibration sessions to be derived directly from confirmed per-file metadata (with no separate review lifecycle), and their metadata to remain editable after the fact, so that I am not asked to re-review what the inbox already confirmed.

**Why this priority**: Once per-file metadata is fixed at inbox confirm, the session key is deterministic and there is nothing left to review; the review lifecycle adds friction and duplicates the inbox gate. Folds in the universal-gate decision.

**Independent Test**: Confirm inbox items that form a session; verify the session appears as derived, already-confirmed inventory with no Confirm/Re-open/Reject affordances; edit a session's metadata and verify it re-opens the same editable per-file metadata table without any lifecycle state transition.

**Acceptance Scenarios**:

1. **Given** confirmed inbox items, **When** sessions are derived, **Then** they appear as already-confirmed inventory with no review states (discovered/candidate/needs_review/confirmed/rejected) and no Confirm/Re-open/Reject controls.
2. **Given** a derived session, **When** the user edits its metadata, **Then** the same persisted, editable per-file metadata is updated with no lifecycle gate.

---

### User Story 15 - Coordinate-based target resolution at light ingestion (Priority: P2)

As a user ingesting light frames, I want the app to resolve the target by sky coordinates (nearest within a FOV-aware radius) rather than by the `OBJECT` string, and to propagate the chosen target to any project that consumes those lights, so that target identity is robust to inconsistent naming.

**Why this priority**: `OBJECT` is free-text and user-set in capture software; coordinate nearest-neighbour over the existing target/SIMBAD database is robust and enables automatic project linkage. Builds on US10 (light groups carry a pointing).

**Independent Test**: Classify a light sub-group with a plate-solved pointing; verify a ranked list of recommended targets within the FOV-aware radius is offered (plus free-text search and manual set), the chosen `target_id` drives the group's canonical label, and it auto-propagates to a linked project.

**Acceptance Scenarios**:

1. **Given** a light sub-group with a pointing (decimal `RA`/`DEC`, or converted `OBJCTRA`/`OBJCTDEC`), **When** target resolution runs, **Then** the app ranks candidate targets by angular separation within a FOV-aware radius and uses `OBJECT` only as an initial display name, never for matching.
2. **Given** a resolved or user-chosen `target_id`, **When** it is set, **Then** it becomes the sub-group's canonical target (driving the label and unifying the group key) and propagates to any project consuming those lights.
3. **Given** no pointing and no user-set target, **When** classification runs, **Then** the light sub-group falls into the needs-review bucket (target is mandatory for lights).

---

### User Story 16 - Extended header extraction (Priority: P1)

As a user, I want the app to extract the additional header fields the new grouping recipes rely on (offset, temperatures, pointing, rotation, readout mode, focal length, observer location, and local time), so that single-type grouping, target resolution, and flat↔light matching have the inputs they need.

**Why this priority**: None of the new grouping inputs are extracted today (only 13 core fields are read); without them every new grouping dimension behaves as "(unknown)". Foundational to US10/US12/US15.

**Independent Test**: Extract metadata from real FITS and XISF files and verify the new fields are populated with correct units and fallbacks (e.g. decimal `RA`/`DEC` preferred over converted `OBJCTRA`/`OBJCTDEC`; `SET-TEMP`/`CCD-TEMP`/`DET-TEMP`; `ROTATANG`; `FOCALLEN`; observer `SITE*`→`OBSGEO-*`→`(LAT/LONG/ALT)-OBS`; `DATE-LOC`), and that filename-encoded metadata is not used as a source.

**Acceptance Scenarios**:

1. **Given** a FITS or XISF file, **When** metadata is extracted, **Then** the extended fields (offset, set/ccd/det temperature, decimal pointing, rotation, readout mode, focal length, observer location, local time) are populated using the documented keyword and fallback chains, with XISF unit conversions applied.
2. **Given** a field is absent in a file, **When** grouping uses it, **Then** the file falls into an explicit "(unknown)" bucket with a warning rather than blocking, and filename-encoded metadata is never used as an extraction source.

## Requirements *(mandatory)*

### Functional Requirements

**Reviewable plan surface (US1)**

- **FR-001**: Confirming an inbox item MUST produce a reviewable plan describing each intended filesystem action (source → destination, or catalogue-in-place) before any file is moved.
- **FR-002**: A confirmed (planned) item MUST remain visible and selectable in the queue, distinctly marked as planned (e.g. greyed with a planned badge), rather than being removed.
- **FR-003**: The system MUST present the plan's actions in an in-context plan panel within the inbox surface (at the bottom of the central detail area) and MUST require an explicit Apply step to execute them.
- **FR-003a**: The system MUST let the user apply a single item's plan or batch-apply all pending planned items in one action; each applied action MUST be individually recorded in the audit log.
- **FR-004**: "View plan" and plan review MUST keep the user within the inbox surface and MUST NOT navigate to the Archive page or any unrelated page.
- **FR-005**: Applying a plan MUST execute its actions, MUST never overwrite existing files silently, and MUST write an audit record for each attempted action and its outcome.
- **FR-006**: Users MUST be able to review the full plan and cancel a plan before applying, leaving all files untouched.
- **FR-006a**: Applying an inbox plan MUST require an explicit user approval recorded before apply (the approval token spec 017 issues and spec 025 FR-001 requires). The inbox apply path MUST NOT mint an approval on the user's behalf. A plan carrying no approval MUST be refused with `plan.approval_required` and MUST NOT be applied. Under `inbox.plan.apply_all` and `inbox.plan.apply_selected` this is a per-plan result rather than a whole-batch failure: an unapproved plan is reported individually and the remaining plans still apply.
- **FR-006b**: The recorded approval MUST be handed to the shared apply executor for verification, so an absent, empty, or mismatched token — a plan re-approved or tampered with since the user approved it — is refused with `plan.approval.stale` (spec 025 FR-011). There is no approval TTL.
- **FR-006c**: The one exception is the mkdir-only auto-apply path (`plans::auto_apply`), which creates directories, moves no user file, and does not route through inbox apply. Every path that moves or catalogues a user file is bound by FR-006a.
- **FR-007**: The system MUST refuse to apply a plan whose source files have changed since the plan was generated, and MUST surface the staleness to the user. This staleness (`plan.stale`, raised per item by the executor's content check) is distinct from the approval staleness of FR-006b.

**Structured, groupable review + metadata (US2)**

- **FR-008**: The review list MUST present each item in a structured layout (type(s), file count, format, master indicator) using no pills, following the standard sidebar layout, with no overflowing or clipped content.
- **FR-009**: The review list MUST support grouping by one or more dimensions in a user-chosen order (multi-level / nested grouping), with collapsible groups, and items missing a grouping dimension gathered under an explicit "none" group. The available grouping dimensions MUST include target, frame type, filter, exposure, date, and source.
- **FR-010**: The detail panel MUST display, per file, the extracted header metadata: image type, filter, exposure, binning, gain, temperature, object, and observation date (showing each as unavailable when absent).
- **FR-011**: When an item contains more than one frame type, the detail panel MUST show the explicit composition (each frame type and its count) rather than only a "mixed" label.
- **FR-012**: The detail panel MUST resolve and display classification/metadata (including master type) for single-file calibration master items without hanging.

**Overrides (US3)**

- **FR-013**: Users MUST be able to override the inferred frame type and other surfaced header fields (at minimum filter, exposure, and binning) for a file before confirming.
- **FR-014**: Users MUST be able to select multiple files (within an item and across items, including via the grouped list) and apply an override to all selected at once, with an accurate applied-count.
- **FR-015**: Overrides MUST be reflected in the item's classification breakdown and in the destination/plan, the breakdown MUST remain visible after overrides are applied, and overrides MUST be scoped to the intended files only (no leakage across items).
- **FR-016**: Overrides MUST be recorded as app-side metadata only and MUST NOT modify the user's files (header values in the files are never rewritten).
- **FR-016a**: Overrides MUST persist across rescans of the same files — keyed to the file's content so they re-apply automatically — and MUST be invalidated only when the file's content changes.

**Move vs catalogue-in-place (US4)**

- **FR-017**: The system MUST decide between generating a move plan and cataloguing in place based on the **organization state** of the file's source (organized → catalogue in place; unorganized → move plan), independent of the source's content `kind` and of the frame type. For an item spanning files of mixed provenance, the decision MUST be made per file (files from organized sources catalogued in place; files from unorganized sources moved), so a single item's plan MAY contain both catalogue and move actions. **(Iteration 2026-06-23 note:** under the source-group model a single-type sub-item derives from one leaf folder = one source = one `organization_state`, so its provenance is uniform; the per-file mixed-provenance split no longer arises at the sub-item level, and each item's plan is wholly move **or** wholly catalogue.**)**
- **FR-018**: Cataloguing in place MUST record the files in the database with no move plan and no file movement.
- **FR-019**: Calibration masters and light frames MUST follow the same organization-state rule; a master from an unorganized source gets a reviewable move plan and is registered as a master, while a master from an organized source is registered without a move.
- **FR-019a**: Each source MUST carry an explicit organization state (organized / unorganized) that is orthogonal to its content `kind`; the same kind MAY have both organized and unorganized sources simultaneously.
- **FR-019b**: When adding a non-inbox source, the system MUST require the user to explicitly choose its organization state (no silent default) and MUST explain the consequence of the choice in the add-source / wizard flow (ideally with a flow diagram). `inbox`-kind sources are unorganized by definition. The organization state MUST be changeable later, affecting only future confirmations.

**Auto-split (US5)**

- **FR-020**: ~~Confirming a folder containing more than one frame type MUST automatically produce a distinct plan action per frame type, each routed to its own destination, without a separate manual split step.~~ **[SUPERSEDED by FR-034 / FR-050 / SC-012]** — under single-type sub-items a mixed folder materializes N single-type items at classify time, each confirmed into its own 1:1 plan; there is no mixed item to auto-split.

**Statistics (US6)**

- **FR-021**: The inbox queue summary MUST present counts broken down by type (folders per type, masters per type, images per type) consistent with the queue contents.

**Destructive destination (US7)**

- **FR-022**: When a plan includes a destructive action, the system MUST present a clearly labelled destructive-destination control (Archive folder vs System Trash) within the plan/review surface, defaulting to Archive, with its meaning explained at the point of use.
- **FR-023**: Destructive actions MUST prefer a recoverable destination (archive or system trash) over permanent deletion and MUST record the chosen destination in the audit log.

**Destination preview (cross-cutting)**

- **FR-024**: During review (before applying), the system MUST show the resolved destination for each file/group based on the active naming pattern, and MUST degrade gracefully (a clear message, not a blank field) when no pattern is configured.

**Per-type destination patterns (iteration 2026-06-21)**

- **FR-025**: The destination path structure MUST be configurable per frame-type class, with a distinct token-based pattern for at least: light, flat, master flat, bias, master bias, dark, master dark. Patterns use the shared path-token vocabulary and are editable in Settings.
- **FR-026**: Each per-type pattern MUST have a sensible built-in default reflecting the attributes meaningful for that type; calibration types MUST NOT include a target segment. Default intent — light: target/filter/date; flat: filter/date; dark: exposure (+gain/temp/binning as configured); bias: gain/temp/binning; master flat/bias/dark: as their raw counterpart minus date (masters are not per-night).
- **FR-026a**: The resolver MUST select the pattern by the file's resolved type including master-vs-raw (a master dark uses the master-dark pattern, etc.).
- **FR-026b**: Per-type patterns MUST be persisted in settings and user-overridable; an invalid/empty pattern falls back to that type's built-in default. (Light-master / integration routing is out of scope for this iteration — TBD.)

**Destination root selection (iteration 2026-06-21)**

- **FR-027**: For a move from a non-inbox source, the default destination root is the source's own root (reorganize in place).
- **FR-028**: Inbox-kind sources MUST move into a chosen library root (never catalogued/left in place); a destination root is always required for inbox items.
- **FR-029**: When more than one registered root is a valid destination for the item's frame type, the user MUST explicitly select the destination root before the plan can be applied.
- **FR-030**: When exactly one valid destination root exists for the frame type, it MUST be selected automatically with no prompt.
- **FR-031**: The plan/review surface MUST display the full absolute destination path (selected root path + relative path) for each action, not just the root-relative path.

**Mandatory path-attribute capture (iteration 2026-06-21)**

- **FR-032**: Plan generation MUST be gated on the presence of every path-load-bearing attribute for each file; a missing value MUST block the plan and surface the file in the needs-review flow, consistent with how missing IMAGETYP is handled.
- **FR-033**: The set of path-load-bearing attributes MUST be defined per frame type (enumerated in research.md) and MUST drive both the gate (FR-032) and the per-type destination structure (FR-025/FR-026).

**Single-type ingest (iteration 2026-06-23, US10/US13/US16)**

- **FR-034**: The inbox unit of work MUST be one single-type sub-item per homogeneous group within a leaf folder, replacing the folder-level item (supersedes the one-item-per-leaf-folder invariant).
- **FR-035**: A group key per frame type MUST be `frame_type` plus a configurable set of identity dimensions (per the default recipes in research.md R-9); every dimension MUST be individually toggleable in settings per frame type.
- **FR-036**: Continuous dimensions MUST be bucketed (exposure normalized to canonical seconds; set-temperature bucketed, default 2 °C tolerance) and discrete dimensions normalized then matched exactly.
- **FR-037**: Temperature grouping MUST use `SET-TEMP` by default with a toggle to use `CCD-TEMP`; a `CCD-TEMP`-vs-`SET-TEMP` deviation beyond a configurable threshold (default 2 °C) MUST surface a metadata-quality warning but MUST NOT split the group; light frames MUST NOT group by temperature.
- **FR-038**: Pointing grouping MUST use decimal `RA`/`DEC` (falling back to converted `OBJCTRA`/`OBJCTDEC`) within a configurable `pointing_tolerance_deg`.
- **FR-039**: The optical-train key MUST be a composite of `TELESCOP`, `INSTRUME`, and `FOCALLEN`; light and flat frames MUST group/match on the full optical train (so `FOCALLEN` captures focal reducers implicitly).
- **FR-040**: Flat↔light rotation matching MUST compare the flat group's `ROTATANG` against the light group's `ROTATANG` (near-exact) and MUST warn on any deviation; because manual-rotator drift does not update `ROTATANG`, only the flat-vs-light group deviation is determinable; when `ROTATANG` is absent, a configurable `flat_rotation_required` (default off) decides exclusion, and light grouping uses a separate loose `light_rotation_tolerance_deg` for cross-setup variance.
- **FR-041**: The pipeline MUST be classify-then-split: scan stays lazy (no eager header reads or hashing) and single-type sub-items are materialized only at classify time.
- **FR-042**: Item identity MUST be the composite `(root_id, relative_path, group_key)` with a per-sub-group content signature; group keys MUST be deterministic so rescans of unchanged content produce identical items.
- **FR-043**: Source-group provenance MUST be exposed on each item (source-group id and a display label, parent leaf folder → child single-type items).
- **FR-053**: Metadata extraction MUST be extended to the fields the new recipes rely on: `OFFSET`; `SET-TEMP`/`CCD-TEMP`/`DET-TEMP`; `RA`/`DEC` with `OBJCTRA`/`OBJCTDEC`→decimal fallback; `ROTATANG` (= `ROTATOR`)/`ROTNAME`/`OBJCTROT`; `READOUTM` (optional, default off); `FOCALLEN`; observer location via `SITE*`→`OBSGEO-*`→`(LAT/LONG/ALT)-OBS`; and time via `DATE-LOC`/`DATE-END`/`MJD-AVG`/`MJD-OBS`; filename-encoded metadata MUST NOT be used as a trusted source.

**Field-agnostic reclassify (iteration 2026-06-23, US11)**

- **FR-044**: Reclassify MUST be field-agnostic over a typed property registry, accepting an arbitrary per-file property map plus a bulk "set all" form; a new `inbox.property_registry` operation MUST expose the registry so the UI renders a generic editor.
- **FR-045**: The metadata editor MUST fill only MISSING properties (header-present values shown read-only); all overrides MUST be index-only and MUST NEVER be written back to FITS/XISF files; the one explicit exception is the "correct classification" frame-type action.
- **FR-046**: Overrides MUST be persisted keyed to `(source_group, relative_file_path, property_key)` so they survive re-partitioning of items, with staleness keyed to file size + mtime.

**Generalized gate (iteration 2026-06-23, US12)**

- **FR-047**: The missing-mandatory gate MUST generalize to the union of mandatory grouping properties and destination-pattern tokens; the per-frame mandatory set MUST be derived from the active pattern ∪ enabled grouping dimensions ∪ hard calibration/session keys (not hardcoded). For light frames, `target` MUST be enforced as a hard mandatory key (even though it is not a header or pattern token), satisfiable by coordinate auto-resolution (FR-052) OR an explicit user pick; a light with no pointing and no user-set target MUST route to needs-review.
- **FR-048**: Files missing a mandatory attribute or with an unclassifiable frame type MUST be collected into a per-source-group "needs-review" sub-item that blocks plan creation until resolved.
- **FR-049**: The system MUST follow a split-before-confirm loop: when the user supplies missing values it re-runs classification + grouping and re-splits the sub-items; only a fully-resolved single-type sub-item is confirmable, and confirm MUST reject any item still carrying missing mandatory attributes.

**Confirm simplification & lifecycle drop (iteration 2026-06-23, US10/US14)**

- **FR-050**: Confirm MUST be simplified: the action is always "confirm" (the "split"/"mixed" path is removed); each item carries one chosen destination `rootId`; the `destination_root_required` candidate-roots flow is retained; and the 1:1 plan link is preserved.
- **FR-051**: Acquisition and calibration sessions MUST be derived, already-confirmed inventory: the review lifecycle (discovered/candidate/needs_review/confirmed/rejected) and the Confirm/Re-open/Reject affordances MUST be removed, while session metadata remains editable post-hoc.

  > **Amended 2026-07-09**: scope narrowed by the 006 iteration of 2026-07-03
  > (pending-iteration.md disposition 4), which deliberately retained the
  > `ignored` canonical state and shipped the Ignore/recover flow (006
  > FR-010, T420/T421/T403). FR-051/SC-018 remain satisfied for
  > Confirm/Re-open/Reject affordances (removed — migration 0050,
  > SessionsPage); the six-state `SessionState` contract is retained by
  > product decision. See `sync-conflicts-2026-07-09.md`.

**Coordinate target resolution (iteration 2026-06-23, US15)**

- **FR-052**: Light targets MUST be resolved by coordinate-based, FOV-aware nearest-neighbour over the target database at light ingestion; `OBJECT` MUST be used only as an initial display name and never for search/matching; the chosen target MUST propagate to any linked project.

**Migration (iteration 2026-06-23, US10/US14)**

- **FR-054**: A migration MUST re-derive existing folder-level items into source-groups plus single-type sub-items, filesystem-free from already-persisted metadata; items with an open plan MUST be kept as a single legacy sub-item until their plan resolves or is discarded.

### Key Entities *(include if feature involves data)*

- **Inbox item**: A unit awaiting review (a sub-frame folder or a single master file) with a source, classification state (unclassified / classified / planned / applied), and a per-type composition.
- **Source organization state**: A per-source flag — *organized* (catalogue-in-place) vs *unorganized* (eligible for move plans) — orthogonal to the source's content `kind` (`light_frames`/`calibration`/`project`/`inbox`). Set explicitly by the user when adding a non-inbox source (inbox is unorganized by definition), changeable later. Drives FR-017.
- **File metadata record**: Per-file extracted header values (image type, filter, exposure, binning, gain, temperature, object, date) surfaced for review and override.
- **Override**: A user-supplied correction to one or more header-derived values for one or more files; app-side only, never written to the file.
- **Grouping**: An ordered list of dimensions (e.g. target, then frame type, then filter) used to nest the review list.
- **Plan**: A reviewable, named set of filesystem actions (move / catalogue / archive / trash) generated on confirm, with a state (open / applied / cancelled / stale) and an audit trail.
- **Plan action**: A single intended filesystem operation (source → destination, catalogue-in-place, or destructive) with a resolved destination preview.
- **Source group** (iteration 2026-06-23): A leaf folder discovered at scan, modelling provenance ("ingested together") — its files are partitioned into single-type sub-items at classify. Carries id, root, relative path, content signature, format, lane, and child count.
- **Single-type sub-item** (iteration 2026-06-23): One homogeneous inbox item within a source group, identified by `(root_id, relative_path, group_key)` with an authoritative frame type, a group label, and a per-sub-group content signature; replaces the folder-level inbox item and maps 1:1 to a plan.
- **Property registry** (iteration 2026-06-23): The typed catalogue of metadata properties (key, kind, unit, source header(s), overridability, applicable frame types, validation) that drives the field-agnostic editor, grouping recipes, and the mandatory-attribute gate.
- **Override** (iteration 2026-06-23): A user-supplied value for a missing property, persisted keyed to `(source_group, relative_file_path, property_key)`; index-only and **never written to FITS/XISF** files; re-drives classification, grouping, path resolution, and the gate.
- **Derived session** (iteration 2026-06-23): An acquisition or calibration session computed from confirmed per-file metadata, with no review lifecycle; its identity is deterministic from the inbox-confirmed metadata, which remains editable post-hoc.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of confirm operations that move files do so only after an explicit Apply on a reviewable plan; zero files are moved at confirm time.
- **SC-002**: After confirming an item, the user can review the resulting plan and its actions without leaving the inbox surface (no navigation to the Archive or any unrelated page) in 100% of cases.
- **SC-003**: A confirmed item remains visible (as "planned") until its plan is applied or cancelled; zero confirmed items vanish without trace.
- **SC-004**: A user can see each file's frame type, filter, exposure, and binning, and correct any of them, for every reviewable item (no metadata field is permanently hidden).
- **SC-005**: Applying an override to a selection of N files updates exactly those N files and the reported count equals N in 100% of cases.
- **SC-006**: The review list can be grouped by at least two dimensions in a chosen order, and the resulting nesting matches the data in 100% of cases.
- **SC-007**: Confirming an item from an already-organized library root results in zero file movements and a catalogue record, for both light and calibration items.
- **SC-008**: ~~Confirming a mixed-type folder yields one plan action per distinct frame type with no separate user action, in 100% of cases.~~ **[SUPERSEDED by SC-012]** (a mixed folder produces N single-type items and zero "mixed" items; each item confirms into its own 1:1 plan).
- **SC-009**: The Calibration page and the inbox detail panel render without runtime errors for freshly-confirmed masters (zero crash reports for the master/detail views).
- **SC-010**: The inbox summary's per-type counts match the actual queue contents exactly.
- **SC-011**: No status pills overflow the sidebar; the review list and detail panel follow the standard pinned-bar/scrolling-content layout at the supported window sizes.
- **SC-012**: A mixed folder produces N single-type items and zero "mixed" items.
- **SC-013**: Every inbox item has at most one open plan.
- **SC-014**: Reclassify accepts any registry property (including temperature, gain, target) with no code change.
- **SC-015**: No item with missing mandatory attributes can create a plan.
- **SC-016**: Overrides never modify source files (file bytes are unchanged after an override).
- **SC-017**: A light sub-group resolves a canonical target by coordinates within the FOV radius (or prompts the user); the chosen target appears on the linked project.
- **SC-018**: No session exposes a Confirm/Re-open/Reject review action.

  > **Amended 2026-07-09**: see the FR-051 annotation above — scope narrowed
  > by 006's retained `ignored` state and Ignore/recover flow; SC-018 is
  > satisfied for the removed Confirm/Re-open/Reject affordances specifically.
  > See `sync-conflicts-2026-07-09.md`.
- **SC-019**: A rescan of unchanged content yields identical group keys (no item churn).

## Out of Scope / Non-Goals

- Rewriting FITS/XISF file headers from user overrides (overrides are app-side only — FR-016).
- Any image processing (calibration, stacking, registration, etc.) — the PixInsight boundary is unchanged.
- Designing a new naming-pattern editor; this feature consumes the existing active pattern for destination resolution.
- A general cross-page "Plans/Archive" management redesign beyond surfacing the inbox-generated plan in-context.
- Automatic application of plans without explicit user Apply.

**In scope (iteration 2026-06-23)**: the single-type sub-item model (homogeneous items materialized at classify, item↔plan 1:1), the typed property registry and field-agnostic reclassify, the generalized missing-mandatory gate, source-group provenance, coordinate-based light target resolution, extended header extraction, and dropping the session review lifecycle in favour of derived sessions.

**Out of scope (iteration 2026-06-23)**: any image processing remains out (the PixInsight boundary is unchanged — no calibration, stacking, registration, or header rewriting from overrides); overrides stay index-only.

## Assumptions

- **Organization state is a new per-source field**: Today's `SourceKind` (`light_frames`/`calibration`/`project`/`inbox`) describes content role, not organization, so a new explicit per-source organization-state field is introduced. FR-017 keys on it. There is no silent default for non-inbox sources — the user chooses at add-time; `inbox` sources are unorganized by definition.
- **Overrides never modify files**: Consistent with the PixInsight boundary and Local-First custody, overrides are stored as application metadata and never written back into FITS/XISF headers (FR-016).
- **Plans remain the mutation mechanism**: This feature changes the *visibility, placement, and triggering* of plans, not the underlying reviewable-plan/audit machinery, which already exists.
- **Destination resolution reuses the active naming pattern**: Destination previews and plan destinations are resolved from the already-configured naming pattern, surfaced earlier (at review) rather than via a new resolver.
- **Metadata is already extracted**: Per-file FITS/XISF header values are already extracted during scan/classify; this feature surfaces, persists, and lets users correct them rather than adding new extraction capability.
- **Standard layout convention applies**: The list and detail follow the project's established page layout convention (pinned bars, scrolling content) so nothing overflows.
- **Folds prior specs**: The mixed-folder split (spec 005) becomes implicit in confirm; cross-root inbox (spec 039) and master detection (spec 040) remain in force and are extended, not replaced.
- **Recent bug fixes are stopgaps**: PR #298 restored basic function (masters classify, override scope, breakdown persistence, destination placeholder, calibration crash); this spec defines the intended behavior that supersedes those stopgaps.

## Dependencies

- Existing reviewable-plan and audit subsystem (plan generation, apply, audit records, archive/trash safeguards).
- Existing FITS/XISF metadata extraction (frame type, filter, exposure, binning, gain, temperature, object, date).
- Existing naming-pattern resolver for destination computation.
- Existing calibration master detection/registration (spec 040) and cross-root inbox (spec 039).
