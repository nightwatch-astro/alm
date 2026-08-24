> **MIGRATED:** current truth now lives under `docs/journeys/`, whose
> `INDEX.md` is the generated routing table. This file is frozen legacy
> history from the pre-migration campaign and may not match current
> behavior. See `docs/product/journeys/README.md`.

# Wave 0 rerun plan — per-journey state

For each journey Wave 0 touches: the union of stages needing rerun, which tasks trigger each, the minimal rerun set, and coverage gaps. This is the sheet the team executes: rerun only the listed stages, not whole journeys. Stage labels are quoted from `JNN-slug/journey.md`; NEW stages (behavior Wave 0 introduces that no current journey stage covers) are flagged **journey doc update needed**.

## J1 — J01-first-run-setup-data-sources

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t123 | Stage 7 "Ongoing management (Settings → Data Sources) … Override protection (set → visible change → and backend readback agrees → remove override)". | [delta](J01-first-run-setup-data-sources/deltas/2026-07-14-q15-t123.md) |
| q15-t125 | Stage 7 "Rescan a folder … Remap … Disable … Delete (un-register)" | [delta](J01-first-run-setup-data-sources/deltas/2026-07-14-q15-t125.md) |

- **L2/manual gap:** gap — `register_light_root`/`register_project_root` are helpers in `inbox_ui_journeys.rs`; no dedicated Data-Sources L2

## J2 — J02-ingest-review-reclassify-confirm-move

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t125 | Stage 1 "On Inbox, Rescan picks up new folders". | [delta](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q15-t125.md) |
| q16-t131 | Stage 2 "the item surfaces a needs-review state … affected rows get \"needs <attribute>\" badges" | [delta](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q16-t131.md) |
| q16-t132 | Stage 1 "a status-bar breakdown always matches the queue's real contents" | [delta](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q16-t132.md) |
| q16-t133 | Stage 2 inbox detail | [delta](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q16-t133.md) |
| q27-f10 | NEW stage inside Stage 4 "Confirm" | [delta](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q27-f10.md) |
| q27-f5 | NEW stage inside Stage 4 "Confirm turns a classified item into a plan" | [delta](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q27-f5.md) |
| q27-f6 | NEW stage inside Stage 4 "Confirm" | [delta](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q27-f6.md) |


## J3 — J03-ingest-confirm-catalogue-in-place

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q27-f10 | NEW stage inside Stage 4 "Applying the plan writes the files' identity and metadata into the library's index" | [delta](J03-ingest-confirm-catalogue-in-place/deltas/2026-07-14-q27-f10.md) |
| q27-f5 | NEW stage inside Stage 2 "Confirming an item that came from an organized root produces a plan whose actions are all catalogue in place" | [delta](J03-ingest-confirm-catalogue-in-place/deltas/2026-07-14-q27-f5.md) |


## J4 — J04-sessions-review-derived

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q16-t129 | Stage 2 "the corresponding acquisition session(s) appear automatically, with counts matching what was actually moved/catalogued". | [delta](J04-sessions-review-derived/deltas/2026-07-14-q16-t129.md) |
| q16-t131 | Stage 2 "Detail panel … unresolved values render as an explicit unresolved state, not bare dashes" | [delta](J04-sessions-review-derived/deltas/2026-07-14-q16-t131.md) |
| q16-t132 | Stage 1 "each session row must be distinguishable even when FITS metadata is missing" | [delta](J04-sessions-review-derived/deltas/2026-07-14-q16-t132.md) |
| q16-t133 | Stage 2 "shows the session's frame type and calibration linkage in addition to what the row already showed" | [delta](J04-sessions-review-derived/deltas/2026-07-14-q16-t133.md) |
| q27-f10 | Stage 2 "the corresponding acquisition session(s) appear automatically" | [delta](J04-sessions-review-derived/deltas/2026-07-14-q27-f10.md) |


## J5 — J05-project-lifecycle

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q16-t132 | Stage 3 "the project detail's per-channel (per-filter) breakdown shows actual sub-frame counts and total integration time … not a placeholder dash" | [delta](J05-project-lifecycle/deltas/2026-07-14-q16-t132.md) |
| q16-t133 | Stage 1–4 project detail | [delta](J05-project-lifecycle/deltas/2026-07-14-q16-t133.md) |
| q27-f11 | NEW stage (the framing-grouping stage) | [delta](J05-project-lifecycle/deltas/2026-07-14-q27-f11.md) |
| q27-f2 | NEW stage (between Stage 2 "Attach sources" and Stage 3 "Review the real numbers") | [delta](J05-project-lifecycle/deltas/2026-07-14-q27-f2.md) |
| q27-f3 | NEW stage (extends the framing-grouping stage) | [delta](J05-project-lifecycle/deltas/2026-07-14-q27-f3.md) |
| q27-f4 | Stage 1 "Create (/projects/new): name the project, optionally pick a processing-tool profile" | [delta](J05-project-lifecycle/deltas/2026-07-14-q27-f4.md) |
| q27-f6 | Stage 4/5 area | [delta](J05-project-lifecycle/deltas/2026-07-14-q27-f6.md) |
| q27-f7 | NEW stage | [delta](J05-project-lifecycle/deltas/2026-07-14-q27-f7.md) |


## J6 — J06-cleanup-scan-review-apply

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t123 | Stage 3 "if any protected item is included, its protection must be explicitly acknowledged before Approve & apply becomes clickable". | [delta](J06-cleanup-scan-review-apply/deltas/2026-07-14-q15-t123.md) |

- **L2/manual gap:** gap — cleanup L2 not present; `journeys.rs::cleanup_plan_review` is L1

## J7 — J07-archive-delete

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t123 | Stage 2 "reviewing it (protected items must be acknowledged, same as cleanup)". | [delta](J07-archive-delete/deltas/2026-07-14-q15-t123.md) |
| q16-t132 | Stage 3 "The Archive page lists archived projects with their real audit history (not placeholder rows)" | [delta](J07-archive-delete/deltas/2026-07-14-q16-t132.md) |


## J8 — J08-calibration-ingest-masters-matching

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t122 | Stage 5 "An 'Offset tolerance' setting (Settings → Calibration) controls whether sessions … can match" | [delta](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q15-t122.md) |
| q16-t128 | Stage 2 "The Calibration page shows one row per master file, with kind-conditional fingerprint columns". | [delta](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t128.md) |
| q16-t129 | Stage 3 "selecting a master surfaces ranked candidate sessions … real context (target, filter, night, frame count) rather than opaque ids". | [delta](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t129.md) |
| q16-t131 | Stage 2 "one row per master file, with kind-conditional fingerprint columns … show as a dash by design" | [delta](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t131.md) |
| q16-t132 | Stage 2 "MastersTable meta lines/cells" | [delta](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t132.md) |
| q16-t133 | Stage 2 master detail | [delta](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t133.md) |


## J9 — J09-targets-planning

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q16-t132 | Stage 4 "Opposition and Sessions columns always render as a dash today" | [delta](J09-targets-planning/deltas/2026-07-14-q16-t132.md) |
| q16-t133 | Stage 3 "Target detail shows real identity data … add/remove their own aliases … observing notes" | [delta](J09-targets-planning/deltas/2026-07-14-q16-t133.md) |


## J10 — J10-settings-appearance-i18n

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t122 | Stage 1 "Settings groups … panes … Every pane auto-saves" | [delta](J10-settings-appearance-i18n/deltas/2026-07-14-q15-t122.md) |
| q15-t126 | Stage 5 "The bottom log panel … filters by severity (chips for Error/Warn/Info/Debug) … only shows deep diagnostics once the log level is turned down to Debug". | [delta](J10-settings-appearance-i18n/deltas/2026-07-14-q15-t126.md) |
| q27-f11 | Stage 1 "Settings groups … panes … Every pane auto-saves" | [delta](J10-settings-appearance-i18n/deltas/2026-07-14-q27-f11.md) |


## J12 — J12-failure-refusal-handling

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t127 | Stage 1 "A refused lifecycle transition surfaces its refusal reason inline … the same reason the audit record stores".; Stage 5 "Every refusal/failure is afterwards findable in the Audit Log with outcome refused/failed and the same reason on demand". | [delta](J12-failure-refusal-handling/deltas/2026-07-14-q15-t127.md) |
| q16-t130 | Stage 3 "A partial apply failure lists failed items by name with per-item reasons". | [delta](J12-failure-refusal-handling/deltas/2026-07-14-q16-t130.md) |

- **L2/manual gap:** gap — `journeys/failure-refusal-handling` scenario to be authored

## J13 — J13-audit-activity-investigation

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t122 | Stage 2 "Settings → Audit Log holds the durable record"; Stage 3 "Filtering by entity/date narrows the trail" | [delta](J13-audit-activity-investigation/deltas/2026-07-14-q15-t122.md) |
| q15-t123 | Stage 2 "Settings → Audit Log holds the durable record … every attempted mutating action". | [delta](J13-audit-activity-investigation/deltas/2026-07-14-q15-t123.md) |
| q15-t124 | Stage 3 "Filtering by entity/date narrows the trail". | [delta](J13-audit-activity-investigation/deltas/2026-07-14-q15-t124.md) |
| q15-t125 | Stage 2 "Settings → Audit Log holds the durable record … plan applications without exception". | [delta](J13-audit-activity-investigation/deltas/2026-07-14-q15-t125.md) |
| q15-t126 | Stage 1 "The status-bar Log toggle opens the Activity panel: live stream, severity chips, follow mode". | [delta](J13-audit-activity-investigation/deltas/2026-07-14-q15-t126.md) |
| q15-t127 | Stage 2 "Settings → Audit Log holds the durable record". | [delta](J13-audit-activity-investigation/deltas/2026-07-14-q15-t127.md) |
| q27-f3 | Stage 2 "Settings → Audit Log holds the durable record". | [delta](J13-audit-activity-investigation/deltas/2026-07-14-q27-f3.md) |

- **L2/manual gap:** gap — no L2 covers non-plan audit rows (settings/protection/equipment/source)

## J14 — J14-target-first-project-start

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q27-f4 | Stage 2 "the wizard opens with the association already made: name pre-filled … the target shown as a fact in the summary rail" | [delta](J14-target-first-project-start/deltas/2026-07-14-q27-f4.md) |

- **L2/manual gap:** gap — `journeys/target-first-project` scenario to be authored

## J15 — J15-equipment-observing-site-setup

| Task | Stage(s) hit | Delta |
|------|--------------|-------|
| q15-t124 | Stage 1 "Settings → Equipment: register cameras and telescopes … compose optical trains"; Stage 2 "Filters: adjust the seeded list" | [delta](J15-equipment-observing-site-setup/deltas/2026-07-14-q15-t124.md) |

- **L2/manual gap:** gap — `journeys/equipment-site-setup` scenario to be authored

## NEW stages Wave 0 introduces (journey doc update needed)

These behaviors exist in no current journey stage. Each needs a user-journeys stage addition AND new Layer-2 coverage (flagged as gaps above). Do NOT renumber existing stages — append.

| Journey | New behavior | Tasks |
|---------|--------------|-------|
| J2 / J3 | Inbox confirm gains an **attribution-suggestion** sub-stage (ranked `IngestionAttributionCandidate`s) and a **chosenAttribution** persist step. | q27-f5, q27-f6, q27-f10 |
| J5 | Project detail gains **framing grouping** (suggested clustering), **merge/split/reassign**, an **is_mosaic** create field, and (blocked) per-framing source views/manifests. | q27-f2, q27-f3, q27-f4, q27-f7 |
| J5 (reopen) | Attributing new subs to a **completed** project reopens it via the spec-009 completed→processing edge. | q27-f6 |
| J10 | Settings gains **framing clustering tunables** (R11a defaults). | q27-f11 |
| J14 | Target-first create exposes the **is_mosaic** flag with target inherited by all framings. | q27-f4 |
| J13 | Audit now durably covers **settings, protection, equipment, and source/rescan** mutations (previously bus-only) plus **framing adjustments** — the Audit Log's coverage story widens well beyond lifecycle/plan events. | q15-t122, q15-t123, q15-t124, q15-t125, q27-f3 |

## Coverage gaps (no L2 today)

- **Framing (J2/J3/J5):** no Layer-2 covers attribution/clustering/mosaic; authored by **F-Framing-9** (Windows-E2E) + extended `inbox_ui_*`.
- **Non-plan audit (J13):** no L2 covers settings/protection/equipment/source audit rows; only plan events are covered via `plan_review_apply_with_audit`.
- **Refusal handling (J12):** `journeys/failure-refusal-handling` scenario to be authored; unresolved-chip-vs-error distinction (q16-t130) has no L2.
- **Equipment (J15):** no dedicated coverage-matrix area or L2; `journeys/equipment-site-setup` to be authored.
