# J04 — Sessions: review derived. Offline drive plan

Prepared 2026-08-25 from source at `HEAD` of branch `chore/journey-validation-formula`
(`git rev-parse HEAD` = d583a7e81). Windows host untouched. Every conclusion below
is **STATIC ONLY, UNVERIFIED AGAINST RUNNING APP** unless it cites a prior run record.

Journey document: `docs/journeys/J04-sessions-review-derived/journey.md` (version 7,
`last_reviewed: 2026-07-14`). It has **no `deltas/` directory**; the six Δ entries live
inline in its Delta log. The `trace:` field cites six `docs/product/journeys/J04-.../deltas/*.md`
paths — that legacy tree still exists (`docs/product/journeys/J04-sessions-review-derived/deltas/`)
and is a pre-migration duplicate, not live evidence.

## Census

| Item | Count | Command |
| --- | --- | --- |
| Steps (S1–S7) | 7 | `rg -c '^### S' journey.md` |
| Expects (total) | 15 | `rg -c '^- \*\*Expect' journey.md` |
| Expects (negative) | 7 | `rg -c '^- \*\*Expect \(negative\)' journey.md` |
| Success criteria | 7 | `rg -c '^- SC[0-9]' journey.md` |
| Known gaps | 4 | `rg -c '^- G[0-9]' journey.md` |
| Delta-log entries | 6 | `rg -c '^- \*\*Δ' journey.md` |
| Windows-journeys doc Tests | 5 | `docs/development/windows-journeys/journey-04-sessions-review.md` |

All commands exit 0.

---

## 1. Expectation inventory

Signal taxonomy per the cross-cutting rule: `toast` / `navigation` / `visible-state` /
`refusal-reason` / `per-item-error`. Sessions is a **read-only derived surface** — only
three steps mutate anything (S4 notes autosave, S4 calibration unassign, S6 reveal), so
most Expects settle on `visible-state`.

### S1 — Sessions empty before confirm

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E1.1 | List shows no rows for that data | `[data-testid="sessions-list"] .pv-listtable__empty` contains `No sessions match the current filters.` and `[data-rowkind="sessions-table-row"]` count is 0 | visible-state |
| E1.2 | (neg) Raw unreviewed scan results never appear as sessions | same as E1.1, taken **after** an Inbox scan has run but before confirm | visible-state |

> Trap for E1.1: the empty branch is gated on `loading` (`SessionsTable.tsx:358-363`).
> A cold load renders a `Skeleton` (`aria-label` = `Loading…`), not the empty string.
> Assert the empty text, not "zero rows", or a still-loading table reads as a pass.

### S2 — Sessions appear automatically after apply

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E2.1 | Session row(s) appear | `[data-testid^="sessions-row-"]` count > 0 | visible-state |
| E2.2 | No "review this session" action | no button in `[data-testid="sessions-page"]` whose accessible name matches the review vocabulary (see §2 SC3 recipe) | visible-state |
| E2.3 | Frame counts match what the plan moved/catalogued | per-row `Frames` cell (4th `[role="cell"]`… see §2 cell-index map) | visible-state |
| E2.4 | Catalogued session shows real target/filter/binning/gain/night from the pipe-delimited key | Target cell text ≠ `Session — <date> · <disc>`; `Night` cell = ISO date | visible-state |
| E2.5 | Target cell falls back to the session's own name when target absent | Target cell text | visible-state |
| E2.6 | (neg) No Confirm/Re-open/Reject/Ignore control and no review-state pill anywhere on the page | DOM sweep, §2 | visible-state |
| E2.7 | (neg) One invalid-JSON `frame_ids` session does not empty the list; intact rows keep their counts; the corrupt one shows 0 | row count + per-row `Frames` cells | visible-state |

> **E2.5 is stale as written.** The doc says the fallback is `session.target ?? session.name`.
> Source uses `sessionDisplayName()` (`apps/desktop/src/features/sessions/displayName.ts:31-42`),
> which appends a discriminator: the frame-folder basename, else `<frames>f · <id[0:8]>`.
> Expect `Session — 2026-05-03 · BLUE` (or `· 4f · a1b2c3d4`), **not** a bare
> `Session — 2026-05-03`. See §6/F1.
>
> **E2.7 is not reachable by UI action.** Corrupting `frame_ids` requires a direct
> SQLite `UPDATE`. See §3 for the one place where that is legitimate (it corrupts an
> *input*, not the derived output).

### S3 — Filter, group, sort

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E3.1 | Dropdown options come from the full unfiltered set | pick a value in `#filterbar-filter`, then re-read `#filterbar-camera` option count — unchanged | visible-state |
| E3.2 | Active sort column exposes `aria-sort` | `th[aria-sort]` on the clicked column, value `ascending`/`descending` | visible-state |
| E3.3 | "Grouped by X" hint under the list while grouping is active | `[data-testid="sessions-grouping-hint"]` text `Grouped by <label>` | visible-state |
| E3.4 | (neg) There is no frame-type filter | — **CONTRADICTED BY SOURCE, see §6/F2** | — |
| E3.5 | (neg) No secondary/multi-column sort control | — **CONTRADICTED IN PART, see §6/F3** | — |

### S4 — Open a session's detail

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E4.1 | Full-height drag-resizable side panel on a wide window | `[data-testid="listpage-detail"][data-dock="side"]` present, plus a `[aria-label="Resize detail panel"]` handle | visible-state |
| E4.2 | Bottom dock when narrow | `[data-testid="listpage-detail"][data-dock="bottom"]` | visible-state |
| E4.3 | Chosen side width persists across restart | `localStorage` entry keyed by `dockId` `"sessions"` (see §2), then re-read the computed `--pv-side-detail-w` | visible-state |
| E4.4 | Per-page pin overrides automatic placement | `[role="radiogroup"]` with `[role="radio"][aria-label="Right"]`; `aria-checked="true"` after click; `data-dock` follows | visible-state |
| E4.5 | Detail shows target, filter, frame count, exposure, total integration, night, camera, gain, binning, sensor temperature, confirmed-by | `[data-testid="property-table"]` rows, matched by `[role="rowheader"]` label text (§2) | visible-state |
| E4.6 | Total integration rendered as `1h 30m` | Integration row value | visible-state |
| E4.7 | Source badge (FITS/User/Inferred/Default) only when a real value is present | source cell text ∈ {`FITS`,`User`,`Inferred`,`Default`} | visible-state |
| E4.8 | An applicable-but-empty field renders an `Unresolved` chip, never a bare em dash and never a source badge | `[data-testid="unresolved-chip"]` inside that row's value cell | visible-state |
| E4.9 | A non-applicable field renders a blank em dash with no chip | value cell text `—`, no chip | visible-state |
| E4.10 | Read-only Calibration section listing linked matches | `[data-testid="session-calib-list"]` | visible-state |
| E4.11 | Explicit "no calibration match" empty state | `[data-testid="session-calib-empty"]`, text `No calibration match` | visible-state |
| E4.12 | Notes section autosaves on a debounced pause | `[data-testid="session-notes-saved"]`, text `Saved` | visible-state |
| E4.13 | Notes rejects input past 16 KiB | `[data-testid="session-notes-error"]` (`role="alert"`), text `Note exceeds the 16,384-byte limit.` | per-item-error |
| E4.14 | Notes persists across navigating away and back | textarea `value` after re-select | visible-state |
| E4.15 | (neg) Closing via ✕ or Escape never mutates the session or triggers a lifecycle transition | detail gone (`[data-testid="listpage-detail"]` absent); no toast; `sessions-row-<id>` Frames/Night cells unchanged | visible-state |
| E4.16 | Escape closes the panel even when focus stayed on `<body>` | as E4.15, with `document.activeElement === document.body` before the key | visible-state |
| E4.17 | An open nested dialog consumes Escape first | open `[data-testid="session-calib-unassign-confirm"]`, press Escape → dialog closes, detail stays | visible-state |

> **E4.5/E4.6/E4.8 have a hard static blocker.** `InventorySession.exposure` and
> `set_temp` are hardcoded `None` in the backend projection
> (`crates/app/core/src/inventory.rs:381` and `:399`). See §6/F4 — this is the single
> most important thing your drive run should confirm, because it determines whether
> E4.6 is testable at all.
>
> E4.12 timing: the debounce is **5000 ms** (`apps/desktop/src/lib/notes.ts:15`,
> `NOTE_DEBOUNCE_MS = 5_000`). Do not conclude "autosave broken" before 6 s of
> idle. The doc says "a debounced pause" without a number.

### S5 — Follow a linked project from session detail

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E5.1 | Navigation lands on the Projects list | URL hash `#/projects` | navigation |
| E5.2 | The id is discarded, no project pre-selected | — **STALE, CONTRADICTED BY SOURCE, see §6/F5** | navigation |

### S6 — Reveal the session's source root

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E6.1 | OS file browser opens to root path + `relativePath` | not observable in the webview — see §2 "unmappable" | — |
| E6.2 | Distinct sessions under one root reveal distinct folders | not observable in the webview | — |
| E6.3 | A session with no `relativePath` falls back to the root path | not observable in the webview | — |
| E6.4 | If the reveal call fails, an error toast shows and the panel stays open | `div[role="alert"].pv-toast__item--error` containing `Could not reveal the location.`, plus `[data-testid="listpage-detail"]` still present | toast |
| E6.5 | (neg) The reveal action is not offered when no source path resolves | no button whose text is `Show in File Explorer` inside the detail | visible-state |

### S7 — Rescan the Inbox without disturbing Sessions

| # | Expect | Observable | Signal |
| --- | --- | --- | --- |
| E7.1 | Session count and identities stay the same | snapshot the set of `data-testid="sessions-row-<id>"` values before and after; compare as sets | visible-state |
| E7.2 | (neg) Rescan never duplicates a session | row-count equality (same snapshot) | visible-state |
| E7.3 | (neg) Rescan never reintroduces a review state | the SC3 sweep, re-run | visible-state |

### Expects I could NOT map to an observable

1. **E6.1, E6.2, E6.3** — the reveal *target path* is computed in the frontend
   (`resolveRevealPath`, re-exported from `@/lib/path` via
   `apps/desktop/src/features/sessions/revealInventory.ts:12`) and then handed to the
   `nativeReveal` IPC command. Nothing renders the path. The webview cannot see which
   Explorer window opened. Settle these by reading the `audit_log_entry` row the
   command writes (`entityKind: 'inventory_row'`, `entityId` = session id;
   `revealInventory.ts:16-23`) out of the SQLite DB, **not** from the DOM.
   *This is a FINDING about the journey doc: three Expects in S6 have no UI observable.*
2. **E4.3 (persistence across restart)** — mappable only across an app restart, which
   is a separate drive segment. `useAdaptiveDock` persists per `dockId`
   (`apps/desktop/src/ui/useAdaptiveDock.ts:68,107`); `SessionsPage.tsx:316` passes
   `dockId="sessions"`. The exact storage key string is not literal in the hook (it
   reads/writes a keyed record); resolve it at drive time via
   `Object.keys(localStorage)` rather than guessing. **Not verified in source.**
3. **E2.3 "match what the plan actually moved/catalogued"** — the plan's own action
   count is on the Inbox/Plans surface, not on Sessions. This Expect requires
   cross-surface arithmetic; the plan totals must be captured during the J02/J03
   precondition, before Sessions is opened. Record the plan/action counts then.
4. **E2.7** — reachable only by direct DB corruption; no UI path exists. See §3.
5. **E3.4, E3.5, E5.2** — unmappable because the doc's assertion is contradicted by
   source (see §6). Do not drive them as written.

---

## 2. Selector map

Every entry below was read from the cited source line. Where no `data-testid` exists I
give the role/accessible name or the i18n key with its resolved `en-GB` value from
`apps/desktop/messages/en-GB.json`.

### Page and list

| Observable | Selector | Source |
| --- | --- | --- |
| Sessions page root | `[data-testid="sessions-page"]` | `SessionsPage.tsx:317` |
| Layout body (dock state) | `[data-testid="listpage-body"][data-dock="side|bottom"]` | `ListPageLayout.tsx:318-319` |
| Primary content column | `[data-testid="listpage-main"]` | `ListPageLayout.tsx:321` |
| List container | `[data-testid="sessions-list"]` | `SessionsTable.tsx:357` |
| Empty state | `.pv-listtable__empty` — text `No sessions match the current filters.` (key `sessions_no_match`) | `SessionsTable.tsx:363` |
| Loading skeleton | `[aria-label="Loading…"]` (key `common_loading`) | `SessionsTable.tsx:360` |
| Load-error state | `.pv-listtable__empty` — text `Could not load sessions.` (key `sessions_load_error`) | `SessionsPage.tsx:323` |
| Session row | `[data-testid="sessions-row-<sessionId>"]`, also `[data-rowkind="sessions-table-row"]` | `SessionsTable.tsx:305-306` |
| Selected row | row class contains `pv-sessions-table__row--selected`, and `_selected` sets the row's selected attribute | `SessionsTable.tsx:310-313` |
| Group header | `[data-testid="sessions-group-<dimension>-<key>"]`, `aria-expanded` | `SessionsTable.tsx:280-281` |
| Grouping hint footer | `[data-testid="sessions-grouping-hint"]` — text `Grouped by {dims}` (key `sessions_grouping_hint`), levels joined by ` › ` | `SessionsTable.tsx:351-352, 377-378` |
| Virtual scroll viewport | `[data-testid="sessions-virtual-sizer"]` | `SessionsTable.tsx:372` |
| Row connectivity pill | `[data-testid="sessions-row-connectivity-<sessionId>"]` | `SessionsTable.tsx:322` |

**Column cell order** (`SessionsTable.tsx:136-173`), for reading a row's cells by index:
`0 Target · 1 Filter · 2 Frames · 3 Integration · 4 Night · 5 Camera · 6 Projects`.
Header labels resolve to `Target` / `Filter` / `Frames` / `Integration` / `Night` /
`Camera` / `Projects` (keys `projects_create_target_label`, `common_filter`,
`projects_wizard_col_frames`, `projects_wizard_col_integration`, `sessions_col_night`,
`settings_calmatch_camera`, `common_projects`).

**Virtualization trap — CONFIRMED.** The table renders in `virtualized` mode
(`SessionsTable.tsx:369`) using `@tanstack/react-virtual`
(`apps/desktop/src/ui/Table.tsx:5,183`). Off-screen rows are **absent from the DOM**.
A row-count assertion for E7.1/E7.2 must either scroll the viewport to the end or
read the count from `sessions.list` IPC instead. Mitigating note read at
`Table.tsx:150-151`: with no measured viewport the virtualizer yields zero items and
every row renders without spacers — so in a headless/zero-height case the full set may
appear. Treat neither behaviour as guaranteed; scroll and re-read.

### Toolbar

| Observable | Selector | Source |
| --- | --- | --- |
| Search box | `input[aria-label="Search sessions"]` (key `sessions_search_aria`); placeholder `Search target, filter, camera…` | `SessionsPage.tsx:251-252` |
| Type filter | `select#filterbar-kind`, label `Type` (key `sessions_kind_filter_label`); options `Acquisition` / `Dark` / `Flat` / `Bias` / `All` | `SessionsPage.tsx:256-267`, `FilterToolbar.tsx:298` |
| Filter filter | `select#filterbar-filter`, label `Filter` | `SessionsPage.tsx:269-275` |
| Camera filter | `select#filterbar-camera`, label `Camera` | `SessionsPage.tsx:276-283` |
| "All" option | option text `All` (key `common_all`) | `FilterToolbar.tsx:191` |
| Group-by slot 1 | `select[aria-label="Group by"]` (key `inbox_group_by_aria`); none-option `Group: none`; value options `Group: Target` etc. | `FilterToolbar.tsx:139-157` |
| Group-by slot 2/3 | `select[aria-label="Then group by (level 2)"]` / `(level 3)` (key `inbox_group_by_level_aria`); none-option `then: —` | `FilterToolbar.tsx:142, 148` |
| Group-by option values | `target` / `filter` / `night` / `camera` / `month` | `SessionsPage.tsx:125`, `SessionsTable.tsx:82-88` |
| Sort header button | `th[aria-sort]` on the column; the button inside has `aria-label="Sort by <col>"` (key `common_sort_by_aria`) | `SessionsTable.tsx:253-261` |

> The Group-by control is **three `<select>` slots**, not one — `maxLevels` defaults to
> 3 (`FilterToolbar.tsx:125,128`). Slots 2 and 3 are `disabled` until the previous slot
> has a value (`FilterToolbar.tsx:130`).

### Detail panel

| Observable | Selector | Source |
| --- | --- | --- |
| Detail section | `[data-testid="listpage-detail"][data-dock="side|bottom"]`, `role="complementary"`, `aria-label="Close session details"` (key `cmp_listpage_close_session_details_aria`, passed as `detailLabel`) | `ListPageLayout.tsx:328-333`, `SessionsPage.tsx:311,320` |
| Detail content wrapper | `[data-testid="detailpanel-content"]` | `DetailPanel.tsx:141` |
| Close ✕ | `button[aria-label="Close details"]` (key `inbox_close_details_aria`), text `✕` | `ListPageLayout.tsx:354-360` |
| Side resize handle | `[aria-label="Resize detail panel"]` (key `list_page_layout_dock_resize_aria`) | `ListPageLayout.tsx:336-338` |
| Dock placement control | `[role="radiogroup"]` containing `[role="radio"]` with `aria-label` `Auto` / `Bottom` / `Right`; active one has `aria-checked="true"` | `DetailDockPlacementControl.tsx` options; `SegControl.tsx:67,74-75,89` |
| Detail title | `[data-testid="detailpanel-content"] strong` — `sessionDisplayName(session)` | `SessionDetail.tsx:346` |
| Equipment subtitle | camera · `g<gain>` · setTemp · binning, ` · `-joined | `SessionDetail.tsx:184-191` |
| Detail connectivity pill | `[data-testid="session-detail-connectivity"]` | `SessionDetail.tsx:328` |
| Reveal button | `button` with text `Show in File Explorer` on Windows (key `reveal_label_windows`); `title` = `Open the session's source location in the OS file browser` (key `sessions_reveal_title`) | `SessionDetail.tsx:336`, `apps/desktop/src/lib/reveal-label.ts` |
| Property table | `[data-testid="property-table"]`, `role="table"`, `aria-label="Properties"` (key `cmp_property_table_aria`); two instances (colA/colB) | `PropertyTable.tsx:149-153`, `SessionDetail.tsx:353-354` |
| A property row | `[role="row"]` whose `[role="rowheader"]` text equals the label | `PropertyTable.tsx:186-189` |
| A property value | that row's `[data-testid="property-table-cell-value"]` (`role="cell"`) | `PropertyTable.tsx:191-196` |
| Unresolved chip | `[data-testid="unresolved-chip"]` — text `Unresolved` (key `cmp_unresolved_chip`) | `RenderValue.tsx:92-97` |
| Not-applicable marker | plain text `—` in the value cell, no chip | `RenderValue.tsx:117` |
| Source badge | **no testid** — read the source cell's text: `FITS` / `User` / `Inferred` / `Default` (keys `cmp_source_fits` / `_user` / `_inferred` / `_default`) | `RenderValue.tsx:55-84` |
| Linked-projects heading | text `Linked projects` (key `sessions_linked_projects_heading`) | `SessionDetail.tsx:357` |
| Linked-project chip | `button` inside the linked block, text = project name; **no testid** | `SessionDetail.tsx:361-368` |
| No linked projects | text `None` (key `common_none`) | `SessionDetail.tsx:372` |
| Calibration section header | `[role="button"]` whose text contains `Calibration` (key `sessions_calib_heading`) | `SessionDetail.tsx:386`, `Section.tsx:27` |
| Calibration list | `[data-testid="session-calib-list"]` | `SessionDetail.tsx:118` |
| Calibration empty | `[data-testid="session-calib-empty"]` — title `No calibration match`, desc `No calibration masters are assigned to this session yet.` | `SessionDetail.tsx:109-113` |
| Unassign button | `[data-testid="session-calib-unassign-<kind>"]` (`dark`/`flat`/`bias`), text `Unassign`, `data-variant="danger"` | `SessionDetail.tsx:138-144` |
| Unassign confirm dialog | `[data-testid="session-calib-unassign-confirm"]` — title `Remove this assignment?` | `SessionDetail.tsx:153-163` |
| Notes section header | `[role="button"]` whose text contains `Notes` (key `sessions_notes_heading`) | `SessionDetail.tsx:397` |
| Notes textarea | `[data-testid="session-notes-textarea"]`, `aria-label="Session notes"`, placeholder `Add notes about this session…` | `SessionNotesSection.tsx:99-102` |
| Notes byte counter | `[data-testid="session-notes-byte-counter"]` | `SessionNotesSection.tsx:112` |
| Notes saved signal | `[data-testid="session-notes-saved"]` — text `Saved` (key `sessions_notes_saved`) | `SessionNotesSection.tsx:118-120` |
| Notes saving signal | `.pv-project-notes__saved` — text `Saving…` (key `common_saving`), **no testid** | `SessionNotesSection.tsx:114` |
| Notes over-limit error | `[data-testid="session-notes-error"]`, `role="alert"` — text `Note exceeds the 16,384-byte limit.` | `SessionNotesSection.tsx:127-135` |
| Frame inventory section | `[data-testid="session-frame-inventory"]`; scan button `[data-testid="frame-inventory-scan-btn"]`; summary `[data-testid="frame-inventory-summary"]` | `SessionFrameInventory.tsx:161,168,177` |
| Raw cleanup section | `[data-testid="session-raw-cleanup"]`; **collapsed by default** (`defaultOpen={false}`) | `RawFrameCleanupSection.tsx:156`, `SessionDetail.tsx:411` |

### Toasts

| Observable | Selector | Source |
| --- | --- | --- |
| Any toast | `div[role="alert"].pv-toast__item` inside `div.pv-toast__container[aria-live="polite"]` | `ToastContainer.tsx` (`ToastItem`) |
| Toast message text | `.pv-toast__message` | `ToastContainer.tsx` (`ToastItem`) |
| Error toast | `.pv-toast__item--error` | `ToastContainer.tsx` `VARIANT_CLASS` |
| Info toast | `.pv-toast__item--info` | same |
| Dismiss | `button[aria-label="Dismiss notification"]` (key `ui_toast_dismiss_aria`) | `ToastContainer.tsx` |

Toast strings this journey can produce:

- `Could not reveal the location.` (key `common_reveal_error`, variant `error`) — `SessionsPage.tsx:226`
- `Assignment removed.` (key `sessions_calib_unassign_success`, variant `info`) — `SessionDetail.tsx:95`
- `Could not remove the assignment.` (key `sessions_calib_unassign_failed`, variant `error`) — `SessionDetail.tsx:96,101`

**Auto-dismiss confirmed at 5000 ms** with no testid
(`apps/desktop/src/shared/toast.ts:38-39,88-92`). Sample the DOM within ~2 s of the
action, or install a `MutationObserver` on `.pv-toast__container` before clicking. A
DOM read taken 6 s later legitimately finds nothing — that is not evidence of a missing
toast.

**Toast class names are NOT hashed.** `ToastContainer.tsx` uses literal `pv-toast__*`
BEM strings, so the class selectors above are safe. Contrast with the source badge and
`Section` header, which use vanilla-extract hashed classes (`pt.sourceBadgeFits`,
`sec.title`) — for those, match on **text**, never class.

### Selector-trap check against J04's surfaces

| Trap from sibling units | Applies to J04? |
| --- | --- |
| Banner classes hashed → use `role="alert"` | **Yes, in the toast form.** Toasts and the notes over-limit error are both `role="alert"`. Toast classes happen to be literal, but `role="alert"` is the stable handle. |
| `Btn`/`Pill` expose `data-variant` | **Yes, confirmed.** `Pill.tsx:31` and `Btn.tsx:32` both emit `data-variant`. Used for the unassign button (`danger`) and connectivity pills (`warn`/`danger`, `connectivity.ts:34-36`). |
| Long tables virtualized → off-screen rows absent | **Yes, confirmed.** See the virtualization trap above. |
| Toasts auto-dismiss at 5000 ms, no testid | **Yes, confirmed** at `toast.ts:88`. |
| Some sections collapsed by default | **Partly.** Calibration and Notes are `defaultOpen` (`SessionDetail.tsx:386,397`) — open. Raw sub-frame cleanup is `defaultOpen={false}` (`:411`) — collapsed. `SessionFrameInventory` needs an explicit scan click. |

### Selectors I could NOT verify in source

1. **`localStorage` key for the persisted side-dock width/pin (E4.3).** The hook keys by
   `dockId` but composes the key indirectly (`useAdaptiveDock.ts:68,107`); I did not read
   the literal string. Enumerate `Object.keys(localStorage)` at drive time.
2. **Grouping-dims persistence key** — `SessionsPage.tsx:124` passes
   `storageKey: 'sessions.grouping.dims.v1'` to `useGrouping`; I did not read
   `useGrouping` to confirm it is used verbatim as the storage key rather than prefixed.
   Verify with `Object.keys(localStorage)` before asserting on it.
3. **`Section` collapsed/expanded state has no `aria-expanded`** (`Section.tsx`, only
   `role="button"`). Detect open/closed by the ▾ / ▸ glyph or by child presence, not by
   an ARIA attribute. This is itself a small a11y finding — see §6/F7.
4. **Linked-project chip** has no testid and no `aria-label`; the only handle is the
   project name text inside the linked block.
5. **Which `[data-testid="property-table"]` holds which field.** `SessionDetail.tsx:312-314`
   splits `factProps` at `Math.ceil(n/2)`, and the list length varies with whether the
   `integration` and `confirmedby` rows are present. **Never index the columns.** Query
   both tables and match on the `[role="rowheader"]` label text.

**Property-row label map** (`SessionDetail.tsx:227-310`), `en-GB` values:

| Field | Row label | i18n key | Declared source |
| --- | --- | --- | --- |
| type | `Frame type` | `inbox_frame_type_label` | (none) |
| target | `Target` | `projects_create_target_label` | `inferred` if provenance.target else `fits` |
| filter | `Filter` | `common_filter` | `inferred` / `fits` |
| frames | `Frames` | `projects_wizard_col_frames` | (none) |
| exposure | `Exposure` | `calibration_fp_exposure` | `fits` |
| integration | `Total integration` | `sessions_col_total_integration` | (none) — row omitted when null |
| night | `Night` | `sessions_col_night` | `fits` |
| camera | `Camera` | `settings_calmatch_camera` | `fits` |
| gain | `Gain` | `settings_calmatch_gain` | `fits` |
| binning | `Binning` | `settings_calmatch_binning` | `fits` |
| temp | `Sensor temp` | `settings_calmatch_sensor_temp` | `fits` |
| confirmedby | `Confirmed by` | `sessions_col_confirmed_by` | `user` — row omitted when absent |

> Note the doc says "sensor temperature"; the rendered label is **`Sensor temp`**.

### SC3 review-control sweep (the concrete recipe for E2.6 / E7.3)

Run inside `[data-testid="sessions-page"]`. Collect every `button`, `[role="button"]`,
and `[role="radio"]` accessible name, plus every `[data-variant]` element's text, then
match case-insensitively against `confirm`, `re-open`, `reopen`, `reject`, `ignore`,
`needs review`, `needs-review`, `candidate`.

**Expected non-empty matches that are NOT review controls** — do not report these as
failures:

- `Unassign` → its `ConfirmModal` uses the word "Remove", but the dialog is titled
  `Remove this assignment?` and the confirm action label is `Unassign`
  (`SessionDetail.tsx:153-158`). The generic `ConfirmModal` may still expose a
  cancel/confirm pair; check its own labels before flagging.
- Any `Confirmed by` property-row **label** — this is a metadata field, not a control.
  Scope the sweep to interactive elements to avoid it.

Static confirmation: `rg -in 'needs.review|needs_review|reopen|re-open|reject|ignore'
apps/desktop/src/features/sessions/*.ts{,x}` (exit 0) returns only doc comments, the
`SessionNotesSection.tsx:58` word "reject" in a byte-guard comment, and the
`RelationProposal*` components — whose render sites were **removed** from
`SessionsPage.tsx` (see the comment block at `SessionsPage.tsx:336-344`) and
`SessionDetail.tsx:378-382`. So SC3 should hold. If a `rejected` pill appears, the
RelationProposal render sites have been restored and that is the real story.

---

## 3. Fixture recipe

**Base: reuse the recipe on `astro-plan-mg6h8` verbatim.** Its comment dated
2026-08-24 20:06 is the live version. The comment dated 2026-08-24 14:59 has a
**section 6 ("Throwaway root and the shared real-FITS sample") that is DEAD** — it
instructs copying real frames out of `D:\Astrophotography`. Do not follow it. Do not
read a byte of `D:\Astrophotography`.

### What the existing recipe already gives you

Generator: `C:\jv-throwaway\mkfits.js` — Node, no dependencies, idempotent, writes real
FITS (2880-byte header blocks, 80-char cards, BITPIX 16, BZERO 32768) plus a monolithic
XISF. Invoke:

```bash
ssh <journey-host> "node C:/jv-throwaway/mkfits.js C:\\jv-throwaway"
```

Measured by that unit: 19 files, 37,844,362 bytes, a few seconds. Images are
1024×1024 uint16 — a deliberate deviation from real 6248×4176 geometry, traded for
copy/scan speed; the **headers** carry the realism (`INSTRUME`, `CAMERAID`, `EXPTIME`,
`DATE-OBS`, `IMAGETYP`, `XBINNING`/`YBINNING`, `CCD-TEMP`, `GAIN`, `TELESCOP`,
`FOCALLEN`, `OBJECT`/`FILTER`, `STACKCNT`/`NCOMBINE` on masters).

Contents relevant to J04: 3 lights cam A `M 51`/BLUE/300 s · 1 light cam A BLUE/180 s ·
2 lights cam C (no serial) `M 42`/LUM/180 s · 1 XISF light cam A BLUE/300 s · raw
bias/dark/flat triplets · 2 masters · 1 corrupt `.fits` (2880 bytes of junk).

That unit recorded, on that build: **3 derived sessions** —
`M 42/LUM/2026-10-18` (2 frames), `M 51/BLUE/2026-05-03` (1 frame),
`M 51/BLUE/2026-05-03` (4 frames = 3 FITS + XISF).

### What I add for J04

1. **A J04-unique throwaway root.** Do not reuse `C:\jv-throwaway\library` in place,
   because other units share the host and J04's S7 rescan-idempotency check must not see
   another unit's mutations. Generate into a J04-private tree:

   ```bash
   ssh <journey-host> "node C:/jv-throwaway/mkfits.js C:\\jv-j04"
   ```

   Register roots (siblings, never nested — the wizard hard-rejects parent/child overlap):

   ```
   light_frames  C:\jv-j04\library\Captures
   calibration   C:\jv-j04\library\Calibration
   project       C:\jv-j04\projects
   ```

   **UNVERIFIED:** I have not confirmed `mkfits.js` accepts an arbitrary destination
   root as `argv[2]`; the recipe only shows `C:\jv-throwaway`. Confirm by reading the
   script's argv handling on the host before relying on `C:\jv-j04`. If it hardcodes
   the path, fall back to generating into `C:\jv-throwaway` and copying the `library\`
   subtree to `C:\jv-j04\library`.

2. **A second same-night metadata-less pair, for E2.5 / SC4.** SC4's uniqueness claim
   needs *two* sessions that both lack `target`. The base set has no such pair. Add two
   light frames in two sibling folders under `Captures`, each with `OBJECT` **absent**,
   the same `DATE-OBS` night, the same `FILTER`, same `EXPTIME`, same camera:

   ```
   C:\jv-j04\library\Captures\NoObject\runA\  → one light, no OBJECT
   C:\jv-j04\library\Captures\NoObject\runB\  → one light, no OBJECT
   ```

   The app must produce two rows with **distinct** Target-cell text. `sessionDisplayName`
   should render `Session — <date> · runA` and `… · runB` (folder basename
   discriminator). If both render identically, #654 is live again.

3. **A frame whose file is missing on disk, for E2.3.** `frame_count` excludes frames
   whose `file_record.state = 'missing'`
   (`crates/persistence/targets/src/repositories/inventory.rs:363`). After apply,
   delete one frame file from a confirmed session and let the inventory scan mark it
   missing. The row's Frames count must **drop by one** while the session survives.
   This is the cheapest proof that the count is computed and not stored.

4. **The corrupt-`frame_ids` row, for E2.7.** No UI path creates it. Stop the app, then
   against `C:\jv-j04\app.db`:

   ```sql
   UPDATE <session table> SET frame_ids = 'not json' WHERE id = '<one session id>';
   ```

   Restart, open Sessions. Every other row must still list with its own count; the
   corrupted row must show `0`. This corrupts an **input** the app reads, which is the
   legitimate use of direct SQL — see the fixture-integrity contract below.
   **UNVERIFIED:** I did not read the session table name; resolve it from the migration
   before writing.

### Fixture-integrity contract — what I supply vs what the app must produce

This journey reviews **derived** data, so this split is the whole point.

**Inputs my recipe supplies** (allowed to be handed to the app):

- FITS/XISF files on disk with their header cards.
- Folder tree names (the token-pattern parser reads them).
- Registered library-root paths.
- The J02/J03 confirm-and-apply action (a user decision — Tier 1).
- One deliberately corrupted `frame_ids` cell (item 4) — an input to
  `list_sessions_for_root`'s JSON guard.
- One deleted frame file (item 3) — an input to the inventory scan.

**Outputs the app must produce, and which must NEVER be written by fixture SQL:**

| Derived value | Derived FROM | What proves derivation ran |
| --- | --- | --- |
| session row existence + grouping | `session_key` built at ingest from header target/filter/binning/gain/night | 3 sessions from 7 light files, split by gain/binning/exposure — a split the fixture never states |
| `frames` count | `json_array_length(frame_ids)` minus frames whose `file_record.state = 'missing'` (`inventory.rs:363`) | item 3: delete one file → count drops by one without any DB write by me |
| `target` / `filter` / `binning` / `gain` | `parse_session_key_fields` over the pipe-delimited key (`crates/app/core/src/inventory.rs:275-289`) | a catalogued (in-place) session shows real values, not `Session — <date>` |
| `capturedOn` (Night) | `night` segment of the key, computed by `sessions::observing_night`; falls back to `created_at[..10]` (`inventory.rs:371-377`) | Night must equal the **observing night** from `DATE-OBS`, not today's date. The M 42 set has `DATE-OBS 2026-10-19` in a folder named `2026-10-18` — Night should read `2026-10-18` if the noon boundary is applied. Related open bead: `astro-plan-uzcbj`. |
| `camera` (display name) | `build_camera_map` — majority vote over per-frame `camera`, resolved against the registered camera list (`crates/app/core/src/inventory.rs:232-243`, `persistence .../inventory.rs:425-444`) | cam C has **no serial**; its row must still name the gear |
| `name` / Target-cell identity | `sessionDisplayName` (`displayName.ts:31-42`) | item 2's two rows differ |
| Integration column | `integrationSeconds` = `parseFloat(exposure) × frames` (`integration.ts:16-24`), formatted by `formatIntegration` (`apps/desktop/src/lib/format.ts:38-46`) | **see §6/F4 — this input is hardcoded `None`, so nothing can prove it** |
| `calibrationMatches` | batch-loaded from `calibration_assignment` | the Calibration section lists the raw dark/flat/bias assignment for the 300 s BLUE cam-A session; the cam-B master must NOT match cam-A lights (PR #1742) |
| `relativePath` | parent folder of the session's first frame | distinct sessions reveal distinct folders (audit row, not DOM) |

**Never inject by SQL:** any of the rows in the right-hand column. The precedent this
repo already paid for is a fixture that set `camera_body_id` by direct `UPDATE`, which
left 11 tests green after the real derivation was nulled out. In particular, do not
seed `session_key`, `frame_ids`, `frames`, or `camera` — only the file headers and the
confirm-and-apply user action.

### Durability tier caveat

Per the constitution: **frame-to-session attribution is Tier 1** (user-knowledge-bearing,
committed synchronously) — a missing or wrong attribution after apply is a real defect.
**Classification evidence and scan results are Tier 2** and may be batched or written
asynchronously. Session *derivation* is driven by the event-driven `plan_listener` →
`ingest_light_frames` path; the windows-journeys doc's own troubleshooting note says
grouping "is event-driven and can take a moment". So:

- A session row that has not appeared **yet** is a Tier-2 lag, not a defect. Poll.
  Only a session that never appears is a failure.
- Notes autosave is a 5000 ms debounce plus a write — the `Saved` signal is the
  authority, not elapsed time.
- Do **not** file a delay as a defect. Record the observed latency and how long you
  polled.

---

## 4. Precondition and teardown

### Start state

1. Host is free. It is a **serial resource** — one app instance, one validator. Read
   the `astro-plan-ldj0v` comment dated 2026-08-25 for the hardcoded bridge port, the
   single-instance guard, and the current host state before launching anything.
2. Deploy: `git fetch origin` then `git reset --hard origin/main` as its **own**
   command. If `.rs` files changed, touch them (recompile trap):
   `Get-ChildItem <files>.rs | ForEach-Object { $_.LastWriteTime = Get-Date }`.
3. Fresh DB, then walk the setup wizard. It is **8 steps, not 7** (Language, Theme,
   Source Folders, Processing Tools, Configuration, Observing Site, Confirm, Scan) —
   `astro-plan-ko0tv` tracks the doc still saying 7. Drive it by clicking the button
   whose text starts `Continue`.
4. Register the three J04 roots from §3.
5. **Capture S1 before confirming anything.** Scan the Inbox, then open Sessions and
   record E1.1/E1.2 while raw scan results exist and nothing is confirmed. Once you
   confirm, S1 is unrecoverable without another DB reset.
6. Then run J02/J03's confirm-and-apply. **Record the plan and action counts** at that
   moment — E2.3 needs them and they are not visible from Sessions.

### Reset between segments

```powershell
Get-Process desktop_shell,node,cargo,rustc -EA SilentlyContinue | Stop-Process -Force
Remove-Item C:\jv-j04\app.db,C:\jv-j04\app.db-wal,C:\jv-j04\app.db-shm -Force
schtasks /Run /TN JVDev
```

~90 s to a clean first-run wizard on an incremental build. The `library\` fixtures
survive; only the DB goes.

### Reading the DB (needed for E6.1–E6.3)

The data lives in the **WAL** — copy all three files or you will read an empty 4 KB
database:

```bash
scp host:C:/jv-j04/app.db{,-wal,-shm} .
```

`audit_log_entry` columns are `at, trigger, entity_type, from_state, to_state, outcome,
reason_code, payload` — **there is no `topic` column**. A search on `topic` returns
nothing and that zero means nothing.

### Teardown

- Kill `desktop_shell`, `node`, `cargo`, `rustc`.
- Delete `C:\jv-j04\app.db*`.
- Leave `C:\jv-j04\library` in place (regenerating costs a few seconds but the roots
  must match what your run record describes). Delete the whole `C:\jv-j04` tree only
  when the J04 run record is filed.
- Do not touch `C:\jv-throwaway` — another unit owns it.
- Never register, point a step at, or read `D:\Astrophotography`. A metadata `stat` is
  the only permitted access.

---

## 5. Known-gap list — do NOT re-file these

| Expect | Status | Owner |
| --- | --- | --- |
| E2.7 / G4 — a 0 frame count from a corrupt `frame_ids` is indistinguishable from genuinely unattributed frames; no in-app repair path | Known, open | `astro-plan-dq9r3` |
| Night derivation disagrees with `sessions::key` across the noon boundary (inbox grouping side) | Known, open | `astro-plan-uzcbj` |
| Sessions collapse offset / optic-train / temperature / exposure heterogeneity, corrupting calibration match | Known, open | `astro-plan-qgyu` |
| Session DTO fields defaulted with `TODO(astro-plan-kyo7.88)` in `inventory.rs` | Known, open | `astro-plan-v3nu` — but see §6/F4: it names `optical_train_id`, `metadata`, `warnings`, `framesets` and **does not name `exposure` or `set_temp`** |
| Relation-proposal / matching-settings UI invokes 15 Tauri commands that do not exist; render sites removed | Known, open | `astro-plan-6yep`, `astro-plan-ic9h.20` |
| Setup wizard is 8 steps, J01 says 7 | Known, open | `astro-plan-ko0tv` |
| Windows validation host under-provisioned | Known, open | `astro-plan-iizrh` |

Gaps the journey doc lists as **resolved** — treat a failure here as a *regression*,
not a known gap: G1 (`#773` notes editing, PR #891), G2 (retired; PR #849 source-badge
coupling), G3 (`#889` connectivity).

Gaps the journey doc still lists as **NOT met** but which source says are fixed — see
§6/F1 (SC4/#654) and §6/F5 (SC5/S5). Verify them on the running app; a pass there means
the *document* is stale, not that the product regressed.

---

## 6. Static-evidence findings

Each is **STATIC ONLY, UNVERIFIED AGAINST RUNNING APP**.

### F1 — SC4 / S2's "#654 not met" claim is stale (doc defect, P3)

The journey doc (`journey.md:231-233`, `:90-96`) says two same-night metadata-less
sessions "can render the identical label `Session — {date}`", open bug #654.
`apps/desktop/src/features/sessions/displayName.ts:31-42` now appends a discriminator —
the frame-folder basename, else `<frames>f · <id[0:8]>` — and both the list
(`SessionsTable.tsx:317`) and the detail (`SessionDetail.tsx:346`) call it. Labels
should be unique. Also the doc's stated fallback expression `session.target ?? session.name`
no longer matches the code.

### F2 — S3's "there is no frame-type filter" is false (doc defect, P2)

`journey.md:105-107` asserts as a negative Expect: "There is no frame-type filter —
sessions are light frames only". `SessionsPage.tsx:256-267` renders a `Type` filter with
options `Acquisition` / `Dark` / `Flat` / `Bias`, defaulting to `light`, and
`SessionsPage.tsx:119-121,138` pushes it to the backend as `frameFilter`. The page's own
docstring (`:20-24`) explains why (#652: the ledger carries calibration groups too).
Driving this Expect as written produces a guaranteed false FAIL.

### F3 — S3's "single-column sort" negative is right about sort, wrong about grouping (doc defect, P3)

`journey.md:107-109` says `SessionSort` carries one `{col, dir}` pair and "no
secondary-sort UI exists anywhere". The sort half is correct
(`SessionsTable.tsx:58-61`). But the Group-by control is **three ordered slots**
(`FilterToolbar.tsx:125,128`, `maxLevels = 3`), and S3's own Do step says "the Group-by
control (Target/Filter/Night/Camera/Month)" as if it were one. A driver reading the
negative may mistake the level-2/level-3 selects for the "secondary sort" the doc says
does not exist.

### F4 — `InventorySession.exposure` and `set_temp` are hardcoded `None` (product defect, P2 — the highest-value item here)

`crates/app/core/src/inventory.rs:381` — `let exposure = None;`, with the comment "No
exposure in session_key; would come from the fingerprint/provenance join in a full
implementation (TODO(astro-plan-kyo7.88))". `:399` — `set_temp: None`. Neither is
conditional; every session in the projection gets `None`.

Downstream consequences, all traced:

- `integrationSeconds()` returns `null` immediately (`integration.ts:19`,
  `if (!session.exposure) return null`).
- The list's **Integration** column renders `formatIntegration(null)` = `'—'`
  (`format.ts:39`, `EMPTY = '—'` at `:22`). The column promises a total and can only
  ever show a dash.
- Sorting by Integration is a **no-op**: `compareSessions` case `'exposure'` computes
  `(integrationSeconds(a) ?? 0) - (integrationSeconds(b) ?? 0)` = `0` for every pair
  (`SessionsTable.tsx:119`).
- The detail's `Total integration` row is **omitted entirely** — it is spread in only
  when `integrationSec != null` (`SessionDetail.tsx:258-266`).
- The detail's `Exposure` row renders the `Unresolved` chip with no source badge.
- The detail's `Sensor temp` row renders the `Unresolved` chip — and its own comment
  (`:292-294`) says it is "Applicable to every light session … always present so an
  absent value renders the unresolved chip", which is now *always* the case.
- `equipmentSubtitle` never includes a temperature (`SessionDetail.tsx:188`).

**This makes E4.6 (`"1h 30m"` grammar, Δ6, PR #1288) untestable on Sessions.** Δ6 claims
Sessions now matches Projects' h/m grammar; the shared formatter is adopted, but it is
never reached with a value. `astro-plan-v3nu` owns the `inventory.rs` TODO cluster but
names `optical_train_id` / `metadata` / `warnings` / `framesets` — **not `exposure` or
`set_temp`**. This looks unowned. Parent should decide whether to extend `v3nu` or file
a new bead; I filed nothing.

**Contradicts a prior run record.** The `astro-plan-mg6h8` recipe comment reports
observed session integration times `6m` / `5m` / `18m`. Source says those cells must all
be `—`. One of the two is wrong; the drive run settles it. Note that `18m` is also
arithmetically inconsistent with 4 frames × 300 s (= `20m`) and `5m` with 1 frame × 180 s
(= `3m`), which makes the run record independently suspect. **Do not carry those figures
into a report.**

### F5 — S5 / SC5's "the id is dropped" claim is stale (doc defect, P2)

`journey.md:173-189` and `:234-236` state that `SessionsPage.tsx:256`'s `onOpenProject`
handler discards the project id: `onOpenProject={() => navigate({ to: '/projects' })}`.
Current source, `SessionsPage.tsx:302-304`:

```tsx
onOpenProject={(id) =>
  navigate({ to: '/projects', search: { selected: id } })
}
```

The id **is** passed as the `selected` search param, which is exactly what
`ProjectsPage.tsx:83` consumes. SC5 should now be met. The doc's cited line number
(`:256`) no longer points at the handler either.

### F6 — S6's three path Expects have no UI observable (doc defect, P3)

`journey.md:194-200` states four things about the revealed folder. Three of them
(root+`relativePath` join, distinct folders per session, root fallback) are only
observable outside the webview. Nothing in `SessionDetail.tsx` or
`revealInventory.ts` renders the resolved path. The journey's own cross-cutting rule
demands a success signal per mutating step; for S6 the only in-app signal is the
*failure* toast (E6.4). The doc should name the audit row
(`entity_type='inventory_row'`) as the observable, or the step is unfalsifiable by a
UI-only driver.

### F7 — `Section` exposes no `aria-expanded` (product defect, P3, a11y)

`apps/desktop/src/ui/Section.tsx` gives the collapsible header `role="button"` but no
`aria-expanded`; the open/closed state is conveyed only by the `▾` / `▸` text glyph.
Assistive tech cannot announce collapse state, and an automated driver has no ARIA
handle. Affects the Calibration, Notes, and Raw-cleanup sections in J04's detail panel,
and every other `Section` consumer. Contrast `SessionsTable.tsx:281` (the group header
*does* set `aria-expanded`) and `SegControl.tsx:75` (`aria-checked`).

### F8 — the windows-journeys J04 doc is a strict subset and carries zero selectors (doc defect, P3)

`docs/development/windows-journeys/journey-04-sessions-review.md` has **5 Tests** against
the journey's 7 steps and 15 Expects. It covers S1, S2, S2-negative, S4-notes, and S7.
It has **no coverage** of S3 (filter/group/sort, `aria-sort`, grouping hint), S4's
adaptive dock / property-grid / source-badge / unresolved-chip / calibration-section /
Escape behaviour, S5 (linked project), or S6 (reveal) — i.e. nothing from Δ2 through Δ7,
all of which post-date it. It also names **no `data-testid` and no selector of any kind**,
so it cannot be "reused rather than re-deriving selectors" as the brief hoped; §2 above
is derived from source instead. Its "Journey facts" section still points at
`docs/product/journeys/user-journeys.md` Journey 4, a pre-migration path. A concurrent
unit is censusing this whole directory — this belongs in that census, not in a separate
bead.

### F9 — J04 has no `deltas/` directory; six `trace:` entries point at the pre-migration tree (doc defect, P3)

`ls docs/journeys/J04-sessions-review-derived/` returns `journey.md` only (exit 0). The
`trace:` block cites six `docs/product/journeys/J04-sessions-review-derived/deltas/*.md`
files; that legacy directory does exist (`ls` exit 0) alongside a 3.4 K legacy
`journey.md`. Which of the two trees is authoritative is not stated in either. The Δ
entries were folded inline into the live doc's Delta log, so the legacy deltas are
duplicates, but a reader following `trace:` lands in the old tree.

---

## Command log

All run from `<repo>`, branch
`chore/journey-validation-formula`, 2026-08-25. All read-only.

| Command | Exit | Result |
| --- | --- | --- |
| `bd show astro-plan-ba3yl` | 0 | brief |
| `bd update astro-plan-ba3yl --claim` | 0 | claimed as `rs-ba3yl` |
| `ls docs/journeys/` | 0 | 18 journey dirs |
| `ls -R docs/journeys/J04-sessions-review-derived/` | 0 | `journey.md` only — no `deltas/` |
| `ls docs/product/journeys/J04-sessions-review-derived/` | 0 | `deltas/`, `journey.md` (3.4 K legacy) |
| `ls docs/development/windows-journeys/` | 0 | 11 files incl. `journey-04-sessions-review.md` |
| `rg -c '^### S' journey.md` | 0 | 7 |
| `rg -c '^- \*\*Expect' journey.md` | 0 | 15 |
| `rg -c '^- \*\*Expect \(negative\)' journey.md` | 0 | 7 |
| `rg -c '^- SC[0-9]' journey.md` | 0 | 7 |
| `rg -c '^- G[0-9]' journey.md` | 0 | 4 |
| `rg -c '^- \*\*Δ' journey.md` | 0 | 6 |
| `rg -n 'data-testid\|testId' <7 component files>` | 0 | 29 hits, tabulated in §2 |
| `rg -n 'data-variant' ui/Pill.tsx ui/Btn.tsx` | 0 | 2 hits — trap confirmed |
| `rg -n 'virtualized\|useVirtualizer' ui/Table.tsx` | 0 | `@tanstack/react-virtual` — trap confirmed |
| `rg -n '5000\|duration' shared/toast.ts` | 0 | `duration ?? 5000` at `:88` — trap confirmed |
| `rg -n 'MAX_NOTE_BYTES\|NOTE_DEBOUNCE_MS' lib/notes.ts` | 0 | `16_384`, `5_000` |
| `rg -n 'filterbar-' components/FilterToolbar.tsx` | 0 | 4 id sites → `#filterbar-<key>` |
| `rg -n 'exposure' app/core/src/inventory.rs` | 0 | `let exposure = None;` at `:381` — F4 |
| `rg -in 'needs.review\|reopen\|re-open\|reject\|ignore' features/sessions/*.ts{,x}` | 0 | comments + unmounted RelationProposal* only |
| `python3` read of `messages/en-GB.json` (≈60 keys) | 0 | all resolved; `common_reveal_windows` / `_macos` / `_linux` **absent** — the real keys are `reveal_label_*` |
| `bd show astro-plan-mg6h8` | 0 | recipe + section-6 retraction |
| `bd comments astro-plan-mg6h8 --json` | 0 | 11 comments; the 2026-08-24T20:06:35Z one is the live recipe |
| `bd show astro-plan-v3nu` / `astro-plan-kyo7.88` | 0 | v3nu open, kyo7.88 closed |
| `mkdir -p <scratch>/journey-prep/J04` | 0 | artifacts dir |

Zero-result sanity check: the review-vocabulary search was validated against a case
known to match first — `rg -c 'Confirm' SessionDetail.tsx` returned **8**, so the
pattern machinery works and the subsequent narrow search's "comments only" outcome is
real, not a silent no-match. `grep | head` was avoided throughout (it masks no-match
exit status), and no pattern uses `\b` (absent from POSIX ERE).
