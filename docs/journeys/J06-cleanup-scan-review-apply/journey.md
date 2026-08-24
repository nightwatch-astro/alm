---
id: J06
title: Reclaim disk space from processing outputs and raw sub-frames without losing anything protected
version: 4
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [cleanup, projects, sessions, plans]
interfaces: [desktop-ui]
trace:
  - pre-migration journey.md @ git 66026463
  - deltas/2026-07-14-jval-docdrift.md (folded — PR #413 status verified)
  - deltas/2026-07-14-q15-t123.md (superseded by current code — see G2)
  - spec-017 WP-E (project-level cleanup review flow)
  - spec-048 US3 (session-scoped raw sub-frame cleanup)
  - spec-025 FR-004 (destructive-confirm apply gate)
  - docs/development/journey-run-2026-07-14.md (Journey 6 section — live-app
    validation, build 7e522c16; project-level flow only, S5/S6 not exercised)
  - docs/development/windows-journeys/journey-06-cleanup-scan-apply.md
  - PR #413 (merged 2026-07-04 — scan/review/generate cleanup UI)
  - issue #741, issue #807, issue #766, issue #780, issue #806 (all open)
  - PR #894 (fixes #563)
  - PR #1190 (design-refresh handoff 06 — destructive-red token +
    Approve & apply variant scoped to delete-only plans)
---

## Goal
An astrophotographer wants to reclaim disk space from processing outputs a
project no longer needs (intermediates superseded by masters/finals) or from
raw light/dark/flat/bias sub-frames a session no longer needs, without ever
having a protected file deleted or moved without an explicit, reviewed
decision. "Done" is: the reclaimed files are gone from their original
location — moved to the chosen destination where cleanup policy assigned
them Archive, permanently removed where policy assigned them Delete — and a
re-scan confirms the candidate is no longer offered, with nothing protected
ever touched without an acknowledged, reviewed step.

## Preconditions
- P1: A project exists with processing outputs of mixed kind (intermediate,
  master, final) already recorded, OR a session exists with raw sub-frames
  already recorded in inventory.
- P2: Protection categories/policy are configured (defaults apply if the
  user has not customized them) so scans can classify candidates as
  protected or not.

## Steps

### S1 — Scan a project's outputs for cleanup candidates {#S1}
- **Do:** From a project's Outputs/Cleanup section, run "Scan for cleanup
  candidates."
- **Expect:** A read-only preview lists candidates grouped by kind
  (Intermediates/Masters/Finals) with per-item size and confidence; protected
  items are shown locked with no selection affordance; a total reclaimable
  size is shown for the current candidate set. Scanning a project with no
  candidates shows a clear empty result instead of an empty table.
- **Expect (negative):** No plan is created and no file on disk is moved,
  renamed, or deleted by scanning; running the scan twice in a row on an
  unchanged project returns the same grouping and total.
- **Trace:** `apps/desktop/src/features/projects/OutputsCleanupSections.tsx`,
  `crates/app/core/src/cleanup_generator.rs`

### S2 — Choose a destination and generate the plan {#S2}
- **Do:** Pick a destructive destination — Archive folder (default) or
  System trash — then click "Generate cleanup plan."
- **Expect:** A real, reviewable plan is created 1:1 with the candidates in
  scope; the chosen destination is fixed at this point and shown read-only
  in the review overlay from here on.
- **Expect:** The destination governs only the items cleanup policy assigned
  Archive. An item whose policy is Delete is permanently removed under
  either choice (S4).
- **Expect (negative):** Nothing on disk is touched by generating the plan;
  the destination cannot be changed after generation without discarding and
  restarting.
- **Trace:** `apps/desktop/src/features/projects/cleanupStore.ts`

### S3 — Review the plan {#S3}
- **Do:** Open the review overlay that follows plan generation.
- **Expect:** Every item in the plan is listed 1:1 with the generated plan;
  if any protected item is included, its protection must be explicitly
  acknowledged (per item) before "Approve & apply" becomes clickable;
  "Approve & apply" is also disabled whenever the plan holds zero items;
  choosing "Discard" leaves disk untouched and returns cleanly. "Approve &
  apply" (`plan-review-approve-apply`) renders in the app's red destructive
  style exactly when some plan item's action is `delete` — that is, when
  cleanup policy assigned Delete to that item's data type. The destination
  chosen at S2 does not affect this style: a System-trash plan's items carry
  action `archive` and are rerouted to the trash only at apply time, so such
  a plan renders in the neutral primary style even though applying it
  removes the files from their original location (G6).
- **Expect:** A plan carrying an item whose action is `delete` also shows a
  destructive-confirm checkbox (`plan-review-confirm-destructive`, labelled
  "I confirm these items may be deleted."). "Approve & apply" stays disabled
  until it is checked, and checking it persists the confirmation per item so
  it survives closing and reopening the review.
- **Expect:** A System-trash plan shows neither the red destructive style
  nor that checkbox, because both are keyed on item action `delete` rather
  than on the plan's destination (G6).
- **Expect (negative):** No item whose action is `delete` is ever applied
  without a recorded confirmation: the executor refuses such an item with
  `destructive_unconfirmed` and marks it `refused` rather than deleting it.
- **Expect (negative):** "Approve & apply" stays disabled while any protected
  item's acknowledgement is outstanding, or while the plan holds zero items —
  in both cases the overlay shows no explanatory text, only the disabled
  control (no "this plan is empty" or similar message). A zero-item plan
  cannot actually be produced by either flow in the first place: the project
  flow's Generate control does not render unless S1's scan found candidates,
  and the session flow's Generate (S6) is disabled while no frame is
  selected — so this overlay state is unreachable via the documented S1–S4 /
  S5–S6 path; the server-side rejection is defense-in-depth only.
- **Trace:** `PlanReviewOverlay` (`hasDestructiveItems` drives both the
  button variant and the confirm gate; approve is disabled on
  `plan.itemsTotal === 0` and the cleanup flow passes no `emptyReason`, so
  no message renders for that case); `PlanProtectionGate`;
  `plans::approve::approve_plan` (rejects a zero-item plan with
  `plan.items.empty`, not reachable via the shipped UI); contract operation
  `plans.confirm.destructive` → `confirm_plan_destructive_items`
  (`plan_items.destructive_confirmed`); PR #1190

### S4 — Approve and apply {#S4}
- **Do:** Click "Approve & apply" on a plan whose destination is Archive and
  that contains no protected item, checking the destructive-confirm box
  first if the plan carries any item whose action is `delete` (see Known
  gaps for the Trash-destination and protected-item cases).
- **Expect:** Live per-item progress is shown ("Applying N of M…"); each
  item's outcome (succeeded/failed with reason) is visible afterward; the
  moved files are present at the Archive destination; re-scanning the
  project afterward shows the applied items gone from the candidate list.
- **Expect:** An item whose cleanup policy is Delete is permanently removed
  from disk even though the destination is Archive folder: the destination
  reroutes only policy-Archive items, so a policy-Delete item is removed
  outright with no archive copy and no rollback, and its file is not present
  at the Archive destination afterwards (G7).
- **Expect (negative):** The overlay never reports a plan as fully applied
  while any item's outcome is unknown; a failed item's reason is shown
  rather than a silent skip.
- **Expect (negative):** No policy-Delete item is removed without the
  destructive confirmation from S3 recorded against it.
- **Trace:** `crates/app/core/src/plan_apply.rs`,
  `crates/fs/executor/src/run.rs`

### S5 — Scan a session's raw sub-frames for cleanup candidates {#S5}
- **Do:** From a session's detail view, run the raw sub-frame cleanup scan.
- **Expect:** A read-only preview lists individual light/dark/flat/bias
  frames with type, size, and protection state; non-protected frames are
  preselected; protected frames show no selection control; the reclaimable
  total reflects only the currently selected frames. A per-root protection
  override set on a source (Settings → Data Sources) now actually governs
  this classification for the frames it owns: a root marked "Unprotected"
  correctly preselects its session-attributed frames as non-protected,
  rather than the override being silently ignored in favor of the global
  default.
- **Expect (negative):** No file is moved or altered by scanning.
- **Trace:** `apps/desktop/src/features/sessions/RawFrameCleanupSection.tsx`;
  `crates/app/core/src/cleanup_generator.rs` `frame_protection_source` (PR
  #894 fixes #563 — a per-root override previously never reached
  session-attributed frames because resolution was keyed under the session
  id, which has no shipped override surface, and silently fell back to the
  global default; it is now keyed under the root when no per-session
  override row exists).

### S6 — Select frames and generate a session cleanup plan {#S6}
- **Do:** Adjust the frame selection if needed, choose Archive or System
  trash, and click "Generate cleanup plan."
- **Expect:** A plan is generated for the selected frames and hands off to
  the same review/apply flow as S3/S4; "Generate cleanup plan" is disabled
  while no frame is selected.
- **Expect (negative):** Nothing moves until the resulting plan is approved
  and applied.
- **Trace:** `apps/desktop/src/features/inventory/store.ts` (`useGenerateRawFrameCleanupPlan`)

## Success criteria
- SC1: A project scan against an unchanged candidate set returns the same
  grouping, protection flags, and reclaimable total on repeated runs (S1),
  and disk contents are unchanged.
- SC2: An Archive-destination plan with no protected item, once approved,
  reports every item succeeded, and a subsequent scan (S1 or S5) no longer
  offers those items as candidates (S2–S4).
- SC3: Any plan containing a protected item cannot reach "Approve & apply"
  enabled without an explicit per-item acknowledgement (S3).
- SC4: A plan can never reach an enabled "Approve & apply" with zero items:
  the project flow's Generate control cannot exist without at least one
  candidate (S1 gates it) and the session flow's Generate is disabled while
  no frame is selected (S6); approving a zero-item plan is separately
  rejected server-side as defense-in-depth. No step surfaces an explanatory
  reason to the user for this (S1–S3, S5–S6).
- SC5: A session raw-frame scan preselects only non-protected frames and
  offers no selection control on protected frames (S5).
- SC6: "Approve & apply" carries the red destructive style if and only if
  the plan holds at least one item whose action is `delete`, independent of
  the destination chosen at S2 (S3).
- SC7: A plan holding a `delete` item cannot have that item applied unless
  the destructive confirmation is recorded; an unconfirmed `delete` item
  ends `refused` with `destructive_unconfirmed` rather than deleted (S3–S4).

## Known gaps
- G1: (dissolved 2026-07-15) — tracked as issue #741; trash destination fails every apply item.
- G2: (dissolved 2026-07-15) — tracked as issue #807; protected-item acknowledgement is cosmetic.
- G3: (dissolved 2026-07-15) — tracked as issue #766; applied plans lack durable audit rows.
- G4: (dissolved 2026-07-15) — tracked as issue #876; no free-space estimate at review.
- G5: (dissolved 2026-07-15) — tracked as issue #780; reopen reconcile can misreport cleanup candidates.
- G6: A System-trash plan gets neither the red destructive style nor the
  destructive-confirm checkbox: both are keyed on item action `delete`, and
  a trash plan's items carry action `archive` until apply reroutes them. A
  plan that sends files to the OS trash therefore reviews as if it were a
  move. Tracked as astro-plan-cricc.
- G7: The Archive-folder destination hint states the archive folder is
  reversible until emptied, which does not hold for policy-Delete items in
  the same plan. Tracked as astro-plan-8zz72.
- G8: `destructive_unconfirmed` has no mapped user-facing message; a refused
  destructive item surfaces through the generic apply-failed path, so that
  refusal is not validatable as a distinct message today.
- G9: The Trash-destination apply path and the protected-item
  acknowledgement path are not stepped in S1–S6; validating them needs a
  fixture with a protected candidate and an OS trash available.
- Dropped: the legacy 2026-07-04 note that the cleanup review UI "requires
  PR #413 (open)" is stale — PR #413 merged 2026-07-04
  (`feat: review and safely apply project cleanup plans with live
  progress`); the scan/review/generate UI is fully shipped (folded from
  deltas/2026-07-14-jval-docdrift.md).

## Delta log

- **Δ2** 2026-07-15 · S5 · behavior-change
  A per-root protection override now actually governs cleanup
  classification for the session-attributed raw frames it owns; previously
  the override was cosmetic there (resolution was keyed under the session
  id, found no override row, and silently inherited the global default).
  Evidence: PR #894 (fixes #563) · by: journey-scribe (intent-gated)

- **Δ3** 2026-07-20 · S3 · behavior-change
  "Approve & apply" now renders in the red destructive button style only
  when the plan contains an item whose action is `delete`; every other plan
  renders it in the neutral primary style — previously the button was
  unconditionally styled destructive.
  Evidence: PR #1190 · by: journey-scribe (intent-gated)

- **Δ4** 2026-07-15 · S3, S4 · behavior-change
  Applying a plan that holds a `delete` item now requires an explicit
  destructive confirmation: the review overlay shows a confirm checkbox, the
  confirmation persists per item, and the executor refuses an unconfirmed
  `delete` item as `destructive_unconfirmed` instead of removing the file.
  Evidence: PR #855 · by: journey-scribe (intent-gated)
