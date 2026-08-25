# Windows validation — Journey 10: Settings, appearance, and i18n

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 10 (specs
  018/019/043/046).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes for settings persistence + translated error
  codes (spec 046); the layout convention and theme switch are frontend-only.
- Changed surfaces: **Settings** (12 panes / 3 sections), Appearance, bottom
  log panel, left sidebar, global command palette, every page's pinned
  header/scrolling-content layout.
- What this journey proves: every settings pane auto-saves (no global Save
  button); themes apply live and survive restart; the bottom log panel is a
  layout participant (shrinks content, never overlays); every user-facing
  string — including backend error codes — routes through the translation
  catalog; the 1100×720 minimum layout keeps headers pinned while only
  content scrolls.
- Automated coverage baseline today: **this journey has NO Layer-2 coverage
  and no Playwright mock coverage at all** (confirmed by both
  `verify-on-windows-journeys.md` and
  `e2e-mock-coverage-audit-2026-07-05.md`) beyond the generic
  `all_top_level_screens_load` smoke check that `/settings` renders without
  crashing. Spec 046 (i18n/error-codes) and spec 043 (redesign/layout
  convention) are both marked `✅ Implemented` in `specs/SPEC_STATUS.md`,
  meaning the underlying behavior is real and shipped — it's the UI-level
  verification of that behavior that has no automated proof at any layer.

## Windows environment mechanics (read once, applies to every Test below)

> Canonical mechanics: `docs/development/windows-native-rust-dev.md` §"Validation driving (MCP bridge, reset, recompile trap)". The steps below are the self-contained per-journey copy; reconcile to that doc if they drift.

- Windows checkout: `C:\dev\astro-plan`. Deploy: `git fetch origin`, then
  `git reset --hard origin/main` as its OWN command.
- **Recompile trap**: touch changed `.rs` files after a reset if Rust
  changed; otherwise a hard refresh suffices.
- Reset to fresh first-run if needed:
  `Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force`.
- Launch: `powershell.exe -NoProfile -Command "Start-Process -FilePath 'cmd.exe' -ArgumentList '/k','C:\dev\astro-plan\run-dev.bat' -WorkingDirectory 'C:\dev\astro-plan'"`.
  Kill: `Get-Process desktop_shell,cargo | Stop-Process -Force`.
- Blank window recovery: restart dev server; if still blank, `pnpm install`
  with `$env:CI="true"`, relaunch.
- Tauri MCP bridge (optional): `cargo tauri dev --config
  src-tauri\tauri.dev.conf.json --features dev-tools` (bridge WS on `127.0.0.1:9223`), connect with
  `driver_session host=localhost port=9223`, invoke via `webview_execute_js` →
  `window.__TAURI__.core.invoke('<snake_command>', {args})`.

## Preconditions
1. Deploy as above; complete setup with at least one registered source.
2. Sanity: Settings is reachable from the left nav.

## Tests

### Test 1 — Pane grouping, no global Save
Steps:
1. Open **Settings**.
Expected:
- 12 panes grouped into Library (Data Sources, Equipment, Ingestion, Naming,
  Catalogs, Planner), Processing (Tools, Calibration, Cleanup), and
  Application (General, Advanced, Audit Log). No global "Save" button
  anywhere — every field auto-saves.
FAIL if:
- A global Save button exists, or a field change requires it to persist.

### Test 2 — Theme switch applies live and persists
Steps:
1. In Appearance, switch through all 4 named themes plus "System".
2. After settling on one, fully restart the app.
Expected:
- Each theme applies immediately with no reload needed. After restart, the
  last-chosen theme is still active.
FAIL if:
- A theme requires a reload to apply, or resets after restart.

### Test 3 — Font-size is visual-only (expected, not a bug)
Steps:
1. Change the Density/Font-size controls in Appearance.
Expected:
- Density affects the app; Font-size is confirmed to change nothing outside
  the settings pane itself (documented limitation, not a regression).
FAIL if:
- Font-size actually does nothing you'd expect it not to do differently
  than documented — i.e. only report FAIL if it does something surprising
  (like crashing), not for the known no-op.

### Test 4 — Ingestion settings persist without a consuming pipeline (expected)
Steps:
1. Toggle symlink-following / hashing-eagerness in the Ingestion pane.
2. Restart the app.
Expected:
- Values persist through the real backend round-trip after restart (no scan
  pipeline reads them yet — expected, not a bug).
FAIL if:
- The values don't persist across restart.

### Test 5 — Target Planner altitude-threshold clamp
Steps:
1. In the Planner pane, try setting "usable altitude" outside 0–90.
2. Set a valid in-range value.
Expected:
- Out-of-range input clamps to the valid range. A valid value immediately
  affects the Targets planner view (see Journey 9's caveats about the
  site-gate for what "affects" currently means).
FAIL if:
- Out-of-range input is accepted as-is.

### Test 6 — Bottom log panel is a layout participant, not an overlay
Steps:
1. Expand the bottom log panel.
2. Filter by each severity chip (Error/Warn/Info/Debug); set the level to
   Debug.
3. Export the visible log window.
Expected:
- Expanding shrinks the main content area rather than covering it. Deep
  diagnostics only appear once the level is turned down to Debug. Export
  produces a JSON file matching only the currently filtered/visible rows.
FAIL if:
- The panel overlays content instead of sharing layout space, or export
  includes rows outside the current filter.

### Test 7 — 1100×720 pinned-header layout convention
Steps:
1. Resize the window to exactly 1100×720 (the documented minimum supported
   size).
2. Visit several pages (Sessions, Inbox, Projects, Targets) and scroll each
   page's content.
Expected:
- On every page, the header/action bar stays pinned while only the content
  area scrolls.
FAIL if:
- Any page's header scrolls out of view along with the content.

### Test 8 — Translated errors, never a raw code
Steps:
1. Trigger a backend error (e.g. attempt an invalid remap path in Data
   Sources, or another action you know will be rejected).
Expected:
- The error banner/toast shows a translated, human-readable message — never
  a raw backend error code (e.g. something like `E_INVALID_PATH`) or an
  English-only string leaking through untranslated in a non-English UI
  configuration if one is set up.
FAIL if:
- A raw error code or key leaks into the UI.

### Test 9 — Command palette (Ctrl+K) and keyboard-only navigation
Steps:
1. Press Ctrl+K, search for a real target or session by name.
2. Navigate to it using only the keyboard (no mouse).
Expected:
- Live backend-search results appear, and keyboard-only navigation reaches
  the result end to end.
FAIL if:
- Results don't reflect real backend data, or keyboard navigation gets
  stuck/requires the mouse.

### Test 10 — Sidebar collapse persists
Steps:
1. Collapse the left sidebar.
2. Reload the app.
Expected:
- The collapsed state persisted.
FAIL if:
- The sidebar resets to expanded after reload.

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot + exact
on-screen text / toast; for Test 7, note the exact window size used.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **`/settings` route renders without an uncaught error** — `automatable`,
  already covered by `all_top_level_screens_load`.
- **Everything else in this journey (pane grouping/auto-save, theme
  persistence, ingestion-settings persistence, altitude-threshold clamp, log
  panel layout/filter/export, 1100×720 convention, translated-error
  surfacing, command palette, sidebar persistence)** — all `automatable` in
  principle (deterministic UI state/persistence checks, no native OS
  dialogs involved) but **zero Layer-2 coverage and zero mock coverage
  today**. Flagged in the batched new-journey plan as **"Batch: Settings +
  layout-convention + i18n regression guard"** — lowest filesystem-mutation
  risk of the 10 journeys, but the layout-convention and no-raw-error-code
  checks are cross-cutting regression guards worth having cheaply once,
  since they protect every other journey's perceived trustworthiness too.
