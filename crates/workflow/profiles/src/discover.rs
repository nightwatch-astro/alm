// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-OS processing-tool executable auto-discovery (spec 011 T015, R2).
//!
//! Discovery is a pure read operation — safe to call repeatedly, no writes.
//! Results are marked `auto_detected = true`; the user must explicitly save
//! before paths become active.
//!
//! Platform coverage (R2):
//! - **macOS**: scan `/Applications` for `.app` bundles by name.
//! - **Linux**: search `PATH` + known directories (`/usr/bin`, `/usr/local/bin`,
//!   `/opt/pixinsight/bin`, etc.).
//! - **Windows**: query the registry — `App Paths` plus `Uninstall` entries (matched
//!   by `DisplayName`, exe resolved from `DisplayIcon`/`InstallLocation`) across the
//!   64-bit, 32-bit (`WOW6432Node`) and per-user hives — so a tool is found wherever
//!   it was installed; falls back to a `%ProgramFiles%[(x86)]\<Tool>\bin\<exe>` scan.
//!
//! All paths are absolute; relative paths are never returned.

use std::path::{Path, PathBuf};

/// Result of a single-tool discovery attempt.
#[derive(Clone, Debug)]
pub struct DiscoveryResult {
    pub tool_id: String,
    /// Absolute path to the discovered executable (or .app on macOS).
    pub path: PathBuf,
    /// Whether the executable exists and is accessible at the time of discovery.
    pub available: bool,
}

// ── macOS ─────────────────────────────────────────────────────────────────────

/// Known tools: `(tool_id, CFBundleIdentifier, executable name inside Contents/MacOS)`.
#[cfg(target_os = "macos")]
const MACOS_TOOLS: &[(&str, &str, &str)] = &[
    ("pixinsight", "com.pixinsight.PixInsight", "PixInsight"),
    ("siril", "org.free-astro.siril", "Siril"),
];

#[cfg(target_os = "macos")]
#[must_use]
pub fn discover_all() -> Vec<DiscoveryResult> {
    // Application-based detection: ask Launch Services / Spotlight where the app is
    // (by bundle id), so a `.app` is found wherever the user keeps it; fall back to
    // scanning the standard Applications folders. `dedup_by_tool` keeps the first match.
    let mut results: Vec<DiscoveryResult> = Vec::new();
    results.extend(spotlight_discover());
    results.extend(applications_scan());
    dedup_by_tool(results)
}

/// Resolve installed apps via Spotlight: `mdfind` by `CFBundleIdentifier` returns the
/// `.app` bundle path regardless of install location.
#[cfg(target_os = "macos")]
fn spotlight_discover() -> Vec<DiscoveryResult> {
    use std::process::Command;
    let mut results: Vec<DiscoveryResult> = Vec::new();
    for &(tool_id, bundle_id, exe) in MACOS_TOOLS {
        let query = format!("kMDItemCFBundleIdentifier == '{bundle_id}'");
        let Ok(out) = Command::new("/usr/bin/mdfind").arg(&query).output() else { continue };
        if !out.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let app = line.trim();
            if app.is_empty() {
                continue;
            }
            let exe_path = Path::new(app).join("Contents").join("MacOS").join(exe);
            if exe_path.exists() {
                results.push(DiscoveryResult {
                    tool_id: tool_id.to_owned(),
                    available: true,
                    path: exe_path,
                });
                break; // first hit per tool is enough
            }
        }
    }
    results
}

/// Fallback: scan `/Applications` and `~/Applications` for the known `.app` names.
#[cfg(target_os = "macos")]
fn applications_scan() -> Vec<DiscoveryResult> {
    let mut dirs: Vec<PathBuf> = vec![PathBuf::from("/Applications")];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(Path::new(&home).join("Applications"));
    }
    // (tool_id, relative `.app` path under the dir, exe). PixInsight historically nests
    // under `PixInsight/PixInsight.app`; the flat form is kept as a fallback.
    let apps: &[(&str, &str, &str)] = &[
        ("pixinsight", "PixInsight/PixInsight.app", "PixInsight"),
        ("pixinsight", "PixInsight.app", "PixInsight"),
        ("siril", "Siril.app", "Siril"),
    ];
    let mut results: Vec<DiscoveryResult> = Vec::new();
    for dir in &dirs {
        for &(tool_id, app, exe) in apps {
            let p = dir.join(app).join("Contents").join("MacOS").join(exe);
            if p.exists() {
                results.push(DiscoveryResult {
                    tool_id: tool_id.to_owned(),
                    available: true,
                    path: p,
                });
            }
        }
    }
    results
}

// ── Linux ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
#[must_use]
pub fn discover_all() -> Vec<DiscoveryResult> {
    // Application-based detection: PATH (the canonical CLI mechanism, great for Siril),
    // the freedesktop `.desktop` registry (the standard "installed apps" index — covers
    // non-standard locations), plus known install dirs and Flatpak/Snap export wrappers.
    let candidates: &[(&str, &str)] = &[
        ("pixinsight", "/opt/PixInsight/bin/PixInsight"),
        ("pixinsight", "/usr/local/bin/PixInsight"),
        ("siril", "/usr/bin/siril"),
        ("siril", "/usr/local/bin/siril"),
        ("siril", "/snap/bin/siril"),
        // Flatpak export wrappers are directly executable (they shell out to `flatpak run`).
        ("siril", "/var/lib/flatpak/exports/bin/org.free_astro.Siril"),
    ];
    let mut results = probe_candidates(candidates);
    // PATH search.
    results.extend(discover_from_path(&[("pixinsight", "PixInsight"), ("siril", "siril")]));
    // `.desktop` registry scan (resolves apps installed to non-standard locations).
    results.extend(desktop_file_discover());
    // Deduplicate: keep first found per tool_id.
    dedup_by_tool(results)
}

/// Scan freedesktop `.desktop` files (system + per-user + Flatpak/Snap export dirs),
/// match by filename/`Exec`, and resolve the `Exec` command to an absolute binary.
/// Flatpak/Snap *launcher* commands (`flatpak run …` / `snap run …`) are skipped here —
/// those are covered by their directly-executable export wrappers in the candidate list,
/// because a launcher command does not map to a single tool executable path.
#[cfg(target_os = "linux")]
fn desktop_file_discover() -> Vec<DiscoveryResult> {
    // (tool_id, lowercase name substring)
    let tools: &[(&str, &str)] = &[("pixinsight", "pixinsight"), ("siril", "siril")];

    let mut dirs: Vec<PathBuf> = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(Path::new(&home).join(".local/share/applications"));
        dirs.push(Path::new(&home).join(".local/share/flatpak/exports/share/applications"));
    }

    let mut results: Vec<DiscoveryResult> = Vec::new();
    for dir in &dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
            let Ok(contents) = std::fs::read_to_string(&path) else { continue };
            let Some(exec_line) = contents.lines().find(|l| l.trim_start().starts_with("Exec="))
            else {
                continue;
            };
            let exec = exec_line.trim_start().trim_start_matches("Exec=").trim();
            let cmd = exec.split_whitespace().next().unwrap_or("");
            // Skip launcher commands — their export wrappers are probed separately.
            if cmd.is_empty() || cmd == "flatpak" || cmd == "snap" {
                continue;
            }
            let cmd_lower = cmd.to_lowercase();
            for &(tool_id, name_sub) in tools {
                if fname.contains(name_sub) || cmd_lower.contains(name_sub) {
                    if let Some(abs) = resolve_executable(cmd) {
                        results.push(DiscoveryResult {
                            tool_id: tool_id.to_owned(),
                            available: true,
                            path: abs,
                        });
                    }
                }
            }
        }
    }
    results
}

/// Resolve a `.desktop` `Exec` command to an absolute path: absolute as-is, otherwise
/// searched on `PATH`.
#[cfg(target_os = "linux")]
fn resolve_executable(cmd: &str) -> Option<PathBuf> {
    let p = Path::new(cmd);
    if p.is_absolute() {
        return p.exists().then(|| p.to_path_buf());
    }
    let path_var = std::env::var_os("PATH")?;
    // Bound to a local because the borrowing iterator, as a tail expression, would be
    // dropped after `path_var` rather than before it.
    let found = absolute_path_dirs(&path_var).map(|dir| dir.join(cmd)).find(|c| c.exists());
    found
}

// ── Windows ───────────────────────────────────────────────────────────────────

/// Known tools: `(tool_id, display-name substring [lowercased], install subdir, exe filename)`.
#[cfg(target_os = "windows")]
const WINDOWS_TOOLS: &[(&str, &str, &str, &str)] = &[
    ("pixinsight", "pixinsight", "PixInsight", "PixInsight.exe"),
    ("siril", "siril", "Siril", "siril.exe"),
];

#[cfg(target_os = "windows")]
#[must_use]
pub fn discover_all() -> Vec<DiscoveryResult> {
    // Application-based detection: ask the OS where the app is installed (registry)
    // rather than assuming a fixed path, so a tool installed to a non-default
    // location is still found. The Program Files scan is a fallback. `dedup_by_tool`
    // keeps the first (registry-preferred) match per tool.
    let mut results: Vec<DiscoveryResult> = Vec::new();
    results.extend(registry_discover());
    results.extend(program_files_scan());
    dedup_by_tool(results)
}

/// Resolve installed apps from the Windows registry: per-exe `App Paths` and the
/// `Uninstall` entries (matched by `DisplayName`, exe derived from `DisplayIcon`
/// or `InstallLocation`). Covers 64-bit, 32-bit (`WOW6432Node`), and per-user hives.
#[cfg(target_os = "windows")]
fn registry_discover() -> Vec<DiscoveryResult> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let mut results: Vec<DiscoveryResult> = Vec::new();

    // 1. App Paths — default value of `...\App Paths\<exe>` is the full exe path.
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let root = RegKey::predef(hive);
        for &(tool_id, _name, _subdir, exe) in WINDOWS_TOOLS {
            let key_path = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
            if let Ok(k) = root.open_subkey(&key_path) {
                if let Ok(p) = k.get_value::<String, _>("") {
                    push_if_exe(&mut results, tool_id, p.trim().trim_matches('"'));
                }
            }
        }
    }

    // 2. Uninstall entries — match DisplayName, derive the exe from DisplayIcon/InstallLocation.
    let uninstall_roots = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];
    for (hive, path) in uninstall_roots {
        let root = RegKey::predef(hive);
        let Ok(unkey) = root.open_subkey(path) else { continue };
        for name in unkey.enum_keys().flatten() {
            let Ok(sub) = unkey.open_subkey(&name) else { continue };
            let display: String = sub.get_value("DisplayName").unwrap_or_default();
            let dl = display.to_lowercase();
            for &(tool_id, name_sub, _subdir, exe) in WINDOWS_TOOLS {
                if !dl.contains(name_sub) {
                    continue;
                }
                // DisplayIcon is often the exe itself (with an optional ",<index>" suffix).
                if let Ok(icon) = sub.get_value::<String, _>("DisplayIcon") {
                    let icon_path = icon.split(',').next().unwrap_or("").trim().trim_matches('"');
                    push_if_exe(&mut results, tool_id, icon_path);
                }
                // InstallLocation is the install dir — probe `bin\<exe>` then `<exe>`.
                if let Ok(loc) = sub.get_value::<String, _>("InstallLocation") {
                    let loc = loc.trim().trim_matches('"');
                    if !loc.is_empty() {
                        let base = Path::new(loc);
                        push_if_exe_path(&mut results, tool_id, base.join("bin").join(exe));
                        push_if_exe_path(&mut results, tool_id, base.join(exe));
                    }
                }
            }
        }
    }
    results
}

/// Fallback: scan the default Program Files layout (`<Tool>\bin\<exe>` + non-`bin`).
#[cfg(target_os = "windows")]
fn program_files_scan() -> Vec<DiscoveryResult> {
    let mut dirs: Vec<String> = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        dirs.push(pf);
    } else {
        dirs.push(r"C:\Program Files".to_owned());
    }
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        dirs.push(pf86);
    }
    let mut results: Vec<DiscoveryResult> = Vec::new();
    for dir in &dirs {
        let pf = Path::new(dir);
        for &(tool_id, _name, subdir, exe) in WINDOWS_TOOLS {
            push_if_exe_path(&mut results, tool_id, pf.join(subdir).join("bin").join(exe));
            push_if_exe_path(&mut results, tool_id, pf.join(subdir).join(exe));
        }
    }
    results
}

/// Push a result if `path_str` is an absolute, existing `.exe`.
#[cfg(target_os = "windows")]
fn push_if_exe(results: &mut Vec<DiscoveryResult>, tool_id: &str, path_str: &str) {
    if !path_str.is_empty() {
        push_if_exe_path(results, tool_id, PathBuf::from(path_str));
    }
}

#[cfg(target_os = "windows")]
fn push_if_exe_path(results: &mut Vec<DiscoveryResult>, tool_id: &str, path: PathBuf) {
    let is_exe =
        path.extension().and_then(|e| e.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("exe"));
    if path.is_absolute() && is_exe && path.exists() {
        results.push(DiscoveryResult { tool_id: tool_id.to_owned(), available: true, path });
    }
}

// ── Fallback for other platforms ──────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
#[must_use]
pub fn discover_all() -> Vec<DiscoveryResult> {
    vec![]
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// Only `discover_all()` on Linux uses this helper; keep it available for the
// unit test on every platform.
#[cfg(any(target_os = "linux", test))]
fn probe_candidates(candidates: &[(&str, &str)]) -> Vec<DiscoveryResult> {
    let mut results = Vec::new();
    for &(tool_id, path_str) in candidates {
        let path = PathBuf::from(path_str);
        if path.exists() {
            results.push(DiscoveryResult { tool_id: tool_id.to_owned(), available: true, path });
        }
    }
    results
}

/// Search directories from a `PATH`-style variable value, absolute ones only.
///
/// POSIX gives an empty `PATH` element the meaning "current working directory",
/// so a leading, trailing or doubled separator would otherwise yield a candidate
/// that `Path::exists` resolves against wherever the app was started from.
/// `split_paths` turns an empty element into an empty path, which `is_absolute`
/// rejects along with every other relative element.
///
/// The value is a parameter so tests need not mutate the process environment,
/// which `unsafe_code = "forbid"` puts out of reach anyway.
#[cfg(any(target_os = "linux", test))]
fn absolute_path_dirs(path_var: &std::ffi::OsStr) -> impl Iterator<Item = PathBuf> + '_ {
    std::env::split_paths(path_var).filter(|dir| dir.is_absolute())
}

#[cfg(target_os = "linux")]
fn discover_from_path(names: &[(&'static str, &str)]) -> Vec<DiscoveryResult> {
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let mut results = Vec::new();
    for dir in absolute_path_dirs(&path_var) {
        for &(tool_id, exe_name) in names {
            let p = dir.join(exe_name);
            if p.exists() {
                results.push(DiscoveryResult {
                    tool_id: tool_id.to_owned(),
                    available: true,
                    path: p,
                });
            }
        }
    }
    results
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn dedup_by_tool(items: Vec<DiscoveryResult>) -> Vec<DiscoveryResult> {
    // spec 042 (T203): `unique_by` keeps the first occurrence of each key, which
    // is identical to the prior `HashSet`-insert filter (first match wins).
    use itertools::Itertools as _;
    items.into_iter().unique_by(|r| r.tool_id.clone()).collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// discover_all() is always callable and returns a Vec (may be empty).
    #[test]
    fn discover_all_does_not_panic() {
        let results = discover_all();
        // All returned paths must be absolute.
        for r in &results {
            assert!(r.path.is_absolute(), "discovered path must be absolute: {:?}", r.path);
        }
        // All tool_ids must be known seeds.
        for r in &results {
            assert!(
                crate::seed::find(&r.tool_id).is_some(),
                "discovered unknown tool_id '{}'",
                r.tool_id
            );
        }
        drop(results); // just ensuring no panic
    }

    #[test]
    fn probe_nonexistent_returns_empty() {
        let results = probe_candidates(&[("pixinsight", "/no/such/path/PixInsight")]);
        assert!(results.is_empty());
    }

    /// `join_paths` writes the host separator, so the malformed shapes under test
    /// are the host's own (`::` on Unix, `;;` on Windows).
    fn path_var(dirs: &[&str]) -> std::ffi::OsString {
        std::env::join_paths(dirs.iter().map(std::path::PathBuf::from))
            .expect("fixture elements contain no path separator")
    }

    #[test]
    fn absolute_path_dirs_drops_empty_and_relative_elements() {
        let bin = fs_pathsafe::test_support::abs("/opt/astro/bin");
        let local = fs_pathsafe::test_support::abs("/usr/local/bin");
        // Leading, doubled and trailing separators, plus a relative element.
        let var = path_var(&["", &bin, "", "tools/bin", &local, ""]);

        let dirs: Vec<PathBuf> = absolute_path_dirs(&var).collect();

        assert_eq!(dirs, vec![PathBuf::from(&bin), PathBuf::from(&local)]);
    }

    #[test]
    fn absolute_path_dirs_yields_nothing_when_no_element_is_absolute() {
        let var = path_var(&["", ".", "tools/bin", ""]);
        assert_eq!(absolute_path_dirs(&var).count(), 0);
    }
}
