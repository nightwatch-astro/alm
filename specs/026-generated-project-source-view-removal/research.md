# Research: Generated Project Source View Removal

**Spec**: `specs/026-generated-project-source-view-removal/spec.md`  
**Status**: IMPLEMENTED (post-hoc record, corrected 2026-08-24). This document was
left on the pre-implementation value when `spec.md`, `plan.md`, `data-model.md`
and `tasks.md` were all flipped to Implemented.

## R1: When Source Views Become Stale

A `PreparedSourceView` is considered stale when any of the following holds:

- A referenced inventory item's canonical path no longer resolves under the
  current root mapping.
- The recorded link target on disk no longer exists, has changed kind (e.g.
  symlink replaced by a regular file), or points outside the original
  inventory item.
- **Copy-kind only (A3, GRILL 2026-05-22)**: The content hash of the copied
  file no longer matches the hash recorded at creation (`hash_diverged`). This
  signals that the copy has drifted from its canonical source. Link-kind views
  (symlink, junction) skip content hash because they carry no unique bytes.
- The project's workflow profile has changed such that the view's strategy
  (symlink vs copy vs junction) is no longer the project's preferred strategy.
- A canonical inventory item the view references has been archived or marked
  deleted.

Detection runs as a read-only sweep on project open and on demand. Stale views
are surfaced in the UI; no mutation happens without a reviewed plan.

## R2: Cross-Platform Link and Junction Cleanup

**v1 strategies: symlink, junction, copy only. `hardlink` is deferred to v1.x
(R-026-Strategies, GRILL 2026-05-22).**

| Strategy   | Windows                              | macOS / Linux         |
| ---------- | ------------------------------------ | --------------------- |
| symlink    | `RemoveDirectoryW` / `DeleteFileW`   | `unlink(2)`           |
| junction   | reparse point removal via `DeviceIoControl` then `RemoveDirectoryW` | not applicable |
| copy       | archive (hard-coded default, R-026-Dest-Archive) | archive then trash    |
| hardlink   | *(deferred to v1.x)*                 | *(deferred to v1.x)*  |

Removal MUST never recurse into the link target. Junction handling on Windows
must explicitly detect reparse points so the OS does not follow the junction
and delete inventory content. Long-path prefix (`\\?\`) is required on
Windows. Symlink loops are guarded by visiting only the recorded view
membership, never by walking the filesystem.

## R3: Archive vs Delete for View Files

**Updated (A1 + R-026-Dest-Archive, GRILL 2026-05-22)**: The destructive
destination for ALL view removal is hard-coded to `archive`. There is no
user-selectable `destructiveDestination` on the remove request. Rationale:
preserving reversibility is a constitutional requirement; the user can always
permanently delete from the archive surface after review.

- Link-kind views (symlink, junction) carry no unique bytes; an archive
  step is still taken to preserve the audit trail, but the archive action is
  effectively a no-op for bytes (the link entry is removed and nothing is
  moved to the archive folder).
- Copy-kind views carry duplicated bytes; the archive step moves the copy to
  `<library_root>/.astro-plan-archive/<planId>/` before deletion, consistent
  with spec 017 R-Archive-1.
- Mixed-kind views are refused at create time (A2); no mixed-action plan is
  produced in v1.
- `hardlink` removal semantics are deferred to v1.x (R-026-Strategies).

## R4: Regeneration Cost Analysis

- Link/junction/hardlink regeneration: O(items) syscalls, no byte movement.
  Practically instant for typical project sizes (hundreds to low thousands of
  items).
- Copy regeneration: dominated by disk I/O proportional to the bytes copied.
  This cost is the primary reason copy-kind views default to archive on
  removal rather than direct delete.
- Regeneration always reads the canonical database first and resolves current
  inventory paths, so it correctly handles root remaps that occurred between
  removal and regeneration.

## Decisions

- D1: Treat removal and regeneration as `FilesystemPlan` variants, not as
  bespoke operations. Both plans flow through the full spec 017/025 pipeline:
  `plan.approve` (with approvalToken) → `plan.apply` (with per-item FS
  revalidation, paused state, `plan.resume`). See R-026-Pipeline.
- D2: Archive is the hard-coded destructive destination for all view removal;
  there is no user-selectable alternative in v1 (R-026-Dest-Archive, GRILL
  2026-05-22).
- D3: Stale detection is read-only and never auto-mutates. Spec 017 cleanup
  plans MAY include stale views as passive candidates; the user explicitly
  approves any action (R-026-StaleAutoInclude, GRILL 2026-05-22).
- D4: Preserve `PreparedSourceView` membership history after removal so
  regeneration is reproducible. Removed views have an indefinite regenerable
  lifetime (A4, GRILL 2026-05-22).
- D5 *(R-026-Lifecycle, GRILL 2026-05-22)*: View removal and regeneration are
  allowed only when the owning project is in `setup_incomplete | ready |
  prepared | processing | blocked | completed`. Requests on `archived` projects
  are refused with `lifecycle.read_only`. The unarchive path flows through spec
  009 R-Unarchive (`archived → ready`).
- D6 *(R-026-Pipeline, GRILL 2026-05-22)*: All spec 017/025 error codes can
  surface during plan apply (e.g. `item.stale`, `disk.full`,
  `volume.unavailable`, `path.invalid`). The response for both remove and
  regenerate includes a `plan_id` that tracks the plan through the standard
  pipeline.
