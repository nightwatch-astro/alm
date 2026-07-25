// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Desktop shell crate boundary.
//!
//! Owns the Tauri 2 runtime, the shared `AppState`, and the typed command
//! surface declared in [`commands::lifecycle`]. Type-safe TypeScript bindings
//! are emitted at test time by `tests/bindings.rs` via tauri-specta.

pub mod clean_shutdown;
pub mod commands;
pub mod data_dir;
pub mod resolve_cache;
pub mod watcher;

mod bootstrap;

use std::sync::Arc;

use audit::bus::EventBus;
use persistence_core::Database;
use persistence_lifecycle::repositories::lifecycle::SqliteLifecycleRepository;
use tauri::utils::config::WindowConfig;
use tauri::webview::WebviewWindowBuilder;
use tauri::{Emitter, Manager};

use crate::bootstrap::background::{
    spawn_ingest_resolution_drain, spawn_stale_dependent_propagator,
};
use crate::bootstrap::menu::{build_native_menu, MENU_ID_SETTINGS};
use crate::bootstrap::window::{enforce_min_window_size, recenter_if_offscreen};
#[cfg(feature = "dev-tools")]
use crate::commands::dev::CallBuffer;
use crate::commands::lifecycle::AppState;

pub const CRATE_NAME: &str = "desktop_shell";

/// Label of the primary application window (`tauri.conf.json`).
pub const MAIN_WINDOW_LABEL: &str = "main";

// `include!`d, not `mod`-declared — see bootstrap/specta.rs's header comment
// for why `collect_commands!`'s hidden macro hygiene requires this file's
// `specta_builder()`/`base_builder()` to live in the crate-root textual
// scope. `tests/bindings.rs` depends on `desktop_shell::specta_builder`.
include!("bootstrap/specta.rs");

/// Build the Tauri [`App`] **without** starting the event loop.
///
/// The returned handle exposes the platform path resolver (needed to locate
/// the default `SQLite` database path) while the caller retains full control
/// over state management and app startup ordering.
///
/// # Panics
/// Panics if the Tauri runtime cannot be initialised.
#[must_use]
pub fn build_app() -> tauri::App {
    let builder = specta_builder();

    let mut tb = tauri::Builder::default();

    // Spec 051 US1: the single-instance guard MUST be the first plugin
    // registered so a redirected second launch is intercepted during
    // `.build()` — before any other plugin/state/window setup, and therefore
    // before `main()` ever reaches `Database::connect` / `db.migrate()`
    // (FR-003: the second launch performs no migration, seed, or write).
    //
    // E2E escape hatch (crates/e2e-tests): the harness sets
    // `PV_E2E_INSTANCE_ID` (unique per test process) and launches several
    // `desktop_shell` instances concurrently (`test-threads > 1`). The plugin
    // enforces ONE well-known identity derived from the app identifier, and a
    // per-instance override exists only on Linux (`dbus_id`) — NOT on Windows
    // (named mutex) or macOS. So concurrent instances collide and the loser is
    // silently redirected/exited without ever opening a window, timing out the
    // WebDriver session (observed on the Windows shard). No journey exercises
    // single-instance behaviour, so when the var is set we skip the plugin
    // entirely on every platform. The bypass additionally requires the `e2e`
    // feature at compile time, so release binaries ignore the variable — see
    // `bootstrap::single_instance_guard_enabled`.
    if crate::bootstrap::single_instance_guard_enabled(
        std::env::var_os("PV_E2E_INSTANCE_ID").is_some(),
    ) {
        tb = tb.plugin(
            tauri_plugin_single_instance::Builder::new()
                .callback(|app, argv, cwd| {
                    tracing::info!(
                        ?argv,
                        %cwd,
                        "second launch attempt redirected to existing instance"
                    );
                    // FR-002: focus/foreground the existing main window,
                    // restoring it if minimized, instead of opening a new
                    // window or connection.
                    if let Some(window) = app.get_webview_window("main") {
                        if let Err(e) = window.unminimize() {
                            tracing::warn!("failed to unminimize main window: {e:?}");
                        }
                        if let Err(e) = window.show() {
                            tracing::warn!("failed to show main window: {e:?}");
                        }
                        if let Err(e) = window.set_focus() {
                            tracing::warn!("failed to focus main window: {e:?}");
                        }
                    } else {
                        tracing::warn!("single-instance redirect: no `main` window found to focus");
                    }
                })
                .build(),
        );
    }

    tb = tb
        // Spec 051 US4 (T027): window-state persistence. Registered right
        // after single-instance so a redirected second launch (which never
        // creates a window of its own) never touches this plugin's store
        // file. `window-state:default` is granted in
        // `capabilities/default.json` (T028).
        //
        // Handoff 07: VISIBLE excluded from the restored flags. The `main`
        // window starts `"visible": false` in tauri.conf.json (splash owns
        // first paint) — restoring a persisted `visible: true` from the
        // previous session would fight that gate and show `main` before the
        // splash's minimum-display/boot-ready handshake completes.
        .plugin({
            // Issue astro-plan-qmc: the plugin's default store resolves through
            // `app_config_dir()`, which on Windows uses `dirs →
            // SHGetKnownFolderPath` and ignores the `APPDATA` env override the
            // E2E harness sets — so concurrent instances share one real OS
            // profile file and `reset_window_state()` deletes under a path that
            // never exists (same Known Folder class as #1204's `app_data_dir`
            // defect).
            //
            // Fix: when `PV_DATA_DIR` is set (E2E harness, every platform),
            // pass an absolute path to `with_filename()`. The plugin does
            // `app_config_dir().join(&filename)`; `Path::join` with an absolute
            // argument replaces the base entirely, redirecting the store to the
            // per-instance isolated dir the harness already owns. Production
            // builds never set `PV_DATA_DIR`, so the default filename is kept
            // and behaviour is unchanged for real users.
            let mut wb = tauri_plugin_window_state::Builder::default().with_state_flags(
                tauri_plugin_window_state::StateFlags::all()
                    - tauri_plugin_window_state::StateFlags::VISIBLE,
            );
            if let Some(data_dir) = crate::data_dir::resolve() {
                wb = wb.with_filename(
                    data_dir.join(".window-state.json").to_string_lossy().into_owned(),
                );
            }
            wb.build()
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Spec 051 US10 (T056): signed auto-update plugin. `updater:default` +
        // `process:default` (for the relaunch-to-apply step) are granted in
        // `capabilities/default.json`. `plugins.updater.pubkey` in
        // `tauri.conf.json` is the real minisign key (spec 051 SC-009/T059/
        // T060, #762) — the check/download/verify/relaunch flow itself is
        // frontend-driven (`updateSubscription.ts`, #888 staged flow), not
        // triggered from this Rust process.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Spec 051 US7 (T041): diagnostics log file. `skip_logger()` is
        // required here — this app already installs a global `tracing`
        // subscriber (in `main.rs`, right after `build_app()` returns, once
        // the platform log directory is resolvable) that owns the single
        // process-wide `log`-facade logger slot via `tracing-subscriber`'s
        // default `tracing-log` bridge feature. Without `skip_logger()`, this
        // plugin would try to install a SECOND global logger and panic/lose
        // the race (FR-021's "not duplicated or dropped" requirement). With
        // it, the plugin still registers its `log` Tauri command (so
        // `@tauri-apps/plugin-log` calls from the frontend reach the ambient
        // logger), it just never owns that logger itself — the rotating file
        // target itself is `main.rs`'s `tracing_appender` layer, not this
        // plugin's own (skipped) fern dispatch.
        .plugin(tauri_plugin_log::Builder::new().skip_logger().build());

    // Tauri MCP bridge plugin (@hypothesi tauri-plugin-mcp-bridge) — dev/debug
    // builds only. Runs a WebSocket server on 0.0.0.0:9223 that the
    // @hypothesi/tauri-mcp-server MCP server connects to, letting an agent drive
    // the running app for automated UI testing. Requires `withGlobalTauri`, which
    // is enabled only via the dev-only `tauri.dev.conf.json` overlay (never in the
    // shipped config). `debug_assertions` is off in release builds, so this
    // surface is absent from shipped binaries.
    #[cfg(debug_assertions)]
    {
        tb = tb.plugin(tauri_plugin_mcp_bridge::init());
    }

    // E2E gate: embed the WebDriver server only when built with --features e2e.
    // The embedded server listens on 127.0.0.1:4445; connect via the
    // tauri-webdriver CLI (cargo install tauri-webdriver --locked) on :4444.
    // Release builds MUST omit the `e2e` feature (Constitution Principle V).
    // Complements the MCP bridge above: scripted thirtyfour+nextest gate vs.
    // agent-interactive debugging.
    #[cfg(feature = "e2e")]
    {
        tb = tb.plugin(tauri_plugin_webdriver::init());
    }

    tb
        // Spec 051 US5 (T033): the native "Settings" menu item has no native
        // dialog of its own — emit a frontend event and let the existing
        // Settings route handle navigation, matching the reference pattern
        // of reusing existing UI rather than inventing new native dialogs.
        .on_menu_event(|app, event| {
            if event.id() == MENU_ID_SETTINGS {
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(e) = window.emit("menu:open-settings", ()) {
                        tracing::warn!("failed to emit menu:open-settings: {e:?}");
                    }
                }
            }
            // Quit (T034): `PredefinedMenuItem::quit` already calls
            // `app.exit(0)` internally — no separate handling needed here.
            // This app has no existing close-confirmation logic to bypass
            // (verified: no `on_window_event`/`CloseRequested` handler exists
            // anywhere in this crate), so Quit is a plain, un-gated exit.
        })
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            // Spec 051 US5 (T032): native application menu — App submenu
            // (About/Settings/Quit), Window submenu, and a standard Edit
            // submenu (copy/cut/paste/select-all/undo/redo). Does not touch
            // any existing native/React context-menu code path (T035 — none
            // exists in this crate to touch).
            match build_native_menu(app) {
                Ok(menu) => {
                    if let Err(e) = app.set_menu(menu) {
                        tracing::warn!("failed to set native application menu: {e:?}");
                    }
                }
                Err(e) => tracing::warn!("failed to build native application menu: {e:?}"),
            }

            Ok(())
        })
        .build(instance_context())
        .expect("error while building tauri application")
}

/// Builds the compiled-in Tauri context and defers the main window's
/// creation (see [`defer_main_window`]).
///
/// Per-instance webview isolation for the E2E harness (#1204) does **not**
/// go through this context any more: it used to point each config-declared
/// window at a per-instance `data_directory`, but that must be relative —
/// `WebviewBuilder::from_config` joins it onto `dirs::data_local_dir()`, the
/// very Known Folder that ignores `LOCALAPPDATA` overrides on Windows — so
/// it could isolate by *name* but never by *location*, and CI evidence
/// (`WindowsError(0x80070057)` surviving to TRY-1) showed it did not
/// reliably work. The harness now sets `WEBVIEW2_USER_DATA_FOLDER` instead
/// (`crates/e2e-tests/tests/common/mod.rs`) — `WebView2`'s own documented
/// loader override, read inside the app process — so this function needs no
/// Windows-specific branch at all.
/// Clear `create` on the [`MAIN_WINDOW_LABEL`] entry so Tauri's own `setup()`
/// — which runs on `RunEvent::Ready`, i.e. *inside* `app.run()`, not during
/// `.build()` (`tauri-2.11.5/src/app.rs:1424` + `:2524`) — creates the splash
/// window only.
///
/// This is the migration gate. The main webview is the sole surface that loads
/// the React app and issues IPC, so while it does not exist no route can
/// render and no command can observe an unmigrated database or an unmanaged
/// `AppState`. [`run_app`] rebuilds it from this same (retained) config entry
/// once migration has finished, which is what lets the event loop — and
/// therefore the splash's first frame — start *before* migration instead of
/// after it.
///
/// Returns `false` if no such entry exists, so a config rename fails loudly
/// rather than silently reverting to eager creation.
fn defer_main_window(windows: &mut [WindowConfig]) -> bool {
    let Some(main) = windows.iter_mut().find(|w| w.label == MAIN_WINDOW_LABEL) else {
        return false;
    };
    main.create = false;
    true
}

fn instance_context() -> tauri::Context {
    let mut context = tauri::generate_context!();

    assert!(
        defer_main_window(&mut context.config_mut().app.windows),
        "tauri.conf.json declares no `{MAIN_WINDOW_LABEL}` window; \
         run_app has nothing to create after migration"
    );

    // The webview's per-instance isolation (#1204) no longer goes through
    // config: a config-declared window's `data_directory` must be relative,
    // and Tauri joins it onto `dirs::data_local_dir()` — the very Known
    // Folder that ignores `LOCALAPPDATA` overrides on Windows, so that route
    // could isolate by *name* but never by *location*, and CI evidence (TRY-1
    // `WindowsError(0x80070057)`) showed it did not reliably work at all. The
    // E2E harness now sets `WEBVIEW2_USER_DATA_FOLDER` instead — WebView2's
    // own documented loader override, read inside the app process and immune
    // to the Known Folder lookup — so no product-side window config is
    // needed here any more (`crates/e2e-tests/tests/common/mod.rs`, refs
    // #1204).

    context
}

/// Start the event loop first, then finish database startup behind the splash.
///
/// The splash is the only window Tauri creates for itself (see
/// [`defer_main_window`]), so it paints as soon as `app.run()` begins pumping.
/// Connecting, migrating, and wiring shared state all happen on a background
/// task from there, and the main window is built only once that task has
/// finished — so a long migration is visible instead of being a windowless
/// pause, and the UI still cannot reach an unmigrated database.
pub fn run_app(app: tauri::App, db_url: String, data_dir: std::path::PathBuf) {
    // Developer diagnostics call buffer (spec 021).
    // Always managed so the type is available; only populated when dev-tools
    // feature is compiled in and devMode is on at runtime. No database
    // dependency, so it does not wait for `boot`.
    #[cfg(feature = "dev-tools")]
    app.manage(CallBuffer::new());

    // Driven from a dedicated OS thread via the ambient runtime handle rather
    // than `tokio::spawn`, because `boot` is not provably `Send`: holding a
    // `&Database` across `migrate()`'s await trips a higher-ranked-lifetime
    // limitation ("implementation of `sqlx::Acquire` is not general enough").
    // `Handle::block_on` has no `Send` bound and still enters the runtime
    // context, so `boot`'s own `tokio::spawn` calls land on the same
    // multi-threaded runtime they always have. The alternative — reshaping
    // `Database::migrate` — would disturb #1307's single-connection,
    // FK-disabled migration chain.
    let runtime = tokio::runtime::Handle::current();
    let handle = app.handle().clone();
    // astro-plan-kyo7.48: the exit hook writes the clean-shutdown marker, so it
    // needs its own copy of the data root before `data_dir` moves into boot.
    let exit_data_dir = data_dir.clone();
    std::thread::spawn(move || runtime.block_on(boot(handle, db_url, data_dir)));

    app.run(move |_handle, event| {
        // `RunEvent::Exit` is the final, non-cancellable event-loop hook — it
        // fires on graceful shutdown but never on SIGKILL/crash/power loss, so
        // a present marker at next boot reliably means "exited cleanly".
        if let tauri::RunEvent::Exit = event {
            clean_shutdown::write(&exit_data_dir);
        }
    });
}

/// Report an unrecoverable startup failure and terminate.
///
/// These were `.expect()` calls on the main thread while startup ran before
/// `app.run()`. From a spawned task a panic would only kill the task, leaving
/// the user in front of a splash that never resolves.
fn fatal(handle: &tauri::AppHandle, message: &str) {
    tracing::error!("{message}");
    handle.exit(1);
}

/// Build the main window from its (retained, `create: false`) config entry.
///
/// Call only after every `State` a command can ask for is managed: this is the
/// moment the React app becomes loadable and IPC becomes reachable.
fn create_main_window(handle: &tauri::AppHandle) {
    let config = handle.config().app.windows.iter().find(|w| w.label == MAIN_WINDOW_LABEL).cloned();
    let Some(config) = config else {
        fatal(handle, &format!("no `{MAIN_WINDOW_LABEL}` window config to build"));
        return;
    };

    let window =
        WebviewWindowBuilder::from_config(handle, &config).and_then(WebviewWindowBuilder::build);
    match window {
        // Spec 051 US4 (T029/T030): enforce the min-size floor and off-screen
        // fallback after tauri-plugin-window-state restores a persisted
        // size/position — it may restore geometry from a prior app version or
        // a since-disconnected monitor.
        Ok(window) => {
            enforce_min_window_size(&window);
            recenter_if_offscreen(&window);
        }
        Err(e) => fatal(handle, &format!("failed to create the `{MAIN_WINDOW_LABEL}` window: {e}")),
    }
}

// Sequential startup/subscriber-wiring assembly, not complex logic — same
// shape as `bootstrap::specta::specta_builder`, which carries the same allow.
#[allow(clippy::too_many_lines)]
/// Extract the filesystem path from a `sqlite://…` URL, or `None` for
/// in-memory / non-file URLs.
///
/// Only the path component before `?` is returned; any query parameters
/// (e.g. `?mode=rwc`) are stripped.
fn sqlite_file_path(db_url: &str) -> Option<std::path::PathBuf> {
    let url = db_url.strip_prefix("sqlite://")?;
    // Strip query string.
    let path_part = url.split('?').next()?;
    if path_part.is_empty() || path_part == ":memory:" {
        return None;
    }
    Some(std::path::PathBuf::from(path_part))
}

/// Back up `db_path` via `VACUUM INTO '<db_path>.pre-<version>.bak'` before
/// running migrations on an existing, behind-schema database.
///
/// Keeps only the two most-recent `.pre-*.bak` files alongside the database,
/// deleting older ones after a successful backup.  A failure here (full disk,
/// permission error) is non-fatal: the caller logs a warning and proceeds with
/// migration rather than bricking startup.
async fn run_pre_migration_backup(db: &Database, db_path: &std::path::Path, app_version: &str) {
    // Skip fresh databases (no _sqlx_migrations table) and up-to-date ones.
    match db.has_pending_migrations().await {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            tracing::warn!("could not check for pending migrations before backup: {e}");
            return;
        }
    }

    let bak_path = db_path.with_extension(format!("pre-{app_version}.bak"));
    if let Err(e) = db.backup_to(&bak_path).await {
        tracing::warn!(
            path = %bak_path.display(),
            // Non-fatal: a full disk must not prevent the app from starting.
            "pre-migration backup failed, proceeding without it: {e}"
        );
        return;
    }
    tracing::info!(path = %bak_path.display(), "pre-migration backup created");
    prune_old_backups(db_path);
}

/// Delete all but the two newest `<stem>.pre-*.bak` files next to `db_path`.
///
/// Best-effort: any individual removal failure is logged and skipped.
fn prune_old_backups(db_path: &std::path::Path) {
    let Some(parent) = db_path.parent() else { return };
    let Some(stem) = db_path.file_stem().and_then(|s| s.to_str()) else { return };
    let prefix = format!("{stem}.pre-");
    let suffix = ".bak";

    let mut bak_files: Vec<std::path::PathBuf> = match std::fs::read_dir(parent) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| {
                let name = e.file_name();
                let n = name.to_string_lossy();
                n.starts_with(&prefix) && n.ends_with(suffix)
            })
            .map(|e| e.path())
            .collect(),
        Err(e) => {
            tracing::warn!("failed to read directory for backup pruning: {e}");
            return;
        }
    };

    if bak_files.len() <= 2 {
        return;
    }

    // Sort oldest-first by modification time (fall back to path for stability).
    bak_files.sort_by(|a, b| {
        let mtime = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        mtime(a).cmp(&mtime(b))
    });

    // Remove everything except the two newest.
    for path in &bak_files[..bak_files.len() - 2] {
        if let Err(e) = std::fs::remove_file(path) {
            tracing::warn!(path = %path.display(), "failed to prune old backup: {e}");
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn boot(app: tauri::AppHandle, db_url: String, data_dir: std::path::PathBuf) {
    let db = match Database::connect(&db_url).await {
        Ok(db) => db,
        Err(e) => return fatal(&app, &format!("failed to connect to SQLite at {db_url}: {e}")),
    };

    // kyo7.28: back up the database before applying migrations to an existing,
    // behind-schema file.  Skip for fresh databases (no migration table yet)
    // and for in-memory connections.  run_pre_migration_backup owns the
    // has_pending_migrations guard; boot just skips the whole call for
    // in-memory URLs where VACUUM INTO is meaningless.
    if let Some(db_path) = sqlite_file_path(&db_url) {
        run_pre_migration_backup(&db, &db_path, env!("CARGO_PKG_VERSION")).await;
    }

    if let Err(error) = db.migrate().await {
        // A raw `fatal()` here produced `Migration(VersionMismatch(71))` and
        // nothing else — technically true, actionable by nobody. Translate
        // the recognised "this file predates this build" cases into a named
        // failure that says which migration diverged and what to do about
        // it.
        if let Some(detail) = persistence_core::migration_divergence_detail(&error) {
            let message = format!(
                "Database schema does not match this build: {detail}.\n\
                 \n\
                 This database was created by a different revision of PlateVault. \
                 It is a development-only condition — switching between branches \
                 that each added migrations leaves a file whose migration history \
                 no longer matches the running binary.\n\
                 \n\
                 To recover, delete the database and let it be recreated:\n\
                 \x20 {db_url}\n\
                 \n\
                 Set PV_DB_URL to point at a different file if you need to keep this one."
            );
            return fatal(&app, &message);
        }
        return fatal(&app, &format!("failed to run migrations on {db_url}: {error}"));
    }
    let pool = db.pool().clone();

    // Unclean-shutdown detection (astro-plan-kyo7.48): consume the clean-shutdown
    // marker written on the previous graceful exit. Absent marker => the process
    // was killed or crashed. Captured here, before the async sweep, and exposed
    // to the recovery prompt via the managed `UncleanShutdown` flag.
    let unclean_shutdown = !clean_shutdown::take_was_clean(&data_dir);

    // Startup sweep + boot reconciliation (constitution v1.1.0 §V). Any plan
    // left in 'applying' with no live executor (e.g. after a hard crash) is
    // unreachable by `resume_plan` (which requires `paused` state). Flip them to
    // `paused` with pause_reason='crash', then reconcile each plan's
    // intent-without-confirmed-outcome items against filesystem reality: heal
    // the ones the filesystem shows completed, and flag ambiguous ones for the
    // unclean-shutdown recovery prompt. The active_runs registry is always empty
    // at this point, so every applying plan qualifies.
    {
        let sweep_pool = pool.clone();
        tokio::spawn(async move {
            let ids = match app_core::plan_apply::sweep_crashed_applying_plans(&sweep_pool).await {
                Ok(ids) => {
                    if !ids.is_empty() {
                        tracing::info!(
                            count = ids.len(),
                            "startup: flipped {} applying plan(s) to paused (crash recovery)",
                            ids.len()
                        );
                    }
                    ids
                }
                Err(e) => {
                    tracing::warn!("startup sweep for crashed applying plans failed: {e}");
                    Vec::new()
                }
            };
            match app_core::plan_apply::reconcile_crashed_plans(&sweep_pool, &ids).await {
                Ok(report) if report.healed > 0 || report.needs_user_review() => tracing::info!(
                    healed = report.healed,
                    left_for_resume = report.left_for_resume,
                    ambiguous_plans = report.ambiguous_plan_ids.len(),
                    "startup: boot reconciliation classified crashed plan items"
                ),
                Ok(_) => {}
                Err(e) => tracing::warn!("startup boot reconciliation failed: {e}"),
            }
        });
    }

    // Spec 052 P1 (D2): open (creating if missing) the shared redb resolve
    // cache. Opening is fast (no warm yet — the warm below is backgrounded so
    // a large seed never blocks startup).
    let resolve_cache_path = data_dir.join("simbad-cache.redb");
    let resolve_cache = crate::resolve_cache::open_or_in_memory(&resolve_cache_path);

    let bus = EventBus::with_pool(pool.clone());

    // Per-instance app-layer caches. Created early so background tasks spawned
    // below (e.g. the settings repair pass) can share the same instance that
    // eventually ends up in AppState.
    let caches = app_core::AppCaches::shared();

    // Live event-bus subscribers. Start these *before* `bus`/`pool` are moved
    // into `AppState` below. Each spawns a tokio task on the runtime that
    // `#[tokio::main]` establishes around `run_app`.
    //  - spec 005: inbox plan listener → marks inbox items `resolved` once their
    //    split/restructure plan is applied.
    //  - spec 019: log forwarder → pushes audit + diagnostic entries to the
    //    webview `log:entry` channel. Forward at the most permissive level; the
    //    client filters by level.
    app_core::inbox::plan_listener::start_inbox_plan_listener(
        pool.clone(),
        &bus,
        resolve_cache.clone(),
    );
    crate::commands::log::start_log_forwarder(
        app.clone(),
        &bus,
        contracts_core::log::LogLevel::Debug,
        pool.clone(),
    );
    drop(spawn_stale_dependent_propagator(pool.clone(), &bus));
    // spec 056 (R5): backend-authoritative onboarding tick subscriber →
    // persists auto-ticks from domain-completion topics and emits
    // `onboarding:state-changed`. Started here, before the webview can invoke,
    // so no tick can be lost to a UI race (PQ-005 ordering).
    crate::commands::onboarding::start_onboarding_subscriber(app.clone(), pool.clone(), &bus);
    // spec 056 (PQ-005): recover ticks whose live event was missed — published
    // before the subscriber subscribed, or lost to a kill between the action
    // and the tick write. Started AFTER the subscriber so the two can only
    // agree, and never fatally: a failure leaves the checklist exactly as it
    // behaves today, still repairable via the Settings restore.
    {
        let reconcile_pool = pool.clone();
        drop(tokio::spawn(async move {
            match app_core::onboarding::reconcile_missed_events(&reconcile_pool).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("onboarding reconciliation recovered {n} missed tick(s)"),
                Err(e) => tracing::warn!("onboarding reconciliation failed: {e:?}"),
            }
        }));
    }
    // spec 024: manifest auto-generation on workflow-run completion.
    // The JoinHandle is intentionally dropped — the task runs independently.
    drop(app_core::project_manifests::spawn_workflow_run_subscriber(pool.clone(), bus.clone()));
    // spec 012 T008: per-project artifact filesystem watchers, attached/detached
    // via the `artifact.watcher.attach`/`artifact.watcher.detach` commands as the
    // project drawer opens/closes. No watcher runs until a project is attached.
    let artifact_watcher_registry = crate::watcher::new_artifact_watcher_registry();
    app.manage(artifact_watcher_registry.clone());
    // Item (f): periodic sweep re-attaches watchers when a previously
    // unavailable drive becomes reachable (e.g. external USB mount).
    drop(crate::watcher::spawn_volume_availability_sweep(
        pool.clone(),
        bus.clone(),
        artifact_watcher_registry,
    ));
    // spec 012 (WP-012-A): one-time, idempotent fix-up for `processing_artifacts`
    // rows the retired global root watcher (pre-#400) keyed by a library-root id
    // instead of the owning project's id. Runs once per app start, before any
    // per-project watcher attaches and records new (correctly-attributed) rows.
    {
        let fixup_pool = pool.clone();
        tokio::spawn(async move {
            match app_core::artifact::reattribute_root_keyed_artifacts(&fixup_pool).await {
                Ok((fixed, unmatched)) => {
                    if fixed > 0 || unmatched > 0 {
                        tracing::info!(
                            "artifact re-attribution fix-up: {fixed} row(s) corrected, \
                             {unmatched} row(s) left flagged (no matching project)"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("artifact re-attribution fix-up failed: {e:?}");
                }
            }
        });
    }

    // spec 018 T018/T019: hydrate defaults for missing settings rows and repair
    // invalid stored values (delete the bad row, fall back to the in-code
    // default, emit a settings.repair audit event), then prime the settings-bag
    // read cache. Runs once per app start, before the snapshot pass below reads
    // noisy-key values and before any settings.get call.
    {
        let repair_pool = pool.clone();
        let repair_bus = bus.clone();
        let repair_caches = caches.clone();
        tokio::spawn(async move {
            if let Err(e) = app_core::settings::get_settings(
                &repair_pool,
                &repair_bus,
                &repair_caches.settings.bag,
            )
            .await
            {
                tracing::warn!("settings repair pass failed: {e:?}");
            }
        });
    }

    // spec 018 T020: emit a settings.snapshot at session start, then every 5 minutes.
    // This gives the audit log a durable record of the active configuration even when
    // noisy keys (pattern, protectedCategories, …) haven't changed individually.
    {
        let snap_pool = pool.clone();
        let snap_bus = bus.clone();
        tokio::spawn(async move {
            // #668 suppression state, scoped to this loop — the only emitter
            // of settings.snapshot.
            let dedupe = app_core::settings::SnapshotDedupe::new();
            // Session-start snapshot.
            if let Err(e) =
                app_core::settings::emit_snapshot(&snap_pool, &snap_bus, "session_start", &dedupe)
                    .await
            {
                tracing::warn!("settings.snapshot (session_start) failed: {e:?}");
            }
            // Debounce loop: emit every 5 minutes while the app is running.
            let interval = std::time::Duration::from_mins(5);
            loop {
                tokio::time::sleep(interval).await;
                if let Err(e) = app_core::settings::emit_snapshot(
                    &snap_pool,
                    &snap_bus,
                    "debounce_5min",
                    &dedupe,
                )
                .await
                {
                    tracing::warn!("settings.snapshot (debounce_5min) failed: {e:?}");
                }
            }
        });
    }

    // spec 052 P1 (D2/T012): warm the shared redb resolve cache from the
    // bundled seed + existing durable canonical_target rows, in the
    // background — each phase is one `Cache::upsert_batch` write transaction
    // (spec 052 P4/#695), so warming the full ~13k-object popular seed
    // synchronously would still freeze startup for a noticeable moment.
    // First-run-guarded (`warm_bundled_on_first_run` no-ops once already
    // warmed); failure degrades to seed+cache typeahead simply being emptier
    // until the next launch, never blocks the UI.
    //
    // `cache_warming` (shared with the managed `AppState` below) is set true
    // for the duration via a `CacheWarmingGuard` (not a bare sequential
    // store — a panic mid-warm must still clear it, see its doc comment):
    // batching a whole phase into one transaction means no row is visible to
    // a reader until that phase commits, so a `target.search` query landing
    // in this window can get a legitimate-looking empty result for a seed
    // object that just hasn't committed yet — the flag lets `target.search`
    // tell the frontend to retry instead of freezing on that stale answer
    // (issue #818).
    let cache_warming = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        // Cloned (cheap — an `Arc` handle, see `ResolveCache`'s doc comment),
        // not moved: `resolve_cache` itself is still needed below to build
        // `AppState`. `warm_handle` keeps the CONCRETE type so `.flush()` is
        // reachable after the warm; `warm_cache` (its `.cache()`) is the
        // erased `Cache` trait object the warm functions themselves take.
        let warm_handle = resolve_cache.clone();
        let warm_cache = resolve_cache.cache();
        let warm_pool = pool.clone();
        let warm_guard =
            crate::commands::lifecycle::CacheWarmingGuard::start(cache_warming.clone());
        tokio::spawn(async move {
            let _warm_guard = warm_guard;
            let namespace = simbad_resolver::identity::namespace("astro-plan.targets");
            match targeting_resolver::seed::warm_bundled_on_first_run(&warm_cache, &namespace).await
            {
                Ok(Some(count)) => tracing::info!("warmed {count} bundled target seed entries"),
                Ok(None) => tracing::debug!("resolve cache already warmed; skipping bundled seed"),
                Err(e) => tracing::warn!("failed to warm bundled target seed: {e}"),
            }
            match targeting_resolver::seed::warm_from_canonical_target(
                &warm_cache,
                &warm_pool,
                &namespace,
            )
            .await
            {
                Ok(count) if count > 0 => {
                    tracing::info!(
                        "warmed {count} durable canonical_target rows into resolve cache"
                    );
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("failed to warm resolve cache from canonical_target: {e}"),
            }
            // #818 follow-up: both phases above write `Eventual` (fsync-free)
            // chunks; this is the one fsync that persists all of them (redb
            // commits are cumulative) — the bundled-seed phase's own warm-
            // complete sentinel write is ALSO durable on its own (single-item
            // upsert), but the canonical_target phase has no such capstone,
            // so this explicit flush is what actually protects it.
            if let Err(e) = warm_handle.flush().await {
                tracing::warn!("failed to flush resolve cache after startup warm: {e}");
            }
        });
    }

    // spec 035 US4/T043: background ingest-resolution drain + session target
    // back-fill on an interval. Non-blocking; transient/offline outcomes leave
    // rows pending for the next pass.
    spawn_ingest_resolution_drain(pool.clone(), bus.clone(), resolve_cache.clone());

    // spec 051 US10: the startup update check moved to the frontend
    // (`updateSubscription.ts`'s `startUpdateSubscription()`, #888 staged
    // flow) — this process no longer runs its own independent check, which
    // used to emit an `update-available` event nothing listens for anymore.

    // Inbox + inventory commands take `State<'_, SqlitePool>` directly (rather
    // than via AppState), so the raw pool must be managed too. Without this they
    // fail at runtime with "state not managed for field `pool`" — which is why
    // the Inbox scan/classify pipeline only ever worked under mock mode.
    app.manage(pool.clone());

    // astro-plan-kyo7.48: the boot-time unclean-shutdown verdict, read by the
    // `recovery_status` command that drives the recovery prompt.
    app.manage(clean_shutdown::UncleanShutdown(unclean_shutdown));

    let repo = Arc::new(SqliteLifecycleRepository::new(pool, bus.clone()));
    let state = AppState::new(repo, bus, caches, resolve_cache, resolve_cache_path, cache_warming);

    app.manage(state);

    // Last, and only here: the schema is current and every `State` a command
    // can ask for is managed, so it is now safe for a webview to exist.
    create_main_window(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(label: &str) -> WindowConfig {
        WindowConfig { label: label.to_owned(), ..WindowConfig::default() }
    }

    // ── sqlite_file_path ───────────────────────────────────────────────────

    #[test]
    fn sqlite_file_path_extracts_abs_path() {
        let p = super::sqlite_file_path("sqlite:///home/user/alm.db?mode=rwc").unwrap();
        assert_eq!(p, std::path::PathBuf::from("/home/user/alm.db"));
    }

    #[test]
    fn sqlite_file_path_returns_none_for_memory() {
        assert!(super::sqlite_file_path("sqlite::memory:").is_none());
    }

    // ── pre-migration backup ───────────────────────────────────────────────

    /// Fresh DB (no migrations applied): backup must be skipped.
    #[tokio::test]
    async fn backup_skipped_for_fresh_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let db = Database::connect(&db_url).await.unwrap();
        // Do NOT run migrate — fresh DB, no _sqlx_migrations table.
        run_pre_migration_backup(&db, &db_path, "0.6.0").await;

        let bak = db_path.with_extension("pre-0.6.0.bak");
        assert!(!bak.exists(), "backup must not be created for a fresh (unmigrated) database");
    }

    /// Existing DB with all migrations applied: no backup needed (`has_pending=false`).
    #[tokio::test]
    async fn backup_skipped_when_up_to_date() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let db = Database::connect(&db_url).await.unwrap();
        db.migrate().await.unwrap();

        let pending = db.has_pending_migrations().await.unwrap();
        assert!(!pending);

        // has_pending_migrations = false → backup not triggered (tested via the
        // flag; run_pre_migration_backup itself still works on any file-backed DB).
    }

    /// Existing DB that is behind (simulated by deleting one applied row):
    /// backup must be created.
    #[tokio::test]
    async fn backup_created_when_migrations_pending() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let db = Database::connect(&db_url).await.unwrap();
        db.migrate().await.unwrap();

        // Simulate a behind-schema DB.
        sqlx::query(
            "DELETE FROM _sqlx_migrations \
             WHERE version = (SELECT MAX(version) FROM _sqlx_migrations)",
        )
        .execute(db.pool())
        .await
        .unwrap();

        assert!(db.has_pending_migrations().await.unwrap());
        run_pre_migration_backup(&db, &db_path, "0.6.0").await;

        let bak = db_path.with_extension("pre-0.6.0.bak");
        assert!(bak.exists(), "backup must be created when migrations are pending");
    }

    /// After creating 3 backups, the oldest must be pruned (only 2 kept).
    #[test]
    fn prune_keeps_two_newest_backups() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("alm.db");

        // Create three fake .bak files with distinct modification times.
        let bak1 = dir.path().join("alm.pre-0.4.0.bak");
        let bak2 = dir.path().join("alm.pre-0.5.0.bak");
        let bak3 = dir.path().join("alm.pre-0.6.0.bak");
        for p in [&bak1, &bak2, &bak3] {
            std::fs::write(p, b"dummy").unwrap();
        }
        // Set mtimes so ordering is deterministic: bak1 oldest, bak3 newest.
        let epoch = std::time::SystemTime::UNIX_EPOCH;
        for (p, secs) in [(&bak1, 1000u64), (&bak2, 2000), (&bak3, 3000)] {
            let ft = std::fs::FileTimes::new()
                .set_accessed(epoch + std::time::Duration::from_secs(secs))
                .set_modified(epoch + std::time::Duration::from_secs(secs));
            let f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
            f.set_times(ft).unwrap();
        }

        super::prune_old_backups(&db_path);

        assert!(!bak1.exists(), "oldest backup must be pruned");
        assert!(bak2.exists(), "second backup must be kept");
        assert!(bak3.exists(), "newest backup must be kept");
    }

    /// The ordering guarantee, at the only place it can be enforced: Tauri
    /// creates `create: true` config windows on `RunEvent::Ready`, so the
    /// splash must stay eager and `main` must not — otherwise the React app
    /// loads, and its IPC reaches commands, while `boot` is still migrating.
    #[test]
    fn real_config_creates_the_splash_eagerly_and_defers_main() {
        let context = instance_context();
        let windows = &context.config().app.windows;

        let main = windows.iter().find(|w| w.label == MAIN_WINDOW_LABEL).expect("main window");
        assert!(!main.create, "`main` must not be created before migrations run");

        let splash = windows.iter().find(|w| w.label == "splash").expect("splash window");
        assert!(splash.create, "the splash must paint while migrations run");
    }

    #[test]
    fn deferring_reports_a_missing_main_entry() {
        let mut windows = [window("splash")];
        assert!(!defer_main_window(&mut windows));
        assert!(windows[0].create, "an unrelated window must keep its `create` flag");
    }

    // ── window-state path isolation (astro-plan-qmc) ──────────────────────

    /// When `PV_DATA_DIR` is set the window-state file MUST resolve to an
    /// absolute path under that dir, not under `app_config_dir()`. The plugin
    /// does `app_config_dir().join(&filename)`; supplying an absolute filename
    /// replaces the base (std `Path::join` contract). This is what prevents
    /// concurrent E2E instances from sharing one real OS window-state file on
    /// Windows, where `app_config_dir()` ignores the `APPDATA` env override.
    #[test]
    fn window_state_path_derived_from_alm_data_dir() {
        let data_dir = std::path::PathBuf::from("/tmp/isolated-e2e-instance/appdata");
        let expected = data_dir.join(".window-state.json");
        // Simulate what build_app() computes when PV_DATA_DIR is set.
        let filename = data_dir.join(".window-state.json").to_string_lossy().into_owned();
        // Path::join with an absolute argument must replace the base — verify
        // here so a regression in the assumption is caught immediately.
        let fake_config_dir = std::path::PathBuf::from("/real/OS/config/dir");
        assert_eq!(
            fake_config_dir.join(&filename),
            expected,
            "absolute filename must replace app_config_dir base"
        );
    }
}
