// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use domain_core::ids::Timestamp;
use persistence_plans::repositories::artifacts as repo;

async fn make_pool() -> SqlitePool {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    db.pool().clone()
}

fn make_bus(pool: &SqlitePool) -> audit::bus::EventBus {
    audit::bus::EventBus::with_pool(pool.clone())
}

#[tokio::test]
async fn detect_inserts_classified_artifact() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    let id = detect(
        &pool,
        &bus,
        "proj-1",
        "output/MasterDark_bin1x1.xisf",
        "pixinsight",
        2048,
        "2026-06-01T09:55:00Z",
        "2026-06-01T10:00:00Z",
    )
    .await
    .unwrap();

    assert!(!id.is_empty());

    let artifacts = list(&pool, "proj-1", &[]).await.unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].kind, "master");
    assert_eq!(artifacts[0].classification_source, "rule");
}

#[tokio::test]
async fn detect_unknown_file_falls_back_to_intermediate() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    detect(
        &pool,
        &bus,
        "proj-1",
        "output/random_output.xisf",
        "pixinsight",
        1024,
        "2026-06-01T09:55:00Z",
        "2026-06-01T10:00:00Z",
    )
    .await
    .unwrap();

    let artifacts = list(&pool, "proj-1", &[]).await.unwrap();
    assert_eq!(artifacts[0].kind, "intermediate");
    assert_eq!(artifacts[0].classification_source, "fallback");
    assert!(artifacts[0].classification_confidence < 0.2);
}

#[tokio::test]
async fn classify_override_applies_and_clears() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    let art_id = detect(
        &pool,
        &bus,
        "proj-1",
        "output/img.xisf",
        "pixinsight",
        512,
        "2026-06-01T09:55:00Z",
        "2026-06-01T10:00:00Z",
    )
    .await
    .unwrap();

    // Apply override → final.
    let summary =
        classify_override(&pool, &bus, "proj-1", &art_id, Some("final"), None).await.unwrap();
    assert_eq!(summary.kind, "final");
    assert_eq!(summary.classification_source, "manual_override");
    #[allow(clippy::float_cmp)]
    {
        assert_eq!(summary.classification_confidence, 1.0);
    }

    // Clear override.
    let summary2 = classify_override(&pool, &bus, "proj-1", &art_id, None, None).await.unwrap();
    // After clearing, rule-based or fallback classification applies.
    assert!(
        summary2.classification_source == "rule" || summary2.classification_source == "fallback"
    );
}

/// T018: classify -> override -> rescan -> override preserved. A rescan
/// is a second `detect()` call on the same path; the A8 in-place-update
/// branch never touches `kind`/`classification_source` (see
/// `detect_inplace_update_on_rerun`), so an override must survive it —
/// this asserts that combination explicitly rather than relying on two
/// separate tests to imply it.
#[tokio::test]
async fn classify_override_survives_rescan() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    let art_id = detect(
        &pool,
        &bus,
        "proj-1",
        "output/img.xisf",
        "pixinsight",
        512,
        "2026-06-01T09:55:00Z",
        "2026-06-01T10:00:00Z",
    )
    .await
    .unwrap();

    classify_override(&pool, &bus, "proj-1", &art_id, Some("master"), None).await.unwrap();

    // Rescan: same path, updated size/mtime (e.g. the file grew).
    let rescanned_id = detect(
        &pool,
        &bus,
        "proj-1",
        "output/img.xisf",
        "pixinsight",
        1024,
        "2026-06-01T11:55:00Z",
        "2026-06-01T12:00:00Z",
    )
    .await
    .unwrap();
    assert_eq!(art_id, rescanned_id, "rescan must update the same row, not create a new one");

    let artifacts = list(&pool, "proj-1", &[]).await.unwrap();
    let row = artifacts.iter().find(|a| a.id == art_id).unwrap();
    assert_eq!(row.kind, "master", "override kind must survive a rescan");
    assert_eq!(
        row.classification_source, "manual_override",
        "override source must survive a rescan"
    );
}

#[tokio::test]
async fn detect_inplace_update_on_rerun() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    let id1 = detect(
        &pool,
        &bus,
        "proj-1",
        "output/img.xisf",
        "pixinsight",
        1024,
        "2026-06-01T09:55:00Z",
        "2026-06-01T10:00:00Z",
    )
    .await
    .unwrap();

    // Second detect on same path → A8 in-place update, same id.
    let id2 = detect(
        &pool,
        &bus,
        "proj-1",
        "output/img.xisf",
        "pixinsight",
        2048,
        "2026-06-01T11:55:00Z",
        "2026-06-01T12:00:00Z",
    )
    .await
    .unwrap();

    assert_eq!(id1, id2, "A8: same path must return same artifact id");

    // Only one row.
    let artifacts = list(&pool, "proj-1", &[]).await.unwrap();
    assert_eq!(artifacts.len(), 1);
}

#[tokio::test]
async fn mark_missing_and_resolved() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    let art_id = detect(
        &pool,
        &bus,
        "proj-1",
        "output/img.xisf",
        "pixinsight",
        512,
        "2026-06-01T09:55:00Z",
        "2026-06-01T10:00:00Z",
    )
    .await
    .unwrap();

    mark_missing_batch(
        &pool,
        &bus,
        "proj-1",
        &[GoneArtifact { id: art_id.clone(), path: "output/img.xisf".to_owned() }],
    )
    .await
    .unwrap();
    let arts = list(&pool, "proj-1", &["missing"]).await.unwrap();
    assert_eq!(arts.len(), 1);
    assert_eq!(arts[0].state, "missing");

    mark_resolved(&pool, &bus, "proj-1", &art_id).await.unwrap();
    // user_resolved_missing rows are excluded from default listing.
    let present = list(&pool, "proj-1", &["present", "missing"]).await.unwrap();
    assert!(present.is_empty());
}

#[tokio::test]
async fn complete_run_emits_workflow_run_completed() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    // Insert a tool launch row.
    sqlx::query(
        "INSERT INTO tool_launches (id, project_id, tool_id, launched_at, outcome, audit_id)
         VALUES ('tl-1','proj-1','pixinsight','2026-06-01T08:00:00Z','spawned','aud-1')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let updated = complete_run(&pool, &bus, "proj-1", "pixinsight", "tl-1").await.unwrap();
    assert!(updated);

    // Idempotent second call.
    let updated2 = complete_run(&pool, &bus, "proj-1", "pixinsight", "tl-1").await.unwrap();
    assert!(!updated2);
}

// ── sweep_stale_launches (#727) ─────────────────────────────────────────────

#[tokio::test]
async fn sweep_completes_launch_with_no_recent_activity() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    // Launched long enough ago (> DEFAULT_ATTRIBUTION_WINDOW = 6h) with no
    // artifacts ever attributed to it — the sweep must complete it.
    sqlx::query(
        "INSERT INTO tool_launches (id, project_id, tool_id, launched_at, outcome, audit_id)
         VALUES ('tl-stale','proj-1','pixinsight','2020-01-01T00:00:00Z','spawned','aud-1')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let completed = sweep_stale_launches(&pool, &bus, "proj-1").await.unwrap();
    assert_eq!(completed, 1);

    let launches = tl_repo::list_launches_for_project(&pool, "proj-1").await.unwrap();
    assert!(launches[0].completed_at.is_some());
}

#[tokio::test]
async fn sweep_leaves_recently_launched_run_open() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    let recent = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO tool_launches (id, project_id, tool_id, launched_at, outcome, audit_id)
         VALUES ('tl-fresh','proj-1',?,?, 'spawned','aud-1')",
    )
    .bind("pixinsight")
    .bind(&recent)
    .execute(&pool)
    .await
    .unwrap();

    let completed = sweep_stale_launches(&pool, &bus, "proj-1").await.unwrap();
    assert_eq!(completed, 0);

    let launches = tl_repo::list_launches_for_project(&pool, "proj-1").await.unwrap();
    assert!(launches[0].completed_at.is_none());
}

#[tokio::test]
async fn sweep_uses_last_artifact_activity_not_launch_time() {
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    // Launch started long ago, but an artifact under it was seen recently
    // (e.g. re-touched by an on-attach reconciliation pass) — the run is
    // still active and must NOT be completed.
    sqlx::query(
        "INSERT INTO tool_launches (id, project_id, tool_id, launched_at, outcome, audit_id)
         VALUES ('tl-active','proj-1','pixinsight','2020-01-01T00:00:00Z','spawned','aud-1')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let art_id = detect(
        &pool,
        &bus,
        "proj-1",
        "output/img.xisf",
        "pixinsight",
        512,
        "2020-01-01T00:05:00Z",
        "2020-01-01T00:05:00Z",
    )
    .await
    .unwrap();
    repo::set_tool_launch_id(&pool, &art_id, "tl-active").await.unwrap();
    repo::touch_artifacts(&pool, std::slice::from_ref(&art_id)).await.unwrap(); // bumps last_seen_at to now

    let completed = sweep_stale_launches(&pool, &bus, "proj-1").await.unwrap();
    assert_eq!(completed, 0);
}

// ── T028: artifact.detected AND artifact.classified both emitted (FR-009) ──

#[tokio::test]
async fn detect_emits_artifact_detected_and_artifact_classified() {
    use audit::event_bus::{TOPIC_ARTIFACT_CLASSIFIED, TOPIC_ARTIFACT_DETECTED};

    let pool = make_pool().await;
    let bus = make_bus(&pool);

    // Subscribe BEFORE detect so we capture the events.
    let mut rx = bus.subscribe();

    detect(
        &pool,
        &bus,
        "proj-t028",
        "output/MasterFlat_bin1x1.xisf",
        "pixinsight",
        1024,
        "2026-06-01T09:55:00Z",
        "2026-06-01T10:00:00Z",
    )
    .await
    .unwrap();

    // Collect events published synchronously by the detect call.
    // EventBus.publish is async; read with a short timeout.
    let mut detected = false;
    let mut classified = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    while !(detected && classified) {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(env)) if env.topic == TOPIC_ARTIFACT_DETECTED => {
                detected = true;
                // Payload must have artifact_id.
                assert!(env.payload.get("artifactId").is_some());
                assert_eq!(env.payload["projectId"].as_str(), Some("proj-t028"));
            }
            Ok(Ok(env)) if env.topic == TOPIC_ARTIFACT_CLASSIFIED => {
                classified = true;
                // Payload must have classification and confidence.
                assert!(env.payload.get("classification").is_some());
                assert!(env.payload.get("confidence").is_some());
                assert_eq!(env.payload["projectId"].as_str(), Some("proj-t028"));
            }
            Ok(Ok(_)) => {} // other topics, keep draining
            Ok(Err(_)) | Err(_) => break,
        }
    }

    assert!(detected, "artifact.detected must be emitted by detect()");
    assert!(classified, "artifact.classified must be emitted by detect() (T028 FR-009)");
}

#[tokio::test]
async fn artifact_classify_response_is_flat_shape() {
    // Verifies the contract: ArtifactClassifyResponse has flat fields,
    // not a nested { artifact: … } envelope (spec 033 T028 regression guard).
    use contracts_core::tools::ArtifactClassifyResponse;
    let pool = make_pool().await;
    let bus = make_bus(&pool);

    let art_id = detect(
        &pool,
        &bus,
        "proj-flat",
        "output/img.xisf",
        "pixinsight",
        512,
        "2026-06-01T09:55:00Z",
        "2026-06-01T10:00:00Z",
    )
    .await
    .unwrap();

    // Simulate what the Tauri command does: call classify_override then build
    // ArtifactClassifyResponse with the flat shape.
    let summary =
        classify_override(&pool, &bus, "proj-flat", &art_id, Some("final"), None).await.unwrap();

    let resp = ArtifactClassifyResponse {
        artifact_id: summary.id.clone(),
        classification: summary.kind.clone(),
        confidence: Some(summary.classification_confidence),
        classified_at: "2026-06-01T10:01:00Z".to_owned(),
    };

    // Serialise and check the JSON does NOT have a nested "artifact" key.
    let json = serde_json::to_value(&resp).unwrap();
    assert!(
        json.get("artifact").is_none(),
        "flat shape must not have nested 'artifact' key; got: {json}"
    );
    assert_eq!(json["artifactId"].as_str(), Some(summary.id.as_str()));
    assert_eq!(json["classification"].as_str(), Some("final"));
    assert!(json.get("confidence").is_some());
}
