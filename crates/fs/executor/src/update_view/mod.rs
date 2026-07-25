// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Additive no-clobber filesystem installer for Update View plans
//! (spec 062 FR-100).
//!
//! `install_item` copies source bytes to a temporary file, fsyncs, then
//! renames atomically to the destination. Never overwrites an existing path.

pub mod install;

pub use install::{install_item, InstallError, InstallErrorCode, InstallOutcome};
