// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared harness for spec 037 Layer-2 real-UI E2E journeys.
//!
//! All journeys are real (WP-C) but `#[ignore]`d: they need the
//! `tauri-webdriver` CLI, a `desktop_shell --features e2e` build, and a
//! served frontend — none of which exist in the Layer-1 `cargo test
//! --workspace` job (ci.yml). The dedicated e2e.yml workflow runs them with
//! `--run-ignored all` after standing that environment up.
//!
//! # `slow_` test naming convention
//!
//! **Threshold**: prefix a test with `slow_` when its average wall time on
//! Windows CI exceeds 3× the suite median, measured on post-fix runs only.
//! With a current median of ~7s, the threshold is ~21s. Pre-fix timings are
//! invalid — they measure a bug, not the test. Re-evaluate the prefix after
//! any fix that materially changes a test's runtime.
//!
//! `e2e.yml`'s Windows shards use LPT (longest-processing-time first)
//! assignment: each `slow_` test is pinned to a separate shard, non-slow
//! tests fill the remaining slots by greedy LPT. The shard filter expressions
//! use `test(=slow_<name>)` with no `--partition` flag — nextest ANDs
//! `--partition` and `-E filterset`, which would exclude an explicitly named
//! test that doesn't hash to that bucket.
//!
//! Adding a new slow test: rename the function with `slow_`, measure its
//! average over 2+ post-fix CI runs, add a row to the timing table in
//! e2e.yml, re-run LPT, and update the shard `-E` expressions there.
//!
//! Mechanism (mirrors `.github/workflows/e2e.yml`, research D10):
//! - `desktop_shell` is built with `cargo build -p desktop_shell --features
//!   e2e`, which compiles in `tauri-plugin-webdriver` (Choochmeque) — an
//!   embedded W3C WebDriver server on loopback. Release builds omit the
//!   `e2e` feature so the automation surface is never present (Constitution
//!   Principle V).
//! - The `tauri-webdriver` CLI (`cargo install tauri-webdriver --locked`)
//!   proxies a loopback port -> the embedded plugin server on another, and
//!   manages the target app's process lifecycle via the `tauri:options`
//!   capability — it does **not** take the app binary as a CLI argument.
//!   Both ports are allocated per test PROCESS (each nextest test is its own
//!   process) rather than fixed at `:4444`/`:4445`, so concurrent journeys
//!   (`test-threads > 1`) never collide — see [`InstanceEnv`].
//! - thirtyfour (this crate's W3C client) connects to the CLI's proxy port and
//!   sends `tauri:options.application` = the built `desktop_shell` binary
//!   path in the New Session capabilities. No `browserName` is set (see
//!   `quickstart.md`).
//! - The app loads its own frontend from the Tauri `devUrl` ([`app_url`])
//!   automatically on launch, so the harness does **not** call
//!   `driver.goto(...)` after connecting. Export `PV_DEV_URL` to put concurrent
//!   local lanes on separate ports (issue #1409).
//! - `window.__PV_E2E__.invoke(...)` (exposed by the frontend when built
//!   with `VITE_E2E=1`, see `apps/desktop/src/main.tsx`) is the real-IPC
//!   invoke bridge used by [`E2eApp::invoke`].
//!
//! See `crates/e2e-tests/README.md` for the full run procedure.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use simbad_resolver::Cache as _;
use thirtyfour::components::escape_string;
use thirtyfour::prelude::*;

/// Pre-warmed resolve cache file, built once per process from the stripped
/// E2E seed (~200 entries). Copied into each [`InstanceEnv`]'s appdata dir
/// before launch so the app's `warm_bundled_on_first_run` sentinel check
/// immediately no-ops (~150ms) instead of warming the full 13k-row bundled
/// seed (~2-14s depending on platform/build profile).
///
/// The file is a real `simbad-cache.redb` produced by the same
/// `targeting_resolver::seed::warm_cache` + sentinel write the app uses —
/// byte-for-byte compatible with the production open path.
struct PrewarmedCache {
    /// Temp dir keeping the pre-warmed file alive for the process lifetime.
    _dir: tempfile::TempDir,
    /// Absolute path to the pre-warmed `.redb` file.
    path: PathBuf,
}

impl PrewarmedCache {
    /// Build a pre-warmed resolve cache from the stripped E2E seed.
    ///
    /// Spawns a dedicated OS thread so the inner `block_on` never races with
    /// the ambient tokio runtime that `#[tokio::test]` E2E tests run under
    /// (calling `block_on` from inside an async context panics).
    fn build() -> Result<Self> {
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
fn prewarmed_cache() -> &'static PrewarmedCache {
    static CACHE: OnceLock<PrewarmedCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        PrewarmedCache::build().expect(
            "failed to build the pre-warmed resolve cache — \
             E2E boot will fall back to full seed warm (slower but functional)",
        )
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
struct InstanceEnv {
    /// Kept alive for the process lifetime so the paths derived from it stay
    /// valid; never read directly.
    _root: tempfile::TempDir,
    /// Env vars to set (and transitively propagate through the
    /// `tauri-webdriver` CLI, which does not `env_clear()` its spawned
    /// `desktop_shell` child) so the app resolves its `app_data_dir`/
    /// `app_config_dir` (and, on Windows, `app_local_data_dir`) under this
    /// instance's isolated root instead of the shared real OS profile.
    vars: Vec<(&'static str, String)>,
    /// Isolated SQLite file this instance's app connects to (`PV_DB_URL`).
    db_path: PathBuf,
}

impl InstanceEnv {
    fn new() -> Result<Self> {
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
fn instance_env() -> &'static InstanceEnv {
    static ENV: OnceLock<InstanceEnv> = OnceLock::new();
    ENV.get_or_init(|| {
        InstanceEnv::new().expect(
            "failed to allocate an isolated E2E instance environment \
             (temp dir creation or ephemeral port binding failed)",
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
fn pick_port_pair() -> Result<(u16, u16)> {
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

/// Raw `tauri.conf.json` the app's `generate_context!` compiles in, embedded
/// here so this harness and the app cannot disagree about the default `devUrl`.
const TAURI_CONF_JSON: &str = include_str!("../../../../apps/desktop/src-tauri/tauri.conf.json");

/// Environment variable that overrides the app's `devUrl`, read by
/// `apps/desktop/src-tauri/src/lib.rs` (`desktop_shell::DEV_URL_ENV`).
const DEV_URL_ENV: &str = "PV_DEV_URL";

/// Vite dev-server / `vite preview` URL the app loads its frontend from:
/// `$PV_DEV_URL` when the lane sets one, else `build.devUrl` from
/// `tauri.conf.json`. The app resolves the same two sources in the same order,
/// so a lane that serves `dist` on its own port only has to export the var.
///
/// The app loads this URL on its own at launch — do NOT `driver.goto(app_url())`
/// after connecting. Kept for journeys that need to assert the current URL or
/// navigate within the SPA.
///
/// MUST be the `localhost` host form, byte-identical to `devUrl`: `localhost`
/// and `127.0.0.1` are DIFFERENT web origins with separate localStorage.
/// Navigating journeys to a `127.0.0.1` URL splits app state across two origins
/// — preferences written on one (e.g. `setupCompleted`,
/// `complete_first_run_gate`) are invisible on the other, which made `Shell`'s
/// localStorage-based setup gate and `SetupPage`'s backend-based check
/// ping-pong `/setup` ↔ `/inbox` indefinitely.
pub fn app_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| {
        if let Some(raw) = std::env::var_os(DEV_URL_ENV) {
            return raw.to_string_lossy().trim_end_matches('/').to_string();
        }
        let conf: serde_json::Value = serde_json::from_str(TAURI_CONF_JSON)
            .expect("apps/desktop/src-tauri/tauri.conf.json is not valid JSON");
        conf["build"]["devUrl"]
            .as_str()
            .expect("tauri.conf.json declares no string `build.devUrl`")
            .trim_end_matches('/')
            .to_string()
    })
}

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

// ---------------------------------------------------------------------------
// Private deserialization target for invoke() responses
// ---------------------------------------------------------------------------

/// Raw bridge response shape, using `Value` so no `T: Default` bound is needed.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvokeOutcome {
    ok: bool,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

impl InvokeOutcome {
    fn into_result<T: DeserializeOwned>(self) -> Result<T> {
        if self.ok {
            // Unit-returning commands (`Result<(), _>` — e.g.
            // `artifact_watcher_attach`) legitimately resolve with `null`/
            // `undefined`; `Option<Value>` deserialises JSON null to `None`, so
            // treat an absent value as `Value::Null` rather than an error.
            let raw = self.value.unwrap_or(Value::Null);
            serde_json::from_value(raw).context("failed to deserialise invoke value into T")
        } else {
            Err(anyhow!("invoke error: {}", self.error.unwrap_or_else(|| "unknown error".into())))
        }
    }
}

// ---------------------------------------------------------------------------
// E2eApp — the main harness handle
// ---------------------------------------------------------------------------

/// Handle for a running test app + WebDriver session.
///
/// Call [`E2eApp::launch`] to start, [`E2eApp::shutdown`] to tear down.
pub struct E2eApp {
    pub driver: WebDriver,
    driver_proc: Option<Child>,
    /// Proxy port this instance's `tauri-webdriver` CLI is listening on.
    /// Stored here (rather than read from the `InstanceEnv` singleton) because
    /// ports are re-picked on every [`E2eApp::launch_with`] call — the singleton
    /// no longer owns ports, so `Drop` and `shutdown` must use the port that was
    /// actually bound for this session.
    proxy_port: u16,
    /// Retained past launch so failures *after* a successful launch can still
    /// read it (#1204). [`drain_into`] threads keep filling these buffers for
    /// the session's lifetime, so this stays current rather than frozen at
    /// launch. Previously `launch_with` dropped it on the success path, which
    /// left the only Windows-side evidence of a broken webview session
    /// unreachable exactly when a bridge wait timed out.
    proc_log: ProcLog,
}

/// How much persisted state [`E2eApp::launch_with`] wipes before spawning
/// the app process. See [`E2eApp::launch`] vs [`E2eApp::relaunch`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum ResetScope {
    /// Wipe DB + webview storage + window-state (a fresh journey).
    Full,
    /// Wipe DB + window-state, but keep webview storage (localStorage) —
    /// simulates a real app restart within one journey.
    PreserveWebviewStorage,
}

impl E2eApp {
    /// Launch a full E2E session: preflight → reset DB → spawn the
    /// `tauri-webdriver` CLI proxy → create the WebDriver session with a
    /// deadline-bounded retry loop.
    ///
    /// Why the retry loop (CI evidence: run 28694907445, ubuntu):
    /// `desktop_shell` initialises the webdriver **plugin** in `build_app()`
    /// but only creates its **window** when `run_app()` starts the event loop
    /// — after DB connect, migrations, and the ~13k-row bundled-seed load
    /// (`apps/desktop/src-tauri/src/main.rs`). A debug build on a CI runner
    /// spends tens of seconds in that gap. The `tauri-webdriver` CLI's
    /// session handler only waits for the plugin *port* (30 s), then forwards
    /// session-create to the plugin, whose own window-wait is 10 s — so a
    /// slow boot yields `no such window` (404) even though the app is healthy
    /// and seconds away from ready.
    ///
    /// The CLI (`tauri-webdriver` 0.1.1, `src/server.rs::handle_plugin`)
    /// kills any prior app instance and relaunches on every `POST /session`
    /// whose capabilities carry a `tauri:options.application` value — and an
    /// **empty string still counts**: `extract_app_path` returns
    /// `Some("".into())`, so the CLI kills the booting app and then fails
    /// `Command::new("")` with ENOENT ("Failed to launch Tauri app: No such
    /// file or directory", CI run 28695295960). The only no-relaunch path is
    /// to omit `tauri:options` entirely (`extract_app_path` → `None`), which
    /// forwards the session-create straight to the plugin in the running
    /// app. So: attempt 1 sends the real path (launch); retries send **no
    /// `tauri:options` at all** (reuse the booting instance) until the
    /// window exists or [`LAUNCH_TIMEOUT`] elapses. Connection-level errors
    /// (`RequestFailed`) mean the CLI never received the POST — the app was
    /// not launched — so the real path is kept for the next attempt.
    ///
    /// The app auto-loads its frontend from the Tauri `devUrl` on launch, so
    /// no `driver.goto(...)` call is needed here (see module docs).
    pub async fn launch() -> Result<Self> {
        Self::launch_with(ResetScope::Full).await
    }

    /// Simulate a real app restart WITHIN one journey: a fresh WebDriver
    /// session + a fresh `desktop_shell` process, but WITHOUT wiping the
    /// webview's persisted web storage (localStorage & co).
    ///
    /// [`Self::launch`] always calls `reset_webview_storage()` so that
    /// state set by one journey (test function) can't leak into the NEXT
    /// journey's fresh [`Self::launch`] — those are different real OS user
    /// profiles' worth of isolation, correctly enforced. But a journey that
    /// wants to prove something actually SURVIVES a real app relaunch (e.g.
    /// `settings_journeys.rs`'s theme persistence test) must call this
    /// instead of `launch()` for its second call: calling `launch()` again
    /// wipes the very localStorage state the journey is trying to prove
    /// persisted, which is a harness bug, not a product one.
    ///
    /// That was a Windows-only symptom, and since #1204 it is Windows-only
    /// for a different reason. The old note here said the `EBWebView` path
    /// was the one that "actually deletes real localStorage files" — it was
    /// not: it pointed under the isolated `LOCALAPPDATA`, which no Known
    /// Folder lookup honours, so it deleted nothing either. Windows storage
    /// is now genuinely wiped, via the absolute `WEBVIEW2_USER_DATA_FOLDER`
    /// this harness sets.
    ///
    /// The Linux `localstorage`/`storage` paths still do not match
    /// WebKitGTK's real storage location, so that branch remains a no-op —
    /// pre-existing, and left alone here deliberately rather than fixed
    /// blind alongside a Windows change.
    ///
    /// Still resets the database and window-state store (same as `launch()`)
    /// — those are unrelated to the webview storage this exists to preserve,
    /// and journeys that use this (see `settings_journeys.rs`) already expect
    /// a fresh DB / first-run gate after "relaunching".
    pub async fn relaunch() -> Result<Self> {
        Self::launch_with(ResetScope::PreserveWebviewStorage).await
    }

    async fn launch_with(scope: ResetScope) -> Result<Self> {
        preflight()?;
        let env = instance_env();
        reset_database(&env.db_path)?;
        if matches!(scope, ResetScope::Full) {
            reset_webview_storage(&env.vars);
        }
        reset_window_state(&env.vars);

        let app_binary = app_binary_path()?;

        // Port-rebind retry: pick fresh ports on every attempt so a relaunch
        // never reuses a port that was freed by the preceding shutdown and
        // grabbed by another process in the TOCTOU window between
        // `pick_port_pair`'s listener drop and `tauri-webdriver`'s own bind.
        //
        // Up to PORT_REBIND_ATTEMPTS are made; each picks a brand-new port
        // pair. An early `tauri-webdriver` exit (detected via `try_wait`) is
        // the tell-tale sign that the CLI failed to bind its port — on that
        // signal we kill the process, re-pick, and retry immediately rather
        // than burning the full LAUNCH_TIMEOUT.
        const PORT_REBIND_ATTEMPTS: u32 = 3;
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 1..=PORT_REBIND_ATTEMPTS {
            let (proxy_port, native_port) = pick_port_pair().with_context(|| {
                format!("failed to pick ephemeral port pair on attempt {attempt}")
            })?;
            let webdriver_url = format!("http://127.0.0.1:{proxy_port}");

            let (mut driver_proc, proc_log) = spawn_tauri_webdriver(env, proxy_port, native_port)
                .with_context(|| {
                format!(
                    "failed to spawn the tauri-webdriver CLI on port {proxy_port} \
                         (attempt {attempt})"
                )
            })?;

            let deadline = Instant::now() + LAUNCH_TIMEOUT;
            let mut launched = false;

            let session_result: Result<WebDriver> = loop {
                // Check whether `tauri-webdriver` exited early — this is the
                // port-bind-failure signal. The CLI exits immediately if it
                // cannot bind `proxy_port` (e.g. another process stole the port
                // in the TOCTOU gap). Detecting this early avoids waiting the
                // full LAUNCH_TIMEOUT before re-picking.
                match driver_proc.try_wait() {
                    Ok(Some(status)) => {
                        break Err(anyhow::anyhow!(
                            "tauri-webdriver exited early with {status} on attempt {attempt} \
                             (proxy_port={proxy_port}, native_port={native_port}); \
                             likely port-bind failure — will retry with fresh ports\n{}",
                            proc_log.dump()
                        ));
                    }
                    Ok(None) => {} // still running, proceed
                    Err(e) => {
                        // `try_wait` failing is unusual but non-fatal here; log
                        // and continue — we will discover the exit via the
                        // WebDriver error path below.
                        let _ = e;
                    }
                }

                let mut caps = Capabilities::new();
                if !launched {
                    // Only the launching attempt may carry tauri:options: the
                    // CLI treats ANY present `application` value (even "") as
                    // "kill the current app and relaunch". Retries must omit
                    // the key so the POST is forwarded to the already-booting
                    // instance.
                    if let Err(e) = caps.set(
                        "tauri:options",
                        json!({ "application": app_binary.to_string_lossy() }),
                    ) {
                        kill_driver_proc(&mut driver_proc);
                        return Err(e)
                            .context("failed to set the tauri:options.application capability");
                    }
                }

                match WebDriver::new(&webdriver_url, caps).await {
                    Ok(driver) => break Ok(driver),
                    Err(e) => {
                        // Any typed WebDriver response means the CLI handled
                        // the POST — and therefore already spawned the app
                        // process. Only a transport-level RequestFailed means
                        // it didn't.
                        use thirtyfour::error::WebDriverErrorInner;
                        if !matches!(e.as_inner(), WebDriverErrorInner::RequestFailed(_)) {
                            launched = true;
                        }
                        if Instant::now() >= deadline {
                            // Ask the CLI to kill the app it launched (any
                            // DELETE /session/{id} triggers that), then kill
                            // the CLI itself — otherwise the leaked pair holds
                            // this instance's ports and poisons every later
                            // launch sharing this process (exactly what CI's
                            // TRY-2 "can not listen to address" failure was,
                            // back when ports were fixed at 4444/4445).
                            blocking_session_delete(proxy_port);
                            kill_driver_proc(&mut driver_proc);
                            break Err(anyhow::Error::new(e).context(format!(
                                "WebDriver session not created within {LAUNCH_TIMEOUT:?} \
                                 against {webdriver_url} (attempt {attempt}) — is \
                                 `tauri-webdriver` running, and was {} built with \
                                 `--features e2e`?\n{}",
                                app_binary.display(),
                                proc_log.dump()
                            )));
                        }
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                    }
                }
            };

            let driver = match session_result {
                Ok(d) => d,
                Err(e) => {
                    // Kill any still-running CLI before re-picking ports.
                    blocking_session_delete(proxy_port);
                    kill_driver_proc(&mut driver_proc);
                    last_err = Some(e);
                    if attempt < PORT_REBIND_ATTEMPTS {
                        // Brief pause so the OS reclaims the port before the
                        // next pick.
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    continue;
                }
            };

            // Set the script timeout EXPLICITLY (#1205). Until this call
            // existed, every `execute_async` inherited whatever default the
            // driver happened to use (W3C says 30s) — never a deliberate
            // choice. A legitimate IPC invoke on a loaded Windows runner can
            // exceed 30s, which surfaces as a bare "Script execution timed out"
            // with no indication of which command was in flight.
            //
            // This does NOT hide a genuine hang: `invoke` names the command in
            // its error context, so a script that never calls back still fails
            // — it just fails at a budget we chose, naming the culprit, instead
            // of at an undocumented default anonymously.
            // Argument order is (script, page_load, implicit) — NOT the
            // page-load-first order the name ordering might suggest. Passing
            // these reversed silently swaps the two budgets and still compiles,
            // so keep the labels below when editing.
            let timeouts = TimeoutConfiguration::new(
                /* script */ Some(SCRIPT_TIMEOUT),
                /* page_load */ Some(SCRIPT_TIMEOUT_PAGE_LOAD),
                // Implicit wait stays ZERO: every wait in this harness is an
                // explicit poll loop, and thirtyfour's own default notes that
                // ElementQuery requires zero. A non-zero implicit wait would
                // silently stack on top of those and inflate every negative
                // assertion.
                /* implicit */
                Some(Duration::from_secs(0)),
            );
            if let Err(e) = driver.update_timeouts(timeouts).await {
                blocking_session_delete(proxy_port);
                kill_driver_proc(&mut driver_proc);
                return Err(e).context("failed to set explicit WebDriver timeouts");
            }

            // The plugin binds a new session to
            // `webview_windows().keys().first()`
            // (`tauri-plugin-webdriver-0.2.1/src/server/handlers/session.rs:24`)
            // — a `HashMap` key order, and the splash window now exists BEFORE
            // `main` does (the app builds `main` only after migrations). Without
            // an explicit switch the session can hold the splash, whose document
            // has no `__PV_E2E__` bridge, and every journey would fail in
            // `wait_bridge_ready` with no indication why.
            if let Err(e) = Self::switch_to_main_window(&driver, deadline).await {
                blocking_session_delete(proxy_port);
                kill_driver_proc(&mut driver_proc);
                return Err(e).context(proc_log.dump());
            }

            return Ok(Self { driver, driver_proc: Some(driver_proc), proxy_port, proc_log });
        }

        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("launch failed after {PORT_REBIND_ATTEMPTS} port-rebind attempts")
        }))
    }

    /// Bind the session to the `main` window, waiting for the app to create it.
    ///
    /// The plugin's window handles ARE the Tauri window labels
    /// (`webview_windows().keys()`), so `main` is matched by name, not by
    /// position.
    async fn switch_to_main_window(driver: &WebDriver, deadline: Instant) -> Result<()> {
        let deadline = deadline.max(Instant::now() + Duration::from_secs(60));
        let main = WindowHandle::from("main");
        loop {
            let handles = driver.windows().await.unwrap_or_default();
            if handles.contains(&main) {
                return driver
                    .switch_to_window(main)
                    .await
                    .context("failed to switch the session to the `main` window");
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "the app never created its `main` window; handles seen: {handles:?}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Resize the real OS window and return the viewport actually ACHIEVED,
    /// which may be smaller than requested.
    ///
    /// Needed because layout-dependent behaviour cannot be asserted at the
    /// default window size: `tauri.conf.json` opens at 1280x820, while the
    /// side dock only engages at `window.innerWidth >= 1400`
    /// (`useAdaptiveDock.ts`'s `threshold`). A journey that wants the docked
    /// layout has to ask for it.
    ///
    /// **Not a one-line `set_window_rect`.** That call sizes the OUTER
    /// window (frame included), but every threshold in the app keys off
    /// `window.innerWidth`. The two differ by the window chrome, which is
    /// not a constant we can hardcode: ~0 under `xvfb-run` (no window
    /// manager, so no decorations) but real on Windows. Passing 1400 blind
    /// would yield innerWidth 1400 on Linux and something smaller on
    /// Windows. So: set, MEASURE, correct by the observed delta.
    ///
    /// **Best-effort, deliberately not an assertion.** GitHub-hosted
    /// **Windows runners are fixed at 1024x768 and cannot be resized** — the
    /// runner service runs in non-interactive Session 0 with no real display
    /// attached, so `ChangeDisplaySettings` has nothing to act on
    /// (actions/runner-images#2935, #8606). Hard-failing on a short request
    /// would make every docked-layout journey Linux-only by construction.
    /// Callers needing a minimum must assert on the return value — better
    /// still, avoid depending on one, as
    /// `targets_ui_identity_columns_stay_pinned_while_table_scrolls` does by
    /// pinning the dock rather than relying on the width threshold.
    ///
    /// Convergence is capped rather than looped-until-stable: a request
    /// exceeding the screen is clamped by the OS and would otherwise spin.
    pub async fn set_viewport(&self, target_w: u32, target_h: u32) -> Result<(i64, i64)> {
        const ATTEMPTS: usize = 4;
        let (screen_w, screen_h) = self.screen_size().await.unwrap_or((-1, -1));
        // Ask for no more than the screen can hold: anything larger is
        // clamped anyway, and requesting it only wastes attempts.
        let tw = if screen_w > 0 { i64::from(target_w).min(screen_w) } else { i64::from(target_w) };
        let th = if screen_h > 0 { i64::from(target_h).min(screen_h) } else { i64::from(target_h) };
        let (mut outer_w, mut outer_h) = (tw, th);
        let mut last = (0, 0);

        for _ in 0..ATTEMPTS {
            self.driver
                .set_window_rect(0, 0, outer_w.max(1) as u32, outer_h.max(1) as u32)
                .await
                .with_context(|| format!("set_window_rect to {outer_w}x{outer_h} failed"))?;

            let (inner_w, inner_h) = self.inner_size().await?;
            last = (inner_w, inner_h);
            if inner_w == tw && inner_h == th {
                return Ok(last);
            }

            // A non-positive reading means the webview reported no viewport at
            // all (not laid out yet, or `innerWidth` came back 0/-1) — not that
            // the window is too small. Correcting by `target - 0` would then ADD
            // the full target every pass (1400 -> 2800 -> 4200 -> 5600), blowing
            // the window far past the screen and leaving content so wide that
            // overflow-dependent journeys can never trip. Observed on Ubuntu CI:
            // a 5380px client width and a reported 0x0 viewport. Stop and report
            // what we last saw rather than diverging.
            if inner_w <= 0 || inner_h <= 0 {
                return Ok(last);
            }

            // Never ask for more than the screen can show, for the same reason.
            outer_w = (outer_w + tw - inner_w).min(if screen_w > 0 { screen_w } else { i64::MAX });
            outer_h = (outer_h + th - inner_h).min(if screen_h > 0 { screen_h } else { i64::MAX });
        }
        Ok(last)
    }

    /// Seed a persisted app preference BEFORE the frontend reads it, then
    /// reload so the read is genuinely cold.
    ///
    /// The reload is not optional. `data/preferences.ts` memoises into a
    /// module-level `cachedPreferences` on first read, so writing
    /// localStorage into an already-booted page changes nothing the app will
    /// ever look at. Only a real reload drops that cache — which also makes
    /// this the one path that exercises the cold read.
    pub async fn seed_preference(&self, key: &str, value_json: &str) -> Result<()> {
        let script = format!(
            "var k = 'alm-preferences';\
             var cur = {{}};\
             try {{ cur = JSON.parse(localStorage.getItem(k)) || {{}}; }} catch (e) {{ cur = {{}}; }}\
             cur[{}] = {};\
             localStorage.setItem(k, JSON.stringify(cur));\
             return localStorage.getItem(k);",
            escape_string(key),
            value_json
        );
        self.driver
            .execute(&script, vec![])
            .await
            .with_context(|| format!("failed to seed the {key:?} preference"))?;
        self.driver.refresh().await.context("failed to reload after seeding a preference")?;
        Ok(())
    }

    /// `window.innerWidth`/`innerHeight` — the viewport the app's own
    /// breakpoints see, as opposed to the OS window `set_window_rect` sets.
    async fn inner_size(&self) -> Result<(i64, i64)> {
        let v: Value = self
            .driver
            .execute("return [window.innerWidth, window.innerHeight];", vec![])
            .await
            .context("failed to read window.innerWidth/innerHeight")?
            .convert()
            .context("innerWidth/innerHeight were not a JSON array")?;
        let get = |i: usize| v.get(i).and_then(Value::as_i64).unwrap_or(-1);
        Ok((get(0), get(1)))
    }

    /// Physical screen size, used only to explain a failed resize.
    async fn screen_size(&self) -> Result<(i64, i64)> {
        let v: Value = self
            .driver
            .execute("return [screen.width, screen.height];", vec![])
            .await
            .context("failed to read screen.width/height")?
            .convert()
            .context("screen.width/height were not a JSON array")?;
        let get = |i: usize| v.get(i).and_then(Value::as_i64).unwrap_or(-1);
        Ok((get(0), get(1)))
    }

    /// Issue a Tauri command through the `window.__PV_E2E__` bridge.
    ///
    /// The bridge is exposed by the desktop app when it is built with
    /// `VITE_E2E=1` (see `apps/desktop/src/main.tsx`). This replaces the old
    /// better-sqlite3 reader approach: instead of reading the DB directly, we
    /// assert UI→real-backend round-trips against real command output
    /// (FR-008).
    ///
    /// The injected WebDriver callback is the last script argument
    /// (`arguments[arguments.length-1]`); the bridge resolves it with
    /// `{ok:true,value}` or `{ok:false,error}`.
    pub async fn invoke<T: DeserializeOwned>(&self, command: &str, args: Value) -> Result<T> {
        let script = r#"
            var cmd      = arguments[0];
            var cmdArgs  = arguments[1];
            var callback = arguments[arguments.length - 1];
            if (!window.__PV_E2E__ || typeof window.__PV_E2E__.invoke !== 'function') {
                callback({ ok: false, error: '__PV_E2E__ bridge missing (build with VITE_E2E=1)' });
                return;
            }
            window.__PV_E2E__.invoke(cmd, cmdArgs).then(function(value) {
                callback({ ok: true, value: value });
            }).catch(function(err) {
                // `unwrap()` (`apps/desktop/src/api/ipc.ts`) throws the raw
                // `ContractError` envelope object on a rejected command, not
                // a JS `Error` instance — `String(err)` on a plain object
                // stringifies to the useless "[object Object]" (round 4,
                // #470: masked a real `no_link_kind` backend error behind
                // that placeholder). Prefer JSON.stringify so `code`/
                // `message`/`details` are readable; fall back to
                // `err.message`/`String(err)` only if JSON serialisation
                // itself fails or yields nothing useful (e.g. a real `Error`
                // instance, whose own fields aren't enumerable).
                var serialized;
                try {
                    serialized = JSON.stringify(err);
                } catch (jsonErr) {
                    serialized = null;
                }
                if (!serialized || serialized === '{}') {
                    serialized = (err && err.message) ? String(err.message) : String(err);
                }
                callback({ ok: false, error: serialized });
            });
        "#;

        let ret = self
            .driver
            .execute_async(script, vec![json!(command), args])
            .await
            // Name the command (#1205). This used to be a bare
            // "execute_async failed", so a script timeout told us nothing about
            // WHICH invoke never called back — the CI log was undiagnosable.
            // With the command named, raising SCRIPT_TIMEOUT stays safe: a
            // genuine hang still fails, and now says what hung.
            .with_context(|| {
                format!(
                    "execute_async failed for command {command:?} \
                     (script timeout is {SCRIPT_TIMEOUT:?}); a timeout here means the \
                     bridge never invoked the WebDriver callback for that command"
                )
            })?;

        let outcome: InvokeOutcome = ret.convert().with_context(|| {
            format!("failed to deserialise InvokeOutcome from bridge response for {command:?}")
        })?;

        outcome.into_result::<T>()
    }

    /// Poll a command through the `invoke` bridge until `predicate` accepts the
    /// deserialised value or `timeout` elapses.
    ///
    /// Several real backend effects in this app are event-driven rather than
    /// synchronous with the triggering call (e.g. the inbox plan-apply listener
    /// creates `acquisition_session` rows asynchronously after a plan-applied
    /// event, and the artifact watcher's reconciliation pass runs on its own
    /// task). Polling a real read command until the expected state appears is
    /// the wait primitive for those cases — never a blind `sleep`.
    ///
    /// # Errors
    /// Returns the last error (invoke failure, or a "predicate never matched"
    /// message once `timeout` elapses) if the predicate never accepts a
    /// value. The timeout variant includes the last successfully-decoded
    /// (but non-matching) response, truncated, so a caller can see whether
    /// the backend returned an empty/unrelated result or the expected data
    /// present-but-unmatched by the predicate (a predicate bug) without a
    /// second CI round just to add that dump.
    pub async fn invoke_until<T, P>(
        &self,
        command: &str,
        args: Value,
        timeout: Duration,
        mut predicate: P,
    ) -> Result<T>
    where
        T: DeserializeOwned + std::fmt::Debug,
        P: FnMut(&T) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut last_err: Option<anyhow::Error> = None;
        let mut last_value: Option<String> = None;
        loop {
            match self.invoke::<T>(command, args.clone()).await {
                Ok(value) if predicate(&value) => return Ok(value),
                Ok(value) => {
                    let dump = format!("{value:?}");
                    last_value = Some(if dump.len() > 4096 {
                        format!("{}...[truncated]", &dump[..4096])
                    } else {
                        dump
                    });
                }
                Err(e) => last_err = Some(e),
            }
            if Instant::now() >= deadline {
                return Err(last_err.unwrap_or_else(|| match &last_value {
                    Some(v) => anyhow!(
                        "invoke_until({command}) timed out after {:?} without a matching \
                         value; last response: {v}",
                        timeout
                    ),
                    None => anyhow!(
                        "invoke_until({command}) timed out after {:?} without a matching value \
                         (never returned successfully)",
                        timeout
                    ),
                }));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Navigate to a top-level SPA route and wait for the shell to settle.
    ///
    /// The router uses HASH history (`createHashHistory()`,
    /// `apps/desktop/src/app/router.tsx`): routes live in the URL fragment
    /// (`/#/inbox`) and the pathname is ignored entirely. Navigating to
    /// `{app_url()}{path}` therefore always lands on the index route `/`,
    /// whose first-run gate redirects a fresh DB to `/setup` — the target
    /// page never mounts (CI run 28751553798: Inbox's "Rescan all roots"
    /// deterministically never appeared on all three OSes). Navigate to the
    /// hash form instead. Waits for `document.readyState == "complete"`
    /// instead of a fixed sleep.
    /// The navigation is VERIFIED: several app-level redirects can move the
    /// page away from the requested route right after landing (the Shell
    /// redirects everything to `/setup` while the `setupCompleted` preference
    /// is false, `SetupPage` bounces to `/inbox` once setup completes, the
    /// index gate redirects asynchronously). Retry until the URL actually
    /// stays on the target route, and fail with the URL it kept landing on —
    /// far more diagnosable in CI than a downstream "element never appeared".
    pub async fn goto_route(&self, path: &str) -> Result<()> {
        let url = format!("{}/#{path}", app_url());
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut last = String::new();
        loop {
            self.driver.goto(&url).await.with_context(|| format!("goto {url} failed"))?;
            self.wait_document_ready(Duration::from_secs(10)).await?;

            // Wait for the URL to land on the target, then confirm it STAYS
            // there (a late-resolving redirect can still yank it away).
            if self.wait_url_contains(path, Duration::from_secs(3)).await.is_ok() {
                tokio::time::sleep(Duration::from_millis(700)).await;
                let current =
                    self.driver.current_url().await.context("failed to read current_url")?;
                last = current.to_string();
                if last.contains(path) {
                    return Ok(());
                }
            } else if let Ok(current) = self.driver.current_url().await {
                last = current.to_string();
            }

            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "route {path} did not stick within 20s — the app kept redirecting \
                     away (last URL: {last}); is the first-run gate complete \
                     (E2eApp::complete_first_run_gate)?"
                ));
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    /// Poll `document.readyState` until `"complete"` or `timeout` elapses.
    pub async fn wait_document_ready(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let state: String = self
                .driver
                .execute("return document.readyState", vec![])
                .await
                .context("failed to read document.readyState")?
                .convert()
                .context("failed to deserialise document.readyState")?;
            if state == "complete" {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(anyhow!("document.readyState never reached 'complete'"));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Poll `current_url()` until it contains `needle` or `timeout` elapses.
    ///
    /// The index route's first-run gate (`apps/desktop/src/app/router.tsx`)
    /// redirects to `/setup` from an **async** `beforeLoad`:
    /// `checkFirstRunComplete` does a dynamic `import('@/bindings/index')` plus a
    /// `firstrun_state` IPC round-trip, so the redirect lands slightly *after*
    /// the page's `__PV_E2E__` bridge becomes ready. Asserting the URL the
    /// instant `wait_bridge_ready` returns races that redirect — poll for it.
    pub async fn wait_url_contains(&self, needle: &str, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            let url = self.driver.current_url().await.context("failed to read current_url")?;
            let current = url.to_string();
            if current.contains(needle) {
                return Ok(current);
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "URL never contained {needle:?} within {timeout:?} (last: {current})"
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// `true` once `window.__PV_E2E__.invoke` exists — a real signal that
    /// `main.tsx` finished its top-level module evaluation for the current
    /// page load (used instead of a blind sleep after `goto_route`).
    pub async fn bridge_ready(&self) -> Result<bool> {
        let script = r"
            return !!(window.__PV_E2E__ && typeof window.__PV_E2E__.invoke === 'function');
        ";
        let ret =
            self.driver.execute(script, vec![]).await.context("bridge_ready script failed")?;
        ret.convert::<bool>().context("failed to deserialise bridge_ready result")
    }

    /// Page state captured when a wait times out (#1204, #1272).
    ///
    /// Returns a human-readable one-liner and never fails: this runs on an
    /// already-failing path, so a diagnostic that could itself error would
    /// replace the real failure with its own.
    ///
    /// The name predates its second caller — [`Self::wait_testid`] and
    /// [`Self::wait_testid_enabled`] use it too, since "the element never
    /// appeared" needs exactly the same questions answered: what route are we
    /// on, did the page finish loading, is an error boundary showing, and did
    /// anything render at all.
    async fn bridge_failure_context(&self) -> String {
        let url = match self.driver.current_url().await {
            Ok(u) => u.to_string(),
            Err(e) => format!("<current_url failed: {e}>"),
        };

        // One script, so a dying session yields one error rather than four.
        //
        // `presentTestids` is the decisive datum for a testid wait (#1272):
        // "project-row-<id> never appeared" is ambiguous on its own, but the
        // list of testids that ARE present separates "wrong route", "route
        // rendered but list empty" and "nothing rendered at all" immediately.
        // Capped at 40 and truncated so a large DOM cannot bury the failure.
        let probe = r#"
            var boundary = document.querySelector('[data-testid="app-error-boundary-fallback"]');
            var ids = Array.prototype.slice
                .call(document.querySelectorAll('[data-testid]'), 0, 40)
                .map(function (el) { return el.getAttribute('data-testid'); });
            return JSON.stringify({
                readyState: document.readyState,
                hasBridge:  !!window.__PV_E2E__,
                bridgeKeys: window.__PV_E2E__ ? Object.keys(window.__PV_E2E__) : [],
                errorBoundary: boundary ? (boundary.innerText || '').slice(0, 300) : null,
                bodyChars: document.body ? document.body.innerHTML.length : 0,
                testidCount: document.querySelectorAll('[data-testid]').length,
                presentTestids: ids
            });
        "#;
        let page = match self.driver.execute(probe, vec![]).await {
            Ok(ret) => {
                ret.convert::<String>().unwrap_or_else(|e| format!("<undeserialisable: {e}>"))
            }
            Err(e) => format!("<probe script failed: {e}>"),
        };

        format!("url={url}; page={page}")
    }

    /// Wait for [`Self::bridge_ready`] to become `true`.
    pub async fn wait_bridge_ready(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        // Retain the last probe error (#1204). This loop used to call
        // `.unwrap_or(false)`, which DISCARDED the underlying WebDriver error on
        // every iteration — so a dead session or a crashed page spun silently to
        // the deadline and reported the generic "never became ready", throwing
        // away the actual cause each time round.
        let mut last_err: Option<String>;
        loop {
            match self.bridge_ready().await {
                Ok(true) => return Ok(()),
                Ok(false) => last_err = None,
                Err(e) => last_err = Some(format!("{e:#}")),
            }
            if Instant::now() >= deadline {
                let probed = self.bridge_failure_context().await;
                let cause = last_err.map_or_else(
                    || "no probe error — the bridge simply never appeared".to_owned(),
                    |e| format!("last probe error: {e}"),
                );
                // The in-page probe above can only speak if the webview session
                // is alive enough to evaluate script — and #1204's signature is
                // precisely that it is not. The driver/app log is the one
                // channel that does not depend on the faulty session, so dump
                // it here rather than only on a launch failure.
                return Err(anyhow!(
                    "window.__PV_E2E__ bridge never became ready within {timeout:?}; \
                     {cause}; {probed}\n{}",
                    self.proc_log.dump()
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// `true` when the shared `AppErrorBoundary` fallback
    /// (`[data-testid="app-error-boundary-fallback"]`, `apps/desktop/src/app/AppErrorBoundary.tsx`)
    /// is present in the DOM — the real, shipped signal that a route's
    /// component tree threw an uncaught render error (FR-007).
    pub async fn error_boundary_visible(&self) -> Result<bool> {
        use thirtyfour::error::WebDriverErrorInner;

        match self.driver.find(By::Css("[data-testid='app-error-boundary-fallback']")).await {
            Ok(_) => Ok(true),
            Err(e) if matches!(e.as_inner(), WebDriverErrorInner::NoSuchElement(_)) => Ok(false),
            Err(e) => Err(e).context("failed to query for the error boundary fallback"),
        }
    }

    // ---------------------------------------------------------------------
    // Failure-site diagnostics (fix-lane round 5, PR #477): purely
    // best-effort evidence-gathering for a failing journey's error message —
    // never used for assertions. Each dump degrades to an inline error string
    // rather than propagating, so a diagnostic failing never masks the real
    // assertion failure it was called from.
    // ---------------------------------------------------------------------

    /// Dump DOM + TanStack Query + buffered-error evidence for a failing
    /// real-UI journey, deciding between the three live hypotheses for the
    /// Windows-only `inbox_ui_mixed_folder_splits_into_single_type_items`
    /// "found 0 rows" failure (round 3/4 narrowed it to: real webview only):
    ///
    /// - (a) UI IPC channel error/race: `queryState` (status/fetchStatus/
    ///   error/dataUpdatedAt/fetchFailureCount) for the `['inbox','all']`
    ///   query key (`apps/desktop/src/features/inbox/store.ts`), plus
    ///   `e2eErrors` — uncaught `error`/`unhandledrejection` events buffered by
    ///   the `VITE_E2E` listener installed in `apps/desktop/src/main.tsx`. If
    ///   the query never reaches `status: "success"` with `dataUpdatedAt > 0`,
    ///   or `e2eErrors` is non-empty, the UI's own IPC channel is implicated
    ///   rather than the backend (which round-3 already proved returns the
    ///   right rows via the diagnostic-only invoke bridge).
    /// - (b) layout/virtualizer race: `containerFound` / `containerRectHeight`
    ///   / `rowCount` / `containerOuterHtml` (truncated) for the
    ///   `[data-testid="inbox-virtual-sizer"]` scroll viewport
    ///   (`apps/desktop/src/ui/Table.tsx`'s virtualizer measures this
    ///   element) — a 0-height container with `rowCount: 0` but a non-empty,
    ///   well-formed `containerOuterHtml` (e.g. spacer rows present) points at
    ///   the virtualizer, not the query layer.
    /// - (c) stale frontend artifact: `buildTime`, baked in at Vite
    ///   config-eval time via the `VITE_BUILD_TIME` define
    ///   (`apps/desktop/vite.config.ts`) — compare against the CI job's wall
    ///   clock in the run this dump came from.
    ///
    /// Returns a single JSON object; a field-level failure (bridge not
    /// exposed, container missing, query client absent) becomes a `null` /
    /// error-string value in that field rather than an `Err` for the whole
    /// call, so partial evidence is never lost to an all-or-nothing dump.
    pub async fn dump_ui_diagnostics(&self) -> Value {
        let script = r#"
            var callback = arguments[arguments.length - 1];
            function truncate(s, n) {
                if (typeof s !== 'string') return s;
                return s.length > n ? s.slice(0, n) + '...[truncated]' : s;
            }
            try {
                var container = document.querySelector('[data-testid="inbox-virtual-sizer"]');
                var rows = document.querySelectorAll('[data-testid^="inbox-item-"]');
                var rect = container ? container.getBoundingClientRect() : null;
                var e2e = window.__PV_E2E__;
                var queryState = null;
                if (e2e && e2e.queryClient) {
                    try {
                        var s = e2e.queryClient.getQueryState(['inbox', 'all']);
                        if (s) {
                            queryState = {
                                status: s.status,
                                fetchStatus: s.fetchStatus,
                                error: s.error ? String(s.error.message || s.error) : null,
                                dataUpdatedAt: s.dataUpdatedAt,
                                errorUpdatedAt: s.errorUpdatedAt,
                                fetchFailureCount: s.fetchFailureCount,
                                dataLength: Array.isArray(s.data) ? s.data.length : null
                            };
                        }
                    } catch (qerr) {
                        queryState = { queryStateError: String(qerr) };
                    }
                }
                callback({
                    ok: true,
                    value: {
                        bridgeExposed: !!e2e,
                        buildTime: e2e ? e2e.buildTime : null,
                        documentReadyState: document.readyState,
                        containerFound: !!container,
                        containerRectHeight: rect ? rect.height : null,
                        rowCount: rows.length,
                        containerOuterHtml: truncate(container ? container.outerHTML : null, 4096),
                        queryState: queryState,
                        e2eErrors: (window.__e2eErrors || []).slice(-30)
                    }
                });
            } catch (err) {
                callback({ ok: false, error: String(err) });
            }
        "#;

        match self.driver.execute_async(script, vec![]).await {
            Ok(ret) => ret
                .convert::<Value>()
                .unwrap_or_else(|e| json!({ "dump_ui_diagnostics_decode_error": e.to_string() })),
            Err(e) => json!({ "dump_ui_diagnostics_execute_error": e.to_string() }),
        }
    }

    /// Generic evidence dump for a failing journey centred on ONE
    /// `data-testid` element (unlike `dump_ui_diagnostics`, which is
    /// hardcoded to the Inbox virtualizer/query-key investigation) — e.g. a
    /// dialog/modal that should have closed after a submit action but is
    /// still present. Captures whether the element is still in the DOM, its
    /// (truncated) `outerHTML` — including any inline error banner it may be
    /// showing — and the buffered `window.__e2eErrors` (uncaught
    /// `error`/`unhandledrejection` events, `VITE_E2E` listener installed in
    /// `apps/desktop/src/main.tsx`). Never used for assertions; a failure at
    /// any step degrades to an inline error string rather than propagating.
    pub async fn dump_testid_diagnostics(&self, testid: &str) -> Value {
        let script = format!(
            r#"
            var callback = arguments[arguments.length - 1];
            function truncate(s, n) {{
                if (typeof s !== 'string') return s;
                return s.length > n ? s.slice(0, n) + '...[truncated]' : s;
            }}
            try {{
                var el = document.querySelector('[data-testid="{testid}"]');
                callback({{
                    ok: true,
                    value: {{
                        found: !!el,
                        outerHtml: truncate(el ? el.outerHTML : null, 8192),
                        e2eErrors: (window.__e2eErrors || []).slice(-30)
                    }}
                }});
            }} catch (err) {{
                callback({{ ok: false, error: String(err) }});
            }}
        "#
        );

        match self.driver.execute_async(&script, vec![]).await {
            Ok(ret) => ret.convert::<Value>().unwrap_or_else(
                |e| json!({ "dump_testid_diagnostics_decode_error": e.to_string() }),
            ),
            Err(e) => json!({ "dump_testid_diagnostics_execute_error": e.to_string() }),
        }
    }

    /// Force TanStack Query to invalidate + refetch every query whose key has
    /// `key_json` (a JSON array literal, e.g. `["sessions"]`) as a prefix, via
    /// the E2E-only `window.__PV_E2E__.queryClient` bridge
    /// (`apps/desktop/src/main.tsx`, `VITE_E2E` gate) — the SAME QueryClient
    /// instance the mounted page reads from, not a page reload.
    ///
    /// Exists because a query younger than its 30s `staleTime`
    /// (`apps/desktop/src/data/queryClient.ts`) serves its cached value on
    /// remount/refocus WITHOUT a network refetch, so a `driver.refresh()`
    /// alone is only a reliable proof of freshness if the reload fully
    /// discarded the prior QueryClient's cache — not guaranteed on every
    /// WebDriver backend (root cause of the cross-PR
    /// `reconcile_drops_externally_deleted_frame_from_real_ui_count` flake,
    /// CI evidence: "last seen: Some(\"2\")" persisting the entire 15s wait,
    /// only possible from a served-stale-cache render, not a fresh backend
    /// read). Awaits `invalidateQueries`'s returned promise, which TanStack
    /// Query resolves only once every currently-active matching query's
    /// refetch settles, so the caller can assert the freshly-rendered DOM
    /// immediately after this returns.
    ///
    /// Lane nD's frontend reconcile invalidation (PR #517, MERGED) wires
    /// `sessions.all` + `inventory` prefix invalidation into the real
    /// "Reconcile" button's click handler
    /// (`apps/desktop/src/features/settings/DataSources.tsx::handleReconcile`)
    /// — but this journey triggers `inventory.reconcile.run` directly over
    /// the invoke bridge (documented KNOWN GAP, no UI trigger for that path),
    /// which #517's handler never runs. This is the freshness guarantee for
    /// that read, not belt-and-braces.
    ///
    /// The question this doc comment used to leave open — "re-evaluate
    /// whether `driver.refresh()` alone is sufficient" — is settled: it is
    /// not (#1113). A reload remounts the app through the setup gate and
    /// route restore, so the document a journey is asserting against can be
    /// torn down under it; the observed failure was an Inbox page with no
    /// `inbox-list` element at all for a full 20s budget while WebDriver went
    /// on serving detached row handles from the pre-reload document. Prefer
    /// this method for any settle-then-assert step, so the settle signal and
    /// the assertion read one live document. Reserve `driver.refresh()` for
    /// steps that are genuinely exercising reload or route-restore behaviour
    /// (see `complete_first_run_gate`, which needs a reload because the
    /// preferences module caches its localStorage read in module state).
    pub async fn invalidate_query(&self, key_json: &str) -> Result<()> {
        let script = format!(
            r#"
            var callback = arguments[arguments.length - 1];
            var e2e = window.__PV_E2E__;
            if (!e2e || !e2e.queryClient) {{
                callback({{ ok: false, error: '__PV_E2E__.queryClient bridge missing (build with VITE_E2E=1)' }});
                return;
            }}
            e2e.queryClient.invalidateQueries({{ queryKey: {key_json} }}).then(function () {{
                callback({{ ok: true }});
            }}).catch(function (err) {{
                callback({{ ok: false, error: String(err) }});
            }});
        "#
        );
        let outcome: InvokeOutcome = self
            .driver
            .execute_async(&script, vec![])
            .await
            .context("invalidate_query execute_async failed")?
            .convert()
            .context("failed to deserialise invalidate_query result")?;
        outcome.into_result::<Value>().map(drop)
    }

    /// Drain the last ~30 real browser console entries (chromedriver/
    /// WebView2 `"browser"` log type, W3C `GET /session/{id}/log`) — best
    /// effort. Some WebDriver stacks (notably older Edge/WebView2 driver
    /// builds) reject the log endpoint entirely; that's captured as an error
    /// string rather than failing the caller, per this module's diagnostics
    /// contract.
    pub async fn dump_console_log(&self) -> Value {
        match self.driver.get_log("browser").await {
            Ok(entries) => {
                let tail: Vec<_> = entries.iter().rev().take(30).rev().collect();
                json!({ "console_log": tail })
            }
            Err(e) => json!({ "console_log_error": e.to_string() }),
        }
    }

    // ---------------------------------------------------------------------
    // Real-DOM interaction helpers (additive, shared across per-area UI
    // journeys — inbox/calibration/targets/sessions/lifecycle/settings/
    // source-view/per-frame-inventory).
    // These drive the ACTUAL rendered `data-testid` elements (click/type/
    // read), never the invoke bridge, so journeys built on them are proving
    // real UI interaction rather than a second copy of the IPC-level tests.
    // ---------------------------------------------------------------------

    /// Locate a single element by its exact `data-testid` attribute.
    pub async fn find_testid(&self, testid: &str) -> Result<WebElement> {
        self.driver
            .find(By::Css(format!("[data-testid='{testid}']")))
            .await
            .with_context(|| format!("no element with data-testid={testid:?}"))
    }

    /// Locate the first element whose `data-testid` STARTS WITH `prefix` —
    /// for dynamic testids keyed by a real backend id (e.g.
    /// `plan-group-<planId>`, `inbox-item-<inboxItemId>`) that the journey
    /// doesn't know in advance.
    pub async fn find_testid_prefix(&self, prefix: &str) -> Result<WebElement> {
        self.driver
            .find(By::Css(format!("[data-testid^='{prefix}']")))
            .await
            .with_context(|| format!("no element with data-testid starting with {prefix:?}"))
    }

    /// All elements whose `data-testid` starts with `prefix`.
    pub async fn find_all_testid_prefix(&self, prefix: &str) -> Result<Vec<WebElement>> {
        self.driver
            .find_all(By::Css(format!("[data-testid^='{prefix}']")))
            .await
            .with_context(|| format!("query for data-testid prefix {prefix:?} failed"))
    }

    /// Lowercased, trimmed `textContent` of every element whose `data-testid`
    /// starts with `prefix`, read as ONE snapshot of the live document.
    ///
    /// Prefer this over [`Self::find_all_testid_prefix`] + per-element
    /// `.text()` whenever the texts are asserted on. The two-step form is not
    /// equivalent on a list that re-renders (the Inbox list swaps row nodes
    /// constantly): a handle can be detached before `.text()` reads it, and
    /// `.text()` on a detached handle raises `stale element reference`. A
    /// caller that defaults that error to `""` turns a WebDriver failure into
    /// something shaped like product data — the #1111 failure mode, which
    /// reported two blank Type badges the product can never render. A single
    /// snapshot cannot interleave with a re-render, and a driver failure
    /// propagates as an error rather than as text.
    pub async fn testid_prefix_texts(&self, prefix: &str) -> Result<Vec<String>> {
        let script = format!(
            r#"
            return JSON.stringify(
                Array.prototype.map.call(
                    document.querySelectorAll('[data-testid^="{prefix}"]'),
                    function (el) {{ return el.textContent || ''; }}
                )
            );
            "#
        );
        let raw = self
            .driver
            .execute(&script, vec![])
            .await
            .with_context(|| format!("snapshotting text for data-testid prefix {prefix:?} failed"))?
            .json()
            .as_str()
            .with_context(|| {
                format!("the {prefix:?} text snapshot script did not return a string")
            })?
            .to_owned();
        let texts: Vec<String> = serde_json::from_str(&raw)
            .with_context(|| format!("the {prefix:?} text snapshot was not a JSON array"))?;
        Ok(texts.into_iter().map(|t| t.trim().to_lowercase()).collect())
    }

    /// The dynamic suffix of the first `data-testid` starting with `prefix`
    /// (e.g. `prefix = "inbox-item-"` on `data-testid="inbox-item-abc123"`
    /// returns `"abc123"`) — lets a journey discover a real backend id from
    /// the rendered DOM instead of a second invoke round-trip.
    ///
    /// POLLS for the element (up to [`DEFAULT_FIND_TIMEOUT`]) rather than
    /// doing a single immediate lookup: this is frequently called straight
    /// after an action that triggers an async refetch + re-render (e.g. an
    /// Inbox rescan, which re-runs `inbox.scan` then re-fetches the list), so
    /// the row may not exist the instant this is called. Same
    /// route/refetch-render race [`Self::find_waiting`] documents — waiting
    /// here means callers don't each have to remember a preceding
    /// `wait_testid_prefix_present`.
    pub async fn testid_suffix(&self, prefix: &str) -> Result<String> {
        let deadline = Instant::now() + DEFAULT_FIND_TIMEOUT;
        let el = loop {
            if let Ok(el) = self.find_testid_prefix(prefix).await {
                break el;
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "no data-testid starting with {prefix:?} appeared within {DEFAULT_FIND_TIMEOUT:?}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        };
        let full = el
            .attr("data-testid")
            .await
            .context("failed to read data-testid attribute")?
            .ok_or_else(|| anyhow!("element matched by prefix {prefix:?} has no data-testid"))?;
        full.strip_prefix(prefix)
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("data-testid {full:?} did not start with {prefix:?}"))
    }

    /// `true` if an element with the exact `data-testid` is currently in the DOM.
    pub async fn testid_exists(&self, testid: &str) -> Result<bool> {
        use thirtyfour::error::WebDriverErrorInner;
        match self.driver.find(By::Css(format!("[data-testid='{testid}']"))).await {
            Ok(_) => Ok(true),
            Err(e) if matches!(e.as_inner(), WebDriverErrorInner::NoSuchElement(_)) => Ok(false),
            Err(e) => Err(e).context("testid_exists query failed"),
        }
    }

    /// Click the element with the given `data-testid`.
    pub async fn click_testid(&self, testid: &str) -> Result<()> {
        self.find_testid(testid)
            .await?
            .click()
            .await
            .with_context(|| format!("click {testid} failed"))
    }

    /// Rendered text content of the element with the given `data-testid`.
    pub async fn text_testid(&self, testid: &str) -> Result<String> {
        self.find_testid(testid)
            .await?
            .text()
            .await
            .with_context(|| format!("read text of {testid} failed"))
    }

    /// `true` when the element with the given `data-testid` is enabled — the
    /// real DOM `disabled` state, not an assumption from response shape.
    pub async fn is_enabled_testid(&self, testid: &str) -> Result<bool> {
        self.find_testid(testid).await?.is_enabled().await.context("is_enabled query failed")
    }

    /// Poll for an element with the given `data-testid` to appear, returning it.
    ///
    /// On timeout, attaches the page context (#1272). "never appeared" alone
    /// cannot distinguish a wrong route from an empty list from a page that
    /// rendered nothing, and a real CI failure
    /// (`project-row-… never appeared within 15s`,
    /// `inventory_journeys::reconcile_drops_externally_deleted_frame…`) was
    /// undiagnosable for exactly that reason.
    pub async fn wait_testid(&self, testid: &str, timeout: Duration) -> Result<WebElement> {
        let deadline = Instant::now() + timeout;
        // Retain the last lookup error instead of discarding it. `NoSuchElement`
        // is the expected, boring case while polling, but a dead session or a
        // malformed selector surfaces here too and used to be swallowed --
        // the same shape as the `.unwrap_or(false)` removed from
        // `wait_bridge_ready` in #1211.
        let mut last_err: Option<String>;
        loop {
            match self.find_testid(testid).await {
                Ok(el) => return Ok(el),
                Err(e) => last_err = Some(format!("{e:#}")),
            }
            if Instant::now() >= deadline {
                let probed = self.bridge_failure_context().await;
                let cause = last_err.map_or_else(String::new, |e| format!("; last error: {e}"));
                return Err(anyhow!(
                    "data-testid={testid:?} never appeared within {timeout:?}{cause}; {probed}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Poll until the element with the given `data-testid` becomes enabled.
    ///
    /// Attaches the same page context as [`Self::wait_testid`] on timeout
    /// (#1272) -- "never became enabled" is ambiguous between an element that
    /// stayed disabled and one that never rendered at all.
    pub async fn wait_testid_enabled(&self, testid: &str, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.is_enabled_testid(testid).await.unwrap_or(false) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                let probed = self.bridge_failure_context().await;
                return Err(anyhow!(
                    "data-testid={testid:?} never became enabled within {timeout:?}; {probed}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Poll until at least one element whose `data-testid` starts with
    /// `prefix` appears in the DOM.
    pub async fn wait_testid_prefix_present(&self, prefix: &str, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.find_testid_prefix(prefix).await.is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "no data-testid starting with {prefix:?} appeared within {timeout:?}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Poll until no element with the given `data-testid` remains in the DOM.
    pub async fn wait_testid_gone(&self, testid: &str, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if !self.testid_exists(testid).await.unwrap_or(true) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(anyhow!("data-testid={testid:?} never disappeared within {timeout:?}"));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Select an `<option>` by its `value` attribute on the
    /// `<select data-testid=..>`.
    ///
    /// NOT implemented via WebDriver's option-click
    /// (`SelectElement::select_by_value`): on WebKitGTK that click does not
    /// reliably fire the `change` event a React-CONTROLLED `<select
    /// onChange>` needs, so React never updates its state and re-renders the
    /// select straight back to its previous value (observed on the Inbox
    /// bulk-reclassify frame-type select, PR #457 — the checkbox on the same
    /// pane committed fine while every option-click silently reverted).
    /// Instead set the value and dispatch bubbling `input` + `change` events
    /// — exactly what Playwright's `selectOption` does — then VERIFY the
    /// value stuck.
    pub async fn select_testid(&self, testid: &str, value: &str) -> Result<()> {
        let el = self.find_testid(testid).await?;
        let script = r#"
            var el = arguments[0];
            var value = arguments[1];
            el.value = value;
            el.dispatchEvent(new Event('input',  { bubbles: true }));
            el.dispatchEvent(new Event('change', { bubbles: true }));
            return el.value;
        "#;
        let out: String = self
            .driver
            .execute(script, vec![el.to_json()?, json!(value)])
            .await
            .with_context(|| format!("select value {value:?} on {testid} failed"))?
            .convert()
            .context("failed to deserialise the select result")?;
        if out != value {
            return Err(anyhow!(
                "select {testid}: value {value:?} did not stick (got {out:?}) — \
                 is there an <option value={value:?}>?"
            ));
        }
        Ok(())
    }

    /// Clear then type into the `<input data-testid=..>`, verifying the typed
    /// value actually landed in the live DOM `.value` before returning —
    /// retrying through render churn if it didn't.
    ///
    /// Unlike `select_testid` (which already verifies its committed value,
    /// PR #457) and the search-input fill in `targets_journeys.rs` (verify +
    /// retry after #841's "typed into the wrong/stale element" garble), this
    /// helper previously trusted `clear()` + `send_keys()` blindly. On a
    /// CONTROLLED React input inside a pane that re-renders as async queries
    /// land (e.g. the Inbox bulk-property fields, which mount only once
    /// `inbox.property_registry` resolves), a re-render racing the keystrokes
    /// can silently drop or truncate them, leaving React state empty even
    /// though `send_keys` itself reported success — the caller (`handleBulkApply`)
    /// then skips the property entirely since it treats `''` as "unchanged".
    pub async fn fill_testid(&self, testid: &str, value: &str) -> Result<()> {
        let deadline = Instant::now() + DEFAULT_FIND_TIMEOUT;
        loop {
            let el = self.find_testid(testid).await?;
            el.clear().await.with_context(|| format!("clear {testid} failed"))?;
            el.send_keys(value).await.with_context(|| format!("send_keys {testid} failed"))?;
            let live_value: String = self
                .driver
                .execute("return arguments[0].value;", vec![el.to_json()?])
                .await
                .with_context(|| format!("reading live .value of {testid} failed"))?
                .convert()
                .with_context(|| format!("failed to deserialize live .value of {testid}"))?;
            if live_value == value {
                return Ok(());
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "fill {testid}: value {value:?} never stuck (last read: {live_value:?}) \
                     after retrying for {DEFAULT_FIND_TIMEOUT:?}"
                );
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Poll `driver.find(by)` until it resolves an element or
    /// [`DEFAULT_FIND_TIMEOUT`] elapses.
    ///
    /// WHY this exists (CI-only bug, reproducible on ubuntu + windows,
    /// #457/#458): after `goto_route(..)` + `wait_bridge_ready(..)`, the
    /// target route's React component subtree has NOT necessarily finished
    /// mounting and painting its controls. `wait_bridge_ready` only proves
    /// `main.tsx` finished top-level module evaluation (the
    /// `window.__PV_E2E__` bridge exists) — it says nothing about whether
    /// the current route's page component has rendered yet. A single
    /// immediate `driver.find(..)` for a page control (e.g. Inbox's "Rescan
    /// all roots" button) therefore RACES that render and intermittently
    /// fails with `no element with aria-label=..` on a slow CI runner, even
    /// though the string is correct and the control does render a beat later.
    /// Polling is the fix — the same wait primitive the `data-testid`
    /// helpers above already use, applied to the aria-label / button-text
    /// locators too.
    pub async fn find_waiting(&self, by: By, what: &str) -> Result<WebElement> {
        let deadline = Instant::now() + DEFAULT_FIND_TIMEOUT;
        loop {
            match self.driver.find(by.clone()).await {
                Ok(el) => return Ok(el),
                Err(e) => {
                    if Instant::now() >= deadline {
                        // Include the URL the page actually sits on — a
                        // missing element is very often "the app is on a
                        // different route", which this makes diagnosable
                        // straight from a CI log.
                        let url = self
                            .driver
                            .current_url()
                            .await
                            .map_or_else(|_| "<unknown>".to_owned(), |u| u.to_string());
                        return Err(e).with_context(|| {
                            format!(
                                "{what} never appeared within {DEFAULT_FIND_TIMEOUT:?} \
                                 (current URL: {url})"
                            )
                        });
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Poll the text content of the element at `data-testid` until `predicate`
    /// accepts it or `timeout` elapses — the DOM-read equivalent of
    /// [`Self::invoke_until`], for asserting a real backend mutation (e.g. a
    /// reconcile pass) landed in a re-rendered, product-owned element instead
    /// of only in the IPC response.
    pub async fn wait_testid_text<P>(
        &self,
        testid: &str,
        timeout: Duration,
        mut predicate: P,
    ) -> Result<String>
    where
        P: FnMut(&str) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut last_seen: Option<String> = None;
        loop {
            if let Ok(el) = self.driver.find(By::Css(format!("[data-testid='{testid}']"))).await {
                if let Ok(text) = el.text().await {
                    if predicate(&text) {
                        return Ok(text);
                    }
                    last_seen = Some(text);
                }
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "text of data-testid={testid:?} never matched within {timeout:?} \
                     (last seen: {last_seen:?})"
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Click an element located by its exact `aria-label` — for the few real
    /// controls that carry no `data-testid` (e.g. Inbox's "Rescan all roots",
    /// whose label is more stable across i18n pluralisation than its text
    /// node). Polls for the element (via [`Self::find_waiting`]) rather than
    /// doing a single immediate lookup, so it survives the route-render race
    /// described on `find_waiting` (the CI `no element with aria-label=..`
    /// failure this fix addresses).
    pub async fn click_by_aria_label(&self, label: &str) -> Result<()> {
        let xpath = format!("//*[@aria-label={}]", escape_string(label));
        self.find_waiting(By::XPath(&xpath), &format!("element with aria-label={label:?}"))
            .await?
            .click()
            .await
            .with_context(|| format!("click aria-label={label:?} failed"))
    }

    /// Complete the app's first-run gate the way the wizard's Finish step
    /// does (`SetupWizard.tsx`), without driving the wizard UI.
    ///
    /// Journeys that visit ANY shell page need this: the Shell component
    /// itself redirects every route to `/setup` while the `setupCompleted`
    /// localStorage preference is false (`apps/desktop/src/app/Shell.tsx` —
    /// a second gate besides the index route's `beforeLoad`, and the reason
    /// `/#/inbox` bounced back to `/#/setup` on CI run 28767450494 even
    /// after the hash-history fix).
    ///
    /// Mirrors the wizard's completion sequence:
    /// 1. `firstrun.complete` (backend gate — the CALLER must already have
    ///    registered at least one raw and one project source, its real
    ///    preconditions);
    /// 2. set `setupCompleted: true` in the `alm-preferences` localStorage
    ///    blob (what `SetupWizard` does via `setPreference`);
    /// 3. reload the page — the preferences module caches its localStorage
    ///    read in module state (`apps/desktop/src/data/preferences.ts`), so
    ///    a direct localStorage write is invisible until a fresh page load.
    pub async fn complete_first_run_gate(&self) -> Result<()> {
        self.complete_first_run_gate_impl(true).await
    }

    /// Like [`Self::complete_first_run_gate`] but LEAVES spec-056 onboarding
    /// enabled, so the orientation walk auto-runs and the Getting-started
    /// checklist renders. Only `onboarding_journey.rs` (VC-004) needs this;
    /// every other journey suppresses onboarding so the walk's modal overlay
    /// never intercepts its own UI interactions.
    pub async fn complete_first_run_gate_onboarding(&self) -> Result<()> {
        self.complete_first_run_gate_impl(false).await
    }

    /// Shared first-run gate completion. When `suppress_onboarding` is true the
    /// deterministic onboarding suppression flag is set before the reload so
    /// neither the walk nor the checklist renders (`isOnboardingSuppressed()`,
    /// `apps/desktop/src/features/onboarding/store.ts`).
    async fn complete_first_run_gate_impl(&self, suppress_onboarding: bool) -> Result<()> {
        let _: Value = self
            .invoke("firstrun_complete", json!({}))
            .await
            .context("firstrun.complete failed — were a raw AND a project source registered?")?;

        let script = r#"
            var raw = localStorage.getItem('alm-preferences');
            var prefs = {};
            try { prefs = raw ? JSON.parse(raw) : {}; } catch (e) { prefs = {}; }
            prefs.setupCompleted = true;
            localStorage.setItem('alm-preferences', JSON.stringify(prefs));
        "#;
        self.driver
            .execute(script, vec![])
            .await
            .context("failed to persist setupCompleted preference")?;

        // Write the flag EXPLICITLY in both directions, before the reload so the
        // onboarding store reads it at boot.
        //
        // Clearing it in the `false` branch is not redundant: on Windows the
        // webview's localStorage is NOT isolated per test the way the DB and
        // app-data dirs are. `InstanceEnv` redirects APPDATA/LOCALAPPDATA, but
        // WebView2 does not resolve its user-data folder from those, so every
        // test in a shard shares one localStorage origin. Each journey that
        // calls the suppressing variant leaves the flag set, and whichever
        // onboarding-enabled test runs after it inherits the suppression and
        // silently renders no walk. That is exactly what made
        // `orientation_walk_then_real_confirm_renders_live_auto_tick` fail on
        // Windows shard 2/2 only, deterministically, while passing on every
        // ubuntu shard (WebKitGTK honours the redirected XDG dirs, so each
        // process really does get a clean profile).
        //
        // Diagnosed from the failure-path dump in `onboarding_journey.rs`:
        // `suppressedFlag:"true"` with a healthy backend and a mounted shell.
        let flag_script = if suppress_onboarding {
            // Otherwise the spec-056 US1 walk auto-runs and its modal overlay
            // intercepts every subsequent `goto_route`/click in the journey.
            r#"localStorage.setItem('alm-onboarding-suppressed', 'true');"#
        } else {
            r#"localStorage.removeItem('alm-onboarding-suppressed');"#
        };
        self.driver
            .execute(flag_script, vec![])
            .await
            .context("failed to write the onboarding suppression flag")?;

        // Clear the bridge marker on the PRE-refresh document before asking
        // for the reload (#1385-followup — CI run 29779614765 and local
        // repro under `--partition hash:4/4`, `test-threads = 2`): under
        // load, `driver.refresh()` can return before WebKitGTK's navigation
        // has actually started, so the OLD document (bridge already set from
        // before this call) is still what `execute()` runs against for a
        // stretch afterward. `wait_bridge_ready` below then reads that STALE
        // true, `complete_first_run_gate` returns "ready", and the real
        // reload — delayed, not skipped — lands moments later and tears down
        // `window.__PV_E2E__` right as the caller's very next `invoke()`
        // fires (observed as "invoke error: __PV_E2E__ bridge missing"
        // immediately after this function returns, only under concurrent
        // nextest execution — never standalone, never on Windows, which
        // serialises this profile for an unrelated reason, see
        // `.config/nextest.toml`). Deleting the marker here means a
        // subsequent `true` reading can only come from the NEW document's
        // own `main.tsx` re-assigning it — a real condition, not a race.
        self.driver
            .execute("delete window.__PV_E2E__;", vec![])
            .await
            .context("failed to clear the pre-refresh __PV_E2E__ marker")?;

        // KEEP the reload (#1113 reviewed): this is not a settle step. The
        // preferences module caches its localStorage read in module state, so
        // the write above is invisible without a fresh page load —
        // `invalidate_query` cannot substitute for it.
        self.driver.refresh().await.context("page refresh after first-run completion failed")?;
        self.wait_document_ready(Duration::from_secs(10)).await?;
        self.wait_bridge_ready(Duration::from_secs(15)).await?;

        // Verify the preference actually survived the reload: if the
        // webview's storage backend dropped it, every shell route would
        // silently bounce back to /setup — fail HERE with a named cause
        // instead of a downstream "element never appeared".
        let persisted: bool = self
            .driver
            .execute(
                r#"
                try {
                    var raw = localStorage.getItem('alm-preferences');
                    return raw ? JSON.parse(raw).setupCompleted === true : false;
                } catch (e) { return false; }
                "#,
                vec![],
            )
            .await
            .context("failed to read back the setupCompleted preference")?
            .convert()
            .context("failed to deserialise the setupCompleted read-back")?;
        if !persisted {
            return Err(anyhow!(
                "setupCompleted=true did not persist in localStorage across the reload — \
                 the webview storage backend dropped the preference"
            ));
        }
        Ok(())
    }

    /// Fill an `<input>`/`<select>`-less text field located by its exact
    /// `aria-label` (clear then type) — for real form fields that carry no
    /// `data-testid` (e.g. `TargetSearch`'s combobox input). Polls for the
    /// element (via [`Self::find_waiting`]) rather than doing a single
    /// immediate lookup, so it survives the same route-render race
    /// `find_waiting` documents.
    pub async fn fill_by_aria_label(&self, label: &str, value: &str) -> Result<()> {
        let xpath = format!("//*[@aria-label={}]", escape_string(label));
        let el = self
            .find_waiting(By::XPath(&xpath), &format!("element with aria-label={label:?}"))
            .await?;
        el.clear().await.with_context(|| format!("clear aria-label={label:?} failed"))?;
        el.send_keys(value).await.with_context(|| format!("send_keys aria-label={label:?} failed"))
    }

    /// Click the first `<button>` whose full trimmed text content equals
    /// `text` exactly — for real controls with no `data-testid` and no
    /// stable `aria-label` (e.g. Settings' "+ Add site" / "Save" buttons).
    /// Only safe to use when `text` is unambiguous in the current DOM (no
    /// two same-labelled buttons visible at once) — callers with an
    /// ambiguity risk (e.g. a dialog whose confirm button repeats a trigger
    /// button's label) should scope the search to a container element
    /// instead via `app.driver.find(...)` + `WebElement::find(...)`. Polls
    /// for the element (via [`Self::find_waiting`]) rather than doing a
    /// single immediate lookup, so it survives the same route-render race
    /// `find_waiting` documents.
    pub async fn click_button_text(&self, text: &str) -> Result<()> {
        let xpath = format!("//button[normalize-space(.)={}]", escape_string(text));
        self.find_waiting(By::XPath(&xpath), &format!("<button> with text {text:?}"))
            .await?
            .click()
            .await
            .with_context(|| format!("click button text={text:?} failed"))
    }

    /// Count of elements anywhere on the page whose `title` attribute equals
    /// `title` exactly — a real, coarse but honest way to assert a specific
    /// disclosure/placeholder tooltip is (or is not) present, when the
    /// underlying element carries no `data-testid` (e.g. the Targets table's
    /// per-row "Opposition date unknown" / "Lunar distance unknown"
    /// disclosures, spec 047). NOT routed through [`Self::find_waiting`]:
    /// callers use this to assert an ABSENCE (a zero count is frequently the
    /// expected, correct result), so polling for presence here would be
    /// wrong — callers that need to wait for a nonzero count should poll
    /// this fn themselves.
    pub async fn count_elements_with_title(&self, title: &str) -> Result<usize> {
        let xpath = format!("//*[@title={}]", escape_string(title));
        Ok(self
            .driver
            .find_all(By::XPath(&xpath))
            .await
            .with_context(|| format!("query for title={title:?} failed"))?
            .len())
    }

    /// Count of `<button>`s anywhere on the page whose full trimmed text
    /// content equals `text` exactly — used as a real, honest "no such
    /// control exists" check (e.g. proving no global "Save" button exists on
    /// a settings pane, spec 018's auto-save-only convention). NOT routed
    /// through [`Self::find_waiting`] for the same reason as
    /// [`Self::count_elements_with_title`]: a zero count is frequently the
    /// expected, correct result.
    pub async fn count_buttons_with_text(&self, text: &str) -> Result<usize> {
        let xpath = format!("//button[normalize-space(.)={}]", escape_string(text));
        Ok(self
            .driver
            .find_all(By::XPath(&xpath))
            .await
            .with_context(|| format!("query for button text={text:?} failed"))?
            .len())
    }

    /// Read an `aria-label`ed checkbox's checked state (e.g. a `Toggle`
    /// component) — real DOM state, not an assumption from response shape.
    /// Polls for the element (via [`Self::find_waiting`]) rather than doing a
    /// single immediate lookup, so it survives the same route-render race
    /// `find_waiting` documents.
    pub async fn checkbox_checked_by_aria_label(&self, label: &str) -> Result<bool> {
        let xpath = format!("//*[@aria-label={}]", escape_string(label));
        self.find_waiting(By::XPath(&xpath), &format!("element with aria-label={label:?}"))
            .await?
            .is_selected()
            .await
            .with_context(|| format!("is_selected() on aria-label={label:?} failed"))
    }

    /// Close the app's window gracefully (round 3, fix-464-theme) before
    /// falling through to the ordinary [`Self::shutdown`] teardown, so a
    /// value written to `localStorage` right before this call actually
    /// survives a following [`Self::relaunch`].
    ///
    /// [`Self::shutdown`]'s `driver.quit()` makes the `tauri-webdriver` CLI
    /// force-kill the app process — the CLI's only handle on the app's
    /// lifetime (see `blocking_session_delete`'s doc). CI evidence (run
    /// 28808552431, then run 28810006837 even with a 1s pre-kill flush
    /// delay) shows this reliably loses a `localStorage` write on Windows:
    /// the raw value read back after a relaunch was `null`, not merely
    /// stale — WebView2 commits `localStorage` to its on-disk LevelDB-backed
    /// store on a graceful shutdown, not on a timer, so a delay before an
    /// abrupt kill cannot save it.
    ///
    /// Triggers a REAL native window close — `@tauri-apps/api/window`'s
    /// `getCurrentWindow().close()` (dynamically imported the same way
    /// `apps/desktop/src/data/theme.ts`'s `syncNativeWindowTheme` already
    /// does), not DOM's bare `window.close()` (a no-op for a top-level
    /// window in most engines). This app has no `on_window_event`/
    /// `CloseRequested` handler (`apps/desktop/src-tauri/src/lib.rs`), so
    /// closing the only window exits the process the same way the native
    /// Quit menu item's `app.exit(0)` does (`lib.rs`'s `on_menu_event`) —
    /// real-user fidelity, not a synthetic teardown path.
    ///
    /// Polls for the `__PV_E2E__` bridge to actually disappear (proof the
    /// window/process tore down) rather than trusting the `close()` promise
    /// resolved before the OS finished reaping the process, then hands off
    /// to [`Self::shutdown`] — by then the app is normally already gone, so
    /// that call is just cleaning up the (already-dead) CLI session and
    /// freeing its proxy port, not the thing that kills the app.
    ///
    /// Falls back to [`Self::shutdown`]'s abrupt kill if the graceful close
    /// doesn't complete within the deadline (e.g. the dynamic import fails
    /// outside a real Tauri runtime) — best-effort, never hangs a journey.
    pub async fn graceful_shutdown(self) -> Result<()> {
        let script = r#"
            var callback = arguments[arguments.length - 1];
            import('@tauri-apps/api/window').then(function (mod) {
                return mod.getCurrentWindow().close();
            }).then(function () {
                callback(true);
            }).catch(function () {
                callback(false);
            });
        "#;
        let _: bool = self
            .driver
            .execute_async(script, vec![])
            .await
            .ok()
            .and_then(|ret| ret.convert::<bool>().ok())
            .unwrap_or(false);

        // Proof the window/process actually tore down: once it has, WebDriver
        // commands against the now-gone window/session fail — treat any
        // error the same as an explicit "bridge gone" (`Ok(false)`).
        let shutdown_timeout = if cfg!(target_os = "windows") {
            Duration::from_secs(60)
        } else {
            Duration::from_secs(10)
        };
        let deadline = Instant::now() + shutdown_timeout;
        loop {
            match self.bridge_ready().await {
                Ok(false) | Err(_) => break,
                Ok(true) if Instant::now() >= deadline => break,
                Ok(true) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        // Small extra margin for the OS to finish reaping the process right
        // after the window/webview teardown completes.
        tokio::time::sleep(Duration::from_millis(200)).await;

        self.shutdown().await
    }

    #[cfg(target_os = "windows")]
    pub async fn wait_for_webview_storage_flush() -> Result<()> {
        let dir = lookup(&instance_env().vars, "WEBVIEW2_USER_DATA_FOLDER")
            .map(PathBuf::from)
            .context("WEBVIEW2_USER_DATA_FOLDER was not configured")?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(leveldb) = find_leveldb_dir(&dir) {
                // Wait for a DATA file (.ldb) or a write-ahead log (.log with
                // non-zero size). Structural files (LOCK, CURRENT, MANIFEST-*)
                // appear before localStorage content is committed, so checking
                // "any file exists" is insufficient — that's what caused the
                // TRY-1-only "no persisted detailDock entry" on loaded runners
                // (bead astro-plan-msdw).
                if std::fs::read_dir(&leveldb).ok().is_some_and(|entries| {
                    entries.flatten().any(|entry| {
                        let path = entry.path();
                        if !path.is_file() {
                            return false;
                        }
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if ext == "ldb" {
                            return true;
                        }
                        // LevelDB .log files are the WAL — a non-empty one
                        // means data has been written (even if not yet
                        // compacted into .ldb).
                        if ext == "log" {
                            return path.metadata().map_or(false, |m| m.len() > 0);
                        }
                        false
                    })
                }) {
                    // Data files appeared: wait until their total size has
                    // been stable across 3 consecutive 200 ms polls before
                    // returning.  WebView2's LevelDB commit is asynchronous —
                    // the file can be present but still growing while the WAL
                    // is being flushed.  A fixed 2 s sleep (the previous
                    // approach at the call site) over-waits on fast runners
                    // and could theoretically under-wait on a very slow one.
                    // Three stable readings at 200 ms each cap the stability
                    // window at 600 ms; the 15 s overall deadline still
                    // bounds the worst case.
                    let mut stable_count = 0u8;
                    let mut prev_size = leveldb_data_size(&leveldb);
                    while Instant::now() < deadline {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        let cur_size = leveldb_data_size(&leveldb);
                        if cur_size == prev_size {
                            stable_count += 1;
                            if stable_count >= 3 {
                                return Ok(());
                            }
                        } else {
                            stable_count = 0;
                            prev_size = cur_size;
                        }
                    }
                    // Deadline exceeded while waiting for stability; proceed
                    // anyway — data files are present and the caller will read
                    // them now.
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "WebView2 profile did not expose persisted LevelDB data files within 15s"
                );
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Quit the WebDriver session and kill the `tauri-webdriver` CLI process
    /// if present. Quitting the session (a `DELETE /session/{id}` through the
    /// CLI) makes the CLI terminate the app process it launched on our
    /// behalf; killing the CLI afterwards frees its proxy port.
    pub async fn shutdown(mut self) -> Result<()> {
        // `quit()` consumes the WebDriver, which can't be moved out of a
        // Drop-implementing type; WebDriver is a cheap Arc-backed handle, so
        // quitting a clone quits the same underlying session.
        let _ = self.driver.clone().quit().await;
        if let Some(mut child) = self.driver_proc.take() {
            kill_driver_proc(&mut child);
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn find_leveldb_dir(root: &Path) -> Option<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "leveldb") {
                    return Some(path);
                }
                pending.push(path);
            }
        }
    }
    None
}

/// Sum the sizes of all `.ldb` and `.log` data files in `leveldb_dir`.
///
/// Used by [`E2eApp::wait_for_webview_storage_flush`] to detect when
/// WebView2's WAL has stopped growing (a stable size across consecutive
/// polls means the commit is complete).  Returns 0 on any I/O error —
/// a safe fallback that keeps the stability counter from accidentally
/// advancing while the directory is unreadable.
#[cfg(target_os = "windows")]
fn leveldb_data_size(leveldb_dir: &Path) -> u64 {
    std::fs::read_dir(leveldb_dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| {
                    let path = e.path();
                    if !path.is_file() {
                        return None;
                    }
                    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
                    if matches!(ext, "ldb" | "log") {
                        path.metadata().ok().map(|m| m.len())
                    } else {
                        None
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

impl Drop for E2eApp {
    /// Best-effort teardown for journeys that bail mid-way with `?` and never
    /// reach [`E2eApp::shutdown`]. Without this, the failed test leaks the
    /// `tauri-webdriver` CLI AND the app it launched, which would poison every
    /// later launch sharing this process — this is exactly what CI run
    /// 28694907445's TRY-2 `can not listen to address: 127.0.0.1:4444` /
    /// `Plugin server not ready after timeout` cascade was, back when ports
    /// were fixed at 4444/4445 instead of allocated per process
    /// ([`InstanceEnv`]).
    ///
    /// `driver.quit()` is async and cannot be awaited here, so the app-kill
    /// is requested with a synchronous raw-HTTP `DELETE /session/…` instead:
    /// the CLI kills its app process after ANY session-delete round trip,
    /// regardless of the session id being real.
    fn drop(&mut self) {
        if let Some(mut child) = self.driver_proc.take() {
            blocking_session_delete(self.proxy_port);
            kill_driver_proc(&mut child);
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Kill and reap the `tauri-webdriver` CLI child process (best-effort).
///
/// `std::process::Child` does NOT kill on drop — letting it fall out of scope
/// leaves the CLI alive and its port occupied (the CI TRY-2 leak).
fn kill_driver_proc(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Synchronously send `DELETE /session/e2e-cleanup` to the `tauri-webdriver`
/// CLI (on `proxy_port`) over a raw std TCP socket (best-effort, short
/// timeouts, no async and no extra HTTP-client dependency — this must be
/// callable from `Drop`).
///
/// The CLI kills the app process it launched after ANY `/session/{id}` DELETE
/// round trip (it does not validate the id) — this is the only handle we have
/// on the app's lifetime, since the CLI spawned it, not the harness.
fn blocking_session_delete(proxy_port: u16) {
    let attempt = || -> std::io::Result<()> {
        let addr = format!("127.0.0.1:{proxy_port}");
        let timeout = Duration::from_secs(5);
        let mut stream = std::net::TcpStream::connect_timeout(&addr.parse().unwrap(), timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        use std::io::{Read, Write};
        stream.write_all(
            format!(
                "DELETE /session/e2e-cleanup HTTP/1.1\r\n\
                 Host: 127.0.0.1:{proxy_port}\r\n\
                 Content-Length: 0\r\n\
                 Connection: close\r\n\r\n"
            )
            .as_bytes(),
        )?;
        // Wait for the response (the CLI kills the app only AFTER the
        // forwarded round trip completes); the body content is irrelevant.
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf);
        Ok(())
    };
    let _ = attempt();
}

/// Pre-flight check: verify the `tauri-webdriver` CLI is on `$PATH` and the
/// `desktop_shell` binary has been built, with a named, actionable error for
/// each (FR-015). Old per-OS driver checks (`WebKitWebDriver`/`msedgedriver`)
/// are obsolete since D10 standardized on `tauri-plugin-webdriver` for every
/// OS — there is no per-OS native driver binary left to check.
fn preflight() -> Result<()> {
    check_tauri_webdriver_cli()?;
    check_app_binary()?;
    Ok(())
}

/// Verify `tauri-webdriver` is reachable on `$PATH` by attempting to spawn it.
/// A spawn failure with `NotFound` means the CLI is missing; any other
/// outcome (including a non-zero exit from an unrecognised flag) means the
/// binary exists.
fn check_tauri_webdriver_cli() -> Result<()> {
    match Command::new("tauri-webdriver").arg("--help").output() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(anyhow!(
            "the `tauri-webdriver` CLI is not on $PATH.\n\
             Install it with: cargo install tauri-webdriver --locked\n\
             (mirrors the \"Install tauri-webdriver CLI\" step in .github/workflows/e2e.yml)"
        )),
        Err(e) => Err(e).context("failed to probe for the tauri-webdriver CLI on $PATH"),
    }
}

/// Verify the `desktop_shell` binary this harness will launch actually
/// exists, so a missing build fails with a named error here instead of a
/// confusing WebDriver session-creation failure.
fn check_app_binary() -> Result<()> {
    let path = app_binary_path()?;
    if path.is_file() {
        Ok(())
    } else {
        Err(anyhow!(
            "desktop_shell binary not found at {}.\n\
             Build it with: cargo build -p desktop_shell --features e2e\n\
             Or point at an existing build with: PV_E2E_APP_BIN=/path/to/binary",
            path.display()
        ))
    }
}

/// Resolve the path to the built `desktop_shell` binary.
///
/// Mirrors `.github/workflows/e2e.yml`'s "Build desktop_shell with e2e
/// feature" step (`cargo build -p desktop_shell --features e2e`), which
/// places the binary at `<workspace_root>/target/debug/desktop_shell[.exe]`.
/// Override with `PV_E2E_APP_BIN=/path/to/binary` (documented in
/// `quickstart.md`) to point at a different build (e.g. a release profile).
fn app_binary_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("PV_E2E_APP_BIN") {
        return Ok(PathBuf::from(path));
    }

    // CARGO_MANIFEST_DIR is `<workspace_root>/crates/e2e-tests` at compile time.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or_else(|| anyhow!("failed to resolve workspace root from CARGO_MANIFEST_DIR"))?;

    let binary_name = if cfg!(windows) { "desktop_shell.exe" } else { "desktop_shell" };
    Ok(workspace_root.join("target").join("debug").join(binary_name))
}

/// Cap on buffered lines per stream in [`ProcLog`]. Every reader wants the
/// *tail* — the lines immediately preceding the failure — so a ring buffer of
/// this size stays cheap even when a chatty app runs for a whole journey.
///
/// Note this is no longer a [`LAUNCH_TIMEOUT`]-bounded window: since #1204,
/// [`E2eApp::wait_bridge_ready`] also reads it, arbitrarily far into a
/// journey. The tail is still the right window, but on a long, noisy journey
/// these 200 lines may be mostly unrelated chatter — raise it if a real
/// investigation ever gets truncated.
const DIAGNOSTIC_LOG_LINES: usize = 200;

/// Bounded ring-buffer capture of the `tauri-webdriver` CLI child process's
/// stdout/stderr, drained continuously by background threads (see
/// [`drain_into`]) — diagnostics only, read on a launch failure in
/// [`E2eApp::launch_with`] and on a bridge-wait timeout in
/// [`E2eApp::wait_bridge_ready`] (#1204). Previously nothing surfaced whether
/// the app even started on a launch failure (undiagnosable macOS
/// `Connection refused` runs, issue #489); the CLI's own child
/// (`desktop_shell`) inherits stdio from the CLI by default, so piping the
/// CLI's streams transitively captures the app's own console output too, not
/// just the CLI's log.
///
/// This is the only diagnostic channel that does not run *through* the
/// webview session, which is what makes it the useful one when the session
/// itself is the fault.
struct ProcLog {
    stdout: Arc<Mutex<VecDeque<String>>>,
    stderr: Arc<Mutex<VecDeque<String>>>,
}

impl ProcLog {
    fn dump(&self) -> String {
        format!(
            "--- tauri-webdriver CLI stdout (last {DIAGNOSTIC_LOG_LINES} lines; \
             desktop_shell inherits this fd by default, so its own console output \
             normally appears here too) ---\n{}\n\
             --- tauri-webdriver CLI stderr ---\n{}",
            Self::render(&self.stdout),
            Self::render(&self.stderr),
        )
    }

    fn render(buf: &Arc<Mutex<VecDeque<String>>>) -> String {
        let lines = buf.lock().unwrap();
        if lines.is_empty() {
            "<empty>".to_owned()
        } else {
            lines.iter().cloned().collect::<Vec<_>>().join("\n")
        }
    }
}

/// Spawn a background thread draining `reader` line-by-line into `buf`
/// (bounded to [`DIAGNOSTIC_LOG_LINES`]). Draining is mandatory, not just for
/// diagnostics: an unread OS pipe fills and blocks the writing process once
/// its buffer is full, which would hang the CLI — and therefore the app it
/// launched — mid-journey, long after a successful launch moved past the
/// code that reads this buffer's contents.
fn drain_into<R: std::io::Read + Send + 'static>(reader: R, buf: Arc<Mutex<VecDeque<String>>>) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let mut buf = buf.lock().unwrap();
            if buf.len() >= DIAGNOSTIC_LOG_LINES {
                buf.pop_front();
            }
            buf.push_back(line);
        }
    });
}

/// Spawn the `tauri-webdriver` CLI proxy as a background child process,
/// bound to the supplied ports and to this instance's isolated DB/app-data
/// root ([`InstanceEnv`]).
///
/// Ports are passed explicitly rather than read from `env` because they are
/// re-picked on every [`E2eApp::launch_with`] call — `env` holds only the
/// stable per-process app-data root, not the ports.
///
/// Mirrors `.github/workflows/e2e.yml`: the CLI is installed once
/// (`cargo install tauri-webdriver --locked`) and this harness starts it per
/// session. `--port`/`--native-port` select this instance's ephemeral ports.
///
/// `tauri-webdriver`'s own `Command::new(&app_path)` (spawning
/// `desktop_shell`) does not `env_clear()`, so every env var set here —
/// `TAURI_WEBDRIVER_PORT` (read by `tauri_plugin_webdriver::init()`,
/// `apps/desktop/src-tauri/src/lib.rs`) matching `--native-port`,
/// `PV_DB_URL`, and the app-data/config dir overrides — propagates
/// transitively into the app process, isolating it without touching
/// `.github/workflows/e2e.yml`.
///
/// stdout/stderr are piped (not inherited) and drained into a [`ProcLog`] so
/// a launch failure can print what the CLI (and transitively, the app it
/// launched) actually did — see [`ProcLog`]'s docs.
fn spawn_tauri_webdriver(
    env: &InstanceEnv,
    proxy_port: u16,
    native_port: u16,
) -> Result<(Child, ProcLog)> {
    let mut cmd = Command::new("tauri-webdriver");
    cmd.arg("--port")
        .arg(proxy_port.to_string())
        .arg("--native-port")
        .arg(native_port.to_string())
        .env("TAURI_WEBDRIVER_PORT", native_port.to_string())
        .env("PV_DB_URL", format!("sqlite://{}?mode=rwc", env.db_path.display()))
        // `native_port` is unique per launch attempt (re-picked by
        // `pick_port_pair` in `launch_with`), so it doubles as a cheap
        // per-instance marker. Its mere presence tells
        // `apps/desktop/src-tauri/src/lib.rs` to skip the single-instance
        // plugin entirely (see that file's plugin registration): the plugin
        // enforces one identifier-derived identity with a per-instance
        // override only on Linux, so concurrently-launched `desktop_shell`
        // instances otherwise collide and the loser is silently
        // redirected/exited without opening a window (WebDriver then times
        // out). Real users/non-e2e builds never set this, so the guard
        // stays active for them.
        .env("PV_E2E_INSTANCE_ID", native_port.to_string())
        // OS-trash boundary double for headless CI. The Windows Shell trash
        // (`trash::delete` -> `IFileOperation`) needs an interactive
        // window-station/desktop and blocks indefinitely in the non-interactive
        // CI runner context — verified: a real interactive Windows desktop
        // trashes on every volume (incl. external + no-Recycle-Bin) in <300ms,
        // only the headless session hangs. A real Recycle-Bin move is
        // unperformable here, so the app does a deterministic filesystem
        // removal instead (see `fs_executor::ops::trash_op`), matching the
        // FakeSpawner/FakeResolver boundary pattern. Production/live never sets
        // this and always uses real OS trash.
        .env("PV_E2E_OS_TRASH_FAKE", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &env.vars {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().map_err(|e| {
        anyhow!(
            "failed to spawn tauri-webdriver: {e} \
             (install with `cargo install tauri-webdriver --locked`)"
        )
    })?;

    let stdout_buf = Arc::new(Mutex::new(VecDeque::new()));
    let stderr_buf = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stdout) = child.stdout.take() {
        drain_into(stdout, stdout_buf.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        drain_into(stderr, stderr_buf.clone());
    }

    Ok((child, ProcLog { stdout: stdout_buf, stderr: stderr_buf }))
}

/// Reset the application database so each test starts from a clean state.
///
/// FR-006: if `PV_DB_URL` is set and looks like `sqlite://PATH?...`, strip
/// the `sqlite://` prefix and everything from `?` onward, then remove that
/// file (errors are ignored so a missing file doesn't fail startup).
///
/// The app connects to exactly this instance's isolated `db_path`
/// ([`InstanceEnv`], passed through as `PV_DB_URL` by
/// [`spawn_tauri_webdriver`]), so no other process/journey can share or race
/// this file. Without removing it here, state would accumulate ACROSS
/// sequential launches within the SAME process (`relaunch()`, or a journey
/// that calls `launch()` more than once) — a journey that completes
/// first-run leaves `firstrun.complete` + its registered roots +
/// unacknowledged inbox items behind for the next launch, breaking both the
/// fresh-DB startup-redirect expectation and every "only item in the list"
/// selection. The `-wal`/`-shm` sidecars are removed too so SQLite can't
/// replay a stale WAL into the fresh DB.
fn reset_database(db_path: &Path) -> Result<()> {
    let _ = std::fs::remove_file(db_path);
    for sidecar in ["-wal", "-shm"] {
        let mut os = db_path.as_os_str().to_owned();
        os.push(sidecar);
        let _ = std::fs::remove_file(PathBuf::from(os));
    }
    Ok(())
}

/// Best-effort wipe of the webview's persisted web storage (localStorage &
/// co.) so preferences set by one journey (`alm-preferences.setupCompleted`,
/// grouping dims, theme) can't leak into the next. Without this, a journey
/// that completes first-run leaves `setupCompleted: true` behind, and the
/// next launch's `SetupPage` immediately bounces `/setup` → `/inbox`
/// (`SetupPage.tsx`), breaking the fresh-DB startup-redirect expectation the
/// journeys share. Called before the app process is spawned, so nothing
/// holds these files open. Failures are ignored (first run has no storage).
///
/// `vars` is this instance's [`InstanceEnv::vars`] — the SAME env overrides
/// passed to the spawned app, so paths resolved here always match where the
/// app actually writes, never the real (unisolated) OS profile.
fn reset_webview_storage(vars: &[(&'static str, String)]) {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "windows") {
        // Since #1204 this instance's WebView2 user-data folder is wherever
        // we told the loader to put it (`WEBVIEW2_USER_DATA_FOLDER`, set in
        // `InstanceEnv::new`), so the reset targets a path this harness
        // itself chose — nothing is derived, mirrored, or guessed.
        //
        // The previous target — `<isolated LOCALAPPDATA>/<identifier>/
        // EBWebView` — never existed. `LOCALAPPDATA` does not move a Known
        // Folder, so the app had been writing to the REAL profile all along:
        // every "reset" silently deleted nothing, and Windows journeys shared
        // one localStorage no matter how carefully each one reset.
        if let Some(dir) = lookup(vars, "WEBVIEW2_USER_DATA_FOLDER") {
            candidates.push(PathBuf::from(dir));
        }
    } else if cfg!(target_os = "macos") {
        // WKWebView website data (incl. localStorage) lives under
        // ~/Library/WebKit/<identifier>/WebsiteData.
        if let Some(home) = lookup(vars, "HOME") {
            candidates.push(
                PathBuf::from(home)
                    .join("Library/WebKit/dev.astro-plan.astro-library-manager/WebsiteData"),
            );
        }
    } else if let Some(dir) = app_data_dir(vars) {
        // WebKitGTK stores localStorage / IndexedDB inside the app data dir.
        candidates.push(dir.join("localstorage"));
        candidates.push(dir.join("storage"));
    }
    for path in candidates {
        let _ = std::fs::remove_dir_all(path);
    }
}

/// Reset `tauri-plugin-window-state`'s persisted geometry (spec 051 US4)
/// before each journey launch, for the same reason `reset_database()` and
/// `reset_webview_storage()` exist: sequential journeys in the same CI job
/// share one real OS user profile, so without this a later journey's app
/// process restores whatever size/position/maximized state an EARLIER
/// journey's process happened to exit in. Kept as a defensive hygiene reset
/// (a restored off-screen/minimized geometry is a real way to hang WebDriver
/// element queries), but it is NOT a fix for the Windows real-UI E2E failure
/// on `inbox_ui_mixed_folder_splits_into_single_type_items`: CI run
/// 28782673323 (main@9ee504d1, BEFORE this function existed) and run
/// 28786351305 (this branch, AFTER it landed) fail identically — same
/// "found 0" assertion, same ~152s duration, on both TRY 1 and TRY 2. The
/// real root cause of that failure is still open; see the diagnostic dump
/// added at the failure site in `inbox_ui_journeys.rs` (round 3,
/// fix-main-e2e-interplay) for the next data point.
///
/// Issue astro-plan-qmc: `app_config_dir()` uses `dirs →
/// SHGetKnownFolderPath` on Windows and ignores `APPDATA`, so the old
/// Windows branch was deleting under a path that never existed — a silent
/// no-op. The fix in `lib.rs` redirects the plugin's store to `PV_DATA_DIR`
/// via an absolute `with_filename()` path, so this function now reads that
/// same env var directly (identical on all platforms — no per-OS branching
/// needed any more).
///
/// Logs a warning when the file can't be found at its expected location AND
/// the current app process is not a first-run (i.e. the file has had a
/// chance to be written): a missing file on the very first launch is normal,
/// but a mismatch between where the app writes and where this function deletes
/// is the bug this function is here to catch.
///
/// `vars` — see [`reset_webview_storage`]'s doc on why this takes the
/// instance's env overrides instead of reading the real OS env.
fn reset_window_state(vars: &[(&'static str, String)]) {
    // On every platform the app writes `.window-state.json` under PV_DATA_DIR
    // when that variable is set (astro-plan-qmc fix in `lib.rs`). Fall back to
    // `app_config_dir` on Linux/macOS where the env vars ARE honoured by the
    // platform dirs resolver and `PV_DATA_DIR` is only set for app-data (not
    // config-dir) isolation.
    let path = if let Some(data_dir) = app_data_dir(vars) {
        data_dir.join(".window-state.json")
    } else if let Some(cfg_dir) = app_config_dir(vars) {
        cfg_dir.join(".window-state.json")
    } else {
        eprintln!(
            "[e2e harness] reset_window_state: neither PV_DATA_DIR nor \
             an app_config_dir override is set — window-state file not reset"
        );
        return;
    };

    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Normal on the very first launch of a fresh instance (no prior
            // session to have written it). Log at debug level only.
            eprintln!(
                "[e2e harness] reset_window_state: file not found at {} \
                 (expected on first launch)",
                path.display()
            );
        }
        Err(e) => {
            eprintln!(
                "[e2e harness] reset_window_state: failed to remove {} — {e} \
                 (window-state may bleed between sequential journeys)",
                path.display()
            );
        }
    }
}

/// Look up `key` in an [`InstanceEnv::vars`]-shaped override list.
fn lookup<'a>(vars: &'a [(&'static str, String)], key: &str) -> Option<&'a str> {
    vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str())
}

/// Resolve the per-OS Tauri `app_config_dir` for the app identifier
/// `dev.astro-plan.astro-library-manager` (`tauri.conf.json`) under this
/// instance's isolated env overrides (`vars`, [`InstanceEnv::vars`]) instead
/// of the real OS env. Mirrors `tauri::path::PathResolver::app_config_dir`
/// (`dirs::config_dir()/<identifier>`) without needing a Tauri runtime in the
/// test harness:
/// - Linux:   `$XDG_CONFIG_HOME`
/// - macOS:   `~/Library/Application Support` (same as `app_data_dir`)
/// - Windows: `%APPDATA%` (roaming, same as `app_data_dir`)
fn app_config_dir(vars: &[(&'static str, String)]) -> Option<PathBuf> {
    const APP_IDENTIFIER: &str = "dev.astro-plan.astro-library-manager";
    let base = if cfg!(target_os = "windows") {
        lookup(vars, "APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        lookup(vars, "HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        lookup(vars, "XDG_CONFIG_HOME").map(PathBuf::from)
    };
    base.map(|b| b.join(APP_IDENTIFIER))
}

/// This instance's app-data root — the directory the app actually writes its
/// SQLite default, `simbad-cache.redb`, and logs into.
///
/// Since #1204 this is simply `PV_DATA_DIR`, which the app honours directly
/// (`desktop_shell::data_dir::resolve`), on every platform. It deliberately
/// does NOT mirror `tauri::path::PathResolver::app_data_dir`'s per-OS
/// `dirs::data_dir()/<identifier>` derivation any more: that derivation is
/// what the harness used to reimplement, and on Windows the reimplementation
/// and the app disagreed silently — the harness resetting files under the
/// isolated root while the app read and wrote the real one.
fn app_data_dir(vars: &[(&'static str, String)]) -> Option<PathBuf> {
    lookup(vars, "PV_DATA_DIR").map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// FITS fixture writer
// ---------------------------------------------------------------------------

/// Write a minimal single-block (2880-byte) FITS file with the given header
/// cards, so journeys can drive the real inbox classify/confirm/ingest
/// pipeline against real files on disk (no product code touched).
///
/// Mirrors the proven fixture writer already used by
/// `crates/app/inbox/src/confirm.rs` tests and
/// `crates/app/core/tests/ingest_sessions_integration.rs` (T045/T046) — same
/// card set, same padding — so the real classifier/session-grouping code
/// accepts it exactly as it does at Layer 1.
///
/// Writes **no** `EXPTIME` card, so every frame type routes to the
/// `__needs_review__` sentinel (T070 mandatory-attribute gate: lights need
/// `OBJECT`+`FILTER`+`EXPTIME`, darks need `EXPTIME`+`GAIN`). That is what the
/// needs-review journeys want; a journey that needs a frame to actually
/// CLASSIFY must use [`write_minimal_fits_with_exposure`].
pub fn write_minimal_fits(
    dir: &Path,
    name: &str,
    imagetyp: &str,
    object: Option<&str>,
    filter: Option<&str>,
    date_obs: Option<&str>,
) -> Result<PathBuf> {
    write_minimal_fits_with_exposure(dir, name, imagetyp, object, filter, date_obs, None)
}

/// [`write_minimal_fits`] plus an optional `EXPTIME` card.
///
/// `EXPTIME` is a hard mandatory attribute for lights AND darks
/// (`mandatory_set_for`, `crates/app/inbox/src/classify.rs`), so it is the
/// difference between a fixture that classifies into a real grouping bucket
/// and one that collapses into the single `__needs_review__` sentinel bucket.
/// Header set matches the Layer-1 `t066_mixed_folder_produces_n_sub_items`
/// fixtures (`EXPTIME=300.0`, `GAIN=100`), which prove a light + a dark
/// materialize as two distinct single-type sub-items.
pub fn write_minimal_fits_with_exposure(
    dir: &Path,
    name: &str,
    imagetyp: &str,
    object: Option<&str>,
    filter: Option<&str>,
    date_obs: Option<&str>,
    exposure_s: Option<f64>,
) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut block = vec![b' '; 2880];
    let mut idx = 0usize;
    let mut write_card = |card: &str| {
        let bytes = card.as_bytes();
        let len = bytes.len().min(80);
        block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
        idx += 1;
    };
    write_card(&format!("{:<80}", format!("IMAGETYP= '{imagetyp}'")));
    if let Some(o) = object {
        write_card(&format!("{:<80}", format!("OBJECT  = '{o}'")));
    }
    if let Some(f) = filter {
        write_card(&format!("{:<80}", format!("FILTER  = '{f}'")));
    }
    if let Some(d) = date_obs {
        write_card(&format!("{:<80}", format!("DATE-OBS= '{d}'")));
    }
    if let Some(e) = exposure_s {
        write_card(&format!("{:<80}", format!("EXPTIME = {e}")));
    }
    write_card(&format!("{:<80}", "GAIN    = 100"));
    write_card(&format!("{:<80}", "XBINNING= 1"));
    write_card(&format!("{:<80}", "YBINNING= 1"));
    block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
    std::fs::write(&path, &block).with_context(|| format!("write fixture FITS {path:?}"))?;
    Ok(path)
}

/// Scan a root through IPC and return the id of the inbox item it yields.
///
/// Spec 058 T012/FR-015 changed the shape every scan-seeded journey depended
/// on: `inbox.scan.folder` no longer writes a placeholder `inbox_items` row,
/// so an ordinary folder now comes back as `items: []` plus a source-group
/// row. Reading `scan["items"][0]` therefore fails with an empty-items error
/// that reads like "the scan found nothing" when the scan in fact worked.
///
/// Classification is what materializes the real single-type item rows, so
/// this seeds the way the product now does: scan, then classify the group the
/// scan recorded, then take the item.
///
/// Master-only folders still come back with items directly (a detected master
/// is its own item row with no source group), so the direct hit is preferred
/// when present rather than treated as an error.
pub async fn scan_and_classify_one_item(
    app: &E2eApp,
    root_id: &str,
    root_absolute_path: &str,
) -> Result<String> {
    let scan: Value = app
        .invoke(
            "inbox_scan_folder",
            serde_json::json!({
                "req": {
                    "rootId": root_id,
                    "rootAbsolutePath": root_absolute_path,
                }
            }),
        )
        .await?;

    if let Some(id) = scan["items"][0]["inboxItemId"].as_str() {
        return Ok(id.to_owned());
    }

    let list: Value =
        app.invoke("inbox_list", serde_json::json!({ "req": { "limit": 500 } })).await?;
    let group_id = list["sourceGroups"]
        .as_array()
        .and_then(|groups| {
            groups.iter().find(|g| g["rootId"].as_str() == Some(root_id)).or_else(|| groups.first())
        })
        .and_then(|g| g["sourceGroupId"].as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("scan recorded no source group to classify: scan={scan} list={list}")
        })?
        .to_owned();

    let _: Value = app
        .invoke(
            "inbox_classify_source_group",
            serde_json::json!({
                "req": {
                    "sourceGroupId": group_id,
                    "rootAbsolutePath": root_absolute_path,
                }
            }),
        )
        .await?;

    let after: Value =
        app.invoke("inbox_list", serde_json::json!({ "req": { "limit": 500 } })).await?;
    after["items"]
        .as_array()
        .and_then(|items| {
            items.iter().find(|i| i["rootId"].as_str() == Some(root_id)).or_else(|| items.first())
        })
        .and_then(|i| i["inboxItemId"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("classifying source group {group_id} materialized no item: {after}")
        })
}
