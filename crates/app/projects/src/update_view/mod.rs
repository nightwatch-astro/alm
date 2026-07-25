// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(
    clippy::missing_errors_doc,
    clippy::explicit_auto_deref,
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    dead_code
)]

//! Project Update View use cases (spec 062 US3/US5 FR-055–FR-059, FR-093–FR-100).
//!
//! Entry points:
//! - [`plan_update_view`]     — generate a bounded additive plan.
//! - [`approve_update_view`]  — approve an open plan after digest verification.
//! - [`apply_update_view`]    — start the long-running installer.
//! - [`run_apply_loop`]       — drive the install loop synchronously (tests / adapter).
//! - [`cancel_update_view`]   — signal the applying operation to stop.
//! - [`discard_update_view`]  — discard an open or stale plan.
//! - [`query_update_view`]    — read an `UpdateViewPlan` DTO.
//! - List queries for sessions, items, conflicts, and overlay mappings.

mod apply;
mod approve;
mod cancel;
mod discard;
pub mod installer;
mod plan;
mod query;

pub use apply::{
    apply_update_view, run_apply_loop, ApplyUpdateViewRequest, ApplyUpdateViewResponse,
};
pub use approve::{approve_update_view, ApproveUpdateViewRequest, ApproveUpdateViewResponse};
pub use cancel::{cancel_update_view, CancelUpdateViewRequest};
pub use discard::{discard_update_view, DiscardUpdateViewRequest};
pub use installer::{InstallItem, InstallerCallbacks};
pub use plan::{plan_update_view, PlanUpdateViewRequest, PlanUpdateViewResponse};
pub use query::{
    list_update_view_added_sessions, list_update_view_conflicts, list_update_view_items,
    list_update_view_overlay_mappings, list_update_view_pinned_sessions, query_update_view,
    AddedSessionPage, ConflictPage, ItemPage, OperationProgress, OverlayMappingPage,
    PinnedSessionPage, UpdateViewPlan,
};
