---
id: J14
title: Start a project from a target and keep the two linked
version: 1
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [targets, projects]
interfaces: [desktop-ui, desktop-ui-macos]
trace:
  - pre-migration journey.md @ git 66026463
  - deltas/2026-07-14-q27-f4.md (evidence reviewed, not folded — see Δ/gap notes)
  - spec-008 (project create/onboard/edit), spec-035 (SIMBAD target resolution & canonical_target_id)
  - github: nightwatch-astro/alm#612, nightwatch-astro/alm#719
  - docs/development/journey-run-2026-07-14.md (Journey 14 section)
---

## Goal

The user has decided "I want to shoot <target> next." Starting from that
target in the Targets planner, they create a project without retyping the
target's name, and the two stay linked from then on: the project shows which
target it serves, and the target shows every project shot for it. Done means
the link survives creation, renaming, and navigating away and back.

## Preconditions

- P1: At least one target already exists in the catalog (seeded, or added via
  the Targets "add target" search — local seed/cache first, with a "search
  more catalogues" NED/VizieR fallback for names SIMBAD misses, per spec 052).
- P2: At least one confirmed session exists for that target, so the wizard's
  sources step has something selectable (creation currently requires
  selecting at least one — see G2).

## Steps

### S1 — Find the target on the Targets page {#S1}
- **Do:** On the Targets page, filter the list by designation or alias, or
  browse the planner columns, then select the target to open its detail.
- **Expect:** The target's detail pane opens, showing its identity, its
  sessions, and its linked projects (initially none).

### S2 — Start a project from the target's detail {#S2}
- **Do:** From the target detail's primary action, choose "+ New project
  here."
- **Expect:** The project-creation wizard opens on its Name & profile step.
- **Expect (negative):** The project name field is not pre-filled from the
  target, and the target is not shown anywhere in the wizard as a linked
  fact — the wizard's initial state is identical whether it was opened from
  this target's detail, from the Projects page's "+ New project" button, or
  from the command palette. Starting "from" a target currently carries no
  target reference forward at all.
- **Trace:** #612

### S3 — Name the project and pick a processing profile {#S3}
- **Do:** Enter a project name; choose PixInsight/WBPP, Siril, or
  planetary/lunar as the workflow profile.
- **Expect:** A "From target context: …" chip appears in the wizard's
  sub-toolbar once a name is typed.
- **Expect (negative):** The chip is not a reference to the target the user
  started from — it echoes the first word of whatever name is currently
  typed (split on whitespace/·/—). Renaming the project changes the chip's
  text instead of leaving a stable, id-based target reference; there is
  nothing here that "survives editing the name" because there was no target
  reference to begin with.
- **Trace:** #612

### S4 — Select light-frame sources {#S4}
- **Do:** On the sources step, select one or more of the target's confirmed
  sessions.
- **Expect:** The target's sessions are listed and selectable.
- **Expect (negative):** Selecting zero sessions does not allow advancing —
  the Next button stays disabled until at least one session is selected, so
  a project cannot be created with no initial sources through this flow even
  though the backend supports it.
- **Trace:** #719

### S5 — Optional calibration mapping {#S5}
- **Do:** Map available flats/darks/bias to the selected sources.
- **Expect (negative):** The "available" flats/darks/bias offered are
  hardcoded fixture rows (`MOCK_FLAT_ROWS`/`SHARED_ROWS` in
  `StepCalibration.tsx`), not real calibration-matching results for the
  sessions selected at S4 — the same Ha/OIII rows, master ids, and scores
  render regardless of what was actually selected. Whatever the user picks
  here also does not appear in the review-step summary —
  `StepReview.tsx` renders a hardcoded fixture plan (`NGC7000_HOO/…`) that
  ignores its `wizardState` prop entirely — and is not sent to the backend on
  create; the create request carries only name/tool/path/sources/notes, so
  anything mapped here is silently discarded at both display and
  persistence.
- **Trace:** #719, #327, #599

### S6 — Review and create {#S6}
- **Do:** Review the summary panel and click Create.
- **Expect:** The project is created; a confirmation toast appears; the app
  returns to the Projects list with the new project present.
- **Expect (negative):** The new project's Target column and detail header
  show no target ("—") even though this flow started from a specific
  target's detail page.
- **Trace:** #612

### S7 — Check the reverse link {#S7}
- **Do:** Return to the target's detail (from the Targets page, or from the
  project if it exposes a target link).
- **Expect (negative):** The target's Projects section still reads "no
  projects linked" — the project created in S6 does not appear, and there is
  no reverse navigation to try, because no association was ever persisted.
- **Trace:** #612

## Success criteria

- SC1: A project created via S2–S6 carries the originating target's real id
  (`canonical_target_id` set to that target) — checkable both via the
  project's Target column/detail header and the target's Projects list.
- SC2: Editing the project's name after S2 does not detach the association
  (the link is by id, not by parsing the name text).
- SC3: From the target's detail, the newly created project is listed and
  clicking it opens the project already selected; from the project, the
  target link opens the Targets page with that target selected — a working
  round trip in both directions.
- SC4: Selecting zero sources at S4 still allows completing the wizard,
  matching the backend's documented `setup_incomplete` outcome for a
  no-source create.

## Known gaps

- G1: (dissolved 2026-07-15) — tracked as issue #612; project-creation entry points never pass a target id.
- G2: (dissolved 2026-07-15) — tracked as issue #719 (also #887); Sources step and CreateProjectDialog mounting.
- G3: (dissolved 2026-07-15) — mosaic-flag UI is delivered by the spec-008 framing implementation lane (use cases merged in #857).

## Delta log

(none — first FORMAT version)
