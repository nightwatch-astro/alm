---
id: J06
title: Reclaim disk space from processing outputs and raw sub-frames without losing anything protected
version: 6
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
  - PR #1735 (trash-destination plans review as destructive)
  - PR #1739 (unreadable frame attribution refuses raw-frame cleanup)
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
  style exactly when applying the plan would remove a file from where it
  lives now — either some item's action is `delete` (cleanup policy assigned
  Delete to that item's data type) or the destination chosen at S2 is System
  trash, which reroutes every `archive` item into the OS bin at apply time.
- **Expect:** Such a plan also shows a destructive-confirm checkbox
  (`plan-review-confirm-destructive`, labelled "I confirm these items may be
  deleted."). "Approve & apply" stays disabled until it is checked, and
  checking it persists the confirmation per item so it survives closing and
  reopening the review. A System-trash plan is gated the same way as a
  `delete` plan: the review overlay, the confirmation writer, and the apply
  refuser all treat trash-destination `archive` items as destructive.
- **Expect:** Each item's action pill renders in the danger style under the
  same rule, so a trash-destination plan does not present its items as
  ordinary moves.
- **Expect (negative):** No destructive item is ever applied without a
  recorded confirmation: the executor refuses such an item with
  `destructive_unconfirmed` and marks it `refused` rather than removing it —
  including a trash-destination `archive` item, whose effective executor
  action is `Trash`.
- **Expect (negative):** "Approve & apply" stays disabled while any protected
  item's acknowledgement is outstanding, or while the plan holds zero items —
  in both cases the overlay shows no explanatory text, only the disabled
  control (no "this plan is empty" or similar message). A zero-item plan
  cannot actually be produced by either flow in the first place: the project
  flow's Generate control does not render unless S1's scan found candidates,
  and the session flow's Generate (S6) is disabled while no frame is
  selected — so this overlay state is unreachable via the documented S1–S4 /
  S5–S6 path; the server-side rejection is defense-in-depth only.
- **Trace:** `apps/desktop/src/features/plans/PlanReviewOverlay.tsx:83-91`
  (`isDestructiveItem` — `delete`, or `archive` with
  `destructiveDestination === 'os_trash'`) feeding `:217` (button variant and
  confirm gate) and `:446` (item pill); approve is disabled on
  `plan.itemsTotal === 0` (`:544`) and the cleanup flow passes no
  `emptyReason` (only `ProjectDetail.tsx:462`, the archive flow, does), so no
  message renders for that case; `PlanProtectionGate`;
  `plans::approve::approve_plan` (rejects a zero-item plan with
  `plan.items.empty`, not reachable via the shipped UI); contract operation
  `plans.confirm.destructive` →
  `crates/persistence/plans/src/repositories/plans.rs:356-363`
  (`plan_items.destructive_confirmed`);
  `crates/app/core/src/plan_apply/paths.rs:304-306` (apply-time refuser,
  keyed on the effective action from the reroute at `:212-230`); PR #1190,
  PR #1735

### S4 — Approve and apply {#S4}
- **Do:** Click "Approve & apply" on a plan whose destination is Archive and
  that contains no protected item, checking the destructive-confirm box
  first if the plan carries any item whose action is `delete` (see Known
  gaps for the Trash-destination apply path and the protected-item case).
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
- **Expect (negative):** No candidate list is shown at all while any
  session's frame-to-session attribution is unreadable, even for a different
  session than the one being scanned: the scan is refused library-wide with a
  red banner naming the affected session ids and telling the user to repair
  or re-import those sessions. The refusal is blocking and non-retryable, and
  the app offers no way to repair the session from inside the product (G10).
- **Trace:** `apps/desktop/src/features/sessions/RawFrameCleanupSection.tsx`
  (`:181-182` renders the refusal in a danger `Banner`);
  `crates/app/core/src/cleanup_generator/raw_frames.rs:65-97`
  (`refuse_unreadable_frame_attribution`, `internal.data`, blocking,
  non-retryable) called from `scan_raw_frames` at `:148`;
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
- **Expect (negative):** No plan is generated while any session's
  frame-to-session attribution is unreadable: generation is refused on the
  same library-wide condition as the S5 scan, so a frame can never be absent
  from the preview and present in the plan (or the reverse).
- **Trace:** `apps/desktop/src/features/inventory/store.ts`
  (`useGenerateRawFrameCleanupPlan`);
  `crates/app/core/src/cleanup_generator/raw_frames.rs:229`
  (`generate_raw_frame_plan` calls the same refusal)

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
  applying the plan would remove a file from where it lives now — at least
  one item's action is `delete`, or the destination chosen at S2 is System
  trash (S3).
- SC7: A destructive item cannot be applied unless the destructive
  confirmation is recorded; an unconfirmed one ends `refused` with
  `destructive_unconfirmed` rather than removed. This holds for a
  trash-destination `archive` item as well as a `delete` item (S3–S4).
- SC8: While any session's frame-to-session attribution is unreadable,
  neither the raw sub-frame preview nor the raw sub-frame plan can be
  produced anywhere in the library, and the user is told which sessions are
  affected (S5–S6).

## Known gaps
- G1: (dissolved 2026-07-15) — tracked as issue #741; trash destination fails every apply item.
- G2: (dissolved 2026-07-15) — tracked as issue #807; protected-item acknowledgement is cosmetic.
- G3: (dissolved 2026-07-15) — tracked as issue #766; applied plans lack durable audit rows.
- G4: (dissolved 2026-07-15) — tracked as issue #876; no free-space estimate at review.
- G5: (dissolved 2026-07-15) — tracked as issue #780; reopen reconcile can misreport cleanup candidates.
- G6: (dissolved 2026-08-24 by PR #1735 — astro-plan-cricc closed) — a
  System-trash plan reviewed as if it were a move.
- G7: The Archive-folder destination hint states the archive folder is
  reversible until emptied, which does not hold for policy-Delete items in
  the same plan. Tracked as astro-plan-8zz72.
- G8: `destructive_unconfirmed` has no mapped user-facing message; a refused
  destructive item surfaces through the generic apply-failed path, so that
  refusal is not validatable as a distinct message today.
- G9: The Trash-destination apply path and the protected-item
  acknowledgement path are not stepped in S1–S6; validating them needs a
  fixture with a protected candidate and an OS trash available.
- G10: A session whose `frame_ids` is unreadable blocks raw sub-frame
  cleanup for the whole library and there is no in-app repair or re-import
  affordance to clear the condition, so S5–S6 dead-end until the database is
  repaired outside the app. Tracked as astro-plan-dq9r3.
- G11: A trash-destination plan still shows an archive path in the review
  table's destination column, so the row text disagrees with what apply
  does. Tracked as astro-plan-5jfcc.
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

- **Δ5** 2026-08-24 · S3, S4, SC6, SC7 · behavior-change
  A cleanup plan whose destination is System trash now reviews as
  destructive: red "Approve & apply", danger item pills, and the
  destructive-confirm checkbox, with apply refusing an unconfirmed item.
  Previously such a plan reviewed as an ordinary move and skipped the gate
  entirely (former G6).
  Evidence: PR #1735 (3326320ba), astro-plan-cricc · by: journey-scribe
  (intent-gated)

- **Δ6** 2026-08-24 · S5, S6, +SC8 · behavior-change
  A session whose frame-to-session attribution is unreadable now refuses
  raw sub-frame cleanup for the entire library — both the preview and the
  plan — naming the affected sessions, where such a session previously read
  as having zero frames and its frames could be offered as candidates.
  Evidence: PR #1739 (b747f21c8), astro-plan-l2s06 · by: journey-scribe
  (intent-gated)
