// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

fn main() {
    // `mut` is only exercised by the `#[cfg(windows)]` block and the
    // capability-glob branch below.
    #[allow(unused_mut)]
    let mut attributes = tauri_build::Attributes::new();

    // A capability file cannot be `cfg`-gated, so the dev-only grants live in
    // `capabilities/dev/` and this narrows the default `./capabilities/**/*`
    // glob to the top level when `dev-tools` is off. Those grants name
    // permissions from plugins the same feature gates, and an unknown
    // permission is a build error, so the two gates cannot drift apart.
    if std::env::var_os("CARGO_FEATURE_DEV_TOOLS").is_none() {
        println!("cargo:rerun-if-changed=capabilities");
        attributes = attributes.capabilities_path_pattern("./capabilities/*.json");
    }

    // Windows-only workaround for a known upstream tauri-build/embed-resource
    // limitation: `tauri_build::build()`'s default app-manifest embedding uses
    // `rustc-link-arg-bins`, which only links the manifest into the crate's
    // own [[bin]] target — never into `tests/*.rs` integration test binaries.
    // Without the Common-Controls-v6 manifest, any test binary that touches
    // `tauri::test::mock_builder()`/`mock_context()` (this crate's
    // tests/commands.rs) crashes on startup on Windows with
    // STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) before any test runs.
    // See https://github.com/tauri-apps/tauri/issues/13419 and
    // https://github.com/tauri-apps/tauri/issues/13954. Embedding the same
    // manifest ourselves via `cargo:rustc-link-arg` (no `-bins` suffix) applies
    // it to every linked artifact, including tests.
    #[cfg(windows)]
    {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        add_windows_manifest();
    }

    tauri_build::try_build(attributes).expect("tauri_build::try_build failed");
}

#[cfg(windows)]
fn add_windows_manifest() {
    static WINDOWS_MANIFEST_FILE: &str = "windows-app-manifest.xml";

    let manifest = std::env::current_dir().unwrap().join(WINDOWS_MANIFEST_FILE);

    println!("cargo:rerun-if-changed={}", manifest.display());
    // Embed the Windows application manifest file into every binary produced
    // from this crate (bins, cdylib, and test executables alike).
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.to_str().unwrap());
    // Turn linker warnings (e.g. a malformed manifest) into hard errors.
    println!("cargo:rustc-link-arg=/WX");
}
