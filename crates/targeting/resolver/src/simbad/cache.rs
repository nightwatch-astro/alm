// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! The persistent, shared resolve-cache handle (spec 052 P1 D2).

use crate::ResolveError;

use super::convert::from_cache_error;

/// A shared handle to the persistent SIMBAD resolve cache (spec 052 P1 D2: one
/// global redb file, no TTL, warmed from the bundled seed + existing
/// `canonical_target` rows — see [`crate::seed`]).
///
/// Cloning is cheap (an `Arc` over one `redb::Database`, mirroring
/// [`simbad_resolver::Store`]); open it once at app startup and clone it into
/// every [`super::SimbadResolver`] built afterward.
#[derive(Clone)]
pub struct ResolveCache(simbad_resolver::Store);

impl ResolveCache {
    /// Open (creating if missing) the durable, file-backed resolve cache at
    /// `path`, with the store's bulk-batch writes configured `Eventual`
    /// (`simbad-resolver` 0.3.2's [`simbad_resolver::BatchDurability`] —
    /// skips the fsync per [`simbad_resolver::Cache::upsert_batch`]
    /// transaction; [`Self::flush`] does one fsync at the end persisting
    /// every chunk, since redb commits are cumulative). Single-item
    /// [`simbad_resolver::Cache::upsert`] calls (e.g. an in-flight resolve
    /// while a chunked seed warm is running) stay durable regardless — this
    /// setting only relaxes the *bulk seed/backfill warm* path
    /// (`crate::seed`), matching the app's own chunk size
    /// (`crate::seed::WARM_CHUNK_SIZE`'s ~13-chunk bundled seed warm going
    /// from ~13 fsyncs to 1).
    ///
    /// A file that is shorter than the layout recorded in its own redb header —
    /// the shape an unclean shutdown or a torn write leaves behind — aborts the
    /// open by panic rather than by `Err`, because redb 4.1.0 checks it with a
    /// release-live `assert!` (`page_store/page_manager.rs:231`). This deletes
    /// such a file and opens once more; the cache is a reproducible projection
    /// of the bundled seed and the `canonical_target` rows, so nothing
    /// user-authored is lost.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::Network`] if the redb file cannot be opened, its
    /// tables cannot be initialised, or a file that aborted the open cannot be
    /// deleted.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, ResolveError> {
        let path = path.as_ref();
        if let Some(result) = Self::open_store(path) {
            return result;
        }

        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(ResolveError::Network(format!(
                    "the resolve cache file at {} aborted the open and could not be removed: {e}",
                    path.display()
                )));
            }
        }

        Self::open_store(path).unwrap_or_else(|| {
            Err(ResolveError::Network(format!(
                "the resolve cache store aborted the open of a freshly created file at {}",
                path.display()
            )))
        })
    }

    /// Open the backing store, returning `None` when the open aborted by panic
    /// instead of returning. Containing that panic depends on panics unwinding:
    /// the release profile in the workspace `Cargo.toml` deliberately keeps the
    /// default `panic = "unwind"`.
    fn open_store(path: &std::path::Path) -> Option<Result<Self, ResolveError>> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            simbad_resolver::Store::open_with(path, simbad_resolver::BatchDurability::Eventual)
                .map(Self)
                .map_err(|e| from_cache_error(&e))
        }))
        .ok()
    }

    /// An ephemeral, in-memory resolve cache (nothing persisted) — for tests
    /// and offline-only construction. Always `Durable` (the crate has no
    /// `Eventual` in-memory variant — there is no "reopen after a crash"
    /// scenario for a store with nothing on disk to begin with).
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::Network`] if the in-memory store cannot be
    /// created.
    pub fn in_memory() -> Result<Self, ResolveError> {
        simbad_resolver::Store::in_memory().map(Self).map_err(|e| from_cache_error(&e))
    }

    /// Borrow the crate's own [`simbad_resolver::Cache`] trait object (e.g. for
    /// [`crate::seed`] warming or a "clear resolve cache" action).
    #[must_use]
    pub fn cache(&self) -> impl simbad_resolver::Cache + 'static {
        self.0.cache()
    }

    /// Force one fully durable commit, persisting every `Eventual` bulk-warm
    /// chunk written since [`Self::open`] (redb commits are cumulative — see
    /// [`simbad_resolver::RedbCache::flush`]). Call once after the LAST
    /// warm/backfill phase of a startup or `target.cache.clear` re-warm —
    /// don't rely on the cache closing naturally to persist those chunks, as
    /// it stays open for the rest of the process's lifetime. A cheap,
    /// safe-to-call no-op if nothing `Eventual` was written this session
    /// (e.g. every phase short-circuited on its own gate).
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::Network`] if the (empty) commit fails.
    pub async fn flush(&self) -> Result<(), ResolveError> {
        // `Store::cache()` returns the CONCRETE `RedbCache` (unlike
        // `Self::cache()` above, which erases it to `impl Cache` — `flush`
        // is redb-specific, not part of the portable `Cache` trait).
        self.0.cache().flush().await.map_err(|e| from_cache_error(&e))
    }
}

#[cfg(test)]
mod tests {
    use super::ResolveCache;

    /// Regression for astro-plan-3v3r.10.42 / 10.29. The fixture must be a
    /// VALID redb store truncated below its own recorded layout: random bytes
    /// fail redb's header parse and return `Err` long before the release-live
    /// `assert!` at `page_store/page_manager.rs:231` that this guard contains.
    #[test]
    fn a_truncated_cache_file_is_recreated_instead_of_aborting_the_open() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("simbad-cache.redb");

        drop(ResolveCache::open(&path).expect("a fresh cache file must open"));
        let full_len = std::fs::metadata(&path).expect("stat the store").len();
        assert!(
            full_len > 4096,
            "fixture must be longer than the truncation point, got {full_len}"
        );
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("reopen for truncation")
            .set_len(4096)
            .expect("truncate the store below its recorded layout");

        ResolveCache::open(&path).expect("a truncated cache file must reopen, not abort");
        assert!(path.exists(), "the reopened cache must leave a usable file behind");
    }
}
