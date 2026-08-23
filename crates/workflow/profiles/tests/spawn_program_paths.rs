// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Every process this crate spawns names its program by absolute path, so the
//! program that runs is not selected by the inherited `PATH`.
//!
//! Asserted against the source text: the macOS spawn sites are `cfg`-gated and
//! reach the real OS, so there is no seam at which a test can observe the
//! resolved program without spawning a binary (constitution III).

const SOURCES: &[(&str, &str)] = &[
    ("discover.rs", include_str!("../src/discover.rs")),
    ("launch.rs", include_str!("../src/launch.rs")),
];

#[test]
fn every_spawned_program_is_an_absolute_path() {
    for (name, src) in SOURCES {
        for (offset, _) in src.match_indices("Command::new(\"") {
            let program = src[offset + "Command::new(\"".len()..]
                .split('"')
                .next()
                .expect("a literal program name is terminated by a quote");
            assert!(
                program.starts_with('/'),
                "{name}: spawns '{program}' by bare program name; use an absolute path",
            );
        }
    }
}
