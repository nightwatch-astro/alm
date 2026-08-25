# Windows validation — Journey 3: Ingest → confirm (catalogue-in-place)

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 3 (spec 041).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — same `inbox.*` pipeline as Journey 2, but the
  deciding factor is the root's **organization state** (`organized`).
- Changed surfaces: Inbox (same UI as Journey 2), plan review overlay.
- What this journey proves: cataloguing an already-organized folder teaches
  PlateVault about the files WITHOUT moving a single byte — no
  destination-root picker appears, the plan reports move-count 0, and the
  file set/content hashes are byte-identical after apply.
- Automated coverage baseline today: `plan_review_apply_with_audit`
  (`crates/e2e-tests/tests/journeys.rs`) registers its fixture root and
  explicitly flips it to **unorganized** to force a move — so today's
  Layer-2 coverage exercises the MOVE branch, not catalogue-in-place. This
  journey's core "organized root → 0 moves, byte-identical files" behavior
  has **no Layer-2 coverage today** and no Playwright mock coverage either
  (`docs/development/e2e-mock-coverage-audit-2026-07-05.md`, Journey 3 row).

## Windows environment mechanics (read once, applies to every Test below)

> Canonical mechanics: `docs/development/windows-native-rust-dev.md` §"Validation driving (MCP bridge, reset, recompile trap)". The steps below are the self-contained per-journey copy; reconcile to that doc if they drift.

- Windows checkout: `C:\dev\astro-plan`. Deploy: `git fetch origin`, then
  `git reset --hard origin/main` as its OWN command.
- **Recompile trap**: after a reset, touch changed `.rs` files
  (`Get-ChildItem <files>.rs | ForEach-Object { $_.LastWriteTime = Get-Date }`)
  before relaunching if Rust changed; otherwise a hard refresh suffices.
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
2. Register a light-frames (or similar) root and explicitly mark it
   **organized** (either during the setup wizard or via Settings → Data
   Sources, if an edit-organization-state control is exposed there — if not,
   register it as organized during first-run setup).
3. Ensure the folder already contains real, correctly-sorted files with
   complete metadata (filter/target present) so Journey 2's needs-review gate
   doesn't interfere.
4. Note one file's exact byte size and last-modified timestamp in Explorer
   before you begin, for the byte-identity check in Test 4.

## Tests

### Test 1 — Classification behaves like Journey 2
Steps:
1. Rescan the organized root from Inbox.
Expected:
- Files classify exactly as in Journey 2 (single-type items; the same
  needs-review gate applies if metadata is missing).
FAIL if:
- Classification differs in a way that suggests organization-state leaked
  into classification logic itself.

### Test 2 — Confirm produces a catalogue plan, not a move plan
Steps:
1. Click **Confirm** on an item from the organized root.
Expected:
- The plan reports a move count of **0** and a catalogue count matching the
  file count. **No destination-root picker** appears (there's nothing to
  pick — the files stay put).
FAIL if:
- A root picker appears, or the move count is non-zero.

### Test 3 — Review overlay shows catalogue actions
Steps:
1. Open the plan review overlay for the confirmed item.
Expected:
- The overlay lists **catalogue** actions (not move actions), and still
  shows the Archive-vs-System-Trash destructive-destination control even
  though these actions don't need it (documented UI consistency choice, not
  a bug).
FAIL if:
- The overlay is missing entirely, or shows move-style actions.

### Test 4 — Apply leaves bytes untouched, files become visible in Sessions
Steps:
1. Click Apply.
2. In Explorer, re-check the same file's size and last-modified timestamp
   from the Preconditions step.
3. Open **Sessions** and confirm the corresponding session now appears.
Expected:
- The file's size/timestamp/content are unchanged (spot-check by re-opening
  the file if you have a viewer) — only PlateVault's database now knows
  about it. It becomes visible in Sessions.
FAIL if:
- The file was moved, renamed, or its bytes changed, or it never appears in
  Sessions after apply.

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- A root registered as "unorganized" by mistake will show Journey-2-style
  move behavior instead of this journey's catalogue behavior — re-register or
  use the organization-state edit control if available, and re-verify which
  state the root is actually in before blaming the app.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot + exact
on-screen text / toast, plus the file's size/timestamp before and after for
Test 4.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Organized-root confirm → 0 moves, catalogue count = file count, no root
  picker, byte-identical apply** — `automatable` (a straightforward variant
  of the existing `plan_review_apply_with_audit` journey: register a root,
  leave it at its default `organized` state instead of flipping it, confirm,
  apply, then assert `movedCount == 0`, `catalogueCount >= 1`, and that the
  original file's content hash is unchanged post-apply). **Zero Layer-2
  coverage today** — flagged in the batched new-journey plan as **"Batch:
  Catalogue-in-place plan variant"**, a low-effort addition since it reuses
  almost all of `plan_review_apply_with_audit`'s existing fixture code minus
  the `sources.set_organization_state` call.
