---
id: J09
title: Find, add, and plan around an astrophotography target
version: 8
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [targets]
interfaces: [desktop-ui]
trace:
  - pre-migration journey.md @ git 66026463
  - deltas/2026-07-14-jval-docdrift.md (folded: astronomy columns real, favourites DB-backed, aria-sort, library-vs-seed correction)
  - deltas/2026-07-14-q16-t132.md (not folded — see Delta log)
  - deltas/2026-07-14-q16-t133.md (not folded — see Delta log)
  - commits 6b263a1e, 94dfa492, 1efdc0c5, fd87e99c, 6a51dfd5, ba68bf27 (target search: NED/VizieR fallback, coordinate-based suggestions, hybrid UX, lookup caching, Enter/offline handling)
  - PR #415 (aria-sort on sortable table headers, incl. Targets)
  - issues #757, #758, #817 (open, known gaps)
  - docs/development/journey-run-2026-07-14.md (Journey 9 section — validation
    evidence for S1-S5, dupes #658/#792/#574 hit)
  - issues #658, #815, #816 (open — audit-2026-07-15 corrections to S1/S2/S3)
  - PR #890 (merged, fixes #573) · PR #896 (merged, fixes #579, #580)
  - PR #940 (Moon-aware detail "Best date"; list Opposition unchanged —
    addresses the naming half of #792)
  - PR #912 (merged, fixes #574) · PR #905 (merged, fixes #815)
  - PR #914 (merged, carried nJ09c/nJ10a review nits: row density,
    site-edit cache refresh, palette caching)
  - spec-054-adaptive-detail-dock (FR-002, FR-006, FR-007, FR-008, FR-009 —
    adaptive detail dock, scrollable panel body, pinned/no-auto-hide table
    columns)
  - PR #1719 (a truncated resolve cache is recreated instead of crashing)
---

## Goal

The astrophotographer wants to find a specific object (by designation or
common name), add it to their own target library if it isn't there yet, and
review or edit what the app knows about it — its identity, its aliases, their
own notes, and tonight's real observing prospects at their configured site.
"Done" is: the target is in the user's library, findable by any of its names,
and its detail panel shows accurate identity data and (when a site is
configured) real per-site astronomy for tonight.

## Preconditions

- P1: The app has the bundled seed index available (Messier, Caldwell, and a
  slice of NGC/IC/Sharpless/LBN/LDN) for local, offline typeahead matching.
- P2: A network connection is optional; it is required only to resolve a
  target that isn't in the local seed or cache.
- P3: An observing site is configured in Settings for planner columns (Max
  altitude, Tonight's sparkline, Visible-tonight, Lunar separation, Filters,
  Image time, Opposition) to compute real values; without a site, those
  columns disclose that they need one rather than showing a number.

## Steps

### S1 — Browse and search the target library {#S1}
- **Do:** Open Targets. Search by designation or by any alias (catalog-
  provided, or user-added since the list was last loaded — see S3 for a
  known gap where an alias added in the same session isn't searchable until
  the list reloads). Narrow the window while the side dock (S3) is open, to
  where the table would drop below its own 1000px min-width floor.
- **Expect:** The list shows the targets the user has actually added to their
  library (not the full bundled seed catalog), sortable by any column with a
  single active sort indicator, and optionally groupable (e.g. by catalogue).
  A search for a designation or a catalog-provided alias (e.g. "M31" or
  "Andromeda") finds the same row. The active sortable column header
  announces its sort direction to assistive technology (`aria-sort`). On a
  large added-target set, first paint reveals rows in chunks of 300 rather
  than blocking on the full set; during that reveal window a search matches
  only rows already revealed, catching up as more are revealed a moment
  later. Opening the page no longer freezes the app. The table's fixed
  column order, left to right, is: star (favorite), designation, type, max
  alt, opposition, lunar dist, filters, imaging time, sessions. No column is
  pinned and none is ever auto-hidden: the table has its own 1000px
  min-width floor, independent of the side dock, below which the WHOLE
  table (every column together) scrolls horizontally as one unit inside its
  own scroll container.
- **Expect (negative):** At full window width there is no horizontal
  scrollbar and no clipped cell in the table; as the side dock narrows the
  available width below the table's min-width floor, no column silently
  disappears and none is singled out to stay in place — the whole table
  scrolls horizontally as one unit instead.
- **Expect (negative):** The bundled seed catalog (thousands of entries) is
  never materialized as browsable rows in this list — it is reachable only
  through the Add-target search (S2).
- **Trace:** commits fd87e99c, PR #415;
  `apps/desktop/src/features/targets/TargetsPage.tsx` (`REVEAL_CHUNK = 300`
  progressive reveal), `TargetsTable.tsx` (per-target-id astronomy row
  cache) — PR #890 fixes #573 (opening previously froze the app: astronomy
  altitude sampling ran synchronously over the entire catalogue on every
  render). PR #914 fixes a carried nJ09c-review nit: virtualized row height
  now tracks the active density setting instead of a fixed height, so
  scrolling stays visually aligned when density changes. Column order/
  scroll-as-one-unit behavior per `TargetsTable.tsx` (`<colgroup>` order),
  `merges-3.css` (`.pv-targets-table` `min-width: 1000px`,
  `table-layout: fixed`), `merges-2.css` (`.pv-targets-table__scroll`
  `overflow-x: auto`) — no `position: sticky` column rule and no
  column-hiding logic exist anywhere in this file. spec-054/FR-006–FR-008
  (a pinned-column, priority-scroll, no-auto-hide design) were never
  delivered — superseded, no tracked issue.

  The app shell's Targets sidebar badge shows the count of the user's own
  favourited targets (the same set "My Targets" filters to, see S5) — PR
  #912 fixes #574, where it previously showed the size of the entire
  ~13073-entry bundled/resolved catalog.
  Trace: `apps/desktop/src/app/Sidebar.tsx`,
  `apps/desktop/src/features/targets/TargetList.tsx`.

### S2 — Add a target {#S2}
- **Do:** Open Add target and type a name or designation.
- **Expect:** Local, offline typeahead results appear first (from the bundled
  seed plus any previously cached lookups). Confirming a local match persists
  exactly one canonical target row — re-adding the same target never creates
  a duplicate. If the name isn't in the local seed/cache, the app also tries
  a SIMBAD lookup on-demand while the user keeps typing; once the query is
  long enough, an "unresolved" outcome (including "offline" / online
  resolution disabled) is shown as a plain, non-fatal state, never as an
  error. If both the local and SIMBAD phases come up empty, the dialog offers
  an explicit "search more catalogues" action (falls back to SIMBAD's wider
  Sesame/NED/VizieR lookup) framed as a next step, not a dead end; pressing
  Enter triggers that same action only when it is the sole actionable thing
  on screen (zero suggestions) — with any suggestion present, Enter selects
  the highlighted one instead. A resolved lookup (any phase) is cached so the
  same name resolves instantly next time.
- **Expect (negative):** An unresolvable name never fabricates a row; the
  dialog states the outcome inline instead. The results dropdown and the
  "no matches" message now paint above the Add-target/Create-project dialog
  (raised stacking order) and are visible/clickable for a real mouse-driven
  user — previously correct in the DOM but clipped invisible beneath the
  dialog.
- **Expect (negative):** A damaged resolve cache never takes the app down and
  never loses anything the user authored: a cache file left truncated by an
  unclean shutdown or a torn write is deleted and recreated, because the cache
  is a reproducible projection of the bundled seed and the stored canonical
  targets. If the file cannot be recreated, the failure is reported as a
  resolve error rather than crashing.
- **Trace:** commits 6b263a1e, 94dfa492, 1efdc0c5, fd87e99c, 6a51dfd5,
  ba68bf27; journey-run-2026-07-14.md Journey 9 section. PR #905 fixes #815
  (`apps/desktop/src/features/targets/AddTargetDialog.tsx`).
- **Trace (cache recovery):**
  `crates/targeting/resolver/src/simbad/cache.rs:35-52` (a redb file that
  aborts the open through a release-live `assert!` is deleted and reopened
  once), `:53-67` (`ResolveError::Network` when it cannot be removed or
  recreated); PR #1719.

### S3 — Review and edit target identity {#S3}
- **Do:** Open a target's detail panel. Add or remove an alias, set or clear
  a display label, write an observing note. Resize the window across the
  1400px logical-width threshold; separately, use the panel's per-page pin
  to force a placement.
- **Expect:** The detail panel shows real identity data (designation, type,
  coordinates, source, and an optional catalog id — shown as an explicit
  unresolved value, not omitted, when the target has no SIMBAD OID).
  Adding a user alias attaches it to the target with a visible "Remove"
  control; catalog-provided aliases have none. Setting or clearing a
  display label updates the detail heading immediately. Notes save and
  persist across a restart. The panel docks to the SIDE, full-height and
  drag-resizable, when the window is ≥1400px logical wide (the shared
  adaptive-dock default — Targets passes no page-specific threshold), and
  to the BOTTOM below that width; the chosen side-dock width persists across
  restarts, and a per-page pin (Auto/Side/Bottom) overrides the automatic
  width-based choice when set. A linked project in the detail's Projects
  list shows its lifecycle state (e.g. "Archived") with the same
  StatusTag styling every other project surface uses, not raw unstyled
  text. Every section — the altitude graph, alias
  list/add control, display-label editor, notes, Coverage/links sections,
  and the panel's own back button — is reachable by scrolling within the
  panel in either placement; nothing below the altitude graph is clipped or
  unreachable (previously a real defect, issue #816: the panel's fill-mode
  container clipped everything below the graph with no scroll affordance —
  fixed by the adaptive dock's scrollable panel body).
- **Expect (negative):** A catalog-provided alias has no remove control. A
  user-added alias is NOT searchable from the list (S1), and a changed
  display label does NOT propagate to the list row, until the Targets list
  is reloaded/remounted — the detail view and the list are not live-linked
  today (open defect, issue #658, P2, reproduced twice in the 2026-07-14
  validation run; unaffected by the dock/scroll fix above — a distinct
  live-link gap, not a layout gap).
- **Trace:** issue #658 (open, live-link gap, unaffected by spec-054);
  issue #816 (dissolved — fixed by spec-054 adaptive dock, see Δ6);
  journey-run-2026-07-14.md Journey 9 section; `TargetDetailV2.tsx` (wraps
  `DetailPanel fill`, own `.pv-planner__scroll` region); spec-054/FR-001
  (adaptive side/bottom placement), FR-003 (per-page pin), FR-005
  (resizable/persistent side width). FR-002 (a Targets-specific 1500px
  threshold) was never delivered — superseded, no tracked issue.

### S4 — Read tonight's real per-site astronomy {#S4}
- **Do:** With an observing site configured, open a target's row or detail
  panel and read Max altitude, Tonight's sparkline, Visible-tonight, Lunar
  separation, recommended Filters, Image time, and Opposition.
- **Expect:** These values are computed from the target's real coordinates,
  tonight's date, and the configured site — they vary meaningfully by target
  and by site, and change as the site or date changes (not a value stable
  across reloads regardless of input). "Why this guidance" opens (from either
  the row or the detail panel), names the per-filter thresholds behind the
  recommendation, and closes on Escape or an outside click. The
  Visible-tonight rating reflects whether the target actually clears the
  usable altitude threshold tonight, so it varies by target and site even on
  a night with no astronomical-twilight dark window (e.g. high-latitude
  summer) — a target near the zenith no longer reads identically to one that
  never rises just because there's no dark window; imaging time stays zero
  on such a night regardless. The per-row "Tonight" altitude sparkline is
  larger and wider than before, with a coloured line, a soft fill under the
  curve, twilight shading either side of the dark window, and a marker at
  the transit (highest) point. In the detail panel, the "Best date" stat is
  the nearest Moon-favourable night to the target's opposition (±15 nights,
  earlier night wins ties, scored with the live Moon-avoidance parameters)
  and explains itself via a tooltip — mirrored into the value's
  accessible name — stating one of three things: the date diverged from the
  opposition and why (both nights' Moon illumination/separation), it
  coincides because the opposition night's Moon is favourable, or no
  favourable night exists within ±2 weeks (the opposition date is then shown
  with that disclosure). The detail date may therefore legitimately differ
  from the list's "Opposition" column, which stays the pure geometric
  transit-at-midnight date with unchanged sort.
- **Expect (negative):** Without a configured observing site, these columns
  disclose that they need a site rather than rendering a plausible-looking
  number. The Sessions column shows the target's real, backend-populated
  session count, sortable by that count; a target with zero or unresolved
  sessions still shows the muted dash rather than a bare "0".
- **Trace:** deltas/2026-07-14-jval-docdrift.md;
  `apps/desktop/src/features/targets/planner-derive.ts`,
  `AltitudeSparkline.tsx` — PR #896 fixes #579 (visibility rating no longer
  uniform on a no-dark-window night) and #580 (larger sparkline with
  twilight shading + transit marker). PR #914 fixes a carried nJ09c-review
  nit: editing the configured observing site's coordinates now correctly
  invalidates the cached per-target altitude/Moon data, so these columns
  refresh immediately instead of showing stale values until an unrelated
  setting changes (`TargetsTable.tsx` cache key now derives from site
  geometry).

### S5 — Toggle a favourite / "My Targets" {#S5}
- **Do:** Star a target from the row or detail panel; switch the list to "My
  Targets".
- **Expect:** The star toggles with immediate visual feedback; "My Targets"
  shows exactly the user's starred set; the favourite state is stored in the
  database and survives an app restart (not a browser-local preference).
- **Trace:** deltas/2026-07-14-jval-docdrift.md

## Success criteria

- SC1: Searching a target's designation and searching any of its aliases
  (catalog, or user-added since the list was last loaded) both resolve to
  the same row (S1, S3) — not currently true for an alias added in the same
  session without an intervening list reload (see S3, issue #658).
- SC2: Adding a target already in the library never creates a second row for
  it (S2).
- SC3: Every stubbed or site-dependent planner value is either a real
  per-site computation or an explicit "needs a site" / "—" disclosure — never
  a concrete-looking number that isn't actually computed (S4).
- SC4: A favourite toggled in one session is still set after an app restart
  (S5).

## Known gaps

- G1: (dissolved 2026-07-15) — tracked as issue #877; Sessions column always renders as a dash.
- G2: (dissolved 2026-07-15) — tracked as issues #757 and #758; unresolved coordinates crash the altitude graph.
- G3: (dissolved 2026-07-15) — tracked as issue #817; dark-window fill implies imaging time at zero.

## Delta log

- **Δ2** 2026-07-15 · S1 · behavior-change
  Opening Targets no longer freezes the app on a large added-target set —
  astronomy sampling is now per-target-id cached and first paint is
  chunked (300 rows), with search covering only the currently-revealed
  rows during that reveal window.
  Evidence: PR #890 (fixes #573) · by: journey-scribe (intent-gated)

- **Δ3** 2026-07-15 · S4 · behavior-change
  The Visible-tonight rating now discriminates by altitude on nights with
  no astronomical-twilight dark window instead of reading uniformly for
  every target; the Tonight sparkline is larger, with twilight shading and
  a transit-point marker.
  Evidence: PR #896 (fixes #579, #580) · by: journey-scribe (intent-gated)

- **Δ4** 2026-07-17 · S4 · behavior-change
  The detail panel's "Best date" no longer duplicates the list's
  "Opposition": it is now the nearest Moon-favourable night to the
  opposition (±15 nights, live Moon-avoidance parameters, broadband
  scoring) with a self-explaining tooltip covering the diverged /
  coincides / none-found states; the list column keeps the pure geometric
  opposition date and sort. Automated coverage: mock-Playwright
  targets_planner 9.5c (CI) + a real-UI Best-date assertion in
  targets_journeys.rs; manual tooltip-styling check added as Windows
  journey-09 Test 9.
  Evidence: PR #940 (addresses the naming half of #792; spec 044 FR-009
  amendment, iteration 2026-07-17) · by: best-moon-date lane
  (intent-gated)

- **Δ5** 2026-07-17 · S1, S2, S4 · behavior-change
  The Targets sidebar badge now shows the count of the user's own
  favourited targets instead of the whole bundled catalog. The Add-target
  search dropdown (matches and "no match" message) is now visible above the
  Add-target/Create-project dialogs instead of clipped invisible beneath
  them. Virtualized row height now tracks the density setting, and editing
  the observing site's coordinates now correctly refreshes cached
  altitude/Moon data for every row instead of showing stale values.
  Evidence: PR #912 (fixes #574), PR #905 (fixes #815), PR #914 (carried
  nJ09c/nJ10a review nits, no matching issues) · by: journey-scribe
  (intent-gated)

- **Δ6** 2026-07-17 · S1, S3 · behavior-change
  The target detail panel now uses the adaptive dock: full-height,
  drag-resizable side placement at ≥1400px logical window width (width
  persists; a per-page pin overrides the automatic choice), bottom
  placement below that width. Every section is now reachable by scrolling
  in either placement — fixes issue #816 (content below the altitude graph
  was clipped invisible with no scroll affordance). Issue #658 (alias/label
  live-link to the list) is a distinct, still-open gap, unaffected by this
  change.
  Evidence: spec-054-adaptive-detail-dock (FR-001, FR-003, FR-005) · by:
  journey-scribe (intent-gated)

- **Δ7** 2026-07-20 · S3, S4 · behavior-change
  The Sessions column now shows the target's real, backend-populated
  session count instead of a hardcoded dash with a no-op sort; a linked
  project in the detail panel now shows its lifecycle state (e.g.
  "Archived") with the shared StatusTag styling instead of raw text.
  Evidence: PR #1293 (refs #622, #739) · by: journey-scribe (intent-gated)

Note (not a Δ entry — provenance for why two deltas were not folded into the
body above): `deltas/2026-07-14-q16-t132.md` and `2026-07-14-q16-t133.md`
describe a shared-value-renderer / unresolved-vs-not-applicable rendering
model and a "detail panels lead with new information" content model for this
surface. Their own source (spec-030 iteration `2026-07-14-q16-applied.md`)
records these as spec-only, and issues #619/#620 (still OPEN as of
2026-07-15) track the gap against the shipped UI. Confirmed at this audit:
`apps/desktop/src/features/targets/` does not use the shared `RenderValue`
component, and `TargetsTable.tsx`'s unknown-coordinates / no-site branches
still render a plain "—", not the FR-137 "unresolved chip". Not verifiably
shipped for this surface — left out of current-truth steps.

- **Δ8** 2026-08-24 · S2 · behavior-change
  A resolve-cache file left truncated by an unclean shutdown or a torn write is
  now deleted and recreated instead of aborting the app: the cache is a
  reproducible projection of the bundled seed and the stored canonical targets,
  so nothing user-authored is lost, and a file that cannot be recreated is
  reported as a resolve error.
  Evidence: PR #1719 (54615f80e) · by: journey-scribe (intent-gated)
