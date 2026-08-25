// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-instance path and port allocation for concurrently-running
//! `desktop_shell` processes.
//!
//! Shared by two callers that must not disagree about what "isolated instance"
//! means: `tests/common/boot.rs::InstanceEnv` (nextest journeys, root is a
//! `tempfile::TempDir`) and the `journey-instance` binary (MCP-bridge journey
//! driving, root is a named directory a validator can inspect afterwards). The
//! root is a parameter for exactly that reason.
//!
//! Everything here is a pure function of `(base, instance)` — deliberately, so
//! two instances derived from the SAME base still diverge. A caller that
//! supplies distinct bases per instance would isolate them by accident and
//! would not exercise this module.

use std::path::{Path, PathBuf};

/// First bridge port. Matches `tauri_plugin_mcp_bridge::Config`'s own default
/// so instance 1 lands where every existing journey doc says it does.
pub const BRIDGE_BASE_PORT: u16 = 9223;

/// Port distance between consecutive instances.
///
/// `tauri_plugin_mcp_bridge::discovery::find_available_port` scans
/// `base_port..base_port + 100` and returns the first free port, so a stride of
/// 100 makes each instance's scan window disjoint from every other instance's:
/// instance N can never drift into instance N+1's advertised port.
pub const BRIDGE_PORT_STRIDE: u16 = 100;

/// Bridge base port for a 1-based instance number.
///
/// The plugin still scans upward from this, so it is the port the instance
/// takes only when the port is free — which is why `journey-instance` checks
/// availability and refuses to launch rather than advertising a port the app
/// may abandon.
#[must_use]
pub fn bridge_port(instance: u16) -> u16 {
    BRIDGE_BASE_PORT + (instance.saturating_sub(1)) * BRIDGE_PORT_STRIDE
}

/// `tauri_plugin_webdriver`'s default port (0.2.1). The plugin is compiled in by
/// the `e2e` feature — the same feature a second instance needs — and it does
/// NOT scan for a free port: the loser of a collision panics its server thread.
const WEBDRIVER_BASE_PORT: u16 = 4445;

/// `TAURI_WEBDRIVER_PORT` for a 1-based instance number.
///
/// Journey driving goes through the MCP bridge, not WebDriver, so this exists
/// only to keep the `e2e` build's WebDriver server from colliding. A stride of 1
/// suffices because the plugin binds exactly this port or fails.
#[must_use]
pub fn webdriver_port(instance: u16) -> u16 {
    WEBDRIVER_BASE_PORT + instance.saturating_sub(1)
}

/// Isolated root for a 1-based instance number, under a shared `base`.
#[must_use]
pub fn instance_root(base: &Path, instance: u16) -> PathBuf {
    base.join(format!("pv-instance-{instance}"))
}

/// App-data dir inside an instance root (`PV_DATA_DIR`).
#[must_use]
pub fn appdata_dir(root: &Path) -> PathBuf {
    root.join("appdata")
}

/// SQLite file inside an instance root.
///
/// Kept distinct from [`appdata_dir`] so a reset that deletes the DB cannot
/// take the pre-warmed resolve cache with it.
#[must_use]
pub fn db_path(root: &Path) -> PathBuf {
    root.join("e2e-test.db")
}

/// `PV_DB_URL` value for an instance root.
#[must_use]
pub fn db_url(root: &Path) -> String {
    format!("sqlite://{}?mode=rwc", db_path(root).display())
}

/// The ambient `PV_DB_URL`, when it is not the one this instance would use.
///
/// `apps/desktop/src-tauri/src/main.rs` prefers `PV_DB_URL` over the path it
/// derives from `PV_DATA_DIR`, so one exported URL puts every instance on one
/// database however well their roots are separated. That is how the historical
/// Windows lane shared `wizard-test.db`. Detected rather than overridden,
/// because the precedence has a legitimate caller: the nextest harness sets
/// `PV_DB_URL` per instance and derives its fresh-DB reset from the exact path
/// it named (`tests/common/helpers.rs`).
///
/// A caller that exports a shared `PV_DB_URL` *after* consuming this instance's
/// environment is outside what any check here can observe.
#[must_use]
pub fn conflicting_db_url<'a>(root: &Path, ambient: Option<&'a str>) -> Option<&'a str> {
    let ambient = ambient?;
    if ambient.trim().is_empty() || ambient.trim() == db_url(root) {
        return None;
    }
    Some(ambient)
}

/// Directory the app resolves `app_config_dir` (window state, webview
/// storage) under, once [`location_vars`] is applied.
///
/// This is where the per-OS redirection actually lands, so a test can assert
/// two instances do not share it without launching an app.
#[must_use]
pub fn config_root(root: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        root.join("appdata")
    } else if cfg!(target_os = "macos") {
        root.join("Library").join("Application Support")
    } else {
        root.join("xdg-config")
    }
}

/// Env overrides that place one app instance's app-data, app-config and
/// webview storage under `root` instead of the shared real OS profile.
///
/// Issue #1204: the per-OS location vars are honoured on Linux (`XDG_*`) and
/// macOS (`HOME`) and silently ignored on Windows — Tauri resolves app dirs
/// through `dirs`, which calls `SHGetKnownFolderPath`, and the Known Folder API
/// reads the user's shell profile rather than `APPDATA`/`LOCALAPPDATA`. So on
/// Windows every concurrent instance shared one real app-data root however
/// these were set, colliding over `simbad-cache.redb` and — fatally — over the
/// WebView2 user-data folder.
///
/// `PV_DATA_DIR` is an explicit override the app itself honours
/// (`desktop_shell::data_dir`), so isolation no longer depends on the OS
/// agreeing to be redirected. The per-OS vars stay: they still place
/// `app_config_dir` (window-state) under this root on Linux and macOS, which
/// `PV_DATA_DIR` does not cover.
///
/// `WEBVIEW2_USER_DATA_FOLDER` is WebView2's own documented loader override:
/// when set it REPLACES the `userDataFolder` argument the app passes to
/// `CreateCoreWebView2EnvironmentWithOptions`. It is read by the WebView2
/// loader inside the app process, so unlike `APPDATA`/`LOCALAPPDATA` it cannot
/// be quietly bypassed by a Known Folder lookup — and unlike a config-declared
/// window's `data_directory` (which must be RELATIVE, and resolves under
/// `dirs::data_local_dir()`), it takes an absolute path, so the folder
/// genuinely lives under this instance's root instead of merely having a unique
/// name in a shared one. Without it the loser of a concurrent pair could not
/// create its webview at all (`WindowsError(0x80070057)`), never opened a
/// window, and never brought up its WebDriver port — surfacing four layers
/// downstream as `bridge never became ready`.
#[must_use]
pub fn location_vars(root: &Path) -> Vec<(&'static str, String)> {
    let mut vars: Vec<(&'static str, String)> =
        vec![("PV_DATA_DIR", appdata_dir(root).display().to_string())];
    vars.extend(if cfg!(target_os = "windows") {
        vec![
            ("APPDATA", root.join("appdata").display().to_string()),
            ("LOCALAPPDATA", root.join("localappdata").display().to_string()),
            ("WEBVIEW2_USER_DATA_FOLDER", root.join("webview2").display().to_string()),
        ]
    } else if cfg!(target_os = "macos") {
        // app_config_dir resolves under $HOME on macOS.
        vec![("HOME", root.display().to_string())]
    } else {
        vec![
            ("XDG_DATA_HOME", root.join("xdg-data").display().to_string()),
            ("XDG_CONFIG_HOME", root.join("xdg-config").display().to_string()),
        ]
    });
    vars
}

#[cfg(test)]
mod tests {
    use super::{
        appdata_dir, bridge_port, config_root, db_path, db_url, instance_root, location_vars,
    };
    use std::path::Path;

    /// `max_attempts` in `tauri_plugin_mcp_bridge::discovery::find_available_port`
    /// (0.11.2). Written out rather than read from `BRIDGE_PORT_STRIDE`, which is
    /// the value under test.
    const PLUGIN_SCAN_WIDTH: u16 = 100;

    /// The gating property: two instances derived from ONE shared base
    /// disagree about every custody-bearing path. A per-instance base would
    /// supply this isolation for free and prove nothing, so the base is
    /// deliberately identical here.
    #[test]
    fn instances_sharing_a_base_share_no_db_and_no_config_dir() {
        let base = Path::new("/shared/base");
        let one = instance_root(base, 1);
        let two = instance_root(base, 2);

        assert_ne!(one, two, "instance roots collapsed onto one path");
        assert_ne!(db_path(&one), db_path(&two), "both instances would open one SQLite file");
        assert_ne!(db_url(&one), db_url(&two), "both instances would get one PV_DB_URL");
        assert_ne!(
            config_root(&one),
            config_root(&two),
            "both instances would share one app-config dir"
        );

        let data_dir = |root: &Path| {
            location_vars(root)
                .into_iter()
                .find(|(k, _)| *k == "PV_DATA_DIR")
                .expect("location_vars must set PV_DATA_DIR")
                .1
        };
        assert_ne!(data_dir(&one), data_dir(&two), "both instances would share one PV_DATA_DIR");
    }

    /// The same property against a real filesystem: distinct strings are not
    /// enough if the paths alias, which they can on a case-insensitive volume.
    /// Both instances are derived from ONE base directory.
    #[test]
    fn a_write_through_one_instance_is_invisible_to_the_other() {
        let base = std::env::temp_dir().join(format!(
            "pv-instance-alias-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let one = instance_root(&base, 1);
        let two = instance_root(&base, 2);
        for root in [&one, &two] {
            std::fs::create_dir_all(appdata_dir(root)).expect("failed to create an instance root");
        }
        std::fs::write(db_path(&one), b"one").expect("failed to write instance 1's db");
        std::fs::write(db_path(&two), b"two").expect("failed to write instance 2's db");

        assert_eq!(std::fs::read(db_path(&one)).expect("instance 1 db unreadable"), b"one");
        assert_eq!(std::fs::read(db_path(&two)).expect("instance 2 db unreadable"), b"two");

        std::fs::remove_dir_all(&base).expect("failed to clean the test base");
    }

    /// Every location var must point INSIDE the instance root, or the app
    /// silently keeps using the real OS profile for whatever that var covers.
    #[test]
    fn every_location_var_points_inside_the_instance_root() {
        let root = instance_root(Path::new("/shared/base"), 3);
        let prefix = root.display().to_string();
        let vars = location_vars(&root);
        assert!(!vars.is_empty(), "no location vars produced");
        for (key, value) in vars {
            assert!(value.starts_with(&prefix), "{key}={value} escapes the instance root {prefix}");
        }
    }

    /// The historical lane pin: one `PV_DB_URL` exported for every instance.
    /// Instance 2 must reject instance 1's URL, and both must accept their own.
    #[test]
    fn a_foreign_pv_db_url_is_rejected_and_the_instances_own_is_not() {
        let base = Path::new("/shared/base");
        let one = instance_root(base, 1);
        let two = instance_root(base, 2);
        let lane_pin = "sqlite://C:\\dev\\astro-plan\\wizard-test.db?mode=rwc";

        assert_eq!(super::conflicting_db_url(&two, Some(lane_pin)), Some(lane_pin));
        assert!(
            super::conflicting_db_url(&two, Some(&db_url(&one))).is_some(),
            "instance 2 must reject instance 1's database"
        );
        assert_eq!(super::conflicting_db_url(&one, Some(&db_url(&one))), None);
        assert_eq!(super::conflicting_db_url(&two, Some(&db_url(&two))), None);
        assert_eq!(super::conflicting_db_url(&one, None), None);
        assert_eq!(super::conflicting_db_url(&one, Some("  ")), None);
    }

    /// The `e2e` build's WebDriver server binds one port and panics its thread
    /// on a collision, so no two instances may be given the same one.
    #[test]
    fn every_instance_gets_its_own_webdriver_port() {
        let ports: Vec<u16> = (1..8u16).map(super::webdriver_port).collect();
        let mut unique = ports.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ports.len(), "webdriver ports collide: {ports:?}");
        assert_eq!(super::webdriver_port(1), 4445, "instance 1 must keep the plugin default");
    }

    /// Disjoint scan windows: the plugin scans 100 ports up from the base, so
    /// consecutive instances must be at least that far apart.
    #[test]
    fn consecutive_bridge_ports_have_disjoint_scan_windows() {
        assert_eq!(bridge_port(1), super::BRIDGE_BASE_PORT);
        for n in 1..8u16 {
            let gap = bridge_port(n + 1) - bridge_port(n);
            assert!(
                gap >= PLUGIN_SCAN_WIDTH,
                "instance {n} can scan into instance {}'s port (gap {gap})",
                n + 1
            );
        }
    }
}
