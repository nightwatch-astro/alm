// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Filesystem plan apply executor (spec 025).
//!
//! This crate owns the per-item executor loop, filesystem operation
//! primitives (move, archive, trash, delete, remove-generated-link),
//! failure taxonomy, rollback hooks, cancellation, and pause/resume
//! state machine.
//!
//! It has **no UI, no database, and no Tauri dependencies** — it is
//! purely domain and filesystem logic. Integration with persistence and
//! the audit bus lives in `crates/app/core/src/plan_apply.rs`.
//!
//! Constitution §II: never overwrite silently (CAS drift → fail);
//! audit record per attempted action + outcome; destructive ops prefer
//! archive/trash over permanent delete.

#![allow(clippy::doc_markdown)]

pub mod failure;
pub mod ops;
pub mod reconcile;
pub mod run;
pub mod update_view;

pub use failure::{PlanItemFailure, RollbackOutcome};
pub use reconcile::{classify, MutationShape, ReconcileVerdict};
pub use run::{
    ApplyOutcome, CancellationToken, ExecutorCallbacks, ItemProgressEvent, RunConfig,
    TerminalCounts,
};
