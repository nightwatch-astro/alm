// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `E2eApp` — the main harness handle for a running test app + WebDriver session.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use thirtyfour::components::escape_string;
use thirtyfour::prelude::*;

use super::boot::{
    app_binary_path, instance_env, pick_port_pair, LAUNCH_TIMEOUT, SCRIPT_TIMEOUT,
    SCRIPT_TIMEOUT_PAGE_LOAD,
};
use super::boot::{DEFAULT_FIND_TIMEOUT, APP_URL};
use super::helpers::{
    blocking_session_delete, drain_into, kill_driver_proc, preflight, reset_database,
    reset_webview_storage, reset_window_state, spawn_tauri_webdriver, ProcLog,
};
#[cfg(target_os = "windows")]
use super::helpers::{find_leveldb_dir, leveldb_data_size};

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
    driver_proc: Option<std::process::Child>,
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
    /// `{APP_URL}{path}` therefore always lands on the index route `/`,
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
        let url = format!("{APP_URL}/#{path}");
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
        use super::helpers::lookup;
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
