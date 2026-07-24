// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Project create/update, health, manifests, notes, and prepared-view use cases.
//!
//! Groups the project-domain use-case modules (spec 008 / 009 / 024 / 026).
//! Each sub-module is re-exported from the crate root so existing
//! `app_core::<module>` paths remain stable.
//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b). Its only
//! cross-group dependency was on the now-extracted `app_core_errors` leaf; the
//! remaining cross-references (`project_setup` ↔ `project_health`) are internal
//! to this crate. `app_core` re-exports these modules at their original
//! `app_core::<module>` paths so the public surface is byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

pub mod prepared_views;
pub mod project_health;
pub mod project_manifests;
pub mod project_notes;
pub mod project_setup;
pub mod session_membership;
pub mod source_view_generate;
pub mod source_view_verify;
#[cfg(test)]
pub(crate) mod test_support;
