// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Target catalog, resolution, search, and ingest-resolution use cases.
//!
//! Groups the target-domain use-case modules (spec 035 / spec 036). Each
//! sub-module is re-exported from the crate root so existing
//! `app_core::<module>` paths remain stable.
//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b). Its only
//! cross-group dependency was on the now-extracted `app_core_errors` leaf; the
//! remaining cross-references (`target_dto`) are internal to this crate.
//! `app_core` re-exports these modules at their original `app_core::<module>`
//! paths so the public surface is byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

/// Process-global in-memory cache statics for the target catalog snapshot and
/// resolver settings (in-memory caching layer, F0 foundation). See each
/// accessor's doc comment for its owning write-site invalidation calls.
pub mod caches;
pub mod frame_writer;
pub mod ingest_resolution;
pub mod ingest_sessions;
pub mod ingestion_operation;
/// Cached FITS/XISF header extraction (in-memory caching layer, F0/W-FITS).
/// Lives here (not `app_core`) so both `app_core_inbox` and
/// `app_core_targets::ingest_sessions` — the two current extractor call
/// sites — can reach it without a cyclic crate dependency; see the module
/// doc comment for the full reachability argument.
pub mod metadata_cache;
pub mod resolver_settings;
pub mod target_dto;
pub mod target_favourites;
pub mod target_management;
pub mod target_resolve;
pub mod target_search;
