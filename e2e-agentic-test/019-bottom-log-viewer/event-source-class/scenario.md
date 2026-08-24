# Windows validation — Log panel event-source class renders the real source (not the literal template)

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Change facts (context — you do not act on this section)
- Spec / feature: 019 bottom-log-viewer (FR-013 source enum), fixed as part of
  PR #386.
- Branch to test: `fix-019-003-regressions`.
- Touches Rust backend? no · Frontend only? yes.
- No new Tauri commands. Pre-existing commands used to generate log activity:
  `settings_update` (Advanced pane), `source_protection_set` (Data Sources
  pane).
- Changed surface: `apps/desktop/src/app/LogPanel.tsx`, the `LogEntryRow`
  sub-component's per-row `<span>` for the entry's source tag.
- **The bug**: the row's source `<span>` was missing template-literal
  backticks, so EVERY entry rendered the class attribute as the literal,
  un-interpolated string `alm-logpanel__event-source--{entry.source}` — the
  same literal text on every row regardless of the entry's actual source.
  The fix makes the class interpolate the real value, e.g.
  `alm-logpanel__event-source--settings` or
  `alm-logpanel__event-source--audit`.
- **Why this is invisible to plain looking at the screen**: the row's VISIBLE
  text (e.g. the word "settings" or "audit" printed in the row) was NEVER
  affected by this bug — only the CSS class ATTRIBUTE was wrong, and no CSS
  rule currently styles that modifier class (spec 019 does not call for
  per-source colors), so there is no color/visual difference either. **You
  cannot verify this fix by eye.** You must inspect the DOM (element
  inspector or the Tauri MCP bridge) and read the actual `class` attribute
  text on the source span.
- **Known gap, not a regression from this PR**: the log panel exposes a
  `sourceFilter` / `setSourceFilter` state in `LogPanelContext`, and the
  filtering logic honors it, but **no UI control currently sets it** — there
  is no "filter by source" chip/dropdown in the app today, only a LEVEL
  filter (`all` / `error` / `warn` / `info` / `debug`) and a debug-gated
  "Include diagnostics" toggle. Do not go looking for a source-filter
  control; it does not exist yet. This scenario verifies the class fix via
  DOM inspection of rows from at least two different real sources instead.

## Preconditions — get the app to the right state

1. Deploy the branch on the Windows checkout `C:\dev\astro-plan`:
   - `git fetch origin`
   - `git reset --hard origin/fix-019-003-regressions`   (own command)
   - Frontend-only change — no forced Rust recompile needed; a hard refresh
     (Ctrl+R) after launch is enough if the app was already running.
2. If you do not already have a running dev app with at least one registered
   source (Light frames or Project), start clean:
   - `Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force`
   - `New-Item -ItemType Directory -Force -Path 'C:\dev\astro-plan\test-data\raw-lights'`
3. Launch:
   `powershell.exe -NoProfile -Command "Start-Process -FilePath 'cmd.exe' -ArgumentList '/k','C:\dev\astro-plan\run-dev.bat' -WorkingDirectory 'C:\dev\astro-plan'"`
   Wait for the window (process `desktop_shell.exe`; Vite on `localhost:5173`).
4. If the app opens on `/setup` (fresh DB), complete first-run setup: on the
   "Source Folders" step, add `C:\dev\astro-plan\test-data\raw-lights` under
   **Light frames**; add the same folder under **Projects** too (reusing one
   folder for both is fine — the wizard only needs at least one path
   registered per required kind, it does not validate folder contents).
   Continue through the remaining steps with defaults until the app lands on
   its post-setup page (e.g. Sessions).
5. Sanity: the app renders normally (not a blank window), and the bottom of
   the window shows a collapsed log strip/bar (the log panel's closed-state
   header).

## Tests

### Test 1 — Opening the log panel expands full-width, closing restores layout
Steps:
1. Click the log panel's expand control at the bottom of the window (the
   small triangle/chevron control in the bottom bar, or the bar itself).
Expected:
- The panel expands across the full width of the bottom of the app window,
  taking vertical space from the content area above it (the workspace above
  visibly shrinks) rather than floating over it.
- Level filter chips appear: **All / Error / Warn / Info / Debug**, plus a
  Follow-tail toggle and an Export control in the panel header.
FAIL if:
- The panel overlays content instead of resizing the layout, or no chips /
  follow / export controls appear.
2. Press **Escape**.
Expected:
- The panel collapses back to the bottom strip.
FAIL if:
- Escape does nothing (panel stays open).
3. Re-open the panel for the remaining tests (click the expand control
   again).

### Test 2 — Generate a "settings"-source entry and inspect its class
Steps:
1. With the log panel still open at the bottom, navigate to **Settings**
   (left nav) → **Advanced** (under the "Application" group). The log panel
   should remain visible/open at the bottom while you navigate (US1: opening
   logs does not lose page context, and the panel stays open across
   navigation).
2. In the "Log level" section, change the **Log level** dropdown to a
   different value than it currently shows (e.g. from "Info" to "Warn"), then
   change it back to "Info" (two changes = two entries, extra safety margin).
3. Look at the log panel: a new row (or two) should appear at the top of the
   list (newest-first) with a visible source label reading **"settings"**.
4. Inspect the DOM for that row's source element: right-click it and choose
   "Inspect" if the app exposes a context menu / DevTools (WebView2 apps
   launched via `cargo tauri dev` often allow this), OR use the Tauri MCP
   bridge's `webview_find_element` / `webview_dom_snapshot` tool if you have
   bridge access (see Mechanics below) to read the row's `class` attribute.
Expected:
- The source `<span>` for this row has `class` containing both
  `alm-logpanel__event-source` AND `alm-logpanel__event-source--settings`
  (the literal word "settings" appended, matching the visible text).
- The class attribute does NOT contain the literal substring
  `{entry.source}` anywhere.
FAIL if:
- No new row appears (settings changes aren't being logged at all — a
  separate regression, report it), OR the class is literally
  `alm-logpanel__event-source--{entry.source}` (the bug this scenario
  exists to catch), OR the class omits the `--settings` modifier entirely.

### Test 3 — Generate an "audit"-source entry (via source protection override) and inspect its class
Steps:
1. Navigate to **Settings** → **Data Sources** (top of the "Library" group in
   the left settings nav).
2. Find the row for the Light frames (or Projects) source you registered in
   preconditions. Each registered root row has a protection Pill (e.g.
   "Protected") and an **"Override"** button.
3. Click **Override**. A protection-level control appears (Protected /
   Normal / Unprotected). Pick a different level than currently shown, then
   click **"Save override"**.
4. Look at the (still-open) log panel: a new row should appear with a
   visible source label reading **"audit"**.
5. Inspect that row's source `<span>` class attribute the same way as Test 2.
Expected:
- A new row appears with visible source text "audit".
- Its class contains `alm-logpanel__event-source` AND
  `alm-logpanel__event-source--audit`, and does NOT contain the literal
  `{entry.source}` substring.
FAIL if:
- No new row appears after saving the override, OR the class shows the
  un-interpolated literal, OR the modifier is wrong (e.g. still says
  `--settings` from the previous test's row instead of `--audit` — this
  would indicate the interpolation is stuck on a stale value rather than
  reading each row's own `entry.source`).

### Test 4 — Two different rows have two different, correctly-interpolated classes side by side
Steps:
1. With both the Test 2 ("settings") and Test 3 ("audit") rows visible in
   the log list at the same time (scroll if needed — newest-first order
   means the audit row should be above the settings row if done in the order
   above), inspect both rows' classes together.
Expected:
- The two rows have DIFFERENT modifier suffixes (`--settings` vs `--audit`)
  that each match their own visible source text. This proves the class is
  genuinely interpolated per-row from `entry.source`, not a single
  hardcoded/fallback value that happens to look right once.
FAIL if:
- Both rows show the same modifier class despite different visible source
  text, or either still shows the raw template literal.

### Test 5 — No visible styling change (expected, not a bug)
Steps:
1. Compare the visual appearance (color, font, background) of the "settings"
   row's source tag and the "audit" row's source tag.
Expected:
- They look IDENTICAL visually — same color/style. This is expected: spec
  019 does not define per-source colors, so the newly-correct class is a
  styling hook with no CSS rule attached yet, not a visible change. Confirm
  this is what you see so nobody mistakes "no visual difference" for "the
  fix didn't do anything."
FAIL if:
- (This step cannot really "fail" visually — just confirm you did NOT see a
  crash, broken layout, or missing text where the source tag should be.)

## Troubleshooting
- Blank window: restart the dev server; if still blank, run `pnpm install`
  in `C:\dev\astro-plan` with `$env:CI="true"`, relaunch.
- No new log rows appear after Test 2/3 actions: hard-refresh (Ctrl+R) once
  to confirm the log subscription is live, then repeat the action. If still
  no rows, this may indicate the settings/protection change didn't actually
  reach the backend — check for an error toast on the Settings page itself
  first (a failed save wouldn't be expected to log anything).
- **DOM inspection without a bridge**: if right-click → Inspect is
  unavailable in the packaged dev window, you likely need the Tauri MCP
  bridge. Launch with `cargo tauri dev --config src-tauri\tauri.dev.conf.json
  --features dev-tools`, `PV_MCP_BRIDGE_ENABLE=1`, and
  `PV_MCP_BRIDGE_BIND=0.0.0.0` (the feature compiles the bridge in, the first
  variable starts it, the second widens its `127.0.0.1:9223` bind so the gateway
  address reaches it), connect from WSL via
  `driver_session host=<gateway> port=9223` (gateway =
  `ip route show default | awk '{print $3}'`), then use
  `webview_dom_snapshot` or `webview_find_element` targeting
  `.alm-logpanel__event-source` elements, or `webview_execute_js` running:
  ```js
  Array.from(document.querySelectorAll('.alm-logpanel__event-source'))
    .map(el => el.className);
  ```
  to dump every row's class string at once.

## Report back
For each Test: PASS / FAIL + one line of what you saw, including the literal
class-attribute string(s) you read for Tests 2–4 (paste them verbatim — this
is the whole point of the regression check).
