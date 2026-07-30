// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journeys: the shared `PlanReviewOverlay`
//! (issue #1220 — "the single shared reviewable-mutation surface … has zero
//! real-backend coverage").
//!
//! `apps/desktop/src/features/plans/PlanReviewOverlay.tsx` is the ONE
//! component the cleanup, archive, restore, and source-view-removal flows
//! all render for review + apply (constitution principle II: reviewable
//! filesystem mutation, never silent). Every existing real-UI journey that
//! touches a plan (`journeys.rs::cleanup_plan_review`,
//! `archive_journeys.rs::setup_archived_project`) drives `plans.approve` /
//! `plans.apply.direct` directly over the invoke bridge and never opens this
//! overlay's DOM — so its own gates (`plan-review-approve-apply`,
//! `plan-review-confirm-destructive`, `plan-review-empty-reason`) had zero
//! real-backend hits. These journeys click through the real component
//! instead: the Project detail Cleanup section (`OutputsCleanupSections.tsx`)
//! for the destructive-confirm gate, and the plan-gated Archive lifecycle
//! transition (`ProjectDetail.tsx`/`useProjectDetailActions.ts`) for the
//! 0-item empty-reason refusal.
//!
//! Run (CI): `cargo nextest run -p e2e_tests --profile e2e --run-ignored all`
//! (serial, `.config/nextest.toml`). See `crates/e2e-tests/README.md`.

mod common;

use std::time::Duration;

use common::{settle_first_run_redirect, E2eApp};
use serde_json::json;

const INVOKE_TIMEOUT: Duration = Duration::from_secs(30);
const UI_TIMEOUT: Duration = Duration::from_secs(20);

/// Complete first-run (registers a `light_frames` AND a `project` root, the
/// real `firstrun.complete` precondition, then routes through the real gate
/// — `E2eApp::complete_first_run_gate` also clears the Shell's client-side
/// `setupCompleted` flag, which a bare `firstrun_complete` invoke does not).
/// Mirrors the proven `targets_journeys.rs`/`inventory_journeys.rs` pattern.
async fn complete_first_run(app: &E2eApp) -> anyhow::Result<()> {
    settle_first_run_redirect(app).await?;
    let raw_dir = tempfile::tempdir()?;
    let _: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({ "path": raw_dir.path().to_string_lossy(), "category": "light_frames", "scanSettings": null }),
        )
        .await?;
    let project_dir = tempfile::tempdir()?;
    let _: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({ "path": project_dir.path().to_string_lossy(), "category": "project", "scanSettings": null }),
        )
        .await?;
    app.complete_first_run_gate().await
}

/// Real project, unprotected (a real user opts a project out of the
/// safe-by-default `"protected"` level via Settings before a first cleanup —
/// `source.protection.set` mirrors that, same convention as
/// `journeys.rs::cleanup_plan_review`). Returns `(project_id, project_dir)`;
/// the caller must hold `project_dir` alive for as long as it needs the real
/// fixture file on disk.
async fn create_unprotected_project(
    app: &E2eApp,
    label: &str,
) -> anyhow::Result<(String, tempfile::TempDir)> {
    let project_dir = tempfile::tempdir()?;
    let create: serde_json::Value = app
        .invoke(
            "projects_create",
            json!({
                "req": {
                    "requestId": format!("e2e-plan-review-create-{label}"),
                    "name": format!("E2E Plan Review Project {label}"),
                    "tool": "PixInsight",
                    "path": project_dir.path().to_string_lossy(),
                    "initialSources": [],
                    "notes": null,
                    "canonicalTargetId": null,
                }
            }),
        )
        .await?;
    let project_id = create["projectId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("projects.create returned no projectId: {create}"))?
        .to_owned();

    let _: serde_json::Value = app
        .invoke(
            "source_protection_set",
            json!({
                "request": {
                    "sourceId": project_id,
                    "level": "unprotected",
                    "blockPermanentDelete": null,
                    "categories": null,
                }
            }),
        )
        .await?;

    Ok((project_id, project_dir))
}

/// Destructive-confirm gate + real approve/apply, driven entirely through
/// `PlanReviewOverlay`'s own DOM (issue #1220): a real intermediate output,
/// a cleanup policy that maps `intermediate -> delete` (so the generated
/// plan item's action is genuinely `"delete"`, gating
/// `plan-review-confirm-destructive` — `CleanupSection`'s destination radio
/// only labels the plan's destructive destination, the item ACTION comes
/// from cleanup policy, see `cleanup_generator/generate.rs::generate`), then
/// real UI clicks: Scan -> Generate plan -> the overlay opens ->
/// `plan-review-approve-apply` is disabled while the destructive checkbox is
/// unticked -> tick `plan-review-confirm-destructive` -> the button enables
/// -> click it -> the fixture file is genuinely deleted from disk (executor
/// `ExecutorItemAction::Delete`, `crates/fs/executor/src/run/dispatch.rs`).
///
/// Backend REAL: `projects.create`, `source.protection.set`,
/// `artifact.watcher.attach`, `artifact.list`, `cleanup.policy.update`.
/// Frontend REAL (never invoked directly): `cleanup.scan`,
/// `cleanup.plan.generate`, `plans.approve`, `plans.confirm.destructive`,
/// `plans.apply.direct` — all driven by clicking the real
/// `OutputsCleanupSections.tsx` + `PlanReviewOverlay.tsx` DOM.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn plan_review_destructive_confirm_gate_and_apply() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;

    let (project_id, project_dir) = create_unprotected_project(&app, "destructive").await?;

    // Real output, named per PixInsight's real "integration_*" convention —
    // classifies as `ArtifactKind::Intermediate`
    // (`crates/workflow/artifacts/src/default_rules.rs`).
    let original_path = project_dir.path().join("integration_M31_Ha_destructive.xisf");
    std::fs::write(&original_path, b"not-a-real-xisf-file")?;

    let _: serde_json::Value = app
        .invoke("artifact_watcher_attach", json!({ "request": { "projectId": project_id } }))
        .await?;
    let artifacts: serde_json::Value = app
        .invoke_until(
            "artifact_list",
            json!({ "request": { "projectId": project_id, "includeStates": [] } }),
            INVOKE_TIMEOUT,
            |v: &serde_json::Value| v["artifacts"].as_array().is_some_and(|a| !a.is_empty()),
        )
        .await?;
    anyhow::ensure!(
        artifacts["artifacts"][0]["kind"] == "intermediate",
        "expected the fixture output to classify as intermediate: {artifacts}"
    );

    // Policy maps intermediate -> delete, the ONLY way a cleanup-generated
    // item's action becomes `"delete"` (destructive) rather than `"archive"`.
    let _: serde_json::Value = app
        .invoke(
            "cleanup_policy_update",
            json!({
                "request": {
                    "entries": [
                        { "dataType": "intermediate", "action": "delete" },
                        { "dataType": "master", "action": "keep" },
                        { "dataType": "final", "action": "keep" },
                    ],
                    "autoOnCompletion": false,
                }
            }),
        )
        .await?;

    // ── Real UI from here: Projects list -> project row -> Cleanup section ──
    app.goto_route("/projects").await?;
    app.wait_testid(&format!("project-row-{project_id}"), UI_TIMEOUT).await?.click().await?;

    app.wait_testid("cleanup-scan-btn", UI_TIMEOUT).await?.click().await?;
    app.wait_testid_prefix_present("cleanup-group-", UI_TIMEOUT).await?;

    app.wait_testid("cleanup-generate-btn", UI_TIMEOUT).await?.click().await?;
    app.wait_testid("plan-review-overlay", UI_TIMEOUT).await?;

    // The destructive checkbox only renders because the plan carries a real
    // `delete`-action item — its mere presence is already backend proof.
    app.wait_testid("plan-review-confirm-destructive", UI_TIMEOUT).await?;
    // Refusal proof: apply is blocked until the destructive action is
    // explicitly confirmed (constitution II — never apply a destructive
    // action silently).
    anyhow::ensure!(
        !app.is_enabled_testid("plan-review-approve-apply").await?,
        "expected Approve & apply to be disabled before the destructive checkbox is ticked"
    );

    app.click_testid("plan-review-confirm-destructive").await?;
    app.wait_testid_enabled("plan-review-approve-apply", UI_TIMEOUT).await?;
    app.click_testid("plan-review-approve-apply").await?;

    // Real filesystem side effect (permanent delete, not archive) — ground
    // truth for "applied", independent of which footer button/badge the
    // overlay happens to render on success vs. partial failure.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if !original_path.exists() {
            break;
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "expected the confirmed destructive item to be permanently deleted from disk: {original_path:?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    app.shutdown().await
}

/// 0-item archive plan refusal (issue #1220's `plan-review-empty-reason`):
/// a project with no recorded artifacts progressed to `completed`, then the
/// real, plan-gated `completed -> archived` lifecycle transition button is
/// clicked. The transition refuses (`plan.required`), which drives
/// `useProjectDetailActions.ts::handleGenerateArchivePlan` to generate the
/// real (0-item) archive plan and open `PlanReviewOverlay` automatically —
/// the ENTIRE flow is one real UI click, no direct plan-generate invoke.
///
/// The generator's own diagnostic (`archive_generator.rs`'s `empty_reason`,
/// a plain Rust string literal — never routed through the Paraglide i18n
/// catalog, so asserting its content is locale-stable) explains the refusal;
/// `plan-review-approve-apply` stays disabled because `itemsTotal == 0`, so
/// nothing can be silently applied.
///
/// Backend REAL: `projects.create`, `lifecycle.transition.apply` (x3, real
/// progression to `completed`). Frontend REAL (never invoked directly):
/// `lifecycle.transition.apply` (the refused `archived` hop),
/// `archive.plan.generate` — driven by clicking the real
/// `transition-btn-archived` DOM button.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn plan_review_empty_archive_plan_refuses_apply() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;

    let (project_id, _project_dir) = create_unprotected_project(&app, "empty-archive").await?;

    // Real lifecycle progression to `completed` — no artifacts are ever
    // recorded for this project, so the archive plan the UI click below
    // generates has zero items.
    let hops: &[(&str, &str)] =
        &[("setup_incomplete", "ready"), ("ready", "processing"), ("processing", "completed")];
    for (current_state, next_state) in hops.iter() {
        let transition: serde_json::Value = app
            .invoke(
                "lifecycle_transition_apply",
                json!({
                    "request": {
                        "entityType": "project",
                        "contractVersion": "2.0.0",
                        "requestId": uuid::Uuid::new_v4().to_string(),
                        "entityId": project_id,
                        "currentState": current_state,
                        "nextState": next_state,
                        "actionLabel": null,
                        "actor": "user",
                    }
                }),
            )
            .await?;
        anyhow::ensure!(
            transition["status"] == "success",
            "expected {current_state} -> {next_state} to succeed for a fresh project: {transition}"
        );
    }

    app.goto_route("/projects").await?;
    app.wait_testid(&format!("project-row-{project_id}"), UI_TIMEOUT).await?.click().await?;

    // The plan-gated `archived` transition refuses (no approved plan yet)
    // and the refusal handler auto-generates + opens the review overlay.
    app.wait_testid("transition-btn-archived", UI_TIMEOUT).await?.click().await?;
    app.wait_testid("plan-review-overlay", UI_TIMEOUT).await?;

    app.wait_testid("plan-review-empty-reason", UI_TIMEOUT).await.map_err(|e| {
        anyhow::anyhow!(
            "expected a real `empty_reason` banner from `archive.plan.generate` on a 0-item \
             plan: {e}"
        )
    })?;
    let reason_text = app.text_testid("plan-review-empty-reason").await?;
    anyhow::ensure!(
        !reason_text.trim().is_empty(),
        "expected the empty-reason banner to carry the generator's real diagnostic sentence"
    );

    // Refusal proof: nothing to apply, so Approve & apply stays disabled —
    // the overlay never lets a 0-item plan be silently "applied".
    app.wait_testid("plan-review-approve-apply", UI_TIMEOUT).await?;
    anyhow::ensure!(
        !app.is_enabled_testid("plan-review-approve-apply").await?,
        "expected Approve & apply to stay disabled on a 0-item plan"
    );

    app.shutdown().await
}
