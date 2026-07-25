// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Tauri command surface.
//!
//! `lifecycle` exposes the spec 002 surface as native Tauri 2 commands wired
//! through tauri-specta for TS binding generation. The legacy envelope-based
//! `OperationCommandDispatcher` boundary in `envelope` is preserved for the
//! contract test suite and any operation that does not yet have a typed
//! Tauri command.
//!
//! Spec 029 stub modules remain for command groups whose persistence layer is
//! not yet wired (`sessions.calendar`/`split`/`merge`, `review.queue`,
//! `preferences.get`/`set`, `scan.start`, `equipment.list`,
//! `projects.create_plan`).
//!
//! Spec 030 modules add equipment CRUD, status, cleanup, calibration
//! tolerances, ingestion settings, tools, inbox scan, and cleanup scan
//! commands.

pub mod artifacts;
pub mod audit;
pub mod calibration;
pub mod calibration_tolerances;
pub mod cleanup;
#[cfg(feature = "dev-tools")]
pub mod dev;
pub mod envelope;
pub mod equipment;
pub mod firstrun;
pub mod inbox;
pub mod ingestion;
pub mod inventory;
pub mod inventory_frame;
pub mod lifecycle;
pub mod log;
pub mod manifests;
pub mod native;
pub mod onboarding;
pub mod patterns;
pub mod plan_apply;
pub mod plans;
pub mod preferences;
pub mod prepared_views;
pub mod projects;
pub mod protection;
pub mod recovery;
pub mod review;
pub mod roots;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod status;
pub mod target_favourites;
pub mod target_lookup;
pub mod target_management;
pub mod tools;
