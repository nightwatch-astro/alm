# Research: Per-Frame Inventory with Live Session Membership

**Feature**: 048-per-frame-inventory | **Date**: 2026-07-04

Ground-truth code references confirmed during exploration (redesign-ui-platevault base @ a2936734, plus campaign coordination facts as of 2026-07-04).

## Baseline (what already exists)

- **`file_record`** (migration `crates/persistence/core/migrations/0001_initial_schema.sql:728-739`): `id, root_id→library_root, relative_path, size_bytes, mtime, content_hash (nullable), state CHECK(observed|changed|classified|missing|rejected|protected), first_seen_at, last_seen_at, UNIQUE(root_id, relative_path)`; indexes on `root_id`, `state`. This IS the per-frame inventory entity — the `'missing'` and `'protected'` states already exist.
- **Light path already writes records**: `crates/app/targets/src/ingest_sessions.rs` (wired at `apps/desktop/src-tauri/src/lib.rs:711` via `start_inbox_plan_listener`) writes `file_record` (`upsert_file_record` ~:314) and appends real ids into `acquisition_session.frame_ids` (`upsert_session` ~:389). **Gap: writes `size_bytes = 0` and no hash.**
- **Calibration path writes nothing per-frame**: `crates/app/inbox/src/plan_listener.rs:211-214` (`register_master_if_applicable`) hardcodes `INSERT INTO calibration_session (... frame_ids ...) VALUES (?, ?, '[]', ...)`. **Gap: calibration frames are never recorded.**
- **No frames↔sessions join table**: `crates/persistence/core/migrations/0001_initial_schema.sql:29` defers it ("relational join table deferred to T006"); membership is the JSON `frame_ids` array. Kept as-is (spec scope decision).
- **Cleanup generator**: `crates/app/core/src/cleanup_generator.rs` (merged PR #389, two-step scan→generate per D11) classifies off `processing_artifacts.kind`; the stale comment at `:24-30` says per-frame files can't be enumerated. Archive destination `.astro-plan-archive/<planId>/` (D24); destructive vocab canonical `archive | trash`.
- **Artifact reconciler**: `crates/workflow/artifacts/reconciler.rs` — on-attach rescan → `present`/`missing`/`user_resolved`; `app_core::artifact::mark_missing`/`recovered`; audit events `artifact.missing`/`recovered` on the spec-002 event bus. PR #409 adds longest-prefix path→project attribution (`crates/workflow/artifacts/src/project_mapping.rs`).
- **Watchers**: `crates/fs/inventory/src/watcher.rs` (`notify` v7, inbox-only, `InboxFileEvent::{Added,Removed,Modified}`, `RecursiveMode::Recursive` — **symlink gating absent**); `artifact_watcher.rs` (per-project output, drawer-lifecycle attach/detach via `ArtifactWatcherRegistry`).
- **Settings store**: spec-018; the generic `protection_defaults` KV (scope/key/value) is the D13 precedent — extend generic KV before minting new tables.
- **Cross-plan overlap guard**: PR #408 registers subtree-prefix path sets (`crates/fs/planner/src/path_set.rs` + `app_core::plan_apply`); per-frame cleanup plans inherit this via the shared apply path.
- **Wizard**: the real unified 5-step first-run page (spec 038/003) — verify against code before wiring the per-root step.

---

## R1 — Per-root config storage (reconcile mode + detection triggers + symlink flag)

**Decision**: Store per-root config in the spec-018 settings KV, scoped per root (following the `protection_defaults` D13 precedent), rather than adding columns to `library_root`.

**Options considered**:
- (A) Settings KV scoped per root — reuses existing store, no migration for the spec, matches D13 "extend generic KV before new tables", easy to default-when-absent. **Chosen.**
- (B) New columns on `library_root` (`reconcile_mode`, `detection_triggers`, `follow_symlinks`) — more discoverable/typed, but requires a migration and touches a hot table shared with root remapping; heavier.
- (C) A dedicated `root_reconcile_config` table — most explicit but the most schema surface for a small, sparse config.

**Rationale**: The config is small, sparse, and per-root; the KV precedent already exists and avoids a migration in the spec phase. Keys (illustrative): scope = root id; `reconcile.mode` ∈ {`flag_missing`(default), `auto_reconcile`}; `detection.live` (bool, default true), `detection.scheduled` (bool/cadence, default off), `detection.on_open` (bool, default off), `detection.follow_symlinks` (bool, default false). If typed validation or querying pressure later demands it, migrate to (B) — noted for impl, not spec.

**Constitution**: symlink-follow default `false` satisfies "MUST NOT follow symlinks unless enabled per root".

## R2 — Detection strategy per storage class

**Decision**: Per-root, layered triggers with a live watch by default and rescan-based fallbacks.

- **Live watch (default on)**: reuse `notify` v7 with a per-root scope modeled on `ArtifactWatcherRegistry` attach/detach, but keyed to raw/calibration roots and their library/project lifecycle. Detach when the relevant surface closes; do not hold live watches on idle roots indefinitely.
- **Opt-out for removable/network storage** → polling/rescan fallback (spec-012 already establishes a debounced polling fallback for hostile filesystems). When live is off or unreliable, rely on scheduled + on-open + on-demand.
- **Scheduled background (opt-in)**: periodic reconcile on a configurable cadence while the app runs.
- **On library-open / on project-open (opt-in)**: an on-attach reconciliation rescan (the reconciler's established pattern).
- **On-demand (always)**: an explicit "Rescan for changes" action.

**Rescan mechanics**: a reconcile pass walks the root (respecting symlink gating), compares recorded `file_record` rows for that root against disk, and transitions `state` observed/classified ↔ `missing`, emitting audit events and progress. Missing detection is fundamentally rescan-driven even when live events exist (deletes/moves are confirmed by the pass), mirroring the artifact reconciler. Live events act as triggers that schedule a scoped rescan rather than mutating records directly.

**Rationale**: Matches the existing "raw roots scanned on demand" design (research R8 in spec-012 lineage) while honoring the user's request for live events where feasible. Avoids continuously watching large external drives.

## R3 — Move-follow identity

**Decision**: Never auto-follow moves. A frame whose file moved under the same root is surfaced as `missing`; the file at the new path is seen as new inventory. A **user-initiated relink** confirms identity by **sha256 content hash computed on demand**, never by file size or mtime.

**Rationale**: The user confirmed same-camera FITS share identical sizes, so size is not an identity key; mtime is unreliable across copy tools. sha256 is reliable. Keeping the hash **lazy/on-demand** (only when the user relinks) honors the constitution's "hashing must be lazy/optional" and the product's local-first custody. `file_record.content_hash` (nullable) is the storage slot; populate it only at relink time.

**Rejected**: automatic size+mtime move-following (unsafe for astro data), eager hashing at ingest (violates lazy-hashing).

## R4 — How the cleanup generator consumes per-frame records

**Decision**: The generator's scan step enumerates present `file_record` rows for the targeted root(s)/session(s) — now populated for all frame types with real sizes — classifies each as raw light/dark/flat vs artifact, applies protection (`resolve_protection`) and confidence, and emits per-frame candidates grouped by session in the generate step. Reclaimable bytes = sum of selected present frames' `size_bytes`. Remove the `:24-30` stale-refusal path.

**Rationale**: The generator already has the two-step scan→generate shape (D11) and protection plumbing; the only missing input was populated per-frame records, which US1 supplies. Missing frames (`state = 'missing'`) are excluded (nothing to reclaim). Cleanup plans reuse the shared apply path and thus PR #408's overlap guard and the `.astro-plan-archive/<planId>/` destination (D24).

## R5 — Calibration-match "source missing" flagging

**Decision**: When a reconcile pass marks a calibration frame `missing`, flag any calibration match/fingerprint that references it as "source missing / unverifiable" (a soft flag), and clear the flag on recovery. Do not invalidate or delete the match.

**Rationale**: Mirrors spec-017's soft-reference "warn if referent missing" and constitution II's confidence-level principle. Calibration matching lives in `crates/calibration/core`; the flag is a derived/annotated state on the match surface, computed from the referenced frame's presence, not a new lifecycle.

## R6 — Symlink / junction gating

**Decision**: Add per-root symlink/junction gating to both the reconcile walker and any live watch. Default: do not follow. Only follow when the root's `detection.follow_symlinks` is enabled.

**Rationale**: Constitution product constraint. The current `watcher.rs` uses `RecursiveMode::Recursive` ungated — this feature must add the gate for raw roots. Implementation detects links during the walk and skips traversal into them unless enabled; live watching should likewise not register link targets unless enabled.

## R7 — Backfill of `size_bytes = 0` records

**Decision**: The reconcile pass corrects any present `file_record` whose `size_bytes` is 0/unknown to the real on-disk size (and can lazily set `mtime`). No separate migration/backfill job; the first reconcile of a root after this feature ships repairs historical rows. New records get the real size at apply.

**Rationale**: Avoids a one-shot migration; reconciliation is already walking the root and stat-ing files. Keeps size capture cheap (stat, no hash).

## Migration note (impl-time only)

The spec/plan add no migration. Implementation will need at most one migration **iff** R1 lands as `library_root` columns (option B) or a dedicated config table (option C); the chosen KV approach (A) needs none. Take the next free migration number at implementation time after checking the base and open PRs (base tops at 0053; #404 holds 0052 in flight). Duplicate migration versions abort fresh-DB migrate on startup.
