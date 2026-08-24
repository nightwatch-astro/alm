// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journey: per-frame inventory reconciliation
//! (spec 048).
//!
//! Real backend REAL: `roots.register`, `inbox.scan.folder`,
//! `inbox.classify`, `inbox.confirm`, `inbox.plan.apply` (catalogue-in-place),
//! `projects.create`, `inventory.reconcile.run`
//! (`apps/desktop/src-tauri/src/commands/inventory_frame.rs`, spec 048 T006).
//!
//! Real DOM (trigger): Settings → Data Sources
//! (`apps/desktop/src/features/settings/DataSources.tsx`) now renders a
//! per-root "Reconcile now" button (`reconcile-now-<rootId>` testid) wired to
//! `inventory.reconcile.run` — previously a documented gap (zero frontend
//! callers; see git history on this file for the prior invoke-bridge
//! workaround). This journey clicks that REAL button rather than invoking
//! the command directly.
//!
//! Real DOM (effect): the project's "Add sources" session picker
//! (`SessionSourcePicker`, mounted from `EditProjectPane`) queries
//! `sessions.list` — the spec-048 T014 active/non-missing `frame_count`
//! read path (`crates/app/core/src/sessions.rs::active_frame_summary`) — and
//! renders it per-session. This journey reads that REAL, product-rendered
//! frame count before and after the real, UI-triggered reconcile pass.
//!
//! Catalogue-in-place: same reasoning as `source_view_journeys.rs` — the
//! `roots.register` default (`organized`) keeps both fixture files at their
//! literal on-disk paths, so this journey can delete one of them directly to
//! simulate a real, external raw-frame loss (the disconnected-drive /
//! moved-by-another-tool scenario spec 048 exists to detect).
//!
//! Run (CI): `cargo nextest run -p e2e_tests --profile e2e --run-ignored all`
//! (serial, `.config/nextest.toml`). See `crates/e2e-tests/tests/journeys.rs`
//! module docs and `README.md` for the full local run procedure.

mod common;

use std::time::Duration;

use common::{settle_first_run_redirect, write_minimal_fits_with_exposure, E2eApp};
use serde_json::json;
use thirtyfour::By;

const INVOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Registers a disposable `project`-category root purely to satisfy
/// `firstrun.complete`'s precondition (one `light_frames` root — this
/// journey's own ingest root satisfies that half — AND one `project` root,
/// `crates/persistence/lifecycle/src/repositories/first_run`), then routes
/// through the real gate. A `projects.create` Project entity (this journey
/// creates one below) is a DIFFERENT concept from a registered `project`
/// source root and does not satisfy this precondition on its own. Without
/// this, `Shell.tsx`'s client-side `setupCompleted` gate bounces every
/// `goto_route` to a Shell-wrapped page (`/projects`) back to `/setup`
/// indefinitely (mirrors the proven `inbox_ui_journeys.rs` pattern).
async fn complete_first_run(app: &E2eApp) -> anyhow::Result<()> {
    let project_dir = tempfile::tempdir()?;
    let _: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({ "path": project_dir.path().to_string_lossy(), "category": "project", "scanSettings": null }),
        )
        .await?;
    app.complete_first_run_gate().await
}

/// External raw-frame deletion → clicking the real "Reconcile now" button
/// (Settings → Data Sources) → the real Add-sources session picker's frame
/// count drops from 2 to 1.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn reconcile_drops_externally_deleted_frame_from_real_ui_count() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    // ── 1. Real ingest precondition: two same-identity light frames group ──
    // into ONE real `acquisition_session` with `frame_count == 2` (same
    // OBJECT/FILTER/GAIN/BINNING/night, spec 035 US4), catalogued in place so
    // both files stay at their real, individually-deletable paths.
    let root_dir = tempfile::tempdir()?;
    let keep_name = "light_m33_001.fits";
    let lose_name = "light_m33_002.fits";
    let keep_path = write_minimal_fits_with_exposure(
        root_dir.path(),
        keep_name,
        "Light Frame",
        Some("M 33"),
        Some("Ha"),
        Some("2026-01-12T22:00:00"),
        Some(300.0),
    )?;
    let lose_path = write_minimal_fits_with_exposure(
        root_dir.path(),
        lose_name,
        "Light Frame",
        Some("M 33"),
        Some("Ha"),
        Some("2026-01-12T23:00:00"),
        Some(300.0),
    )?;
    anyhow::ensure!(
        keep_path.exists() && lose_path.exists(),
        "fixture FITS files were not written"
    );

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
    let _apply: serde_json::Value =
        app.invoke("inbox_plan_apply", json!({ "inboxItemId": inbox_item_id })).await?;
    anyhow::ensure!(
        keep_path.exists() && lose_path.exists(),
        "catalogue-in-place (organized default) must never move either file"
    );

    // Event-driven session grouping (spec 035 US4 plan_listener) — poll until
    // BOTH frames have joined the same session.
    let sessions: serde_json::Value = app
        .invoke_until("sessions_list", json!({}), INVOKE_TIMEOUT, |v: &serde_json::Value| {
            v.as_array().is_some_and(|arr| {
                arr.iter().any(|s| {
                    s["sessionKey"]["target"] == "M 33" && s["frameCount"].as_i64() == Some(2)
                })
            })
        })
        .await?;
    let session = sessions
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s["sessionKey"]["target"] == "M 33"))
        .ok_or_else(|| anyhow::anyhow!("no M 33 session found: {sessions}"))?;
    let session_id = session["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("session has no id: {session}"))?
        .to_owned();
    anyhow::ensure!(
        session["frameCount"].as_i64() == Some(2),
        "expected the two same-identity frames to group into one 2-frame session: {session}"
    );

    // ── 2. Real project (setup precondition) ──
    //
    // This journey's DOM focus is the Add-sources session picker's real frame
    // count, not project creation itself — created over the invoke bridge
    // like every other journey's preconditions.
    let project_dir = tempfile::tempdir()?;
    let create: serde_json::Value = app
        .invoke(
            "projects_create",
            json!({
                "req": {
                    "requestId": "e2e-inventory-create",
                    "name": "E2E Per-Frame Inventory Project",
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

    complete_first_run(&app).await?;

    // ── 3. Real UI (BEFORE): open the project, open Add sources, read the ──
    // real per-session frame count from the real DOM.
    app.goto_route("/projects").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.wait_testid(&format!("project-row-{project_id}"), Duration::from_secs(15))
        .await?
        .click()
        .await?;
    app.wait_testid("edit-project-btn", Duration::from_secs(15)).await?.click().await?;
    app.wait_testid("edit-project-add-sources-toggle", Duration::from_secs(10))
        .await?
        .click()
        .await?;
    let frames_before = app
        .wait_testid_text(
            &format!("session-picker-frames-{session_id}"),
            Duration::from_secs(10),
            |text| !text.trim().is_empty(),
        )
        .await?;
    anyhow::ensure!(
        frames_before.trim() == "2",
        "expected the real Add-sources picker to show frameCount=2 before reconcile: \
         {frames_before:?}"
    );

    // ── 4. Real filesystem mutation: an external tool/user deletes one raw ──
    // frame from disk — the exact scenario spec 048 exists to detect.
    std::fs::remove_file(&lose_path)?;
    anyhow::ensure!(!lose_path.exists(), "fixture file was not actually removed: {lose_path:?}");
    anyhow::ensure!(keep_path.exists(), "the surviving frame must still be present: {keep_path:?}");

    // ── 5. Real UI trigger: click "Reconcile now" on the root's card in ──
    // Settings → Data Sources. This is the real button added by spec 048
    // T022 (frontend) — `inventory.reconcile.run` previously had zero UI
    // callers; see this file's module docs for the prior invoke-bridge
    // workaround this replaces.
    //
    // #894 (kebab-menu consolidation, merged to main after this journey was
    // written) moved every per-source action — including "Reconcile now" —
    // behind a per-root kebab (⋯) menu trigger
    // (`apps/desktop/src/features/settings/DataSources.tsx`,
    // `pv-data-sources__kebab-btn`): `reconcile-now-<rootId>` is now a MENU
    // ITEM that only exists in the DOM once its OWN card's kebab is open, so
    // it must be opened first. Only two roots exist on this page at this
    // point (this journey's own `light_frames`/`raw`-category root, and the
    // throwaway `project`-category root `complete_first_run` registers to
    // satisfy the first-run gate) — `CATEGORY_ORDER` in `DataSources.tsx`
    // renders `raw` before `project`, so the first kebab trigger in document
    // order is deterministically this root's.
    app.goto_route("/settings/sources").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.driver
        .find(By::Css("[data-testid='data-sources-kebab-btn']"))
        .await
        .map_err(|e| anyhow::anyhow!("failed to find the root's kebab (⋯) menu trigger: {e}"))?
        .click()
        .await
        .map_err(|e| anyhow::anyhow!("failed to open the root's kebab (⋯) menu: {e}"))?;
    let reconcile_testid = format!("reconcile-now-{root_id}");
    app.wait_testid(&reconcile_testid, Duration::from_secs(15))
        .await?
        .click()
        .await
        .map_err(|e| anyhow::anyhow!("failed to click the real Reconcile now button: {e}"))?;

    // ── 5b. Poll the SAME backend read the picker uses until it reflects ──
    // the drop, BEFORE reloading (mirrors the fix for
    // sessions_ui_derived_view_invariants, `git log 5c4ab4c5`: a command
    // returning does not guarantee every downstream read is immediately
    // consistent — a single page load only fetches once, and if that one
    // fetch lands before the backend state is fully settled, nothing
    // thereafter retriggers a refetch for `wait_testid_text` to catch. This
    // proves `sessions.list` itself — the exact command the picker queries —
    // has caught up (i.e. the UI-triggered reconcile actually completed and
    // took effect) before the reload below, so the reload's one-shot fetch
    // cannot race it.
    app.invoke_until("sessions_list", json!({}), INVOKE_TIMEOUT, |v: &serde_json::Value| {
        v.as_array().is_some_and(|arr| {
            arr.iter().any(|s| s["id"] == json!(session_id) && s["frameCount"].as_i64() == Some(1))
        })
    })
    .await
    .map_err(|e| anyhow::anyhow!("sessions.list never settled to frameCount=1 for the reconciled session before the UI reload: {e}"))?;

    // ── 6. Real UI (AFTER): force sessions.list to re-fetch ──
    //
    // `goto_route` only changes the URL fragment on the SAME document — per
    // the HTML fragment-navigation algorithm this never creates a new
    // Document, so the shared `QueryClient` (30s `staleTime`,
    // `apps/desktop/src/data/queryClient.ts`) survives across it and the
    // `sessions.list` query stays cached from the BEFORE read above. The
    // freshness guarantee is the `invalidate_query` below, which refetches
    // that exact query in place.
    //
    // This step used to ALSO do a `driver.refresh()` to discard the whole
    // QueryClient. Removed per #1113: a reload remounts the app through the
    // setup gate and route restore, so the document the assertions read can
    // be torn down under them while WebDriver keeps serving detached handles
    // from the pre-reload document — and it was never the actual guarantee
    // here anyway (the comment on `invalidate_query` below already recorded
    // that the reload "is not a guaranteed proof of that on every WebDriver
    // backend"). Invalidation keeps one live document.
    app.goto_route("/projects").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.wait_testid(&format!("project-row-{project_id}"), Duration::from_secs(15))
        .await?
        .click()
        .await?;
    app.wait_testid("edit-project-btn", Duration::from_secs(15)).await?.click().await?;
    app.wait_testid("edit-project-add-sources-toggle", Duration::from_secs(10))
        .await?
        .click()
        .await?;

    // Deterministic fix for the cross-PR flake (CI evidence: "last seen:
    // Some(\"2\")" surviving the full 15s wait — only possible from a
    // served-stale-cache render, since step 5b above already proved a fresh
    // backend read returns 1). Invalidating the exact query the picker reads
    // is the whole freshness guarantee for this read — see step 6 for why the
    // page reload that used to precede it was removed.
    // Lane nD's frontend reconcile invalidation (PR #517, MERGED) wires this
    // same invalidation into the real "Reconcile" button's click handler, but
    // this journey calls `inventory_reconcile_run` directly over the invoke
    // bridge (no UI trigger for that path, module docs' KNOWN GAP) — #517
    // does not cover it. Belt-and-braces now rather than the only fix; drop
    // once the bridge-triggered path has a few weeks of green CI.
    app.invalidate_query(r#"["sessions"]"#).await?;

    let frames_after = app
        .wait_testid_text(
            &format!("session-picker-frames-{session_id}"),
            Duration::from_secs(15),
            |text| text.trim() == "1",
        )
        .await?;
    anyhow::ensure!(
        frames_after.trim() == "1",
        "expected the real Add-sources picker to show frameCount=1 after reconcile: \
         {frames_after:?}"
    );

    app.shutdown().await
}
