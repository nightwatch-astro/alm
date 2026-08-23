// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journey: whole-project archive -> trash ->
//! permanent delete (coverage-matrix Journey 7, "Archive lifecycle + trash +
//! permanent delete" — previously **zero automated coverage at any layer**).
//!
//! Was blocked on a channel-free apply command for archive/cleanup plans
//! (shared blocker with Journey 6, `journeys.rs::cleanup_plan_review`);
//! `plans.apply.direct` (a.k.a. `plans_apply_direct`, spec 037) removes that
//! blocker — same executor, same durable audit trail as `plans.apply_real`,
//! no `tauri::ipc::Channel` required, so it can be invoked directly from this
//! WebDriver harness.
//!
//! Run (CI): `cargo nextest run -p e2e_tests --profile e2e --run-ignored all`
//! (serial, `.config/nextest.toml`). See `crates/e2e-tests/README.md`.

mod common;

use std::time::Duration;

use common::E2eApp;
use serde_json::json;

const INVOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Real lifecycle progression to `completed` -> real archive plan ->
/// `plans.apply.direct` (real filesystem move into `.astro-plan-archive`) ->
/// `archive.list` durable read. One real archived project + plan per call —
/// the caller then exercises exactly ONE destructive archive-management
/// command (`archive.send_to_trash` XOR `archive.permanently_delete`)
/// against it, because both commands now really remove the plan's entire
/// archive subtree (#732): calling one after the other against the SAME plan
/// would legitimately find nothing left for the second call, which is not
/// what either sub-journey is testing. `label` disambiguates request ids and
/// the project name across calls in the same test.
///
/// Returns `(project_id, plan_id, project_dir)` — the caller must hold
/// `project_dir` (a `TempDir` guard) alive for as long as it still needs the
/// real archived file on disk.
async fn setup_archived_project(
    app: &E2eApp,
    label: &str,
) -> anyhow::Result<(String, String, tempfile::TempDir)> {
    // 1. Create a project with no sources — it starts `setup_incomplete`
    // (real, documented lifecycle rule; `projects.create` does not
    // auto-advance an empty-source project).
    let project_dir = tempfile::tempdir()?;
    let create: serde_json::Value = app
        .invoke(
            "projects_create",
            json!({
                "req": {
                    "requestId": format!("e2e-archive-create-{label}"),
                    "name": format!("E2E Archive Project {label}"),
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
    anyhow::ensure!(
        create["lifecycle"] == "setup_incomplete",
        "expected a sourceless project to start setup_incomplete: {create}"
    );

    // 2. Real lifecycle progression to `completed` (archive.plan.generate
    // only enumerates a project's own artifacts; nothing about that
    // requires `completed`, but a project is realistically archived only
    // once its processing is done — and `completed -> archived` is itself
    // plan-required, so this journey proves the plan-driven closure path,
    // never a direct transition into `archived`).
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
                        // `requestId` deserializes as a real `Uuid`
                        // (contracts_core::lifecycle transition_request_base!),
                        // unlike most other request ids — a hand-crafted
                        // placeholder string fails IPC deserialization.
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

    // 3. Real per-project protection override (US2): a project id has no
    // protection override until a caller sets one, and the app's
    // safe-by-default level is "protected" (constitution II) — without
    // this, every archive item refuses apply with `protected.source`. A
    // real user sets this the same way before a first archive.
    // 2-level model (issue #506): "normal" is retired — "unprotected" is the
    // non-gating override this test needs.
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

    // 4. A real processing output, classified `intermediate` by the real
    // artifact-kind rules (`crates/workflow/artifacts/src/default_rules.rs`).
    let original_path = project_dir.path().join(format!("integration_M31_Ha_{label}.xisf"));
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

    // 5. Generate the whole-project archive plan (spec 017 US2/WP-B) — every
    // observed artifact becomes an `archive`-action item. No filesystem
    // mutation happens here (FR-002).
    let generate: serde_json::Value = app
        .invoke("archive_plan_generate", json!({ "projectId": project_id, "title": null }))
        .await?;
    let plan_id = generate["planId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("archive.plan.generate returned no planId: {generate}"))?
        .to_owned();
    anyhow::ensure!(
        generate["itemCount"].as_i64().unwrap_or(0) >= 1,
        "expected at least 1 real archive item on the generated plan: {generate}"
    );

    // 6. Approve — the explicit gate. `plans.apply.direct` requires the token
    // this call mints; no apply path may mint its own (constitution §II).
    let approve: serde_json::Value = app.invoke("plans_approve", json!({ "id": plan_id })).await?;
    let approval_token = approve["approvalToken"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("plans.approve returned no approvalToken: {approve}"))?
        .to_owned();

    // 7. Apply — the real filesystem mutation, channel-free (spec 037). Runs
    // the same executor `plans.apply_real` uses, just without a progress
    // `Channel`.
    let apply: serde_json::Value = app
        .invoke("plans_apply_direct", json!({ "planId": plan_id, "approvalToken": approval_token }))
        .await?;
    anyhow::ensure!(
        apply["planId"] == json!(plan_id) && apply["newState"] == "applying",
        "expected plans.apply.direct to start applying the generated archive plan: {apply}"
    );

    // 8. Poll the real, durable apply status until the executor finishes.
    let status: serde_json::Value = app
        .invoke_until(
            "plans_apply_status",
            json!({ "planId": plan_id }),
            INVOKE_TIMEOUT,
            |v: &serde_json::Value| {
                matches!(
                    v["planState"].as_str(),
                    Some("applied" | "partially_applied" | "failed" | "cancelled")
                )
            },
        )
        .await?;
    anyhow::ensure!(
        status["planState"] == "applied",
        "expected the archive plan to apply cleanly, got: {status}"
    );
    anyhow::ensure!(
        status["itemsApplied"].as_i64().unwrap_or(0) >= 1,
        "expected at least 1 durably-recorded applied item: {status}"
    );

    // 9. Real filesystem side effect: the source file moved out of the
    // project folder and into the app-managed `.astro-plan-archive`
    // subtree (never a silent overwrite — constitution II).
    anyhow::ensure!(
        !original_path.exists(),
        "expected the archived file to have moved away from {original_path:?}"
    );
    let archive_subtree_exists = std::fs::read_dir(project_dir.path())
        .ok()
        .and_then(|mut entries| {
            entries.find(|e| e.as_ref().is_ok_and(|e| e.file_name() == ".astro-plan-archive"))
        })
        .is_some();
    anyhow::ensure!(
        archive_subtree_exists,
        "expected a `.astro-plan-archive` subtree under the project folder after apply"
    );

    // 10. C5 lifecycle closure: applying an `origin = archive` plan to a
    // clean `applied` terminal drives the owning project into `archived` —
    // the ONLY legitimate way to reach that state (`completed -> archived`
    // is plan-required, step 2 above never attempted it directly). Durable
    // read via `archive.list` (C5: projects-only surface).
    let archive_list: serde_json::Value = app
        .invoke_until("archive_list", json!({}), INVOKE_TIMEOUT, |v: &serde_json::Value| {
            v["entries"].as_array().is_some_and(|a| a.iter().any(|e| e["id"] == json!(project_id)))
        })
        .await?;
    let entry = archive_list["entries"]
        .as_array()
        .and_then(|a| a.iter().find(|e| e["id"] == json!(project_id)))
        .ok_or_else(|| {
            anyhow::anyhow!("expected project {project_id} in archive.list: {archive_list}")
        })?;
    anyhow::ensure!(
        entry["archivedViaPlanId"] == json!(plan_id),
        "expected the archive.list entry to carry the owning plan id: {entry}"
    );

    Ok((project_id, plan_id, project_dir))
}

/// Whole-project archive: real lifecycle progression to `completed` ->
/// `archive.plan.generate` -> `plans.apply.direct` -> real filesystem move
/// into the app-managed archive subtree -> `archive.list` durable read ->
/// `archive.send_to_trash` -> `archive.permanently_delete` honoring the
/// `blockPermanentDelete` protection default.
///
/// Backend REAL: `projects.create`, `lifecycle.transition.apply` (x3),
/// `source.protection.set`, `artifact.watcher.attach`, `artifact.list`,
/// `archive.plan.generate`, `plans.apply.direct`, `plans.apply.status`,
/// `archive.list`, `archive.send_to_trash`, `settings.update`,
/// `archive.permanently_delete`.
///
/// `archive.send_to_trash` and `archive.permanently_delete` now execute real
/// filesystem operations (#732 — previously metadata-only stubs). Both
/// commands act on a plan's ENTIRE archive subtree, so this journey uses two
/// independent real archived projects/plans (`setup_archived_project`): one
/// exercises real OS-trash removal, the other exercises the real
/// `blockPermanentDelete` gate + real permanent delete. Running both
/// commands sequentially against the SAME plan would have nothing left for
/// the second call once the first really removes the files.
///
/// Prefixed `slow_`: two separate `setup_archived_project` fixture sequences
/// (each involving a real app launch) plus real filesystem operations on
/// disk. Measured 14-36s across 2 post-fix runs (avg 25.5s), above the
/// >3× median (~21s) threshold. The `e2e.yml` shard filters pin each
/// `slow_` test to a separate shard; see the timing table in that file.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn slow_archive_lifecycle_apply_trash_permanent_delete() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;

    // ── Sub-journey A: archive.send_to_trash really removes the file ──────
    let (_project_a, plan_a, dir_a) = setup_archived_project(&app, "trash").await?;

    let archive_root_a = dir_a.path().join(".astro-plan-archive");
    let send_to_trash: serde_json::Value =
        app.invoke("archive_send_to_trash", json!({ "planId": plan_a })).await?;
    anyhow::ensure!(
        send_to_trash["planId"] == json!(plan_a)
            && send_to_trash["itemsMoved"].as_i64().unwrap_or(0) >= 1,
        "expected archive.send_to_trash to report the real archived item count: {send_to_trash}"
    );
    // Real filesystem side effect (#732): the archived file is genuinely
    // gone from the app-managed archive subtree, not just audit-logged.
    anyhow::ensure!(
        !any_file_under(&archive_root_a),
        "expected archive.send_to_trash to leave no files under {archive_root_a:?}"
    );

    // ── Sub-journey B: archive.permanently_delete honors blockPermanentDelete,
    // then really removes the file once unblocked ─────────────────────────
    let (_project_b, plan_b, dir_b) = setup_archived_project(&app, "delete").await?;
    let archive_root_b = dir_b.path().join(".astro-plan-archive");

    // The app's default is `true` (blocked) — assert the call is refused
    // without depending on how the WebDriver bridge's `.catch` stringifies
    // the rejected `ContractError` object.
    let blocked = app
        .invoke::<serde_json::Value>(
            "archive_permanently_delete",
            json!({ "planId": plan_b, "confirmText": "DELETE" }),
        )
        .await;
    anyhow::ensure!(
        blocked.is_err(),
        "expected archive.permanently_delete to be refused while blockPermanentDelete=true (default)"
    );
    // Refused before touching the filesystem (constitution II).
    anyhow::ensure!(
        any_file_under(&archive_root_b),
        "expected the blocked delete to leave the archived file untouched: {archive_root_b:?}"
    );

    // Explicitly disable the protection default (a real, user-facing
    // Settings > Cleanup toggle) and retry — the real unblocked path.
    let _: serde_json::Value = app
        .invoke(
            "settings_update",
            json!({ "scope": "cleanup", "values": { "blockPermanentDelete": false } }),
        )
        .await?;
    let permanently_delete: serde_json::Value = app
        .invoke("archive_permanently_delete", json!({ "planId": plan_b, "confirmText": "DELETE" }))
        .await?;
    anyhow::ensure!(
        permanently_delete["planId"] == json!(plan_b)
            && permanently_delete["itemsDeleted"].as_i64().unwrap_or(0) >= 1,
        "expected archive.permanently_delete to succeed once unblocked: {permanently_delete}"
    );
    // Real filesystem side effect (#732): the archived file is genuinely
    // removed, not just audit-logged.
    anyhow::ensure!(
        !any_file_under(&archive_root_b),
        "expected archive.permanently_delete to leave no files under {archive_root_b:?}"
    );

    app.shutdown().await
}

/// True if `root` exists and contains at least one file (recursively).
/// Used to assert real archive-management side effects without hardcoding
/// the item-id-derived archive filename.
fn any_file_under(root: &std::path::Path) -> bool {
    fn walk(dir: &std::path::Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else { return false };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if walk(&path) {
                    return true;
                }
            } else if path.is_file() {
                return true;
            }
        }
        false
    }
    root.is_dir() && walk(root)
}
