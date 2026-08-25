# Windows validation — Journey 4: Sessions review (derived groupings, live membership)

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 4 (spec 041/045 —
  derived-inventory model; the prior session-lifecycle state machine was
  intentionally removed).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `sessions.list` (event-driven grouping
  after a plan applies), session notes update.
- Changed surfaces: **Sessions** page (list + detail), no review/approve
  controls by design.
- What this journey proves: Sessions is a read-only, always-current
  derived view over already-confirmed-and-applied inventory — never a
  place with its own Confirm/Re-open/Reject/Ignore gate — and rescanning
  never resurrects a review state or duplicates a session.
- Automated coverage baseline today: Layer-2 journey
  `ingestion_sessions_search` polls `sessions.list` until a real, resolved,
  grouped session appears after a plan applies (event-driven
  `plan_listener` → `ingest_light_frames`), then exercises
  `calibration.match.suggest` and `search.global` against it — a real
  IPC-level proof of the grouping pipeline. It does **not** assert the
  UI-level absence of review controls/pills, notes editing, or
  rescan-idempotency; the mock-Playwright suite covers `lifecycle_detail.spec.ts`
  (session rows render, detail opens) but not the "no review controls"
  assertion either (`docs/development/e2e-mock-coverage-audit-2026-07-05.md`).

## Windows environment mechanics (read once, applies to every Test below)

> Canonical mechanics: `docs/development/windows-native-rust-dev.md` §"Validation driving (MCP bridge, reset, recompile trap)". The steps below are the self-contained per-journey copy; reconcile to that doc if they drift.

- Windows checkout: `C:\dev\astro-plan`. Deploy: `git fetch origin`, then
  `git reset --hard origin/main` as its OWN command.
- **Recompile trap**: touch changed `.rs` files after a reset if Rust
  changed (`Get-ChildItem <files>.rs | ForEach-Object { $_.LastWriteTime = Get-Date }`);
  otherwise a hard refresh suffices.
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
2. This journey reuses state — do Journey 2 or 3's ingest→confirm→apply flow
   first (no fresh DB needed once that's done).
3. Sanity: Sessions is reachable from the left nav.

## Tests

### Test 1 — Nothing appears before a plan applies
Steps:
1. Before confirming/applying any inbox item, open **Sessions**.
Expected:
- Sessions shows nothing for that not-yet-applied data.
FAIL if:
- A session appears from raw, unreviewed scan data.

### Test 2 — Session appears automatically after apply, with real counts
Steps:
1. Complete Journey 2 or 3 (confirm + apply an inbox item).
2. Return to Sessions.
Expected:
- The corresponding session appears automatically, with counts matching what
  was actually moved/catalogued. There is no separate "review this session"
  step.
FAIL if:
- The session doesn't appear, or requires an extra approve/review action.

### Test 3 — No review-state controls or pills
Steps:
1. Open the session list and its detail panel.
Expected:
- **No** Confirm, Re-open, Reject, or Ignore controls anywhere, and no
  review-state pills (e.g. needs-review/candidate).
FAIL if:
- Any of those controls or pills appear — this would be a regression of the
  intentionally-removed session-lifecycle state machine.

### Test 4 — Notes edit doesn't trigger a lifecycle transition
Steps:
1. Open a session's detail, edit its Notes field, let it auto-save.
Expected:
- The edit saves without any reopen/re-confirm prompt or visible lifecycle
  transition.
FAIL if:
- Editing notes triggers any transition prompt or state change indicator.

### Test 5 — Rescan doesn't duplicate or resurrect review state
Steps:
1. Go to Inbox, Rescan (no new files added).
2. Return to Sessions.
Expected:
- No duplicate sessions appear, and no review state reappears.
FAIL if:
- A duplicate session appears, or any review-state UI resurfaces.

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- No session appears after apply: session grouping is event-driven and can
  take a moment — wait a few seconds and refresh before treating it as a
  failure; if it never appears, that IS a failure.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot + exact
on-screen text.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Event-driven session grouping after plan apply (`sessions.list` resolves
  with real `targetIds`)** — `automatable`, already covered by
  `ingestion_sessions_search`.
- **Absence of Confirm/Re-open/Reject/Ignore controls + no review pills,
  notes-edit-doesn't-transition, rescan-idempotency** — `automatable`
  (simple DOM-absence + no-op assertions) but **zero Layer-2 or mock
  coverage today**. Low implementation cost — flagged in the batched
  new-journey plan as **"Batch: Sessions derived-view invariants"**; a good
  candidate to fold into an extension of `ingestion_sessions_search` rather
  than a brand-new journey, since it already reaches a resolved session.
