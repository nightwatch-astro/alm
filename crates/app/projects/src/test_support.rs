// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared test fixtures for project-path anchoring (Constitution I).
//!
//! Cross-platform note: a leading-slash path like `/library/...` is absolute
//! on Unix but NOT on Windows (`Path::is_absolute` requires a drive or UNC
//! prefix there), so it would fall into the relative-anchoring branch of
//! `project_setup::create` and be rejected. Tests therefore either submit
//! paths relative to the registered [`TEST_PROJECT_ROOT`], or build
//! platform-absolute paths with [`abs`].

use domain_core::first_run::{OrganizationState, RegisterSourceRequest, ScanDepth, SourceKind};
use persistence_lifecycle::repositories::first_run as first_run_repo;
use sqlx::SqlitePool;

/// Platform-absolute path of the project folder registered by test setups.
#[cfg(windows)]
pub(crate) const TEST_PROJECT_ROOT: &str = "C:/library/projects-root";
#[cfg(not(windows))]
pub const TEST_PROJECT_ROOT: &str = "/library/projects-root";

/// Make a Unix-style test path absolute on the current platform.
pub fn abs(path: &str) -> String {
    fs_pathsafe::test_support::abs(path)
}

/// Register a project-kind source so relative request paths have an anchor
/// (mirrors the first-run wizard registering a project folder). Goes through
/// the sanctioned `first_run` repository (DB-boundary rule: no raw SQL outside
/// `crates/persistence/db`).
pub async fn register_project_root(pool: &SqlitePool, path: &str) {
    first_run_repo::register_source(
        pool,
        &RegisterSourceRequest {
            kind: SourceKind::Project,
            path: path.to_owned(),
            kind_subtype: None,
            scan_depth: ScanDepth::Recursive,
            organization_state: OrganizationState::Organized,
        },
    )
    .await
    .unwrap();
}
