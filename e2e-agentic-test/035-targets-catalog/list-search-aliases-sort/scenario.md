# Targets — catalog list, alias-aware search, sort + aria-sort

> Two-stage verification plan for the Targets (Planner) list surface.
> Stage 1 is executed by an agent driving the REAL Windows app over the Tauri
> MCP bridge. Stage 2 is a human-judgment pass in Claude Desktop and runs ONLY
> after Stage 1 reports PASS. Mock mode is FORBIDDEN for both stages
> (`.env` must have `VITE_USE_MOCKS=false`; the run is invalid otherwise).
>
> Shared launch / reset / bridge mechanics: see `e2e-agentic-test/AGENT-RUNNER.md`
> (authored on the `verify-plans-setup-sources` lane). This file only adds what
> is specific to this scenario.

## Feature facts (context — do not act on this section)

- Specs: 035 SIMBAD target resolution (FR-002 bundled seed, FR-007 aliases of
  one physical object = one canonical target, FR-010 search spans all
  catalogues), 036 gen-3 Targets rebuild, 043 §4 Targets redesign (shared
  list-page layout, task #73/#82/#84/#85), 044 planner mock columns.
- Surfaces: `apps/desktop/src/features/targets/TargetsPage.tsx`,
  `TargetsTable.tsx` (virtualized, padding-spacer pattern).
- IPC: `target_list` (no args) → `TargetListItem[]`; each item carries
  `id`, `primaryDesignation`, `effectiveLabel`, `objectType`, `aliases`,
  `constellation`, `magnitude`.
- Search is client-side and alias-aware (#103b/#29): whitespace/case
  insensitive over designation + label + every alias ("M31" ≡ "M 31" ≡ "m31";
  "Andromeda" resolves to M 31 via aliases).
- Sorting is header-driven (SortHeader buttons); default sort
  `designation asc`. Table is FLAT by default; grouping is opt-in via the
  top-bar Group-by control (`targets-group-<key>` header rows).
- The list is VIRTUALIZED (~13k rows with all catalogues enabled): only a
  window of `<tr>` rows exists in the DOM, bracketed by two aria-hidden
  spacer rows (`.alm-targets-table__spacer`).

### Convoy preconditions

- **Requires PR #415 merged** for Test 5 only (aria-sort on the active `<th>`
  — `feat: single-column Archive page, richer Sessions list, and
  screen-reader sort announcements`, base `redesign-ui-platevault`). If #415
  is not yet merged, run Tests 1–4 and report Test 5 as
  `BLOCKED — PR #415 not merged`, not FAIL.

## Preconditions

1. Windows checkout `C:\dev\astro-plan` on branch `redesign-ui-platevault`
   (`git fetch origin` then `git reset --hard origin/redesign-ui-platevault`
   as its OWN command). Frontend-only surface, but the seeded target catalog
   is backend: after reset, force a Rust rebuild if any `.rs` changed since
   the last build (see AGENT-RUNNER.md recompile trap).
2. Fresh DB (`Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force`),
   launch via `run-dev.bat`, complete first-run setup with any one Light
   frames folder + one Projects folder (see AGENT-RUNNER.md).
3. The bundled catalog seed populates targets on first use — no fixture
   files are needed for this scenario.
4. Connect the Tauri MCP bridge (`driver_session host=<WSL gateway> port=9223`).
   The gateway address reaches the bridge only when `run-dev.bat` sets
   `PV_MCP_BRIDGE_ENABLE=1` and `PV_MCP_BRIDGE_BIND=0.0.0.0` and passes
   `--features dev-tools` (see AGENT-RUNNER.md).

## Stage 1 — Agent validation via Tauri MCP

Run every test. Capture a `webview_screenshot` at each 📸 checkpoint. After
the run, `read_logs` and report any `ERROR`-level entries emitted during the
tests (unexpected errors = FAIL even if the UI looked right).

### Test 1 — List loads from the real backend

1. Navigate to **Targets** (left nav). Wait for the table
   (`webview_wait_for` on `.alm-targets-table`).
2. Via `webview_execute_js`, invoke the same command the page uses and count:
   ```js
   window.__TAURI__.core.invoke('target_list').then(r => (r.Ok ?? r.ok ?? r).length ?? JSON.stringify(r).slice(0,200));
   ```
Expected:
- The table renders with columns, left→right: ★ · Designation · Type ·
  Max alt · Tonight · Visible · Opposition · Lunar · Filters · Img time ·
  Sessions.
- The footer under the table reads a target count > 0, and `target_list`
  returned a non-empty array (count in the same order of magnitude as the
  footer — footer counts only the enabled-catalogue subset, so footer ≤ IPC
  count).
- 📸 checkpoint: full page.
FAIL if: empty table with a non-empty IPC result, an error EmptyState, or
`target_list` rejects.

### Test 2 — Virtualization: windowed rows, correct scrollbar

1. `webview_execute_js`:
   ```js
   (() => {
     const rows = document.querySelectorAll('.alm-targets-table__row').length;
     const spacers = document.querySelectorAll('.alm-targets-table__spacer').length;
     const footer = document.querySelector('.alm-targets-table__footer')?.textContent;
     return JSON.stringify({ rows, spacers, footer });
   })();
   ```
2. Scroll the list viewport to the bottom:
   ```js
   (() => { const el = document.querySelector('.alm-targets-table__scroll'); el.scrollTop = el.scrollHeight; return el.scrollTop; })();
   ```
   then re-run step 1's snippet.
Expected:
- With the full catalog (hundreds+ rows), the rendered `rows` count is a
  small window (well under 100), and at least one spacer row exists.
- After scrolling to the bottom, rendered rows are DIFFERENT targets (last
  designations alphabetically for the default sort) and the top spacer is
  present (spacers ≥ 1). Scrolling is smooth — no multi-second freeze.
FAIL if: every row is rendered at once for a large list (rows ≈ footer
count), the table is blank after scroll, or row heights are visibly stretched
(the runaway-height bug described in TargetsTable.tsx).

### Test 3 — Alias-aware search

1. Type `M 31` (WITH space) into the top-bar search
   (`webview_find_element` by `aria-label` = the Targets search field, then
   `webview_interact` / `webview_keyboard`).
2. Note the visible designations. Clear, type `M31` (no space).
3. Clear, type `Andromeda`.
4. Clear, type `zzzz-no-such-target`.
Expected:
- Steps 1 and 2 return the SAME result set, including M 31.
- Step 3 also surfaces M 31 (alias resolution via `aliases` on the list
  payload). If the seed row for M 31 carries no "Andromeda" alias, report the
  observed alias list from `target_list` for M 31 instead of guessing —
  that is a data gap, not a UI FAIL.
- Step 4 shows the empty-state text ("no match" copy), not a crash, and the
  footer count reads 0.
FAIL if: spaced vs compact queries differ, or the empty query state errors.

### Test 4 — Header sort toggles and reorders rows

1. Click the **Designation** header button once (it is the default active
   `asc` column → this toggles to `desc`). Read the first 3 visible
   designations via JS.
2. Click **Type** once (switches column, resets to `asc`).
3. Click **Max alt** twice (asc → desc); read the first visible Max alt cell.
Expected:
- Step 1 reverses the designation order (last-alphabetical first).
- Step 2 orders rows by object type ascending; the SortHeader arrow moves to
  the Type header.
- Step 3 (desc) puts the LARGEST altitude value first (values are the
  deterministic stub model — ordering must still be monotonic).
FAIL if: clicking a header does nothing, the arrow indicator does not follow
the active column, or ordering is not monotonic in the sorted column.

### Test 5 — aria-sort on the active column `<th>` (requires PR #415)

1. `webview_execute_js`:
   ```js
   (() => {
     const marked = [...document.querySelectorAll('.alm-targets-table th[aria-sort]')];
     return JSON.stringify(marked.map(th => [th.textContent.trim(), th.getAttribute('aria-sort')]));
   })();
   ```
2. Click the active header once and re-run.
Expected:
- Exactly ONE `<th>` carries `aria-sort` — the active sort column — with
  value `ascending` or `descending` matching the visible arrow.
- After the toggle click, the same `<th>`'s value flips.
FAIL if: zero or 2+ headers carry `aria-sort`, or the value does not match
the actual direction. BLOCKED (not FAIL) if PR #415 is unmerged.

### Test 6 — Grouping opt-in and collapse

1. In the top-bar Group-by control, select **Catalogue** as the first
   dimension.
2. Assert group header rows exist: `webview_find_element` on
   `[data-testid^="targets-group-"]`.
3. Click one group header; assert its `aria-expanded` flips to `false` and
   its child rows disappear; click again to restore.
4. Clear the grouping. Assert no `targets-group-` testids remain (FLAT
   default restored).
Expected: as inline above. 📸 checkpoint while grouped.
FAIL if: grouping renders no headers, collapse does not hide children, or
clearing grouping leaves headers behind.

**Stage 1 verdict**: PASS only if Tests 1–4 and 6 PASS and Test 5 is PASS or
BLOCKED. Any FAIL, or any new ERROR log line from these actions, is a Stage 1
FAIL — do not proceed to Stage 2.

## Stage 2 — Final Claude Desktop pass (only after Stage 1 PASS)

Human-judgment visual/UX validation on the same running app, window sized
**1100×720**.

1. **Layout discipline**: the top bar (search + filters + Group-by + "Add
   target") stays pinned while ONLY the table body scrolls; the footer count
   stays visible. No horizontal scrollbar at 1100×720; no clipped column
   headers (Lunar / Img time are intentionally abbreviated).
2. **Themes**: switch Settings → General between **Warm Slate** (light) and
   **Observatory** (dark). On both: row hover/selection contrast readable,
   sparkline and filter badges legible, group header rows distinguishable
   from data rows, sort arrow visible.
3. **i18n**: no raw message keys (e.g. `targets_col_...`) anywhere in the bar,
   headers, tooltips, empty states.
4. **UX judgment**: search feels instant on the full catalog; sorting a 13k
   list does not stutter; the star (favourite) toggle does not also select
   the row; empty states read as intentional copy, not debug text.
5. Sign-off: record PASS/FAIL per point with screenshots for both themes.

## Verdict rubric

- **PASS**: all Stage 1 tests green (Test 5 may be BLOCKED with reason) AND
  all Stage 2 points signed off.
- **FAIL**: any functional mismatch, any raw i18n key, layout scroll
  violation, or unexplained ERROR logs.
- Report per test: PASS / FAIL / BLOCKED + one line of observed evidence
  (verbatim text/JS results for the IPC and aria-sort assertions).
