# J02 drive plan — offline prep (phase A)

Journey: `docs/journeys/J02-ingest-review-reclassify-confirm-move/journey.md` v8.
Source read at working-tree commit `38d07af07` (2026-08-24), branch
`chore/journey-validation-formula`.

**Everything below is STATIC evidence read from source. Nothing here was
observed in a running app.** The Windows host was not contacted.

Windows click-sequence doc reused: `docs/development/windows-journeys/journey-02-inbox-ingest-move.md`
(7 Tests). It carries no selectors — every selector below was read from React
source. Three of its Tests are wrong as written; see §5.

## Census

| Section | Count |
| --- | --- |
| Steps (`### S`) | 8 |
| `Expect` blocks | 21 |
| Success criteria | 7 |
| Known gaps in doc | 3 |
| Delta entries | 7 (Δ2–Δ8; no Δ1) |
| Expects mapped to a verified observable | 18 |
| Expects with no observable | 3 (E1.4, E3.5, E7.6) |
| Selectors verified in source | 41 |
| Selectors that do not exist | 4 (§6) |

## 1. Expectation inventory

Signal column: which of toast / navigation / visible state / refusal reason /
per-item error settles it. Per the journeys' cross-cutting rule a badge changing
elsewhere is not sufficient evidence.

### S1 — Rescan the inbox

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E1.1 | Newly scanned folder appears as one source-group row, non-confirmable | `[data-testid^="inbox-source-group-"]` present; its Type cell reads `not yet classified`; no `inbox-confirm-btn` reachable for it (source-group rows are not selectable — `useInboxSelection.ts:261`) | visible state |
| E1.2 | After classification the source-group row is replaced by exactly N item rows | `[data-testid^="inbox-source-group-"]` count drops to 0 for that id; `[data-testid^="inbox-item-"]` count rises by N | visible state |
| E1.3 | Badge, status-bar breakdown and visible row count agree | sidebar count vs `[data-testid="statusbar-inbox-summary"]` vs `[data-testid="inbox-stats-summary"]` vs `[data-testid^="inbox-item-"]`.length | visible state |
| E1.4 | Opening Inbox never spins a runaway re-render loop | **NO OBSERVABLE** — no render counter is exposed. See §6. | — |
| E1.5 | Replacing a source-group row never drops the selection | selected row keeps `.pv-inbox-table__row--selected` (`InboxList.tsx:460`) | visible state |
| E1.6 | Rescan failure signal | **NONE EXISTS** — see finding F2. Absence of a toast is the expected (defective) outcome, not a drive error. | refusal reason (absent) |

**Classification is NOT automatic.** `useInboxSelection.ts:259-261` — "Unlike the
item-scoped classification query this does NOT fire on selection … it is driven
by an explicit button in the row." The drive sequence for S1 is Rescan **then
click Classify on each source-group row**. The Windows doc omits this.

### S2 — Inspect an item's detail

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E2.1 | Detail docks side ≥1400px, bottom below | `[data-testid="listpage-detail"][data-dock="side"]` / `[data-dock="bottom"]` (`ListPageLayout.tsx:328-329`) | visible state |
| E2.2 | Detail body is its own scroll region | `.pv-inbox-detail__scroll` exists and has `scrollHeight > clientHeight` when overflowing (`InboxDetail.tsx:363`) | visible state |
| E2.3 | File count on row equals file count in detail | row Count cell vs `[data-testid="inbox-files-popover-trigger"]` text `File metadata (N)` | visible state |
| E2.4 | Each field distinguishable: real value + source pill / unresolved chip / blank | `[data-testid="unresolved-chip"]` (text `Unresolved`) vs a `SourceBadge` span whose text is one of `FITS`/`User`/`Inferred`/`Default` (`RenderValue.tsx:67-84`) | visible state |
| E2.5 | Source-group detail describes the folder and offers classification, not confirmation | `[data-testid^="inbox-source-group-classify-"]` present; no confirm control | visible state |
| E2.6 | Detail tracks the selected item across a search/filter change | selected row id unchanged after typing in the search box | visible state |
| E2.7 | Metadata load failure shows an explicit error and Confirm stays disabled | `inbox-confirm-btn` has `disabled` (`canConfirm` includes `!fileMetadataError`, `useInboxConfirmFlow.ts:415`) | visible state |
| E2.8 | Reveal is NOT available from Inbox | **CONTRADICTED BY SOURCE** — `[data-testid="inbox-reveal-btn"]` exists (`InboxDetail.tsx:356`) and calls `revealInOs` via `handleReveal` (`:197`). See finding F3. | visible state |

### S3 — Resolve missing metadata via bulk reclassify

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E3.1 | Banner names exactly what is missing, inline in the Files column | `[data-testid="inbox-missing-attr-banner"]`, title `Required metadata missing`, body names the count; per-file badge `[data-testid^="inbox-missing-attr-"]` reads `needs <attrs>` | visible state |
| E3.2 | Confirm disabled while unresolved | `inbox-confirm-btn[disabled]` (`canConfirm` includes `!hasMissingRequiredMeta`) | visible state |
| E3.3 | Applying a value to a selection applies in one call, reported as an applied count | click `[data-testid="bulk-apply-btn"]` (label `Apply to selected (N)`); a heterogeneous selection relabels to `Apply anyway (N)` | visible state |
| E3.4 | Item re-partitions to single-type and Confirm re-enables automatically | `inbox-confirm-btn` loses `disabled`; row Type cell changes | visible state |
| E3.5 | Override visible with provenance AND a reset path | provenance: SourceBadge text `User`. **Reset path: NO GENERAL OBSERVABLE.** Only `[data-testid="bulk-undo-btn"]` exists, and it is frame-type-only and in-session-only. See finding F4. | visible state (partial) |
| E3.6 | Override survives a later rescan | after clicking Rescan the SourceBadge still reads `User` | visible state |
| E3.7 | A direct confirm on an unresolved item fails with `inbox.missing_path_attributes` | via bridge: `invoke('inbox_confirm', …)` rejects with that code; through the UI the toast reads `Some files are missing required attributes. Assign the missing values in the file list, then confirm again.` | refusal reason |
| E3.8 | A selection spanning types warns before overwriting | **CONTRADICTED BY THE DOC'S OWN CORRECTION** — `[data-testid="bulk-heterogeneous-warning"]` + `[data-testid="bulk-heterogeneous-ack"]` exist (`InboxNeedsReview.tsx:223,247`). See finding F5. | visible state |

### S4 — Choose a destination library root

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E4.1 | One valid root → no picker | `[data-testid="inbox-dest-root-select"]` absent (rendered only when `applicableRoots.length > 1`, `InboxDetail.tsx:337`) | visible state |
| E4.2 | Two+ valid roots → control lists `<folder name> · <category>`, defaults to Auto | select present; first option text `Auto`; option text is `${basename(r.path)} · ${r.category}` (`:346-348`) | visible state |
| E4.3 | Choice arms only the selected item; returns to Auto for another item | select `.value === ''` after selecting a different row | visible state |
| E4.4 | Two roots sharing a last path segment are NOT told apart (corrected) | two `<option>` elements with identical text | visible state |

### S5 — Confirm into a plan

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E5.1 | Confirm creates a plan, moves nothing | toast `Plan created (N items). Review below before applying.` | toast |
| E5.2 | With an unresolved multi-root choice, confirm proceeds, a toast says choose a destination, the pick surfaces in the review surface | toast `Choose a destination library root to generate the plan.` then `[data-testid="inbox-root-picker"]` inside the overlay | toast + visible state |
| E5.3 | Item stays visible, marked planned | `[data-testid="inbox-item-plan-pending-<id>"]`, text `Plan pending review` | visible state |
| E5.4 | No file on disk changes from Confirm alone | filesystem listing of the inbox root unchanged | visible state (out-of-band) |
| E5.5 | Light-frame confirm first shows ranked attribution suggestions | `[data-testid="inbox-attribution-picker"]`, heading `Where do these lights belong?` | visible state |
| E5.6 | `Leave unassigned` always offered | `[data-testid="inbox-attribution-option-unassigned"]`, text `Leave unassigned` | visible state |
| E5.7 | A completed-project candidate carries a reopen warning | option description contains `Reopens a completed project; archived raw subs may be unavailable` | visible state |
| E5.8 | Picking + `Confirm with this attribution` produces the plan in one call | `[data-testid="inbox-attribution-confirm"]` then E5.1's toast | toast |
| E5.9 | Nothing preselected; no plan while the list is on screen | `inbox-attribution-confirm` starts `disabled` (`selected === ''`, `AttributionPicker.tsx:133`) | visible state |
| E5.10 | Cancelling dismisses without a plan | picker unmounts; no `inbox-item-plan-pending-*` appears | visible state |
| E5.11 | Confirming one sibling leaves the other N−1 unchanged (SC7) | only one row gains `inbox-item-plan-pending-*` | visible state |

### S6 — Review the plan

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E6.1 | Every plan item shows action, source and destination in full | expand `[data-testid="plan-group-toggle-<inboxItemId>"]`, then `[data-testid="inbox-source-absolute-<rowIdx>"]` and `[data-testid="inbox-dest-absolute-<rowIdx>"]` (`PlanGroupRow.tsx:401,421`) — file rows only exist while expanded (`:355 isExpanded`) | visible state |
| E6.2 | Escape / Discard causes no mutation | overlay `[data-testid="plan-approval-overlay"]` closes; filesystem unchanged | visible state |
| E6.3 | A pending root choice is resolvable from inside the surface | `[data-testid="inbox-root-option-<rootId>"]` (`PlanRootPicker.tsx:41`) | visible state |
| E6.4 | No protection status here (corrected) | no protection-gate element in the inbox overlay — inbox uses `PlanApprovalOverlay`/`PlanPanel`, never `PlanReviewOverlay` | visible state |

### S7 — Apply the plan

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E7.1 | Apply reports an aggregate outcome for single and batch flows | single: toast `Plan applied.`; apply-selected: `N plan(s) are being applied.`; apply-all: `All N plans are being applied.`; any failure: `N plans applied; M failed.` | toast |
| E7.2 | Files move to the per-frame-type pattern path | destination in `inbox-dest-absolute-*` matches the built-in default for the class — light `{target}/{filter}/{date}/light/`, dark `darks/{exposure}/`, flat `flats/{filter}/{date}/`, bias `bias/` (`crates/patterns/src/per_type.rs:146-156`) — then verify on disk | visible state + out-of-band |
| E7.3 | One token renders exactly one folder level (`Ha/OIII`, `M42/Trapezium`, `α Centauri`) | applied path depth equals the depth shown in `inbox-dest-absolute-*`; `α Centauri` resolves to its own folder | visible state |
| E7.4 | The Apply gesture stays one click | `plan-apply-selected` / `plan-apply-all` / `plan-apply-one-<id>` — one click, no interstitial | visible state |
| E7.5 | An unapproved plan is refused with `plan.approval_required`, nothing moves | via bridge only: `invoke('inbox_plan_apply', …)` without approving → `plan.approval_required`; message catalog string `Approve the plan before applying it.` (`err_plan_approval_required`) | refusal reason |
| E7.6 | A plan whose source changed since confirm refuses to apply | **NOT REACHABLE THROUGH THE UI APPLY BUTTON** — see finding F1. The `plan-stale-<id>` badge and the `disabled` state on `plan-apply-one-*` can only appear after an apply attempt already marked the plan stale. | refusal reason (unreachable) |
| E7.7 | A destination collision is refused, never overwritten | failed count in the partial-apply toast; per-item reason is NOT rendered (see E7.8); backend code `conflict.destination_exists` (`crates/fs/executor/src/failure.rs:113`) | toast (aggregate only) |
| E7.8 | Per-item error identifiable by name | **NO OBSERVABLE (already corrected in the doc)** — the response carries a per-item error but no inbox UI renders it. Confirm the aggregate count only. | — |

### S8 — Verify the applied outcome

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E8.1 | Badge, `Confirm all (N)` counter and status-bar breakdown all decrement | sidebar count; `[data-testid="inbox-bulk-confirm-btn"]` text `Confirm all (N)`; `[data-testid="statusbar-inbox-summary"]`; `[data-testid^="inbox-stats-type-"]` | visible state |
| E8.2 | The applied action appears in audit history | `.pv-audit-log__event` rows under the Audit Log pane — no testid exists anywhere in `AuditLog.tsx`; see §6 | visible state |
| E8.3 | A refused destination collision appears in audit history | same, with the outcome column showing the failure | visible state |

## 2. Selector map

Every entry below was read from the cited file:line. Where a string is i18n'd
both the KEY and its resolved `en-GB` value are given, resolved from
`apps/desktop/messages/en-GB.json` (2258 keys).

### Page-level actions — `InboxTopBar.tsx`

| Control | Selector | i18n key → en-GB |
| --- | --- | --- |
| Rescan | **no testid** — `button[aria-label="Rescan all roots"]` (`:151`) | `inbox_rescan_all_roots_aria` → `Rescan all roots`; label `common_rescan` → `Rescan`, busy `common_rescanning` → `Rescanning…` |
| Review plans | `[data-testid="inbox-review-plans-btn"]` (`:121`) | `inbox_review_plans_with_count` → `Review plans ({count})`; zero-count `inbox_review_plans` → `Review plans` |
| Confirm all | `[data-testid="inbox-bulk-confirm-btn"]` (`:138`) | `inbox_confirm_all` → `Confirm all ({count})`; busy `common_confirming` → `Confirming…` |

### Queue list — `InboxList.tsx`

| Control | Selector |
| --- | --- |
| List root | `[data-testid="inbox-list"]` (`:513`) |
| Item row | `[data-testid="inbox-item-<inboxItemId>"]` (`:456`) |
| Source-group row | `[data-testid="inbox-source-group-<sourceGroupId>"]` (`:410`) |
| Classify a source group | `[data-testid="inbox-source-group-classify-<sourceGroupId>"]` (`:436`) — `inbox_source_group_classify` → `Classify`, busy `inbox_source_group_classifying` → `Classifying…` |
| Row classification cell | `[data-testid="inbox-row-classification"]` (`:426`, `:496`) — unclassified source group reads `inbox_state_not_yet_classified` → `not yet classified` |
| Planned pill | `[data-testid="inbox-item-plan-pending-<inboxItemId>"]` (`:486`) — `inbox_row_plan_pending` → `Plan pending review` |
| Group header | `[data-testid="inbox-group-<dimension>-<key>"]` (`:389`) |
| Selected row | `tr.pv-inbox-table__row--selected` (`:460`) |

### Detail — `InboxDetail.tsx`

| Control | Selector | i18n |
| --- | --- | --- |
| Confirm | `[data-testid="inbox-confirm-btn"]` (`:323`) | `inbox_confirm_to_inventory` → `Confirm to inventory`; busy `common_working` → `Working…` |
| Destination-root select | `[data-testid="inbox-dest-root-select"]` (`:340`) | label `inbox_dest_root_label` → `Library:`; aria `inbox_dest_root_aria` → `Destination library`; default option `projects_edit_channels_auto_tag` → `Auto` |
| Reveal | `[data-testid="inbox-reveal-btn"]` (`:356`) | label is per-OS: `reveal_label_windows` → `Show in File Explorer`; title `inbox_reveal_title` → `Open this item's location in the OS file browser` |
| Unclassified blocking banner | `[data-testid="inbox-unclassified-alert"]` (`:386`) | `inbox_frame_types_required_title` → `Frame types required`, or when only non-frameType attrs are absent `inbox_mandatory_attrs_required_title` → `More detail needed: {attrs}` |
| Per-file reclassify checkbox | `[data-testid="reclassify-select-<idx>"]` (`:263`) — index-based |
| Per-file type override | `[data-testid="override-select-<relativeFilePath>"]` (`:276`) — **embeds the raw relative path**; on Windows it will contain `\`. Do not build a CSS attribute selector from it; enumerate with `document.querySelectorAll('[data-testid^="override-select-"]')`. |
| Detail scroll region | `.pv-inbox-detail__scroll` (`:363`) |

### Needs-review / bulk reclassify — `InboxNeedsReview.tsx`

| Control | Selector | i18n |
| --- | --- | --- |
| Section title | — | `inbox_needs_review_title` → `Needs review ({count})` |
| Select all | `[data-testid="reclassify-select-all"]` (`:128`) | `common_select_all` → `Select all`; once selected `inbox_n_selected` → `{count} selected` |
| Frame-type select | `[data-testid="bulk-frame-type"]` (`:156`) | `inbox_frame_type_label` → `Frame type`; empty option `inbox_unchanged_placeholder` → `— unchanged —` |
| Other bulk fields | `[data-testid="bulk-filter"]`, `bulk-exposure-s`, `bulk-binning`, `bulk-gain`, `bulk-temperature-c`; unknown registry keys fall back to `bulk-prop-<key>` (`:206`) |
| Heterogeneous warning | `[data-testid="bulk-heterogeneous-warning"]` (`:223`) | `inbox_bulk_heterogeneous_title` → `Selection spans different detected frame types` |
| Heterogeneous ack | `[data-testid="bulk-heterogeneous-ack"]` (`:247`) | `inbox_bulk_heterogeneous_ack_label` → `I understand — apply {type} to every selected file anyway` |
| Apply | `[data-testid="bulk-apply-btn"]` (`:264`) | `inbox_apply_to_selected` → `Apply to selected ({count})`; heterogeneous `inbox_bulk_apply_anyway` → `Apply anyway ({count})`; busy `common_applying` → `Applying…` |
| Undo banner / button | `[data-testid="bulk-undo-banner"]` (`:296`), `[data-testid="bulk-undo-btn"]` (`:311`) | `inbox_bulk_undo_message` → `Applied a frame-type override to {count} file(s).`; button `inbox_bulk_undo_button` → `Undo` |
| Bulk / undo error | `[data-testid="inbox-detail-banner-mt2"]` (`:283`, `:322`, `:352`) — **not unique**, three sites share it |

### Files column — `InboxFilesColumn.tsx`

| Control | Selector | i18n |
| --- | --- | --- |
| Popover trigger | `[data-testid="inbox-files-popover-trigger"]` (`:146`) | `inbox_file_metadata_count` → `File metadata ({count})` |
| Popup | `[data-testid="inbox-files-popup"]` (`:157`) |
| Missing-attr banner | `[data-testid="inbox-missing-attr-banner"]` (`:197`) | `inbox_required_metadata_missing_title` → `Required metadata missing`; body `inbox_required_metadata_body` → `{count} files missing required attribute(s) for their destination — confirm disabled. Assign the missing value(s) in “Needs review” above, then confirm.` |
| Per-file badge | `[data-testid="inbox-missing-attr-<basename>"]` (`:73`) | `inbox_needs_attrs` → `needs {attrs}`; tooltip `inbox_missing_attrs_title` → `Missing required attribute(s): {attrs}` |

### Attribution picker — `AttributionPicker.tsx`

| Control | Selector | i18n |
| --- | --- | --- |
| Picker | `[data-testid="inbox-attribution-picker"]` (`:124`) | `inbox_attribution_title` → `Where do these lights belong?` |
| Leave unassigned | `[data-testid="inbox-attribution-option-unassigned"]` (`:101`) | `inbox_attribution_unassigned` → `Leave unassigned`; desc `Confirm without attributing. Assign later from the project's framings.` |
| Other options | **NO TESTID** — select by accessible name: `inbox_attribution_add_to_framing` → `Add to the existing framing in {project}`; `inbox_attribution_new_framing` → `Add as a new framing in {project}`; `inbox_attribution_new_project` → `Start a new project`; `inbox_attribution_flag_optic_difference` → `Add to {project}, flagged as a different optic train` |
| Reopen warning text | — | `inbox_attribution_reopen_warning` → `Reopens a completed project; archived raw subs may be unavailable` |
| Confirm | `[data-testid="inbox-attribution-confirm"]` (`:136`) | `inbox_attribution_confirm` → `Confirm with this attribution` |
| Cancel | **no testid** — `button` with text `Cancel` (`inbox_attribution_cancel`) |

### Plan review surface — `PlanApprovalOverlay.tsx`, `PlanPanel.tsx`, `PlanGroupRow.tsx`, `PlanRootPicker.tsx`

| Control | Selector | i18n |
| --- | --- | --- |
| Overlay | `[data-testid="plan-approval-overlay"]` (`PlanApprovalOverlay.tsx:79`) | title `inbox_review_plans_title` → `Review plans` |
| Panel / bar / scroll | `[data-testid="plan-panel"]` (`:312`), `plan-panel-bar` (`:323`), `plan-panel-scroll` (`:368`) |
| Select all plans | `[data-testid="plan-select-all"]` (`:333`) | aria `inbox_select_all_plans_aria` → `Select all plans` |
| Total count | `[data-testid="plan-total-count"]` (`:337`) | `plan_count_label` → `{count} plans` · `action_count_label` → `{count} actions` |
| Apply selected | `[data-testid="plan-apply-selected"]` (`:348`) | `inbox_apply_selected_plans` → `Apply selected ({count})`; aria `Apply selected plans` |
| Apply all | `[data-testid="plan-apply-all"]` (`:359`) | `inbox_apply_all` → `Apply all`; aria `Apply all plans` |
| Root picker | `[data-testid="inbox-root-picker"]` (`PlanRootPicker.tsx:27`) | `inbox_choose_dest_root_title` → `Choose a destination library root` |
| Root option | `[data-testid="inbox-root-option-<rootId>"]` (`PlanRootPicker.tsx:41`) |
| Plan group | `[data-testid="plan-group-<inboxItemId>"]` (`PlanGroupRow.tsx:103`) |
| Expand a plan | `[data-testid="plan-group-toggle-<inboxItemId>"]` (`:129`) |
| Plan destination summary | `[data-testid="plan-group-dest-<inboxItemId>"]` (`:203`); move indicator `[data-testid="plan-panel-summary-arrow"]` (`:221`); in-place indicator `[data-testid="plan-panel-inplace"]` (`:207`, `inbox_inplace_label` → `In place`) |
| Full source path (per file, expanded) | `[data-testid="inbox-source-absolute-<rowIdx>"]` (`:401`) |
| Full destination path (per file, expanded) | `[data-testid="inbox-dest-absolute-<rowIdx>"]` (`:421`) |
| Stale badge | `[data-testid="plan-stale-<inboxItemId>"]` (`:262`) | `inbox_stale` → `Stale` |
| Apply one | `[data-testid="plan-apply-one-<inboxItemId>"]` (`:281`) | `inbox_apply_action` → `Apply` |
| Discard one | `[data-testid="plan-cancel-<inboxItemId>"]` (`:292`) | `inbox_discard` → `Discard` |

**Index hazard:** `plan-file-row-*` uses `rowOffset + a.index` (`:370`) while
`inbox-dest-absolute-*` uses `rowOffset + actionPos` (`:358`). These diverge if
`a.index` ever differs from the position in the array. Read the paths from
`inbox-source-absolute-*` / `inbox-dest-absolute-*`; do not compute indices from
`plan-file-row-*`.

### Toasts — the highest-risk selector in this journey

`ToastContainer` is rendered with **no testid** (`Shell.tsx:171`). Verified DOM
(`ui/ToastContainer.tsx:33-36, 66-70`):

```
div.pv-toast__container[aria-live="polite"]
  > div[role="alert"].pv-toast__item.<variant class>
      > span.pv-toast__message   <- the text
```

Read toast text with
`[...document.querySelectorAll('.pv-toast__container .pv-toast__message')].map(e=>e.textContent)`.
A DOM sample that does not include `.pv-toast__container` cannot see a toast at
all — this is the exact failure mode that produced the false
"no UI response whatsoever" report in an earlier run.

Complete toast catalogue for J02, key → en-GB:

| Key | en-GB | Where |
| --- | --- | --- |
| `inbox_toast_plan_created` | `Plan created ({count} items). Review below before applying.` | S5 success |
| `inbox_toast_choose_dest_root` | `Choose a destination library root to generate the plan.` | S5 multi-root |
| `inbox_toast_invalid_destination_root` | `That destination root is not valid.` | S4/S5 refusal |
| `inbox_toast_no_destination_root` | `No library root is registered for this frame type.` | S5 refusal |
| `inbox_toast_missing_path_attrs` | `Some files are missing required attributes. Assign the missing values in the file list, then confirm again.` | S3 refusal |
| `inbox_toast_has_open_plan` | `An open plan already exists for this item.` | S5 refusal |
| `inbox_toast_stale_classification` | `Folder changed since classification — rescan to refresh.` | S5 refusal |
| `inbox_toast_confirm_failed` | `Could not confirm: {message}` | S5 fallback |
| `inbox_toast_bulk_confirmed` | `{count} items confirmed — review plans below.` | Confirm all |
| `inbox_toast_bulk_partial` | `{success} confirmed; {fail} skipped (mixed, missing metadata, or needs root pick).` | Confirm all partial |
| `inbox_toast_bulk_all_need_review` | `Bulk confirm: all items need review (mixed folders or missing metadata).` | Confirm all refusal |
| `inbox_plan_applied_toast` | `Plan applied.` | S7 apply-one success |
| `inbox_plan_apply_failed_toast` | `Apply failed — please try again.` | S7 apply-one failure |
| `inbox_toast_plans_applying` | `{count} plan(s) are being applied.` | S7 apply-selected success |
| `inbox_toast_plans_partial` | `{applied} plan(s) applied; {failed} failed.` | S7 apply-selected partial |
| `inbox_toast_apply_failed` | `Apply failed — please try again.` | S7 apply-selected failure |
| `inbox_toast_all_plans_applying` | `All {count} plans are being applied.` | S7 apply-all success |
| `inbox_toast_all_plans_partial` | `{applied} plans applied; {failed} failed.` | S7 apply-all partial |
| `inbox_toast_plan_discarded` | `Plan discarded. Item is available for re-confirmation.` | S6 discard |
| `inbox_toast_classify_group_failed` | `Could not classify that folder: {message}` | S1 classify failure |
| `inbox_toast_reveal_error` | `Could not open the location.` | S2 reveal failure |

### Status bar, badge, audit

| Observable | Selector |
| --- | --- |
| Status-bar inbox summary | `[data-testid="statusbar-inbox-summary"]` (`useInboxPageStatus.tsx:96`) |
| Per-type breakdown | `[data-testid="inbox-stats-summary"]` (`InboxStatsSummary.tsx:38`), `[data-testid="inbox-stats-type-<frameType>"]` (`:53`) |
| Sidebar inbox badge | **no testid** — `[data-testid="sidebar"]` then the `<a>` whose accessible name is the Inbox nav label; the count is the last `<span>` inside it (`Sidebar.tsx:178-187`). Rendered only when `count > 0` **and the sidebar is not collapsed** — a collapsed sidebar shows no badge at all. |
| Audit Log pane | navigate to `#/settings`, then click the `[data-testid="settings-nav-item"]` whose `textContent` is `Audit Log` (`settings_nav_pane_audit`). **Hash-navigating straight to the sub-pane does not switch it** (recipe on `astro-plan-mg6h8`). |
| Audit rows | **no testid exists in `AuditLog.tsx`** — use `.pv-audit-log__event` (event type), `.pv-audit-log__entity`, `.pv-audit-log__ts`, `.pv-audit-log__detail`, `.pv-audit-log__actor`; empty state `.pv-audit-log__empty` |
| Dock placement | `[data-testid="listpage-detail"][data-dock="side"|"bottom"]` (`ListPageLayout.tsx:328-329`) |

## 3. Fixture recipe

MOCK DATA ONLY. No real astrophotography file is read, copied or referenced.
`D:\Astrophotography` is off limits — do not register it, do not read it.

### Reused unchanged from `astro-plan-mg6h8`

- Build: `scripts/win-native-dev.ps1 -McpBridge` (do NOT hand-roll
  `cargo build -p desktop_shell --features dev-tools` — the bridge answers
  `get_window_info` but every `execute_js` times out without the
  `tauri.dev.conf.json` overlay). Verify the overlay: count `__TAURI_IIFE__`
  occurrences in `target\debug\desktop_shell.exe`; expect 2, `0` means rebuild.
- Launch on console session 1 via `schtasks /Run /TN JVDev` with a per-journey
  `.cmd` carrying the isolation env vars. A GUI started over plain SSH does not
  render and is reaped when SSH closes.
- Talk to the host with `powershell -EncodedCommand <base64 utf-16le>`; the
  remote default shell is `cmd.exe` and word-splits a quoted `-Command`.
- Drive with `ssh <journey-host> "node C:/jv-throwaway/drv.js" < commands.json`
  if `ToolSearch '+tauri'` returns nothing in your session.
- Smoke test first: `get_window_info`, `execute_js('1+1')` → 2,
  `typeof window.__TAURI__` → `"object"`.
- Generator: `C:\jv-throwaway\mkfits.js` (Node only, no dependencies; valid
  2880-byte FITS header blocks, BITPIX 16 / BZERO 32768; 1024x1024 uint16
  ≈2 MB/frame) — also in-repo as `scripts/gen-mock-fits.py`.
- React-controlled inputs need the native value setter plus an `input` event,
  and the click must be a **separate** bridge call.
- Never send modifier-key combos — the renderer freezes.
- The host is strictly serial: bridge port 9223 is hardcoded, Vite holds 5173
  with `strictPort`, and there is a single-instance guard. Queue, do not
  parallelise.

### What I added for J02

`astro-plan-mg6h8`'s fixture set lives **under the registered library roots**,
which yields catalogue-in-place plans. J02 is the **move** path, so J02's
fixtures must sit under an **inbox** root that is not nested inside any library
root (the wizard hard-rejects parent/child overlap).

**Throwaway root, unique to J02: `C:\j02-throwaway\`.** Do not touch
`C:\jv-throwaway\` or `C:\xp-throwaway\` — other units share this host.

```
C:\j02-throwaway\
  j02-dev.cmd                  launcher (PV_DATA_DIR / PV_DB_URL point here)
  app.db  app.db-wal  app.db-shm
  appdata\
  inbox\                       <- INBOX root
    2026-08-25_mixed\          the S1 split fixture, ONE leaf folder
    2026-08-25_missing\        the S3 needs-review fixture
  library\
    Captures\                  <- light_frames root  (destination for lights)
    Captures2\                 <- second light_frames root (S4 picker)
    Backup\Captures2\          <- third light_frames root, SAME basename as
                                  Captures2 (S4's E4.4 negative)
    Calibration\               <- calibration root (destination for darks)
  projects\                    <- project root
  archive\  trash\  logs\
```

**Four roots are mandatory, not three.** `destination_kind_for`
(`crates/app/inbox/src/confirm.rs:850-859`) sends `Light` to a `LightFrames`
root and every calibration class — `Flat`, `Dark`, `Bias`, and all three master
classes — to a `Calibration` root. With no calibration root registered, the dark
sibling of the mixed folder fails confirm with `inbox.no_destination_root`
(`No library root is registered for this frame type.`). The journey's P1 names
only a light-frames root; that is insufficient for S1/S5. Register the
calibration root.

#### `inbox\2026-08-25_mixed\` — one leaf folder, deliberately mixed AND misclassified

Frame types are decided by `parse_frame_type`
(`crates/calibration/master-detect/src/lib.rs:142-171`): the value is
lowercased, the substrings `master`/`frame`/`frames` are stripped, and the first
recognised token wins — `dark`, `bias`|`offset`, `flat`, `light`|`science`|`object`.
So `IMAGETYP = 'Light Frame'` → Light and `'Dark Frame'` → Dark.

| n | File | IMAGETYP | OBJECT | FILTER | EXPTIME | GAIN | Purpose |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 3 | `light_ha_300_00{1,2,3}.fits` | `Light Frame` | `M 42` | `Ha` | `300.0` | `100` | the clean light group |
| 2 | `light_ha_120_00{1,2}.fits` | `Light Frame` | `M 42` | `Ha` | `120.0` | `100` | proves exposure splits the group |
| 2 | `dark_300_00{1,2}.fits` | `Dark Frame` | *absent* | *absent* | `300.0` | `100` | the calibration sibling → forces the calibration root |
| 1 | `sub_misfiled_001.fits` | **`Dark Frame`** | `M 42` | `Ha` | `300.0` | `100` | **the deliberate MISCLASSIFICATION.** Carries a full light header but declares itself a dark, so it lands in the dark group and must be reclassified to `light` in S3. Because the selection then spans detected types, this is also what makes `bulk-heterogeneous-warning` (E3.8) fire. |

Expected S1 outcome: **4 item rows** from this one folder —
`light · Ha · 300s` (3 files), `light · Ha · 120s` (2), `dark · 300s` (3,
including the misfiled sub). N = 4 only if the classifier splits the darks
further; treat the row count as an observation, not a prediction, and record
what you see.

#### `inbox\2026-08-25_missing\` — the S3 needs-review fixture

Mandatory attributes per frame type are exact
(`crates/app/inbox/src/classify.rs:1072-1079`):

| Frame type | Mandatory |
| --- | --- |
| Light | `frameType`, `target`, `filter`, `exposureS` |
| Dark / DarkFlat | `frameType`, `exposureS`, `gain` |
| Bias | `frameType`, `gain` |
| Flat | `frameType`, `filter` |

| n | File | Omission | Gate it exercises |
| --- | --- | --- | --- |
| 2 | `light_nofilter_00{1,2}.fits` | `FILTER` absent, everything else present | `needs filter` badge; Confirm disabled; `inbox.missing_path_attributes` |
| 1 | `light_notarget_001.fits` | `OBJECT` absent, no `OBJCTRA`/`OBJCTDEC` | `needs target` |
| 1 | `light_greek_001.fits` | `OBJECT = α Centauri`, `FILTER = Ha` | E7.3 — Greek-letter target resolves to its own folder, not a fallback |
| 1 | `light_slash_001.fits` | `FILTER = Ha/OIII`, `OBJECT = M42/Trapezium` | E7.3 — one token renders exactly one segment (PR #1724) |

Every frame in both folders also carries, as `mkfits.js` already emits:
`INSTRUME = 'ZWO ASI2600MM Pro'`, `TELESCOP`, `FOCALLEN`, `XBINNING = 1`,
`YBINNING = 1`, `CCD-TEMP = -10.0`, `DATE-OBS = '2026-08-25T21:14:03'`,
`NAXIS1 = 1024`, `NAXIS2 = 1024`.

Do **not** add a master (`STACKCNT`/`NCOMBINE`) or a corrupt file to J02's
inbox. They belong to J08 and J12 and only add noise to J02's split count.

#### Predicted destinations after S7

Built-in defaults, `crates/patterns/src/per_type.rs:146-156` (only if the DB
carries no `patternsByType` override — a fresh `app.db` does not):

- light → `<lightRoot>\{target}\{filter}\{date}\light\`
- dark → `<calRoot>\darks\{exposure}\`
- flat → `<calRoot>\flats\{filter}\{date}\`
- bias → `<calRoot>\bias\`

## 4. Precondition and teardown

### Start state

1. App stopped, `C:\j02-throwaway\app.db{,-wal,-shm}` deleted → clean first-run
   wizard. Deleting `localStorage` alone gives a `/`↔`/setup` redirect loop.
2. Fixtures generated under `C:\j02-throwaway\inbox\` (§3). They survive a DB
   reset; regenerate only if you wipe them.
3. Complete the wizard. It is **8 steps** (Language, Theme, Source Folders,
   Processing Tools, Configuration, Observing Site, Confirm, Scan) — J01's doc
   still says 7 (`astro-plan-ko0tv`). Drive it by clicking the button whose text
   starts `Continue`; the Observing Site step may need the click repeated.
4. Register all four roots with the manual-add controls:
   `manual-add-by-path-<kind>`, `manual-path-input-<kind>`,
   `manual-add-path-btn-<kind>` for kinds `light_frames`, `calibration`,
   `project`, `inbox`. The `e2e-path-input-*` testids named in
   `docs/development/windows-native-rust-dev.md:240-244` **do not exist**
   (`astro-plan-y0jj7`).
5. Register the two same-basename light roots (`library\Captures2` and
   `library\Backup\Captures2`) for S4 only. Note this makes **three**
   light_frames roots, so S4's E4.1 (single-root auto-select) cannot be observed
   in the same session — do E4.1 first with one light root, then add the other
   two, or split S4 across two DB resets.
6. Sidebar expanded, window ≥1400px wide for E2.1's side-dock case; shrink below
   1400px for the bottom-dock case.

### Teardown

- Stop the app: `Get-Process desktop_shell,node,cargo,rustc -EA SilentlyContinue | Stop-Process -Force`.
- Leave `C:\j02-throwaway\` in place with `app.db*` deleted, so the next unit
  reaches a clean wizard in ~90 s without a rebuild.
- Post the final fixture inventory (file count and total bytes) so a later unit
  can tell an apply-moved file from a missing one.
- Do not delete `C:\jv-throwaway\` or `C:\xp-throwaway\`.
- Nothing in J02 reaches a trash or permanent-delete path. If you ever see one
  offered, stop and escalate — that is J07's territory, not J02's.

## 5. Known-gap list — do NOT re-file these

| Ref | What | Recorded in |
| --- | --- | --- |
| K1 | Inbox apply refuses a plan carrying no approval, with `plan.approval_required`, and nothing moves. **Correct behaviour.** | PR #1740, Δ7; `crates/app/core/src/inbox_plan.rs:59` |
| K2 | A corrupt FITS classifies via `EvidenceSource::None` into `unclassified_files` and does not crash the scan. **Correct behaviour.** | PR #1737/#1719/#1733; `crates/app/inbox/src/classify.rs:826,829` |
| K3 | One pattern token renders exactly one path segment, so a metadata value containing `/` cannot add a folder level. **Correct behaviour.** | PR #1724, Δ8; `crates/safe-filename/src/lib.rs` |
| K4 | No per-item apply error is rendered — only an aggregate applied/failed count. The doc already corrected this (S7 Trace). | journey S7 Trace |
| K5 | The destination-root picker surfaces inside the Review-plans overlay, **not** as an inline modal at Confirm time. | `docs/product/journeys/J02-…/deltas/2026-07-14-jval-docdrift.md` |
| K6 | Two roots sharing a last path segment are not disambiguated in the picker (E4.4). Already a corrected Expect + candidate gap. | journey S4 |
| K7 | The Inbox move-plan overlay has no protection-status rendering; protection is a source/archive concept. | journey S6 Trace |
| K8 | SC-009 supersession signal (a superseded sibling's open plan blocked with an explicit signal) is deliberately absent. A folder-wide reclassify refusal while a sibling has an open plan is **intended**. | G3; `specs/058-inbox-drop-parent-items/sc-009-boundary.md`; PR #1097 |
| K9 | The first-run Scan step reports `Nothing detected in this folder.` for a root whose groups yield no master candidate, and totals masters instead of groups. | `astro-plan-1xlik` |
| K10 | J01 drift: the wizard is 8 steps with a Theme step; source protection has 2 tiers, not 3. | `astro-plan-ko0tv`, `astro-plan-jklcq` |
| K11 | `e2e-path-input-*` testids do not exist; use `manual-add-by-path-*`. | `astro-plan-y0jj7`, `astro-plan-npts7` |

### Windows doc Tests that are wrong as written — fix the sequence, do not file a product bug

| Test | Problem |
| --- | --- |
| Test 1 | "click Rescan" then expect the split. Rescan alone leaves a **source-group row**; classification needs an explicit `inbox-source-group-classify-*` click. |
| Test 4 | Expects "a root-picker prompt appears before a plan is generated". Confirm proceeds, a toast says choose a destination, and the picker appears inside the Review-plans overlay. Superseded by K5. |
| Test 7 | Stale-plan refusal. Statically unreachable through the Apply button — see F1. |

## 6. Static-evidence findings

Every finding here is **STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

### F1 — S7's stale-source refusal cannot fire on a first Apply gesture (candidate P1)

**STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

`check_cas` compares the source against the **approval-time** snapshot and skips
the check entirely when both snapshot fields are absent
(`crates/fs/executor/src/ops/cas_check.rs:30-46`; failure text reads
"source size changed since approval", `:69`). The snapshot is written by
`plans::approve` (`crates/app/core/src/plans/approve.rs:101-107`), i.e. at
approval, not at confirm.

Since PR #1740, approval happens **inside the Apply click handler**:
`handleApplyOne` calls `commands.plansApprove(planId)` and then `runPlanApply`
(`apps/desktop/src/features/inbox/useInboxPlanApplyFlow.ts:47-59`);
`approvePlans` does the same for both batch paths (`:30-32`, `:97`, `:131`).
So on a plan's first Apply gesture the snapshot is taken milliseconds before it
is checked, and an edit made between confirm and Apply is captured as the
baseline rather than detected as drift.

Nothing else marks a plan stale. `plan.stale` is purely
`plan_row.state == "stale"` (`crates/app/core/src/inbox_plan.rs:146`, `:316`);
`item_stale = 1` is only ever written by the apply loop's own CAS failure
(`crates/persistence/plans/src/repositories/plan_apply.rs:640,700-703`); there
is no watcher and no confirm-time snapshot.

The one surviving detection route: `approvePlans` swallows failures because
"a plan already in `approved` state rejects a second `plans.approve` yet still
carries a usable approval" (`useInboxPlanApplyFlow.ts:26-32`). A plan approved
by an *earlier* gesture keeps its older snapshot, so a second Apply can report
`ItemStale`.

Consequence for the drive: **E7.6, SC5 and Windows Test 7 are expected to fail**
as an interaction between #1740 and the approval-time snapshot. Ask the running
app the question rather than asserting it — see §7 Q1.

### F2 — a failed inbox rescan produces no user-visible signal (candidate P2)

**STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

`useInboxRescan` catches the scan failure and stores it in local state
(`apps/desktop/src/features/inbox/store/scan.ts:80-84`), but
`useInboxListData` destructures only `{ loading: rescanLoading, rescan }`
(`useInboxListData.ts:124-127`) and returns only those two (`:176-177`);
`InboxPage.tsx:90-91` takes only those two. The `error` field has no consumer
anywhere. There is no toast on the failure path and none on the success path
either. S1 is a mutating step with **no failure signal**, which is exactly what
the journeys' cross-cutting rule forbids.

### F3 — S2's "reveal is not available from Inbox" is stale (document defect)

**STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

The journey asserts as a negative Expect that "The source folder is NOT
revealable from this detail today — `nativeReveal` is wired only into the
Sessions feature, not Inbox". A reveal button exists at
`apps/desktop/src/features/inbox/InboxDetail.tsx:351-359`
(`data-testid="inbox-reveal-btn"`), calling `handleReveal` (`:197-212`) →
`revealInOs` → `commands.nativeReveal` (`apps/desktop/src/shared/native/reveal.ts:102`),
with its own failure toast `inbox_toast_reveal_error`. E2.8 as written should be
reported as a doc correction, not a product gap.

### F4 — S3's "reset path" has no general control (document defect / narrow gap)

**STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

The only reset-shaped control in the Inbox feature is `bulk-undo-btn`
(`InboxNeedsReview.tsx:311`). It is gated on `lastFrameTypeUndo`, which is
`useState` in `useInboxReclassifyState.ts:80` — component-local and lost on
remount or rescan — and its label and aria are frame-type-specific
(`inbox_bulk_undo_aria` → `Undo the last bulk frame-type override`). There is no
reset for a filter, exposure or binning override, and no persistent reset for
any override. The backend primitive the journey cites
(`set_manual_override_reset_stale`) still has no general UI caller: a
case-insensitive search for `reset` across `features/inbox/*.tsx` returns only
an unrelated comment in `PlanApprovalOverlay.tsx:25`.

### F5 — S3's Trace removed a claim the UI actually implements (document defect)

**STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

S3's Trace records that the legacy claim "a selection spanning different
detected types warns before overwriting" was removed because "no corroborating
warning UI or backend check found". The warning exists:
`[data-testid="bulk-heterogeneous-warning"]` (`InboxNeedsReview.tsx:223`) with
title `Selection spans different detected frame types`, plus a **blocking**
acknowledgement checkbox `[data-testid="bulk-heterogeneous-ack"]` (`:247`) — and
`bulk-apply-btn` is `disabled` until `heterogeneousAcked` is true (`:260`).
Attributed to issue #611.

### F6 — G2 is stale; spec 058 has landed (document defect, high impact on the drive)

**STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

G2 says the spec-058 behaviour in S1, S2, SC1, SC6 and SC7 is "not yet
implemented as of `38227ca3`", and instructs the validator to judge S1/S2
against a folder-placeholder model instead. Δ6 already records it as IMPLEMENTED
(PR #1194), and the source agrees:

- `persist_folder_placeholder` and `exclude_split_placeholder!` no longer exist
  as code — both names survive only inside comments
  (`crates/app/inbox/src/classify.rs:492`, `:3501`;
  `crates/app/core/src/inbox_plan.rs:927`).
- The source-group model is live end to end: `inbox_source_groups` rows written
  at scan time, `list_unclassified_source_groups` on the read side, an
  `inbox_classify_source_group` command
  (`apps/desktop/src-tauri/src/commands/inbox.rs:127`, `:408-421`, `:453-455`,
  `:598-599`), and a `row.kind === 'sourceGroup'` branch in the list
  (`InboxList.tsx:410-436`).

**Judge S1, S2, SC1, SC6 and SC7 against the journey body, not against G2.**

### F7 — selectors that do not exist, and unstable ones

**STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

Absent testids, each confirmed by reading the rendering component:

1. **Rescan** — no testid; only `aria-label="Rescan all roots"`
   (`InboxTopBar.tsx:151`).
2. **Toasts** — `<ToastContainer />` is rendered bare at `Shell.tsx:171`; the
   container, the item and the message all have no testid. Use
   `.pv-toast__container .pv-toast__message`.
3. **Audit log** — `apps/desktop/src/features/settings/AuditLog.tsx` contains
   **zero** `data-testid` attributes. S8's audit Expects can only be read from
   `pv-audit-log__*` class names, or more reliably from the database.
4. **`SourceBadge`** — no testid and only vanilla-extract generated class names
   (`RenderValue.tsx:67-84`). Provenance must be read as text: `FITS`, `User`,
   `Inferred`, `Default`.
5. **Attribution candidate options** — only the `Leave unassigned` option has a
   testid (`AttributionPicker.tsx:101`); the ranked candidates must be selected
   by accessible name.
6. **Sidebar inbox badge** — no testid, and it is not rendered at all when the
   sidebar is collapsed (`Sidebar.tsx:178`).

Unstable or non-unique testids:

7. `[data-testid="inbox-detail-banner-mt2"]` is used at three sites in
   `InboxNeedsReview.tsx` (`:283`, `:322`, `:352`) — never assume one match.
8. `override-select-<relativeFilePath>` embeds the raw relative path, which on
   Windows contains `\`. Enumerate with a `^=` prefix query instead.
9. `reclassify-select-<idx>` is positional and shifts as the needs-review set
   changes.

### F8 — broken trace reference (document defect, cosmetic)

**STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.**

The journey's `trace:` lists `deltas/2026-07-14-jval-docdrift.md`, but
`docs/journeys/J02-ingest-review-reclassify-confirm-move/deltas/` does not
exist. The file is at
`docs/product/journeys/J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-jval-docdrift.md`
and is marked MIGRATED/frozen. All seven `e2e-agentic-test` scenario paths and
`specs/058-inbox-drop-parent-items/sc-009-boundary.md` do exist.

## 7. Questions for the running app

Record answers; do not pre-judge them.

- **Q1 (F1)** — Confirm a plan, modify the source file, then click Apply once.
  Does the plan apply successfully (confirming F1) or is it refused? Capture the
  toast text and the `plan-stale-<id>` presence. Then, without resetting, click
  Apply a second time on a plan that was already approved and check whether
  `ItemStale` appears on that attempt.
- **Q2 (F2)** — Point the inbox root at a path that fails to scan (e.g. revoke
  read access) and click Rescan. Is any toast, banner or console error produced?
- **Q3 (E1.2)** — After clicking Classify on `2026-08-25_mixed`, how many item
  rows appear, and does the misfiled sub land in the dark group?
- **Q4 (E1.4)** — Leave the Inbox page open for 60 s. Does React DevTools or the
  IPC monitor show repeated `inbox.list` calls?
- **Q5 (E7.3)** — What exact folder name does `Ha/OIII` produce, and does
  `α Centauri` get its own folder?
- **Q6 (E1.3)** — Do the sidebar badge, `statusbar-inbox-summary`,
  `inbox-stats-summary` and the visible row count agree before and after S7?

## Commands run, with exit codes

All read-only, all in `<repo>`. No file in the
repository was modified.

| Command | Exit |
| --- | --- |
| `bd show astro-plan-a5f4f` | 0 |
| `BEADS_ACTOR=… BD_ACTOR=… bd update astro-plan-a5f4f --claim` | 0 |
| `bd show astro-plan-mg6h8`, `bd comments astro-plan-mg6h8` | 0 |
| `cat /tmp/u-common.md` | 0 |
| `git rev-parse --short HEAD` → `38d07af07` | 0 |
| `rg -n 'data-testid' apps/desktop/src/features/inbox/` | 0 |
| `python3 -c` catalogue lookups against `apps/desktop/messages/en-GB.json` (2258 keys) | 0 |
| `rg -c 'persist_folder_placeholder' -g '!*.md' .` (1 hit, comment only) | 0 |
| `rg -c 'exclude_split_placeholder' -g '!*.md' .` (3 hits, comments only) | 0 |
| `rg -c 'sourceGroup' apps/desktop/src/features/inbox/InboxList.tsx` → 17 (positive control for the two searches above) | 0 |
| `grep -c '^### S' journey.md` → 8; `grep -c '^- \*\*Expect' journey.md` → 21 | 0 |
| existence check of 7 `e2e-agentic-test` scenario paths | 0 (all present) |
| `ls docs/journeys/J02-…/deltas/` | **1** (no such directory — F8) |
| combined census pipeline (steps/expects/SC/G/deltas/deltas-dir) | **1** (the trailing `ls` failed; every count before it printed) |

Two non-zero exits, both expected and both reported above. No search reported a
zero that was not sanity-checked against a pattern known to match.

### Tooling artefact worth knowing

A local output-rewriting hook mangles some literals in tool output: `IMAGETYP`
was rendered as `n`, and `== "stale"` as `=n`, in `rg`/`grep` output. Where a
literal mattered I re-read the file through `python3` instead. Header keyword
names in §3 are the real source literals.
