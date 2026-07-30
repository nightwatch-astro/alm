// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-frame inventory use cases (spec 048 T006/T015).
//!
//! `list_frames` backs `inventory.frame.list`: a read-only projection of
//! `file_record` rows for a session or root.
//!
//! `run_reconcile` backs `inventory.reconcile.run` for the `on_demand`
//! trigger: it walks the root's on-disk state (via
//! `fs_inventory::reconcile::reconcile_root`, gated by the root's
//! `detection.follow_symlinks` setting — spec 048 T003/T004), corrects any
//! present record whose recorded size is stale or `0` (FR-006/FR-012, T015 —
//! emitting `frame.size_backfilled`), and marks newly-absent records
//! `missing` / newly-present records recovered (FR-007/FR-009/FR-011),
//! emitting `frame.missing`/`frame.recovered`. It NEVER creates, deletes, or
//! moves a file (FR-008/INV-2) — only `file_record` rows are written.
//!
//! Applying a root's `reconcile.mode` to session `frame_ids` array membership
//! (dropping a missing id from the array in auto-reconcile mode, FR-010) is
//! left for the full US2 T021 implementation: this pass always retains the
//! id in the array and relies on the existing `state != 'missing'` filter
//! (`app_core::sessions::active_frame_summary`) to exclude it from active
//! counts/totals, which already satisfies FR-009's flag-missing behaviour.

use contracts_core::error_code::ErrorCode;
use contracts_core::inventory_frame::RawFrameType;
use contracts_core::{ContractError, ErrorSeverity};
use sqlx::SqlitePool;

use app_core_errors::db_err;

mod list;
mod reconcile;
mod relink;
#[cfg(test)]
mod tests;

pub use list::list_frames;
pub use relink::relink_frame;

use contracts_core::inventory_frame::{
    InventoryReconcileRunRequest, InventoryReconcileRunResponse,
};

/// `inventory.reconcile.run` — the reconcile pass followed by the project
/// `source_missing` block check (spec 009 US4, FR-020/FR-021).
///
/// The two are deliberately one exported symbol rather than two the caller
/// must remember to chain: reconcile is the only production writer of
/// `file_record.state='missing'`, so a project whose sources vanished can only
/// be detected here. Composing at the command layer instead is what left the
/// `source_missing` trigger with zero production callers, and is unreachable
/// from a Layer-1 test.
///
/// # Errors
///
/// Returns `ContractError` per `reconcile::run_reconcile`. A failed
/// project-health check is retried on the next reconcile for the same root,
/// because the frame-state writes have already committed by then.
#[expect(
    clippy::cognitive_complexity,
    reason = "reconcile pass over inventory rows with per-row health outcomes"
)]
pub async fn run_reconcile(
    pool: &SqlitePool,
    bus: &audit::bus::EventBus,
    req: &InventoryReconcileRunRequest,
) -> Result<InventoryReconcileRunResponse, ContractError> {
    let retry_pending =
        persistence_plans::repositories::projects::begin_source_missing_health_check(
            pool,
            &req.root_id,
        )
        .await
        .map_err(db_err)?;

    let response = match reconcile::run_reconcile(pool, bus, req).await {
        Ok(response) => response,
        Err(error) => {
            if !retry_pending {
                let _ =
                    persistence_plans::repositories::projects::clear_source_missing_health_check(
                        pool,
                        &req.root_id,
                    )
                    .await;
            }
            return Err(error);
        }
    };

    let should_check = response.newly_missing > 0 || retry_pending;
    if should_check {
        match app_core_projects::project_health::check_project_source_missing_invariant(
            pool,
            bus,
            crate::caches::project_block_debounce(),
        )
        .await
        {
            Ok(app_core_projects::project_health::SourceMissingCheckOutcome::RetryRequired) => {
                return Ok(response);
            }
            Ok(app_core_projects::project_health::SourceMissingCheckOutcome::Complete) => {}
            Err(error) => {
                tracing::warn!(%error, "project source-missing health check failed after reconcile");
                return Ok(response);
            }
        }
    }

    if let Err(error) =
        persistence_plans::repositories::projects::clear_source_missing_health_check(
            pool,
            &req.root_id,
        )
        .await
    {
        tracing::warn!(%error, "failed to clear project source-missing health retry");
    }

    Ok(response)
}

fn internal(msg: impl std::fmt::Display) -> ContractError {
    ContractError::new(ErrorCode::InternalDatabase, msg.to_string(), ErrorSeverity::Fatal, true)
}

/// Raw row shape shared by both list paths. `pub(crate)` so
/// `cleanup_generator` (spec 048 US3) can reuse the same lookups instead of
/// duplicating the `file_record` query.
pub(crate) struct FrameRow {
    pub(crate) id: String,
    pub(crate) root_id: String,
    pub(crate) relative_path: String,
    pub(crate) size_bytes: i64,
    pub(crate) state: String,
}

/// `calibration_session.kind` → [`RawFrameType`]. `flat_dark` frames are a
/// dark exposure taken at flat settings; they are classified as `Dark` here
/// since there is no dedicated wire variant (documented simplification).
fn raw_frame_type_from_calibration_kind(kind: &str) -> RawFrameType {
    match kind {
        "flat" => RawFrameType::Flat,
        "bias" => RawFrameType::Bias,
        _ => RawFrameType::Dark, // "dark" | "flat_dark"
    }
}

fn frame_row_from_repo_row(r: persistence_core::repositories::q_core::FileRecordRow) -> FrameRow {
    FrameRow {
        id: r.id,
        root_id: r.root_id,
        relative_path: r.relative_path,
        size_bytes: r.size_bytes,
        state: r.state,
    }
}

pub(crate) async fn rows_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<FrameRow>, ContractError> {
    let rows = persistence_core::repositories::q_core::file_records_by_ids(pool, ids)
        .await
        .map_err(db_err)?;
    Ok(rows.into_iter().map(frame_row_from_repo_row).collect())
}

async fn rows_by_root(pool: &SqlitePool, root_id: &str) -> Result<Vec<FrameRow>, ContractError> {
    let rows = persistence_core::repositories::q_core::file_records_by_root(pool, root_id)
        .await
        .map_err(db_err)?;
    Ok(rows.into_iter().map(frame_row_from_repo_row).collect())
}

/// Best-effort reverse lookup: which session (if any) references `frame_id`,
/// and that session's frame type. Used only for the root-scoped list, where
/// `file_record` itself carries no frame-type column. A frame referenced by
/// no session (e.g. never joined, or a future hard-delete edge case) falls
/// back to `Light` — documented limitation of this scaffold; session-scoped
/// listing (the common case per contracts/operations.md) does not need this
/// fallback at all.
pub(crate) async fn owning_session_frame_type(
    pool: &SqlitePool,
    frame_id: &str,
) -> Result<(Option<String>, RawFrameType), ContractError> {
    let like = format!("%\"{frame_id}\"%");

    if let Some(session_id) =
        persistence_core::repositories::q_core::find_acquisition_session_id_by_frame_like(
            pool, &like,
        )
        .await
        .map_err(db_err)?
    {
        return Ok((Some(session_id), RawFrameType::Light));
    }

    if let Some((session_id, kind)) =
        persistence_core::repositories::q_core::find_calibration_session_by_frame_like(pool, &like)
            .await
            .map_err(db_err)?
    {
        return Ok((Some(session_id), raw_frame_type_from_calibration_kind(&kind)));
    }

    Ok((None, RawFrameType::Light))
}

/// Batch-build a reverse lookup from frame_id to (session_id, frame_type) for
/// all sessions in the database. Replaces the per-frame LIKE full-table scan in
/// the root-scoped list path (DSD-8). Callers that list frames by root can call
/// this once and look up each frame in O(1) instead of issuing a LIKE query per
/// frame.
pub(crate) async fn build_frame_session_map(
    pool: &sqlx::SqlitePool,
) -> Result<std::collections::HashMap<String, (String, RawFrameType)>, ContractError> {
    use std::collections::HashMap;

    let mut map: HashMap<String, (String, RawFrameType)> = HashMap::new();

    // Acquisition sessions (light frames).
    let acq_rows = persistence_core::repositories::q_core::all_acquisition_session_frame_ids(pool)
        .await
        .map_err(db_err)?;

    for (session_id, frame_ids_json) in acq_rows {
        let ids: Vec<String> = serde_json::from_str(&frame_ids_json).unwrap_or_default();
        for fid in ids {
            map.entry(fid).or_insert_with(|| (session_id.clone(), RawFrameType::Light));
        }
    }

    // Calibration sessions (dark/flat/bias frames).
    let cal_rows = persistence_core::repositories::q_core::all_calibration_session_frame_ids(pool)
        .await
        .map_err(db_err)?;

    for (session_id, frame_ids_json, kind) in cal_rows {
        let frame_type = raw_frame_type_from_calibration_kind(&kind);
        let ids: Vec<String> = serde_json::from_str(&frame_ids_json).unwrap_or_default();
        for fid in ids {
            map.entry(fid).or_insert_with(|| (session_id.clone(), frame_type));
        }
    }

    Ok(map)
}

fn iso_now() -> String {
    // Redirect to canonical Timestamp::now_iso() (bd astro-plan-kyo7.88).
    // NOTE: previously used Iso8601::DEFAULT (sub-second precision); now Rfc3339
    // (no sub-second). Persisted mtime/created_at string sort order is unchanged
    // (both are lexicographically sortable ISO 8601). New rows get Rfc3339 format.
    domain_core::ids::Timestamp::now_iso()
}
