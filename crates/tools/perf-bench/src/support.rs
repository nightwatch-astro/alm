// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared measurement primitives: the sqlx statement counter and the
//! one-JSON-object-per-scenario writer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing_subscriber::Layer;

/// Counts tracing events whose target is exactly `sqlx::query`.
///
/// sqlx emits one event per statement execution under the `sqlx::query` target
/// (`sqlx-core/src/logger.rs`), which gives a statement-count proxy for DB
/// pressure without adding any instrumentation dependency inside the production
/// crates. Both the normal and the slow-statement branch use that same target,
/// so the count is one per statement at either level.
///
/// The match is exact rather than a `starts_with("sqlx")` prefix. `sqlx::pool::
/// acquire` also matches that prefix and fires only when a connection acquire
/// exceeds `acquire_slow_threshold` (`sqlx-core/src/pool/inner.rs`), which is
/// load-dependent: it never fires on an idle machine and does fire on a
/// contended CI runner. Counting it made `sqlx_stmts` read one higher under
/// load, so the same commit produced 10 and 11 on separate runs and
/// `scripts/check-perf-baseline.sh` hard-failed an unrelated PR (astro-plan-hgh6).
///
/// The inner `Arc<AtomicU64>` lets the layer and the harness share the same
/// counter. The newtype wrapper is required by Rust's orphan rule: `Layer` is
/// a foreign trait and `Arc` is a foreign type, so the impl must be on a
/// local type.
pub struct SqlxCounterLayer(Arc<AtomicU64>);

impl SqlxCounterLayer {
    pub fn new() -> (Self, Arc<AtomicU64>) {
        let inner = Arc::new(AtomicU64::new(0));
        (Self(inner.clone()), inner)
    }
}

impl<S: tracing::Subscriber> Layer<S> for SqlxCounterLayer {
    // Declare DEBUG interest so the registry does not drop sqlx query events
    // before they reach this layer, even when the fmt layer's EnvFilter is set
    // to a higher level (e.g. "error").
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(tracing::level_filters::LevelFilter::DEBUG)
    }

    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() == "sqlx::query" {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Print one scenario result as a single JSON object on stdout.
///
/// `scripts/check-perf-baseline.sh` parses each line independently and reads
/// `scenario`, `sqlx_stmts`, and `wall_ms`; every other key is carried through
/// into the baseline file unread.
pub fn print_result(scenario: &str, n: usize, wall_ms: u128, extra: &serde_json::Value) {
    let mut obj = serde_json::json!({
        "scenario": scenario,
        "n": n,
        "wall_ms": wall_ms,
    });
    if let serde_json::Value::Object(ref extra_map) = extra {
        if let serde_json::Value::Object(ref mut m) = obj {
            m.extend(extra_map.clone());
        }
    }
    println!("{obj}");
}

/// Read a `usize` scenario-size knob from the environment.
pub fn env_size(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt as _;

    /// Both sqlx logging branches use the `sqlx::query` target and differ only in
    /// level, so a slow statement must still count as exactly one statement.
    #[test]
    fn counts_a_query_event_at_either_level() {
        let (layer, counter) = SqlxCounterLayer::new();
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "sqlx::query", summary = "one");
            tracing::warn!(target: "sqlx::query", summary = "two");
        });

        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    /// `sqlx::pool::acquire` fires on a slow connection acquire, which depends on
    /// machine load rather than on the query plan. A `starts_with("sqlx")`
    /// predicate counts it and makes the baseline gate flake (astro-plan-hgh6).
    #[test]
    fn ignores_targets_that_are_not_statements() {
        let (layer, counter) = SqlxCounterLayer::new();
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "sqlx::pool::acquire", "slow acquire");
            tracing::debug!(target: "sqlx_sqlite::something", "unrelated");
        });

        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }
}
