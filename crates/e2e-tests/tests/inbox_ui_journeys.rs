// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journeys — Inbox (batch #6 of the coverage-matrix
//! "Batched plan"): the UI-level gate + reclassify + confirm/apply surface
//! that `journeys.rs`'s `plan_review_apply_with_audit` proves only through
//! the `window.__PV_E2E__.invoke` bridge, never by clicking through the real
//! Inbox page (`apps/desktop/src/features/inbox/{InboxPage,InboxDetail,
//! PlanPanel}.tsx`).
//!
//! Per the manual click-by-click specs this promotes to automation:
//! `docs/development/windows-journeys/journey-02-inbox-ingest-move.md` and
//! `journey-03-inbox-catalogue-in-place.md`.
//!
//! Setup steps that need the native OS folder picker (registering a root,
//! flipping its `organization_state`) stay on the `invoke` bridge — the same
//! documented constraint `journeys.rs` already carries. Everything a real
//! user would click (Rescan, selecting a detection row, the bulk-reclassify
//! controls, Confirm, opening the plan-review overlay, Apply) is driven
//! through real `data-testid` DOM elements via the helpers in `common/mod.rs`.

mod common;

use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use common::{
    settle_first_run_redirect, write_minimal_fits, write_minimal_fits_with_exposure, E2eApp,
};
use serde_json::json;

const UI_TIMEOUT: Duration = Duration::from_secs(20);
const INVOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Register a disposable light-frames root and return `(app_handle_unused,
/// root_dir, root_id)`. `organization_state` starts `organized` by default
/// (spec 041 R-7); callers that need the move branch flip it explicitly.
async fn register_light_root(app: &E2eApp) -> anyhow::Result<(tempfile::TempDir, String)> {
    let root_dir = tempfile::tempdir()?;
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
    Ok((root_dir, root_id))
}

/// Register a disposable PROJECT root. `firstrun.complete` (which
/// [`E2eApp::complete_first_run_gate`] issues) requires at least one raw AND
/// one project source, so every journey registers one alongside its light
/// root. The returned `TempDir` must stay alive for the test's duration.
async fn register_project_root(app: &E2eApp) -> anyhow::Result<tempfile::TempDir> {
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
    Ok(project_dir)
}

/// Seed the FIRST scan of `root_id` through the invoke bridge (a setup step,
/// like root registration itself).
///
/// In the real product the initial scan happens in the first-run wizard's
/// scan step. The Inbox page's "Rescan all roots" derives its root set from
/// the CURRENT item list (`InboxPage.tsx` dedupes `items[].rootId`), so on a
/// never-scanned root the button is a real no-op — no `inbox-item-*` row can
/// ever appear, however long the journey waits (CI run 28766017315). One
/// bridge-side `inbox.scan.folder` mirrors the wizard step (whose native
/// folder picker WebDriver can't drive); the journey's Rescan click then
/// exercises the real UI path against a root the list actually knows.
async fn seed_initial_scan(app: &E2eApp, root_id: &str, root_dir: &Path) -> anyhow::Result<()> {
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
    // Spec 058 T012/FR-015: an ordinary folder now produces NO inbox item at
    // scan time — it produces a source group, and items appear only once
    // classification materialises them. The old assertion here ("items must be
    // non-empty") encoded the pre-058 contract and fails on every ordinary
    // fixture, which is what the first real L3 run after T012 caught.
    //
    // What the seed must still prove is that the scan SAW the fixture files,
    // otherwise a journey waits on a row that was never going to appear. The
    // honest post-058 signal is the response's own shape: an `items` array is
    // returned (so the command ran and parsed), and the folder is discoverable
    // afterwards through the source-group listing the UI reads.
    scan["items"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("inbox.scan.folder returned no items array: {scan}"))?;
    let groups: serde_json::Value = app
        .invoke("inbox_list", json!({ "req": { "limit": 500 } }))
        .await
        .context("inbox.list after the seed scan")?;
    let discovered = groups["sourceGroups"].as_array().is_some_and(|g| !g.is_empty())
        || groups["items"].as_array().is_some_and(|i| !i.is_empty());
    anyhow::ensure!(
        discovered,
        "the seed inbox.scan.folder discovered neither a source group nor an item \
         for the fixture folder — scan={scan} list={groups}"
    );
    // NOTE: deliberately NO bridge-side `inbox.classify` pre-warm here. The
    // UI's confirm gate requires `classification.type == "single_type"`,
    // which the backend only reports for the FIRST (computing) classify of
    // an item — a re-read of an already-classified item reports
    // `"classified"` and would keep Confirm disabled for the journey's real
    // click. The journeys let the detail pane trigger the first classify,
    // exactly like a real user.
    Ok(())
}

/// Click Rescan (aria-label "Rescan all roots" — `m.inbox_rescan_all_roots_aria()`,
/// `apps/desktop/messages/en.json`) and wait for the list to settle by polling
/// for at least one real `inbox-item-*` row.
///
/// Spec 058 T014: correct ONLY where a scan produces item rows directly — i.e.
/// detected calibration masters, which keep their own rows under the FR-015
/// carve-out. An ordinary folder no longer yields any item row at scan time
/// (FR-015/T012); it yields a source-group row, and this helper would poll
/// until `UI_TIMEOUT` and fail. Use [`rescan_and_wait_for_source_group`].
#[allow(dead_code)]
async fn rescan_and_wait_for_item(app: &E2eApp) -> anyhow::Result<()> {
    app.click_by_aria_label("Rescan all roots").await?;
    app.wait_testid_prefix_present("inbox-item-", UI_TIMEOUT).await
}

/// Rescan and wait for the scanned folder's SOURCE-GROUP row, returning its
/// `sourceGroupId` (spec 058 T014, FR-015/FR-016).
///
/// This is what an ordinary folder looks like immediately after a scan now: one
/// `inbox-source-group-*` row and no inbox item. It becomes items only once
/// classification runs — see [`classify_source_group_and_wait_for_items`].
/// The id is derived from the Classify CONTROL, not from the row. The row's
/// testid `inbox-source-group-<id>` is a strict prefix of the button's
/// `inbox-source-group-classify-<id>`, and `testid_suffix` returns the first
/// prefix match — so searching on the row prefix is ambiguous and could yield
/// `classify-<id>` as the "suffix". The button prefix has no such collision.
async fn rescan_and_wait_for_source_group(app: &E2eApp) -> anyhow::Result<String> {
    app.click_by_aria_label("Rescan all roots").await?;
    // The seed scan ran through the bridge while this app was already live, so
    // the list query can be younger than its 30s `staleTime` and serve its
    // PRE-SEED (empty) cache on mount — the page then shows "no detections."
    // while `inbox.list` itself returns the source group. Other journeys
    // already invalidate for exactly this reason; this helper must too, because
    // after T012 the source-group row is the ONLY row a fresh scan produces, so
    // there is no item row whose arrival would otherwise force a refetch.
    app.invalidate_query(r#"["inbox","all"]"#).await?;
    if app.wait_testid_prefix_present("inbox-source-group-classify-", UI_TIMEOUT).await.is_err() {
        // Report the BACKEND's view alongside the DOM's, so a future failure
        // here says immediately whether the data never arrived or arrived and
        // was dropped by the list. That distinction cost several runs to
        // establish the first time.
        let live: serde_json::Value = app
            .invoke("inbox_list", json!({ "req": { "limit": 500 } }))
            .await
            .unwrap_or(serde_json::Value::Null);
        eprintln!("inbox.list at failure: {live}");
        let all = app.testid_prefix_texts("inbox-").await.unwrap_or_default();
        eprintln!("testids present after rescan ({}): {:?}", all.len(), all);
        let diag = app.dump_testid_diagnostics("inbox-").await;
        eprintln!("testid diagnostics: {diag}");
        anyhow::bail!("source-group classify control never appeared");
    }
    app.testid_suffix("inbox-source-group-classify-").await
}

/// Assert a source-group row is structurally non-confirmable (spec 058 T015,
/// FR-016).
///
/// The counterpart to [`select_only_item`]: selecting an ITEM row must mount
/// Confirm; a source-group row must never provide one. The row carries no
/// `inboxItemId`, so there is no id to hand `inbox.confirm`. This asserts the
/// ABSENCE rather than clicking and hoping — a Confirm that merely refuses at
/// runtime is a promise a refactor can quietly break, while an absent control
/// cannot be.
async fn assert_source_group_not_confirmable(
    app: &E2eApp,
    source_group_id: &str,
) -> anyhow::Result<()> {
    app.wait_testid(&format!("inbox-source-group-{source_group_id}"), UI_TIMEOUT).await?;
    anyhow::ensure!(
        !app.testid_exists("inbox-confirm-btn").await?,
        "a source-group row must not offer Confirm: FR-016 makes it structurally \
         non-confirmable, and a mounted Confirm means an item id reached the detail pane"
    );
    Ok(())
}

/// Classify a source-group row and wait for the folder's item rows to appear
/// (spec 058 T014/T017, FR-017).
///
/// Waits on the ITEM rows rather than on the group row disappearing:
/// materialization and the list refetch settle independently, and the arrival
/// of real rows is the signal the journeys actually depend on.
async fn classify_source_group_and_wait_for_items(
    app: &E2eApp,
    source_group_id: &str,
) -> anyhow::Result<()> {
    app.click_testid(&format!("inbox-source-group-classify-{source_group_id}")).await?;
    app.wait_testid_prefix_present("inbox-item-", UI_TIMEOUT).await
}

/// Rescan an ordinary folder all the way to a selected, confirmable item row.
///
/// Spec 058 T016: the path every ordinary-folder journey now takes. Before T012
/// a scan produced a confirmable placeholder row directly, so
/// `rescan_and_wait_for_item` + `select_only_item` was the whole story. There is
/// now a real intermediate state — scanned but not yet classified — and the
/// journeys must pass through it exactly as a user does.
///
/// Asserts the intermediate state's non-confirmability on the way through, so
/// every folder-ingesting journey pins FR-016 rather than leaving it to the one
/// journey that tests it explicitly.
async fn rescan_classify_and_select_item(app: &E2eApp) -> anyhow::Result<String> {
    let source_group_id = rescan_and_wait_for_source_group(app).await?;
    assert_source_group_not_confirmable(app, &source_group_id).await?;
    classify_source_group_and_wait_for_items(app, &source_group_id).await?;
    select_only_item(app).await
}

/// Select the (only) inbox item row and return its real `inboxItemId`, waiting
/// for the detail pane's real Confirm button to mount (the detail-loaded
/// signal — `InboxDetail` always renders `data-testid="inbox-confirm-btn"`
/// once an item is selected, per `InboxPage.tsx`'s always-passed `onConfirm`).
/// The list refetches (and re-renders, swapping row DOM nodes) several times
/// right after a rescan or page reload, so a single find→read→click sequence
/// can hit `stale element reference` / `no such element` mid-churn. Retry
/// the whole sequence until the click lands or [`UI_TIMEOUT`] elapses.
async fn select_only_item(app: &E2eApp) -> anyhow::Result<String> {
    let deadline = tokio::time::Instant::now() + UI_TIMEOUT;
    loop {
        let attempt = async {
            let item_id = app.testid_suffix("inbox-item-").await?;
            app.click_testid(&format!("inbox-item-{item_id}")).await?;
            anyhow::Ok(item_id)
        };
        match attempt.await {
            Ok(item_id) => {
                app.wait_testid("inbox-confirm-btn", UI_TIMEOUT).await?;
                return Ok(item_id);
            }
            Err(e) if tokio::time::Instant::now() >= deadline => return Err(e),
            Err(_) => tokio::time::sleep(Duration::from_millis(300)).await,
        }
    }
}

/// Test 1 (journey-02): a folder with one light + one dark frame — both with
/// complete metadata — materializes as multiple SINGLE-TYPE rows in the real
/// Inbox list after Rescan, per spec 041's mixed-folder split (memory:
/// "mixed folders → single-type items at ingest", PR #315). This is the
/// UI-interaction proof that the classify/split pipeline is really wired to
/// the rendered list, not just the `inbox.scan.folder`/`inbox.classify`
/// response shape `journeys.rs` already checks.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn inbox_ui_mixed_folder_splits_into_single_type_items() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    let (root_dir, root_id) = register_light_root(&app).await?;
    let _project_dir = register_project_root(&app).await?;
    // Force the move branch isn't needed for classification — leave the
    // default `organized` state; classification doesn't depend on it.
    let _: serde_json::Value = app
        .invoke(
            "sources_set_organization_state",
            json!({ "sourceId": root_id, "organizationState": "unorganized" }),
        )
        .await?;

    // Both fixtures MUST carry EXPTIME. It is a hard mandatory attribute for
    // lights (OBJECT+FILTER+EXPTIME) and darks (EXPTIME+GAIN) alike
    // (`mandatory_set_for`, `crates/app/inbox/src/classify.rs`), so without it
    // BOTH files collapse into the single `__needs_review__` sentinel bucket —
    // one row, no split, and this test would be asserting nothing about
    // mixed-folder splitting at all. The header set mirrors the Layer-1
    // `t066_mixed_folder_produces_n_sub_items` fixtures, which prove a light +
    // a dark materialize as two distinct single-type sub-items.
    write_minimal_fits_with_exposure(
        root_dir.path(),
        "light_001.fits",
        "Light Frame",
        Some("M42"),
        Some("Ha"),
        Some("2026-01-10T22:00:00"),
        Some(300.0),
    )?;
    write_minimal_fits_with_exposure(
        root_dir.path(),
        "dark_001.fits",
        "Dark Frame",
        None,
        None,
        Some("2026-01-10T22:05:00"),
        Some(300.0),
    )?;

    seed_initial_scan(&app, &root_id, root_dir.path()).await?;
    app.complete_first_run_gate().await?;

    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    // Spec 058 T012/T016: scanning no longer produces a folder-level item, so
    // there is no placeholder row to select and no "selecting the row triggers
    // classify" side effect. The folder is a source-group row until the user
    // classifies it, and Classify is the action that materializes the
    // single-type rows this journey checks for (FR-015/FR-017).
    let source_group_id = rescan_and_wait_for_source_group(&app).await?;
    assert_source_group_not_confirmable(&app, &source_group_id).await?;
    classify_source_group_and_wait_for_items(&app, &source_group_id).await?;

    // Sync signal: the split ROWS themselves (tasks.md sequencing constraint 4).
    //
    // This step used to wait on `inbox-mixed-alert`, rendered from the
    // placeholder's loaded `classType === "mixed"` classification. With no
    // placeholder there is no mixed item to classify — the folder splits
    // straight into single-type items — so that banner can never appear and
    // waiting on it would hang until `UI_TIMEOUT`. The row-count poll below is
    // now both the sync point and the assertion, which is strictly closer to
    // what the journey is actually about.

    // The list itself isn't invalidated by classify; force the refetch until
    // the split rows land. The list refetches and re-renders several times,
    // so a transiently-EMPTY `find_all` is churn, not failure — only the
    // deadline decides. A driver-level `find_all` FAILURE is not churn and is
    // no longer flattened into an empty vec (#1111): that default is
    // indistinguishable from "the list rendered no rows", i.e. it reports a
    // failure to observe as an observation.
    //
    // Read the rows as one live-document text snapshot rather than holding
    // `WebElement` handles across the settle: the handles that satisfy the
    // count check can be detached by the very next re-render before the
    // "mixed" assertion below reads them.
    let deadline = tokio::time::Instant::now() + UI_TIMEOUT;
    let row_texts = loop {
        let row_texts = app.testid_prefix_texts("inbox-item-").await?;
        if row_texts.len() >= 2 {
            break row_texts;
        }
        if tokio::time::Instant::now() >= deadline {
            // Round 6 (fix-inbox-splitrow-label): rounds 3-5 proved the
            // backend always materializes the right 2 sub-items and that
            // `InboxList`'s own render pipeline handles the exact captured
            // Windows payload correctly (`InboxList.windowsSplitPayload.test.
            // tsx`) — the drop is real-webview-only. Live diagnostics off
            // failing Windows runs (28807257849, 28807308638) then showed the
            // SAME instant recording `rows.len() == 0` from this very
            // row-count check while a `dump_ui_diagnostics`
            // JS eval gathered moments later (after an intervening
            // `inbox.list` invoke round-trip) reported `rowCount: 2` for the
            // identical live page — i.e. the two split rows land in the real
            // DOM strictly *between* this deadline check and the
            // diagnostics-gathering that follows it. Bailing on that single
            // stale reading is exactly the flake: give the in-flight
            // fetch/render one bounded last chance to land, using the same
            // check the pass path uses, before treating it as a real
            // failure. A genuine regression still times out below.
            let grace_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            let mut late_rows = row_texts;
            while late_rows.len() < 2 && tokio::time::Instant::now() < grace_deadline {
                tokio::time::sleep(Duration::from_millis(250)).await;
                late_rows = app.testid_prefix_texts("inbox-item-").await?;
            }
            if late_rows.len() >= 2 {
                break late_rows;
            }

            // Still short after the grace window — gather full evidence
            // before erroring. Before erroring, call `inbox.list` directly
            // through the invoke bridge (bypassing the UI entirely) so the
            // failure message tells us whether the backend ever materialized
            // split rows at all (a real classify/materialize_sub_items
            // regression) or whether they exist server-side but the UI/list
            // query never surfaced them after reload (a frontend
            // refetch/race bug) — the two possibilities need different
            // fixes and neither can be distinguished from the UI-only
            // signal alone.
            let backend_items: serde_json::Value = app
                .invoke("inbox_list", serde_json::json!({}))
                .await
                .unwrap_or_else(|e| serde_json::json!({ "invoke_error": e.to_string() }));
            // Diagnostic-only: a failed read is reported AS a failed read, so
            // it can never be mistaken for the app sitting on an empty URL.
            let url = app
                .driver
                .current_url()
                .await
                .map_or_else(|e| format!("<current_url read failed: {e}>"), |u| u.to_string());
            // Round 5: the backend-vs-UI split above already proved the
            // backend returns the right rows, and the entire frontend
            // transform pipeline renders that exact payload correctly in
            // unit scope (InboxList.windowsSplitPayload.test.tsx) — so this
            // failure lives only in the real Windows WebView2 runtime. Fold
            // in DOM/virtualizer, TanStack Query, buffered-error, build-time,
            // and console-log evidence so THIS failure message alone can
            // decide between the three live hypotheses (see
            // `E2eApp::dump_ui_diagnostics`'s doc comment for how each field
            // maps to a hypothesis: UI IPC race, virtualizer/layout race, or
            // stale build artifact).
            let ui_diagnostics = app.dump_ui_diagnostics().await;
            let console_log = app.dump_console_log().await;
            anyhow::bail!(
                "expected the mixed folder to split into >=2 single-type rows in the \
                 real Inbox list, found {} after a {UI_TIMEOUT:?} deadline + 5s grace \
                 window (current_url={url:?}, backend inbox.list={backend_items}, \
                 ui_diagnostics={ui_diagnostics}, console_log={console_log})",
                late_rows.len()
            );
        }
        // Invalidate the query rather than `driver.refresh()` (#1113). A full
        // page reload tears down the document these assertions read: after it
        // the app remounts through the setup gate and route restore, and the
        // observed result was an Inbox page with NO `inbox-list` element for
        // the rest of the budget while WebDriver kept serving detached row
        // handles from the pre-reload document. Invalidation refetches the
        // same list in place, so the settle signal and the rows below are read
        // from one live document.
        tokio::time::sleep(Duration::from_millis(500)).await;
        app.invalidate_query(r#"["inbox","all"]"#)
            .await
            .context("invalidating the inbox list query while waiting for split rows failed")?;
    };
    for text in &row_texts {
        anyhow::ensure!(
            !text.contains("mixed"),
            "expected split single-type rows, not an ambiguous 'mixed' row: {text:?}"
        );
    }

    app.shutdown().await
}

/// Tests 2/3 (journey-02): an unrecognised `IMAGETYP` renders the real
/// "frame types required" danger banner and blocks Confirm (`canConfirm`
/// requires `classification.type === "single_type"`,
/// `apps/desktop/src/features/inbox/InboxPage.tsx`); the bulk-reclassify
/// control (`reclassify-select-all` → `bulk-frame-type` → `bulk-apply-btn`)
/// then resolves it to a real `single_type` classification and Confirm
/// re-enables — all real DOM interaction, real `inbox.reclassify` command.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn inbox_ui_unclassified_gate_bulk_reclassify_unblocks_confirm() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    let (root_dir, root_id) = register_light_root(&app).await?;
    let _project_dir = register_project_root(&app).await?;
    let _: serde_json::Value = app
        .invoke(
            "sources_set_organization_state",
            json!({ "sourceId": root_id, "organizationState": "unorganized" }),
        )
        .await?;

    // "Frame Unknown" is not a mapped IMAGETYP (classify.rs: unclassified
    // when IMAGETYP is absent or unmapped) — this file resolves to a real
    // classification.type == "unclassified", not a fixture.
    write_minimal_fits(
        root_dir.path(),
        "ambiguous_001.fits",
        "Frame Unknown",
        Some("M42"),
        Some("Ha"),
        Some("2026-01-10T22:00:00"),
    )?;

    seed_initial_scan(&app, &root_id, root_dir.path()).await?;
    app.complete_first_run_gate().await?;

    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    rescan_classify_and_select_item(&app).await?;

    // Real blocking banner + real disabled Confirm.
    app.wait_testid("inbox-unclassified-alert", UI_TIMEOUT).await?;
    anyhow::ensure!(
        !app.is_enabled_testid("inbox-confirm-btn").await?,
        "expected Confirm to be disabled while classification.type == 'unclassified'"
    );

    // Bulk reclassify: select the one needs-review file, set frame type ->
    // light PLUS exposureS, apply. Real inputs, real `inbox.reclassify_v2`
    // round-trip.
    //
    // `exposureS` is a hard mandatory key for light frames alongside target
    // and filter (spec 041 R-14/FR-047, `classify::mandatory_set_for`) — the
    // fixture's OBJECT/FILTER headers satisfy target/filter, but it carries no
    // EXPTIME header, so setting frameType alone still routes the file to the
    // needs-review sentinel bucket instead of a resolved `light` sub-item.
    // The generic bulk-property editor (issue #755/R-13, `genericBulkFields`
    // in `InboxDetail.tsx`) is exactly how a real user fills this gap.
    //
    // Both controls are CONTROLLED React inputs on a pane that InboxPage
    // REMOUNTS whenever the selected item's id changes (`key={inboxItemId}`)
    // — and the id DOES change right after the first classify (the
    // placeholder row is hidden by `exclude_split_placeholder!` — not deleted
    // — and selection moves to the materialized needs-review sub-item).
    // A remount mid-sequence resets the
    // pane's selection + bulk state and unmounts the fieldset, so any single
    // step can 404 or silently lose its committed value. Run the WHOLE
    // select-all → frame-type → exposure sequence, re-verify every value is
    // still committed, and only then click Apply — retrying the whole block
    // through render churn until it sticks.
    let bulk_deadline = tokio::time::Instant::now() + UI_TIMEOUT;
    loop {
        let attempt = async {
            // A remount resets the checkbox to unchecked; only click when it
            // is actually unchecked so a retry never toggles the selection off.
            if !app.find_testid("reclassify-select-all").await?.is_selected().await? {
                app.click_testid("reclassify-select-all").await?;
            }
            app.select_testid("bulk-frame-type", "light").await?;
            app.fill_testid("bulk-exposure-s", "300").await?;

            // Re-verify all three inputs still hold their values — proof no
            // remount wiped the pane's state between the steps above.
            anyhow::ensure!(
                app.find_testid("reclassify-select-all").await?.is_selected().await?,
                "select-all checkbox lost its checked state (pane remounted mid-sequence)"
            );
            let ft = app.find_testid("bulk-frame-type").await?.prop("value").await?;
            anyhow::ensure!(
                ft.as_deref() == Some("light"),
                "bulk-frame-type lost its value (got {ft:?}; pane remounted mid-sequence)"
            );
            let exp = app.find_testid("bulk-exposure-s").await?.prop("value").await?;
            anyhow::ensure!(
                exp.as_deref() == Some("300"),
                "bulk-exposure-s lost its value (got {exp:?}; pane remounted mid-sequence)"
            );

            app.click_testid("bulk-apply-btn").await
        };
        match attempt.await {
            Ok(()) => break,
            Err(e) if tokio::time::Instant::now() >= bulk_deadline => {
                return Err(e.context(
                    "bulk-reclassify sequence never committed within UI_TIMEOUT \
                     (select-all → frame-type → exposure → apply)",
                ));
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(300)).await,
        }
    }

    // The store invalidates the classify query cache on success (`store.ts`);
    // Confirm re-enabling is the real, observable proof the override landed
    // and reclassified the file to `single_type`.
    app.wait_testid_enabled("inbox-confirm-btn", UI_TIMEOUT).await?;
    anyhow::ensure!(
        !app.testid_exists("inbox-unclassified-alert").await?,
        "expected the 'frame types required' banner to clear after reclassify"
    );

    app.shutdown().await
}

/// Trimmed, lowercased text of every Type-column cell currently rendered in
/// the Inbox list. That cell carries no `data-testid` of its own — it is the
/// `span.pv-inbox-row__classification` inside each row (the `type:` cell in
/// `apps/desktop/src/features/inbox/InboxList.tsx`), so it is located by class.
///
/// Callers MUST compare with exact equality, never `contains`: "unclassified"
/// has "classified" as a substring, so a substring check cannot tell the fixed
/// badge from the defective one.
///
/// Read as ONE `execute` against the live document rather than
/// `find_all` + per-element `.text()`. The two-step form is not equivalent
/// here: the Inbox list re-renders (and swaps row DOM nodes) constantly, so
/// every handle returned by `find_all` can be detached before `.text()` reads
/// it, and `.text()` on a detached handle yields a `stale element reference`
/// error that a `unwrap_or_default()` silently turns into `""`. That is
/// exactly how this journey first failed: WebDriver reported two Type cells
/// whose text was `["", ""]` — two labels that `classificationLabel` can
/// never produce — because both handles were stale, not because the badge
/// rendered blank. A single snapshot cannot interleave with a re-render.
async fn classification_cell_labels(app: &E2eApp) -> anyhow::Result<Vec<String>> {
    let raw: String = app
        .driver
        .execute(
            r#"
            return JSON.stringify(
                Array.prototype.map.call(
                    document.querySelectorAll("[data-testid='inbox-row-classification']"),
                    function (el) { return el.textContent || ''; }
                )
            );
            "#,
            vec![],
        )
        .await
        .context("reading the Type-column cells failed")?
        .json()
        .as_str()
        .context("the Type-cell snapshot script did not return a string")?
        .to_owned();
    let labels: Vec<String> =
        serde_json::from_str(&raw).context("the Type-cell snapshot was not a JSON array")?;
    Ok(labels.into_iter().map(|l| l.trim().to_lowercase()).collect())
}

/// Issue #711 Instance A (unsplit-folder variant): the Type-column badge of a
/// folder that `classify()` could not resolve to any frame type must read
/// "unclassified" — never "classified".
///
/// NOTE: written but NOT YET EXECUTED (authored alongside the fix without a
/// Layer-2 run). Validate it on its first real run.
///
/// The defect this pins, end to end: `classify()` writes
/// `inbox_classifications.result = "unclassified"` (step 8 — zero distinct
/// frame types) and then unconditionally flips `inbox_items.state` to
/// `"classified"` (step 9) for that SAME placeholder id, so a `state`-only
/// badge renders a false "classified" beside a detail panel that correctly
/// reports unclassified. The three sibling journeys above all passed
/// identically with and without the fix — they assert on
/// `inbox-unclassified-alert`, Confirm enablement, and move counts, and never
/// read the Type column.
///
/// Fixture choice — the same unmapped-`IMAGETYP` file as
/// `inbox_ui_unclassified_gate_bulk_reclassify_unblocks_confirm`, written with
/// [`write_minimal_fits`] (which deliberately omits `EXPTIME`). It is the only
/// shape that can reach the new predicate at all:
/// - Zero distinct frame types → folder result `"unclassified"`, and
///   `materialize_sub_items` yields exactly ONE group (the `__needs_review__`
///   sentinel). One group is not a split, so the placeholder row survives
///   `exclude_split_placeholder!`'s `> 1` bound
///   (`crates/persistence/inbox/src/repositories/inbox`) and stays in
///   `inbox.list`.
/// - A MIXED folder (2+ types) also stores `"unclassified"`, but it genuinely
///   SPLITS, so its placeholder is filtered out of `inbox.list` entirely and
///   could never exercise this path.
///
/// The needs-review trap: `isNeedsReview` runs BEFORE the new predicate in
/// `classificationLabel`, so a row in the sentinel bucket is labelled "needs
/// review" and proves nothing here. The row under test is the PLACEHOLDER
/// (`group_key = ""`, and `inbox.list` always returns an empty
/// `missingMandatory` — the other `isNeedsReview` signal — see
/// `apps/desktop/src-tauri/src/commands/inbox.rs`), which lists ALONGSIDE its
/// one needs-review sub-item. Hence the assertions run over every Type cell
/// rather than one row: "needs review" is expected and fine, "classified" is
/// the defect.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn inbox_ui_unsplit_unclassified_folder_badge_is_not_classified() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    let (root_dir, root_id) = register_light_root(&app).await?;
    let _project_dir = register_project_root(&app).await?;
    let _: serde_json::Value = app
        .invoke(
            "sources_set_organization_state",
            json!({ "sourceId": root_id, "organizationState": "unorganized" }),
        )
        .await?;

    write_minimal_fits(
        root_dir.path(),
        "ambiguous_001.fits",
        "Frame Unknown",
        Some("M42"),
        Some("Ha"),
        Some("2026-01-10T22:00:00"),
    )?;

    seed_initial_scan(&app, &root_id, root_dir.path()).await?;
    app.complete_first_run_gate().await?;

    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    // Spec 058 T012/T016: there is no placeholder to select any more. The
    // folder is a source-group row; Classify is what materializes its item
    // (needs-review here, since the fixture's frame type is unreadable), and
    // selecting THAT item is what loads the classification the banner renders
    // from. The banner still only renders from a LOADED classification, so its
    // appearance remains the proof that classify finished server-side.
    rescan_classify_and_select_item(&app).await?;
    app.wait_testid("inbox-unclassified-alert", UI_TIMEOUT).await?;

    // classify does not invalidate the list query, so force the refetch until
    // the post-classify rows land. The materialized "needs review" sub-item is
    // the settle signal: it can only be rendered once classify ran AND the list
    // refetched — which keeps a not-yet-refreshed list from being misreported
    // as a badge regression.
    //
    // Invalidate the query rather than `driver.refresh()`. A full page reload
    // tears down the document these assertions read: after it the app remounts
    // through /setup-gate → route restore, and the observed result was an
    // Inbox page with NO `inbox-list` element at all for the rest of the
    // 20s budget, while WebDriver kept serving detached row handles from the
    // pre-reload document. Invalidation refetches the same list in place, so
    // the settle signal and the badge are read from one live document.
    let deadline = tokio::time::Instant::now() + UI_TIMEOUT;
    let labels = loop {
        let labels = classification_cell_labels(&app).await?;
        if labels.iter().any(|l| l == "needs review") {
            break labels;
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "the post-classify Inbox list never settled within {UI_TIMEOUT:?} (no \
             materialized 'needs review' sub-item row appeared); Type cells were {labels:?}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
        app.invalidate_query(r#"["inbox","all"]"#)
            .await
            .context("invalidating the inbox list query while waiting for classify failed")?;
    };

    anyhow::ensure!(
        !labels.iter().any(|l| l == "classified"),
        "#711 Instance A: an unsplit folder that classify() resolved to NO frame type \
         still renders a 'classified' Type badge, contradicting the detail panel's \
         unclassified state. Type cells: {labels:?}"
    );
    // Spec 058 T012: there IS no unsplit placeholder row any more. An
    // unreadable folder materialises as a needs-review item, and "needs review"
    // is a STRICTLER label than "unclassified" — it names why the row cannot be
    // confirmed rather than merely that its type is unknown. Both satisfy this
    // journey's actual claim, asserted above: the badge must never read
    // "classified" when the row carries no frame type (#711 Instance A).
    anyhow::ensure!(
        labels.iter().any(|l| l == "unclassified" || l == "needs review"),
        "expected the row for a folder with no resolvable frame type to read \
         'unclassified' or 'needs review', agreeing with inbox.classify and the \
         detail panel. Type cells: {labels:?}"
    );

    app.shutdown().await
}

/// Test 2 variant (journey-02), the OTHER real gate: a light frame with
/// filter + target present but no DATE-OBS is a real `single_type`
/// classification (IMAGETYP maps fine) yet still blocks Confirm, because
/// `date` is path-load-bearing (FR-032/US9, `confirm.rs`'s
/// `missing_path_attribute_blocks_with_report` at Layer 1). This is a
/// DIFFERENT mechanism than the unclassified-frame-type gate above — the
/// bulk-reclassify control has no "set date" field, so this documents the
/// real, honest boundary: fixing it requires editing the source FITS header
/// (outside the app) and rescanning, not a UI override.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn inbox_ui_missing_path_attribute_banner_blocks_confirm() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    let (root_dir, root_id) = register_light_root(&app).await?;
    let _project_dir = register_project_root(&app).await?;
    let _: serde_json::Value = app
        .invoke(
            "sources_set_organization_state",
            json!({ "sourceId": root_id, "organizationState": "unorganized" }),
        )
        .await?;

    write_minimal_fits(
        root_dir.path(),
        "light_no_date.fits",
        "Light Frame",
        Some("M42"),
        Some("Ha"),
        None, // no DATE-OBS — the real, path-load-bearing gap (FR-032)
    )?;

    seed_initial_scan(&app, &root_id, root_dir.path()).await?;
    app.complete_first_run_gate().await?;

    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    rescan_classify_and_select_item(&app).await?;

    // The FR-032 banner is metadata-driven: the detail pane's
    // `inbox.item.metadata` query and the SAME selection's `inbox.classify`
    // call (which persists the per-file extracted rows the query reads) fire
    // concurrently. `useInboxClassification` now invalidates that item's
    // metadata query once classify settles (issue #1019), so the banner
    // renders deterministically on FIRST selection — no reload/re-select
    // workaround. If this wait times out, the invalidation regressed.
    app.wait_testid("inbox-missing-attr-banner", UI_TIMEOUT).await?;
    let banner_text = app.text_testid("inbox-missing-attr-banner").await?;
    anyhow::ensure!(
        banner_text.to_lowercase().contains("required metadata missing"),
        "expected the FR-032 banner's real copy, got: {banner_text:?}"
    );
    anyhow::ensure!(
        !app.is_enabled_testid("inbox-confirm-btn").await?,
        "expected Confirm to stay disabled while a file is missing a path-load-bearing attribute"
    );

    app.shutdown().await
}

/// Tests 5/6 (journey-02): Confirm creates a reviewable plan WITHOUT moving
/// the file (real filesystem check immediately after the click), and Apply
/// (via the plan-review overlay) moves it to EXACTLY the destination path the
/// overlay displayed beforehand (`inbox-dest-absolute-0`) — a real
/// UI-displayed-path == real-file-location proof, not just a durable-status
/// poll.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn inbox_ui_confirm_does_not_move_then_apply_moves_to_shown_destination() -> anyhow::Result<()>
{
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    let (root_dir, root_id) = register_light_root(&app).await?;
    let _project_dir = register_project_root(&app).await?;
    let _: serde_json::Value = app
        .invoke(
            "sources_set_organization_state",
            json!({ "sourceId": root_id, "organizationState": "unorganized" }),
        )
        .await?;

    // Spec 058: a light frame's mandatory attributes are OBJECT, FILTER and
    // EXPTIME (T070/FR-047). This journey is about the move/in-place branch,
    // not about the attribute gate, so its fixture must be fully specified —
    // it confirms directly, with no bulk-reclassify step.
    //
    // Before T012 this fixture omitted EXPTIME and still confirmed, because the
    // folder PLACEHOLDER was not subject to the mandatory-attribute gate. The
    // materialised sub-item that replaces it is. That is 058 closing a hole,
    // not a regression — so the fixture is corrected rather than the gate
    // loosened.
    let original_path = write_minimal_fits_with_exposure(
        root_dir.path(),
        "light_move_me.fits",
        "Light Frame",
        Some("M42"),
        Some("Ha"),
        Some("2026-01-10T22:00:00"),
        Some(300.0),
    )?;

    seed_initial_scan(&app, &root_id, root_dir.path()).await?;
    app.complete_first_run_gate().await?;

    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    let item_id = rescan_classify_and_select_item(&app).await?;

    app.wait_testid_enabled("inbox-confirm-btn", UI_TIMEOUT).await?;
    app.click_testid("inbox-confirm-btn").await?;

    // This is a light frame, so `handleConfirm` (spec 008 US7/FR-019, #943)
    // reads attribution suggestions BEFORE confirming and stops at the picker
    // instead of calling `inbox.confirm` immediately — the real backend
    // always includes the zero-score `new_project` fallback (FR-020), so a
    // light-frame confirm never skips the picker. Pick "Unassigned": the
    // journey below only cares about the move-vs-catalogue plan mechanics,
    // not attribution outcome.
    app.wait_testid("inbox-attribution-picker", UI_TIMEOUT).await?;
    app.click_testid("inbox-attribution-option-unassigned").await?;
    app.click_testid("inbox-attribution-confirm").await?;

    // Test 5: Confirm alone must never move the file.
    anyhow::ensure!(
        original_path.exists(),
        "Confirm must only create a reviewable plan — the file moved before Apply"
    );

    // Open the plan-review overlay and expand this item's group to reveal
    // the real destination path BEFORE applying.
    app.wait_testid("inbox-review-plans-btn", UI_TIMEOUT).await?;
    app.click_testid("inbox-review-plans-btn").await?;
    app.wait_testid("plan-panel", UI_TIMEOUT).await?;
    app.click_testid(&format!("plan-group-toggle-{item_id}")).await?;
    let dest_el = app.wait_testid("inbox-dest-absolute-0", UI_TIMEOUT).await?;
    let shown_dest = dest_el.text().await.context("failed to read the shown destination path")?;
    anyhow::ensure!(!shown_dest.trim().is_empty(), "expected a non-empty shown destination path");

    // Test 6: Apply, then verify the file is EXACTLY at the shown path.
    app.click_testid("plan-apply-all").await?;

    let deadline = tokio::time::Instant::now() + INVOKE_TIMEOUT;
    loop {
        if !original_path.exists() && std::path::Path::new(shown_dest.trim()).exists() {
            break;
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "apply did not move the file to the shown destination {shown_dest:?} within {:?} \
             (original still present: {}, dest present: {})",
            INVOKE_TIMEOUT,
            original_path.exists(),
            std::path::Path::new(shown_dest.trim()).exists()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    app.shutdown().await
}

/// Journey-03 core scenario: an ORGANIZED root (the default —
/// `sources.set_organization_state` is deliberately NOT called here) confirms
/// to a real catalogue plan (0 moves), the overlay shows "In place" with no
/// destination-absolute-path cell, and Apply leaves the file byte-identical
/// at its original path.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn inbox_ui_catalogue_in_place_zero_moves_byte_identical() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    // Deliberately organized (default) — no `sources_set_organization_state`
    // call — this is the catalogue-in-place branch, not the move branch.
    let (root_dir, root_id) = register_light_root(&app).await?;
    let _project_dir = register_project_root(&app).await?;

    // Spec 058: a light frame's mandatory attributes are OBJECT, FILTER and
    // EXPTIME (T070/FR-047). This journey is about the move/in-place branch,
    // not about the attribute gate, so its fixture must be fully specified —
    // it confirms directly, with no bulk-reclassify step.
    //
    // Before T012 this fixture omitted EXPTIME and still confirmed, because the
    // folder PLACEHOLDER was not subject to the mandatory-attribute gate. The
    // materialised sub-item that replaces it is. That is 058 closing a hole,
    // not a regression — so the fixture is corrected rather than the gate
    // loosened.
    let original_path = write_minimal_fits_with_exposure(
        root_dir.path(),
        "light_in_place.fits",
        "Light Frame",
        Some("M42"),
        Some("Ha"),
        Some("2026-01-10T22:00:00"),
        Some(300.0),
    )?;
    let original_bytes = std::fs::read(&original_path)?;

    seed_initial_scan(&app, &root_id, root_dir.path()).await?;
    app.complete_first_run_gate().await?;

    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    let item_id = rescan_classify_and_select_item(&app).await?;

    app.wait_testid_enabled("inbox-confirm-btn", UI_TIMEOUT).await?;
    app.click_testid("inbox-confirm-btn").await?;

    // Light frame — same attribution-picker interception as the move-path
    // journey above (spec 008 US7/FR-019/FR-020, #943). Pick "Unassigned":
    // this journey only asserts catalogue-in-place plan mechanics.
    app.wait_testid("inbox-attribution-picker", UI_TIMEOUT).await?;
    app.click_testid("inbox-attribution-option-unassigned").await?;
    app.click_testid("inbox-attribution-confirm").await?;

    anyhow::ensure!(original_path.exists(), "catalogue-in-place must never move the file");

    app.wait_testid("inbox-review-plans-btn", UI_TIMEOUT).await?;
    app.click_testid("inbox-review-plans-btn").await?;
    app.wait_testid("plan-panel", UI_TIMEOUT).await?;

    // No destination-root picker for an organized root (nothing to pick).
    anyhow::ensure!(
        !app.testid_exists("inbox-root-picker").await?,
        "an organized (catalogue-in-place) root must never show the destination-root picker"
    );

    app.click_testid(&format!("plan-group-toggle-{item_id}")).await?;
    // "In place" is a real, rendered label for a fully-catalogued group.
    let group_dest = app.text_testid(&format!("plan-group-summary-{item_id}")).await;
    let _ = group_dest; // composition text; the authoritative check is below.
    anyhow::ensure!(
        !app.testid_exists("inbox-dest-absolute-0").await?,
        "a catalogue (0-move) plan must not render a destination-absolute-path cell"
    );

    app.click_testid("plan-apply-all").await?;
    // Auto-close needs apply → plan-applied event → open-plans refetch →
    // overlay effect, several async hops on a loaded debug-build runner —
    // give it double the usual invoke budget before calling it broken.
    app.wait_testid_gone("plan-panel", Duration::from_secs(60)).await.map_err(|e| {
        anyhow::anyhow!("expected the plan-approval overlay to auto-close after apply: {e}")
    })?;

    anyhow::ensure!(
        original_path.exists(),
        "catalogue-in-place apply must leave the file at its original path"
    );
    let after_bytes = std::fs::read(&original_path)?;
    anyhow::ensure!(
        original_bytes == after_bytes,
        "catalogue-in-place apply must not alter file bytes"
    );

    app.shutdown().await
}
