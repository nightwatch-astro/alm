---
id: J12
title: See why an action failed or was refused, and what to do next
version: 5
status: draft
last_reviewed: 2026-07-20
actors: [astrophotographer]
surfaces: [plans, projects, audit, shell]
interfaces: [desktop-ui]
trace:
  - pre-migration journey.md @ git 66026463
  - specs/030-ui-audit-revision/spec.md (FR-130-134 §8.3, FR-135-140 §12)
  - deltas/2026-07-14-q15-t127.md (folded into S5 / Known gaps G1)
  - deltas/2026-07-14-q16-t130.md (folded into S3)
  - docs/development/journey-run-2026-07-14.md (Journey 12 section — live
    Windows validation; source for step-level Trace notes below)
  - issue #1236 (re-verify S3/S4 against current main, closed by this pass)
  - PR #1041 (approve_plan FS snapshot, closes #829), PR #1054 (per-item
    failure reason threaded to PlanReviewOverlay, closes #607), PR #855
    (mid-run item retry re-execution, closes #742) — re-verification
    evidence for the 2026-07-20 Trace updates to S3/S4 below
  - PR #1711 (destination containment refused at apply)
  - PR #1722 (unclean-shutdown recovery banner; presence judged by the entry)
  - PR #1080 (issue #603 — empty-plan diagnostic in the review overlay)
---

## Goal
When an action the user takes fails or is refused — a lifecycle transition
that cannot succeed from the current state, a generated plan with nothing in
it, a filesystem plan that only partially applies, a plan that went stale
before apply — the user sees *what* happened, *why*, and *what to do next*
without leaving the surface they were on, and can later find the same
refusal/failure, with the same reason, in the Audit Log. Done means every
one of these classes produces a specific, actionable, already-translated
explanation in place — never a generic "failed" toast, a silently disabled
control, or a raw error code.

## Preconditions
- P1: A project exists in a lifecycle state that offers a transition control
  gated on an unmet precondition (e.g. Archive from `completed`, which
  requires an approved filesystem plan) — not a transition forbidden
  outright, since forbidden edges are never rendered as controls at all
  (`lifecycleFooterActions`, `apps/desktop/src/features/projects/
  lifecycle-actions.ts`).
- P2: A source/plan-generating action (e.g. a cleanup or archive scan) can be
  run against a library state that matches zero candidates.
- P3: A confirmed filesystem plan is pending apply, and at least one item it
  references can be made to fail during apply (e.g. its source file is
  removed or made inaccessible after confirm but before apply).
- P4: A confirmed, not-yet-applied plan exists whose referenced source data
  can be changed on disk after confirmation, to force staleness.
- P5: Audit Log access is available for the project/plan touched above.
- P6: For S6/S7: a harness able to induce apply-time environment failures
  (volume removed, disk full, source locked) and to kill the app mid-apply
  (see G4).

## Steps

### S1 — A refused lifecycle transition surfaces its reason via toast or the plan-review dialog {#S1}
- **Do:** Open a project whose current lifecycle state offers a transition
  control gated on an unmet precondition (e.g. Archive from `completed`),
  and click it.
- **Expect:** The refusal surfaces at the point of the click, in the user's
  language: a `plan.required`/`plan.not_approved` edge shows an info toast
  naming that a plan is required and — for the completed/blocked→archived
  edge — auto-generates and opens the plan-review dialog for that
  transition; any other refusal shows an error toast carrying the backend's
  reason text. A transition that is structurally forbidden from the current
  lifecycle state (e.g. `processing` → `ready`) is never offered as a
  control at all, so there is no disabled-with-no-reason case to hit.
- **Expect (negative):** The control is never clickable-and-silent — every
  click produces a toast, a dialog, or a visible state change; the generic
  fallback toast ("Transition refused.") with no specific reason is never
  the only feedback shown.
- **Trace:** Live validation (2026-07-14, real Windows app) found the
  Archive-transition refusal reason was recorded correctly in the audit row
  but not perceivable at the control — issue #600 (open, P0, filed
  2026-07-11 design review, reproduced live 2026-07-14). Mechanism verified
  in code: `apps/desktop/src/features/projects/ProjectDetail.tsx:259-328`
  (`handleTransition`/`handleGenerateArchivePlan`),
  `apps/desktop/src/features/projects/lifecycle-actions.ts` (forbidden
  edges excluded from `footerActions`; doc comment: "Forbidden edges ...
  are not included"). See report for candidate Known gap.

### S2 — An empty generated plan states why it is empty {#S2}
- **Do:** Run a plan-generating action (cleanup/archive scan, or similar)
  against library state that matches nothing.
- **Expect:** The resulting plan view states the reason no items were
  produced (e.g. that current rules matched no candidates), instead of only
  disabling the Approve control.
- **Expect:** For an archive plan this holds: a zero-item plan review shows a
  warning banner carrying the generator's own diagnostic sentence above the
  disabled Approve control.
- **Expect (negative):** Approve is never simply greyed out with no
  accompanying explanation of why there is nothing to approve — except in the
  cleanup flow, which passes no diagnostic (G2).
- **Trace:** `apps/desktop/src/features/plans/PlanReviewOverlay.tsx:613-617`
  (banner rendered when `plan.itemsTotal === 0 && emptyReason`),
  `:60-68` (the caller must forward it — the persisted plan carries no such
  field); `apps/desktop/src/features/projects/useProjectDetailActions.ts:162`
  and `ProjectDetail.tsx:462` (the archive flow forwards
  `archive.plan.generate`'s `emptyReason`); no cleanup call site forwards
  one. Issue #603.

### S3 — A partial apply failure names failures and offers retry {#S3}
- **Do:** Apply a confirmed plan where at least one item fails during apply
  (e.g. its source file went missing or became inaccessible after confirm).
- **Expect:** Failed items are listed by name with a per-item reason;
  previously succeeded items in the same run keep their applied state
  visible; a retry action is offered. A missing/unresolved metadata value
  shown anywhere in this view uses the shared muted "unresolved" chip
  (`UnresolvedChip`, `apps/desktop/src/components/RenderValue.tsx`,
  i18n `cmp_unresolved_chip`) and is never confusable with an item-failure
  indicator — the chip marks absent data, not a failed action (FR-137).
- **Do:** Trigger retry on the terminal (failed) plan.
- **Expect:** A new plan is generated scoped to only the previously-failed
  items (`plansRetry(planId, 'failed')`, the plan-review overlay's Retry
  action); items that already succeeded are not included in it.
- **Expect (negative):** A partial failure never hides or rolls back the
  items that already succeeded, and never presents a single undifferentiated
  "plan failed" message in place of per-item detail.
- **Trace:** RE-VERIFIED 2026-07-20 against current `main`: this step now
  matches the running app; the 2026-07-14 finding was accurate at the time
  but has since been fixed. `approve_plan`
  (`crates/app/core/src/plans/approve.rs:105`) now snapshots per-item FS
  metadata (`approved_mtime`/
  `approved_size_bytes`) at approval, and `check_cas`
  (`crates/fs/executor/src/ops/cas_check.rs:43-98`) compares it at apply
  time, returning `ItemStale`/`SourceMissing` instead of skipping
  permissively — landed in PR #1041 (closes #829). The per-item id/reason
  payload is no longer discarded: `PlanReviewOverlay.tsx:488-493`
  renders a Result column with a state `Pill` plus `item.failureReason`
  text sourced from the durable `plan_items.failure_reason` column —
  landed in PR #1054 (closes #607). Integration coverage now exists for
  the exact 2026-07-14 reproduction: `crates/app/core/tests/
  plan_apply_lifecycle_integration.rs` asserts 2 succeeded + 1 failed item
  reaches plan state `partially_applied` with `items_applied=2`/
  `items_failed=1` (not the old silent `itemsFailed=0`). Retry mechanism
  re-verified: `handleGenerateRetryPlan`
  (`apps/desktop/src/features/plans/PlanReviewOverlay.tsx:388-408`,
  `retryable` gate at `:237`) is reachable once `effectiveState` is `failed`/
  `partially_applied`/`cancelled`, and calls `plansRetry(planId, 'failed')`
  which still creates a new plan scoped to the failed subset — covered by
  `PlanReviewOverlay.test.tsx` ("offers \"Generate retry plan\" after a
  partially_applied outcome and drives plans.retry", ~line 379). The
  separate in-run `retry_plan_item` path (`crates/app/core/src/plan_apply/lifecycle.rs:639`) is also now fixed — issue #742 (mid-run retry never
  re-executed) landed in PR #855, covered by
  `crates/fs/executor/src/run/tests.rs:463`
  (`mid_run_retry_reexecutes_already_passed_item`)
  — but this remains a distinct mechanism from `plansRetry`, per the
  doc's original framing. Unresolved-chip claim re-verified via
  `apps/desktop/src/components/RenderValue.tsx:76-87`; the "never
  confusable with a failure indicator" half is now also verified — the
  Result column's failed-state `Pill` (PR #1054) is visually and
  semantically distinct from `UnresolvedChip`. A real-UI journey covering
  partial-apply recovery is now proposable: this step (and the
  `PlanReviewOverlay` surface generally) still has zero real-backend/
  real-UI coverage per this doc's own gap language, but the underlying
  mechanism is no longer a known-broken target to validate against.

### S4 — A stale plan refuses to apply and offers regeneration {#S4}
- **Do:** Attempt to apply a plan whose referenced source data changed on
  disk after it was confirmed.
- **Expect:** Apply is refused; the plan is visibly marked stale and the
  changed file(s)/items are identifiable; a regenerate action is offered in
  place of apply.
- **Expect (negative):** Apply never proceeds silently against stale plan
  data, and the stale state is never indistinguishable from a normal
  pending-apply plan.
- **Trace:** RE-VERIFIED 2026-07-20 against current `main`: the shared root
  cause (#829) is fixed — see S3 — so the original "applies silently,
  `itemsFailed=0`" reproduction no longer occurs. But this step's specific
  expectations (a visibly *stale*-marked plan, apply refused up front, a
  distinct regenerate action) still do not match the running app, so the
  "unmet" framing stands, for a different and more precise reason than the
  2026-07-14 note gave. What actually happens now: a CAS mismatch detected
  mid-apply pauses the run (R-Pause-1) rather than either applying silently
  or cleanly refusing up front — `crates/app/core/tests/
  plan_apply_lifecycle_integration.rs::apply_pauses_on_stale_item_cas_mismatch`
  asserts plan state `paused` with `pause_reason="item.stale"`, source file
  left untouched. The UI surfaces this as a generic paused badge with the
  **raw, untranslated** reason string — `PlanReviewOverlay.tsx:716-717`
  renders `m.plans_review_paused_badge({ reason: progress.pauseReason })`,
  confirmed by `PlanReviewOverlay.test.tsx` asserting the literal text
  "Paused — item.stale" — and offers only a "Resume" button (re-attempts
  the same operation), never a distinct "stale"/"regenerate" affordance.
  Separately, there is dead code aimed at exactly this step's UX: `inbox_
  plan.rs:130-135` computes `InboxPlanView.stale` as `plan_row.state ==
  "stale"`, which the frontend (`apps/desktop/src/features/inbox/
  PlanPanel.tsx:1002-1009`) uses to disable Apply and show a
  "discard and re-confirm" banner (`inbox_stale_plan_warning`) — but no
  code path in the repo ever writes the literal string `"stale"` to
  `plans.state`: `PlanState` (`crates/contracts/core/src/lifecycle.rs:
  60-72`) has no `Stale` variant, `TerminalCounts::terminal_state`
  (`crates/fs/executor/src/run/mod.rs:69`) never returns it, and every raw
  `UPDATE plans SET state = ...` call site was audited (now
  `crates/persistence/plans/src/repositories/plan_apply.rs`, `plans.rs`,
  `projects/`) with none writing `'stale'`. The schema settles it: the
  `plans.state` CHECK constraint
  (`crates/persistence/core/migrations/0001_initial_schema.sql:1777`) does not
  list `'stale'` among its ten allowed values, so that value cannot be
  stored at all — the branch is unreachable by construction, not merely
  unwritten. So `InboxPlanView.stale` is
  always `false` in practice — the one code path that would satisfy this
  step's exact expectation ("visibly marked stale ... regenerate action
  offered in place of apply") is unreachable. Net: staleness detection
  itself is real and no longer silent (genuine improvement over
  2026-07-14), but it surfaces as an untranslated pause reason with a
  retry-style Resume button, not the distinct stale-marking +
  regenerate flow this step describes — still unwired for every plan
  type, now for a dead-code reason rather than a missing-snapshot reason.

### S5 — Every refusal and failure is later findable in the Audit Log {#S5}
- **Do:** After triggering a refusal/failure from S1-S4, open the Audit Log
  for the affected project/plan.
- **Expect:** Lifecycle-transition refusals (S1) appear as durable audit
  entries with an outcome and the same reason text the user saw at the
  moment of refusal.
- **Expect (negative):** The reason text shown in the Audit Log never
  diverges from the reason text the user saw at the moment of refusal.
- **Trace:** Confirmed for S1 in the running app (2026-07-14 live run): the
  Archive-transition refusal was recorded durably with a matching reason
  (`plan.required`) — but only visible via a `title=` hover on the entity
  cell, no dedicated detail/state-change column exists (issue #749, open).
  S3/S4 plan-apply outcomes are not durably audited at all — see Known
  gaps G1.

### S6 — An apply that hits a refusal or an environment failure says which, and whether it stopped {#S6}
- **Do:** Apply a plan that hits each class of apply-time outcome: an item
  whose destination leaves the chosen folder, an item whose destructive
  confirmation is missing, an item from a protected source, an item whose
  source is locked or already gone, a destination that already exists, and an
  environment failure — the library drive unplugged mid-run, or the
  destination out of space.
- **Expect:** Each item carries its own outcome and reason text in the plan
  review's Result column, distinguishable per item, and the counts add up:
  succeeded + failed + skipped/cancelled equals the plan's item total.
- **Expect:** Refusals and per-item failures do NOT stop the run — the
  remaining items are still attempted, and the plan reaches a terminal state
  (`applied`, `partially_applied`, or `failed`). The exceptions are exactly
  three codes: `volume.unavailable`, `disk.full` and `item.stale` pause the
  whole run instead. A paused run shows a paused badge with its reason and a
  Resume control; resuming re-attempts from where it stopped.
- **Expect:** A code the product considers resolvable by retrying
  (`permission.denied`, `source.locked`, `volume.unavailable`, `disk.full`,
  `item.stale`, the three `os_trash.*` codes,
  `copy.succeeded.delete.failed`) is offered through the retry path; the rest
  (`conflict.destination_exists`, `source.missing`, `path.invalid`,
  `root_escape`, `symlink`, `destructive_unconfirmed`, `protected.source`,
  `trash.unavailable`, `materialization.unsupported`) need the underlying
  cause fixed first, and retrying alone will not clear them.
- **Expect (negative):** A refused item is never recorded as succeeded and
  never partially written: the containment refusals are decided before any
  filesystem mutation for that item.
- **Expect (negative):** Every code above surfaces as untranslated backend
  text today, not as a written-for-the-user sentence (G3) — that is the known
  state, not a validation pass.
- **Trace:** `crates/fs/executor/src/failure.rs:110-137` (code strings),
  `:139-166` (`is_recoverable`), `:168-170` (`triggers_pause` —
  `VolumeUnavailable | DiskFull | ItemStale`);
  `crates/fs/executor/src/run/loop_.rs:442-461` (gate refusal is per item and
  the run continues unless the code pauses), `:415-435`
  (`destructive_unconfirmed`); `crates/fs/executor/src/ops/path_gate.rs:125-132`
  (`root_escape` / `path.invalid` / `symlink`);
  `crates/fs/executor/src/run/mod.rs:69` (`terminal_state`);
  `apps/desktop/src/features/plans/PlanReviewOverlay.tsx:488-493` (per-item
  reason text), `:716-729` (paused badge + Resume); PR #1711.

### S7 — After a crash mid-apply, the app raises the interrupted plan itself {#S7}
- **Do:** Kill the app while a plan is applying, then start it again.
- **Expect:** A banner states that PlateVault was not shut down cleanly and
  how many plans are waiting for review, offering "Review & resume" — which
  opens the plan review for the first interrupted plan — and "Dismiss", which
  hides it for this session.
- **Expect:** Items the filesystem shows as finished are healed to succeeded;
  items with no effect on disk are left for the resume; items whose state
  cannot be judged are marked failed with an explicit
  ambiguous-state-at-boot reason, so the plan is reviewable rather than stuck.
- **Expect (negative):** Nothing is resumed or mutated by the banner
  appearing, or by dismissing it.
- **Expect (negative):** A dangling symlink at the destination is never read
  as a completed move — presence is judged by the directory entry, not by
  what it resolves to.
- **Trace:** `apps/desktop/src/features/recovery/RecoveryBanner.tsx`, mounted
  at `apps/desktop/src/app/Shell.tsx:164`, strings at
  `apps/desktop/messages/en-GB.json:2616-2618`;
  `crates/app/core/src/plan_apply/reconcile.rs:14-18` (verdict policy),
  `:38-51` (`ReconcileReport`), `:135-149` ("filesystem state ambiguous at
  boot; needs user resume/repair");
  `crates/fs/executor/src/reconcile.rs:61-90` (verdicts), `:100-101`
  (`symlink_metadata`); PR #1722.

## Success criteria
- SC1: Every offered lifecycle-transition control that is refused (S1)
  surfaces a reason at the moment it is clicked (toast, or the plan-review
  dialog for plan-gated edges); zero clicks across a full pass of a
  project's lifecycle controls produce no feedback at all.
- SC2: Every plan-generating action that yields zero items (S2) shows an
  explanatory message; no run ends with just a disabled Approve and no text.
- SC3: In a partial-apply run (S3), failed item count + succeeded item count
  always equals the run's total item count, and a generated retry plan (S3)
  contains only the failed subset (verified by item id set).
- SC4: A stale plan (S4) never transitions to an applied state without an
  intervening regenerate; 0 stale-plan applies succeed silently.
- SC5: For every S1 refusal triggered, a matching durable audit row with the
  same reason text is retrievable afterward (S5).
- SC6: In every apply run, succeeded + failed + skipped/cancelled equals the
  plan's item total, and the run reached a terminal state unless it paused on
  one of exactly three codes (S6).
- SC7: After a kill mid-apply, the interrupted plan is surfaced by the app on
  the next start without the user searching for it, and no item is recorded
  succeeded on ambiguous filesystem evidence (S7).

## Known gaps
- G1: (dissolved 2026-07-15) — tracked as issues #647 and #766; plan-apply outcomes lack durable audit rows.
- G2: The cleanup flow forwards no empty-plan diagnostic, so a zero-item
  cleanup plan still reviews as a disabled Approve with no explanation — the
  mechanism S2 describes exists but only the archive flow uses it.
- G3: Executor failure codes and pause reasons reach the user as raw
  identifiers (`item.stale`, `destructive_unconfirmed`, `root_escape`): the
  message catalogue maps only the archive/trash-side codes
  (`err_os_trash_permission_denied`, `err_path_permission_denied`). J12's
  Goal ("never a raw error code") is therefore not met for the executor
  family stepped at S6.
- G4: S6's environment branches (`volume.unavailable`, `disk.full`,
  `permission.denied`, `source.locked`) and S7 require OS-level setup —
  unplugging a volume, filling a disk, killing the process mid-apply — that
  the desktop UI cannot induce; validating them needs harness support outside
  the app.

## Delta log

- **Δ2** 2026-07-20 · S3 · behavior-change
  Partial-apply failure handling shipped: `approve_plan` now snapshots
  per-item FS metadata and the CAS check consults it at apply time instead
  of skipping permissively, so a missing/changed source is caught rather
  than silently counted as success; the per-item failure reason now reaches
  the plan-review table (Result column) instead of being discarded to an
  aggregate count. Mid-run per-item retry re-execution also shipped.
  Evidence: PR #1041 (closes #829), PR #1054 (closes #607), PR #855
  (closes #742); re-verified against
  `crates/app/core/tests/plan_apply_lifecycle_integration.rs` and
  `apps/desktop/src/features/plans/PlanReviewOverlay.test.tsx` · by:
  re-verification pass for issue #1236 (intent-gated)

- **Δ3** 2026-07-20 · S4 · behavior-change
  The shared root cause (#829) is fixed, so a stale plan no longer applies
  with `itemsFailed=0` — a CAS mismatch now pauses the run instead. The
  step's specific expectation (a plan visibly marked stale, with a
  regenerate action offered in place of apply) still does not hold: the
  pause surfaces as an untranslated `pause_reason` string with a
  retry-style Resume action, and the one code path that would compute a
  dedicated `stale` flag (`inbox_plan.rs`'s `plan_row.state == "stale"`)
  is unreachable dead code — no plan-state write in the codebase ever sets
  the literal value `"stale"`. The "unmet" verdict stands; the mechanism
  and evidence behind it changed.
  Evidence: PR #1041 (closes #829); `crates/contracts/core/src/
  lifecycle.rs:60-72` (`PlanState` has no `Stale` variant);
  `crates/app/core/src/inbox_plan.rs:130-135`; `apps/desktop/src/features/
  plans/PlanReviewOverlay.test.tsx` ("Paused — item.stale") · by:
  re-verification pass for issue #1236 (intent-gated)

- **Δ4** 2026-08-24 · +S6, +SC6 · behavior-change
  A move or copy whose destination leaves the chosen folder is now refused at
  apply — `root_escape`, or `path.invalid` for an item with no root and a
  relative path — with the item marked refused and the run continuing;
  previously such a destination was written. The refusal joins the failure
  family this journey exists to explain.
  Evidence: PR #1711 (846a170c0) · by: journey-scribe (intent-gated)

- **Δ5** 2026-08-24 · +S7, +SC7 · behavior-change
  After an unclean shutdown the app now raises interrupted plans itself,
  through a banner offering review-and-resume, and boot reconciliation judges
  filesystem presence by the directory entry: a dangling symlink no longer
  reads as a finished move, and an unjudgeable item is marked failed with an
  ambiguous-state-at-boot reason instead of being assumed complete.
  Evidence: PR #1722 (0f8cff68c) · by: journey-scribe (intent-gated)
