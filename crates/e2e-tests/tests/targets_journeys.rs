// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journeys — Targets catalog, SIMBAD resolve-on-
//! demand path (offline seed hit, no live network), the stub-disclosure
//! guard, and real planner astronomy after an observing site is created
//! (batches #7 and #8 of the coverage-matrix "Batched plan", Journey 9).
//! Promotes `docs/development/windows-journeys/journey-09-targets-planning.md`.
//!
//! Deliberately DOES NOT exercise a live SIMBAD network lookup (journey-09
//! Test 3): a real HTTP dependency in a CI journey would be flaky and the
//! repo's own convention for #14 (SIMBAD resolution) is the offline
//! `FakeResolver` at Layer 1 — `target.resolve`/`target.search`'s bundled
//! offline seed cache (loaded on every boot, see `journeys.rs` module docs)
//! gives a deterministic, network-free real backend round-trip instead.
//!
//! Batch #8 (real planner astronomy) was documented as blocked on PR #440
//! (observing-site creation UI). Verified against `apps/desktop/src/features/
//! targets/site-gate.ts` while authoring this file: `readSiteExists()` now
//! reads the real `observing-sites/site-store` (no longer a hardcoded
//! `false`) — #440 has landed, so `targets_planner_real_astronomy_after_site_creation`
//! below is no longer a "gate-prompt-only" placeholder journey.

mod common;

use std::time::Duration;

use anyhow::Context;
use common::{settle_first_run_redirect, E2eApp};
use serde_json::{json, Value};
use thirtyfour::{By, WebElement};

const UI_TIMEOUT: Duration = Duration::from_secs(20);

/// Extended timeout for waits that depend on the Targets table rendering its
/// first row. On Windows CI cold boots the 13k-row bundled seed load can take
/// 30-60s after the app process starts, and the targets list TanStack Query
/// won't resolve until the seed is loaded. TRY-1 failures at exactly 20s
/// (standard `UI_TIMEOUT`) that pass on TRY-2 in <8s are the signature of a
/// warm-disk-cache dependency, not a product bug.
const TARGETS_TABLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Real, per-target disclosure titles (spec 047) — asserted verbatim against
/// `apps/desktop/messages/en.json` so a copy change fails this test loudly
/// instead of silently drifting.
const OPPOSITION_UNKNOWN_TITLE: &str = "Opposition date unknown — this target has no coordinates.";
const LUNAR_UNKNOWN_TITLE: &str = "Lunar distance unknown — this target has no coordinates.";

/// Complete first-run via the invoke bridge (native folder pickers can't be
/// driven by WebDriver — same documented constraint as `journeys.rs`/
/// `smoke.rs`) so `/targets` is directly reachable.
///
/// Registers BOTH a raw (`light_frames`) and a `project` root:
/// [`E2eApp::complete_first_run_gate`] requires at least one of each, and
/// routing through the real gate (not a bare `firstrun_complete` invoke)
/// also clears the Shell's separate `setupCompleted` localStorage flag — a
/// journey that only calls the backend command still gets bounced to
/// `/setup` on every subsequent `goto_route` (`inbox_ui_journeys.rs`'s
/// `register_project_root`/`complete_first_run_gate` pairing is the proven
/// pattern this mirrors).
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

/// DIAGNOSTIC ONLY (not a fix): capture what the `.pv-target-search` widget
/// actually rendered when the suggestion-poll loop in [`add_target_via_ui`]
/// times out, so the NEXT CI run tells us whether Phase 1 (`target.search`)
/// threw, is still "resolving" (stuck on Phase 2 / SIMBAD), or genuinely
/// rendered zero suggestions.
///
/// Two refuted hypotheses already ruled out by code review (see the branch's
/// PR discussion): (1) the test fills the input via real `send_keys`, not a
/// JS value-set, so React's change/debounce handlers should fire; (2) Phase 1
/// (local seed search, `crates/app/targets/src/target_search.rs`, no network)
/// completes and calls `setSuggestions` BEFORE Phase 2 (SIMBAD `target.resolve`)
/// even starts, so a stuck/slow Phase 2 should not be able to block Phase 1's
/// already-rendered result. This capture exists to falsify or confirm both
/// with real evidence instead of guessing further.
///
/// Everything is printed via `anyhow::Error`'s `Display`/`Debug` chain so it
/// shows up directly in the nextest failure's stdout/stderr — no artifact
/// upload plumbing required. A best-effort screenshot is ALSO written to
/// `target/e2e-diagnostics/` (uploaded by the workflow's
/// `upload-artifact` step on failure) in case the DOM text alone doesn't
/// explain it.
async fn dump_target_search_diagnostics(app: &E2eApp, query: &str) -> String {
    let mut report = format!("=== target-search diagnostics for query {query:?} ===\n");

    // (a) DOM dump: outerHTML of the whole `.pv-target-search` root (input +
    // filters + status + any rendered options), not the full page — small and
    // directly relevant.
    let outer_html_script = r#"
        var el = document.querySelector("[data-testid='target-search']");
        return el ? el.outerHTML : '<[data-testid="target-search"] not found in DOM>';
    "#;
    match app.driver.execute(outer_html_script, vec![]).await {
        Ok(ret) => match ret.convert::<String>() {
            Ok(html) => report
                .push_str(&format!("--- [data-testid='target-search'] outerHTML ---\n{html}\n")),
            Err(e) => report.push_str(&format!("(failed to deserialise outerHTML: {e})\n")),
        },
        Err(e) => report.push_str(&format!("(outerHTML script execution failed: {e})\n")),
    }

    // (b) Explicit state check: is a real `role="alert"` field error visible
    // (Phase 1 threw, `TargetSearch.tsx`'s catch branch), or is the
    // `--resolving` status still showing (stuck on Phase 2 / SIMBAD)?
    // Diagnostic-only, but a failed `.text()` read is reported AS a failed
    // read rather than defaulted to "" (#1111) — otherwise a stale handle
    // renders as `text=""`, which is indistinguishable from a real element
    // whose text is genuinely empty and would misdirect the next investigation.
    for (label, css) in [
        ("error state", "[data-testid='field-error']"),
        ("resolving (SIMBAD phase-2) state", "[data-testid='target-search-status-resolving']"),
        ("generic status line", "[data-testid='target-search-status']"),
    ] {
        match app.driver.find(By::Css(css)).await {
            Ok(el) => {
                let text = el
                    .text()
                    .await
                    .map_or_else(|e| format!("<text read failed: {e}>"), |t| format!("{t:?}"));
                report.push_str(&format!("--- {label}: PRESENT, text={text} ---\n"));
            }
            Err(_) => report.push_str(&format!("--- {label}: absent ---\n")),
        }
    }

    // (c) Best-effort screenshot — written to a fixed, predictable path per
    // query so the workflow's failure-only `upload-artifact` step can pick up
    // `target/e2e-diagnostics/*.png` regardless of which test/OS failed.
    let dir = std::path::Path::new("target/e2e-diagnostics");
    if std::fs::create_dir_all(dir).is_ok() {
        let safe_query: String =
            query.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let path = dir.join(format!("target-search-timeout-{safe_query}.png"));
        match app.driver.screenshot(&path).await {
            Ok(()) => report.push_str(&format!("--- screenshot written to {path:?} ---\n")),
            Err(e) => report.push_str(&format!("(screenshot capture failed: {e})\n")),
        }
    } else {
        report.push_str("(failed to create target/e2e-diagnostics/ directory)\n");
    }

    // (d) Round 2 (keepMounted alone did NOT fix this — still fails
    // identically cross-platform, deterministically): bypass the UI and
    // invoke the REAL backend `target_search` command directly through the
    // `window.__PV_E2E__.invoke` bridge, mirroring `inbox_ui_journeys.rs`'s
    // direct-invoke diagnostic. This tells us whether the backend genuinely
    // returns hits for this exact query independent of the frontend
    // combobox/portal — local Chromium+mocks repro (both dev and prod
    // builds) proves the combobox mounts and renders options correctly for
    // typed queries, so if THIS call also returns real hits, the bug is
    // somewhere between the UI's `runSearch` and the DOM (state never
    // reaching `open`/`suggestions`); if it errors or never returns, the bug
    // is in the real IPC round-trip or the backend itself.
    let direct_invoke: serde_json::Value = match app
        .invoke(
            "target_search",
            json!({
                "req": {
                    "contractVersion": "1.0",
                    "requestId": "diag-direct-search",
                    "query": query,
                    "catalogFilter": [],
                    "typeFilter": [],
                    "limit": 20
                }
            }),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => json!({ "direct_invoke_error": e.to_string() }),
    };
    report.push_str(&format!(
        "--- direct backend invoke of target_search({query:?}) ---\n{direct_invoke:#}\n"
    ));

    // (e) General UI-state diagnostics (bridge exposed?, build time, buffered
    // window errors/rejections) — the #477-era instrumentation
    // (`E2eApp::dump_ui_diagnostics`) shared with `inbox_ui_journeys.rs`.
    // `queryState` will be null here (TargetSearch doesn't use react-query),
    // but `bridgeExposed`/`buildTime`/`e2eErrors` are exactly what
    // distinguishes "stale/mismatched frontend build" from "a real uncaught
    // exception the UI swallowed silently" from "genuinely nothing went
    // wrong at the JS level".
    let ui_diag = app.dump_ui_diagnostics().await;
    report.push_str(&format!("--- dump_ui_diagnostics ---\n{ui_diag:#}\n"));

    // (f) Real browser console log tail (best-effort; some WebDriver stacks
    // reject the log endpoint entirely, per `dump_console_log`'s doc).
    let console_log = app.dump_console_log().await;
    report.push_str(&format!("--- dump_console_log ---\n{console_log:#}\n"));

    // (g) Round 3 correction: (a) above only dumps `.pv-target-search`'s
    // OWN outerHTML, and `Combobox.Portal` renders the suggestion listbox at
    // `document.body` — a SIBLING of that subtree, never a descendant — so
    // "absent from (a)'s dump" was never real evidence the listbox didn't
    // render, only that it isn't nested under the search root (which it
    // never was, portal or no portal). Look for it where it ACTUALLY lives:
    // (i) the real listbox via the input's `aria-controls` id (works
    // wherever it was portaled to), (ii) a page-wide count + first-match
    // outerHTML for the exact selector `add_target_via_ui`'s suggestion-poll
    // loop waits on, and (iii) a page-wide count of `[role="listbox"]` /
    // `[role="option"]` in case the class name itself has drifted (e.g. a
    // CSS consolidation rename) independent of Base UI's own ARIA roles.
    let listbox_script = r#"
        var callback = arguments[arguments.length - 1];
        function truncate(s, n) {
            if (typeof s !== 'string') return s;
            return s.length > n ? s.slice(0, n) + '...[truncated]' : s;
        }
        try {
            var input = document.querySelector("[data-testid='target-search-input']");
            var controlsId = input ? input.getAttribute('aria-controls') : null;
            var listboxEl = controlsId ? document.getElementById(controlsId) : null;
            var optionEls = document.querySelectorAll("[data-testid='target-search-option']");
            var roleListboxEls = document.querySelectorAll('[role="listbox"]');
            var roleOptionEls = document.querySelectorAll('[role="option"]');
            callback({
                ok: true,
                value: {
                    ariaControlsId: controlsId,
                    listboxFoundById: !!listboxEl,
                    listboxOuterHtml: truncate(listboxEl ? listboxEl.outerHTML : null, 4096),
                    optionSelectorCount: optionEls.length,
                    optionSelectorFirstOuterHtml: truncate(optionEls[0] ? optionEls[0].outerHTML : null, 2048),
                    roleListboxCount: roleListboxEls.length,
                    roleOptionCount: roleOptionEls.length
                }
            });
        } catch (err) {
            callback({ ok: false, error: String(err) });
        }
    "#;
    match app.driver.execute_async(listbox_script, vec![]).await {
        Ok(ret) => match ret.convert::<serde_json::Value>() {
            Ok(v) => {
                report.push_str(&format!("--- real-listbox / role-based diagnostics ---\n{v:#}\n"))
            }
            Err(e) => {
                report.push_str(&format!("(failed to deserialise listbox diagnostics: {e})\n"))
            }
        },
        Err(e) => report.push_str(&format!("(listbox diagnostics script execution failed: {e})\n")),
    }

    report
}

/// Poll [`E2eApp::count_elements_with_title`] until it reports at least one
/// match or `UI_TIMEOUT` elapses.
///
/// `count_elements_with_title`'s own doc explicitly does NOT poll — a zero
/// count is frequently the CORRECT, expected result for its other callers
/// (asserting an absence), so it hands the "wait for a nonzero count"
/// responsibility to callers that need it. `targets_ui_astronomy_columns_
/// disclose_placeholder_without_site` was calling it exactly once, right
/// after `add_target_via_ui` returns — a real IPC round trip (the target
/// list refetch that must happen before the new row's astronomy cells exist)
/// racing a single synchronous check, with no retry. This is the fix: poll
/// like every other "wait for a real UI state" helper in this harness
/// (`find_waiting`, the suggestion-poll loop in `add_target_via_ui`).
async fn wait_for_title_count_at_least(
    app: &E2eApp,
    title: &str,
    min_count: usize,
) -> anyhow::Result<usize> {
    let deadline = tokio::time::Instant::now() + UI_TIMEOUT;
    loop {
        let count = app.count_elements_with_title(title).await?;
        if count >= min_count {
            return Ok(count);
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(count);
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Wait for the Targets list to be ready in the UI: confirms ≥1 target
/// exists via IPC, then drives the TanStack Query to reflect that data in
/// the DOM via a retry-invalidation loop.
///
/// Three-stage design:
/// 1. `invoke_until target_list` (TARGETS_TABLE_TIMEOUT): proves the backend
///    has at least one target. Decoupled from TanStack Query — measures real
///    IPC latency, not the UI's internal query state. Resolves the cold-start
///    timing gap on Windows runners (bead h182).
/// 2. `invalidate_query(["targets"])`: forces TanStack Query to refetch;
///    awaits the refetch completion. This should bring `count ≥ 1` into
///    React state and trigger a render with rows.
/// 3. Retry loop (up to TARGETS_TABLE_TIMEOUT): if rows still absent after
///    the first invalidation, re-invalidates every 10s. Covers:
///    - Race between `invalidateQueries` Promise resolution and React's
///      async commit (rows exist in cache but haven't hit the DOM yet);
///    - `useStaleSelectionCleanup` mid-invalidation navigation that can
///      cause the component to re-enter loading state;
///    - Any in-flight concurrent `load()` refetch that returns before the
///      invalidation and sets `data=[]`, leaving `count=0` until
///      re-invalidated.
async fn wait_targets_in_ipc_then_invalidate(app: &E2eApp) -> anyhow::Result<()> {
    // Stage 1: wait for backend to have data.
    app.invoke_until("target_list", json!({}), TARGETS_TABLE_TIMEOUT, |v: &Value| {
        v.as_array().is_some_and(|a| !a.is_empty())
    })
    .await
    .context(
        "target_list IPC never returned ≥1 target within TARGETS_TABLE_TIMEOUT — \
         the add or the DB write may have silently failed",
    )?;

    // Stage 2+3: invalidate + retry until rows are in the DOM.
    let outer_deadline = tokio::time::Instant::now() + TARGETS_TABLE_TIMEOUT;
    loop {
        // Invalidate TanStack Query and await the refetch (blocks until
        // the query has fresh data or SCRIPT_TIMEOUT elapses).
        app.invalidate_query(r#"["targets"]"#)
            .await
            .context("failed to invalidate targets query")?;

        // Poll DOM for up to 10s after each invalidation.
        let poll_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let mut found = false;
        while tokio::time::Instant::now() < poll_deadline {
            if app.driver.find(By::Css("[data-testid='targets-table-row']")).await.is_ok() {
                found = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if found {
            return Ok(());
        }

        if tokio::time::Instant::now() >= outer_deadline {
            let url = app
                .driver
                .current_url()
                .await
                .map_or_else(|_| "<unknown>".to_owned(), |u| u.to_string());
            anyhow::bail!(
                "no [data-testid='targets-table-row'] appeared within TARGETS_TABLE_TIMEOUT \
                 after repeated invalidate-and-poll cycles; current URL: {url}"
            );
        }
    }
}

/// Diagnostics for a `wait_for_title_count_at_least` timeout: the first real
/// target row's outerHTML (does the row even exist? does it carry the
/// expected muted/unknown cells?) plus a page-wide count for both disclosure
/// titles, so a future red run shows real evidence instead of a bare
/// "expected >= 1, got N".
async fn dump_astronomy_diagnostics(app: &E2eApp) -> String {
    let mut report = String::from("=== astronomy-columns diagnostics ===\n");

    let row_html_script = r#"
        var el = document.querySelector("[data-testid='targets-table-row']");
        return el ? el.outerHTML : '<[data-testid="targets-table-row"] not found in DOM>';
    "#;
    match app.driver.execute(row_html_script, vec![]).await {
        Ok(ret) => match ret.convert::<String>() {
            Ok(html) => report.push_str(&format!(
                "--- first [data-testid='targets-table-row'] outerHTML ---\n{html}\n"
            )),
            Err(e) => report.push_str(&format!("(failed to deserialise row outerHTML: {e})\n")),
        },
        Err(e) => report.push_str(&format!("(row outerHTML script execution failed: {e})\n")),
    }

    match app.count_elements_with_title(OPPOSITION_UNKNOWN_TITLE).await {
        Ok(n) => report.push_str(&format!("--- OPPOSITION_UNKNOWN_TITLE count: {n} ---\n")),
        Err(e) => report.push_str(&format!("(OPPOSITION_UNKNOWN_TITLE count query failed: {e})\n")),
    }
    match app.count_elements_with_title(LUNAR_UNKNOWN_TITLE).await {
        Ok(n) => report.push_str(&format!("--- LUNAR_UNKNOWN_TITLE count: {n} ---\n")),
        Err(e) => report.push_str(&format!("(LUNAR_UNKNOWN_TITLE count query failed: {e})\n")),
    }

    let ui_diag = app.dump_ui_diagnostics().await;
    report.push_str(&format!("--- dump_ui_diagnostics ---\n{ui_diag:#}\n"));

    report
}

/// Add a target via the REAL "Add target" dialog (search -> select suggestion
/// -> confirm), all real DOM interaction. `query` should match the bundled
/// offline seed (e.g. a Messier designation) so resolution never needs a live
/// SIMBAD network call. Returns the resolved target's real id, read from the
/// real `?selected=<id>` URL the app navigates to on success
/// (`TargetsPage.tsx::handleAdded`).
async fn add_target_via_ui(app: &E2eApp, query: &str) -> anyhow::Result<String> {
    // Open: at this point no dialog is open yet, so the top-bar trigger's
    // "Add target" text is unambiguous.
    app.click_button_text("Add target").await?;

    // Poll for the dialog to actually mount: it opens asynchronously after the
    // trigger click, same route/render race `E2eApp::find_waiting` documents.
    let popup = app
        .find_waiting(By::Css("[data-testid='add-target-popup']"), "the Add-target dialog popup")
        .await?;
    let input = popup.find(By::Css("[data-testid='target-search-input']")).await?;

    // #841 (dialog focus race, product fix filed + out of this branch's
    // scope): a keystroke can land on the modal's own close (X) button
    // instead of the search input if the dialog's mount/focus sequencing
    // hasn't settled by the moment we type. Wait for the REAL
    // `document.activeElement` to actually BE this input, then verify the
    // input's live `.value` DOM property (via JS — not `outerHTML`/the
    // `value` ATTRIBUTE, which can lag a React-controlled input's live
    // state) holds what we just typed; retry the whole focus-wait + type
    // once if either check fails, rather than bailing on the first race.
    type_into_search_input(app, &input, query).await?;

    // Poll for a real suggestion option to render (offline seed search).
    //
    // Round 3 root cause (#463): this MUST be `app.driver.find` (page-scoped),
    // NOT `popup.find` (scoped to `.pv-add-target__popup`'s subtree).
    // `TargetSearch.tsx`'s `Combobox.Portal` renders the suggestion listbox at
    // `document.body` — a SIBLING of the dialog popup, never a descendant of
    // it — so `popup.find(By::Css(".pv-target-search__option"))` was
    // structurally unable to ever match, regardless of whether the backend
    // responded or the popup actually rendered. This explained every prior
    // round's "no suggestion rendered" symptom even though round-3
    // diagnostics proved the backend answers instantly/correctly and the UI
    // buffers no errors: the assertion itself was searching the wrong DOM
    // subtree, cross-platform and deterministically. The mock-Playwright spec
    // (`targets_planner.spec.ts` 9.2b) never hit this because Playwright's
    // `page.locator(...)` is page-scoped by default.
    let deadline = tokio::time::Instant::now() + UI_TIMEOUT;
    let option = loop {
        if let Ok(opt) = app.driver.find(By::Css("[data-testid='target-search-option']")).await {
            break opt;
        }
        if tokio::time::Instant::now() >= deadline {
            let diagnostics = dump_target_search_diagnostics(app, query).await;
            anyhow::bail!(
                "no suggestion rendered for query {query:?} within {UI_TIMEOUT:?} \
                 (offline seed search should be instant for a bundled designation)\n{diagnostics}"
            );
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    };
    option.click().await?;

    // Confirm — scoped to the dialog popup so this can't accidentally hit the
    // top-bar trigger button, which shares the exact same "Add target" text.
    popup
        .find(By::XPath(".//button[normalize-space(.)='Add target']"))
        .await
        .context("no scoped 'Add target' confirm button inside the dialog popup")?
        .click()
        .await
        .context("click the dialog's Add target confirm button failed")?;

    let url = app
        .wait_url_contains("selected=", UI_TIMEOUT)
        .await
        .context("expected the app to navigate to ?selected=<targetId> after a successful add")?;
    let id = url
        .split("selected=")
        .nth(1)
        .ok_or_else(|| {
            anyhow::anyhow!("URL contained 'selected=' but split produced no id: {url}")
        })?
        .split('&')
        .next()
        .unwrap_or_default()
        .to_owned();
    anyhow::ensure!(!id.is_empty(), "extracted an empty target id from URL: {url}");
    Ok(id)
}

/// Wait for `input` to actually hold `document.activeElement` focus, type
/// `query` into it, then verify the input's live `.value` DOM property
/// equals `query` — retrying the whole focus-wait + type sequence once if
/// either check fails (#841: a keystroke can otherwise land on the dialog's
/// own close button during its mount/focus race, rather than the search
/// input `add_target_via_ui` just found).
async fn type_into_search_input(
    app: &E2eApp,
    input: &WebElement,
    query: &str,
) -> anyhow::Result<()> {
    const FOCUS_TIMEOUT: Duration = Duration::from_secs(5);

    for attempt in 0..2 {
        let deadline = tokio::time::Instant::now() + FOCUS_TIMEOUT;
        loop {
            let is_focused: bool = app
                .driver
                .execute("return arguments[0] === document.activeElement;", vec![input.to_json()?])
                .await
                .context("document.activeElement check failed")?
                .convert()
                .context("failed to deserialize the document.activeElement check result")?;
            if is_focused {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "search input never gained document.activeElement focus within \
                     {FOCUS_TIMEOUT:?} (attempt {attempt})"
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        input.send_keys(query).await?;

        let live_value: String = app
            .driver
            .execute("return arguments[0].value;", vec![input.to_json()?])
            .await
            .context("reading the search input's live .value failed")?
            .convert()
            .context("failed to deserialize the search input's live .value")?;
        if live_value == query {
            return Ok(());
        }
        // Landed on the wrong element or got garbled — clear whatever's
        // there (established pattern: `E2eApp::fill_testid`) and retry once.
        input.clear().await.ok();
    }

    anyhow::bail!(
        "search input's live .value never matched the typed query {query:?} after 2 attempts \
         (#841 dialog focus race)"
    );
}

/// Test 2 (journey-09): adding the same bundled-seed target twice via the
/// real UI resolves to the SAME real target id both times — re-adding never
/// creates a duplicate catalog row.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn targets_ui_add_target_no_duplicate_on_reconfirm() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;

    app.goto_route("/targets").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;

    let first_id = add_target_via_ui(&app, "M 1").await?;
    // Re-add the exact same designation.
    let second_id = add_target_via_ui(&app, "M 1").await?;

    anyhow::ensure!(
        first_id == second_id,
        "expected re-adding the same target to resolve to the SAME real id \
         (no duplicate), got {first_id:?} then {second_id:?}"
    );

    app.shutdown().await
}

/// Test 6a/6b (journey-09), the safety-critical stub-disclosure guard: with
/// NO observing site configured, the top bar shows the real "set up your
/// observing site" prompt (never a fabricated Moon summary), and per-target
/// Opposition/Lunar-separation cells show an explicit, real "unknown"
/// disclosure ("—" with a real title) rather than a fabricated-looking
/// number (`deriveRowMoonPlanning`'s real `!night` branch,
/// `apps/desktop/src/features/targets/astro/row-planning.ts`).
///
/// Post-fix measured time (runs 30020688075, 30019274329): 7.0s avg — below
/// the `slow_` threshold (>3× median ≈ 21s post-fix). Not prefixed `slow_`.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn targets_ui_astronomy_columns_disclose_placeholder_without_site() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;

    app.goto_route("/targets").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    // Guarantee at least one real target row exists regardless of whether the
    // bundled seed catalog is pre-listed on a fresh DB.
    add_target_via_ui(&app, "M 1").await?;

    // Wait for the table to actually render a row before asserting on
    // row-level cell content. On a Windows cold boot the seed load can take
    // 30-60s, and without this gate the 20s title-element poll below fires
    // against an empty table — producing the TRY-1-only failures tracked by
    // bead astro-plan-h182.
    wait_targets_in_ipc_then_invalidate(&app).await?;

    app.wait_testid("planner-site-prompt", UI_TIMEOUT).await.map_err(|e| {
        anyhow::anyhow!("expected the real site-setup prompt with no observing site: {e}")
    })?;
    anyhow::ensure!(
        !app.testid_exists("moon-summary").await?,
        "no observing site exists — a real MoonSummary must not render"
    );

    // Poll rather than a single check: the target row `add_target_via_ui` just
    // added still needs the real list-query refetch to land before its
    // astronomy cells (and therefore this title) exist in the DOM.
    let opposition_count = wait_for_title_count_at_least(&app, OPPOSITION_UNKNOWN_TITLE, 1).await?;
    if opposition_count < 1 {
        let diagnostics = dump_astronomy_diagnostics(&app).await;
        anyhow::bail!(
            "expected at least one real 'Opposition unknown' disclosure with no observing \
             site within {UI_TIMEOUT:?}, got {opposition_count}\n{diagnostics}"
        );
    }
    let lunar_count = wait_for_title_count_at_least(&app, LUNAR_UNKNOWN_TITLE, 1).await?;
    if lunar_count < 1 {
        let diagnostics = dump_astronomy_diagnostics(&app).await;
        anyhow::bail!(
            "expected at least one real 'Lunar distance unknown' disclosure with no observing \
             site within {UI_TIMEOUT:?}, got {lunar_count}\n{diagnostics}"
        );
    }

    app.shutdown().await
}

/// Test 6c (journey-09) / batch #8: creating a real observing site via the
/// real Settings → Target Planner → Observing Sites UI flips the real
/// `useObserverSiteExists()` gate, and the Targets page immediately (no
/// reload) swaps the site-setup prompt for a real `MoonSummary` — proving
/// the planner astronomy pipeline is really wired end-to-end from a real
/// user action, not just a settings round-trip.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn targets_planner_real_astronomy_after_site_creation() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;

    app.goto_route("/targets").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.wait_testid("planner-site-prompt", UI_TIMEOUT).await.map_err(|e| {
        anyhow::anyhow!(
            "precondition failed: expected the site-setup prompt before any site exists: {e}"
        )
    })?;

    // Real UI: Settings -> Target Planner -> Observing Sites -> Add site.
    // The active pane is a real PATH param (`settingsPaneRoute`, path
    // `/settings/$pane` — `apps/desktop/src/app/router.tsx`), read via
    // `useParams` in `SettingsPage.tsx`, NOT a `?pane=` query string; the
    // query-string form silently falls back to the default 'sources' pane
    // (CI: "+ Add site" never appeared — the Planner pane never mounted).
    app.goto_route("/settings/planner").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.click_button_text("+ Add site").await?;

    // Poll for the form to actually mount: it opens asynchronously after the
    // trigger click, same route/render race `E2eApp::find_waiting` documents.
    let name_input =
        app.find_waiting(By::Id("observing-site-name"), "the observing-site name input").await?;
    name_input.send_keys("E2E Test Observatory").await?;
    let lat_input = app.driver.find(By::Id("observing-site-lat")).await?;
    lat_input.send_keys("51.4778").await?; // Royal Observatory Greenwich latitude
    let lon_input = app.driver.find(By::Id("observing-site-lon")).await?;
    lon_input.send_keys("-0.0015").await?;
    let elevation_input = app.driver.find(By::Id("observing-site-elevation")).await?;
    elevation_input.send_keys("45").await?;
    // Timezone/twilight/horizon keep their real defaults (local IANA zone,
    // astronomical twilight, 0° horizon) — only the load-bearing fields for
    // ephemeris computation are set explicitly.

    app.click_button_text("Save").await?;

    // Real, durable proof the site persisted: `settings.get('observing')`
    // now carries a non-empty `observingSites` array.
    let settings: serde_json::Value = app
        .invoke_until(
            "settings_get",
            json!({ "scope": "observing" }),
            UI_TIMEOUT,
            |v: &serde_json::Value| {
                v["values"]["observingSites"].as_array().is_some_and(|a| !a.is_empty())
            },
        )
        .await
        .context("expected the new observing site to persist to real settings storage")?;
    anyhow::ensure!(
        settings["values"]["observingActiveSiteId"].is_string(),
        "expected the first-ever site to auto-become active: {settings}"
    );

    // Real UI: back to Targets — the gate must flip WITHOUT a reload, since
    // every consumer subscribes to the live site-store via useSyncExternalStore.
    app.goto_route("/targets").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.wait_testid("moon-summary", UI_TIMEOUT).await.map_err(|e| {
        anyhow::anyhow!(
            "expected a real MoonSummary to render now that an observing site exists: {e}"
        )
    })?;
    anyhow::ensure!(
        !app.testid_exists("planner-site-prompt").await?,
        "expected the site-setup prompt to disappear now that a site exists"
    );

    // FR-009 amendment (iteration 2026-07-17): the detail pane's "Best date"
    // stat is Moon-aware and self-explaining. Open a real target's detail and
    // assert the stat's explanation via its aria-label mirror (the same text
    // the hover tooltip shows) — the popup itself is NOT hover-opened here:
    // real-pointer hover is not a primitive this bridge harness exercises
    // reliably, and the aria-label mirror is the accessible contract.
    add_target_via_ui(&app, "M 1").await?;
    let row = app
        .find_waiting(By::Css("[data-testid='targets-table-row']"), "a targets-table row to open")
        .await?;
    row.click().await?;
    let best_date =
        app.wait_testid("proptable-tooltip-bestdate", UI_TIMEOUT).await.map_err(|e| {
            anyhow::anyhow!("expected the detail Best-date stat with its Moon explanation: {e}")
        })?;
    // A MISSING aria-label and an EMPTY one are different regressions; don't
    // let `unwrap_or_default()` collapse them into the same `got ""` message.
    let label = best_date
        .attr("aria-label")
        .await
        .context("reading the Best-date stat's aria-label failed")?
        .context("the Best-date stat has no aria-label attribute at all")?;
    anyhow::ensure!(
        label.contains("Matches opposition")
            || label.contains("falls near full Moon")
            || label.contains("No Moon-favourable night"),
        "expected the Best-date aria-label to carry one of the three real Moon-state \
         explanations (coincides / diverged / none found), got {label:?}"
    );

    app.shutdown().await
}

/// Measure, relative to the scroll container's own left edge, where the
/// pinned cells and a non-pinned control cell actually render.
///
/// Positions are taken from `getBoundingClientRect` — real post-layout
/// geometry — because that is the only thing that can observe `position:
/// sticky`. This measurement is the entire reason T026 must be a Layer-2
/// journey: jsdom has no layout engine, so a Layer-1 test reports 0 for
/// every one of these and would pass against a completely unpinned table.
async fn measure_pinned_columns(app: &E2eApp) -> anyhow::Result<serde_json::Value> {
    let script = r#"
        var sc = document.querySelector("[data-testid='targets-table-scroll']");
        if (!sc) { return { error: 'no [data-testid="targets-table-scroll"] in the DOM' }; }
        var row = sc.querySelector("[data-testid='targets-table-row']");
        if (!row) { return { error: 'no [data-testid="targets-table-row"] rendered' }; }
        var headRow = sc.querySelector('thead tr');
        var scLeft = sc.getBoundingClientRect().left;
        function offset(el) {
            return el ? Math.round(el.getBoundingClientRect().left - scLeft) : null;
        }
        var cells = row.children;
        return {
            scrollWidth: Math.round(sc.scrollWidth),
            clientWidth: Math.round(sc.clientWidth),
            scrollLeft: Math.round(sc.scrollLeft),
            cellCount: cells.length,
            star: offset(cells[0]),
            designation: offset(cells[1]),
            designationHeader: offset(headRow ? headRow.children[1] : null),
            control: offset(cells[cells.length - 1])
        };
    "#;
    let v: serde_json::Value = app
        .driver
        .execute(script, vec![])
        .await
        .context("failed to measure the pinned columns")?
        .convert()
        .context("pinned-column measurement was not JSON")?;
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        anyhow::bail!("cannot measure pinned columns: {err}");
    }
    Ok(v)
}

fn px(m: &serde_json::Value, key: &str) -> anyhow::Result<i64> {
    m.get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("measurement {key:?} missing or not a number in {m}"))
}

/// T026 (spec 054, #1257) — the star and designation columns stay put while
/// the rest of the Targets table scrolls sideways, so a row's identity is
/// never lost (FR-006/FR-007, shipped in #1253).
///
/// **Pins the dock rather than relying on the 1400px width threshold**, so
/// this runs on both supported platforms. GitHub-hosted Windows runners are
/// fixed at 1024x768 and cannot be resized — the runner service runs in
/// non-interactive Session 0 with no real display (actions/runner-images
/// #2935, #8606) — so a journey demanding a 1400px viewport would be
/// Linux-only by construction. A user pin is a first-class supported
/// configuration, not a test-only shortcut: `useAdaptiveDock` honours an
/// override at any width wide enough for a side dock at all.
///
/// Pinning also sidesteps a counter-intuitive trap. A LARGER screen produces
/// LESS horizontal overflow, because the table's 1000px min-width floor is
/// fixed while the space left for it grows: at a 1400px viewport the table
/// gets ~760px and overflows by ~240px, but at 1600px it gets ~960px and
/// overflows by only ~40px. Asserting against a fixed viewport would be
/// fragile in the direction people intuitively assume is safer. So the
/// assertion is on MEASURED overflow — ~616px at the Windows runners'
/// 1024px, ~240px at 1400px.
///
/// Asserts a non-pinned CONTROL column moves by the scroll distance. Without
/// it this journey would pass in the one case it most needs to catch: if the
/// table never actually scrolled, every "drift == 0" assertion below would
/// hold trivially against a static table and the test would be vacuous.
///
/// Measured on Windows CI: 8-11s (warm), stable across runs. Not prefixed
/// `slow_` — see `slow_targets_ui_dock_pin_and_width_survive_a_real_restart`
/// for the convention and timing threshold.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn targets_ui_identity_columns_stay_pinned_while_table_scrolls() -> anyhow::Result<()> {
    const MIN_SCROLL: i64 = 200;

    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;

    // Best-effort: widens the window where the display allows (Ubuntu, whose
    // xvfb screen e2e.yml sizes to 1600x1200) and is a no-op on a Windows
    // runner that cannot resize. Nothing below depends on the result — the
    // dock is pinned explicitly, and the real gate is measured overflow.
    let (viewport_w, viewport_h) = app.set_viewport(1400, 900).await?;

    app.goto_route("/targets").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    let target_id = add_target_via_ui(&app, "M 1").await?;

    // Pin the side dock through the app's REAL persisted preference, then
    // let the reload apply it. The reload is what makes this work at all:
    // `data/preferences.ts` memoises into a module-level cache on first
    // read, so seeding localStorage into a booted page would be inert.
    app.seed_preference("detailDock", r#"{"targets":{"placement":"side","width":420}}"#).await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    // Re-establish the target viewport after the preference-seed page reload.
    // `seed_preference` calls `driver.refresh()` which keeps the OS window
    // size, but on some CI runs the WebView's reported `innerWidth` drifts back
    // to the screen default (1600 on the xvfb screen). Calling `set_viewport`
    // again here ensures the layout that mounts on the next navigation sees the
    // correct 1400 px width when `useAdaptiveDock` initialises from storage.
    let _ = app.set_viewport(1400, 900).await;

    // Return to the SELECTED target, not bare `/targets`. The side dock only
    // takes width when there is a detail to show — `ListPageLayout` mounts the
    // panel solely when its `detail` prop is non-null. Navigating to the bare
    // route drops the `?selected=` that `add_target_via_ui` landed on, which
    // leaves the pinned preference correctly loaded but with nothing to
    // render, so the table keeps full width and barely overflows at all.
    app.goto_route(&format!("/targets?selected={target_id}")).await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    // Wait for the table to populate before asserting on scroll geometry. On
    // Windows cold boots the seed load can take 30-60s, and
    // DEFAULT_FIND_TIMEOUT (20s) is insufficient (bead astro-plan-h182).
    wait_targets_in_ipc_then_invalidate(&app).await?;
    app.find_waiting(
        By::Css("[data-testid='targets-table-scroll']"),
        "the loaded Targets table scroll container",
    )
    .await?;

    // Gate: wait for the side dock to settle to its seeded 420px width AND for
    // the table to have sufficient horizontal overflow before asserting on column
    // positions. Two failure modes are observed on CI:
    //
    // (a) Dock-width transient (the root cause): `useAdaptiveDock` reads
    //     width=420 from preferences synchronously on first render, setting the
    //     `--pv-side-detail-w` CSS variable immediately. However WebKit's flex
    //     layout can take several seconds to commit the CSS-variable reflow after
    //     a SPA navigation — `rendered_side_width` reads back ~300px while the
    //     reflow is in flight. Polling dock width directly is the settle signal;
    //     calling `set_viewport` nudges the layout engine to recompute.
    //
    // (b) Viewport drift: `seed_preference` calls `driver.refresh()`, which can
    //     snap the WebView back to the xvfb screen default (1600px). At 1600px
    //     the dock IS 420px (correctly read from preferences) but the table gets
    //     ~960px and overflows by only ~40px — below MIN_SCROLL. `set_viewport`
    //     corrects the window size, which also triggers the layout reflow from (a).
    //
    // The gate checks BOTH conditions: dock_w >= SEEDED_WIDTH-tolerance (primary:
    // guards (a)) and overflow >= MIN_SCROLL (secondary: guards (b)). This bails
    // with a specific diagnostic on timeout instead of silently falling through
    // to an assert — distinguishing "dock never settled" from "viewport drifted".
    {
        const SEEDED_WIDTH: i64 = 420;
        const SETTLE_TOLERANCE: i64 = 20;
        const DOCK_SETTLE_TIMEOUT: Duration = Duration::from_secs(15);
        let deadline = tokio::time::Instant::now() + DOCK_SETTLE_TIMEOUT;
        loop {
            let dock_w = rendered_side_width(&app).await?;
            let cols = measure_pinned_columns(&app).await?;
            let overflow = px(&cols, "scrollWidth")? - px(&cols, "clientWidth")?;
            if dock_w >= SEEDED_WIDTH - SETTLE_TOLERANCE && overflow >= MIN_SCROLL {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "dock did not settle within {DOCK_SETTLE_TIMEOUT:?}: \
                     dock_w={dock_w}px (need >= {}), overflow={overflow}px \
                     (need >= {MIN_SCROLL}). \
                     Failure mode (a) if dock_w is low: WebKit has not yet \
                     committed the CSS-variable flex reflow after SPA navigation \
                     (bead astro-plan-l3rw). \
                     Failure mode (b) if dock_w is correct but overflow is low: \
                     seed_preference refresh() snapped innerWidth back to the xvfb \
                     screen default. viewport={viewport_w}x{viewport_h}. cols={cols}",
                    SEEDED_WIDTH - SETTLE_TOLERANCE
                );
            }
            let _ = app.set_viewport(1400, 900).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    let before = measure_pinned_columns(&app).await?;
    let overflow = px(&before, "scrollWidth")? - px(&before, "clientWidth")?;
    anyhow::ensure!(
        overflow >= MIN_SCROLL,
        "the table must really overflow horizontally for this journey to mean \
         anything, but scrollWidth-clientWidth is only {overflow}px (need \
         >= {MIN_SCROLL}) at a {viewport_w}x{viewport_h} viewport. Likely \
         causes, in the order they have actually bitten: (1) the side dock \
         mounted nothing because no target is selected — the panel needs a \
         non-null `detail`, so the route must keep `?selected=<id>`; (2) the \
         seeded `detailDock` preference did not survive the reload past the \
         module-level cache in `data/preferences.ts`; (3) the table's 1000px \
         min-width floor changed. A reported 0x0 viewport with a huge \
         clientWidth means `set_viewport` failed to converge. Note a WIDER \
         viewport yields LESS overflow, so a large screen is not the safe \
         direction here: {before}"
    );

    // Scroll to the far right — the worst case for identity loss.
    app.driver
        .execute(
            "var sc = document.querySelector(\"[data-testid='targets-table-scroll']\");\
             sc.scrollLeft = sc.scrollWidth; return sc.scrollLeft;",
            vec![],
        )
        .await
        .context("failed to scroll the targets table horizontally")?;

    let after = measure_pinned_columns(&app).await?;
    let scrolled = px(&after, "scrollLeft")?;
    anyhow::ensure!(
        scrolled >= MIN_SCROLL,
        "expected the table to really scroll by >= {MIN_SCROLL}px, got {scrolled}px: {after}"
    );

    // The control proves the scroll actually moved content.
    let control_drift = px(&after, "control")? - px(&before, "control")?;
    anyhow::ensure!(
        control_drift <= -MIN_SCROLL,
        "the non-pinned control column should have moved left by ~{scrolled}px, but it \
         shifted {control_drift}px — the table did not really scroll, so the pinned \
         assertions below would be vacuous.\nbefore: {before}\nafter:  {after}"
    );

    // The point of the feature: identity does not move, at all.
    for key in ["star", "designation", "designationHeader"] {
        let drift = px(&after, key)? - px(&before, key)?;
        anyhow::ensure!(
            drift == 0,
            "{key} must stay pinned while the table scrolls {scrolled}px, but it drifted \
             {drift}px. A non-zero drift here is exactly the regression this journey \
             exists to catch (an approximate sticky offset, or a percentage offset \
             resolving against the scroll container instead of the table).\n\
             before: {before}\nafter:  {after}"
        );
    }

    app.shutdown().await
}

/// Reads the app's REAL persisted `detailDock` entry for one dock, straight
/// out of `localStorage` — the bytes, not a React value. Returns
/// `(placement, width)`.
async fn read_dock_pref(app: &E2eApp, dock_id: &str) -> anyhow::Result<(String, i64)> {
    let script = format!(
        "var raw = localStorage.getItem('alm-preferences');\
         if (!raw) {{ return null; }}\
         var dock = (JSON.parse(raw).detailDock || {{}})['{dock_id}'];\
         return dock ? [String(dock.placement), dock.width] : null;"
    );
    let v: Value = app
        .driver
        .execute(&script, vec![])
        .await
        .context("failed to read the persisted detailDock preference")?
        .convert()
        .context("the detailDock preference did not deserialise")?;
    anyhow::ensure!(!v.is_null(), "no persisted detailDock entry for {dock_id:?} at all");
    let placement =
        v.get(0).and_then(Value::as_str).context("persisted placement was not a string")?;
    // Deliberately NOT `Number(dock.width)` on the JS side and NOT a lossy
    // fallback here: `width: null` is the legitimate "never resized"
    // sentinel (`DetailDockPref.width: number | null`), and `Number(null)`
    // silently coercing to `0` once let a drag that never reached the
    // resize handler read back as "resized to 0px" instead of surfacing the
    // real problem. A null width is a caller bug (this helper is only used
    // once the test has forced a resize) that must fail loudly, not compare
    // equal to some later default-width fallback.
    let width = v.get(1).and_then(Value::as_i64).with_context(|| {
        format!("persisted width for {dock_id:?} was null or not a number: {v:?}")
    })?;
    Ok((placement.to_string(), width))
}

/// The rendered width of the side dock, as the browser actually lays it out.
async fn rendered_side_width(app: &E2eApp) -> anyhow::Result<i64> {
    let v: Value = app
        .driver
        .execute(
            "var el = document.querySelector(\"[data-testid='listpage-detail']\");\
             return el ? Math.round(el.getBoundingClientRect().width) : -1;",
            vec![],
        )
        .await
        .context("failed to measure the side dock")?
        .convert()
        .context("side dock width did not deserialise")?;
    v.as_i64().context("side dock width was not a number")
}

/// Drives the Targets page to a pinned side dock holding a NON-DEFAULT width,
/// using only real UI: the three-state placement control, then a drag of the
/// resize handle that reaches the actual `onResizeStart` pointer-event
/// handler. Returns the persisted `(placement, width)`.
///
/// The drag is a JS-dispatched `PointerEvent` sequence, NOT
/// `action_chain()`. `tauri-plugin-webdriver` 0.2.1's Actions API cannot
/// drive this interaction at all, on two independent counts (verified by
/// reading its vendored source and by running this test locally with each
/// workaround attempted in turn — both left the CSS `--pv-side-detail-w` var
/// and the persisted width unchanged at their defaults):
/// - `.../src/server/handlers/actions.rs`'s `PointerAction::PointerMove` has
///   no `origin` field, so the W3C-spec `origin: "pointer"` /
///   `origin: WebElement` that `move_by_offset`/`move_to_element_center`
///   produce is silently dropped; every move is executed as if it were
///   `origin: "viewport"`, landing the pointer far from the handle even when
///   using `move_to` with the handle's own on-screen coordinates.
/// - `.../src/platform/executor.rs`'s `dispatch_pointer_event` synthesizes a
///   `MouseEvent` (`mousedown`/`mousemove`/`mouseup`) via
///   `element.dispatchEvent()`, never a `PointerEvent`. Browsers do not
///   synthesize Pointer Events from a script-dispatched, untrusted
///   MouseEvent, so `ResizeHandle`'s `onPointerDown` and
///   `useAdaptiveDock.onResizeStart`'s `window.addEventListener('pointermove'
///   | 'pointerup', ...)` (`apps/desktop/src/ui/useAdaptiveDock.ts`) never
///   fire — regardless of coordinates.
///
/// Dispatching real `PointerEvent`s ourselves reaches the same handler,
/// exercises the same `setWidth` -> `writeStored` -> `localStorage.setItem`
/// path a genuine OS drag would, and differs from a native drag only in how
/// the pointer sequence is injected — which this WebDriver plugin version
/// cannot do for Pointer Events at all.
async fn pin_and_widen_dock(app: &E2eApp) -> anyhow::Result<(String, i64)> {
    // Option order in `DetailDockPlacementControl` is Auto, Bottom, Right.
    let side = app
        .find_waiting(
            By::Css("[data-testid='dock-placement-control'] button[role='radio']:nth-of-type(3)"),
            "the 'Right' option of the dock placement control",
        )
        .await?;
    side.click().await.context("clicking the 'Right' dock placement option failed")?;

    let handle = app
        .find_waiting(By::Css("[data-testid='dock-resize-handle']"), "the dock resize handle")
        .await?;
    let (center_x, center_y) = handle
        .rect()
        .await
        .context("reading the resize handle's screen position failed")?
        .icenter();
    // The side panel sits on the right edge, so dragging LEFT grows it.
    let end_x = center_x - DRAG_PX;

    let script = format!(
        "var handle = document.querySelector(\"[data-testid='dock-resize-handle']\");\
         if (!handle) return false;\
         function fire(target, type, x, buttons) {{\
           target.dispatchEvent(new PointerEvent(type, {{\
             bubbles: true, cancelable: true, pointerId: 1, isPrimary: true,\
             pointerType: 'mouse', button: 0, buttons: buttons,\
             clientX: x, clientY: {center_y}\
           }}));\
         }}\
         fire(handle, 'pointerdown', {center_x}, 1);\
         fire(window, 'pointermove', {end_x}, 1);\
         fire(window, 'pointerup', {end_x}, 0);\
         return true;"
    );
    let found: bool = app
        .driver
        .execute(&script, vec![])
        .await
        .context("dispatching the synthetic resize-handle drag failed")?
        .convert()
        .context("the drag dispatch result did not deserialise")?;
    anyhow::ensure!(
        found,
        "the dock resize handle disappeared before the drag could be dispatched"
    );

    read_dock_pref(app, "targets").await
}

/// How far to drag the resize handle. Must land the width clear of the 420px
/// default: restoring a value that happens to equal the default would pass
/// against a restore that never happened.
const DRAG_PX: i64 = 140;

/// T023 (spec 054) — the reload half, and the last open task in that spec.
///
/// The dock's pin + width are covered across a REMOUNT at Layer 1 (#1195,
/// #1265). A remount proves less than it looks: `getPreferences()` hands back
/// a module-level `cachedPreferences` whenever one exists, so a remount
/// re-reads the CACHE and never touches storage. Those tests stayed green even
/// with `setItem` stubbed out entirely.
///
/// Only a real restart drops that module cache and forces a cold read back
/// from real storage, and jsdom cannot do it — hence Layer 2. `relaunch()`
/// preserves webview storage for exactly this; `graceful_shutdown()` is what
/// makes it meaningful on Windows, where WebView2 flushes its LevelDB store on
/// a clean window close but NOT on a forced kill.
///
/// Note `relaunch()` resets the SQLite DB, so the target added before the
/// restart is gone afterwards and a fresh one is added to give the dock
/// something to render. That is fine here: the dock preference lives in
/// `localStorage`, which is the thing under test.
///
/// Prefixed `slow_`: two full app launches plus a graceful-shutdown +
/// WebView2 storage-flush cycle, consistently 75-88s on Windows CI
/// (5 measured runs: 74.2, 75.0, 75.9, 78.2, 88.0s). The `e2e.yml`
/// shard filters pin each `slow_` test to a separate shard; see the
/// comment block in that file for the convention and rebalance procedure.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn slow_targets_ui_dock_pin_and_width_survive_a_real_restart() -> anyhow::Result<()> {
    const DEFAULT_WIDTH: i64 = 420;

    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;
    app.set_viewport(1400, 900).await?;

    app.goto_route("/targets").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    add_target_via_ui(&app, "M 1").await?;

    let (placement, width) = pin_and_widen_dock(&app).await?;
    anyhow::ensure!(
        placement == "side",
        "the 'Right' placement option should have persisted placement=side, got {placement:?}"
    );
    anyhow::ensure!(
        width != DEFAULT_WIDTH,
        "the drag must move the width OFF its {DEFAULT_WIDTH}px default, otherwise a restore \
         that never happened would still satisfy this journey — got {width}px. Either the \
         drag did not reach the handler, or clamping pinned it back to the default."
    );

    // Clean window close: WebView2 flushes localStorage here and not on a kill.
    app.graceful_shutdown().await?;

    #[cfg(target_os = "windows")]
    {
        // Wait for the WebView2 LevelDB store to both appear AND stabilise
        // (size unchanged across 3 × 200 ms polls). The stability check
        // lives inside wait_for_webview_storage_flush so this single call
        // replaces the previous pattern of flush-wait + fixed 2 s sleep.
        E2eApp::wait_for_webview_storage_flush().await?;
    }

    // Cold start: new process, empty module cache, storage read from disk.
    let app = E2eApp::relaunch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;
    app.set_viewport(1400, 900).await?;

    let (restored_placement, restored_width) = read_dock_pref(&app, "targets").await?;
    anyhow::ensure!(
        restored_placement == placement && restored_width == width,
        "the dock preference must survive a real restart exactly, but {placement}/{width}px \
         came back as {restored_placement}/{restored_width}px. This is the assertion a \
         remount cannot make: it would read the module-level cache instead of storage."
    );

    // Storage surviving is only half of it — the app must also READ it back on
    // a cold boot and lay the dock out at that width.
    app.goto_route("/targets").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    add_target_via_ui(&app, "M 1").await?;

    let laid_out = rendered_side_width(&app).await?;
    anyhow::ensure!(
        (laid_out - restored_width).abs() <= 2,
        "after a real restart the side dock should render at its restored {restored_width}px \
         (±2px for borders), but measured {laid_out}px. A -1 here means no side dock rendered \
         at all, so the restored 'side' pin never reached the layout."
    );

    app.shutdown().await
}
