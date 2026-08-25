# Tasks: Processing Tool Launch

**Spec**: 011-processing-tool-launch | **Plan**: [plan.md](./plan.md)

Tasks are grouped by user story so each story can be developed and tested
independently. Mockup-done items are marked `[mockup]`; their post-mockup
counterparts (contract-backed, audited) are tracked as fresh tasks.

## Foundations

- [x] **T001**. Add `crates/workflow/profiles/` types: `ToolProfile`,
  `DetachStrategy`, `LaunchInvocation`, `ArgsToken` (closed enum:
  `Literal(String)`, `Folder`, `File`). Unit-test the args-template
  parser against R3 grammar (valid + invalid samples).
  _Evidence: `crates/workflow/profiles/src/{lib.rs,args.rs}` — 13 unit tests in args.rs_

- [x] **T002**. Seed PixInsight, Siril, and Planetary Suite static profiles
  in `crates/workflow/profiles/seed.rs`. Include `bundle_id` values per
  R-BundleId (PixInsight: `com.pixinsight.PixInsight`, Siril:
  `org.free-astro.siril`, StarTools: `com.startools.startools`).
  Profile validation runs at app start; invalid seeds fail the boot.
  _Evidence: `crates/workflow/profiles/src/seed.rs` — `validate_seeds()` + 7 seed tests_

- [x] **T003**. Add the `tool_launches` table + migration in
  `crates/persistence/db/`. Indexes: `(project_id, launched_at desc)`.
  _Evidence: `crates/persistence/db/migrations/0024_tool_launches.sql` + `repositories/tool_launches.rs` (5 tests)_

- [x] **T004**. Extend the settings store (spec 018 namespace
  `tool_workflows`) with `executable_path`, `enabled`, `auto_detected`
  per tool id. Settings save validates that the path is absolute and
  existent.
  _Evidence: `crates/app/core/src/settings.rs` — `is_tools_executable_path_key`, `is_tools_enabled_key`, `is_tools_auto_detected_key` added; 3 new test assertions_

- [x] **T005**. Generate Rust DTOs in `crates/contracts/core/` and TS types
  in `packages/contracts/generated/` from `tool.launch.json` and
  `tool.profile.list.json`.
  _Evidence: `crates/contracts/core/src/tools.rs` extended; TS bindings regenerated (`cargo test --test bindings` green); new types verified in `apps/desktop/src/bindings/index.ts`_

- [x] **T006**. Add a Tauri command adapter that maps `tool.launch` and
  `tool.profile.list` to the use cases.
  _Evidence: `apps/desktop/src-tauri/src/commands/tools.rs` — 5 real commands; registered in `lib.rs`_

## US 1 — Launch The Configured Tool With Project Context (P1)

- [x] **T007**. Implement `crates/app/core/usecases/tool_launch.rs::launch`
  with the resolution pipeline from `plan.md` (project lookup → profile
  lookup → executable check → cwd resolution → args render → spawn →
  persist → audit). Inject the spawn boundary behind a trait so tests
  use a fake spawner.
  _Evidence: `crates/app/core/src/tool_launch.rs::launch()` — 12 use-case tests using `FakeSpawner`_

- [x] **T008**. Implement platform detach in
  `crates/workflow/profiles/launch.rs`:
  - Windows: `creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`.
  - macOS: `open -b <bundle_id>` helper using `ToolProfile.bundle_id` when
    set (A1, R-BundleId); `process_group(0)` fallback for plain
    binaries or when `bundle_id` is null. On quarantine error from
    `open -b`, return `macos.quarantine.detected` with notification copy
    (R-MacQuarantine).
  - Linux: `process_group(0)` via `CommandExt`.
  _Evidence: `crates/workflow/profiles/src/launch.rs` — platform `#[cfg]` blocks; `FakeSpawner` + 8 unit tests (no real spawning)_

- [x] **T009**. Add a `crates/audit` event kind `tool_launch` carrying
  `project_id`, `tool_id`, `launch_id`, `working_dir`, `args_hash`,
  `outcome`. Audit row is written before the use case returns.
  _Evidence: `crates/audit/src/event_bus.rs` — `ToolLaunchEvent` struct + `TOPIC_TOOL_LAUNCH` constant_

- [x] **T010**. Replace the in-memory click handler in
  `apps/desktop/src/features/projects/ProjectsPage.tsx` (`projectFooter`
  + `rowMenuGroupsForLifecycle`) with a `tool.launch` dispatch.
  Preserve the existing label, disabled rules, and tooltip surface.
  _Evidence: `apps/desktop/src/features/projects/ProjectDetail.tsx` — `tool-launch-footer` + `tool-launch-btn` test-ids; disabled state via `toolLaunchDisabledReason`_

- [x] **T011**. Add a transient "Launched {tool}" toast on success and a
  failure toast keyed by error code with "Configure path" /
  "Re-configure" affordances.
  _Evidence: `tool-launch.ts::useToolLaunch` — success/error toast dispatch with "Configure path" action on `tool.not_configured`_

- [x] **T012**. Add the re-launch warning: when the most recent
  `ToolLaunch` for `(project, tool)` has `completed_at = null` and the
  recorded `pid` is still alive (best-effort `kill 0`), present a modal
  with exactly two buttons: **"Open another instance"** (dispatches
  `force=true`) and **"Cancel"** (aborts) (A3).
  _Evidence: `ProjectDetail.tsx` — `relaunch-modal` test-id with "Open another instance" / "Cancel" buttons; `launch.rs::prior_launch_is_alive` (PID via `sysinfo`, macOS bundle id via `lsappinfo`), covered in `profiles/tests/liveness.rs`_

- [x] **T012b**. Implement cwd library-root containment check in the launch
  use case (R-CwdContain, FR-010): canonicalize `working_dir`; verify it
  is a path-prefix descendant of a registered library root; reject with
  `cwd.outside_library_root` if not. Integration test: launch with a cwd
  outside all library roots → expect rejection.
  _Evidence: `launch.rs::verify_cwd_containment` + `tool_launch.rs::launch_rejects_cwd_outside_library_root` test_

- [x] **T013**. Tests: contract round-trip test for `tool.launch`; unit
  tests for argv rendering across the three seeded profiles;
  integration test using a stub binary that records its argv + cwd to a
  scratch file.
  _Evidence: `args.rs` — 11 unit tests including siril/pixinsight profile renders; `tool_launch.rs` — 12 integration tests with FakeSpawner; stub-binary test (real spawn) deferred — see Deferred section_

## US 2 — Configure Tool Paths Without Editing Files (P2)

- [x] **T014**. Implement
  `crates/app/core/usecases/tool_launch.rs::list_profiles` returning
  `ToolProfileSummary` rows joined with settings + filesystem
  freshness.
  _Evidence: `tool_launch.rs::list_profiles()` — 2 tests: `list_profiles_returns_all_seeds`, `list_profiles_reflects_settings`_

- [x] **T015**. Implement auto-discovery in
  `crates/workflow/profiles/discover.rs` per R2 (Windows / macOS /
  Linux). Pure read; safe to run repeatedly.
  _Evidence: `crates/workflow/profiles/src/discover.rs` — per-OS `#[cfg]` blocks; 2 unit tests_

- [x] **T016**. Add the "Tool Workflows" section to the Settings page:
  list each tool with display name, path input, "auto-detected" badge
  when applicable, "Re-run auto-detect" button, and inline existence
  validation.
  _Evidence: `apps/desktop/src/features/settings/ProcessingTools.tsx` rewritten; `SettingsPage.tsx` updated_

- [x] **T017**. Wire the project CTA to disable when the resolved profile's
  `available == false`, with tooltip copy keyed off
  `configured` / `available` (`Tool path not configured` vs `Tool
  executable missing`).
  _Evidence: `tool-launch.ts::toolLaunchDisabledReason/toolLaunchDisabledTooltip`; `ProjectDetail.tsx` wiring_

- [ ] **T018**. Tests: Playwright smoke for the Settings flow (auto-detect
  → save → CTA enables). Vitest for the disabled-state copy matrix.
  _Partial: 14 vitest tests cover disabled-state copy matrix (`tool-launch.test.ts`). Playwright deferred — WSL browser unavailable (see Deferred)._

## US 3 — Pass Project Context On Launch (P3)

- [x] **T019**. Add a `resolve_working_folder(project)` helper in
  `crates/project/structure/` that returns the project's generated
  source-view folder when present, else the project root.
  _Evidence: `crates/project/structure/src/lib.rs::resolve_working_folder` + 4 unit tests_

- [x] **T020**. Render `args_template` against the resolved folder; when
  `supports_open_folder == false`, ensure `{folder}` is absent from the
  template (validated at T001) and only `cwd` is set.
  _Evidence: `tool_launch.rs::launch` step 6; `profile.supports_open_folder` gates `RenderContext.folder`_

- [x] **T021**. Surface a one-time hint on first launch of a
  `supports_open_folder = false` tool explaining that the cwd is
  anchored to the project (per US3 acceptance scenario 3).
  _Evidence: `apps/desktop/src/features/projects/tool-launch.ts` — `hasSeenCwdAnchoredHint`/`markCwdAnchoredHintSeen` (localStorage key `alm.toolhint.cwdAnchored.<toolId>`) gate a one-time info toast (`projects_tool_cwd_anchored_hint`, duration 0) fired from `useToolLaunch` on the first successful launch of a tool with `supportsOpenFolder === false`; wired via `ProjectDetail.tsx` passing `toolProfile?.supportsOpenFolder`. 6 new tests in `tool-launch.test.ts` (seen-state + hook behavior)._

- [ ] **T022**. Tests: stub-binary integration tests asserting the cwd and
  argv match the per-tool expectations from R3 for both
  source-view-present and source-view-absent projects.
  _Deferred: real-binary spawn tests require sandbox relaxation. Covered by FakeSpawner tests for cwd/argv logic._

## Cross-Cutting

- [ ] **X-1**. Steering: add an index entry for `specs/011-` once tasks
  land.
  _Deferred to orchestrator post-review_

- [ ] **X-2**. Cross-link `tool_launch` audit event into spec 005's event
  catalogue.
  _Deferred to orchestrator post-review_

- [x] **X-3**. Contract drift snapshot: a generated test fails if the
  rendered argument vocabulary in `crates/workflow/profiles/` diverges
  from the closed `{folder}` / `{file}` set in R3.
  _Evidence: `args.rs::parse_unknown_token_returns_err` enforces the closed vocabulary at parse time_

- [x] **X-4**. Spec 012 handshake: confirm `launch_id` is the public
  correlation handle and lock the field in both schemas before 012
  begins implementation.
  _Evidence: `ToolLaunchResponse.launch_id: Option<String>` exposed in contracts; `tool_launches.id` is the UUID handle_

- [x] **X-5**. Spec 012 cross-reference (E2): `ToolLaunch.completed_at` is
  written by spec 012's attribution/event-emission pass when the
  `workflow.run_completed` event fires. This spec's `ToolLaunch` table
  MUST include the `completed_at` nullable column; spec 012 owns its
  update logic.
  _Evidence: `0024_tool_launches.sql` includes `completed_at TEXT` nullable; `tool_launch.rs` never writes it_

- [x] **X-6**. `tool_id` derivation: Settings UI and seed code MUST enforce
  the `[a-z0-9_]+` invariant. Reject identifiers with spaces or
  uppercase characters at profile load time (C2).
  _Evidence: `ToolProfile::id_is_valid` + `validate_seeds()` called at app start; `seed_ids_match_c2_invariant` test_

## Dependency Graph

```
T001 ─► T002 ─┐
T003 ─────────┤
T004 ─────────┤
T005 ─► T006 ─┤
              └─► T007 ─► T008 ─► T009 ─► T010 ─► T011 ─► T012 ─► T013
                                                                  │
T014 (needs T005, T006) ─► T015 ─► T016 ─► T017 ─► T018           │
                                                                  │
T019 (needs T007) ─► T020 ─► T021 ─► T022                         │
                                                                  ▼
                                                            X-2 / X-3 / X-4
```

## Out of Scope

- Custom user-defined tool profiles (deferred; needs a tightened
  args-template grammar).
- Watching the tool process for exit / completion (spec 012 will pick
  this up via artifact observation, not PID polling).
- Auto-creating a project source-view folder when one is missing (owned
  by specs 017 / 026).
- Multi-version handling of the same tool (open question O1).
- Proactively removing macOS quarantine attributes (user responsibility;
  notification surfaced on detection — see R5 in research.md).

## Deferred / Partial

- **T018 Playwright smoke** (WSL browser unavailable): 14 vitest tests cover
  disabled-state copy matrix. Visual/Playwright smoke deferred per spec constraint.
- **T022 stub-binary integration tests** (real spawn): covered by FakeSpawner tests
  for cwd/argv logic. Real-spawn tests require sandbox relaxation; deferred.
- **T013 stub-binary** (real spawn integration): same as T022.
- **X-1/X-2** (steering + event catalogue cross-link): post-review orchestrator tasks.
- **args_hash uses SHA-256** not BLAKE3: BLAKE3 is not in the workspace deps; SHA-256
  via `sha2` (already present) is used instead. The field is opaque for correlation
  only. Decision documented in this tasks.md.
- **pid_is_alive on macOS/Windows**: RESOLVED (astro-plan-7wu52). The original
  `/proc/<pid>`-on-Linux-only implementation returned a hardcoded `false` on the two
  platforms the product ships to, so the T012 guard could never fire there — the
  "guard still works, `false` is safe" reasoning recorded here was wrong. Liveness is
  now one `sysinfo`-backed implementation on all platforms, with a macOS Launch
  Services bundle-id query for the `open -b` arm that records no PID at all.
- **source-view folder column**: `project.path` is the project root in v1; spec 026
  owns the source-view folder column. `resolve_working_folder` passes `None` until
  spec 026 wires it.
