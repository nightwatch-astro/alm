// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Entry point for session-materialization integration tests (Spec 062 US1).
//!
//! Real-SQLite tests validating atomic apply, panel-group creation, idempotency,
//! and cancel behaviour for `inbox.materialization.apply`.

#![allow(clippy::doc_markdown)]

mod session_materialization;
