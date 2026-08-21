// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Background tasks spawned once at startup by `run_app`: the stale-dependent
//! propagator (spec 002 FR-003) and the SIMBAD ingest-resolution drain
//! (spec 035 US4).

use audit::bus::EventBus;
use audit::stale_propagator::{resolve_project_dependents_hook, StalePropagator};
use sqlx::SqlitePool;

/// Spawn the spec 002 FR-003 (#713) stale-dependent propagator.
///
/// Never spawned in production before this — projections/prepared sources
/// only recomputed their staleness through calling code that happened to
/// run the exact update, not automatically on the owning project's
/// transition (research.md §6 fan-out).
pub fn spawn_stale_dependent_propagator(
    pool: SqlitePool,
    bus: &EventBus,
) -> tokio::task::JoinHandle<()> {
    StalePropagator::new().with_hook(resolve_project_dependents_hook(pool)).spawn(bus)
}

/// Spawn the spec-035 US4/T043 background ingest-resolution drain.
///
/// This is now a **backstop**, not the primary resolution path (issue #1256):
/// the plan-applied event path (`app_core::inbox::plan_listener::
/// ingest_light_frames_if_applicable`) triggers the same
/// [`app_core::ingest_resolution::drain_and_backfill_once`] pass immediately
/// after a plan's light frames are ingested, so a session's
/// `canonical_target_id` no longer waits on this timer in the common case.
/// This loop still exists to catch anything the event path missed — a
/// dropped/lagged broadcast event, an app crash mid-resolution, or a resolver
/// that was offline when the plan applied — within a bounded interval.
/// Failures are logged, never fatal — the next pass retries.
///
/// A pass that resolved at least one queued target raises an OS notification
/// (spec 051 US8); the common empty tick stays silent.
pub fn spawn_ingest_resolution_drain(
    app: tauri::AppHandle,
    pool: SqlitePool,
    bus: EventBus,
    resolve_cache: targeting_resolver::simbad::ResolveCache,
) {
    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(30);
        loop {
            tokio::time::sleep(interval).await;
            let summary =
                app_core::ingest_resolution::drain_and_backfill_once(&pool, &bus, &resolve_cache)
                    .await;
            crate::bootstrap::notify::ingest_drain_completed(&app, &summary);
        }
    });
}
