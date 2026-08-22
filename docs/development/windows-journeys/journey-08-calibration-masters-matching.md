# Windows validation — Journey 8: Calibration: ingest cal frames → masters → matching

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 8 (spec 040
  calibration master detection, spec 007 calibration matching).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `calibration_master_detect`,
  `confirm_master_integration`, `calibration.match.suggest`, assignment
  commands, Settings → Calibration "Offset tolerance" persistence.
- Changed surfaces: Inbox (masters ingest as individual items), **Calibration**
  page (one row per master, kind-conditional fingerprint columns, matching
  view), Settings → Calibration.
- What this journey proves: a folder with several master files (darks,
  flat, bias) classifies as separate individual items (not one folder
  aggregate); the Calibration page shows kind-conditional columns (a bias's
  temperature/gain show as a dash by design); matching surfaces ranked
  candidate sessions with real context and flags hard-rule mismatches rather
  than hiding them; assignment is advisory/confirmable, never auto-applied;
  the offset-tolerance setting persists and immediately affects matching.
- Automated coverage baseline today: Layer-2 journey
  `ingestion_sessions_search` calls the real `calibration.match.suggest`
  command and asserts a well-formed response shape (candidates may
  legitimately be empty since no masters were seeded in that journey's
  fixture) — it does **not** exercise the master-ingest pipeline itself, the
  Calibration page's UI, kind-conditional columns, assign/cancel, or the
  offset-tolerance setting. No Playwright mock spec covers any part of this
  journey (`docs/development/e2e-mock-coverage-audit-2026-07-05.md`, Batch 4
  — 7 vitest files exist under `features/calibration/` but no Playwright
  e2e).

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
2. Register a calibration root. Prepare files: 2 dark masters (different
   temperature/gain), 1 flat master, 1 bias master, plus light frames whose
   gain matches one dark and mismatches the other.
3. Sanity: Calibration page is reachable from the left nav.

## Tests

### Test 1 — Masters ingest as individual items, not a folder aggregate
Steps:
1. Inbox → Rescan the calibration folder.
Expected:
- Each master file appears as its own item with its own fingerprint
  (gain/temperature/binning/filter where relevant) — not one aggregate row
  for the whole folder.
FAIL if:
- The folder collapses into a single ambiguous item.

### Test 2 — Confirm + apply registers masters with kind-conditional columns
Steps:
1. Confirm and apply the master items.
2. Open the **Calibration** page.
Expected:
- One row per master file. A bias's temperature/gain columns show a dash
  (by design, not a bug) since they don't apply to that kind. No master
  *light* frame appears on this page.
FAIL if:
- A master light frame appears here, or dash-by-design columns instead show
  fabricated-looking numbers.

### Test 3 — Matching surfaces ranked candidates with real context
Steps:
1. Select a master (e.g. one of the darks) and open its matching view.
Expected:
- Ranked candidate sessions appear with real context: target, filter, night,
  frame count (not opaque ids). The session whose gain mismatches shows a
  mismatch indicator rather than being silently hidden.
FAIL if:
- Candidates show opaque ids only, or the mismatched session is silently
  omitted instead of flagged.

### Test 4 — Assignment is advisory and confirmable
Steps:
1. Start assigning a master to a session, then click Cancel.
2. Check the bottom log panel / audit trail for any new entry from the
   cancel.
3. Start again and confirm the assignment.
Expected:
- Step 1–2: cancelling fires no backend call (no new audit/log entry).
  Step 3: confirming records the assignment and its usage count increments.
FAIL if:
- Cancel still creates a backend record, or confirming doesn't record a
  usage count.

### Test 5 — Offset tolerance persists and immediately affects matching
Steps:
1. Go to Settings → Calibration, change "Offset tolerance".
2. Restart the app (relaunch, no DB reset).
3. Return to the matching view for the same master.
Expected:
- The new tolerance value persisted across restart, and the matching view
  immediately reflects it (a previously-mismatched session may now show
  clean, or vice versa, depending on direction of change).
FAIL if:
- The value resets after restart, or the matching view doesn't reflect the
  change.

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- No candidate sessions appear at all: confirm light frames were actually
  ingested and applied (Journey 2) before opening the matching view.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot + exact
on-screen text.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **`calibration.match.suggest` real round-trip (well-formed response)** —
  `automatable`, already covered (partially) by `ingestion_sessions_search`.
- **Masters-as-individual-items ingest, Calibration page kind-conditional
  columns, ranked matching UI with mismatch indicator, assign/cancel,
  offset-tolerance persistence + immediate effect** — all `automatable` but
  **zero Layer-2 coverage today** (also zero mock coverage). Flagged in the
  batched new-journey plan as **"Batch: Calibration masters ingest +
  matching UI"** — moderate-high priority since spec 040 shipped without a
  `plan.md`/`tasks.md` (a documented artifact deviation) and has the least
  automated scrutiny of any recently-shipped backend feature.
