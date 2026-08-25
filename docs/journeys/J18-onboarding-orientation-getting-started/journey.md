---
id: J18
title: Get oriented after setup and track first-run progress
version: 2
status: draft
last_reviewed: 2026-07-19
actors: [astrophotographer]
surfaces: [onboarding, shell]
interfaces: [desktop-ui, desktop-ui-macos]
trace:
  - specs/056-onboarding-redesign (implemented; PR #1048)
  - specs/010-guided-first-project-flow/spec.md (superseded — the 3-step
    non-modal `guided/` coach this replaces)
  - github: platevault/platevault#881
  - product decision 2026-07-18 (user-approved onboarding redesign, relayed
    via orchestration run run-onb-0718)
---

## Goal

A user who just finished first-run setup gets a one-time guided tour of the
app's five workflow pages, then keeps a persistent, per-page "Getting
started" checklist in the sidebar that tracks their real first-run progress
as they do actual work — never from injected demo data, never blocking any
workflow. Done means: the tour ran exactly once (or was skipped) and never
auto-runs again; the checklist accurately reflects real domain state at every
point (auto-ticked on real events, manually checkable for the rest); and the
user can locate any tracked control via a non-blocking spotlight, collapse or
permanently remove the checklist, and restore it later without losing or
faking progress.

## Preconditions

- P1: First-run setup has just completed (fresh install or reset dev
  database) — inventory, sessions, projects, and targets are all empty; no
  demo/sample records exist anywhere.
- P2: The desktop app is running with the sidebar in its default expanded
  state, unless a step says otherwise (S13 covers the icon-collapsed
  variant).

## Steps

### S1 — Orientation walk launches automatically after setup {#S1}
- **Do:** Finish the first-run setup wizard.
- **Expect:** Immediately after the wizard closes, a modal page-walk opens
  on its own, its first stop anchored to the Inbox page.
- **Expect (negative):** No demo, sample, or placeholder inventory, session,
  project, or target record exists anywhere in the app as a result of the
  walk starting — the walk operates purely as an overlay on an empty, real
  library.

### S2 — Complete the walk end to end {#S2}
- **Do:** Use Next to advance through every stop.
- **Expect:** The walk has exactly six stops in order: Inbox, Sessions,
  Calibration, Targets, Projects, then a final stop pointing at the
  sidebar's "Getting started" section (this last stop highlights the
  sidebar without requiring page navigation). Each of the first five stops'
  Next brings the corresponding page into view; Back returns to the prior
  stop and its page. Finishing the last stop closes the walk.
- **Expect (negative):** While the walk is open, clicking outside its
  tooltip does not dismiss it or perform any action on the underlying
  page — it is modal, controlled only by Next, Back, Skip, and Escape.
- **Trace:** product decision 2026-07-18

### S3 — Skip the walk instead of finishing it {#S3}
- **Do:** From a fresh, not-yet-oriented install, open the walk (S1) and
  either click Skip on any stop or press Escape.
- **Expect:** The walk closes immediately from whichever stop it was on.
- **Expect:** Skipping is recorded as equivalent to finishing for the
  purposes of "never auto-runs twice" (S4) — skip and finish are the two
  terminal outcomes of the same one-time walk.

### S4 — The walk never auto-runs a second time {#S4}
- **Do:** After completing S2 or S3, fully restart the app (quit and
  relaunch) at least once.
- **Expect (negative):** The walk does not open automatically on any
  subsequent launch, regardless of whether it was finished or skipped.

### S5 — Replay the walk from Settings {#S5}
- **Do:** Go to Settings → Advanced and use the restart-tour control.
- **Expect:** The walk reopens starting at stop 1 (Inbox), regardless of
  whether it was previously finished or skipped.
- **Expect (negative):** Replaying the walk does not reset or alter the
  Getting-started checklist's tick state — the two are independent.

### S6 — The Getting-started trigger sits in the sidebar {#S6}
- **Do:** With the walk closed, look at the sidebar's workflow navigation.
- **Expect:** A Getting-started trigger appears above the Settings entry,
  separated from the primary workflow nav the same way Settings is. It
  carries a progress ring summarizing progress across all page groups, and
  is labelled while the sidebar is expanded.
- **Do:** Click the trigger.
- **Expect:** The checklist opens as a flyout panel beside the sidebar, with
  an overall progress indicator at its top. It is a panel, not an inline
  sidebar section: the sidebar's own content does not reflow to make room.
- **Expect:** The flyout is non-modal — the rest of the app stays visible and
  clickable, and clicking outside it (including a nav link) closes it.
- **Expect (negative):** Nothing resembling the checklist is visible in the
  sidebar before the trigger is clicked.

### S7 — Per-page groups and auto-expand {#S7}
- **Do:** Open the Getting-started flyout, then navigate between Inbox,
  Sessions, Calibration, Targets, and Projects — re-opening the flyout after
  each navigation, since navigating closes it.
- **Expect:** The section contains exactly five page groups, one each for
  Inbox, Sessions, Calibration, Targets, and Projects; each group lists
  between 2 and 4 short item labels.
- **Expect:** Each item's inline label is 3–5 words long; the fuller
  explanatory copy lives only in the item's tooltip, not inline.
- **Expect:** Whichever page group matches the page currently open is
  auto-expanded; the others may be collapsed.
- **Expect:** Hovering an item label surfaces its explanatory tooltip copy
  (distinct from the short label). Keyboard-focusing the item's checkbox
  surfaces the same copy, and Escape dismisses it without moving focus
  (WCAG 1.4.13 — the reveal is owned by the checkbox, not the label, because
  the label is not focusable).

### S8 — Prerequisite-gated item shows reason and jump-link {#S8}
- **Do:** With no inventory item confirmed yet, open the Projects group and
  locate its "create first project" item.
- **Expect:** The item shows a prerequisite-not-met state, with reason text
  explaining what is missing (at least one confirmed inventory item) and a
  jump-link that navigates to where that prerequisite is satisfied (Inbox).
- **Expect (negative):** The item cannot be manually checked complete while
  its prerequisite is unmet — the gate is enforced, not just displayed.

### S9 — A real Inbox confirm auto-ticks its item {#S9}
- **Do:** Follow the jump-link from S8 (or navigate to Inbox directly) and
  confirm a real Inbox candidate into inventory.
- **Expect:** The corresponding Inbox checklist item ticks automatically,
  driven by the real `inventory.confirmed` domain event — no manual check
  action is needed.
- **Expect:** The tick performs its completion choreography: the item shows
  a brief emphasis in place, then moves into a completed area of its group;
  the overall progress affordance pulses to reflect the change.
- **Expect:** The Projects "create first project" item from S8 now shows its
  prerequisite as met (reason text and lock state clear).
- **Expect (negative):** No demo or sample inventory record was created to
  produce this tick — only the user's own confirmed item exists.

### S9a — A real project create auto-ticks its item {#S9a}
- **Do:** With the S8/S9 prerequisite now met, create a real project through
  the normal project-creation flow (any workflow profile, any name).
- **Expect:** The Projects group's "create first project" item ticks
  automatically, driven by the real `project.created` domain event — no
  manual check action is needed.
- **Expect:** The tick performs the same completion choreography as S9: brief
  in-place emphasis, then move into the group's completed area; the overall
  progress affordance pulses.
- **Expect (negative):** No demo, sample, or placeholder project was created
  to produce this tick — only the project the user just created for real
  exists, and it carries the user-entered name and profile, not fixture
  content.

### S9b — A real tool launch auto-ticks its item {#S9b}
- **Do:** Open the project created in S9a and launch it in its configured
  external processing tool through the normal open-in-tool action, letting
  the launch actually spawn the tool process.
- **Expect:** The Projects group's item tied to opening a project in a tool
  ticks automatically, driven by the real `tool.launch` domain event with
  outcome `spawned` — no manual check action is needed.
- **Expect:** The tick performs the same completion choreography as S9/S9a.
- **Expect (negative):** A tool-launch attempt that does not reach outcome
  `spawned` (e.g. the tool fails to start or the launch is cancelled) does
  not tick the item — only the `spawned` outcome counts, matching the
  restore-source exclusion pattern in S10 of filtering by real, specific
  event shape rather than by event family.

### S10 — Restore-source events never tick {#S10}
- **Do:** Perform a restore action that returns a previously archived or
  trashed item to an active state (e.g. via the archive/trash restore
  workflow), producing a domain event of similar shape to a fresh confirm.
- **Expect (negative):** No checklist item ticks as a result of the restore
  event — restore-source events are excluded from auto-tick eligibility
  regardless of which page or item they might otherwise resemble.

### S11 — Manual check and dismiss for non-event items {#S11}
- **Do:** Locate a checklist item that has no corresponding real domain
  event (an informational or self-paced item) and manually check it; on a
  different item, use its dismiss action instead of checking it.
- **Expect:** The manually checked item enters the completed state via the
  user's action alone, with the same in-place-then-move choreography as an
  auto-tick.
- **Expect:** The dismissed item leaves the active list without being
  counted as completed in the overall progress line.
- **Expect (negative):** Manually checking or dismissing an item never
  triggers the real domain event it would otherwise correspond to, and
  never changes any other item's or group's state.

### S12 — Spotlight find on a checklist item {#S12}
- **Do:** Click the find/magnify affordance next to a checklist item whose
  target control is on the current page (or in the sidebar itself).
- **Expect:** A non-modal spotlight highlights the real control in place,
  with a pulse animation that settles after a brief period.
- **Expect (negative):** Under a reduced-motion setting, the spotlight
  appears without the pulse animation.
- **Expect (negative):** The spotlight never blocks clicks on the
  underlying page — every control beneath and around it remains clickable
  while the spotlight is showing.

### S13 — Spotlight dismiss matrix {#S13}
- **Do:** Repeat S12 and dismiss the resulting spotlight five separate ways,
  once each: (a) click the spotlighted target itself, (b) click anywhere
  else on the page, (c) press Escape, (d) toggle the find affordance off
  again, (e) navigate to a different route.
- **Expect:** Each of the five actions dismisses the spotlight on its own —
  no leftover spotlight overlay persists after any of them.
- **Expect:** Dismissing via (a) both performs the target control's normal
  action and clears the spotlight in the same interaction — it does not
  take a separate click to clear it first.

### S14 — Icon-collapsed sidebar keeps the same flyout {#S14}
- **Do:** Collapse the sidebar to its icon-only mode.
- **Expect:** The Getting-started trigger becomes a bare progress ring with
  no label; opening it shows the SAME flyout, with the same overall and
  per-page progress, without requiring the sidebar to expand.
- **Expect (negative):** The checklist is not rendered inline at either
  sidebar width — the flyout is the only host, so collapsing the sidebar
  changes the trigger's appearance and nothing else.

### S15 — Collapse state persists across restart {#S15}
- **Do:** Collapse the checklist section from inside the flyout (not the
  whole sidebar), then fully restart the app.
- **Expect:** The section is still collapsed when the flyout is re-opened
  after relaunch.
- **Expect (negative):** Collapsing does not reset, hide, or remove any
  item's tick state or the overall progress count.

### S16 — Permanently remove the section {#S16}
- **Do:** Open the Getting-started section's header menu and choose the
  permanent-remove action; confirm at the resulting prompt.
- **Expect:** A confirmation prompt appears before the section is removed
  (it is not a single-click irreversible action).
- **Expect:** After confirming, both the checklist and its sidebar trigger
  (the progress ring) disappear — there is no leftover ring to click.
- **Expect (negative):** The removed section does not reappear on its own —
  not after an app restart, not after navigating between pages, not after
  completing further real domain actions that would otherwise have ticked
  items. It stays absent until explicitly restored (S17).

### S17 — Restore from Settings re-seeds from real state {#S17}
- **Do:** In Settings, use the control that restores the Getting-started
  section.
- **Expect:** The sidebar trigger reappears, and opening it shows the
  checklist with its auto-tick items
  re-seeded from actual current database state — any item whose real
  domain event already occurred (e.g. an inventory item confirmed earlier
  in this journey) shows as already complete, not reset to zero.
- **Expect (negative):** No demo or sample data is created by the restore;
  prerequisite-gated items whose prerequisite is already satisfied in real
  data show unlocked immediately, not locked pending a fresh event.

### S18 — The checklist never blocks ordinary workflow {#S18}
- **Do:** With the Getting-started trigger present (flyout open or closed)
  and, separately, with a spotlight active from S12, perform a complete
  real workflow unrelated to any checklist affordance (e.g. navigate
  directly to Projects and create a project without touching the
  checklist).
- **Expect:** The workflow completes exactly as it would with the checklist
  absent; any relevant checklist item updates afterward without having
  interrupted the workflow.
- **Expect (negative):** At no point does the checklist or an active
  spotlight intercept a click meant for the page, force navigation away
  from the user's current action, or present a blocking modal during
  ordinary work.

## Success criteria

- SC1: Across one fresh install, the orientation walk auto-opens exactly
  once (S1) and zero additional times across at least one subsequent app
  restart (S4) — checkable via a persisted one-time-orientation flag and by
  observing no walk overlay on relaunch.
- SC2: Both terminal walk outcomes (Finish via S2, Skip/Escape via S3)
  independently satisfy SC1 — the walk does not distinguish "skipped" from
  "finished" for re-run purposes.
- SC3: Every one of the 5 page groups (Inbox, Sessions, Calibration,
  Targets, Projects) contains between 2 and 4 items at all times — a fixed,
  checkable count range.
- SC4: The overall progress numerator changes only in response to a real
  domain event — `inventory.confirmed` (S9), `project.created` (S9a), or
  `tool.launch` outcome=spawned (S9b) — or an explicit manual check (S11).
  Restore-source events (S10) and non-spawned tool-launch outcomes (S9b
  negative) each produce zero ticks across at least one exercised action of
  their kind.
- SC5: Zero demo, sample, or seeded-for-display records exist in inventory,
  sessions, projects, or targets at any point during or after this journey
  — every record present traces to a real user action taken during the
  journey.
- SC6: The permanently removed section (S16) stays absent across at least
  one full app restart and one unrelated navigation sequence, until
  Settings restore (S17) is used — never reappearing on its own.
- SC7: Every one of the five spotlight-dismiss triggers in S13 clears the
  spotlight with no leftover overlay, and no click on the underlying page
  is ever swallowed by an active spotlight (S12, S18).
- SC8: A checklist item's completed state (auto- or manually-ticked) never
  regresses back to incomplete on its own — only a permanent-remove +
  restore cycle can change how it's re-seeded, and only from real DB state.

## Delta log

### v2 — 2026-07-19 — realign to the shipped flyout architecture

Authored in parallel with spec 056 and never reconciled against what shipped,
so v1 described a UI that does not exist. Amended against the implementation
on PR #1048.

- **S6** rewritten. v1 had the checklist as an inline accordion in the sidebar,
  expanded by default. It ships as a **flyout**: a progress-ring trigger sits
  above Settings and opens a portalled non-modal panel beside the sidebar.
  Rendered inline, the list blended into the sidebar's own surface and read as
  navigation. Added a negative expectation that nothing checklist-like is
  visible before the trigger is clicked.
- **S7** now re-opens the flyout after each navigation, because navigating
  closes it. The tooltip expectation names the checkbox as the keyboard
  reveal owner and adds Escape-without-moving-focus (WCAG 1.4.13, #1103) —
  the label is not focusable, which is how the keyboard path regressed.
- **S14** no longer describes the icon-collapsed width as a *different*
  presentation. Both widths use the same flyout; only the trigger differs
  (labelled row vs bare ring). Added a negative expectation against an inline
  host at either width.
- **S15** collapses the section from inside the flyout rather than collapsing
  an inline accordion.
- **S16** expects the trigger to disappear along with the checklist, so no
  dead ring is left behind.
- **S17/S18** reworded from "the section in the sidebar" to the trigger plus
  its flyout.

The v1 Known-gaps entry (G1: "no code implements this design yet; first
validation run pending") is removed — the implementation has landed and the
steps above were checked against it.
