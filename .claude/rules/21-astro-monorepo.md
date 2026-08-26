# Astro Monorepo Structure

The repository is a monorepo with a Tauri/React desktop app at the edge,
language-neutral contracts in `packages/contracts`, and granular Rust crates for
domain, metadata, filesystem, lifecycle, audit, and persistence boundaries.

Primary paths:

- `apps/desktop/`: Tauri + React desktop shell.
- `crates/domain/core/`: pure domain types and invariants.
- `crates/targeting/`: target identity, query normalization, coordinate
  resolution, and skymath re-exports.
- `crates/targeting/resolver/`: SIMBAD-backed target resolution with bundled
  seed index and local SQLite cache.
- `crates/sessions/`: acquisition and calibration session concepts.
- `crates/calibration/core/`: calibration matching and reuse policy model.
- `crates/calibration/master-detect/`: heuristic detection of calibration
  master files (STACKCNT/NCOMBINE headers, tool-specific patterns).
- `crates/workflow/profiles/`: processing tool/workflow profile model, seeded
  tool profiles, `{folder}`/`{file}` args-template rendering, per-OS executable
  discovery, and the `ProcessSpawner` detach seam (spec 011).
- `crates/workflow/artifacts/`: workflow artifact observation — rule-driven
  classifier, debounce watcher, reconciler, and tool-launch attribution.
- `crates/project/structure/`: app-owned project envelope rules.
- `crates/patterns/`: shared token-pattern parser and resolver (consumed by
  Inbox confirm pipeline, archive plans, and project source views).
- `crates/fs/inventory/`: filesystem scan records and root/path model.
- `crates/fs/executor/`: filesystem plan apply executor — per-item operation
  loop, failure taxonomy, rollback hooks, cancellation, and pause/resume.
- `crates/fs/pathsafe/`: reparse-aware symlink/junction detection and safe
  directory walkers shared by `fs_inventory` and `fs_executor`.
- `crates/metadata/core/`: shared extracted metadata model.
- `crates/metadata/fits/`: FITS metadata extraction adapter boundary.
- `crates/metadata/xisf/`: XISF metadata extraction adapter boundary.
- `crates/metadata/video/`: planetary/lunar video metadata adapter boundary.
- `crates/audit-types/`: pure audit event, envelope, and publisher-seam types —
  no SQL, no runtime. Split from `audit` to break the `persistence_db`
  dependency cycle; `audit` re-exports everything here.
- `crates/audit/`: audit event bus and `EventBus` runtime wiring.
- `crates/safe-filename/`: cross-platform path-segment sanitizer (bidi/homoglyph
  aware, Windows reserved names rejected on all platforms).
- `crates/persistence/core/`: Database, pool, WAL, migrations, shared
  audit-write primitives.
- `crates/persistence/calibration/`: calibration equipment, assignments,
  tolerances, calibration queries.
- `crates/persistence/inbox/`: inbox items, classification, metadata, source
  groups.
- `crates/persistence/lifecycle/`: lifecycle transitions, provenance, settings,
  onboarding, audit events, first-run.
- `crates/persistence/plans/`: plans, plan-apply, projects, artifacts, manifests,
  source views.
- `crates/persistence/targets/`: targets, framing, inventory, resolver queries,
  target-management queries.
- `crates/contracts/core/`: Rust contract DTO boundary.
- `crates/app/core/`: application use-case orchestration (Tauri command
  handlers and cross-domain coordinator).
- `crates/app/cache/`: shared in-process `moka`-backed cache primitives for
  app-layer debouncing and memoization.
- `crates/app/calibration/`: calibration matching and equipment use cases.
- `crates/app/errors/`: canonical DB-to-contract error conversion helpers.
- `crates/app/inbox/`: inbox use cases (mixed-folder split, content signatures).
- `crates/app/lifecycle/`: lifecycle transition, ledger, provenance, and
  artifact use cases.
- `crates/app/projects/`: project create/update, health, manifests, notes, and
  prepared-view use cases.
- `crates/app/settings/`: settings read/write use cases.
- `crates/app/targets/`: target catalog, resolution, search, and
  ingest-resolution use cases.
- `packages/contracts/`: language-neutral schemas and generated TypeScript
  surface.
- `docs/` and `docs/research/`: project documentation and technology/domain
  research.
- `specs/`: SpecKit feature artifacts.
- `tests/`: cross-crate and end-to-end tests.

Contracts are source-of-truth artifacts for future UI-to-core and possible
remote backend transport. Concrete schemas are produced during planning.

SQLite is the canonical local store for metadata, relationships, rules,
lifecycle, plans, and audit history. JSON Schema based operation contracts form
the transport boundary, with Tauri as the first adapter.

Tool launches are detached: the app spawns PixInsight, Siril, or a planetary
tool and never supervises or kills the process (constitution III). Launch rows
in `crates/app/core/src/tool_launch.rs` record `pid`, `launched_at`,
`project_id`, and `tool_id`; `pid` is nullable because macOS `open -a` does not
return one. `completed_at` is written by artifact observation
(`crates/app/lifecycle/src/artifact/launches.rs`), not by the launch path. A
prior `spawned` launch with a null `completed_at` triggers the re-launch warning
when `workflow_profiles::launch::prior_launch_is_alive` confirms the tool is
still running: by PID when one was recorded, otherwise by asking macOS Launch
Services whether an application with the profile's `bundle_id` is running. Both
are point-in-time queries made only when the user attempts a re-launch; nothing
is polled or awaited. Tests inject
`workflow_profiles::launch::FakeSpawner` instead of spawning binaries; liveness
itself is covered against real processes in
`crates/workflow/profiles/tests/liveness.rs`. See
[`specs/011-processing-tool-launch/research.md`](../../specs/011-processing-tool-launch/research.md).

The workspace Cargo manifest defines a `dev-tools` feature (default off) that
compile-time-gates the developer-mode surface from spec 021 (recording proxy,
`/dev/contracts` route, `dev.contracts.list` / `dev.calls.list` / `dev.export`
Tauri commands). Release binaries MUST omit the `dev-tools` feature so the
developer surface is absent at runtime, not merely hidden behind a flag.
