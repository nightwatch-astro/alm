# J06 drive plan — cleanup: scan → review → apply

Offline phase A. Prepared 2026-08-25 against the primary checkout
`<repo>` on branch `chore/journey-validation-formula`.
Every conclusion here is **STATIC EVIDENCE, read from source. Nothing was
observed running.** The Windows host was not touched.

Journey source: `docs/journeys/J06-cleanup-scan-review-apply/journey.md` (id J06,
version 6, status draft, last_reviewed 2026-07-14).

## 0. Census

| Item | Count | Command |
| --- | --- | --- |
| Steps | 6 | `grep -cE '^### S' journey.md` |
| `Expect:` bullets | 10 | `grep -cE '^\- \*\*Expect:\*\*' journey.md` |
| `Expect (negative):` bullets | 10 | `grep -cE '^\- \*\*Expect \(negative\):\*\*' journey.md` |
| Expect bullets per step | S1 2, S2 3, S3 5, S4 4, S5 3, S6 3 | `awk '/^### S/{s=$2} /^\- \*\*Expect/{c[s]++} END{...}'` |
| Success criteria | 8 (SC1–SC8) | `grep -cE '^\- SC[0-9]' journey.md` |
| Known gaps | 11 (G1–G11) + 1 "Dropped" note | `grep -cE '^\- G[0-9]' journey.md` |
| Delta-log entries | 5 (Δ2–Δ6; no Δ1) | `grep -cE '^\- \*\*Δ' journey.md` |
| `deltas/` files still on disk | 0 | `ls docs/journeys/J06-cleanup-scan-review-apply/deltas/` → exit 1, no such directory |

All patterns above returned non-zero counts, so none is a silent-zero.

## 1. Expectation inventory

Numbering: `S<step>.E<n>` in document order; `(neg)` marks a negative Expect.
"Signal" is the class of observable the cross-cutting rule demands.

### S1 — Scan a project's outputs

- **S1.E1** — grouped preview (Intermediates/Masters/Finals), per-item size and
  confidence, protected items locked with no selection affordance, total
  reclaimable size; empty result instead of an empty table when no candidates.
  Signal: **visible state** — `cleanup-group-<dataType>` blocks,
  `cleanup-candidate-<index>` rows, `cleanup-reclaimable`, and the
  `projects_cleanup_no_candidates_title` empty state.
- **S1.E2 (neg)** — no plan created, nothing moved/renamed/deleted; two scans on
  an unchanged project give the same grouping and total. Signal: **visible state
  + filesystem diff** (compare a recursive listing/hashes of the throwaway root
  before and after; plans list unchanged).

### S2 — Choose a destination and generate

- **S2.E1** — a real reviewable plan 1:1 with the candidates in scope; the
  destination is fixed and read-only in the review overlay from here on.
  Signal: **navigation/overlay open** (`plan-review-overlay` appears) + the
  modal subtitle carrying `<n> items · Archive folder|System trash`.
- **S2.E2** — destination governs only policy-Archive items; a policy-Delete
  item is removed under either choice. Signal: **visible state** — per-item
  `action` pill in `plan-review-items` reads `delete` for those items.
- **S2.E3 (neg)** — nothing touched on disk by generating; destination cannot be
  changed after generation without discard+restart. Signal: **filesystem diff**
  plus **absence of a control** (no destination control inside the overlay).

### S3 — Review the plan

- **S3.E1** — every item 1:1; protected items require per-item acknowledgement
  before "Approve & apply" is clickable; approve disabled at zero items;
  "Discard" leaves disk untouched. Signal: **visible state** (disabled
  attribute on `plan-review-approve-apply`) + **filesystem diff** for Discard.
- **S3.E2** — `plan-review-approve-apply` renders red destructive exactly when
  some item action is `delete` OR the S2 destination is System trash.
  Signal: **visible state** — `data-variant` attribute (see §2).
- **S3.E3** — destructive-confirm checkbox
  `plan-review-confirm-destructive` labelled "I confirm these items may be
  deleted."; approve stays disabled until checked; the confirmation persists per
  item and survives closing and reopening the review; trash plans gated like
  `delete` plans in overlay, confirmation writer, and apply refuser.
  Signal: **visible state**. **The "survives closing and reopening" clause has
  no observable — see S-F1.**
- **S3.E4** — each item's action pill renders in the danger style under the same
  rule. Signal: **visible state** — pill `data-variant="danger"`.
- **S3.E5 (neg)** — no destructive item applied without a recorded
  confirmation: the executor refuses with `destructive_unconfirmed` and marks it
  `refused`. Signal: **per-item error** — `plan-review-item-result-<index>`.
  **The state word `refused` has no observable — see S-F2; and the message is
  unmapped — G8.**
- **S3.E6 (neg)** — approve stays disabled while an acknowledgement is
  outstanding or the plan is empty, with **no** explanatory text; and the
  zero-item overlay state is unreachable through the documented path. Signal:
  **absence of an observable** — `plan-review-empty-reason` must not exist.
  The unreachability half is not drivable at all; it is a source claim
  (verified statically, see S-F5).

> The journey lists five `Expect` bullets under S3; the inventory splits the
> first into E1 and renumbers, giving six entries for five bullets because the
> "Discard" clause carries its own distinct observable.

### S4 — Approve and apply

- **S4.E1** — live per-item progress "Applying N of M…"; each item's outcome
  visible afterward; moved files present at the Archive destination; a re-scan
  shows applied items gone. Signal: **visible state** (`plan-review-progress`),
  **per-item error/result**, **filesystem diff**, then **visible state** again.
- **S4.E2** — a policy-Delete item is permanently removed even when the
  destination is Archive folder, with no archive copy and no rollback (G7).
  Signal: **filesystem diff** — file absent at source and absent under the
  archive folder.
- **S4.E3 (neg)** — the overlay never reports fully applied while an outcome is
  unknown; a failed item's reason is shown, not silently skipped. Signal:
  **per-item error**.
- **S4.E4 (neg)** — no policy-Delete item removed without the S3 confirmation
  recorded. Signal: **per-item error** + **filesystem diff**.

### S5 — Scan a session's raw sub-frames

- **S5.E1** — per-frame preview with type, size, protection; non-protected
  frames preselected; protected frames show no selection control; reclaimable
  total reflects only selected frames; a per-root "Unprotected" override
  governs the classification for its session-attributed frames.
  Signal: **visible state** — `raw-cleanup-candidate-<frameId>`,
  `raw-cleanup-select-<frameId>` presence/checked, `raw-cleanup-reclaimable`.
- **S5.E2 (neg)** — nothing moved or altered by scanning. Signal: **filesystem
  diff**.
- **S5.E3 (neg)** — no candidate list at all while any session's frame
  attribution is unreadable; library-wide refusal in a red banner naming the
  affected session ids; blocking, non-retryable; no in-app repair (G10).
  Signal: **refusal reason** — a `role="alert"` banner inside
  `session-raw-cleanup` (see §2 for the exact text).

### S6 — Select frames and generate a session plan

- **S6.E1** — a plan for the selected frames handing off to the same
  review/apply flow; "Generate cleanup plan" disabled while no frame is
  selected. Signal: **navigation/overlay open** + **visible state** (disabled
  attribute on `raw-cleanup-generate-btn`).
- **S6.E2 (neg)** — nothing moves until the plan is approved and applied.
  Signal: **filesystem diff**.
- **S6.E3 (neg)** — no plan generated while any session's attribution is
  unreadable, on the same library-wide condition. Signal: **refusal reason** —
  a `role="alert"` banner in the generate area.

### Expects with no drivable observable

1. **S3.E3, persistence clause** — "the confirmation persists per item so it
   survives closing and reopening the review". No UI observable exists (S-F1).
2. **S3.E5 (neg), the word `refused`** — no such value can appear (S-F2).
3. **S3.E5 (neg), the message** — `destructive_unconfirmed` has no user-facing
   mapping; already recorded as G8.
4. **S3.E6 (neg), unreachability clause** — a claim about what cannot be
   reached; a driving run can only fail to produce it, never confirm it.
5. **S4.E4 (neg)** — same signal as S3.E5; blocked by the same G8 message gap.
6. **S5.E1, per-root-override clause** — needs a per-root "Unprotected"
   override plus session-attributed frames under that root; drivable, but only
   with the Settings → Data Sources override surface, which this plan does not
   have a verified selector for (see §2 unverified list).

## 2. Selector map

Every entry below was read from source. Where no `data-testid` exists the
fallback is named explicitly. **Nothing here is inferred from a naming
convention.**

### Framework facts that decide the fallbacks

- `Banner` (`apps/desktop/src/ui/Banner.tsx:25-34`) styles itself with
  vanilla-extract `variantStyles` (`Banner.css.ts:21`) — the class names are
  **hashed**, so `.pv-banner--danger` does not exist. Use the implicit role:
  `variant="danger"` → `role="alert"`, `variant="warn"` → `role="status"` with
  `aria-live="polite"`, `variant="info"` → **no role at all**.
- `Btn` sets `data-variant={variant ?? 'default'}` (`ui/Btn.tsx:32`); variants
  are `primary | danger | destructive | ghost` (`:8`).
- `Pill` sets `data-variant={variant}` (`ui/Pill.tsx:31`).
- `Table` applies a row's `_testid` as `data-testid` on the `<tr>`
  (`ui/Table.tsx:266`) and passes a `data-testid` prop through to the `<table>`
  element; `scrollTestId` is a **separate** prop for the scroll wrapper
  (`:158`), and `PlanReviewOverlay` does not pass it.
- `Section` puts `data-testid` on the outer div but renders children **only when
  open** (`ui/Section.tsx:24,48`); the toggle is
  `[role="button"][aria-expanded]` (`:27-29`) — no testid.
- Toasts: `ToastContainer` is mounted at `app/Shell.tsx:171`. Each toast is
  `div[role="alert"].pv-toast__item` with the text inside
  `span.pv-toast__message`, plus `pv-toast__item--info` for `variant: 'info'`
  (`ui/ToastContainer.tsx:18-40`). **Auto-dismiss default is 5000 ms**
  (`shared/toast.ts:88`) — sample within that window or you will report
  "no toast" for a toast that fired.

### Project cleanup flow (`features/projects/OutputsCleanupSections.tsx`)

Mounted at `features/projects/ProjectBottomDetail.tsx:114` with
`defaultOpen={true}`.

| Observable | Selector | Source |
| --- | --- | --- |
| Cleanup section root | `[data-testid="project-cleanup-preview"]` | `:241` |
| Section title | key `projects_cleanup_title` = "Cleanup preview" | `:238` |
| Scan button | `[data-testid="cleanup-scan-btn"]` | `:262` |
| Scan button label | `projects_cleanup_scan_btn` = "Scan for cleanup candidates"; pending `projects_cleanup_scanning` = "Scanning…" | `:264-266` |
| Reclaimable total | `[data-testid="cleanup-reclaimable"]`, `projects_cleanup_reclaimable` = "{size} reclaimable" | `:271-275` |
| Scan failed | `role="alert"` banner, `projects_cleanup_scan_failed` = "Could not scan for cleanup candidates." | `:281` |
| Empty result | `projects_cleanup_no_candidates_title` = "No cleanup candidates" / desc "The cleanup policy keeps every data type, or no processing artifacts have been observed for this project." | `:286-287` |
| Protection hint (shown once, only when some candidate is protected) | `[data-testid="cleanup-protected-note"]`, `projects_cleanup_row_protected_hint` = "Protected — requires explicit acknowledgement during plan review before it can be applied" | `:293-294` |
| Group block | `[data-testid="cleanup-group-intermediate"]`, `…-master`, `…-final` | `:303` |
| Group heading labels | "Intermediates" / "Masters" / "Finals" (`projects_cleanup_type_*`) | `:136-140` |
| Group meta | `projects_cleanup_group_meta` = "{count} files · {size}" | `:310` |
| Candidate row | `[data-testid="cleanup-candidate-<index>"]` (0-based, per group render) | `:165` |
| Candidate columns | "File", "Size", "Confidence", "Protection" | `:149-152` |
| Confidence cell | `projects_cleanup_confidence_pct` = "{pct}%" | `:176` |
| Protected cell | `[data-variant="warn"]` pill reading "Protected" (`settings_cleanup_protection_protected`) + a decorative lock icon | `:178-185` |
| Normal cell | `[data-variant="ghost"]` pill reading "Normal" | `:187` |
| Destination label | `projects_cleanup_dest_label` = "Destructive destination"; the radio group carries it as `aria-label` | `:330,333` |
| Destination options | "Archive folder" (`plans_dest_archive`) hint "App-managed archive folder — reversible until you empty it"; "System trash" (`plans_dest_trash`) hint "OS-native recycle bin / trash" | `:337-343` |
| Generate button | `[data-testid="cleanup-generate-btn"]`, label `projects_cleanup_generate_btn` = "Generate cleanup plan", pending "Generating…" | `:357-361` |
| Generate failed | `role="alert"` banner, "Could not generate the cleanup plan." | `:365` |
| Plan-created toast | toast text `projects_cleanup_plan_created_toast` = "Cleanup plan created with {count} items — review before anything is applied." | `:225` |
| Review overlay title | `projects_cleanup_review_title` = "Review cleanup plan" | `:403` |
| Protected-categories panel (always present, always locked) | `[data-testid="cleanup-protected"]`, `projects_cleanup_protected_label` = "Protected — never proposed for cleanup" | `:374,377` |

**Selector caution for S1.E1:** the project candidate table has **no selection
column at all** (`candidateColumns()` at `:147-153` is File/Size/Confidence/
Protection). "Protected items shown locked with no selection affordance" is
therefore true of every row, protected or not; the only protected/unprotected
difference is the pill plus the decorative lock. Do not look for a checkbox that
is absent for protected rows — there is none for any row.

**No destination control exists inside the overlay**, which is how S2.E3's
"cannot be changed after generation" reads: assert the absence of any
`RadioGroup` inside `plan-review-overlay`, not a disabled one.

### Plan review overlay (`features/plans/PlanReviewOverlay.tsx`)

| Observable | Selector | Source |
| --- | --- | --- |
| Overlay root | `[data-testid="plan-review-overlay"]` | `:575` |
| Subtitle (item count · destination) | modal subtitle `"{n} items · Archive folder"` / `"… · System trash"` | `:563-567`, `:130-136` |
| No-mutation banner | `role="status"` (info variant has no role; this one is `variant="info"` → **no role**) — match on text `plans_review_no_mutation_note` = "Nothing has been changed on disk. Review every proposed item below; applying requires explicit approval." | `:584-588` |
| Free-space estimate | `[data-testid="plan-review-free-space"]` | `:597` |
| Empty-reason banner | `[data-testid="plan-review-empty-reason"]` — **never rendered by the cleanup flow**, which passes no `emptyReason`; only the archive flow does (`ProjectDetail.tsx:462`) | `:613-616` |
| Items table | `[data-testid="plan-review-items"]` (the `<table>`) | `:630` |
| Item row | `[data-testid="plan-review-item-<index>"]` | `:439` |
| Item action pill | inside the row: `[data-variant="danger"]` when destructive, `[data-variant="info"]` otherwise; text is the raw action word (`archive` / `delete` / `move`) | `:446-448` |
| `to` cell for delete items | `plans_review_deletion_target` = "Deleted, not moved" | `:454` |
| Item protection pill | "Protected" (`data-variant="warn"`) / "Normal" (`data-variant="ghost"`) | `:461-463` |
| Item result | `[data-testid="plan-review-item-result-<index>"]`; `pending` renders `common_none` = "None", otherwise a pill of the state word plus the raw `failureReason` text | `:474-497` |
| Item columns | Item, Action, From, To, Protection, Link kind, Result, Reason, Linked | `:422-432` |
| Approve & apply | `[data-testid="plan-review-approve-apply"]` | `:547` |
| **Destructive styling (SC6)** | `[data-testid="plan-review-approve-apply"][data-variant="destructive"]` vs `[data-variant="primary"]` | `:538` + `Btn.tsx:32` |
| Approve label | `plans_review_approve_apply_btn` = "Approve & apply"; approving "Approving…"; running `common_applying` = "Applying…" | `:549-553` |
| Approve disabled predicate | `busy \|\| !gateReady \|\| plan == null \|\| plan.itemsTotal === 0 \|\| (hasDestructiveItems && !destructiveConfirmed)` | `:540-546` |
| Destructive-confirm checkbox | `[data-testid="plan-review-confirm-destructive"]` (an `<input type="checkbox">`) | `:653` |
| Its label / aria-label | `plans_review_confirm_destructive_label` = "I confirm these items may be deleted."; confirming shows `common_confirming` = "Confirming…" | `:652,656-658` |
| Discard | no testid — button text `plans_review_discard_btn` = "Discard plan" | `:525` |
| Progress region | `[data-testid="plan-review-progress"]`, `role="status" aria-live="polite"`, text `plans_review_progress_running` = "Applying {applied} of {total}…" | `:676-682` |
| Applied pill | `plans_review_progress_done` = "{count} items applied" | `:704` |
| Apply failed pill | `plans_review_apply_failed` = "Could not apply plan." | `:708` |
| Failed count | `plans_review_progress_failed` = "{count} items failed" | `:711` |
| Cancel run | `[data-testid="plan-review-cancel-run"]` | `:694` |
| Paused badge / resume | `[data-testid="plan-review-paused-badge"]`, `[data-testid="plan-review-resume"]`, `[data-testid="plan-review-resume-stalled-badge"]` | `:715,725,736` |
| Retry (terminal failed/partial/cancelled) | `[data-testid="plan-review-retry"]` | `:517` |
| Reopen (approved) | `[data-testid="plan-review-reopen"]` | `:532` |
| Load error | `role="alert"` banner, `plans_review_load_error` = "Could not load plan." | `:580` |
| Overlay default title | `plans_review_overlay_title` = "Review plan" (overridden per flow) | `:562` |

**Virtualisation trap for S3.E1 / S2.E1 ("1:1"):** the items table is
`virtualized` with `overscan: 8` (`PlanReviewOverlay.tsx:628`,
`Table.tsx:183-189`), so rows outside the scroll viewport are **absent from the
DOM** and `[data-testid^="plan-review-item-"]` will undercount. Verify 1:1 from
the modal subtitle's `plan.itemsTotal`, or scroll `.pv-listtable__scroll` and
accumulate. `[data-testid="table-spacer"]` (`Table.tsx:372,389`) is present when
windowing is active.

### Protection gate (`features/plans/PlanProtectionGate.tsx`)

**This component has zero `data-testid` attributes** (`rg -n data-testid` on the
file returns nothing). Every observable is text or role:

| Observable | Selector | Source |
| --- | --- | --- |
| Outstanding count pill | `plans_gate_require_ack` = "{done} of {total} require acknowledgement" | `:158` |
| All acknowledged pill | `plans_all_acknowledged` = "All acknowledged" | `:157` |
| Instruction line (outstanding) | `plans_review_acknowledge` = "Review each protected item below — protection is permanent and cannot be overridden by acknowledging it." | `:161` |
| Instruction line (done) | `plans_may_proceed` = "Acknowledging does not override protection — these items are permanently excluded from archive/move/delete and will not be applied." | `:161` |
| Per-item acknowledge button | button text `plans_gate_acknowledge_btn` = "Acknowledge" (rendered only while not acknowledged) | `:210-213` |
| Acknowledged marker | pill `plans_gate_acknowledged` = "Acknowledged" | `:180` |
| Per-item note | `plans_protected_item_note` = "Protected — this item is excluded from archive/move/delete and will not be applied, regardless of any rewritten action shown above." | `:192` |
| No protected items | `plans_gate_no_protected` = "No protected items." | `:134` |

Note the shipped copy already tells the user that acknowledging does **not**
override protection. Journey S3.E1's wording ("acknowledged … before Approve &
apply becomes clickable") is about the button gate only; the protected item
still will not be applied. Do not read the UI copy as a contradiction of the
journey.

### Session raw-frame flow (`features/sessions/RawFrameCleanupSection.tsx`)

Mounted at `features/sessions/SessionDetail.tsx:411` with
**`defaultOpen={false}`** — the section body is not in the DOM until expanded.
Expand with `[data-testid="session-raw-cleanup"] [role="button"][aria-expanded="false"]`.

| Observable | Selector | Source |
| --- | --- | --- |
| Section root | `[data-testid="session-raw-cleanup"]` | `:156` |
| Section title | `sessions_rawcleanup_title` = "Raw sub-frame cleanup" | `:153` |
| Scan button | `[data-testid="raw-cleanup-scan-btn"]`, label `sessions_rawcleanup_scan_btn` = "Scan for cleanup candidates", pending "Scanning…" | `:163-167` |
| Reclaimable (selected only) | `[data-testid="raw-cleanup-reclaimable"]`, "{size} reclaimable" | `:172-176` |
| **Scan refusal (S5.E3)** | `role="alert"` banner inside the section, text = the backend message verbatim | `:181-182` |
| Empty | `sessions_rawcleanup_empty_title` = "No raw sub-frame cleanup candidates" | `:186` |
| Candidate row | `[data-testid="raw-cleanup-candidate-<frameId>"]` | `:124` |
| Select checkbox (absent for protected frames) | `[data-testid="raw-cleanup-select-<frameId>"]`, `aria-label` `sessions_rawcleanup_select_aria` = "Select {path} for cleanup" | `:130-133` |
| Columns | "File", session frame type column, "Size", "Protection" | `:48-51` |
| Generate button | `[data-testid="raw-cleanup-generate-btn"]`, label `sessions_rawcleanup_generate_btn` = "Generate cleanup plan", pending "Generating…"; `disabled` while `selected.size === 0` | `:210-219` |
| Generate refusal (S6.E3) | `role="alert"` banner in the generate block | `:221-223` |
| Review overlay title | `sessions_rawcleanup_review_title` = "Review raw sub-frame cleanup plan" | `:232` |

**Exact refusal text (S5.E3 / S6.E3), not i18n'd — it is a backend English
string and will read the same in any locale**
(`crates/app/core/src/cleanup_generator/raw_frames.rs:88-98`):

> frame-to-session attribution is unreadable for `N` session(s) (`ids`), so raw
> sub-frame cleanup cannot tell an unattributed frame from one of theirs. Repair
> or re-import those sessions first.

Both banners render `errMessage(error)`, so the assertion is on that text inside
a `role="alert"` element — there is no dedicated refusal testid.

### Settings → Cleanup (precondition surface, `features/settings/Cleanup.tsx`)

| Observable | Selector | Source |
| --- | --- | --- |
| Per-type action control | `[aria-label="Action for Intermediate files"]` / `"… Calibration masters"` / `"… Final images"` (`settings_cleanup_action_aria` = "Action for {type}") | `:300-302`, `:57-67` |
| Its options | "Keep" / "Archive" / "Delete" (`settings_cleanup_action_*`); `dangerValue="delete"` | `:288-299` |
| Default protection select | a `<select>` in the row labelled "Default protection" with options "Protected" / "Unprotected" | `:233-253` |
| Block permanent delete | `Toggle` in the row labelled "Block permanent delete" | `:219-231` |
| Protected-category warning | `role="alert"` banner, "Protected categories are set to a destructive action: {types}. These are costly to reacquire — confirm this is intentional." | `:268-270` |
| Section titles | "Source Protection", "Cleanup Policy" | `:210,259` |

### Selectors I could NOT verify in source

1. **Settings → Data Sources per-root protection override** (needed for the
   S5.E1 override clause). `SourceProtectionOverride.tsx` is referenced in a
   comment at `crates/app/core/src/cleanup_generator/scan.rs`, but I did not
   locate and read the component, so I name no selector for it. Treat the
   override clause as unmapped rather than guessing.
2. **Session frame-type column header** — I read the key
   (`sessions_frame_inventory_col_type`, `RawFrameCleanupSection.tsx:49`) but did
   not resolve its en-GB value.
3. **`Modal` subtitle element** — I relied on the subtitle *text*; I did not read
   `ui/Modal.tsx`, so I name no structural selector for it.
4. **`SegControl` internals** — the per-type policy control is addressed by its
   `aria-label` on the group; I did not read `SegControl` to learn whether each
   option is a `button`, `input[type=radio]`, or something else.
5. **`RadioGroup` option internals** — same: the group's `aria-label` is
   verified, the per-option element type is not.
6. **`Lock` icon** — rendered `decorative` in project candidate rows
   (`OutputsCleanupSections.tsx:183`), so it is very likely `aria-hidden` and not
   addressable by role; I did not read the component to confirm.

## 3. Fixture recipe

**MOCK DATA ONLY.** Never read, copy, or reference a real astrophotography file
or a live library path.

### Reused from `astro-plan-mg6h8` (environment recipe, unchanged)

- Windows host `<journey-host>`, checkout `C:\dev\astro-plan`, already updated to
  current main; node v24.19.0 / npm 11.17.0 present; `pnpm install` needs
  `CI=true` (fails with `NO_TTY` otherwise); Vite on `127.0.0.1:5173` with mocks
  OFF; `desktop_shell.exe` built with `--features dev-tools,e2e`.
- Session-1 launch: `schtasks /Create` with `/RU <journey-host>\<user> /IT`, then `/Run`,
  scheduling `scripts/win-native-dev.ps1 -McpBridge` (which already sets
  `--features dev-tools` and `PV_MCP_BRIDGE_ENABLE=1`,
  `win-native-dev.ps1:68-69,85-89`).
- MCP bridge port **9223** (pinned client 0.11.2 in `.mcp.json`; `@latest` is
  0.12.0 and fails silently at connect).
- `cargo.exe` is a rustup symlink a non-interactive SSH session cannot execute —
  use `rustup run stable cargo`.

`astro-plan-mg6h8` carries **no** file-level fixture recipe (`rg -i
"fixture|synthetic|FITS|header"` over its comment body returns two hits, both
about using a throwaway root). Everything in the rest of this section is **added
by this unit**.

### What J06 actually needs (added)

Three facts decide the fixture, all read from source:

1. **The default cleanup policy is all-`Keep`**
   (`crates/app/core/src/cleanup_generator/policy.rs`,
   `default_cleanup_policy()`), so a default install yields **zero** cleanup
   candidates and S1 cannot show a grouped preview. A data type must be opted
   into Archive or Delete first.
2. **Candidates come from the `artifacts` table, not from disk.**
   `scan_with_policy` enumerates
   `artifacts_repo::list_artifacts_for_project(pool, project_id, &["present"])`
   and skips `Unclassified`. A file merely dropped in a folder is not a
   candidate until artifact observation has recorded it `present`.
3. **`default_protection` defaults to `"protected"`** and
   `protected_categories` defaults to `["lights","masters","finals"]`
   (`crates/domain/core/src/settings.rs`, defaults block), and
   `block_permanent_delete` defaults `true`. The 2026-07-14 run recorded that
   with `defaultProtection=protected` **every** candidate failed apply with
   `protected.source` (#807), and only succeeded end-to-end with protection set
   to Normal.

Classification is by **file name**, case-insensitively, prefix or suffix on the
stem (`crates/workflow/artifacts/src/rules.rs:86-108`). Verified default rules
(`crates/workflow/artifacts/src/default_rules.rs:34-158`):

| Name shape | Match | Kind | Confidence |
| --- | --- | --- | --- |
| `MasterDark*`, `MasterFlat*`, `MasterBias*` | prefix | Master | 0.95 |
| `master_dark*`, `master_flat*`, `master_bias*` | prefix | Master | 0.95 |
| `*_combined` (stem) | suffix | Final | 0.85 |
| `*_c` (stem) | suffix | Final | 0.80 |
| `result*` | prefix | Final | 0.80 |
| `integration_*` | prefix | Intermediate | 0.90 |
| `*_ABE`, `*_DBE` (stem) | suffix | Intermediate | 0.88 |
| `pp_*` | prefix | Intermediate | 0.88 |

### Files to create (synthetic, realistic headers)

Write real FITS with `astropy.io.fits` on the host, or the repo's own XISF
writer. Header sets below are realistic and internally consistent; adjust
`DATE-OBS` to the run date.

Project output folder (drives S1–S4), one of each kind:

- `integration_M31_Ha.fits` — Intermediate, the only type safe to opt into a
  destructive action. `IMAGETYP='LIGHT'`, `INSTRUME='ZWO ASI2600MM Pro'`,
  `TELESCOP='SW Esprit 100ED'`, `FILTER='Ha'`, `EXPTIME=300.0`,
  `XBINNING=1`, `YBINNING=1`, `CCD-TEMP=-10.0`, `SET-TEMP=-10.0`, `GAIN=100`,
  `OFFSET=50`, `NAXIS1=6248`, `NAXIS2=4176`, `OBJECT='M31'`,
  `DATE-OBS='2026-08-20T22:14:03'`, `STACKCNT=24`.
- `integration_M31_OIII.fits` — second Intermediate so grouping and the total
  are non-trivial; same header with `FILTER='OIII'`, `STACKCNT=18`.
- `MasterDark_300s_-10C_g100_bin1.fits` — Master. `IMAGETYP='DARK'`,
  `EXPTIME=300.0`, `CCD-TEMP=-10.0`, `GAIN=100`, `XBINNING=1`,
  **`STACKCNT=40`** and **`NCOMBINE=40`** (a master must carry these).
- `M31_HaOIII_combined.fits` — Final. `IMAGETYP='LIGHT'`, `OBJECT='M31'`,
  `STACKCNT=42`, `NCOMBINE=42`.
- `notes_readme.txt` — must classify Unclassified and must therefore **never**
  appear as a candidate; this is the control that proves the scan is filtering
  rather than listing the folder.

Session raw sub-frames (drives S5–S6), under the same throwaway root, in a
session folder:

- `M31_Light_Ha_300s_0001.fits` … `_0004.fits` — `IMAGETYP='LIGHT'`, headers as
  the Intermediate above, no `STACKCNT`.
- `Dark_300s_-10C_0001.fits`, `_0002.fits` — `IMAGETYP='DARK'`.
- `Flat_Ha_bin1_0001.fits` — `IMAGETYP='FLAT'`, `EXPTIME=2.5`.
- `Bias_g100_bin1_0001.fits` — `IMAGETYP='BIAS'`, `EXPTIME=0.0`.

Give each file real pixel data of the stated dimensions (a small non-zero
random or gradient array is fine) so `size_bytes` is non-zero — the 2026-07-14
run noted `size_bytes` reading 0 everywhere, and an empty stub cannot
distinguish that defect from a fixture mistake.

### Setup order (each step is SETUP, never an Expect substitute)

1. Create the throwaway root (§4) and write the files above.
2. Add the root as a data source and let ingest/scan record the frames, so
   session attribution exists for S5.
3. Create a project and attach the output folder so artifact observation records
   the outputs `present` (`artifact.watcher.attach` → `artifact.list`; the
   Windows doc names these).
4. **Settings → Cleanup → Cleanup Policy**: set "Intermediate files" to
   **Archive** for the S2/S4 Archive happy path. For the S4.E2 policy-Delete
   expectation set it to **Delete** in a separate pass — with "Block permanent
   delete" considered (default ON).
5. **Settings → Cleanup → Source Protection**: set "Default protection" to
   **Unprotected** for the S4 happy path. Run S3's protection-acknowledgement
   expectation in a *separate* pass with it left at "Protected".
6. Confirm the scan lists exactly the intended candidates and that
   `notes_readme.txt` is absent from every group before generating anything.

## 4. Precondition and teardown

- **Throwaway root, unique to J06 and to this attempt:**
  `C:\pv-journey\J06-<UTC yyyymmddHHMM>\` — e.g.
  `C:\pv-journey\J06-202608251530\`. Create it yourself, verify it is empty
  before writing, and record the literal path in the run report. J06 shares the
  host with other units, so never reuse another journey's root and never write
  under `C:\dev\astro-plan`.
- **App start state:** past first-run setup, on the project detail view for the
  fixture project. The Cleanup section is open by default; the session
  Raw sub-frame cleanup section is **collapsed** by default and must be expanded.
- **Two settings passes.** The protection and policy values in §3 step 4–5
  differ between the S3 acknowledgement expectation and the S4 apply
  expectation. Decide the pass order up front; changing them mid-plan
  invalidates an already-generated plan's premises.
- **Archive destination:** the app writes to `.astro-plan-archive/<planId>/`
  (observed in the 2026-07-14 run). It must land inside the throwaway root —
  an archive destination escaping the library root is refused by design
  (PR #1738), which is correct behaviour and not a defect.
- **Teardown:** delete the whole throwaway root, including
  `.astro-plan-archive/`; empty the OS recycle bin **only** of items you put
  there in the trash-destination pass, and say so; remove the fixture data
  source and project, or leave them and say explicitly what you left; restore
  Settings → Cleanup to Keep/Protected so the next unit does not inherit a
  destructive policy. Report the state you left behind either way.
- **Do not** run the S4 trash pass or any delete unattended on the owner's live
  console session without saying so in the report.

## 5. Known-gap list — do NOT re-file these

From the journey's own Known gaps and Delta log, plus the shared
known-correct list in `/tmp/u-common.md`.

Still open, expect them:

| Gap | What it is | Recorded as |
| --- | --- | --- |
| G7 | The Archive-folder hint says "reversible until you empty it" (verified text, `projects_cleanup_dest_archive_hint`), which does not hold for policy-Delete items in the same plan | `astro-plan-8zz72` |
| G8 | `destructive_unconfirmed` has no user-facing message; the refusal surfaces through the generic apply-failed path, so it is not validatable as a distinct message | journey Known gaps (no bead named) |
| G9 | The Trash-destination apply path and the protected-item acknowledgement path are not stepped in S1–S6; validating them needs a protected candidate and an OS trash | journey Known gaps |
| G10 | One unreadable `frame_ids` row blocks raw sub-frame cleanup **library-wide** with no in-app repair, so S5–S6 dead-end until the DB is repaired outside the app | `astro-plan-dq9r3` |
| G11 | A trash-destination plan still shows an archive path in the review table's destination column, so the row text disagrees with what apply does | `astro-plan-5jfcc` |

Dissolved — do not re-file, and do not expect the old behaviour:

| Gap | Resolution |
| --- | --- |
| G1 (trash destination failed every apply item) | dissolved 2026-07-15, was issue #741 |
| G2 (protected-item acknowledgement cosmetic) | dissolved 2026-07-15, was issue #807 — and `deltas/2026-07-14-q15-t123.md` is marked in the journey's `trace:` as "superseded by current code — see G2" |
| G3 (applied plans lacked durable audit rows) | dissolved 2026-07-15, was issue #766 |
| G4 (no free-space estimate at review) | dissolved 2026-07-15, was issue #876 — the estimate now renders at `plan-review-free-space` |
| G5 (reopen reconcile misreported candidates) | dissolved 2026-07-15, was issue #780 |
| G6 (a System-trash plan reviewed as an ordinary move) | dissolved 2026-08-24 by PR #1735, `astro-plan-cricc` closed |
| "requires PR #413 (open)" | stale; PR #413 merged 2026-07-04, the scan/review/generate UI is shipped |

Correct-by-design behaviours from `/tmp/u-common.md` that will look like bugs:

- `blockPermanentDelete` defaults **ON**
  (`crates/domain/core/src/settings.rs`), so a default install refuses a
  permanent delete.
- A trash-destination plan renders **destructive** and requires the confirmation
  (PR #1735). That is Δ5, not a defect.
- An archive destination escaping the library root is **refused** (PR #1738).
- Inbox plan apply refuses without a recorded approval, `plan.approval_required`
  (PR #1740) — relevant only if you touch an inbox plan incidentally.

Behaviour changes since the 2026-07-14 run — expect the NEW behaviour:

- **Δ2** (PR #894, fixes #563): a per-root protection override now governs
  session-attributed raw frames; previously cosmetic there.
- **Δ3** (PR #1190): "Approve & apply" is red only when the plan is destructive,
  no longer unconditionally.
- **Δ4** (PR #855): a `delete` plan requires the destructive confirmation.
- **Δ5** (PR #1735): a trash-destination plan is destructive too.
- **Δ6** (PR #1739): an unreadable frame attribution refuses raw-frame cleanup
  library-wide, naming the sessions.

Prior-run baseline, `docs/development/journey-run-2026-07-14.md` Journey 6
(build 7e522c16, verdict PARTIAL, 2 PASS / 1 FAIL / 1 PARTIAL): **only the
project-level flow was exercised — S5 and S6 were never run.** The session
flow is therefore first-time coverage for your run. That run also filed #804
(Settings → Cleanup per-type table was a disconnected fixture with no real
policy control); **that is now fixed** — `features/settings/Cleanup.tsx:257`
carries a real `cleanup.policy.get`/`update`-backed control, so the policy no
longer has to be set over IPC.

## 6. Static-evidence findings

Each is **STATIC ONLY, UNVERIFIED AGAINST RUNNING APP**. None is filed as a
bead; the parent dispatches.

- **S-F1 — the destructive confirmation does not survive closing and reopening
  the review, as far as the UI is concerned.** S3 expects "checking it persists
  the confirmation per item so it survives closing and reopening the review".
  The write is real (`plansConfirmDestructive`,
  `PlanReviewOverlay.tsx:281-293`, persisting `plan_items.destructive_confirmed`),
  but nothing reads it back: `rg -c destructiveConfirmed
  apps/desktop/src/bindings/index.ts` exits 1 (sanity-checked against
  `destructiveDestination`, which returns 9), and neither
  `PlanDetail_Serialize` (`index.ts:7518-7539`) nor
  `PlanItemDetail_Serialize` (`:7631-7647`) carries such a field. The overlay's
  `destructiveConfirmed` starts `false` (`:220`) and `handleClose` resets it to
  `false` (`:274`). Predicted observable on reopen: checkbox unchecked,
  "Approve & apply" disabled again until re-checked. Whether this is a DOC
  overclaim or a PRODUCT read-back gap is **not decidable from source** — the
  DB persistence the sentence asserts does exist; the user-visible half does
  not. Give both readings if you drive it.
- **S-F2 — no item can ever display the state `refused`.** S3's negative Expect
  says the executor "marks it `refused`". `refused` exists only in the transient
  progress event (`crates/fs/executor/src/run/loop_.rs:427`);
  `crates/persistence/plans/src/repositories/plan_apply.rs:732-735` maps
  `"refused" | "stale"` to `"failed"` before persisting, and
  `PlanItemState` in the frontend DTO is
  `pending | applying | succeeded | failed | skipped | cancelled`
  (`index.ts:7665`). So `plan-review-item-result-<index>` will read `failed`.
  Combined with G8 (no message mapping — `rg -c destructive_unconfirmed
  apps/desktop/src apps/desktop/messages` exits 1), the only distinguishing
  text available is the raw `failureReason` string the executor writes.
- **S-F3 — the reclaimable total includes protected candidates.**
  `crates/app/core/src/cleanup_generator/scan.rs` adds `size` to
  `total_reclaimable_bytes` **before** resolving protection, and never subtracts
  it. With the default `default_protection: "protected"` this means S1's "total
  reclaimable size for the current candidate set" advertises bytes that no apply
  can reclaim. The session flow differs: `raw-cleanup-reclaimable` renders
  `selectedBytes` (`RawFrameCleanupSection.tsx:175`), and protected frames get
  no checkbox, so they are excluded there. Not currently in the known-gap list.
- **S-F4 — with default settings, every project cleanup candidate is protected.**
  `default_protection: "protected"` plus
  `protected_categories: ["lights","masters","finals"]`
  (`crates/domain/core/src/settings.rs` defaults) and the project-keyed
  resolution in `scan.rs` mean S1's mixed protected/unprotected preview and
  S4's happy path are unreachable without changing Settings → Cleanup. The
  2026-07-14 run confirmed the apply consequence empirically (#807, since
  dissolved as G2), so this is a **precondition**, not a new defect. It is why
  §3 step 5 exists.
- **S-F5 — the zero-item overlay state is genuinely unreachable through S1–S6,
  and S3.E6's "no explanatory text" holds.** The project Generate control renders
  only under `hasCandidates` (`OutputsCleanupSections.tsx:326`); the session
  Generate is `disabled` while `selected.size === 0`
  (`RawFrameCleanupSection.tsx:214`); and the cleanup overlay passes no
  `emptyReason`, so `plan-review-empty-reason` (`PlanReviewOverlay.tsx:613-616`)
  cannot render — `grep -n emptyReason ProjectDetail.tsx` returns only `:462`,
  the archive flow. The journey's own trace for this is accurate.
- **S-F6 — the S1 "no selection affordance" wording is ambiguous.**
  `candidateColumns()` (`OutputsCleanupSections.tsx:147-153`) has no selection
  column for any row, so the protected/unprotected distinction the sentence
  implies does not exist in the project flow. It does exist in the session flow
  (`raw-cleanup-select-<frameId>` is `null` for protected frames,
  `RawFrameCleanupSection.tsx:125`). Two validators could read S1.E1
  differently; the journey text is what is imprecise, not the app.
- **S-F7 — the journey's own trace cites two paths that do not resolve.**
  `crates/app/core/src/cleanup_generator.rs` (cited in S1 and S5) does not
  exist; the module is the directory `crates/app/core/src/cleanup_generator/`
  with `mod.rs`, `policy.rs`, `scan.rs`, `generate.rs`, `raw_frames.rs`. And the
  `deltas/` directory the `trace:` field cites twice is absent
  (`ls docs/journeys/J06-cleanup-scan-review-apply/deltas/` → exit 1); both
  entries are already annotated "folded" and "superseded", so this is trace
  hygiene, not lost content. Doc-side, low severity.
- **S-F8 — every line:line the journey cites into `PlanReviewOverlay.tsx` is
  still accurate as of this reading**: `:83-91` (`isDestructiveItem`), `:217`
  (`hasDestructiveItems`, feeding the `:538` variant and the `:540-546` gate),
  `:446` (item pill), `:544` (`itemsTotal === 0`). No drift to correct.

## 7. Questions for the driving phase (do not answer offline)

1. Does the destructive-confirm checkbox read unchecked after closing and
   reopening the review overlay (S-F1)?
2. What exact text appears in `plan-review-item-result-<index>` for an
   unconfirmed destructive item — the bare `failed` pill, or `failed` plus a
   `destructive_unconfirmed`-derived string (S-F2, G8)?
3. Does `cleanup-reclaimable` include protected candidates' bytes (S-F3)?
4. Is `size_bytes` still 0 for detected artifacts, as the 2026-07-14 run noted?
5. Does the trash-destination row still show an archive path in the `To` column
   (G11), and does the danger pill render on those `archive` items (Δ5)?
6. With a per-root "Unprotected" override, are session-attributed frames
   preselected as non-protected (Δ2, PR #894)?
