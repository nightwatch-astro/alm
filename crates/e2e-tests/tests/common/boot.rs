// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Process-scoped boot infrastructure: pre-warmed cache, isolated instance
//! environment, ephemeral port allocation, and the shared timeout constants.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};
use simbad_resolver::Cache as _;

/// Pre-warmed resolve cache file, built once per process from the stripped
/// E2E seed (~200 entries). Copied into each [`InstanceEnv`]'s appdata dir
/// before launch so the app's `warm_bundled_on_first_run` sentinel check
/// immediately no-ops (~150ms) instead of warming the full 13k-row bundled
/// seed (~2-14s depending on platform/build profile).
///
/// The file is a real `simbad-cache.redb` produced by the same
/// `targeting_resolver::seed::warm_cache` + sentinel write the app uses —
/// byte-for-byte compatible with the production open path.
pub(super) struct PrewarmedCache {
    /// Temp dir keeping the pre-warmed file alive for the process lifetime.
    pub(super) _dir: tempfile::TempDir,
    /// Absolute path to the pre-warmed `.redb` file.
    pub(super) path: PathBuf,
}

impl PrewarmedCache {
    /// Build a pre-warmed resolve cache from the stripped E2E seed.
    ///
    /// Spawns a dedicated OS thread so the inner `block_on` never races with
    /// the ambient tokio runtime that `#[tokio::test]` E2E tests run under
    /// (calling `block_on` from inside an async context panics).
    pub(super) fn build() -> Result<Self> {
        let dir = tempfile::tempdir().context("failed to create pre-warm temp dir")?;
        let path = dir.path().join("simbad-cache.redb");

        let path_clone = path.clone();
        std::thread::spawn(move || -> Result<()> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to build tokio runtime for pre-warm")?;

            rt.block_on(async {
                let resolve_cache = targeting_resolver::simbad::ResolveCache::open(&path_clone)
                    .context("failed to open pre-warm redb file")?;
                let cache = resolve_cache.cache();
                let namespace = simbad_resolver::identity::namespace("astro-plan.targets");

                let seed = targeting_resolver::seed::bundled_e2e()
                    .context("failed to parse e2e seed asset")?;
                targeting_resolver::seed::warm_cache(&cache, &seed, &namespace)
                    .await
                    .context("failed to warm e2e seed into pre-warm cache")?;

                // Write the sentinel so `warm_bundled_on_first_run` skips the
                // full seed load. Uses the FULL seed's `generated_at` as the
                // version key (what the app's sentinel check compares against).
                let full_seed = targeting_resolver::seed::bundled()
                    .context("failed to parse full bundled seed for sentinel")?;
                let sentinel = simbad_resolver::ResolvedIdentity {
                    simbad_oid: Some(-1),
                    primary_designation: "\u{2205} ALM SEED WARM SENTINEL".to_owned(),
                    common_name: Some(full_seed.generated_at.clone()),
                    object_type: simbad_resolver::ObjectType::Other,
                    otype_raw: String::new(),
                    ra_deg: 0.0,
                    dec_deg: 0.0,
                    v_mag: None,
                    aliases: vec![simbad_resolver::ResolvedAlias::new(
                        "\u{2205} ALM SEED WARM SENTINEL",
                        simbad_resolver::AliasKind::Designation,
                    )],
                    source: simbad_resolver::TargetSource::Seed,
                };
                cache
                    .upsert(&sentinel, &namespace)
                    .await
                    .context("failed to write pre-warm sentinel")?;

                resolve_cache.flush().await.context("failed to flush pre-warm cache")?;
                anyhow::Ok(())
            })
        })
        .join()
        .map_err(|_| anyhow::anyhow!("pre-warm thread panicked"))??;

        Ok(Self { _dir: dir, path })
    }
}

/// Process-wide pre-warmed cache singleton.
pub(super) fn prewarmed_cache() -> &'static PrewarmedCache {
    static CACHE: OnceLock<PrewarmedCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        PrewarmedCache::build().expect("failed to build the pre-warmed resolve cache for E2E boot")
    })
}

/// Per-process isolated E2E instance environment: an isolated
/// app-data/app-config/DB root — so concurrent `cargo-nextest` PROCESSES
/// (`test-threads > 1`; nextest gives each `#[test]` its own OS process, so
/// there is no in-process races to guard, only cross-process port/file
/// collisions) never share a SQLite file or webview profile.
///
/// Lazily allocated once per process and reused for every
/// [`E2eApp::launch`]/[`E2eApp::relaunch`] call in that test: `relaunch()`
/// (`ResetScope::PreserveWebviewStorage`) depends on the SAME app-data root
/// surviving across a launch -> shutdown -> relaunch sequence within one
/// journey (that's the whole point of the webview-storage-preserving
/// restart), so this must NOT be re-picked per `launch_with` call.
///
/// Ports are intentionally NOT stored here; they are picked fresh on every
/// [`E2eApp::launch_with`] call (see [`pick_port_pair`]) so that a relaunch
/// never reuses a port that was freed by the preceding shutdown and may have
/// been grabbed by another process in the interim.
pub(super) struct InstanceEnv {
    /// Kept alive for the process lifetime so the paths derived from it stay
    /// valid; never read directly.
    pub(super) _root: tempfile::TempDir,
    /// Env vars to set (and transitively propagate through the
    /// `tauri-webdriver` CLI, which does not `env_clear()` its spawned
    /// `desktop_shell` child) so the app resolves its `app_data_dir`/
    /// `app_config_dir` (and, on Windows, `app_local_data_dir`) under this
    /// instance's isolated root instead of the shared real OS profile.
    pub(super) vars: Vec<(&'static str, String)>,
    /// Isolated SQLite file this instance's app connects to (`PV_DB_URL`).
    pub(super) db_path: PathBuf,
}

impl InstanceEnv {
    pub(super) fn new() -> Result<Self> {
        let root = tempfile::tempdir().context("failed to create isolated E2E instance dir")?;
        let db_path = root.path().join("e2e-test.db");

        // Pre-warm: copy the once-per-process warmed resolve cache into this
        // instance's appdata dir so the app's `warm_bundled_on_first_run`
        // finds the sentinel and skips the full 13k-row seed load entirely.
        let appdata = root.path().join("appdata");
        std::fs::create_dir_all(&appdata)
            .context("failed to create instance appdata dir for pre-warm copy")?;
        let dest = appdata.join("simbad-cache.redb");
        std::fs::copy(&prewarmed_cache().path, &dest).with_context(|| {
            format!(
                "failed to copy pre-warmed cache {} -> {}",
                prewarmed_cache().path.display(),
                dest.display()
            )
        })?;

        // Issue #1204: the per-OS location vars below are honoured on Linux
        // (`XDG_*`) and macOS (`HOME`), and silently ignored on Windows —
        // Tauri resolves app dirs through `dirs`, which calls
        // `SHGetKnownFolderPath`, and the Known Folder API reads the user's
        // shell profile rather than `APPDATA`/`LOCALAPPDATA`. So on Windows
        // every concurrent instance shared one real app-data root however
        // these were set, colliding over `simbad-cache.redb` and — fatally —
        // over the WebView2 user-data folder.
        //
        // `PV_DATA_DIR` is an explicit override the app itself honours
        // (`desktop_shell::data_dir`), so isolation no longer depends on the
        // OS agreeing to be redirected. The per-OS vars stay: they still
        // place `app_config_dir` (window-state) under this root on Linux and
        // macOS, which `PV_DATA_DIR` does not cover.
        let mut vars: Vec<(&'static str, String)> =
            vec![("PV_DATA_DIR", appdata.display().to_string())];
        vars.extend(if cfg!(target_os = "windows") {
            vec![
                ("APPDATA", root.path().join("appdata").display().to_string()),
                ("LOCALAPPDATA", root.path().join("localappdata").display().to_string()),
                // The other half of #1204, and the fatal half: concurrent
                // instances shared ONE WebView2 user-data folder, so the loser
                // could not create its webview at all
                // (`WindowsError(0x80070057)`), never opened a window, and
                // never brought up its WebDriver port — surfacing four layers
                // downstream as `bridge never became ready`.
                //
                // `WEBVIEW2_USER_DATA_FOLDER` is WebView2's own documented
                // loader override: when set, it REPLACES the `userDataFolder`
                // argument the app passes to
                // `CreateCoreWebView2EnvironmentWithOptions`. Microsoft
                // documents it as the intended lever for testing/deployment
                // overrides, which is exactly this.
                //
                // It is read by the WebView2 loader inside the app process, so
                // unlike APPDATA/LOCALAPPDATA it cannot be quietly bypassed by
                // a Known Folder lookup — and unlike a config-declared
                // window's `data_directory` (which must be RELATIVE, and
                // resolves under `dirs::data_local_dir()`), it takes an
                // absolute path, so the folder genuinely lives under this
                // instance's temp root instead of merely having a unique name
                // in a shared one.
                ("WEBVIEW2_USER_DATA_FOLDER", root.path().join("webview2").display().to_string()),
            ]
        } else if cfg!(target_os = "macos") {
            // app_config_dir resolves under $HOME on macOS (see
            // `app_config_dir` below).
            vec![("HOME", root.path().display().to_string())]
        } else {
            vec![
                ("XDG_DATA_HOME", root.path().join("xdg-data").display().to_string()),
                ("XDG_CONFIG_HOME", root.path().join("xdg-config").display().to_string()),
            ]
        });
        Ok(Self { _root: root, vars, db_path })
    }
}

/// The process-wide [`InstanceEnv`] singleton — see its docs for why this
/// must be lazily-allocated-once rather than per-launch.
pub(super) fn instance_env() -> &'static InstanceEnv {
    static ENV: OnceLock<InstanceEnv> = OnceLock::new();
    ENV.get_or_init(|| {
        InstanceEnv::new().expect(
            "failed to allocate an isolated E2E instance environment \
             (temp dir creation or pre-warmed cache copy failed)",
        )
    })
}

/// Bind two ephemeral (`:0`) TCP ports on loopback and return them, dropping
/// the listeners immediately so `tauri-webdriver` can bind them itself.
///
/// This has an inherent bind-race window between the listener drop here and
/// `tauri-webdriver`'s own bind a moment later (the standard "ask the OS for
/// a free port, then let someone else use it" pattern used by e.g. the
/// `portpicker` crate). The residual TOCTOU risk is mitigated by calling this
/// fresh on every [`E2eApp::launch_with`] attempt (never reusing a port that
/// was held by a now-dead process) and by the outer port-rebind retry loop in
/// `launch_with` that re-picks on a detected early `tauri-webdriver` exit.
pub(super) fn pick_port_pair() -> Result<(u16, u16)> {
    let a = std::net::TcpListener::bind("127.0.0.1:0")
        .context("failed to bind an ephemeral port for the tauri-webdriver proxy")?;
    let b = std::net::TcpListener::bind("127.0.0.1:0")
        .context("failed to bind an ephemeral port for the tauri-plugin-webdriver native server")?;
    let proxy_port = a.local_addr().context("failed to read proxy port local_addr")?.port();
    let native_port = b.local_addr().context("failed to read native port local_addr")?.port();
    drop(a);
    drop(b);
    Ok((proxy_port, native_port))
}

/// Vite dev-server / `vite preview` URL the app's Tauri `devUrl` points at
/// (`apps/desktop/src-tauri/tauri.conf.json`). The app loads this URL on its
/// own at launch — do NOT `driver.goto(APP_URL)` after connecting. Kept for
/// journeys that need to assert the current URL or navigate within the SPA.
///
/// MUST be the `localhost` host form, byte-identical to `devUrl`: the app
/// boots on `http://localhost:5173`, and `localhost` vs `127.0.0.1` are
/// DIFFERENT web origins with separate localStorage. Navigating journeys to
/// a `127.0.0.1` URL splits app state across two origins — preferences
/// written on one (e.g. `setupCompleted`, `complete_first_run_gate`) are
/// invisible on the other, which made `Shell`'s localStorage-based setup
/// gate and `SetupPage`'s backend-based check ping-pong `/setup` ↔ `/inbox`
/// indefinitely.
pub const APP_URL: &str = "http://localhost:5173";

/// Overall deadline for WebDriver session creation in [`E2eApp::launch`].
///
/// Must comfortably cover a debug-build app boot on a cold CI runner: DB
/// connect + migrations + the ~13k-row bundled target-seed load all happen
/// BEFORE the window exists (observed ~30 s serial on ubuntu-latest, CI run
/// 28694907445), and the plugin's own per-attempt window-wait is only 10 s.
///
/// Raised from 120 s to 240 s (CI run 29592400990, PR #951): with the `e2e`
/// nextest profile's `test-threads` raised above 1, two of these CPU-heavy
/// boots (WebKitGTK/WebView2 init + SQLite migrate + seed load, all on a
/// 4-vCPU runner) can now genuinely run concurrently, and one of two
/// concurrent boots measurably exceeded 120 s on both attempts while sibling
/// tests booted normally — real contention, not a hang. 240 s keeps meaningful
/// headroom under `.config/nextest.toml`'s `slow-timeout` hard-kill ceiling
/// (`period = 60s, terminate-after = 5` => 300 s per attempt) for in-journey
/// polling after a slow-but-successful launch.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(240);

/// Default deadline for the convenience "find an element by aria-label /
/// button text, then act on it" helpers ([`E2eApp::click_by_aria_label`] and
/// friends). These poll for their target rather than doing a single
/// immediate `find` — see [`E2eApp::find_waiting`] for the CI race this
/// guards against. 20 s comfortably covers a debug-build route render on a
/// cold CI runner without masking a genuinely-absent element for long.
pub const DEFAULT_FIND_TIMEOUT: Duration = Duration::from_secs(20);

/// Budget for waits that depend on the ingest-resolution drain
/// (`apps/desktop/src-tauri/src/bootstrap/background.rs`,
/// `spawn_ingest_resolution_drain`).
///
/// That task is the ONLY caller of `backfill_session_targets` in the app —
/// there is no event-driven plan-applied listener for it — and its loop is
/// `sleep(30s)` FIRST, then resolve, then back-fill. A session's `targetIds`
/// therefore cannot populate until a drain tick lands, and ticks come every
/// 30 s starting 30 s after launch.
///
/// Waiting on that with a 30 s budget is a coin flip: the poll window and the
/// drain period are the SAME length, so whether a tick falls inside the window
/// depends on where setup happens to finish relative to the drain's phase.
/// That is what made `ingestion_sessions_search` flake (#1205) — it failed at
/// 155 s and passed on retry at 38 s, on the same commit.
///
/// 90 s guarantees at least two ticks inside the window regardless of phase.
///
/// Use this ONLY for predicates that gate on `targetIds` being populated.
/// Waits on `sessionKey`/`frameCount` observe session GROUPING, which is
/// event-driven and genuinely prompt — those must keep the shorter budget so
/// a real grouping regression still fails fast.
///
/// This is a test-side fix for a test-side race. It deliberately does NOT
/// change the 30 s drain interval, because that interval also sets the real
/// user-visible latency for target resolution after an ingest, and changing it
/// is a product decision (see #1205).
pub const DRAIN_BACKED_TIMEOUT: Duration = Duration::from_secs(90);

/// Deadline for a single `execute_async` script, set explicitly on the session
/// (#1205). Before this existed the suite silently inherited the driver's own
/// default — the W3C default is 30 s, which a legitimate IPC invoke can exceed
/// on a saturated Windows runner, producing a bare "Script execution timed out"
/// that names neither the command nor the budget it blew.
///
/// 90 s is chosen to sit *below* nextest's per-attempt hard kill (`period = 60s,
/// terminate-after = 5` => 300 s in `.config/nextest.toml`) so a script timeout
/// still fails as a readable test error rather than as a process kill, while
/// leaving room for several sequential invokes in one journey. Raising this
/// cannot mask a true hang: [`E2eApp::invoke`] names the in-flight command in
/// its error context, so a script that never calls back still fails loudly.
pub const SCRIPT_TIMEOUT: Duration = Duration::from_secs(90);

/// Deadline for a document navigation. `goto_route` is followed by an explicit
/// [`E2eApp::wait_bridge_ready`] poll, so this only needs to bound the raw
/// navigation itself.
pub const SCRIPT_TIMEOUT_PAGE_LOAD: Duration = Duration::from_secs(60);
