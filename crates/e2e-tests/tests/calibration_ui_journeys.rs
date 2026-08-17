// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journeys — Calibration masters (batch #3 of the
//! coverage-matrix "Batched plan", Journey 8): masters-ingest-as-individual-
//! items and the Calibration page's kind-conditional master detail, promoted
//! from `docs/development/windows-journeys/journey-08-calibration-masters-matching.md`.
//!
//! Spec 040 (calibration master detection) shipped without a `plan.md`/
//! `tasks.md` (a documented deviation) and had the least automated scrutiny
//! of any recently-shipped backend feature before this file.
//!
//! ## Documented, real gap found while authoring this file (NOT fixed here)
//!
//! Journey-08's Tests 3-5 (ranked-candidate matching view, assign/cancel,
//! offset-tolerance affecting live matching) describe a
//! `MatchCandidatesPanel` (`apps/desktop/src/features/calibration/
//! MatchCandidatesPanel.tsx`) UI. That component is fully implemented and
//! unit-tested (`MatchCandidatesPanel.test.tsx`, jsdom) but **no page mounts
//! it** — `git grep MatchCandidatesPanel apps/desktop/src` outside its own
//! test finds no importer, and `CalibrationPage.tsx` (read in full while
//! authoring this file) renders only `MastersTable` + `MasterDetail`, never
//! the match panel. `CalibrationMatchPanel.tsx` (`features/projects/`, a
//! DIFFERENT, read-only component wired into the project detail page) even
//! says in its own doc comment "assignment is done from the Calibration page
//! (CalibrationPage + MasterDetail)" — that wiring does not exist in the
//! code as of this writing. This is a real product gap, not a test gap: the
//! matching/assign UI is unreachable from the real app today, so it cannot
//! be promoted to a Layer-2 UI journey without a product wiring fix first.
//! Flagged in the coverage matrix rather than faked here.

mod common;

use std::time::Duration;

use common::{settle_first_run_redirect, write_minimal_fits, E2eApp};
use serde_json::json;

const UI_TIMEOUT: Duration = Duration::from_secs(20);

/// Registers a disposable `light_frames` root and a disposable `project`
/// root: `firstrun.complete` (which [`E2eApp::complete_first_run_gate`]
/// issues) specifically requires one of EACH of those two categories — a
/// `calibration` root (this file's actual test subject) does not satisfy
/// either precondition (`crates/persistence/lifecycle/src/repositories/first_run`).
/// Every journey below registers this pair alongside its calibration root so
/// `goto_route` to a Shell-wrapped page (`/inbox`, `/calibration`) doesn't get
/// bounced back to `/setup` by `Shell.tsx`'s client-side `setupCompleted` gate.
async fn complete_first_run(app: &E2eApp) -> anyhow::Result<()> {
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

/// Seed the FIRST scan of `root_id` through the invoke bridge — mirrors
/// `inbox_ui_journeys.rs`'s `seed_initial_scan` (same root cause it
/// documents, CI run 28766017315): the Inbox page's "Rescan all roots"
/// button derives the set of roots it scans from the CURRENT item list
/// (`InboxPage.tsx`'s `roots` `useMemo`, deduped by `item.rootId`), and
/// `useInboxRescan` (`store.ts`) is a real no-op — it never calls
/// `inbox.scan.folder` — whenever that derived list is empty:
/// `if (roots.length === 0) { onComplete(); return; }`. A calibration root
/// registered via `roots_register` has never been scanned, so the item list
/// is empty on first mount, "Rescan all roots" derives zero roots to ping,
/// and no `inbox-item-*` row can EVER appear no matter how long a journey
/// waits — this is what made both journeys below rely on a real button click
/// that could never do the one thing the assertion needs. One bridge-side
/// `inbox.scan.folder` (mirroring the first-run wizard's own scan step, which
/// WebDriver can't drive because it needs the native folder picker) seeds the
/// list so the real Rescan click that follows exercises a root the page
/// actually knows about, exactly like `inbox_ui_journeys.rs` already does.
async fn seed_initial_scan(
    app: &E2eApp,
    root_id: &str,
    root_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let scan: serde_json::Value = app
        .invoke(
            "inbox_scan_folder",
            json!({
                "req": {
                    "rootId": root_id,
                    "rootAbsolutePath": root_dir.to_string_lossy(),
                    "followSymlinks": false,
                }
            }),
        )
        .await?;
    let items = scan["items"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("inbox.scan.folder returned no items array: {scan}"))?;
    anyhow::ensure!(
        !items.is_empty(),
        "expected the seed inbox.scan.folder to discover the fixture master file(s): {scan}"
    );
    Ok(())
}

async fn register_calibration_root(app: &E2eApp) -> anyhow::Result<(tempfile::TempDir, String)> {
    let root_dir = tempfile::tempdir()?;
    let register: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({
                "path": root_dir.path().to_string_lossy(),
                "category": "calibration",
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
    Ok((root_dir, root_id))
}

/// Test 1 (journey-08): a folder with a master dark AND a master bias — real
/// WBPP-style `IMAGETYP = "Master Dark"` / `"Master Bias"` headers, detected
/// by the real `PixInsightDetector`
/// (`crates/calibration/master-detect/src/pixinsight.rs`) — materializes as
/// TWO individual real Inbox items, never one ambiguous folder aggregate.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn calibration_ui_masters_ingest_as_individual_items() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    let (root_dir, root_id) = register_calibration_root(&app).await?;
    write_minimal_fits(
        root_dir.path(),
        "master_dark_001.fits",
        "Master Dark",
        None,
        None,
        Some("2026-01-05T12:00:00"),
    )?;
    write_minimal_fits(
        root_dir.path(),
        "master_bias_001.fits",
        "Master Bias",
        None,
        None,
        Some("2026-01-05T12:00:00"),
    )?;
    // Seed BEFORE `complete_first_run` — mirrors `inbox_ui_journeys.rs`'s
    // ordering exactly. `complete_first_run` -> `complete_first_run_gate`
    // does a real `driver.refresh()` (full page reload) to make the
    // `setupCompleted` localStorage write visible to the cached preferences
    // module. Calling the bridge-side seed invoke AFTER that refresh (as an
    // earlier revision of this file did) races the reload: CI run
    // 28808954263 (ubuntu-latest) hit `__PV_E2E__ bridge missing` on the
    // seed invoke, then a JS execution error on retry — a classic
    // WebDriver-vs-navigation race, not a build flake (12/14 other bridge-
    // using tests in the same job/build passed). Seeding first keeps the
    // invoke on the ORIGINAL page load, well after the initial
    // `wait_bridge_ready`, with no refresh in between.
    seed_initial_scan(&app, &root_id, root_dir.path()).await?;
    complete_first_run(&app).await?;

    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.click_by_aria_label("Rescan all roots").await?;
    if let Err(e) = app.wait_testid_prefix_present("inbox-item-", UI_TIMEOUT).await {
        let backend_items: serde_json::Value = app
            .invoke("inbox_list", json!({}))
            .await
            .unwrap_or_else(|err| json!({ "invoke_error": err.to_string() }));
        let ui_diagnostics = app.dump_ui_diagnostics().await;
        let console_log = app.dump_console_log().await;
        anyhow::bail!(
            "{e} (backend inbox.list={backend_items}, ui_diagnostics={ui_diagnostics}, \
             console_log={console_log})"
        );
    }

    // One live-document snapshot, not `find_all` + per-handle `.text()`: a
    // `stale element reference` defaulted to "" would UNDERCOUNT the master
    // labels and report a driver failure as a missing product label (#1111).
    let row_texts = app.testid_prefix_texts("inbox-item-").await?;
    anyhow::ensure!(
        row_texts.len() >= 2,
        "expected the master dark + master bias to appear as 2 individual real \
         Inbox items (not one folder aggregate), found {}",
        row_texts.len()
    );
    let saw_master_label = row_texts.iter().filter(|t| t.contains("master")).count();
    anyhow::ensure!(
        saw_master_label >= 2,
        "expected each master item's row to carry a real 'N master' label \
         (`inbox_master_row_label`), got {saw_master_label} of {} rows",
        row_texts.len()
    );

    app.shutdown().await
}

/// Test 2 (journey-08): confirming + applying a master bias item via the real
/// Inbox UI registers it as a real `CalibrationMaster` row on the real
/// Calibration page (`master-row-<id>` — added as a thin, additive test hook
/// on `MastersTable.tsx`, mirroring the existing `inbox-item-<id>` row-testid
/// convention already used by `InboxList.tsx`), and the master's detail
/// renders spec-030 Q16 missing-value semantics: every matrix property row is
/// always present — Temperature (not applicable to a bias master) renders the
/// blank not-applicable marker, while applicable-but-missing values (this
/// minimal fixture carries no camera header) render the unresolved chip —
/// never a fabricated number.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn calibration_ui_confirmed_master_shows_kind_conditional_detail() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    let (root_dir, root_id) = register_calibration_root(&app).await?;
    write_minimal_fits(
        root_dir.path(),
        "master_bias_002.fits",
        "Master Bias",
        None,
        None,
        Some("2026-01-06T12:00:00"),
    )?;
    // Seed BEFORE `complete_first_run` — see the matching comment in
    // `calibration_ui_masters_ingest_as_individual_items` for why: the
    // gate's internal `driver.refresh()` must not land between the initial
    // `wait_bridge_ready` and this invoke.
    seed_initial_scan(&app, &root_id, root_dir.path()).await?;
    complete_first_run(&app).await?;

    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.click_by_aria_label("Rescan all roots").await?;
    if let Err(e) = app.wait_testid_prefix_present("inbox-item-", UI_TIMEOUT).await {
        let backend_items: serde_json::Value = app
            .invoke("inbox_list", json!({}))
            .await
            .unwrap_or_else(|err| json!({ "invoke_error": err.to_string() }));
        let ui_diagnostics = app.dump_ui_diagnostics().await;
        let console_log = app.dump_console_log().await;
        anyhow::bail!(
            "{e} (backend inbox.list={backend_items}, ui_diagnostics={ui_diagnostics}, \
             console_log={console_log})"
        );
    }
    let item_id = app.testid_suffix("inbox-item-").await?;
    app.click_testid(&format!("inbox-item-{item_id}")).await?;
    app.wait_testid_enabled("inbox-confirm-btn", UI_TIMEOUT).await.map_err(|e| {
        anyhow::anyhow!(
            "expected the single master-bias item to classify single_type and enable Confirm: {e}"
        )
    })?;
    app.click_testid("inbox-confirm-btn").await?;

    app.wait_testid("inbox-review-plans-btn", UI_TIMEOUT).await?;
    app.click_testid("inbox-review-plans-btn").await?;
    app.wait_testid("plan-panel", UI_TIMEOUT).await?;
    app.click_testid("plan-apply-all").await?;

    // Real backend read: confirm the master really registered
    // (`calibration.masters.list`), and use its real id to find the real DOM
    // row on the Calibration page below — never a fixture id.
    let masters: serde_json::Value = app
        .invoke_until("calibration_masters_list", json!({}), UI_TIMEOUT, |v: &serde_json::Value| {
            v.as_array().is_some_and(|a| !a.is_empty())
        })
        .await?;
    let master = masters
        .as_array()
        .and_then(|a| {
            a.iter().find(|m| m["kind"].as_str().unwrap_or_default().eq_ignore_ascii_case("bias"))
        })
        .ok_or_else(|| {
            anyhow::anyhow!("expected a real bias master in calibration.masters.list: {masters}")
        })?;
    let master_id = master["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("master has no id: {master}"))?
        .to_owned();

    app.goto_route("/calibration").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.wait_testid(&format!("master-row-{master_id}"), UI_TIMEOUT).await.map_err(|e| {
        anyhow::anyhow!(
            "expected the real master {master_id} as its own row on the Calibration page: {e}"
        )
    })?;

    // Select it: `MasterDetail` mounts in the right-side detail pane.
    app.click_testid(&format!("master-row-{master_id}")).await?;

    // Wait for the detail pane to actually render (the "Kind" property is
    // ALWAYS present, unconditionally) before asserting anything is absent —
    // otherwise a not-yet-rendered pane would trivially "pass" the absence
    // check below.
    let deadline = tokio::time::Instant::now() + UI_TIMEOUT;
    loop {
        if page_contains_text(&app, "Kind").await? {
            break;
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "MasterDetail never rendered its 'Kind' property within {UI_TIMEOUT:?}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    // Q16 missing-value semantics (spec-030 FR-135/FR-137): the Temperature
    // row is ALWAYS present. For a bias master `setTemp` is not-applicable
    // per the field-applicability matrix, so it renders the blank marker —
    // a deliberate "n/a", visually distinct from missing data.
    anyhow::ensure!(
        page_contains_text(&app, "Temperature").await?,
        "expected the Temperature property row to be present (not-applicable marker) for a bias master"
    );

    // And applicable-but-missing values (no camera header in this minimal
    // fixture) must surface the unresolved chip — never a fabricated value,
    // never a silently omitted row.
    app.wait_testid("unresolved-chip", UI_TIMEOUT).await.map_err(|e| {
        anyhow::anyhow!(
            "expected at least one unresolved chip in the master detail for \
             applicable-but-missing fields (e.g. camera): {e}"
        )
    })?;

    app.shutdown().await
}

/// `true` if any element on the current page contains the exact text
/// `needle` — a coarse but real DOM check for a property LABEL
/// (`MasterDetail.tsx` renders every matrix row unconditionally under Q16;
/// value state is conveyed by the marker/chip, not row presence).
async fn page_contains_text(app: &E2eApp, needle: &str) -> anyhow::Result<bool> {
    use thirtyfour::error::WebDriverErrorInner;
    let xpath = format!("//*[text()[contains(., '{needle}')]]");
    match app.driver.find(thirtyfour::By::XPath(&xpath)).await {
        Ok(_) => Ok(true),
        Err(e) if matches!(e.as_inner(), WebDriverErrorInner::NoSuchElement(_)) => Ok(false),
        Err(e) => Err(e.into()),
    }
}
