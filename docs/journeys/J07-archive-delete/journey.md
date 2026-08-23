---
id: J07
title: Archive a completed project, then trash or permanently delete it
version: 5
status: draft
last_reviewed: 2026-07-14
actors: [astrophotographer]
surfaces: [archive, projects, plans, audit]
interfaces: [desktop-ui]
trace:
  - pre-migration docs/product/journeys/J07-archive-delete/journey.md @ 66026463
  - deltas/2026-07-14-jval-docdrift.md (folded — verified in apps/desktop/src/features/projects/ProjectDetail.tsx)
  - deltas/2026-07-14-q15-t123.md (folded — verified in crates/app/core/src/protection.rs)
  - deltas/2026-07-14-q16-t132.md (folded — verified in apps/desktop/src/features/archive/ArchiveTable.tsx, ArchiveDetail.tsx)
  - specs/016-source-protection-defaults/spec.md (FR-004, SC-003)
  - specs/030-ui-audit-revision/spec.md (FR-090, FR-130–FR-134, FR-135–FR-140)
  - e2e-agentic-test/017-cleanup-archive-review-plans/archive-lifecycle/scenario.md (D7/D14/D15/D24)
  - docs/development/journey-run-2026-07-14.md (Journey 7 section — live-app validation, build 7e522c16)
  - PR #401, PR #415, PR #826, PR #849
  - PR #883 (issue #732 — send-to-trash and permanently-delete perform real
    filesystem work; the earlier audit-only stubs are gone)
  - spec-054-adaptive-detail-dock (FR-004 — shared adaptive dock)
  - PR #1190 (design-refresh handoff 06 — Approve & apply variant scoped to
    delete-only plans)
---

## Goal

An astrophotographer who considers a project's imaging work finished moves
that project's files out of the active library into a reviewable, audited
archive location, and can later remove the archived files — first to the OS
trash, or, with an explicit typed confirmation, permanently. "Done" means:
the project's files are relocated only through an approved, reviewable plan;
the project's lifecycle field reads `archived` only after that plan has
actually been applied; and no permanent deletion ever happens without the
user typing the literal word `DELETE`.

## Preconditions

- P1: A project exists in the `completed` lifecycle state.
- P2: The project has real files on disk under a registered library root
  (source) available to archive.
- P3: For S7's success path only: "Block permanent delete" is off in
  Cleanup/Protection settings. It defaults to on, so a default install
  refuses permanent deletion outright.

## Steps

### S1 — Attempt to archive a completed project {#S1}
- **Do:** From the completed project's detail view, choose the action to
  archive it.
- **Expect:** The lifecycle transition is refused server-side because no
  archive plan yet exists for this project; the client responds to that
  refusal by generating the archive plan and opening the plan review in the
  same interaction — no separate manual step and no backend-only command is
  needed.
- **Expect (negative):** The project's lifecycle does not change on this
  click alone; a bare refusal never silently flips state.
- **Trace:** apps/desktop/src/features/projects/ProjectDetail.tsx (`handleGenerateArchivePlan`).

### S2 — Review the generated archive plan {#S2}
- **Do:** Review the plan's item list before approving.
- **Expect:** Each item shows both its source path and its destination path
  (the app-managed archive folder for this plan). Items from a protected
  source are called out separately from normal/unprotected items and are
  flagged as requiring acknowledgement, with a stated reason.
- **Expect:** Acknowledging a protected item writes a durable audit record
  (checkable via the Audit Log) for that acknowledgement.
- **Expect (negative):** Approving/applying the plan stays unavailable until
  every protected item has been individually acknowledged.
- **Trace:** crates/app/core/src/protection.rs (`plan_protection_check`,
  `acknowledge_protected_item`); specs/016-source-protection-defaults/spec.md
  FR-004.

### S3 — Apply the archive plan {#S3}
- **Do:** Approve and apply the reviewed plan.
- **Expect:** "Approve & apply" (`plan-review-approve-apply`) renders in the
  app's neutral primary style, not the red destructive style: the shared
  plan-review overlay (also used by J06) derives that style from
  `hasDestructiveItems`, true only when some plan item's action is `delete`,
  and an archive plan's items all carry action `archive`.
- **Expect:** Files move into an app-managed, collision-free archive folder
  scoped to this plan (`.astro-plan-archive/<planId>/…`, a documented
  deviation from the originally specced token-pattern destination, D24).
  Only once apply succeeds does the project's lifecycle flip to `archived`,
  and the project's Edit pane becomes read-only with a stated reason.
- **Expect (negative):** If apply has not run, or fails, the lifecycle stays
  unchanged and the Edit pane stays editable.
- **Expect (negative):** Apply never overwrites an existing file at the
  destination.

### S4 — Find the archived project on the Archive page {#S4}
- **Do:** Open Archive; search by name, reason, or original path; sort by
  name, type, reason, size, or archived date.
- **Expect:** The archived project appears as a row with its type, reason,
  size, and archived timestamp, reflecting only real archived projects.
- **Expect (negative):** No placeholder/fixture rows ever appear on this
  page.
- **Expect (negative):** A missing reason or size is never rendered as a
  fabricated value (e.g. a bare `0`) — it renders through the shared
  `renderValue()` as a distinct unresolved state; absence is only ever used
  as the lowest sort key, never as the displayed value.
- **Trace:** apps/desktop/src/features/archive/ArchiveTable.tsx (`renderValue`,
  `compareEntries`); PR #849; specs/030-ui-audit-revision/spec.md
  FR-135–FR-138.

### S5 — View archived project detail and its audit history {#S5}
- **Do:** Select the archived row. Resize the window across the
  wide-window threshold.
- **Expect:** The archive detail uses the same shared adaptive dock as
  Sessions/Calibration/Projects/Targets: a full-height, drag-resizable side
  panel on a wide window (width + per-page pin persist), a bottom dock when
  narrow. The detail pane header shows the project name (title), its
  entity type (pill), and its original path (subtitle, or a stated
  fallback when there is no path), plus a dated, human-readable
  audit-history table (timestamp + detail text) for this project (durable
  `audit_log_entry` history, not the live event bus). Archived-at, reason,
  and size are intentionally not repeated in the detail pane — they live
  only on the Archive row (S4); a former duplicate "Details" table
  repeating those fields was removed.
- **Expect (negative):** The audit-history table is not simply a repeat of
  the row's own list columns.
- **Trace:** apps/desktop/src/features/archive/ArchiveDetail.tsx; PR #849
  (dropped the duplicate Details table per decision T133, "detail-as-delta
  audit"); specs/030-ui-audit-revision/spec.md FR-139–FR-140. Corrects the
  prior migrated claim that archived-at/reason/size/entity-type/path all
  appear in the detail pane — that table was removed by #849 (merged
  2026-07-14T20:01Z). spec-054/FR-004 (shared adaptive dock).

### S5a — Restore an archived project through a reviewable plan {#S5a}
- **Do:** With an archived project selected, choose Restore, then review and
  apply the generated plan.
- **Expect:** Restore generates a plan rather than moving anything itself: a
  `restore`-origin plan of `move` items lands in `ready_for_review`, each
  item's source the archive path and its destination the original path, and
  it opens in the same shared plan-review overlay used at S2.
- **Expect:** Applying that plan returns the files to their original paths
  and closes the lifecycle with trigger `archive.plan.restore.applied`.
- **Expect:** The control is offered only for a row that was archived
  through a plan; without an `archivedViaPlanId` it is disabled.
- **Expect (negative):** Generating the restore plan moves, renames, or
  deletes no file on disk.
- **Expect (negative):** Restore never overwrites a file already present at
  the original path — such an item fails with `conflict.destination_exists`.
- **Trace:** `archive-restore-btn`; contract operation
  `archive.plan.generate_restore`; `archive_generator::generate_restore`;
  message keys `archive_restore_project_btn`,
  `archive_restore_plan_created_toast`, `archive_restore_generate_failed`.

### S6 — Send archived files to the OS trash {#S6}
- **Do:** With the archived project selected, choose "Send to trash".
- **Expect:** Each file under the archive path is handed to the OS
  trash/Recycle Bin, so it leaves the archive location on disk. Recovery is
  through the OS bin only — the app records no per-item trash location.
- **Expect:** A durable audit event `archive.sent_to_trash` records
  `items_moved`, counted from operations that actually succeeded rather than
  from the archive's item count.
- **Expect:** A file already absent from disk counts as a no-op rather than
  a failure; the command raises only when zero items moved (`archive.empty`
  when the archive holds nothing).
- **Expect:** A failed trash call surfaces its own reason —
  `os_trash.permission.denied`, `os_trash.unavailable`, or `os_trash.full`.
  This path supplies no archive fallback, so a trash failure is a hard
  failure and the affected files stay in the archive location.
- **Expect:** When some files trash and others fail, the command still
  reports success; the shortfall shows only as `items_moved` being lower
  than the archive's item count (G5).
- **Expect (negative):** `items_moved` is never reported as the archive's
  item count when fewer files actually moved — the recorded number is the
  real outcome.
- **Trace:** `plans::archive::send_archive_to_trash`;
  `fs_executor::ops::trash_op::trash_file`; audit topic
  `archive.sent_to_trash`; contract operation `archive.send_to_trash`;
  PR #883.

### S7 — Permanently delete archived files {#S7}
- **Do:** Choose "Delete permanently"; a confirmation dialog requires typing
  the literal word `DELETE`.
- **Expect:** The confirm control stays disabled until the typed text is an
  exact, case-sensitive match for `DELETE` (UI constant
  `DELETE_CONFIRM_TEXT`); the backend independently rejects a mismatched
  `confirm_text` against `PERMANENT_DELETE_CONFIRM_TEXT` with
  `confirm.text.mismatch`.
- **Expect:** Confirming removes each archived file from disk. The removal
  is permanent: it does not pass through the OS trash, the executor records
  no rollback for a delete, and the app offers no restore path for a deleted
  archive.
- **Expect:** A durable audit event `archive.permanently_deleted` records
  `items_deleted`, counted from removals that actually happened. A file
  already gone counts as a no-op; the command raises only when nothing was
  deleted.
- **Expect:** A failed removal surfaces its own reason —
  `path.permission_denied` or `archive.delete_failed`.
- **Expect (negative):** A half-typed or wrong-case entry leaves the confirm
  control disabled; Cancel leaves every file on disk.
- **Expect (negative):** While "Block permanent delete" is on, the deletion
  is refused server-side (`plan.blocked_by_protection`) and no file is
  removed. That setting defaults to on (P3).
- **Trace:** `plans::archive::permanently_delete_archive`;
  `fs_executor::ops::delete_op::delete_file`; audit topic
  `archive.permanently_deleted`; contract operation
  `archive.permanently_delete`; `ArchivePage` (`DELETE_CONFIRM_TEXT`, delete
  modal); PR #883.

### S8 — Reveal archived files {#S8}
- **Do:** Choose the platform-native reveal control ("Show in File
  Explorer" on Windows) for a selected archived entry.
- **Expect:** The control is present and its label follows the OS-native
  convention.
- **Expect (negative):** Today this control is always disabled — it does
  not silently no-op; a tooltip states it, and no files are opened (see
  Known gaps G3).

## Success criteria

- SC1: Choosing Archive on a completed project with no existing plan always
  ends with a plan generated and its review open in the same interaction
  (S1) — no case reaches a dead-end refusal.
- SC2: A project's lifecycle field reads `archived` if and only if an
  `origin=archive` plan for that project has been applied (S3).
- SC3: The permanent-delete confirm control is enabled if and only if the
  typed input is exactly `DELETE` (S7).
- SC4: Every permanent-delete attempt while "Block permanent delete" is
  enabled is refused, with zero files removed (S7).
- SC5: Every protected-item acknowledgement during archive-plan review
  (S2) resolves to a durable `audit_log_entry` row.
- SC6: Every completed "Send to trash" leaves zero of that archive's files
  at the archive path, and `items_moved` equals the number of files that
  actually left it (S6).
- SC7: Every confirmed permanent delete removes the archived files from disk
  with no OS-trash copy and no app restore path (S7).
- SC8: Applying a generated restore plan returns every restored item to the
  path it was archived from, or fails that item with a stated reason (S5a).

## Known gaps

- G1: (dissolved 2026-07-15) — tracked as issue #885; Restore is a reviewable restore-plan generator, archive confirmed a real file move.
- G2: (dissolved 2026-07-15) — tracked as issue #886; masters archivable tracked as #886; targets stay non-archivable (DB-only); session files archivable via session-scoped cleanup flow (J06 S5-S6).
- G3: (dissolved 2026-07-15) — tracked as issue #874; reveal is a permanently disabled stub.
- G4: After S6 or S7 the archive row keeps its `archivedViaPlanId`, so
  Restore (S5a) stays enabled and still generates a plan; every item of that
  plan then fails at apply with `source.missing`. The enabled control is not
  an available recovery path. Tracked as astro-plan-vuubi.
- G5: A partially successful S6 or S7 reports success, with the shortfall
  visible only in the recorded count. Tracked as astro-plan-mlcyq.
- G6: The OS-trash refusal paths (`os_trash.permission.denied`,
  `os_trash.unavailable`, `os_trash.full`) and the delete refusal
  `path.permission_denied` cannot be induced from the desktop UI; validating
  them needs OS-level setup outside this journey.

## Delta log

- **Δ2** 2026-07-14 · S4, S5 · behavior-change
  Archive adopts the shared value renderer: missing size/reason never
  render as a fabricated value (S4). The detail pane drops its duplicate
  "Details" table (archived-at/reason/size/type/path all already shown on
  the row or in the header) in favor of a minimal header plus the audit
  history, per the detail-panel-adds-new-information rule (S5).
  Evidence: PR #849 (merged 2026-07-14T20:01Z),
  specs/030-ui-audit-revision/spec.md FR-135–FR-140 (Wave-0 Q16, decision
  T133) · by: journey-scribe (intent-gated)

- **Δ3** 2026-07-17 · S5 · behavior-change
  Archive detail now uses the shared adaptive dock (side when wide,
  bottom when narrow, resizable, pin persists) — same mechanism as
  Sessions/Calibration/Projects/Targets.
  Evidence: spec-054-adaptive-detail-dock (FR-004) · by: journey-scribe
  (intent-gated)

- **Δ4** 2026-07-20 · S3 · behavior-change
  "Approve & apply" on the archive plan now renders in the neutral primary
  button style instead of the red destructive style, since an archive plan
  never deletes — previously the button was unconditionally styled
  destructive regardless of plan content, on every plan reviewed through
  the shared `PlanReviewOverlay` (also used by J06).
  Evidence: PR #1190 · by: journey-scribe (intent-gated)

- **Δ5** 2026-07-15 · S6, S7 · behavior-change
  Send-to-trash and permanently-delete now perform real filesystem work:
  trash hands each archived file to the OS trash/Recycle Bin, and permanent
  delete removes it from disk with no trash copy and no rollback. Both
  previously recorded only an audit event and left every file in place.
  Evidence: PR #883 (issue #732) · by: journey-scribe (intent-gated)
