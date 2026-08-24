# Spec Status & Plan

**Living index of SpecKit feature status, dependencies, and the actionable frontier.**

Last reconciled: **2026-06-23** (full sweep). **2026-07-03**: 043 list-page
consistency + mock-mode E2E test redo landed (see the 043 row + CI note below).
**2026-07-03 (later)**: `redesign-ui-platevault` reconciled with its pushed
remote (041 single-type impl + 046/037/#360 merged); CI re-enabled; verified
against code — 041 iter-2 is implemented on the branch, 017 has UI. See the
updated 041 / 017 rows and the CI note.
**Last reconciled 2026-07-04 (post-convoy: 19 PRs merged).**
**2026-07-09**: `redesign-ui-platevault` (PR #349) is merged into `main`
(merge commit `6fcaa766`) — 041 iteration-2, 043 foundation/round-2, and the
044/047/048/049 work that had landed on the branch are now all on `main`.
Versioning was reset to 0.x for the first release cut (`cbd91378`); the
release/tag lane is owned separately from this document — this refresh does
not touch release-please or workflow files. 041/043/047/051 rows re-verified
against `origin/main` code; 044/048/049 are marked **in-flight** — active
lanes (044 Track B, 048 per-frame-inventory, 049 source-view-generation) are
still landing PRs and these rows will move again once those lanes report.
**2026-08-24 (partial sweep — gap fill only)**: rows added for 052, 053, 054, 055
and 057, which had no row at all despite existing as spec directories. Only those
five were verified against `origin/main`; every other row still dates from its own
note above and may have drifted. 034 and 059-060 have no spec directory, so their
absence from this table is correct.

> The per-spec `Status:` line in each `spec.md` had drifted badly — most still read
> "Draft" despite shipping. This document is the reconciled source of truth. Status
> here was verified against task completion, real code on `main`, and merge evidence,
> not the `Status:` field alone. When a spec ships or changes, update both its own
> `Status:` line **and** this table.

## Legend

| Marker | Meaning |
|---|---|
| ✅ Implemented | Shipped + verified |
| 🟡 Closeout-ready / partial | Core shipped; remaining tasks are DEFERRED or a small remainder |
| 🟠 Substantial open work | Real unfinished surface |
| 🔵 Active | In progress now |
| ⚪ Not started | Greenfield / placeholder |
| 🔴 Superseded | Replaced by another spec |

## Status by spec

| Spec | Status | Notes |
|---|---|---|
| 001 astro-library-manager | ✅ Closed | Umbrella/planning baseline; `tasks.md` retired 2026-08-17 (82 tasks named paths that were never built) |
| 002 data-lifecycle-state-model | ✅ Implemented (review) | 57/57; lifecycle partly *dropped* in inbox redesign — worth a tension review |
| 003 first-run-source-setup | ✅ Implemented | 32/32 (via 027/029) |
| 004 native-filesystem-controls | ✅ Implemented | 32/32 |
| 005 inbox-mixed-folder-split | 🔴 Superseded by 041 | Reassigned to 041's single-type model, which shipped. Source-group schema exists: `inbox_source_groups` (`crates/persistence/core/migrations/0001_initial_schema.sql:1151`) |
| 006 inventory-library-lifecycle | ✅ Implemented (closed 2026-07-03) | Core + 041/043/040 reconciliation landed; 12 open tasks all DEFERRED (Playwright-in-WSL, docs, additive-contract, spec-002-blocked enum snapshot). (Superseded a 2026-06-23 verify pass that found it NOT closeable due to a phantom `session.mixed_state` guard + 043 filter gaps; those were reconciled before this closeout.) |
| 007 calibration-matching-rules | ✅ Implemented (closed, 39/42) | Engine + adapters + DTOs shipped; `require_same_offset` **exists** in Rust (043 Settings toggle persistence: `9f0dc724`/#395). PR #589 (`f8cea5eb`, lane nH2) closed the previously-absent contract-test gap: extended `packages/contracts/tests/conformance-harness.mjs` (the generic AJV runner, already existed for spec 033 T063) with calibration.match.* fixtures — T002/T015/T020/T024/T029/T037/T041/T042 all `[x]`. 3 open remain, all DEFERRED: T032/T033 (quickstart doc, 1k-master perf check), T040 (blocked on spec 002's `CalibrationType` enum not yet finalized) |
| 008 project-create-onboard-edit | ✅ Implemented | Create / source-add / update / channel-inference commands + marker write + Q27 framing layer (`framing` table, `crates/app/core/src/framing.rs`, `projects_framing_*`) all on `main`. 12 open items live in beads; `tasks.md` retired 2026-08-17 |
| 009 project-lifecycle-model | ✅ Implemented | 21/21 tasks checked, but FR-003's "primary + More menu" pattern was never rebuilt in `ProjectDetail.tsx` (issue #764) — task-complete framing overstates UI parity |
| 010 guided-first-project-flow | 🔴 Superseded by 056 | Coach machinery deleted; `crates/app/core/src/onboarding.rs` is its successor. `tasks.md` retired 2026-08-17. Only FR-009 (no demo data) carries forward |
| 011 processing-tool-launch | ✅ Implemented (closed 2026-07-03) | Launch pipeline + UI + cwd-guard + detach/pid shipped & tested; T021 hint + X-1/X-2 done in closeout; 2 open (T018 Playwright, T022 real-spawn) DEFERRED (WSL/sandbox-blocked). Unblocks 012 |
| 012 processing-artifact-observation | 🟡 Closeout-ready — 35/37, 2 honest deferrals | PR #589 (`f8cea5eb`, lane nH2) closed 6 of the prior open set. 2 remain: T026 (Playwright MCP scenario, RE-ADJUDICATED — prior rationale updated, stays deferred) and TX02 (docs/research write-up, DEFERRED) |
| 013 target-lookup-from-fits-object | 🔴 Superseded by 035 | Fully subsumed by 035 — every FR/US covered by SIMBAD resolve-on-demand, or its one unique feature (fuzzy variant matching + confidence tiers) deliberately reversed (035 clarification Q4: exact-match only). 3 open tasks are obsolete stubs (spec-014 download pipeline / removed `catalog_equivalences`). Target-identity model retained in `crates/targeting/` |
| 014 catalog-index-licensing | 🔴 Superseded by 035 | download-catalog mechanism abandoned; attribution model retained |
| 015 token-pattern-builder | ✅ Implemented | Chip-based naming-pattern builder shipped: `crates/patterns/` (registry/resolver/validator/sanitize, ~64 tests) + contracts + Tauri `pattern_validate`/`resolve`/`preview` + live `PatternChipsEditor` in `NamingStructure.tsx` (validate + preview). Full SpecKit artifact set exists (~30 tasks, not "0/0"). Deferred downstream scope (per-source overrides, session-backed preview) handed to spec 018 |
| 016 source-protection-defaults | ✅ Implemented | 20/20 (underpins 017); closed by `cae0acf1` / #405 |
| 017 cleanup-archive-review-plans | 🟡 42/51 — small remainder open | Backend + archive/trash executor done; Archive UI exists; cleanup-plan review UI (WP-E) shipped via `d758b532`/#413. PR #492 (`bbbd11ff`) added the destination-path preview, retry-plan action in the plan review dialog, the virtualized overlay, and quickstart/a11y/perf-honest tasks. Remaining open (9 tasks): T015/T016/T021 (`[MOCKUP-DONE]` markers not yet promoted), T017/T018 (destination-conflict + archive-path integration tests), T023/T024 (approved→draft reopen state-machine test + spec-025 mock-executor coordination test), T050 (200-plan render perf check), T052 (spec-025 applying-state handoff coordination) |
| 018 settings-configuration-model | ✅ Reconciled + implemented (#348) | 42/46; spec reconciled to as-built scope/values architecture; backend + UI shipped & verified (live T034 walkthrough); 4 obsolete (contracts mirror, 014 key); open: FR-006↔043 density tension (cross-spec decision) |
| 019 bottom-log-viewer | ✅ Implemented (closed 2026-07-03) | Panel + backend + forwarder shipped; closeout added T006/T011 jsdom tests + T029 docs index + fixed dotted `log.recent`→`log_recent` binding bug; 1 open (T028 Playwright quickstart) DEFERRED (needs Tauri runtime host) |
| 020 router-url-state | ✅ Implemented | 22/23 |
| 021 developer-contract-diagnostics | 🟡 Closeout-ready — 35/37, 2 honest open (behind `dev-tools` feature) | PR #589 (`f8cea5eb`, lane nH2) re-verified the prior open set with dev-tools evidence; 2 genuinely stay open: T015 (`ts_hash`/`rust_hash` mismatch computation) and T033 (manual `devMode` + Cmd+K quickstart pass, human-driven) |
| 022 mantine-prototype-design-system | 🔴 Superseded by 027 | Mantine is not a dependency; only the banned-token lint rule `apps/desktop/biome-plugins/no-legacy-token-namespace.grit` still names it. `tasks.md` retired 2026-08-17 |
| 023 target-identity-history-notes | ✅ **Closed** | US1–US4 shipped on gen-3 (migration 0048 + `target.sessions.list`/`target.projects.list`/`target.note.*`) + caveats (note-edit audit event, UUID project deep-link, 16 KB note cap) + `speckit-verify` passed. `target.primary.rename` dropped; FR-005/FR-007 deferred |
| 024 project-manifests-and-notes | ✅ Implemented (closed 2026-06-23) | 32/37; 5 open all DEFERRED (FR-006/export/contract-tests); notes display-on-load fixed at close-out |
| 025 filesystem-plan-application | 🟡 Implemented; 6 items open | Real apply shipped via 041; overlap guard FR-017 done (`4b693ea7`/#408). Progress UI shipped: `apps/desktop/src/features/plans/usePlanApplyProgress.ts`, `PlanReviewOverlay.tsx` (incl. paused/`resumeStalled`). Open: `resume_plan` executor re-spawn (issue #575), rollback-audit `EXDEV` coverage (issue #577), 3 race/device-unavailable tests, docs cross-links, 10k-item perf benchmark |
| 026 generated-source-view-removal | ✅ Implemented (closed) | 23/23. No longer vestigial — spec 049 restored a live generation path (WBPP-ready source views, `54b56d28`/#439), so remove/regenerate is exercised for real. PR #545 (`384398df`, lane nG) closed the remaining P3 scope: stale-detection sweep (`sweep_view_staleness`, reusing #535's verify classification) wired into `list_views` so the UI gets freshened state on every load; per-item audit events + a Settings-adjacent UI audit history (`apps/desktop/src/features/projects/ViewAuditHistory.tsx`); a `kind_diverged` data-reconciliation scan on `Database::migrate()`; and real per-item cross-platform apply for view removal/regeneration, backed by a real-executor e2e test (`crates/e2e-tests/tests/source_view_journeys.rs`) that **found and fixed 2 latent bugs never exercised by an actual filesystem apply before**: `remove_prepared_view` left the archive destination empty (every archive item failed `source.missing`); `regenerate_prepared_view` used the raw DB id as a filesystem path. PR survived a rebase over the concurrent external #544 sqlx-drain |
| 027 frontend-implementation | ✅ Implemented | 99/99 |
| 028 frontend-quality-hardening | 🔴 Obsolete as a work queue | Its last substantive item named `ProjectDetailPane`, which does not exist; the shipped component is `apps/desktop/src/features/projects/ProjectBottomDetail.tsx`. UI design is owned by 032 |
| 029 tauri-backend-wiring | ✅ Implemented | 52/52 |
| 030 ui-audit-revision | 🔴 Superseded | Delivered by 031/032; `tasks.md` retired 2026-08-17 (117 unchecked boxes under a superseded spec) |
| 031 design-v3-implementation | ✅ Closed | superseded by 032 |
| 032 design-v4-implementation | ✅ Implemented | |
| 033 validation-bugfix-remediation | 🟡 Closeout-ready — 92/96 (4 honest partials, 0 open) | No longer blocked on 017 (017 shipped its remainder via #492). PR #561 (`a504a7fc`, lane nH1) closed the real-backend e2e gaps: T015/T024/T025/T031/T036a done via `crates/e2e-tests`/`app_core`/`app-tauri` real-backend tests (first real CI execution passed both ubuntu+windows legs, run 29143485185); T085 (doc resolution sweep) and T087 (dead-code adjudication: `@tanstack/react-table` removed, `angularSeparationFromMoonDeg` kept + documented test-only, and a real `[object Object]` error-display bug fixed in `PlanReviewOverlay.tsx`'s 3 catch blocks by switching to the canonical `errMessage()` helper) both closed [X]. Obsolete issue #1 closed. **4 remaining are honest partials** (marked `[~]`, not gaps): T006 (real-backend WebView harness — blocked on a pre-built `desktop_shell` binary not available headlessly in the WSL sandbox; superseded for automated evidence by spec 037's `crates/e2e-tests` Layer-2 harness), T049 (lifecycle real-backend round-trip — same T006 blocker, Rust unit tests are the authoritative signal), T083 (full automated suite headless — real-backend specs honestly `test.skip`, everything else green), T086 (gate suite green; the literal human-driven Windows-native Layer-4 walkthrough is deferred to the campaign's Windows-validation lane) |
| 035 simbad-target-resolution | ✅ Implemented | validated end-to-end 2026-06-23 |
| 036 retire-legacy-targets | ✅ Implemented | PR #255 |
| 037 e2e-integration-testing | ✅ Effectively complete (35/40; 5 open are deliberate SUPERSEDED decisions, not gaps) | Layer-1 + CI Stage A done; Layer-2 tauri-driver journeys merged (`1419b1a0`/#403) — `search.global`/`sessions.list`/`calibration.masters` are real backends. `sessions.transition` was **deleted** by spec 041 FR-051 (not pending). PR #531 (`35f86be4`, lane nE) closed T038/T039 with evidence: `contracts/coverage-matrix.md` refreshed (StepSite gap closed via new `tests/e2e/setup_wizard_site_step.spec.ts`; 046 i18n + 047 moon-pill items re-verified already-closed), `docs/development/testing.md` Layer-2 section rewritten, the dead `test:e2e:real` script fixed (`apps/desktop/scripts/run-e2e-real.sh` now backs `apps/desktop/package.json`'s script), and an offline-coverage claim corrected — `crates/targeting/resolver/tests/simbad_live.rs` is a pre-existing, ungated suite that hits the real SIMBAD endpoint as part of the default `cargo test --workspace` (not re-gated here, flagged as a separate backlog item). T001–T003/T006–T008 remain unchecked by design (SUPERSEDED — offline `FakeResolver`/`FakeSpawner` test doubles replaced the originally-planned `wiremock`/`better-sqlite3` approach). Still-open follow-up (not regressions): full 6-step wizard happy path E2E, Data Sources management E2E |
| 037 ipc-wrapper-removal | ✅ Complete | All caller areas migrated + merged 2026-07-03 (sessions #369, shell #372, settings #373, setup #374, targets #375, inbox #376, projects #377, dev #378, + fix `ad3497e1`); **0 live `@/api/commands` imports**. dev-tools commands generated under `--features dev-tools` (option C). Phase-4 teardown done: `commands.ts` + its guard test deleted, dead mocks removed, SC-001/SC-005 enforced by `api/ipc-boundary.guard.test.ts`; also swept guided + source-views callers |
| 038 wizard-scan-step | ✅ Implemented | merged (no committed tasks.md) |
| 039 cross-root-inbox | 🔴 Superseded by 041 | Scope fully implemented via 041 — cross-root `inbox_list`, inbox optional (`REQUIRED_KINDS`), rescan-all, bounded/virtualized. All 3 US + 7 FR + 5 SC verified in code 2026-07-03. No plan/tasks.md authored |
| 040 calibration-masters-detection | ✅ Implemented | validated end-to-end 2026-06-23 |
| 041 inbox-plan-surface | 🟡 iteration-1 + iteration-2 implemented and merged to `main` (72/80 tasks ticked) | iter-1 (confirm + plan surface + apply + destination model) shipped 59/59. iter-2 (single-type sub-items) merged to `main` via PR #349 (`6fcaa766`): code evidence for T071 (`bfddb736`, confirm split/mixed removed), T072 (`7991ac25`, contracts+bindings), T076 (`009da1b4`, session-review lifecycle dropped), T077 (`807b24bc`, plan_open legacy guard), and T081 (`8b566a62`/`f2de3243` — `crates/app/inbox/src/classify.rs:696,699` wires `raw.offset`/`raw.set_temp_c` into `FrameMetadata`, no longer hardcoded `None`) all exist on `main`, but `tasks.md` checkboxes for T071–T073/T076–T079/T081 are **not yet ticked** — a dedicated independent `speckit-verify` audit lane is reconciling the paper trail; do not tick or close related GH issues until it reports. Supersedes 005 |
| 042 stdlib-adoption | ✅ Implemented | 80/97; reconciled #310 |
| 043 ui-redesign-platevault | 🟡 Merged to `main` (PR #349, `6fcaa766`); remainder open | Foundation + per-page round-2 verified against code: 4-theme tokens + Appearance picker, shared `<SortHeader>`/`.alm-sorth`, flat-by-default `.alm-listgroup` on all 4 list pages (#360), `InfoTip`/`SettingsKit`, Inbox bottom inspector, `eslint no-restricted-syntax` style-ban wired into lint, Archive single-column + Sessions inbox-parity + `aria-sort` (`34e59139`/#415), platform-native reveal-labels sweep. Since the merge, several former STUBs were closed on `main`: offset Settings-toggle persistence (`9f0dc724`/#395), Targets list RA/Dec/constellation/magnitude enrichment (`50632d99`/#57), real altitude/moon/opposition (moved to and shipped by 044/047). Still `// STUB:`-marked and open: target↔project/session linkage blocked on task **#54** (`ProjectsTable.tsx:82,201`, `TargetDetailV2.tsx:17,20`), Outputs/Cleanup accepted-output backend model (`OutputsCleanupSections.tsx:80`), pill-system unification, resizable splitters, Settings per-pane polish. No committed `tasks.md` (spec.md only) — task-count tracking not applicable. |
| 044 targets-planner-astronomy | 🟡 Track B complete minus 2 deferred tasks (36/40) | Research-led astronomy engine track (astronomy-engine + Lorentzian filter model), merged to `main` via #349 and further PRs: real tonight altitude/rise-set/imaging-time (`a395ce93`/#436), observing-site management + first-run setup (`ceef2e1e`/#440), planner un-gated on the real site store (`4e5c3a4f`/#450), planner mock-E2E coverage (`c42e8404`/#454), and PR #499 (`1a0c4644`) completed the remainder — real Moon geometry + dark-window awareness for future-night planning, plus a 13k-row moon-geometry perf regression fix. Open: T017 (deferred — optional FITS-observer prefill), T036 (deferred — wire `@tanstack/react-table` sort/filter/group into `TargetsTable.tsx`); T038 (verify-on-windows) deferred to the campaign's Windows-validation lane; T039 is this SPEC_STATUS update |
| 045 review-state-real | 🔴 Superseded by 041 | |
| 046 i18n-error-codes | ✅ Implemented | 36/36 (#311–#314). #410 fixed an audit-detail i18n regression (raw backend text instead of translated message) inside this "Implemented" window (`5e05b349`) |
| 047 targets-planner-moon-filters | ✅ Implemented (T001–T027, T029; T028 verify-on-windows pending) | Track A of the planner split, merged to `main` (via #349, `6fcaa766`): real Moon summary (US1), real per-target lunar distance + sort (US2), real per-band Moon-avoidance filter guidance pills + explanation popover + Settings → Target Planner per-band table + filter-by-recommendation (US3), real next-opposition date + sort (US4, `0906728f`/#430). All former spec 044 §3 mock symbols (`MOCK_MOON_PHASE_FRAC`, `mockLunarDistanceDegFor`, `filtersFor`) deleted. Perf-optimized opposition scan validated at 5,000 rows. T028 (verify-on-windows) deferred to the campaign's Windows-validation lane |
| 048 per-frame-inventory | ✅ 33/44 tasks — reconciliation note 2026-07-19, issue #764 | Real code on `main`: session frame counts + disk usage (`5dc00c90`/#435), raw-frame-vs-disk reconciliation + symlink-gated scans (`a70db417`/#442), an accurate per-frame-type destination-pattern preview (`58ccfcbd`/#390), a real-UI journey (`c526dc10`/#470), and PR #517 (`0463bbd2`, lane D/nD) — reconcile wiring + a settings-edits-silently-reverted fix + a manual reconcile action. `048-complete-per-frame-inventory` (PR #500) and `048-us5-calibration-missing-flag` (PR #503) — previously listed open — are both **merged** (2026-07-12/13), along with PR #507; `tasks.md` now shows 33/44 checked (11 open, largely deferred/Windows items), not the 15/44 this row previously claimed. |
| 049 source-view-generation | 🟡 41/46 — small remainder open | WBPP-ready source views with zero-copy links (US1, `54b56d28`/#439) and profile-driven source-view layout (US2, `51a0a64d`/#443) merged to `main`; a real-UI journey landed (`c526dc10`/#470) alongside a related sessions `root_id` persistence fix (`1da1e3f2`/#480). PR #535 (`de28d844`, lane nF) closed the US4 verify chain end-to-end: contract + read-only `VerifySourceView` use case (`crates/app/projects/src/source_view_verify.rs`) + DTOs + `sourceview_verify` command (`apps/desktop/src-tauri/src/commands/prepared_views.rs`) + UI action, a Settings → Source Views pane (`apps/desktop/src/features/settings/SourceViews.tsx`), per-project/per-generation destination overrides, and a Windows long-path warning (all in `source_view_generate.rs`). Open (5): T032b is n/a (documented — spec 026's regen has no "added item" path to test, out of scope here); **T031 parked with an honest blocked-on-external-048 note** (per-frame selection integration needs spec 048's `inventory.frame.*` contract, which isn't merged yet — PRs #500/#503/#507 open); T043 (lint/typecheck/per-crate test pass — note its own `workspace-wide test is red on main` caveat is now **stale**, see lane A's finding above); T044 (docs cross-link into spec 026, feeds lane nG); T045 (Windows real-app verify, deferred to the campaign's Windows-validation lane). The previously-tracked "materialization-summary UX" deferred item was re-verified as already covered by #490 |
| 050 publishable-crate-extractions | 📄 Plan-of-record | Mini specs for the FITS/XISF publishable-crate extraction program; landed via #429 (docs-only, plan-of-record for a future extraction effort), merged into `main` |
| 051 tauri-shell-integration | 🟡 Partial | 34/64 tasks ticked (undercounts — several merged tasks, e.g. T001–T009, aren't checked off). US1 single-instance guard (`64a94881`/#471), US2 favourites-in-DB (`54378086`/#472), US3 cleanup-overrides-in-DB (`c990f967`/#474), US4 window-state persistence + US5 native menu bar + US7 diagnostics log file (`e9b1622b`/#476), US6 native theme sync (`29617775`/#475), and US10 signed-update groundwork (`c1f3ede9`/#473, updater key fix `a33dc427`, build/sign pipeline `60732f2f`/#469) are all merged to `main`. **Not yet implemented**: US8 OS notifications on long-task completion (no `tauri_plugin_notification` registration found in `apps/desktop/src-tauri/src/lib.rs`) and US9 release-build native behavior / reload-guard (no `prevent_default` plugin registration found) — both real open scope, not just unticked boxes |
| 052 simbad-caching-dual-lookup-cone-search | ✅ Implemented | Row added 2026-08-24 (the spec shipped 2026-07-28 and was never indexed here). Resolver facade, dual TAP/Sesame lookup, oid recovery, normalize choke-point, seed-builder `v_mag` column, cache-clear command and cone search all ship: `crates/app/inbox/src/cone_search.rs`, `apps/desktop/src-tauri/src/commands/target_lookup.rs`, `apps/desktop/src/features/inbox/ConeSearchSuggestions.tsx`. Task state lives in beads; the legacy task file was retired rather than reconciled. Outstanding speckit-verify run `astro-plan-n6zv`; Windows verification `astro-plan-ltoy` |
| 053 frame-type-suggestion | ⚪ Not started | Row added 2026-08-24. Backlog stub only — `speckit.specify` define-only, `spec.md` is the sole artifact (no plan, research, data-model or contracts). Zero implementation: `git grep -lniE 'frame_?type_?suggest\|suggestFrameType\|frame-type suggestion' -- crates/ apps/ packages/` returns nothing. Supporting research is complete at `docs/research/078-heuristic-frame-type-suggestion.md`. Tracked in `astro-plan-221lo` |
| 054 adaptive-detail-dock | 🟡 Delivered (partial) | Row added 2026-08-24. Implemented directly on `main` via #1003 per the spec's own reconciliation note: adaptive placement (`apps/desktop/src/ui/useAdaptiveDock.ts`), drag-resize and the 3-state placement control (`apps/desktop/src/components/DetailDockPlacementControl.tsx`, `apps/desktop/src/components/ListPageLayout.tsx`), full `DetailPanel` adoption. Still open per that note: Targets pinned columns / conditional horizontal scroll (T024/T025) and typed `AppPreferences` persistence, neither with a tracked issue |
| 055 typography-rework | ✅ Implemented | Row added 2026-08-24. FR-001 self-hosted hinted Inter statics (`apps/desktop/src/assets/fonts/Inter-*.woff2`, CDN loads gone) and FR-006 whole-app engine zoom (`apps/desktop/src/data/theme.ts`, `apps/desktop/src/data/theme.zoom.test.ts`, Settings control in `apps/desktop/src/features/settings/General.tsx`). The spec's own `Status:` line carried a truncated fragment ("and task generation") until 2026-08-24 |
| 056 onboarding-redesign | ✅ Implemented | Successor to 010. 15 frontend modules + backend (`crates/app/core/src/onboarding.rs`, `apps/desktop/src-tauri/src/commands/onboarding.rs`), tick subscriber wired at `apps/desktop/src-tauri/src/lib.rs:679`, 6 e2e specs under `tests/e2e/onboarding_*.spec.ts` |
| 057 branding-splash | 🟡 Partial, US3 not built | Row added 2026-08-24. US1 splash window ships (`apps/desktop/src/splash/main.ts`, `apps/desktop/splash.html`, boot-ready handshake in `apps/desktop/src-tauri/src/lib.rs`) and app icons are regenerated (`apps/desktop/src-tauri/icons/`), but two spec claims were false and are corrected in the spec header: there is **no** 800 ms minimum-display floor (`apps/desktop/src/splash/main.ts:13`), so US1 scenario 2 and SC-002 are unmet; and US3's switchable variant system plus live startup-status line does not exist (`git grep -lniE 'splash.?variant\|SplashVariant\|mark.only\|live.?status' -- apps/ crates/` exits 1; `pv-status-text` is static text at `apps/desktop/splash.html:212`). Both are product decisions, not filed work — `astro-plan-yzhd9` |
| 058 inbox-drop-parent-items | 📄 Specified (2026-07-19) | Full SpecKit trail exists (spec, plan, research, data-model, contracts, tasks, checklists). Drops the folder-placeholder ("parent") inbox item. A folder yields one item when homogeneous and N sibling items when mixed. Siblings are linked by `source_group_id`; a split breaks lifecycle linkage. Root-causes #711 and supersedes the read-side patches #1038 / #1081. Greenfield by product-owner decision, with no migration. Accepted risk: existing v0.5.0 databases strand open inbox plans. 9 questions pending review (`specs/058-inbox-drop-parent-items/PENDING_REVIEW_QUESTIONS.md`) |
| 061 selectable-app-language | ✅ Implemented | Two-locale (en-GB, pt-BR) picker in Settings General and as the setup wizard's first step (`apps/desktop/src/features/setup/steps/StepLanguage.tsx`); `tests/e2e/setup_wizard_language_step.spec.ts` |
| 062 session-heterogeneity | 🟡 Partial, US2 not built | US1/US3/US4 shipped. US2 has no backend: the 15 commands invoked from `apps/desktop/src/features/sessions/sessionsGroupsIpc.ts` (`panel_group_*`, `mosaic_*`, `relation_proposal_*`, `matching_settings_*`) have no handlers. Each name appears in that one file and nowhere else in `apps/`, `crates/`, or `packages/` |
| tiny/ catalog-entry, settings-key, targets-tanstack-query, reclassify-split | 📄 Micro-specs | reference notes, not tracked features. `reclassify-split-per-item-and-rederivation` is the target architecture spec 058 descopes (Q-6): per-item reclassify as the only user-facing classification action, folder re-derivation as a separate never-blocking identity operation, plan interlock retired by irrelevance, supersede-and-surface via event inversion. Blocked on 058 Q-5. |

## Dependency DAG

```
FOUNDATION (all ✅ — nothing blocked here)
  022 mantine ─▶ 027 frontend ─▶ 029 tauri-wiring ─▶ 032 design-v4
  002 lifecycle ✅   020 router ✅   030/031 (superseded/closed)

INBOX CHAIN
  005 mixed-folder 🔴 ─▶ 041 inbox-plan-surface ✅ ─┬─▶ 039 cross-root-inbox 🔴 (superseded, fully implemented via 041)
  038 wizard-scan ✅                                ├─▶ 025 plan-application 🟡 (closeout-ready, 52/59 via #589)
  016 protection ✅ ─▶ 017 cleanup/archive 🟡 ──────┼─▶ 033 validation-bugfix 🟡 (closeout-ready, 92/96 via #561)
                                                    └───┘

TARGETS CHAIN
  013 fits-lookup 🟡 ┐
  014 catalog 🔴 ────┴─▶ 035 SIMBAD ✅ ─┬─▶ 036 retire-legacy ✅
                                        ├─▶ 023 target-identity ✅ (closed, gen-3 delivery)
                                        └─▶ 006 sessions ✅ ─▶ 044 planner-astronomy 🔵 (Track B in-flight on main, lane B active) ─▶ 047 moon-filters ✅ (merged to main via #349)

CALIBRATION CHAIN
  006 inventory/sessions ✅ ─▶ 007 matching-rules ✅ ─▶ 040 masters ✅

PROJECTS CHAIN
  006 inventory ─▶ 008 project-create 🟡 ─▶ 009 lifecycle ✅ ─▶ 010 guided-flow 🔴 ─▶ 056 onboarding-redesign 📄
                       └─▶ 024 manifests/notes ✅
                  011 tool-launch ✅ ─▶ 012 artifact-observation 🟡

INFRA / CROSS-CUTTING (mostly independent)
  018 settings ✅   021 dev-diagnostics 🟡   019 log-viewer ✅
  046 i18n ✅   042 stdlib ✅   043 ui-redesign 🟡 (merged to main via #349; remainder open)
  037 e2e ✅ ◀── Layer-2 tauri-driver journeys merged (#403); closed out via #531 (35/40, 5 open by design as SUPERSEDED)
  037 ipc-removal ✅ (all phases done+merged; commands.ts deleted, guards in CI)
  026 source-view-removal ✅ (23/23, closed via #545)
  048 per-frame-inventory ✅ (33/44, #500/#503/#507 merged — issue #764)   049 source-view-generation 🟡 (41/46, T031 blocked on 048)
  050 publishable-crate-extractions 📄 (plan-of-record, PR #429)
  051 tauri-shell-integration 🟡 (US1–US7 + US10-groundwork merged; US8 notifications + US9 release-native-behavior open)
```

## Actionable frontier — what can be worked on now (unblocked)

**Post-campaign refresh (2026-07-11):** the release-finish campaign closed
out 017, 025, 012, 008, 021, 007's contract-test gap, 033, 037, and 026 (see
their rows above). The frontier below reflects what's left after that.

| Priority | Spec | Why ready | Size |
|---|---|---|---|
| 1 | **025 T012** (resume-executor gap, issue #575) | Product decision pending: release-blocker vs known limitation — `resume_plan` doesn't re-spawn the executor for pending items | 🟡 Medium (real production fix, not just a test) |
| 2 | **008 US1-5** (guided-flow duplication, issue #586) | `CreateProjectDialog` validation logic duplicated instead of reused by spec 010's guided flow | 🟢 Small |
| 3 | **050 publishable-crate-extractions** | Plan-of-record (#429); all extraction tasks unblocked but unstarted | 🟡 Medium, multi-crate |
| active | **043 ui-redesign** | Merged to `main`; remainder (target↔project linkage #54, Outputs/Cleanup backend model, pill-system, splitters) unblocked | 🟡 Small–Medium remainder |
| active | **051 US8/US9** | Shell-integration foundation (US1–US7, US10-groundwork) merged; notifications + release-native-behavior plugins not yet registered | 🟡 Small–Medium |
| parked | **049 T031** + **048 remainder** | Parked on external session's open PRs #500/#503/#507 (per-frame inventory not yet on `main`) | 🟡 Small once unblocked |
| parked | **044 T017** | Optional FITS-observer prefill, pending a session-observer IPC binding | 🟢 Small |
| watch | **#489 macOS Real-UI** | Upstream `tauri-plugin-webdriver` limitation; `workflow_dispatch` re-test path in place (#528), no fix owned here | — |
| watch | **025 T025** (rollback-audit coverage, issue #577) | Genuinely untested — needs a real cross-device `EXDEV` error, not producible in an ordinary tempdir | 🟡 Small once a test seam exists |

**Suggested parallel lanes:** one engineer on 025's #575 (needs a design call
first — see Open Watch Items below) + 039; another on 043's remainder +
051's US8/US9; 050's crate extractions are a good next-campaign candidate.

**Versioning note (2026-07-09):** the release baseline was reset to 0.x
(`cbd91378`) ahead of the first release cut. This document and its frontier
are unaffected — no spec here tracks or blocks on version numbers, and this
lane does not touch `.github/workflows/**`, tags, or release PRs.

## Closeout-ready (verify pass, not new work)

**Closed 2026-07-03:** 006, 007, 011, 019 flipped to Implemented after code-verified closeout (deferred tails documented; 011 T021 + 019 T006/T011/T029 + the `log_recent` bug done this session). 024 was closed earlier via #357 (2026-06-23). **026** closed 2026-07-11 via #545 (23/23) — the "vestigial" premise was resolved by spec 049 restoring a live generation path. No verify-flip work remains in this group.

## Blocked / not-yet-actionable

- **033 validation-bugfix** — no longer blocked: 017's remainder shipped (#492) and #561 closed the real-backend e2e gaps; see the 033 row. No longer a blocked/not-yet-actionable item.
- **037 e2e** — closed out via #531; see the 037 row. No longer a blocked/not-yet-actionable item.
- **044 planner-astronomy** — Track B (astronomy-engine) complete minus 2 deferred tasks; see the 044 row.

## Deviations of record

- **PR #411 mkdir-only plan auto-apply (constitution II).** Creating a project now
  auto-applies folder-creation-only plans instead of requiring an explicit review
  click. This is a user-approved deviation from constitution Principle II
  ("plan application MUST be explicit"), decided **2026-07-04**, superseding
  handover decision D16. Reviewability-as-record is preserved: the plan row,
  a `plan.approved` audit event (actor `auto.mkdir_only`), and per-item apply
  audit records are still written for every automatic application; only the
  manual approval click is skipped, and only for plans whose actions are
  exclusively `mkdir` (+ the app-owned `write_manifest` marker). Any plan
  containing a user-file action (move, copy, link, delete, archive, trash,
  catalogue, or unknown) keeps the explicit review flow unchanged. See the
  commit's own DEVIATION NOTE (`4162435d`, #411): "'plan application MUST be
  explicit' is relaxed for mkdir-only plans by explicit user decision of
  2026-07-04; the fuller spec amendment rides the campaign amendment set."

## Known repo-health issues (2026-06-23)

- **Fixed 2026-06-23 (PR #346):** the `Clippy (workspace, deny warnings)` CI red —
  `crates/app/targets/src/target_management.rs:506` `clippy::unnecessary_map_or`
  (`map_or(false, …)` → `is_some_and(…)`). Workspace `cargo clippy --all-targets -- -D warnings` now green.
- **Fixed 2026-06-23 (PR #317):** duplicate migration version `0046`
  (`session_canonical_target` + `target_constellation_magnitude`) broke fresh-install
  startup and every real-backend integration test. The later file was renumbered to `0047`.
  Watch for this class of collision when concurrent branches pick the next migration number.
- **Coordination (2026-06-23):** the `041-single-type-impl` / `041-single-type-ingest` branches
  (iteration-2, in progress by another agent) still carry the **old duplicate `0046` pair** — they
  predate PR #317 and must be rebased onto current `main` before adding a single-type migration,
  which should take the next free number (≥0048).

## Artifact drift audit (2026-06-23)

Per-spec review of plan/research/data-model/contracts/tasks vs shipped code.

### ✅ Resolved (PRs #343 / #344)
- **`os_trash` "code bug" was a false alarm** — the flagged `crates/app/core/src/inbox/confirm.rs`
  was **dead code** (orphaned pre-042-split copy; `app_core::inbox` re-exports `app_core_inbox`).
  The live `crates/app/inbox/src/confirm.rs` already uses `archive | trash` and is tested. The dead
  file was **deleted** (#343). No live bug ever existed.
- **017 destination enums** (`plan.get.json`, `plan.list.json`, `data-model.md`) `archive | os_trash`
  → `archive | trash` (#343). *Error-code strings `os_trash.*` were left intact — they match the live
  executor `crates/fs/executor/src/failure.rs` and are NOT drift.*
- **013** STALE/SUPERSEDED banners on `tasks.md` + `research.md` (#344).
- **023** reconcile banner + status flip (retired gen-2 model, nav reversed) (#344).
- **002** research §6.2 `catalog.download.*` SUPERSEDED banner (#344).
- **035** `plan.md` migration filename corrected (#344).
- **040** artifact-completeness deviation recorded in spec.md (#344).
- **0047** migration's stale internal `Migration 0046:` comment fixed (#341).
- **Clippy CI red** fixed (#346) — see repo-health above.
- **008 contract drift** closed (#346): `project.create.json` gained optional `canonicalTargetId`;
  `project.source.add.json` gained optional `role` (`light|dark|flat|bias`) + `selection`
  (`selected|candidate`), matching `SourceRole`/`SourceSelection` (`crates/contracts/core/src/projects_v2.rs`).
- **006 `noop` enum + 007 `mismatchedDimensions`** — re-verified **already correct**; the audit over-flagged
  them (no change needed).
- **Prose `os_trash`** destination-value mentions in 016 (spec/plan/research) + 025 (research) → `trash` (#346).
- **002** session-lifecycle-historical note added to spec.md near the supersession notice (#346).
- **024** uncontracted commands (`project.note.get`, `project.manifest.reveal_in_os`) recorded as deferred
  tasks TX.11/TX.12 (#346).

### ⬜ Still open
- 002/007 minor: `confidence` placement / `canonicalTargetId` on the session DTO contract (cosmetic; verify
  on next contract regen).
- 023 — full re-scope onto gen-3 + 035 (banner added; rewrite pending — needs product input on scope).
- 024 — author the two deferred JSON contracts (low priority).
- 041 `contracts/operations.md:60` `os_trash` prose — left to the active single-type agent.
- The live confirm path is `crates/app/inbox`; an earlier duplicate in `app_core` was removed — watch for
  other dead pre-042-split copies if similar drift appears.
- **2026-07-03 — mock-mode E2E green again (PR #364):** `lifecycle_detail.spec.ts` asserted on the
  pre-redesign `.alm-sessions-table__group` header, but #360 renamed it to `.alm-listgroup` AND made
  Sessions flat-by-default (no group rows unless grouped), so both tests failed. Fixed to assert on
  `.alm-sessions-table__row`; full mock-mode Playwright suite now 9 passed / 1 skipped.
- **CI test-disable (#356) — RESOLVED 2026-07-03.** The blanket `if: false` disable of all test
  jobs was removed and CI re-enabled on `redesign-ui-platevault` (commit `9a6c49a4`); tests are green
  (974 frontend vitest, `app_core_targets` 79). Pushed via `gh`'s `workflow`-scoped token
  (`gh auth setup-git` — git's stored OAuth credential lacked the scope).
