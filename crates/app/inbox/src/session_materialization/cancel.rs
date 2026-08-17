// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `session.materialization.cancel` command handler.
//!
//! Sets the `cancel_requested` flag on the in-flight progress tracker so the
//! apply loop stops between sessions. If the apply loop has already committed
//! its terminal transaction the cancellation is a no-op (the operation is
//! already `applied`).

use std::sync::Arc;

use sqlx::SqlitePool;

use persistence_core::DbResult;
use persistence_sessions::repositories::materialization::get_operation_by_public_id;

use super::progress::MaterializationProgress;

/// Result of a cancellation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    /// Flag set; apply loop will observe it within the next cancellation check.
    CancelRequested { operation_id: String },
    /// The operation already reached a terminal state.
    AlreadyTerminal { operation_id: String, state: String },
    /// The operation is not in a cancellable state (e.g. `ready`).
    NotCancellable { operation_id: String, state: String },
}

/// Signal cancellation for the operation identified by `operation_public_id`.
///
/// If a live [`MaterializationProgress`] tracker is supplied its `cancel_requested`
/// flag is set. The function also reads the current operation state from the DB to
/// distinguish already-terminal from not-yet-started operations.
///
/// # Errors
///
/// Returns a database error if the operation row cannot be read.
pub async fn request_cancel(
    pool: &SqlitePool,
    operation_public_id: &str,
    tracker: Option<Arc<MaterializationProgress>>,
) -> DbResult<CancelOutcome> {
    let op = get_operation_by_public_id(pool, operation_public_id).await?;
    match op.state.as_str() {
        "applied" | "cancelled" | "failed" => {
            return Ok(CancelOutcome::AlreadyTerminal {
                operation_id: op.public_id,
                state: op.state,
            });
        }
        "ready" => {
            // Not yet in flight; no tracker to signal.
            return Ok(CancelOutcome::NotCancellable {
                operation_id: op.public_id,
                state: op.state,
            });
        }
        _ => {} // applying or cancelling: proceed
    }
    if let Some(t) = tracker {
        t.request_cancel();
    }
    Ok(CancelOutcome::CancelRequested { operation_id: op.public_id })
}
