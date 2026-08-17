// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Inbox use cases for spec 005 (Inbox Mixed-Folder Split).
//!
//! # Modules
//!
//! - [`signature`] — content signature computation (R-Sig-1).
//! - [`scan`] — recursive folder scan, leaf detection, video lane.
//! - [`classify`] — `inbox.classify` use case: extract IMAGETYP, normalise,
//!   persist evidence, compute classification result.
//! - [`confirm`] — `inbox.confirm` use case: TOCTOU guard, plan creation.
//! - [`reclassify`] — `inbox.reclassify` use case: manual overrides + re-aggregate.
//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b). Its only
//! cross-module dependency was on the now-extracted `app_core_errors` leaf.
//! `app_core` re-exports this crate at `app_core::inbox` so the public surface
//! stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

pub mod attribution;
pub mod classify;
pub mod cone_search;
pub mod confirm;
pub mod grouping;
pub mod metadata;
pub mod plan_listener;
pub mod property_registry;
pub mod reclassify;
pub mod repair;
pub mod scan;
pub mod session_materialization;
pub mod signature;
pub mod stats;
pub mod target_recommendations;
