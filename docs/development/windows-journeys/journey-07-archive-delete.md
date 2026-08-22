# Windows validation — Journey 7: Archive → (delete from archive)

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 7 (specs
  017/025, D15/D24).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `archive.plan.generate`, `plans.approve`,
  `plans.apply_real`, lifecycle transition to `archived`, trash/permanent
  delete commands.
- Changed surfaces: a completed project's Archive action, the review
  overlay, the **Archive** page (list, Send to trash, Delete permanently,
  Reveal).
- What this journey proves: archiving is the ONE legitimate way a project's
  lifecycle reaches `archived` and is refused without an applied plan first;
  after archive, the project's Edit pane is read-only; the Archive page shows
  real audit history with a deliberately narrower scope than a first guess
  (no Masters/Targets/Sessions tabs, no working Restore); permanent delete
  requires typing the literal word `DELETE`.
- Automated coverage baseline today: **this journey has NO Layer-2 coverage
  and no Playwright mock coverage at all** — confirmed by both
  `docs/development/verify-on-windows-journeys.md` ("Journeys 7, 9, and 10
  have no Layer-2 coverage at all") and
  `docs/development/e2e-mock-coverage-audit-2026-07-05.md` (Journey 7 row:
  "UNCOVERED"). The only automated signal touching this journey at all is
  the generic `all_top_level_screens_load` smoke test, which merely confirms
  the `/archive` route renders without an uncaught error — it asserts
  nothing about archive/trash/delete behavior. This document is currently
  the **only verification of any kind** for the archive/trash/permanent-
  delete flow, which is also the single highest-risk flow in the product
  (permanent, irreversible file deletion).

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
  `window.__TAURI__.core.invoke('<snake_command>', {args})`. Note: as of
  2026-07-05 no shipped UI button generates an archive plan yet — you may
  need to invoke `archive_plan_generate` directly via the bridge (see below)
  to reach a state the Archive UI action expects, then drive the rest by
  clicking.

## Preconditions
1. Deploy as above.
2. Have a project in the `completed` lifecycle state (drive it there via
   normal lifecycle transitions from Settings/project detail, or via
   `lifecycle.transition.apply` through the bridge if there is no UI path).
3. **CAUTION**: Tests 5–6 permanently move/delete real files. Use disposable
   test data only — never point this journey at real astrophotography data.

## Tests

### Test 1 — Archive refuses without an applied plan
Steps:
1. On the completed project, click **Archive**.
Expected:
- The action is refused (an explanatory message, not a silent lifecycle
  flip). The project's lifecycle state does not change.
FAIL if:
- The project silently flips to `archived` with no plan ever generated.

### Test 2 — Generate → review → approve → apply flips lifecycle
Steps:
1. Generate the archive plan (there is no shipped UI button for this as of
   2026-07-05 — invoke `archive_plan_generate` for this project via the
   Tauri MCP bridge, or use whatever UI entry point exists if one has since
   shipped).
2. Review the plan (protected items require the same acknowledgement gate as
   Cleanup); approve; apply.
3. Check the project's lifecycle state.
Expected:
- Files move into an app-managed archive folder
  (`.astro-plan-archive/<planId>/` under the project or library root — check
  in Explorer) and **only then** does the lifecycle flip to `archived`.
FAIL if:
- The lifecycle flips before the plan actually applies, or files don't
  appear at the expected archive path.

### Test 3 — Archived project's Edit pane is read-only
Steps:
1. Open the archived project's Edit pane.
Expected:
- It's read-only (no editable source/notes/manifest controls).
FAIL if:
- Any edit control is still interactive.

### Test 4 — Archive page lists real history with the documented narrower scope
Steps:
1. Open the **Archive** page.
Expected:
- The archived project is listed with real audit history (not placeholder
  rows). There is **no** Masters/Targets tab, **no** Sessions tab, and the
  **Restore** button is hidden or disabled (deferred by design, decision
  D15) — these are expected absences, not bugs.
FAIL if:
- The listed history is placeholder/fake data, or Restore actually works
  (would indicate an undocumented feature landed without this doc being
  updated — report as a finding either way).

### Test 5 — Send to trash uses the OS Recycle Bin
Steps:
1. On an archived project, click **Send to trash**.
2. Open the Windows Recycle Bin.
Expected:
- The files appear in the Recycle Bin (recoverable), not permanently gone.
FAIL if:
- Files are permanently deleted instead of moved to the Recycle Bin, or
  nothing happens.

### Test 6 — Permanent delete requires the literal word DELETE
Steps:
1. On a different archived project (disposable test data only), click
   **Delete permanently**.
2. Type a lowercase or partial "delete" first.
3. Type the exact literal `DELETE`.
Expected:
- Step 2: the confirm button stays disabled. Step 3: it enables and, on
  click, permanently removes the files.
FAIL if:
- The button enables on the wrong text, or nothing is actually removed after
  typing `DELETE` and confirming.

### Test 7 — Reveal uses the OS-native label
Steps:
1. On an archive row, look at the reveal control.
2. If there's nothing to reveal (e.g. after permanent delete), check its
   disabled state.
Expected:
- The label reads "Show in File Explorer" (Windows-native wording, not a
  generic "explorer" or "Reveal"); it's disabled when nothing exists to
  reveal.
FAIL if:
- A generic/non-native label is shown, or the control is clickable with
  nothing to reveal.

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- No UI button to generate the archive plan: this is a documented, known gap
  as of 2026-07-05 — use the Tauri MCP bridge to invoke
  `archive_plan_generate` directly; this is not a bug in your test run.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot + exact
on-screen text / toast. For Tests 5–6, confirm explicitly whether files were
recoverable (Recycle Bin) or permanently gone, since this is the highest-risk
journey in the product.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Everything in this journey** — `automatable` in principle (deterministic
  UI→IPC round-trips over `archive.plan.generate` / `plans.approve` /
  `plans.apply_real` / OS trash / permanent delete) but **zero Layer-2
  coverage and zero mock coverage today**. This is the single largest
  whole-stack gap in the product's automated test coverage, and also the
  highest product risk (irreversible deletion). Flagged in the batched
  new-journey plan as **"Batch: Archive lifecycle + trash + permanent
  delete"**, the top-priority new Layer-2 journey to author — but it shares
  the SAME blocking prerequisite as Journey 6: no channel-free apply command
  exists yet for archive plans (`plans.apply_real` needs a
  `tauri::ipc::Channel`). Recommend landing the channel-free apply path
  first (shared with Journey 6's gap), then authoring this journey
  immediately after, given the risk profile.
