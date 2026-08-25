// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Platform-absolute fixture paths for containment tests.
//!
//! `Path::is_absolute` requires a drive or UNC prefix on Windows, so a
//! leading-slash literal like `/mnt/library` is relative there and every
//! containment entry point refuses it as
//! [`contain::ContainmentError::RootNotAbsolute`](crate::contain::ContainmentError::RootNotAbsolute).
//! Tests that assert containment verdicts on made-up paths therefore build
//! their roots through [`abs`] instead of writing the literal.

/// Make a Unix-style test path absolute on the current platform.
#[must_use]
pub fn abs(path: &str) -> String {
    if cfg!(windows) {
        format!("C:{path}")
    } else {
        path.to_owned()
    }
}

/// [`abs`] as a `PathBuf`.
#[must_use]
pub fn abs_path(path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(abs(path))
}
