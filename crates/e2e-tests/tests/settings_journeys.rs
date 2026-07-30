// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journeys — Settings auto-save + theme persistence
//! (batch #11 of the coverage-matrix "Batched plan", Journey 10). Promotes
//! two of `docs/development/windows-journeys/journey-10-settings-appearance-i18n.md`'s
//! Tests: #1 (pane grouping / no global Save) + #4 (a setting persists
//! through the real backend) combined into one journey, and #2 (theme
//! applies live and survives a restart) as a second.
//!
//! Deliberately scoped to these two: they're real, cheap, deterministic UI
//! state/persistence checks with no native OS dialogs involved — the
//! "lowest filesystem-mutation risk" batch per the coverage matrix. The
//! remaining Tests (ingestion-settings-persist-across-restart, altitude
//! clamp, log-panel layout/export, 1100x720 layout convention, translated
//! backend-error surfacing, command palette, sidebar persistence) are left
//! as documented follow-ups — several need either a scrollable content
//! fixture or a real backend-rejected action reachable without a native
//! dialog, which would meaningfully grow this file's scope.
//!
//! Note on cross-session persistence and `E2eApp::launch()`/`relaunch()`:
//! both reset the SQLite DB on every call (`reset_database()`), so a
//! backend-persisted setting (e.g. Ingestion's toggles, which round-trip
//! through `ingestion.settings.get`/`update` into the same DB) CANNOT be
//! proven to survive a relaunch in this harness — the DB reset would erase
//! it regardless of whether the real app would have kept it. The
//! ingestion-settings-persist-across-restart scenario (journey-10 Test 4) is
//! left as a follow-up for that reason, not an oversight.
//!
//! (theme-settings-db, 2026-07-09) The theme choice used to be the one
//! exception — purely `localStorage`-backed
//! (`apps/desktop/src/data/theme.ts`), and therefore untouched by
//! `reset_database()`, which is why the original version of Test 2 below
//! asserted it survived a `relaunch()`. Theme is now DB-backed (settings
//! `general` scope, `theme` key) so the WebView2 force-kill data-loss finding
//! (a graceful shutdown flushes `localStorage`'s LevelDB store; a forced kill
//! does not) can't silently lose the user's choice. `localStorage` is kept
//! only as a synchronous boot cache, reconciled from the DB by
//! `hydrateThemeFromSettings()` shortly after boot. That moves theme into the
//! same "cannot be proven to survive a relaunch in this harness" bucket as
//! Ingestion above — this harness's `reset_database()` wipes the very row
//! `hydrateThemeFromSettings()` would otherwise confirm survived, and will
//! instead reconcile the boot cache back to the default. Test 2 below is
//! trimmed to the live-apply assertion only (unaffected by the DB reset); a
//! true cross-relaunch proof needs a harness `ResetScope` that preserves the
//! DB, left as a follow-up alongside Ingestion's.

mod common;

use std::time::Duration;

use anyhow::Context;
use common::{settle_first_run_redirect, E2eApp};
use serde_json::json;
use thirtyfour::By;

const UI_TIMEOUT: Duration = Duration::from_secs(20);

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

/// Test 1 (journey-10): the Ingestion pane has NO global "Save" button
/// anywhere, and toggling a real field auto-saves through the real backend
/// (`ingestion.settings.update`) without any explicit save action — proven
/// by a fresh `ingestion.settings.get` read reflecting the change.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn settings_ui_ingestion_toggle_autosaves_no_global_save_button() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;

    // The active pane is a real PATH param (`settingsPaneRoute`, path
    // `/settings/$pane` — `apps/desktop/src/app/router.tsx`), read via
    // `useParams` in `SettingsPage.tsx`, NOT a `?pane=` query string; the
    // query-string form silently falls back to the default 'sources' pane
    // (CI: "Follow symbolic links" never appeared — the Ingestion pane
    // never actually mounted).
    app.goto_route("/settings/ingestion").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;

    anyhow::ensure!(
        app.count_buttons_with_text("Save").await? == 0,
        "expected NO global 'Save' button on a settings pane (auto-save-only convention, spec 018)"
    );

    // Real backend precondition: `followSymlinks` defaults to false.
    let before: serde_json::Value = app.invoke("ingestion_settings_get", json!({})).await?;
    anyhow::ensure!(
        before["followSymlinks"] == json!(false),
        "expected the real default followSymlinks=false before toggling: {before}"
    );
    anyhow::ensure!(
        !app.checkbox_checked_by_aria_label("Follow symbolic links").await?,
        "expected the real checkbox to start unchecked, matching the backend default"
    );

    // Real DOM interaction: toggle it. No save button exists — this is the
    // only action a real user could take to change the value.
    app.click_by_aria_label("Follow symbolic links").await?;
    anyhow::ensure!(
        app.checkbox_checked_by_aria_label("Follow symbolic links").await?,
        "expected the real checkbox to reflect the toggle immediately"
    );

    // Real backend round-trip: confirm the auto-save actually persisted,
    // with no Save click anywhere in this journey.
    let after: serde_json::Value = app
        .invoke_until("ingestion_settings_get", json!({}), UI_TIMEOUT, |v: &serde_json::Value| {
            v["followSymlinks"] == json!(true)
        })
        .await
        .map_err(|e| {
            anyhow::anyhow!("expected the toggle to auto-save to the real backend: {e}")
        })?;
    anyhow::ensure!(after["followSymlinks"] == json!(true), "unexpected persisted value: {after}");

    app.shutdown().await
}

/// Test 2 (journey-10): switching the theme applies live (`<html
/// data-theme>` changes with no reload) and the write actually lands in both
/// the localStorage boot cache AND the settings DB (spec 018 `general`
/// scope, `theme` key — theme-settings-db).
///
/// (theme-settings-db, 2026-07-09) Trimmed from the original version, which
/// also asserted the choice survived a full `relaunch()`. That assertion
/// relied on theme being purely `localStorage`-backed and therefore
/// untouched by `E2eApp::relaunch()`'s unconditional `reset_database()`; now
/// that the DB is the source of truth, `relaunch()` wipes the very row that
/// would prove it survived, same as every other backend-persisted setting
/// (see module docs, Ingestion's `-persist-across-restart` follow-up). A
/// true cross-relaunch proof needs a harness `ResetScope` that preserves the
/// DB — left as a follow-up.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn settings_ui_theme_applies_live_and_persists_to_settings_db() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    complete_first_run(&app).await?;

    app.goto_route("/settings/general").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;

    // "Observatory" (`THEMES` id `observatory-dark`) — a real, non-default,
    // canonical theme swatch (handoff 03: Espresso Dark is now a disabled,
    // picker-hidden variant — see apps/desktop/src/data/theme.ts `enabled`),
    // matched by its visible name (the swatch button's full text also
    // includes its light/dark mode label, so an exact-match helper would be
    // wrong here — use `contains`). The predicate excludes "Cool" so this
    // can't accidentally match the new "Observatory Cool" / "Observatory
    // Cool · Light" swatches, which also contain the substring "Observatory".
    // Poll for the swatch to actually mount: it opens asynchronously
    // after the navigation, same route/render race `E2eApp::find_waiting`
    // documents.
    let xpath = "//button[contains(., 'Observatory') and not(contains(., 'Cool'))]";
    app.find_waiting(By::XPath(xpath), "the 'Observatory' theme swatch button")
        .await?
        .click()
        .await
        .context("click the Observatory theme swatch failed")?;

    // Live apply: no reload needed.
    let theme: String = app
        .driver
        .execute("return document.documentElement.getAttribute('data-theme')", vec![])
        .await
        .context("failed to read document.documentElement's data-theme")?
        .convert()
        .context("failed to deserialise data-theme")?;
    anyhow::ensure!(
        theme == "observatory-dark",
        "expected the theme to apply live with no reload, got data-theme={theme:?}"
    );

    // The localStorage boot cache is written synchronously alongside the
    // live apply.
    let stored: serde_json::Value = app
        .driver
        .execute("return window.localStorage.getItem('pv.theme')", vec![])
        .await
        .context("failed to read localStorage['pv.theme']")?
        .convert()
        .context("failed to deserialise localStorage['pv.theme']")?;
    anyhow::ensure!(
        stored == serde_json::json!("observatory-dark"),
        "expected localStorage['pv.theme']=\"observatory-dark\" to be written on click, got {stored:?}"
    );

    // The settings DB write-through (theme-settings-db) is fire-and-forget,
    // so poll `settings.get` rather than asserting immediately.
    let settings: serde_json::Value = app
        .invoke_until(
            "settings_get",
            json!({ "scope": "general" }),
            UI_TIMEOUT,
            |v: &serde_json::Value| v["values"]["theme"] == json!("observatory-dark"),
        )
        .await
        .map_err(|e| {
            anyhow::anyhow!("expected the theme choice to persist to the settings DB: {e}")
        })?;
    anyhow::ensure!(
        settings["values"]["theme"] == json!("observatory-dark"),
        "unexpected persisted settings.theme: {settings}"
    );

    app.shutdown().await
}
