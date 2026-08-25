---
id: J04
title: Review acquisition sessions as a derived, always-current inventory
version: 7
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [sessions, projects]
interfaces: [desktop-ui, desktop-ui-macos]
trace:
  - docs/product/journeys/J04-sessions-review-derived/journey.md @ 66026463 (pre-migration doc)
  - docs/product/journeys/J04-sessions-review-derived/deltas/2026-07-14-jval-docdrift.md
  - docs/product/journeys/J04-sessions-review-derived/deltas/2026-07-14-q16-t129.md
  - docs/product/journeys/J04-sessions-review-derived/deltas/2026-07-14-q16-t131.md
  - docs/product/journeys/J04-sessions-review-derived/deltas/2026-07-14-q16-t132.md
  - docs/product/journeys/J04-sessions-review-derived/deltas/2026-07-14-q16-t133.md
  - docs/product/journeys/J04-sessions-review-derived/deltas/2026-07-14-q27-f10.md
  - docs/development/journey-run-2026-07-14.md (live Windows validation, build 7e522c16)
  - docs/development/windows-validation-journeys-tracker.md
  - PR #415 (merged) · PR #849 (merged, Q16 missing-value rendering)
  - PR #891 (merged, fixes #772, #773, #568) · PR #899 (merged, fixes #564, #567)
  - PR #906 (merged, fixes #771)
  - spec-054-adaptive-detail-dock (FR-001, FR-004, FR-005 — adaptive
    side/bottom dock, resizable+persistent width, per-page pin)
  - PR #1736 (guarded JSON reads over frame_ids)
---

## Goal

See acquisition sessions — a night's worth of a target/filter combination,
already confirmed and applied through the Inbox — as a read-only, always
up-to-date inventory list, without performing any separate review or
approval step. Done means: every applied plan is reflected as session
row(s) with correct counts, every row is identifiable and filterable, and
the view carries no leftover review-state controls.

## Preconditions

- P1: At least one Inbox item has been confirmed and its plan applied
  (journey J02 or J03), so confirmed inventory exists to derive sessions
  from.
- P2: The library root(s) backing that inventory are connected (not
  missing/disabled) — root reconnect scenarios are out of scope here (see
  Known gaps G3).

## Steps

### S1 — Sessions stays empty until inventory is confirmed {#S1}
- **Do:** Open Sessions before any Inbox item has been confirmed and
  applied.
- **Expect:** The list shows no rows for that data.
- **Expect (negative):** Raw, unreviewed scan results never appear as
  sessions — derivation only reads already-confirmed inventory.

### S2 — Sessions appear automatically after a plan applies {#S2}
- **Do:** Confirm an Inbox item and apply its plan (J02/J03), then open
  Sessions.
- **Expect:** The corresponding session row(s) appear with no additional
  "review this session" action; frame counts match what the plan actually
  moved/catalogued (live-verified 2026-07-14: a 22-file folder split into
  3 sessions with counts 4/14/4, the exact canonical gain/binning split).
  A catalogued session's target/filter/binning/gain/night are now parsed
  from the real pipe-delimited session key (was JSON-only before, which
  silently discarded every field for a catalogue-ingested session) — rows
  show their real values instead of a generic "Session — {date}" whenever
  that metadata actually exists. Each row's Target cell falls back to the
  session's own name (`session.target ?? session.name`) rather than a
  blank/dash only when the target is genuinely absent.
- **Expect (negative):** No Confirm, Re-open, Reject, or Ignore control and
  no review-state pill (e.g. needs-review/candidate) appears anywhere on
  the page — the Inbox confirm gate the user already passed is the only
  gate (live-verified 2026-07-14: DOM/button scan found none).
- **Expect (negative):** One session whose stored frame list is not valid JSON
  never empties the whole list: every intact session still lists with its own
  frame count, and the corrupt one reports zero frames rather than failing the
  query. That zero is indistinguishable from a session whose frames are
  genuinely unattributed (G4) — the surface that must not confuse the two is
  raw sub-frame cleanup, which refuses outright (J06/S5).
- **Trace (resilience):**
  `crates/persistence/targets/src/repositories/inventory.rs:135`
  (`list_sessions_for_root`), `:161-170` and `:197-206` (per-row `frame_count`
  and first-frame lookup read `frame_ids` through
  `CASE WHEN json_valid(...) THEN ... ELSE '[]' END`), `:412` and `:434`
  (`list_session_cameras`, the S3 Camera dropdown, guarded the same way);
  PR #1736.
- **Trace:** `apps/desktop/src/features/sessions/SessionsTable.tsx:296-299`;
  docs/development/journey-run-2026-07-14.md;
  `crates/app/core/src/inventory.rs` `parse_session_key_fields`,
  `crates/app/core/src/sessions.rs` (PR #891, PR #899 — fix #564). Corrected:
  the name fallback is not always a *unique* discriminator — two same-night
  sessions that both lack metadata can render the identical label
  `Session — {date}` (open bug #654, P3, live-reproduced). SC4 below is
  narrowed to describe this precisely rather than claim 100% uniqueness the
  current fallback doesn't deliver; #654 is a distinct, still-open gap from
  the #564 key-parse fix above (it covers sessions with no metadata at all,
  not sessions whose real metadata was previously mis-parsed).

### S3 — Filter, group, and sort the session list {#S3}
- **Do:** Use the Filter and Camera dropdowns, the Group-by control
  (Target/Filter/Night/Camera/Month), and click a sortable column header.
- **Expect:** Dropdown options are populated from the full, unfiltered
  session set (picking one filter never removes the other options); the
  active sort column exposes `aria-sort`; a "Grouped by X" hint appears
  under the list while grouping is active.
- **Expect (negative):** There is no frame-type filter — sessions are light
  frames only; calibration frames are handled on their own surface. There is
  no secondary/multi-column sort control — sort is single-column
  (`SessionSort` in `SessionsTable.tsx` carries one `{col, dir}` pair; no
  secondary-sort UI exists anywhere in the codebase).
- **Trace:** PR #415 (merged; live-confirmed 2026-07-14: dropdowns,
  grouping, `aria-sort`, footer hint all working). Corrected: the prior
  "set a secondary sort" step was unsupported by any code path
  (`apps/desktop/src/features/sessions/SessionsTable.tsx:55-58`) and not in
  PR #415's own description — dropped rather than left as an unfalsifiable
  aspiration.

### S4 — Open a session's detail {#S4}
- **Do:** Select a session row. Resize the window across the wide-window
  threshold; separately, use the panel's per-page pin.
- **Expect:** The session detail uses the adaptive dock: a full-height,
  drag-resizable side panel on a wide window, a bottom dock when narrow.
  The chosen side-dock width persists across an app restart, and a per-page
  pin overrides the automatic width-based placement when set. A detail
  panel opens showing the session's attribute set —
  target, filter, frame count, exposure, total integration time (when
  derivable, rendered as "1h 30m" — the same h/m grammar Projects and the
  setup wizard use, not a decimal-hours or seconds variant), night, camera,
  gain, binning, sensor temperature, and
  confirmed-by — each carrying a source badge (FITS/User/Inferred/Default)
  only when a real value is present; a field that is applicable but has no
  value renders a distinct "unresolved" chip (never a bare em dash and never
  a source badge); a field that does not apply to this entity renders a
  blank em dash with no chip. Below the property grid, the panel is now
  organized into two further sections: a read-only Calibration section
  listing the session's linked calibration matches (with an explicit
  "no calibration match" empty state when there are none), and a Notes
  section — a free-text editor that autosaves on a debounced pause after
  typing stops, shows an explicit saved signal, rejects input past a 16 KiB
  limit, and persists across navigating away and back.
- **Expect (negative):** Closing the panel via the ✕ control or Escape never
  mutates the session or triggers any lifecycle transition.
- **Expect:** Pressing Escape closes the panel — `ListPageLayout` now
  registers a document-level Escape keydown handler that closes whichever
  detail is open (bottom, side, or the dual strip). This holds even when
  focus stayed on `<body>` after selecting the row. An open nested dialog
  (e.g. a Base UI `Dialog`) that stops propagation on its own Escape
  handling closes first — the page-level listener only fires once no such
  dialog consumes the key first.
- **Trace:** `apps/desktop/src/features/sessions/SessionDetail.tsx`
  (Calibration/Notes sections, PR #891 fixes #568),
  `apps/desktop/src/features/sessions/SessionNotesSection.tsx` (debounced
  autosave + 16 KiB guard, PR #891 fixes #773),
  `crates/app/core/src/inventory.rs` (`calibrationMatches`, batch-loaded
  from `calibration_assignment`, PR #891 fixes #772);
  `apps/desktop/src/components/PropertyTable.tsx:171-205`,
  `apps/desktop/src/components/RenderValue.tsx:100-132` (Q16 shared
  renderer, PR #849, merged). Corrected: the previous "renders as an em
  dash" claim described the pre-#849 behavior (issue #620/#770: a missing
  value showed a bare dash still carrying a "FITS" source pill,
  indistinguishable from a real value); PR #849 decouples the source badge
  from missing values app-wide including Sessions detail. #770 appears
  resolved by this merge but is still open in the tracker as of this audit
  — worth a maintainer check. PR #906 fixes #771 (previously "✕ or Escape"
  was live-tested false on 2026-07-14): `ListPageLayout.tsx` now closes the
  open detail on a document-level Escape keydown, deferring to any nested
  dialog's own Escape handling first. This fix is shared across every
  `ListPageLayout` consumer (Sessions/Calibration/Inbox/Projects), not
  Sessions-specific. spec-054/FR-001, FR-004, FR-005 (adaptive side/bottom
  placement, resizable+persistent width, per-page pin).

### S5 — Follow a linked project from session detail {#S5}
- **Do:** Click a linked project chip in session detail.
- **Expect:** Navigation lands on the Projects list. The Projects route
  supports pre-selecting a project via a `selected` search param
  (`ProjectsPage.tsx:83`), but the Sessions caller does not currently pass
  it: `SessionsPage.tsx`'s `onOpenProject` handler discards the clicked
  project's id and always navigates to `/projects` with no selection
  (`onOpenProject={() => navigate({ to: '/projects' })}` — the `id` argument
  `SessionDetail.tsx` passes is never read).
- **Trace:** `apps/desktop/src/features/sessions/SessionsPage.tsx:256`;
  `apps/desktop/src/features/projects/ProjectsPage.tsx:83`. Corrected:
  commit `eaa5f9b2` ("Sessions detail — linked projects to aux column",
  2026-06-22) dropped `navigate({ to: '/projects', search: { selected: id
  } })` in favor of the id-less call while repositioning the linked-projects
  column; the commit message describes a layout change, not this behavior
  change, so there is no intent evidence — this looks like an unintentional
  regression, not a deliberate simplification. Not caught live on
  2026-07-14 (no project was linked to the test session at that point, so
  the step was skipped rather than exercised).

### S6 — Reveal the session's source root {#S6}
- **Do:** With a resolvable source path, click the reveal action ("Show in
  File Explorer" on Windows) in session detail.
- **Expect:** The OS file browser opens to the session's own frame folder:
  the owning library root's path joined with the session's new
  `InventorySession.relativePath` (derived from the parent folder of the
  session's first frame). Distinct sessions under the same root now reveal
  distinct folders. A session with no resolvable `relativePath` (legacy or
  never-scanned) falls back to the root path rather than failing. If the
  reveal call fails, an error toast is shown and the panel stays open.
- **Expect (negative):** The reveal action is not offered at all when no
  source path can be resolved for the selection (never a dead/no-op
  button).
- **Trace:** `crates/app/core/src/inventory.rs` (`InventorySession.relativePath`,
  PR #891); `apps/desktop/src/features/sessions/revealInventory.ts`
  `resolveRevealPath` (root + relativePath join, native-separator aware, PR
  #899 fixes #567). Corrected: the previous body described Reveal as opening
  the shared `InventorySource` root for every session (live-tested
  2026-07-14 across three M 51 sessions, open bugs #567, #651) — that defect
  is fixed by this join; #651 described the identical symptom and is closed
  by the same fix (its own tracker state may lag).

### S7 — Rescan the Inbox without disturbing Sessions {#S7}
- **Do:** Re-run an Inbox scan of an already-confirmed source.
- **Expect:** Session count and identities stay the same.
- **Expect (negative):** Rescanning never duplicates a session and never
  reintroduces a review/approval state that was intentionally dropped from
  this view.

## Success criteria

- SC1: Before any plan applies, Sessions shows 0 rows for that inventory
  (S1).
- SC2: After a plan applies, session row count and per-row frame counts
  match the applied plan exactly (S2), and one session with an unreadable
  frame list reduces that agreement to its own row rather than emptying the
  list (S2).
- SC3: 0 review-lifecycle controls (Confirm/Re-open/Reject/Ignore, review
  pill) are present anywhere on the Sessions list or detail (S2, S4).
- SC4: 100% of rows render a non-blank Target-cell identity (never a bare
  dash) when FITS metadata is null (S2). Currently NOT met for uniqueness:
  two same-night sessions that both lack metadata can render the identical
  label `Session — {date}` — open bug #654.
- SC5: Selecting a linked-project chip lands on that exact project,
  pre-selected, every time (S5). Currently NOT met: no project is ever
  pre-selected — see S5 (`SessionsPage.tsx:256` drops the id argument).
- SC6: Reveal opens the session's own folder, never a root ancestor (S6).
  Met as of PR #899: Reveal joins the owning root with the session's own
  `relativePath`, falling back to the root only when no relative path is
  recorded (legacy/unscanned sessions) — see S6.
- SC7: A rescan changes neither session count nor introduces any
  review-state indicator (S7).

## Known gaps

- G1: (dissolved 2026-07-15, resolved 2026-07-15) — tracked as issue #773;
  session notes editing, shipped via PR #891 — see S4.
- G2: (retired) — the prior G2 ("PropertyTable source badge on missing
  values, unmerged fix on `impl/q16-missing-value`") is stale. PR #849
  (merged) adopted the shared `RenderValue`/`PropertyTable` renderer on
  Sessions detail, coupling the source badge to value presence
  (`PropertyTable.tsx:201`); see S4. Retired per the id-stability rule
  (never renumbered, never reused) rather than deleted.
- G3: (dissolved 2026-07-15) — tracked as issue #889; was wrongly scoped out, now tracked (connectivity state).
- G4: A session whose stored frame list is unreadable reports a frame count of
  zero here, which reads identically to a session whose frames are genuinely
  unattributed. Nothing on this page distinguishes the two. Tracked as
  astro-plan-dq9r3, which also covers the absence of any in-app repair path.

## Delta log

- **Δ2** 2026-07-15 · S4 · behavior-change
  Session detail now shows a read-only Calibration-linkage section (with an
  explicit no-match empty state) and a Notes section with debounced-autosave
  free text (16 KiB limit, persists across navigation), reorganized below
  the property grid.
  Evidence: PR #891 (fixes #772, #773, #568) · by: journey-scribe
  (intent-gated)

- **Δ3** 2026-07-15 · S2, S6 · behavior-change
  Catalogued sessions now parse their real pipe-delimited
  target/filter/binning/gain/night key (previously JSON-only, silently
  discarding these fields), so rows show real metadata instead of a generic
  "Session — {date}" whenever it exists. Reveal now opens the session's own
  frame folder (root + `relativePath`), not the shared library-root folder
  every session under it previously opened.
  Evidence: PR #899 (fixes #564, #567) · by: journey-scribe (intent-gated)

- **Δ4** 2026-07-17 · S4 · behavior-change
  Pressing Escape now closes the session detail panel (a shared
  `ListPageLayout` fix — also applies to Calibration/Inbox/Projects), where
  previously only the ✕ button worked; a nested dialog's own Escape
  handling still takes precedence.
  Evidence: PR #906 (fixes #771) · by: journey-scribe (intent-gated)

- **Δ5** 2026-07-17 · S4 · behavior-change
  Session detail now uses the adaptive dock: a full-height, drag-resizable
  side panel on a wide window (width and a per-page pin both persist across
  restarts), a bottom dock when narrow.
  Evidence: spec-054-adaptive-detail-dock (FR-001, FR-004, FR-005) · by:
  journey-scribe (intent-gated)

- **Δ6** 2026-07-20 · S4 · behavior-change
  Total integration time now renders as "1h 30m" (h/m grammar), matching
  Projects and the setup wizard — previously Sessions, Projects, and the
  wizard each used a different format (decimal hours, h/m, or h/m/s) for
  the same quantity.
  Evidence: PR #1288 (refs #631) · by: journey-scribe (intent-gated)

- **Δ7** 2026-08-24 · S2, S3, SC2, +G4 · behavior-change
  One session whose stored frame list is not valid JSON no longer empties the
  Sessions list or its Camera dropdown: every read of that column is guarded,
  so intact rows still list with their own frame counts. The corrupt session
  reports zero frames, which is indistinguishable from genuinely unattributed
  frames (G4).
  Evidence: PR #1736 (da71a9c44) · by: journey-scribe (intent-gated)
