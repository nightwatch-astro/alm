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

#[cfg(test)]
mod tests {
    use super::single_instance_guard_enabled;

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
}
