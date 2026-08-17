// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Re-exports of persistence-layer plan-result queries for use by
//! [`super::apply`].
//!
//! All SQL lives in [`persistence_inbox::repositories::plan_result`].

pub use persistence_inbox::repositories::plan_result::{
    get_plan_snapshot_for_operation, get_site_resolution_revision, list_proposed_session_frames,
    list_proposed_sessions, PlanResultSnapshotRow, ProposedSessionFrameRow, ProposedSessionRow,
    SiteResolutionRevisionRow,
};
