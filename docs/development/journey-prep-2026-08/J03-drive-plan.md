# J03 drive plan — ingest: confirm and catalogue IN PLACE

Offline prep, phase A. Bead `astro-plan-1ffmh`. Sources read at `origin/main`
= `83bfd4718` (fetched 2026-08-25). Every selector below was read out of that
tree; nothing here was observed in a running app.

Journey: `docs/journeys/J03-ingest-confirm-catalogue-in-place/journey.md`
(version 5, `last_reviewed` 2026-07-14). No `deltas/` directory exists at that
path — the delta log is inline in the journey (Δ2–Δ5) and the `trace:` entries
point at `docs/product/journeys/...`, a path that no longer exists in the tree.

## 1. Expectation inventory

Steps S1–S5, 12 Expects total. Signal column names the ONE observable that
settles it; "a badge elsewhere" is never accepted.

| # | Step | Expect (abridged) | Signal kind | Observable |
|---|---|---|---|---|
| E1 | S1 | Mixed folder materialises as separate single-type items | visible state | one `inbox-item-<id>` row per type, `inbox-row-classification` text per row |
| E2 | S1 | Missing mandatory metadata → needs-review gate, Confirm disabled | visible state | `inbox-confirm-btn` has `disabled`; needs-review surface `bulk-frame-type` present |
| E3 | S1 | Detail uses the shared adaptive dock, side at ≥1400px, width persists | visible state | `dock-placement-control`, `dock-resize-handle` present; placement from computed layout |
| E4 | S1 | Per-page Auto/Bottom/Right override present | visible state | three options inside `dock-placement-control` |
| E5 | S1 (neg) | File list never cut off below the window, either placement | visible state | detail file-list element `scrollHeight > clientHeight` and `getBoundingClientRect().bottom <= innerHeight` |
| E6 | S2 | move_count 0, catalogue_count == file_count, every action "catalogue in place" | visible state | `plan-panel-inplace` present in `plan-group-dest-<id>`; file-count tooltip = `{n} catalogued in place`; **NOT the confirm toast** — see F1 |
| E7 | S2 (neg) | No destination-root picker | visible state (absence) | `inbox-dest-root-select` absent |
| E8 | S2 | Light-frame attribution picker appears, ranked identically, zero FS I/O | visible state | `inbox-attribution-picker` + `inbox-attribution-confirm` |
| E9 | S3 | Each item reads as a catalogue action, shows its unchanged path; Escape/Discard close without mutation | visible state | per-row `plan-panel-inplace` present AND `inbox-dest-absolute-<idx>` ABSENT; `inbox-source-absolute-<idx>` equals the pre-confirm path |
| E10 | S3 (neg) | Destructive-destination control absent for an all-catalogue plan | visible state (absence) | `plan-destructive-archive` / `plan-destructive-trash` / `plan-destructive-confirm` all absent |
| E11 | S4 | Identity+metadata written to index; explicit success signal; visible in Sessions; audit entry with outcome | toast + visible state | toast text `Plan applied.`; `sessions-page` row for the session; Audit Events row with outcome `applied` |
| E12 | S4 (neg) | File set and content hashes byte-identical after apply; stale item refused | external state | host-side hash/size/mtime manifest diff; `plan-stale-<id>` badge for the stale case |
| E13 | S4 (neg) | Unapproved plan refused with `plan.approval_required` | refusal reason | **not observable in the UI** — see F2 |
| E14 | S5 | Organized item → all catalogue; unorganized item → all move; routed purely by source-root org state | visible state | `plan-panel-inplace` on the organized group vs `plan-panel-summary-arrow` on the unorganized group, in the same overlay |

Unmappable / degraded: **E13** (no UI observable at all, F2) and **E6** (the
Expect as written names a response field the UI never renders, F1). E12 needs
an out-of-app manifest; the app shows nothing that proves byte identity.

## 2. Selector map

Verified in source. Anything not listed here was not verified — do not invent
one. Toasts carry **no** `data-testid`: read
`document.querySelectorAll('.pv-toast__container .pv-toast__message')` and take
`textContent`; each toast item is `[role="alert"]`
(`apps/desktop/src/ui/ToastContainer.tsx:33,36,66,70`).

### Inbox list and detail

| Purpose | Selector | Source |
|---|---|---|
| Inbox list root | `[data-testid="inbox-list"]` | `InboxList.tsx:513` |
| One item row | `[data-testid="inbox-item-<inboxItemId>"]` | `InboxList.tsx:456` |
| Row classification cell | `[data-testid="inbox-row-classification"]` | `InboxList.tsx:426,496` |
| Pending-plan marker on a row | `[data-testid="inbox-item-plan-pending-<id>"]` | `InboxList.tsx:486` |
| **Group header when grouped by org state** | `[data-testid="inbox-group-orgState-organized"]` | `InboxList.tsx:389` + `InboxControls.tsx:110-113` (`id: 'orgState'`) |
| Confirm button | `[data-testid="inbox-confirm-btn"]` | `InboxDetail.tsx:323` |
| Destination-root picker (must be ABSENT) | `[data-testid="inbox-dest-root-select"]` | `InboxDetail.tsx:340` |
| Attribution picker | `[data-testid="inbox-attribution-picker"]`, confirm `[data-testid="inbox-attribution-confirm"]` | `AttributionPicker.tsx:124,136` |
| Review-plans trigger | `[data-testid="inbox-review-plans-btn"]` | `InboxTopBar.tsx:121` |
| Rescan (no testid) | `[aria-label="Rescan all roots"]` | `InboxTopBar.tsx:153` |

i18n keys, en-GB values (`apps/desktop/messages/en-GB.json`):

- `inbox_confirm_to_inventory` = `Confirm to inventory` (the Confirm button label,
  `InboxDetail.tsx:143`)
- `inbox_dest_root_label` = `Library:`
- `inbox_dim_org_state` = `Org. state`
- `inbox_review_plans` = `Review plans`; `inbox_review_plans_with_count` =
  `Review plans ({count})`
- `common_rescan` = `Rescan`; `common_rescanning` = `Rescanning…`;
  `inbox_rescan_all_roots_aria` = `Rescan all roots`

### Plan review overlay

| Purpose | Selector | Source |
|---|---|---|
| Overlay | `[data-testid="plan-approval-overlay"]` | `PlanApprovalOverlay.tsx:79` |
| Panel / bar / scroll | `plan-panel`, `plan-panel-bar`, `plan-panel-scroll` | `PlanPanel.tsx:293,312,323,368` |
| Plan+action counts | `[data-testid="plan-total-count"]` | `PlanPanel.tsx:337` |
| Apply selected / all | `plan-apply-selected`, `plan-apply-all` | `PlanPanel.tsx:348,359` |
| Per-group row / dest cell | `plan-group-<id>`, `plan-group-dest-<id>` | `PlanGroupRow.tsx:103,203` |
| **IN-PLACE marker** | `[data-testid="plan-panel-inplace"]` | `PlanGroupRow.tsx:207` (group) and `:409` (per file row) |
| **MOVE marker** | `[data-testid="plan-panel-summary-arrow"]` | `PlanGroupRow.tsx:221` |
| Per-file row | `[data-testid="plan-file-row-<idx>"]` | `PlanGroupRow.tsx:370` |
| Per-file source path | `[data-testid="inbox-source-absolute-<idx>"]` | `PlanGroupRow.tsx:401` |
| Per-file destination path (must be ABSENT on catalogue) | `[data-testid="inbox-dest-absolute-<idx>"]` | `PlanGroupRow.tsx:421` |
| Stale badge | `[data-testid="plan-stale-<id>"]` | `PlanGroupRow.tsx:262` |
| Apply one / discard | `plan-apply-one-<id>`, `plan-cancel-<id>` | `PlanGroupRow.tsx:281,292` |
| Destructive control (must be ABSENT) | `plan-destructive-archive`, `plan-destructive-trash`, `plan-destructive-confirm` | `PlanDestructiveControl.tsx:47,63,80` |

Gating that makes E6/E10 falsifiable:

- `allInPlace = actions.length > 0 && actions.every(a => a.action === 'catalogue')`
  (`PlanGroupRow.tsx:93-95`). True → renders `plan-panel-inplace`; false →
  renders `plan-panel-summary-arrow`. The two are mutually exclusive.
- Per file: `inPlace = a.action === 'catalogue' || a.destinationPreview === a.fromPath`
  (`PlanGroupRow.tsx:364-365`) → `plan-panel-inplace`, else the arrow plus
  `inbox-dest-absolute-<idx>`.
- `hasDestructive = plans.some(p => p.actions.some(a => a.requiresDestructiveConfirm))`
  (`PlanPanel.tsx:199-201`), and `PlanDestructiveControl` renders only under
  `{hasDestructive && …}` (`PlanPanel.tsx:401`). So the journey doc's S3
  negative is right and the Windows doc contradicts it — see F3.

en-GB values that read differently between the two modes:

- `inbox_inplace_label` = `In place`
- `inbox_plan_file_count_tooltip_inplace` = `{count} catalogued in place`
- `inbox_plan_file_count_tooltip_mixed` = `{moved} moved · {inPlace} catalogued in place`
  — the group's file-count cell `title` picks mixed whenever `moveCount > 0`
  (`PlanGroupRow.tsx:243-252`). **This tooltip is the closest UI proxy for
  E6's move_count/catalogue_count.**
- `plan_count_label` = `{count} plan` / `{count} plans`;
  `action_count_label` = `{count} action` / `{count} actions`
- `inbox_apply_all` = `Apply all`;
  `inbox_apply_selected_plans` = `Apply selected ({count})`;
  `common_applying` = `Applying…`
- `inbox_stale` = `Stale`; `plans_review_col_to` = `Destination`

### Toast strings (exact en-GB text to match)

| Key | en-GB | Emitted at |
|---|---|---|
| `inbox_toast_plan_created` | `Plan created ({count} items). Review below before applying.` | `useInboxConfirmFlow.ts:177` |
| `inbox_plan_applied_toast` | `Plan applied.` | `useInboxPlanApplyFlow.ts:78` |
| `inbox_plan_apply_failed_toast` | `Apply failed — please try again.` | `useInboxPlanApplyFlow.ts:68,85` |
| `inbox_toast_plans_applying` | `{count} plan(s) are being applied.` | `:115` |
| `inbox_toast_plans_partial` | `{applied} plan(s) applied; {failed} failed.` | `:107` |
| `inbox_toast_all_plans_applying` / `_partial` | same shapes | `:147` / `:139` |
| `inbox_toast_stale_classification` | `Folder changed since classification — rescan to refresh.` | `useInboxConfirmFlow.ts:225` |
| `inbox_toast_has_open_plan` | `An open plan already exists for this item.` | `:222` |
| `inbox_toast_confirm_failed` | `Could not confirm: {message}` | `:230` |
| `err_plan_approval_required` | `Approve the plan before applying it.` | mapped at `lib/error-messages.ts:63`, **never rendered by the inbox apply flow** (F2) |

### Sessions, audit, dock, wizard

| Purpose | Selector | Source |
|---|---|---|
| Sessions page | `[data-testid="sessions-page"]`, route `#/sessions` | `SessionsPage.tsx:317`, `app/router.tsx:50` |
| Sessions list | `[data-testid="sessions-list"]` | `SessionsTable.tsx:357` |
| One session row | `[data-testid="sessions-row-<sessionId>"]` | `SessionsTable.tsx:305` |
| Session detail pane | `[data-testid="session-detail"]` | `SessionDetail.tsx:210` |
| Audit Events pane | **no testid exists**; heading `settings_auditlog_title` = `Audit Events` | `settings/AuditLog.tsx:286` |
| Audit outcome text | `applied` / `ok` / `refused` / `failed` / `paused` | `AuditLog.tsx:81-89` |
| Dock placement control | `[data-testid="dock-placement-control"]`, aria `Detail panel placement`, options `Auto`/`Bottom`/`Right` | `DetailDockPlacementControl.tsx:56,61,66,71` |
| Dock resize handle | `[data-testid="dock-resize-handle"]` | `ui/ResizeHandle.tsx:24` |
| Wizard org-state select | `[data-testid="org-select-<kind>-<index>"]`, aria `Organization state`, options `Already organized` / `Needs organizing` | `steps/StepSourceFolders.tsx:343-354` |
| Wizard add-by-path | `manual-add-by-path-<kind>`, `manual-path-input-<kind>`, `manual-add-path-btn-<kind>` | `steps/StepSourceFolders.tsx:432,436,452` |

Sessions rows DO have testids (see above — an earlier pass of this document
wrongly said they did not; the components are `SessionsTable.tsx` /
`SessionDetail.tsx`, which a `*Page|*List|*Row` filename filter misses).
Audit rows genuinely have none — `AuditLog.tsx` contains zero `data-testid`
occurrences, checked against a control pattern that matches 38 times in the
same file. So settle E11's audit half from the database (§4) and its Sessions
half from `sessions-row-<id>`.

## 3. Fixture recipe

Reuse `astro-plan-mg6h8`'s recipe wholesale: generator
`C:\jv-throwaway\mkfits.js` (Node, no deps, idempotent, 19 files /
37,844,362 bytes, 1024×1024 uint16, headers carry `INSTRUME`, `EXPTIME`,
`DATE-OBS`, `IMAGETYP`, `XBINNING`, `YBINNING`, `CCD-TEMP`, `GAIN`, `TELESCOP`,
`FOCALLEN`, `OBJECT`, `FILTER`, plus `STACKCNT`/`NCOMBINE` on the two masters),
launcher `jv-dev.cmd` via `schtasks /Run /TN JVDev`, driver
`C:\jv-throwaway\drv.js` over `ws://127.0.0.1:9223`, and its smoke test. Do not
re-derive any of it and do not write a second generator.

**What J03 adds to that recipe:**

1. **A J03-private root.** mg6h8's `C:\jv-throwaway\library` is shared. Generate
   a separate tree so a sibling unit cannot move your files under you:
   ```
   ssh <journey-host> "node C:/jv-throwaway/mkfits.js C:\jv-throwaway-j03"
   ```
   Register `C:\jv-throwaway-j03\library\Captures` (light_frames) and
   `C:\jv-throwaway-j03\library\Calibration` (calibration). Siblings only — the
   wizard rejects parent/child overlap.
2. **Explicit organized state.** Set `org-select-light_frames-<index>` to
   `organized` on the Source Folders step. There is NO organization-state
   control on Settings → Data Sources (F4), so the wizard is the only place to
   choose it; a root added later via Data Sources → Add defaults non-inbox
   categories to organized with no picker.
3. **A pre-apply byte manifest**, which mg6h8's recipe does not produce and
   which E12 cannot be settled without. Before step S1:
   ```powershell
   Get-ChildItem -Recurse -File C:\jv-throwaway-j03\library |
     Get-FileHash -Algorithm SHA256 |
     Select-Object Hash,Path |
     Sort-Object Path | Export-Csv C:\jv-throwaway-j03\pre.csv -NoTypeInformation
   ```
   Re-run to `post.csv` after apply and `Compare-Object` the two. A zero diff
   is E12's success signal; any diff is its failure signal.
4. **A second, unorganized inbox root for S5.** Register
   `C:\jv-throwaway-j03\inbox` as category `inbox` (always defaults to
   unorganized) and copy two generated lights into it, so one Inbox session
   holds one organized-source item and one unorganized-source item.
5. **A staleness case for E12's second clause** (optional, run last because it
   dirties the manifest): after confirm and before apply, append one FITS block
   to a single confirmed file and expect `plan-stale-<id>`.

Do not exercise the corrupt-FITS file for J03 — its behaviour is already
settled (`EvidenceSource::None` → `unclassified_files`, PR #1737).

## 4. Precondition and teardown

**Start state.** Clean first-run wizard against a J03-private database, so
mg6h8's roots are not in the index:

```
Get-Process desktop_shell,node,cargo,rustc -EA SilentlyContinue | Stop-Process -Force
# point jv-dev.cmd at PV_DB_URL=sqlite://C:\jv-throwaway-j03\app.db?mode=rwc
#                and PV_DATA_DIR=C:\jv-throwaway-j03\appdata
schtasks /Run /TN JVDev
```

Then: wizard is 8 steps (Language, Theme, Source Folders, Processing Tools,
Configuration, Observing Site, Confirm, Scan) — drive it by clicking the button
whose text starts `Continue`, then `Start scan →`, then `Finish`. Give the
Observing Site step a second click; mg6h8 measured a swallowed first click.

**Surfacing files into the Inbox (P2).** Inbox's `Rescan all roots` only reaches
category `inbox`. For the organized `Captures` root you must rescan that root
specifically from Settings → Data Sources: open
`[data-testid="data-sources-kebab-btn"]` on the root card (aria label
`Source actions`) and click the item labelled `Rescan`. That menu item has no
testid (`settings/RootCard.tsx:156-162`).

**Durable-record checks.** Copy all three SQLite files or you read an empty DB —
the data is in the WAL:

```
scp <journey-host>:C:/jv-throwaway-j03/app.db{,-wal,-shm} .
```

- E6/E14 routing: `select action, count(*) from plan_item group by action;`
- E8 attribution durability (Tier 1): `plans.chosen_framing_id` is written at
  confirm time (`crates/app/inbox/src/confirm.rs:331-345`) and bound when the
  session is created — assert it is non-null before apply.
- E11 audit: `audit_log_entry` columns are `at, trigger, entity_type,
  from_state, to_state, outcome, reason_code, payload` (no `topic` column).
- E13, if you attempt it: the refusal lands in `results[].error` only.

Hash-navigating inside Settings does not switch the sub-pane: set
`location.hash = '#/settings'` and then *click* the nav item.

**Teardown.** Kill `desktop_shell,node,cargo,rustc`; delete
`C:\jv-throwaway-j03\app.db*`; leave `C:\jv-throwaway-j03\library` in place if a
successor round is expected, otherwise `Remove-Item -Recurse -Force
C:\jv-throwaway-j03`. Leave `C:\jv-throwaway` and `schtasks /TN JVDev` alone —
they are shared. Never touch `D:\Astrophotography`.

## 5. Known-gap list — do NOT re-file

| Expect | Already recorded as | Note |
|---|---|---|
| E13 backend refusal `plan.approval_required` | PR #1740 (`c0f7e10ea`), journey Δ5 | Correct behaviour; `inbox_plan.rs:202-207` reads the approval and never mints one. Verified. |
| Corrupt FITS → `unclassified_files` via `EvidenceSource::None` | PR #1737 | Not in J03 scope. |
| Approval self-minting defeating Principle II | `astro-plan-3v3r.8.22` (P1, open) | Filed against `inbox_plan.rs:188`, which PR #1740 has since fixed. The frontend instance (F2) is a different locus — report it, do not open a duplicate under the old dedup key. |
| Plan approval durable and never revoked; select-all leaves `selectedSet` undiscriminated | `astro-plan-ecmxf` (open) | Will confound repeat Apply attempts. |
| An inbox refusal is undiagnosable from the UI (`inbox.destination_collision`) | `astro-plan-vw8c2` (open) | Same *shape* as F2 but a different error code and a different code path. |
| A master item never gets per-file metadata rows, so its pre-confirm required-attribute status is unknowable | `astro-plan-lbfho` (open) | Touches E2 if you review a master item. |
| Inbox grouping derives the observing night with no noon boundary | `astro-plan-uzcbj` (open) | Affects session grouping you will see under E11. |
| Setup wizard is 8 steps, J01 doc says 7 | `astro-plan-ko0tv` | Documentation only. |
| `windows-native-rust-dev.md:240-244` names `e2e-path-input-*` testids that do not exist | `astro-plan-npts7` | Use `manual-*` instead. |
| Journey's own "Known gaps": cross-plan overlap protection | PR #408, merged 2026-07-04 | Closed; the journey already dropped it. |

Nothing in the journey's Δ2–Δ5 log is an open gap. Journey epic:
`astro-plan-qrpb`.

## 6. Static-evidence findings

All six are **STATIC ONLY, UNVERIFIED AGAINST RUNNING APP**.

**F1 — E6's Expect names a response field no UI surface renders.** The confirm
success toast is `inbox_toast_plan_created` with `count: result.itemsTotal`
only (`useInboxConfirmFlow.ts:175-181`). `ConfirmResponse.move_count` and
`.catalogue_count` exist on the contract (`crates/app/inbox/src/confirm.rs:108-110`,
returned at `:379-380`) but are not rendered anywhere. So "the response reports
a move count of 0 and a catalogue count equal to the file count" is settleable
only from the file-count tooltip (`{n} catalogued in place`), from the presence
of `plan-panel-inplace`, or from `plan_item.action` in the DB — not from any
number on screen. A validator looking for a count will report a false negative.

**F2 — E13 is unreachable via per-plan Apply and its reason is never shown.**
`handleApplyOne` calls `commands.plansApprove(planId)` immediately before
`runPlanApply` (`useInboxPlanApplyFlow.ts:60-73`), so the user's own Apply
gesture always mints the approval it then consumes; a `plan.approval_required`
refusal cannot arise on that path. `handleApplyAll` and `handleApplySelected`
do scope approvals to the plans rendered at that moment
(`:96-101`, `:129-133`), which is the only way to reach the refusal. And on
every failure path the toast collapses to `Apply failed — please try again.`
(`:68,85,124`) or a bare count (`{applied} plan(s) applied; {failed} failed.`);
`err_plan_approval_required` = `Approve the plan before applying it.` is mapped
at `lib/error-messages.ts:63` but no inbox apply call site renders it.
Consequence for the drive: E13's success signal must be read from
`results[].error` over the bridge or from the DB, never from the toast.

**F3 — the Windows doc contradicts the journey on E10, and the journey is
right.** `docs/development/windows-journeys/journey-03-inbox-catalogue-in-place.md:82-88`
tells the validator to expect the Archive-vs-System-Trash control to *still*
show for an all-catalogue plan and to FAIL if it is missing. Source says the
opposite: the control renders only under `{hasDestructive && …}`
(`PlanPanel.tsx:401`) with `hasDestructive` derived from
`requiresDestructiveConfirm` (`:199-201`). A validator following the Windows
doc would file a false FAIL. **Use the journey document's S3 negative Expect.**
The Windows doc is otherwise reusable, and I did reuse its click sequences.

**F4 — no organization-state control exists on Settings → Data Sources.**
`RootCard.tsx` offers rescan, reconcile, remap, edit-protection, disable and
delete (`:156-241`); no organized/unorganized toggle. The only editor is the
wizard's `org-select-<kind>-<index>` (`steps/StepSourceFolders.tsx:343`). The
Windows doc's Precondition 2 hedges "if an edit-organization-state control is
exposed there" — it is not. Do not waste a host slot looking.

**F5 — journey trace line numbers have drifted.** S2 cites
`crates/app/inbox/src/confirm.rs:293-303` for the `OrganizationState::Organized`
→ `catalogue` routing; on `83bfd4718` that code is at `:437-444` and `:293-303`
is unrelated plan-insert code. S4's `crates/app/core/src/inbox_plan.rs:202-212`
is accurate. The `trace:` header entries pointing at
`docs/product/journeys/J03-.../deltas/*` reference a directory that does not
exist in the tree.

**F6 — E12's "no filesystem I/O" is provable from source, so the on-host check
is a regression guard rather than a discovery.** `catalogue_noop()` is an
unconditional `Ok(())` with no I/O (`crates/fs/executor/src/ops/catalogue_op.rs:19-21`),
dispatched for `ExecutorItemAction::Catalogue`. Coverage that already exists:
`catalogued_frame_is_recorded_identically_to_a_moved_frame`
(`crates/app/core/tests/ingest_sessions_integration.rs`, PR #1613 /
`08a3da549`) applies one plan holding a move item and a destination-less
catalogue item over byte-identical fixtures with `filetime`-pinned mtimes, then
compares the two `file_record` rows on `root_id`, `size_bytes`, `mtime`,
`content_hash` and `state`, allowing only `relative_path` and the id /
wall-clock columns to differ. **So "the attribution/identity record is as rich
as a move's" is already covered — do not spend host time re-proving it.** The
same PR added an ingested-session `total_size_bytes` sum assertion. What remains
uncovered at Layer 2, per that Windows doc's own E2E-sync section: the
end-to-end "organized root → `movedCount == 0`, `catalogueCount == file_count`,
no root picker, byte-identical after apply" path has **zero** Layer-2 and zero
Playwright coverage. That whole-pipeline claim, E14's mixed-root routing, and
E11's Sessions-plus-audit visibility are the gaps worth the host slot.

## Questions for the driving phase

Recorded rather than answered, because they need a running app:

1. Does `inbox-dest-root-select` stay absent for an organized item that has
   more than one applicable root of its frame type (Δ3 / PR #938)?
2. Does the attribution picker actually appear on a catalogue-mode light-frame
   confirm (Δ4 / issue #943 says no Inbox UI surfaces the backend pass — Δ2 and
   Δ4 read as contradictory)?
3. In the S5 mixed run, do both groups render in the one overlay with opposite
   in-place/arrow markers?
4. Does `plan-stale-<id>` appear for a file mutated between confirm and apply?
