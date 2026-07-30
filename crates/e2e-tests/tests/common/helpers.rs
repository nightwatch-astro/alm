// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Private infrastructure helpers: process management, database/storage reset,
//! path resolution, and the diagnostic log buffer.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use super::boot::InstanceEnv;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Kill and reap the `tauri-webdriver` CLI child process (best-effort).
///
/// `std::process::Child` does NOT kill on drop — letting it fall out of scope
/// leaves the CLI alive and its port occupied (the CI TRY-2 leak).
pub(super) fn kill_driver_proc(child: &mut Child) {
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
pub(super) fn blocking_session_delete(proxy_port: u16) {
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
pub(super) fn preflight() -> Result<()> {
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
pub(super) fn app_binary_path() -> Result<PathBuf> {
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
pub(super) const DIAGNOSTIC_LOG_LINES: usize = 200;

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
pub(super) struct ProcLog {
    pub(super) stdout: Arc<Mutex<VecDeque<String>>>,
    pub(super) stderr: Arc<Mutex<VecDeque<String>>>,
}

impl ProcLog {
    pub(super) fn dump(&self) -> String {
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
pub(super) fn drain_into<R: std::io::Read + Send + 'static>(
    reader: R,
    buf: Arc<Mutex<VecDeque<String>>>,
) {
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
pub(super) fn spawn_tauri_webdriver(
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
pub(super) fn reset_database(db_path: &Path) -> Result<()> {
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
pub(super) fn reset_webview_storage(vars: &[(&'static str, String)]) {
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
pub(super) fn reset_window_state(vars: &[(&'static str, String)]) {
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
pub(super) fn lookup<'a>(vars: &'a [(&'static str, String)], key: &str) -> Option<&'a str> {
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
pub(super) fn app_config_dir(vars: &[(&'static str, String)]) -> Option<PathBuf> {
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
pub(super) fn app_data_dir(vars: &[(&'static str, String)]) -> Option<PathBuf> {
    lookup(vars, "PV_DATA_DIR").map(PathBuf::from)
}

#[cfg(target_os = "windows")]
pub(super) fn find_leveldb_dir(root: &Path) -> Option<PathBuf> {
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
pub(super) fn leveldb_data_size(leveldb_dir: &Path) -> u64 {
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
