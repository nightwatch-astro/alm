# Windows validation — Journey 6: Cleanup: scan → review → apply

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 6 (spec
  017/025).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `artifact.watcher.attach`,
  `artifact.list`, `cleanup.policy.update`, `cleanup.scan`,
  `cleanup.plan.generate`, `plans.approve`, and (once a real UI Apply button
  exists) `plans.apply_real`.
- Changed surfaces: a project's Outputs/Cleanup section (scan button,
  candidate list grouped by kind, protection-acknowledgement gate, plan
  review overlay, apply progress UI).
- What this journey proves: scanning is read-only (a real preview, no plan
  yet); a plan is only created by an explicit "Generate cleanup plan" click,
  with the destructive destination (Archive vs. System trash) fixed at that
  point; protected items must be explicitly acknowledged before Approve &
  apply is clickable; applying shows live per-item progress and actually
  moves files; an empty plan can't be approved.
- Automated coverage baseline today: Layer-2 journey `cleanup_plan_review`
  drives a REAL `projects.create` → `artifact.watcher.attach` →
  `artifact.list` → `cleanup.policy.update` → `cleanup.scan` →
  `cleanup.plan.generate` → `plans.approve` round-trip and stops at
  `ready_for_review` → `approved` — it does **not** apply the plan. This is
  a **documented, known gap**: `plans.apply_real` (the real apply command)
  takes a `tauri::ipc::Channel` progress argument with no channel-free
  equivalent for cleanup/archive plans (unlike `inbox.plan.apply` for inbox
  plans), and the shipped Cleanup/Archive UI does not yet wire an Apply
  button for `cleanup.plan.generate` output at all — so the actual "Apply"
  step in this journey (Tests 4–6 below) has **no automated coverage of any
  kind today**, mock or Layer-2. This document is the only current
  verification for it.

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
2. Have a project with real processing outputs of **mixed kind**
   (intermediate/master/final) — e.g. drop a file named per PixInsight's
   `integration_*` convention into the project's output folder (classifies as
   Intermediate) and other files matching Master/Final naming.
3. Mark at least one output as **protected** (via whatever protection-default
   or per-item control the app exposes) so Test 3's acknowledgement gate is
   exercised.
4. In Settings → Cleanup, ensure Intermediate is opted into Archive (or
   whichever action you intend to test) so the generator has a real
   candidate.

## Tests

### Test 1 — Scan is read-only
Steps:
1. In the project's Outputs/Cleanup section, click **Scan for cleanup
   candidates**.
Expected:
- A read-only preview appears, grouped by kind (Intermediates/Masters/
  Finals), protected items shown locked/unselectable, a total reclaimable
  size shown. Nothing on disk changes.
FAIL if:
- Any file moves/changes as a result of scanning alone.

### Test 2 — Generate creates a real plan with a fixed destination
Steps:
1. Choose a destructive destination (Archive folder or System trash).
2. Click **Generate cleanup plan**.
Expected:
- This is the point a real, reviewable plan is created. The destination is
  now fixed and shown read-only in the review overlay from here on.
FAIL if:
- The destination remains editable after generation, or no plan is actually
  created (still just a preview).

### Test 3 — Protection acknowledgement gates Approve
Steps:
1. Open the review overlay. If a protected item is included, try clicking
   **Approve & apply** before acknowledging.
2. Explicitly acknowledge the protected item, then click Approve & apply.
Expected:
- Step 1: Approve & apply stays disabled until protection is acknowledged.
  Step 2: it becomes clickable only after acknowledgement.
FAIL if:
- Approve & apply is clickable while a protected item is unacknowledged.

### Test 4 — Apply shows live progress and actually moves files
Steps:
1. With the plan approved, click Apply (or the equivalent apply control if
   separate from Approve).
2. Watch for a live "Applying N of M…" indicator.
3. In Explorer, verify the files landed at the chosen destination (Archive
   folder) rather than being deleted outright, if Archive was chosen.
Expected:
- Live per-item progress is visible during apply; files are moved to the
  correct destination on disk.
FAIL if:
- No progress indicator appears, or files are deleted outright when Archive
  was the chosen destination, or files don't move at all.

### Test 5 — Re-scan shows applied items gone
Steps:
1. After apply, click **Scan for cleanup candidates** again.
Expected:
- The applied items no longer appear as candidates.
FAIL if:
- They still appear (suggests the apply didn't really happen or the scan
  doesn't reflect real state).

### Test 6 — Empty plan cannot be approved
Steps:
1. Start a new cleanup review, deselect every candidate.
2. Try to click Approve & apply.
Expected:
- The control stays disabled with nothing selected.
FAIL if:
- An empty plan can be approved.

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- No "Scan for cleanup candidates" button visible at all: confirm you're on a
  build with PR #413 merged (already the case on `main` as of 2026-07-05).

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot + exact
on-screen text / toast, and (for Test 4) the exact destination folder
contents in Explorer.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Scan → generate → approve (real artifact detection, real policy, real
  plan generation, `ready_for_review` → `approved`)** — `automatable`,
  already covered by `cleanup_plan_review`.
- **Apply (progress UI, real file move to Archive/Trash, post-apply re-scan,
  empty-plan block)** — `automatable` in principle, but **blocked** on a
  real gap: no channel-free apply command exists for cleanup/archive plans,
  and the UI itself has no wired Apply button for generator output yet.
  Flagged in the batched new-journey plan as **"Batch: channel-free generic
  apply + Cleanup Apply-button wiring"** — this is the single blocking
  prerequisite for extending BOTH this journey and Journey 7 (Archive) to
  full Layer-2 coverage; until it lands, Tests 4–6 above remain manual-only.
