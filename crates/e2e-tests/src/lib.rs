// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-UI end-to-end test crate (spec 037, Layer 2).
//!
//! The library target carries only [`instance`] — the std-only per-instance
//! path and port allocation shared by the nextest harness and the
//! `journey-instance` binary. All WebDriver logic lives under `tests/`:
//!
//! - `tests/common/` — the thirtyfour WebDriver harness (tauri-webdriver CLI
//!   + app lifecycle, the `window.__PV_E2E__` invoke bridge, fresh-DB reset).
//! - `tests/journeys.rs` — the high-level user journeys.
//! - `tests/smoke.rs` — the all-top-level-screens-load smoke.
//!
//! Keeping the logic in `tests/` means `thirtyfour` is a dev-dependency that
//! only compiles for the test binaries, never for `cargo build --workspace`.
//!
//! STATUS: scaffold. The journeys are `#[ignore]`d stubs — they compile and
//! appear in `cargo nextest list`, but their assertions are `todo!()` pending a
//! stable backend command surface (research D9). The harness itself (driver
//! launch, capabilities, `__PV_E2E__` bridge) is wired; see `README.md`.

pub mod instance;
