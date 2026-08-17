// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! App-layer Inbox session materialization — `inbox.materialization.apply`.
//!
//! Drives the `session_materialization_operation` CAS lifecycle:
//! `ready → applying → applied` (or `cancelled` / `failed`).
//!
//! The caller owns command-ledger claim/finish; this module owns the domain
//! write window between `applying` and the terminal result snapshot.

pub mod apply;
pub mod cancel;
pub mod plan_query;
pub mod progress;
