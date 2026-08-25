# Implementation Plan: Per-Frame Inventory with Live Session Membership

**Branch**: `048-per-frame-inventory` | **Date**: 2026-07-04 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/048-per-frame-inventory/spec.md`

## Summary

Complete PlateVault's per-frame inventory so raw sub-frame cleanup is possible and derived sessions stay honest about what is on disk. The primary requirements: (1) write a durable per-frame `file_record` with a real byte size at plan **apply** for every frame type — fixing calibration sessions that record `frame_ids = '[]'` and light records that land with `size_bytes = 0`; (2) add external-change ("missing") detection for raw roots, reusing the spec-012 artifact reconciler pattern plus per-root live watching, governed by a per-root flag-missing (default) / auto-reconcile toggle set in the wizard, mutating records/UI only; (3) unblock `crates/app/core/src/cleanup_generator.rs` to enumerate per-frame records with real sizes and propose individual raw sub-frames as reviewable candidates grouped by session; (4) flag (not invalidate) calibration matches whose referenced frame goes missing. The technical approach reuses existing rails — `file_record` (already has `size_bytes`/`content_hash`/`state`), the `acquisition_session`/`calibration_session` `frame_ids` arrays, the plan-apply listener that already writes light records, the artifact reconciler's present/missing/recovered state machine, and the spec-018 settings store — rather than introducing a frames↔sessions join table (the T006 relational table stays deferred).

## Technical Context

**Language/Version**: Rust (workspace edition per `Cargo.toml`), TypeScript/React for the desktop shell.

**Primary Dependencies**: `sqlx` (SQLite), `notify` v7 (already a dependency; wraps `ReadDirectoryChangesW`/`inotify`/`FSEvents`/`kqueue`), Tauri + tauri-specta for the contract boundary, the spec-018 settings store, the spec-002 audit event bus.

**Storage**: SQLite (canonical local store). Reuses `file_record` (migration `crates/persistence/core/migrations/0001_initial_schema.sql:728-739`), `acquisition_session` / `calibration_session` (`frame_ids` JSON arrays), and the spec-018 settings KV. Per-root config extends the settings KV (see research R1); no new frame/session tables.

**Testing**: `cargo test` (per-crate to avoid the known workspace-red baseline), integration tests under `tests/`, TS typecheck; real-app verification via `verify-on-windows` + a tauri-driver Layer-2 journey.

**Target Platform**: Desktop (Windows primary dev target, plus macOS/Linux) via Tauri.

**Project Type**: Desktop app over granular Rust crates with a language-neutral contract boundary.

**Performance Goals**: Reconciliation of a ≥10,000-frame root completes without blocking the UI and reports progress (SC-005). No eager hashing; byte size via `stat` only.

**Constraints**: Local-first custody; reconciliation MUST NOT mutate files; hashing lazy (sha256 on demand only); scans/watches MUST NOT follow symlinks/junctions unless enabled per root; sessions remain derived (no lifecycle state machine).

**Scale/Scope**: Libraries of tens of thousands of frames across multiple roots, some on removable/network storage.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|-----------|------------|
| I. Local-First File Custody | **PASS.** Frames stay on disk; per-frame records reference them by `root_id` + `relative_path` (existing `file_record UNIQUE(root_id, relative_path)`). No files copied into an app store. Root remapping preserved via the root abstraction. |
| II. Reviewable Filesystem Mutation | **PASS.** Reconciliation only updates records/UI and emits audit events (`frame.missing`/`frame.recovered`); it never moves/deletes files. Cleanup candidates are reviewable plans generated read-only; destructive actions prefer archive/trash and flow through the existing plan-apply path (which now also has the PR #408 cross-plan overlap guard). Inference-based cleanup carries confidence. |
| III. PixInsight Boundary | **PASS.** No calibration/registration/integration/editing; this feature only inventories, reconciles, and plans cleanup. |
| IV. Research-Led Domain Modeling | **PASS.** Open modeling choices (per-root config storage, detection strategy per storage class, move-follow identity) are resolved in `research.md` with options + tradeoffs + defaults. |
| V. Portable Contracts & Durable Records | **PASS.** New operations expressed as language-neutral contracts (`crates/contracts/core` + `packages/contracts` + generated bindings). SQLite is the durable record; reconciliation is a projection over disk truth. |

**Result**: PASS (initial). Re-checked after Phase 1 design — still PASS (no new violations; see Complexity Tracking — empty).

## Project Structure

### Documentation (this feature)

```text
specs/048-per-frame-inventory/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (operations.md)
├── checklists/
│   └── requirements.md  # Spec quality checklist (from /speckit-specify)
└── tasks.md             # Phase 2 output (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
crates/
├── app/
│   ├── targets/src/ingest_sessions.rs   # write real size_bytes at apply; shared per-frame writer
│   ├── inbox/src/plan_listener.rs        # fill calibration frame_ids + write calibration file_records (fix hardcoded '[]')
│   ├── core/src/cleanup_generator.rs     # consume per-frame records (real size), group by session; drop stale raw-refusal
│   └── settings/                         # per-root reconcile-mode + detection-trigger config (spec-018 KV)
├── fs/
│   ├── inventory/src/watcher.rs          # per-root live watch + symlink/junction gating + removable/network opt-out/polling
│   └── planner/                          # (PR #408 path_set overlap guard — reused, not modified)
├── workflow/artifacts/                    # reconciler.rs = pattern to mirror for raw-frame reconcile
├── calibration/core/                      # flag matches whose referenced frame is missing
├── persistence/db/                        # repositories for per-frame reconcile queries; impl-time migration for per-root config if KV insufficient
└── contracts/core/                        # DTOs for reconcile/rescan/relink + per-root config

apps/desktop/
├── src-tauri/src/lib.rs                   # wire raw-root reconciler/watcher at startup (near start_inbox_plan_listener)
└── src/features/                          # wizard per-root config step; session/inventory surfaces show missing frames
packages/contracts/                        # generated TS surface for the new operations
tests/                                     # integration: apply→inventory, delete→rescan→missing, cleanup raw-sub preview
```

**Structure Decision**: Extend the existing granular crates rather than adding new ones. The per-frame inventory entity is the existing `file_record`; session membership stays the `frame_ids` JSON arrays; reconciliation is a new module mirroring `crates/workflow/artifacts/reconciler.rs`; per-root config extends the spec-018 settings store. This keeps pure-domain crates free of new cross-crate deps and follows the crate-split-by-domain rule.

## Phase 0 — Research

See [research.md](./research.md). Resolves: R1 per-root config storage (settings KV vs `library_root` columns), R2 detection strategy per storage class (live/scheduled/on-open/on-demand + polling fallback), R3 move-follow identity (sha256 on demand, not size/mtime), R4 how the cleanup generator consumes per-frame records, R5 calibration-match "source missing" flagging, R6 symlink/junction gating, R7 backfill of `size_bytes = 0` records.

## Phase 1 — Design

See [data-model.md](./data-model.md), [contracts/operations.md](./contracts/operations.md), and [quickstart.md](./quickstart.md).

## Complexity Tracking

> No constitution violations. Table intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
