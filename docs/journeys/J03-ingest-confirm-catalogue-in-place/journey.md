---
id: J03
title: Catalogue an already-organized folder without moving files
version: 5
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [inbox-confirm, plans, sessions, audit]
interfaces: [desktop-ui, desktop-ui-macos]
trace:
  - pre-migration journey.md @ git 42c596d68b621a46e54e647fdb7c48716fdb68c1
  - docs/product/journeys/J03-ingest-confirm-catalogue-in-place/deltas/2026-07-14-jval-docdrift.md
  - PR #898 (framing-attribution backend applies identically on
    catalogue-in-place confirms)
  - docs/product/journeys/J03-ingest-confirm-catalogue-in-place/deltas/2026-07-14-q27-f5.md,
    2026-07-14-q27-f10.md (legacy pre-merge drafts; superseded by S2 below)
  - issue #943 (Inbox attribution picker — the UI caller for PR #898)
  - docs/development/windows-journeys/journey-11-framing-clustering-attribution.md
  - PR #938 (fixes #768 — destination-root picker was shown regardless of
    the item's source-root organization state)
  - PR #1740 (explicit approval before an inbox plan applies)
  - spec-054-adaptive-detail-dock (FR-001, FR-003, FR-005 — shared adaptive
    dock: Inbox uses the same side-≥1400px/bottom-below placement, per-page
    Auto/Bottom/Right override, and drag-resizable side width as every other
    list page; FR-014/FR-015's permanent Inbox split was never built and was
    withdrawn in #1068)
---

## Goal

The user has a light-frames (or similar) folder that is already organized
exactly the way they want it, and wants PlateVault to know about those files
— so they show up in Sessions, projects, and calibration matching — without
moving, copying, or rewriting a single byte on disk. Done means: the files
are indexed and visible in derived views, and the file set and content
hashes on disk are byte-for-byte unchanged from before confirm.

## Preconditions

- P1: A library root is registered with organization state **organized** —
  either chosen explicitly on the setup wizard's per-root Organized/
  Unorganized control (including when the wizard is reopened via
  Settings → Advanced → "Restart setup wizard", which prefills the same
  control for already-registered roots), or by registering a new non-inbox
  root via Settings → Data Sources' "Add" flow, which has no
  organization-state picker and defaults every non-inbox category to
  organized automatically (inbox-category roots always default to
  unorganized). The root contains files that are not yet in PlateVault's
  index.
- P2: The organized root's new files have been surfaced into the Inbox
  queue — for an **inbox-category** root this happens via Inbox's own
  "Rescan all roots"; for a non-inbox organized root, the user has run
  **Rescan** on that specific root from Settings → Data Sources (Inbox's
  "Rescan all roots" only reaches roots of category `inbox` and does not
  surface other roots' new files).

## Steps

### S1 — Classify files from the organized root {#S1}
- **Do:** The user opens Inbox and reviews an item whose files came from the
  organized root (surfaced per P2).
- **Expect:** The item classifies exactly as it would from any other root —
  a folder mixing frame types materializes as separate single-type items,
  and an item missing mandatory metadata (e.g. filter, target) shows the
  same needs-review gate with Confirm disabled until resolved, as in the
  move-mode journey. Reviewing the item's detail uses the same shared
  adaptive dock as move-mode (see J02/S2): side placement (full-height,
  drag-resizable, width persists) at ≥1400px window width, bottom dock
  below that, with the same per-page Auto/Bottom/Right override — and the
  detail's own file list scrolls within its own scroll region rather than
  overflowing the window, in either placement.
- **Expect (negative):** The Inbox detail for an organized-root item's
  file list is never cut off below the window in either placement — same
  scroll-region guarantee as move-mode (J02/S2).
- **Trace:** crates/app/inbox/src/confirm.rs (needs-review gate is
  root-agnostic); spec-054/FR-001, FR-003, FR-005 (see J02/S2 for the
  layout detail, shared across both ingest modes).

### S2 — Confirm the item {#S2}
- **Do:** The user clicks Confirm on the classified item.
- **Expect:** The response reports a move count of 0 and a catalogue count
  equal to the file count; every resulting plan action is "catalogue in
  place" (destination equals source, staying under the organized root); no
  destination-root picker is shown.
- **Expect (negative):** No destination-root picker appears — there is
  nothing to pick, since the files are staying where they are.
- **Expect (light-frame items — attribution pick):** The attribution
  picker described in J02/S5 appears here too. Candidates are ranked
  identically regardless of organized-vs-unorganized routing, and the
  pick persists framing/project membership at confirm time with zero
  filesystem I/O.
- **Trace:** crates/app/inbox/src/confirm.rs:293-303 (OrganizationState::
  Organized routes every file to the `catalogue` action);
  `apps/desktop/src/features/inbox/InboxDetail.tsx` `applicableRoots`
  (filtered by the item's `organizationState`, not just frame-type
  category, per PR #938 fixes #768 — previously the picker rendered
  whenever more than one applicable root existed for the frame type, even
  for an organized-source item, though any selection there was silently
  ignored server-side).
- **Trace:** `crates/app/inbox/src/attribution.rs` (`suggest_candidates`
  reads staged per-file metadata, so it is organization-state-agnostic);
  `crates/app/inbox/src/confirm.rs` (the apply-path runs before the
  catalogue/move dispatch). See J02/S5 for the UI trace. Issue: #943.

### S3 — Review the plan {#S3}
- **Do:** The user opens the plan review overlay ("Review plans (N)"), the
  same overlay used for move-mode plans.
- **Expect:** Each item reads as a catalogue action, still shows its
  (unchanged) path, and Escape/Discard both close without mutation.
- **Expect (negative):** The destructive-destination control (Archive vs.
  System Trash) is absent for a plan made entirely of catalogue actions —
  it only appears when at least one action in the plan requires destructive
  confirmation.
- **Trace:** apps/desktop/src/features/inbox/PlanPanel.tsx (destructive
  control rendered only when an action requires destructive confirm, per
  `PlanPanel.test.tsx: 'does NOT show the destructive control when no
  action requires destructive confirm'`).

### S4 — Apply the plan {#S4}
- **Do:** The user applies the plan (per-item or Apply-all).
- **Expect:** The catalogue action writes the files' identity and metadata
  into the library's index; an explicit success signal appears; the files
  become visible in Sessions; the apply is recorded in the Audit Log with
  outcome, exactly as a move-mode apply is.
- **Expect (negative):** The on-disk file set and content hashes for the
  organized root are unchanged byte-for-byte after apply — the executor
  performs no filesystem I/O for a catalogue action (it is a documented
  no-op). If a file under review changed on disk after confirm, the item is
  refused as stale rather than silently catalogued with outdated metadata
  (same staleness gate the shared apply pipeline uses for move plans).
- **Expect (negative):** A catalogue plan is not applied without an approval
  the user's own Apply gesture recorded: the same `plan.approval_required`
  refusal as move-mode (J02/S7) applies, so a plan that appeared after the
  render the user reviewed is refused rather than catalogued unseen.
- **Trace:** `crates/fs/executor/src/ops/catalogue_op.rs`;
  `crates/fs/executor/src/run/dispatch.rs:77`
  (`ExecutorItemAction::Catalogue`); `crates/app/core/src/plan_apply/` (the
  stale/`item.stale` gate applies ahead of per-action dispatch,
  action-agnostic); `crates/app/core/src/inbox_plan.rs:202-212` (approval is
  read, never minted); PR #1740.

### S5 — Mixed-root run routes independently per root {#S5}
- **Do:** In the same Inbox session, the user confirms one item sourced from
  an organized root and another item of the same frame type sourced from an
  unorganized (inbox) root.
- **Expect:** The organized-root item's plan is entirely catalogue actions;
  the unorganized-root item's plan is entirely move actions — the routing
  is decided purely by each file's source root's organization state, never
  by frame type or file kind.

## Success criteria

- SC1: For a confirm sourced entirely from an organized root, the confirm
  response reports `move_count == 0` and `catalogue_count == file_count`
  (S2).
- SC2: After apply, the organized root's file set and per-file content
  hashes are identical to their pre-apply values (S4).
- SC3: After apply, every applied file is visible in Sessions and has an
  Audit Log entry recording the catalogue outcome (S4).
- SC4: In a mixed-root run, catalogue-vs-move routing matches each item's
  source root's organization state 100% of the time, independent of frame
  type (S5).

## Known gaps

None specific to catalogue-in-place mode. The legacy doc's only referenced
gap (cross-plan overlap protection, tracked via Journey 2's shared
confirm/plan pipeline) shipped in PR #408 (merged 2026-07-04) and is
provably closed — dropped rather than carried forward.

## Delta log

- **Δ2** 2026-07-17 · S2 · behavior-change
  Confirm's backend attribution pass (framing/project matching,
  `chosenAttribution` apply-path) now runs identically on organized-root
  (catalogue-mode) confirms as on move-mode confirms — but no Inbox UI
  surfaces it either way yet.
  Evidence: PR #898 · by: journey-scribe (intent-gated)

- **Δ3** 2026-07-17 · S2 · behavior-change
  The destination-root picker is now correctly suppressed for an item
  sourced from an organized root even when more than one root would
  otherwise be applicable to its frame type — previously it rendered
  regardless of the item's organization state, and any selection there was
  silently ignored server-side (catalogue-mode confirms always resolve to
  the source root).
  Evidence: PR #938 (fixes #768) · by: journey-scribe (intent-gated)

- **Δ4** 2026-07-20 · S2 · behavior-change
  Confirm on a light-frame item shows the ranked attribution picker
  (J02/S5) before creating the catalogue plan, and the user's pick rides
  the same single confirm call. Catalogue-mode and move-mode confirms
  present the picker identically.
  Evidence: issue #943, spec-008 US7/FR-019/FR-022/SC-008 · by: rust-pro

- **Δ5** 2026-08-24 · S4 · behavior-change
  Applying a catalogue plan now requires an approval recorded by the user's
  own Apply gesture; an unapproved plan is refused with
  `plan.approval_required`. Previously the inbox apply use case minted its
  own approval, so any plan reachable by the command applied without one.
  Evidence: PR #1740 (c0f7e10ea) · by: journey-scribe (intent-gated)
