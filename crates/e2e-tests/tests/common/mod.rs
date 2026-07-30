// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared harness for spec 037 Layer-2 real-UI E2E journeys.
//!
//! All journeys are real (WP-C) but `#[ignore]`d: they need the
//! `tauri-webdriver` CLI, a `desktop_shell --features e2e` build, and a
//! served frontend — none of which exist in the Layer-1 `cargo test
//! --workspace` job (ci.yml). The dedicated e2e.yml workflow runs them with
//! `--run-ignored all` after standing that environment up.
//!
//! # `slow_` test naming convention
//!
//! **Threshold**: prefix a test with `slow_` when its average wall time on
//! Windows CI exceeds 3× the suite median, measured on post-fix runs only.
//! With a current median of ~7s, the threshold is ~21s. Pre-fix timings are
//! invalid — they measure a bug, not the test. Re-evaluate the prefix after
//! any fix that materially changes a test's runtime.
//!
//! `e2e.yml`'s Windows shards use LPT (longest-processing-time first)
//! assignment: each `slow_` test is pinned to a separate shard, non-slow
//! tests fill the remaining slots by greedy LPT. The shard filter expressions
//! use `test(=slow_<name>)` with no `--partition` flag — nextest ANDs
//! `--partition` and `-E filterset`, which would exclude an explicitly named
//! test that doesn't hash to that bucket.
//!
//! Adding a new slow test: rename the function with `slow_`, measure its
//! average over 2+ post-fix CI runs, add a row to the timing table in
//! e2e.yml, re-run LPT, and update the shard `-E` expressions there.
//!
//! Mechanism (mirrors `.github/workflows/e2e.yml`, research D10):
//! - `desktop_shell` is built with `cargo build -p desktop_shell --features
//!   e2e`, which compiles in `tauri-plugin-webdriver` (Choochmeque) — an
//!   embedded W3C WebDriver server on loopback. Release builds omit the
//!   `e2e` feature so the automation surface is never present (Constitution
//!   Principle V).
//! - The `tauri-webdriver` CLI (`cargo install tauri-webdriver --locked`)
//!   proxies a loopback port -> the embedded plugin server on another, and
//!   manages the target app's process lifecycle via the `tauri:options`
//!   capability — it does **not** take the app binary as a CLI argument.
//!   Both ports are allocated per test PROCESS (each nextest test is its own
//!   process) rather than fixed at `:4444`/`:4445`, so concurrent journeys
//!   (`test-threads > 1`) never collide — see [`InstanceEnv`].
//! - thirtyfour (this crate's W3C client) connects to the CLI's proxy port and
//!   sends `tauri:options.application` = the built `desktop_shell` binary
//!   path in the New Session capabilities. No `browserName` is set (see
//!   `quickstart.md`).
//! - The app loads its own frontend from the Tauri `devUrl` (`:5173`)
//!   automatically on launch, so the harness does **not** call
//!   `driver.goto(...)` after connecting.
//! - `window.__PV_E2E__.invoke(...)` (exposed by the frontend when built
//!   with `VITE_E2E=1`, see `apps/desktop/src/main.tsx`) is the real-IPC
//!   invoke bridge used by [`E2eApp::invoke`].
//!
//! See `crates/e2e-tests/README.md` for the full run procedure.

// Each test binary under `tests/` compiles this module separately and uses only
// the subset of the harness it needs, so items and re-exports that are live for
// one binary are genuinely unused in another. `dead_code` covers the
// definitions; `unused_imports` covers the re-exports below, which otherwise
// fail the `-D warnings` clippy gate on whichever binary happens not to use one.
#![allow(dead_code, unused_imports)]

mod app;
mod boot;
mod fixtures;
mod helpers;

pub use app::E2eApp;
pub use boot::{
    APP_URL, DEFAULT_FIND_TIMEOUT, DRAIN_BACKED_TIMEOUT, LAUNCH_TIMEOUT, SCRIPT_TIMEOUT,
    SCRIPT_TIMEOUT_PAGE_LOAD,
};
pub use fixtures::{
    scan_and_classify_one_item, settle_first_run_redirect, write_minimal_fits,
    write_minimal_fits_with_exposure,
};
