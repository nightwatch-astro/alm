// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 056 Layer-2 real-UI journey: onboarding orientation walk + live
//! auto-tick from real work (T032, VC-004; behavioral contract J18).
//!
//! Real UI → real IPC → real backend, no mocks:
//!  1. Finish the real first-run gate, then assert the orientation walk
//!     auto-renders in the real webview (`.pv-onboarding-tooltip`) and
//!     completes via its real Skip control (FR-001/FR-003).
//!  2. On `/inbox`, the sidebar "Getting started" checklist renders with the
//!     `inbox.confirm_first` auto-tick item still unchecked.
//!  3. Drive a REAL inventory confirm (`roots.register` → `inbox.scan.folder`
//!     → `inbox.classify` → `inbox.confirm`). `inbox.confirm` publishes the
//!     real `inventory.confirmed` bus event
//!     (`crates/app/inbox/src/confirm.rs`); the backend onboarding subscriber
//!     (`apps/desktop/src-tauri/src/commands/onboarding.rs`) persists the tick
//!     for `inbox.confirm_first` (registry `completion_topic`,
//!     `crates/app/core/src/onboarding.rs`) and emits
//!     `onboarding:state-changed`; the store re-reads and the checklist
//!     re-renders.
//!  4. Assert the LIVE auto-tick appears in the product UI: the checklist's
//!     overall progressbar `aria-valuenow` (done count) increments (VC-004).
//!     The progressbar is used rather than the settled item row because a
//!     complete group collapses to a one-line done header (FR-031), so the
//!     per-item `data-state` node can be transient — the section-level done
//!     count is the stable, always-rendered UI reflection of the tick.
//!
//! This is the ONE journey that must let onboarding RUN, so it uses
//! `complete_first_run_gate_onboarding()` (the onboarding-enabled variant);
//! every other journey uses `complete_first_run_gate()`, which sets the
//! deterministic suppression flag so the walk's modal overlay never intercepts
//! its flow (`ONBOARDING_SUPPRESSED_STORE_ID` in `store.ts`).
//!
//! Run (CI): `cargo nextest run -p e2e_tests --profile e2e --run-ignored all`
//! (via `just test-e2e` → `run-e2e-real.sh`). Cannot run in the WSL dev
//! sandbox (no webview) — CI (`e2e.yml`, 3-OS matrix) is the first real run
//! point; see `crates/e2e-tests/README.md`.

mod common;

use std::time::{Duration, Instant};

use common::{
    scan_and_classify_one_item, settle_first_run_redirect, write_minimal_fits_with_exposure, E2eApp,
};
use serde_json::json;

const UI_TIMEOUT: Duration = Duration::from_secs(20);
const INVOKE_TIMEOUT: Duration = Duration::from_secs(30);
const TICK_TIMEOUT: Duration = Duration::from_secs(15);

/// Poll a boolean-returning JS snippet through the WebDriver until it returns
/// `true` or `timeout` elapses. Returns the final observed value.
async fn wait_dom_true(app: &E2eApp, js: &str, timeout: Duration) -> anyhow::Result<bool> {
    let deadline = Instant::now() + timeout;
    loop {
        let ret = app.driver.execute(js, vec![]).await?;
        if ret.convert::<bool>().unwrap_or(false) {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Dump every input to the orientation walk's auto-run gate, for the failure
/// path only. Each condition is reported independently so the log distinguishes
/// "suppression flag leaked in" from "backend state never hydrated" from
/// "orientationDone was already true" — the three ways the gate can silently
/// decline to render. Best-effort: returns a marker string rather than erroring,
/// since this runs while a test is already failing.
async fn walk_gate_diagnostics(app: &E2eApp) -> String {
    let js = r#"
        var out = {};
        try {
            var raw = localStorage.getItem('alm-preferences');
            out.setupCompleted = raw ? JSON.parse(raw).setupCompleted === true : false;
        } catch (e) { out.setupCompleted = 'read-failed: ' + e; }
        out.suppressedFlag = localStorage.getItem('alm-onboarding-suppressed');
        out.shellMounted = !!document.querySelector("[data-testid='frame']");
        out.pageAnchorPresent = !!document.querySelector("[data-testid='frame-main']");
        out.joyrideOverlay = !!document.querySelector('.react-joyride__overlay');
        out.checklistPresent = !!document.querySelector("[data-testid='onb-checklist']");
        out.route = location.hash;
        out.uncaughtErrors = (window.__e2eErrors || []).slice(0, 5);
        return JSON.stringify(out);
    "#;
    let dom = match app.driver.execute(js, vec![]).await {
        Ok(v) => v.convert::<String>().unwrap_or_else(|e| format!("<unconvertible: {e}>")),
        Err(e) => format!("<dom probe failed: {e}>"),
    };
    // Ask the backend directly too: if the projection is healthy here but the
    // store cache is null, the fault is the UI's hydrate path, not the backend.
    let backend = match app.invoke::<serde_json::Value>("onboarding_state_get", json!({})).await {
        Ok(v) => v["state"]["flags"].to_string(),
        Err(e) => format!("<onboarding_state_get failed: {e}>"),
    };
    format!("dom={dom} backendFlags={backend}")
}

/// Open the Getting-started flyout and wait for the checklist inside it.
///
/// The checklist is NOT inline in the sidebar: `ChecklistPopover` portals it to
/// `document.body`, so nothing matching `.pv-onb-checklist` (or any
/// `[data-item-id]`) exists until the `.pv-onb-ring` trigger is clicked. This
/// test predated that redesign and queried the checklist directly, which is why
/// it failed with "checklist item `inbox.confirm_first` did not render".
///
/// Idempotent, because the trigger TOGGLES: it clicks only when `aria-expanded`
/// is not already `"true"`. Navigating closes the flyout, so this must be called
/// again after every `goto_route`.
///
/// Mirrors `openChecklist` in `tests/e2e/support/harness.ts` — the mock-mode
/// Playwright suite needed exactly the same fix for exactly the same reason.
async fn open_checklist(app: &E2eApp) -> anyhow::Result<()> {
    let opened = wait_dom_true(
        app,
        r#"
        var ring = document.querySelector("[data-testid='onb-ring']");
        if (!ring) return false;
        if (ring.getAttribute('aria-expanded') !== 'true') { ring.click(); }
        return !!document.querySelector("[data-testid='onb-checklist']");
        "#,
        UI_TIMEOUT,
    )
    .await?;
    anyhow::ensure!(
        opened,
        "Getting-started flyout did not open (trigger [data-testid='onb-ring'] missing or click ignored); {}",
        walk_gate_diagnostics(app).await
    );
    Ok(())
}

/// Read the checklist overall-progress `aria-valuenow` (settled "done" count).
/// `-1` when the progressbar is not in the DOM. Requires the flyout to be open.
async fn read_progress_done(app: &E2eApp) -> anyhow::Result<i64> {
    let ret = app
        .driver
        .execute(
            r#"
            var pb = document.querySelector("[data-testid='onb-checklist-progress'][role='progressbar']");
            return pb ? Number(pb.getAttribute('aria-valuenow')) : -1;
            "#,
            vec![],
        )
        .await?;
    Ok(ret.convert::<i64>().unwrap_or(-1))
}

/// Orientation walk (real UI) → real inventory confirm → live auto-tick renders
/// in the checklist (VC-004).
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn orientation_walk_then_real_confirm_renders_live_auto_tick() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    // ── Real ingest precondition: one light frame in a light_frames root, plus
    // a project root purely to satisfy `firstrun.complete` (needs one raw AND
    // one project source — see `inventory_journeys.rs::complete_first_run`).
    let root_dir = tempfile::tempdir()?;
    let fixture = write_minimal_fits_with_exposure(
        root_dir.path(),
        "light_m31_001.fits",
        "Light Frame",
        Some("M 31"),
        Some("Ha"),
        Some("2026-01-12T22:00:00"),
        Some(300.0),
    )?;
    anyhow::ensure!(fixture.exists(), "fixture FITS file was not written");

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

    let project_dir = tempfile::tempdir()?;
    let _: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({
                "path": project_dir.path().to_string_lossy(),
                "category": "project",
                "scanSettings": null,
            }),
        )
        .await?;

    // The onboarding-ENABLED gate (every other journey suppresses it): this is
    // the ONE journey that must let the walk auto-run and the checklist render.
    app.complete_first_run_gate_onboarding().await?;

    // ── 1. Real UI: the orientation walk auto-runs (setupCompleted &&
    // !orientationDone && not suppressed).
    let walk_present = wait_dom_true(
        &app,
        r#"return !!document.querySelector("[data-testid='onboarding-tooltip']");"#,
        UI_TIMEOUT,
    )
    .await?;
    if !walk_present {
        // The walk's auto-run gate (OrientationWalk.tsx) is a silent AND of four
        // conditions, and a failed `onboarding.state.get` is swallowed by the
        // store as a console.warn — so a bare "did not render" cannot say WHICH
        // precondition was missing. This dumps all four plus any uncaught error
        // the VITE_E2E buffer captured, so a CI-only failure is diagnosable from
        // the log alone instead of needing a local Windows repro.
        let diag = walk_gate_diagnostics(&app).await;
        anyhow::bail!(
            "orientation walk did not auto-render after the first-run gate \
             (VC-004 / FR-001); gate state: {diag}"
        );
    }

    // Complete the walk via its real Skip control. Skip (not step-through) is
    // deliberate: route navigation per stop + joyride re-anchoring is flaky in
    // a harness that cannot be run locally, and Skip still exercises the real
    // completion path (FR-003) and clears the modal so the sidebar is
    // interactable below.
    app.driver
        .execute(
            r#"
            var btn = document.querySelector("[data-testid='onboarding-tooltip-skip']");
            if (btn) { btn.click(); }
            return !!btn;
            "#,
            vec![],
        )
        .await?;
    let walk_gone = wait_dom_true(
        &app,
        r#"return !document.querySelector("[data-testid='onboarding-tooltip']");"#,
        UI_TIMEOUT,
    )
    .await?;
    anyhow::ensure!(walk_gone, "orientation walk overlay did not dismiss after Skip");

    // Backend flag flipped so it never auto-runs again (FR-004).
    let _done: serde_json::Value = app
        .invoke_until("onboarding_state_get", json!({}), INVOKE_TIMEOUT, |v: &serde_json::Value| {
            v["state"]["flags"]["orientationDone"] == json!(true)
        })
        .await?;

    // ── 2. On /inbox the sidebar checklist renders; the auto-tick item is
    // present and still unchecked (no settled `data-state`).
    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(UI_TIMEOUT).await?;
    open_checklist(&app).await?;
    let item_unchecked = wait_dom_true(
        &app,
        r#"
        var el = document.querySelector('[data-item-id="inbox.confirm_first"]');
        return !!el && el.getAttribute('data-state') !== 'auto_checked';
        "#,
        UI_TIMEOUT,
    )
    .await?;
    anyhow::ensure!(
        item_unchecked,
        "checklist item `inbox.confirm_first` did not render (unchecked) on /inbox"
    );
    let done_before = read_progress_done(&app).await?;
    anyhow::ensure!(
        done_before >= 0,
        "checklist overall progressbar not rendered on /inbox (done_before={done_before})"
    );

    // ── 3. Real inventory confirm → publishes `inventory.confirmed`.
    let inbox_item_id =
        scan_and_classify_one_item(&app, &root_id, root_dir.path().to_string_lossy().as_ref())
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

    let _confirm: serde_json::Value = app
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

    // ── 4. VC-004: the LIVE auto-tick renders — the checklist's overall done
    // count increments once the store re-reads on `onboarding:state-changed`.
    //
    // Re-open first (a no-op when it is still open): the progressbar lives
    // inside the portalled flyout, so a closed flyout would make this read -1
    // and report "the tick never rendered" when the tick was actually fine.
    open_checklist(&app).await?;
    let ticked_in_ui = wait_dom_true(
        &app,
        &format!(
            r#"
            var pb = document.querySelector("[data-testid='onb-checklist-progress'][role='progressbar']");
            return !!pb && Number(pb.getAttribute('aria-valuenow')) > {done_before};
            "#
        ),
        TICK_TIMEOUT,
    )
    .await?;
    anyhow::ensure!(
        ticked_in_ui,
        "live auto-tick never rendered: checklist done count stayed at {done_before} after a \
         real inventory confirm (VC-004)"
    );

    // Belt: the backend projection agrees the tick was persisted (auto, not
    // manual — an event-sourced tick).
    let _final: serde_json::Value = app
        .invoke_until("onboarding_state_get", json!({}), INVOKE_TIMEOUT, |v: &serde_json::Value| {
            v["state"]["items"].as_array().is_some_and(|items| {
                items.iter().any(|i| {
                    i["itemId"] == json!("inbox.confirm_first")
                        && i["state"] == json!("auto_checked")
                })
            })
        })
        .await?;

    app.shutdown().await
}
