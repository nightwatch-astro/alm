// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `target.cache.clear` support (spec 052 P1 FR-002): wipe the shared redb
//! resolve cache and re-warm it from the bundled seed + existing durable
//! `canonical_target` rows. Never touches `canonical_target` itself — the
//! redb cache is an explicitly reproducible projection (constitution §V).
//!
//! `simbad_resolver::Cache` exposes no delete-all primitive, so "clear" is
//! file-level: drop every handle to the old redb `Database`, delete the file,
//! reopen. [`AppState::resolve_cache`]'s write lock ensures no new reader
//! starts mid-swap; a reader that already cloned the handle just before the
//! swap keeps working against the (now-orphaned) old file until it finishes,
//! which is why the old handle is dropped before the file delete — a
//! concurrent straggler can still transiently make the delete fail on
//! Windows (sharing violation), surfaced as an error rather than silently
//! leaving a stale cache in place.
//!
//! The re-warm (bundled seed + durable `canonical_target` rows, chunked into
//! ~1000-entry `Cache::upsert_batch` write transactions per phase since spec
//! 052 P4/#695 + #818 follow-up, `Eventual`-durability since #818's second
//! follow-up — see [`targeting_resolver::simbad::ResolveCache::open`]) runs
//! as a **background task** spawned right after the swap, mirroring the
//! startup warm in `lib.rs` (issue #695: awaiting it inline froze the
//! `target.cache.clear` IPC call — and, because the write lock used to stay
//! held for the whole warm, every other resolve cache reader too — for
//! minutes on a debug build). The background task never takes
//! `AppState::resolve_cache`'s lock; it is handed the fresh cache's own
//! handle (cloned out before the swap, so it keeps warming the same
//! underlying store regardless of what `AppState::resolve_cache` is swapped
//! to next), plus a second clone of the whole [`ResolveCache`] (not just its
//! erased `.cache()`) to call `ResolveCache::flush` on once both phases
//! finish — the one fsync that persists every `Eventual` chunk.
//!
//! Chunking each phase's warm means nothing in a given chunk is visible to a
//! reader until THAT chunk's transaction commits (no more per-entry partial
//! visibility, but no more one-giant-atomic-transaction either) — a
//! `target.search` query racing this window can get a legitimate-looking
//! empty result for an object the seed does contain simply because its
//! chunk hasn't committed yet (issue #818). A
//! [`crate::commands::lifecycle::CacheWarmingGuard`] set true before this
//! spawn and moved into the task (so a panic mid-warm still clears it) makes
//! `AppState::cache_warming` true for the spawn's whole duration, so
//! `target.search` can surface that in-flight state and the frontend can
//! retry instead of freezing on a stale empty result.
//!
//! [`ResolveCache`]: targeting_resolver::simbad::ResolveCache

use contracts_core::ContractError;

use crate::commands::lifecycle::AppState;

/// Namespace seed for redb-cache ids — MUST match
/// `targeting_resolver::simbad`'s production seed exactly (asserted there by
/// `namespace_matches_sqlite_identity_derivation`) so re-warmed ids stay
/// consistent with anything already promoted to `canonical_target`.
const NAMESPACE_SEED: &str = "astro-plan.targets";

/// Open the shared redb resolve cache at `path` (creating it if missing).
///
/// Falls back to an ephemeral in-memory cache on failure (e.g. a corrupt or
/// unwritable file) rather than hard-crashing startup — the app still works,
/// just without a persistent typeahead cache until the next restart.
///
/// # Panics
///
/// Panics only if the in-memory fallback itself cannot be constructed, which
/// `simbad_resolver`'s `InMemoryBackend` never fails to do in practice.
#[must_use]
pub fn open_or_in_memory(path: &std::path::Path) -> targeting_resolver::simbad::ResolveCache {
    targeting_resolver::simbad::ResolveCache::open(path).unwrap_or_else(|e| {
        tracing::warn!(
            path = %path.display(),
            "failed to open the resolve cache file, falling back to in-memory: {e}"
        );
        targeting_resolver::simbad::ResolveCache::in_memory()
            .expect("in-memory resolve cache must never fail to construct")
    })
}

/// Clear the shared resolve cache and schedule its re-warm (bundled seed +
/// durable rows) as a background task. Returns as soon as the fresh, empty
/// cache is swapped in — the caller (the `target.cache.clear` IPC command)
/// no longer waits for the re-warm to finish (issue #695).
///
/// # Errors
///
/// Returns [`ContractError`] (`internal.database`) if the old file cannot be
/// removed or the fresh cache cannot be opened. A failure in the background
/// re-warm itself is only logged (`tracing::warn!`) — the cache is already
/// swapped in and usable (just emptier until the warm catches up), which
/// matches how startup already degrades on a warm failure.
pub async fn clear_and_rewarm(state: &AppState) -> Result<(), ContractError> {
    let mut guard = state.resolve_cache.write().await;

    // Drop every handle this process holds on the old file BEFORE touching
    // it on disk (Windows locks a memory-mapped file exclusively).
    *guard = targeting_resolver::simbad::ResolveCache::in_memory()
        .map_err(|e| ContractError::internal(e.to_string()))?;

    if let Err(e) = std::fs::remove_file(&state.resolve_cache_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(ContractError::internal(format!(
                "failed to remove the resolve cache file: {e}"
            )));
        }
    }

    let fresh = targeting_resolver::simbad::ResolveCache::open(&state.resolve_cache_path)
        .map_err(|e| ContractError::internal(e.to_string()))?;

    // Clone the fresh cache's own handle (cheap — an `Arc` over the redb
    // `Database`, see `ResolveCache::cache`) for the background warm BEFORE
    // moving `fresh` into the guard, then release the write lock immediately
    // — the warm never needs it, exactly like the startup warm in `lib.rs`
    // never takes it at all. A `target.cache.clear` that lands mid-warm
    // takes this same write lock (uncontended, since it's already released),
    // deletes the file this task is still writing into, and reopens a new
    // one; the stale task keeps writing into the now-unlinked file (wasted,
    // harmless) or — on Windows — the delete surfaces its own sharing-
    // violation error rather than corrupting anything.
    let warm_handle = fresh.clone();
    let warm_cache = fresh.cache();
    let warm_pool = state.repo.pool().clone();
    *guard = fresh;
    drop(guard);

    // #818: flip on before spawning so a `target.search` racing this warm
    // sees `cache_warming = true` and knows to retry rather than treat an
    // empty result as settled. `CacheWarmingGuard` (not a bare sequential
    // store) so a panic mid-warm still clears it — see its doc comment.
    let warm_guard =
        crate::commands::lifecycle::CacheWarmingGuard::start(state.cache_warming.clone());

    tokio::spawn(async move {
        let _warm_guard = warm_guard;
        let namespace = simbad_resolver::identity::namespace(NAMESPACE_SEED);
        match targeting_resolver::seed::warm_bundled_on_first_run(&warm_cache, &namespace).await {
            Ok(Some(count)) => {
                tracing::info!("re-warmed {count} bundled target seed entries after cache clear");
            }
            Ok(None) => tracing::debug!(
                "resolve cache clear: bundled seed already warmed (unexpected on a freshly cleared cache)"
            ),
            Err(e) => {
                tracing::warn!("failed to re-warm bundled target seed after cache clear: {e}");
            }
        }
        match targeting_resolver::seed::warm_from_canonical_target(
            &warm_cache,
            &warm_pool,
            &namespace,
        )
        .await
        {
            Ok(count) if count > 0 => {
                tracing::info!("re-warmed {count} durable canonical_target rows after cache clear");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(
                "failed to re-warm resolve cache from canonical_target after cache clear: {e}"
            ),
        }
        // #818 follow-up: persists every `Eventual` chunk from both phases
        // above in one fsync (redb commits are cumulative) — see the
        // matching comment in `lib.rs`'s startup warm.
        if let Err(e) = warm_handle.flush().await {
            tracing::warn!("failed to flush resolve cache after cache-clear re-warm: {e}");
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use audit::bus::EventBus;
    use persistence_core::Database;
    use persistence_lifecycle::repositories::lifecycle::SqliteLifecycleRepository;
    use simbad_resolver::Cache as _;

    use super::*;

    async fn test_state() -> (AppState, tempfile::TempDir) {
        let db = Database::in_memory().await.expect("in-memory database");
        db.migrate().await.expect("run migrations");
        let pool = db.pool().clone();
        let bus = EventBus::with_pool(pool.clone());
        let repo = Arc::new(SqliteLifecycleRepository::new(pool, bus.clone()));

        let dir = tempfile::tempdir().expect("tempdir");
        let cache_path = dir.path().join("resolve-cache.redb");
        let cache = open_or_in_memory(&cache_path);
        let cache_warming = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let app_caches = app_core::AppCaches::shared();
        (AppState::new(repo, bus, app_caches, cache, cache_path, cache_warming), dir)
    }

    /// Regression test for #695: `clear_and_rewarm` must return as soon as
    /// the swap is done, without awaiting the seed/durable re-warm inline.
    /// A generous deadline (the real bundled warm is the ~12-minute-on-a-
    /// debug-build bug; a correct implementation returns in well under a
    /// second even against the full bundled seed).
    #[tokio::test]
    async fn clear_and_rewarm_returns_before_the_warm_completes() {
        let (state, _dir) = test_state().await;

        let outcome = tokio::time::timeout(Duration::from_secs(2), clear_and_rewarm(&state)).await;

        assert!(outcome.is_ok(), "clear_and_rewarm did not return promptly");
        outcome.unwrap().expect("clear_and_rewarm failed");
    }

    /// The background task scheduled by `clear_and_rewarm` must actually
    /// warm the fresh cache (bundled seed) — "returns fast" must not mean
    /// "never warms".
    #[tokio::test]
    async fn clear_and_rewarm_warms_the_fresh_cache_in_the_background() {
        let (state, _dir) = test_state().await;

        clear_and_rewarm(&state).await.expect("clear_and_rewarm failed");

        let cache = state.resolve_cache.read().await.clone();
        let warmed = poll_until_non_empty(&cache).await;
        assert!(warmed, "background re-warm never populated the fresh cache");
    }

    /// Bounded-poll deadline for [`poll_until`]/[`poll_until_non_empty`]
    /// against a REAL, file-backed warm (`test_state` opens a real temp-dir
    /// redb file, not an in-memory store) — the full bundled seed, chunked
    /// into ~14 `Cache::upsert_batch` write transactions (#818 follow-up),
    /// each a real fsync. A tighter (10s) deadline here was previously
    /// observed to fail ONLY on Windows CI (macOS/Linux always passed):
    /// Windows fsync is well documented as markedly slower than Linux/macOS,
    /// especially on a virtualized CI runner, so a real (not stuck) warm can
    /// legitimately still be running past 10s there. This is generous enough
    /// to absorb that without masking an actual stuck task (a genuinely
    /// hung/panicked warm still fails the assertion, just after a longer
    /// wait) — reasoned through the `CacheWarmingGuard` Drop path (an
    /// unconditional store, no early-return/`?` to skip — see its doc
    /// comment) and tokio's task scheduling (both platform-independent), so
    /// a real leak would misbehave identically on every OS, not Windows-only.
    const WARM_SETTLE_DEADLINE: Duration = Duration::from_mins(1);

    /// Regression test for #818: `cache_warming` must be observably `true`
    /// while the background re-warm is running and `false` once it settles,
    /// so `target.search` can tell a still-warming empty result apart from a
    /// genuine miss (batching the warm into one write transaction per phase
    /// removed the old per-entry incremental visibility that used to mask a
    /// query racing this window).
    #[tokio::test]
    async fn cache_warming_flag_is_true_during_the_warm_and_false_after() {
        use std::sync::atomic::Ordering;

        let (state, _dir) = test_state().await;
        assert!(
            !state.cache_warming.load(Ordering::Relaxed),
            "flag must start false — no warm has been scheduled yet"
        );

        clear_and_rewarm(&state).await.expect("clear_and_rewarm failed");
        // `clear_and_rewarm` sets the flag before spawning and returns before
        // the swap task even runs (regression-tested above), so it must
        // already read true right after the call returns.
        assert!(
            state.cache_warming.load(Ordering::Relaxed),
            "flag must be true immediately after clear_and_rewarm schedules the warm"
        );

        let settled = poll_until(|| !state.cache_warming.load(Ordering::Relaxed)).await;
        assert!(settled, "flag never flipped back to false once the warm finished");
    }

    /// Polls (bounded) rather than sleeping a fixed duration — avoids both
    /// flakiness on a slow CI runner and a needlessly slow test on a fast one.
    async fn poll_until(mut done: impl FnMut() -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + WARM_SETTLE_DEADLINE;
        loop {
            if done() {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Polls (bounded) rather than sleeping a fixed duration — avoids both
    /// flakiness on a slow CI runner and a needlessly slow test on a fast one.
    async fn poll_until_non_empty(cache: &targeting_resolver::simbad::ResolveCache) -> bool {
        let deadline = tokio::time::Instant::now() + WARM_SETTLE_DEADLINE;
        loop {
            if !cache.cache().list().await.expect("cache list failed").is_empty() {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
