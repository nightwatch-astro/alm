// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Resolve one journey-driving app instance and print its environment.
//!
//! Usage: `journey-instance <N> [--reset]`, `N` 1-based.
//!
//! Stdout is a shell-evaluable `export` block and nothing else, so
//! `eval "$(journey-instance 2)"` is the whole integration. Stderr carries the
//! human-readable summary, including the bridge `host:port` a validator passes
//! to `driver_session`.
//!
//! Every path and the bridge port come from [`e2e_tests::instance`], the same
//! module the nextest harness uses, so a journey-driven instance and a
//! nextest-driven instance cannot disagree about what isolation means.

use std::io::Write as _;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("journey-instance: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut instance: Option<u16> = None;
    let mut reset = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--reset" => reset = true,
            other => {
                let n: u16 = other
                    .parse()
                    .map_err(|_| format!("expected a 1-based instance number, got {other:?}"))?;
                if n == 0 {
                    return Err("instance numbers are 1-based; 0 is not an instance".to_owned());
                }
                instance = Some(n);
            }
        }
    }
    let instance = instance.ok_or("usage: journey-instance <N> [--reset]")?;

    let base = std::env::var_os("PV_JOURNEY_ROOT")
        .map_or_else(|| std::env::temp_dir().join("pv-journeys"), PathBuf::from);
    let root = e2e_tests::instance::instance_root(&base, instance);

    if reset {
        // Only ever a path this binary derived, never a caller-supplied one.
        if let Err(e) = std::fs::remove_dir_all(&root) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(format!("failed to reset {}: {e}", root.display()));
            }
        }
    }
    std::fs::create_dir_all(e2e_tests::instance::appdata_dir(&root))
        .map_err(|e| format!("failed to create {}: {e}", root.display()))?;

    let bind = std::env::var("PV_MCP_BRIDGE_BIND")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_owned());
    let port = e2e_tests::instance::bridge_port(instance);

    // The plugin scans upward from `base_port`, so a busy port would silently
    // move the app somewhere this block does not advertise. Refuse instead. The
    // stride in `instance` already rules out a sibling instance taking it, so
    // reaching this error means instance N is already running.
    TcpListener::bind((bind.as_str(), port)).map_err(|e| {
        format!(
            "bridge port {bind}:{port} for instance {instance} is not available ({e}); \
             another instance {instance} is probably already running"
        )
    })?;

    let mut vars = e2e_tests::instance::location_vars(&root);
    vars.push(("PV_DB_URL", e2e_tests::instance::db_url(&root)));
    vars.push(("PV_E2E_INSTANCE_ID", format!("journey-{instance}")));
    vars.push(("PV_MCP_BRIDGE_ENABLE", "1".to_owned()));
    vars.push(("PV_MCP_BRIDGE_BIND", bind.clone()));
    vars.push(("PV_MCP_BRIDGE_PORT", port.to_string()));
    vars.push(("TAURI_WEBDRIVER_PORT", e2e_tests::instance::webdriver_port(instance).to_string()));
    // Client side: `@hypothesi/tauri-mcp-server` reads these when
    // `driver_session` is called without explicit host/port arguments.
    vars.push(("MCP_BRIDGE_HOST", bind.clone()));
    vars.push(("MCP_BRIDGE_PORT", port.to_string()));

    let mut out = std::io::stdout().lock();
    for (key, value) in &vars {
        writeln!(out, "export {key}={}", shell_single_quote(value))
            .map_err(|e| format!("failed to write the export block: {e}"))?;
    }
    out.flush().map_err(|e| format!("failed to flush the export block: {e}"))?;

    eprintln!("instance {instance}");
    eprintln!("  root      {}", root.display());
    eprintln!("  database  {}", e2e_tests::instance::db_path(&root).display());
    eprintln!("  config    {}", e2e_tests::instance::config_root(&root).display());
    eprintln!("  bridge    {bind}:{port}");
    eprintln!("  webdriver {}", e2e_tests::instance::webdriver_port(instance));
    eprintln!("  connect   driver_session host={bind} port={port}");
    Ok(())
}

/// Quote for POSIX `sh` so a path containing spaces survives `eval`.
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::shell_single_quote;

    #[test]
    fn quoting_survives_spaces_and_embedded_quotes() {
        assert_eq!(shell_single_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_single_quote("it's"), r"'it'\''s'");
    }
}
