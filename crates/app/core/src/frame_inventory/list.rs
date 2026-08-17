// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `inventory.frame.list` — read-only projection of `file_record` rows.

use contracts_core::error_code::ErrorCode;
use contracts_core::inventory_frame::{
    FramePresenceState, InventoryFrame, InventoryFrameListRequest, InventoryFrameListResponse,
    RawFrameType,
};
use contracts_core::{ContractError, ErrorSeverity};
use sqlx::SqlitePool;

use app_core_errors::db_err;

use super::{
    build_frame_session_map, raw_frame_type_from_calibration_kind, rows_by_ids, rows_by_root,
};

fn presence_state(state: &str) -> FramePresenceState {
    match state {
        "missing" => FramePresenceState::Missing,
        "protected" => FramePresenceState::Protected,
        _ => FramePresenceState::Present,
    }
}

async fn frame_ids_for_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<(Vec<String>, RawFrameType)>, ContractError> {
    if let Some(frame_ids_json) =
        persistence_core::repositories::q_core::get_acquisition_session_frame_ids(pool, session_id)
            .await
            .map_err(db_err)?
    {
        let ids: Vec<String> = serde_json::from_str(&frame_ids_json).unwrap_or_default();
        return Ok(Some((ids, RawFrameType::Light)));
    }

    if let Some((frame_ids_json, kind)) =
        persistence_core::repositories::q_core::get_calibration_session_frame_ids_and_kind(
            pool, session_id,
        )
        .await
        .map_err(db_err)?
    {
        let ids: Vec<String> = serde_json::from_str(&frame_ids_json).unwrap_or_default();
        return Ok(Some((ids, raw_frame_type_from_calibration_kind(&kind))));
    }

    Ok(None)
}

/// `inventory.frame.list` — list per-frame inventory entries for a session or
/// root (spec 048 T006/T014). `present_*` totals exclude `missing` frames.
///
/// # Errors
///
/// Returns `ContractError` (`internal.database`) on a query failure, or
/// `internal.error` when neither `session_id` nor `root_id` is set.
pub async fn list_frames(
    pool: &SqlitePool,
    req: &InventoryFrameListRequest,
) -> Result<InventoryFrameListResponse, ContractError> {
    let include_missing = req.include_missing.unwrap_or(false);

    let mut frames = Vec::new();

    if let Some(session_id) = &req.scope.session_id {
        let Some((ids, frame_type)) = frame_ids_for_session(pool, session_id).await? else {
            return Ok(InventoryFrameListResponse {
                frames: Vec::new(),
                present_count: 0,
                present_size_bytes: 0,
            });
        };
        for row in rows_by_ids(pool, &ids).await? {
            let state = presence_state(&row.state);
            if !include_missing && state == FramePresenceState::Missing {
                continue;
            }
            frames.push(InventoryFrame {
                frame_id: row.id,
                root_id: row.root_id,
                relative_path: row.relative_path,
                frame_type,
                size_bytes: row.size_bytes,
                state,
                session_id: Some(session_id.clone()),
            });
        }
    } else if let Some(root_id) = &req.scope.root_id {
        // DSD-8: batch-build the reverse map once instead of a per-frame LIKE scan.
        let frame_map = build_frame_session_map(pool).await?;
        for row in rows_by_root(pool, root_id).await? {
            let state = presence_state(&row.state);
            if !include_missing && state == FramePresenceState::Missing {
                continue;
            }
            let (session_id, frame_type) = frame_map
                .get(&row.id)
                .map_or((None, RawFrameType::Light), |(sid, ft)| (Some(sid.clone()), *ft));
            frames.push(InventoryFrame {
                frame_id: row.id,
                root_id: row.root_id,
                relative_path: row.relative_path,
                frame_type,
                size_bytes: row.size_bytes,
                state,
                session_id,
            });
        }
    } else {
        return Err(ContractError::new(
            ErrorCode::InternalError,
            "inventory.frame.list: scope must set session_id or root_id".to_owned(),
            ErrorSeverity::Warning,
            false,
        ));
    }

    let present_count =
        u32::try_from(frames.iter().filter(|f| f.state != FramePresenceState::Missing).count())
            .unwrap_or(u32::MAX);
    let present_size_bytes = frames
        .iter()
        .filter(|f| f.state != FramePresenceState::Missing)
        .map(|f| f.size_bytes)
        .sum();

    Ok(InventoryFrameListResponse { frames, present_count, present_size_bytes })
}
