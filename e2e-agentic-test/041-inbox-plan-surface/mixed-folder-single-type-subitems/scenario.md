# 041 ingest — mixed folder materializes single-type sub-items (no "mixed" items)

> Two-stage verification plan. Stage 1 is executed by an agent driving the
> real Windows app over the Tauri MCP bridge; Stage 2 is a human-judgment
> Claude Desktop pass and runs **only after Stage 1 passes**.
> Shared runner mechanics (launch, reset, deploy, bridge connect, window
> sizing): see `../../AGENT-RUNNER.md`. Fixtures: see `../FIXTURES.md`.

## Coverage

| Requirement | Assertion in this scenario |
|---|---|
| FR-034 / US10 / SC-012 | One leaf folder mixing frame types yields N single-type items, zero "mixed" items |
| FR-041 | Sub-items appear only after classify (lazy scan) |
| FR-042 / SC-019 | Rescan of unchanged content produces identical items (no churn) |
| FR-043 / US13 | Each sub-item carries source-group provenance (shared parent leaf folder, sibling grouping) |
| FR-008 / SC-011 | Structured, pill-free list rows; no overflow at 1100×720 |
| FR-009 / SC-006 | Two-level grouping (target → frame type) nests correctly |
| FR-011 | Composition is explicit (per-type counts), never a bare "mixed" label |
| FR-021 / US6 / SC-010 | Status-bar per-type breakdown matches queue contents |

Convoy preconditions: none — this behavior is on `redesign-ui-platevault`
already (spec 041 iteration 2, PR #315 lineage).

## Preconditions

1. Deploy branch `redesign-ui-platevault` on `C:\dev\astro-plan`
   (fetch + `git reset --hard origin/redesign-ui-platevault` as its own
   command; touch `.rs` files if Rust changed since the running binary —
   see AGENT-RUNNER.md recompile trap).
2. Clean state: kill the app, `Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force`,
   remove old fixtures (FIXTURES.md § Cleanup).
3. Generate **RECIPE-MIXED** (FIXTURES.md): `test-data\inbox-drop\night1`
   with 2×light/Ha/300s + 2×light/Ha/120s + 2×dark/300s. Also create the
   empty **RECIPE-DEST** folder (`test-data\library-lights`).
4. Launch the dev app with the bridge and E2E hooks
   (`VITE_E2E=1`, dev overlay config, `--features dev-tools`, WS bridge on
   `127.0.0.1:9223` unless `PV_MCP_BRIDGE_BIND` widens it), connect
   `driver_session` from WSL, and set the window to **1100×720** via
   `manage_window`.
5. Real backend only: confirm `.env` has `VITE_USE_MOCKS=false`; a scenario
   run against mocks is INVALID.
6. Complete first-run setup via the E2E path inputs
   (`e2e-path-input-inbox` → `C:\dev\astro-plan\test-data\inbox-drop`,
   `e2e-add-path-btn-inbox`; `e2e-path-input-light_frames` →
   `C:\dev\astro-plan\test-data\library-lights`, org-state select on the row
   → **organized**), then Continue through the remaining steps to finish.
7. Navigate to `/inbox`.

## Stage 1 — Agent validation via Tauri MCP

Run steps in order. Every step lists an exact Expected result; any FAIL
stops the stage.

1. **Scan picks up the drop folder.** Click the top-bar **Rescan** button
   (fallback: `webview_execute_js` →
   `window.__TAURI__.core.invoke('inbox_scan', { rootId: null })`), then wait
   for `[data-testid="inbox-list"]` to be non-empty (`webview_wait_for`).
   - Expected: the list renders at least one row for `night1`.
   - IPC assertion (via `ipc_monitor` capture or a direct
     `invoke('inbox_list', {})`): response `items[]` contains entries with
     `relativePath` = `night1` (or `night1/...`).
   - FAIL if: no items appear after 30 s, or the page falls back to an error
     banner.
2. **Classification materializes 3 single-type sub-items.** Select the first
   `night1` row (click it in `[data-testid="inbox-list"]`); classification
   runs on selection. Then re-read `inbox_list`.
   - Expected: exactly **3** items whose provenance is the `night1` leaf
     folder, with `groupFrameType`/`groupExposure` distinguishing them:
     `light`+`300`, `light`+`120`, `dark`+`300`. Each item's
     `classification.type` (via `invoke('inbox_classify', { req: {
     inboxItemId, rootAbsolutePath, forceRescan: false } })`) is
     `single_type`.
   - Expected: **zero** items labelled mixed: no
     `[data-testid="inbox-mixed-alert"]` is shown for any of the three
     sub-item rows, and no list row text contains the word "mixed".
   - FAIL if: a single folder-level row with a mixed composition persists as
     the only entry after classify, or fewer/more than 3 single-type items
     exist for `night1`, or any sub-item spans two frame types.
3. **Explicit composition, not "mixed" (FR-011).** With a light sub-item
   selected, read the detail panel.
   - Expected: the per-file metadata table lists exactly that sub-item's
     2 files with type/filter/exposure/binning/gain/temp/object/date columns
     populated (`Ha`, `300`/`120`, `NGC 7000`, the fixture dates); absent
     values render as a dash, not blank crashes.
   - FAIL if: the detail hangs on a loading state, shows files from a
     sibling sub-item, or presents a bare "mixed" label anywhere.
4. **Source-group provenance (FR-043).** Group the list by **source** using
   the top-bar grouping selects (FilterToolbar grouping slots), or verify the
   default rendering.
   - Expected: the three sub-items are presented under their shared parent
     (`night1` / the inbox root's name), sibling relationship visible; group
     header nodes use `data-testid="inbox-group-<dimension>-<key>"`.
   - FAIL if: sub-items appear with no way to see they were ingested
     together (no shared parent/folder association anywhere in row or group
     rendering).
5. **Two-level grouping (FR-009/SC-006).** Set grouping slot 1 = `target`,
   slot 2 = `frameType`.
   - Expected: nested collapsible groups — `NGC 7000` at the top level
     containing a `light` group with the two light sub-items; the darks
     (no target) gather under an explicit "(none)" top-level group
     containing a `dark` group. Collapsing `NGC 7000` hides its children.
   - FAIL if: nesting order does not match slot order, darks disappear
     instead of bucketing under "(none)", or groups are not collapsible.
6. **Stats breakdown (FR-021/SC-010).** Read the status bar:
   `[data-testid="statusbar-inbox-summary"]` and
   `[data-testid="inbox-stats-summary"]` with per-type entries
   `[data-testid="inbox-stats-type-<frameType>"]`.
   - Expected: per-type counts consistent with the queue: light 4 images /
     dark 2 images (folder counting per the derived stats: the `night1`
     folder counts once overall), no bare "N folders"-only summary.
   - FAIL if: the per-type strip is absent, or counts disagree with the
     actual fixture contents.
7. **Rescan produces no churn (FR-042/SC-019).** Record the 3 sub-items'
   `inboxItemId` + group labels from `inbox_list`. Click **Rescan**, wait for
   completion, re-read `inbox_list`.
   - Expected: the same 3 items (same identity `(root, relative_path,
     group_key)` — same ids or, at minimum, identical group labels/keys and
     counts; note in the report whether ids are stable) and the list does not
     flicker into duplicates.
   - FAIL if: item count changes, a "mixed" item appears, or duplicate
     sub-items accumulate.
8. **Layout sanity at 1100×720 (SC-011).** With the window at 1100×720 and
   the detail open, take `webview_screenshot` (checkpoint `S1-mixed-01`).
   Query via `webview_execute_js`:
   `document.querySelector('.alm-page__bar') !== null` and check
   `document.scrollingElement.scrollHeight <= window.innerHeight + 1`.
   - Expected: top bar and status bar pinned (no page-level scrollbar; only
     the list/detail scroll containers scroll); no horizontal overflow in
     the list (`inbox-list` `scrollWidth <= clientWidth + 1`); no pill
     elements overflowing the sidebar.
   - FAIL if: the whole page scrolls, or list rows clip/overflow.
9. **Log check.** `read_logs` (bridge) for the run window.
   - Expected: no `ERROR`-level entries from inbox classify/list; no React
     error-boundary or unhandled-rejection lines.
   - FAIL if: any classify/list error is logged during steps 1–8.

Screenshot checkpoints: `S1-mixed-01` (grouped list + detail at 1100×720),
`S1-mixed-02` (status-bar stats close-up).

### Stage 1 verdict

- **PASS**: steps 1–9 all pass.
- **FAIL**: any FAIL-if condition. Report the step number, the observed
  IPC payload / DOM snapshot excerpt, and the screenshot.

## Stage 2 — Final Claude Desktop pass

Human-judgment validation. Only run after Stage 1 reports PASS. Preconditions
identical (the state left by Stage 1 is acceptable; rescan if needed).

1. Open `/inbox`, look at the three `night1` sub-items. Judge: are the rows
   **legible and self-explanatory** — can you tell at a glance which item is
   which (type, exposure, filter, count) without opening the detail? Is the
   "ingested together" relationship discoverable?
2. Apply grouping target → frameType, collapse and expand groups. Judge:
   does the nesting read naturally? Are "(none)" buckets understandable?
3. Switch theme (Settings → General → theme cards): verify the list, group
   headers, and detail panel in **Warm Clay** (light) and **Observatory**
   (dark) at minimum. Judge: contrast of group headers, badges, and the
   stats strip in both themes; nothing becomes invisible.
4. Resize the window narrower (~1000 px) and back. Judge: action bars stay
   pinned, only content scrolls, nothing overflows (page layout convention).
5. i18n scan: all visible strings on the page come from the catalog (no raw
   message keys like `inbox_...` rendered, no hardcoded dev English like
   "TODO"); counts pluralize correctly ("1 item" vs "2 items") where shown.
6. Sign-off: record PASS/FAIL per point with screenshots for 3 (both
   themes). Overall verdict PASS only if Stage 1 passed AND no point above
   reveals a usability/visual defect worth filing; otherwise FAIL with a
   filed issue list.
