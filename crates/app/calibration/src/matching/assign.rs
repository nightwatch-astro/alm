// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `calibration.match.assign` / `.unassign` — persist/remove a calibration
//! master assignment.

use audit::bus::EventBus;
use audit::event_bus::Source;
use audit::{AuditLogEntry, Outcome, Severity};
use calibration_core::assign::{dimension_names, evaluate_assign, AssignError};
use contracts_core::calibration_match::{
    contract_to_kind, kind_to_contract, AssignedDto, CalibrationMatchAssignRequest,
    CalibrationMatchAssignResponse, CalibrationMatchUnassignRequest,
    CalibrationMatchUnassignResponse, UnassignErrorDto, ASSIGN_CONTRACT_VERSION,
    UNASSIGN_CONTRACT_VERSION,
};
use domain_core::ids::{EntityId, Timestamp};
use domain_core::lifecycle::data_asset::EntityType;
use persistence_calibration::repositories::calibration_assignment::{
    self as assign_repo, UpsertParams,
};
use sqlx::SqlitePool;

use super::loaders::{load_config, load_master_by_id, load_session};
use super::responses::error_assign_response;
use crate::audit_ids::audit_entity_id;

/// Record an assignment mutation in the authoritative audit log, then emit it
/// on the live bus.
///
/// #1120: `assign`/`unassign` previously recorded only a `bus.publish`, whose
/// `events` row `crates/audit/src/bus.rs` documents as non-authoritative
/// transient diagnostics — so assignment history was absent from the
/// FR-131/T121 durable record. `topic` and `payload` stay byte-identical to
/// that pre-#1120 publish, leaving live subscribers unaffected; `topic`
/// doubles as the audit `trigger` (the `protection.source.set` precedent).
///
/// # Errors
/// Returns `Err` if the durable `audit_log_entry` insert fails — constitution
/// §II makes that row load-bearing, so the caller's command must fail with it.
/// A bus-emit failure is swallowed inside `write_audit`.
async fn write_assignment_audit(
    bus: &EventBus,
    topic: &str,
    assignment_id: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    let entry = AuditLogEntry::new(
        EntityType::Calibration,
        audit_entity_id("calibration.assignment", assignment_id),
        topic,
        "user",
        Outcome::Applied,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_payload(payload.clone());

    bus.write_audit(entry, topic, Source::User, payload).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// `calibration.match.assign` — persist a calibration master assignment.
///
/// # Errors
/// Returns `Err(String)` on database error.
pub async fn assign(
    pool: &SqlitePool,
    bus: &EventBus,
    req: CalibrationMatchAssignRequest,
) -> Result<CalibrationMatchAssignResponse, String> {
    let config = load_config(pool).await;

    let session = match load_session(pool, &req.session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(error_assign_response(
                &req.request_id,
                "session.not_found",
                &format!("Session {} not found", req.session_id),
                None,
            ));
        }
        Err(e) => return Err(e),
    };

    let master = match load_master_by_id(pool, &req.master_id).await {
        Ok(Some(m)) => m,
        Ok(None) => {
            return Ok(error_assign_response(
                &req.request_id,
                "master.not_found",
                &format!("Master {} not found", req.master_id),
                None,
            ));
        }
        Err(e) => return Err(e),
    };

    match evaluate_assign(&session, &master, req.r#override, &config) {
        Err(AssignError::SessionMixedState) => Ok(error_assign_response(
            &req.request_id,
            "session.mixed_state",
            "Session is mixed; split it first",
            None,
        )),
        Err(AssignError::ObserverLocationMissing) => Ok(error_assign_response(
            &req.request_id,
            "match.observer_location_missing",
            "Session is missing observer_location or exposure_start_utc",
            None,
        )),
        Err(AssignError::IncompatibleDimensions { dimensions }) => {
            let dim_names = dimension_names(&dimensions);
            Ok(error_assign_response(
                &req.request_id,
                "incompatible.dimensions",
                "Master has incompatible hard-rule dimensions; use override=true to force",
                Some(dim_names),
            ))
        }
        Ok(decision) => {
            let calibration_type = kind_to_contract(master.kind)
                .ok_or_else(|| "dark_flat type cannot be assigned in v1".to_owned())?;

            let assignment_id = domain_core::ids::new_id();
            let assigned_at = Timestamp::now_iso();
            let mismatch_names = dimension_names(&decision.mismatched_dimensions);

            assign_repo::upsert(
                pool,
                UpsertParams {
                    id: &assignment_id,
                    session_id: &req.session_id,
                    calibration_type: master.kind.as_str(),
                    master_id: &req.master_id,
                    confidence: decision.confidence,
                    was_override: decision.was_override,
                    mismatched_dimensions: &mismatch_names,
                    assigned_at: Some(&assigned_at),
                },
            )
            .await
            .map_err(|e| e.to_string())?;

            // Durable audit row + live event (T030, #1120).
            write_assignment_audit(
                bus,
                "calibration.assignment.created",
                &assignment_id,
                serde_json::json!({
                    "assignmentId": assignment_id,
                    "sessionId": req.session_id,
                    "masterId": req.master_id,
                    "calibrationType": master.kind.as_str(),
                    "confidence": decision.confidence,
                    "wasOverride": decision.was_override,
                    "mismatchedDimensions": mismatch_names,
                }),
            )
            .await?;

            Ok(CalibrationMatchAssignResponse {
                status: "success".to_owned(),
                contract_version: ASSIGN_CONTRACT_VERSION.to_owned(),
                request_id: req.request_id,
                assigned: Some(AssignedDto {
                    assignment_id,
                    session_id: req.session_id,
                    master_id: req.master_id,
                    calibration_type,
                    was_override: decision.was_override,
                    mismatched_dimensions: if decision.mismatched_dimensions.is_empty() {
                        None
                    } else {
                        Some(dimension_names(&decision.mismatched_dimensions))
                    },
                    assigned_at,
                }),
                confidence: Some(decision.confidence),
                error: None,
            })
        }
    }
}

/// `calibration.match.unassign` — remove a session's assignment for one
/// calibration type, returning it to "no master assigned" for that type.
///
/// # Errors
/// Returns `Err(String)` on database error.
pub async fn unassign(
    pool: &SqlitePool,
    bus: &EventBus,
    req: CalibrationMatchUnassignRequest,
) -> Result<CalibrationMatchUnassignResponse, String> {
    let type_str = contract_to_kind(req.calibration_type).as_str();

    let existing =
        assign_repo::get(pool, &req.session_id, type_str).await.map_err(|e| e.to_string())?;

    let Some(existing) = existing else {
        return Ok(CalibrationMatchUnassignResponse {
            status: "error".to_owned(),
            contract_version: UNASSIGN_CONTRACT_VERSION.to_owned(),
            request_id: req.request_id,
            error: Some(UnassignErrorDto {
                code: "assignment.not_found".to_owned(),
                message: format!("No {type_str} assignment exists for session {}", req.session_id),
            }),
        });
    };

    assign_repo::delete(pool, &req.session_id, type_str).await.map_err(|e| e.to_string())?;

    // Mirrors "calibration.assignment.created" (T030, #1120).
    write_assignment_audit(
        bus,
        "calibration.assignment.removed",
        &existing.id,
        serde_json::json!({
            "assignmentId": existing.id,
            "sessionId": req.session_id,
            "masterId": existing.master_id,
            "calibrationType": type_str,
        }),
    )
    .await?;

    Ok(CalibrationMatchUnassignResponse {
        status: "success".to_owned(),
        contract_version: UNASSIGN_CONTRACT_VERSION.to_owned(),
        request_id: req.request_id,
        error: None,
    })
}
