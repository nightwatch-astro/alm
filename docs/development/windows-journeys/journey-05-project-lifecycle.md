# Windows validation — Journey 5: Project lifecycle (create → attach → manifests/notes → tool launch → artifacts)

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 5 (specs 008, 009,
  012, 024).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `projects.create`,
  `lifecycle.transition.apply`, `lifecycle.ledger.list`, artifact watcher
  commands, tool-launch spawn.
- Changed surfaces: `/projects/new`, project Edit pane, manifests/notes UI,
  "Open in {tool}" launch button, artifact list.
- What this journey proves: project creation is duplicate-name-safe and
  creates real on-disk folders under the registered project library (not the
  app's working directory — PR #414); attach/remove sources is
  confirmed-sessions-only with a last-source removal guard; manifests are
  append-only; tool launch is containment-checked; the artifact watcher only
  observes the project's own output folder.
- Automated coverage baseline today: Layer-2 journey `lifecycle_integrity`
  drives a real `projects.create` (asserting the sourceless project starts
  `setup_incomplete`), a real `lifecycle.transition.apply` round-trip
  (accepting `success`/`noop`/`error` as all valid, with a required real
  error shape on refusal), and a real `lifecycle.ledger.list` read proving a
  durable ledger row exists. It does **not** drive the create-wizard UI,
  source attach/remove, manifests/notes, tool launch, or the artifact
  watcher. The mock-Playwright suite covers only the transition **button** →
  success-toast case (`lifecycle_transitions.spec.ts`); the post-transition
  pill re-render test is intentionally `test.skip`'d there pending
  real-backend coverage, which this manual journey (or a future Layer-2
  journey) is the closest thing to today.

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
1. Deploy as above.
2. Complete Journeys 2/3 so at least one confirmed session exists to attach.
3. In Settings → Tools, configure a processing-tool executable path (point at
   any real `.exe`, e.g. `notepad.exe`, for the launch-spawn test).
4. Note your registered **project library** root path (from Settings → Data
   Sources) — you'll verify new folders land under it, not the app's working
   directory.

## Tests

### Test 1 — Duplicate project name blocks creation with an inline error
Steps:
1. Go to `/projects/new`, type a name that already exists (any case).
Expected:
- An inline field error appears immediately; creation is blocked from that
  step (no generic toast instead).
FAIL if:
- A generic toast appears instead of an inline field error, or creation
  proceeds.

### Test 2 — Create with a unique name creates real folders in the right place
Steps:
1. Create a project with a unique name.
2. In Explorer, navigate to your registered project library root, then into
   the new project's folder.
Expected:
- A toast confirms creation. `lights/`, `darks/`, `flats/` (or equivalent)
  subfolders exist **under the registered project library root**, not under
  the app's own working directory.
FAIL if:
- The folders are missing, or they exist under the wrong root (e.g. next to
  the `desktop_shell.exe` binary).

### Test 3 — Attach sources: unlinked-confirmed-only, last-source guard
Steps:
1. Open the project's Edit pane, click **Add sources**.
2. Confirm the picker only lists unlinked, already-confirmed sessions (not
   unconfirmed inbox data).
3. Attach one, then try to remove it if it's the last remaining source.
Expected:
- Step 2: unconfirmed inbox data never appears in the picker. Step 3: an
  inline confirm reads something like "You can't remove the last confirmed
  source."
FAIL if:
- Unconfirmed data appears in the picker, or the last source removes without
  any guard.

### Test 4 — Per-channel integration time shows real numbers
Steps:
1. With at least one attached session, view the project detail's per-channel
   (per-filter) breakdown.
Expected:
- Real sub-frame counts and total integration time in hours/minutes, not a
  placeholder dash.
FAIL if:
- The breakdown shows a dash/placeholder despite attached data.

### Test 5 — Manifests and notes are append-only / auto-save
Steps:
1. Make a lifecycle-relevant change (e.g. change attached sources).
2. Check the manifests list for a new snapshot.
3. Type into Notes, stop typing, wait a few seconds.
Expected:
- A new manifest snapshot appears (prior ones are never overwritten). Notes
  auto-save with a live byte counter; no manual Save button exists anywhere
  on the page.
FAIL if:
- A manifest is overwritten instead of appended, or notes require a manual
  Save action.

### Test 6 — Tool launch spawns and is containment-checked
Steps:
1. Click "Open in {tool}" with a valid configured executable.
2. Check Windows Task Manager for the new process.
3. If reachable, configure a working directory that resolves outside every
   registered root, and try launching again.
Expected:
- Step 2: the configured executable actually spawns as a new process. Step
  3: launch refuses with a plain message (not a silent no-op), and the
  project's lifecycle state is untouched by either outcome.
FAIL if:
- Nothing spawns in Task Manager, or an out-of-root launch silently succeeds
  or silently does nothing without a message.

### Test 7 — Artifact watcher observes only this project's output folder
Steps:
1. With the project open, drop a new file into its output folder.
2. Confirm it's picked up and listed as an artifact with a kind/confidence.
3. Close the project, drop another file into its output folder while closed,
   then reopen the project.
Expected:
- Step 2: the artifact appears while the project is open. Step 3: the
  while-closed file is picked up the next time the project reopens.
  PlateVault never modifies or deletes the artifact file itself.
FAIL if:
- The while-open file never appears, or the while-closed file is never
  picked up on reopen, or the artifact file itself gets modified/deleted.

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- Folders land in the wrong place: confirm your registered project library
  root in Settings → Data Sources before blaming the app — PR #414 fixed the
  wrong-location bug; if you still see it on `main`, that's a real
  regression, report it as FAIL.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot + exact
on-screen text / toast, and (for Test 2) the actual Explorer path where
folders landed.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Sourceless project starts `setup_incomplete`, real
  `lifecycle.transition.apply` round-trip (success/noop/error all
  well-formed), real `lifecycle.ledger.list` durable row** —
  `automatable`, already covered by `lifecycle_integrity`.
- **Create-wizard duplicate-name inline error, folder-location correctness,
  attach/remove sources UX, per-channel integration time, manifests/notes
  UI, tool-launch spawn + containment, artifact watcher** — all
  `automatable` in principle but **zero Layer-2 coverage today** (and only
  the single transition-button case has mock-Playwright coverage). Flagged
  in the batched new-journey plan as **"Batch: Project lifecycle UI
  surface"** — moderate priority; the tool-launch-spawn and artifact-watcher
  parts specifically need a real process/filesystem watcher, which only
  Layer-2 (not the mock layer) can prove.
