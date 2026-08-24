# Scaffold Architecture

The initial scaffold is a monorepo with small Rust crates and a placeholder
desktop app. It is designed to keep pure domain unit tests fast while isolating
future heavy dependencies:

- parser dependencies stay in metadata adapter crates
- SQLite dependencies stay in the persistence crate
- Tauri dependencies stay in the desktop shell
- contract generation stays in the contracts package/crate
- pure model and rule tests stay in dependency-light domain crates

Filesystem grouping follows domain ownership where a family is expected to grow:

- `crates/fs/inventory` and `crates/fs/planner` keep read-only discovery
  separate from reviewable mutation planning. Future `fs/watcher` and
  `fs/executor` crates can be added without pulling watcher or mutation
  dependencies into inventory tests.
- `crates/metadata/core`, `crates/metadata/fits`, `crates/metadata/xisf`, and
  `crates/metadata/video` keep shared metadata types separate from heavier
  format-specific parser dependencies.
- `crates/app/core` leaves room for future app-facing crates without mixing
  orchestration with desktop shell code.
- `crates/project/structure` isolates the app-owned project envelope rules from
  future project lifecycle, manifests, and source-mapping crates.
- the `crates/persistence/` crates isolate database dependencies from pure domain and
  filesystem-planning tests.
- `crates/workflow/profiles` leaves room for tool-specific profile crates such
  as `workflow/pixinsight` or `workflow/planetary` if the plan justifies them.
- `crates/calibration/core` keeps reusable calibration policy separate from
  future heavier matching or statistics crates.

This document records scaffold intent only. Product and technical decisions
belong in `specs/001-astro-library-manager/plan.md` and supporting SpecKit
artifacts.
