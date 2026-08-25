# Windows validation — Journey 11: Framing clustering + Inbox-confirm attribution (spec 008 Q27)

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: no dedicated top-level entry in `docs/journeys/` (J01-J17)
  — this is a cross-cutting sub-flow of Journey 2 (Ingest → confirm), Journey 4
  (Sessions review), and Journey 5 (Project lifecycle): grouping a project's
  light sessions into **framings** and ranking Inbox-confirm attribution
  candidates against existing framings/projects (spec 008 Q27, research
  R11a). Tracked here as its own scenario because it is a substantial,
  independently-testable backend feature (F-Framing-1/2/3/5/6/10/11).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `projects.framing.list` / `.merge` /
  `.split` / `.reassign`, real `inbox.confirm` attribution ranking +
  `chosenAttribution` apply-path, real `settings.get` / `settings.update`
  for the clustering tolerance tunables.
- Changed surfaces:
  - **Settings → Framing** (NEW, F-Framing-11): the ONLY real frontend UI
    this feature has. Four number inputs (pointing tolerance fraction,
    no-equipment pointing fallback, rotation tolerance, mosaic panel
    envelope) with a Restore Defaults action.
  - Everything else (framing list/merge/split/reassign, the Inbox-confirm
    attribution candidate ranking + picking) is **real, shipped backend
    IPC with NO frontend consumer yet** — `projects.framing.*` and
    `inbox.confirm`'s `attributionCandidates`/`chosenAttribution` fields
    exist, are wired end-to-end, and are Layer-1 tested, but no page in the
    app renders a framing list, a merge/split/reassign control, or an
    attribution-candidate picker. **This is a real, currently-accurate
    product gap, not a testing gap** — flagged explicitly rather than
    silently worked around (same convention as the journey-04 Test 4
    finding). Tests 2-4 below drive the backend directly via the Tauri MCP
    bridge invoke, exactly as documented in
    `.claude/rules/50-tauri-mcp.md`'s referenced Tauri MCP context, because
    there is no UI path to reach them.
- What this journey proves: (a) the Settings → Framing tunables pane
  actually persists through the real settings store, at the real R11a
  defaults; (b) multi-night/multi-filter light sessions of the same
  target/optic-train/pointing/rotation collapse into one framing; (c) the
  Inbox-confirm attribution pass ranks a matching framing above a
  new-framing/new-project fallback and a `chosenAttribution` pick
  materializes as real framing membership once the plan applies; (d)
  `projects.framing.merge`/`split`/`reassign` mutate membership and flip
  `clustering` to `user_adjusted` without touching the filesystem.
- Automated coverage baseline today (Layer 1, real SQLite, no mocks):
  - `crates/sessions/src/clustering.rs` — pure clustering algorithm:
    multi-night/multi-filter collapse, pointing/rotation tolerance splits,
    `user_adjusted` protection, NULL-geometry exclusion, RA-wrap/near-pole
    geometry edge cases.
  - `crates/app/core/src/framing.rs` — `list`/`merge`/`split`/`reassign`
    use cases: persistence round-trip, `user_adjusted` flips, cross-project
    rejection, partial-mutation rejection on invalid input.
  - `crates/app/inbox/src/attribution.rs` — attribution ranking
    (`compute_candidates`), the apply-path (`apply_chosen_attribution`),
    mosaic target inheritance (no per-frame OBJECT/coordinate resolution
    for `isMosaic` projects), completed-project reopen (Q25 warning), and
    the F-Framing-11 settings-tunables wiring (a stored
    `framingPointingFractionOfFov` override changes the ranking outcome,
    not just the parameter struct).
  - `crates/app/core/tests/attribution_integration.rs` — SC-008 end-to-end:
    a `chosenAttribution` pick persisted at confirm time materializes as
    real framing membership only once the plan actually applies, driven
    through the real `plan_listener` → `ingest_sessions` pipeline (no
    direct internal calls).
  - `crates/app/settings/src/lib.rs` + `crates/persistence/db/src/
    repositories/settings.rs` — the four `framing*` settings keys'
    validation bounds + `load_settings`/`get_settings` round-trip.
  - No Layer-2 (`crates/e2e-tests`, real-UI thirtyfour) or mock-Playwright
    coverage of `projects.framing.*` or `inbox.confirm`'s attribution
    fields exists — there is no UI surface for either automated layer to
    drive. `tests/e2e/settings_framing.spec.ts` (mock-Playwright) covers
    the Settings → Framing pane only.

## Windows environment mechanics (read once, applies to every Test below)

> Canonical mechanics: `docs/development/windows-native-rust-dev.md` §"Validation driving (MCP bridge, reset, recompile trap)". The steps below are the self-contained per-journey copy; reconcile to that doc if they drift.

- Windows checkout: `C:\dev\astro-plan`. Deploy: `git fetch origin`, then
  `git reset --hard origin/main` (or the named PR branch) as its OWN
  command.
- **Recompile trap**: touch changed `.rs` files after a reset if Rust
  changed (`Get-ChildItem <files>.rs | ForEach-Object { $_.LastWriteTime = Get-Date }`);
  otherwise a hard refresh suffices.
- Reset to fresh first-run if needed:
  `Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force`.
- Launch: `powershell.exe -NoProfile -Command "Start-Process -FilePath 'cmd.exe' -ArgumentList '/k','C:\dev\astro-plan\run-dev.bat' -WorkingDirectory 'C:\dev\astro-plan'"`.
  Kill: `Get-Process desktop_shell,cargo | Stop-Process -Force`.
- Blank window recovery: restart the dev server; if still blank, `pnpm
  install` with `$env:CI="true"`, relaunch.
- Tauri MCP bridge (required for Tests 2-4): `cargo tauri dev --config
  src-tauri\tauri.dev.conf.json --features dev-tools` (bridge WS on `127.0.0.1:9223`), connect with
  `driver_session host=localhost port=9223`, invoke via `webview_execute_js`
  → `window.__TAURI__.core.invoke('<snake_command>', {args})`. Command
  names below are given in their dotted (`projects.framing.list`) and
  bridge-invoke snake_case (`projects_framing_list`) forms.

## Preconditions
1. Deploy as above.
2. This journey reuses state — do Journey 2 or 3's ingest→confirm→apply flow
   first to have at least one project + light session on disk, or seed a
   project via `projects_create` over the bridge.
3. Sanity: Settings is reachable from the left nav.

## Tests

### Test 1 — Settings → Framing pane (real UI)
Steps:
1. Open **Settings → Framing** (Library group, next to Target Planner).
2. Read the four values: Pointing tolerance, No-equipment pointing
   fallback, Rotation tolerance, Mosaic panel envelope.
3. Change "Rotation tolerance" to `5`, click/tab elsewhere to blur.
4. Wait ~1 second (auto-save debounce), then switch to another Settings
   pane (e.g. Appearance) and back to Framing.
Expected:
- Step 2: the four fields read `0.1`, `0.2`, `3`, `1` on a fresh DB (R11a
  shipped defaults) — no global Save button (auto-save on blur, matching
  every other Settings pane).
- Step 4: "Rotation tolerance" still reads `5` after navigating away and
  back — it round-tripped through the real settings store, not just local
  component state.
FAIL if:
- The pane doesn't appear in the nav, the fields don't match the R11a
  defaults on a fresh DB, or the edited value reverts after navigating
  away and back.

### Test 2 — Multi-night/multi-filter collapse into one framing (bridge-driven)
Steps:
1. Ingest ≥2 light-frame sessions for the SAME target, same optic-train
   (telescope+camera+focal-length), pointing within ~10% of the FOV
   diagonal, rotation within ~3° — e.g. two different nights/filters of the
   same composition — via Journey 2's real ingest→confirm→apply flow.
2. Over the bridge: `invoke('projects_framing_list', { req: { projectId:
   '<project-id>' } })`.
Expected:
- Exactly ONE framing is returned whose `sessionIds` (or member count)
  includes both sessions, `clustering: "suggested"`.
FAIL if:
- Two separate framings appear for what should be one composition, or the
  call errors.

### Test 3 — Inbox-confirm attribution ranking + apply (bridge-driven)
Steps:
1. With the framing from Test 2 in place, ingest ONE more light frame of
   the same target/optic-train/pointing (still inside tolerance) but do
   NOT apply the plan yet — stop right after `inbox_classify`.
2. Over the bridge: `invoke('inbox_confirm', { req: { inboxItemId: '<id>',
   contentSignature: '<sig>', rootAbsolutePath: '<abs path>' } })` (omit
   `chosenAttribution` on this first call).
3. Read the response's `attributionCandidates` array.
4. Re-run `inbox_confirm` for a fresh item (new `contentSignature`) with
   `chosenAttribution: { kind: 'add_to_framing', framingId: '<top
   candidate's framingId>' }` set.
5. Mark the resulting plan `applied` (or apply it through the normal UI
   flow) and re-run Test 2's `projects_framing_list` call.
Expected:
- Step 3: the top-ranked candidate is `kind: "add_to_framing"` pointing at
  the Test-2 framing, with `matchScore` closer to 1.0 than any other
  candidate; a `new_project` fallback candidate is always present, ranked
  last.
- Step 5: the framing's member count has grown by one — the picked
  attribution materialized as real session membership only after the plan
  applied, never before.
FAIL if:
- No candidate references the existing framing, the fallback candidate is
  missing, or the framing's membership changes before the plan applies.

### Test 4 — `framing.merge` / `split` / `reassign` (bridge-driven)
Steps:
1. With ≥2 framings under one project (from Tests 2-3, or seed a second one
   manually), call `invoke('projects_framing_merge', { req: { requestId:
   '<uuid>', projectId: '<id>', primaryFramingId: '<id>',
   mergeFramingIds: ['<other-id>'] } })`.
2. Call `invoke('projects_framing_list', { req: { projectId: '<id>' } })`
   again.
Expected:
- Step 2: the merged framing is gone, the primary framing now carries every
  session from both, and `clustering: "user_adjusted"` on the primary — a
  later re-derivation (none exists in the UI yet) could never silently
  undo this pick.
- No files moved or changed on disk — this is DB-metadata-only membership
  mutation (§II: no reviewable plan for framing membership).
FAIL if:
- The call errors, sessions are lost, or any filesystem side effect
  occurs.

## Troubleshooting
- Bridge invoke returns `command not found`: confirm the bridge session is
  attached to the freshly (re)compiled binary — a stale binary after
  `git reset --hard` will not expose new/renamed commands (see the `Tauri
  bridge driving quirks` note: 2s JS timeout, two-step nav+probe).
- `attributionCandidates` empty for a light item with real geometry:
  confirm the item's FITS/XISF headers actually carry
  TELESCOP/INSTRUME/FOCALLEN/RA/DEC/rotator — geometry-less items are
  legitimately excluded (NULL-geometry exclusion, Q16), not a bug.

## Report back
Per Test: PASS / FAIL + one line of what you saw. On FAIL, screenshot (Test
1) or the raw bridge response (Tests 2-4) + exact error text.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Settings → Framing pane (Test 1)** — `automatable`, covered by
  `tests/e2e/settings_framing.spec.ts` (mock-Playwright: R11a defaults
  render, edit + auto-save round-trip survives an unmount/remount).
- **Multi-night/multi-filter collapse, attribution ranking + apply-path,
  merge/split/reassign (Tests 2-4)** — `not automatable at Layer 2 or
  mock-Playwright today`: these are real, shipped IPC commands with zero
  frontend consumer. Fully covered at Layer 1 (real SQLite, no mocks —
  see the file list in "Automated coverage baseline" above), including an
  SC-008-style real-pipeline proof
  (`crates/app/core/tests/attribution_integration.rs`) that mirrors what
  Tests 2-3 exercise manually here. Promoting this to a Layer-2 thirtyfour
  journey or a mock-Playwright spec is blocked on product UI work
  (a framing list/merge/split/reassign surface and an attribution-candidate
  picker at Inbox confirm), not on test-harness capability — flagged here
  as the product gap this journey surfaces, not silently worked around.
