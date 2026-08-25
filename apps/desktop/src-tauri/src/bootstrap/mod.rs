// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Composition-root helpers for `build_app`/`run_app` (issue #981), split by
//! why each part changes: window geometry restoration, the native
//! application menu, and the background task spawners. Not part of the
//! crate's public API.
//!
//! The `specta`/`invoke_handler` builder pair (`bootstrap/specta.rs`) is grouped
//! here conceptually but is `include!`d from `lib.rs`'s crate-root scope
//! instead of declared as a `mod` of this one — see that file's header
//! comment for why a real module boundary breaks it.

pub mod background;
pub mod menu;
pub mod notify;
pub mod window;

/// Whether `build_app` registers the single-instance guard (spec 051 US1).
///
/// The E2E bypass is scoped two ways, and both must hold to skip the guard:
/// the binary is built with the `e2e` feature (which release builds MUST NOT
/// enable, mirroring `dev-tools` — see `Cargo.toml`), and `PV_E2E_INSTANCE_ID`
/// is set at runtime. The compile-time half is what keeps a shipped binary
/// from being talked out of its guard by a stray environment variable.
pub const fn single_instance_guard_enabled(e2e_instance_id_set: bool) -> bool {
    !(cfg!(feature = "e2e") && e2e_instance_id_set)
}

/// The address the MCP bridge binds, or `None` when it must not start.
///
/// Arguments are the raw values of `PV_MCP_BRIDGE_ENABLE` and
/// `PV_MCP_BRIDGE_BIND`; `None` means unset. The gate is scoped two ways and
/// both must hold: the binary is built with `dev-tools`, and the enable value is
/// exactly `1`. Every other value -- `0`, `true`, empty, arbitrary text --
/// leaves the port closed, so a typo fails closed rather than opening an
/// unauthenticated control surface.
///
/// The default is loopback, not the plugin's own `0.0.0.0`. A blank
/// `PV_MCP_BRIDGE_BIND` falls back to that default for the same reason.
///
/// Compiled only into the builds that can use it and into test builds, so a
/// release binary carries neither this decision nor the plugin it feeds.
#[cfg(any(feature = "dev-tools", test))]
pub fn mcp_bridge_bind_address<'a>(enable: Option<&str>, bind: Option<&'a str>) -> Option<&'a str> {
    if !cfg!(feature = "dev-tools") || enable != Some("1") {
        return None;
    }
    Some(match bind {
        Some(addr) if !addr.trim().is_empty() => addr,
        _ => "127.0.0.1",
    })
}

/// Base port the MCP bridge scans upward from.
///
/// The argument is the raw value of `PV_MCP_BRIDGE_PORT`; `None` means unset,
/// and so does an unparseable or out-of-range value, because the plugin default
/// is a working configuration and refusing to start the bridge over a typo
/// would be the worse failure.
///
/// Concurrent app instances on one host need predictable ports: the plugin
/// scans `base_port..base_port + 100` and takes the first free one, so without
/// a per-instance base the port an instance ends up on depends on launch order.
/// `crates/e2e-tests/src/instance.rs` derives one base per instance number and
/// this is the var it sets.
#[cfg(any(feature = "dev-tools", test))]
pub fn mcp_bridge_base_port(port: Option<&str>) -> Option<u16> {
    port?.trim().parse().ok().filter(|p| *p != 0)
}

#[cfg(test)]
mod tests {
    use super::{mcp_bridge_base_port, mcp_bridge_bind_address, single_instance_guard_enabled};

    /// The release-leak regression: without the `e2e` feature compiled in, no
    /// value of `PV_E2E_INSTANCE_ID` may disable the guard.
    #[test]
    #[cfg(not(feature = "e2e"))]
    fn env_var_alone_cannot_disable_the_guard() {
        assert!(single_instance_guard_enabled(true));
        assert!(single_instance_guard_enabled(false));
    }

    /// In an `e2e` build the bypass still requires the runtime marker, so a
    /// developer running that build by hand keeps the guard.
    #[test]
    #[cfg(feature = "e2e")]
    fn e2e_build_bypasses_only_when_marker_is_set() {
        assert!(!single_instance_guard_enabled(true));
        assert!(single_instance_guard_enabled(false));
    }

    /// The release-leak regression: without `dev-tools` compiled in, no
    /// combination of the two variables may start the bridge.
    #[test]
    #[cfg(not(feature = "dev-tools"))]
    fn env_vars_alone_cannot_start_the_bridge() {
        assert_eq!(mcp_bridge_bind_address(Some("1"), None), None);
        assert_eq!(mcp_bridge_bind_address(Some("1"), Some("0.0.0.0")), None);
    }

    /// Unset, and every value that is not exactly `1`, leaves the port closed.
    #[test]
    #[cfg(feature = "dev-tools")]
    fn only_the_exact_value_one_starts_the_bridge() {
        for enable in [None, Some(""), Some("0"), Some("true"), Some("yes"), Some("1 "), Some("01")]
        {
            assert_eq!(
                mcp_bridge_bind_address(enable, None),
                None,
                "enable={enable:?} must not start the bridge"
            );
        }
        assert_eq!(mcp_bridge_bind_address(Some("1"), None), Some("127.0.0.1"));
    }

    /// The bind default is loopback rather than the plugin's `0.0.0.0`, and an
    /// explicit address (the WSL-client, Windows-host case) is honoured.
    #[test]
    #[cfg(feature = "dev-tools")]
    fn bind_defaults_to_loopback_and_honours_an_explicit_address() {
        assert_eq!(mcp_bridge_bind_address(Some("1"), None), Some("127.0.0.1"));
        assert_eq!(mcp_bridge_bind_address(Some("1"), Some("   ")), Some("127.0.0.1"));
        assert_eq!(mcp_bridge_bind_address(Some("1"), Some("0.0.0.0")), Some("0.0.0.0"));
        assert_eq!(mcp_bridge_bind_address(Some("1"), Some("192.168.1.20")), Some("192.168.1.20"));
    }

    /// A usable value pins the base port; every unusable one falls back to the
    /// plugin default rather than failing the launch.
    #[test]
    fn base_port_is_pinned_only_by_a_usable_value() {
        assert_eq!(mcp_bridge_base_port(Some("9323")), Some(9323));
        assert_eq!(mcp_bridge_base_port(Some(" 9423 ")), Some(9423));
        for raw in [None, Some(""), Some("   "), Some("0"), Some("-1"), Some("70000"), Some("92a3")]
        {
            assert_eq!(mcp_bridge_base_port(raw), None, "port={raw:?} must not pin the base port");
        }
    }
}
