// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `calibration.match.suggest` — single-session suggest.

use calibration_core::{suggest as domain_suggest, CalibrationKind};
use contracts_core::calibration_match::{
    contract_to_kind, match_to_dto, CalibrationMatchSuggestRequest,
    CalibrationMatchSuggestResponse, SuggestStatus, SUGGEST_CONTRACT_VERSION,
};
use sqlx::SqlitePool;

use calibration_core::ranking::multi_kind_suggest_status;

use super::context::{apply_session_context, load_session_contexts};
use super::loaders::{load_config, load_masters, load_session};
use super::responses::{error_suggest_response, guard_error_suggest_response};

/// `calibration.match.suggest` — suggest ranked calibration masters for one session.
///
/// # Errors
/// Returns `Err(String)` on database error.
pub async fn suggest(
    pool: &SqlitePool,
    req: CalibrationMatchSuggestRequest,
) -> Result<CalibrationMatchSuggestResponse, String> {
    let config = load_config(pool).await;

    let session = match load_session(pool, &req.session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(error_suggest_response(
                &req.request_id,
                "session.not_found",
                &format!("Session {} not found", req.session_id),
            ));
        }
        Err(e) => return Err(e),
    };

    let calibration_types: Vec<CalibrationKind> = req
        .calibration_types
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|ct| contract_to_kind(*ct))
        .collect();

    let types_ref: Vec<CalibrationKind> = if calibration_types.is_empty() {
        vec![CalibrationKind::Dark, CalibrationKind::Flat, CalibrationKind::Bias]
    } else {
        calibration_types
    };

    let masters = load_masters(pool, &types_ref).await?;

    match domain_suggest(&session, &masters, &types_ref, &config) {
        Err(error_code) => Ok(guard_error_suggest_response(&req.request_id, &error_code)),
        Ok(matches) => {
            // A suggest request covers up to three calibration kinds and the
            // match list is ranked only within each kind, so the status must
            // reduce per kind rather than compare the two leading entries.
            let status_str = multi_kind_suggest_status(&matches);
            let suggest_status = match status_str {
                "match" => SuggestStatus::Match,
                "ambiguous" => SuggestStatus::Ambiguous,
                _ => SuggestStatus::NoMatch,
            };
            // P9: enrich with session context (target/filter/night/frame
            // count) — one batched lookup, not N+1. All candidates share the
            // same `req.session_id` here (single-session suggest).
            let session_ctx =
                load_session_contexts(pool, std::slice::from_ref(&req.session_id)).await;
            let dto_matches: Vec<_> = matches
                .iter()
                .filter_map(match_to_dto)
                .map(|dto| apply_session_context(dto, &session_ctx))
                .collect();
            Ok(CalibrationMatchSuggestResponse {
                status: "success".to_owned(),
                contract_version: SUGGEST_CONTRACT_VERSION.to_owned(),
                request_id: req.request_id,
                suggest_status: Some(suggest_status),
                matches: Some(dto_matches),
                error: None,
            })
        }
    }
}
