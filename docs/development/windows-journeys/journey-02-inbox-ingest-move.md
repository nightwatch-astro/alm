# Windows validation — Journey 2: Ingest → review/reclassify → confirm (move mode)

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 2 (spec 041).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `inbox.scan.folder`, `inbox.classify`,
  `inbox.confirm`, `inbox.plan.apply`, `plans.apply.status`.
- Changed surfaces: **Inbox** page (queue, needs-review banners/badges, bulk
  reclassify control, root-picker prompt, plan review overlay).
- What this journey proves: a folder of mixed frame types splits into clean
  single-type items, missing-metadata items are gated from confirming until
  fixed, confirming only ever creates a reviewable plan (never moves a file
  by itself), and applying a plan actually moves files to the resolved
  destination path with a stale-plan refusal if the source changed underneath
  it.
- Automated coverage baseline today: Layer-2 journeys
  `plan_review_apply_with_audit` (register → scan → classify → confirm →
  apply → poll `plans.apply.status` until `applied` → assert the source file
  moved) and `ingestion_sessions_search` (same pipeline then session
  grouping) both drive this journey's REAL backend round-trip, but through
  the `window.__PV_E2E__.invoke` bridge, not by clicking through the actual
  Inbox UI. Mixed-folder splitting, the needs-review banner/badges, the bulk
  reclassify control, the root-picker prompt, and the stale-plan-refusal UI
  are **not exercised by any Layer-2 journey nor any Playwright mock spec**
  (`docs/development/e2e-mock-coverage-audit-2026-07-05.md`, Batch 1) — this
  document's UI-level Tests are today's only coverage for those.

## Windows environment mechanics (read once, applies to every Test below)

> Canonical mechanics: `docs/development/windows-native-rust-dev.md` §"Validation driving (MCP bridge, reset, recompile trap)". The steps below are the self-contained per-journey copy; reconcile to that doc if they drift.

- Windows checkout: `C:\dev\astro-plan`. Deploy: `git fetch origin`, then
  `git reset --hard origin/main` as its OWN command.
- **Recompile trap**: after a reset, cargo may skip rebuilding because the
  mtime looks unchanged — if you deployed Rust changes, touch the changed
  `.rs` files (`Get-ChildItem <files>.rs | ForEach-Object { $_.LastWriteTime = Get-Date }`)
  before relaunching. Frontend-only: a hard refresh suffices.
- Reset to fresh first-run when a Test needs it:
  `Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force` (DB is the
  first-run source of truth; clearing `localStorage` alone causes a redirect
  loop).
- Launch: `powershell.exe -NoProfile -Command "Start-Process -FilePath 'cmd.exe' -ArgumentList '/k','C:\dev\astro-plan\run-dev.bat' -WorkingDirectory 'C:\dev\astro-plan'"`.
  Kill: `Get-Process desktop_shell,cargo | Stop-Process -Force`.
- Blank window recovery: restart the dev server; if still blank,
  `pnpm install` with `$env:CI="true"`, relaunch.
- Tauri MCP bridge (optional, for programmatic driving): launch with
  `cargo tauri dev --config src-tauri\tauri.dev.conf.json --features dev-tools` (bridge on
  `127.0.0.1:9223`), connect with `driver_session host=localhost port=9223`,
  invoke commands via `webview_execute_js` →
  `window.__TAURI__.core.invoke('<snake_command>', {args})`. Native folder
  pickers still need `VITE_E2E=1` stand-ins (see Journey 1's doc for the
  exact mechanism) if you need to add roots this way instead of by hand.

## Preconditions
1. Deploy as above.
2. Ensure setup is complete with one inbox root and one light-frames root
   registered (run Journey 1's wizard once, or reuse an already-set-up DB).
3. Prepare test files: drop a folder containing **mixed frame types** (e.g.
   two light frames + a dark frame) into the inbox folder on disk. Separately
   prepare one FITS/XISF file with a **missing mandatory field** (no filter
   AND no target/coordinates) for Test 2.
4. Sanity: the app renders and Inbox is reachable from the left nav.

## Tests

### Test 1 — Mixed folder splits into single-type items
Steps:
1. On **Inbox**, click **Rescan**.
Expected:
- The mixed folder materializes as multiple **single-type** items (e.g.
  `light · Ha · 300s`, `dark · 300s`), each still visibly grouped back to its
  shared source folder. The status-bar breakdown matches the real contents.
FAIL if:
- It shows one ambiguous "mixed" row instead of split single-type items.

### Test 2 — Needs-review gate blocks Confirm
Steps:
1. Rescan after dropping the file with a missing mandatory field.
Expected:
- A danger banner names exactly what's missing (e.g. "missing filter"),
  affected rows show a "needs `<attribute>`" badge, and **Confirm** is
  disabled on that item.
FAIL if:
- Confirm is clickable, or the banner is generic/doesn't name the missing
  field.

### Test 3 — Bulk reclassify resolves the gate
Steps:
1. Select the affected row(s) from Test 2.
2. Open the bulk reclassify control, set the missing value (frame type,
   filter, exposure, or binning), apply to selection.
3. Rescan again.
Expected:
- The item re-partitions into a clean single-type item, Confirm re-enables,
  and the override survives the rescan (doesn't revert).
FAIL if:
- Confirm stays disabled, or the rescan reverts the override.

### Test 4 — Root-picker prompt vs. auto-select
Steps:
1. If you have 2+ registered light-frame roots, click **Confirm** on a light
   item.
2. If you only have exactly one valid root, click **Confirm** on a light item
   instead.
Expected:
- Multiple valid roots: a root-picker prompt appears before a plan is
  generated. Exactly one valid root: no prompt, it's auto-chosen.
FAIL if:
- The behavior is reversed (prompts with one root, or silently picks with
  multiple).

### Test 5 — Confirm never moves a file by itself
Steps:
1. Click **Confirm** on a classified item.
2. Check the source file's location in Explorer immediately after.
Expected:
- The item stays visible in the queue, now marked "planned" (does not
  disappear). The file has NOT moved yet.
FAIL if:
- The file already moved before you apply the plan, or the item disappears
  from the queue on confirm.

### Test 6 — Apply moves the file to the resolved destination
Steps:
1. Open the plan review overlay for the confirmed item.
2. Note the full destination path shown (e.g.
   `{target}/{filter}/{date}/light/`).
3. Click Apply.
4. Verify in Explorer that the file now exists at that exact path and no
   longer at its original location.
Expected:
- The path shown in review matches the file's actual new location exactly.
FAIL if:
- The file lands somewhere other than the shown path, or the shown path was
  never displayed before Apply.

### Test 7 — Stale-plan refusal
Steps:
1. Confirm an item (creating a plan) but do NOT apply yet.
2. Externally modify the source file on disk (e.g. append a byte, or edit its
   FITS header with another tool).
3. Try to Apply the existing plan.
Expected:
- The apply is refused with a clear message (stale plan / source changed),
  not silently applied with outdated actions.
FAIL if:
- The stale plan applies anyway.

## Troubleshooting
- Blank window: restart dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- Confirm/Apply behaves like an old build: confirm you touched changed `.rs`
  files after `git reset --hard`.
- No inbox root registered: run Journey 1's wizard first (or `roots.register`
  via the Tauri MCP bridge) before starting this journey's Preconditions.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot + exact
on-screen text / toast.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Register → scan → classify → confirm → apply → durable
  `plans.apply.status` proof** — `automatable`, already covered by
  `plan_review_apply_with_audit` (IPC-level, not UI-driven).
- **Mixed-folder splitting, needs-review banner/badges, bulk reclassify,
  root-picker prompt vs. auto-select, plan-overlay destination-path display,
  stale-plan refusal** — all `automatable` in principle (deterministic
  UI→IPC round-trips) but **zero Layer-2 UI-level coverage today**. See the
  batched new-journey plan — flagged as **"Batch: Inbox UI-level gate +
  reclassify + root-picker"**, the single highest-value new Layer-2 journey
  to add (largest, most mutation-relevant, most completely uncovered at the
  UI-interaction level, matching Batch 1 of the mock-coverage audit but for
  the real-backend layer).
