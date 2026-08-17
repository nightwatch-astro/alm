// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Ingestion operation — operation-identity and state-query side for
//! `inbox.materialization.apply` (Spec 062 §Ingestion).
//!
//! This module ties together the command ledger and the persistence-sessions
//! materialization repositories. App-layer use cases call `query_operation`
//! to read the current operation state and `query_result_sessions` to page
//! through the result snapshot.

pub mod operation_query;
