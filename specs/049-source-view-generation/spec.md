# Feature Specification: Source View Generation

**Feature Branch**: `049-source-view-generation`

**Created**: 2026-07-04

**Status**: Implemented (post-hoc record, verified 2026-08-24; open questions were
resolved 2026-07-04). All four user stories ship: US1 `sourceview_generate`
(`apps/desktop/src-tauri/src/commands/prepared_views.rs:130`, registered at
`bootstrap/specta.rs:447`, covered by
`crates/app/core/tests/source_view_generation_us1.rs`); US2 profile-driven layout
via `workflow_profiles::seed::resolve_source_view_layout`
(`crates/app/projects/src/source_view_generate/generate.rs:78`); US3
`preparedview_regenerate` (`specta.rs:445`,
`crates/app/core/tests/source_view_generation_us3_regenerate.rs`); US4
`sourceview_verify` (`prepared_views.rs:151`,
`crates/app/core/tests/source_view_verify_us4.rs`). Destination model shipped as
`sourceview_destination_get` / `_set` (`prepared_views.rs:169`/`:188`), and
`crates/e2e-tests/tests/source_view_journeys.rs` exercises the surface
end-to-end.

**Input**: User description: "Restore source-view generation: the app generates a
processing-tool-ready (WBPP/PixInsight-first) project input directory on disk —
the user's selected light frames organized by the tool's expected structure
(session/night, filter, exposure grouping) plus the matched calibration
frames/masters — using symlinks (Windows: junctions/hardlink fallback per
filesystem capability), never copies by default."

> **Companion to Spec 026.** This feature is the **generation** counterpart of
> [Spec 026 — Generated Project Source View Removal](../026-generated-project-source-view-removal/spec.md),
> which built (and left inert) the **remove / regenerate / stale-detect**
> machinery for prepared source views. Spec 026 was marked *POSSIBLY OBSOLETE*
> on 2026-07-03 because no path in the app ever **created** a generated view.
> **This spec restores that path** (product decision 2026-07-04, reversing the
> retire lean recorded in `docs/development/orchestrator-handover-2026-07-03.md`).
> This spec MUST NOT duplicate spec 026's remove/regenerate/stale machinery; it
> reuses the `PreparedSourceView` / `PreparedSourceViewItem` entities and the
> spec 017/025 plan review→approve→apply pipeline, and adds only the
> **generation** (first-materialization) surface.

## Overview

PlateVault helps astrophotographers prepare inputs for external processing
tools without processing images itself. A user who is about to run PixInsight's
WeightedBatchPreProcessing (WBPP) needs the frames for a project laid out in the
folder structure WBPP expects — light frames grouped by acquisition session /
night, filter, and exposure, alongside the calibration frames or masters that
match them. Today users assemble that tree by hand, copying gigabytes of raw
subs into a scratch folder, which wastes disk, drifts from the canonical
library, and is error-prone.

This feature lets the app **generate** that tool-ready input directory on disk
as a reviewable filesystem plan of **link** actions (symlinks by default; on
Windows, junctions or hardlinks selected by a filesystem-capability check),
**never copies by default**. The generated tree is a **reproducible projection**
of the canonical database — the database stays the source of truth, the links
are recorded as app-created projections (never mistaken for originals), and
every generation is an auditable, reviewable filesystem plan. The app produces
**only the file tree**; it never writes WBPP/PixInsight configuration, process
icons, or `.xpsm`/`.xosm` files (Constitution III — PixInsight boundary).

Removal, regeneration-after-removal, and stale detection of these views are
already specified by Spec 026 and are reused unchanged.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Generate a WBPP-ready source view for a project (Priority: P1)

As a PixInsight user preparing to run WBPP, I want the app to generate a folder
tree of my project's selected light frames plus their matched calibration,
organized the way WBPP expects, using links rather than copies, so I can point
WBPP at one folder without hand-assembling it or duplicating gigabytes of raw
subs.

**Why this priority**: This is the whole point of the feature and the MVP. Without
first-time generation, spec 026's remove/regenerate surface has nothing to act on.

**Independent Test**: For a project with selected light frames and existing
calibration matches, request a source-view generation, review the produced
filesystem plan, approve and apply it, and confirm a folder tree appears on disk
whose entries are links (symlinks, or junctions/hardlinks on Windows) pointing to
the canonical source files — with zero originals copied — and that the canonical
database is unchanged.

**Acceptance Scenarios**:

1. **Given** a project with selected present light frames and matched
   calibration, **When** the user requests generation, **Then** a reviewable
   filesystem plan of link-creation actions is produced and nothing is written
   to disk until the user approves and applies it.
2. **Given** an approved generation plan is applied on a filesystem that supports
   symlinks, **When** apply succeeds, **Then** the destination tree contains one
   link per selected frame and matched calibration item, every link resolves to
   its canonical source, and no original file was copied or moved.
3. **Given** the plan is applied, **When** it completes, **Then** a
   `PreparedSourceView` record (per spec 026) is created referencing each source
   inventory item, the view is marked `current`, and each created link is
   recorded as an app-created projection (not an original).
4. **Given** any generation, **When** it runs, **Then** it produces **no** WBPP
   or PixInsight configuration/process-icon/`.xpsm`/`.xosm` files — only image
   links and their containing folders.

---

### User Story 2 - Per-tool profile structure (Priority: P2)

As a user who processes with different tools (WBPP today, possibly Siril later),
I want the generated tree's layout to follow the selected processing profile's
expected structure, so the output matches whatever tool I am about to run rather
than one hardcoded shape.

**Why this priority**: Tool-agnostic structure is a core product principle
(PRODUCT.md: "Stay tool-agnostic … selectable profiles, not hardcoded
assumptions"). WBPP is the first profile; the layout must not be baked in.

**Independent Test**: Generate a view with the WBPP profile selected and confirm
lights are grouped by session/night, filter, and exposure with calibration in
WBPP's expected location; change the profile's layout pattern and regenerate, and
confirm the tree structure changes accordingly without touching canonical data.

**Acceptance Scenarios**:

1. **Given** the WBPP profile is selected, **When** a view is generated, **Then**
   light frames are grouped by acquisition session/night, filter, and exposure,
   and matched calibration is placed in the profile's expected calibration
   location.
2. **Given** a profile's layout is expressed as a token pattern, **When** the
   pattern is changed, **Then** regenerating the view produces the new folder
   structure using the same canonical sources, with no image processing performed.
3. **Given** two source frames would map to the same destination path under a
   layout, **When** the plan is built, **Then** it is refused with a validation
   error that points at the offending pattern (collisions are impossible by
   construction — never silently suffixed, merged, or dropped).

---

### User Story 3 - Regenerate after a selection or match change (Priority: P2)

As a user who added, culled, or re-matched frames after generating a view, I want
to regenerate the view to reflect the current canonical selection, so the tree I
hand to WBPP is never out of date.

**Why this priority**: A generated tree that silently drifts from the current
selection is worse than none. Reuses spec 026's regeneration machinery so removal
and regeneration stay a single mechanism.

**Independent Test**: Generate a view, then change the project's selected lights
or its calibration matches, request regeneration, and confirm the new plan
adds/removes exactly the changed items and flags any unresolved references —
reusing the spec 026 regeneration plan.

**Acceptance Scenarios**:

1. **Given** an existing generated view, **When** the project's selected lights
   or calibration matches change and the user regenerates, **Then** a reviewable
   plan is produced reflecting the current canonical selection (spec 026
   regeneration machinery), and it flags any source that no longer resolves.
2. **Given** a source referenced by the view is now missing/unresolved, **When**
   regeneration runs, **Then** that item is surfaced (skipped and flagged), and
   the remaining view is still regenerable.
3. **Given** stale-view detection and removal, **When** those are needed, **Then**
   they are handled by spec 026, not re-implemented here.

---

### User Story 4 - Verify a generated view before processing (Priority: P2)

As a user about to launch WBPP against a generated view, I want to verify every
link still resolves to a present source, so I discover broken or stale links
before starting a long processing run rather than after it fails.

**Why this priority**: Verification protects an expensive downstream operation.
It leans on spec 026 stale detection but frames it as an explicit
pre-processing check.

**Independent Test**: Generate a view, remove or move one source outside the app,
run verify-before-processing, and confirm the broken item is reported without any
filesystem mutation and without auto-repair.

**Acceptance Scenarios**:

1. **Given** a generated view whose sources are all present, **When** the user
   verifies it, **Then** it reports clean and is safe to process.
2. **Given** a generated view with a source that is missing, moved, or a link
   that no longer resolves, **When** the user verifies it, **Then** each broken
   item is reported with its source reference, no file is mutated, and no
   auto-repair occurs (repair is via explicit regeneration).

---

### Edge Cases

- **No symlink capability on the destination** (e.g., exFAT, SMB share, or Windows
  without symlink privilege): the app resolves a documented fallback
  (junction for directories / hardlink for same-volume files) or **refuses** with
  a clear reason — it never silently copies. Copy requires explicit per-generation
  opt-in.
- **Cross-drive selection**: selected lights span multiple volumes. The link kind
  is resolved per drive-scope from the settings pair (intra-drive default vs
  cross-drive default) and recorded per item; the cross-drive selector never offers
  `hardlink`. A capability-drift fallback (e.g., symlink privilege lost) is surfaced
  as a non-silent plan-time notice (FR-004b) — the app never silently copies or
  produces an unrecorded kind.
- **Missing / unresolved source frame at generation time**: surfaced (skipped and
  flagged); the whole view is not failed for one missing item unless the user
  chooses strict mode.
- **Moved or remapped library root**: sources resolve via root + relative path
  (Constitution I), not stale absolute paths.
- **Case-insensitive / case-preserving destination filesystem**: two sources
  differing only by case must not silently collide; the collision is a plan
  validation error (FR-009a/FR-017) that refuses the plan — never silently merged
  or suffixed.
- **Windows long paths (> 260 chars)**: surfaced as a plan warning/failure with a
  clear reason rather than a truncated or partially-created tree.
- **Destination path already exists as a user-owned file/folder**: surfaced in the
  plan; never silently overwritten (Constitution II — never overwrite silently).
- **Duplicate frame filenames across sessions**: each session links into its own
  directory by construction; where a profile layout aggregates across sessions the
  pattern MUST carry a session/night/setup token. Any residual same-path collision
  is a plan validation error (FR-009a) that refuses and points at the pattern —
  never a silent suffix.
- **Generated links treated as originals by cleanup**: prevented — link nature is
  recorded so spec 016/017 protection never offers a generated link as an original
  cleanup candidate.

## Requirements *(mandatory)*

### Functional Requirements

**Generation core (US1)**

- **FR-001**: The system MUST generate a project source view as a **reviewable
  filesystem plan** of link-creation actions; nothing is written to disk until the
  user approves and applies the plan through the spec 017/025 review→approve→apply
  pipeline.
- **FR-002**: The generated tree MUST contain **only** the project's selected light
  frames plus the calibration frames/masters matched to them; it MUST NOT include
  unselected or unmatched frames.
- **FR-003**: Generation MUST default to **link** materialization (symlink) and
  MUST NOT copy by default. Copy materialization MUST require an explicit
  per-generation user opt-in.
- **FR-004**: The link strategy MUST be resolved **deterministically at plan time**
  from a **settings pair** — a default link kind for **intra-drive**
  (source and destination on the same volume) and a default link kind for
  **cross-drive** (source and destination on different volumes) — applied per
  `(view × drive-scope)`. Each source item is classified by drive-scope, the
  matching default kind is chosen, and the resolved kind is **recorded per item**
  (`PreparedSourceViewItem.materialization`). The detected capability and the
  resolved kind(s) MUST be shown to the user before apply.
- **FR-004a**: The settings surface (settings pane **and** the generation dialog)
  MUST offer only **currently-valid** kinds so an invalid choice is impossible by
  construction: (a) the **cross-drive** selector MUST NOT offer `hardlink`
  (hardlinks cannot cross volumes by definition); (b) `symlink` options MUST be
  **greyed out with Developer Mode guidance** when symlink creation is unavailable
  on the platform (Windows without the symlink privilege), consistent with FR-004c.
  Because the saved settings are already capability-constrained, the plan-time
  fallback is a **rare edge path**, not a routine prompt.
- **FR-004b**: When, at plan time, a saved link kind is no longer achievable for a
  source's drive-scope (**capability drift** — e.g., Developer Mode revoked since
  the setting was saved, or the target volume is FAT/exFAT/SMB lacking support),
  the system MUST resolve the documented fallback for that drive-scope and surface
  a **non-silent plan-time notice** naming the affected items, the requested kind,
  and the applied fallback. It MUST NOT silently copy and MUST NOT silently produce
  an unrecorded kind.
- **FR-004c**: When symlink creation is unavailable on the platform, the symlink
  option MUST be **disabled (greyed out) with Developer Mode instructions** at
  selection time in both the settings pane and the generation dialog — it MUST NOT
  be a hard error raised only after the user selects it.
- **FR-005**: Every generated link MUST be recorded as an **app-created
  projection** (not original data) so inventory, cleanup, and protection
  (specs 016/017/048) never treat it as an original file.
- **FR-006**: The canonical database MUST remain the source of truth; the generated
  view MUST be recorded as a reproducible projection using the spec 026
  `PreparedSourceView` / `PreparedSourceViewItem` entities, with each item
  referencing its canonical source inventory item.
- **FR-007**: Every generation MUST emit **per-item audit records** (attempted
  action and outcome), consistent with Constitution II reviewable-mutation.

**Per-tool profile structure (US2)**

- **FR-008**: The view tree layout MUST be determined by the selected
  **workflow/processing profile** (spec 011). WBPP is one profile; the layout MUST
  be profile-driven, not hardcoded.
- **FR-009**: The tree grouping (e.g., WBPP: session/night → filter → exposure)
  MUST be expressed via the shared **token-pattern resolver** (crate `patterns`,
  spec 015) so grouping is configurable per profile rather than fixed in code.
  Each session's frames MUST link into their **own directory**; where a profile
  layout aggregates frames across sessions, the pattern MUST carry a
  session/night/setup token so collisions are impossible by construction (FR-009a).
- **FR-009a**: Destination-path collisions MUST be **impossible by construction**.
  If two source frames resolve to the same destination path under a layout (e.g.,
  an aggregating pattern missing a session/night/setup token), plan building MUST
  raise a **validation error** that refuses the plan and points at the offending
  pattern. The system MUST NOT apply a silent suffix, MUST NOT silently merge, and
  MUST NOT drop either frame.
- **FR-010**: Matched calibration frames/masters MUST be placed in the active
  profile's expected calibration location within the generated tree. Whether raw
  calibration frames or masters are linked MUST follow the **active profile and the
  resolved calibration match** — masters when the match resolved masters (spec 040),
  otherwise the matched raw calibration sets.
- **FR-010a**: Calibration matching MUST NOT be a prerequisite for generation. When
  a project has selected lights but no (or partial) calibration matches, the system
  MUST still generate the light-frame view and MUST surface a **plan-review warning
  "no calibration applied"** listing the unmatched light groups. Generation MUST NOT
  auto-run matching.
- **FR-011**: The system MUST NOT generate any WBPP/PixInsight configuration,
  process-icon, `.xpsm`, `.xosm`, or equivalent tool-control files — **only** the
  input image tree (Constitution III — PixInsight boundary).

**Regeneration & stale detection (US3)**

- **FR-012**: When the project's selected lights or calibration matches change, the
  system MUST be able to **regenerate** the view to reflect the current canonical
  state, producing a new reviewable plan and flagging any unresolved references.
- **FR-013**: View **removal**, **regeneration-after-removal**, and **stale
  detection** are owned by **spec 026**; this feature MUST reuse that machinery and
  MUST NOT duplicate it.

**Verify before processing (US4)**

- **FR-014**: Before a processing tool is launched against a generated view, the
  system MUST offer a **verification** that every link resolves to a present
  canonical source and report every broken, missing, or stale item.
- **FR-015**: Verification MUST be **read-only** (no filesystem mutation) and MUST
  NOT auto-repair; repair is via explicit regeneration (FR-012).

**Cross-platform safety (all stories)**

- **FR-016**: Generation MUST NOT overwrite an existing user-owned file or folder at
  a destination path; a collision MUST be surfaced in the plan and MUST NOT be
  silently clobbered.
- **FR-017**: On case-insensitive/case-preserving destination filesystems, two
  sources differing only by case MUST NOT silently collide; the collision MUST be
  surfaced as a plan **validation error** (per FR-009a) — refused, never silently
  suffixed or merged.
- **FR-018**: On Windows, destination paths exceeding the classic 260-character
  limit MUST be surfaced as a clear plan warning/failure rather than producing a
  truncated or partial tree.
- **FR-019**: A source frame that is missing or unresolved at generation time MUST
  be surfaced (skipped and flagged); generation MUST NOT link to a nonexistent
  target, and MUST NOT fail the whole view for a single missing item unless the
  user selects strict mode.
- **FR-020**: Sources MUST be resolved via library root + relative path
  (Constitution I) so moved or remapped roots are handled without stale absolute
  paths.
- **FR-021**: Generation MUST be permitted only for project lifecycle states
  consistent with spec 026 (FR-012 there); the plan MUST route through the spec
  017/025 review→approve→apply pipeline and MUST NOT bypass it.
- **FR-021a**: The generation plan MUST use a distinct plan origin
  `prepared_view_generation` (plan type `source_view_generation`), separate from
  spec 026's `prepared_view_regeneration`, so first-materialization is routed and
  audited distinctly from regeneration-after-removal (regeneration stays on spec
  026's origin).
- **FR-021b**: The generated view's destination MUST default to an app-owned
  project-envelope subfolder `<project>/source-views/<view>/` (spec 024). A
  **per-project override** MUST be persistable, and a **per-generation override**
  MUST be settable in the generation dialog. Destination overrides MUST still obey
  all cross-platform safety FRs (no silent overwrite, collision refusal, long-path
  handling).
- **FR-022**: A generated view's materialization kind MUST be **resolved
  deterministically per drive-scope** from the settings pair (FR-004) and
  **recorded per item**. This relaxes spec 026 FR-008's single-kind-per-view
  invariant to *deterministic kind per drive-scope, recorded* (spec 026 amended
  accordingly). The system MUST NOT produce an **unrecorded or non-deterministic**
  mixed-kind view: every item's kind is chosen by rule and stored, and any
  capability-drift fallback is surfaced non-silently (FR-004b).

### Key Entities *(include if feature involves data)*

- **Generated Source View** (`PreparedSourceView`, reused from spec 026): the
  canonical record of a generated projection for a project, with `kind`
  (`symlink | junction | copy`), `state`, per-item membership, and source
  inventory references. This feature adds the first-materialization (`current`)
  path.
- **Generated Source View Item** (`PreparedSourceViewItem`, reused from spec 026):
  one link in the view, carrying its canonical source inventory reference, its
  `view_relative_path` under the project workspace, and its recorded
  materialization kind.
- **View Generation Plan** (new `FilesystemPlan` variant): a reviewable plan whose
  actions are per-item link-creation (or, with opt-in, copy) resolved against
  current inventory paths, carrying the distinct origin `prepared_view_generation`
  (plan type `source_view_generation`); parallels spec 026's `ViewRegenerationPlan`.
- **Link-Kind Settings Pair** (spec 018 KV): two persisted settings —
  `source_view_link_kind_intra_drive` (default `hardlink`) and
  `source_view_link_kind_cross_drive` (default `symlink`) — that supply the
  deterministic per-drive-scope link kind resolved at plan time. Stored as flat
  `SettingsState` fields (no migration; spec 018 settings KV).
- **Filesystem Capability Result**: the destination/volume link-capability probe
  outcome (symlink/junction/hardlink availability, privilege, cross-volume
  constraints) used to constrain the settings/dialog choices and to detect
  capability drift at plan time.
- **Workflow Profile** (spec 011): selects the tree layout; WBPP is the first
  profile.
- **Layout Pattern** (spec 015 token pattern): the per-profile grouping expression
  (session/night, filter, exposure) resolved to destination relative paths.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can select a project's lights and generate a WBPP-ready tree in
  which 100% of the selected present lights and their matched calibration appear as
  links, with **0** originals copied by default.
- **SC-002**: The generated tree contains **0** tool configuration files (only image
  links and folders).
- **SC-003**: Applying a generation plan creates nothing outside the chosen
  destination and **0** silent overwrites of existing user files.
- **SC-004**: On a destination lacking symlink capability, the app resolves a
  documented fallback (junction/hardlink) or refuses with a clear reason in 100% of
  cases — it never silently copies.
- **SC-005**: After a selection or match change, regeneration produces a tree
  matching the new canonical selection with **0** dangling links applied and all
  unresolved references flagged.
- **SC-006**: Verify-before-processing reports every broken/missing link before
  launch; a view whose sources are all present verifies clean (0 false alarms).
- **SC-007**: **0%** of generated links are ever offered by cleanup as original data
  (100% recorded as projections).

## Assumptions

- The project already has a selection of light frames; this feature **consumes**
  calibration matches (specs 007/040), it does not compute them. Matching is **not**
  a prerequisite: when no (or partial) matches exist the light view is still
  generated and a "no calibration applied" warning lists the unmatched groups
  (FR-010a); matching is never auto-run.
- Per-frame selection granularity depends on spec **048** per-frame inventory;
  where per-frame records exist, selection and linking are per frame (missing
  frames excluded per 048 FR-009); otherwise selection falls back to session level.
- Filesystem plan application (link creation, per-item revalidation, pause/resume,
  audit) is provided by specs 025/017; this feature produces the plan, not the
  executor.
- Removal, regeneration-after-removal, and stale detection are provided by spec 026
  and reused unchanged.
- The generated view lives in the app-owned project envelope subfolder
  `<project>/source-views/<view>/` (crate `project/structure`, spec 024) by
  default; a per-project override is persistable and a per-generation override is
  settable in the dialog (FR-021b).
- WBPP is the first and only profile shipped with a defined layout; other tools'
  layouts are future profiles.

## Out of Scope

- **WBPP/PixInsight configuration generation** — no `.xpsm`/`.xosm`, process icons,
  weighting config, or any tool-control file. Only the input file tree.
- **Other tools' configuration files** (Siril scripts, planetary/lunar tool config,
  etc.).
- **Copying by default** — copy is an explicit per-generation opt-in only; the
  default and preferred path is links.
- **Image processing itself** — calibration, debayer, registration, integration,
  drizzle, stacking, or editing (Constitution III).
- **View removal, regeneration-after-removal, and stale-view detection** — owned by
  spec 026; reused here, not re-specified.
- **The filesystem plan executor / apply engine** — owned by specs 025/017.
- **Calibration matching** — owned by specs 007/040; consumed here.
- **The `hardlink`-as-primary strategy details and the settings UI for per-root
  behavior** beyond what the capability check needs.

## Clarifications

All nine open questions were resolved by the user on 2026-07-04 and folded into the
requirements above. Resolutions (traceability):

- **CL-1 (was OQ-1) — Generation plan origin.** Add a distinct
  `prepared_view_generation` plan origin (plan type `source_view_generation`) for
  first-materialization, separate from spec 026's `prepared_view_regeneration`;
  regeneration-after-removal stays on spec 026's origin. → FR-021a; requires
  migration `0061` to expand the `plans.origin` / `plan_type` CHECK constraints.
- **CL-2 (was OQ-2) — Per-drive-scope link kind (relaxes spec 026 FR-008).** A
  **settings pair** supplies the default link kind for **intra-drive** and for
  **cross-drive**, resolved deterministically at plan time per `(view × drive-scope)`
  and recorded per item, with a non-silent plan-time notice on capability-drift
  fallback. The settings UI is **capability-constrained** so invalid choices are
  impossible: the cross-drive selector never offers `hardlink`; symlink is greyed
  out with Developer Mode guidance where unavailable (settings pane and dialog).
  The plan-time fallback is therefore a rare drift-only edge path. → FR-004,
  FR-004a, FR-004b, FR-004c, FR-022. **This relaxes spec 026 FR-008 (single kind
  per view) → spec 026 amended** (deterministic kind per drive-scope, recorded).
- **CL-3 (was OQ-3) — No link kind available.** Refuse with a clear error; copy is
  an explicit per-generation opt-in only, never silent. → FR-003, FR-004b.
- **CL-4 (was OQ-4) — Raw vs masters.** Follow the active profile and the resolved
  calibration match — masters when the match resolved masters (spec 040), else the
  matched raw calibration sets. → FR-010.
- **CL-5 (was OQ-5) — Collisions.** Collisions are **impossible by construction**:
  each session links into its own directory; aggregating layouts MUST carry a
  session/night/setup token; any plan-time collision is a **validation error**
  (refuse, point at the pattern) — never a silent suffix. → FR-009, FR-009a, FR-017.
- **CL-6 (was OQ-6) — Destination.** Default `<project>/source-views/<view>/`
  (project envelope, spec 024); per-project override persisted; per-generation
  override in the dialog. → FR-021b.
- **CL-7 (was OQ-7) — Matching not a prerequisite.** Generate anyway; plan review
  shows a "no calibration applied" warning listing unmatched groups. → FR-010a.
- **CL-8 (was OQ-8) — Symlink capability.** Capability check; where symlinks are
  unavailable, grey out the symlink option in settings and dialog with Developer
  Mode instructions (not a hard error at selection time). → FR-004a, FR-004c.
- **CL-9 (was OQ-9) — Selection granularity vs spec 048.** Per-frame selection
  where 048 per-frame inventory exists (missing frames excluded per 048 FR-009);
  session-level fallback where per-frame records are absent.

## Cross-Spec Conflicts & Notes

- **Spec 026 (generation restored).** Spec 026 is marked *POSSIBLY OBSOLETE* on the
  premise that no generation path exists. This spec restores it (product decision
  2026-07-04). A cross-reference note is recorded in spec 026's spec.md. **Revert
  note:** if the product decision is reversed and generation is retired again, drop
  this spec and restore spec 026's obsolete banner.
- **Spec 026 FR-008 amended (CL-2).** The single-kind-per-view invariant is relaxed
  to *deterministic kind per drive-scope, recorded per item*. Spec 026's spec.md and
  data-model.md carry the amendment and a revert note. **Revert note:** to restore
  the strict single-kind invariant, re-tighten spec 026 FR-008 and change this
  spec's FR-004/FR-022 to refuse cross-drive-forced mixed views instead of resolving
  per drive-scope.
- **Spec 048 dependency**: per-frame selection depends on 048 per-frame inventory
  (CL-9). No conflict; this spec degrades gracefully to session-level selection.
