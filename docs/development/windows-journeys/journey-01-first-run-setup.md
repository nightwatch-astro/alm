# Windows validation — Journey 1: First-run setup → data sources

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 1 (specs 003/016).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `roots.register`, `firstrun.*`,
  `sources.set_organization_state`, remap/disable/delete commands.
- Changed surfaces: the setup wizard (`/setup`, 5 steps: Source Folders →
  Processing Tools → Configuration → Confirm → Scan) and **Settings → Data
  Sources**.
- What this journey proves: a fresh install can register light/calibration/
  project/inbox folders, finish setup, and keep managing those folders
  (rescan/remap/disable/delete) without ever silently touching files on disk.
- Automated coverage baseline today: Layer-2 journey
  `first_run_resolve_create_project` (`crates/e2e-tests/tests/journeys.rs`)
  covers the wizard's first-run redirect + a real `target.resolve` +
  `projects.create`/`projects.list` round-trip — it does **not** drive the
  wizard's folder-adding UI (native picker) or any Data Sources card action.
  No Playwright mock-layer spec covers this journey at all (see
  `docs/development/e2e-mock-coverage-audit-2026-07-05.md` on
  `research/e2e-mock-coverage-audit`). Everything below except the bare
  redirect-to-`/setup` behavior is **manual-only today**.

## Windows environment mechanics (read once, applies to every Test below)

> Canonical mechanics: `docs/development/windows-native-rust-dev.md` §"Validation driving (MCP bridge, reset, recompile trap)". The steps below are the self-contained per-journey copy; reconcile to that doc if they drift.

- Windows checkout: `C:\dev\astro-plan` (separate from any WSL/Linux checkout
  — the app serves from THIS checkout only).
- Deploy: `cd C:\dev\astro-plan`, `git fetch origin`, then
  `git reset --hard origin/main` as its OWN command (a guard rejects it
  bundled with other git commands).
- **Recompile trap**: `git reset --hard` restores file content but leaves an
  OLD mtime, so cargo thinks nothing changed and skips recompiling — the app
  silently stays on the old binary (symptom: a command that IS in the code
  returns "not found"). If you deployed a branch with Rust changes, force a
  rebuild: `Get-ChildItem <changed-files>.rs | ForEach-Object { $_.LastWriteTime = Get-Date }`
  before relaunching. Frontend-only changes: a hard refresh (Ctrl+R) suffices.
- **Reset to a fresh first-run** (required for this journey):
  `Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force`. Clearing
  `localStorage` alone is NOT a reset — it causes a `/`↔`/setup` redirect
  loop. The DB is the first-run source of truth.
- Launch (detached, native Windows, never over `/mnt/c`):
  `powershell.exe -NoProfile -Command "Start-Process -FilePath 'cmd.exe' -ArgumentList '/k','C:\dev\astro-plan\run-dev.bat' -WorkingDirectory 'C:\dev\astro-plan'"`.
  App process = `desktop_shell.exe`; Vite on `localhost:5173`;
  `VITE_USE_MOCKS=false` (real backend). First launch after a reset
  recompiles Rust — allow a few minutes.
  Kill: `Get-Process desktop_shell,cargo | Stop-Process -Force`.
- Blank window (`#root` empty) recovery: restart the dev server; if still
  blank, run `pnpm install` in `C:\dev\astro-plan` with `$env:CI="true"`, then
  relaunch.

### Tauri MCP bridge (if driving the app programmatically instead of pure vision)
- Launch with the dev overlay so the bridge is live:
  `cargo tauri dev --config src-tauri\tauri.dev.conf.json --features dev-tools` (bridge needs the
  `dev-tools` feature; WebSocket on `127.0.0.1:9223`).
- Connect via `driver_session host=localhost port=9223` — WSL runs in
  mirrored networking mode (`networkingMode=mirrored` in `.wslconfig`), so
  `localhost` reaches Windows services directly; the old NAT gateway-IP
  lookup (`ip route show default`) is obsolete. Firewall already allows 9223.
- Prefer `webview_execute_js` → `window.__TAURI__.core.invoke('<snake_command>', {args})`
  to call a backend command directly; `ipc_execute_command` rejects many real
  commands.
- **This journey's native folder picker cannot be driven by the bridge.**
  Relaunch with `VITE_E2E=1` to expose stand-in controls:
  `data-testid="e2e-path-input-<kind>"` / `e2e-add-path-btn-<kind>`
  (`kind` ∈ `light_frames`, `calibration`, `project`, `inbox`). Set the path
  via the input's native value setter + dispatch an `input` event, THEN click
  the add button in a **separate** call so React commits the state.

## Preconditions
1. Deploy + reset the DB as above.
2. Launch the app and wait for the window.
3. Sanity: the app renders (not a blank window).

## Tests

### Test 1 — Fresh install lands on the setup wizard
Steps:
1. With a freshly reset DB, launch the app.
Expected:
- The window shows "Setup · Step 1 of 5" (Source Folders).
FAIL if:
- The app lands anywhere other than `/setup` (e.g. a blank Inbox).

### Test 2 — Add a Light frames folder via the native picker
Steps:
1. On Step 1, click "Add folder" under **Light frames** (required).
2. Pick any real folder in the OS file browser (or, if using the Tauri MCP
   bridge, use the `VITE_E2E=1` stand-in input/button for `light_frames`
   described above).
3. Choose **organized** or **unorganized** for that folder.
Expected:
- The folder appears as a card with the path and your organized/unorganized
  choice. Nothing is registered with the backend yet — this is a working
  buffer.
FAIL if:
- The picker doesn't open, the path silently drops, or the folder is already
  registered with the backend before Step 4 (Confirm).

### Test 3 — Confirm step registers and Scan runs
Steps:
1. Skip Steps 2–3 (Processing Tools, Configuration) using their skip/default
   controls.
2. Reach Step 4 (Confirm) → review the summary of every category you added.
3. Proceed to Step 5 (Scan).
Expected:
- Step 4 only shows a summary; no scan has started yet. Once you proceed,
  each registered folder scans and reaches a terminal state (including
  "0 items" for an empty folder); **Finish** enables only once every source
  is done.
FAIL if:
- A scan starts before you leave Step 4, or Finish enables while a source is
  still scanning.

### Test 4 — Finish lands on Inbox and the completion flag sticks
Steps:
1. Click Finish.
2. Fully quit and relaunch the app (no DB reset).
Expected:
- Finish lands on the Inbox page. Relaunching goes straight to Inbox, never
  back to `/setup`.
FAIL if:
- Relaunch re-shows the wizard.

### Test 5 — Data Sources: Rescan
Steps:
1. Go to **Settings → Data Sources**.
2. Click **Rescan** on the registered card.
Expected:
- The scan re-runs without re-prompting for a path.
FAIL if:
- Rescan asks for a path again, or errors.

### Test 6 — Data Sources: Remap is preview-then-apply, never mutates files
Steps:
1. Click **Remap** on a source card.
2. Paste a different, valid, existing path.
3. Click **Verify**.
4. Only after Verify succeeds, click **Apply remap**.
Expected:
- Verify samples files at the new path with no file movement. Only Apply
  remap persists the new path in PlateVault's record.
FAIL if:
- Any file on disk moved at any point, or Apply remap is clickable before a
  successful Verify.

### Test 7 — Data Sources: Disable is reversible, no confirm needed
Steps:
1. Click **Disable** on a source card.
2. Click the same control again to re-enable.
Expected:
- Disabling removes the source from Inbox scan/ingest but its history stays
  visible; re-enabling needs no confirmation dialog.
FAIL if:
- Disable requires a confirm step, or disabling hides prior history.

### Test 8 — Data Sources: Delete is registration-only and blocked with dependents
Steps:
1. Pick a source that is currently **offline** (or mark one offline) and has
   no dependent records (sessions/projects). Click **Delete**.
2. Separately, attempt Delete on a source that DOES have dependent records.
Expected:
- Step 1: a confirm appears; confirming un-registers the source. Files on
  disk are untouched (spot-check in Explorer).
- Step 2: the Delete button is blocked/disabled with an explanatory message.
FAIL if:
- Delete removes files from disk, or succeeds despite dependents.

### Test 9 — "Show in File Explorer" reveal
Steps:
1. On a source card, click "Show in File Explorer".
Expected:
- Windows Explorer opens at exactly that folder (not a parent directory).
FAIL if:
- No Explorer window opens, or it opens the wrong/parent folder.

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- A step/command behaves like an old build: confirm you touched changed
  `.rs` files after `git reset --hard` so cargo actually recompiled.
- Stuck in a `/`↔`/setup` redirect loop: you cleared `localStorage` instead of
  resetting the DB — delete `wizard-test.db*` and relaunch.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, capture a
screenshot and the exact on-screen text / toast.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Fresh-DB → `/setup` redirect, wizard steps 1–5 default path, target
  resolve, project create** — `automatable`, already covered by
  `first_run_resolve_create_project`.
- **Adding a folder via the native OS picker** — `manual only` (native OS
  dialog; the Layer-2 harness's `VITE_E2E=1` stand-in already substitutes for
  it in automated runs, so this Test is really validating the *real* OS
  picker wiring, which the harness cannot do).
- **Data Sources Rescan / Remap (verify-then-apply) / Disable / Delete /
  Reveal** — all `automatable` at the IPC-round-trip level (each is a
  `roots.*` / `sources.*` command), currently **not exercised by any Layer-2
  journey**. See the batched new-journey plan (item 4 of the audit) — this is
  flagged there as **"Batch: Data Sources lifecycle ops"**, using the
  `VITE_E2E=1` stand-in for the one native-picker step it still needs, or
  seeding the root via a real `roots.register` invoke and driving only the
  card actions through the UI.
