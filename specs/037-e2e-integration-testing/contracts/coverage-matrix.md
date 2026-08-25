# Coverage Matrix: Feature Area → Test Layer

**Feature**: 037-e2e-integration-testing

This feature exposes **no new product contracts** — it consumes the existing
language-neutral command contracts unchanged. This file is the auditable coverage
mapping required by FR-019 / SC-001. Final per-test names are filled in by
`/speckit.tasks`; this fixes the required coverage.

Legend: **L1** = real-backend integration test required; **L2** = appears in a
Layer-2 smoke journey; **—** = covered implicitly via screen-load smoke.

> **Naming note.** This table's `L1`/`L2` columns predate the CI tier names and
> do NOT line up with them. Here `L1` means "real-backend integration" and `L2`
> means "smoke journey". The CI tiers are: **L1** = isolated unit (no real I/O),
> **L2** = integration against real SQLite/filesystem, **L3** = real-UI journey.
> So this table's `L1` column corresponds to the CI tiers' **L2**, and its `L2`
> column to the CI tiers' **L3**. The column headers are left unchanged because
> every row references them; use this note when reading across.

| # | Feature area | L1 | L2 | Notes |
|---|---|:--:|:--:|---|
| 1 | First-run source setup | ✅ | ✅ | setup wizard → root persisted |
| 2 | Native filesystem controls | ✅ | — | path validation/side effects via L1 |
| 3 | Inbox mixed-folder split | ✅ | ✅ | classify + split; **re-shaped by spec 058** (source-group row before classification, N item rows after, no aggregate row) — see the dated section below |
| 4 | Inventory / data lifecycle state | ✅ | ✅ | ledger + transitions |
| 5 | Calibration matching & masters | ✅ | ✅ | suggest + assign |
| 6 | Sessions | ✅ | ✅ | list/merge/split/transition |
| 7 | Projects: create/onboard/edit | ✅ | ✅ | CRUD round-trip |
| 8 | Project lifecycle model | ✅ | ✅ | blocked/ready transitions |
| 9 | Project manifests & notes | ✅ | ✅ | manifest + note persistence |
| 10 | Processing tool launch | ✅ | smoke | wiring only; **no real launch** |
| 11 | Processing artifact observation | ✅ | ✅ | artifact detection |
| 12 | Target lookup from FITS OBJECT | ✅ | ✅ | OBJECT → canonical |
| 13 | Target identity, history, notes | ✅ | ✅ | identity + notes |
| 14 | SIMBAD target resolution | ✅ | ✅ | **HTTP-boundary mocked** (wiremock) |
| 15 | Token pattern builder | ✅ | ✅ | parse/resolve tokens |
| 16 | Source protection defaults | ✅ | — | protection asserted via L1 + plans |
| 17 | Cleanup & archive review plans | ✅ | ✅ | plan generation/review; protected-item gate covered separately by `cleanup_protection_gate` — `cleanup_plan_review` sets protection `unprotected`, so it does not exercise a refusal |
| 18 | Filesystem plan application | ✅ | ✅ | **mutation + audit record assert** |
| 19 | Settings / configuration model | ✅ | ✅ | persist + reload |
| 20 | Bottom log viewer | ✅ | ✅ | log stream render |
| 21 | Router & URL state | n/a | ✅ | **all top-level screens load** (FR-007) |
| 22 | Audit event model (cross-cutting) | ✅ | via #18 | bus + stale propagation; **spec 030 Q15/#647 (T122–T125, 2026-07-14)**: settings/protection/equipment/source+root mutations now write durable `audit_log_entry` rows (previously bus-only for protection/source/root, no audit at all for equipment) — see the Q15 row below |
| 23 | Framing clustering + Inbox-confirm attribution (spec 008 Q27) | ✅ | ❌ | clustering/list/merge/split/reassign + attribution ranking/apply-path all real IPC, **zero frontend consumer** — see the dated section below |
| 24 | Onboarding: orientation walk + real-work auto-ticks (spec 056) | ✅ | ✅ | walk + live auto-tick real-UI journey; behavioral contract **J18** — see the dated section below |

**Required round-trip proof (FR-008)**: areas #1, #7, #12/#14 each round-trip a
UI value through the real backend.

**Required mutation+audit proof (FR-009)**: area #18 (filesystem plan
application).

## Layer-1 implementation status (T009–T020) — 2026-06-19

All backend feature areas now have ≥1 passing real-backend integration test
(real SQLite + migrations, no mocked backend). Full workspace: 76 suites ok, 0
failed, **0 ignored** (no faked/skipped passes).

| Areas | Test file | Tests |
|---|---|---|
| #7 (+ #18/#22 audit) | `crates/app/core/tests/us1_coverage_smoke.rs` | 2 |
| #1/#2/#16 | `crates/app/core/tests/first_run_integration.rs` | 4 |
| #3/#4 | `crates/app/core/tests/inbox_lifecycle_integration.rs` | 4 |
| #5 | `crates/app/core/tests/calibration_integration.rs` | ✓ |
| #6 | `crates/app/core/tests/sessions_integration.rs` | ✓ |
| #7/#8/#9 | `crates/app/core/tests/projects_integration.rs` | 7 |
| #10/#11 | `crates/app/core/tests/tools_artifacts_integration.rs` | 5 |
| #12/#13 | `crates/app/core/tests/targets_integration.rs` | 7 |
| #14 | `crates/app/core/tests/simbad_resolution_integration.rs` | ✓ (offline, FakeResolver) |
| #15 | `crates/patterns/tests/pattern_integration.rs` | 4 |
| #17/#18/#22 | `crates/app/core/tests/plan_apply_audit_integration.rs` | ✓ (mutation+audit) |
| #19/#20 | `crates/app/core/tests/settings_logs_integration.rs` | ✓ |
| #21 | — (Layer-2 only, by design) | see US3 |
| #22 Q15 durable-audit sweep (spec 030 T122–T127, 2026-07-14) | in-crate `#[cfg(test)]` mods: `crates/app/settings/src/lib.rs`, `crates/app/core/src/protection.rs`, `crates/app/core/src/first_run.rs`, `crates/app/calibration/src/equipment.rs` | applied+refused/failed rows resolve to real `audit_log_entry` (SC-009); ≥1 refused/failed test per consumer (settings, protection, equipment, source/root) |

Shared harness: `crates/app/core/tests/support/mod.rs` (T005).
**Implementation note**: research D2's `wiremock` boundary stub was superseded by
the repo's existing `targeting` `FakeResolver` (offline) for #14 and `FakeSpawner`
for #10 — fewer deps, matches repo convention. No `wiremock` dependency was added.

**Explicit exclusions (not implemented-feature backend areas)**: Catalog index
licensing (014), Developer contract diagnostics (021, dev-only), Design/UI specs
(022, 026–032) — covered implicitly by #21 screen-load smoke; remediation specs
033/036 fold into the areas above. Any area later found implemented but unmapped
MUST be added here or reported as a gap (FR-002).

## Spec 041 iteration — destination model (US8/US9) — 2026-06-21

Per-type destination patterns, destination-root selection, full absolute-path
preview, and the missing-path-attribute gate (FR-025–FR-033). Folds into areas
#3/#4 (inbox lifecycle).

| Scenario (quickstart Iteration 2026-06-21) | Layer-1 test |
|---|---|
| Per-type pattern resolution + calibration structure (no target) | `crates/patterns/src/per_type.rs`, `crates/patterns/src/resolver.rs` (`pattern_str_*`), `confirm.rs::calibration_destinations_omit_target` |
| Root resolution: in-place / inbox-target / single-auto / multi-require / none | `confirm.rs::{non_inbox_source_moves_in_place, inbox_single_candidate_auto_selects, inbox_multi_candidate_requires_selection, inbox_no_candidate_blocks}` |
| Missing path-attribute gate (US9) | `confirm.rs::missing_path_attribute_blocks_with_report` |
| Settings per-type pattern round-trip + validation (FR-026b) | `app_core settings.rs::update_patterns_by_type_*`, `persistence_db settings.rs` |
| Frontend: root picker, absolute-path preview, missing-attr annotations | `apps/desktop/src/features/inbox/__tests__/{PlanPanel,InboxDetail}.test.tsx`; settings: `NamingStructure` vitest |

**Windows real-app E2E (T060)**: the live tauri-MCP run of the quickstart
destination-model scenarios (calibration structure, inbox root selection,
multi-root prompt vs single-root auto, missing-date gate) is the recommended
post-merge verification loop (see the `tauri-mcp-windows-verify-mechanics`
memory); Layer-1 + vitest coverage above gates the merge.

## Layer-2 real-UI journey status — 2026-07-04 (WP-C, D21/D22)

Thirteen journey files in `crates/e2e-tests/tests/` hold 29 real-UI tests, all
29 `#[ignore]`d by design. `e2e.yml` runs its shards with `--run-ignored all`,
so the attribute is the routing mechanism: it keeps these tests out of a plain
`cargo test` run, and the required "Real-UI journeys (L3) — ubuntu-latest" and
"— windows-latest" contexts execute every one of them.

Harness: thirtyfour + `tauri-plugin-webdriver`/`tauri-webdriver`, the
`window.__PV_E2E__` invoke bridge (D21 renamed the harness's stale
`__APP_E2E__` references to match — confirmed landed). CI (`e2e.yml`, 3-OS
matrix) is the first real run point (no webview in the WSL dev sandbox);
local gates (compile, clippy, fmt) are clean.

| Journey | File | Areas | Real commands exercised |
|---|---|---|---|
| `first_run_resolve_create_project` | `journeys.rs` | #1, #7, #12/#14 | `target.resolve` (offline bundled-seed cache hit), `projects.create`, `projects.list` |
| `plan_review_apply_with_audit` | `journeys.rs` | #3, #16, #17, #18 | `roots.register`, `sources.set_organization_state`, `inbox.scan.folder`, `inbox.classify`, `inbox.confirm`, `inbox.plan.apply`, `plans.apply.status` |
| `ingestion_sessions_search` | `journeys.rs` | #3, #4, #6, #5, #12/#14 | inbox pipeline (as above) + `sessions.list` (event-driven session grouping/resolution), `calibration.match.suggest`, `search.global` |
| `lifecycle_integrity` | `journeys.rs` | #7/#8 | `projects.create`, `lifecycle.transition.apply`, `lifecycle.ledger.list` |
| `cleanup_plan_review` (NEW, D22; apply extended 2026-07-05) | `journeys.rs` | #10/#11, #17 | `projects.create`, `source.protection.set`, `artifact.watcher.attach`, `artifact.list`, `cleanup.policy.update`, `cleanup.scan`, `cleanup.plan.generate`, `plans.approve`, `plans.apply.direct`, `plans.apply.status` |
| `archive_lifecycle_apply_trash_permanent_delete` (NEW, 2026-07-05) | `archive_journeys.rs` | Journey 7 | `projects.create`, `lifecycle.transition.apply` (x3), `source.protection.set`, `artifact.watcher.attach`, `artifact.list`, `archive.plan.generate`, `plans.apply.direct`, `plans.apply.status`, `archive.list`, `archive.send_to_trash`, `settings.update`, `archive.permanently_delete` |
| `cleanup_protection_gate` (NEW, 2026-07-20) | `cleanup_protection_gate_journey.rs` | #10/#11, #17 | `projects.create`, `settings.update` (global `defaultProtection`), `artifact.watcher.attach`, `artifact.list`, `cleanup.policy.update`, `cleanup.scan`, `cleanup.plan.generate`, `plans.approve`, `plans.apply.direct`, `plans.apply.status`, `plans.get` |
| `plan_review_destructive_confirm_gate_and_apply` + `plan_review_empty_archive_plan_refuses_apply` (NEW, 2026-07-21, PR #1385) | `plan_review_overlay_journeys.rs` | #17, #18 | `roots.register`, `projects.create`, `artifact.watcher.attach`, `artifact.list`, `cleanup.policy.update`, `lifecycle.transition.apply` (x3), then the real `PlanReviewOverlay` DOM: `cleanup.scan` / `cleanup.plan.generate` / `plans.approve` / `plans.apply_real` via the Cleanup section buttons, and `archive.plan.generate` via the plan-gated `archived` transition |
| `all_top_level_screens_load` | `smoke.rs` | #21 | real routes + the shipped `AppErrorBoundary` fallback presence check |

**Spec 052 (SIMBAD cache / dual-lookup / cone-search) sync — 2026-07-14.**
Real-network paths (`target.resolve_explicit` NED/VizieR fallback,
`target.cone_search.suggest` live SIMBAD cone search) are validated manually via
`docs/development/windows-validation/052-simbad-live-validation.md` — CI must
not depend on CDS/NED availability, so these stay manual-only. Offline paths
are Layer-1 covered (`crates/app/core/tests/resolution_e2e.rs`,
`simbad_resolution_integration.rs`); the P2UX affordance is vitest-covered
(`TargetSearch.test.tsx`). Planned journey (not yet written):
`resolver_cache_clear` in `settings_journeys.rs` asserting `target.cache.clear`
succeeds offline with a seed re-warm count > 0.

**Corrections to prior scaffold claims** (the original stub doc comments were
partly aspirational, not verified against real code — corrected here per this
task's brief: "keep it accurate to REAL current behavior, don't trust spec
prose"):

- `sessions.transition` is NOT exercised by any journey — spec 041 FR-051
  (T076) deliberately deleted the command; the original `lifecycle_integrity`
  stub's mention of it is struck (D22).
- `audit.list`/`audit.export` were fixture stubs when the journeys were
  authored; **PR #388 (merged) wired them to the real `audit_log_entry`
  table** (`apps/desktop/src-tauri/src/commands/audit.rs` now reads via
  `persistence_db::repositories::audit`), and PR #401 (in flight) adds
  entity-filtered audit reads. No journey asserts through `audit.list`;
  durable-record proofs use `plans.apply.status` (reads the real
  `plan_apply_events` table) and `lifecycle.ledger.list` — the read paths
  closest to the mutations being proved, kept as the primary assertion
  surfaces by choice (more robust than the general audit feed). The original
  stubs' references to `events.recent` were aspirational — that command does
  not exist.
- **RESOLVED 2026-07-05: `cleanup_plan_review`'s apply gap.** The blocker was
  a missing channel-free apply command for archive/cleanup plans (unlike
  `inbox.plan.apply` for inbox plans) — `plans.apply_real` takes a
  `tauri::ipc::Channel` progress argument this WebDriver harness declines to
  construct (buildable from a test script, but only by reaching into
  Tauri internals — see #1234).
  `plans.apply.direct` (a.k.a. `plans_apply_direct`) now exists: same executor
  (`app_core::plan_apply::apply_plan` with `event_sink: None`), same
  caller-supplied approval token, and same durable audit trail as
  `plans.apply_real`, no `Channel` required. `cleanup_plan_review` now drives a real apply past `plans.approve`
  and asserts the real filesystem mutation + audit record. **Correction to
  the original claim**: the Cleanup/Archive UI already had a real Apply
  affordance before this — `OutputsCleanupSections.tsx`'s `CleanupSection`
  calls `cleanupPlanGenerate` (via `useGenerateCleanupPlan`) and hands off to
  the shared `PlanReviewOverlay` (protection gate → `plans.approve` →
  `plans.apply_real` with live progress), and `ProjectDetail.tsx` wires the
  same overlay for `archive.plan.generate` — landed by PR #413 (2026-07-04)
  and PR #438, before this audit's original claim was written. No new UI
  button was added: the existing Channel-based `PlanReviewOverlay` path is
  strictly better for a live UI (streamed per-item progress) than a
  fire-and-poll channel-free call would be, so `plans.apply.direct`'s
  consumers are the Layer-2 harness and any future non-UI caller, not this
  overlay. Existing vitest coverage (`PlanReviewOverlay.test.tsx`: "approve &
  apply drives plans.approve → apply with the token and reports completion")
  already covers the button's happy path.
- **Second, previously-undiscovered bug found and fixed while landing the
  above**: `protection::generate_plan` (the shared persistence tail for both
  `archive_generator::generate` and `cleanup_generator::generate`) always
  stored `archive_path: None` for every plan item, regardless of action. The
  spec-025 executor's fallback for `archive`-action items with no
  `archive_path` uses `to_relative_path` verbatim — and both generators left
  that fallback unusable (`archive_generator` sets it equal to the source
  path, so source == destination and every apply failed with
  `conflict.destination_exists`; `cleanup_generator` leaves it an empty
  string). **Every real archive/cleanup apply failed 100% of the time before
  this fix, with zero prior test coverage to catch it** — exactly the gap
  this journey work was meant to close. Fixed in
  `crates/app/core/src/protection.rs::compute_archive_destination` (destination
  convention: `<parent-dir-of-source>/.astro-plan-archive/<planId>/<itemId>-<fileName>`);
  regression test:
  `crates/app/core/src/archive_generator.rs::generate_computes_distinct_archive_destination_per_item`.
  As a side effect, `archive.send_to_trash`/`archive.permanently_delete`
  (which count `archive_path.is_some()` items) also went from always
  reporting `archive.empty` to reporting the real count.

## Spec 035 iteration — US4 ingest → session → target — 2026-06-21

Applied light frames create `acquisition_session` records grouped by capture
identity and link a resolved `canonical_target` (FR-016). Folds into areas
#3/#4 (inbox lifecycle) + the Sessions read path. Closes GitHub issue #307
(empty Sessions page; targets never linked).

| Scenario (spec 035 US4 acceptance) | Layer-1 test |
|---|---|
| M31 cache-hit grouping: two alias-spelled light frames (`M 31`, `NGC 224`) → ONE session, `frame_ids` length 2, `canonical_target_id` = seeded M31, `list_sessions` frame_count 2 + target name | `crates/app/core/tests/ingest_sessions_integration.rs::two_m31_frames_group_into_one_linked_session` (T045) |
| Unknown OBJECT → session created, `canonical_target_id` NULL, `ingest_resolution` pending, never fabricated; `resolve_pending` (FakeResolver) + back-fill → linked | `crates/app/core/tests/ingest_sessions_integration.rs::unknown_object_session_backfills_after_resolve` (T046) |
| Per-frame ingest unit coverage (light detection, binning, DATE-OBS UTC fallback, session_key) | `crates/app/targets/src/ingest_sessions.rs` unit tests |
| Inline cache-hit / miss-enqueue / drain / offline-pending | `crates/app/targets/src/ingest_resolution.rs` unit tests |

**Background drain (T043)**: the `resolve_pending` + `backfill_session_targets`
interval task in `apps/desktop/src-tauri/src/lib.rs::run_app` is exercised
function-by-function at Layer 1 (T046 calls both directly); the live interval
loop is validated in the Windows real-app E2E loop.

## Unified-main audit — 2026-07-05 (Layer-2 + manual-Windows + mock-layer reconciliation)

Read-heavy audit against `origin/main` post spec-043-redesign convergence
(PR #349 merged, plus #430/#435/#436/#439/#442/#443 landed after it). No
product code changed. Confirms the Layer-2 harness described above survived
the convergence intact and adds the missing cross-reference to the mock
(Playwright) layer and to the 10 user journeys, including several
post-convergence features (040, 043, 044, 046, 047, 048, 049) that predate
this file's existing rows.

**Three test layers now exist; this file previously tracked only two.**
Layer-2 (`crates/e2e-tests/`, this file, above) and Layer-1 (`cargo test
--workspace`, above) are unchanged in shape. A third, **mock-Playwright**
layer (`apps/desktop/tests/e2e/*.spec.ts`, `VITE_USE_MOCKS=true`, run via
`pnpm --filter @astro-plan/desktop test:e2e`) exists and is now tracked here
for the first time — see
`docs/development/e2e-mock-coverage-audit-2026-07-05.md` (branch
`research/e2e-mock-coverage-audit`) for the full spec-by-spec breakdown. A
fourth surface, **manual-Windows** (`docs/development/windows-journeys/`,
this audit's new artifact, plus the pre-existing
`docs/development/verify-on-windows-journeys.md`), is the catch-all for
everything neither automated layer reaches.

### Per-journey coverage (10 user journeys, `docs/product/user-journeys.md`)

| Journey | Layer-1 | Layer-2 | Mock-Playwright | Manual-Windows doc |
|---|:--:|:--:|:--:|---|
| 1 First-run → data sources | ✅ | 🟡 wizard redirect + resolve + create only | 🟡 legacy-state + index-redirect regressions + **Observing Site step (`setup_wizard_site_step.spec.ts`, NEW 2026-07-09)**: optional-copy render, blank-skip advances to Confirm, out-of-range-latitude inline validation, field retention across Back/Continue. Data Sources Rescan/Disable/Enable/Delete have vitest coverage (`DataSources.rescan.test.tsx`, `DataSources.disable-delete.test.tsx` — see correction below); full 6-step happy path and Remap/Reveal remain uncovered at every layer | `windows-journeys/journey-01-first-run-setup.md` |
| 2 Ingest → reclassify → confirm (move) | ✅ | ✅ real-UI (`inbox_ui_journeys.rs`): mixed-folder split, unclassified-frame-type gate + bulk reclassify, missing-path-attribute gate, Confirm-doesn't-move + Apply-moves-to-shown-path, unsplit-folder Type-badge reads "unclassified" not "classified" (`inbox_ui_unsplit_unclassified_folder_badge_is_not_classified`, #711 Instance A, NEW 2026-07-19 — **written, not yet executed**). Root-picker prompt (2+ roots) and stale-plan refusal remain unautomated (follow-up) | ✅ `inbox_ingest_confirm.spec.ts` (batch 1, PR #448, 2026-07-05): mixed-folder split, needs-review gate + bulk reclassify, single-type confirm→plan toast, plan-approval overlay review→apply/cancel | `windows-journeys/journey-02-inbox-ingest-move.md` |
| 3 Ingest → confirm (catalogue-in-place) | ✅ | ✅ real-UI (`inbox_ui_journeys.rs::inbox_ui_catalogue_in_place_zero_moves_byte_identical`): organized root → 0-move catalogue plan, no root picker, no destination-absolute cell, byte-identical apply | ✅ `inbox_ingest_confirm.spec.ts` (batch 1, PR #448): catalogue-in-place plan distinguishable from a move plan in the review overlay | `windows-journeys/journey-03-inbox-catalogue-in-place.md` |
| 4 Sessions review (derived) | ✅ | 🟡 real-UI (`sessions_journeys.rs`): nothing before apply, real session row appears automatically, no review-lifecycle controls anywhere, no-op rescan never duplicates. Notes-edit invariant (Test 4) found untestable — see finding below | 🟡 rows/detail render only (`lifecycle_detail.spec.ts`, pre-existing) | `windows-journeys/journey-04-sessions-review.md` |
| 5 Project lifecycle | ✅ | 🟡 real-UI (`lifecycle_ui_journeys.rs`): create-wizard makes real `lights/`/`darks/` folders under the registered project library root (PR #414 regression guard) + blocks a duplicate name with a real inline field error. Attach/remove-source UX, manifests/notes, tool launch, artifact watcher still IPC-only | ✅ `project_lifecycle_create.spec.ts` (batch 3, PR #453, 2026-07-05): creation-wizard happy path, duplicate-name inline block, empty-name Create-disabled gate; `project_lifecycle_surfaces.spec.ts` + `project_lifecycle_transitions_full.spec.ts` (landed same window): notes autosave, manifests/outputs/tool-launch affordance, attach/remove-source guards, full state-machine transitions. `lifecycle_transitions.spec.ts` (pre-existing): transition button + pill-refresh (`test.skip`, real-backend only) | `windows-journeys/journey-05-project-lifecycle.md` |
| 6 Cleanup scan→review→apply | ✅ | ✅ `cleanup_plan_review` now applies past `approved` via `plans.apply.direct` + asserts the real FS move + audit (2026-07-05) | ✅ `cleanup_review.spec.ts` (batch 2, PR #447, 2026-07-05): scan→review candidates with confidence+protection→generate plan→protection gate→approve & apply | `windows-journeys/journey-06-cleanup-scan-apply.md` |
| 7 Archive → delete | ✅ (backend only) | ✅ `archive_lifecycle_apply_trash_permanent_delete` (`archive_journeys.rs`, NEW 2026-07-05): real apply + `archive.list` + `archive.send_to_trash`/`archive.permanently_delete` metadata + `blockPermanentDelete` gate | ✅ `archive_lifecycle.spec.ts` (batch 2, PR #447, 2026-07-05): archive page listing + canonical actions, send-to-trash, typed-`DELETE` permanent-delete gate | `windows-journeys/journey-07-archive-delete.md` |
| 8 Calibration masters → matching | ✅ | 🟡 real-UI (`calibration_ui_journeys.rs`): masters ingest as individual items + kind-conditional detail (Tests 1/2). Matching/assign UI (Tests 3-5) found UNREACHABLE from the real app during this pass — see finding below, not automatable until fixed | ✅ `calibration_masters_matching.spec.ts` (batch 4, PR #452, 2026-07-05): masters as individual items with kind-conditional Filter/Exposure columns, aging pill + fingerprint detail, per-project match-status confidence, configurable matching tolerances | `windows-journeys/journey-08-calibration-masters-matching.md` |
| 9 Targets & planning | ✅ (backend only) | 🟡 real-UI (`targets_journeys.rs`): add-target no-dup, stub-disclosure guard (no site), real astronomy after site creation (#440 confirmed landed) | ✅ `targets_planner.spec.ts` (batch 5, PR #454, 2026-07-05 + planner site-gate regression guard): no-site prompt / real-astronomy-after-site-creation / persisted-site-after-reload (9.1a–c), catalog list + typeahead + on-demand SIMBAD resolve (9.2a–c), honest-empty favourites/sessions states (9.3a–b) | `windows-journeys/journey-09-targets-planning.md` |
| 10 Settings/appearance/i18n | ✅ | 🟡 real-UI (`settings_journeys.rs`): no-global-Save + real auto-save round-trip, theme live-apply + settings-DB persistence (theme-settings-db, 2026-07-09 — supersedes the old localStorage-only cross-relaunch claim, see note below). Remaining sub-tests (altitude clamp, log-panel layout/export, 1100×720 convention, translated backend errors, command palette, sidebar persistence) still route-load-smoke only | ✅ `settings_appearance_i18n.spec.ts` (batch 6, PR #455, 2026-07-05; stabilized PR #494): Ingestion/Cleanup panes auto-save round-trip, 4-theme switch + persistence, 1100×720 pinned-header layout convention, no-raw-message-key + plural-form (audit event count) i18n assertions, log-panel filter/Escape-close | `windows-journeys/journey-10-settings-appearance-i18n.md` |

Legend: ✅ solid coverage at that layer · 🟡 partial/IPC-only/smoke-only ·
❌ none. Layer-1 "✅" means the backend logic is real-tested; it says
nothing about the UI surface, which is exactly the gap the other columns
track.

### Post-convergence feature areas not yet in the table above (specs 040/043/044/046/047/048/049)

These shipped after this file's original 22 areas were enumerated and were
never folded in. Status verified against real code on `main`
(2026-07-05), not against spec-doc `Status:` headers, which lag behind
what's actually merged (`specs/SPEC_STATUS.md` itself is stale in places —
see the finding below).

| Spec | Area | Layer-1 | Layer-2 | Mock | Manual-Windows | Note |
|---|---|:--:|:--:|:--:|:--:|---|
| 037 | Shared `PlanReviewOverlay` refusal gates (issue #1220) | ✅ | ✅ `plan_review_overlay_journeys.rs` | 🟡 (overlay review→apply/cancel only, `inbox_ingest_confirm.spec.ts`) | journey-06/07 | `PlanReviewOverlay.tsx` is the single review surface for the cleanup, archive, restore, and source-view-removal flows. Two real-UI tests cover its gates: `plan_review_destructive_confirm_gate_and_apply` (a `delete`-action cleanup item renders `plan-review-confirm-destructive`, `plan-review-approve-apply` stays disabled until it is ticked, apply then permanently removes the file from disk) and `plan_review_empty_archive_plan_refuses_apply` (a 0-item `archive.plan.generate` renders `plan-review-empty-reason` and keeps `plan-review-approve-apply` disabled). Prior plan journeys drive `plans.approve`/`plans.apply.direct` over the invoke bridge and never open the overlay DOM, so these testids had no real-backend hits |
| 040 | Calibration master detection | ✅ | 🟡 (suggest only) | ❌ | journey-08 | Shipped without `plan.md`/`tasks.md` (documented deviation); least-scrutinized recent backend feature |
| 041 | Inbox single-type sub-items / destination model | ✅ | 🟡 (IPC only) | ❌ | journey-02/03 | iteration-2 now on `main` via #349 |
| 043 | UI redesign (theming, layout convention, `aria-sort`) | n/a | 🟡 (smoke only) | ❌ | journey-10 | Foundation + round-2 shipped; pill-system unification and resizable splitters still pending per SPEC_STATUS |
| 044 | Targets planner — Track B ephemeris/observer engine | n/a (frontend-only) | ❌ | ❌ | journey-09 | **Compute engine merged (`a395ce93`) but functionally unreachable**: real astronomy is gated behind `useObserverSiteExists()`, and `site-gate.ts::readSiteExists()` is hardcoded `return false` — no site-creation UI/command exists on `main` until PR #440 (spec 044 US3, open) merges. Verify this is still true before reusing this row. |
| 046 | i18n infrastructure & error-code translation | ✅ | n/a (cross-cutting) | ❌ | journey-10 | `specs/SPEC_STATUS.md`: Implemented, 36/36. **No longer single-locale.** 046 shipped the catalogue with English hard-pinned and no switcher (its own FR-004); spec 061 supersedes that with a second locale and a runtime chooser — see the 061 row. The catalogue-completeness guarantee 046 established (1856 keys, zero unused) is what let 061 be an add-locales job rather than a string-extraction job |
| 047 | Targets planner — Track A (Moon/filter/opposition) | ✅ | ❌ | ❌ | journey-09 | Implemented in code but **also gated by the same site-exists check as 044** (spec 047 D7) — see 044 row; spec's own T028 explicitly defers verify-on-windows here |
| 048 | Per-frame inventory / live session membership | ✅ | ✅ `inventory_journeys.rs::reconcile_drops_externally_deleted_frame_from_real_ui_count` | ❌ | — (folds into journeys 4/6) | `main` PRs #435/#442/#500 merged; US1-US4 frontend surfaces landed (this PR): a per-root "Reconcile now" button (Settings → Data Sources, `reconcile-now-<rootId>` testid) drives `inventory.reconcile.run`; per-session frame inventory + relink UI drives `inventory.frame.list`/`inventory.frame.relink`; a session-scoped raw sub-frame cleanup review drives `cleanup.candidates.scan`/`cleanup.plan.generate` (US3, #500); per-root reconcile-mode/detection-trigger controls (wizard Scan step + Settings) drive `inventory.root_config.{get,set}`. The journey now clicks the REAL "Reconcile now" button instead of invoking the command over the bridge directly — closes the prior zero-frontend-callers gap. **US5 (T037-T039, calibration match missing-frame awareness) merged via #503**: Layer-1 covers both trigger paths (`crates/app/core/tests/calibration_missing_flag_integration.rs`, real `run_reconcile`/artifact `mark_missing`/`mark_recovered`, no mocks); the flag surfaces in `MasterDetail.tsx` (`calibration.masters.get`), but journey 8's Matching/assign UI is flagged UNREACHABLE (see row 8) so no new Layer-2 journey was written for it — same blocker, not a new gap |
| 049 | Source-view generation (symlinks/junctions) | ✅ (partial) | 🟡 `source_view_journeys.rs::generate_source_view_creates_reviewable_wbpp_plan` | ❌ | — (new journey needed, none written this pass) | `main` PR #439 (US1) + #443 (US2 profile layout) merged; junction/symlink behavior is real, OS-specific filesystem behavior a mock layer structurally cannot prove — highest-value Layer-2 candidate of the unlisted specs. New journey drives the REAL "Generate source view" dialog end-to-end to a reviewable, approved `prepared_view_generation` plan and asserts the real WBPP 3-level destination layout — **but stops at `approved`**: real symlink/junction materialization needs `plans.apply_real`'s `tauri::ipc::Channel`, which no product UI constructs for this plan type (`SourceViewsSection`/`ProjectBottomDetail` drop the generated plan id on the floor — see the journey's module docs), the same channel-free-apply blocker already tracked for cleanup/archive (batch items #1/#2 below). Also surfaced a real, pre-existing data-quality gap: `projects.source.add`/`projects.create` have shipped empty `filter_snapshot`/`exposure_snapshot` since spec 003 and were never wired to the real per-session values available since spec 048, so every real project's generated layout falls back to `nofilter`/`unknown-exposure` folders instead of the frame's real filter |
| 061 | Selectable application language | ✅ `crates/app/core/tests/locale_settings_gate.rs` | 🟡 (written, CI-unproven) | 🟡 (Playwright, mock-mode) | ⏳ pending (`astro-plan-8tc`) | **All four PRs still open at the time of writing (#1310 runtime, #1319 wizard step, #1320 Settings control, #1321 pt-BR) — re-verify this row once they land.** Layer-1 is the strongest link: the gate writes `locale` through the real settings use case against real SQLite, drops the handle, reopens and asserts the *stored value*, because `settings_update` returns `Ok` for an unregistered key and silently persists nothing (research D8) — a UI-only check would have passed against a feature that loses the preference every restart. Layer-2: the wizard journey (`setup_wizard_language_step.spec.ts`) was executed 2/2 by its author; the Settings journey (`settings_appearance_i18n.spec.ts`) was **written but never run** — no dev server in the authoring session — so treat it as unproven until CI. Mock mode cannot prove persistence at all here: it no-ops `settings_update('general', …)`, which is exactly why the durable proof lives at Layer-1. Manual-Windows is genuinely outstanding and covers the three things no automated layer reaches: layout at 1100×720 under longer Portuguese strings, count rendering at 0/1/many across the 50 plural keys, and a cold start into a non-Settings route proving the `main.tsx` boot wiring applies a saved locale to app chrome — that last one has no unit test by design, matching the repo's untested-boot-call-site convention, which is precisely how the wiring came to be missing in the first place |

**Finding (verified against code, not spec prose)**: `specs/SPEC_STATUS.md`
row 77 (044) reads "Track B specced, implementation in progress... T001–T003
landed" — this is stale; `git log` shows `a395ce93` ("real tonight altitude,
rise/set, and imaging-time in the Targets planner", #436) already merged to
`main`, well past T001–T003. Do not use `SPEC_STATUS.md` prose alone to
judge what's implemented; check the running code (as this audit did for the
site-gate finding above).

### Layer-2-only flows (cannot be reached by the mock-Playwright layer, ever)

These are backend-driven behaviors the mock layer structurally cannot
assert, because they require a real Tauri `Channel`, a real filesystem, a
real async event pipeline, or a real OS integration — mocking them would
just test the mock, not the product:

- **`plans.apply.status` durable progress polling and the real file-move
  side effect** (`plan_review_apply_with_audit`) — needs a real filesystem
  and the real `plan_apply_events` table.
- **Event-driven session grouping after plan apply**
  (`ingestion_sessions_search`) — needs the real async `plan_listener` →
  `ingest_light_frames` pipeline; a mock can fake the end state but not
  prove the pipeline actually fires.
- **`lifecycle.transition.apply` + `lifecycle.ledger.list` real DTO
  round-trip** (`lifecycle_integrity`) — needs the real backend's
  business-rule engine, not a canned mock response.
- **`artifact.watcher.attach` real filesystem reconciliation + live watch**
  (`cleanup_plan_review`) — needs a real directory watcher.
- **Archive/cleanup plan `apply` with a `tauri::ipc::Channel` progress
  argument** — structurally cannot be driven by Playwright at all (no
  Tauri IPC channel in a browser context); this is Layer-2-or-manual-only
  by construction, not just by current gap. (The channel-free sibling,
  `plans.apply.direct`, is what Journeys 6/7 now drive at Layer-2 — it does
  not change this bullet, which is about the real UI's Channel-based path.)
- **Symlink/junction creation for source views (spec 049)** and **OS trash
  semantics (spec 017/025)** — real, OS-specific filesystem behavior; a
  mock can only assert the UI *called* the right command, never that the
  junction/trash operation actually succeeded on that OS.
- **Native OS folder pickers, "Show in File Explorer" reveal, tool-launch
  process spawn** — real OS integrations outside any webview.

### Batched plan — new Layer-2 (thirtyfour) journeys to author, ordered by risk/value

1. **DONE (2026-07-05). Archive lifecycle + trash + permanent delete**
   (Journey 7) — highest product risk (irreversible deletion), previously
   **zero automated coverage at any layer**. The channel-free apply
   command (`plans.apply.direct`) that blocked this now exists;
   `crates/e2e-tests/tests/archive_journeys.rs::archive_lifecycle_apply_trash_permanent_delete`
   covers real lifecycle progression → `archive.plan.generate` → apply →
   real FS move → `archive.list` → `archive.send_to_trash` →
   `archive.permanently_delete` (honoring `blockPermanentDelete`, and
   honestly asserting only the real metadata-level response since neither
   management command performs real filesystem I/O yet — see
   coverage-matrix note above).
2. **DONE (2026-07-05). Cleanup plan apply completion** (Journey 6) —
   `cleanup_plan_review` now extends past `plans.approve` via
   `plans.apply.direct` and asserts the real filesystem mutation + audit
   record.
3. **Calibration masters ingest → Calibration page → matching → assign**
   (Journey 8, spec 040) — DONE for the reachable half (2026-07-05,
   `crates/e2e-tests/tests/calibration_ui_journeys.rs`): masters ingest as
   individual real Inbox items (not a folder aggregate) and confirmed
   masters appear as their own real Calibration-page row with genuinely
   kind-conditional detail (a property is OMITTED, not dash-faked, when its
   fingerprint field is null). Added a thin, additive `master-row-<id>`
   test-hook to `MastersTable.tsx` (mirrors `InboxList`'s existing
   `inbox-item-<id>` convention) since no per-row testid existed.
   **FINDING (real product gap, not fixed here)**: the matching/assign UI
   (Tests 3-5 — ranked candidates, assign/cancel, offset-tolerance) is
   `MatchCandidatesPanel.tsx`, fully implemented and unit-tested, but **no
   page mounts it** — `CalibrationPage.tsx` renders only `MastersTable` +
   `MasterDetail`. `CalibrationMatchPanel.tsx` (a different, read-only
   component on the project detail page) says in its own doc comment
   "assignment is done from the Calibration page (CalibrationPage +
   MasterDetail)", which is not true of the code as of this writing. This
   makes Tests 3-5 unreachable from the real app and therefore
   un-automatable at Layer 2 until product wiring lands.
4. **Source-view generation** (spec 049) — PARTIALLY DONE (2026-07-05,
   `crates/e2e-tests/tests/source_view_journeys.rs::generate_source_view_creates_reviewable_wbpp_plan`):
   drives the REAL "Generate source view" dialog (real project row →
   `SourceViewsSection` button → dialog submit) to a real, reviewable
   `prepared_view_generation` plan and asserts the WBPP 3-level
   `{date}/{filter}/{exposure}` destination layout — then stops at
   `approved`. The remaining scope (assert real symlinks/junctions exist
   on disk) is **blocked** on the same channel-free apply command as
   items #1/#2: `plans.apply_real`'s `tauri::ipc::Channel` is never
   constructed for this plan type by any product UI
   (`ProjectBottomDetail` drops the generated plan id — see the
   journey's module docs). Also surfaced the empty
   `filter_snapshot`/`exposure_snapshot` data-quality FINDING recorded
   in the 049 row above.
5. **Per-frame inventory reconciliation** (spec 048) — DONE end-to-end,
   trigger included (updated this pass;
   `crates/e2e-tests/tests/inventory_journeys.rs::reconcile_drops_externally_deleted_frame_from_real_ui_count`):
   real external raw-frame deletion → clicking the REAL "Reconcile now"
   button on the root's Settings → Data Sources card (`reconcile-now-<rootId>`
   testid, spec 048 T022 frontend) → the real Add-sources session picker's
   frame count drops 2→1 in the product DOM. Previously the reconcile
   *trigger* stayed on the invoke bridge because zero frontend callers
   existed for `inventory.reconcile.run` — that gap is now closed; the
   command's response-shape assertions were replaced with the same
   `sessions.list` settle-poll used for the AFTER read, since a UI click no
   longer exposes the raw IPC response to the test. The raw sub-frame
   cleanup-candidate feed now has its own frontend surface too
   (`RawFrameCleanupSection`, session-scoped, unit-tested) but no dedicated
   Layer-2 journey yet — the next highest-value addition here.
6. **Inbox UI-level gate + reclassify + root-picker** (Journeys 2/3) — DONE
   (2026-07-05, `crates/e2e-tests/tests/inbox_ui_journeys.rs`): mixed-folder
   splitting, the unclassified-frame-type gate + bulk-reclassify unblock, the
   missing-path-attribute (FR-032) gate as a distinct real mechanism, Confirm
   never moving a file before Apply, Apply moving to exactly the
   overlay-displayed destination path, and the catalogue-in-place (0-move,
   byte-identical) variant. Extended 2026-07-19 (#711 Instance A): the
   Type-column badge of an UNSPLIT folder that resolved to no frame type
   must read "unclassified", never "classified"
   (`inbox_ui_unsplit_unclassified_folder_badge_is_not_classified`). The four
   pre-existing journeys above all passed identically with and without the
   fix — they assert on `inbox-unclassified-alert`, Confirm enablement and
   move counts, and never read the Type column, so badge correctness was a
   genuine coverage gap rather than a dead harness (a testid-rename positive
   control produced a real 20s timeout failure). That journey is WRITTEN BUT
   NOT YET EXECUTED — validate it on its first Layer-2 run. Remaining gap
   (follow-up, not done): the multi-root destination picker prompt
   (`inbox-root-picker`, needs 2+ registered light-frame roots) and the
   stale-plan-refusal UI (external file mutation between confirm and apply)
   are still unautomated at this layer.
7. **Targets catalog + SIMBAD resolve-on-demand + stub-disclosure guard**
   (Journey 9) — DONE (2026-07-05, `crates/e2e-tests/tests/targets_journeys.rs`):
   add-target via the real dialog resolves to the same real id on re-add (no
   duplicate), and — the safety-critical part — with no observing site
   configured the real site-setup prompt renders (never a fabricated Moon
   summary) and the per-target Opposition/Lunar-separation cells show a real
   explicit "unknown" disclosure rather than a fabricated-looking number.
   Deliberately does NOT exercise a live SIMBAD network lookup (flaky in CI);
   uses the bundled offline seed cache instead, matching the repo's existing
   offline-`FakeResolver` convention for #14.
8. **Real planner astronomy end-to-end** (Journey 9, specs 044/047) — DONE
   (2026-07-05, `targets_journeys.rs::targets_planner_real_astronomy_after_site_creation`).
   **Verified against code while authoring this**: PR #440 has landed —
   `apps/desktop/src/features/targets/site-gate.ts::readSiteExists()` now
   reads the real `observing-sites/site-store`, no longer a hardcoded
   `false` — so this is no longer blocked. The journey creates a real
   observing site via the real Settings → Target Planner → Observing Sites
   UI and asserts the Targets page's site-setup prompt is replaced by a real
   `MoonSummary`, with no reload needed (`useSyncExternalStore` reactivity).
9. **Sessions derived-view invariants** (Journey 4) — DONE (2026-07-05,
   `crates/e2e-tests/tests/sessions_journeys.rs`): nothing appears before a
   plan applies, a real session appears automatically post-apply (no review
   step), no Confirm/Re-open/Reject/Ignore controls anywhere on the page,
   and a no-op rescan never duplicates the session.
   **FINDING (real, not fixed here)**: journey-04 Test 4 ("edit a session's
   Notes field") describes a feature that does not exist —
   `SessionDetail.tsx` (read in full) has no notes field, and no session
   notes command exists anywhere in the codebase. Its own doc comment says
   metadata is edited "post-hoc via the inbox per-file metadata/override
   tables" instead. This is either a stale journey-doc claim (spec 041
   FR-051/T076 removed the review lifecycle around the same time) or a
   feature that never shipped — flagged here rather than silently dropped;
   worth a follow-up to correct journey-04 or ship the feature.
10. **Project lifecycle UI surface** (Journey 5) — PARTIALLY DONE
    (2026-07-05, `crates/e2e-tests/tests/lifecycle_ui_journeys.rs`): the
    create-wizard's Tests 1/2 (duplicate-name blocks with a real inline
    field error; a unique name creates real `lights/`/`darks/` folders under
    the registered project library root — the exact PR #414 regression).
    Still open as follow-up: attach/remove-sources UX, per-channel
    integration time, manifests/notes autosave, tool-launch spawn +
    containment, artifact watcher; tool-launch and the watcher specifically
    need Layer-2 (a real process/filesystem watcher), not the mock layer.
11. **Settings + layout-convention + i18n regression guard** (Journey 10) —
    PARTIALLY DONE (2026-07-05, `crates/e2e-tests/tests/settings_journeys.rs`):
    no-global-Save-button + real auto-save round-trip (Test 1), and theme
    live-apply + settings-DB persistence (Test 2). Still open as follow-up:
    the 1100×720 layout convention and no-raw-error-code checks are cheap,
    cross-cutting regression guards worth adding next; altitude clamp,
    log-panel layout/export, command palette, and sidebar persistence remain
    unautomated too.
    UPDATE (theme-settings-db, 2026-07-09): theme moved from purely
    `localStorage`-backed to DB-backed (settings `general` scope, `theme`
    key), with `localStorage` kept only as a synchronous boot cache
    (`hydrateThemeFromSettings()` reconciles it from the DB after boot) — a
    fix for WebView2 only flushing `localStorage`'s LevelDB store on a
    graceful shutdown, losing the choice on a forced kill (the finding Test 2
    originally diagnosed via `graceful_shutdown()`/CI run 28810006837). This
    moves theme into the same "cannot be proven to survive a relaunch in this
    harness" bucket as Ingestion (both `E2eApp::launch()`/`relaunch()`
    unconditionally wipe the DB via `reset_database()`), so Test 2's
    cross-relaunch assertion was trimmed to the live-apply + DB write-through
    checks only. A true cross-relaunch proof needs a harness `ResetScope`
    that preserves the DB — left as a follow-up alongside
    ingestion-settings-persist-across-restart.

See `docs/development/windows-journeys/journey-0{1..9,10}-*.md` for the
click-by-click manual scripts covering all of the above until each is
promoted to a real Layer-2 journey, and
`docs/development/e2e-mock-coverage-audit-2026-07-05.md` for the
mock-Playwright layer's own parallel batched fix-list (batches 1–7 there
target the same gaps from the mock-layer side where a mock CAN reach the
behavior — see the "Layer-2-only flows" section above for which ones a mock
never will).

## Mock-Playwright batch completion + StepSite gap closed — 2026-07-09

All 6 fixer batches from `e2e-mock-coverage-audit-2026-07-05.md`'s
prioritized list landed the same day they were audited (PRs #447 batch 2,
#448 batch 1, #452 batch 4, #453 batch 3, #454 batch 5, #455 batch 6; #494
later stabilized a flake in #455's suite). Batch 7 (retire the orphaned
`tests/integration/*.spec.ts` first-run specs, outside `playwright.config.ts`'s
`testDir`) is also done (`4c221a63`/`824291b6`). The per-journey table above
is updated accordingly — Journeys 2/3/5/6/7/8/9/10 move from "❌ none" to
"✅" at the Mock-Playwright layer.

This pass (spec 037 close-out) additionally verified the remaining named
gaps in that audit against today's code and closed the one still genuinely
open and mock-reachable: **`StepSite.tsx`** (the first-run wizard's
Observing Site step, spec 044 US3/T016) had no dedicated test at any layer,
mock or vitest — `tests/e2e/setup_wizard_site_step.spec.ts` (NEW) now covers
its own rendering/copy, `siteStepError` inline validation, the FR-025
optional/skippable behavior, and field-value retention across Back/Continue.
Real site persistence (`saveSites`) is NOT exercised here — `SetupWizard.
handleFinish` gates that whole branch behind `!isMockMode`, so it is
structurally unreachable from this mock layer; it stays covered by
`ObservingSites.test.tsx` (vitest, the Settings editor sharing the same
field set) and the Layer-2 `targets_journeys.rs` site-creation journey.

The other two named audit targets were re-verified as **already closed** by
the existing batch work, not newly gapped: 046 i18n cross-cutting
(no-raw-message-key + plural-form assertions) and 047 moon-pill/opposition
disclosure are both exercised by `settings_appearance_i18n.spec.ts` and
`targets_planner.spec.ts` respectively (see the per-journey table).

**Correction (2026-07-20, issue #1235): the Data Sources line above was
stale.** Rescan and Disable/Enable/Delete already had vitest component
coverage *before* this note was originally written:
`apps/desktop/src/features/settings/DataSources.rescan.test.tsx` (rescan,
`7e19d7bb`, 2026-07-04) and `DataSources.disable-delete.test.tsx`
(disable/enable/delete, `7219fb57`, 2026-07-04) — both predate this section's
2026-07-09 date. Per this file's own convention (the `ObservingSites.test.tsx`
precedent a few lines above), a vitest `.test.tsx` counts as real coverage,
so "UNCOVERED" was never accurate for those two actions. **Remap and Reveal
remain genuinely uncovered at every layer** — no test references `remap` or
`reveal`/`RemapRootDialog`/`handleReveal` in any of the three DataSources
`.test.tsx` files, and no Mock-Playwright spec exists for Data Sources at all
(only `setup_wizard_site_step.spec.ts`, `settings_framing.spec.ts`,
`settings_appearance_i18n.spec.ts`, and `source_view_*.spec.ts` exist under
`tests/e2e/`). **Still open (not closed this pass, out of this task's named
scope)**: the full 6-step wizard happy path (Sources→Tools→Config→Site→
Confirm→Scan) end to end, and any Mock-Playwright or Layer-2 real-UI coverage
of Data Sources management (rescan/remap/disable/delete/reveal) — these
remain follow-up candidates, not regressions.

## Spec 051 (tauri-shell-integration) — shipped-and-untested surface — 2026-07-20

Per `specs/SPEC_STATUS.md:92` (🟡 Partial, 34/64 tasks ticked — undercounts,
several merged tasks aren't checked off), US1/US2/US3/US4/US5/US6/US7/US10 are
merged to `main` and this matrix has **zero reference to any of them** at any
layer:

- US1 single-instance guard (`64a94881`/#471)
- US2 favourites-in-DB (`54378086`/#472)
- US3 cleanup-overrides-in-DB (`c990f967`/#474)
- US4 window-state persistence (`e9b1622b`/#476)
- US5 native menu bar (`e9b1622b`/#476)
- US6 native theme sync (`29617775`/#475)
- US7 diagnostics log file (`e9b1622b`/#476)
- US10 signed-update groundwork (`c1f3ede9`/#473, `a33dc427`, `60732f2f`/#469)

None of these have a Layer-1 integration test, a Layer-2 real-UI journey, or a
Mock-Playwright spec in this repo as of this writing — they are desktop-shell
integrations (native OS behavior, window chrome, update signing) that are
plausibly hard to assert in the existing harnesses, but that has not been
verified either way; this row exists so the gap is tracked rather than
silently absent. US8 (OS notifications) and US9 (release-build native
behavior/reload-guard) are real open *product* scope per `SPEC_STATUS.md`, not
a testing gap, and are excluded from this row.

## Specs not yet implemented — no coverage expected — 2026-07-20

Per `specs/SPEC_STATUS.md:92-95`, these specs in the 050-058 range have no
shipped product code, so their absence from this matrix is not a test gap:

- **050** publishable-crate-extractions — docs-only plan-of-record (#429)
- **056** onboarding-redesign — 📄 Specified (2026-07-18), no implementation
- **058** inbox-drop-parent-items — 📄 Specified (2026-07-19), no implementation

Check `specs/SPEC_STATUS.md` before re-deriving shipped-vs-specified for any
spec above 049; specs 052-055/057 (including 053, which has a `specs/`
directory but no row in `SPEC_STATUS.md`'s table) aren't tracked there and
are out of scope here too.

## Spec 056 — three-layer onboarding (orientation walk + checklists + spotlight) — 2026-07-18

New area #24 (above). The behavioral contract is journey **J18**
(`docs/journeys/J18-onboarding-orientation-getting-started/journey.md`, VC-001):
one-time orientation walk after first-run, a per-page "Getting started"
checklist that auto-ticks from real domain events (never demo data), a
non-blocking find spotlight, and remove/restore/replay controls.

| Layer | Coverage | Where |
|---|---|---|
| L1 (real backend) | publisher → subscriber → persisted tick for the three real milestone events (`inventory.confirmed`, `project.created`, `tool.launch` with `outcome=="spawned"`); restore-sourced events inert; settled states never downgraded (VC-003) | `apps/desktop/src-tauri/tests/onboarding_subscriber_integration.rs` |
| L1 (seed/restore) | pre-existing confirmed inventory/projects/launches pre-tick on seed AND restore; manual/dismissed rows survive restore; settle sets `section_hidden_at` (FR-031) | `crates/app/core/tests/onboarding_seed_integration.rs` |
| L2 (real UI → real IPC → real backend) | orientation walk auto-renders + completes, then a **real inventory confirm** drives a **live auto-tick that renders in the checklist** (VC-004) | `crates/e2e-tests/tests/onboarding_journey.rs::orientation_walk_then_real_confirm_renders_live_auto_tick` (`just test-e2e`) |
| Mock-Playwright | walk (incl. skip/Escape/never-twice/replay), accordion semantics + tooltip-on-focus, spotlight dismissal matrix, persistence flags, reduced-motion parity. **Known limit (VC-002)**: mock-mode event path is a no-op, so auto-ticking is NOT validated there — that is exactly what the L2 journey covers | `tests/e2e/onboarding_{orientation,checklist,choreography,spotlight,removal}.spec.ts` |

**Milestone-event follow-ups (research R4)**: two milestones have no dedicated
bus topic in v1 (no new backend events minted) so their checklist items are
manual — `calibration.master.registered` and a site-saved event are filed as
follow-ups against campaign tracker #881.

## Spec 008 Q27 iteration — framing clustering + Inbox-confirm attribution (F-Framing-8/9/11) — 2026-07-17

New area #23 (above). Grouping a project's light sessions into **framings**
(tolerance-based clustering, R11a) and ranking Inbox-confirm **attribution**
candidates against existing framings/projects (FR-019/FR-020/FR-022) are
real, shipped backend features with **zero frontend consumer** for the
grouping/attribution flow itself — `projects.framing.list/merge/split/
reassign` and `inbox.confirm`'s `attributionCandidates`/`chosenAttribution`
fields are wired IPC commands with no page rendering a framing list, a
merge/split/reassign control, or an attribution-candidate picker. This is a
real product gap (no UI exists yet), not a testing gap — flagged explicitly
per this feature's own auditing convention (see the journey-04 Test 4
precedent). The one exception: **Settings → Framing** (NEW, F-Framing-11),
the clustering tolerance tunables pane, is a real, shipped UI surface.

| Scenario (F-Framing-8 Layer-1 sweep) | Layer-1 test |
|---|---|
| Multi-night/multi-filter sessions collapse into one framing | `crates/sessions/src/clustering.rs::multi_night_multi_filter_sessions_collapse_into_one_framing` |
| Pointing/rotation beyond tolerance splits into distinct framings; `user_adjusted` framings never modified by re-derivation | `clustering.rs::pointing_beyond_tolerance_splits_into_distinct_framings`, `::user_adjusted_framing_membership_is_never_modified` |
| NULL-geometry sessions excluded, never zero-defaulted | `clustering.rs::null_geometry_sessions_are_excluded_not_zero_defaulted` |
| `framing.merge`/`split`/`reassign` persistence + `user_adjusted` flips, cross-project/partial-mutation rejection | `crates/app/core/src/framing.rs` (`merge_folds_sessions_and_deletes_merged_framings`, `split_moves_selected_sessions_into_a_new_framing`, `reassign_moves_session_between_framings`, + rejection-path tests) |
| Attribution ranking: `add_to_framing` top-ranked within tolerance, `new_framing` suggested out of tolerance, `flag_optic_difference` for a same-target/different-optic-train match, trailing `new_project` fallback always present | `crates/app/inbox/src/attribution.rs` (`compute_candidates_ranks_add_to_framing_top_when_within_tolerance`, `::compute_candidates_suggests_new_framing_when_pointing_is_out_of_tolerance`, `::compute_candidates_flags_optic_difference_for_same_target_different_optic_train`, `::compute_candidates_returns_only_new_project_when_nothing_matches`) |
| Mosaic first-new-panel suggestion (US6 AS3) | `attribution.rs::compute_candidates_mosaic_first_new_panel_suggests_new_framing` |
| Mosaic new-framing inherits the project's declared target, ignoring a misleading per-frame OBJECT resolution (NEW this pass — the one genuine coverage gap found) | `attribution.rs::apply_new_framing_for_mosaic_project_inherits_declared_target_ignoring_resolved_object` |
| User-pick-only apply via `chosenAttribution`, geometry-unavailable rejection, unassigned no-op | `attribution.rs` (`apply_new_project_creates_project_framing_and_persists_plan_pick`, `::apply_add_to_framing_links_existing_framing_without_creating_a_new_one`, `::apply_unassigned_is_a_noop`, `::apply_new_framing_without_geometry_is_rejected`) |
| Completed-project attribution match → add + reopen, honoring the Q25 raw-subs-archived warning | `attribution.rs::apply_to_completed_project_reopens_and_surfaces_raw_subs_warning` |
| SC-008 end-to-end: a `chosenAttribution` pick persisted at confirm time materializes as real framing membership only once the plan applies, via the real `plan_listener` → `ingest_sessions` pipeline (no internal calls) | `crates/app/core/tests/attribution_integration.rs::chosen_framing_pick_materializes_as_session_membership_once_the_plan_applies` |
| F-Framing-11 settings wiring: a stored `framingPointingFractionOfFov` override actually changes `compute_candidates`' ranking outcome (not just the parameter struct); defaults reproduce `ToleranceParams::defaults()` bit-for-bit | `attribution.rs::tolerance_params_reads_r11a_defaults_when_unset`, `::tolerance_params_honours_stored_settings_overrides`, `::compute_candidates_honours_widened_pointing_tolerance_setting` |
| Framing settings keys: validation bounds, `load_settings`/`get_settings` round-trip | `crates/app/settings/src/lib.rs::framing_tolerances_round_trip_through_db`, `::framing_tolerances_reject_out_of_range_values`; `crates/persistence/db/src/repositories/settings.rs::load_settings_applies_stored_framing_tolerance_overrides` |

**Deliberately out of this sweep's scope**: per-framing source view /
manifest coverage (F-Framing-7) — externally blocked on the Q20
(spec-026/049) and Q10 (spec-024) iterations, not yet started, per
F-Framing-7's own task note.

**F-Framing-9 (this pass)**: `docs/development/windows-journeys/
journey-11-framing-clustering-attribution.md` documents the click-by-click
Settings → Framing pane scenario (Test 1, real UI) plus the Tauri-MCP-bridge
`invoke()` scenario for framing grouping (Test 2), attribution ranking +
apply-path (Test 3), and merge/split/reassign (Test 4) — bridge-driven
because no UI exists to click through for those three. `tests/e2e/
settings_framing.spec.ts` (NEW, mock-Playwright) covers Test 1's mock-mode
round-trip: R11a defaults render, an edit auto-saves and survives an
unmount/remount via the `settings_get('framing')`/`settings_update('framing',
…)` mock fixture (`apps/desktop/src/api/mocks.ts`).

**Promoting Tests 2-4 to a Layer-2 thirtyfour journey or a mock-Playwright
spec is blocked on product UI work** (a framing list/merge/split/reassign
surface and an attribution-candidate picker at Inbox confirm), not on
test-harness capability — the same class of gap as row 8's
`MatchCandidatesPanel.tsx` "implemented but unreachable" finding, but here
even earlier: no consuming component exists at all yet.

## Spec 058 — Inbox drops the parent item — 2026-07-20

Re-shapes area #3 (and every Layer-2 journey that touches the Inbox queue).
**Status refreshed 2026-07-20 at `94db2e7a`** — verified against real code,
not against spec prose. The rows below say what the coverage must become; the
State column says what is actually true, so a later reader can tell a plan
from a fact. The L2 rows are marked ⚠️ rather than ✅ deliberately: the
journeys were rewritten for the new row shape and every referenced testid was
checked against real app source, but **the SC-005 journeys have never been
run** (T030), so nothing here is evidence they pass.

The behaviour change: a scanned folder no longer produces a placeholder inbox
row. Before classification a folder appears as a **source-group row** that is
structurally non-confirmable; after classification it becomes exactly N **item
rows**, one per frame-type group, with no aggregate row alongside them. Every
row states only facts about its own files. This deletes the read-side
suppression predicate that has been hiding the placeholder (SC-007) rather
than adding a new one.

| Coverage obligation | Layer | Target test | State at `94db2e7a` |
|---|---|---|---|
| A scanned-but-unclassified folder is visible as exactly one row, and that row is NOT an inbox item (SC-002b) | L2 | `crates/e2e-tests/tests/inbox_ui_journeys.rs`, source-group-row variant of `rescan_and_wait_for_item` (T014) | ⚠️ helper split done (T014) — `rescan_and_wait_for_source_group`; source-group rows render. Not yet run. |
| Selecting a source-group row asserts Confirm is **absent**; selecting an item row asserts Confirm is present | L2 | split of `select_only_item` (T015) | ⚠️ `assert_source_group_not_confirmable` (T015) asserts Confirm is ABSENT; `select_only_item` still asserts it present. Not yet run. |
| Classification replaces the source-group row with N item rows without dropping the user's selection (FR-017/FR-023, CHK011 rule: N=1 → select that item; N>1 → select the folder group header, never a sibling; N=0 → the source-group row stays selected, [#1178](https://github.com/platevault/platevault/issues/1178)) | L2 | T016/T017 update to the five journeys in `inbox_ui_journeys.rs` | ⚠️ written (T016/T017). CHK011 N>1 selects NOTHING — the folder group header now exists (T034) but header selection is a follow-up; selecting a sibling would designate a primary (D-002). |
| 1 row for a uniform folder, N rows for N frame-type groups, zero aggregate rows (SC-002) | L1 + L2 | `crates/app/inbox` classify tests; `inbox_ui_journeys.rs::inbox_ui_mixed_folder_splits_into_single_type_items` | ✅ L1: placeholder deleted at source (T012) and the suppression removed (T024); `t031_two_frame_types_yield_two_items_and_no_aggregate` pins N items + zero aggregate. ⚠️ L2 not run. |
| List badge, detail badge and the item's own classification agree — the #711 exit condition (SC-001) | L1 + L2 | as above | ✅ achieved by construction — there is no longer a row that can disagree, rather than one hidden at read time. |
| Confirming one sibling leaves the other N−1 unchanged in state, classification and plan binding (SC-006) | L1 + L2 | `confirm.rs::confirm_alters_exactly_one_item_and_leaves_its_sibling_untouched_sc006`; `inbox_ui_journeys.rs` | ✅ L1 asserted in both directions (T037). ⚠️ L2 not run. |
| Group the Inbox by folder and see one folder's siblings under one header (SC-010, D-007) | L1 + L2 | `InboxList.folderGrouping.test.tsx`; `inbox_ui_journeys.rs` | ✅ L1 (T034 folder dimension). ⚠️ L2 not asserted. |
| Reclassify does not report `classified` while mandatory attributes are missing — #711 Instance B stays fixed (SC-011) | L1 | the `22f94a9e` regression test (PR #1086) | ✅ merged, must be preserved |

### The three SC-005 journeys are the gate

Spec 058's own exit bar names three Real-UI journeys that regressed under
#1038 and must pass again after the re-shape. All three live in
`crates/e2e-tests/tests/inbox_ui_journeys.rs` and all three call the helpers
T014/T015 split, so all three change mechanically even where their assertions
do not:

| SC-005 journey | Test |
|---|---|
| catalogue-in-place zero-moves | `inbox_ui_catalogue_in_place_zero_moves_byte_identical` |
| confirm-then-apply-to-shown-destination | `inbox_ui_confirm_does_not_move_then_apply_moves_to_shown_destination` |
| bulk-reclassify-unblocks-confirm | `inbox_ui_unclassified_gate_bulk_reclassify_unblocks_confirm` |

The two remaining journeys in that file
(`inbox_ui_mixed_folder_splits_into_single_type_items`,
`inbox_ui_missing_path_attribute_banner_blocks_confirm`) call the same helpers
and are in scope for T016 for that reason.

### Explicitly NOT covered by spec 058 — do not add a row for it

**SC-009** (a re-derivation that removes an item with an open plan marks it
superseded, blocks its plan pending the user's decision, and surfaces an
explicit superseded signal) is **knowingly not delivered by spec 058**. D-005
stands as a recorded decision; only its mechanism is descoped, to
`specs/tiny/reclassify-split-per-item-and-rederivation.md` (PR #1097),
together with the retired FR-020/021/022. The real exit bar for 058 is the
other **eleven** criteria. Full record:
`specs/058-inbox-drop-parent-items/sc-009-boundary.md`.

Consequently the folder-wide reclassify refusal survives this feature: both
PG-3 interlocks (`crates/app/inbox/src/reclassify.rs` group-open-plan block,
`crates/app/inbox/src/classify.rs` `plan_open` re-derivation guard) stay in
place. A Layer-2 journey asserting a *successful* reclassify across a folder
with an open sibling plan would fail correctly, and must not be written until
the follow-on lands.

### Journey docs

`docs/journeys/J02-ingest-review-reclassify-confirm-move/journey.md` is at v4
with the Δ4 behaviour delta (S1, S2, SC1, +SC6, +SC7) and carries G2/G3 marking
the not-yet-implemented boundary and the SC-009 exclusion. J03
(catalogue-in-place) shares the Inbox queue surface and inherits the same
row-shape change; it has not been re-versioned in this pass because its steps
describe confirm-and-catalogue rather than the scan/classify row shape — worth
a re-read once phases 2–4 land, not a silent assumption that it is unaffected.

## Recorded for whoever sequences the next journey-suite batch — 2026-07-20

Net wall-clock impact of the proposed next batch of real-UI journey work, so
sequencing doesn't have to re-derive it (issue #1228):

- Four new real-UI journeys: cleanup/apply (~45s), archive/trash (~50s), root
  remap (~35s), first-run wizard (~40s) — **+170s**.
- Delete `plan_review_apply_with_audit`
  (`crates/e2e-tests/tests/journeys.rs:155`, confirmed still present as of
  this writing) as subsumed by the new cleanup/apply journey — **−30s**.
- Move six IPC-only tests to `apps/desktop/src-tauri/tests/` — **−180s**.
- **Net: ≈ −40s.**

Caveats: `.config/nextest.toml`'s `e2e` profile runs with `test-threads > 1`,
so real wall-clock is better than this raw sum suggests. The actual
constraint is per-boot memory on a 4-vCPU CI runner, not seconds — sequence
this batch to bound concurrent app boots, not to shave time. **This estimate
assumes the Real-UI E2E suite actually runs to completion on `main`, which is
not true today**: `main`'s `e2e.yml` concurrency group cancels in-flight runs
on every subsequent merge, so the suite has been silently non-reporting
(#1202, #1208; fix in #1268). Re-baseline these numbers once #1268 lands and
a full run is observed to finish.

Documentation only; no code change follows from this note in this task.
