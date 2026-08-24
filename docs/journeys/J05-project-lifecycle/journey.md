---
id: J05
title: Run a project from creation through tool launch and output tracking
version: 5
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [projects, plans, audit]
interfaces: [desktop-ui]
trace:
  - pre-migration journey.md @ git 66026463
  - e2e-agentic-test/008-project-create-onboard-edit/*
  - e2e-agentic-test/024-project-manifests-and-notes/manifests-notes-reveal-labels/scenario.md
  - e2e-agentic-test/011-processing-tool-launch/tool-launch-containment/scenario.md
  - e2e-agentic-test/012-processing-artifact-observation/artifact-attribution/scenario.md
  - e2e-agentic-test/journeys/full-project-lifecycle/scenario.md
  - deltas/2026-07-14-jval-docdrift.md (folded as correction)
  - spec-054-adaptive-detail-dock (FR-004, FR-011 — unified adaptive dock,
    1100×720 bottom-mode usability)
  - PR #1725 (absolute-PATH auto-detect; the macOS working-folder claim
    corrected in the product)
---

## Goal
The user turns a set of already-confirmed acquisition sessions into a
tracked project: create it, attach the sessions it should use, watch its
documentation (per-channel numbers, manifests, notes) stay accurate as the
project changes, launch their processing tool against it, and have the
outputs that tool writes get recorded automatically. Done means the project
detail view reflects real attached data at every stage and every filesystem
side effect (folder creation, tool launch, artifact discovery) is either
safe-by-construction or was explicitly reviewed.

## Preconditions
- P1: At least one confirmed acquisition session exists and is unattached to
  any other project (see J02/J03).
- P2: A library root is registered so project folders and tool working
  directories have somewhere valid to resolve into.
- P3: A processing-tool profile (e.g. PixInsight) with a configured
  executable path exists if S5 (tool launch) is to be exercised; project
  creation and the rest of the journey do not require it.

## Steps

### S1 — Create the project {#S1}
- **Do:** Open the project creation flow, enter a name, optionally choose a
  processing-tool profile, and proceed to create.
- **Expect:** On success, a toast names the created project and its folder
  outcome (folders created / folder creation failed / plan pending review);
  the user returns to the projects list (the created project is not
  auto-selected). The project's on-disk folder structure (e.g. `lights/`,
  `darks/`, `flats/`) is created automatically inside the user's registered
  project library root, never elsewhere.
- **Expect (negative):** Entering a name that collides with an existing
  project (case-insensitively) is rejected only when the user submits the
  form — the wizard returns to the name step with an inline field error
  naming the conflict; no project is created and no folders are written.
- **Expect (negative):** If a plain file already occupies where a project
  folder should go, creation still succeeds for the project record, but the
  user is told which folder could not be created and that plan step remains
  available for review rather than being silently dropped.
- **Trace:** e2e-agentic-test/008-.../create-wizard-field-errors/scenario.md; e2e-agentic-test/008-.../project-mkdir-auto-apply/scenario.md; e2e-agentic-test/008-.../project-path-root-anchoring/scenario.md; correction — navigation target verified against `apps/desktop/src/features/projects/wizard/WizardPage.tsx` (`handleCreate` calls `navigate({ to: '/projects' })` with no `selected` search param) and `WizardPage.test.tsx` ("shows success toast and navigates to /projects after successful create"); duplicate-name-at-submit verified against the same `handleCreate` (`findDuplicateProjectName` runs inside `handleCreate`, not on keystroke), consistent with `docs/product/journeys/J05-project-lifecycle/deltas/2026-07-14-jval-docdrift.md`

### S2 — Attach sources {#S2}
- **Do:** From the project's edit view, add sources from a picker and, when
  needed, remove a previously attached source.
- **Expect:** The edit view opens as a real dialog (`role=dialog`, an
  accessible name, backdrop, Escape-to-close, and a focus trap) centred over
  the page — not a full-window overlay covering the sidebar, list, and
  action bars. The picker offers only unlinked, already-confirmed sessions;
  removing any source except the last one takes effect immediately.
- **Expect (negative):** Not-yet-confirmed inbox data never appears as an
  attachable source. Removing the *last* remaining source is blocked behind
  an inline confirmation, because it would drop the project back to an
  incomplete-setup state.
- **Expect (negative):** A project in a locked lifecycle state (e.g.
  archived) refuses source edits with an explicit message rather than
  silently no-op-ing.
- **Trace:** e2e-agentic-test/008-.../edit-project-sources/scenario.md;
  `apps/desktop/src/features/projects/edit/EditProjectPane.tsx` (rendered
  through the shared `Modal`, PR #1290, closes #660).

### S3 — Review real per-channel numbers {#S3}
- **Do:** Open the project detail view at the 1100×720 minimum window size,
  then at a wide window.
- **Expect:** The project detail is now unified onto the same shared
  adaptive dock as other list pages (Sessions/Calibration/Archive/Targets):
  a full-height, drag-resizable side panel on a wide window, a bottom dock
  when narrow, fully usable at the 1100×720 minimum in bottom mode
  (previously a bespoke side-and-bottom dual layout with no narrow
  fallback). The per-channel (per-filter) breakdown shows actual sub-frame
  counts and total integration time, computed from the currently attached
  sessions and formatted as hours/minutes (e.g. "1h 30m" — the same grammar
  Sessions and the setup wizard use for the identical quantity).
- **Expect (negative):** No channel row shows a placeholder dash or a bare
  `0` where the real value is simply unknown — a missing value is
  distinguishable from a real zero. At the 1100×720 minimum, no part of the
  detail's content is unreachable or clipped in bottom-dock mode.
- **Trace:** e2e-agentic-test/008-.../per-channel-integration-time/scenario.md;
  spec-054/FR-004, FR-011 (unified adaptive dock, 1100×720 usability).

### S4 — Track manifests and notes {#S4}
- **Do:** Trigger any lifecycle-relevant change (creation, a source change,
  a later completed cleanup/archive) and, separately, type freeform notes on
  the project.
- **Expect:** Each lifecycle-relevant change appends a new manifest
  snapshot to an append-only list; opening "reveal" on a manifest opens its
  folder. Notes autosave a few seconds after typing stops, showing a live
  byte counter against a hard size cap.
- **Expect (negative):** No manifest is ever overwritten or removed by a
  later change — the history of prior snapshots stays intact.
- **Trace:** e2e-agentic-test/024-project-manifests-and-notes/manifests-notes-reveal-labels/scenario.md

### S5 — Launch the processing tool {#S5}
- **Do:** With a tool executable configured, choose "Open in {tool}" from
  the project.
- **Expect:** The tool process launches without changing the project's
  lifecycle state. Whether the project's working folder reaches the tool
  depends on how the tool is started: a launch that runs the executable
  directly starts it in that folder, while a macOS launch of an `.app` bundle
  goes through Launch Services, which ignores the caller's folder — every
  seeded macOS profile takes that arm. The one-time hint shown for such a tool
  says only that the tool takes no folder to open, and no longer tells the user
  the working folder is anchored to the project.
- **Expect (negative):** If the project's working directory would resolve
  outside every registered library root, the launch is refused with a
  plain explanation instead of spawning into an unexpected location. If the
  OS itself fails to spawn the process, that failure is reported plainly,
  not swallowed.
- **Expect (negative):** Auto-detecting a tool's executable never offers a
  path that depends on where the app happened to be started: only absolute
  `PATH` directories are searched, so an empty `PATH` element cannot produce a
  candidate resolved against the current folder.
- **Trace:** e2e-agentic-test/011-processing-tool-launch/tool-launch-containment/scenario.md;
  `crates/workflow/profiles/src/launch.rs:12` (the macOS `open -b` arm),
  `:159-166` (`working_dir` is not applied and cannot be), `:148`, `:187`,
  `:198` (`current_dir` on the direct-exec arms); hint string
  `projects_tool_cwd_anchored_hint`
  (`apps/desktop/messages/en-GB.json:1570`); PR #1725.

### S6 — Observe artifacts the tool produces {#S6}
- **Do:** Leave the project open while the processing tool writes files
  into its output folder; separately, close the project and let files land
  while it is closed, then reopen it.
- **Expect:** While open, new files in the output folder are recorded as
  artifacts with a kind (intermediate/master/final) and a confidence level.
  Files written while the project was closed are picked up the next time it
  is reopened.
- **Expect (negative):** The watcher only observes the project's own output
  folder, never the whole library, and the application never modifies or
  deletes an artifact file itself.
- **Trace:** e2e-agentic-test/012-processing-artifact-observation/artifact-attribution/scenario.md

## Success criteria
- SC1: Creating a project with a valid, unique name results in the project
  appearing in the projects list with its registered-root folders existing
  on disk (S1).
- SC2: A duplicate name (any casing) never creates a project or folders;
  the rejection is surfaced at the name field on submit, not as a generic
  toast (S1).
- SC3: The last-remaining-source removal is always intercepted by a
  confirmation; every other removal is immediate (S2).
- SC4: Per-channel counts and integration time in the detail view always
  match the sum of the currently attached sessions' subs (S3).
- SC5: The manifest list length only grows; it never shrinks or replaces an
  existing entry (S4).
- SC6: A tool launch whose working directory falls outside all registered
  roots never spawns a process (S5).
- SC7: Every file appearing in a project's output folder — whether the
  project was open or closed at the time — is eventually recorded as an
  artifact with a kind and confidence (S6).

## Known gaps
- G1: Rejecting an unconfirmed session as a project source is enforced by
  the backend but has no dedicated UI path to trigger it today (carried
  from legacy doc). (accepted by user, 2026-07-15)
- G2: (dissolved 2026-07-15) — tracked as issue #887 (also #719); dialog-vs-wizard design review.

## Delta log

- **Δ2** 2026-07-17 · S3 · behavior-change
  Project detail is unified onto the shared adaptive dock (side when wide,
  bottom when narrow, resizable, pin persists) instead of its previous
  bespoke side-and-bottom dual layout, and is now fully usable at the
  1100×720 minimum window in bottom mode.
  Evidence: spec-054-adaptive-detail-dock (FR-004, FR-011) · by:
  journey-scribe (intent-gated)

- **Δ3** 2026-07-20 · S2 · behavior-change
  The project edit view now opens as a real dialog (role=dialog, Escape,
  focus trap, backdrop) — previously a bare positioned div with no
  containing block covered the entire 1280x800 window, including the
  sidebar and action bars, with none of those affordances.
  Evidence: PR #1290 (closes #660) · by: journey-scribe (intent-gated)

- **Δ4** 2026-07-20 · S3 · behavior-change
  Total integration time now renders as "1h 30m" (h/m grammar), matching
  Sessions and the setup wizard — previously Projects showed a decimal-hours
  variant ("1.5h") for the same quantity.
  Evidence: PR #1288 (refs #631) · by: journey-scribe (intent-gated)

- **Δ5** 2026-08-24 · S5 · behavior-change
  Tool auto-detect now searches only absolute `PATH` directories, so it can no
  longer suggest an executable path that resolves against whatever folder the
  app was started from. On macOS the one-time hint no longer claims the tool
  starts in the project folder: a bundle launch goes through Launch Services,
  which ignores the caller's folder, and the hint now says only that the tool
  takes no folder to open.
  Evidence: PR #1725 (079da6535) · by: journey-scribe (intent-gated)
