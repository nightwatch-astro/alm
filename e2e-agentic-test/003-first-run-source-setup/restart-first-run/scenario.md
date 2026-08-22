# Windows validation — "Restart first-run setup" control in Settings › Advanced

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Change facts (context — you do not act on this section)
- Spec / feature: 003 first-run-source-setup, User Story 3 "Restart Setup From
  Settings" (P3), FR-013, FR-016, SC-004.
- Branch to test: `fix-019-003-regressions` (PR #386).
- Touches Rust backend? no (the `firstrun.restart` Tauri command already
  existed and is unchanged) · Frontend only? yes.
- Tauri command exercised (pre-existing, newly wired to a UI caller):
  `firstrun_restart(request: { confirm: bool })` → returns
  `{ restartedAt, prefilledSources: RegisterSourceResponse[] }` on success, or
  a `ContractError` (e.g. if `confirm` is not `true`, or on a DB failure).
- Changed surfaces: Settings → Advanced pane
  (`apps/desktop/src/features/settings/Advanced.tsx`).
- What changed for the user: Advanced settings previously had exactly ONE
  "Restart" control — "Restart guided flow" (spec 010's guided product tour:
  a coach-mark walkthrough). There was no way to reopen the initial
  Raw/Calibration/Project/Inbox **source setup wizard** from the UI, even
  though the backend command for it already existed. This PR adds a second,
  distinctly-labeled, confirm-gated "Restart first-run setup" control in its
  own "Source Setup Wizard" section. Confirming it reopens the `/setup`
  wizard with your **currently registered sources pre-filled** into its
  working buffer for editing (nothing is deleted), clears the setup-completed
  flag, and navigates to `/setup`.

## Important note on wizard step count
The spec text describes an aspirational 8-step wizard (Welcome → Raw →
Calibration → Project → Inbox → Detect Tools → Download Catalogs → Finish).
**That refactor has not shipped.** The wizard actually running on the real app
today is 5 steps, and step 1 ("Source Folders") shows all four categories
(Light frames / Calibration frames / Projects / Inbox) together as separate
compact "cards" on ONE page, not as four separate wizard pages. The stepper
text reads **"Setup · Step 1 of 5"** on that first page. Use the ACTUAL app
behavior (5 steps, unified Source Folders page) as ground truth for this
scenario, not the 8-step description — if you observe something different
from "Setup · Step 1 of 5" with one unified page, note it as a discrepancy in
your report rather than assuming you did something wrong.

## Preconditions — get the app to the right state

1. Deploy the branch on the Windows checkout `C:\dev\astro-plan`:
   - `git fetch origin`
   - `git reset --hard origin/fix-019-003-regressions`   (run as its OWN command)
   - This change is frontend-only — no Rust files changed, so no forced
     recompile is required (a hard refresh / dev-server relaunch is enough).
2. Reset to a clean first-run state (the DB is the source of truth for
   first-run gating — clearing localStorage alone is not sufficient and can
   cause a `/` ↔ `/setup` redirect loop):
   - `Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force`
3. Create two throwaway folders to register as sources during setup (any
   empty folders work — the wizard does not scan or require real FITS files
   to register a root):
   - `New-Item -ItemType Directory -Force -Path 'C:\dev\astro-plan\test-data\raw-lights'`
   - `New-Item -ItemType Directory -Force -Path 'C:\dev\astro-plan\test-data\project-1'`
4. Launch the dev app (detached, native Windows — never over `/mnt/c`):
   - `powershell.exe -NoProfile -Command "Start-Process -FilePath 'cmd.exe' -ArgumentList '/k','C:\dev\astro-plan\run-dev.bat' -WorkingDirectory 'C:\dev\astro-plan'"`
   - Wait for the window. App process is `desktop_shell.exe`; Vite serves
     `localhost:5173`. `run-dev.bat` sets `PV_DB_URL` to the
     `wizard-test.db` file above.
5. Sanity: the app renders (not a blank window). Since the DB is fresh, it
   should land directly on the `/setup` wizard, Welcome/Source-Folders step,
   NOT on `/sessions`. If it shows a blank `#root`, see Troubleshooting.
6. Complete first-run setup ONCE, to establish the "already set up, sources
   registered" precondition this scenario needs:
   a. On the "Source Folders" step (Step 1 of 5), find the **Light frames**
      card and click its "Add folder" button. In the native folder picker,
      navigate to and select `C:\dev\astro-plan\test-data\raw-lights`.
      Confirm the folder now appears listed under Light frames.
   b. Find the **Projects** card and click its "Add folder" button. Select
      `C:\dev\astro-plan\test-data\project-1`. Confirm it appears listed
      under Projects.
   c. Leave Calibration and Inbox empty (both optional).
   d. Click **Continue** through the remaining steps (Processing Tools,
      Configuration, Confirm) using default/stub values, then click the
      final action to finish (label is "Start scan →" or similar on the
      Confirm step). Wait for the app to navigate away from `/setup` (it
      lands on Sessions or similar).
7. Navigate to **Settings** (left nav) → under the "Application" group click
   **Advanced**.
8. Sanity: the Advanced pane renders with a "Database" section and a "Log
   level" section at minimum (not a blank pane).

## Tests

### Test 1 — Both restart controls are visible and clearly distinct
Steps:
1. On Settings → Advanced, scroll through the pane and locate every section
   whose heading or button mentions "restart".
Expected:
- There are exactly TWO distinct restart controls:
  1. A section about the guided tour (heading mentions "Guided Tour") with a
     button labeled **"Restart guided flow"**, and body text describing
     whether the guided flow is active/dismissed/completed.
  2. A separate section titled **"Source Setup Wizard"** with a button
     labeled **"Restart first-run setup"**, and description text that reads
     (or closely paraphrases): "Reopens the initial source setup wizard so
     you can add, remove, or correct your Raw, Calibration, Project, and
     Inbox folders. This is different from the guided tour above — it edits
     which folders the library uses, not the walkthrough. Your currently
     registered sources are pre-filled for editing."
- The two buttons are visually and textually distinguishable — a user could
  not reasonably click one thinking it was the other.
FAIL if:
- Only one restart control exists, OR the "Restart first-run setup" button
  is missing, OR its section/description text conflates it with the guided
  tour (e.g. reuses "guided" language), OR the two sections are not clearly
  separated.

### Test 2 — Clicking "Restart first-run setup" shows a danger-styled confirm gate, no side effects yet
Steps:
1. Click the **"Restart first-run setup"** button.
Expected:
- The button disappears and is replaced, in the same section, by an inline
  danger-styled box containing:
  - Description text: "This reopens the source setup wizard and clears its
    completed status. Your existing sources will be pre-filled for you to
    review and adjust — nothing is deleted."
  - A red/danger-styled button labeled **"Yes, restart setup"**.
  - A plain/ghost button labeled **"Cancel"**.
- The app is still on the Settings → Advanced page. No navigation occurred.
FAIL if:
- Clicking the button navigates away immediately (no confirm step), OR no
  confirm box appears, OR the confirm box's wording talks about the guided
  tour instead of the source setup wizard.

### Test 3 — Cancel reverts cleanly, no backend call, no navigation
Steps:
1. From the confirm box shown in Test 2, click **Cancel**.
Expected:
- The confirm box disappears; the pane reverts to showing the plain
  **"Restart first-run setup"** button again.
- The app remains on Settings → Advanced (no navigation to `/setup`).
- No error message appears.
FAIL if:
- The app navigates to `/setup` anyway, OR an error appears, OR the button
  is missing/disabled after cancelling (it should be immediately clickable
  again).

### Test 4 — Confirm restarts the wizard, clears completion, and PRE-FILLS previously registered sources
Steps:
1. Click **"Restart first-run setup"** again, then click **"Yes, restart
   setup"** in the confirm box.
2. Observe the button briefly read "Restarting…" (may be too fast to catch —
   not a failure if you miss it).
3. Wait for navigation.
4. On the page you land on, confirm the stepper reads **"Setup · Step 1 of
   5"** and the heading is the Source Folders step ("Where does your data
   live?").
5. Look at the **Light frames** card and the **Projects** card on this page.
Expected:
- The app navigates to `/setup` in well under a second (SC-004) — this
  should feel instantaneous, not require a spinner wait.
- The wizard opens on Step 1 of 5 (Source Folders), NOT the middle of a
  stale in-progress run.
- The **Light frames** card already lists `C:\dev\astro-plan\test-data\raw-lights`
  (the folder you registered in precondition step 6a) — it is NOT empty.
- The **Projects** card already lists `C:\dev\astro-plan\test-data\project-1`
  (from precondition step 6b) — it is NOT empty.
- Calibration and Inbox cards remain empty (matching what you left them as).
- Nothing was deleted: the original registered sources are still intact and
  editable (you could remove/re-add rows, add a scan-depth override via the
  row's advanced expander, etc.) — you do not need to actually edit them,
  just confirm they are present and the row's "remove" control is available.
FAIL if:
- The wizard opens with EMPTY source cards (this is the core regression this
  scenario protects against — the prefill silently failing would look like
  the wizard forgot everything), OR the wizard resumes mid-flow at a later
  step instead of Step 1, OR navigation does not happen / hangs for more
  than ~2 seconds, OR the app crashes or shows a blank page.
6. Optional deeper check: without finishing the wizard again, navigate the
   address/URL to the app's root route (if the shell exposes a "Home" nav
   item, click it; otherwise use whatever back-to-root affordance the shell
   provides). Confirm you are redirected BACK to `/setup` rather than to
   Sessions — this proves the completion flag was actually cleared, not just
   the sources prefilled.
   FAIL if: the root route resolves to Sessions/another main page instead of
   redirecting to `/setup`.
7. Clean-up: finish the wizard again (Continue through the remaining steps
   with the pre-filled sources) so the app returns to a normal
   "setup-completed" state before Test 5.

### Test 5 — Error path: `firstrun.restart` failing shows an inline error, no navigation, no side effects, and is retryable
This test simulates the backend command failing (e.g. a transient DB error)
without needing to break the real database. It requires driving the app's
webview via the Tauri MCP bridge's JS execution (see "Tauri MCP bridge" below
in Troubleshooting/Mechanics). If you do NOT have bridge access in this
environment, skip this test and report it as "SKIPPED — no bridge access" —
do not attempt to guess a DB-corruption method, as file-level tampering can
leave the app in a state Test 4's clean-up can't recover from.

Steps (via bridge):
1. Connect to the app's webview via `webview_execute_js` (see mechanics
   below) and run this JS to force ONLY the `firstrun_restart` IPC call to
   reject, leaving every other command working normally:
   ```js
   (function () {
     const core = window.__TAURI__.core;
     if (core.__origInvoke) return 'already patched';
     core.__origInvoke = core.invoke;
     core.invoke = function (cmd, args) {
       if (cmd === 'firstrun_restart') {
         return Promise.reject('database unavailable');
       }
       return core.__origInvoke(cmd, args);
     };
     return 'patched';
   })();
   ```
2. Back in the app UI, navigate to Settings → Advanced (if not already
   there). Click **"Restart first-run setup"**, then click **"Yes, restart
   setup"**.
Expected:
- The app stays on Settings → Advanced (no navigation to `/setup`).
- An inline error appears with `role="alert"` reading: **"Could not restart
  first-run setup: database unavailable"**.
- The confirm box (with "Yes, restart setup" / "Cancel") is still visible and
  both buttons are clickable again (not stuck disabled) — this proves the
  in-flight flag was reset and a retry is possible.
- Nothing was prefilled and no navigation occurred: this is a true no-op
  other than the visible error text.
3. Restore normal behavior via the bridge:
   ```js
   (function () {
     const core = window.__TAURI__.core;
     if (core.__origInvoke) { core.invoke = core.__origInvoke; delete core.__origInvoke; return 'restored'; }
     return 'nothing to restore';
   })();
   ```
4. Click **"Yes, restart setup"** again (the confirm box should still be
   open from step 2 above; if not, click "Restart first-run setup" once
   more) and confirm this time it succeeds normally — navigates to `/setup`
   with sources pre-filled as in Test 4. This proves recovery works without
   an app restart.
5. Clean-up: finish the wizard again so the app returns to "setup-completed".
FAIL if:
- The error text differs from the exact copy above, OR the app navigates
  away despite the failure, OR the confirm/cancel buttons remain
  disabled/stuck after the failure (no retry possible), OR after restoring
  normal behavior the retry still fails.

## Troubleshooting
- Blank window (empty content): restart the dev server; if still blank, run
  `pnpm install` in `C:\dev\astro-plan` with `$env:CI="true"`, then relaunch.
- Native folder picker won't accept the throwaway folders: confirm they exist
  (`Test-Path 'C:\dev\astro-plan\test-data\raw-lights'`) — create them per
  precondition step 3 if missing.
- Stuck in a `/` ↔ `/setup` redirect loop: this means only localStorage was
  cleared, not the DB — delete `wizard-test.db*` and relaunch (see
  Preconditions step 2).
- **Tauri MCP bridge** (needed only for Test 5): launch with the dev overlay
  enabled (`cargo tauri dev --config src-tauri\tauri.dev.conf.json
  --features dev-tools` plus `PV_MCP_BRIDGE_ENABLE=1`, without which the bridge
  does not start; WS on `127.0.0.1:9223`, so a gateway connection also needs
  `PV_MCP_BRIDGE_BIND=0.0.0.0` at launch), connect from WSL via
  `driver_session host=<gateway> port=9223` where
  `gateway = ip route show default | awk '{print $3}'`; then use
  `webview_execute_js` to run the JS snippets in Test 5. If you are a
  pure-vision computer-use agent without bridge/MCP tool access, Test 5
  cannot be performed — report it as SKIPPED rather than improvising a DB
  file lock, which risks leaving the DB in a bad state for later scenarios.

## Report back
For each Test: PASS / FAIL / SKIPPED + one line of what you saw. On any FAIL,
capture a screenshot and the exact on-screen text / any inline error.
