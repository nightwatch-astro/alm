// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI E2E journeys.
//!
//! Every journey launches the real `desktop_shell` binary (built with
//! `--features e2e`) behind `tauri-plugin-webdriver`/`tauri-webdriver`, and
//! asserts against the REAL backend (real SQLite, real filesystem, real
//! command handlers) — never a fixture/mock. Setup and assertion steps use
//! the `window.__PV_E2E__.invoke(...)` bridge (real IPC, no channel-taking
//! commands); mutating steps that require a `tauri::ipc::Channel` argument
//! (`plans.apply` a.k.a. `plans_apply_real`) are deliberately routed through
//! the channel-free command variants that exist for exactly this purpose
//! (`inbox.plan.apply` for inbox plans; `plans.apply.direct` a.k.a.
//! `plans_apply_direct`, spec 037, for archive/cleanup plans — `plans.approve`
//! remains available separately for journeys that only need the reviewable
//! `ready_for_review` -> `approved` step) rather than reaching into product
//! frontend code to fabricate a Channel from a WebDriver script — see each
//! journey's doc comment for the specific reasoning.
//!
//! Per research D9/D22: `sessions.transition` was deliberately deleted by
//! spec 041 FR-051 (T076) and is NOT exercised here. `audit.list`/
//! `audit.export` were fixture stubs when these journeys were first authored;
//! PR #388 has since wired them to the real `audit_log_entry` table (with
//! #401 adding entity-filtered reads). The journeys keep their durable-record
//! proofs on `plans.apply.status` (the plan executor's own `plan_apply_events`
//! trail) and `lifecycle_ledger_list` — those read paths sit closest to the
//! mutations being proved, which stays the more robust assertion regardless.
//!
//! Run (CI): `cargo nextest run -p e2e_tests --profile e2e
//! --run-ignored all` (serial,
//! `.config/nextest.toml`). Locally: build `desktop_shell --features e2e`,
//! `cargo install tauri-webdriver --locked`, serve the frontend on the
//! `tauri.conf.json` `devUrl` port (or a `PV_DEV_URL` of your own) with
//! `VITE_E2E=1`, then run the same command — see `README.md`.

mod common;

use std::time::Duration;

use common::{write_minimal_fits_with_exposure, E2eApp, DRAIN_BACKED_TIMEOUT};
use serde_json::json;

const INVOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// First-run wizard state → SIMBAD target resolve (offline, bundled-seed
/// cache hit) → project creation linked to the resolved target.
///
/// Backend REAL: `target.resolve`, `projects.create`, `projects.list`.
///
/// `main.rs` loads the bundled Messier/Caldwell/NGC seed into the target
/// resolution cache on every fresh boot, before the UI starts (see
/// `apps/desktop/src-tauri/src/main.rs`), so resolving "M 31" is a real,
/// deterministic, network-free cache hit — no `wiremock`/`FakeResolver`
/// needed at this layer.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn first_run_resolve_create_project() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;

    // Real UI: a fresh DB has no registered sources, so the router redirects
    // to the first-run wizard. This is the journey's real-UI proof (FR-007
    // adjacent) — the rest of the journey proves the UI -> backend round trip
    // via the invoke bridge, since the native folder picker the wizard's
    // "Add folder" buttons open cannot be driven by WebDriver (documented
    // constraint, see `e2e-agentic-test/003-first-run-source-setup/`).
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    // The index route's first-run gate redirects to /setup from an *async*
    // `beforeLoad` (dynamic import + a `firstrun_state` IPC round-trip), so the
    // redirect lands shortly after the bridge is ready — poll for it rather than
    // racing the immediate assertion (which non-deterministically caught the URL
    // still at "/").
    app.wait_url_contains("/setup", Duration::from_secs(15))
        .await
        .map_err(|e| anyhow::anyhow!("expected a fresh DB to redirect to /setup: {e}"))?;

    // Real backend round-trip (FR-008): resolve a bundled-seed target.
    let resolve: serde_json::Value = app
        .invoke(
            "target_resolve",
            json!({
                "req": {
                    "contractVersion": "1.0",
                    "requestId": "e2e-first-run-resolve",
                    "query": "M 31",
                    "override": null,
                }
            }),
        )
        .await?;
    anyhow::ensure!(
        resolve["status"] == "resolved",
        "expected M 31 to resolve from the bundled offline seed cache: {resolve}"
    );
    let target = &resolve["target"];
    let target_id = target["targetId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("resolved target has no targetId: {resolve}"))?
        .to_owned();
    let ra_deg = target["raDeg"]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("resolved target has no raDeg: {resolve}"))?;
    anyhow::ensure!(
        (5.0..15.0).contains(&ra_deg),
        "M 31 RA out of the expected bundled-seed range (~10.68): {ra_deg}"
    );

    // Real backend round-trip: create a project linked to the resolved
    // target, then confirm it via a fresh `projects.list` read.
    let project_dir = tempfile::tempdir()?;
    let create: serde_json::Value = app
        .invoke(
            "projects_create",
            json!({
                "req": {
                    "requestId": "e2e-first-run-create",
                    "name": "E2E M31 Project",
                    "tool": "PixInsight",
                    "path": project_dir.path().to_string_lossy(),
                    "initialSources": [],
                    "notes": null,
                    "canonicalTargetId": target_id,
                }
            }),
        )
        .await?;
    let project_id = create["projectId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("projects.create returned no projectId: {create}"))?
        .to_owned();

    let projects: serde_json::Value =
        app.invoke("projects_list", json!({ "filters": null })).await?;
    let found = projects
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("projects.list did not return an array: {projects}"))?
        .iter()
        .any(|p| p["id"] == project_id);
    anyhow::ensure!(found, "expected project {project_id} in projects.list: {projects}");

    app.shutdown().await
}

/// Filesystem plan review → apply → durable-record assertion.
///
/// Backend REAL: `roots.register`, `sources.set_organization_state`,
/// `inbox.scan.folder`, `inbox.classify`, `inbox.confirm`, `plans.approve`,
/// `inbox.plan.apply`, `plans.apply.status`.
///
/// `inbox.plan.apply` (not `plans.apply_real`) is the mutating step: it calls the
/// SAME `apply_plan` core function `plans.apply_real` uses, just without a
/// progress `Channel` — the exact real, channel-free equivalent this journey
/// needs (see module docs). It refuses a plan that carries no approval, so the
/// journey approves first, as the UI does.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn plan_review_apply_with_audit() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    // The `__PV_E2E__` invoke bridge is installed by an async dynamic import in
    // `apps/desktop/src/main.tsx`, so it is not present the instant the session
    // is created — wait for it before the first `invoke` (FR-008).
    app.wait_bridge_ready(Duration::from_secs(30)).await?;

    // 1. Register a disposable light-frames root with one real FITS file.
    let root_dir = tempfile::tempdir()?;
    let file_name = "light_001.fits";
    let original_path = write_minimal_fits_with_exposure(
        root_dir.path(),
        file_name,
        "Light Frame",
        Some("M 42"),
        Some("Ha"),
        Some("2026-01-10T22:00:00"),
        Some(300.0),
    )?;
    anyhow::ensure!(original_path.exists(), "fixture FITS file was not written");

    let register: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({
                "path": root_dir.path().to_string_lossy(),
                "category": "light_frames",
                "scanSettings": null,
            }),
        )
        .await?;
    let root_id = register["sourceId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("roots.register returned no sourceId: {register}"))?
        .to_owned();

    // `roots.register` defaults non-inbox sources to "organized" (already in
    // place, nothing to reorganize). This journey needs a real move, so flip
    // the source to "unorganized" — a real, documented state transition
    // (spec 041 R-7/T030), not a workaround around the backend.
    let _: serde_json::Value = app
        .invoke(
            "sources_set_organization_state",
            json!({ "sourceId": root_id, "organizationState": "unorganized" }),
        )
        .await?;

    // 2. Scan + classify + confirm — this is the real reviewable plan (FR-009
    // requires the plan to exist and be reviewable before it applies).
    // Spec 058 T012: scan records a source group and no placeholder item;
    // classification materializes the real rows. See
    // `common::scan_and_classify_one_item`.
    let inbox_item_id = common::scan_and_classify_one_item(
        &app,
        &root_id,
        root_dir.path().to_string_lossy().as_ref(),
    )
    .await?;

    let classify: serde_json::Value = app
        .invoke(
            "inbox_classify",
            json!({
                "req": {
                    "inboxItemId": inbox_item_id,
                    "forceRescan": false,
                    "rootAbsolutePath": root_dir.path().to_string_lossy(),
                }
            }),
        )
        .await?;
    let content_signature = classify["contentSignature"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("inbox.classify returned no contentSignature: {classify}"))?
        .to_owned();

    let confirm: serde_json::Value = app
        .invoke(
            "inbox_confirm",
            json!({
                "req": {
                    "inboxItemId": inbox_item_id,
                    "contentSignature": content_signature,
                    "destructiveDestination": null,
                    "rootAbsolutePath": root_dir.path().to_string_lossy(),
                    "rootId": null,
                }
            }),
        )
        .await?;
    let plan_id = confirm["planId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("inbox.confirm returned no planId: {confirm}"))?
        .to_owned();
    anyhow::ensure!(!plan_id.is_empty(), "expected a real (non-empty) plan id: {confirm}");

    // 3. Apply — the real filesystem mutation (FR-009).
    // Constitution II (astro-plan-tykek): `inbox.plan.apply` refuses a plan the
    // caller has not approved, mirroring the UI's approve-then-apply gesture.
    let _: serde_json::Value =
        app.invoke("plans_approve", json!({ "id": confirm["planId"] })).await?;
    let apply: serde_json::Value =
        app.invoke("inbox_plan_apply", json!({ "inboxItemId": inbox_item_id })).await?;
    anyhow::ensure!(
        apply["planId"] == json!(plan_id),
        "inbox.plan.apply applied a different plan than confirm created: {apply}"
    );

    // 4. Poll the real, durable apply status until the executor finishes —
    // `plans.apply.status` reads `plan_apply_events`, the plan executor's own
    // durable audit trail for filesystem mutation (FR-016) and the read path
    // closest to the mutation being proved.
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
        "expected the plan to apply cleanly, got: {status}"
    );
    anyhow::ensure!(
        status["itemsApplied"].as_i64().unwrap_or(0) >= 1,
        "expected at least 1 durably-recorded applied item: {status}"
    );

    // 5. Real filesystem side effect: the original path is gone (moved).
    anyhow::ensure!(
        !original_path.exists(),
        "expected the source file to have moved away from {original_path:?}"
    );

    app.shutdown().await
}

/// Inbox confirm → ingest session grouping (async, event-driven) →
/// calibration suggest → global search by alias.
///
/// Backend REAL: `inbox.plan.apply` (triggers the real `plan_listener` ->
/// `ingest_light_frames` session-grouping pipeline, spec 035 US4),
/// `sessions.list`, `calibration.match.suggest`, `search.global`.
///
/// The OBJECT header is "M 31" — a bundled-seed entry — so the ingest
/// pipeline's target resolution is a real, network-free cache hit
/// (`canonical_target_id` gets linked inline, not left pending).
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn ingestion_sessions_search() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    // Bridge is installed by an async dynamic import (`main.tsx`) — wait for it.
    app.wait_bridge_ready(Duration::from_secs(30)).await?;

    let root_dir = tempfile::tempdir()?;
    let original_path = write_minimal_fits_with_exposure(
        root_dir.path(),
        "light_m31_001.fits",
        "Light Frame",
        Some("M 31"),
        Some("Ha"),
        Some("2026-01-11T21:30:00"),
        Some(300.0),
    )?;
    anyhow::ensure!(original_path.exists(), "fixture FITS file was not written");

    let register: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({
                "path": root_dir.path().to_string_lossy(),
                "category": "light_frames",
                "scanSettings": null,
            }),
        )
        .await?;
    let root_id = register["sourceId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("roots.register returned no sourceId: {register}"))?
        .to_owned();
    let _: serde_json::Value = app
        .invoke(
            "sources_set_organization_state",
            json!({ "sourceId": root_id, "organizationState": "unorganized" }),
        )
        .await?;

    // Spec 058 T012: scan records a source group and no placeholder item;
    // classification materializes the real rows. See
    // `common::scan_and_classify_one_item`.
    let inbox_item_id = common::scan_and_classify_one_item(
        &app,
        &root_id,
        root_dir.path().to_string_lossy().as_ref(),
    )
    .await?;

    let classify: serde_json::Value = app
        .invoke(
            "inbox_classify",
            json!({
                "req": {
                    "inboxItemId": inbox_item_id,
                    "forceRescan": false,
                    "rootAbsolutePath": root_dir.path().to_string_lossy(),
                }
            }),
        )
        .await?;
    let content_signature = classify["contentSignature"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("inbox.classify returned no contentSignature: {classify}"))?
        .to_owned();

    let confirm: serde_json::Value = app
        .invoke(
            "inbox_confirm",
            json!({
                "req": {
                    "inboxItemId": inbox_item_id,
                    "contentSignature": content_signature,
                    "destructiveDestination": null,
                    "rootAbsolutePath": root_dir.path().to_string_lossy(),
                    "rootId": null,
                }
            }),
        )
        .await?;

    // Constitution II (astro-plan-tykek): `inbox.plan.apply` refuses a plan the
    // caller has not approved, mirroring the UI's approve-then-apply gesture.
    let _: serde_json::Value =
        app.invoke("plans_approve", json!({ "id": confirm["planId"] })).await?;
    let _: serde_json::Value =
        app.invoke("inbox_plan_apply", json!({ "inboxItemId": inbox_item_id })).await?;

    // Poll `sessions.list` for the real, grouped-and-resolved session instead
    // of a blind sleep.
    //
    // NOTE: the previous comment here claimed this is event-driven — "the
    // plan-listener reacts to the plan-applied event asynchronously". That is
    // true of session GROUPING, but NOT of the `targetIds` this predicate
    // waits on. `backfill_session_targets` has exactly one caller in the app,
    // the 30 s-interval ingest-resolution drain, so this wait is bounded by
    // that drain's cadence and needs [`DRAIN_BACKED_TIMEOUT`], not the plain
    // 30 s one. Waiting 30 s on a 30 s-period task is what flaked (#1205).
    let sessions: serde_json::Value = app
        .invoke_until("sessions_list", json!({}), DRAIN_BACKED_TIMEOUT, |v: &serde_json::Value| {
            v.as_array().is_some_and(|arr| {
                arr.iter().any(|s| s["targetIds"].as_array().is_some_and(|t| !t.is_empty()))
            })
        })
        .await?;
    let session = sessions
        .as_array()
        .and_then(|arr| {
            arr.iter().find(|s| s["targetIds"].as_array().is_some_and(|t| !t.is_empty()))
        })
        .ok_or_else(|| anyhow::anyhow!("no resolved session found: {sessions}"))?;
    let session_id = session["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("session has no id: {session}"))?
        .to_owned();

    // Real backend round-trip: calibration suggest against the real session
    // (candidates may legitimately be empty — no calibration masters were
    // seeded — the proof here is a real, non-error response, not a fixture).
    let suggest: serde_json::Value = app
        .invoke(
            "calibration_match_suggest",
            json!({
                "req": {
                    "contractVersion": "1.0",
                    "requestId": "e2e-ingest-suggest",
                    "sessionId": session_id,
                    "calibrationTypes": null,
                }
            }),
        )
        .await?;
    anyhow::ensure!(
        suggest.get("status").is_some(),
        "expected a real calibration.match.suggest response shape: {suggest}"
    );

    // Real backend round-trip (FR-008): the bundled M31 seed carries the
    // "Andromeda Galaxy" common-name alias; search must resolve it.
    let search: serde_json::Value =
        app.invoke("search_global", json!({ "query": "Andromeda" })).await?;
    let alias_hit = search
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("search.global did not return an array: {search}"))?
        .iter()
        .any(|r| r["kind"] == "target");
    anyhow::ensure!(
        alias_hit,
        "expected the Andromeda alias to resolve to a target result: {search}"
    );

    app.shutdown().await
}

/// Lifecycle integrity: a real refusal/blocked `TransitionResponse`, and a
/// real ledger read. The original scaffold's `events.recent` mention was
/// aspirational (no such command exists); `lifecycle.ledger.list` is the
/// real durable read path for lifecycle state (and `audit.list` — a stub
/// when this was authored, wired to `audit_log_entry` by PR #388 since —
/// remains available as a future complementary assertion surface).
///
/// Backend REAL: `projects.create`, `lifecycle.transition.apply`,
/// `lifecycle.ledger.list`.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn lifecycle_integrity() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    // Bridge is installed by an async dynamic import (`main.tsx`) — wait for it.
    app.wait_bridge_ready(Duration::from_secs(30)).await?;

    // 1. Create a project with no sources — it starts life "setup_incomplete"
    // (real, per `projects.create`'s documented lifecycle rule) and does NOT
    // auto-advance since `initialSources` is empty.
    let project_dir = tempfile::tempdir()?;
    let create: serde_json::Value = app
        .invoke(
            "projects_create",
            json!({
                "req": {
                    "requestId": "e2e-lifecycle-create",
                    "name": "E2E Lifecycle Project",
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

    // 2. Real DTO: attempt an explicit setup_incomplete -> ready transition.
    // Whether the real business rule allows or refuses this (it may require
    // sources first) is NOT asserted here — the point is that the response
    // is a real, well-formed `TransitionResponse`, not a fixture.
    let transition: serde_json::Value = app
        .invoke(
            "lifecycle_transition_apply",
            json!({
                "request": {
                    "entityType": "project",
                    "contractVersion": "2.0.0",
                    // `TransitionRequest.request_id` is a real `Uuid` on the
                    // wire (crates/contracts/core/src/lifecycle.rs), unlike the
                    // free-form String requestIds elsewhere — a slug here fails
                    // arg deserialisation before the command runs.
                    "requestId": "e2e00000-0000-4000-8000-000000000001",
                    "entityId": project_id,
                    "currentState": "setup_incomplete",
                    "nextState": "ready",
                    "actionLabel": null,
                    "actor": "user",
                }
            }),
        )
        .await?;
    let transition_status = transition["status"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("transition response has no status: {transition}"))?;
    anyhow::ensure!(
        matches!(transition_status, "success" | "noop" | "error"),
        "unexpected transition status: {transition}"
    );
    if transition_status == "error" {
        let error = &transition["error"];
        anyhow::ensure!(
            error["code"].is_string() && !error["message"].as_str().unwrap_or_default().is_empty(),
            "refused transition must carry a real error code + message, not a placeholder: {transition}"
        );
    }

    // 3. Real read: the ledger carries a row for this project (durable
    // record proof via the lifecycle-owned read path).
    let ledger: serde_json::Value = app
        .invoke(
            "lifecycle_ledger_list",
            json!({
                "filter": {
                    "entityTypes": ["project"],
                    "projectId": project_id,
                }
            }),
        )
        .await?;
    let has_row = ledger
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("lifecycle.ledger.list did not return an array: {ledger}"))?
        .iter()
        .any(|row| row["entityId"] == project_id);
    anyhow::ensure!(has_row, "expected a ledger row for project {project_id}: {ledger}");

    app.shutdown().await
}

/// Cleanup plan review: real artifact observation -> scan -> generate ->
/// approve -> apply (spec 017/037 D22/Journey 6 — newly in scope now that the
/// WP-A generator (#389) and the channel-free `plans.apply.direct` command
/// exist).
///
/// Backend REAL: `projects.create`, `source.protection.set`,
/// `artifact.watcher.attach` (spec 012, #400), `artifact.list`,
/// `cleanup.policy.update`, `cleanup.scan`, `cleanup.plan.generate`,
/// `plans.approve`, `plans.apply.direct`, `plans.apply.status`.
///
/// FORMERLY a documented gap: applying the generated plan needed
/// `plans.apply_real`, which takes a `tauri::ipc::Channel` progress argument
/// this harness declines to construct — see the module docs: it is buildable
/// from a WebDriver script, but only by reaching into Tauri internals.
/// `plans.apply.direct` (spec 037) is the channel-free equivalent — same
/// executor, same durable audit trail, no `Channel` required — so this
/// journey now drives the real filesystem mutation instead of stopping at
/// `approved`.
///
/// `source.protection.set` marks this project `normal` before generating the
/// plan: the app's safe-by-default protection level is `"protected"`
/// (constitution II), and a project id has no protection override until a
/// caller sets one — a real user would do the same via Settings before a
/// first cleanup, exactly like this call.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn cleanup_plan_review() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    // Bridge is installed by an async dynamic import (`main.tsx`) — wait for it.
    app.wait_bridge_ready(Duration::from_secs(30)).await?;

    // The artifact watcher's attach-time reconciliation pass requires the
    // project's output folder to already exist on disk.
    let project_dir = tempfile::tempdir()?;

    let create: serde_json::Value = app
        .invoke(
            "projects_create",
            json!({
                "req": {
                    "requestId": "e2e-cleanup-create",
                    "name": "E2E Cleanup Project",
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

    // Real output file, named per PixInsight's real "integration_*" naming
    // convention (`crates/workflow/artifacts/src/default_rules.rs`), which
    // classifies as `ArtifactKind::Intermediate` — eligible for cleanup once
    // the policy allows it (below).
    let original_path = project_dir.path().join("integration_M31_Ha.xisf");
    std::fs::write(&original_path, b"not-a-real-xisf-file")?;

    // Real per-project protection override (US2): without it, the item
    // resolves to the app's safe-by-default "protected" level and the apply
    // step below refuses every item with `protected.source` — not a bug,
    // the documented constitution-II gate — so a real cleanup flow always
    // sets this (or the global default) before a first-time cleanup.
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

    // Attaching the watcher runs a real, synchronous-enough reconciliation
    // pass over existing files (spec 012 T005) — poll `artifact.list` for it
    // rather than assuming it lands before the next call returns.
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

    // Default cleanup policy is all-Keep (safe default) — opt Intermediate
    // into Archive so the generator has a real candidate (not a fixture).
    let _: serde_json::Value = app
        .invoke(
            "cleanup_policy_update",
            json!({
                "request": {
                    "entries": [
                        { "dataType": "intermediate", "action": "archive" },
                        { "dataType": "master", "action": "keep" },
                        { "dataType": "final", "action": "keep" },
                    ],
                    "autoOnCompletion": false,
                }
            }),
        )
        .await?;

    let scan: serde_json::Value =
        app.invoke("cleanup_scan", json!({ "projectId": project_id })).await?;
    anyhow::ensure!(
        !scan["candidates"].as_array().unwrap_or(&Vec::new()).is_empty(),
        "expected a real cleanup candidate for the intermediate artifact: {scan}"
    );

    let generate: serde_json::Value = app
        .invoke("cleanup_plan_generate", json!({ "request": { "projectId": project_id } }))
        .await?;
    let plan_id = generate["planId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("cleanup.plan.generate returned no planId: {generate}"))?
        .to_owned();
    anyhow::ensure!(
        generate["itemCount"].as_i64().unwrap_or(0) >= 1,
        "expected at least 1 real cleanup item on the generated plan: {generate}"
    );

    let approve: serde_json::Value = app.invoke("plans_approve", json!({ "id": plan_id })).await?;
    anyhow::ensure!(
        approve["planId"] == json!(plan_id) && approve["newState"] == "approved",
        "expected plans.approve to move the generated plan to approved: {approve}"
    );
    let approval_token = approve["approvalToken"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("plans.approve returned no approvalToken: {approve}"))?
        .to_owned();

    // Apply — the real filesystem mutation (channel-free, spec 037). The token
    // minted by the approve call above is the gate; no apply path mints its own.
    let apply: serde_json::Value = app
        .invoke("plans_apply_direct", json!({ "planId": plan_id, "approvalToken": approval_token }))
        .await?;
    anyhow::ensure!(
        apply["planId"] == json!(plan_id) && apply["newState"] == "applying",
        "expected plans.apply.direct to start applying the approved plan: {apply}"
    );

    // Poll the real, durable apply status until the executor finishes —
    // `plans.apply.status` reads `plan_apply_events` (the same durable proof
    // `plan_review_apply_with_audit` uses for the inbox path).
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
        "expected the cleanup plan to apply cleanly, got: {status}"
    );
    anyhow::ensure!(
        status["itemsApplied"].as_i64().unwrap_or(0) >= 1,
        "expected at least 1 durably-recorded applied item: {status}"
    );

    // Real filesystem side effect: the original output file moved out of the
    // project folder into the app-managed archive subtree.
    anyhow::ensure!(
        !original_path.exists(),
        "expected the cleanup candidate to have moved away from {original_path:?}"
    );
    let archived_somewhere = std::fs::read_dir(project_dir.path())
        .ok()
        .and_then(|mut entries| {
            entries.find(|e| e.as_ref().is_ok_and(|e| e.file_name() == ".astro-plan-archive"))
        })
        .is_some();
    anyhow::ensure!(
        archived_somewhere,
        "expected a `.astro-plan-archive` subtree under the project folder after apply"
    );

    app.shutdown().await
}
