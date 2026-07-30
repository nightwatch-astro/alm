// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journey — Sessions derived-view invariants
//! (batch #9 of the coverage-matrix "Batched plan", Journey 4). Promotes
//! `docs/development/windows-journeys/journey-04-sessions-review.md`'s
//! UI-level Tests (1/2/3/5) — the existing `ingestion_sessions_search`
//! journey (`journeys.rs`) already proves the real event-driven grouping
//! pipeline via `sessions.list`; this file adds the real Sessions PAGE
//! assertions that journey doesn't touch (nothing before apply, no
//! review-state controls, rescan doesn't duplicate).
//!
//! ## Finding while authoring this file: journey-04 Test 4 is untestable as
//! written
//!
//! Test 4 ("edit a session's Notes field, let it auto-save") describes a
//! Notes field on the session detail. `SessionDetail.tsx`
//! (read in full while authoring this) has NO notes field at all — its own
//! doc comment says "Session metadata remains editable post-hoc via the
//! inbox per-file metadata/override tables", and a repo-wide search for a
//! session notes field/command found none. Spec 041 FR-051 (T076) removed
//! the review-lifecycle actions (Confirm/Re-open/Reject/Ignore) — this
//! journey's own module doc suggests the notes-editing claim in journey-04
//! may be a stale holdover from before that removal, not a real, currently
//! reachable feature. Test 4 is therefore skipped here rather than faked;
//! flagged in the coverage matrix as a documentation-accuracy follow-up
//! (either the feature needs to ship, or journey-04 needs correcting).

mod common;

use std::time::Duration;

use anyhow::Context;
use common::{settle_first_run_redirect, write_minimal_fits_with_exposure, E2eApp};
use serde_json::json;

const UI_TIMEOUT: Duration = Duration::from_secs(30);

/// Registers a disposable `light_frames` root and a disposable `project`
/// root purely to satisfy `firstrun.complete`'s precondition (one of EACH,
/// `crates/persistence/lifecycle/src/repositories/first_run`), then routes
/// through the real gate. Without this, `Shell.tsx`'s client-side
/// `setupCompleted` gate bounces every `goto_route` to a Shell-wrapped page
/// (`/sessions`, `/inbox`) back to `/setup` indefinitely (mirrors the proven
/// `inbox_ui_journeys.rs` pattern).
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

/// Force a real reload of the CURRENTLY-loaded route rather than calling
/// `E2eApp::goto_route` again with the identical URL.
///
/// `goto_route` builds `{APP_URL}/#{path}` and calls `driver.goto(url)`; when
/// the app is already sitting on that exact URL (no intervening navigation
/// away from it), asking the browser to "navigate to" a byte-identical URL
/// is a well-known no-op in several engines (no reload, no route remount, no
/// TanStack Query refetch) — this bit Test 2 below, which re-visits
/// `/sessions` immediately after the real ingest pipeline runs entirely over
/// the invoke bridge (no navigation happens in between), so the page never
/// re-fetches and the newly-applied session's row never appears within the
/// polling deadline (CI: both ubuntu and windows hung on
/// `wait_testid_prefix_present("sessions-row-", ..)`, ruling out a timing
/// flake). An explicit `driver.refresh()` is unambiguous.
///
/// KEPT as a reload (#1113 reviewed) rather than converted to
/// `invalidate_query`: this helper is route-generic — it re-enters whatever
/// route the app is on and has no single query key to invalidate — and its
/// callers re-find every element afterwards, so no handle survives the
/// reload. A caller that needs to settle ONE query before asserting on the
/// DOM should use `E2eApp::invalidate_query` instead of this.
async fn reload_current_route(app: &E2eApp) -> anyhow::Result<()> {
    app.driver.refresh().await.context("page refresh failed")?;
    app.wait_document_ready(Duration::from_secs(10)).await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await
}

/// Tests 1/2/3/5 (journey-04) in one journey: nothing appears before a plan
/// applies, a real session appears automatically (event-driven grouping)
/// after apply with no separate review step, no review-state controls exist
/// anywhere on the page, and a no-op Inbox rescan never duplicates the
/// session.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn sessions_ui_derived_view_invariants() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;
    complete_first_run(&app).await?;

    // Test 1: nothing appears before any inbox item is confirmed + applied.
    app.goto_route("/sessions").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    anyhow::ensure!(
        app.find_all_testid_prefix("sessions-row-").await?.is_empty(),
        "expected Sessions to show nothing before any plan has applied"
    );

    // Real ingest pipeline (mirrors `ingestion_sessions_search` in
    // `journeys.rs`) — this journey's new value is the Sessions PAGE
    // assertions below, not re-proving the ingest pipeline itself.
    let root_dir = tempfile::tempdir()?;
    write_minimal_fits_with_exposure(
        root_dir.path(),
        "light_m31_sessions_001.fits",
        "Light Frame",
        Some("M 31"),
        Some("Ha"),
        Some("2026-01-12T21:30:00"),
        Some(300.0),
    )?;
    let register: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({ "path": root_dir.path().to_string_lossy(), "category": "light_frames", "scanSettings": null }),
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
    let _: serde_json::Value = app
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
    let _: serde_json::Value =
        app.invoke("inbox_plan_apply", json!({ "inboxItemId": inbox_item_id })).await?;

    // Test 2: the real session appears automatically on the real Sessions
    // page (no separate review/approve step). Session grouping is
    // event-driven — `inbox.plan_apply` returns as soon as the plan's
    // executor finishes; the plan-listener that folds the applied light
    // frame into an `acquisition_session` row (spec 035 US4/T042,
    // `ingest_light_frames_if_applicable` in
    // `crates/app/inbox/src/plan_listener.rs`) reacts to the
    // `plan.applying.completed` event on its OWN background task afterwards.
    // A single reload raced that background task (CI: both ubuntu and
    // windows hung on `wait_testid_prefix_present`, ruling out a timing
    // flake — the backend simply hadn't grouped the frame into a session
    // yet by the time the one-shot TanStack Query fetch ran, and nothing
    // thereafter re-triggers a refetch for `wait_testid_prefix_present` to
    // observe). Poll the real backend command the page itself queries
    // (`inventory.list` via `useInventorySources`, NOT `sessions.list` —
    // that's a different, session-review-era projection) until the grouped
    // session exists, mirroring `invoke_until`'s documented wait primitive
    // for this exact class of event-driven backend effect
    // (`journeys.rs`'s `ingestion_sessions_search`). Only once the backend
    // is ready does reloading the UI mean anything.
    app.invoke_until(
        "inventory_list",
        json!({
            "req": {
                "contractVersion": "2.0.0",
                "requestId": "e2e-sessions-derived-view-poll",
                "filters": null,
            }
        }),
        UI_TIMEOUT,
        |v: &serde_json::Value| {
            v["sources"].as_array().is_some_and(|sources| {
                sources.iter().any(|s| s["sessions"].as_array().is_some_and(|ss| !ss.is_empty()))
            })
        },
    )
    .await
    .map_err(|e| {
        anyhow::anyhow!(
            "expected the backend to group the applied light frame into a real \
             acquisition session: {e}"
        )
    })?;

    // The app is ALREADY sitting on `/sessions` from Test 1 (no navigation
    // happened during the ingest pipeline above, which is invoke-only) —
    // reload rather than `goto_route` to the identical URL, see
    // `reload_current_route`. The backend is now known-ready, so the page's
    // one-shot fetch on remount will find the session immediately.
    reload_current_route(&app).await?;
    app.wait_testid_prefix_present("sessions-row-", UI_TIMEOUT).await.map_err(|e| {
        anyhow::anyhow!("expected a real session row to appear automatically after apply: {e}")
    })?;
    let rows_after_apply = app.find_all_testid_prefix("sessions-row-").await?;
    anyhow::ensure!(
        rows_after_apply.len() == 1,
        "expected exactly 1 real session after applying 1 light frame, found {}",
        rows_after_apply.len()
    );

    // Test 3: select it and confirm NO review-state controls exist anywhere
    // on the page (list + detail) — the intentionally-removed
    // Confirm/Re-open/Reject/Ignore review lifecycle (spec 041 FR-051/T076).
    let session_id = app.testid_suffix("sessions-row-").await?;
    app.click_testid(&format!("sessions-row-{session_id}")).await?;
    for label in ["Confirm", "Re-open", "Reopen", "Reject", "Ignore"] {
        anyhow::ensure!(
            app.count_buttons_with_text(label).await? == 0,
            "expected NO '{label}' review-lifecycle control anywhere on the Sessions page \
             (spec 041 FR-051 removed this state machine)"
        );
    }

    // Test 5: a no-op Inbox rescan (no new files) must never duplicate the
    // session or resurrect a review state.
    app.goto_route("/inbox").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.click_by_aria_label("Rescan all roots").await?;
    // Give the (no-op) rescan a moment to settle, then re-check Sessions.
    app.wait_bridge_ready(Duration::from_secs(15)).await?;

    app.goto_route("/sessions").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.wait_testid_prefix_present("sessions-row-", UI_TIMEOUT).await?;
    let rows_after_rescan = app.find_all_testid_prefix("sessions-row-").await?;
    anyhow::ensure!(
        rows_after_rescan.len() == 1,
        "expected a no-op rescan to never duplicate the session, found {} rows",
        rows_after_rescan.len()
    );

    app.shutdown().await
}
