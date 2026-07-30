// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `mark_resolved` / `mark_missing` / `mark_recovered` — the reconcile pass
//! (on-attach rescan: detect new files + mark gone files as missing).

use audit::bus::EventBus;
use audit::event_bus::{
    ArtifactRecovered, ArtifactUserResolved, Source, TOPIC_ARTIFACT_RECOVERED,
    TOPIC_ARTIFACT_USER_RESOLVED,
};
use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_plans::repositories::artifacts::{self as repo};

/// Mark a `missing` artifact as user-resolved.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn mark_resolved(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    artifact_id: &str,
) -> Result<(), String> {
    repo::mark_artifact_user_resolved(pool, artifact_id)
        .await
        .map_err(|e| format!("DB mark resolved failed: {e}"))?;

    let now = Timestamp::now_iso();
    let _ = bus
        .publish(
            TOPIC_ARTIFACT_USER_RESOLVED,
            Source::System,
            ArtifactUserResolved {
                artifact_id: artifact_id.to_owned(),
                project_id: project_id.to_owned(),
                at: now,
            },
        )
        .await;
    Ok(())
}

/// Mark an artifact as recovered (reconciliation — file back on disk).
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn mark_recovered(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    artifact_id: &str,
    path: &str,
    size_bytes: i64,
) -> Result<(), String> {
    repo::mark_artifact_recovered(pool, artifact_id, size_bytes, None)
        .await
        .map_err(|e| format!("DB mark recovered failed: {e}"))?;

    let now = Timestamp::now_iso();
    let _ = bus
        .publish(
            TOPIC_ARTIFACT_RECOVERED,
            Source::System,
            ArtifactRecovered {
                artifact_id: artifact_id.to_owned(),
                project_id: project_id.to_owned(),
                path: path.to_owned(),
                at: now.clone(),
            },
        )
        .await;

    // spec 048 US5 (FR-025): clear "master missing" on any calibration match
    // whose master's generated master file is this now-recovered artifact.
    emit_calibration_match_flag_for_artifact(pool, bus, artifact_id, &now, true).await;

    Ok(())
}

/// spec 048 US5 (FR-024/025, PATH A): emit `calibration_match.source_missing`
/// / `.source_recovered` for every calibration match whose master's
/// generated master file is `artifact_id`. Best-effort — a lookup/publish
/// failure here must not fail the artifact reconcile pass, since the flag is
/// re-derived live on next read regardless (never the durable record).
async fn emit_calibration_match_flag_for_artifact(
    pool: &SqlitePool,
    bus: &EventBus,
    artifact_id: &str,
    at: &str,
    recovered: bool,
) {
    let Ok(assignments) =
        persistence_calibration::repositories::calibration_assignment::find_by_source_artifact(
            pool,
            artifact_id,
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
                    Source::System,
                    audit::event_bus::CalibrationMatchSourceRecovered {
                        match_id: assignment.id,
                        frame_id: artifact_id.to_owned(),
                        at: at.to_owned(),
                    },
                )
                .await;
        } else {
            let _ = bus
                .publish(
                    audit::event_bus::TOPIC_CALIBRATION_MATCH_SOURCE_MISSING,
                    Source::System,
                    audit::event_bus::CalibrationMatchSourceMissing {
                        match_id: assignment.id,
                        frame_id: artifact_id.to_owned(),
                        at: at.to_owned(),
                    },
                )
                .await;
        }
    }
}
