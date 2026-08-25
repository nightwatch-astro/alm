---
id: J15
title: Register owned equipment and observing sites in Settings
version: 1
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [equipment, observing-sites]
interfaces: [desktop-ui, desktop-ui-macos]
trace: [docs/product/journeys/J15-equipment-observing-site-setup/journey.md @42c596d6, deltas/2026-07-14-q15-t124.md, PR#826 (0cdc81cc), spec-030 T017/T018, spec-044 Track B US3/US6, spec-047 T015, docs/development/journey-run-2026-07-14.md (Journey 15 section, run @7e522c16)]
---

## Goal

An astrophotographer records the cameras, telescopes, optical trains, and
filters they own, and the site(s) they observe from, once in Settings, so
this equipment/site data is durably tracked, editable without orphaning
other records, and auditable. Done when: every piece of gear the user
cares to name exists in Settings → Equipment (or they've deliberately left
it unregistered), and at least the primary observing site is registered in
Settings → Target Planner with a default/active site chosen so the
planner's per-night observability output (max-altitude, visible-tonight,
imaging time) reflects a real location instead of the no-site placeholder.

## Preconditions

- P1: The app is installed and launched; Settings is reachable. First-run
  setup (J01) may already have created one observing site via its own
  optional Observing Site step, but this journey does not depend on that
  having happened — equipment and site registration are independent of
  any prior library content.

## Steps

### S1 — Open Settings → Equipment {#S1}
- **Do:** From Settings, open the Equipment pane.
- **Expect:** Optical Trains, Cameras, Telescopes, and Filters sections
  each load their current list (or an empty-state message) independently.
- **Expect (negative):** A load failure in one section shows its own
  inline error and does not block the other three sections from loading.

### S2 — Register a camera and a telescope {#S2}
- **Do:** Add a camera with a name and one or more comma-separated aliases
  matching the strings the capture software writes into FITS headers
  (e.g. `INSTRUME`). Add a telescope the same way, plus its focal length
  in millimeters.
- **Expect:** Each new entry appears in its table immediately with a
  "Manual" source badge (distinct from "Auto-detected"); aliases render as
  the comma-joined list, or "—" when none are set.
- **Expect (negative):** Saving with the name field blank is rejected
  inline before any request is sent — no row is added, and nothing reaches
  the audit trail (S5).

### S3 — Compose an optical train {#S3}
- **Do:** Add an optical train: a name, a focal length in millimeters, and
  — optionally — a registered camera and/or telescope picked from the ones
  just added.
- **Expect:** The train appears in the Optical Trains table showing the
  resolved camera/telescope names, or "None" for either link left unset; a
  train with no camera or telescope linked is accepted.
- **Expect (negative):** Saving without a numeric focal length is rejected
  inline — no train row is created.
- **Trace:** issue-835 — a train with neither camera nor telescope linked
  saves successfully today; the pre-migration doc's "train requires its
  parts" is not implemented (client or server side) as of this writing.

### S4 — Adjust the filter list {#S4}
- **Do:** Edit or remove one of the seeded filters (Ha, SII, OIII, NII, L,
  R, G, B, HO, SO, UV/IR Cut) to match the real filter wheel, or add a new
  one with a name and a category (narrowband / broadband / dual-band /
  other / custom).
- **Expect:** The change is reflected immediately in the Filters table.
- **Expect (negative):** Saving a filter name that already exists is
  rejected with an inline save error and no duplicate row is created —
  filters are the only equipment type with this protection today (see G2
  for cameras/telescopes/trains/sites).

### S5 — Equipment changes are durably audited, reads are not {#S5}
- **Do:** Perform a create, an edit, and a removal on any equipment entity
  (reuse the actions from S2–S4); separately, open the Equipment pane
  without changing anything.
- **Expect:** Each create/edit/removal — whether it succeeds or is
  refused (e.g. an invalid save reaching the backend) — produces exactly
  one durable, user-attributed entry in the audit trail (Settings → Audit
  Log, journey J13); merely opening or viewing the pane produces none.
- **Expect (negative):** A camera or telescope still referenced by an
  optical train cannot be removed — the pane blocks the attempt with an
  "in use" message before any delete request reaches the backend, and the
  referenced record is never deleted or left orphaned.
- **Trace:** PR#826 (0cdc81cc) — equipment CRUD previously wrote no audit
  row at all; `crates/app/calibration/src/equipment.rs` write_equipment_audit
  now covers create/update/delete on all four entity types, applied and
  refused.

### S6 — Open Settings → Target Planner and add an observing site {#S6}
- **Do:** From Settings, open the Target Planner pane; add an observing
  site with a name, latitude, longitude, IANA timezone, twilight
  definition (astronomical/nautical), and minimum horizon altitude
  (elevation is optional).
- **Expect:** The site appears in the Observing Sites table with its
  formatted coordinates and timezone; because it is the first site ever
  added, it is automatically marked both Default and Active.
- **Expect (negative):** Latitude outside ±90°, longitude outside ±180°,
  a non-numeric elevation, or a minimum horizon altitude outside 0–90° is
  each rejected inline — no site is saved.
- **Trace:** spec-044 Track B US6 T016 (first-site default+active),
  `apps/desktop/src/features/settings/ObservingSites.tsx`
  handleSubmit (range validation, first-site pointer assignment).

### S7 — Add a second site and switch the active one {#S7}
- **Do:** Add another site; mark it Active (or Default).
- **Expect:** The status pills on both rows update immediately — only one
  row shows Active, only one shows Default (a site may hold both, either,
  or neither) — and every screen that depends on the active site (e.g. the
  Targets planner's per-night observability columns) reflects the newly
  active site's location without a restart.
- **Expect (negative):** Switching the pointer does not alter the
  previous active/default site's own coordinates, timezone, or other
  fields — only the pointer moves.
- **Trace:** spec-044 Track B US3 T022 (active-site switch recomputes
  observability via `useActiveSite()`/`useObservingState()` subscribers);
  issue-839 notes the planner surfaces the recomputed *values* but never
  names which site is active (out of scope for this Expect, see report).

### S8 — Remove a site {#S8}
- **Do:** Remove the currently active (or default) site.
- **Expect:** The Default/Active pointers are reassigned to a remaining
  site automatically; if no sites remain, the planner falls back to its
  no-site state.
- **Expect (negative):** The deletion never leaves Default or Active
  pointing at a site id that no longer exists.
- **Trace:** spec-044 Track B US3 T020; `ObservingSites.tsx`
  handleConfirmDelete (reassigns default/active to `remaining[0]` or
  `null`). issue-840: this auto-fallback is disclosed in the confirm-dialog
  copy but never offers the user an explicit choice among remaining sites
  — the pre-migration doc's "forces an explicit fallback choice" is not
  implemented; today's behavior is the automatic reassignment described
  above.

### S9 — Tune per-band moon-avoidance guidance {#S9}
- **Do:** In the same Target Planner pane, edit the distance/width values
  for one or more of the seven fixed bands (L, R, G, B, Ha, SII, OIII);
  use Restore Defaults; reload the app and revisit the pane.
- **Expect:** Each edited cell commits on blur/Enter with no inline
  validation error for an in-range value, and the planner's per-band
  guidance reflects the new value for the remainder of the running
  session.
- **Expect (negative):** This table's bands are fixed and independent of
  the Filters registered in S4 — renaming or removing a registered Filter
  has no effect on this table's rows or values (see G3).
- **Expect (negative):** An edited cell does not currently survive an app
  reload — it reverts to the shipped default on revisit — and the Restore
  Defaults control gives no indication of its scope (no tooltip, label, or
  confirmation naming what it resets) before firing.
- **Trace:** issue-836 (edits revert on reload), issue-837 (Restore
  Defaults scope unstated), docs/development/journey-run-2026-07-14.md
  Journey 15 section.

## Success criteria

- SC1: Every equipment entity type (camera, telescope, optical train,
  filter) can be created, edited, and removed from Settings → Equipment
  in one session, each change reflected in its table without a page
  reload (S2–S4).
- SC2: Every equipment create/edit/remove attempt, applied or refused, has
  exactly one corresponding durable audit-trail row; zero rows are
  written for read-only pane visits (S5).
- SC3: At least one observing site can be added and set Active, and a
  planner screen showing per-night observability (e.g. Targets table
  max-altitude/visible-tonight) visibly changes to match a newly Active
  site within the same session, with no relaunch required (S6–S7).
- SC4: After deleting the site holding the Default or Active pointer,
  both pointers resolve to a remaining site or the no-site state — never
  to a nonexistent id — across single-delete and delete-to-zero cases
  (S8).
- SC5: All seven moon-avoidance bands' distance/width values commit
  on-blur without a validation error and drive the planner's guidance for
  the remainder of the session (S9); persistence across a pane revisit
  after reload, and Restore Defaults stating its scope, are currently NOT
  met (issue-836, issue-837 — candidate Known gap, pending user
  confirmation).

## Known gaps

- G1: (dissolved 2026-07-15) — tracked as issue #879; registered equipment not consumed elsewhere.
- G2: (dissolved 2026-07-15) — tracked as issue #659; no duplicate-name check for most equipment.
- G3: The Equipment → Filters category (narrowband/broadband/dual-band/
  other/custom, per registered filter) is not read by the per-band
  moon-avoidance table; that table's seven bands (L/R/G/B/Ha/SII/OIII,
  `apps/desktop/src/features/targets/astro/moon-avoidance.ts`) are a fixed
  built-in taxonomy, unrelated to the user's registered filter list.
  (accepted by user, 2026-07-15)

## Delta log

(none — initial migrated version)
