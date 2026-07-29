// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `inventory.reconcile.run` — on-demand reconcile pass over a root.

use std::path::Path;

use contracts_core::error_code::ErrorCode;
use contracts_core::inventory_frame::{
    InventoryReconcileRunRequest, InventoryReconcileRunResponse,
};
use contracts_core::{ContractError, ErrorSeverity};
use fs_inventory::reconcile::{reconcile_root, FrameOutcome, KnownFrame};
use sqlx::SqlitePool;

use app_core_errors::db_err;
use app_core_targets::frame_writer::upsert_frame_record;

use super::{internal, iso_now, rows_by_root, FrameRow};

/// Running tallies accumulated across a reconcile pass.
#[derive(Default)]
struct ReconcileTally {
    present: u32,
    newly_missing: u32,
    recovered: u32,
    size_backfilled: u32,
}

/// spec 048 US5 (FR-024/025, PATH B): emit `calibration_match.source_missing`
/// / `.source_recovered` for every calibration match whose master
/// (`calibration_session`) lists `frame_id` among its own raw sub-frames.
/// Best-effort — a lookup/publish failure here must not fail the raw-frame
/// reconcile pass, since the flag is re-derived live on next read regardless
/// (never the durable record).
async fn emit_calibration_match_flag_for_frame(
    pool: &SqlitePool,
    bus: &audit::bus::EventBus,
    frame_id: &str,
    at: &str,
    recovered: bool,
) {
    let Ok(assignments) =
        persistence_calibration::repositories::calibration_assignment::find_by_source_frame(
            pool, frame_id,
        )
        .await
    else {
        return;
    };
    for assignment in assignments {
        if recovered {
            let _ = bus
                .publish(
                    audit::event_bus::TOPIC_CALIBRATION_MATCH_SOURCE_RECOVERED,
                    audit::event_bus::Source::System,
                    audit::event_bus::CalibrationMatchSourceRecovered {
                        match_id: assignment.id,
                        frame_id: frame_id.to_owned(),
                        at: at.to_owned(),
                    },
                )
                .await;
        } else {
            let _ = bus
                .publish(
                    audit::event_bus::TOPIC_CALIBRATION_MATCH_SOURCE_MISSING,
                    audit::event_bus::Source::System,
                    audit::event_bus::CalibrationMatchSourceMissing {
                        match_id: assignment.id,
                        frame_id: frame_id.to_owned(),
                        at: at.to_owned(),
                    },
                )
                .await;
        }
    }
}

/// Apply a single `Present` outcome: correct `size_bytes` when it changed
/// (including the `0` placeholder backfill, T015) and/or flip a previously
/// `missing` record back to `classified` (FR-011), emitting the matching
/// audit events. A no-op when nothing changed.
async fn apply_present_outcome(
    pool: &SqlitePool,
    bus: &audit::bus::EventBus,
    row: &FrameRow,
    real_size_bytes: i64,
    was_missing: bool,
    tally: &mut ReconcileTally,
) -> Result<(), ContractError> {
    tally.present += 1;
    if row.size_bytes == real_size_bytes && !was_missing {
        return Ok(());
    }

    let now = iso_now();
    upsert_frame_record(
        pool,
        &row.root_id,
        &row.relative_path,
        real_size_bytes,
        &now,
        "classified",
    )
    .await
    .map_err(|e| internal(e.message))?;

    if row.size_bytes == 0 {
        tally.size_backfilled += 1;
        bus.publish(
            audit::event_bus::TOPIC_FRAME_SIZE_BACKFILLED,
            audit::event_bus::Source::System,
            audit::event_bus::FrameSizeBackfilled {
                frame_id: row.id.clone(),
                root_id: row.root_id.clone(),
                relative_path: row.relative_path.clone(),
                prior_size_bytes: row.size_bytes,
                size_bytes: real_size_bytes,
                at: now.clone(),
            },
        )
        .await
        .map_err(internal)?;
    }

    if was_missing {
        tally.recovered += 1;
        bus.publish(
            audit::event_bus::TOPIC_FRAME_RECOVERED,
            audit::event_bus::Source::System,
            audit::event_bus::FrameRecovered {
                frame_id: row.id.clone(),
                root_id: row.root_id.clone(),
                relative_path: row.relative_path.clone(),
                at: now.clone(),
            },
        )
        .await
        .map_err(internal)?;

        // spec 048 US5 (FR-025): clear "source missing" on any calibration
        // match whose master's raw sub-frames include this frame.
        emit_calibration_match_flag_for_frame(pool, bus, &row.id, &now, true).await;
    }

    Ok(())
}

/// Apply a single `Missing` outcome: mark the record `missing` when it
/// wasn't already (FR-007/FR-009), emitting `frame.missing`. Applying the
/// root's `reconcile.mode` to session `frame_ids` array membership (FR-010)
/// is left to the full US2 T021 implementation — see module docs.
async fn apply_missing_outcome(
    pool: &SqlitePool,
    bus: &audit::bus::EventBus,
    row: &FrameRow,
    was_missing: bool,
    reason: &str,
    tally: &mut ReconcileTally,
) -> Result<(), ContractError> {
    if was_missing {
        return Ok(());
    }
    tally.newly_missing += 1;
    let now = iso_now();
    persistence_core::repositories::q_core::mark_file_record_missing(pool, &row.id)
        .await
        .map_err(db_err)?;

    bus.publish(
        audit::event_bus::TOPIC_FRAME_MISSING,
        audit::event_bus::Source::System,
        audit::event_bus::FrameMissing {
            frame_id: row.id.clone(),
            root_id: row.root_id.clone(),
            relative_path: row.relative_path.clone(),
            reason: reason.to_owned(),
            at: now.clone(),
        },
    )
    .await
    .map_err(internal)?;

    // spec 048 US5 (FR-024, PATH B): flag any calibration match whose
    // master's raw sub-frames include this now-missing frame.
    emit_calibration_match_flag_for_frame(pool, bus, &row.id, &now, false).await;

    Ok(())
}

/// `inventory.reconcile.run` — on-demand reconcile pass over a root (spec 048
/// T003/T015/US2 groundwork). See module docs for exactly what this pass
/// does and does not apply yet.
///
/// Both reconcile modes report a missing frame identically: the `file_record`
/// row is marked `missing` and its session membership is left intact.
/// Membership is frame-to-session attribution — user knowledge the filesystem
/// cannot supply back (Constitution V, Tier 1) — and every active count,
/// total, and list already filters on `state != 'missing'`
/// (`q_core::active_frame_summaries_batch`, `list_frames`), so removing the id
/// bought nothing and could not be undone when the frame returned.
///
/// # Errors
///
/// Returns `ContractError` (`root.unavailable`) when the root is not
/// registered, or when its directory is not currently walkable — absent
/// (unmounted drive), not a directory, unreadable, or a symlink while
/// `detection.follow_symlinks` is off. In those cases NO frame state is
/// written: an un-walkable root is not evidence that any frame was deleted.
/// Database errors otherwise.
pub async fn run_reconcile(
    pool: &SqlitePool,
    bus: &audit::bus::EventBus,
    req: &InventoryReconcileRunRequest,
) -> Result<InventoryReconcileRunResponse, ContractError> {
    let root_path_str =
        persistence_targets::repositories::inventory::get_library_root_path(pool, &req.root_id)
            .await
            .map_err(db_err)?
            .ok_or_else(|| {
                ContractError::new(
                    ErrorCode::RootUnavailable,
                    format!("library root {} is not registered", req.root_id),
                    ErrorSeverity::Blocking,
                    false,
                )
            })?;
    let root_path = Path::new(&root_path_str);

    let config = app_core_settings::root_config::get_root_config(pool, &req.root_id).await?;

    let known_rows = rows_by_root(pool, &req.root_id).await?;
    let known: Vec<KnownFrame> = known_rows
        .iter()
        .map(|r| KnownFrame {
            id: r.id.clone(),
            relative_path: r.relative_path.clone(),
            recorded_size_bytes: r.size_bytes,
        })
        .collect();

    let report =
        reconcile_root(root_path, &known, config.detection.follow_symlinks).map_err(|reason| {
            ContractError::new(
                ErrorCode::RootUnavailable,
                format!("library root {} cannot be reconciled: {reason}", req.root_id),
                ErrorSeverity::Blocking,
                true,
            )
        })?;
    let reason = format!("{:?}", req.reason).to_lowercase();

    let mut tally = ReconcileTally::default();
    let mut scanned: u32 = 0;

    for entry in &report.entries {
        scanned += 1;
        let Some(row) = known_rows.iter().find(|r| r.id == entry.id) else { continue };
        let was_missing = row.state == "missing";

        match entry.outcome {
            FrameOutcome::Present { real_size_bytes } => {
                apply_present_outcome(pool, bus, row, real_size_bytes, was_missing, &mut tally)
                    .await?;
            }
            FrameOutcome::Missing => {
                apply_missing_outcome(pool, bus, row, was_missing, &reason, &mut tally).await?;
            }
        }
    }

    Ok(InventoryReconcileRunResponse {
        scanned,
        present: tally.present,
        newly_missing: tally.newly_missing,
        recovered: tally.recovered,
        size_backfilled: tally.size_backfilled,
        progress_pct: 100,
    })
}
