# J01 drive plan — offline prep

Journey: `docs/journeys/J01-first-run-setup-data-sources/journey.md` v9 (status
draft, last_reviewed 2026-07-14).
Prepared 2026-08-25 from source at branch `chore/journey-validation-formula`
(working tree of `<repo>`).
Every statement here is STATIC evidence read from source, docs, beads, or `gh`
output. Nothing in this file was observed in a running app by this unit.

The product has **8** wizard steps, not the 7 the journey describes. Step
numbering below follows the PRODUCT (`SetupWizard.tsx:86-130`), and each
journey step id is mapped to it.

| product step | index | label source | en-GB label |
|---|---|---|---|
| 1 Language | 0 | `m.setup_language_label` | Language |
| 2 Theme | 1 | `m.settings_general_theme` | Theme |
| 3 Source Folders | 2 | `m.setup_step_sources_label` | Source Folders |
| 4 Processing Tools | 3 | `m.setup_step_tools_label` | Processing Tools |
| 5 Configuration | 4 | `m.setup_step_config_label_heading` | Configuration |
| 6 Observing Site | 5 | `m.setup_step_site_label` | Observing Site |
| 7 Confirm | 6 | `m.common_confirm` | Confirm |
| 8 Scan | 7 | `m.setup_step_scan_label` | Scan |

---

## 1. Expectation inventory

Signal vocabulary: `toast`, `nav` (route/hash change), `state` (visible control
or text state), `refusal` (a stated reason for a blocked action), `item-err`
(per-item error), `probe` (backend read — never a substitute for a UI Expect).

### S1 — Open the setup wizard

| # | Expect | Observable | Signal |
|---|---|---|---|
| E1.1 | 7-step wizard with the named steps | step-bar card count and labels | state |
| E1.2 | restart pre-fills previously registered folders | folder rows on step 3 | state |
| E1.3 | step bar renders real focusable buttons | `tagName` of `[data-testid="wizard-steps-card"]` | state |
| E1.4 | completed step is a free backward jump | click a prior card, step changes | nav |
| E1.5 | forward jump gated on intermediate validity | `disabled` on later cards | state |
| E1.6 | Scan is never a plain jump target | Scan card `disabled` until entered via Start scan | state |
| E1.7 | (neg) restart never deletes registered folders | `roots_list` count unchanged | probe + state |
| E1.8 | (neg) re-confirming an unchanged restart buffer is not stuck behind "batch registration failed" | absence of `[data-testid="setup-submit-error"]`, and advance to Scan | state |

E1.1 is a KNOWN MISMATCH — see §5.

### S1a — Choose a language

| # | Expect | Observable | Signal |
|---|---|---|---|
| E1a.1 | Language is the first step | step index 0 label | state |
| E1a.2 | base locale starts selected | `aria-pressed="true"` on the base card | state |
| E1a.3 | picking another applies to the wizard immediately, no reload | wizard chrome text changes in place | state |
| E1a.4 | nothing entered on a later visited step is lost | re-visit step 3, rows intact | state |
| E1a.5 | every option keyboard-reachable and selectable | Tab/Enter reaches each card | state |
| E1a.6 | selection exposed via `aria-pressed` | attribute value | state |
| E1a.7 | accessible name is the native name, not the flag | `accessibleName` of the card | state |
| E1a.8 | choice carries through Finish into the app | app chrome locale after S8 | state |
| E1a.9 | choice survives a full app restart | locale after relaunch | state |
| E1a.10 | (neg) not lost by forward-then-back navigation | selection after Back | state |
| E1a.11 | (neg) a missing key falls back to base locale, never a raw key or blank | no `raw_key`-shaped text in the DOM | state |

E1a.11 needs a locale with a deliberately missing key. `pt-BR.json` is the only
non-base catalogue (`apps/desktop/messages/`, 2 files); whether it has a gap is
UNVERIFIED. **Open question for the driving phase:** does any real `pt-BR` key
gap exist, or is E1a.11 unexercisable without editing a catalogue (which this
run must not do)?

### S2 — Add source folders

| # | Expect | Observable | Signal |
|---|---|---|---|
| E2.1 | required categories listed before optional, under Required/Optional headings | group order + `[data-testid="requirement-status-<kind>"]` pill text | state |
| E2.2 | every category carries a keyboard/SR-accessible tooltip | `InfoTip` next to each group heading | state |
| E2.3 | no scan-depth control on a source row | absence of any Recursive/Single-level control in `[data-testid="step-sources-row-main"]` | state |
| E2.4 | Light frames AND Project outputs required; progress blocked while either empty | Continue button `disabled` | state |
| E2.5 | empty path rejected inline with a distinct accessible error | `[role="alert"]` inside the add controls | item-err |
| E2.6 | same-category duplicate rejected inline | `[role="alert"]` text | item-err |
| E2.7 | cross-category duplicate rejected inline | `[role="alert"]` text | item-err |
| E2.8 | parent-or-subfolder overlap in the buffer rejected inline | `[role="alert"]` text | item-err |
| E2.9 | nonexistent / non-directory / unreadable path accepted into the buffer at add time | row appears with no error | state |
| E2.10 | that path is rejected only at S6 registration, as part of a batch-failure message | `[data-testid="setup-submit-error"]` | refusal |
| E2.11 | overlap with a previous-session root deferred to S6 with `path.overlaps_existing` | batch-failure text names the reason | refusal |
| E2.12 | (neg) nothing registered before S6 | `roots_list` returns `[]` | probe |
| E2.13 | (neg) exact duplicate is a hard rejection, never a bypassable warning | no "continue anyway" affordance | state |

E2.5 is a KNOWN MISMATCH — see §5. E2.9 conflicts with source: `handleAdd`
calls `checkPathExists` and sets `addError` before `onAdd`
(`StepSourceFolders.tsx:231-237`), i.e. a nonexistent path IS rejected at add
time. See §6/F1.

### S3 — Point at a processing tool

| # | Expect | Observable | Signal |
|---|---|---|---|
| E3.1 | skip or default accepted with no error | Continue enabled, no error text | state |
| E3.2 | the choice or its absence carries into S6's summary | Confirm step tool lines | state |
| E3.3 | executable picker offers only executable-typed files (no "All files") | native OS dialog filter | state — NOT DRIVABLE |
| E3.4 | an implausible executable pick rejected inline | in-card error | item-err — NOT DRIVABLE via picker |
| E3.5 | a no-extension path treated as plausibly valid | in-card status | state — NOT DRIVABLE via picker |
| E3.6 | status shows Not detected / Detected / Invalid | status pill in `[data-testid="tool-card-<key>"]` | state |
| E3.7 | (neg) an invalid tool-path pick does not block Continue | Continue enabled | state |

E3.3–E3.5 sit behind the native OS dialog and were already recorded
unvalidated in the 2026-08-24 run. `StepTools.tsx` exposes no manual-path
input (`rg data-testid apps/desktop/src/features/setup/steps/StepTools.tsx`
returns only `tool-card-<key>` at line 234), so they stay unexercisable —
report them UNVALIDATED, not FAIL.

### S4 — Configure basic settings

| # | Expect | Observable | Signal |
|---|---|---|---|
| E4.1 | step accepts skip/default with no error | Continue enabled | state |
| E4.2 | untouched protection level stays "protected" | select value | state |
| E4.3 | Theme control on this step is live and bound to the app theme runtime | theme applies to the wizard | state |
| E4.4 | System resolves a dark OS preference to Observatory Cool | applied theme class | state |
| E4.5 | Density choice previews live (wizard applies its own `density-*` class) | `class` of `[data-testid="setup-page"]` | state |
| E4.6 | (neg) the Theme control is a plain `<select>` over all 6 registry themes | option list | state |

E4.2 (three-tier protection), E4.3, E4.4 and E4.6 are KNOWN MISMATCHES — the
Theme control is not on this step at all. See §5.

### S5 — Register an observing site

| # | Expect | Observable | Signal |
|---|---|---|---|
| E5.1 | step can be left entirely blank; Continue not blocked with both coords empty | Continue enabled, label `Continue without a site →` | state |
| E5.2 | Name becomes required as soon as coordinates are entered | Continue `disabled` | state |
| E5.3 | Continue blocks on out-of-range latitude, longitude, or unparsable elevation | Continue `disabled` + stated range text | refusal |
| E5.4 | (neg) valid coordinates never silently dropped for a missing Name | Continue refuses instead of advancing | refusal |
| E5.5 | values carry into S6's summary | Confirm step site line | state |
| E5.6 | on Finish, saved as both default and active site with astronomical-twilight / 0° horizon | settings readback | probe |

### S6 — Confirm sources

| # | Expect | Observable | Signal |
|---|---|---|---|
| E6.1 | summary states per folder: category, organized/unorganized, scan depth "Recursive" | Confirm step text | state |
| E6.2 | summary lists enabled tools with configured path | Confirm step text | state |
| E6.3 | summary carries a "what happens next" note | Confirm step text | state |
| E6.4 | proceeding is what registers every source and starts scanning | `roots_list` before/after Start scan | probe + nav |
| E6.5 | a genuine failure shows a batch-failure message and does not advance to Scan | `[data-testid="setup-submit-error"]` + step unchanged | refusal |
| E6.6 | an exact-duplicate-of-already-registered row is a benign no-op and the wizard advances | advance to Scan with no error banner | nav |
| E6.7 | (neg) no scan starts before leaving this step | `roots_list` empty while on Confirm | probe |

E6.1's scan-depth clause is a KNOWN MISMATCH — see §5.

### S7 — Scan registered folders

| # | Expect | Observable | Signal |
|---|---|---|---|
| E7.1 | every source registered by this flush reaches a terminal state, incl. "0 items" for an empty folder | per-source phase text `Done` | state |
| E7.2 | a genuinely pre-registered source is skipped, not rescanned under a synthetic root id | per-source row shows skip, not a scan | state |
| E7.3 | a same-session-retry source that was never scanned IS rescanned | per-source row shows a scan result | state |
| E7.4 | Detected types and the file-count chip account for unclassified files and masters, reconciling with the folder total | per-source counts vs. the folder's real file count | state |
| E7.5 | an expanded folder table's root row reads "(root)", never blank | Folder/File cell text | state |
| E7.6 | (neg) Finish never enables while any source is scanning | `[data-testid="finish-button"]` disabled | state |
| E7.7 | (neg) no source registered earlier in this session disappears with neither result nor error | row present for every registered source | state |
| E7.8 | (neg) an unparsable FITS/XISF header is a per-file metadata error and the scan continues | per-file error surface + remaining files still scanned | item-err |

E7.4 is a KNOWN PRODUCT DEFECT — see §5. E7.8 (Δ9) has never been exercised;
§3 supplies the fixture for it.

### S8 — Finish setup

| # | Expect | Observable | Signal |
|---|---|---|---|
| E8.1 | setup marked complete and the app lands on Inbox | `location.hash === '#/inbox'` | nav |
| E8.2 | completion flag persists — quit and relaunch goes straight to Inbox | route after relaunch | nav |

E8.2 was unvalidated on 2026-08-24. It is the cheapest outstanding Expect in
the whole first-run arc; do it early.

### S9 — Rescan a data source

| # | Expect | Observable | Signal |
|---|---|---|---|
| E9.1 | the scan re-runs without re-prompting for a path | no dialog opens | state |
| E9.2 | an explicit started→finished signal at the control | menu item text `Rescanning…` → `Rescan` | state |
| E9.3 | a count delta at the control | card meta line file count / `scanned {date}` | state |
| E9.4 | a user-initiated rescan writes a durable workflow-severity audit row | Settings → Audit Log row | probe/state |
| E9.5 | (neg) an automatic/periodic rescan writes only a diagnostic-severity row | audit row severity | probe |
| E9.6 | every S9–S14 action is reached through one `role="menu"` kebab | `[data-testid="data-sources-kebab-btn"]` opens `[role="menu"]` | state |

E9.3 has no dedicated delta indicator in source (see §6/F2). E9.5 has no
user-triggerable path documented — record it as unexercisable unless the driver
can wait out a periodic rescan.

### S10 — Remap a data source

| # | Expect | Observable | Signal |
|---|---|---|---|
| E10.1 | Verify checks the new path against every recorded item (all `file_record` rows plus pending inbox items), no file movement | banner count vs. real item count | state |
| E10.2 | Verify reports "{matched} of {total} recorded items were found" | banner text | state |
| E10.3 | a root with zero recorded items gets its own distinct message | banner text | state |
| E10.4 | Apply remap persists the new path and writes a durable audit row with old→new | card path + Audit Log | state + probe |
| E10.5 | editing the path after Verify invalidates it; Apply unavailable until a fresh Verify | Apply button `disabled` | state |
| E10.6 | (neg) Verify on an empty or nonexistent path never reports success | error banner, no success banner | refusal |
| E10.7 | (neg) Apply not clickable before a successful Verify | `disabled` | state |
| E10.8 | (neg) no file on disk moves at any point | SHA-256 + listing before/after | probe |
| E10.9 | (neg) an unverified Apply is refused server-side with `remap.not_verified` and a `refused` audit row | direct IPC call result + Audit Log | refusal |
| E10.10 | (neg) the server recomputes verification rather than trusting a caller-supplied flag | direct IPC call with `verified: true` on a bad path | refusal |

### S11 — Disable / re-enable

| # | Expect | Observable | Signal |
|---|---|---|---|
| E11.1 | the state visibly flips and persists across reload | `Disabled` pill on the card | state |
| E11.2 | a disabled source drops out of scan/ingest | Inbox/scan no longer covers it | state |
| E11.3 | each transition writes a durable audit row with before→after | Audit Log | probe |
| E11.4 | (neg) disabling requires a confirm step | `[data-testid="disable-root-confirm"]` opens | state |
| E11.5 | (neg) re-enabling applies immediately with no confirm | no modal appears | state |
| E11.6 | (neg) disabling never hides the source's prior history | history still listed | state |

### S12 — Delete (un-register)

| # | Expect | Observable | Signal |
|---|---|---|---|
| E12.1 | a confirm appears | `[data-testid="delete-root-confirm"]` | state |
| E12.2 | confirming un-registers the source and writes a durable audit row | card gone + Audit Log | state + probe |
| E12.3 | reachable whether the source is online or offline | menu item present in both states | state |
| E12.4 | with dependent records, Delete is blocked/disabled with an explanatory message | error text inside the confirm modal | refusal |
| E12.5 | (neg) never removes files from disk | listing + hashes unchanged | probe |
| E12.6 | (neg) never succeeds while dependents exist | card still present after confirm | refusal |

### S13 — Per-source protection override

| # | Expect | Observable | Signal |
|---|---|---|---|
| E13.1 | set is visible in the pane | protection pill on the card | state |
| E13.2 | set is confirmed by a backend readback | pill after reload | state |
| E13.3 | remove is visible in the pane | pill returns to inherited | state |
| E13.4 | each of set and remove writes a durable audit row with a resolvable `auditId` | Audit Log | probe |
| E13.5 | "Restore defaults" states which settings it resets, and each is visible in the pane | Restore-defaults dialog text | state |
| E13.6 | (neg) merely opening/reading the pane produces no audit row | audit count unchanged | probe |

E13.3 / the remove half of E13.4 have NO UI CONTROL — see §6/F3.

### S14 — Reveal in the OS file manager

| # | Expect | Observable | Signal |
|---|---|---|---|
| E14.1 | the OS-native file manager opens at exactly that folder, not a parent | an Explorer window whose location is the root path | state |

### Unmappable Expects (findings about the journey document)

- **E1a.11** — no observable exists without a locale that actually has a
  missing key; the doc names no such fixture.
- **E9.5** — the doc names no way to trigger an automatic/periodic rescan, so
  its negative has no reachable observable.
- **E13.3/E13.4-remove** — no removal control exists in the pane
  (`SourceProtectionOverride.tsx`), so the doc asserts an action with no
  observable.
- **E11.2 / E11.6 / E12.4** — the doc names the outcome but no surface. For
  E11.2 the only concrete surface is that a disabled root drops out of the
  Inbox rescan set; for E11.6 the doc does not say WHERE history is visible;
  for E12.4 the "explanatory message" surface is the confirm modal's `error`
  prop (`DataSources.tsx:279`), not a disabled button as the doc words it.

---

## 2. Selector map

All `data-testid` values below were read from source at the cited path:line.
i18n values are from `apps/desktop/messages/en-GB.json` (the base locale); each
row gives the KEY as well, because a run in `pt-BR` will show different text.
**Nothing here is guessed.** Where no selector exists, the row says so.

### Wizard shell

| Observable | Selector | Source |
|---|---|---|
| setup page root (carries `density-*`) | `[data-testid="setup-page"]` | `SetupPage.tsx:58` |
| step bar container | `[data-testid="wizard-steps-bar"]` | `WizardShell.tsx:175` |
| each step card (a real `<button>`) | `[data-testid="wizard-steps-card"]` | `WizardShell.tsx:195-199` |
| the active step | `[aria-current="step"]` | `WizardShell.tsx:200` |
| scroll region | `[data-testid="wizard-scroll"]` | `WizardShell.tsx:161` |
| step counter text | key `setup_wizard_step_label` → `Setup · Step {step} of {total}` | `SetupWizard.tsx:699` |
| Back control | key `setup_wizard_back` → `← Back` | catalogue |
| Continue control | key `setup_wizard_continue_to` → `Continue to {label} →` | `SetupWizard.tsx:664` |
| Continue on an empty Site step | key `setup_wizard_continue_without_site` → `Continue without a site →` | catalogue |
| Confirm → Scan control | key `setup_wizard_start_scan` → `Start scan →` | catalogue |
| registering in flight | key `setup_wizard_registering` → `Registering…` | catalogue |
| Finish | `[data-testid="finish-button"]`, key `setup_wizard_finish` → `Finish` | `SetupWizard.tsx:636` |
| finishing in flight | key `setup_wizard_finishing` → `Finishing…` | catalogue |
| batch/submit failure banner | `[data-testid="setup-submit-error"]` | `SetupWizard.tsx:712` |
| site-skip acknowledgement | `[data-testid="setup-site-skip-ack"]` | `SetupWizard.tsx:659` |
| site-skip warning | `[data-testid="setup-site-skip-warning"]`, key `setup_step_site_skip_warning` | `SetupWizard.tsx:743` |

Step-bar labels in en-GB, in product order: `Language`, `Theme`,
`Source Folders`, `Processing Tools`, `Configuration`, `Observing Site`,
`Confirm`, `Scan` (`SetupWizard.tsx:86-130` + catalogue).

### Step 3 — Source Folders

| Observable | Selector | Source |
|---|---|---|
| step root | `[data-testid="step-sources"]` | `StepSourceFolders.tsx:151` |
| per-category group | `[data-testid="source-group-<kind>"]`, `kind` ∈ `light_frames` `calibration` `project` `inbox` | `:246` |
| group header row | `[data-testid="step-sources-group-header"]` | `:255` |
| required/optional pill | `[data-testid="requirement-status-<kind>"]`; keys `setup_sources_required` → `required` (plus ` ✓` when met), `setup_sources_optional` → `optional` | `:270,:277` |
| required-ness as data | `[data-required]` / `[data-requirement-met]` on the group | `:247-248` |
| category tooltip | the `InfoTip` next to the group heading (no testid) | `:261` |
| a folder row | `[data-testid="step-sources-row-main"]` | `:334` |
| organized/unorganized select | `[data-testid="org-select-<kind>-<index>"]`; options keys `setup_sources_org_organized` → `Already organized`, `setup_sources_org_unorganized` → `Needs organizing` | `:343,:350-355` |
| remove a row | key `common_remove` → `Remove` | `:361` |
| native picker button | `[data-testid="btn-primary"]` inside `[data-testid="step-sources-add-actions"]`; key `setup_add_folder` → `+ Add folder…` | `:407-419` |
| **manual add-by-path wrapper** | `[data-testid="manual-add-by-path-<kind>"]` | `:432` |
| **manual path input** | `[data-testid="manual-path-input-<kind>"]`; aria key `setup_sources_manual_path_aria` → `{kind} folder path` | `:436-439` |
| **manual add button** | `[data-testid="manual-add-path-btn-<kind>"]`; key `setup_sources_add_by_path` → `Add by path` | `:452-456` |
| add-time rejection message | `[role="alert"].pv-step-sources__picker-error` | `:464-468` |
| per-row error | `.pv-step-sources__row-error` (no testid, no role) | `:365` |

The manual add-by-path control is **unconditional** — not gated on `VITE_E2E`
or any env flag (`StepSourceFolders.tsx:430-458`). Setting the input value
requires the native value setter plus an `input` event, and the click must be a
separate call so React commits state; the button is `disabled` while the
trimmed value is empty (`:454`).

### Step 4 — Processing Tools

| Observable | Selector | Source |
|---|---|---|
| a tool card | `[data-testid="tool-card-<key>"]`, `key` ∈ `pixinsight` `siril` | `StepTools.tsx:234`; ids at `SetupWizard.tsx:476,481` |

No testid on the status pill, the browse control, or the redetect control.

### Step 2 — Theme

| Observable | Selector | Source |
|---|---|---|
| step root | `[data-testid="step-theme"]` | `StepTheme.tsx:18` |
| live specimen | `[data-testid="theme-specimen"]` | `:23` |

### Step 8 — Scan

| Observable | Selector | Source |
|---|---|---|
| step root | `[data-testid="step-scan"]` | `StepScan.tsx:224` |
| footer total | `[data-testid="scan-summary"]` | `:243` |
| a per-source block | `[data-testid="scan-source-<absolute path>"]` | `SourceSummary.tsx:231` |
| "nothing found" state | `[data-testid="scan-empty"]`; key `setup_scan_nothing_detected` → `Nothing detected in this folder.` | `:316` |
| no sources at all | key `setup_scan_no_sources` → `No sources registered yet. Go back and add at least one folder.` | catalogue |
| terminal phase | key `setup_scan_phase_done` → `Done`; pending is `setup_scan_phase_pending` → `Pending` | catalogue |
| table headers | keys `setup_scan_col_folder` → `Folder / File`, `setup_scan_col_files` → `Files`, `setup_scan_col_types` → `Detected types`, `setup_scan_col_format` → `Format` | catalogue |
| per-file unreadable-type surface (E7.8) | keys `setup_scan_classify_failed` → `Could not read types`, `setup_scan_classify_failed_count` → `{count} file with an unreadable type` / `{count} files with unreadable types` | catalogue |
| master count | keys `setup_scan_master` → `Master`, `setup_scan_master_count` → `{count} master(s)` | catalogue |
| folder / file counts | keys `setup_scan_folder_count`, `setup_scan_file_count` → `{count} folder(s)` / `{count} file(s)` | catalogue |

`setup_scan_classify_failed*` is the concrete surface for E7.8. It is a
per-source aggregate, so a damaged file shows as an unreadable-type count on
its root's block, not as a named per-file row — check for that string, and for
the sibling good files still counting.

### Settings → Data Sources (S9–S14)

Pane heading key `common_sources` → `Sources`. Empty state key
`settings_datasources_empty` → `No source folders registered yet. Add one
above.` Add-source button key `settings_datasources_add_btn` →
`+ Add source folder`.

| Observable | Selector | Source |
|---|---|---|
| a card's path | `[data-testid="data-sources-root-path"]` | `RootCard.tsx:106` |
| kebab trigger | `[data-testid="data-sources-kebab-btn"]`; aria key `settings_datasources_actions_aria` → `Source actions`; `aria-haspopup="menu"`, `aria-expanded` | `:141-144` |
| the menu | `[role="menu"]`, items `[role="menuitem"]` | `:150-154` |
| Rescan item | key `common_rescan` → `Rescan`; in flight `common_rescanning` → `Rescanning…` | `:162` |
| Reconcile item | `[data-testid="reconcile-now-<rootId>"]`; key `common_reconcile` → `Reconcile` | `:170-174` |
| Remap item | key `settings_datasources_remap` → `Remap…` | `:186` |
| Edit protection item | key `settings_datasources_edit_protection` → `Edit protection…` | `:197` |
| Disable / Enable item | keys `settings_datasources_disable` → `Disable`, `settings_datasources_enable` → `Enable`; in flight `common_disabling` → `Disabling…`, `common_enabling` → `Enabling…` | `:210-216` |
| Reveal item | `revealLabel()` → on Windows key `reveal_label_windows` → `Show in File Explorer` (macOS `Reveal in Finder`, Linux `Show in file manager`) | `:225`; `lib/reveal-label.ts` |
| **Delete item** | key `settings_datasources_delete` → **`Remove`** (NOT "Delete"); in flight `common_deleting` → `Deleting…` | `:241` |
| offline pill | key `nav_roots_offline_suffix` → `offline` | `:117` |
| disabled pill | key `settings_datasources_disabled_pill` → `Disabled` | `:122` |
| card meta line | keys `data_sources_file_count` → `{formatted} file(s)`, `settings_datasources_scanned` → `scanned {date}` (humanised via `date-fns`, e.g. "2 days ago") | `:55-73` |
| disable confirm | `[data-testid="disable-root-confirm"]`; title key `settings_datasources_disable_confirm_title` → `Disable this source?`; body `settings_datasources_disable_confirm_desc` → `The source will be excluded from scans and ingest until re-enabled. Its history is kept.` | `DataSources.tsx:248-263` |
| delete confirm | `[data-testid="delete-root-confirm"]`; title key `settings_datasources_delete_confirm_title` → `Remove this source?`; body `settings_datasources_delete_confirm_desc` → `"{path}" will no longer be tracked. Files on disk are never touched — this only removes the registration.` | `:267-281` |
| reconcile error | `.pv-data-sources__add-error`; key `settings_datasources_reconcile_error` | `:209-213` |

**The Delete/Remove label trap is the single highest false-negative risk in
S12.** The journey, the delta log and the stale Windows doc all say "Delete";
the DOM says `Remove`. Match on the menu item's position/role or on `Remove`,
never on "Delete".

### Remap dialog (S10)

Modal `[data-testid="remap-root-dialog"]` (`RemapRootDialog.tsx:112`). No
testid on any button or field inside it — drive by accessible name.

| Observable | Selector | Source |
|---|---|---|
| dialog title / aria label | key `settings_datasources_remap_title` → `Remap root` | `:108-111` |
| current-path label | key `settings_datasources_remap_current_path_label` → `Current path` | `:140` |
| **new-path input** | the `DirPicker` text input; `aria-label` = the passed label = key `settings_datasources_remap_new_path_label` → `New path`; placeholder key `ui_dir_picker_no_folder` → `No folder selected` | `:145-150`; `ui/DirPicker.tsx:50-57` |
| native browse button | key `ui_dir_picker_choose` → `Choose folder…` | `DirPicker.tsx:59` |
| Verify | key `settings_datasources_remap_verify_btn` → `Verify`; in flight `settings_datasources_remap_verifying` → `Verifying…` | `:122-124` |
| Apply | key `settings_datasources_remap_apply_btn` → `Apply remap`; in flight `common_applying` → `Applying…` | `:131-133` |
| Cancel | key `common_cancel` → `Cancel` | `:116` |
| all-verified banner | key `settings_datasources_remap_all_verified_count` → `All {total} recorded items were found at the new path.` | `:170` |
| partial banner | key `settings_datasources_remap_not_all_verified_count` → `{matched} of {total} recorded items were found at the new path. Review the items below before applying.` | `:173` |
| zero-items banner | key `settings_datasources_remap_no_items` → `This root has no recorded items to verify — the new path will be applied as-is.` | `:167` |
| per-item pills | keys `settings_datasources_remap_found` → `Found`, `settings_datasources_remap_not_found` → `Not found` | `:187-188` |
| error banner | key `settings_datasources_remap_error` → `Could not remap: {error}` | `:154` |

The `DirPicker` input is a real text field, so E10.5/E10.6 are drivable by
typing (issue #662, closed). Verify is `disabled` while the path is empty or
equal to the current path (`:120`); Apply is `disabled` only while
`verification == null` (`:129`).

### Protection override (S13)

No testid anywhere in `SourceProtectionOverride.tsx`. Drive by:

| Observable | Selector | Source |
|---|---|---|
| level select | `aria-label` = key `settings_source_protect_level_aria` → `Protection level override` | `:177` |
| level label | key `settings_source_protect_level_label` → `Protection level` | catalogue |
| options | keys `settings_cleanup_protection_protected` → `Protected`, `settings_cleanup_protection_unprotected` → `Unprotected` (two levels only) | catalogue |
| save | key `settings_source_protect_save_btn` → `Save override`; in flight `common_saving` → `Saving…` | catalogue |
| inherited state | key `settings_source_protect_inherits_prefix` → `Inherits global default — ` prefixed onto `settings_source_protect_hint_protected` / `_unprotected` | `:50-58` |
| load failure | key `settings_protection_load_error` → `Could not load protection` | catalogue |
| save failure | key `common_save_failed` → `Could not save.` | catalogue |

### Selectors that could not be verified in source

- Language cards (S1a): `StepLanguage.tsx` was NOT read by this unit. Its
  `aria-pressed` behaviour is asserted by the journey and by spec-061 FR-004/5,
  and `StepLanguage.test.tsx` exists, but no selector is stated here rather
  than guessed. **The driving phase must read `StepLanguage.tsx` first, or read
  it from the DOM.**
- Observing Site fields (S5): `StepSite.tsx` was not read; `siteStepHasSite` /
  `siteStepError` are the named validation helpers. Field-level selectors
  unverified.
- Configuration step controls (S4): `StepCatalogs.tsx` was not read. The
  2026-08-24 run observed the three selects by content
  (`Compact/Comfortable/Spacious`, `Protected/Unprotected`); treat that as the
  starting point, not a verified selector.
- Confirm step (S6): `StepConfirm.tsx` was not read; no testid list.
- Audit Log rows (E9.4, E10.4, E11.3, E12.2, E13.4): `AuditLog.tsx` was not
  read. No selector verified.
- Category tooltip (E2.2) and the per-row org-state tooltip: rendered by
  `InfoTip` with no testid; the accessible-name shape is unverified.

---

## 3. Fixture recipe

MOCK DATA ONLY. Do not copy, read, or reference `D:\Astrophotography` or any
real library. The 2026-08-24 run copied nine real frames out of the owner's
library; `astro-plan-ldj0v` exists to purge those. Do not repeat that.

### Reused from `astro-plan-mg6h8`

Taken unchanged from that bead's dispatch and its own recorded environment:

- Windows host `<journey-host>`, checkout `C:\dev\astro-plan`, node v24.19.0,
  `pnpm install` with `CI=true` (fails with NO_TTY otherwise), Vite on
  `127.0.0.1:5173` with mocks OFF.
- `desktop_shell.exe` built `--features dev-tools,e2e`; bridge on
  `127.0.0.1:9223`; MCP client `@hypothesi/tauri-mcp-server@0.11.2` pinned to
  match `tauri-plugin-mcp-bridge` 0.11.2 (0.12.0 fails silently at connect).
- Launch via `scripts/win-native-dev.ps1 -McpBridge` under
  `schtasks /RU <journey-host>\<user> /IT` so the window renders on console session 1.
- A disposable root only; never the owner's library, never a path under the
  repo, never a path you did not create.
- `withGlobalTauri` sanity check: `execute_js('1+1')` must return `2`.
- MCP servers bind at session start — a session older than the `.mcp.json`
  entry will not see the `tauri` tools. Start a fresh session, or drive the
  bridge's own WebSocket protocol (`{id,command,args}` →
  `{id,success,data,error}`, commands `execute_js` / `get_window_info`).

### Added for J01

**J01-unique throwaway root: `C:\jv-j01\`.** Distinct from the
`C:\jv-throwaway\` the 2026-08-24 run used and from anything another unit may
hold. Isolation env, same shape as that run:

```
PV_DATA_DIR=C:\jv-j01\appdata
PV_DB_URL=sqlite://C:\jv-j01\app.db?mode=rwc
```

**Frames: generate, never copy.** `scripts/gen-mock-fits.py` already produces
17 valid mock FITS files with realistic headers — INSTRUME, EXPTIME, DATE-OBS,
IMAGETYP, XBINNING, CCD-TEMP, GAIN, real dimensions, and `STACKCNT`/`NCOMBINE`
on the intended masters. Verified by running it into a scratch directory:

```
python3 scripts/gen-mock-fits.py --output-dir /tmp/j01-genprobe   # exit 0
# "Done — 17 mock FITS files generated."
```

Its default output is the tracked `tests/fixtures/mock-fits-library/` (17
tracked files, `git ls-files`), and it **wipes the output directory on each
run** — so ALWAYS pass `--output-dir`, never let it default, or it rewrites
tracked fixtures.

Generated layout (per-frame-type, per-tool):

```
bias/poseidon-nina/            dark/zwo-nina/
flat/poseidon-nina/            flat/zwo-nina/
light/dwarf3-dwarflab/         light/poseidon-nina/   (+ further light dirs)
master/dark/wbpp-poseidon/     master/flat/nina-poseidon/
master/light/asideepstack-zwo/ master/light/pixinsight-graXpert/
```

Notable coverage already present: a DWARF III light that deliberately OMITS
`IMAGETYP` (real raw behaviour) — that is the unclassified case E7.4 needs — and
a stripped master with no `IMAGETYP`/`EXPTIME`/`GAIN`/`CCD-TEMP`/`INSTRUME`.

**Root layout to build.** Four registered roots, one per category, none nested
in another (nesting is rejected — E2.8):

```
C:\jv-j01\lights\          light_frames, organized    <- copy light/**
C:\jv-j01\calibration\     calibration,  unorganized  <- copy bias/ dark/ flat/ master/
C:\jv-j01\projects\        project,      organized    <- empty (covers the "0 items" half of E7.1)
C:\jv-j01\inbox\           inbox,        unorganized  <- copy 2 light files
```

Plus, NOT registered, for the deferred-rejection Expects:

```
C:\jv-j01\lights\nested\   -> add attempt must be rejected (E2.8)
C:\jv-j01\does-not-exist\  -> for E2.9/E2.10/E10.6
```

**Damaged-header fixtures for E7.8 (Δ9) — new, no generator exists.** Both must
carry a real extension and a header that cannot be parsed:

```powershell
# a .fits whose header has no recognised keyword (PR #1737's exact case)
$b = [byte[]](0..2879 | ForEach-Object { 0x20 })
[System.IO.File]::WriteAllBytes('C:\jv-j01\lights\broken\not-really-fits.fits', $b)
# a non-astronomical file merely carrying the extension
Set-Content -Path 'C:\jv-j01\lights\broken\text-masquerading.fit' -Value 'this is not a FITS file'
# an .xisf with invalid XML
Set-Content -Path 'C:\jv-j01\lights\broken\broken.xisf' -Value 'XISF0100<not-xml'
```

Expected outcome per `crates/metadata/fits/src/lib.rs:83,98-106` and
`crates/metadata/xisf/src/lib.rs:60`: a `MetadataExtractError::Parse` per file,
the scan continues, and the Scan step shows
`setup_scan_classify_failed*`. **Multi-byte trap for the PR #1733 case:** a
card containing a multi-byte UTF-8 character is what used to panic; add one
(e.g. an `OBJECT` card with a non-ASCII name) if you want to exercise #1733
rather than only #1737.

**Remap fixture (S10).** Register `C:\jv-j01\lights`, scan it so `file_record`
rows exist, then:

```powershell
Rename-Item C:\jv-j01\lights C:\jv-j01\lights-moved
```

`online` is computed as `Path::new(&s.path).exists()` at
`apps/desktop/src-tauri/src/commands/roots.rs:68`, so the card flips to
`offline` on the next `roots.list`. Remap to `C:\jv-j01\lights-moved` and
Verify should report all items found. For the partial case, delete one file
under the new path first. For E10.3 (zero recorded items), remap the empty
`projects` root instead.

**Absent-drive fixture (S12 retire-an-offline-source).** Do not touch physical
hardware. Use a virtual drive letter:

```powershell
subst X: C:\jv-j01\removable      # register X:\ as a source, scan it
subst X: /D                       # "drive removed" -> root goes offline
```

Re-attaching is `subst X: C:\jv-j01\removable` again. Note the case-folding
Expect (PR #911): `X:\Foo` vs `x:\foo` must be caught as a duplicate on
Windows.

**Dependent-records fixture (E12.4/E12.6).** Delete must be blocked while
sessions/projects reference the root. Scanning the lights root produces
acquisition sessions, so the lights root is the dependent case and the empty
`projects` root is the clean case. Which of `has_dependents` the backend
actually reports for each is UNVERIFIED by this unit — check
`roots.list`'s `has_dependents` before choosing.

---

## 4. Precondition and teardown

### Starting state

- P1: empty database. Reset by removing the DB the app is actually pointed at.
  The 2026-08-24 run used `PV_DB_URL=sqlite://C:\jv-j01\app.db?mode=rwc`, so
  delete `C:\jv-j01\app.db*` and `C:\jv-j01\appdata\`. **Clearing
  `localStorage` is NOT a reset** — it produces a `/`↔`/setup` redirect loop
  (`docs/development/windows-journeys/journey-01-first-run-setup.md:43-46`,
  otherwise stale — see §5).
- The alternative first-run entry is Settings → Advanced → Restart first-run
  setup (confirm-gated); use it for E1.2/E1.7/E1.8, which need a pre-filled
  buffer and therefore a NON-empty DB.
- P2 (S9–S14): setup already complete with at least one registered source.
  Reach it by completing S1–S8 in the same run.
- Recompile trap: `git reset --hard` leaves old mtimes, so cargo skips the
  rebuild and the app silently serves the old binary (symptom: a command that
  is in the code returns "not found"). Touch changed `.rs` files before
  relaunching.

### Order

Run S1–S8 once on the fresh DB, then S9–S14 against the roots it left. Do
E8.2 (quit/relaunch persistence) immediately after S8 — it is the cheapest
never-validated Expect. Do the restart-first-run Expects (E1.2, E1.7, E1.8)
AFTER S9–S14, since they re-enter the wizard.

### Teardown

- `subst X: /D` if the virtual drive is still mapped.
- Remove `C:\jv-j01\` entirely — every path under it was created by this run.
- Leave `C:\jv-throwaway\` alone; it is another unit's.
- Do not delete anything you did not create. If a step would, STOP.
- Report the exact root used, as the shared-host contract requires.

---

## 5. Known-gap list — do not re-file these

Every GitHub issue historically filed against J01 is CLOSED, verified
2026-08-25 by `gh issue view <n> --json state` (exit 0 each): #704, #707, #501,
#502, #512, #515, #557, #559, #560, #646, #662, #916, #1139. So the live known
gaps are the four beads from the 2026-08-24 run, not the PR-era issues.

| Expect | Known outcome | Recorded as |
|---|---|---|
| E1.1 (7 steps) | product has **8** steps — an undocumented `2. Theme` step, added by PR #1048 (`ea22487eb`, spec 056), missed by the PR #1747 amendment. Document-wrong. | `astro-plan-ko0tv` (P3) |
| E4.2 (protected/normal/unprotected) | only two levels exist; `crates/contracts/core/src/protection.rs:24` — the `normal` tier was retired for #506, existing rows remapped by migration 0070. Document-wrong. | `astro-plan-jklcq` (P3) |
| E4.3, E4.4, E4.6 (Theme control on Configuration) | there is **no theme control on that step**; Theme moved to the new step 2 and is grouped by family (`WARM`/`COOL`), so Δ7's recorded unfiltered-select inconsistency no longer describes the product. Document-wrong. | `astro-plan-jklcq` (P3) |
| E6.1 scan-depth clause | no scan-depth line on Confirm; the string `Recursive` does not appear on the step. `sources-store.ts:279-281` still sends `scanDepth: 'recursive'` with a comment that the UI plumbing was retired (#509 / PR #908). Document-wrong. | `astro-plan-jklcq` (P3) |
| E2.5 (empty path rejected inline with an accessible error) | the add button is `disabled` while the input is empty, so no attempt and no `role=alert` is possible. The user-facing outcome holds; the asserted mechanism does not exist. Document-wrong. | `astro-plan-jklcq` (P3) |
| E7.4 (counts reconcile) | **PRODUCT DEFECT, reproduced.** The Scan step reported `Nothing detected in this folder.` for roots whose frames the backend had discovered (6 `inbox_source_groups` covering 8 light frames), and totalled 2 instead of 6. Those frames reached no user-visible surface. | `astro-plan-1xlik` (P2) |
| S2 selector expectations in the Windows doc | `docs/development/windows-native-rust-dev.md` names `e2e-path-input-*` stand-ins that do not exist; the real controls are `manual-add-by-path-*` / `manual-path-input-*` / `manual-add-path-btn-*`. Document-wrong. | `astro-plan-npts7` (P3) |

Also recorded UNVALIDATED (not failures) by the 2026-08-24 run, and still
outstanding: E3.3, E3.4, E3.5 (behind the native OS dialog), E7.8 (Δ9 —
fixtures never existed; §3 now supplies them), E8.2 (quit/relaunch).

**S9–S14 have never been driven at all.** The 2026-08-24 run skipped them and
the 2026-07-14 run predates the kebab consolidation (PR #894). Everything in
S9–S14 is genuinely open.

### The empty-plan gap does NOT belong to J01

The recorded gap "no explanatory text for an empty plan" is **J06's**, not
J01's: `docs/journeys/J06-cleanup-scan-review-apply/journey.md:107` states the
overlay shows no explanatory text with only the disabled control, and the
2026-07-14 run logs it twice as a duplicate of issue **#603** ("Archive flow
dead-ends on an unexplained empty plan", now CLOSED) at
`docs/development/journey-run-2026-07-14.md:193,296`. J01's own `## Known gaps`
section is an empty comment placeholder. Do not carry it into J01.

Command used: `rg -n -i "explanatory text" docs/journeys/ docs/development/`
(exit 0, 3 hits, none in J01).

### Trace-field status

Every path in J01's `trace:` still resolves. The three `deltas/` files under
`docs/product/journeys/J01-.../` all carry a **MIGRATED** banner declaring
themselves frozen legacy history superseded by the current journey doc — do not
validate against them. Their substance (6-step wizard, `project` required) is
already folded into v9.

---

## 6. Static-evidence findings

All STATIC ONLY, UNVERIFIED AGAINST RUNNING APP.

**F1 — E2.9's premise is contradicted by source.** The journey says a path that
doesn't exist "is accepted into the working buffer at add time (there is no
client-side check for these)". But `handleAdd` calls `checkPathExists` and
returns early with `setAddError(notFound)` before `onAdd`
(`apps/desktop/src/features/setup/steps/StepSourceFolders.tsx:231-237`). If
`checkPathExists` really checks existence, a nonexistent path IS rejected
inline, and E2.10/E2.11's whole deferred-to-S6 story is wrong for the
existence case. The overlap case is separate (`findAddTimeConflict`). This is
either a document defect or a behaviour change since v9; the driving phase
settles it by typing a nonexistent path into `manual-path-input-light_frames`.
STATIC ONLY.

**F2 — S9 has no failure signal and no count-delta control.** `handleRescan`
swallows the error into `console.error` only
(`apps/desktop/src/features/settings/useDataSources.ts:98-100`); no toast, no
banner, no card state. The only started→finished signal is the menu item
relabelling to `Rescanning…` (`RootCard.tsx:162`), and the only count surface
is the card meta line recomputed from `root.fileCount` after
`invalidateRoots` — there is no delta indicator. The journeys' cross-cutting
rule requires each mutating step to name BOTH a success and a failure signal;
a failed rescan is invisible to the user. STATIC ONLY.

**F3 — S13's "remove the override" has no UI control.** `SourceProtectionOverride`
exposes only a two-value select and `Save override`; the retired third level
means "absence of an override" is no longer representable in the control
(`SourceProtectionOverride.tsx:39-48`), and no clear/remove/inherit action
exists in the component. The only reset path in the pane is the shared
`RestoreDefaultsBtn scope="sources"` (`DataSources.tsx:118-124`), which is not
per-source. E13.3 and the remove half of E13.4 look unexecutable through the
UI. STATIC ONLY.

**F4 — a failed Disable is invisible.** `useDataSources.ts:162-170` carries an
in-code admission: `toggleActiveError` is computed and passed to the confirm
modal, but `handleConfirmDisable` closes the dialog unconditionally
(`:181-185`), "so it is never actually shown". E11.3's before→after audit row
may exist while the user sees nothing on failure. STATIC ONLY.

**F5 — an offline source cannot be disabled or re-enabled, and cannot be
rescanned.** The Disable/Enable menu item is rendered only when `!isOffline`
(`RootCard.tsx:199`), as is Rescan (`:151`) and Reconcile (`:165`). So a
source that is offline because its drive is absent cannot be re-enabled once
disabled, which matters directly to J01's "retire an offline source" arc.
Delete/Remove IS always rendered (`:231`), so E12.3 holds in source. STATIC
ONLY.

**F6 — Apply remap is enabled by ANY verification, including a failed one.**
`disabled={!verification || verifying || applying}`
(`RemapRootDialog.tsx:129`) — a Verify that returns `allVerified: false` still
enables Apply, which then sends `verified: false` and is refused server-side.
So E10.7's "Apply is not clickable before a successful Verify" is satisfied
only in the never-verified case; after a partial Verify the button is
clickable and the refusal comes from the backend as an error banner. Not
necessarily a defect — the server gate is the real one — but the driving phase
should expect a clickable Apply plus `Could not remap: …`, not a disabled
button. STATIC ONLY.

**F7 — S10's server-side gate holds in source.** `ensure_remap_verified`
recomputes verification itself and writes a `Refused` audit row with
`remap.not_verified` before returning `ErrorCode::RemapNotVerified`
(`crates/app/core/src/first_run/root_remap.rs:102-148`). E10.9 and E10.10 are
implemented as described; drive them by a direct
`window.__TAURI__.core.invoke('roots_remap_apply', {..., verified: true})`
against a path whose items do not match. STATIC ONLY.

**F8 — the Windows validation doc for J01 is comprehensively stale and must
not be used for selectors or expectations.**
`docs/development/windows-journeys/journey-01-first-run-setup.md` states a
5-step wizard (`:13`, `:86`), no Language and no Theme step, "Disable is
reversible, no confirm needed" (`:152-160`, contradicting E11.4 and
`DataSources.tsx:248`), Delete only for offline sources (`:162-166`,
contradicting E12.3 and PR #894/#559), Remap "samples files" (`:146`,
contradicting the exhaustive count of PR #893/#560), and the nonexistent
`e2e-path-input-*` stand-ins (`:70-74`, already filed as `astro-plan-npts7`).
Its still-valid content is the environment mechanics only: deploy/reset,
the recompile trap, and the localStorage-is-not-a-reset warning. The bead's
instruction to reuse it for click sequences cannot be honoured — the selector
map in §2 was derived from source instead. STATIC ONLY.

**F9 — `online` is pure path existence.** `let online =
std::path::Path::new(&s.path).exists()`
(`apps/desktop/src-tauri/src/commands/roots.rs:68`). No caching, no drive
enumeration — which is what makes the rename and `subst /D` fixtures in §3
valid simulations of a moved and an absent root. STATIC ONLY.

---

## Commands run, with exit codes

| Command | Exit |
|---|---|
| `bd show astro-plan-r47fv` (+ `--json`) | 0 |
| `bd update astro-plan-r47fv --claim` (actor `r47fv-offline`) | 0 |
| `bd show astro-plan-mg6h8`, `bd comments astro-plan-mg6h8` | 0 |
| `ls docs/journeys/`, `ls -R docs/journeys/J01-.../`, `ls docs/development/windows-journeys/` | 0 |
| `rg -n -i "explanatory text" docs/journeys/ docs/development/` | 0 (3 hits) |
| `rg -n "Journey 1\|journey-01\|J01" docs/development/journey-run-2026-07-14.md` | 0 |
| `gh issue view <n>` × 14 | 0 each |
| `git log --oneline -5 --all -- 'docs/journeys/*/runs/**'` | 0 |
| `git show 5f39618cf` (J01 run record, 258 lines) | 0 |
| `rg -n 'data-testid' apps/desktop/src/features/setup/` | 0 |
| `rg -n 'data-testid\|role="menu\|aria-label' .../DataSources.tsx .../RemapRootDialog.tsx` | 0 |
| `git ls-files tests/fixtures/mock-fits-library` (17 files) | 0 |
| `python3 scripts/gen-mock-fits.py --output-dir /tmp/j01-genprobe` | 0 (17 files) |
| `git status --porcelain tests/fixtures` | 0 (empty — nothing tracked was touched) |
| `grep -rn "online:" --include=*.rs crates/` | 0 |
| `grep -n "online" apps/desktop/src-tauri/src/commands/roots.rs` | 0 |
| `grep -n "not_verified..." crates/app/core/src/first_run/root_remap.rs` | 0 |

Search-hygiene note: the `online` census initially returned near-nothing
because the token-savings wrapper compresses the literal `online` out of
displayed output. The figure above comes from a plain `grep` re-run and a
direct read of `crates/contracts/core/src/roots.rs:35`, not from the first
result.
