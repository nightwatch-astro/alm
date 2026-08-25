---
id: J13
title: Reconstruct what happened to my library
version: 3
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [audit, activity, archive, settings]
interfaces: [desktop-ui, desktop-ui-macos]
trace:
  - pre-migration journey.md @ git 66026463
  - deltas/2026-07-14-q15-t122.md (settings audit)
  - deltas/2026-07-14-q15-t123.md (protection audit)
  - deltas/2026-07-14-q15-t124.md (equipment audit)
  - deltas/2026-07-14-q15-t125.md (source audit)
  - deltas/2026-07-14-q15-t126.md (activity-reads-durable-audit, blocked-on-Q9; basis for G2)
  - deltas/2026-07-14-q15-t127.md (negative-space enforcement)
  - PR #826, PR #805, spec-030 FR-130–FR-134/SC-009
  - docs/development/journey-run-2026-07-14.md (Journey 13 live validation run, origin/main @ 7e522c16)
  - GitHub #626, #667, #766, #647, #629, #832 (open defects, corrected against during 2026-07-15 doc audit)
---

## Goal

The astrophotographer needs to reconstruct what PlateVault actually did (or
refused to do) after an unattended scan, a plan apply, a settings change, or a
forgotten background operation. Done means: they can find the specific event
by entity and/or date, see its outcome (applied / refused / failed) and the
actor that performed it, and — for anything refused or failed — see why,
without any ambiguity about whether "no matching row" means "nothing
happened" or "nothing was recorded".

## Preconditions

- P1: The app is running against an existing library with at least one
  registered data source.
- P2: At least one mutating action has already occurred that is eligible for
  the durable Audit Log — e.g. a filesystem plan apply, a settings change, a
  protection override/acknowledgement, an equipment add/edit, or a source
  register/enable/disable/remap.
- P3: At least one attempted mutation was refused or failed (e.g. a
  protection override blocked by dependents, an equipment add with a
  duplicate alias, or a source delete blocked by dependents) — establishable
  by attempting one of these from the relevant page.
- P4: The Activity panel has accumulated one or more events from the current
  session (any in-app action produces at least an info-level row).

## Steps

### S1 — Open the Activity panel {#S1}
- **Do:** Toggle the status-bar Log control.
- **Expect:** The panel opens as a full-width bottom fold-out, expanding to
  40% of the frame height (header always visible, only the event list
  scrolls) — not a small, largely unstyled box capped at 200px. It shows a
  live, newest-first stream of
  in-session events, each with a severity/level and a source indicator; rows
  that reference a specific entity are visually marked as navigable.
- **Expect (negative):** Opening or closing the panel writes no Audit Log
  entry — it is a read.

### S2 — Filter the live stream by severity {#S2}
- **Do:** Select a severity chip (error / warn / info / debug).
- **Expect:** Only rows at that exact level are shown; selecting "all"
  restores the full set. A diagnostics toggle is available only when the
  app's log level setting is "debug", and only then can diagnostic-source
  rows be shown at all.
- **Expect (negative):** A severity chip that matches zero current rows
  renders as an empty filtered list scoped to this live, in-session stream —
  it must never be read as evidence that the durable Audit Log is empty.

### S3 — Cross-link a stream row to its entity {#S3}
- **Do:** Click a row referencing a project, session, or target.
- **Expect:** Navigation lands directly on that entity's own page
  (`/projects`, `/sessions`, `/targets`, each pre-selecting the entity).
- **Expect (negative):** A row referencing a plan, or any entity type with no
  dedicated page (settings, protection, equipment, a data source), does NOT
  currently reach a working destination: `LogPanel.tsx`'s `buildEntityPath`
  targets `/plans/${entityId}` for plans and a bare `/audit?...` for the
  fallback case, but neither `/plans`, `/plans/$id`, nor `/audit` is a
  registered route in `router.tsx` — both fall through to the router's
  not-found handler and silently land on Sessions instead of the plan's page
  or the durable Audit Log. This is an open defect (GitHub #626), not the
  designed fallback the journey originally assumed.
- **Trace:** GitHub #626 (open); `apps/desktop/src/app/router.tsx` (no
  `/plans`, `/plans/$id`, or `/audit` route registered);
  `apps/desktop/src/app/LogPanel.tsx:73-88` (`buildEntityPath`).

### S4 — Manage panel chrome {#S4}
- **Do:** Toggle follow mode; scroll up within the stream; close the panel
  once via its collapse control and once via Escape.
- **Expect:** With follow mode on, the newest row stays pinned at the top of
  the list as new events arrive; scrolling the list up pauses that
  auto-scroll without clearing the persisted follow preference; scrolling the
  list back to the top resumes it. Both the collapse control and Escape close
  the panel.
- **Expect (negative):** Toggling the follow control back on while the list
  is still scrolled away from the top does NOT itself jump to the newest row
  — the scroll-pause is driven purely by the list's own scroll position
  (`scrollPaused`, cleared only by scrolling back to the top), not by the
  follow toggle. This is an open defect (GitHub #832): re-enabling Follow
  visibly flips the chip active but the viewport stays on stale rows until
  the user also scrolls up manually.
- **Trace:** GitHub #832; `apps/desktop/src/app/LogPanel.tsx` (the follow-tail
  `useEffect` gates on `followLogs && !scrollPaused`, and `scrollPaused` is
  only ever cleared by `handleScroll`).

### S5 — Export the live stream {#S5}
- **Do:** Trigger the panel's Export action.
- **Expect:** A native save dialog opens (its window title currently reads
  "Export Audit Log" — the Settings → Audit Log surface's title, not the
  Activity panel's own "Activity" title; an open mislabel defect, GitHub
  #667, not the per-surface title the journey originally assumed); on
  confirming a destination, a file is written with no error shown.
- **Expect (negative):** Cancelling the save dialog leaves no file written
  and shows no error.
- **Trace:** GitHub #667; `apps/desktop/messages/en.json`
  (`logpanel_save_dialog_title` = "Export Audit Log");
  `apps/desktop/src/app/LogPanel.tsx:231`.

### S6 — Open the durable Audit Log {#S6}
- **Do:** Navigate to Settings → Audit Log.
- **Expect:** A table lists attempted mutating actions across the library,
  each row showing timestamp, event, entity, outcome, and actor. In practice
  outcome is always one of applied / refused / failed — the durable
  `audit_log_entry` table's schema only permits those three values.
- **Trace:** `crates/persistence/db/migrations/0002_lifecycle.sql:162`
  (`CHECK (outcome IN ('applied', 'refused', 'failed'))`). The `AuditOutcome`
  contract enum (`crates/contracts/core/src/audit.rs`) additionally declares
  `ok`/`paused` variants, but no writer can ever persist them under this
  constraint, so they never appear in a real row.

### S7 — Find a plan apply by entity and date {#S7}
- **Do:** After applying a filesystem plan, search or filter the Audit Log to
  find that plan by its entity and by the date it ran.
- **Expect:** Constitution Principle II and spec-030 FR-130–FR-134/SC-009
  require the plan-apply event to be present with outcome=applied and an
  actor — this is the core coverage assertion of the journey (SC1). This
  currently does NOT hold: `crates/app/core/src/plan_apply.rs` never writes
  to the durable `audit_log_entry` table on plan completion — only the
  in-memory `EventBus` (feeding the Activity panel) and `plan_apply_events`
  are updated. A live validation run found 10/10 successful plan applies
  (inbox-confirm and cleanup paths) produced zero durable audit rows.
- **Expect (negative):** A refused or failed plan apply is likewise absent
  from the durable Audit Log today, for the same reason.
- **Trace:** GitHub #766, #647 (both open, unresolved on current main;
  #647's comment thread confirms the gap is generic to the plan-apply path,
  not inbox-specific); `docs/development/journey-run-2026-07-14.md`, Journey
  13 HEADLINE. Candidate Known Gap for this journey — needs explicit user
  confirmation before being recorded as one.

### S8 — Find a settings, protection, equipment, source, or calibration change {#S8}
- **Do:** After changing a setting, overriding or acknowledging a protection
  rule, adding/editing equipment, registering/enabling/disabling/remapping
  a data source, or assigning/unassigning a calibration master to a session,
  filter the Audit Log by that entity type and date.
- **Expect:** The change is present as a durable row with outcome and actor.
  An equipment record created by auto-detection (not a direct user action)
  appears with actor=system at diagnostic severity, not attributed to the
  user. A calibration master assign/unassign is a durable row too — it is
  not visible only in the transient Activity panel stream (S1).
- **Expect (negative):** A refused or failed attempt in any of these five
  categories (P3's blocked override/duplicate-alias/blocked-delete case, or
  equivalent) still appears as a row, with outcome=refused or outcome=failed
  and a reason code — it is never silently dropped from the log.
- **Trace:** `crates/app/calibration/src/matching/assign.rs` (assign/unassign
  routed through `EventBus::write_audit`, PR #1287, refs #1120) — previously
  these mutations were recorded only via the non-authoritative in-memory
  `events` table, so they were absent from `audit_log_entry`.

### S9 — Confirm reads produce no audit noise {#S9}
- **Do:** Navigate through several pages/panes performing only reads (no
  edits, no confirmations).
- **Expect:** No new Audit Log rows appear as a result of this navigation.
- **Expect (negative):** The Audit Log's total count changes only with
  attempted mutations, never with browsing alone.

### S10 — Search and page through the Audit Log {#S10}
- **Do:** Use the free-text search box and the date-range pickers; page
  forward and back through the results.
- **Expect:** Search text and date range narrow the result set and its total
  count. A date range that excludes every event renders an explicit empty
  state, distinguishable from the loading state and from an error. Pagination
  controls move between pages and disable at the first and last page.

### S11 — Export the Audit Log {#S11}
- **Do:** Trigger the Audit Log's Export action with the current filters
  applied.
- **Expect:** A file download completes reflecting the filtered set.
- **Expect (negative):** An export failure surfaces an inline error message —
  it never fails silently.

### S12 — Review an archived project's history {#S12}
- **Do:** Navigate to Archive and open an archived project's entry
  (`ArchiveDetail`, on the Archive page — the Projects-page project detail's
  own History section does not query audit data at all, per GitHub #833, so
  it is not a substitute path for this step).
- **Expect:** An "Audit History" table appears, scoped to that project's
  entity id only, with a row per event showing timestamp and an event
  description. Outcome and actor are present in the underlying `AuditEntry`
  data (`archive/store.ts` calls `audit.list` filtered to
  `entityType: 'project', entityId`) but this panel currently renders only
  timestamp + event text — no outcome or actor column — an open defect
  (GitHub #629), not the fuller row shape the journey originally assumed.
- **Expect (negative):** No rows from any other project appear — the query
  is server-side filtered by this project's `entityId`.
- **Trace:** GitHub #629, #833; `apps/desktop/src/features/archive/store.ts`
  (`listArchiveAudit`); `apps/desktop/src/features/archive/ArchiveDetail.tsx`
  (renders only `ts`/`detail` columns).

## Success criteria

- SC1: Every filesystem plan apply performed during the session is findable
  in the Audit Log by entity and by date, with outcome=applied (S7) — target
  is 100% coverage, zero missed plan events. **Not currently met**: see S7's
  Trace (GitHub #766/#647, open) — plan-apply never reaches the durable
  Audit Log today.
- SC2: Every settings, protection, equipment, or source mutation attempt
  (applied, refused, or failed) is a durable Audit Log row carrying outcome
  and actor, and refused/failed rows carry a reason code (S8) — zero
  silently-dropped mutation attempts.
- SC3: A defined read-only page tour (S9) adds zero new Audit Log rows.
- SC4: Every Activity-panel row for project/session/target/plan cross-links
  directly to that entity's page; every other entity type lands on the Audit
  Log filtered to it (S3) — target is no dead-end rows. **Not currently met**
  for plan rows or the non-page-entity fallback: both route to a nonexistent
  path and silently land on Sessions instead (S3, GitHub #626, open).
- SC5: An Audit Log query (search or date range) matching zero events always
  renders an explicit empty state, never the loading or error state (S10).
- SC6: Both export actions (Activity panel S5, Audit Log S11) either produce
  a readable file or show an inline error — never a silent no-op.

## Known gaps

- G1: (dissolved 2026-07-15) — tracked as issue #666; no UI control sets the Activity panel source filter.
- G2: (dissolved 2026-07-15) — tracked as issue #647; activity-over-durable-audit, same audit-classes lane.

## Delta log

- **Δ2** 2026-07-20 · S1 · behavior-change
  The Activity panel now opens as the specified full-width bottom fold-out
  (40% of the frame height, header always visible, list scrolls) —
  previously most of its rows/chips/filters/buttons had no CSS at all and
  the body was hard-capped at 200px, reading as a small unstyled debug box.
  Evidence: PR #1303 (closes #734) · by: journey-scribe (intent-gated)

- **Δ3** 2026-07-20 · S8 · behavior-change
  Assigning or unassigning a calibration master to a session is now a
  durable Audit Log row (outcome + actor) — previously recorded only in the
  transient, non-authoritative in-memory events table, so it never appeared
  in the durable Audit Log at all.
  Evidence: PR #1287 (refs #1120) · by: journey-scribe (intent-gated)
