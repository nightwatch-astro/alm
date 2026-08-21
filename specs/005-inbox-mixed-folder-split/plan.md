# Implementation Plan: Inbox Mixed-Folder Split

**Branch**: `005-inbox-mixed-folder-split` | **Date**: 2026-05-22 (updated) | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/005-inbox-mixed-folder-split/spec.md`

## Summary

Detect whether each Inbox folder is single-type or mixed by reading the FITS
`IMAGETYP` keyword for every file and normalizing via the
`ImageTypNormalizationTable`. Classification is **deterministic** — no
confidence scores, no filename heuristics. Surface a file-level breakdown in
the desktop UI. For mixed folders, generate a reviewable filesystem plan —
one plan item per file, destinations targeting Inventory directly (not sibling
staging) — through the existing plan-apply pipeline. For single-type folders,
confirm directly to Inventory. Maintain the invariant that any Inbox item has
at most one open plan at a time. (Ref: R-IMAGETYP, A5, R-Split-1)

This feature is a **producer** for the reviewable plan pipeline (spec 017
+ spec 025) and a **consumer** of the Naming & Structure token pattern
(spec 015). It does not own mutation, audit, or destination semantics.

## Technical Context

**Language/Version**: Rust 1.75+ (core, classifier, plan generator);
TypeScript 5.x + React 18 (desktop UI shell).  
**Primary Dependencies**: Future Rust crates — `crates/metadata/fits`,
`crates/metadata/xisf`, `crates/metadata/core`, `crates/domain/core`,
`crates/app/core`, `crates/fs/planner`, `crates/patterns/` (spec 015
resolver, per R-CratePatterns), `crates/persistence/db`. Frontend:
existing `apps/desktop/` shell.  
**Storage**: SQLite via `crates/persistence/db` for classification cache,
Inbox item state, and the Inbox→Plan back-reference. Image files remain on
disk per constitution principle I.  
**Testing**: `cargo test --workspace` for crates and contract conformance;
`vitest` for UI hooks; fixture folders for end-to-end classification.  
**Target Platform**: Tauri desktop on Windows, macOS, Linux.  
**Project Type**: Desktop monorepo (Tauri + Rust workspace).  
**Performance Goals**: Classify a 500-file folder in under 2 seconds
(metadata cache hit); generate a 500-item split plan in under 5 seconds
(per SC-004).  
**Constraints**: Local-first, no network calls; lazy/optional hashing;
classifier MUST not modify any image file.  
**Scale/Scope**: Inbox folders up to ~10k files per folder; up to ~200
candidate folders per scan root.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Local-first file custody**: PASS. Classifier reads headers only; no
  files are copied into an app-private store. Library roots remain modeled
  separately from relative paths through `crates/fs/inventory`.
- **Reviewable filesystem mutation**: PASS. The only mutation this feature
  triggers is the split plan, which flows through `crates/fs/planner` and
  the existing plan-apply pipeline (spec 017 + 025). No silent overwrites;
  destructive operations route through trash/archive per the planner crate.
- **PixInsight boundary**: PASS. No calibration, debayer, registration,
  integration, drizzle, stacking, or pixel manipulation. Classification
  reads headers only.
- **Research-led domain modeling**: PASS. See
  [research.md](./research.md) for the deterministic IMAGETYP-only
  classification model, normalization table, prior art comparison (PI WBPP
  file classifier), video lane separation, and content-signature TOCTOU
  safety. (Ref: R-IMAGETYP, A5)
- **Portable contracts and durable records**: PASS. Two language-neutral
  JSON Schema contracts under `contracts/`. The SQLite database is the
  durable record for Inbox items, classifications, and plan
  back-references. The classification result is reproducible from
  metadata and is therefore a projection, not canonical.
- **Cross-platform path safety**: PASS. The plan generator delegates path
  composition to `crates/fs/planner` and the Naming & Structure pattern
  (spec 015), both of which carry path-safety responsibilities.

No violations identified; Complexity Tracking section is empty.

## Project Structure

### Documentation (this feature)

```text
specs/005-inbox-mixed-folder-split/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
└── contracts/
    ├── inbox.classify.json
    └── inbox.confirm.json
```

### Source Code (repository root)

```text
crates/
├── domain/core/                 # FrameType enum, InboxClassificationRule
├── metadata/core/               # shared FrameType extraction + ImageTypNormalizationTable
├── metadata/fits/               # FITS IMAGETYP, FILTER, OBJECT readers
├── metadata/xisf/               # XISF equivalents
├── metadata/video/              # video file detection for inbox.video.* lane
├── patterns/                    # spec 015 pattern resolver (R-CratePatterns)
├── fs/planner/                  # Plan + PlanItem (already planned; consumer)
├── app/core/                    # InboxClassifyUseCase, InboxConfirmUseCase, InboxReclassifyUseCase
├── persistence/db/              # Inbox/Classification/PlanLink repositories
└── contracts/core/              # Rust DTOs matching contracts/*.json

apps/desktop/
└── src/features/inbox/          # UI shell (mockup-only today; to be rewired to contracts)

packages/contracts/
└── inbox/                       # generated TS types from contracts/*.json

tests/
├── contract/inbox/              # contract conformance tests
└── integration/inbox/           # fixture-folder end-to-end tests
```

**Structure Decision**: Single Tauri-desktop monorepo using existing crate
boundaries. The classifier lives in `crates/app/core` orchestrating
`crates/metadata/*` adapters; plan generation reuses `crates/fs/planner`.
No new top-level packages.

## Architecture

### Classifier

- Input: `inbox_item_id` and an optional `force_rescan` flag.
- Reads cached evidence when `content_signature` matches; falls back to
  `crates/metadata/{fits,xisf}` adapters on miss.
- Frame-type classification rule (deterministic — no confidence scores):
  - For each file, read `IMAGETYP` (or XISF equivalent) and normalize via
    `ImageTypNormalizationTable` in `crates/metadata/core`.
  - Files with no readable or recognized `IMAGETYP` receive per-file
    `unclassified = true` markers.
  - Video files (`.ser`, `.avi`, `.mp4`, `.mov`) are skipped; routed to
    the `inbox.video.*` lane.
  - Folder aggregation: `single_type T` if all classified files map to T;
    `mixed` if two or more types appear; `unclassified` if every file is
    unclassified.
- Computes `contentSignature` (R-Sig-1 formula) and returns it in the response.
- Output: `InboxClassification` with breakdown, per-file evidence records,
  unclassified file count, and `contentSignature`. (Ref: R-IMAGETYP, A5, A8)

### Plan Generator

- Driven by the active Naming & Structure token pattern (spec 015),
  resolved via `crates/patterns/` (R-CratePatterns), recorded on the plan.
- Destination paths target Inventory directly — no sibling staging step.
  One plan per `inbox.confirm` action. (Ref: R-Split-1)
- File list comes from `InboxClassificationEvidence.relativeFilePath` rows,
  NOT from `InboxItem.fileCount`. (Ref: A9)
- One `PlanItem` per scanned file, carrying the actual source and destination
  paths. Items grouped by frame type via a group key.
- Validates that every token required by the pattern resolves for every
  file. If any file fails, the whole operation fails `pattern.unset` /
  `pattern.unresolved` with no partial plan.
- When the resulting plan includes destructive items, the caller MUST supply
  `destructiveDestination: "archive" | "os_trash"`. (Ref: R-DestChoice)
- Verifies `contentSignature` matches before creating the plan; returns
  `classification.stale` if drift is detected. (Ref: A8)

### Store Integration

- `crates/persistence/db` owns:
  - `inbox_items` (one row per leaf folder; `lane` field distinguishes FITS vs video)
  - `inbox_classifications` (current result, with `force_rescan` invalidation)
  - `inbox_classification_evidence` (per-file records; `unclassified` + `manual_override` fields)
  - `inbox_plan_links` (Inbox item → open plan id, enforced unique
    where plan state is open)
- The plans publisher (spec 017 event bus) emits state transitions. This
  feature subscribes to `plan.applying.completed`, `plan.applying.paused`,
  and `plan.discarded` to mark Inbox items resolved or release the open-plan
  lock. A background repair query runs every 5 minutes as a safety net.
  (Ref: E4, R-PlanOpen)

### Future Rust Crate Mapping

| Concern | Crate |
|---|---|
| FITS header reads (IMAGETYP, FILTER, OBJECT) | `crates/metadata/fits` |
| XISF header reads | `crates/metadata/xisf` |
| Video file detection (inbox.video.* lane) | `crates/metadata/video` |
| Shared FrameType + ImageTypNormalizationTable | `crates/metadata/core` |
| Classification rule + invariants | `crates/domain/core` |
| Pattern resolver (spec 015) | `crates/patterns/` (R-CratePatterns) |
| Orchestration use cases | `crates/app/core` |
| Plan items + grouping | `crates/fs/planner` |
| DTOs for contracts | `crates/contracts/core` |
| SQLite persistence | `crates/persistence/db` |

### Contract Surface

- `inbox.classify` — idempotent unless `force_rescan: true`. Response
  includes `contentSignature`. (Ref: A8)
- `inbox.confirm` — action depends on classification: `action: "split"` for
  `mixed`, `action: "confirm"` for `single_type`. Requires
  `contentSignature`. Returns `classification.stale` on drift. Accepts
  `destructiveDestination` when plan has destructive items. (Ref: A8,
  R-DestChoice)
- `inbox.reclassify` — NEW. Accepts a list of `{ filePath, frameType }`
  entries. Writes `manualOverride` to evidence rows and triggers
  re-aggregation. (Ref: R-Unclass-1, R-Unclass-2)

## Complexity Tracking

No constitution violations; no entries.
