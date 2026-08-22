# Windows validation — Journey 9: Targets & planning

> For: Claude computer-use ("cowork") on the Windows machine running PlateVault.
> You have NO access to the source repo. Everything you need is in this document.
> Report each Test as PASS / FAIL with what you observed.

## Journey facts (context — you do not act on this section)
- Product journey: `docs/product/user-journeys.md` Journey 9 (specs 023, 035,
  044 Track B, 047 Track A).
- Branch to test: `main` (unless a specific PR branch was named to you).
- Touches Rust backend? yes — real `target.resolve` (SIMBAD/offline seed
  cache), target CRUD/alias/notes commands. Frontend-only for the astronomy
  columns (astronomy-engine runs client-side).
- Changed surfaces: **Targets** page (catalog list, Add target, target
  detail), the planner's astronomy columns and altitude graph.
- **UPDATED 2026-07-09 — the gated-off framing below (Tests 6a/6b/6c) is now
  STALE and describes a state `main` no longer has.** Since this doc was
  written: spec 044 US3 (site CRUD, PR #440-equivalent) merged, and
  `site-gate.ts`'s `readSiteExists()` was fixed to read the real observing-site
  store instead of a hardcoded `false` (PR #450, commit `4e5c3a4f`). **A
  working site-creation UI now exists** (first-run wizard Site step, and
  Settings → Target Planner → Observing Sites), and once a default site
  exists, spec 047 (Moon phase, lunar separation, filter guidance, opposition)
  and spec 044 Track B (real per-site altitude, rise/set, imaging time, date
  picker, per-band moon-free hours, dark-window disclosure — see Test 7) all
  render for real. Tests 6a/6b/6c are KEPT below as regression checks for the
  no-site (gated-off) state, which is still a real, reachable state (a fresh
  install with no site configured) — but do NOT expect them to represent
  "the only state `main` can be in" the way the original note implied. If you
  see real astronomy on a fresh install with no wizard site step completed,
  that's a regression in the gate itself — report it.
- Automated coverage baseline today: **this journey has NO Layer-2 coverage
  and no Playwright mock coverage at all** (confirmed by both
  `verify-on-windows-journeys.md` and
  `e2e-mock-coverage-audit-2026-07-05.md`); `all_top_level_screens_load`
  only proves `/targets` renders without crashing. Spec 047's own task list
  (`specs/SPEC_STATUS.md` row 80) explicitly defers "T028 verify-on-windows"
  to "a separate campaign lane" — **this document is that lane.** ~23 vitest
  component files exist under `features/targets/` (including
  `__setSiteExistsForTest` seams that exercise the real-astronomy rendering
  path in isolation, since the real app can't reach it yet) but none of them
  are a real-backend or real-UI proof.

## Windows environment mechanics (read once, applies to every Test below)

> Canonical mechanics: `docs/development/windows-native-rust-dev.md` §"Validation driving (MCP bridge, reset, recompile trap)". The steps below are the self-contained per-journey copy; reconcile to that doc if they drift.

- Windows checkout: `C:\dev\astro-plan`. Deploy: `git fetch origin`, then
  `git reset --hard origin/main` as its OWN command.
- **Recompile trap**: touch changed `.rs` files after a reset if Rust
  changed; otherwise a hard refresh suffices (this journey's astronomy
  columns are pure frontend — a hard refresh is enough for those; target
  resolve/CRUD are backend, so a full relaunch after a Rust change is
  required for those Tests).
- Reset to fresh first-run if needed:
  `Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force`.
- Launch: `powershell.exe -NoProfile -Command "Start-Process -FilePath 'cmd.exe' -ArgumentList '/k','C:\dev\astro-plan\run-dev.bat' -WorkingDirectory 'C:\dev\astro-plan'"`.
  Kill: `Get-Process desktop_shell,cargo | Stop-Process -Force`.
- Blank window recovery: restart dev server; if still blank, `pnpm install`
  with `$env:CI="true"`, relaunch.
- Tauri MCP bridge (optional): `cargo tauri dev --config
  src-tauri\tauri.dev.conf.json --features dev-tools` (bridge WS on `127.0.0.1:9223`), connect with
  `driver_session host=localhost port=9223`, invoke via `webview_execute_js` →
  `window.__TAURI__.core.invoke('<snake_command>', {args})`. To fake an
  ObserverSite via the bridge instead of the UI, call `settings_update` with
  scope `'observing'` and values `{observingSites: [...], observingActiveSiteId,
  observingDefaultSiteId, usableAltitudeDeg}` (see `observing-sites/site-store.ts`
  for the exact shape) — the real site-creation UI (Test 6b) is the normal path.

## Preconditions
1. Deploy as above; complete first-run setup (bundled seed catalog loads
   automatically).
2. Network connection available for the SIMBAD resolve-on-demand test.
3. Sanity: Targets is reachable from the left nav and renders rows.

## Tests

### Test 1 — Catalog list, search, sort
Steps:
1. Open **Targets**. Search "M31", then separately search "Andromeda".
2. Sort by any column.
Expected:
- Thousands of rows render with smooth virtualized scroll. Both searches
  resolve to the same row. Sorting shows a single active sort indicator.
FAIL if:
- Either search fails to find the row, or scrolling stutters badly, or more
  than one sort indicator is active at once.

### Test 2 — Add target (local match, no duplicate)
Steps:
1. Click **Add target**, type a name that's in the local seed, confirm.
2. Repeat with the exact same name.
Expected:
- Exactly one row persists after both attempts (re-adding never creates a
  duplicate).
FAIL if:
- A second row appears after re-adding.

### Test 3 — SIMBAD resolve-on-demand (success + failure)
Steps:
1. Add a target name NOT in the local seed, with network available.
2. Separately, try an unresolvable/garbage name (or disconnect network).
Expected:
- Step 1: a real SIMBAD lookup happens and the result is cached (adding it
  again later should resolve instantly/offline).
  Step 2: an inline "not found"/unreachable message appears — never a
  fabricated row.
FAIL if:
- A row is fabricated for an unresolvable name, or no inline message
  appears on failure.

### Test 4 — Target detail identity, aliases, notes
Steps:
1. Open a target's detail page.
2. Add a user alias; search for it from the Targets list.
3. Try removing a catalog-provided alias.
Expected:
- Identity fields (designation, type, coordinates, source, catalog id where
  present) are real. The new alias becomes searchable immediately.
  Catalog-provided aliases cannot be removed (no delete control, or a
  disabled one); only user-added ones can be removed.
FAIL if:
- A catalog-provided alias can be removed, or a new user alias isn't
  searchable.

### Test 5 — Favourites is localStorage-only (expected, not a bug)
Steps:
1. Toggle "Favourites"/"My Targets" on a target.
2. Close and reopen the app.
Expected:
- It persisted (same browser profile / same app data). This is
  `localStorage`-only — it will NOT survive a profile reset or follow the
  user across machines. Confirm this is the actual behavior; it's expected,
  not a regression.
FAIL if:
- It doesn't persist across a normal relaunch (that WOULD be a real
  regression, since even the localStorage-only implementation should
  survive a same-machine relaunch).

### Test 6a — Astronomy columns render a disclosed placeholder, not a fabricated value (expected state on `main` today)
Steps:
1. On the Targets table, hover/inspect the Max altitude, sparkline,
   Opposition, Lunar separation, Filters, and Image time columns.
2. Open a target's detail page and look at the altitude graph.
Expected (per the Journey facts above — no ObserverSite can exist on `main`
yet):
- Every one of these shows a "set up your observing site" prompt or an
  explicit approximate/placeholder disclosure — **never** a concrete-looking
  fabricated number with no disclosure. The Sessions column renders a dash.
FAIL if:
- Any column renders a concrete-looking value with NO disclosure — this is
  a constitutional-level failure (a stub must never be mistaken for real
  data) regardless of whether the underlying value happens to be
  placeholder or real.

### Test 6b — A working site-creation path exists (Settings and/or first-run wizard)
Steps:
1. Reset to fresh first-run (see Windows environment mechanics) and check
   whether the setup wizard has a "Site" step (name/lat/lon/timezone).
2. Separately, go to Settings → Target Planner → Observing Sites and add a
   site there.
Expected:
- BOTH paths work: completing the wizard's Site step OR adding a site in
  Settings persists a real, active `ObserverSite` and the planner starts
  rendering real astronomy immediately (no relaunch needed).
FAIL if:
- Neither path actually persists a site (i.e. `readSiteExists()`/the planner
  never flips to real astronomy) — that would mean a real regression in the
  already-merged site-gate fix (PR #450) or site CRUD (spec 044 US3).

### Test 6c — Real astronomy renders once a site exists
Steps:
1. Create a default observing site via either path in Test 6b.
2. Re-open Targets and a target's detail page.
Expected:
- Max altitude, sparkline, Opposition, Lunar separation, Filters, and Image
  time now compute from the real per-site ephemeris (spec 044 Track B +
  047 Track A) — genuine numbers that change when you switch the active site
  or the usable-altitude threshold, not a placeholder pattern.
FAIL if:
- Values look like the old hash-derived placeholder pattern (stable across
  reloads but not reacting to a changed site/threshold) — that would be a
  real regression, not expected behavior.

### Test 7 — Track B: date picker, best-imaging date, Moon separation, moon-free hours, dark-window disclosure, altitude graph (spec 044 Track B, requires Test 6b's site to exist)
Steps:
1. In the Targets top bar, find the date field (label "Plan for") next to
   the Moon summary widget. Note today's Max altitude / Visible / Image time
   values for one target row.
2. Change the date to ~6 months from today. Observe the same target's Max
   altitude / Visible / Image time.
3. Click "Tonight" (appears once you've changed the date) to reset.
4. Open a target's detail page. In the Tonight panel, find a "Best date"
   stat row alongside Max alt / Img time / Lunar dist.
5. In the same detail page, open the Filters guidance popover (hover/click
   the filter pills). Look for a per-band "Xh moon-free" figure next to each
   band's required-separation figure (e.g. next to "Ha: needs ≥ 60°").
6. Look at the Tonight altitude graph. Note the shaded band (usable-altitude
   region, should hug the curve where it's above the dashed guide line, not
   a static full-width band) and — if the current site/date has any evening/
   morning twilight before/after full darkness — a faint muted shading at
   the very start/end of the graph.
7. If your site/date has a genuine no-dark-window night (try a far-north
   site like 65-70°N latitude in June/July), check that the Img time cell
   shows "—" with a ☀ glyph (the iteration 2026-07-15 reason model — there
   is no Visible column any more, see Test 8), and the detail page shows an
   info banner stating the night never gets dark enough above the
   (still-real) altitude curve.
Expected:
- Step 2: the values change to genuinely different numbers for the new date
  (not the same as tonight's).
- Step 3: values return to matching today's from step 1.
- Step 4: a real date + "in N days"/"in N months" appears, in the same
  format as the Targets table's existing "Opposition" column. Since the
  iteration 2026-07-17 (spec 044 FR-009 amendment) the detail date is
  Moon-aware and MAY legitimately differ from the list's Opposition date
  by up to 15 days — see Test 9 for the exact contract; identical dates
  are also fine (it means the opposition night itself is Moon-viable).
- Step 5: every band shows a distinct, plausible hour figure (0 to roughly
  the target's total imaging hours for the night); a narrowband figure
  (Ha/SII/OIII) should generally be ≥ a broadband one (L/R/G/B) for the same
  target/night (narrowband tolerates the Moon much more closely).
- Step 6: the green/ok shading only covers the portion of the curve that is
  actually above the dashed usable-altitude line — it should visibly follow
  the curve's shape, not just be a fixed horizontal band.
- Step 7: "No dark window" (or equivalent wording), never a silent/generic
  "not visible", and never a fabricated dark window.
FAIL if:
- Changing the date doesn't change any values (date picker not wired).
- The per-band moon-free figures are all identical/zero regardless of band
  or clearly implausible (e.g. exceeding the total imaging time for the
  night).
- The usable-altitude shading is a static band unrelated to the curve.
- A genuinely no-dark-window night shows a concrete (fabricated) imaging
  time or "peaks below threshold" instead of an explicit no-dark-window
  disclosure.

### Test 8 — Iteration 2026-07-15: why-glyphs, computation context, no-dark graph, OSC equipment (specs 044/047 Phase 10, requires Test 6b's site to exist)
Steps:
1. In the Targets table, confirm there is NO altitude-sparkline column and
   NO Visible column (hard removal, FR-007): the columns are Designation ·
   Type · Max alt · Opposition · Lunar · Filters · Img time · Sessions
   (plus the ★ star column).
2. In the planner top bar, find the always-visible one-line context label:
   "Computed for: `<site name>` `<lat>`°N · `<twilight>` · ≥`<N>`° · change"
   (FR-033). Click "change" — it must open Settings → Target Planner.
3. Find a target whose Img time is "—". Hover the glyph next to it: ☀ means
   the night never gets dark enough, ▲ means the target never clears the
   threshold during darkness, ☾ means the Moon blocks every band (FR-030).
   Every zero MUST carry a glyph + tooltip — no bare "—" for a target with
   coordinates while a site is active (SC-015).
4. Find a target with a non-zero Img time rendered "2h10m"-style (FR-032).
   If a muted ☾ follows the value, hover it: the tooltip must NAME the
   affected bands (FR-031). Confirm the value and the Opposition cell
   ("14 Apr · in 9 months"-style) render unclipped in a ~1100×720 window
   (FR-032/SC-016, the #792 fix).
5. Set the date picker to June 21 (any site ≥ ~49°N): the detail page's
   altitude graph must shade the ENTIRE plot as non-dark and the
   above-threshold fill must render grey, not green (FR-034 — the #817
   graph/stat agreement), alongside the darkness banner from Test 7 step 7.
6. In the detail Tonight panel, confirm the three-quantity breakdown
   (FR-005): "Dark window", "Above `<N>`° (night)" (uptime), and "Img time"
   as three separate stat rows; also a small bottom-anchored Moon-exclusion
   band on the graph for the displayed band where the Moon interferes
   (FR-007 overlay).
7. In Settings → Equipment, edit (or add) a camera: a "Sensor type" select
   (Unknown / Mono / OSC) must exist; choosing OSC reveals an "OSC passband"
   select (Color (RGB) / Dual-band (Ha + OIII) / Tri-band (Ha + SII + OIII))
   (FR-035). Save an OSC + Dual-band camera and make sure NO mono camera
   remains configured.
8. Back in Targets: the Img time headline must now be the strictest-band
   single-pass window (FR-036) — generally ≤ the value seen in step 4 —
   and the detail panel must add a "Per line (OSC)" row like
   "Ha 4.0h · OIII 1.0h" (FR-037).
9. Set the camera back to Mono (or Unknown): all planner values must return
   to their step-4 readings exactly (FR-038/SC-017 — unknown behaves as
   mono; the iteration never regresses mono users).
Expected:
- Steps 1-2: columns as listed; context label present in one line with a
  working "change" link.
- Step 3: every zero Img time carries a reason glyph with a tooltip; the
  glyph is also its accessible name (screen-reader text).
- Step 4: no clipping; the muted ☾ names bands.
- Step 5: whole-plot non-dark shading + grey (not green) fill.
- Steps 7-9: OSC round-trip changes the headline + adds the per-line row,
  and reverting restores the original numbers exactly.
FAIL if:
- Any bare zero/"—" without a reason glyph for a coordinate-bearing target
  while a site is active.
- The context label is missing, wraps to two lines at 1100×720, or "change"
  goes nowhere.
- A no-dark night still renders a green usable fill (the #817 regression).
- Setting OSC changes nothing, or reverting to Mono/Unknown does NOT
  restore the pre-OSC values byte-identically.

### Test 9 — Iteration 2026-07-17: Moon-aware detail "Best date" (spec 044 FR-009 amendment, requires Test 6b's site to exist)
Steps:
1. Open a target's detail page and find the "Best date" stat row in the
   Tonight panel.
2. Hover (or keyboard-focus, Tab) the stat's VALUE. A styled app tooltip
   (dark token-styled popup, not a browser-default title bubble) must open
   with exactly ONE of these three explanations:
   - diverged: "Opposition `<date>` falls near full Moon (`<X>`% lit,
     `<Y>`° away). Best night within ±2 weeks: `<date>` — Moon `<X>`% lit,
     `<Y>`° from target."
   - coincides: "Matches opposition — the Moon is favourable that night
     (`<X>`% lit, `<Y>`° away)."
   - none found: "No Moon-favourable night within ±2 weeks of opposition;
     showing the opposition date."
3. Compare with the SAME target's row in the Targets table: the list
   "Opposition" cell. With the diverged tooltip the two dates must differ
   (detail moved to a Moon-viable night at most 15 days either side of the
   list's date, which the tooltip names as "Opposition <date>"); with the
   other two tooltips the dates must match.
4. Open Settings → Target Planner and drop the L band's Moon-avoidance
   distance to a small value (e.g. 10°); return to the detail page: the
   Best date should now generally coincide with the opposition (the search
   recomputes live from the edited parameters). Restore the defaults.
Expected:
- Step 2: the tooltip always states one of the three explanations with real
  percentages/degrees — never an empty or generic tooltip.
- Step 3: list-vs-detail relationship matches the tooltip's claim; the LIST
  Opposition cell is byte-identical to pre-iteration behavior (pure
  anti-solar date, unchanged sort).
- Step 4: parameter edits change the outcome without a restart.
FAIL if:
- The Best date value has no tooltip, or it renders as an unstyled
  browser-default title bubble.
- A diverged detail date is more than 15 days from the list Opposition date,
  or the tooltip's "Opposition <date>" does not equal the list cell's date.
- The LIST Opposition column changed (different date/format/sort than
  before the iteration).

## Troubleshooting
- Blank window: restart the dev server; if still blank, `pnpm install` with
  `$env:CI="true"`, relaunch.
- If you see real astronomy values on `main` WITHOUT doing anything special
  in Test 6c's precondition — that itself is a finding (means the site gate
  changed) — do not assume your test setup is wrong; report it.

## Report back
Per Test: PASS / FAIL + one line of what you saw. Explicitly call out in your
report whether the no-site (Test 6a) and site-created (Test 6b/6c/7) paths
both behaved as documented — that both states are reachable, and each renders
what it should.

## E2E-sync (coverage bookkeeping — not for the Windows agent)

- **Everything in this journey** — `automatable` in principle. The site-gate
  blocker described in earlier revisions of this doc is resolved (PR #450 +
  spec 044 US3), so a Layer-2/Playwright-mock journey covering Test 6b/6c/7
  is no longer blocked on backend state the way it once was; it remains
  unimplemented as of this revision (this document is still the only
  verification lane for Test 7's specific behaviors — date picker,
  best-imaging date, per-band moon-free hours, dark-window disclosure, the
  visx altitude graph).
- **Test 8 partial mock coverage** — `tests/e2e/targets_planner.spec.ts`
  9.5a/9.5b (added with the 2026-07-15 iteration) cover the #817
  no-dark-window reason/graph agreement and the #792 no-clipping assertions;
  9.1b covers the removed columns + the FR-033 context label. The OSC
  equipment round-trip (steps 7-9) and glyph tooltip *contents* are only
  verified through this document.
- **Test 9 automated coverage** — `tests/e2e/targets_planner.spec.ts` 9.5c
  (added with the 2026-07-17 iteration) asserts the list-vs-detail
  relationship and the three-state explanation (via the aria-label mirror)
  in CI's mock-Playwright job; the Layer-2 real-UI journey
  (`crates/e2e-tests/tests/targets_journeys.rs`,
  `targets_planner_real_astronomy_after_site_creation`) asserts the
  Best-date stat + explanation against the real backend. The hover-OPENED
  popup (styling, positioning) is only verified through this document —
  neither automated harness exercises real pointer hover.
- The stub-disclosure requirement (Test 6a) remains safety-critical:
  assert that every astronomy column and the altitude graph carry a
  disclosure affordance in the no-site state, regardless of what real
  astronomy Test 6b/6c/7 can show once a site exists.
