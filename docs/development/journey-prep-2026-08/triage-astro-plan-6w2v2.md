# Triage of 29 static findings from the J01, J02, J03 and J06 prep units

Claimed as `astro-plan-6w2v2`. No product code, journey document or tracked file
was modified. The Windows host was not touched and no journey was run. Checkout
`<repo>` at `38d07af07`.

## Classification table

| # | Journey | Finding | Classification | Bead |
| --- | --- | --- | --- | --- |
| S-F1 | J06 | destructive confirmation never read back | PRODUCT (P3, UX only) | astro-plan-k6771 |
| S-F2 | J06 | no item can display `refused` | PRODUCT (P3) | astro-plan-unv5w |
| S-F3 | J06 | reclaimable total includes protected bytes | PRODUCT (P1) | astro-plan-i5qxc |
| S-F4 | J06 | default settings make every candidate protected | WORKS AS INTENDED (precondition) | astro-plan-bi0w9 |
| S-F5 | J06 | zero-item overlay unreachable, no explanatory text | NOT A FINDING (trace is accurate) | — |
| S-F6 | J06 | S1 "no selection affordance" ambiguous | DOCUMENT | astro-plan-1cdr8 |
| S-F7 | J06 | trace cites two paths that do not resolve | DOCUMENT | astro-plan-1cdr8 |
| S-F8 | J06 | every `PlanReviewOverlay.tsx` line cite still accurate | NOT A FINDING (no drift) | — |
| F1 | J01 | E2.9 premise contradicted by source | DOCUMENT | astro-plan-1cdr8 |
| F2 | J01 | S9 rescan has no failure signal | PRODUCT (P2) | astro-plan-547yi |
| F3 | J01 | S13 "remove the override" has no control | PRODUCT (P3) | astro-plan-pk4hb |
| F4 | J01 | a failed Disable is invisible | PRODUCT (P2) | astro-plan-547yi |
| F5 | J01 | offline source cannot be disabled/re-enabled/rescanned | PRODUCT (P2) | astro-plan-gh9yi |
| F6 | J01 | remap Apply enabled by a failed Verify | PRODUCT (P3) | astro-plan-pk4hb |
| F7 | J01 | S10 server-side remap gate holds | NOT A FINDING (confirms correct) | — |
| F8 | J01 | windows-journeys J01 aid comprehensively stale | DOCUMENT (P2) | astro-plan-u1aax |
| F9 | J01 | `online` is pure path existence | NOT A FINDING (fixture rationale) | — |
| F1 | J02 | stale refusal unreachable on a first Apply | PRODUCT (P2, narrowed) | astro-plan-ouwh4 |
| F2 | J02 | failed inbox rescan is silent | PRODUCT (P2) | astro-plan-547yi |
| F3 | J02 | "reveal not available from Inbox" is stale | DOCUMENT | astro-plan-1cdr8 |
| F4 | J02 | S3 reset path has no general control | PRODUCT (P3) | astro-plan-pk4hb |
| F5 | J02 | Trace removed a claim the UI implements | DOCUMENT | astro-plan-1cdr8 |
| F6 | J02 | G2 stale, spec 058 landed | DOCUMENT | astro-plan-1cdr8 |
| F7 | J02 | absent and unstable selectors | WORKS AS INTENDED (no testid is not a defect) | — |
| F1 | J03 | E6 names a response field no surface renders | DOCUMENT | astro-plan-1cdr8 |
| F2 | J03 | E13 unreachable via per-plan Apply, reason never shown | PRODUCT (P3) | astro-plan-ajw7i |
| F3 | J03 | windows J03 aid contradicts the journey | DOCUMENT (P2) | astro-plan-u1aax |
| F4 | J03 | no organization-state control on Data Sources | WORKS AS INTENDED, surprising | astro-plan-45bo0 |
| F5 | J03 | S2 trace line numbers drifted | DOCUMENT | astro-plan-1cdr8 |
| F6 | J03 | E12 "no filesystem I/O" provable from source | NOT A FINDING (confirms correct) | — |

Twelve beads filed, all with `discovered-from:astro-plan-6w2v2`.

## The four rulings

### 1. Destructive confirmation — the gate is present and BACKEND-enforced

Not a Constitution II problem and not a UI-only confirmation.

`crates/fs/executor/src/run/loop_.rs:415-435` refuses any item where
`requires_destructive_confirm && !destructive_confirmed`, before any filesystem
mutation, with `FailureCode::DestructiveUnconfirmed`. The comment there records
that this replaced an older `confirm_required = is_protected` inversion. The
writer is `crates/persistence/plans/src/repositories/plans.rs:336-358` driven by
`crates/app/core/src/plan_apply/lifecycle.rs:757-782`. The executor reads the
flag off the item row, so a UI that never rendered a checkbox still cannot apply
a destructive item.

What is genuinely absent is read-back into the DTO surface, which is a UX defect
only. `PlanReviewOverlay.tsx:220` starts `destructiveConfirmed` at `false` and
`handleClose` resets it (`:274`), so reopening a review shows an unchecked box
even when the DB flag is 1. Re-checking is harmless: the writer is
`UPDATE ... WHERE destructive_confirmed = 0` (`plans.rs:357-358`). Filed P3 as
`astro-plan-k6771`.

The three things the brief asked me to keep distinct, plus a fourth that is
easily mistaken for one of them:

- `plan.approval_required` — the plan row carries no `approval_token`
  (`crates/app/core/src/inbox_plan.rs:18-25`, `:66-69`). About approval.
- `plan.approval.stale` — a separate error code
  (`apps/desktop/src/bindings/index.ts:3885`).
- `plan.stale` — `plan_row.state == "stale"`, produced by the executor's
  per-item CAS result (`inbox_plan.rs:146`).
- `destructive_unconfirmed` — a per-item `FailureCode`
  (`crates/fs/executor/src/failure.rs:121`), not a plan-level error code at all.
  This is the destructive gate. It is not any of the three above.

### 2. `refused` persisting as `failed` — P3, and the audit record is correct

The claim is true of the plan row and the DTO, false of the durable audit
record, so Constitution II's user-facing history is intact.

`crates/persistence/plans/src/repositories/plan_apply.rs:732-735` maps
`"refused" | "stale"` to `"failed"` before persisting `item_state`, and
`PlanItemState` in the DTO is
`pending | applying | succeeded | failed | skipped | cancelled`
(`bindings/index.ts:7665`).

But `crates/app/core/src/plan_apply/callbacks.rs:445-459` publishes the audit
event carrying `new_state: "refused"` verbatim in the payload, `Outcome::Refused`
and `failure_code` (`destructive_unconfirmed`), and `AuditOutcome` in the DTO
includes `"refused"` (`bindings/index.ts:2662`). So the record that Constitution
II designates as user-facing history does distinguish them. The brief's worry
about losing the distinction in a durable record does not materialise.

The concrete defect is an asymmetry rather than the collapse itself: the same
write persists `item_stale = 1` for a stale item (`plan_apply.rs:742-745`),
giving `stale` a queryable durable marker, while `refused` gets no equivalent
column and survives on the row only as free text in `failure_reason`. Filed P3
as `astro-plan-unv5w`.

### 3. Reclaimable total includes protected bytes — confirmed, P1

`crates/app/core/src/cleanup_generator/scan.rs:87` adds every candidate size to
`total_reclaimable_bytes` before protection is resolved at `:102-106`, and never
subtracts. `CleanupCandidate` carries protection only inside a free-text
`reason` string, so nothing downstream can net it out. Protected items are then
refused at apply, so `cleanup-reclaimable`
(`OutputsCleanupSections.tsx:271-275`) advertises bytes no apply can reclaim.
With shipped defaults (`default_protection: "protected"`,
`protected_categories: ["lights","masters","finals"]`,
`crates/domain/core/src/settings.rs`) the overstatement is the whole total.

The session flow is correct by contrast: `raw-cleanup-reclaimable` sums selected
frames only (`RawFrameCleanupSection.tsx:175`) and protected frames get no
checkbox (`:125`). Correctness defect, filed P1 as `astro-plan-i5qxc`.

### 4. windows-journeys staleness — systemic, but the testid claim is much narrower

The testid half of the reported finding does not survive census. Only ONE file
in the directory mentions a testid at all:

```
rg -c -i testid docs/development/windows-journeys/
→ exit 0, one hit: journey-01-first-run-setup.md:1
```

So ten of eleven files cite zero testids and cannot be stale in that respect.
`journey-01` cites two, `e2e-path-input-<kind>` and `e2e-add-path-btn-<kind>`
(`:70-74`); both are absent (`rg -c e2e-path-input apps/desktop/src` exits 1,
sanity-checked against `rg -c cleanup-scan-btn apps/desktop/src --glob '*.tsx'`
returning four files). That is 2 of 2 absent in `journey-01`, 0 cited elsewhere,
and it is already filed as `astro-plan-npts7`.

The staleness itself is systemic and is provable by provenance instead:

```
for f in docs/development/windows-journeys/*.md; do
  git log --format=%ad --date=short --follow -- "$f" | grep -v 2026-08-22 | head -1
done                                                              → exit 0
```

Ten of eleven files have had no content change since 2026-07-17; `journey-02`
since 2026-07-24. The 2026-08-22 commit touching all eleven is `13353a066`
(MCP bridge gating, #1701), not a content refresh. `docs/journeys/` meanwhile is
on v6-v9 with delta logs through 2026-08-24.

Confirmed wrong expectations, each of which mandates failing correct behaviour:

- `journey-01:12-14` states a 5-step wizard; the product has 8.
- `journey-01` Test 7 says "Disable is reversible, no confirm needed" and
  instructs FAIL if "Disable requires a confirm step". A `ConfirmModal`
  `data-testid="disable-root-confirm"` exists
  (`apps/desktop/src/features/settings/DataSources.tsx:245-262`).
- `journey-01` Test 8 restricts Delete to offline sources; Delete is rendered
  unconditionally (`RootCard.tsx:231`).
- `journey-01:146` says Verify "samples files"; verification is exhaustive.
- `journey-03:82-88` expects the destructive-destination control to be present
  for catalogue actions; it is gated on `hasDestructive`
  (`apps/desktop/src/features/inbox/PlanPanel.tsx:199-201`, `:401`) and
  catalogue actions are not destructive.

**Trustworthiness verdict: no file in the directory is trustworthy for
expectations, step counts or selectors.** Only the environment-mechanics
sections are. Three of three files checked against source were defective, which
is a directory-level problem, not three file-level ones. `docs/journeys/README.md`
points validators here, so the directory is an active source of false findings.
Filed P2 as `astro-plan-u1aax`.

## J02 F1 — the deepest ruling, and it narrows

Corroborated independently by J03, and the shared mechanism is real: self-minted
approval collapses the two-phase approve-then-apply contract into one phase.
That single mechanism produces both symptoms, so patching J03's toast would
leave J02's snapshot gap open.

What holds. `check_cas` compares against the approval-time snapshot
(`crates/fs/executor/src/ops/cas_check.rs:43-96`), and `snapshot_from_metadata`
(`:112-117`) is called only by `approve_plan`
(`crates/app/core/src/plans/approve.rs:90-111`). Every UI apply path mints the
approval inside the Apply gesture:
`apps/desktop/src/features/inbox/useInboxPlanApplyFlow.ts:57-73` (per-plan),
`:97` and `:133` (batch),
`apps/desktop/src/features/plans/PlanReviewOverlay.tsx:295-310`
(Approve & apply), and `crates/app/core/src/plans/auto_apply.rs`. So drift
between plan generation or review and apply is baked into the baseline rather
than detected.

What refutes the stronger "the guard is dead" reading. Three live paths:

1. **Long runs.** The snapshot is taken once at approval, but item N is mutated
   arbitrarily later, so mid-run drift IS detectable on a first apply.
2. **Resume.** `handleResume` (`PlanReviewOverlay.tsx:375-387`) calls
   `plan.resume` with no re-approval, so a paused run resumed later re-executes
   against the original snapshot. Pause-on-stale then resume is the check's
   designed use (`plan_apply.rs` `get_last_stale_item` /
   `revalidate_pause_condition`). Pause is exactly when a user goes and edits
   files.
3. **Mid-run retry.** `plansRetry` / `retry_plan_item` reuses the original
   snapshot.

And `check_cas` has a second purpose that is live regardless:
`FailureCode::SourceMissing` when the path is gone (`cas_check.rs:51-55`). It is
not dead code.

J02's named surviving route does not work as described: it assumed a second
Apply on an already-approved plan keeps the older snapshot, but `approve_plan`
requires `ready_for_review` and `handleApplyOne` needs the returned token, so it
returns early with a generic toast and never applies. The batch paths do reach
it, because `approvePlans` swallows failures with `Promise.allSettled`
(`useInboxPlanApplyFlow.ts:30-32`) and the backend then reads the stored token.

Ruling: a P2 gap, not a defeated guard. The substantive framing is that the
snapshot should be stamped at plan generation or review, not at approval, because
Constitution II promises that what the user reviewed is what gets applied. Filed
as `astro-plan-ouwh4`.

## Absence claims, re-derived with a sanity-checked pattern

| Claim | Command | Result |
| --- | --- | --- |
| no `destructiveConfirmed` in the DTO | `rg -c destructiveConfirmed apps/desktop/src/bindings/index.ts` | exit 1 |
| control for the above | `rg -c destructiveDestination apps/desktop/src/bindings/index.ts` | exit 0, 9 |
| widened, both casings, whole frontend | `rg -in 'destructive_?confirmed' apps/desktop/src packages/contracts` | exit 0, 19 hits, none a DTO field |
| `e2e-path-input-*` does not exist | `rg -c e2e-path-input apps/desktop/src` | exit 1 |
| control for the above | `rg -c cleanup-scan-btn apps/desktop/src --glob '*.tsx'` | exit 0, 4 files |
| `cleanup_generator.rs` does not exist | `ls -d crates/app/core/src/cleanup_generator.rs` | exit 1 |
| control for the above | `ls -1 crates/app/core/src/cleanup_generator/` | exit 0, 6 files |
| no org-state control in Settings | `rg -in 'organi[sz]ation\|orgState\|org-select\|setOrganizationState' apps/desktop/src/features/settings/` | exit 0, hits only in `Advanced.tsx` |
| control for the above | `rg -n org-select apps/desktop/src` | exit 0, `StepSourceFolders.tsx:343` |
| only one windows doc cites a testid | `rg -c -i testid docs/development/windows-journeys/` | exit 0, 1 file |

The `destructiveConfirmed` claim is exactly the convention-shaped-search shape
that the J03 unit caught in itself (`sessions-row-<id>` at
`SessionsTable.tsx:305`), so it was widened twice: to `destructive_?confirmed`
case-insensitively across the frontend and contracts, and separately across
`crates/`. The widened search is what surfaced the backend enforcement site that
overturns the finding's severity.

One search-hygiene hazard worth recording: the token-savings wrapper rewrote the
literal `approval_token` to `n` in displayed `rg` output, which made an approval
census read as near-empty. Every approval-path result above was re-derived by
redirecting `rg` to a file and reading the file.

## Reclassified as NOT A FINDING

- **J06 S-F5** — the zero-item overlay is genuinely unreachable and S3.E6's
  "no explanatory text" holds. Verified: the project Generate control renders
  only under `hasCandidates` (`OutputsCleanupSections.tsx:326`), session
  Generate is disabled at zero selection (`RawFrameCleanupSection.tsx:214`),
  and the cleanup overlay passes no `emptyReason`. The journey's own trace is
  accurate; nothing to file.
- **J06 S-F8** — every `PlanReviewOverlay.tsx` line cite in the journey is still
  accurate. A negative result, correctly reported.
- **J01 F7** — the S10 server-side remap gate is implemented as documented
  (`ensure_remap_verified` writes a `Refused` audit row with
  `remap.not_verified` before returning `ErrorCode::RemapNotVerified`,
  `crates/app/core/src/first_run/root_remap.rs:102-148`). Confirms correct
  behaviour.
- **J01 F9** — `online` is pure path existence
  (`apps/desktop/src-tauri/src/commands/roots.rs:68`). Fixture rationale, not a
  finding.
- **J03 F6** — E12's "no filesystem I/O" is provable from source. Confirms
  correct behaviour.
- **J02 F7** — absent testids on Rescan, toasts, the audit log and `SourceBadge`.
  A missing test hook is a validation-ergonomics fact, not a product defect;
  the prep unit already supplied working text and role fallbacks. Not filed.
- **J06 S-F4** — the prep unit's own self-classification as a precondition
  rather than a defect is confirmed. Recorded in `astro-plan-bi0w9`.

## Not filed, per the known-correct list

`blockPermanentDelete` defaults ON; one corrupt `frame_ids` row refusing
raw-frame cleanup library-wide (`astro-plan-dq9r3`); trash-destination plans
rendering destructive (#1735); inbox apply refusing without recorded approval
(#1740); corrupt FITS via `EvidenceSource::None` (#1737); archive destination
escaping the root refused (#1738); identical camera bodies with different serials
(#1742); spec 051 US9 suppression (`astro-plan-1jz79`); the empty-plan
explanatory-text gap, which belongs to J06 (#603) and not J01; Windows-only
root-overlap case-folding (`astro-plan-zjf88`). Nothing already covered by
PR #1613 (`08a3da549`) was filed either.

## Commands and exit codes

| Command | Exit |
| --- | --- |
| `bd show astro-plan-6w2v2`, `bd update astro-plan-6w2v2 --claim` | 0 |
| `git log --oneline -1` | 0 |
| `rg -c destructiveConfirmed apps/desktop/src/bindings/index.ts` | 1 |
| `rg -c destructiveDestination apps/desktop/src/bindings/index.ts` | 0 |
| `rg -in 'destructive_?confirmed' apps/desktop/src packages/contracts` | 0 |
| `rg -n destructive_confirmed crates/ --stats` | 0 |
| `rg -n 'DestructiveUnconfirmed' crates/ --glob '!*tests*'` | 0 |
| `rg -n '"refused"' crates/ apps/desktop/src packages/contracts` | 0 |
| `rg -n refused crates/persistence/plans/src/repositories/plan_apply.rs` | 0 |
| `rg -n 'PlanItemState =' apps/desktop/src/bindings/index.ts` | 0 |
| `rg -n 'AuditOutcome::Refused\|Outcome::Refused' crates/ --glob '!*tests*'` | 0 |
| `rg -n 'total_reclaimable_bytes\|is_protected\|protect' crates/app/core/src/cleanup_generator/scan.rs` | 0 |
| `rg -n 'PlanApprovalRequired\|plan\.approval_required' crates/ apps/desktop/src packages/contracts` | 0 |
| `rg -n 'plansApprove\|approvalToken' apps/desktop/src/features/plans/ apps/desktop/src/features/projects/` | 0 |
| `rg -o -N 'data-testid="[^"]+"' docs/development/windows-journeys/` | 0 |
| `rg -c -i testid docs/development/windows-journeys/` | 0 |
| `rg -c cleanup-scan-btn apps/desktop/src --glob '*.tsx'` | 0 |
| `rg -c e2e-path-input apps/desktop/src` | 1 |
| `ls -d crates/app/core/src/cleanup_generator.rs` | 1 |
| `ls -1 crates/app/core/src/cleanup_generator/` | 0 |
| `ls -1 docs/development/windows-journeys/` | 0 |
| `git log --follow --format='%ad %h %s' --date=short -- docs/development/windows-journeys/journey-01-first-run-setup.md` | 0 |
| per-file last-content-change loop over `docs/development/windows-journeys/*.md` | 0 |
| `rg -n checkPathExists apps/desktop/src` | 0 |
| `rg -n isOffline apps/desktop/src/features/settings/RootCard.tsx` | 0 |
| `rg -n inbox-reveal-btn apps/desktop/src` | 0 |
| `rg -n inbox-source-group-classify apps/desktop/src` | 0 |
| `rg -in 'organi[sz]ation\|orgState\|org-select\|setOrganizationState' apps/desktop/src/features/settings/` | 0 |
| `rg -n org-select apps/desktop/src` | 0 |
| `rg -n 'confirmDisable\|ConfirmDisable\|confirm' apps/desktop/src/features/settings/DataSources.tsx` | 0 |
| `bd create` x 12 | 0 each |
