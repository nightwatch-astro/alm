---
id: J01
title: Register data source folders and keep them current
version: 9
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [setup, data-sources]
interfaces: [desktop-ui]
trace: [docs/product/journeys/J01-first-run-setup-data-sources/journey.md, docs/product/journeys/J01-first-run-setup-data-sources/deltas/2026-07-14-jval-docdrift.md, docs/product/journeys/J01-first-run-setup-data-sources/deltas/2026-07-14-q15-t123.md, docs/product/journeys/J01-first-run-setup-data-sources/deltas/2026-07-14-q15-t125.md, docs/development/journey-run-2026-07-14.md, PR-440, PR-686, PR-404, PR-405, PR-826, issue-647, spec-030 FR-130-FR-134, PR #872, PR #893, PR #894, PR #908, PR #907, PR #903, PR #901, PR #904, PR #911, PR #925, PR #1176, PR #1185, spec-061 FR-004, spec-061 FR-005, PR #1719 (an unparsable FITS/XISF header is an error, not a crash)]
---

## Goal
An astrophotographer takes a fresh PlateVault install from an empty database
to a configured library — light, calibration, project-output, and inbox
folders registered, a processing tool pointed at, and an observing site set —
then keeps those data sources current over time (rescan, remap a moved
drive, disable/re-enable, retire an offline one) without ever risking an
unreviewed filesystem mutation. Done means: setup is marked complete and
skipped on every later relaunch, and every data-source lifecycle action
produces both a visible answer-back and a durable audit record.

## Preconditions
- P1: Empty database (first launch), or the user has chosen **Settings →
  Advanced → Restart first-run setup** and confirmed (a confirm-gated
  control; the pane's onboarding controls are separate and non-destructive).
- P2 (for S9–S14 only): setup has already been completed with at least one
  registered source.

## Steps
### S1 — Open the setup wizard {#S1}
- **Do:** Launch the app for the first time, or trigger a first-run restart
  (P1) and confirm.
- **Expect:** The app opens a 7-step wizard: Language, Source Folders,
  Processing Tools, Configuration, Observing Site, Confirm, Scan. On a
  restart, all previously registered folders are pre-filled. The step bar
  above the wizard content renders each step as a real, focusable button
  (not an inert div): a completed step is always a free backward jump, and a
  jump forward is gated on the steps between here and there being valid
  (Scan is never a plain jump target — entering it is what runs
  registration).
- **Expect (negative):** A first-run restart never deletes previously
  registered folders. Re-confirming an unchanged, pre-filled restart buffer
  no longer gets stuck on Confirm behind a misleading "batch registration
  failed" banner — see S6.
- **Trace:** PR-440 (Observing Site step introduced as part of spec 030's
  wizard rewrite, making the wizard 6 steps), PR-686 (map picker added to
  that step); PR #893 fixes #512 (step bar renders real buttons,
  `apps/desktop/src/ui/WizardShell.tsx`), fixes #704 (restart re-confirm no
  longer sticks); spec-061 US1 inserted the Language step ahead of
  everything else, making it 7 steps (see S1a).

### S1a — Choose a language (Step: Language) {#S1a}
- **Do:** Pick a language card, each labelled with its own native name and a
  decorative flag (e.g. "🇬🇧 English (UK)", "🇧🇷 Português (Brasil)").
- **Expect:** This is always the wizard's first step, before any step that
  explains itself in prose. The base locale starts selected. Picking a
  different one applies to the wizard's own interface immediately — no
  reload, no loss of anything entered on a later step already visited in
  this session (research D2: the choice is a live re-render, not a
  navigation). Every option is reachable and selectable by keyboard alone,
  and the currently-selected option is exposed to assistive technology
  (`aria-pressed`), with the accessible name coming from the native name,
  never the flag.
- **Expect:** The choice carries through the rest of setup and into the main
  app once Finish (S8) completes, and survives a full app restart
  thereafter — the same durable preference Settings → Appearance's language
  control (J10) reads and writes.
- **Expect (negative):** A language picked here is never lost by navigating
  forward and then back — Back-navigation from any later step, or the
  step-tab bar, always reaches this step again, and the earlier choice is
  still selected. A locale a translation key is missing for falls back to
  the base locale for that string rather than showing a raw key or a blank
  region.
- **Trace:** `apps/desktop/src/features/setup/steps/StepLanguage.tsx`;
  `apps/desktop/src/features/setup/SetupWizard.tsx` (step ordering,
  `LocaleProvider`); `apps/desktop/src/data/locale.tsx`
  (`useLocale`/`changeLocale`); `apps/desktop/src/data/locale-meta.ts`
  (native name + flag, accessible naming). Evidence: spec-061 FR-004,
  FR-005, US1.

### S2 — Add source folders (Step: Source Folders) {#S2}
- **Do:** For each folder category — Light frames, Calibration, Project
  outputs, Inbox — add zero or more folders via the native OS picker,
  choosing **organized** or **unorganized** for each (Inbox has no such
  choice; it is unorganized by definition).
- **Expect:** Required categories (Light frames, Project outputs) are listed
  before optional ones (Calibration, Inbox), grouped under explicit
  "Required"/"Optional" headings. Every category carries a keyboard- and
  screen-reader-accessible tooltip explaining what it is for, including that
  Project folders are created later in a guided step. There is no
  scan-depth (Recursive/Single level) control on a source row — every scan
  is recursive; the Confirm summary (S6) always reads "Recursive" for scan
  depth accordingly.
- **Expect:** Light frames AND Project outputs are required categories — the
  wizard blocks progress past this step while either is empty; Calibration
  and Inbox stay optional. At add time, an empty path, a path already
  registered under the same category, a path already registered under a
  different category, and a path that is a parent or subfolder of an
  already-added path (in the working buffer, not yet the backend) are each
  rejected inline with a distinct, accessible error.
- **Expect:** A path that doesn't exist, isn't a directory, or isn't
  readable is accepted into the working buffer at add time (there is no
  client-side check for these) and is only rejected when actually
  registered — batched at S6 (Confirm), where a failing entry surfaces as
  part of a batch-failure message rather than an inline per-row error at add
  time. A path that overlaps a root already registered from a *previous*
  session/restart (not visible in this buffer) follows the same deferred
  path: accepted into the buffer at add time, then rejected at S6
  registration with a distinct `path.overlaps_existing` reason, never
  silently allowed to register both.
- **Expect (negative):** Nothing is registered with the backend at this
  point — this is a working buffer the user can still edit; no source
  registration call fires until S6 (Confirm). An exact-duplicate path is
  never allowed through as a bypassable warning — registering it is always
  a hard rejection.
- **Trace:** `apps/desktop/src/features/setup/sources-store.ts`
  `REQUIRED_KINDS`, `validatePath` (empty/same-kind/cross-kind checks only);
  `crates/app/core/src/first_run.rs` `check_path`/`check_duplicate`
  (existence, directory, permission, duplicate checks — backend-only);
  `check_overlap` (PR #893, fixes #501 — parent/nested-root rejection,
  exact-duplicate escalated from a bypassable `Warning` to `Blocking`; PR
  #911 case-folds this comparison on Windows so a case-variant of an
  already-registered root, e.g. `C:\Foo` vs `c:\foo`, is caught too — Unix
  stays case-sensitive). PR #908 fixes #496/#497/#502/#714 (required-first
  ordering, category tooltips, add-time buffer overlap/duplicate
  validation).

### S3 — Point at a processing tool (Step: Processing Tools) {#S3}
- **Do:** Select PixInsight/WBPP (or another supported tool), browse to its
  executable, or skip.
- **Expect:** The step accepts skip or default with no error and carries the
  choice (or its absence) into S6's summary. The executable picker offers
  only executable-typed files (no "All files" filter). A picked path that
  isn't a plausible executable is rejected inline with an error (a
  best-effort extension check; a no-extension path is treated as plausibly
  valid, matching Linux-style binaries); status shows as one of Not
  detected / Detected / Invalid.
- **Expect (negative):** An invalid tool-path pick does not itself block
  Continue at the wizard level — only the in-step status/error changes.
- **Trace:** `apps/desktop/src/features/setup/steps/StepTools.tsx`. PR #907
  fixes #511 (reject a non-executable pick, e.g. `.zip`) and #510
  (consolidated status pill, icon-only redetect control).

### S4 — Configure basic settings (Step: Configuration) {#S4}
- **Do:** Confirm or adjust basic configuration — including the default
  source-protection level (protected / normal / unprotected), the app
  theme, and the display density applied to newly registered sources — or
  skip.
- **Expect:** The step accepts skip or default with no error; leaving the
  protection level untouched keeps it at "protected". The Theme control is
  live and bound to the same theme runtime Settings → Appearance uses:
  picking one applies it immediately to the wizard itself. Choosing System
  (the default) now resolves a dark OS preference to Observatory Cool
  instead of Observatory (light resolution, Warm Slate, is unchanged). The
  Density control's choice previews live during setup — the wizard applies
  its own `density-*` class since it renders outside the main app shell.
- **Expect (negative — correction, 2026-07-20):** The wizard's Theme
  control is a plain `<select>` listing all 6 registry themes unfiltered
  (`system` + Warm Clay, Warm Slate, Observatory, Espresso, Observatory
  Cool · Light, Observatory Cool), not grouped and not limited to the 4
  canonical themes — unlike Settings → Appearance (J10/S2), which now
  offers only the 4 canonical themes grouped by family since PR #1176.
  This is a real, currently-accurate inconsistency: `StepCatalogs.tsx`
  maps over the full `THEMES` array rather than the same `enabled`-filtered
  list `General.tsx` uses, so Warm Clay and Espresso remain pickable here
  even though they are hidden from Settings.
- **Expect (negative):** none otherwise scoped for this step (theme/density
  choices made here are the same durable preference used everywhere else
  in the app, not a setup-only draft).
- **Trace:** `apps/desktop/src/features/setup/steps/StepCatalogs.tsx`
  `DefaultProtectionControl`, `ThemeControl`, `DensityControl`;
  `apps/desktop/src/features/setup/SetupPage.tsx`. PR #872 fixes #504 (theme
  control was a disabled single-option stub before this) and #505 (density
  had no live preview in the wizard before this).

### S5 — Register an observing site (Step: Observing Site) {#S5}
- **Do:** Use the map picker or manual entry to set Name / Latitude /
  Longitude / Elevation / Timezone.
- **Expect:** The step can be left entirely blank; Continue is not blocked
  while Latitude and Longitude are both empty. As soon as coordinates are
  entered, Name becomes required — Continue blocks on a blank Name the same
  way it blocks on an out-of-range coordinate, matching the equivalent
  Settings → Target Planner site editor. Once Name and both coordinates are
  filled, Continue blocks until Latitude is in [-90, 90], Longitude is in
  [-180, 180], and Elevation (if given) parses as a number.
- **Expect (negative):** Valid coordinates are never silently dropped on
  Finish for lack of a Name — Continue itself refuses to advance instead.
- **Expect:** Values entered here carry into S6's summary and, on Finish
  (S8), are saved as both the default and the active observing site with a
  fixed astronomical-twilight/0°-horizon default (changeable later in
  Settings → Target Planner, which also exposes twilight and horizon —
  fields this wizard step does not).
- **Trace:** PR-440, PR-686;
  `apps/desktop/src/features/setup/steps/StepSite.tsx`
  (`siteStepHasSite`, `siteStepError`);
  `apps/desktop/src/features/setup/SetupWizard.tsx` (`canProceed` for the
  Site step). PR #903 fixes #516 (Name required once coordinates are
  entered).

### S6 — Confirm sources (Step: Confirm) {#S6}
- **Do:** Review the summary of every category added across S2–S5, then
  proceed.
- **Expect:** The summary states, per folder, category, **organized or
  unorganized** state, and scan depth (always "Recursive" — see S2); it
  also lists enabled processing tools with their configured path and a
  "what happens next" note. Proceeding here is what actually registers
  every source with the backend and starts scanning.
- **Expect:** If any folder fails to register for a genuine reason (invalid
  path, overlap, permission), the wizard shows a batch-failure message and
  does not advance to Scan. A row that "fails" only because it is an
  exact-path duplicate of an already-registered source (the pre-filled
  restart case, P1) is not treated as a failure — it is a benign no-op, and
  the wizard advances to Scan on an otherwise-clean batch instead of
  blocking on a misleading failure banner.
- **Expect (negative):** No scan starts before the user leaves this step.
- **Trace:** `apps/desktop/src/features/setup/steps/StepConfirm.tsx`;
  `apps/desktop/src/features/setup/SetupWizard.tsx` `handleEnterScan`;
  `apps/desktop/src/features/setup/sources-store.ts` `flushToDB`
  (`alreadyRegistered` flag, PR #893 fixes #704). PR #901 fixes #515
  (organization state was previously omitted from the summary).

### S7 — Scan registered folders (Step: Scan) {#S7}
- **Do:** Wait for every registered folder to scan.
- **Expect:** Each source that was actually registered by this flush reaches
  a terminal state, including "0 items" for an empty folder. A folder that
  was **genuinely already registered before this wizard run** (S6's
  benign-no-op case, from a prior wizard session) is skipped here rather
  than scanned again under a synthetic root id — it was already ingested
  under its real, existing root. A source that registered on an *earlier
  attempt within this same wizard session* but was never actually scanned
  (e.g. because an unrelated source in that same batch failed and the user
  retried) is rescanned here rather than silently dropped from the Scan step
  with no items and no error. Per-source "Detected types" and the
  file-count chip account for unclassified (no/unmapped IMAGETYP) files
  and calibration masters, so the shown breakdown always reconciles with the
  folder's total file count instead of silently under-counting. An expanded
  folder table's root row shows "(root)" in the Folder/File cell, never a
  blank cell.
- **Expect (negative):** Finish never enables while any source is still
  scanning. No source registered earlier in this wizard session
  disappears from the Scan step with neither a scan result nor an error,
  merely because it carries the `alreadyRegistered` flag.
- **Expect (negative):** One unreadable file never takes the app down: a
  `.fits`/`.fit`/`.fts` or `.xisf` file whose header cannot be parsed — a
  damaged file, or a non-astronomical one that merely carries the extension —
  becomes a metadata error for that file and the scan continues over the rest
  of the folder.
- **Trace (robustness):** `crates/metadata/fits/src/lib.rs:83` and
  `crates/metadata/xisf/src/lib.rs:60` (header parsing runs under
  `catch_unwind`; a caught panic becomes `MetadataExtractError::Parse`, which
  every caller already handles); PR #1719.
- **Trace:** `apps/desktop/src/features/setup/steps/StepScan.tsx` (PR #893
  fixes #704 — scanning an already-registered row via the path-as-rootId
  fallback previously failed the `registered_sources` join and orphaned
  inbox items). PR #904 fixes #513 (unreconciled counts, hidden
  unclassified/masters, blank root row). PR #925 fixes #916 (a
  same-session-retry `alreadyRegistered` source that was never actually
  scanned is now rescanned instead of vanishing).

### S8 — Finish setup {#S8}
- **Do:** Click Finish.
- **Expect:** Setup is marked complete and the app lands on the Inbox. The
  completion flag persists — fully quitting and relaunching the app goes
  straight to Inbox, never back to `/setup`.

### S9 — Rescan a data source {#S9}
- **Do:** From Settings → Data Sources, open a source card's "⋯" menu and
  choose Rescan.
- **Expect:** The scan re-runs without re-prompting for a path, with an
  explicit started→finished signal and a count delta at the control. A
  user-initiated rescan writes a durable, workflow-severity audit row.
- **Expect (negative):** An automatic/periodic rescan writes only a
  diagnostic-severity audit row, never a workflow one.
- **Trace:** issue-647, spec-030 FR-130-FR-134, PR-826. PR #894 fixes #562:
  every per-source action described in S9–S14 (Rescan, Reconcile, Remap,
  Edit protection, Disable/Enable, reveal, Delete) is now reached through
  one consolidated kebab (`role="menu"`/`menuitem`) instead of separate
  card buttons; the answer-back and audit contract for each action is
  unchanged, only its entry point moved.

### S10 — Remap a data source {#S10}
- **Do:** Click Remap, paste a different valid existing path, click Verify,
  then — only if Verify succeeds — click Apply remap.
- **Expect:** Verify checks the new path against every recorded item for
  that root (all confirmed `file_record` rows plus any pending inbox items —
  not a bounded sample) with no file movement, reporting an exhaustive
  "{matched} of {total} recorded items were found" count; a root with zero
  recorded items gets its own distinct message rather than reading as a
  vacuous "all found." Apply remap persists the new path in PlateVault's own
  record and writes a durable audit row recording old→new path.
- **Expect:** Editing the path after a Verify invalidates that result — Apply
  remap becomes unavailable again until a fresh Verify runs against the
  edited path.
- **Expect (negative):** Verify on an empty or nonexistent path never
  reports success; Apply remap is not clickable before a successful Verify;
  no file on disk moves at any point, regardless of outcome. Applying an
  unverified remap is refused server-side (not merely UI-disabled) with a
  `remap.not_verified` error and a `refused` audit row — a scripted or
  replayed Apply call cannot bypass the gate through the UI alone. The
  server independently recomputes the verification result rather than
  trusting a caller-supplied `verified` flag, so a stale preview or a
  direct IPC call also cannot bypass the gate.
- **Trace:** issue-647, spec-030 FR-130-FR-134, PR-826;
  `apps/desktop/src/features/settings/RemapRootDialog.tsx`. PR #893 fixes
  #560 (`relative_paths_for_root` replaces the old 5-row
  `sample_relative_paths`) and #707 (`apply_root_remap` now enforces the
  verified flag at the IPC boundary,
  `crates/app/core/src/first_run.rs`); PR #911 closes the remaining gap by
  having `apply_root_remap` recompute verification itself
  (`compute_remap_verification`) instead of trusting the caller's claim.

### S11 — Disable / re-enable a data source {#S11}
- **Do:** Click Disable on a source card, confirm; click Enable on the same
  card to re-enable.
- **Expect:** The state visibly flips and persists across reload; a disabled
  source drops out of scan/ingest; each transition writes a durable audit
  row with before→after state.
- **Expect (negative):** Disabling requires a confirm step (it stops a
  source from being scanned/ingested, so it is confirm-gated); re-enabling
  is restorative and applies immediately with no confirm step; disabling
  never hides the source's prior history.
- **Trace:** PR-404; issue-647, spec-030 FR-130-FR-134, PR-826;
  `apps/desktop/src/features/settings/DataSources.tsx` (disable/enable
  handlers), `DataSources.disable-delete.test.tsx`.

### S12 — Delete (un-register) a data source {#S12}
- **Do:** Open a source card's "⋯" menu and choose Delete, whether the
  source is online or offline.
- **Expect:** A confirm appears; confirming un-registers the source and
  writes a durable audit row. If the source has dependent records
  (sessions/projects), Delete is blocked/disabled with an explanatory
  message instead.
- **Expect (negative):** Delete never removes files from disk; it never
  succeeds while dependent records exist.
- **Trace:** PR-404; issue-647, spec-030 FR-130-FR-134, PR-826;
  `DataSources.disable-delete.test.tsx`. PR #894 fixes #559: Delete is now
  reachable for online sources too (previously the control was withheld
  entirely from an online card, forcing a disable-first detour).

### S13 — Set / remove a per-source protection override {#S13}
- **Do:** Set a protection override on a source, confirm it is listed, then
  remove it.
- **Expect:** The change is visible in the pane and confirmed by a backend
  readback; each of set and remove writes a durable audit row with a
  resolvable `auditId`; "Restore defaults" states which settings it resets
  and every one of them is visible somewhere in the pane.
- **Expect (negative):** Merely opening or reading the Data Sources pane
  produces no audit row.
- **Trace:** issue-647, spec-030 FR-130-FR-134, PR-826.

### S14 — Reveal a source folder in the OS file manager {#S14}
- **Do:** Click the "Show in File Explorer" (or platform-equivalent) control
  on a source card.
- **Expect:** The OS-native file manager opens at exactly that folder, not a
  parent directory.

## Success criteria
- SC1: S1–S8 (including S1a) complete once per install; after S8,
  relaunching the app never re-shows `/setup`.
- SC2: Each of S9–S13 produces both a visible answer-back at its control and
  a durable audit row; S10 and S12 leave zero filesystem mutation beyond
  PlateVault's own registration state.
- SC3: Required-category gating holds at S2 — Light frames and Project
  outputs block progress when empty; Calibration and Inbox never do.

## Known gaps
<!-- - G1: <step or environment that cannot be validated, and why> -->

## Delta log

- **Δ2** 2026-07-15 · S4 · behavior-change
  The wizard's Configuration step now has a live Theme control (was a
  disabled single-option stub) and a Density control whose choice actually
  previews live during setup (the wizard renders outside the main shell, so
  it needed its own density class to reflect the choice).
  Evidence: PR #872 (fixes #504, #505) · by: journey-scribe (intent-gated)

- **Δ3** 2026-07-15 · S1, S2, S6, S7, S10 · behavior-change
  Registering a source now rejects roots that overlap (parent/nested with)
  an already-registered or same-batch root, and an exact-duplicate is a
  hard rejection rather than a bypassable warning. Remap's Verify reports
  an exhaustive matched/total count instead of a 5-row sample, and Apply is
  refused server-side (not just UI-disabled) when unverified. A restart
  re-confirm of unchanged, pre-filled folders no longer sticks on a
  misleading batch-failure banner, and Scan no longer tries to re-scan
  already-registered rows. The wizard's step-tab bar is now real, focusable
  buttons with free backward jumps and validation-gated forward jumps.
  Evidence: PR #893 (fixes #501, #560, #707, #704, #512) · by: journey-scribe
  (intent-gated)

- **Δ4** 2026-07-15 · S9, S12 · behavior-change
  Every per-source card action (Rescan, Reconcile, Remap, Edit protection,
  Disable/Enable, reveal, Delete) is now reached through one consolidated
  "⋯" kebab menu instead of separate card buttons. Delete is now reachable
  for online sources too (previously withheld from a card entirely while
  its source was online).
  Evidence: PR #894 (fixes #559, #562) · by: journey-scribe (intent-gated)

- **Δ5** 2026-07-17 · S2, S3, S5, S6, S7, S10 · behavior-change
  Source Folders: required categories now list before optional ones under
  explicit headings, every category has a help tooltip, adding a folder
  checks it against everything already in the working buffer (not just
  against previously-registered roots) and rejects duplicates/overlaps
  inline, and the scan-depth control is gone (every scan was already
  recursive). Processing Tools: the executable picker rejects a
  non-executable pick inline instead of showing a false "OK". Observing
  Site: Name is now required as soon as coordinates are entered — previously
  valid coordinates could be silently dropped on Finish. Confirm: the
  summary now states each folder's organized/unorganized state, not just
  scan depth. Scan: per-source detected-type counts reconcile with the total
  file count (unclassified files and masters no longer disappear from the
  breakdown) and an expanded root row reads "(root)" instead of blank.
  Remap: the overlap check case-folds on Windows, and Apply independently
  re-verifies server-side instead of trusting a caller-supplied flag.
  Evidence: PR #908 (fixes #496, #497, #502, #509, #714), PR #907 (fixes
  #511, #510), PR #903 (fixes #516), PR #901 (fixes #515), PR #904 (fixes
  #513), PR #911 (carried nJ01a review nits, no matching issues) · by:
  journey-scribe (intent-gated)

- **Δ6** 2026-07-17 · S7 · behavior-change
  Scan now distinguishes a source that is *genuinely* already registered
  from a prior wizard session (still skipped, unchanged) from one that
  registered on an earlier attempt within the *same* session but was never
  actually scanned — e.g. because an unrelated source in that batch failed
  and the user retried. The latter case previously vanished from the Scan
  step silently (no items, no error); it is now correctly rescanned.
  Evidence: PR #925 (fixes #916, carried nJ01a review nit deferred out of
  PR #911's Rust-only file boundary) · by: journey-scribe (intent-gated)

- **Δ7** 2026-07-20 · S4 · behavior-change
  Choosing System (the wizard's default) now resolves a dark OS preference
  to Observatory Cool instead of Observatory. Also documents a real,
  pre-existing inconsistency the design-refresh wave exposed: the wizard's
  Theme select still lists all 6 registry themes unfiltered, while Settings
  → Appearance (J10/S2) now shows only the 4 canonical themes grouped by
  family — the picker-filtering change in PR #1176 was not applied to the
  wizard's control.
  Evidence: PR #1185 (merged, default dark theme), PR #1176 (closes #1139,
  Settings picker filtering not mirrored in
  apps/desktop/src/features/setup/steps/StepCatalogs.tsx) · by:
  journey-scribe (intent-gated)

- **Δ8** 2026-07-20 · S1, +S1a · behavior-change
  A new Language step is now the wizard's first step, ahead of Source
  Folders and everything else — the wizard is 7 steps, not 6. Picking a
  language applies to the wizard's own interface immediately (no reload),
  survives Back-navigation back to this step, and carries through Finish
  into the running app and across a full restart. Every option shows its
  own native name and a flag; the accessible name is the native name.
  Evidence: spec-061 FR-004, FR-005, US1 · by: journey-scribe (intent-gated)

- **Δ9** 2026-08-24 · S7 · behavior-change
  A file whose FITS or XISF header cannot be parsed now reports a metadata
  error for that file instead of crashing the app: an inbox or source scan over
  one damaged file previously took the whole app down, because the header
  parser panicked on a byte offset inside a multi-byte character.
  Evidence: PR #1719 (54615f80e) · by: journey-scribe (intent-gated)
