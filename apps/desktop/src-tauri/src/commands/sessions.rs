// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 029 / spec 037 session commands exposed to the Tauri webview.
//!
//! Real implementations (T037):
//!   `sessions_list` -- backed by `app_core::sessions::list_sessions` (real DB).
//!   `sessions_get`  -- backed by `app_core::sessions::get_session` (real DB).
//!
//! Remaining stubs (no persistence layer yet):
//!   `sessions_calendar`, `sessions_split`, `sessions_merge`.
//!
//! Spec 041 FR-051 (T076, Phase 13): `sessions_transition` — a stub
//! Confirm/Re-open/Reject-style state transition — was removed. Sessions are
//! derived, already-confirmed inventory with no review lifecycle.

use app_core::sessions as sessions_uc;
use contracts_core::sessions::{
    AcquisitionSession, CalendarData, CalendarDay, CalendarMonth, CalendarSessionStub,
    ConfidenceLevel, MetaValue, ProvenanceOrigin, SessionDetail, SessionKey,
};
use contracts_core::ContractError;
use contracts_core::JsonAny;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use tauri::State;

use crate::AppState;

/// Wrapper for `sessions.split` return value.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionSplitResult {
    pub original: AcquisitionSession,
    pub new: AcquisitionSession,
}

/// `sessions.list` -- returns all acquisition sessions from real DB rows.
///
/// Backed by `acquisition_session` table (migration 0002). Returns an empty
/// list when no sessions exist (not fixtures).
///
/// # Errors
/// Returns `Err(String)` on database failure.
#[tauri::command]
#[specta::specta]
pub async fn sessions_list(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<AcquisitionSession>, ContractError> {
    let limit = limit.map(|l| l.clamp(1, 500));
    tracing::debug!("sessions.list limit={limit:?} offset={offset:?}");
    sessions_uc::list_sessions_paginated(state.repo.pool(), limit, offset)
        .await
        .map_err(ContractError::internal)
}

/// `sessions.get` -- returns a single session detail from real DB rows.
///
/// Returns `Err("session.not_found: <id>")` when the session does not exist.
///
/// # Errors
/// Returns `Err(String)` on database failure or when the session is absent.
#[tauri::command]
#[specta::specta]
pub async fn sessions_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<SessionDetail, ContractError> {
    tracing::debug!("sessions.get id={id}");
    sessions_uc::get_session(state.repo.pool(), &id).await.map_err(ContractError::internal)
}

/// `sessions.calendar` — returns calendar data for a month range.
///
/// # Errors
/// Returns `Err(String)` on failure; the stub never fails.
#[tauri::command]
#[specta::specta]
pub async fn sessions_calendar(
    start_month: String,
    end_month: String,
) -> Result<CalendarData, ContractError> {
    tracing::debug!("stub: sessions.calendar start={start_month} end={end_month}");
    Ok(CalendarData {
        months: vec![CalendarMonth {
            year: 2026,
            month: 5,
            days: vec![
                CalendarDay {
                    day: 18,
                    sessions: vec![CalendarSessionStub {
                        id: "ses-001".to_owned(),
                        target: "M31".to_owned(),
                        filter: "L".to_owned(),
                    }],
                },
                CalendarDay {
                    day: 19,
                    sessions: vec![
                        CalendarSessionStub {
                            id: "ses-003".to_owned(),
                            target: "M31".to_owned(),
                            filter: "R".to_owned(),
                        },
                        CalendarSessionStub {
                            id: "ses-004".to_owned(),
                            target: "M31".to_owned(),
                            filter: "G".to_owned(),
                        },
                    ],
                },
                CalendarDay {
                    day: 20,
                    sessions: vec![CalendarSessionStub {
                        id: "ses-005".to_owned(),
                        target: "NGC 7000".to_owned(),
                        filter: "Ha".to_owned(),
                    }],
                },
            ],
        }],
    })
}

/// `sessions.split` — split a session at a given frame index.
///
/// # Errors
/// Returns `Err(String)` on failure; the stub never fails.
#[tauri::command]
#[specta::specta]
pub async fn sessions_split(
    id: String,
    split_at_index: u32,
) -> Result<SessionSplitResult, ContractError> {
    tracing::debug!("stub: sessions.split id={id} split_at_index={split_at_index}");
    let sessions = stub_sessions();
    let mut original = sessions[0].clone();
    original.id = id;
    original.frame_count = split_at_index;

    let mut new_session = sessions[1].clone();
    "550e8400-e29b-41d4-a716-446655440099".clone_into(&mut new_session.id);
    new_session.frame_count = 18_u32.saturating_sub(split_at_index);

    Ok(SessionSplitResult { original, new: new_session })
}

/// `sessions.merge` — merge multiple sessions into one.
///
/// # Errors
/// Returns `Err(String)` on failure; the stub never fails.
#[tauri::command]
#[specta::specta]
pub async fn sessions_merge(ids: Vec<String>) -> Result<AcquisitionSession, ContractError> {
    tracing::debug!("stub: sessions.merge ids={ids:?}");
    let mut merged = stub_sessions()
        .into_iter()
        .next()
        .ok_or_else(|| ContractError::internal("no stub session available"))?;
    merged.id = ids.into_iter().next().unwrap_or_default();
    merged.frame_count = 30;
    merged.total_integration_seconds = 18000.0;
    Ok(merged)
}

// ---------------------------------------------------------------------------
// Fixture data
// ---------------------------------------------------------------------------

/// Optical train and target UUIDs matching the frontend fixture constants.
mod ids {
    pub const TRAIN_FSQ106: &str = "550e8400-e29b-41d4-a716-446655440101";
    pub const TRAIN_GT81: &str = "550e8400-e29b-41d4-a716-446655440102";
    pub const TARGET_NGC7000: &str = "550e8400-e29b-41d4-a716-446655440201";
    pub const TARGET_M31: &str = "550e8400-e29b-41d4-a716-446655440202";
    pub const TARGET_IC1396: &str = "550e8400-e29b-41d4-a716-446655440203";
    pub const TARGET_M42: &str = "550e8400-e29b-41d4-a716-446655440204";
    pub const PROJECT_NGC7000_NB: &str = "550e8400-e29b-41d4-a716-446655440301";
}

fn meta(
    value: &str,
    raw: &str,
    origin: ProvenanceOrigin,
    confidence: ConfidenceLevel,
    evidence_ref: Option<&str>,
) -> MetaValue {
    MetaValue {
        value: JsonAny::from(serde_json::json!(value)),
        raw: Some(raw.to_owned()),
        origin,
        confidence,
        evidence_ref: evidence_ref.map(ToOwned::to_owned),
    }
}

fn key(target: &str, filter: &str, binning: &str, gain: &str, night: &str) -> SessionKey {
    SessionKey {
        target: target.to_owned(),
        filter: filter.to_owned(),
        binning: binning.to_owned(),
        gain: gain.to_owned(),
        night: night.to_owned(),
    }
}

fn stub_sessions() -> Vec<AcquisitionSession> {
    vec![
        stub_session_ngc7000_ha(),
        stub_session_ic1396_sii(),
        stub_session_m31_l(),
        stub_session_ngc7000_oiii(),
        stub_session_m42_oiii(),
    ]
}

/// Discovered — NGC 7000 Ha.
fn stub_session_ngc7000_ha() -> AcquisitionSession {
    AcquisitionSession {
        id: "550e8400-e29b-41d4-a716-446655440001".to_owned(),
        session_key: key("NGC 7000", "Ha", "1", "100", "2026-04-12"),
        confidence: ConfidenceLevel::Unknown,
        optical_train_id: ids::TRAIN_FSQ106.to_owned(),
        frame_count: 18,
        total_integration_seconds: 10800.0,
        total_size_bytes: 1_258_291_200,
        metadata: HashMap::from([
            (
                "target".to_owned(),
                meta(
                    "NGC 7000",
                    "NGC7000",
                    ProvenanceOrigin::Observed,
                    ConfidenceLevel::Medium,
                    None,
                ),
            ),
            (
                "filter".to_owned(),
                meta("Ha", "H-alpha 7nm", ProvenanceOrigin::Observed, ConfidenceLevel::High, None),
            ),
        ]),
        target_ids: vec![ids::TARGET_NGC7000.to_owned()],
        project_ids: vec![],
        warnings: vec!["target not yet confirmed".to_owned()],
    }
}

/// Discovered — IC 1396 SII.
fn stub_session_ic1396_sii() -> AcquisitionSession {
    AcquisitionSession {
        id: "550e8400-e29b-41d4-a716-446655440002".to_owned(),
        session_key: key("IC 1396", "SII", "1", "100", "2026-04-14"),
        confidence: ConfidenceLevel::Low,
        optical_train_id: ids::TRAIN_FSQ106.to_owned(),
        frame_count: 12,
        total_integration_seconds: 7200.0,
        total_size_bytes: 838_860_800,
        metadata: HashMap::from([(
            "target".to_owned(),
            meta(
                "IC 1396",
                "IC1396",
                ProvenanceOrigin::Inferred,
                ConfidenceLevel::Low,
                Some("fits.object"),
            ),
        )]),
        target_ids: vec![ids::TARGET_IC1396.to_owned()],
        project_ids: vec![],
        warnings: vec!["target confidence low".to_owned(), "no calibration match found".to_owned()],
    }
}

/// Needs review — M31 L.
fn stub_session_m31_l() -> AcquisitionSession {
    AcquisitionSession {
        id: "550e8400-e29b-41d4-a716-446655440003".to_owned(),
        session_key: key("M31", "L", "1", "0", "2026-03-28"),
        confidence: ConfidenceLevel::Medium,
        optical_train_id: ids::TRAIN_GT81.to_owned(),
        frame_count: 60,
        total_integration_seconds: 5400.0,
        total_size_bytes: 2_147_483_648,
        metadata: HashMap::from([(
            "target".to_owned(),
            meta("M31", "M31", ProvenanceOrigin::Observed, ConfidenceLevel::High, None),
        )]),
        target_ids: vec![ids::TARGET_M31.to_owned()],
        project_ids: vec![],
        warnings: vec!["filter origin is inferred \u{2014} please verify".to_owned()],
    }
}

/// Confirmed — NGC 7000 OIII.
fn stub_session_ngc7000_oiii() -> AcquisitionSession {
    AcquisitionSession {
        id: "550e8400-e29b-41d4-a716-446655440005".to_owned(),
        session_key: key("NGC 7000", "OIII", "1", "100", "2026-04-15"),
        confidence: ConfidenceLevel::Confirmed,
        optical_train_id: ids::TRAIN_FSQ106.to_owned(),
        frame_count: 15,
        total_integration_seconds: 9000.0,
        total_size_bytes: 1_048_576_000,
        metadata: HashMap::from([
            (
                "target".to_owned(),
                meta(
                    "NGC 7000",
                    "NGC7000",
                    ProvenanceOrigin::Reviewed,
                    ConfidenceLevel::Confirmed,
                    None,
                ),
            ),
            (
                "filter".to_owned(),
                meta(
                    "OIII",
                    "OIII 6.5nm",
                    ProvenanceOrigin::Reviewed,
                    ConfidenceLevel::Confirmed,
                    None,
                ),
            ),
        ]),
        target_ids: vec![ids::TARGET_NGC7000.to_owned()],
        project_ids: vec![ids::PROJECT_NGC7000_NB.to_owned()],
        warnings: vec![],
    }
}

/// Rejected — M42 OIII.
fn stub_session_m42_oiii() -> AcquisitionSession {
    AcquisitionSession {
        id: "550e8400-e29b-41d4-a716-446655440009".to_owned(),
        session_key: key("M42", "OIII", "1", "100", "2026-02-11"),
        confidence: ConfidenceLevel::Rejected,
        optical_train_id: ids::TRAIN_GT81.to_owned(),
        frame_count: 8,
        total_integration_seconds: 4800.0,
        total_size_bytes: 560_000_000,
        metadata: HashMap::from([(
            "target".to_owned(),
            meta("M42", "M42", ProvenanceOrigin::Observed, ConfidenceLevel::High, None),
        )]),
        target_ids: vec![ids::TARGET_M42.to_owned()],
        project_ids: vec![],
        warnings: vec![
            "high cloud cover during capture".to_owned(),
            "star FWHM > 6 arcsec".to_owned(),
        ],
    }
}
