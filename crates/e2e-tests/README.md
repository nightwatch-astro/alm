# crates/e2e-tests — spec 037 Layer-2 real-UI E2E

Thirtyfour + cargo-nextest end-to-end tests for the Astro Library Manager
desktop app.

## Status

**Real journeys, wired against the real backend (2026-07-04, spec 037 WP-C).**

Six journeys run against the real app + real SQLite + real filesystem:
`first_run_resolve_create_project`, `plan_review_apply_with_audit`,
`ingestion_sessions_search`, `lifecycle_integrity`, `cleanup_plan_review`
(`tests/journeys.rs`), and `all_top_level_screens_load`
(`tests/smoke.rs`). All six are `#[ignore]`d in-source purely so the
Layer-1 `cargo test --workspace` job (ci.yml) — which has no
`tauri-webdriver` CLI, no e2e-feature app build, and no served frontend —
skips them; the e2e workflow opts back in with `--run-ignored all`.
Durable-record proofs use
`plans.apply.status` (`plan_apply_events`) and `lifecycle.ledger.list` — the
read paths closest to the mutations being proved. (`audit.list`/
`audit.export` were stubs when these were authored; PR #388 wired them to
the real `audit_log_entry` table, #401 adds entity-filtered reads — they're
now available as complementary assertion surfaces.) See each journey's doc
comment for exactly which real commands it drives and why.

Cannot run in the WSL dev sandbox (no webview/display) — CI (`e2e.yml`, 3-OS
matrix) is the first real verification point; iterate there.

## How to run

```sh
cargo nextest run -p e2e_tests --profile e2e --run-ignored all
```

## Mechanism

- `desktop_shell` is built with `cargo build -p desktop_shell --features e2e`,
  which compiles in `tauri-plugin-webdriver` (Choochmeque) — an embedded W3C
  WebDriver server listening on `127.0.0.1:4445`. Release builds omit the
  `e2e` feature so the automation surface is never present (Constitution
  Principle V).
- The `tauri-webdriver` CLI (`cargo install tauri-webdriver --locked`) proxies
  `127.0.0.1:4444` to the embedded plugin server on `:4445`, and manages the
  target app's process lifecycle via the `tauri:options` capability — it does
  **not** take the app binary as a CLI argument.
- thirtyfour (this crate's W3C client) connects to the CLI on `:4444` and
  sends `tauri:options.application` = the built `desktop_shell` binary path in
  the New Session capabilities. No `browserName` is set.
- The app loads its own frontend from the Tauri `devUrl` (`$PV_DEV_URL`, else
  `build.devUrl`) automatically on launch; the harness does not call
  `driver.goto(...)` after connecting.
- `window.__PV_E2E__.invoke(...)` is the real-IPC invoke bridge exposed by
  the frontend when built with `VITE_E2E=1` (`apps/desktop/src/main.tsx`).

This mirrors `.github/workflows/e2e.yml` (see `specs/037-e2e-integration-testing/research.md`
D10 and `quickstart.md`).

## Prerequisites

- **tauri-webdriver** installed and on `$PATH`
  (`cargo install tauri-webdriver --locked`).
- The `desktop_shell` binary must be **built with the `e2e` feature**:
  `cargo build -p desktop_shell --features e2e`. Override the binary the
  harness launches with `PV_E2E_APP_BIN=/path/to/binary`.
- Vite dev server / `vite preview` running on the port the app loads its
  frontend from: `build.devUrl` in `apps/desktop/src-tauri/tauri.conf.json`,
  or the `PV_DEV_URL` you export. `PV_DEV_URL` is read by the app binary and by
  this harness, so exporting one value moves both. Concurrent lanes on one host
  MUST each export a distinct `PV_DEV_URL`; otherwise the second lane's app
  loads the first lane's frontend build (issue #1409). Serve with:
  - `VITE_USE_MOCKS=false` — real backend.
  - `VITE_E2E=1` — exposes the `window.__PV_E2E__` invoke bridge.

## Running two lanes at once

`just e2e-dev-url` prints a `PV_DEV_URL` whose port is derived from the
checkout path: stable per worktree, distinct between worktrees. Export it in
both halves of the run:

```bash
export PV_DEV_URL="$(just e2e-dev-url)"
VITE_E2E=1 VITE_USE_MOCKS=false pnpm --filter @astro-plan/desktop preview \
  --port "${PV_DEV_URL##*:}" --strictPort &
cargo nextest run -p e2e_tests --profile e2e --run-ignored all
```

`--strictPort` exits 1 when the port is taken. Without it `vite preview` falls
back to another port, the app keeps loading `PV_DEV_URL`, and the failure looks
like a broken journey.

## Notes

- Old per-OS driver checks (`WebKitWebDriver`/`msedgedriver`) are obsolete —
  `tauri-plugin-webdriver` replaces them on every OS (research D10). The
  harness's `preflight()` only checks for the `tauri-webdriver` CLI and the
  built `desktop_shell` binary.
- Each nextest test process gets its own isolated `tauri-webdriver` proxy
  port, native plugin port, app-data/config dir, and SQLite DB file
  (`tests/common/mod.rs::InstanceEnv`), lazily allocated once per process and
  reused across `launch()`/`relaunch()` calls within that test. `PV_DB_URL`
  is set internally per instance and no longer read from the ambient
  environment — nothing to configure. Each instance also gets its own
  `PV_E2E_INSTANCE_ID`, whose presence tells `apps/desktop/src-tauri/src/lib.rs`
  to skip the single-instance plugin entirely so concurrently-launched
  `desktop_shell` processes no longer collide on Tauri's default
  identifier-derived identity. The plugin's per-instance override exists only
  on Linux (`dbus_id`) — not Windows (named mutex) or macOS — so skipping is
  the only cross-platform fix; no journey exercises single-instance behaviour
  (re-checked against `docs/journeys/` on 2026-08-25).
  It is a no-op outside e2e (real users never set this env var, so the guard
  stays active). Together this lets `test-threads` in the `e2e` nextest profile
  exceed 1 without journeys racing each other; see `.config/nextest.toml`
  for the current value and its rationale. The allocation itself lives in
  `src/instance.rs` and is shared with the `journey-instance` binary
  (`docs/development/parallel-journey-instances.md`).
