// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Batched forms of the on-attach reconciliation phases (kyo7.54).
//!
//! The per-row entry points (`detect`, `mark_missing`,
//! `repo::touch_artifact`) issue one implicit commit per artifact, so a project
//! with thousands of outputs pays thousands of commits every time its drawer
//! opens. Each function here collapses one reconcile phase into a single
//! transaction plus a single batched audit publish, with the same observable
//! per-row outcome.
//!
//! Constitution V: everything written here — `last_seen_at`, `state`,
//! classification, and the audit rows — is re-derivable from a rescan (Tier 2),
//! so batched writes are permitted. Records carrying a user decision are not in
//! this path: `mark_resolved` stays a synchronous single write.

use audit::bus::EventBus;
use audit::event_bus::{
    ArtifactClassified, ArtifactDetected, ArtifactMissing, ArtifactScanIncomplete, ArtifactUpdated,
    CalibrationMatchSourceMissing, Source, TOPIC_ARTIFACT_CLASSIFIED, TOPIC_ARTIFACT_DETECTED,
    TOPIC_ARTIFACT_MISSING, TOPIC_ARTIFACT_SCAN_INCOMPLETE, TOPIC_ARTIFACT_UPDATED,
    TOPIC_CALIBRATION_MATCH_SOURCE_MISSING,
};
use domain_core::ids::{new_id, Timestamp};
use sqlx::SqlitePool;
use workflow_artifacts::{attribute, classify, default_artifact_rules, DEFAULT_ATTRIBUTION_WINDOW};

use persistence_plans::repositories::artifacts::{self as repo, InsertArtifact};

use super::{load_launch_refs, parse_dt};

/// One event ready for [`EventBus::publish_batch`].
type BatchEvent = (String, Source, serde_json::Value);

fn event<P: serde::Serialize>(topic: &str, payload: &P) -> Option<BatchEvent> {
    serde_json::to_value(payload).ok().map(|v| (topic.to_owned(), Source::System, v))
}

/// Refresh `last_seen_at` for every still-present artifact in one statement
/// (reconcile seen phase). Emits no events, exactly like `touch_artifact`.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn touch_seen(pool: &SqlitePool, artifact_ids: &[String]) -> Result<(), String> {
    repo::touch_artifacts(pool, artifact_ids)
        .await
        .map_err(|e| format!("DB touch artifacts failed: {e}"))
}

/// An artifact the reconcile scan found gone from disk.
#[derive(Clone, Debug)]
pub struct GoneArtifact {
    pub id: String,
    pub path: String,
}

/// Transition every gone artifact to `missing` in one statement and publish all
/// `artifact.missing` plus `calibration_match.source_missing` events in one
/// transaction (reconcile gone phase).
///
/// The calibration flags stay best-effort as in [`super::mark_missing`]: a
/// lookup failure drops them rather than failing the phase, because the flag is
/// re-derived on the next read.
///
/// # Errors
/// Returns `Err(String)` if the state transition fails. A failed audit publish
/// is logged, not returned — the state rows are the durable record.
pub async fn mark_missing_batch(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    gone: &[GoneArtifact],
) -> Result<(), String> {
    if gone.is_empty() {
        return Ok(());
    }
    let ids: Vec<String> = gone.iter().map(|g| g.id.clone()).collect();
    repo::mark_artifacts_missing(pool, &ids)
        .await
        .map_err(|e| format!("DB mark missing failed: {e}"))?;

    let now = Timestamp::now_iso();
    let mut events: Vec<BatchEvent> = gone
        .iter()
        .filter_map(|g| {
            event(
                TOPIC_ARTIFACT_MISSING,
                &ArtifactMissing {
                    artifact_id: g.id.clone(),
                    project_id: project_id.to_owned(),
                    path: g.path.clone(),
                    at: now.clone(),
                },
            )
        })
        .collect();

    if let Ok(matches) =
        persistence_calibration::repositories::calibration_assignment::find_match_ids_by_source_artifacts(
            pool, &ids,
        )
        .await
    {
        events.extend(matches.into_iter().filter_map(|(artifact_id, match_id)| {
            event(
                TOPIC_CALIBRATION_MATCH_SOURCE_MISSING,
                &CalibrationMatchSourceMissing {
                    match_id,
                    frame_id: artifact_id,
                    at: now.clone(),
                },
            )
        }));
    }

    publish_all(bus, &events).await;
    Ok(())
}

/// Record that a reconcile scan could not read `unreadable_paths`, so its
/// coverage of the project is partial.
///
/// Nothing is written to the artifact rows: paths under an unreadable directory
/// are unknown state, and the audit row is the only record that the scan skipped
/// them.
pub async fn report_scan_incomplete(bus: &EventBus, project_id: &str, unreadable_paths: &[String]) {
    if unreadable_paths.is_empty() {
        return;
    }
    let events: Vec<BatchEvent> = event(
        TOPIC_ARTIFACT_SCAN_INCOMPLETE,
        &ArtifactScanIncomplete {
            project_id: project_id.to_owned(),
            unreadable_paths: unreadable_paths.to_vec(),
            at: Timestamp::now_iso(),
        },
    )
    .into_iter()
    .collect();
    publish_all(bus, &events).await;
}

/// A file the reconcile scan found on disk.
#[derive(Clone, Debug)]
pub struct DetectedFile {
    pub path: String,
    pub size_bytes: i64,
    pub file_mtime: String,
    pub detected_at: String,
}

/// Record every scanned file in one transaction, then publish all resulting
/// events in one transaction (reconcile detect phase).
///
/// Per-file semantics match [`super::detect`]: a path already in the DB is
/// updated in place and emits `artifact.updated`; a new path is inserted and
/// emits `artifact.detected` + `artifact.classified`; a row whose insert loses
/// the `(project_id, path)` race emits nothing.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn detect_batch(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    tool: &str,
    files: &[DetectedFile],
) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    // One read of the project's rows replaces a per-path lookup, and one launch
    // load replaces a per-file load.
    let existing = repo::list_artifacts_for_project(pool, project_id, &[])
        .await
        .map_err(|e| format!("DB lookup failed: {e}"))?;
    let launches = load_launch_refs(pool, project_id, tool).await?;
    let rules = default_artifact_rules();

    let mut updates: Vec<(&DetectedFile, &repo::ArtifactRow)> = Vec::new();
    let mut inserts: Vec<(&DetectedFile, String, workflow_artifacts::ClassificationResult)> =
        Vec::new();
    for file in files {
        if let Some(row) = existing.iter().find(|r| r.path == file.path) {
            updates.push((file, row));
        } else {
            let file_name = std::path::Path::new(&file.path)
                .file_name()
                .map_or_else(|| file.path.clone(), |n| n.to_string_lossy().into_owned());
            inserts.push((file, new_id(), classify(&file_name, &rules)));
        }
    }

    let launch_ids: Vec<Option<String>> = inserts
        .iter()
        .map(|(file, _, _)| {
            parse_dt(&file.detected_at)
                .and_then(|dt| attribute(tool, dt, &launches, DEFAULT_ATTRIBUTION_WINDOW))
        })
        .collect();

    let mut tx = pool.begin().await.map_err(|e| format!("DB begin failed: {e}"))?;
    for (file, row) in &updates {
        repo::update_artifact_inplace(&mut *tx, &row.id, file.size_bytes, None)
            .await
            .map_err(|e| format!("DB update failed: {e}"))?;
    }
    let rows: Vec<InsertArtifact<'_>> = inserts
        .iter()
        .zip(&launch_ids)
        .map(|((file, id, classification), launch_id)| InsertArtifact {
            id,
            project_id,
            tool_launch_id: launch_id.as_deref(),
            path: &file.path,
            kind: classification.kind.as_str(),
            tool,
            detected_at: &file.detected_at,
            state: "present",
            classification_confidence: classification.confidence,
            classification_source: classification.source.as_str(),
            size_bytes: file.size_bytes,
            file_mtime: &file.file_mtime,
            content_hash: None,
        })
        .collect();
    repo::insert_artifacts_if_absent(&mut tx, &rows)
        .await
        .map_err(|e| format!("DB insert failed: {e}"))?;
    tx.commit().await.map_err(|e| format!("DB commit failed: {e}"))?;

    // A multi-row INSERT OR IGNORE cannot report which rows landed, so one
    // re-read of the project identifies the ids we own (ours) versus the ones a
    // concurrent detector inserted first (skip events for those).
    let after = repo::list_artifacts_for_project(pool, project_id, &[])
        .await
        .map_err(|e| format!("DB lookup failed after insert: {e}"))?;

    let events = detect_events(project_id, tool, &updates, &inserts, &launch_ids, &after);
    publish_all(bus, &events).await;
    Ok(())
}

/// Build the detect phase's events. `landed` is the post-commit row set: an
/// insert absent from it lost the `(project_id, path)` race, so it emits
/// nothing.
fn detect_events(
    project_id: &str,
    tool: &str,
    updates: &[(&DetectedFile, &repo::ArtifactRow)],
    inserts: &[(&DetectedFile, String, workflow_artifacts::ClassificationResult)],
    launch_ids: &[Option<String>],
    landed: &[repo::ArtifactRow],
) -> Vec<BatchEvent> {
    let now = Timestamp::now_iso();
    let mut events: Vec<BatchEvent> = Vec::with_capacity(updates.len() + inserts.len() * 2);
    for (file, row) in updates {
        events.extend(event(
            TOPIC_ARTIFACT_UPDATED,
            &ArtifactUpdated {
                artifact_id: row.id.clone(),
                project_id: project_id.to_owned(),
                path: file.path.clone(),
                tool: tool.to_owned(),
                prior_content_hash: row.content_hash.clone(),
                new_content_hash: None,
                updated_at: now.clone(),
            },
        ));
    }
    for ((file, id, classification), launch_id) in inserts.iter().zip(launch_ids) {
        if !landed.iter().any(|r| &r.id == id) {
            continue;
        }
        events.extend(event(
            TOPIC_ARTIFACT_DETECTED,
            &ArtifactDetected {
                artifact_id: id.clone(),
                project_id: project_id.to_owned(),
                path: file.path.clone(),
                kind: classification.kind.as_str().to_owned(),
                tool: tool.to_owned(),
                classification_source: classification.source.as_str().to_owned(),
                classification_confidence: classification.confidence,
                tool_launch_id: launch_id.clone(),
                detected_at: file.detected_at.clone(),
            },
        ));
        events.extend(event(
            TOPIC_ARTIFACT_CLASSIFIED,
            &ArtifactClassified {
                artifact_id: id.clone(),
                project_id: project_id.to_owned(),
                classification: classification.kind.as_str().to_owned(),
                confidence: Some(classification.confidence),
                classified_at: file.detected_at.clone(),
            },
        ));
    }
    events
}

/// Publish a phase's events, logging (never propagating) a failure: the state
/// rows already committed are the durable record.
async fn publish_all(bus: &EventBus, events: &[BatchEvent]) {
    if let Err(e) = bus.publish_batch(events).await {
        tracing::warn!(error = %e, count = events.len(), "reconcile batch: audit publish failed");
    }
}
