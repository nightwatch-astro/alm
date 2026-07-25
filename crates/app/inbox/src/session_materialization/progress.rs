// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! In-memory progress counters for an in-flight materialization apply.
//!
//! The contract requires at most ten progress updates per second and one update
//! every 500 ms outside the final transaction. This module provides the
//! counter state; the apply loop owns the publishing cadence.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

/// Shared mutable progress state for one `inbox.materialization.apply` call.
///
/// All fields are updated from the apply worker task; reads may come from the
/// progress query handler on any thread.
#[derive(Debug)]
pub struct MaterializationProgress {
    pub total_sessions: i64,
    pub total_frames: i64,
    processed_sessions: AtomicI64,
    processed_frames: AtomicI64,
    /// Set by [`Self::request_cancel`] to signal the apply loop to stop after the current
    /// session commit.
    pub cancel_requested: AtomicBool,
}

impl MaterializationProgress {
    /// Create a new tracker with the totals known before the apply loop starts.
    #[must_use]
    pub fn new(total_sessions: i64, total_frames: i64) -> Arc<Self> {
        Arc::new(Self {
            total_sessions,
            total_frames,
            processed_sessions: AtomicI64::new(0),
            processed_frames: AtomicI64::new(0),
            cancel_requested: AtomicBool::new(false),
        })
    }

    /// Increment processed counts after one session's frames have been written.
    pub fn record_session_done(&self, frame_count: i64) {
        self.processed_sessions.fetch_add(1, Ordering::Relaxed);
        self.processed_frames.fetch_add(frame_count, Ordering::Relaxed);
    }

    /// Current processed session count.
    pub fn processed_sessions(&self) -> i64 {
        self.processed_sessions.load(Ordering::Relaxed)
    }

    /// Current processed frame count.
    pub fn processed_frames(&self) -> i64 {
        self.processed_frames.load(Ordering::Relaxed)
    }

    /// True if the apply loop should abandon work after the current session.
    pub fn is_cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Relaxed)
    }

    /// Signal cancellation. Idempotent.
    pub fn request_cancel(&self) {
        self.cancel_requested.store(true, Ordering::Relaxed);
    }
}

/// A snapshot of current progress for the progress-query contract response.
#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    pub operation_id: String,
    pub state: String,
    pub processed_sessions: i64,
    pub total_sessions: i64,
    pub processed_frames: i64,
    pub total_frames: i64,
    /// False during the final bounded commit; true at all other points where
    /// cancellation is safe without session/membership rows being written.
    pub cancel_safe: bool,
}

impl MaterializationProgress {
    /// Snapshot current counters for the progress-query response.
    #[must_use]
    pub fn snapshot(&self, operation_id: impl Into<String>, state: &str) -> ProgressSnapshot {
        ProgressSnapshot {
            operation_id: operation_id.into(),
            state: state.to_owned(),
            processed_sessions: self.processed_sessions(),
            total_sessions: self.total_sessions,
            processed_frames: self.processed_frames(),
            total_frames: self.total_frames,
            cancel_safe: !self.is_cancel_requested(),
        }
    }
}
