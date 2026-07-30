// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario: `plan_apply_progress` — spec 025 T045.
//!
//! Applies a real 10,000-item plan through `app_core::plan_apply::apply_plan`
//! and measures how stale the long-operation progress projection gets. Items
//! use the `catalogue` action (record-in-place, no filesystem mutation), so
//! the measurement isolates the progress/persistence path from disk I/O.
//!
//! `max_progress_gap_ms` is the metric T045's "within 50 ms of state
//! transition" applies to: the longest interval between two consecutive
//! operation events reaching the sink. Progress is emitted one envelope per
//! group-commit flush window
//! (`crates/app/core/src/plan_apply/callbacks.rs`: 100 items or 250 ms,
//! whichever comes first), so this gap is bounded by the flush policy, not by
//! per-item work.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use app_core::plan_apply::apply_plan;
use audit::bus::EventBus;
use contracts_core::OperationEventType;
use persistence_core::Database;
use persistence_plans::repositories::plans as plans_repo;

use crate::support::{env_size, print_result};

const PLAN_ID: &str = "perf-plan-progress";
const APPROVAL_TOKEN: &str = "perf-token";

/// Run the plan-apply progress scenario against its own tempdir database.
pub async fn run(counter: &Arc<AtomicU64>) {
    let n = env_size("PLAN_N", 10_000);

    let db_dir = tempfile::tempdir().expect("plan db tempdir");
    let db_path = db_dir.path().join("plan.db");
    let db = Database::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("db connect");
    db.migrate().await.expect("migrations");

    seed_approved_plan(&db, n).await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(Instant, OperationEventType)>();
    let sink: app_core::plan_apply::OperationEventSink = Arc::new(move |event| {
        // Best-effort, matching the production Tauri sink: a closed receiver
        // must not fail the run.
        let _ = tx.send((Instant::now(), event.event_type));
    });

    let bus = EventBus::with_pool(db.pool().clone());

    counter.store(0, Ordering::Relaxed);
    let t0 = Instant::now();
    apply_plan(db.pool(), &bus, PLAN_ID, APPROVAL_TOKEN, Some(sink)).await.expect("apply_plan");

    let mut first_event_ms = 0u128;
    let mut last_event = t0;
    let mut max_gap_ms = 0u128;
    let mut progress_events = 0usize;
    let mut events = 0usize;

    while let Some((at, event_type)) = rx.recv().await {
        if events == 0 {
            first_event_ms = at.duration_since(t0).as_millis();
        }
        events += 1;
        max_gap_ms = max_gap_ms.max(at.duration_since(last_event).as_millis());
        last_event = at;
        if event_type == OperationEventType::Progress {
            progress_events += 1;
        }
        if matches!(event_type, OperationEventType::Completed | OperationEventType::Failed) {
            break;
        }
    }

    let wall_ms = t0.elapsed().as_millis();
    let stmts = counter.load(Ordering::Relaxed);

    let status = plans_repo::get_plan(db.pool(), PLAN_ID, false).await.expect("get_plan");

    print_result(
        "plan_apply_progress",
        n,
        wall_ms,
        &serde_json::json!({
            "items_applied": status.items_applied,
            "plan_state": status.state,
            "operation_events": events,
            "progress_events": progress_events,
            "first_event_ms": first_event_ms,
            "max_progress_gap_ms": max_gap_ms,
            "sqlx_stmts": stmts,
        }),
    );
}

/// Insert an approved plan with `n` `catalogue` items.
async fn seed_approved_plan(db: &Database, n: usize) {
    plans_repo::insert_plan(
        db.pool(),
        &plans_repo::InsertPlan {
            id: PLAN_ID,
            title: "Perf progress plan",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .expect("insert_plan");

    for i in 0..n {
        plans_repo::insert_plan_item(
            db.pool(),
            &plans_repo::InsertPlanItem {
                id: &format!("{PLAN_ID}-item-{i}"),
                plan_id: PLAN_ID,
                item_index: i64::try_from(i + 1).expect("index fits i64"),
                name: "frame.fits",
                action: "catalogue",
                from_root_id: None,
                from_relative_path: &format!("perf/raw/frame-{i:05}.fits"),
                to_root_id: None,
                to_relative_path: &format!("perf/raw/frame-{i:05}.fits"),
                reason: "perf",
                protection: "normal",
                linked_entity: None,
                provenance_json: None,
                archive_path: None,
                source_id: None,
                category: None,
            },
        )
        .await
        .expect("insert_plan_item");
    }

    plans_repo::update_plan_state(db.pool(), PLAN_ID, "ready_for_review")
        .await
        .expect("update_plan_state");
    plans_repo::set_approved(db.pool(), PLAN_ID, "2026-07-01T00:00:00Z", APPROVAL_TOKEN)
        .await
        .expect("set_approved");
}
