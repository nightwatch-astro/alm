---
id: J02
title: Move newly-arrived frames from an inbox drop folder into the library
version: 8
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [inbox-confirm, plans, audit]
interfaces: [desktop-ui, desktop-ui-macos]
trace:
  - pre-migration journey.md @ git 42c596d6
  - deltas/2026-07-14-jval-docdrift.md
  - PR #938 (fixes #557 — Inbox page infinite render loop)
  - PR #939 (fixes #552, #569, #553, #554 — mixed-folder banner copy,
    scrollable detail body, missing-attribute banner placement)
  - PR #898 (framing-attribution backend: ranked attribution candidates +
    chosenAttribution apply-path at Inbox confirm)
  - issue #943 (`inbox.attribution.suggest` + Inbox attribution picker —
    the UI caller for PR #898's backend)
  - docs/product/journeys/J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q27-f5.md,
    2026-07-14-q27-f6.md, 2026-07-14-q27-f10.md (legacy pre-merge drafts;
    superseded by S5 below)
  - docs/development/windows-journeys/journey-11-framing-clustering-attribution.md
  - e2e-agentic-test/041-inbox-plan-surface/mixed-folder-single-type-subitems/scenario.md
  - e2e-agentic-test/041-inbox-plan-surface/missing-mandatory-gate/scenario.md
  - e2e-agentic-test/041-inbox-plan-surface/reclassify-field-agnostic/scenario.md
  - e2e-agentic-test/041-inbox-plan-surface/confirm-move-vs-catalogue/scenario.md
  - e2e-agentic-test/041-inbox-plan-surface/plan-overlay-apply-audit/scenario.md
  - e2e-agentic-test/025-filesystem-plan-application/plan-overlap-guard/scenario.md
  - e2e-agentic-test/journeys/grand-inbox-journey/scenario.md
  - spec-058-inbox-drop-parent-items (D-001, D-006, D-007, FR-001–FR-009,
    FR-015–FR-017, FR-023, FR-025, SC-001–SC-004, SC-010 — a scanned folder
    produces no placeholder inbox item; source-group row before classification,
    N item rows after, no aggregate row ever)
  - specs/058-inbox-drop-parent-items/sc-009-boundary.md (SC-009 is knowingly
    NOT delivered by spec 058; the supersession mechanism lands in
    specs/tiny/reclassify-split-per-item-and-rederivation.md, PR #1097)
  - PR #1740 (explicit approval before an inbox plan applies)
  - PR #1724 (one pattern token renders one folder level)
  - spec-054-adaptive-detail-dock (FR-001, FR-003, FR-005 — shared adaptive
    dock: Inbox uses the same side-≥1400px/bottom-below placement, per-page
    Auto/Bottom/Right override, and drag-resizable side width as every other
    list page; FR-014/FR-015's permanent Inbox split was never built and was
    withdrawn in #1068)
---

## Goal

The user has raw frame files sitting unorganized in an inbox drop folder and
wants them safely relocated into the registered light-frames library, with
any missing per-frame metadata resolved before anything moves. "Done" is:
every file that started in the inbox now lives at its computed destination
path under a registered library root, no file was moved without an
explicit, reviewed plan, and the action is visible in the audit history.

## Preconditions

- P1: At least one inbox root and at least one registered light-frames
  library root exist.
- P2: One or more files sit under the inbox root, not yet scanned.

## Steps

### S1 — Rescan the inbox and see items split by type {#S1}
- **Do:** Trigger a rescan of the inbox.
- **Expect:** A newly scanned folder appears in the queue immediately, as a
  single **source-group row** — the folder itself, not an inbox item. That row
  is structurally non-confirmable: it has no frame type, no classification, no
  confirm affordance, and states nothing about its files beyond what scanning
  alone established. Once classification runs on that folder, its source-group
  row is replaced by exactly N **item rows**, one per detected frame-type
  group: one row for a uniform folder, N rows for a folder whose files mix
  frame types (e.g. `light · Ha · 300s`, `light · Ha · 120s`, `dark · 300s`).
  Every row states only facts about its own files. Siblings from one folder
  remain identifiable as a set, and grouping the list by folder shows them
  together under one header; grouping by target or frame type nests them
  correctly. The inbox badge, the status-bar breakdown, and the number of
  visible rows agree by construction, because there is no additional row to
  reconcile away.
- **Expect (negative):** Scanning never creates an inbox item representing a
  folder as a whole — no placeholder, not even transiently, and no aggregate
  row survives classification alongside the item rows it produced. No queue
  item is shown as an undifferentiated "mixed" type. No sibling in a split set
  is primary or distinguished; none carries the others' lifecycle. The badge
  never disagrees with the number of visible queue rows.
  Opening the Inbox page and leaving it open never spins the page in a runaway
  re-render loop (previously the page re-rendered continuously the entire
  time it was open, driven by an unstable page-status node identity and a
  freshly-allocated empty-items array while the item-list query was
  unresolved).
- **Expect (negative):** Replacing a source-group row with its item rows never
  silently drops the user's selection.
- **Trace (stability):** `apps/desktop/src/features/inbox/InboxPage.tsx`
  (`useSetPageStatus` call site, `listData?.items ?? []`); PR #938 fixes
  #557.
- **Trace (058):** spec-058 FR-001/FR-004 (no frame-type-less item ever
  created or exposed), FR-002/FR-003 (1 row for uniform, N for N groups),
  FR-005/FR-006 (sibling set, no primary), FR-015/FR-016/FR-017 (scan creates
  the source group; classification replaces its row), FR-023 (selection
  continuity), FR-025 (group-by-folder), D-001/D-006/D-007. **Not yet
  implemented as of `38227ca3`** — see G2.

### S2 — Inspect an item's per-file detail {#S2}
- **Do:** Select a queue item and open its detail.
- **Expect:** Inbox uses the same shared adaptive dock as every other list
  page (see J09/S3): the item list is the page's primary full-width
  content, with a long item name truncated and its full name available via
  a tooltip on hover/focus, and the detail (per-file metadata, inspector,
  and the plan surface) docks to the SIDE (full-height, drag-resizable,
  width persists across an app restart) when the window is ≥1400px wide,
  and to the BOTTOM below that width. A per-page Auto/Bottom/Right override
  (see J10/S11) pins the placement regardless of window width; Auto follows
  the automatic width rule. The detail body (property tables, mixed-summary line, Files popover trigger,
  needs-review controls) is its own scroll region within that full-height
  right pane, so content taller than the pane scrolls into view rather than
  being clipped by the pane's outer overflow. The detail shows the same
  per-file metadata (frame type, filter, exposure, binning, gain,
  temperature, target, date) that the needs-review gate (S3) computes over;
  the file count shown on the list row and the file count shown in the
  detail agree. Each field is distinguishable as real data (with a source
  pill), an unresolved missing-but-applicable value (chip, no source pill),
  or a not-applicable value (blank/"—", no chip) — never a bare `0`/blank
  standing in for a missing value
  (`apps/desktop/src/components/RenderValue.tsx`, `InboxDetail.tsx` field
  wiring). There is no residual "mixed" parent-folder row to inspect: after
  classification a folder is present only as its N item rows, so the advisory
  banner explaining an already-performed split has nothing left to attach to
  and is gone along with the row. Selecting a source-group row (a folder
  scanned but not yet classified) opens a detail that describes the folder —
  its path and file count — and offers classification, not confirmation. The
  detail continues to track the item the user
  selected even if the user changes the search text or an active filter
  afterward.
- **Expect (negative):** Changing search or filter text never silently
  re-targets the open detail panel to a different item. If per-file
  metadata fails to load, the detail shows an explicit error state rather
  than an empty or stale one, and Confirm stays disabled. The source
  folder is NOT revealable from this detail today — `nativeReveal` is
  wired only into the Sessions feature, not Inbox; corrected from the
  legacy doc's unconditional reveal claim. Below 1400px window width the
  Inbox detail renders as a bottom dock, same as any other adopting page —
  that is expected, not a defect. At any width, its per-file list is never
  cut off below the window — this was a real, previously-observed defect
  (the file list overflowed past the viewport and its bottom rows were
  unreachable) that the detail body's own scroll region
  (`.pv-inbox-detail__scroll`) fixes, independent of side/bottom placement.
- **Trace:** `apps/desktop/src/components/RenderValue.tsx`,
  `apps/desktop/src/features/inbox/InboxDetail.tsx` (renderer wiring,
  `.pv-inbox-detail__scroll` sole scroll region per PR #939 fixes #553;
  the mixed-folder banner copy fixed by PR #939 (#552, #569) is retired by
  spec-058 along with the parent row that carried it);
  `apps/desktop/src/features/sessions/revealInventory.ts` (reveal is
  Sessions-only — no `nativeReveal` call anywhere under
  `features/inbox/`); `apps/desktop/src/ui/useAdaptiveDock.ts`,
  `apps/desktop/src/features/inbox/InboxPage.tsx` (passes no
  `detailPlacement` to `ListPageLayout`, so it inherits the default
  `'adaptive'` placement — same mechanism as every other list page);
  spec-054/FR-001 (adaptive side/bottom placement), FR-005
  (resizable/persistent side width). FR-014/FR-015 (a distinct, permanent
  Inbox split) were never delivered and were withdrawn in #1068: the FILES
  reachability they existed to fix was already solved by PR #939, which made
  the detail body its own scroll region.

### S3 — Resolve missing metadata via bulk reclassify {#S3}
- **Do:** For an item flagged as needing review (missing a mandatory
  attribute for its frame type — most commonly filter for lights, or
  target when there is no filter and no coordinates), select the affected
  files and set the missing value (frame type, filter, exposure, or
  binning) in one action.
- **Expect:** The needs-review item shows a banner naming exactly what is
  missing, placed inline in the detail's Files column right below the
  per-file popover trigger it explains (not as a separate trailing
  full-width alert column), and affected rows carry a "needs `<attribute>`"
  badge; Confirm is disabled while unresolved. Applying a value to a selection of
  affected (missing-value) files applies to the whole selection in one
  call, reported as an applied count. Once every file in the item has the
  missing value, the item re-partitions into a clean single-type item and
  Confirm re-enables automatically. The override is visible with its
  provenance (a source pill distinguishing it from a FITS-derived value)
  and a reset path, and it survives a later rescan.
- **Expect (negative):** Resolving a missing value never rewrites the
  source file's bytes — only PlateVault's own index changes. Attempting to
  confirm an unresolved item is rejected independently of the UI: a direct
  confirm request against that item fails with a typed
  `inbox.missing_path_attributes` error.
- **Trace:** Removed the legacy claim "a selection spanning different
  detected types warns before overwriting" — no corroborating warning UI
  or backend check found (`InboxDetail.tsx` `handleBulkApply`/
  `handleSelectAll`, `crates/app/inbox/src/reclassify.rs`, and the
  `reclassify-field-agnostic` e2e scenario all apply uniformly with no
  type-mismatch check); carried unverified from the legacy doc's
  validation checklist into an unconditional Expect by the migration.
  "Reset path" is UNVERIFIED (not corrected): a backend primitive exists
  (`set_manual_override_reset_stale`,
  `crates/persistence/db/src/repositories/q_inbox.rs`) but no UI control
  invoking it was located — see report. Banner placement (Files-column,
  inline) per PR #939 fixes #554.

### S4 — Choose a destination library root, when more than one applies {#S4}
- **Do:** If more than one registered library root can receive the item's
  frame type, pick one from the item's destination-root control (default:
  Auto).
- **Expect:** With exactly one valid root, the picker is not shown and
  that root is used automatically. With two or more valid roots, the
  control lists each as `<folder name> · <category>` and defaults to
  Auto. The choice arms only the selected item — selecting a different
  item returns the control to Auto for that item, and the value shown
  always equals the root a confirm would actually use.
- **Expect (negative — corrected):** Two registered roots that share the
  same last path segment (e.g. `D:\Astro\library` and `E:\Backup\library`)
  are NOT told apart in the picker — the option label is
  `basename(path) · category` only, with no path-disambiguation logic
  (`apps/desktop/src/features/inbox/InboxDetail.tsx`, destination-root
  `<select>` around the `applicableRoots.map` block). The legacy/migrated
  claim that same-named roots "are still told apart" is corrected; treat
  this as a candidate product gap (see report).

### S5 — Confirm a classified item into a plan {#S5}
- **Do:** Confirm a fully-classified (not needs-review) item.
- **Expect:** Confirm turns the item into a reviewable plan; it never
  moves a file by itself. If no destination root was pre-selected in S4
  and more than one valid root exists, confirm still proceeds, a toast
  tells the user to choose a destination, and the root choice surfaces
  inside the plan review surface (S6) rather than an inline dialog at
  confirm time. Once a plan exists, the item stays visible in the queue,
  now marked "planned" — it does not disappear from the list.
- **Expect (negative):** No file on disk changes as a result of Confirm
  alone.
- **Expect (light-frame items — attribution pick):** Confirm on a
  light-frame item first shows a ranked list of attribution suggestions
  matching the item against existing framings and projects: add to a
  framing, start a new framing (including a mosaic project's first new
  panel), add to a project flagged with an optic-train mismatch, or start
  a new project. A candidate whose project is completed carries a reopen
  warning about archived raw subs. A "Leave unassigned" option is always
  offered. Picking an option and choosing "Confirm with this attribution"
  produces the plan and persists framing/project membership in the same
  single confirm call. Cancelling dismisses the list without creating a
  plan.
- **Expect (negative):** No option is preselected, and no plan is created
  while the list is on screen. Membership is written only for the option
  the user picked.
- **Trace:** `crates/app/inbox/src/attribution.rs`
  (`suggest_candidates`, `apply_chosen_attribution`),
  `apps/desktop/src-tauri/src/commands/inbox.rs`
  (`inbox_attribution_suggest`),
  `apps/desktop/src/features/inbox/AttributionPicker.tsx`,
  `apps/desktop/src/features/inbox/InboxPage.tsx` (`handlePickAttribution`).
  Issue: #943.

### S6 — Review the plan before anything touches disk {#S6}
- **Do:** Open the plan review surface (e.g. via a "Review plans (N)"
  control) for one or more planned items.
- **Expect:** Every plan item shows its action and its source and
  destination path in full. Closing the review (Escape or Discard) causes
  no mutation. A pending destination-root choice from S5 is resolvable
  from inside this surface.
- **Expect (negative):** Nothing under the inbox or the destination root
  changes as a result of opening or discarding this review.
- **Trace (correction):** dropped "any protection status" — the Inbox
  move-plan review surface is `PlanApprovalOverlay`/`PlanPanel.tsx`
  (`apps/desktop/src/features/inbox/`), which has no protection-status
  rendering at all. The generic `PlanReviewOverlay`
  (`apps/desktop/src/features/plans/PlanReviewOverlay.tsx`) that DOES
  gate on protection (spec-016 `PlanProtectionGate`) explicitly documents
  itself as NOT used by Inbox: "Feature-specific bulk surfaces (the
  inbox multi-plan PlanApprovalOverlay) predate it and remain separate."
  Protection is a source/archive-plan concept (spec-016); it does not
  currently apply to inbox move-mode plans.

### S7 — Apply the plan {#S7}
- **Do:** Apply one plan item, or apply all reviewed plan items.
- **Expect:** Applying reports an aggregate outcome (a toast naming the
  applied count and, when any item failed, the failed count) for both
  single-item and apply-all/apply-selected flows. Files move to the path
  resolved from the per-frame-type folder pattern (e.g.
  `{target}/{filter}/{date}/light/`).
- **Expect:** One pattern token always renders exactly one folder level, no
  matter what the metadata says: a `FILTER` of `Ha/OIII` or an `OBJECT` of
  `M42/Trapezium` lands in a single folder whose name contains no separator,
  so the applied path has the depth the reviewed plan showed. A target name
  carrying a Greek letter (`α Centauri`) resolves to its own folder rather
  than falling back.
- **Trace (paths):** `crates/safe-filename/src/lib.rs:105` (`/` is
  substituted like every other reserved character), `:210-220`
  (`EmptyAfterSanitize` instead of an empty segment); PR #1724.
- **Expect:** The Apply gesture stays one click: applying records the user's
  approval for exactly the plans that were on screen, then applies them.
- **Expect (negative):** A plan whose source file changed on disk since it
  was confirmed refuses to apply rather than silently applying an outdated
  action list. A destination collision is refused rather than silently
  overwritten.
- **Expect (negative):** No inbox plan is ever applied without an approval
  the user's own gesture recorded: a plan carrying no approval is refused
  with `plan.approval_required` and nothing moves. A plan that appeared after
  the render the user reviewed is therefore refused rather than applied
  unseen — in an apply-all it is reported in the failed count, not moved.
- **Trace (approval):** `crates/app/core/src/inbox_plan.rs:202-212` (reads
  the recorded approval, never mints one; `:56-59`
  `ErrorCode::PlanApprovalRequired`);
  `apps/desktop/src/features/inbox/useInboxPlanApplyFlow.ts:97`, `:131-133`
  (batch handlers approve the displayed plans only); the mkdir-only
  `plans::auto_apply` path remains the one documented auto-apply exception;
  PR #1740.
- **Trace (correction):** the backend apply response
  (`InboxPlanApplyResult { inboxItemId, planId, state, error }`) carries a
  per-item error, but no inbox UI code renders it per item — only an
  aggregate count reaches the user
  (`apps/desktop/src/features/inbox/InboxPage.tsx`
  `handleApplySelected`/`handleApplyAll`). Corrected from "a failed item
  is identifiable by name with a reason." Also dropped the unverified
  "View session" post-apply link claim — no such affordance found in
  `PlanPanel.tsx`/`PlanApprovalOverlay.tsx`/`usePlanApplyProgress.ts` or
  the message catalog. Stale-source refusal is confirmed via the executor
  CAS check (`crates/fs/executor/src/ops/cas_check.rs:43`, called from
  `run/loop_.rs:467`); destination-collision refusal is confirmed via
  `FailureCode::ConflictDestinationExists`
  (`crates/fs/executor/src/failure.rs:63`, `:113`, `:178` — a destination that
  already exists is an item failure, never an overwrite).

### S8 — Verify the applied outcome {#S8}
- **Do:** Return to the inbox queue and to the audit history after an
  apply.
- **Expect:** The inbox badge, the "Confirm all (N)" counter, and the
  status-bar breakdown all decrement consistently for the moved files. The
  applied action, and any refused destination collision, appear in the
  audit history with their outcome.

## Success criteria

- SC1: After S1, a classified source folder mixing frame types produces
  exactly N single-type queue items (N = distinct detected type/setting
  combinations in that folder) and zero aggregate rows — not 1 mixed item,
  and not N items plus a folder row.
- SC2: An item missing a mandatory attribute cannot be confirmed through
  either the UI (Confirm disabled) or a direct confirm request (typed
  `inbox.missing_path_attributes` rejection) — S3.
- SC3: Every file present in the inbox before S1 is, by the end of S7,
  either present at its resolved destination path under a registered
  library root, or still queued, with the backend apply response
  recording a per-item failure reason even though the current UI only
  surfaces an aggregate applied/failed count (S7).
- SC4: Every successful apply in S7 and every refused destination
  collision in S7 has a matching row in the audit history (S8).
- SC5: A plan whose source file changed after confirm is refused at apply
  time, never silently applied (S7).
- SC6: A folder scanned but not yet classified is visible in the queue as
  exactly one row, and that row cannot be confirmed (S1).
- SC7: Confirming one sibling of an N-way split leaves the other N−1 unchanged
  in state, classification, and plan binding (S5).

## Known gaps

- G1: (dissolved 2026-07-15) — tracked as issue #880; registry editor UI exposes only common fields.
- G2: The spec-058 behaviour described in S1, S2, SC1, SC6 and SC7 is the
  SPECIFIED end state and is **not yet implemented** as of `38227ca3`
  (2026-07-20); phases 2–4 are in flight in a parallel lane. What still holds
  on that commit: scanning creates a folder placeholder inbox item
  (`apps/desktop/src-tauri/src/commands/inbox.rs::persist_folder_placeholder`),
  the placeholder is hidden from the queue by a read-side suppression
  predicate (`crates/persistence/db/src/repositories/inbox.rs::exclude_split_placeholder!`,
  which spec-058 SC-007 deletes), and no source-group row is rendered — the
  Inbox list has no source-group row type
  (`apps/desktop/src/features/inbox/InboxPage.tsx` uses `sourceGroupId` only
  as a detail key/prop). Validate S1/S2 against this gap, not against the
  journey body, until spec 058 lands.
- G3: SC-009 of spec 058 (a superseded sibling's open plan is blocked and the
  user gets an explicit superseded signal) is **knowingly not delivered by
  spec 058** and is therefore deliberately absent from this journey. It lands
  with `specs/tiny/reclassify-split-per-item-and-rederivation.md` (PR #1097).
  See `specs/058-inbox-drop-parent-items/sc-009-boundary.md`. Until then, a
  folder-wide reclassify refusal survives when any sibling has an open plan —
  that refusal is intended, not a defect.

## Delta log

- **Δ2** 2026-07-17 · S5 · behavior-change
  Confirm's backend now computes ranked framing/project attribution
  candidates for light-frame items and accepts a `chosenAttribution` pick
  that persists membership at confirm time — but no Inbox UI surfaces
  either the candidates or the pick, so nothing changes on screen for this
  step yet.
  Evidence: PR #898 · by: journey-scribe (intent-gated)

- **Δ3** 2026-07-17 · S1, S2, S3 · behavior-change
  The Inbox page no longer runs a continuous re-render loop while open
  (PR #938 fixes #557). The stale "mixed" parent-row banner (S1's known
  `#549` residual-row case) no longer promises a Confirm-triggered split
  that cannot happen — it now describes the automatic single-type-item
  split that already occurred (PR #939 fixes #552, #569). The detail body
  is now its own scroll region instead of being clipped by the docked
  panel's overflow (PR #939 fixes #553). The missing-required-attribute
  banner (S3) now renders inline in the Files column, below the popover
  trigger it explains, instead of as a separate trailing alert column (PR
  #939 fixes #554).
  Evidence: PR #938 (fixes #557), PR #939 (fixes #552, #553, #554, #569) ·
  by: journey-scribe (intent-gated)

- **Δ4** 2026-07-19 · S1 · behavior-change
  A folder row whose files resolve to no frame type no longer reports
  "classified" in the Type column. It now reads "unclassified", agreeing
  with what the detail panel already said about that same item — the list
  badge is read from the item's own cached classification instead of from
  its scan state, which is set to "classified" for every scanned folder
  regardless of the result.
  Evidence: commit 4a96389b (fix(inbox): list badge no longer reports
  Classified for an unsplit folder), issue #711 Instance A · by:
  journey-scribe (intent-gated)

- **Δ5** 2026-07-20 · S5 · behavior-change
  Confirm on a light-frame item shows a ranked attribution picker before
  creating the plan. The user's pick rides the same single confirm call
  that creates the plan, so framing/project membership is persisted at
  confirm time. Nothing is preselected and nothing is merged without a
  pick. A read-only `inbox.attribution.suggest` command supplies the
  candidates; reading them from the confirm response was unusable, because
  that confirm's plan blocks any second confirm on the item.
  Evidence: issue #943, spec-008 US7/FR-019/FR-022/SC-008 · by: rust-pro

- **Δ6** 2026-07-21 · S1, S2, SC1, +SC6, +SC7, +G2, +G3 · behavior-change
  A scanned folder no longer produces a placeholder inbox row. Before
  classification the folder appears as a source-group row that is structurally
  non-confirmable; after classification it becomes exactly N item rows, one
  per frame-type group, with no aggregate row alongside them and no hidden row
  to suppress. Every row states only facts about its own files, so the badge,
  the status-bar breakdown and the visible rows agree by construction. The
  residual "mixed" parent row and its advisory banner are gone. SC-009's
  supersession signal is explicitly NOT part of this change (G3).
  Evidence: spec-058-inbox-drop-parent-items (D-001/D-006/D-007,
  FR-001–FR-009, FR-015–FR-017, FR-023, FR-025) ·
  by: journey-scribe (intent-gated) · IMPLEMENTED on
  spec/058-inbox-drop-parent-items (PR #1194); Layer-3 journeys green in CI
  on both Linux and Windows

- **Δ7** 2026-08-24 · S7 · behavior-change
  Applying an inbox plan now requires an approval recorded by the user's own
  gesture: the Apply / Apply all controls approve exactly the plans they
  displayed and the backend refuses an unapproved plan with
  `plan.approval_required`. Previously the apply use case minted its own
  approval, so a plan that appeared after the render was applied unseen and
  the audit record credited the approval to the code.
  Evidence: PR #1740 (c0f7e10ea) · by: journey-scribe (intent-gated)

- **Δ8** 2026-08-24 · S7 · behavior-change
  A metadata value containing `/` no longer adds a folder level to the
  applied destination — one token renders one segment — a dots-only literal
  segment no longer vanishes, and a Greek-letter target name such as
  `α Centauri` is accepted instead of being rejected as mixed-script.
  Evidence: PR #1724 (7b6867f04) · by: journey-scribe (intent-gated)
