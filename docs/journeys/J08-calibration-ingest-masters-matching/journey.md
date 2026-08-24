---
id: J08
title: Ingest calibration masters and match them to sessions
version: 12
status: draft
last_reviewed: 2026-07-15
actors: [astrophotographer]
surfaces: [inbox-confirm, calibration]
interfaces: [desktop-ui]
trace: [docs/product/journeys/J08-calibration-ingest-masters-matching/journey.md @ 66026463, deltas/2026-07-14-jval-docdrift.md, deltas/2026-07-14-q15-t122.md, deltas/2026-07-14-q16-t128.md, deltas/2026-07-14-q16-t129.md, deltas/2026-07-14-q16-t131.md, deltas/2026-07-14-q16-t132.md, deltas/2026-07-14-q16-t133.md, docs/journeys/J08-calibration-ingest-masters-matching/journey.md pilot (PR #848), spec-040 MasterDetector, spec-030 FR-135-FR-140, issue-619, issue-620, PR #851, PR #849, PR #910, PR #939 (fixes #551), spec-054-adaptive-detail-dock (FR-001, FR-004), PR #1032 (issue #868 — compatible sessions computed), PR #1730 (inference predicates), PR #1736 (guarded JSON reads), PR #1737 (unreadable FITS header is an error), PR #1742 (camera body identity dimension), PR #1692 (age limit and gain/binning match-required settings honoured)]
---

## Goal
An astrophotographer gets calibration master frames (darks/flats/bias) into
the library as individually tracked items, then matches them against
acquisition sessions that need calibration. Done means: every ingested
master is a distinct, correctly typed Calibration-page row with trustworthy
(never fabricated) fingerprint data, and every session assigned a master was
assigned through an explicit, confirmable action — never silently.

## Preconditions
- P1: A calibration root is registered (Journey 1, S2).
- P2: Master and light frames are available to ingest.

## Steps
### S1 — Ingest calibration files through the Inbox {#S1}
- **Do:** Point the calibration root at a folder containing several master
  files (e.g. two darks, a flat, a bias) and ingest through the same Inbox
  pipeline used for lights.
- **Expect:** A file classifies as an individual master item — with its own
  type and fingerprint (gain, temperature, binning, filter where relevant) —
  when: an authoritative stack/combine count in its metadata
  (`STACKCNT`/`NCOMBINE`) is greater than 1; or, when no such count is
  present, its filename/path or `IMAGETYP` carries a master naming
  convention. When a stack/combine count IS present it is decisive, whichever
  detector read it, and overrides a naming convention that disagrees (e.g. a
  file named `dark_master_stacked.fit` whose count is 1 is NOT a master).
- **Expect:** Master naming is recognised by delimiter-bounded tokens, not by
  substring: `masterDark.xisf` and a `Masters/` folder are masters, while a
  light frame filed under `Grandmaster Nebula/` or `masterclass_notes/` is
  not. A frame type spelled `dark_flat` is read as `dark_flat`, never as
  `dark`, and reordering the words does not flip the verdict.
- **Expect (negative):** A folder of masters never classifies as one
  folder-level aggregate item; a raw (non-stacked) dark/flat/bias with an
  ordinary filename, no master naming, and no decisive stack count never
  appears as a master; a stack count of 1 is never overridden by a
  "master"/"_stacked" filename into a false-positive master.
- **Expect (negative):** A file whose header cannot be read is never
  classified as a master on the strength of its filename. A FITS file in which
  the parser recognises no keyword is an extraction error, not empty metadata,
  so path inference is never reached for it: the file lands in the item's
  unclassified/needs-review population for the user to type by hand
  (J11/S1–S2), instead of appearing on the Calibration page as a master
  backed by no header evidence at all.
- **Trace:** spec-040 MasterDetector; PR #851 (issue #753 — decisive header
  evidence outranks a naming-only verdict regardless of detector registration
  order); `crates/calibration/master-detect/src/lib.rs`,
  `crates/calibration/master-detect/src/pixinsight.rs` (the PixInsight
  detector reports and honours the stack count, mirroring the Siril one),
  `specs/tiny/inference-predicates-and-aggregation.md`, PR #1730;
  `crates/metadata/fits/src/lib.rs:19-20`, `:102` (`extract` returns
  `MetadataExtractError::Parse` when no keyword was recognised),
  `crates/app/inbox/src/classify.rs:384-393`
  (`unclassified_files`, `EvidenceSource::None` at `:826-829`), PR #1737.

  In the Inbox list itself (pre-confirm), a materialized single-file master
  item now reads by its own authoritative `frameType` rather than the
  legacy folder-level `groupFrameType`, so a lone master item no longer
  mislabels as "Mixed". The classification pill in the Type column is
  quieter (no longer louder than the duplicate frame-type text already
  shown in the Format column for master rows), and the former "Detection"
  column is renamed "Path" and shows the source root's own basename for a
  root-level row instead of a literal "(root)" placeholder shared
  indistinguishably across every root. PR #910 fixes #550, #555, #556
  (`apps/desktop/src/features/inbox/InboxList.tsx`,
  `inboxStatsFromItems.ts`, `grouping.ts`). #549 (mixed-folder placeholder
  double-counting extracted masters) was investigated but is explicitly
  left open — the reporter found no safe frontend-only fix; it needs a
  backend change in `crates/app/inbox`/`crates/persistence/db` (parent
  leaf-folder rows are never retired once single-type sub-items are
  materialized).

  A master item's own pre-confirm Inbox detail view never claims the
  required-attribute gate is "all clear" when it has no per-file metadata
  to evaluate: masters bypass `classify()`'s per-file metadata persistence
  (`crates/app/inbox/src/metadata.rs`), so `fileMetadata` is always empty
  for a master item, and the detail's "No file metadata" empty state now
  appends an explicit caveat ("Required-attribute status is checked when
  you confirm.") instead of silently implying nothing is missing — the
  backend's own `inbox.missing_path_attributes` gate at confirm time is
  independent and can still reject the item. The underlying gap (masters
  never getting a real per-file metadata row) is a backend/data-model
  change still open, overlapping PR #854's in-flight
  `classify.rs`/`confirm.rs`/`reclassify.rs` work; this fix only stops the
  detail view from implying certainty it doesn't have.
  Evidence: PR #939 (fixes #551) — `apps/desktop/src/features/inbox/
  InboxDetail.tsx`.

### S2 — Confirm and register masters {#S2}
- **Do:** Confirm and apply the inbox item(s) covering the ingested masters.
- **Expect:** Each master registers into the calibration store as its own
  item.

### S3 — Browse the Calibration page {#S3}
- **Do:** Open the Calibration page.
- **Expect:** One row per master file. When the master's camera is
  registered under a friendly name in Settings → Equipment, the row (and
  detail, S4) shows that name instead of the raw instrument string the
  capture program wrote into the file header — matching is
  case-insensitive and covers every alias, and renaming the camera in
  Settings updates the list immediately. A master whose camera is not
  registered still shows the raw header string; a master with no camera
  recorded stays blank. This resolution is display-only — calibration
  match scoring still compares the raw header values. Fingerprint columns are
  kind-conditional per an explicit applicability matrix — a dark's
  temperature/gain columns don't apply to a bias and render as an explicit
  not-applicable marker, never inferred from missing data. Sort headers,
  search, and group-by work; a kind filter appears once a second kind
  exists; a search and/or kind filter that matches nothing reads as a
  filter miss — naming the active filter and offering a "Clear filters"
  action — not an empty library, and only when showable masters actually
  exist (a library holding only never-shown kinds still gets the
  onboarding "run a scan" copy, not a misleading filter-miss state).
  Composed identifying strings (meta lines, cells) omit absent
  tokens rather than showing a placeholder inside the joined string. Master
  *light* frames never appear here. Only dark/flat/bias kinds surface in
  this v1 — `dark_flat` and `bad_pixel_map` are out of scope by design.
- **Expect (negative):** A metadata-less master never shows a fabricated
  value such as "Gain 0 · Exposure 0s · Size 0 KB"; no missing numeric ever
  renders as 0; a missing value never carries a source pill, while a real 0
  always renders "0" with its source pill.
- **Expect (negative):** One session whose stored frame list is unreadable
  never empties this page: the master list and the session list both still
  return every intact row, because each JSON read over that column is
  guarded. (The same corruption does block raw sub-frame cleanup outright —
  see J06/S5 — which is deliberate: cleanup can destroy files, a list
  cannot.)
- **Trace:** issue-619, issue-620, spec-030 FR-135-FR-140; PR #849 (missing
  calibration/file details render as an explicit unresolved state instead of
  zero/placeholder values — `RenderValue`/`PropertyTable` shared renderer,
  `master-applicability.ts`, migration 0065 dropping the hardcoded
  `0 AS size_bytes` view column).

### S4 — Open master detail {#S4}
- **Do:** Open a master's detail panel. Resize the window across the
  wide-window threshold.
- **Expect:** The master detail uses the same adaptive dock as other list
  pages (see J04/S4): a full-height, drag-resizable side panel on a wide
  window (width and a per-page pin both persist across restarts), a bottom
  dock when narrow. The panel leads with information not already on the list row
  (full metadata, provenance, related entities, history, actions) and trims
  echoed list columns to a small identifying summary. A "Used by" list of
  the sessions the master is assigned to opens and navigates, and a
  "Compatible" list names the sessions this master could calibrate. Age/created
  date is visible as a value, not only as an aging warning. A metadata-less
  field renders an explicit unresolved chip, never a plausible-looking zero.
- **Expect (negative):** The panel is never a raw dump of every available
  field with no more information than its row.
- **Trace:** issue-619, spec-030 FR-135-FR-140; PR #849. Corrected:
  "Used by" links sessions only, not projects — the panel's only other
  linked-entity list is "Compatible" sessions, which is now real: `masters_get`
  computes it (`crates/app/calibration/src/matching/masters.rs:129`,
  `:165`) and the panel renders those session labels
  (`apps/desktop/src/features/calibration/useMasterDetail.ts:191-193` →
  `MasterDetail.tsx:231`). `MasterDetail.tsx`'s own file-header comment still
  says the field is an empty stub — that comment is stale, tracked as
  astro-plan-i0z00. spec-054/FR-001, FR-004 (adaptive side/bottom dock,
  resizable+persistent width, per-page pin).

### S5 — Use master actions {#S5}
- **Do:** Trigger "Use in project", "Replace master", and the platform-native
  reveal-in-file-manager action from master detail.
- **Expect:** Each performs its documented action with an answer-back, or is
  absent entirely — a rendered button with no behavior is a failing state.
  The reveal action opens the master's own folder using the OS-native label
  (e.g. "Show in File Explorer" on Windows).

### S6 — Review ranked candidate sessions {#S6}
- **Do:** From a project, or the Calibration page's matching view, select an
  unassigned master.
- **Expect:** Ranked candidate sessions to calibrate appear before any
  assignment, each showing real context (target, filter, night, frame
  count) with a confidence value and mismatch indicators. A session whose
  fingerprint fails a hard rule (e.g. wrong gain) shows with a mismatch
  indicator rather than being silently hidden. Absent context never
  fabricates a value (no "1x1" binning placeholder, no empty-string camera)
  — absence renders as an explicit unresolved state.
- **Expect:** Camera body identity is compared as its own dimension. Two
  frames whose headers carry different body serials never appear as candidates
  for each other — that is a hard exclusion, and forcing the assignment past
  it (S7) lists it among the violations. When neither side records a body the
  dimension is skipped and the score is unchanged. When exactly one side
  records one, the candidate stays, at 0.1 below the confidence it would
  otherwise have, and carries a "metadata missing" entry against the camera
  dimension so the unproven agreement is visible rather than assumed.
- **Expect:** The suggestion status summarising a multi-kind request is
  reduced per kind, so a dark's confidence is never compared against a bias's.
- **Expect (negative):** Matching results are unaffected by missing-value
  display handling — ranking is computed on option-typed session/master
  info, never on the display DTO.
- **Expect (negative):** A body serial is claimed only where the header
  supports one: a `CAMERAID` equal to the `INSTRUME` model, or with an empty
  trailing segment, or absent, yields no identity, and the dimension is then
  skipped rather than compared against a fabricated value.
- **Trace:** issue-620, spec-030 FR-135-FR-140; PR #849
  (`crates/app/calibration/src/matching/` de-zeroing);
  `crates/calibration/core/src/rules/mod.rs:49`
  (`UNKNOWN_CAMERA_BODY_PENALTY = 0.1`), `:51-61` (`camera_bodies_conflict` —
  only two present-and-different ids conflict), `:63-99`
  (`optional_camera_rule`), used by `rules/dark.rs:56`, `rules/bias.rs:59`,
  `rules/flat.rs:67`; `crates/calibration/core/src/assign.rs:113-123` (the
  same predicate feeds an override's violation list);
  `crates/calibration/core/src/candidate.rs:72`, `:111-114`
  (`MismatchReason::MetadataMissing`) rendered at
  `apps/desktop/src/features/calibration/MatchCandidatesPanel.tsx:73-84`,
  `:153-171`; PR #1742. Per-kind status reduction:
  `crates/app/calibration/src/matching/suggest.rs`
  (`multi_kind_suggest_status`), PR #1730.

### S7 — Assign a master to a session {#S7}
- **Do:** Assign a candidate master to a session; separately, cancel an
  in-progress assignment.
- **Expect:** Confirming records the assignment, updates the "used by" list,
  and answers back. The same master's usage is visible from the
  session/project side (round-trip navigation).
- **Expect (negative):** Cancelling fires no backend call; no assignment is
  ever applied without an explicit confirm — matching never auto-applies a
  calibration assignment.

### S8 — Change a calibration matching tolerance {#S8}
- **Do:** In Settings → Calibration Matching, toggle a hard "match required"
  requirement (binning, gain, or offset) or change a soft tolerance (sensor
  temperature, dark/bias age).
- **Expect:** The change is durably persisted and still holds after an app
  restart, and the next suggestion respects it: the gain and binning
  "match required" toggles gate candidates, and the dark/bias age limit scores
  the gap between the light session's observing night and the master's, bounded
  by the persisted limit. An unknown observing night on either side skips the
  age dimension rather than penalising the master; a zero or negative stored
  limit falls back to the engine default.
- **Expect (negative):** There is no camera toggle on this page: camera body
  identity (S6) is not configurable — a proven body conflict always excludes,
  and the one-sided penalty is a fixed constant, deliberately not a settings
  key.
- **Trace:** spec-030 FR-130-FR-134 (durable audit intent); issue-647;
  `apps/desktop/src/features/settings/CalibrationMatching.tsx:11-13`,
  `:49-55` (the three shipped toggles);
  `crates/calibration/core/src/rules/mod.rs:44-49` (the penalty is a constant
  because `MatchingRuleConfig` is filled from settings keys with no
  construction-time validation). Corrected: the migrated text named a camera
  "match required" toggle, which this page has never shipped.

## Success criteria
- SC1: Ingesting a folder of correctly-named/tagged, or STACKCNT-confirmed,
  master files (S1) yields one Calibration-page row per master (S3), each
  showing real values or an explicit unresolved state — never a fabricated
  zero.
- SC2: An unassigned master's candidate list (S6) is visible before any
  assignment and every hard-rule mismatch is shown, never hidden.
- SC3: No calibration assignment ever applies without an explicit confirm
  (S7).
- SC4: Zero files backed by unreadable headers appear as masters: a corrupt
  file named like a master ends in the unclassified population, not on the
  Calibration page (S1).
- SC5: For every candidate pair whose two camera body ids are known and
  different, the candidate count is zero; for a pair where exactly one is
  known, the candidate is present with a camera "metadata missing" entry and
  a confidence 0.1 below the otherwise-identical both-absent case (S6).
- SC6: With one session's stored frame list corrupted, the Calibration page
  and the session list still list every intact row (S3).

## Known gaps
- G1: Masters never get per-file metadata rows, so a master item's
  pre-confirm Inbox detail has nothing to evaluate the required-attribute gate
  against; the detail says so rather than implying certainty (S1). The
  data-model fix is still open.
- G2: A corrupt session blocks raw sub-frame cleanup library-wide with no
  in-app repair path (J06/G10, astro-plan-dq9r3). It does not block this
  journey's lists (S3), but a library in that state cannot complete J06.
- G3: Out of scope: `dark_flat` and `bad_pixel_map` kinds are never matched
  in v1 by design, so no step covers them.
- G4: The camera-body dimension (S6) is validatable only with fixtures whose
  headers carry a `CAMERAID` with a real serial segment — a vendor-dependent
  shape (Player One writes one, ZWO writes the bare model or an empty trailing
  segment, Dwarf omits the keyword), so the three cases need three purpose-made
  fixtures rather than any library at hand.

## Delta log

- **Δ2** 2026-07-17 · S1 · behavior-change
  In the pre-confirm Inbox list, a single-file materialized master item no
  longer mislabels as "Mixed" (now reads by its own frame type); the Type
  pill is quieter, and the former "Detection" column is renamed "Path" and
  shows each source root's own basename instead of an indistinguishable
  "(root)" placeholder.
  Evidence: PR #910 (fixes #550, #555, #556) · by: journey-scribe
  (intent-gated)

- **Δ3** 2026-07-17 · S1 · behavior-change
  A master item's pre-confirm Inbox detail view no longer implies "all
  clear" for the required-attribute gate when it has no per-file metadata
  to evaluate — masters bypass per-file metadata persistence entirely, so
  the empty state now appends an explicit caveat that the gate is checked
  at confirm time instead. The underlying per-file-metadata gap for masters
  remains open.
  Evidence: PR #939 (fixes #551) · by: journey-scribe (intent-gated)

- **Δ4** 2026-07-17 · S4 · behavior-change
  Master detail now uses the shared adaptive dock: full-height resizable
  side panel on a wide window (width + per-page pin persist), bottom dock
  when narrow — same mechanism as Sessions/Projects/Archive/Targets.
  Evidence: spec-054-adaptive-detail-dock (FR-001, FR-004) · by:
  journey-scribe (intent-gated)

- **Δ5** 2026-07-20 · S3 · behavior-change
  A search and/or kind filter that matches nothing on the Calibration page
  now names the active filter and offers a "Clear filters" action, and only
  when showable masters actually exist — previously a search miss always
  rendered the "No calibration masters — run a scan" onboarding copy even
  when masters existed, indistinguishable from a truly empty library.
  Evidence: PR #1291 (closes #669, #812) · by: journey-scribe (intent-gated)

- **Δ6** 2026-07-20 · S3 · behavior-change
  Masters now display the camera's registered friendly name (Settings →
  Equipment) instead of the raw instrument header string, case-insensitive
  across aliases; an unregistered camera still shows the raw string.
  Display-only — match scoring is unaffected.
  Evidence: PR #1341 · by: journey-scribe (intent-gated)

- **Δ7** 2026-08-24 · S4 · behavior-change
  Master detail's "Compatible" list is real: the backend computes the
  compatible sessions and the panel names them, where the field was previously
  an empty stub with nothing to show.
  Evidence: PR #1032 (22d9f67a9), issue #868 · by: journey-scribe
  (intent-gated)

- **Δ8** 2026-08-24 · S1, +SC4 · behavior-change
  A FITS file whose header yields no recognisable keyword is now an extraction
  error rather than empty metadata, so it no longer reaches filename-based
  inference: a corrupt file named `masterDark.fits` lands in the
  unclassified/needs-review population instead of being registered as a
  calibration master backed by no header evidence.
  Evidence: PR #1737 (c7f23e22b), following the `fits-header` 0.4.3 bump in
  PR #1733 · by: journey-scribe (intent-gated)

- **Δ9** 2026-08-24 · S1, S6 · behavior-change
  Master inference now claims only what its evidence supports: the path check
  matches a delimiter-bounded `master` token instead of the substring, so
  frames under `Grandmaster Nebula/` or `masterclass_notes/` are no longer
  masters; frame types are parsed from whole tokens, so `dark_flat` is no
  longer read as `dark`; the PixInsight detector reports and honours a
  stack-count header like the Siril one; and a multi-kind suggestion status is
  reduced per kind instead of comparing a dark's confidence against a bias's.
  Evidence: PR #1730 (855c63e3a),
  `specs/tiny/inference-predicates-and-aggregation.md` · by: journey-scribe
  (intent-gated)

- **Δ10** 2026-08-24 · S3, +SC6 · behavior-change
  One session whose stored frame list is not valid JSON no longer empties the
  calibration master list or the session list: every JSON read over that
  column is guarded, so intact rows still return.
  Evidence: PR #1736 (da71a9c44) · by: journey-scribe (intent-gated)

- **Δ11** 2026-08-24 · S6, S7, S8, +SC5 · behavior-change
  Camera body identity is now a matching dimension: two known, different body
  serials exclude the candidate outright, one-sided knowledge keeps it at 0.1
  lower confidence with a camera "metadata missing" entry, and both absent
  costs nothing. Previously only the camera model reached the matcher, so two
  identical bodies were indistinguishable and a master from one body silently
  calibrated lights from the other.
  Evidence: PR #1742 (21d5b6eb9) · by: journey-scribe (intent-gated)

- **Δ12** 2026-08-24 · S8 · behavior-change
  The dark/bias age limit set in Settings → Calibration Matching now scores
  the gap between the light session's observing night and the master's, and
  the gain and binning "match required" toggles now gate candidates. All three
  were persisted-but-dead config that no matching rule read.
  Evidence: PR #1692 (03d61d21b),
  `crates/calibration/core/src/rules/mod.rs:167` (`apply_age_rule`) · by:
  journey-scribe (intent-gated)
