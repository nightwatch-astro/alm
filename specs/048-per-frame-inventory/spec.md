# Feature Specification: Per-Frame Inventory with Live Session Membership

**Feature Branch**: `048-per-frame-inventory`

**Created**: 2026-07-04

**Status**: Partial (verified 2026-08-24 against `origin/main` a52c637f2). The
inventory, cleanup and relink halves ship; the live-detection trigger does not.

Shipped:

- **US1** per-frame inventory with real sizes — frame records are written at plan
  apply by `crates/app/inbox/src/plan_listener.rs` (`stat_frame` → `size_bytes` at
  `:556`), and calibration-master `frame_ids` are populated there rather than left
  as the historical `'[]'` placeholder (`:340`, `:996`, test at `:1096`).
- **US2 (on-demand half)** — `crates/app/core/src/frame_inventory/reconcile.rs` and
  `relink.rs`, exposed as `inventory_reconcile_run`, `inventory_frame_list` and
  `inventory_frame_relink` (`bootstrap/specta.rs:74`, `:413-415`). Relink matches
  on sha256 computed on demand (`relink.rs:21`), per the 2026-07-04 clarification.
- **US3** raw sub-frame cleanup candidates —
  `crates/app/core/src/cleanup_generator/raw_frames.rs`, summing real
  `frame.size_bytes` into `total_reclaimable_bytes` at `:190`/`:204` and refusing
  the scan outright when a session's `frame_ids` is unreadable (`:47`, PR #1739).
- **US4 (config half)** — `crates/app/settings/src/root_config.rs` carries
  per-root `ReconcileMode` (`flag_missing` default, `auto_reconcile` opt-in) and
  the `detection.live` / `.scheduled` / `.on_open` / `.follow_symlinks` keys.
- **US5** missing-frame awareness for calibration matches —
  `crates/app/core/tests/calibration_missing_flag_integration.rs`.

**Not built**: the detection *triggers* US2 and US4 describe. `detection.live`,
`detection.scheduled` and `detection.on_open` are stored and validated but have no
consumer outside `root_config.rs`; nothing starts a frame-tree watcher or a
scheduled sweep, so reconciliation only happens when the user asks for it. The
`notify`-based watcher in `crates/workflow/artifacts/src/watcher.rs` belongs to
spec 012 (artifact observation) and does not cover frame inventory.

**Input**: User description: "Per-frame (per-file) inventory with live session membership. Complete PlateVault's per-frame inventory so raw sub-frame cleanup is possible and sessions stay honest about what is on disk."

## Overview

PlateVault groups ingested images into acquisition and calibration **sessions**. When this spec was written it did not keep an accurate, durable record of the **individual frames** (sub-exposures) that make up each session, nor of how large they are on disk. Two consequences followed:

1. **Raw sub-frame cleanup was impossible.** Raw lights, darks, and flats are the largest consumers of disk space, but because the app could not enumerate the individual frames it recorded (calibration frames were recorded with no frame list at all, and every frame's byte size was recorded as zero), the cleanup review flow refused to consider them and could only offer processing artifacts as cleanup candidates. The biggest disk win was unavailable. (All three conditions have since been removed — see the Status header.)
2. **Sessions drift from reality.** Astrophotographers routinely cull sub-frames **outside** PlateVault — reviewing in Blink or by hand and then deleting rejects or moving them to a "rejects" folder. After that, a PlateVault session still claims frames that are no longer where it recorded them. Counts, size totals, and cleanup previews become wrong, and calibration matches may silently reference frames that have vanished.

This feature completes the per-frame inventory so that every ingested frame — light **and** calibration — is recorded with its real on-disk size and its session membership; it teaches sessions to notice frames that were deleted or moved outside the app; and it unblocks the cleanup review flow to propose individual raw sub-frames as reviewable cleanup candidates. Consistent with PlateVault's principles, reconciliation only updates the app's records and what the user sees — it never deletes or moves a file as a side effect.

## Clarifications

### Session 2026-07-04

Decisions below were resolved with the user in a pre-spec grilling pass and a clarify pass; they are recorded here so the spec is self-contained.

- Q: What is the core scope? → A: Complete per-frame inventory (fill calibration frames, capture real sizes) + external-change/missing detection + unblock raw sub-frame cleanup. No new frame↔session join structure; sessions stay derived.
- Q: Default reconcile behavior when a frame is found absent? → A: Flag-missing by default; auto-reconcile is opt-in per library root; both only touch records/UI, never files.
- Q: How is external-change detection triggered? → A: Per-root config — live filesystem watching default-on (opt-out for removable/network, with polling fallback), user-selectable scheduled-background and on-library-open/on-project-open, and always-available on-demand rescan.
- Q: At what granularity are cleanup candidates generated? → A: Per individual raw sub-frame, presented grouped by session.
- Q: When a calibration frame referenced by a match goes missing? → A: Flag the match "source missing / unverifiable"; keep it — never auto-invalidate or remove.
- Q: When are per-frame records written? → A: At plan **apply** (move or catalogue-in-place) for all frame types; never at confirm time. Real byte size captured then.
- Q: In auto-reconcile mode, what happens to an absent frame's record? → A: Retain the record marked **missing** and drop it from active session membership; never hard-delete silently (audit/reversibility).
- Q: Should a frame moved to a new path under the same root be auto-followed? → A: No — treat as missing at the old path and surface it for the user. Any user-initiated relink MUST match on **sha256 content hash** (computed lazily, on demand), NOT size/mtime, because same-camera FITS share identical sizes and mtime is unreliable.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Accurate per-frame inventory for every session (Priority: P1)

As a user who has ingested light and calibration frames, I want each session to record the individual frames that belong to it, with each frame's true size on disk, so that frame counts and disk-usage totals are trustworthy and downstream features can act on individual frames.

**Why this priority**: This is the foundation. Without a complete, correctly-sized per-frame inventory, neither external-change detection nor raw sub-frame cleanup can function. On its own it delivers immediate value: honest frame counts and real per-session disk-usage totals.

**Independent Test**: Ingest a folder of light frames and a folder of calibration frames through the normal inbox confirm → apply flow. Verify that both the resulting acquisition session and calibration session list the individual frames they contain, and that the disk-usage total shown for each session equals the sum of the actual file sizes on disk (not zero).

**Acceptance Scenarios**:

1. **Given** a set of light frames confirmed and applied through the inbox, **When** the resulting acquisition session is viewed, **Then** it lists every applied light frame as a member and reports a non-zero total disk size equal to the sum of those files' sizes.
2. **Given** a set of calibration frames (darks/flats/bias) confirmed and applied through the inbox, **When** the resulting calibration session is viewed, **Then** it lists every applied calibration frame as a member with real per-frame sizes (previously calibration sessions recorded no frames).
3. **Given** a frame that is catalogued in place (organized source, no move), **When** it is applied, **Then** it is recorded as an inventory frame with its real size exactly as a moved frame would be.
4. **Given** frames already recorded before this feature with a size of zero, **When** the library root is next scanned, **Then** their recorded sizes are corrected to the real on-disk sizes.

---

### User Story 2 - Sessions notice frames removed or moved outside the app (Priority: P2)

As a user who culls rejected sub-frames in Blink or by hand outside PlateVault, I want the app to notice when a recorded frame has been deleted or moved on disk and reflect that in the session, so that my sessions, counts, and disk totals stay honest without me re-importing anything.

**Why this priority**: Keeps inventory trustworthy over time. Depends on US1 (there must be per-frame records to reconcile). Delivers standalone value: users see which recorded frames are no longer present.

**Independent Test**: Ingest and apply a session, then, outside the app, delete one frame and move another to a different folder. Trigger a rescan of that root. Verify the two affected frames are reported as missing (or, for a root configured to auto-reconcile, are dropped from session membership), that counts/totals update accordingly, and that no file on disk was created, deleted, or moved by the app.

**Acceptance Scenarios**:

1. **Given** a recorded frame that is deleted on disk outside the app **and** its root is configured to flag-missing (default), **When** the change is detected, **Then** the frame is marked **missing**, remains visible in the session flagged as missing, and the session's present-frame count and disk total exclude it — and no filesystem mutation occurs.
2. **Given** a recorded frame that is deleted on disk outside the app **and** its root is configured to auto-reconcile, **When** the change is detected, **Then** the frame is automatically dropped from the session's membership, and no filesystem mutation occurs.
3. **Given** a frame previously marked missing that reappears at its recorded location, **When** the change is detected, **Then** it returns to present and rejoins the session's active membership.
4. **Given** a frame moved to a different path under the same root, **When** the change is detected, **Then** it is treated as missing at its old location (the app does not silently re-home the record) and surfaces for the user, never mutating files.
5. **Given** a user chooses to relink a surfaced missing frame to a candidate file, **When** the relink is attempted, **Then** the app confirms the match by comparing sha256 content hashes (computed on demand at that moment), not file size or modification time, and only re-homes the record on a hash match.

---

### User Story 3 - Raw sub-frame cleanup candidates (Priority: P2)

As a user reclaiming disk space, I want the cleanup review flow to propose individual raw sub-frames as reviewable cleanup candidates, grouped by session, so that I can reclaim space from the frames that actually dominate my library — safely and reversibly.

**Why this priority**: This is the headline user value that motivated the feature. It depends on US1 (a real, sized per-frame inventory) and benefits from US2 (so candidates reflect what is still on disk). It is an independent slice once inventory exists.

**Independent Test**: With a session of applied raw frames present, run the cleanup review flow. Verify individual raw sub-frames appear as candidates grouped by their session, with the reclaimable size shown equal to the real sizes of the selected frames, that protected frames/categories are excluded, and that generating the plan performs no filesystem mutation.

**Acceptance Scenarios**:

1. **Given** a session with applied raw sub-frames present on disk, **When** a cleanup plan is generated, **Then** the individual raw sub-frames appear as candidates grouped under their session, with reclaimable size equal to the sum of the selected frames' real sizes, and no filesystem mutation occurs during generation.
2. **Given** frames or categories that are protected, **When** cleanup candidates are generated, **Then** protected frames are excluded from the candidate set.
3. **Given** a frame recorded as missing, **When** cleanup candidates are generated, **Then** the missing frame is not offered as a candidate (there is nothing to reclaim).
4. **Given** inference is used to classify a candidate, **When** it is presented, **Then** it carries a confidence level.

---

### User Story 4 - Configure detection and reconciliation per library root (Priority: P3)

As a user with different storage types (fast internal disks, removable USB drives, network shares), I want to choose, per library root, how the app watches for external changes and whether it flags or auto-drops missing frames — and to set this up when I add the root in the wizard — so that behavior matches each storage medium's realities.

**Why this priority**: Makes US2 correct across heterogeneous storage and honors the requirement to configure this in the wizard. Lower priority because sensible defaults (US2) work without any configuration.

**Independent Test**: In the setup wizard, add a root and confirm the detection/reconcile options are presented with the documented defaults; change them for an existing root in settings; verify the chosen behavior takes effect on the next detection cycle.

**Acceptance Scenarios**:

1. **Given** the setup wizard, **When** a user adds a library root, **Then** they can review and set that root's reconcile mode (flag-missing default, auto-reconcile opt-in) and its detection triggers, with documented defaults pre-selected.
2. **Given** a root on removable or network storage, **When** the user opts out of live filesystem watching, **Then** the app relies on the other configured triggers (scheduled, on-open, on-demand) for that root and does not attempt to hold a live watch.
3. **Given** a root's reconcile mode is changed from flag-missing to auto-reconcile, **When** the next detection cycle finds a missing frame, **Then** it is auto-dropped rather than flagged.
4. **Given** any root, **When** the user requests an on-demand rescan, **Then** it runs regardless of the other trigger settings.

---

### User Story 5 - Missing-frame awareness for calibration matches (Priority: P3)

As a user relying on calibration matches, I want a match that references a calibration frame which has gone missing to be flagged rather than silently broken or auto-removed, so that I understand the match may be unverifiable and can decide what to do.

**Why this priority**: Protects trust in calibration matching once external-change detection exists. Depends on US2. Low priority because it is an awareness refinement, not a core flow.

**Independent Test**: Establish a calibration match, then remove the referenced calibration frame outside the app and trigger detection. Verify the match is flagged "source missing / unverifiable", remains present (not invalidated or deleted), and that the user can act on it.

**Acceptance Scenarios**:

1. **Given** a calibration match whose referenced frame is later marked missing, **When** the match is viewed, **Then** it is flagged "source missing / unverifiable" and is not automatically invalidated or removed.
2. **Given** a previously missing referenced frame that reappears, **When** detection runs, **Then** the match's "source missing" flag clears.

---

### Edge Cases

- **Symlinks/junctions**: Scans and watches MUST NOT follow symlinks or junctions unless the user explicitly enables that for the root. Frames reachable only via an un-enabled link are not inventoried or reconciled through that link.
- **Hostile filesystems**: On network shares or media where live change events are unreliable, the app falls back to a polling/rescan strategy for that root rather than assuming events are delivered.
- **Removable drive absent**: When a root's storage is disconnected, its recorded frames are not treated as permanently deleted; they are reported as unavailable/missing and recover to present when the storage returns (no filesystem mutation, no auto-purge).
- **Duplicate paths within a root**: A frame's identity within a root is its location under that root; two records must never claim the same location.
- **Large libraries**: Reconciliation of a very large root must not block the UI and must report progress rather than appearing frozen.
- **Frame present but changed on disk** (e.g., re-saved, different size): the app notices the change and updates the recorded size; it does not treat a same-path changed file as missing.
- **Partial ingest / interrupted apply**: only frames whose apply actually succeeded are recorded as inventory; a frame that never completed apply is not counted.
- **Cleanup of a session with mixed present/missing frames**: only present frames are offered for cleanup; missing ones are omitted from reclaimable totals.

## Requirements *(mandatory)*

### Functional Requirements

**Per-frame inventory (US1)**

- **FR-001**: The system MUST record a durable per-frame inventory entry for every ingested frame — light and calibration — at the point its filesystem plan is applied (both moved and catalogued-in-place frames).
- **FR-002**: Each per-frame inventory entry MUST record the frame's real on-disk byte size, captured at apply time.
- **FR-003**: Calibration sessions MUST record their individual member frames (previously calibration sessions recorded none).
- **FR-004**: The system MUST NOT require content hashing to record a frame; any hashing remains optional/lazy and MUST NOT be performed eagerly at ingest. Where a content hash is needed (e.g., user-initiated relink of a missing frame), it MUST use sha256 and be computed on demand.
- **FR-005**: Sessions MUST remain **derived** groupings over confirmed inventory; this feature MUST NOT introduce a session review/lifecycle state machine.
- **FR-006**: When a root is scanned, per-frame entries previously recorded with a zero or unknown size MUST have their sizes corrected to the real on-disk value (backfill).

**External-change detection & reconciliation (US2)**

- **FR-007**: The system MUST detect when a recorded frame has been deleted or moved on disk outside the app.
- **FR-008**: Detection and reconciliation MUST only update the app's records and what the user sees; they MUST NEVER create, delete, move, or otherwise mutate files as a side effect.
- **FR-009**: For a root in flag-missing mode (default), a frame found absent MUST be marked **missing**, remain visible in its session flagged as missing, and be excluded from present-frame counts and disk totals.
- **FR-010**: For a root in auto-reconcile mode, a frame found absent MUST be automatically dropped from its session's active membership while its inventory record is retained (marked missing); the system MUST NOT hard-delete inventory records as a silent side effect of reconciliation.
- **FR-011**: A frame previously marked missing that is found present again at its recorded location MUST return to present and rejoin active membership.
- **FR-012**: A frame present at its recorded location but changed on disk MUST be updated in place (e.g., corrected size), not treated as missing.
- **FR-012a**: The system MUST NOT automatically re-home a frame whose file moved to a different path under the same root; it MUST surface such a frame as missing for the user. A user-initiated relink MUST confirm identity by sha256 content hash (computed on demand), never by file size or modification time.

**Per-root configuration (US4)**

- **FR-013**: Reconcile mode MUST be configurable per library root, defaulting to flag-missing, with auto-reconcile as an explicit opt-in.
- **FR-014**: Detection triggers MUST be configurable per library root and MUST include: live filesystem watching (default on), scheduled background reconciliation (opt-in), reconciliation on library-open and on project-open (opt-in), and on-demand rescan (always available).
- **FR-015**: Live filesystem watching MUST be opt-out for removable/network storage, and when disabled or unreliable, the system MUST fall back to the root's other configured triggers (including a polling/rescan strategy) rather than assuming live events.
- **FR-016**: The setup wizard MUST let the user review and set a root's reconcile mode and detection triggers when the root is added, with documented defaults pre-selected; these settings MUST be editable afterward.
- **FR-017**: Scans and watches MUST NOT follow symlinks or junctions for a root unless the user has explicitly enabled that for the root.

**Raw sub-frame cleanup (US3)**

- **FR-018**: The cleanup review flow MUST be able to enumerate per-frame inventory entries and propose individual raw sub-frames (lights, darks, flats) as reviewable cleanup candidates, grouped by their session.
- **FR-019**: Cleanup candidate generation MUST remain read-only (no filesystem mutation) and MUST produce reviewable plans, never immediate actions.
- **FR-020**: The reclaimable size reported for a cleanup selection MUST equal the sum of the selected frames' real sizes.
- **FR-021**: Protected frames and protected categories MUST be excluded from raw sub-frame cleanup candidates.
- **FR-022**: Frames recorded as missing MUST NOT be offered as cleanup candidates.
- **FR-023**: Where classification of a cleanup candidate relies on inference, the candidate MUST carry a confidence level.

**Calibration match awareness (US5)**

- **FR-024**: When a calibration frame referenced by a calibration match is marked missing, the match MUST be flagged "source missing / unverifiable" and MUST NOT be automatically invalidated or removed.
- **FR-025**: When a previously missing referenced frame returns to present, the match's "source missing" flag MUST clear.

### Key Entities *(include if feature involves data)*

- **Per-frame inventory entry**: A durable record of one ingested image file, located by its library root plus its path under that root, carrying its real on-disk size, its presence state (present / missing / recovered), and an optional, lazily-computed sha256 content hash (used only when identity confirmation is needed, e.g. relink). Recorded at plan apply for both moved and catalogued-in-place frames.
- **Session (derived)**: An acquisition or calibration grouping computed from confirmed inventory; it references the per-frame inventory entries that are its members. No review/lifecycle state.
- **Library root**: A modeled storage location, separate from the relative paths beneath it, carrying this feature's per-root reconcile mode and detection-trigger configuration and its symlink-following setting.
- **Reconciliation run**: A pass over a root (triggered live, on schedule, on open, or on demand) that compares recorded per-frame entries against what is on disk and updates presence state and membership according to the root's mode — never mutating files.
- **Cleanup candidate (per-frame)**: A reviewable proposal to archive/trash an individual raw sub-frame, grouped by session, carrying reclaimable size, protection status, and a confidence level where inferred.
- **Calibration match flag**: A "source missing / unverifiable" marker on an existing calibration match whose referenced frame is missing.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After ingesting light and calibration frames, 100% of applied frames of every type appear as members of their session (calibration sessions go from 0% frames recorded to 100%).
- **SC-002**: The disk-usage total shown for any session equals the sum of its present frames' real on-disk sizes (0% of frames report a zero size after apply or the next scan).
- **SC-003**: When a user deletes or moves a recorded frame outside the app and reconciliation runs, the session reflects the change (flagged missing or dropped, per root mode) with zero files created, deleted, or moved by the app.
- **SC-004**: The cleanup review flow can propose raw sub-frames (lights/darks/flats) as candidates — a capability that is impossible today — with reclaimable size accurate to the real frame sizes.
- **SC-005**: Reconciliation of a library root of at least 10,000 frames completes without blocking the UI and reports progress throughout.
- **SC-006**: 100% of calibration matches whose referenced frame goes missing are flagged (not silently broken or removed), and the flag clears when the frame returns.

## Assumptions

- Inbox confirm remains the single ingest gate and plan **apply** is the point at which frames become durable inventory; this feature does not add a competing ingest path and does not write inventory at confirm time.
- Sessions are derived, already-confirmed inventory (no lifecycle state machine is introduced or reintroduced).
- The existing on-demand rescan / reconciliation pattern used for processing artifacts is a suitable model for raw-frame missing detection and can be reused.
- Cross-platform live filesystem change notification is available on the supported desktop platforms; where it is unreliable, per-root polling/rescan is the fallback.
- The root-settings-window **redesign** is out of scope and handled by a separate companion UI spec; this spec owns the per-root setting's storage, its contract, and a minimal wizard hook to set it.
- Byte sizes are obtainable cheaply at apply time without hashing file contents.
- Frame identity within a root is the frame's path under that root; the root abstraction lets moved/remapped roots be recovered without rewriting session history. File size is explicitly NOT a reliable identity key (same-camera FITS share identical sizes); content-based identity uses sha256 computed on demand.
