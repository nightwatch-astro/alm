// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Cached FITS/XISF header extraction (in-memory caching layer, F0/W-FITS).
//!
//! Lives in `app_core_targets` (not `app_core`): the current
//! `FitsExtractor`/`XisfExtractor` call sites are `app_core_inbox::scan` /
//! `classify` / `confirm` and `app_core_targets::ingest_sessions` —
//! `app_core_inbox` already depends on `app_core_targets`, but `app_core`
//! depends on `app_core_inbox` (not the reverse), so a cache placed in
//! `app_core` would be unreachable from those callers without a cyclic
//! dependency. This module sits at the one crate genuinely upstream of both.
//!
//! Wraps [`metadata_fits::FitsExtractor`] / [`metadata_xisf::XisfExtractor`]
//! behind a module-local [`app_core_cache::TtlCache`] keyed by
//! `(path, mtime, size)`. The key is self-invalidating: any change to the
//! file's mtime or size (edit, replace, re-download) produces a new key, so a
//! changed file is a cache miss and a stale-metadata bug cannot occur without
//! also requiring an unmodified file to report unmodified stats.
//!
//! Single-flight: concurrent callers requesting the same key coalesce onto one
//! extraction (`TtlCache::get_or_insert_with` delegates to `moka`'s
//! `get_with`), so a burst of reads for the same file during a scan does not
//! re-parse the header once per caller.
//!
//! Callers needing a fresh read (e.g. after a known external file mutation)
//! should re-`std::fs::metadata` the path — since mtime/size are part of the
//! key, a genuinely changed file already misses; there is no separate
//! `invalidate_*` entry point because there is nothing to explicitly track.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use app_core_cache::{CacheConfig, TtlCache};
use metadata_core::{MetadataExtractError, MetadataExtractor, RawFileMetadata};
use metadata_fits::FitsExtractor;
use metadata_xisf::XisfExtractor;

/// `(path, mtime_unix_secs, size_bytes)` — a file's identity for caching
/// purposes. Second-resolution mtime is coarser than some filesystems'
/// native precision, but is combined with `size` so a same-second rewrite
/// that also changes length is still detected; a same-second rewrite that
/// preserves both mtime-second and byte length is the same accepted risk any
/// mtime-based cache carries.
type CacheKey = (PathBuf, u64, u64);
/// Memoized alongside `Ok` results: an extraction failure for a given
/// `(path, mtime, size)` is deterministic (same bytes in, same parse
/// failure out), so caching `Err` under the TTI is safe and avoids
/// re-parsing an unreadable/malformed file on every lookup.
// Arc wraps Ok so cache hits return a cheap pointer bump instead of
// deep-cloning the struct on every moka `get_with`.
type CacheValue = Result<Arc<RawFileMetadata>, MetadataExtractError>;

static METADATA_CACHE: std::sync::OnceLock<TtlCache<CacheKey, CacheValue>> =
    std::sync::OnceLock::new();

fn cache() -> &'static TtlCache<CacheKey, CacheValue> {
    METADATA_CACHE.get_or_init(|| {
        TtlCache::new(CacheConfig::new(50_000).with_time_to_idle(Duration::from_mins(30)))
    })
}

/// Extract FITS/XISF header metadata for `path`, memoized by `(path, mtime,
/// size)`.
///
/// Dispatches by file extension to [`XisfExtractor`] / [`FitsExtractor`],
/// mirroring the extension-check-then-extract pattern used in
/// `app_core_inbox::scan`/`classify`/`confirm` and
/// `app_core_targets::ingest_sessions`.
///
/// # Errors
///
/// Returns [`MetadataExtractError::Io`] if the file's metadata (mtime/size)
/// or contents cannot be read, or the extension is not `.fits`/`.fit`/`.fts`/
/// `.xisf`. Returns [`MetadataExtractError::Parse`] if the matched extractor
/// cannot parse the header.
pub fn cached_extract(path: &Path) -> Result<Arc<RawFileMetadata>, MetadataExtractError> {
    let key = cache_key(path)?;
    cache().get_or_insert_with(key, || extract_uncached(path))
}

fn cache_key(path: &Path) -> Result<CacheKey, MetadataExtractError> {
    let meta = std::fs::metadata(path).map_err(|e| MetadataExtractError::Io {
        path: path.display().to_string(),
        msg: e.to_string(),
    })?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    Ok((path.to_path_buf(), mtime, meta.len()))
}

fn extract_uncached(path: &Path) -> Result<Arc<RawFileMetadata>, MetadataExtractError> {
    let ext =
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();

    let extracted = if XisfExtractor.supports_extension(&ext) {
        XisfExtractor.extract(path)?
    } else if FitsExtractor.supports_extension(&ext) {
        FitsExtractor.extract(path)?
    } else {
        return Err(MetadataExtractError::Io {
            path: path.display().to_string(),
            msg: format!("unsupported metadata file extension: {ext:?}"),
        });
    };

    extracted.map(Arc::new).ok_or_else(|| MetadataExtractError::Parse {
        path: path.display().to_string(),
        msg: "extractor matched extension but produced no metadata".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::cached_extract;

    /// Minimal valid FITS header: one 2880-byte block containing `END` and a
    /// couple of keyword cards, enough for `FitsExtractor` to parse.
    fn write_minimal_fits(path: &std::path::Path) {
        let mut block = vec![b' '; 2880];
        let cards: &[&[u8]] = &[
            b"SIMPLE  =                    T",
            b"IMAGETYP= 'Light   '",
            b"OBJECT  = 'M31     '",
            b"END",
        ];
        for (i, card) in cards.iter().enumerate() {
            let start = i * 80;
            block[start..start + card.len()].copy_from_slice(card);
        }
        let mut file = std::fs::File::create(path).expect("create fixture fits file");
        file.write_all(&block).expect("write fixture fits header");
    }

    #[test]
    fn cached_extract_hits_on_second_call_for_unchanged_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frame.fits");
        write_minimal_fits(&path);

        let first = cached_extract(&path).expect("first extract succeeds");
        let second = cached_extract(&path).expect("second extract succeeds (cache hit)");

        assert_eq!(first.object, second.object);
        assert_eq!(second.object.as_deref(), Some("M31"));
    }

    #[test]
    fn cached_extract_misses_after_file_is_rewritten_with_different_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frame.fits");
        write_minimal_fits(&path);
        let first = cached_extract(&path).expect("first extract succeeds");
        assert_eq!(first.object.as_deref(), Some("M31"));

        // Rewrite with a different OBJECT and a different file size, forcing
        // a new (path, mtime, size) cache key.
        let mut block = vec![b' '; 2880 * 2];
        let cards: &[&[u8]] = &[
            b"SIMPLE  =                    T",
            b"IMAGETYP= 'Light   '",
            b"OBJECT  = 'NGC7000 '",
            b"END",
        ];
        for (i, card) in cards.iter().enumerate() {
            let start = i * 80;
            block[start..start + card.len()].copy_from_slice(card);
        }
        std::fs::write(&path, &block).expect("rewrite fixture fits file");

        let second = cached_extract(&path).expect("second extract succeeds");
        assert_eq!(
            second.object.as_deref(),
            Some("NGC7000"),
            "changed file must not hit stale cache"
        );
    }

    #[test]
    fn cached_extract_rejects_unsupported_extension() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"not a fits file").expect("write fixture txt file");

        let err = cached_extract(&path).expect_err("unsupported extension is an error");
        assert!(matches!(err, metadata_core::MetadataExtractError::Io { .. }));
    }
}
