---
id: J11
title: Correct an inbox or calibration mistake before it becomes permanent
version: 6
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [inbox-confirm, calibration, plans, shell]
interfaces: [desktop-ui]
trace:
  - pre-migration journey.md @ git 66026463
  - docs/development/journey-run-2026-07-14.md (Journey 11 section)
  - issue #611 (Bulk frame-type override has no heterogeneity warning and no undo, open)
  - issue #664 (calibration.match.suggest observer_location_missing leak/gate, open)
  - crates/app/inbox/src/reclassify.rs
  - crates/persistence/inbox/src/repositories/inbox/items.rs
  - crates/app/core/src/inbox_plan.rs
  - crates/app/core/src/plans/discard.rs, crates/app/core/src/plans/retry.rs
  - crates/app/calibration/src/matching/assign.rs
  - crates/persistence/calibration/src/repositories/calibration_assignment.rs
  - PR #1054 (issue #611 — bulk-override warning, acknowledgement and undo)
  - PR #1032 (issue #875 — calibration.match.unassign)
  - PR #1194 (discard resets an item to its derived pre-plan state)
  - PR #1711 (destination containment refused at apply)
  - PR #1722 (boot reconciliation judges presence by the directory entry)
---

## Goal
Let the user correct their own mistakes during inbox triage and calibration
matching without touching a single file on disk: assign the wrong frame type
to a needs-review file and fix it before confirming, pick the wrong
destination library and change it before confirming, confirm a plan too
early and back out of it before it applies, or assign the wrong calibration
master to a session and replace it with the right one. When a mistake does
reach apply, the user needs it refused rather than written, and needs a way
back — including after the app was killed mid-apply. "Done" is the index
returning to a state indistinguishable from having never made the mistake —
no orphaned plan, no leftover file, no stuck classification — and every
refused item either retried or explicitly abandoned.

## Preconditions
- P1: an inbox item whose files include at least one the scanner could not
  auto-detect a frame type for (a "needs review" file).
- P2: more than one destination root exists for the relevant frame-type
  category, so a root picker is shown.
- P3: an inbox item has been confirmed to a not-yet-applied plan.
- P4: a session has already been assigned a calibration master (bias, dark,
  or flat) that the user now believes is wrong, and at least one other
  compatible master exists to reassign to.
- P5: for S7's refusal branch: an approved plan holding at least one item
  whose stored destination resolves outside the chosen folder (reachable only
  from an existing plan row, not from the S4 picker, which offers registered
  roots only — see G7).
- P6: for S7's recovery branch: a plan left in `applying` by a process kill
  during apply.

## Steps

### S1 — Assign a frame type to a needs-review file, then change your mind {#S1}
- **Do:** open an inbox item with needs-review files; assign a frame type to
  one file, then, before applying, change the pending selection to a
  different frame type; submit.
- **Expect:** the file's classification reflects only the last value
  submitted; the item's classification type (`single_type` / `mixed` /
  `unclassified`) recomputes from the submitted overrides.
- **Expect (negative):** nothing is written until the override is submitted;
  changing the pending dropdown value before submitting never touches the
  index.
- **Trace:** `apps/desktop/src/features/inbox/useInboxReclassifyState.ts:51`,
  `:101-102` (`pendingOverrides`, `handleApplyOverrides`);
  `apps/desktop/src/features/inbox/InboxNeedsReview.tsx:328-342` (the
  submit row); `crates/app/inbox/src/reclassify.rs`.

### S2 — Bulk-assign a frame type across several needs-review files in one action {#S2}
- **Do:** select multiple needs-review files in the same item and submit one
  frame type (and optionally filter/exposure/binning) for the whole
  selection.
- **Expect:** every selected file receives the submitted values in one call;
  the selection and bulk-input fields clear on success; the remaining
  needs-review count drops by the number of files that received a frame
  type.
- **Expect:** when the selection spans more than one already-detected frame
  type, a warning names the type about to be written across it and Apply
  stays disabled until the user ticks the acknowledgement beside it; the
  Apply label changes to the apply-anyway wording. Changing the selection or
  the target type withdraws the acknowledgement, so it can never carry over
  to a different selection.
- **Expect:** immediately after a bulk frame-type apply, an undo banner
  offers to restore each affected file's pre-override detected type, and
  taking it returns those files to that type.
- **Expect (negative):** files not in the selection are unaffected.
- **Expect (negative):** the warning and the undo both key on files that
  already had a detected frame type, so neither appears for a selection whose
  files have none — exactly the needs-review selection this journey's P1
  describes. Such a bulk override is still applied with no warning and is
  recoverable only by resubmitting a value per file (G4).
- **Trace:** `apps/desktop/src/features/inbox/useInboxReclassifyState.ts:164-186`
  (distinct-type count over `frameTypeEffective`, non-null only;
  acknowledgement keyed to `${type}::${selection}`), `:207-238` (undo
  snapshot, captured only for files with a prior type and only when the call
  changes `frameType`);
  `apps/desktop/src/features/inbox/InboxNeedsReview.tsx:219-254` (warning +
  acknowledgement), `:260` (Apply disabled until acknowledged), `:292-316`
  (undo banner); strings at `apps/desktop/messages/en-GB.json`
  (`inbox_bulk_heterogeneous_*`, `inbox_bulk_undo_*`); issue #611.

### S3 — Reclassification is refused while a plan is open on the item {#S3}
- **Do:** with an open (confirmed, unapplied) plan linked to an item, attempt
  to change a file's frame-type assignment on that item.
- **Expect:** the reclassify action is refused with a reason naming the open
  plan; the user must discard the plan (S5) before reclassifying.
- **Trace:** `crates/app/inbox/src/reclassify.rs` (`inbox.has.open.plan` guard).

### S4 — Change the destination library before confirming {#S4}
- **Do:** on an item eligible for more than one destination root of the
  applicable category, pick a root, then pick a different one before
  confirming; alternatively, leave it on "Auto" and let the confirm attempt
  resolve it.
- **Expect:** the confirmed plan's destinations reflect only the last root
  selected at the time of confirm. If "Auto" cannot resolve a single root,
  confirm is refused and the user is prompted to choose among the specific
  candidate roots before the plan is generated — nothing is confirmed to an
  ambiguous or wrong root silently.
- **Expect (negative):** no plan is generated, and no files move, from
  picking a root alone — only confirming does.
- **Trace:** `apps/desktop/src/features/inbox/InboxDetail.tsx:338`
  (`onSelectRoot`); `apps/desktop/src/features/inbox/InboxPage.tsx:336`,
  `:393` (`pendingRootPick`, `inbox.destination_root_required`). The
  apply-time consequence of an out-of-folder destination is S7.

### S5 — Discard a confirmed-but-unapplied plan {#S5}
- **Do:** from the inbox plan surface, discard a plan that has been
  confirmed but not yet applied.
- **Expect:** the plan's state becomes discarded and the originating inbox
  item returns to its pre-plan unconfirmed state, without a page refresh: an
  item that carries a frame type returns to `classified` and is immediately
  confirmable again; an item with no frame type returns to
  `pending_classification` and must be classified before it can be confirmed.
  Under this journey's P1 (needs-review files) the second outcome is the
  reachable one. An audit event records the discard.
- **Expect (negative):** discard never touches any file — the plan was never
  applied, so there is nothing on disk to revert. Discard is refused with
  `plan.in_progress` while the plan is `applying` or `paused`; a plan already
  in one of those states cannot be silently abandoned through this action.
  After an unclean shutdown, an interrupted plan is not left stuck in that
  refusal — S7 is the route back.
- **Trace:** `apps/desktop/src/features/inbox/PlanPanel.tsx` (`onCancel`);
  `crates/app/core/src/inbox_plan.rs:426` (`cancel_inbox_plan` →
  `reset_inbox_item_to_unconfirmed`);
  `crates/persistence/inbox/src/repositories/inbox/items.rs:369-378` (the
  state is derived in SQL from the row's own `frame_type`);
  `crates/app/core/src/plans/discard.rs:32`, `:42-46` (`discard_plan`, the
  `Applying | Paused` guard).

### S6 — Replace a mis-assigned calibration master {#S6}
- **Do:** from the correct master's detail page, assign it to the session
  that currently carries the wrong master for the same calibration type
  (bias/dark/flat); force the assignment past a hard-rule mismatch if
  needed.
- **Expect:** the session now shows the newly assigned master as its
  calibration source for that type; the previous assignment for that
  (session, calibration type) pair is gone — replaced, not duplicated; the
  new and old masters' "used by" counts and session lists update
  accordingly; an audit event records the new assignment.
- **Expect:** the mistake is also correctable by removing the assignment
  outright rather than only by replacing it: unassigning returns the session
  to "no master assigned" for that calibration type.
- **Expect (negative):** assigning a replacement master never mutates or
  moves any file; only the assignment link changes.
- **Trace:** `crates/app/calibration/src/matching/assign.rs` (`assign`
  orchestration + audit emission, `unassign` at `:192`);
  `crates/persistence/calibration/src/repositories/calibration_assignment.rs:92`
  (`ON CONFLICT(session_id, calibration_type)` — replaced, not duplicated);
  `apps/desktop/src/features/calibration/useCalibration.ts:164`
  (`calibrationMatchUnassign` call site),
  `apps/desktop/src/features/calibration/MatchCandidatesPanel.tsx`.

### S7 — Recover after an apply-time refusal or an interrupted apply {#S7}
- **Do:** apply a plan whose destination lies outside the chosen folder (the
  S4 mistake carried past confirm), then, separately, restart the app after
  an apply was interrupted by an unclean shutdown.
- **Expect:** the offending item is refused rather than applied — marked
  `refused` with `root_escape`, or `path.invalid` for an item with no root and
  a relative path — and the run carries on with the remaining items instead of
  aborting. The failed items are recoverable without redoing the triage: the
  plan review offers a retry that generates a child plan from exactly those
  items.
- **Expect:** after an unclean shutdown the app itself raises the interrupted
  apply: a banner states that PlateVault was not shut down cleanly and how
  many plans await review, with "Review & resume" opening the plan review for
  the first of them and "Dismiss" hiding it for the session. An item whose
  filesystem state cannot be judged is marked failed with an
  ambiguous-state reason rather than being assumed complete, so resuming
  starts from a stated position and the plan is reviewable — and therefore
  discardable (S5) — again.
- **Expect (negative):** nothing is resumed, retried, or mutated by the
  banner appearing or by dismissing it; only the user's action in the review
  overlay applies anything.
- **Expect (negative):** a dangling symlink at the destination is never read
  as a completed move.
- **Trace:** `crates/fs/executor/src/run/loop_.rs:171-188` (all three sides
  resolved through one gate), `:442-461` (refusal is per item, the run
  continues); refusal codes at
  `crates/fs/executor/src/ops/path_gate.rs:125-132`;
  `crates/app/core/src/plans/retry.rs:35` (`retry_plan`),
  `apps/desktop/src/features/plans/PlanReviewOverlay.tsx:237`, `:388-408`
  (the retry control and its call);
  `crates/fs/executor/src/reconcile.rs:100-101` (presence judged by the
  directory entry via `symlink_metadata`), verdicts at `:61-90`;
  `crates/app/core/src/plan_apply/reconcile.rs:14-18` (verdict policy),
  `:135-149` ("filesystem state ambiguous at boot; needs user
  resume/repair");
  `apps/desktop/src/features/recovery/RecoveryBanner.tsx`, mounted at
  `apps/desktop/src/app/Shell.tsx:164`, strings at
  `apps/desktop/messages/en-GB.json:2616-2618`; PR #1711, PR #1722.

## Success criteria
- SC1: after S1/S2, the file's/selection's classification matches only the
  last submitted values — no earlier submission is still in effect.
- SC2: after S4, the plan's destination(s) match the root selected at
  confirm time in 100% of cases, including the forced-choice path when
  auto-resolution is ambiguous.
- SC3: after S5, the inbox item carries no plan link and is back in its
  pre-plan state within the same session — confirmable again when it has a
  frame type, classifiable again when it does not — and no file changed on
  disk (path and mtime identical to before S5's precondition).
- SC4: after S6, exactly one active assignment exists for the (session,
  calibration type) pair, and it is the newly assigned master; after an
  unassign, zero exist for that pair.
- SC5: a bulk frame-type override across a selection with two or more
  detected types cannot be applied without an acknowledgement, and is
  reversible in one action immediately afterwards (S2).
- SC6: after S7, every item refused for leaving the chosen folder is
  `refused` with its code, the remaining items of the same plan still reached
  a terminal state, and a retry plan can be generated holding exactly the
  failed items.
- SC7: after an unclean shutdown mid-apply, the app surfaces the interrupted
  plan without the user going looking for it, and no item is recorded
  `succeeded` on ambiguous filesystem evidence (S7).

## Known gaps
- G1: (dissolved 2026-07-15) — tracked as issue #611; no reset-to-detected action in reclassify.
- G2: (dissolved 2026-08-24 by PR #1032) — no master un-assign;
  `calibration.match.unassign` ships and is stepped at S6.
- G3: (dissolved 2026-08-24 by PR #1054) — bulk override has no heterogeneity
  warning; the warning, its acknowledgement gate and the undo all ship and
  are stepped at S2. The residual limit is G4.
- G4: The S2 heterogeneity warning and undo are both blind to a file with no
  detected frame type: the warning counts distinct non-null detected types, so
  a selection of undetected files never trips it, and the undo snapshot skips
  files with no prior type, so no undo banner appears for that selection. This
  is the selection P1 describes, so J11's own precondition reaches the
  unguarded path.
- G5: (G1's subject, re-verified 2026-08-24) There is still no affordance that
  clears a manual frame-type override back to the scanner's detected value.
  The only reversal is the S2 undo banner, which
  exists for one bulk apply and only for files that had a prior detected type;
  the backend write helper (`set_manual_override_reset_stale`,
  `crates/persistence/inbox/src/repositories/q_inbox.rs:55`) is reached only
  by an override write, not by a reset control.
- G6: Out of scope here: the confirm-time and apply-time failure taxonomy
  (`classification.stale`, `inbox.destination_collision`,
  `inbox.missing_path_attributes`, `pattern.unset`,
  `inbox.invalid_destination_root`, `inbox.no_destination_root`,
  `permission.denied`, `volume.unavailable`, `disk.full`, `source.missing`,
  `source.locked`, `copy.succeeded.delete.failed`) belongs to J12, which owns
  how a refusal is explained and what to do next. J11 steps only the refusals
  that are part of correcting a mistake: `inbox.has.open.plan` (S3),
  `inbox.destination_root_required` (S4), `plan.in_progress` (S5), and the
  containment refusals (S7).
- G7: P5 is not establishable through the shipped UI: the destination-root
  picker offers registered roots only, so an out-of-folder destination has to
  be planted in the plan row. S7's refusal branch is therefore validatable
  only against a fabricated plan, not by user action.

## Delta log

- **Δ2** 2026-08-24 · S2, +SC5, G3, +G4 · behavior-change
  A bulk frame-type override across a selection spanning more than one
  detected type now warns and requires an explicit acknowledgement before
  Apply, and an undo banner afterwards restores each affected file's previous
  detected type. Previously the override was written with no warning and no
  reversal. Both are blind to files with no detected type (G4).
  Evidence: PR #1054 (836871a27), issue #611 · by: journey-scribe
  (intent-gated)

- **Δ3** 2026-08-24 · S6, SC4, G2 · behavior-change
  A session's calibration assignment can now be removed outright, not only
  replaced: `calibration.match.unassign` returns the session to "no master
  assigned" for that type and is wired to the UI.
  Evidence: PR #1032 (22d9f67a9), issue #875 · by: journey-scribe
  (intent-gated)

- **Δ4** 2026-08-24 · S5, SC3 · behavior-change
  Discarding a plan now returns the inbox item to the state its own frame type
  implies — `classified` when it has one, `pending_classification` when it
  does not — instead of unconditionally reporting `classified`, which asserted
  a frame type the row may not carry.
  Evidence: PR #1194 (6eaa086e1) · by: journey-scribe (intent-gated)

- **Δ5** 2026-08-24 · +S7, +SC6 · behavior-change
  An item whose destination resolves outside the chosen folder is now refused
  at apply (`root_escape`, or `path.invalid` when it has no root and a
  relative path) and the run continues with the rest of the plan, where such
  a destination was previously written; the failed items are recoverable
  through a retry plan.
  Evidence: PR #1711 (846a170c0) · by: journey-scribe (intent-gated)

- **Δ6** 2026-08-24 · S5, S7, +SC7 · behavior-change
  Boot reconciliation now judges filesystem presence by the directory entry,
  so a dangling symlink no longer reads as a finished move: such an item is
  marked failed with an ambiguous-state reason and its plan is surfaced by the
  unclean-shutdown banner for an explicit resume or repair, which is also the
  route back to a discardable plan after a crash.
  Evidence: PR #1722 (0f8cff68c) · by: journey-scribe (intent-gated)
