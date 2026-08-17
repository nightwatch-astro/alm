#![allow(clippy::doc_markdown)]
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared real-backend integration-test harness — feature 037 (T005).
//!
//! Real SQLite + real migrations + a wired `SqliteLifecycleRepository` and
//! `EventBus`, mirroring the established pattern in `transition_apply.rs`.
//! Layer-1 tests use this instead of re-declaring `setup()` per file.
//!
//! DB provisioning and low-level row helpers delegate to
//! `persistence_lifecycle::test_support`; app-layer concerns (project-root
//! registration via the sanctioned first_run_repo API) remain here.
#![allow(dead_code)]

use std::future::Future;
use std::time::Duration;

use audit::bus::EventBus;
use domain_core::first_run::{OrganizationState, RegisterSourceRequest, ScanDepth, SourceKind};
use persistence_core::Database;
use persistence_lifecycle::repositories::first_run as first_run_repo;
use persistence_lifecycle::repositories::lifecycle::SqliteLifecycleRepository;

/// Provision an isolated in-memory SQLite DB with all migrations applied and a
/// repository/event-bus wired to it. Real backend, no mocks.
pub async fn setup() -> (Database, SqliteLifecycleRepository, EventBus) {
    let db = persistence_core::test_support::setup_db().await;
    let bus = EventBus::with_pool(db.pool().clone());
    let repo = SqliteLifecycleRepository::new(db.pool().clone(), bus.clone());
    (db, repo, bus)
}

/// Platform-absolute path of the project folder registered by
/// [`register_project_root`]. Tests submit project paths RELATIVE to this
/// root: a leading-slash path like `/library/...` is absolute on Unix but NOT
/// on Windows (no drive letter), so it would fall into the relative-anchoring
/// branch of `project_setup::create` and be rejected there. Registering a
/// project root and using relative request paths is portable.
#[cfg(windows)]
pub const TEST_PROJECT_ROOT: &str = "C:/library/projects-root";
#[cfg(not(windows))]
pub const TEST_PROJECT_ROOT: &str = "/library/projects-root";

/// Register a project-kind source so relative project request paths have an
/// anchor (mirrors the first-run wizard registering a project folder). Goes
/// through the sanctioned `first_run` repository API rather than raw SQL so
/// column additions to `registered_sources` are automatically handled.
pub async fn register_project_root(pool: &sqlx::SqlitePool, path: &str) {
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
    .expect("register_project_root failed");
}

/// Seed a minimal `target` row into the legacy `target` table.
///
/// Delegates to `persistence_lifecycle::test_support::insert_target`.
pub async fn insert_target(pool: &sqlx::SqlitePool, id: &str) {
    persistence_lifecycle::test_support::insert_target(pool, id).await;
}

/// Seed a minimal `project` row in the given lifecycle state.
///
/// Delegates to `persistence_lifecycle::test_support::insert_project`.
/// The `_target` parameter is accepted for call-site compatibility but unused
/// (the `projects` table has no mandatory FK to `target`).
pub async fn insert_project(pool: &sqlx::SqlitePool, id: &str, _target: &str, state: &str) {
    persistence_lifecycle::test_support::insert_project(pool, id, state).await;
}

// ── Observable-condition poll helpers ─────────────────────────────────────────
//
// These replace fixed `tokio::time::sleep` calls that were used to wait for
// background executor or EventBus listener tasks to finish. Fixed sleeps are
// fragile on loaded CI runners where Tokio scheduler latency can cause the
// background task to miss the window. Polling every 25 ms with a 2 s cap
// makes each test wait only as long as the work actually takes.

/// Poll `check` every 25 ms until it returns `Some(T)` or the 2-second
/// deadline expires (which panics with `deadline_msg`).
///
/// Use this for conditions other than plan-terminal state (e.g. a row count,
/// a file existing, an inbox-item state). For plan executor completion use
/// [`wait_plan_terminal`], which polls the same way but requires no closure.
pub async fn poll_until<F, Fut, T>(mut check: F, deadline_msg: &str) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    // 10 s covers the new wal_checkpoint(TRUNCATE) on resume/start paths
    // (added by the durability commit) under slow CI runners (Ubuntu).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = check().await {
            return v;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "poll_until timed out after 10 s: {deadline_msg}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Wait for a plan's `state` column to leave `"applying"` (i.e. reach any
/// terminal state: `applied`, `partially_applied`, `failed`, `paused`,
/// `cancelled`). Polls every 25 ms, panics after 2 s.
///
/// Replaces fixed `sleep(300 ms)` / `sleep(200 ms)` waits that followed
/// `apply_plan()` or `resume_plan()` calls and were fragile under parallel
/// nextest runs on loaded CI runners.
pub async fn wait_plan_terminal(pool: &sqlx::SqlitePool, plan_id: &str) {
    poll_until(
        || async {
            let row: Option<(String,)> = sqlx::query_as("SELECT state FROM plans WHERE id = ?")
                .bind(plan_id)
                .fetch_optional(pool)
                .await
                .expect("poll plan state");
            match row {
                Some((s,)) if s != "applying" => Some(()),
                _ => None,
            }
        },
        &format!("plan {plan_id} never left 'applying' state"),
    )
    .await;
}

/// Wait for ONE plan item to reach `expected` state. Polls every 25 ms, panics
/// after 2 s.
///
/// Use this instead of [`wait_plan_terminal`] whenever the awaited work is a
/// single item — most importantly after a retry. `wait_plan_terminal` returns as
/// soon as the PLAN is not `applying`, which can already be true from an earlier
/// attempt: it then returns instantly without the retried item having run, and
/// the assertions that follow race the executor. That is a real flake source
/// (astro-plan-9lkf), and the failure looks like a product bug rather than a
/// test that waited for the wrong thing.
///
/// Waiting on the item's own state is what makes the wait mean "the thing I
/// triggered finished", not merely "something finished at some point".
pub async fn wait_item_state(pool: &sqlx::SqlitePool, item_id: &str, expected: &str) {
    poll_until(
        || async {
            let row: Option<(String,)> =
                sqlx::query_as("SELECT item_state FROM plan_items WHERE id = ?")
                    .bind(item_id)
                    .fetch_optional(pool)
                    .await
                    .expect("poll plan item state");
            match row {
                Some((s,)) if s == expected => Some(()),
                _ => None,
            }
        },
        &format!("plan item {item_id} never reached '{expected}' state"),
    )
    .await;
}
