# Feature Specification: Tauri Shell Integration & Platform Polish

**Feature Branch**: `051-tauri-shell-integration`

**Created**: 2026-07-05

**Status**: Partial (verified 2026-08-24 against `origin/main` a52c637f2). Nine of
ten user stories ship; US9 does not. The five shell-integration plugins are
non-optional dependencies at `apps/desktop/src-tauri/Cargo.toml:59-67`.

Shipped:

- **US1** single instance — `tauri-plugin-single-instance`, initialised at
  `apps/desktop/src-tauri/src/lib.rs:179`.
- **US2** favourited targets as real data — `target_favourites_list` / `_add` /
  `_remove` registered at `bootstrap/specta.rs:133-134` and `:243-244`, backed by
  `crates/persistence/targets/src/repositories/target_favourites.rs`.
- **US3** per-type cleanup overrides as canonical data — `cleanupTypeOverrides`
  settings key with its own validation rule
  (`crates/app/settings/src/descriptors.rs:548`, `:767`).
- **US4** window size and position — `tauri-plugin-window-state`, initialised at
  `lib.rs:235`.
- **US5** native menu bar — `apps/desktop/src-tauri/src/bootstrap/menu.rs`.
- **US6** native window chrome follows the theme —
  `apps/desktop/src/data/theme.ts:313` calls `getCurrentWindow().setTheme(mode)`
  (`decorations: false` at `tauri.conf.json:30`).
- **US7** shareable diagnostics log file — `tauri-plugin-log`.
- **US8** OS notification on long-task completion — `tauri-plugin-notification`,
  initialised at `src/bootstrap/notify.rs:18`.
- **US10** signed in-app updates — `tauri-plugin-updater` (builder at
  `lib.rs:60-64`) plus `tauri-plugin-process` for the relaunch step.

**Not built — US9** ("behaves like a native app, not a browser tab, in release
builds"): no reload (F5 / Cmd+R), devtools-shortcut or `contextmenu` suppression
exists anywhere under `apps/desktop/src`. Do not read the status above as covering
it.

**Input**: User description: "Adopt a decided set of Tauri shell integrations
(single-instance guard, window-state persistence, native theme sync, a
diagnostics log file, OS notifications, signed auto-update, and a native
application menu bar), retire two hand-rolled shims (a `__TAURI_INTERNALS__`
sniff and unguarded F5/reload/native-context-menu behavior in release builds),
and move two client-side data stubs (favourited targets, per-type cleanup
action overrides) from browser storage into the canonical database, per
Constitution Principle I (local-first custody) and V (durable records)."

## Overview

PlateVault is a Tauri desktop shell around a local-first SQLite-backed core.
Several platform-level behaviors that users expect from a native desktop app —
protecting the single database from concurrent writers, remembering window
placement, matching OS chrome to the active theme, surfacing troubleshooting
logs, notifying on long background work, offering signed updates, and
presenting a native menu bar — are either entirely absent or only partially
wired today. Two pieces of user-facing state (favourited targets, per-type
cleanup action overrides) live only in browser `localStorage`, which the
constitution treats as a temporary stub rather than canonical storage: it is
invisible to audit, does not survive a fresh profile/reinstall, and cannot be
inspected or migrated the way database rows can.

This feature closes both gaps in one coordinated pass: it adopts the platform
integrations decided for this release, retires the two known shims, and
migrates the two stubbed data types to durable, DB-backed state — without
touching PixInsight/processing behavior, filesystem mutation semantics, or any
other product surface.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The database is never opened by two running copies of the app (Priority: P1)

As a user, when I double-click the app icon (or a file association, or the
Start-menu entry) while PlateVault is already running, I want the already-open
window to come to the front instead of a second copy of the app starting, so
the single SQLite library database is never written to by two processes at
once.

**Why this priority**: This is a data-integrity protection, not a convenience.
A second concurrent writer against the same SQLite file risks lock contention,
corrupted in-flight plans, or silently divergent state. It has no dependency on
any other story in this feature and should ship first.

**Independent Test**: With the app running, launch it again (icon, shortcut, or
CLI) and confirm exactly one window exists afterward, it is focused/foregrounded,
and no second process attached to the database.

**Acceptance Scenarios**:

1. **Given** the app is running with its main window open, **When** the user
   launches the app a second time, **Then** no new window opens, the existing
   window is brought to the foreground, and the second launch attempt exits
   immediately.
2. **Given** the app is running but its main window is minimized, **When** the
   user launches the app a second time, **Then** the window is restored and
   focused.
3. **Given** the second launch passes different startup arguments (e.g. a file
   or deep link), **When** it is redirected to the existing instance, **Then**
   the running instance receives those arguments so future argument-driven
   behavior (e.g. "open with") has a place to hook in.

---

### User Story 2 - Favourited targets survive restarts and reinstalls as real data (Priority: P1)

As a user who stars targets I'm actively imaging, I want my favourites to be
stored the same durable way as everything else in my library, so they survive
an app update, a moved profile, or a support engineer inspecting my database —
not silently reset because a browser storage area was cleared.

**Why this priority**: Favourites are already a shipped, user-visible feature
implemented as a documented stub (client-side `localStorage` only). Constitution
Principle V requires canonical state to live in the durable record, not in
browser storage. This is a self-contained, low-risk data migration independent
of every other story here.

**Independent Test**: Star a target, restart the app (simulating a fresh
profile by pointing at the same database), and confirm the star is still set;
inspect the database directly and find the favourite recorded there, not in
`localStorage`.

**Acceptance Scenarios**:

1. **Given** a target is not favourited, **When** the user stars it, **Then**
   the favourite is written to the database and the star renders immediately.
2. **Given** a target is favourited, **When** the app restarts against the same
   database, **Then** the star is still shown without any dependency on browser
   storage.
3. **Given** a favourited target, **When** the user unstars it, **Then** the
   database record is removed (or marked inactive) and the star disappears.
4. **Given** a previously-favourited target's record still exists only in a
   user's old `localStorage` value from before this feature, **When** the app
   first runs post-upgrade, **Then** no crash or duplicate-favourite state
   occurs (old browser-storage favourites are not silently resurrected; a clean
   cutover is acceptable — see Assumptions).

---

### User Story 3 - Per-type cleanup action overrides are canonical, audited data (Priority: P2)

As a user who has customized which action (keep/archive/delete) applies to each
cleanup data type, I want those choices stored durably and traceably, because
they directly drive what a cleanup plan proposes to do to my files — the same
category of decision the constitution already requires to be recorded, not held
only in browser storage.

**Why this priority**: Lower risk than US1/US2 (no plan can currently be
generated from this table — it is UI-only today) but still a real
constitution-driven data-ownership gap. Depends on nothing else in this
feature; can ship any time after Foundational.

**Independent Test**: Change a per-type action, restart the app, and confirm the
override is still applied and appears in the audit trail; the same restart-and-persist
check as US2 but against the settings page's per-type action table.

**Acceptance Scenarios**:

1. **Given** a cleanup data type's default action, **When** the user overrides it
   to a different action, **Then** the change is written to the database and an
   audit entry records who/when/what changed.
2. **Given** a saved override, **When** the app restarts, **Then** the override
   is still shown as the active action for that data type.
3. **Given** an override that turns a protected/high-value type destructive,
   **When** it is saved, **Then** the existing impact warning still surfaces
   (unchanged from today), now backed by durable state.
4. **Given** the per-type action table is not yet wired to real cleanup-plan
   generation, **When** this feature ships, **Then** it changes only where the
   override is stored — it does not newly wire cleanup-plan generation to these
   overrides (that remains a separate, not-yet-specified feature).

---

### User Story 4 - Window size and position are remembered across launches (Priority: P2)

As a user who resizes and repositions the app window to fit my desktop layout, I
want it to reopen where I left it, so I don't re-arrange it every session.

**Why this priority**: Pure quality-of-life; independent of every other story.

**Independent Test**: Resize/move the window, quit, relaunch, and confirm the
window reopens at (approximately) the same size and position.

**Acceptance Scenarios**:

1. **Given** the user resizes and/or moves the main window, **When** the app is
   closed and reopened, **Then** the window reopens at the last size and
   position.
2. **Given** the window was maximized when closed, **When** the app reopens,
   **Then** it reopens maximized.
3. **Given** the display configuration changed since last close (e.g. an
   external monitor was disconnected) such that the remembered position is now
   off-screen, **When** the app reopens, **Then** it falls back to a sane
   on-screen default rather than opening somewhere the user cannot see or reach
   it.
4. **Given** a first-ever launch with no prior state, **When** the app opens,
   **Then** it uses today's default size (1280x820, min 1100x720).

---

### User Story 5 - The app presents a native menu bar (Priority: P2)

As a user on any platform, I want a standard native application menu (About,
Settings, Quit, Window, and a conventional Edit menu with copy/paste/select-all),
so the app behaves like other native apps on my OS — this matters most on
macOS, where an app without a menu bar is unusual and where the Cmd+Q /
Cmd+, conventions are expected, and it also gives assistive technology users a
reliable, standard way to reach global actions.

**Why this priority**: Platform convention and accessibility; independent of
the other stories, but naturally grouped with the other "feel native" polish.

**Independent Test**: Open the app on each platform and confirm the native menu
bar (or, on Windows/Linux, the window menu) exposes About, Settings, Quit,
Window, and Edit with working copy/paste/select-all, using the OS's own
keyboard conventions.

**Acceptance Scenarios**:

1. **Given** the app is running, **When** the user opens the native menu,
   **Then** About, Settings, Quit, and Window entries are present and each
   performs its labeled action.
2. **Given** a text field has focus, **When** the user invokes Copy, Paste, or
   Select All from the native Edit menu (or its OS keyboard shortcut), **Then**
   the action applies to that field exactly as it would in any other native
   app.
3. **Given** the user chooses Quit from the native menu, **When** invoked,
   **Then** the app closes the same way the window-close control does today
   (no bypass of any existing close/confirmation behavior).
4. **Given** this feature explicitly excludes native *context* menus, **When**
   the user right-clicks inside the app, **Then** the existing themed React
   context menu still appears — the native menu bar addition does not change
   right-click behavior.

---

### User Story 6 - Native window chrome follows the active theme (Priority: P2)

As a user who has selected one of the app's four visual themes, I want the
window's native titlebar/chrome (on platforms where the OS renders one) to
follow suit — light or dark — rather than staying fixed to the OS default, so
the whole window looks like one coherent surface instead of a themed app inside
mismatched native chrome.

**Why this priority**: Visual polish, independent of the other stories.
Complements, and does not replace, the existing CSS theme system.

**Independent Test**: Switch between the four themes and confirm the native
window chrome (where the OS draws one) shows the nearest light/dark match for
each; confirm the in-app CSS theme is unaffected by this native-chrome
adjustment.

**Acceptance Scenarios**:

1. **Given** any of the four themes is active, **When** the app starts or the
   user switches themes, **Then** the native window chrome reflects that
   theme's light-or-dark family.
2. **Given** a platform where the OS does not render app-controlled window
   chrome (or the user has disabled native decorations), **When** the theme
   changes, **Then** nothing breaks — the native-chrome sync is a no-op there,
   and only the existing CSS theming is visible.
3. **Given** the native chrome sync runs, **When** it does, **Then** it changes
   only OS-drawn chrome — it never overrides or fights the CSS theme applied to
   the app's own content.

---

### User Story 7 - A shareable diagnostics log file exists on disk (Priority: P2)

As a user reporting a problem, I want a log file I can find and attach to a bug
report, so troubleshooting doesn't require me to run the app from a terminal or
know how to capture console output.

**Why this priority**: Support/troubleshooting quality-of-life; independent of
the other stories. The audit trail already recorded in SQLite remains the
canonical record — this is a supplementary, human-shareable artifact.

**Independent Test**: Run the app through some activity, locate the log file on
disk (via a documented location, e.g. the OS-standard log directory), and
confirm it contains recent, readable entries; confirm the file is capped/rotated
rather than growing without bound.

**Acceptance Scenarios**:

1. **Given** the app is running, **When** any log-worthy event occurs
   (startup, error, warning), **Then** it is appended to an on-disk log file in
   the OS-standard log location for the app.
2. **Given** the app has been running across multiple sessions, **When** the
   log file grows past a defined size/age, **Then** it rotates (old content is
   archived or discarded) rather than growing unbounded.
3. **Given** a user needs to share diagnostics, **When** they are pointed at the
   log location, **Then** the file is plain text/human-readable and contains no
   secrets that would be unsafe to share (e.g. no raw credentials — none are
   expected to exist in this app, but the log format must not defeat that
   assumption).
4. **Given** the SQLite audit record remains the constitution-mandated
   canonical history, **When** this log file exists, **Then** it never replaces
   or is treated as a substitute for that audit record — it is a
   troubleshooting convenience only.

---

### User Story 8 - An OS notification appears when a long background task finishes (Priority: P2)

As a user who kicks off a long-running background operation (draining the
ingest/target-resolution queue, a workflow-run's manifest generation, or
applying a filesystem plan) and then switches to another app, I want an OS
notification when it finishes, so I don't have to keep checking back.

**Why this priority**: Quality-of-life for the app's genuinely long-running
operations; independent of the other stories.

**Independent Test**: Kick off a plan apply (or trigger an ingest-resolution
drain / workflow-run completion), switch focus away from the app, and confirm
an OS notification appears once the operation completes, with a summary
appropriate to what finished.

**Acceptance Scenarios**:

1. **Given** a filesystem plan is approved and applied, **When** the apply
   completes (success, partial failure, or failure), **Then** an OS notification
   summarizes the outcome (e.g. counts applied/failed/skipped).
2. **Given** the background ingest-resolution drain or a workflow-run's
   manifest generation completes meaningful work, **When** it finishes,
   **Then** a notification is shown summarizing what completed.
3. **Given** the OS denies or the user has not granted notification permission,
   **When** a background task completes, **Then** the app does not crash or
   block on the missing permission — the completion is still recorded in the
   normal audit/log paths, just without a popup.
4. **Given** the app window is already focused when a task completes, **When**
   the operation finishes, **Then** a notification MAY still be shown (no
   requirement to suppress it while focused) but must not be the only place the
   outcome is visible — existing in-app completion UI is unaffected.

---

### User Story 9 - The app behaves like a native app, not a browser tab, in release builds (Priority: P2)

As a user, I want keys and gestures inherited from the underlying web engine —
page reload (F5 / Cmd+R), the browser devtools shortcut, and the browser's own
right-click context menu — to do nothing (or nothing surprising) in a shipped
build, so an accidental keystroke never silently discards in-progress state
(e.g. an open plan-review dialog) or reveals a native context menu that doesn't
match the app's own themed one.

**Why this priority**: Robustness/polish; independent of the other stories.
Development builds are explicitly excluded so the dev workflow (hot reload,
devtools) is unaffected.

**Independent Test**: In a release build, press F5 and the OS devtools shortcut,
and right-click inside the app; confirm none of them reload the page, open
devtools, or show the browser's native context menu, while the app's own themed
context menu (where implemented) still works.

**Acceptance Scenarios**:

1. **Given** a release build, **When** the user presses the reload shortcut,
   **Then** the app does not reload/reset its in-memory state.
2. **Given** a release build, **When** the user right-clicks anywhere in the
   app, **Then** the browser engine's native context menu never appears
   (existing themed React context menus, where present, are unaffected).
3. **Given** a development build, **When** the same shortcuts are used, **Then**
   existing dev-mode behavior (hot reload, devtools access) is unchanged.

---

### User Story 10 - Signed, in-app updates (Priority: P3)

As a user, I want the app to check for new signed releases and let me install
them without manually downloading an installer, so I stay current with fixes
without a manual process, while being protected from installing a tampered or
unsigned build.

**Why this priority**: Highest value long-term but depends on release-signing
infrastructure (a signing keypair and a publishing pipeline) that does not
exist in this repository yet — it is sequenced last and treated as its own
story so the rest of this feature can ship independently of that
infrastructure work landing.

**Independent Test**: Publish a signed release artifact and manifest at the
configured endpoint, confirm a running older build detects it, verifies its
signature, and can install it; separately, confirm a tampered or unsigned
artifact at that endpoint is rejected and never installed.

**Acceptance Scenarios**:

1. **Given** a newer signed release is published at the update endpoint,
   **When** the app checks for updates, **Then** it detects the new version and
   offers it to the user (this feature does not require silent/automatic
   installation without user awareness).
2. **Given** the user accepts an available update, **When** it downloads,
   **Then** its signature is verified against the app's embedded public key
   before anything is installed, and installation proceeds only on success.
3. **Given** a release artifact or manifest has been tampered with or is
   unsigned, **When** the app attempts to apply it, **Then** the update is
   rejected and the running app is left unchanged.
4. **Given** the update endpoint is unreachable (offline, DNS failure, etc.),
   **When** the app checks for updates, **Then** it fails quietly (no crash, no
   blocking the rest of the app) and the user can retry later.
5. **Given** the signing keypair and publishing pipeline this story depends on
   do not yet exist in this repository, **When** this feature is implemented,
   **Then** the application-side integration (check/verify/install flow,
   config, capability) is complete and ready, while the actual signing
   key/pipeline stand-up is tracked as a documented follow-up (see plan.md /
   research.md) rather than blocking this spec's other nine stories.

---

### Edge Cases

- **Single-instance**: the "existing" instance is unresponsive (hung) when a
  second launch arrives — the second launch still exits rather than piling up
  additional processes; a hung app is a separate, pre-existing failure mode
  this feature does not need to newly solve.
- **Favourites/cleanup-override migration**: a user upgrading from a pre-feature
  build has existing `localStorage` values. A one-time, best-effort import MAY
  be offered, but a clean cutover (old browser values simply stop being read)
  is an acceptable, explicitly assumed outcome — see Assumptions.
  Concurrent edits from two app windows are out of scope; SQLite's single-writer
  model already serializes actual writes.
- **Window-state**: remembered position lands fully off-screen (monitor
  removed/resolution changed) — falls back to the documented on-screen default,
  never opens somewhere unreachable.
- **Theme sync**: a future fifth theme, or a theme that is intentionally neither
  clearly light nor dark, still needs a deterministic nearest-mapping decision
  (see plan.md/research.md for the mapping table); a platform with no native
  chrome to set is a no-op, not an error.
- **Diagnostics log**: disk is full or the log directory is not writable — file
  logging fails silently (falls back to stdout-only, matching today's
  behavior) rather than crashing the app.
- **Notifications**: OS notification permission is denied, or the platform has
  no notification center — the app continues normally; the notification is
  simply not shown, and existing in-app completion signals are unaffected.
- **Native menu / Edit menu**: invoked with no focused text field (e.g. Copy
  with nothing selected) — behaves as a no-op, matching standard OS conventions,
  not an error.
- **Prevent-default (US9)**: an in-app feature that intentionally needs its own
  right-click menu, or a form that intentionally needs Ctrl/Cmd+A select-all
  inside a specific input, continues to work — only the browser engine's
  fallback behaviors are suppressed, not the app's own handlers.
- **Auto-update**: the user is on an OS/architecture that the current release
  pipeline does not publish an artifact for — the app fails the update check
  quietly rather than offering a broken install.

## Requirements *(mandatory)*

### Functional Requirements

**Single-instance (US1)**

- **FR-001**: The system MUST ensure at most one running instance of the app
  has the database open at a time.
- **FR-002**: When a second launch is attempted while an instance is already
  running, the system MUST focus/foreground the existing window (restoring it
  if minimized) instead of opening a new window or a new database connection.
- **FR-003**: The second launch attempt MUST exit without performing any
  database migration, seed, or write of its own.

**Favourites → database (US2)**

- **FR-004**: The system MUST store target favourite status as a database
  record associated with the canonical target, not in browser storage.
- **FR-005**: Users MUST be able to star and unstar a target, with the change
  visible immediately and surviving an app restart.
- **FR-006**: The system MUST NOT silently import stale pre-feature
  `localStorage` favourites data as if it were newly-set by the user (see
  Assumptions for the accepted cutover behavior).

**Cleanup overrides → database (US3)**

- **FR-007**: The system MUST store each per-type cleanup action override as a
  database record, not in browser storage.
- **FR-008**: Each change to a cleanup action override MUST be recorded in the
  existing audit trail (who/when/what changed), consistent with Constitution
  Principle II's treatment of decisions that drive filesystem-affecting plans.
- **FR-009**: The fixed catalog of cleanup data types (their labels, stage
  grouping, and default action) is unaffected by this feature — only the
  user's override of the action is relocated to the database.
- **FR-010**: This feature MUST NOT newly wire cleanup-plan generation to these
  overrides; that remains out of scope until a future feature specifies it.

**Window-state (US4)**

- **FR-011**: The system MUST persist the main window's size, position, and
  maximized state across app restarts.
- **FR-012**: On first launch (no prior state), the system MUST use today's
  default size and minimum size.
- **FR-013**: If the persisted position would be off-screen given the current
  display configuration, the system MUST fall back to a sane on-screen default
  rather than opening off-screen.

**Native menu bar (US5)**

- **FR-014**: The system MUST present a native application menu with, at
  minimum: About, Settings, Quit, and a Window menu.
- **FR-015**: The system MUST present a native Edit menu (or platform-equivalent)
  providing Copy, Paste, and Select All using standard OS conventions and
  keyboard shortcuts.
- **FR-016**: The native menu bar's Quit entry MUST invoke the same shutdown
  path as the window's existing close control.
- **FR-017**: This feature MUST NOT introduce or change any native
  right-click/context menu — existing themed in-app context menus are
  unaffected.

**Native theme sync (US6)**

- **FR-018**: The system MUST set the native window chrome to the nearest
  light/dark match of the currently active theme, for each of the app's four
  themes, on platforms where the OS supports app-controlled window theme.
- **FR-019**: Changing the active in-app theme MUST update the native chrome to
  match, without altering CSS theming.
- **FR-020**: On platforms/configurations where native chrome theming is not
  available, this feature MUST be a no-op with no error surfaced to the user.

**Diagnostics log file (US7)**

- **FR-021**: The system MUST write structured log output to a rotating file in
  the OS-standard per-app log location, in addition to (not instead of) the
  existing stdout logging.
- **FR-022**: The log file MUST rotate (by size and/or age) so it never grows
  without bound.
- **FR-023**: The SQLite audit record MUST remain the canonical, authoritative
  history; the log file is a supplementary, human-shareable troubleshooting
  artifact only.

**OS notifications (US8)**

- **FR-024**: The system MUST show an OS notification when the ingest-resolution
  background drain completes meaningful work, when a workflow-run's manifest
  generation completes, and when an approved filesystem plan finishes applying
  (success, partial, or failure).
- **FR-025**: If OS notification permission is unavailable or denied, the
  system MUST continue operating normally without that notification —
  no crash, no blocking, and the outcome remains visible through existing
  in-app and audit/log channels.

**Cleanups (US9 + hardening)**

- **FR-026**: In release builds, the system MUST suppress the browser engine's
  default page-reload shortcut(s) and its native right-click context menu; this
  suppression MUST NOT apply to development builds.
- **FR-027**: The system MUST detect whether it is running inside the Tauri
  shell using the framework's supported detection mechanism rather than a
  hand-rolled internal-API sniff, with no behavior change versus today.
- **FR-028**: The system's runtime list of registered filesystem sources MUST
  be read from the database, never from browser storage, on every screen that
  displays or acts on that list (this feature verifies and, if needed, closes
  any remaining gap — the first-run wizard's own in-progress buffer is exempt,
  since it is pre-registration scratch state, not the runtime list).

**Auto-update (US10)**

- **FR-029**: The system MUST be able to check a configured update endpoint for
  a newer signed release and report whether one is available.
- **FR-030**: The system MUST verify an update's signature against the app's
  embedded public key before installing it, and MUST refuse installation on
  verification failure.
- **FR-031**: Update checks and installs MUST fail gracefully (no crash, no
  partial/corrupt install) when the endpoint is unreachable or the artifact is
  invalid.
- **FR-032**: The signing keypair and publishing pipeline this story depends on
  MAY be delivered as a tracked follow-up rather than as part of this feature's
  implementation, provided the application-side check/verify/install
  integration is complete and functionally ready against a documented endpoint
  shape.

**Shared infrastructure**

- **FR-033**: All new database-backed state introduced by this feature (US2,
  US3) MUST follow existing migration, repository, and contract conventions
  (root-relative persistence, reviewable/auditable where mutation-adjacent, no
  new bespoke storage mechanism).
- **FR-034**: None of the integrations in this feature MAY weaken the existing
  PixInsight boundary (Constitution III) — no new surface calibrates, debayers,
  registers, integrates, drizzles, stacks, or edits images, and no new surface
  performs filesystem mutation outside the existing reviewable-plan pipeline.

### Key Entities *(include if feature involves data)*

- **Target Favourite**: Marks a canonical target as favourited by the user.
  Attributes: the target it refers to, when it was favourited. Replaces the
  `localStorage`-only stub.
- **Cleanup Type Override**: The user's chosen action (keep/archive/delete) for
  one fixed cleanup data type, overriding that type's built-in default.
  Attributes: which data type it overrides, the chosen action, when it was last
  changed. Replaces the `localStorage`-only stub. Does not replace or duplicate
  the fixed catalog of data types itself (labels/stage/default action stay
  app-defined).
- **Window State** (native-plugin-managed, not app database data): size,
  position, and maximized flag for the main window, persisted by the platform
  integration itself rather than the app's own schema.
- **Diagnostics Log Entry** (file, not database data): a line of structured,
  human-readable troubleshooting output written to the rotating on-disk log
  file; distinct from, and never a substitute for, the SQLite audit record.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Launching the app while an instance is already running never
  results in two windows or two processes holding the database open — verified
  across 100% of repeated-launch attempts in testing.
- **SC-002**: A favourited target's status is recoverable from the database
  alone (no browser storage involved) after an app restart, in 100% of tested
  cases.
- **SC-003**: A per-type cleanup action override is recoverable from the
  database after an app restart, and every override change produces exactly one
  corresponding audit entry.
- **SC-004**: After a manual resize/move and restart, the window reopens within
  a visually negligible tolerance of its last size and position in at least
  95% of relaunches (allowing for OS-level rounding/DPI quirks); it never
  reopens off-screen.
- **SC-005**: For each of the app's four themes, the native window chrome (on
  platforms that support it) visibly matches that theme's light-or-dark family,
  confirmed on at least one Windows, macOS, and Linux build.
- **SC-006**: A user (or support engineer) can locate a readable, rotated
  diagnostics log file for the app without needing to run it from a terminal,
  on all three desktop platforms.
- **SC-007**: When the app is not focused, a completed plan apply, ingest-resolution
  drain, or workflow-run manifest generation surfaces an OS notification in
  under 5 seconds of completion, in testing.
- **SC-008**: In a release build, the reload shortcut and the browser engine's
  native right-click menu are inert in 100% of manual verification passes;
  neither is affected in development builds.
- **SC-009**: A tampered or unsigned artifact placed at the update endpoint is
  rejected in 100% of tested attempts; a genuinely signed newer release is
  detected and installable end-to-end in a controlled test.
- **SC-010**: Zero production code paths read the runtime registered-source
  list from browser storage (confirmed by code audit); the first-run wizard's
  pre-registration buffer is the sole, explicitly-exempted exception.

## Assumptions

- This is a single-user, single-machine desktop app; none of these
  integrations introduce multi-device sync, multi-account, or cross-machine
  state (window position, favourites, and overrides are all local-machine
  concerns).
- A clean cutover from `localStorage` to the database for favourites and
  cleanup overrides is acceptable: users do not need an automatic one-time
  import of pre-feature browser-storage values. If a low-cost best-effort
  import is trivial to add during implementation it MAY be included, but it is
  not a requirement of this spec.
- Native window theme setting, the native application menu, and any future
  tray affordance are desktop-shell (Tauri) concerns only; they have no
  equivalent in a hypothetical future non-desktop backend and do not appear in
  any portable contract (Constitution V is unaffected because none of this
  crosses the UI-to-core contract boundary except the notification triggers,
  which react to already-published domain events).
- The signing keypair and CI publishing pipeline auto-update depends on do not
  exist in this repository today (unlike the reference implementation this
  design is modeled on) and are treated as separate, sequenced follow-up work;
  US10's acceptance is scoped to the in-app integration being ready against a
  documented endpoint/manifest shape.
- "Long background tasks" for notification purposes (US8) are limited to the
  three operations named in FR-024; adding notifications for other operations
  is not in scope of this feature.
- The four existing CSS themes each have an unambiguous nearest light/dark
  mapping for native-chrome purposes; the exact mapping is a plan/research-level
  decision, not a product-level open question.
