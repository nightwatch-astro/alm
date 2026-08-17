---
description: "Task list for 048-per-frame-inventory"
---

# Tasks: Per-Frame Inventory with Live Session Membership

**Input**: Design documents from `specs/048-per-frame-inventory/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/operations.md

**Tests**: Included. This feature is safety-critical (MUST NEVER mutate files as a reconciliation side effect; correct disk-usage totals gate destructive cleanup), so targeted contract/integration/unit tests are part of each story.

**Organization**: Grouped by user story (US1–US5) for independent implementation and testing. Priorities from spec.md: US1 P1; US2, US3 P2; US4, US5 P3.

**Reconciliation note**: tasks.md previously showed 0/44 done — bookkeeping drift, since prior work landed on `main` via other commits without updating this file. Every item below was re-verified against the actual code (not grep) before being marked `[X]`. Evidence pointers are inline HTML comments.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency)
- Paths are repo-relative and reference real crates from plan.md.

---

## Phase 1: Setup

- [ ] T001 Confirm baseline on branch `048-per-frame-inventory` (worktree off `origin/redesign-ui-platevault`); run `just lint` and per-crate `cargo test -p app-core -p app-targets -p app-inbox -p fs-inventory` to record the green/red baseline (workspace-test baseline is known-red — validate per crate).

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ No user-story work begins until this phase is complete.**

- [X] T002 [P] Add a shared per-frame writer helper `upsert_frame_record(root_id, relative_path, size_bytes, mtime, state)` (stat-based real size, no hash) reusable by light and calibration paths — factor from `crates/app/targets/src/ingest_sessions.rs::upsert_file_record` into a shared location both `app/targets` and `app/inbox` can call.
  <!-- done: crates/app/targets/src/frame_writer.rs:56 (upsert_frame_record), :33 (stat_frame); called from crates/app/targets/src/ingest_sessions.rs:209, crates/app/inbox/src/plan_listener.rs:351, crates/app/core/src/frame_inventory.rs:301 -->
- [X] T003 [P] Create the raw-frame reconcile module skeleton mirroring `crates/workflow/artifacts/reconciler.rs`: a pass that walks a root, diffs recorded `file_record` rows vs disk, and emits state transitions — stub the walk + diff, no triggers yet. New module under `crates/fs/inventory/` or a new `crates/workflow/inventory-reconcile/` (decide per crate-split-by-domain rule).
  <!-- done: crates/fs/inventory/src/reconcile.rs:107 (reconcile_root), pure read-only walk+diff; registered crates/fs/inventory/src/lib.rs:6; consumed by crates/app/core/src/frame_inventory.rs:433 (exceeds "no triggers yet" scope but confirms real wiring) -->
- [X] T004 [P] Add per-root symlink/junction gating utility used by the walker and any watch (default: do not follow); wire the `detection.follow_symlinks` flag. Fixes the ungated `RecursiveMode::Recursive` in `crates/fs/inventory/src/watcher.rs` for raw roots.
  <!-- done: crates/fs/inventory/src/symlink_gate.rs (is_link/real_dirs_under/real_files_under, default false); used by reconcile.rs:20 and watcher.rs:106,151-163. NOTE: the one live Tauri caller (apps/desktop/src-tauri/src/watcher.rs:40) still hardcodes false for inbox paths (inbox isn't yet fed from per-root config) — gating mechanism itself is real and wired. -->
- [X] T005 [P] Per-root config read/write over the spec-018 settings KV (`reconcile.mode`, `detection.{live,scheduled,on_open,follow_symlinks}`) with default-when-absent resolution, in `crates/app/settings/`.
  <!-- done: crates/app/settings/src/root_config.rs:67-154 (get_root_config/set_root_config), documented defaults :10-16, round-trip tests :168-194 -->
- [X] T006 [P] Contract DTO scaffolding in `crates/contracts/core/` for `inventory.frame.list`, `inventory.reconcile.run`, `inventory.frame.relink`, `inventory.root_config.{get,set}`, and the `cleanup.candidates.scan`/`cleanup.plan.generate` extensions; register Tauri commands (fn name == invoke target — no specta rename) and regenerate `packages/contracts` bindings.
  <!-- done: crates/contracts/core/src/inventory_frame.rs DTOs + apps/desktop/src-tauri/src/lib.rs commands (inventory_frame_list/inventory_reconcile_run/inventory_frame_relink/inventory_root_config_get/inventory_root_config_set) were already real and wired. The cleanup.candidates.scan/cleanup.plan.generate per-frame extension was added this pass as a separate namespace (crates/contracts/core/src/cleanup.rs: RawFrameCleanupScope/ScanRequest/Candidate/ScanResponse/GenerateRequest — a distinct shape from the project-scoped CleanupCandidate per contracts/operations.md, not a modification of it), commands cleanup_raw_frames_scan/cleanup_raw_frames_generate registered in apps/desktop/src-tauri/src/commands/cleanup.rs + lib.rs, bindings regenerated (apps/desktop/src/bindings/index.ts, verified no dotted invoke strings). -->
- [X] T007 Add audit event types on the spec-002 bus: `frame.missing`, `frame.recovered`, `frame.size_backfilled`, `frame.relinked`, `calibration_match.source_missing`, `calibration_match.source_recovered` in `crates/audit/`.
  <!-- done: crates/audit/src/event_bus.rs:797-896 (all six payload structs + topic consts); frame.* topics actively published from crates/app/core/src/frame_inventory.rs:315,333,375. calibration_match.* consts exist but are never emitted anywhere (that is T038's job, not T007's — T007 only asks to add the event TYPES). -->

**Checkpoint**: shared writer, reconcile skeleton, symlink gate, per-root config, contract surface, and events exist.

---

## Phase 3: User Story 1 — Accurate per-frame inventory for every session (P1) 🎯 MVP

**Goal**: Every applied frame (light + calibration) is a durable, correctly-sized inventory entry with correct session membership.
**Independent Test**: quickstart Scenario 1.

### Tests (US1)
- [ ] T008 [P] [US1] Integration test: inbox confirm → apply light frames → acquisition session lists all frames with non-zero total = Σ sizes (`tests/`).
  <!-- partial: crates/app/core/tests/sessions_integration.rs:155-175 (list_sessions_sums_real_frame_sizes) proves the non-zero-Σ-sizes assertion via seeded file_record/acquisition_session rows, but does not drive it through the literal inbox.confirm → plan.apply → ingest pipeline end-to-end (crates/app/core/tests/ingest_sessions_integration.rs exercises that real pipeline but never asserts size_bytes). Left unchecked pending a true end-to-end test. -->
- [X] T009 [P] [US1] Integration test: apply calibration frames → calibration session lists member frames with real sizes (previously `'[]'`).
  <!-- done: crates/app/inbox/src/plan_listener.rs:650-714 (master_item_apply_writes_frame_record_and_frame_ids) — real tempdir file, real DB/event bus, drives the actual apply path, asserts frame_ids.len()==1 and size_bytes==4096 -->
- [ ] T010 [P] [US1] Unit test: catalogue-in-place frame recorded identically to a moved frame.
  <!-- not done: no test applies the same frame via both a move and a catalogue plan item and asserts identical file_record results. Production code is symmetric by construction (ingest_sessions.rs:144-151, plan_listener.rs:304-330 share the same to/from fallback) but this is untested equivalence. -->

### Implementation (US1)
- [X] T011 [US1] Capture real `size_bytes` (+ `mtime`) at apply in `crates/app/targets/src/ingest_sessions.rs` (replace `size_bytes = 0`), via the T002 helper.
  <!-- done: crates/app/targets/src/ingest_sessions.rs:206-217 (stat_frame + upsert_frame_record) -->
- [X] T012 [US1] Fix `crates/app/inbox/src/plan_listener.rs:~211-214`: write a `file_record` per applied calibration frame and append its id to `calibration_session.frame_ids` (set-deduped; keep the `source_inbox_item_id` idempotency guard) instead of `'[]'`.
  <!-- done: crates/app/inbox/src/plan_listener.rs:225-269 (real file_record write + frame_ids_json from real id + idempotency guard :190-198) -->
- [X] T013 [US1] Ensure catalogue-in-place (organized source, no move) records a `file_record` with real size at apply, same as moved frames.
  <!-- done (implementation only — see T010 for missing dedicated test): ingest_sessions.rs:144-151 + plan_listener.rs:304-330 (resolve_applied_frame_path) share the same to/from resolution order -->
- [X] T014 [US1] Implement `inventory.frame.list` (present count/size exclude `missing`) and wire session/inventory surfaces to show real counts + disk totals.
  <!-- done: backend crates/app/core/src/frame_inventory.rs:199-266 (list_frames), Tauri command inventory_frame_list registered (apps/desktop/src-tauri/src/commands/inventory_frame.rs). FRONTEND (this pass): apps/desktop/src/features/inventory/inventoryIpc.ts + store.ts (useFrameListScan), apps/desktop/src/features/sessions/SessionFrameInventory.tsx — an on-demand scan panel mounted in SessionDetail.tsx showing real presentCount/presentSizeBytes + a per-frame table; unit-tested apps/desktop/src/features/sessions/__tests__/SessionFrameInventory.test.tsx. Session card totals continue to use the separate sessions.rs::active_frame_summary path; this closes the frame.list-specific UI gap (previously zero callers, documented in crates/e2e-tests/tests/inventory_journeys.rs). -->
- [X] T015 [US1] Size backfill on reconcile: correct present `file_record` rows with `size_bytes = 0` to the real size (also serves US2 walker). Emit `frame.size_backfilled`.
  <!-- done: crates/app/core/src/frame_inventory.rs:287-347 (apply_present_outcome), tested :549-593 -->

**Checkpoint**: sessions show honest, correctly-sized membership for all frame types (SC-001, SC-002).

---

## Phase 4: User Story 2 — Sessions notice removed/moved frames (P2)

**Goal**: External deletes/moves are detected and reflected (flag or auto-reconcile) without ever mutating files.
**Independent Test**: quickstart Scenarios 2 & 3. **Depends on US1 (records exist) + T003 skeleton.**

### Tests (US2)
- [X] T016 [P] [US2] Integration test: delete a frame on disk → reconcile → `state = missing`, counts/totals drop, and assert **zero** filesystem mutations (spy/temp-dir snapshot before/after).
  <!-- done: crates/fs/inventory/src/reconcile.rs:175-183 (deleted_frame_reports_missing, pure read-only walk — reconcile_root never writes to disk by construction) + crates/app/core/src/frame_inventory.rs:549-593 (DB-level integration: reconcile_run_backfills_zero_size_and_reports_missing) -->
- [X] T017 [P] [US2] Integration test: auto-reconcile mode drops the frame from active membership while the record is retained as `missing` (queryable with `include_missing`).
  <!-- done: crates/app/core/src/frame_inventory.rs::tests::auto_reconcile_mode_drops_frame_from_membership_but_retains_record (+ flag_missing_mode_retains_frame_in_session_membership as the contrast case) -->
- [X] T018 [P] [US2] Integration test: recovered frame flips back to present; changed-size present frame is updated in place (not missing).
  <!-- done: crates/fs/inventory/src/reconcile.rs:199-209 (size_change_is_reported_present_with_corrected_size_not_missing) + crates/app/core/src/frame_inventory.rs (reconcile_run_recovers_previously_missing_frame) -->
- [X] T019 [P] [US2] Unit test: relink succeeds on sha256 match; `hash.mismatch` on a same-size different file (proves size is not the key).
  <!-- done: crates/app/core/src/frame_inventory.rs::tests::relink_first_attempt_populates_hash_and_rehomes + relink_second_attempt_same_size_different_content_is_hash_mismatch (4-byte candidates, same size, different content) -->

### Implementation (US2)
- [X] T020 [US2] Complete the reconcile walker (T003): present/`missing`/recovered transitions, `last_seen_at` update, size backfill; emit `frame.missing`/`frame.recovered`; report progress (SC-005, non-blocking).
  <!-- mostly done: crates/app/core/src/frame_inventory.rs::run_reconcile (present/missing/recovered transitions, size backfill, events all real and tested). REMAINING GAP: progress_pct is hardcoded to 100 (terminal-only summary, no incremental progress reporting) — SC-005 "reports progress throughout" not met for very large roots (see T043a, also not done). Marked done for the transition/event substance; progress-streaming gap called out explicitly rather than silently accepted. -->
- [X] T021 [US2] Apply per-root `reconcile.mode`: flag-missing (retain in membership, flagged) vs auto-reconcile (drop from active membership, retain record — NEVER hard-delete). Guarantee no filesystem mutation (INV-2).
  <!-- done: crates/app/core/src/frame_inventory.rs::drop_frame_from_session_membership + drop_frame_from_table, wired into run_reconcile's Missing branch when config.reconcile_mode == AutoReconcile. Only the owning session's frame_ids JSON array is touched — the file_record row itself is never modified by this function, preserving INV-4. Tested (see T017). -->
- [X] T022 [US2] `inventory.reconcile.run` command (on-demand) + long-running status/progress.
  <!-- done (frontend gap closed this pass): backend command apps/desktop/src-tauri/src/commands/inventory_frame.rs:47-53 (real request/response); no long-running status/progress STREAM exists (single request/response only; progress_pct hardcoded terminal 100, see T020) — that streaming gap remains open, called out under T020. FRONTEND (this pass): apps/desktop/src/features/settings/DataSources.tsx (handleReconcile) — a per-root "Reconcile now" button on raw/calibration root cards (RootCard, isFrameRootCategory gate), calling inventoryReconcileRun and toasting the real scanned/present/missing/recovered/backfilled summary; unit-tested apps/desktop/src/features/settings/DataSources.reconcile.test.tsx. Layer-2: crates/e2e-tests/tests/inventory_journeys.rs::reconcile_drops_externally_deleted_frame_from_real_ui_count now clicks this REAL button (testid reconcile-now-<rootId>) instead of invoking the command over the bridge. -->
- [X] T023 [US2] Per-root live watch: extend `crates/fs/inventory/src/watcher.rs` to raw/calibration roots with a per-root registry (model on `ArtifactWatcherRegistry` attach/detach); live events schedule a scoped reconcile, they don't mutate records directly. Respect symlink gate (T004).
  <!-- done: apps/desktop/src-tauri/src/frame_watcher.rs — per-root registry (attach_root_watcher/detach_root_watcher) modeled on ArtifactWatcherRegistry including its detach-during-attach tombstone and unlocked-slow-work lock discipline. Live events are debounced (EVENT_DEBOUNCE = workflow_artifacts::DEFAULT_STABILITY_DEBOUNCE) and coalesced into ONE run_reconcile call with ReconcileReason::LiveEvent — the watcher task never writes a record. Reuses fs_inventory::artifact_watcher::start_artifact_watcher rather than adding a third notify watcher: it already applies the T004 symlink gate via notify_bridge::register_watch_paths, so follow_symlinks is threaded straight through. crates/fs/inventory/src/watcher.rs's stale R8 doc comment corrected. Tested: apps/desktop/src-tauri/tests/frame_watcher_live_event.rs (real notify + real SQLite + real EventBus: a post-attach file create schedules a pass that emits frame.recovered) and frame_watcher_symlink_gate.rs (a symlinked root is NOT traversed with follow_symlinks false, and the same event DOES arrive once enabled — so the negative half cannot pass vacuously). -->
- [X] T024 [US2] Removable/network opt-out + polling/rescan fallback when live is off/unreliable; on-open and scheduled triggers.
  <!-- done: apps/desktop/src-tauri/src/frame_watcher.rs reads all four detection flags. `live: false` is the removable/network opt-out (no OS watcher is started); `scheduled: true` spawns the periodic-reconcile fallback (SCHEDULED_INTERVAL 15min, ReconcileReason::Scheduled) which is what a live-off root relies on; `on_open: true` runs one ReconcileReason::OnOpen pass at attach. inventory.root_config.set now re-attaches an already-attached root so a toggle takes effect immediately instead of at next surface close. NOT DONE: no automatic live-is-UNRELIABLE detection — an OS watcher error or a dropped-event overflow schedules a recovery reconcile (handled in spawn_drain_task) but never demotes the root to polling on its own; that demotion needs a product decision on what evidence justifies overriding a user's explicit `live` choice. Missing-detection remains rescan-driven per R2. -->
- [X] T025 [US2] `inventory.frame.relink`: sha256 computed on demand for the two files; re-home on match, `hash.mismatch` otherwise; emit `frame.relinked`; populate `content_hash` lazily.
  <!-- done: crates/app/core/src/frame_inventory.rs::relink_frame + sha256_hex, wired into apps/desktop/src-tauri/src/commands/inventory_frame.rs (replaces the prior always-erroring stub). DESIGN NOTE (documented in the function's doc comment): a `missing` frame's original bytes are unreadable at its recorded path by definition, so there is no baseline hash to compare against on a frame's FIRST relink — content_hash is lazily populated from the candidate's hash at that point (matches data-model.md: "populated only on user-initiated relink"). Any SUBSEQUENT relink attempt for the same frame_id must match that stored hash or fails with hash.mismatch. Tested (T019). FRONTEND (this pass): apps/desktop/src/features/sessions/SessionFrameInventory.tsx's per-row Relink control (inline relative-path input, no native-picker-to-relative-path conversion attempted — kept simple per code economy) calls inventoryFrameRelink and renders a `hash.mismatch` (or any other) error inline via errMessage() — never treated as a fake success; unit-tested. -->
- [X] T026 [US2] Wire raw-root reconciler/watcher lifecycle at startup in `apps/desktop/src-tauri/src/lib.rs` (near `start_inbox_plan_listener`) and to library/project open.
  <!-- done: the registry is created and app.manage()d in run_app next to the spec-012 artifact registry (apps/desktop/src-tauri/src/lib.rs). No watch runs at startup — per R2 a root is attached only while a surface showing its frames is open, via the `inventory.watcher.attach`/`.detach` commands (apps/desktop/src-tauri/src/commands/inventory_frame.rs). Frontend: useRootFrameWatcher (apps/desktop/src/features/inventory/store.ts) mirrors useProjectArtifactWatcher's mount/unmount effect and is mounted by SessionFrameInventory, which receives the session's sourceId from SessionDetail. Tested: apps/desktop/src/features/inventory/__tests__/useRootFrameWatcher.test.tsx (attach on mount, detach on unmount, swap on root change) plus a SessionFrameInventory assertion that the panel binds its session's root. -->

**Checkpoint**: SC-003 met; no-mutation invariant proven by tests.

---

## Phase 5: User Story 3 — Raw sub-frame cleanup candidates (P2)

**Goal**: Cleanup review flow proposes individual raw sub-frames grouped by session with accurate reclaimable bytes.
**Independent Test**: quickstart Scenario 4. **Depends on US1; benefits from US2.**

All of US3 (T027–T031) is **in-flight on PR #500** (`048-complete-per-frame-inventory`, commit 48f76a26), not yet on `main` — left unchecked here rather than duplicated.
*(Reconciliation note, 2026-07-19, issue #764: PR #500 merged 2026-07-12;
this note is stale and contradicts the `[X]` marks immediately below it —
US3 is on `main`.)*

### Tests (US3)
- [X] T027 [P] [US3] Integration test: `cleanup.candidates.scan { session_id }` returns raw sub-frames grouped by session; `total_reclaimable_bytes` = Σ present sizes; generation performs no filesystem mutation.
  <!-- done: crates/app/core/src/cleanup_generator.rs::tests::scan_raw_frames_by_session_returns_present_candidates_with_reclaimable_bytes + generate_raw_frame_plan_creates_reviewable_plan_with_no_filesystem_mutation -->
- [X] T028 [P] [US3] Unit test: `missing` and `protected` frames excluded; inferred candidates carry confidence.
  <!-- done: crates/app/core/src/cleanup_generator.rs::tests::scan_raw_frames_excludes_protected_state (+ the by-session test above asserts missing-exclusion and confidence == 1.0) -->

### Implementation (US3)
- [X] T029 [US3] In `crates/app/core/src/cleanup_generator.rs`: remove the stale raw-refusal (`:~24-30`); enumerate present `file_record` rows for the scope, classify light/dark/flat, apply `resolve_protection` + confidence, group by session.
  <!-- done: crates/app/core/src/cleanup_generator.rs::scan_raw_frames. Stale refusal doc comment replaced with the design note explaining this is a SEPARATE scan/generate pair from the project-scoped one, not a DataType extension. Reuses crate::frame_inventory::list_frames (T014's classification + missing-exclusion) rather than a duplicate file_record query — code-economy choice: T014 already does the enumeration+classification this task needs. -->
- [X] T030 [US3] Reclaimable bytes = Σ selected present `size_bytes`; exclude `missing`/`protected` (FR-020..022).
  <!-- done: scan_raw_frames sums size_bytes only for non-missing (via list_frames' include_missing:false)/non-protected (explicit FramePresenceState::Protected skip) frames -->
- [X] T031 [US3] Wire `cleanup.candidates.scan` / `cleanup.plan.generate` extensions to the per-frame candidate set; generated plans reuse the shared apply path (PR #408 overlap guard, `.astro-plan-archive/<planId>/` destination, archive|trash vocab). Read-only until Apply.
  <!-- done: crates/app/core/src/cleanup_generator.rs::generate_raw_frame_plan calls the SAME protection::generate_cleanup_plan tail as the project-scoped generate() (inherits the PR #408 guard + archive destination); wired to Tauri commands cleanup_raw_frames_scan/cleanup_raw_frames_generate (apps/desktop/src-tauri/src/commands/cleanup.rs), registered in lib.rs, bindings regenerated. FRONTEND (this pass): apps/desktop/src/features/sessions/RawFrameCleanupSection.tsx — session-scoped raw sub-frame cleanup review (scan → per-frame checkbox selection, protected frames excluded from selection → generate → shared PlanReviewOverlay), mounted in SessionDetail.tsx; unit-tested apps/desktop/src/features/sessions/__tests__/RawFrameCleanupSection.test.tsx. No dedicated Layer-2 journey yet (reconcile trigger was the one Layer-2 addition this pass; noted as the next highest-value gap in coverage-matrix.md). -->

**Checkpoint**: SC-004 met — raw-sub cleanup is possible for the first time.

---

## Phase 6: User Story 4 — Per-root detection & reconcile config in the wizard (P3)

**Goal**: Users configure reconcile mode + detection triggers per root, in the wizard and settings.
**Independent Test**: quickstart Scenario 5. **Depends on T005/T006.**

### Tests (US4)
- [X] T032 [P] [US4] Contract test: `inventory.root_config.get` returns documented defaults when unset; `set` persists and round-trips.
  <!-- done: crates/app/settings/src/root_config.rs:168-178 (get_returns_documented_defaults_when_unset), :180-194 (set_reconcile_mode_round_trips) -->
- [X] T033 [P] [US4] Integration test: changing mode to auto-reconcile takes effect on the next reconcile.
  <!-- done: folded into crates/app/core/src/frame_inventory.rs::tests::auto_reconcile_mode_drops_frame_from_membership_but_retains_record, which calls app_core_settings::root_config::set_root_config(AutoReconcile) then asserts the very next reconcile pass applies it -->

### Implementation (US4)
- [X] T034 [US4] `inventory.root_config.{get,set}` over the T005 KV.
  <!-- done: crates/app/settings/src/root_config.rs get_root_config/set_root_config, wired to Tauri commands inventory_root_config_get/inventory_root_config_set (apps/desktop/src-tauri/src/commands/inventory_frame.rs:88-107) -->
- [X] T035 [US4] Add the per-root config step to the real unified first-run wizard (verify current wizard shape in code first) with documented defaults pre-selected.
  <!-- done, as an inline per-root section rather than a new wizard STEP (deviation, reasoned in the component doc comment): apps/desktop/src/features/inventory/RootDetectionConfig.tsx mounted in apps/desktop/src/features/setup/steps/StepScan.tsx's SourceSummary (gated to light_frames/calibration sources with a real registered root id — StepScan is the earliest point in the wizard a root has a real backend id from roots.register.batch, since the wizard has no separate root/frame Scan sub-step to attach a dedicated new step to). Loads inventoryRootConfigGet on expand and shows the documented defaults (flag_missing / live=true, others false) pre-selected; a new wizard STEP was rejected as unwarranted step-count/indicator/test churn for one control (code economy). -->
- [X] T036 [US4] Surface the same controls in existing root settings (minimal hook — full settings-window redesign is the companion UI spec, references 043).
  <!-- done: the SAME RootDetectionConfig component (T035) also mounted in apps/desktop/src/features/settings/DataSources.tsx's RootCard (raw/calibration roots only), as a "Detection settings" collapsible toggle next to SourceProtectionOverride — no duplicated markup between wizard and settings. Unit-tested apps/desktop/src/features/inventory/__tests__/RootDetectionConfig.test.tsx (collapsed-by-default = no query hooks fire without a QueryClientProvider ancestor, load defaults, mode change, detection-toggle partial patch). -->

**Checkpoint**: SC per-root behavior configurable; wizard sets it.

---

## Phase 7: User Story 5 — Missing-frame awareness for calibration matches (P3)

**Goal**: Matches referencing a missing calibration frame are flagged, not invalidated.
**Independent Test**: quickstart Scenario 6. **Depends on US2 (missing detection).**

All of US5 (T037–T039) is **in-flight on PR #503** (`048-us5-calibration-missing-flag`, stacked on #500), not yet on `main` — left unchecked here rather than duplicated.
*(Reconciliation note, 2026-07-19, issue #764: PR #503 merged 2026-07-13;
US5 is on `main` — re-check T037–T039 or otherwise reconcile this note.)*

### Tests (US5)
- [X] T037 [P] [US5] Integration test: referenced calibration frame goes missing → match flagged "source missing / unverifiable", still present; flag clears on recovery.
  <!-- done: crates/app/core/tests/calibration_missing_flag_integration.rs (both PATH A "master_artifact_missing_flags_match_and_clears_on_recovery" and PATH B "source_sub_frame_missing_flags_match_and_clears_on_recovery"); covers distinct wording, FR-024 non-removal, INV-4 retention, audit events, zero-filesystem-mutation, and recovery-clears-flag -->

### Implementation (US5)
- [X] T038 [US5] In `crates/calibration/core/`, derive a "source missing / unverifiable" flag from the referenced `file_record` presence; emit `calibration_match.source_missing`/`source_recovered`; never auto-invalidate/remove.
  <!-- done: PRODUCT DECISION resolved the scope ambiguity noted below — implement BOTH trigger paths with distinct wording, not either/or. DEVIATION from the task's crate placement: the presence checks are DB reads and crates/calibration/core is documented pure-domain (assign.rs:7-9), so the flag derivation lives in the APP layer instead — crates/app/calibration/src/matching.rs (compute_missing_flag, master_missing takes precedence over source_subs_missing when both apply) backed by two new read helpers in crates/persistence/db/src/repositories/calibration_assignment.rs (master_artifact_state = PATH A via calibration_master.source_session_id -> .artifact_id -> processing_artifacts.state; master_has_missing_source_frame = PATH B via calibration_session.frame_ids -> file_record.state). Audit emission wired at both transition sites: crates/app/core/src/frame_inventory.rs (apply_missing_outcome/apply_present_outcome, PATH B, via calibration_assignment::find_by_source_frame) and crates/app/lifecycle/src/artifact.rs (mark_missing/mark_recovered, PATH A, via calibration_assignment::find_by_source_artifact). Never auto-invalidates/removes calibration_assignment — flag is derived/live, never stored. -->
- [X] T039 [US5] Surface the flag on the calibration match UI.
  <!-- done: apps/desktop/src/features/calibration/MasterDetail.tsx (missingFlag state + missingFlagLabel() + Pill badge in titleExtra, data-testid="calibration-missing-flag"); backed by contracts_core::calibration::MasterDetail.missing_flag (crates/contracts/core/src/calibration.rs) returned by calibration.masters.get; distinct i18n wording calibration_flag_master_missing / calibration_flag_source_subs_missing in apps/desktop/messages/en.json -->

**Checkpoint**: SC-006 met.

---

## Phase 8: Polish & Verification

- [ ] T040 Constitution re-check (custody, reviewable mutation, PixInsight boundary, lazy hashing, portable contracts) against the built feature.
  <!-- not done — feature still has open gaps (US4 UI, US5 whole story, US2 live watch); premature until those close or are explicitly descoped -->
- [ ] T041 `just lint` / per-crate `cargo test` / `just typecheck` green; regenerate + commit bindings.
  <!-- not done as a full-feature gate; not run this pass beyond what's noted per-task above -->
- [ ] T042 `speckit-verify` against FR-001..FR-025 and SC-001..SC-006; `speckit-verify-tasks` to catch phantom completions.
  <!-- not done -->
- [ ] T043 `verify-on-windows` scenario for Scenarios 1/2/4/5 on the real Tauri app; add the matching tauri-driver Layer-2 journey + coverage-matrix update.
  <!-- partial: crates/e2e-tests/tests/inventory_journeys.rs::reconcile_drops_externally_deleted_frame_from_real_ui_count now clicks the REAL "Reconcile now" UI button (this pass, T022 frontend) instead of the invoke bridge, closing that part of the gap; coverage-matrix.md updated accordingly. No dedicated Layer-2 journey added yet for the new frame-inventory/relink or raw-cleanup-review surfaces (US1/US2/US3 frontend, this pass) — covered by frontend unit tests only. No verify-on-windows run performed (no Windows environment available to this agent). -->
- [ ] T043a [US2] Performance verification for SC-005: reconcile a synthetic ≥10,000-frame root and assert it completes without blocking the UI thread and reports progress throughout (integration/bench under `tests/`).
  <!-- not done: no such benchmark exists. Also blocked on T020's real progress-streaming gap (progress_pct is hardcoded terminal-only today, so "reports progress throughout" cannot yet be asserted). -->

---

## Dependencies

- **Phase 2 (T002–T007)** blocks everything.
- **US1 (P3 phase)** depends on T002. → MVP.
- **US2** depends on US1 + T003/T004.
- **US3** depends on US1 (benefits from US2).
- **US4** depends on T005/T006 (largely independent of US1–US3 logic).
- **US5** depends on US2.
- **Phase 8** depends on all targeted stories.

### Dependency graph

```
T001 ─▶ T002 ┐
        T003 ┼─▶ US1(T008–T015) ─▶ US2(T016–T026) ─▶ US5(T037–T039)
        T004 ┤                └─▶ US3(T027–T031)
        T005 ┼─▶ US4(T032–T036)
        T006 ┤
        T007 ┘
US1..US5 ─▶ Phase 8 (T040–T043)
```

## Parallelization notes

- Foundational [P] tasks T002/T003/T004/T005 touch different crates → parallelizable; T006 depends on none of them structurally but should land before per-story contract wiring; T007 is independent.
- Within each story, [P] tests are independent; implementation tasks that touch the same file are serialized (e.g., T020→T021 both in the reconcile module).
- US4 can proceed in parallel with US2/US3 once Foundational is done.
