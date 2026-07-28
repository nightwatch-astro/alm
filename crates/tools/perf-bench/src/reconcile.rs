// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario: `reconcile_root_frames` — spec 048 T043a, SC-005 throughput half.
//!
//! Drives the real `app_core::frame_inventory::run_reconcile` over a root of
//! `RECONCILE_N` (default 10,000) present frames. SC-005 has two halves:
//! "completes" (measured here as throughput + statement count) and "reports
//! progress throughout". The second half needs incremental
//! `progress_pct` streaming, which
//! `crates/contracts/core/src/inventory_frame.rs:110-112` documents as a
//! future long-running-operation extension; this scenario does not measure it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use app_core::frame_inventory::run_reconcile;
use app_core_targets::frame_writer::upsert_frame_record;
use audit::bus::EventBus;
use contracts_core::inventory_frame::{InventoryReconcileRunRequest, ReconcileReason};
use persistence_core::Database;
use persistence_targets::repositories::q_targets_ingest::insert_library_root_mirror;

use crate::support::{env_size, print_result};

/// Frames per subdirectory. Reconcile walks the whole root once via
/// `fs_pathsafe::real_files_under`, so the fan-out only affects `read_dir`
/// batch sizes, not the number of stat calls.
const FRAMES_PER_DIR: usize = 200;

/// Run the reconcile scenario against its own tempdir + database.
///
/// Each scenario owns its DB so a scenario's statement count is not perturbed
/// by rows another scenario left behind.
pub async fn run(counter: &Arc<AtomicU64>) {
    let n = env_size("RECONCILE_N", 10_000);

    let dir = tempfile::tempdir().expect("reconcile tempdir");
    let root_path = dir.path().to_str().expect("utf8 tempdir path").to_owned();

    let db_dir = tempfile::tempdir().expect("reconcile db tempdir");
    let db_path = db_dir.path().join("reconcile.db");
    let db = Database::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("db connect");
    db.migrate().await.expect("migrations");

    let root_id = "perf-reconcile-root";
    insert_library_root_mirror(db.pool(), root_id, &root_path, "2026-07-01T00:00:00Z")
        .await
        .expect("insert library root");

    // Write real files and matching `file_record` rows. `size_bytes` is
    // recorded correctly so the timed pass exercises the steady-state
    // "everything present, nothing changed" path — the SC-005 shape (a
    // scheduled/on-open pass over an unchanged library), which is also the
    // cheapest per frame and therefore the honest throughput floor.
    for i in 0..n {
        let sub = format!("session_{:04}", i / FRAMES_PER_DIR);
        std::fs::create_dir_all(dir.path().join(&sub)).expect("create session dir");
        let relative = format!("{sub}/frame_{i:05}.fits");
        let size = 1024 + (i % 16);
        std::fs::write(dir.path().join(&relative), vec![0u8; size]).expect("write frame");
        upsert_frame_record(
            db.pool(),
            root_id,
            &relative,
            i64::try_from(size).expect("size fits i64"),
            "2026-07-01T00:00:00Z",
            "classified",
        )
        .await
        .expect("upsert frame record");
    }

    let bus = EventBus::with_pool(db.pool().clone());
    let req = InventoryReconcileRunRequest {
        root_id: root_id.to_owned(),
        reason: ReconcileReason::OnDemand,
    };

    counter.store(0, Ordering::Relaxed);
    let t0 = Instant::now();
    let resp = run_reconcile(db.pool(), &bus, &req).await.expect("run_reconcile");
    let wall_ms = t0.elapsed().as_millis();
    let stmts = counter.load(Ordering::Relaxed);

    print_result(
        "reconcile_root_frames",
        n,
        wall_ms,
        &serde_json::json!({
            "scanned": resp.scanned,
            "present": resp.present,
            "newly_missing": resp.newly_missing,
            "size_backfilled": resp.size_backfilled,
            "sqlx_stmts": stmts,
        }),
    );
}
