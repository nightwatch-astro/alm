// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Sequential per-item executor loop (spec 025, T013).
//!
//! Walks an approved plan's items in order. For each item:
//!   1. Check cancellation token (between items only — never mid-item).
//!   2. Check skip-set (user-requested skips injected between items).
//!   3. Check per-item FS CAS snapshot (R-FS-1).
//!   4. Perform the filesystem operation.
//!   5. Emit progress event via callbacks.
//!   6. Re-check retry-queue before advancing.
//!
//! Pause conditions (`volume.unavailable`, `disk.full`, `item.stale`)
//! halt the loop and return `ApplyOutcome::Paused`.
//!
//! Cancellation halts forward progress; remaining pending items are
//! batched to `cancelled` by the caller (app/core use case).
//!
//! This module has NO database or audit bus dependencies — those are
//! injected via `ExecutorCallbacks` so the crate can be unit-tested
//! with an in-process fake.
//!
//! Constitution §II: never overwrite silently; per-item audit via callbacks.
//!
//! Split by responsibility (refactor sweep #985): this file holds the
//! shared public types (progress events, item/action model, cancellation +
//! skip + retry coordination primitives); `loop_` is the `execute_plan`
//! forward loop + per-item gate pipeline; `dispatch` is the pure
//! action→filesystem-op dispatch.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;
use domain_core::ids::Timestamp;
use tokio::sync::watch;

use crate::failure::{PlanItemFailure, RollbackOutcome};
use crate::ops::cas_check::CasSnapshot;

mod dispatch;
mod loop_;

#[cfg(test)]
mod tests;

pub use loop_::execute_plan;

// ── Public types ──────────────────────────────────────────────────────────────

/// Snapshot of counters at the end of a run (for terminal state computation).
#[derive(Clone, Debug, Default)]
pub struct TerminalCounts {
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub cancelled: i64,
}

impl TerminalCounts {
    /// Compute the terminal plan state string from the counters.
    /// `cancelled_flag` overrides when cancellation was observed.
    #[must_use]
    pub fn terminal_state(&self, cancelled_flag: bool) -> &'static str {
        if cancelled_flag {
            return "cancelled";
        }
        match (self.succeeded, self.failed) {
            (s, 0) if s > 0 => "applied",
            (0, f) if f > 0 => "failed",
            (s, f) if s > 0 && f > 0 => "partially_applied",
            _ => "failed", // zero successes, zero failures (all skipped)
        }
    }
}

/// Progress event emitted per item state transition.
#[derive(Clone, Debug)]
pub struct ItemProgressEvent {
    pub item_id: String,
    pub prior_state: String,
    pub new_state: String,
    pub at: String,
    pub failure: Option<PlanItemFailure>,
    pub rollback_attempted: bool,
    pub rollback_outcome: RollbackOutcome,
    pub rollback_message: Option<String>,
    /// Structured audit reason for refusals (e.g. `"root_escape"`, `"symlink"`,
    /// `"stale"`, `"destination_exists"`, `"destructive_unconfirmed"`).
    /// `None` for normal success/failure transitions.
    pub audit_reason: Option<String>,
}

impl ItemProgressEvent {
    /// Build a terminal-transition event with the rollback fields at their
    /// non-rollback defaults (`rollback_attempted: false`,
    /// `rollback_outcome: NotApplicable`, `rollback_message: None`).
    ///
    /// Covers every executor-loop transition that never invokes rollback
    /// (skip, gate refusal, stale, protected, success); the one site that
    /// carries a real rollback outcome (a failed mutation) builds the struct
    /// directly instead.
    #[must_use]
    pub fn terminal(
        item_id: impl Into<String>,
        prior_state: impl Into<String>,
        new_state: impl Into<String>,
        failure: Option<PlanItemFailure>,
        audit_reason: Option<String>,
    ) -> Self {
        Self {
            item_id: item_id.into(),
            prior_state: prior_state.into(),
            new_state: new_state.into(),
            at: Timestamp::now_iso(),
            failure,
            rollback_attempted: false,
            rollback_outcome: RollbackOutcome::NotApplicable,
            rollback_message: None,
            audit_reason,
        }
    }
}

/// Final outcome of an `execute_plan` call.
#[derive(Clone, Debug)]
pub enum ApplyOutcome {
    /// All items reached a terminal state.
    Completed(TerminalCounts),
    /// User cancelled; remaining items are pending (caller batch-cancels).
    Cancelled(TerminalCounts),
    /// Run paused due to a recoverable condition.
    Paused { reason: String, counts: TerminalCounts },
}

/// Callbacks injected by the use case layer for persistence + audit.
///
/// Each method is async and returns a `Result<(), String>` where the `String`
/// is an error description. The executor does NOT retry on callback failures —
/// it logs them at `tracing::error!` level and continues.
pub trait ExecutorCallbacks: Send + Sync {
    /// Called when an item transitions to `applying`.
    fn on_item_start(
        &self,
        item_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;

    /// Called when an item resolves (succeeded, failed, skipped, stale).
    fn on_item_progress(
        &self,
        event: ItemProgressEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

/// An item description passed to the executor (de-coupled from the DB row).
#[derive(Clone, Debug)]
pub struct ExecutorItem {
    pub id: String,
    pub plan_id: String,
    pub action: ExecutorItemAction,
    /// **Relative** source path (executor resolves against `library_root` via the path gate).
    ///
    /// Set to `None` for actions that have no source (e.g. `NoOp`, `Mkdir`).
    pub source_path: Option<Utf8PathBuf>,
    /// **Relative** destination path (executor resolves against `destination_root`,
    /// falling back to `library_root`, via the path gate).
    ///
    /// Set to `None` when the destination is implicit (e.g. `Trash`).
    pub destination_path: Option<Utf8PathBuf>,
    /// Absolute source library root — `source_path` is joined against this.
    ///
    /// `None` means "use the path as-is" (legacy / test items with pre-resolved paths).
    pub library_root: Option<Utf8PathBuf>,
    /// Absolute destination library root — `destination_path` is joined against
    /// this instead of `library_root` when set (#765: a cross-root move/link/
    /// mkdir must land under the *picked* destination root, not the source
    /// root). `None` falls back to `library_root`, which preserves same-root
    /// behavior (archive/trash/catalogue, or move within one root) unchanged.
    pub destination_root: Option<Utf8PathBuf>,
    /// Approval-time CAS snapshot (R-FS-1).
    pub cas_snapshot: CasSnapshot,
    /// Protection status from spec-016 (FR-008).
    pub is_protected: bool,
    /// Whether this item requires destructive confirmation (FR-003, D9).
    ///
    /// Derived from action type: `delete` and `trash` ⇒ `true`. Independent of
    /// `is_protected`. Replaces the old `confirm_required = is_protected` inversion.
    pub requires_destructive_confirm: bool,
    /// Whether the destructive confirmation has been satisfied by the user (FR-003).
    pub destructive_confirmed: bool,
    /// Current state when the executor picks it up.
    pub current_state: String,
}

/// Item action categories understood by the executor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutorItemAction {
    Move,
    Archive {
        archive_destination: Utf8PathBuf,
    },
    /// Move to OS trash. Falls back to archive at `fallback_archive_destination` if provided.
    Trash {
        /// Absolute path to use as the archive fallback destination when OS trash is unavailable.
        fallback_archive_destination: Option<Utf8PathBuf>,
    },
    Delete,
    /// Record-in-place: no filesystem mutation. Signals that the file is already
    /// at its final location and only needs to be catalogued (spec 041, T007).
    Catalogue,
    /// Create the destination directory (project scaffolding, spec 008).
    ///
    /// Destination-only (no source). Idempotent when the directory already
    /// exists; a non-directory entry at the destination fails with
    /// `conflict.destination_exists` (constitution §II: never overwrite).
    Mkdir,
    /// Create a real link (or, with `Materialization::Copy`, a real copy) from
    /// source to destination (spec 049 — source view generation).
    Link {
        kind: domain_core::source_view::Materialization,
    },
    /// Write the app-owned project marker file (spec 008 F-1, astro-plan-l3y0).
    ///
    /// Destination-only (no source). `project_id` is the plan item's
    /// `linked_entity`; the executor refuses (rather than guesses) when it is
    /// absent. Idempotent when the destination already holds identical
    /// content — see `ops::write_manifest_op`.
    WriteManifest {
        project_id: String,
    },
    /// RecordOnly — no FS mutation; mark succeeded.
    NoOp,
}

/// Configuration for a single executor run.
#[derive(Clone, Debug)]
pub struct RunConfig {
    pub plan_id: String,
    pub run_id: String,
}

/// A shared cancellation token.
///
/// The apply use case signals cancellation by calling `cancel()`.
/// The executor loop checks `is_cancelled()` between items.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl CancellationToken {
    /// Create a new token that starts in the non-cancelled state.
    #[must_use]
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }

    /// Signal cancellation.
    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }

    /// Return true if cancellation has been signalled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// A shared skip-set: item ids the user has requested to skip.
///
/// The use case injects skip requests between items; the executor checks
/// before picking up each pending item.
///
/// Access is synchronous: each operation takes a `std::sync::Mutex` for a
/// brief, non-blocking critical section (a `HashSet` insert/remove), so there
/// is no need for an async lock — the guard is never held across an `.await`.
#[derive(Clone, Debug, Default)]
pub struct SkipSet {
    inner: Arc<Mutex<HashSet<String>>>,
}

impl SkipSet {
    #[must_use]
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashSet::new())) }
    }

    /// Add an item id to the skip set.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned (only if another thread
    /// panicked while holding the lock — not reachable in normal operation).
    pub fn insert(&self, item_id: &str) {
        self.inner.lock().expect("skip-set mutex poisoned").insert(item_id.to_owned());
    }

    /// Remove and return true if the item was in the skip set.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn take(&self, item_id: &str) -> bool {
        self.inner.lock().expect("skip-set mutex poisoned").remove(item_id)
    }
}

/// A shared retry queue: item ids to re-attempt (per-item retry, US4).
///
/// Access is synchronous for the same reason as [`SkipSet`]: the critical
/// section is a single `HashSet` operation, never held across an `.await`.
#[derive(Clone, Debug, Default)]
pub struct RetryQueue {
    inner: Arc<Mutex<HashSet<String>>>,
}

impl RetryQueue {
    #[must_use]
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashSet::new())) }
    }

    /// Enqueue an item for retry.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned.
    pub fn push(&self, item_id: &str) {
        self.inner.lock().expect("retry-queue mutex poisoned").insert(item_id.to_owned());
    }

    /// Remove and return true if the item is queued for retry.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn take(&self, item_id: &str) -> bool {
        self.inner.lock().expect("retry-queue mutex poisoned").remove(item_id)
    }

    /// Remove and return every queued item id (order unspecified).
    ///
    /// Used between forward-loop items to pick up any retry requests filed
    /// against already-passed items (issue #742) — a plain `take` only
    /// checks a single id, so a retry filed for an EARLIER item was never
    /// consumed by anything.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn drain_all(&self) -> Vec<String> {
        self.inner.lock().expect("retry-queue mutex poisoned").drain().collect()
    }
}
