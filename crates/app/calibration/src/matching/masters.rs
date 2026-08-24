// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `calibration.masters.list` / `.get` (T037, FR-013).

use std::sync::Arc;

use calibration_core::{suggest as domain_suggest, SessionInfo};
use contracts_core::equipment::Camera;
use persistence_calibration::repositories::calibration_assignment as assign_repo;
use persistence_calibration::repositories::equipment as equipment_repo;
use persistence_calibration::repositories::q_calibration;
use sqlx::SqlitePool;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::caches;
use crate::equipment::resolve_camera_display_name;

use super::loaders::{load_config, load_master_by_id};

/// Render a master fingerprint's raw camera/optic-train header string as the
/// registered camera name when one claims it (#879), falling back to the raw
/// string so an unregistered camera still renders what the header said.
fn display_camera(cameras: &[Camera], raw: Option<String>) -> Option<String> {
    raw.map(|raw| resolve_camera_display_name(cameras, &raw).unwrap_or(raw))
}

/// The registered cameras used to resolve fingerprint display names.
///
/// Equipment being unreadable degrades to raw header strings rather than
/// failing the masters read, mirroring the planner's FR-038 fallback.
async fn display_cameras(pool: &SqlitePool) -> Vec<Camera> {
    equipment_repo::list_cameras(pool).await.unwrap_or_default()
}

/// `calibration.masters.list` — return all calibration masters, reading
/// through the process-global `caches::calibration_masters` snapshot
/// (in-memory caching layer, F0). A hit returns the cached list without a DB
/// round trip; a miss loads from the DB, stores the snapshot, and returns it.
///
/// # Errors
/// Returns `Err(String)` on database failure.
pub async fn masters_list(
    pool: &SqlitePool,
) -> Result<Vec<contracts_core::calibration::CalibrationMaster>, String> {
    if let Some(cached) = caches::calibration_masters().load() {
        return Ok((*cached).clone());
    }
    let masters = masters_list_from_db(pool).await?;
    caches::store_calibration_masters(Arc::new(masters.clone()));
    Ok(masters)
}

/// Load all calibration masters from `calibration_master_view` (migration
/// 0033), which joins `calibration_session` with `calibration_fingerprint`.
async fn masters_list_from_db(
    pool: &SqlitePool,
) -> Result<Vec<contracts_core::calibration::CalibrationMaster>, String> {
    let rows = q_calibration::list_calibration_masters(pool).await.map_err(|e| e.to_string())?;

    let cameras = display_cameras(pool).await;
    let now = OffsetDateTime::now_utc();
    let masters = rows
        .into_iter()
        .map(|r| {
            let age_days = compute_age_days(&r.created_at, now);
            let cal_kind = str_to_cal_kind(&r.kind);
            contracts_core::calibration::CalibrationMaster {
                id: r.id.clone(),
                kind: cal_kind,
                fingerprint: contracts_core::calibration::CalibrationFingerprint {
                    // Q16 / FR-136: no absence-synthesizing fallbacks — an
                    // absent row field stays absent; a present one renders as
                    // the registered camera name when one claims it (#879).
                    camera: display_camera(&cameras, r.fp_optic_train),
                    sensor_mode: None,
                    exposure_s: r.fp_exposure_s,
                    temp_c: r.fp_temp_c,
                    gain: r.fp_gain,
                    binning: r.fp_binning,
                    filter: r.fp_filter_name,
                },
                source_session_id: r.source_session_id,
                created_at: r.created_at,
                age_days,
                size_bytes: r.size_bytes.and_then(|b| u64::try_from(b).ok()),
                used_by_session_ids: vec![],
                used_by_project_ids: vec![],
                root_id: r.root_id,
                relative_path: r.frame_relative_path,
            }
        })
        .collect();

    Ok(masters)
}

/// `calibration.masters.get` — return detail for a single calibration master.
///
/// # Errors
/// Returns `Err(String)` when the master is not found or on database failure.
pub async fn masters_get(
    pool: &SqlitePool,
    master_id: &str,
) -> Result<contracts_core::calibration::MasterDetail, String> {
    let row =
        q_calibration::get_calibration_master(pool, master_id).await.map_err(|e| e.to_string())?;

    let r = row.ok_or_else(|| format!("master.not_found: {master_id}"))?;

    let now = OffsetDateTime::now_utc();
    let age_days = compute_age_days(&r.created_at, now);
    let cal_kind = str_to_cal_kind(&r.kind);

    // Load sessions assigned to this master via calibration_assignment.
    let used_by_session_ids =
        q_calibration::list_assignment_session_ids(pool, &r.id).await.unwrap_or_default();

    // Load projects linked to sessions that use this master.
    let used_by_project_ids =
        q_calibration::list_assignment_project_ids(pool, &r.id).await.unwrap_or_default();

    let session_count = u32::try_from(used_by_session_ids.len()).unwrap_or(0);
    let project_count = u32::try_from(used_by_project_ids.len()).unwrap_or(0);

    let missing_flag = compute_missing_flag(pool, &r.id).await?;

    let compatible_sessions = compute_compatible_sessions(pool, &r.id).await?;

    Ok(contracts_core::calibration::MasterDetail {
        id: r.id.clone(),
        kind: cal_kind,
        fingerprint: contracts_core::calibration::CalibrationFingerprint {
            // Q16 / FR-136: no absence-synthesizing fallbacks — an absent row
            // field stays absent; a present one renders as the registered
            // camera name when one claims it (#879).
            camera: display_camera(&display_cameras(pool).await, r.fp_optic_train),
            sensor_mode: None,
            exposure_s: r.fp_exposure_s,
            temp_c: r.fp_temp_c,
            gain: r.fp_gain,
            binning: r.fp_binning,
            filter: r.fp_filter_name,
        },
        source_session_id: r.source_session_id,
        created_at: r.created_at,
        age_days,
        size_bytes: r.size_bytes.and_then(|b| u64::try_from(b).ok()),
        used_by_session_ids,
        used_by_project_ids,
        root_id: r.root_id,
        relative_path: r.frame_relative_path,
        compatible_sessions,
        usage_stats: contracts_core::calibration::MasterUsageStats { session_count, project_count },
        missing_flag,
    })
}

/// #868: compute the real "compatible sessions" list for a master — every
/// light session the domain matcher would surface this master as a candidate
/// for, scored via the same `calibration_core::suggest` path used by
/// `calibration.match.suggest` (single master, so at most one match per
/// session).
async fn compute_compatible_sessions(
    pool: &SqlitePool,
    master_id: &str,
) -> Result<Vec<contracts_core::calibration::CompatibleSessionEntry>, String> {
    let Some(master) = load_master_by_id(pool, master_id).await? else {
        return Ok(vec![]);
    };

    let config = load_config(pool).await;
    let sessions = q_calibration::list_light_acquisition_fingerprints(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut entries = Vec::new();
    for row in sessions {
        let session = SessionInfo {
            id: row.id.clone(),
            session_type: row.session_type.unwrap_or_else(|| "light".to_owned()),
            gain: row.gain,
            offset: row.offset_val,
            exposure_s: row.exposure_s,
            temp_c: row.temp_c,
            filter: row.filter_name,
            rotation_deg: row.rotation_deg,
            binning: row.binning,
            optic_train: row.optic_train,
            camera_body_id: row.camera_body_id,
            observing_night_date: row.observing_night_date,
            has_observer_location: row.has_observer_location.unwrap_or(0) != 0,
            has_exposure_start_utc: row.has_exposure_start_utc.unwrap_or(0) != 0,
        };

        // Single-master suggest: guard errors (mixed-state) just exclude the
        // session, matching how `suggest()` filters unusable candidates.
        if let Ok(matches) =
            domain_suggest(&session, std::slice::from_ref(&master), &[master.kind], &config)
        {
            if let Some(m) = matches.into_iter().next() {
                let soft_mismatches =
                    m.dimensions_mismatched.iter().map(|d| d.dimension.clone()).collect();
                entries.push(contracts_core::calibration::CompatibleSessionEntry {
                    session_id: row.id,
                    score: m.confidence,
                    soft_mismatches,
                });
            }
        }
    }

    Ok(entries)
}

/// spec 048 US5 (FR-024/025): compute a master's derived "missing" flag from
/// live presence state. Always recomputed — never cached — so it clears
/// automatically once the referenced frame/artifact returns to present
/// (data-model.md "Calibration match flag — derived annotation").
///
/// PATH A (generated master artifact missing) takes precedence over PATH B
/// (raw source sub missing) when both apply: a missing generated file is the
/// more actionable / specific signal for the user to act on.
///
/// # Errors
/// Returns `Err(String)` on database failure.
async fn compute_missing_flag(
    pool: &SqlitePool,
    master_id: &str,
) -> Result<Option<contracts_core::calibration::CalibrationMatchMissingFlag>, String> {
    use contracts_core::calibration::CalibrationMatchMissingFlag as Flag;

    if let Some(state) =
        assign_repo::master_artifact_state(pool, master_id).await.map_err(|e| e.to_string())?
    {
        if state != "present" {
            return Ok(Some(Flag::MasterMissing));
        }
    }

    if assign_repo::master_has_missing_source_frame(pool, master_id)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(Some(Flag::SourceSubsMissing));
    }

    Ok(None)
}

fn str_to_cal_kind(kind: &str) -> contracts_core::calibration::CalibrationKind {
    // Canonical parser handles the `flat_dark` legacy alias; unknown values
    // fall back to Dark, preserving prior behavior.
    kind.parse().unwrap_or(contracts_core::calibration::CalibrationKind::Dark)
}

fn compute_age_days(created_at: &str, now: OffsetDateTime) -> u32 {
    if let Ok(created) = time::OffsetDateTime::parse(created_at, &Rfc3339) {
        let diff = now - created;
        u32::try_from(diff.whole_days().max(0)).unwrap_or(0)
    } else {
        0
    }
}
