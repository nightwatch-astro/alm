// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Statement-count regression for the batched on-attach reconciliation phases
//! (kyo7.54).
//!
//! Each phase must issue a bounded number of SQL statements regardless of how
//! many artifacts it touches. At N = 60 the batched phases measure 1 (seen),
//! 3 (missing), and 6 (detect) statements. The per-row implementation issued
//! one UPDATE/INSERT plus one durable audit insert per artifact — 60, 62, and
//! 240+ respectively — so it fails every bound asserted here.
//!
//! Counting technique mirrors `crates/tools/perf-bench/src/main.rs`: sqlx emits
//! one tracing event per statement execution under the `sqlx` target, so a
//! counting layer measures DB pressure without instrumenting production code.

mod support;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use app_core::lifecycle::artifact::{
    detect_batch, mark_missing_batch, touch_seen, DetectedFile, GoneArtifact,
};
use persistence_plans::repositories::artifacts::{
    insert_artifact_if_absent, list_artifacts_for_project, InsertArtifact,
};
use tracing_subscriber::layer::SubscriberExt as _;

const N: usize = 60;
const PROJECT_ID: &str = "proj-batch";

/// Counts tracing events emitted under the `sqlx` target (one per statement
/// execution). `max_level_hint` must claim DEBUG or the registry drops sqlx
/// events before this layer sees them.
struct SqlxCounterLayer(Arc<AtomicU64>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SqlxCounterLayer {
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(tracing::level_filters::LevelFilter::DEBUG)
    }

    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target().starts_with("sqlx") {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

async fn seed_artifact(pool: &sqlx::SqlitePool, id: &str, path: &str) {
    insert_artifact_if_absent(
        pool,
        InsertArtifact {
            id,
            project_id: PROJECT_ID,
            tool_launch_id: None,
            path,
            kind: "intermediate",
            tool: "pixinsight",
            detected_at: "2026-07-01T00:00:00Z",
            state: "present",
            classification_confidence: 0.5,
            classification_source: "rule",
            size_bytes: 1024,
            file_mtime: "2026-07-01T00:00:00Z",
            content_hash: None,
        },
    )
    .await
    .expect("seed artifact");
}

#[tokio::test]
async fn reconcile_phases_issue_bounded_statement_counts() {
    let counter = Arc::new(AtomicU64::new(0));
    let subscriber = tracing_subscriber::registry().with(SqlxCounterLayer(Arc::clone(&counter)));
    tracing::subscriber::set_global_default(subscriber).expect("set subscriber");

    let (db, _repo, bus) = support::setup().await;
    let pool = db.pool();

    let seen_ids: Vec<String> = (0..N).map(|i| format!("seen-{i}")).collect();
    for (i, id) in seen_ids.iter().enumerate() {
        seed_artifact(pool, id, &format!("output/seen_{i}.xisf")).await;
    }
    let gone: Vec<GoneArtifact> = (0..N)
        .map(|i| GoneArtifact { id: format!("gone-{i}"), path: format!("output/gone_{i}.xisf") })
        .collect();
    for g in &gone {
        seed_artifact(pool, &g.id, &g.path).await;
    }

    // ── Seen phase: one UPDATE for the whole set, no audit rows ──────────────
    counter.store(0, Ordering::Relaxed);
    touch_seen(pool, &seen_ids).await.expect("seen phase");
    let seen_stmts = counter.load(Ordering::Relaxed);
    assert!(
        seen_stmts <= 2,
        "seen phase must batch {N} rows into a bounded statement count, got {seen_stmts}"
    );

    // ── Missing phase: one UPDATE + one audit tx ─────────────────────────────
    counter.store(0, Ordering::Relaxed);
    mark_missing_batch(pool, &bus, PROJECT_ID, &gone).await.expect("missing phase");
    let missing_stmts = counter.load(Ordering::Relaxed);
    assert!(
        missing_stmts <= 5,
        "missing phase must batch {N} rows and their audit events, got {missing_stmts}"
    );

    // ── Detect phase: one insert tx + one audit tx ───────────────────────────
    let files: Vec<DetectedFile> = (0..N)
        .map(|i| DetectedFile {
            path: format!("output/new_{i}.xisf"),
            size_bytes: 2048,
            file_mtime: "2026-07-02T00:00:00Z".to_owned(),
            detected_at: "2026-07-02T00:00:00Z".to_owned(),
        })
        .collect();
    counter.store(0, Ordering::Relaxed);
    detect_batch(pool, &bus, PROJECT_ID, "pixinsight", &files).await.expect("detect phase");
    let detect_stmts = counter.load(Ordering::Relaxed);
    assert!(
        detect_stmts <= 8,
        "detect phase must batch {N} inserts and 2 events each, got {detect_stmts}"
    );

    // Behaviour, not just statement count: every phase actually landed.
    let rows = list_artifacts_for_project(pool, PROJECT_ID, &[]).await.expect("list");
    assert_eq!(rows.len(), N * 3, "all seeded and detected rows present");
    assert_eq!(
        rows.iter().filter(|r| r.state == "missing").count(),
        N,
        "gone artifacts transitioned to missing"
    );
    assert_eq!(
        rows.iter().filter(|r| r.path.starts_with("output/new_")).count(),
        N,
        "detected files inserted"
    );
}
