// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Recursive inbox folder scan (spec 005, T-RecursiveScanImpl).
//! Each leaf directory containing at least one FITS or XISF file becomes one
//! `ScannedInboxItem`. Intermediate folders containing only sub-folders are
//! not items. Video-only folders produce items with `lane = "video"`.
//!
//! Spec 040 Phase 2a: detected calibration masters in a leaf folder are
//! extracted from the folder group and represented as individual
//! `ScannedMasterFile` entries within the same `ScannedInboxItem`. The
//! persist layer (inbox_scan_folder command) then creates individual
//! `inbox_items` rows for each master and a single grouped row for the
//! remaining sub-frames.
//!
//! Constitution §I: symlinks/junctions are NOT followed unless explicitly
//! enabled (default: false). Hashing is lazy — only the 64 KB partial read
//! used for content signatures.
#![allow(clippy::doc_markdown)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use app_core_targets::metadata_cache::cached_extract;
use calibration_master_detect::{detect_master, DetectInput, MasterDetection};
use camino::Utf8Path;
use metadata_video::is_video_extension;

use super::signature::compute_content_signature;

// ── FileFormat ────────────────────────────────────────────────────────────────

/// The actual file format detected during scan.
///
/// Distinct from `Lane` — a FITS lane may contain either FITS or XISF files,
/// or a mix. This enum carries the real format so the UI can display it
/// accurately instead of always showing "FITS".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileFormat {
    /// `.fits` / `.fit` / `.fts`
    Fits,
    /// `.xisf`
    Xisf,
    /// Video files (`.ser`, `.avi`, etc.)
    Video,
    /// Folder contains both FITS and XISF files.
    Mixed,
}

impl FileFormat {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fits => "fits",
            Self::Xisf => "xisf",
            Self::Video => "video",
            Self::Mixed => "mixed",
        }
    }
}

/// Derive the folder-level format from the file lists.
fn folder_format(
    fits_files: &[PathBuf],
    xisf_files: &[PathBuf],
    video_files: &[PathBuf],
) -> FileFormat {
    let has_fits = !fits_files.is_empty();
    let has_xisf = !xisf_files.is_empty();
    let has_video = !video_files.is_empty();

    match (has_fits || has_xisf, has_video) {
        (true, _) if has_fits && has_xisf => FileFormat::Mixed,
        (true, _) if has_xisf => FileFormat::Xisf,
        (false, true) => FileFormat::Video,
        // (true, _) FITS-only, plus the (false, false) fallback.
        _ => FileFormat::Fits,
    }
}

/// Derive per-file format from extension.
fn file_format_from_ext(ext: &str) -> FileFormat {
    match ext {
        "xisf" => FileFormat::Xisf,
        "fits" | "fit" | "fts" => FileFormat::Fits,
        _ => FileFormat::Video,
    }
}

// ── ScannedMasterFile ─────────────────────────────────────────────────────────

/// A single calibration master file detected during scan within a leaf folder.
///
/// Each master becomes its own `inbox_items` row (spec 040 FR-005, FR-006).
#[derive(Clone, Debug)]
pub struct ScannedMasterFile {
    /// Absolute path to the master file.
    pub abs_path: PathBuf,
    /// Relative path from the scan root (= the key for the inbox_items row).
    pub relative_path: String,
    /// File format (Fits or Xisf).
    pub format: FileFormat,
    /// Master detection result (frame type, detector provenance).
    pub detection: MasterDetection,
    /// Filter extracted from metadata, if available.
    pub filter: Option<String>,
    /// Exposure in seconds extracted from metadata, if available.
    pub exposure_s: Option<f64>,
}

// ── ScannedInboxItem ──────────────────────────────────────────────────────────

/// A leaf folder discovered during an inbox scan.
#[derive(Clone, Debug)]
pub struct ScannedInboxItem {
    /// Absolute path to the leaf folder.
    pub folder_path: PathBuf,
    /// Relative path from the scan root.
    pub relative_path: String,
    /// FITS (.fits/.fit/.fts) files inside this folder (direct children only).
    ///
    /// Does NOT include XISF files (those are in `xisf_files`).
    pub fits_files: Vec<PathBuf>,
    /// XISF files inside this folder (direct children only).
    pub xisf_files: Vec<PathBuf>,
    /// Video files in this folder.
    pub video_files: Vec<PathBuf>,
    /// Content signature of the folder (computed from FITS/XISF files only).
    pub content_signature: String,
    /// Classification lane.
    pub lane: Lane,
    /// Folder-level format (Fits | Xisf | Mixed | Video).
    pub format: FileFormat,
    /// Calibration masters detected within this folder.
    ///
    /// Each entry becomes its own `inbox_items` row. The remaining non-master
    /// FITS/XISF files are grouped into the folder-level row.
    pub masters: Vec<ScannedMasterFile>,
}

impl ScannedInboxItem {
    /// Files in this folder that classification still has to split, i.e. every
    /// file except the detected calibration masters, which become their own
    /// `inbox_items` rows.
    ///
    /// Carries the spec 058 FR-015 master carve-out: a masters-only folder must
    /// score 0 so `list_unclassified_source_groups` does not surface it as a
    /// scanned-but-unclassified row *in addition to* its master rows. The
    /// subtraction is sound only because `masters` is built by filtering
    /// `fits_files ∪ xisf_files` (see `scan_dir`); it is saturating so a
    /// future violation of that subset relation degrades to 0 rather than
    /// panicking.
    #[must_use]
    pub fn sub_frame_count(&self) -> usize {
        (self.fits_files.len() + self.xisf_files.len()).saturating_sub(self.masters.len())
            + self.video_files.len()
    }
}

/// Whether this item should be classified as FITS or routed to the video lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    Fits,
    Video,
}

impl Lane {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fits => "fits",
            Self::Video => "video",
        }
    }
}

// ── ScanOptions ───────────────────────────────────────────────────────────────

/// Options controlling the scan behaviour.
#[derive(Clone, Debug, Default)]
pub struct ScanOptions {
    /// Follow symlinks/junctions. Default `false` per constitution §I.
    pub follow_symlinks: bool,
    /// Number of worker threads for per-file I/O during a scan.
    ///
    /// `None` (the default) uses the sequential path — one thread, no spawn
    /// overhead. Measured to be at SSD parity vs the multi-threaded variants
    /// (see `crates/tools/perf-bench`).
    ///
    /// `Some(N)` where N > 1 enables the parallel path capped at
    /// `MAX_SCAN_WORKERS`. Intended for rotational HDD libraries where
    /// per-file seek latency amortises across threads.
    pub workers: Option<usize>,
}

// ── FITS / XISF extensions ────────────────────────────────────────────────────

const FITS_ONLY_EXTENSIONS: &[&str] = &["fits", "fit", "fts"];
const XISF_EXTENSIONS: &[&str] = &["xisf"];

fn is_fits_extension(ext: &str) -> bool {
    FITS_ONLY_EXTENSIONS.contains(&ext)
}

fn is_xisf_extension(ext: &str) -> bool {
    XISF_EXTENSIONS.contains(&ext)
}

// ── Master detection at scan time ─────────────────────────────────────────────

/// Attempt to extract metadata and detect whether `path` is a calibration
/// master.  Called only for FITS/XISF files in leaf folders.
///
/// Returns `Some(ScannedMasterFile)` when the file is identified as a master.
/// Returns `None` when not a master or metadata is unreadable.
fn try_detect_master(abs_path: &Path, rel_path: &str, ext: &str) -> Option<ScannedMasterFile> {
    // Cached extract (F0): memoized by (path, mtime, size). Unsupported
    // extensions and headers in which no keyword is readable surface as `Err`
    // here, so such a file is declined outright rather than path-inferred. A
    // readable header that merely lacks `IMAGETYP` still reaches
    // `detect_master` below and may be classified from its path.
    let bundle = cached_extract(abs_path).ok()?;

    let image_typ_raw = bundle.image_typ.as_deref();
    let stack_count = bundle.stack_count;
    let file_name = abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let detect_input = DetectInput { imagetyp: image_typ_raw, stack_count, file_name, rel_path };

    let detection = detect_master(&detect_input)?;

    if !detection.is_master {
        return None;
    }

    let format = file_format_from_ext(ext);
    let filter = bundle.filter.clone();
    let exposure_s = bundle.exposure.as_deref().and_then(|v| v.parse::<f64>().ok());

    Some(ScannedMasterFile {
        abs_path: abs_path.to_owned(),
        relative_path: rel_path.to_owned(),
        format,
        detection,
        filter,
        exposure_s,
    })
}

/// Compute the root-relative path as a forward-slash UTF-8 string for the wire.
///
/// Delegates to `fs_pathsafe::relative_wire_path` (canonical single home
/// for the backslash→slash normalization, bd astro-plan-kyo7.88).
fn relative_utf8(root: &Path, path: &Path) -> String {
    fs_pathsafe::relative_wire_path(root, path)
}

// ── scan_root ────────────────────────────────────────────────────────────────

/// Maximum worker threads for per-leaf I/O during a scan.
///
/// Scan is I/O-bound (64 KB reads + FITS header parse). Staying at or below
/// 8 avoids thrashing a rotational HDD while still saturating an SSD.
const MAX_SCAN_WORKERS: usize = 8;

/// A non-fatal directory-access problem collected during phase 1.
///
/// These are returned in [`ScanOutput::warnings`] so callers can surface them
/// without aborting the scan. The root directory itself is still fatal — only
/// subdirs are demoted to warnings.
#[derive(Clone, Debug)]
pub struct ScanWarning {
    /// Absolute path of the directory that could not be read.
    pub path: PathBuf,
    /// OS error description (e.g. "Permission denied (os error 13)").
    pub reason: String,
}

/// Output of a successful [`scan_root`] call.
///
/// `items` contains every [`ScannedInboxItem`] found in reachable leaf
/// directories. `warnings` contains one entry for each sub-directory that
/// could not be read (EACCES, ENOENT, etc.) but did not abort the scan.
#[derive(Debug, Default)]
pub struct ScanOutput {
    pub items: Vec<ScannedInboxItem>,
    /// Per-directory access errors collected during the phase-1 walk.
    pub warnings: Vec<ScanWarning>,
}

/// Leaf-folder inventory collected by the pure directory walk (phase 1).
///
/// Holds only paths — no per-file I/O yet. The expensive work (signature
/// reads, master-detection header parses) is deferred to the parallel phase.
struct LeafDir {
    dir_path: PathBuf,
    fits_files: Vec<PathBuf>,
    xisf_files: Vec<PathBuf>,
    video_files: Vec<PathBuf>,
}

/// Recursively scan `root` and return one `ScannedInboxItem` per leaf folder
/// that directly contains FITS/XISF or video files.
///
/// For FITS-lane folders, master detection is run per-file so that detected
/// masters can be split into individual `inbox_items` rows by the caller.
///
/// Intermediate folders are not items. Symlinks are not followed unless
/// `options.follow_symlinks = true`.
///
/// Two-phase design: phase 1 is a fast sequential `readdir` walk that builds
/// a list of leaf directories (no per-file I/O). Phase 2 processes each leaf
/// — 64 KB signature reads + FITS header parses for master detection — using
/// either the sequential path (`options.workers == None` or `Some(1)`) or a
/// bounded parallel pool (`options.workers == Some(N > 1)`).
///
/// The default is sequential. On SSD the read channel saturates at one
/// reader and extra threads contend rather than help. Set `workers` to a
/// value > 1 for rotational HDD libraries where parallelism amortises seek
/// latency.
///
/// Results are sorted by `relative_path` for deterministic output regardless
/// of OS `readdir` order.
///
/// # Errors
///
/// Returns an error string if `root` is not a directory the walker will
/// descend under `options.follow_symlinks` — including a symlinked or junction
/// root while following is disabled — or cannot be read.
/// Unreadable *sub*-directories are non-fatal: they are collected in
/// [`ScanOutput::warnings`] and the scan continues with whatever it can reach.
///
/// # Panics
///
/// Panics only if a scan worker thread panics due to an internal bug
/// (e.g. a logic error in `process_leaf`). Per-file I/O failures
/// (unreadable files, parse errors) are silently skipped and do not panic.
pub fn scan_root(root: &Path, options: &ScanOptions) -> Result<ScanOutput, String> {
    fs_pathsafe::probe_root(root, options.follow_symlinks)
        .map_err(|reason| format!("scan root is unusable: {reason} ({})", root.display()))?;

    // Phase 1: collect leaf dirs via fast directory walk (no per-file I/O).
    let mut leaf_dirs: Vec<LeafDir> = Vec::new();
    let mut warnings: Vec<ScanWarning> = Vec::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    collect_leaf_dirs(root, options, &mut leaf_dirs, &mut warnings, &mut visited)?;

    if leaf_dirs.is_empty() {
        return Ok(ScanOutput { items: vec![], warnings });
    }

    // Phase 2: process per-file I/O.
    //
    // Default is sequential (worker_count == 1): no thread spawn overhead,
    // SSD-friendly (a single reader saturates the read channel; extra threads
    // contend rather than help), and correct for all measured configurations.
    //
    // Set options.workers to Some(N > 1) to enable the parallel path. Useful
    // on rotational HDDs where per-file seek latency (~10 ms) amortises well
    // across multiple threads. The parallel path caps at MAX_SCAN_WORKERS.
    let worker_count = options.workers.unwrap_or(1).clamp(1, MAX_SCAN_WORKERS);

    let mut items: Vec<ScannedInboxItem> = Vec::with_capacity(leaf_dirs.len());

    if worker_count == 1 {
        // Sequential path: no thread spawn, no join overhead.
        items.extend(leaf_dirs.iter().map(|leaf| process_leaf(root, leaf)));
    } else {
        // Parallel path: ceiling-divide leaf dirs across workers.
        let chunk_size = leaf_dirs.len().div_ceil(worker_count).max(1);

        std::thread::scope(|s| {
            let handles: Vec<_> = leaf_dirs
                .chunks(chunk_size)
                .map(|chunk| {
                    s.spawn(|| {
                        chunk.iter().map(|leaf| process_leaf(root, leaf)).collect::<Vec<_>>()
                    })
                })
                .collect();

            for handle in handles {
                // Worker panics only on internal bugs; per-file I/O errors are
                // handled inside process_leaf (bad file → skipped, not fatal).
                items.extend(handle.join().expect("scan worker panicked"));
            }
        });
    }

    // Sort by relative_path for deterministic output.
    items.sort_unstable_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(ScanOutput { items, warnings })
}

/// Compute the [`ScannedInboxItem`] for one leaf directory.
///
/// This is the I/O-heavy step: reads up to 64 KB per file for the content
/// signature and parses FITS/XISF headers for master detection.
///
/// Per-file failures (unreadable file, parse error) are silently skipped —
/// callers rely on partial results rather than an all-or-nothing scan.
fn process_leaf(root: &Path, leaf: &LeafDir) -> ScannedInboxItem {
    let relative_path = relative_utf8(root, &leaf.dir_path);
    let all_image_files: Vec<&PathBuf> =
        leaf.fits_files.iter().chain(leaf.xisf_files.iter()).collect();

    let (lane, content_signature) = if all_image_files.is_empty() {
        let sig_refs: Vec<&Path> = leaf.video_files.iter().map(PathBuf::as_path).collect();
        (Lane::Video, compute_content_signature(&sig_refs))
    } else {
        let sig_refs: Vec<&Path> = all_image_files.iter().map(|p| p.as_path()).collect();
        (Lane::Fits, compute_content_signature(&sig_refs))
    };

    let format = folder_format(&leaf.fits_files, &leaf.xisf_files, &leaf.video_files);

    // Master detection: FITS-lane folders only.
    let masters: Vec<ScannedMasterFile> = if lane == Lane::Fits {
        all_image_files
            .iter()
            .filter_map(|abs_path| {
                let ext = abs_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let rel = relative_utf8(root, abs_path);
                try_detect_master(abs_path, &rel, &ext)
            })
            .collect()
    } else {
        vec![]
    };

    ScannedInboxItem {
        folder_path: leaf.dir_path.clone(),
        relative_path,
        fits_files: leaf.fits_files.clone(),
        xisf_files: leaf.xisf_files.clone(),
        video_files: leaf.video_files.clone(),
        content_signature,
        lane,
        format,
        masters,
    }
}

/// Phase 1: recurse into `dir`, classify entries as subdirs or image/video
/// files, and push a [`LeafDir`] for any folder that directly contains
/// FITS/XISF or video files.
///
/// No per-file I/O beyond `readdir` + `file_type`; content and header reads
/// are deferred to [`process_leaf`].
///
/// When called on the scan root (top-level), a read failure propagates as
/// `Err` — there is nothing to return. When called recursively on a subdir,
/// the same failure is pushed into `warnings` and the walk continues.
///
/// `visited` holds the canonical path of every directory already entered and is
/// consulted only when `options.follow_symlinks` is set. Without it a link back
/// to an ancestor re-enters the same real directory once per link component the
/// OS allows, duplicating every frame inside it; the same key shape as
/// `fs_pathsafe::real_files_under_reporting`.
fn collect_leaf_dirs(
    dir: &Path,
    options: &ScanOptions,
    leaf_dirs: &mut Vec<LeafDir>,
    warnings: &mut Vec<ScanWarning>,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    if options.follow_symlinks
        && !visited.insert(std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()))
    {
        return Ok(());
    }

    let read_dir = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read directory {}: {e}", dir.display()))?;

    let mut fits_files: Vec<PathBuf> = Vec::new();
    let mut xisf_files: Vec<PathBuf> = Vec::new();
    let mut video_files: Vec<PathBuf> = Vec::new();
    let mut subdirs: Vec<PathBuf> = Vec::new();

    for entry in read_dir.flatten() {
        let path = entry.path();

        // OS scan boundary: `read_dir` yields `std::path::PathBuf`, which can be
        // non-UTF-8 on a raw disk. We do not lossy-convert (that would corrupt the
        // path that later crosses the IPC boundary as a wire string). A non-UTF-8
        // entry is skipped explicitly with a diagnostic so the scan never panics
        // and never emits a mangled path. Constitution §I (Local-First custody).
        if Utf8Path::from_path(&path).is_none() {
            tracing::warn!(
                path = %path.to_string_lossy(),
                "inbox scan: skipping non-UTF-8 path (cannot represent as a faithful UTF-8 wire value)"
            );
            continue;
        }

        let Ok(file_type) = entry.file_type() else { continue };
        // Reparse-aware check (symlink + Windows junction) shared with
        // fs_inventory/fs_executor — see `fs_pathsafe` (duplication-and-
        // abstraction audit T1-a).
        let is_link = fs_pathsafe::is_link_or_junction(&path);

        if is_link && !options.follow_symlinks {
            // Constitution §I: skip symlinks/junctions unless explicitly enabled.
            continue;
        }

        if file_type.is_dir() || (is_link && options.follow_symlinks && path.is_dir()) {
            subdirs.push(path);
        } else if file_type.is_file() || (is_link && options.follow_symlinks) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();

            if is_fits_extension(&ext) {
                fits_files.push(path);
            } else if is_xisf_extension(&ext) {
                xisf_files.push(path);
            } else if is_video_extension(&ext) {
                video_files.push(path);
            }
        }
    }

    if !fits_files.is_empty() || !xisf_files.is_empty() || !video_files.is_empty() {
        leaf_dirs.push(LeafDir { dir_path: dir.to_owned(), fits_files, xisf_files, video_files });
    }

    // Always recurse into subdirs regardless of whether this dir has files.
    // A read failure on a subdir is non-fatal: record it in warnings and
    // continue so partial results reach the caller instead of nothing.
    for subdir in subdirs {
        if let Err(reason) = collect_leaf_dirs(&subdir, options, leaf_dirs, warnings, visited) {
            warnings.push(ScanWarning { path: subdir, reason });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use metadata_core::{v1_normalization_table, FrameType};
    use std::fs;
    use std::io::Write;

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write_file(dir: &Path, name: &str, content: &[u8]) {
        let path = dir.join(name);
        let mut f = fs::File::create(path).unwrap();
        f.write_all(content).unwrap();
    }

    fn write_realistic_fits(
        dir: &Path,
        name: &str,
        imagetyp: Option<&str>,
        stack_count: Option<(&str, u32)>,
    ) {
        let mut cards = vec![
            "SIMPLE  =                    T".to_owned(),
            "BITPIX  =                    8".to_owned(),
            "NAXIS   =                    2".to_owned(),
            "NAXIS1  =                    1".to_owned(),
            "NAXIS2  =                    1".to_owned(),
            "INSTRUME= 'ZWO ASI2600MM Pro'".to_owned(),
            "TELESCOP= 'Esprit 100ED'".to_owned(),
            "DATE-OBS= '2026-07-09T22:14:31.125'".to_owned(),
            "EXPTIME =                300.0".to_owned(),
            "GAIN    =                  100".to_owned(),
            "OFFSET  =                   50".to_owned(),
            "FILTER  = 'Ha'".to_owned(),
            "OBJECT  = 'M42'".to_owned(),
            "XBINNING=                    1".to_owned(),
            "YBINNING=                    1".to_owned(),
        ];
        if let Some(value) = imagetyp {
            cards.push(format!("IMAGETYP= '{value}'"));
        }
        if let Some((key, value)) = stack_count {
            cards.push(format!("{key:<8}= {value:>20}"));
        }

        let mut block = vec![b' '; 2880];
        for (idx, card) in cards.iter().enumerate() {
            let bytes = card.as_bytes();
            let len = bytes.len().min(80);
            block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
        }
        let end = cards.len() * 80;
        block[end..end + 3].copy_from_slice(b"END");
        block.extend_from_slice(&[0; 2880]);
        write_file(dir, name, &block);
    }

    struct PipelineCase {
        name: String,
        imagetyp: Option<&'static str>,
        stack_count: Option<(&'static str, u32)>,
        expected_type: Option<FrameType>,
        expected_master: bool,
        expected_detector: Option<&'static str>,
    }

    #[allow(clippy::too_many_lines)] // Keep each type's precedence cases adjacent for review.
    fn master_pipeline_cases() -> Vec<PipelineCase> {
        let types = [
            ("light", "LIGHT", "Master Light", FrameType::Light, "flat"),
            ("dark", "DARK", "Master Dark", FrameType::Dark, "flat"),
            ("flat", "FLAT", "Master Flat", FrameType::Flat, "dark"),
            ("bias", "BIAS", "Master Bias", FrameType::Bias, "light"),
            ("darkflat", "DARKFLAT", "Master DarkFlat", FrameType::DarkFlat, "bias"),
        ];
        let mut cases = Vec::with_capacity(types.len() * 8 + 4);

        for (idx, (token, imagetyp, master_imagetyp, frame_type, conflicting_token)) in
            types.into_iter().enumerate()
        {
            cases.push(PipelineCase {
                name: format!("capture_{token}_001.fits"),
                imagetyp: Some(imagetyp),
                stack_count: None,
                expected_type: Some(frame_type),
                expected_master: false,
                expected_detector: None,
            });
            cases.push(PipelineCase {
                name: format!("integration_{token}_030.fits"),
                imagetyp: Some(imagetyp),
                stack_count: Some((if idx % 2 == 0 { "STACKCNT" } else { "NCOMBINE" }, 30)),
                expected_type: Some(frame_type),
                expected_master: true,
                expected_detector: Some("siril"),
            });
            cases.push(PipelineCase {
                name: format!("master_{token}.fits"),
                imagetyp: None,
                stack_count: None,
                expected_type: Some(frame_type),
                expected_master: true,
                expected_detector: Some("pixinsight"),
            });
            cases.push(PipelineCase {
                name: format!("master_{conflicting_token}_header_{token}.fits"),
                imagetyp: Some(imagetyp),
                stack_count: None,
                expected_type: Some(frame_type),
                expected_master: true,
                expected_detector: Some("pixinsight"),
            });
            cases.push(PipelineCase {
                name: format!("{token}_sub_0001.fits"),
                imagetyp: None,
                stack_count: None,
                expected_type: None,
                expected_master: false,
                expected_detector: None,
            });
            cases.push(PipelineCase {
                name: format!("master_{token}_stackcnt_one.fits"),
                imagetyp: Some(imagetyp),
                stack_count: Some(("STACKCNT", 1)),
                expected_type: Some(frame_type),
                expected_master: false,
                expected_detector: Some("siril"),
            });
            cases.push(PipelineCase {
                name: format!("combined_header_{token}.fits"),
                imagetyp: Some(master_imagetyp),
                stack_count: None,
                expected_type: Some(frame_type),
                expected_master: true,
                expected_detector: Some("pixinsight"),
            });
            cases.push(PipelineCase {
                name: format!("{token}_LUM_stacked.fits"),
                imagetyp: None,
                stack_count: None,
                expected_type: Some(frame_type),
                expected_master: true,
                expected_detector: Some("pixinsight"),
            });
        }

        cases.extend([
            PipelineCase {
                name: "integration_stackcnt_only.fits".to_owned(),
                imagetyp: None,
                stack_count: Some(("STACKCNT", 30)),
                expected_type: None,
                expected_master: false,
                expected_detector: None,
            },
            PipelineCase {
                name: "integration_ncombine_only.fits".to_owned(),
                imagetyp: None,
                stack_count: Some(("NCOMBINE", 30)),
                expected_type: None,
                expected_master: false,
                expected_detector: None,
            },
            PipelineCase {
                name: "unknown_header_neutral_name.fits".to_owned(),
                imagetyp: Some("JUNKTYPE"),
                stack_count: None,
                expected_type: None,
                expected_master: false,
                expected_detector: None,
            },
            PipelineCase {
                name: "neutral_no_header.fits".to_owned(),
                imagetyp: None,
                stack_count: None,
                expected_type: None,
                expected_master: false,
                expected_detector: None,
            },
        ]);

        cases
    }

    #[test]
    fn realistic_headers_cover_master_permutations_through_scan_and_classify() {
        let cases = master_pipeline_cases();
        assert_eq!(
            cases.len(),
            44,
            "five frame types must exercise eight evidence paths plus four global negatives"
        );

        for case in cases {
            let tmp = tmpdir();
            write_realistic_fits(tmp.path(), &case.name, case.imagetyp, case.stack_count);
            let path = tmp.path().join(&case.name);

            let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
            assert_eq!(items.len(), 1, "{}: scan must return its FITS folder", case.name);
            assert_eq!(
                items[0].masters.len(),
                usize::from(case.expected_master),
                "{}: scan-time master result",
                case.name
            );
            if let Some(master) = items[0].masters.first() {
                assert_eq!(Some(master.detection.frame_type), case.expected_type, "{}", case.name);
                assert_eq!(
                    master.detection.detector,
                    case.expected_detector.unwrap(),
                    "{}",
                    case.name
                );
            }

            let classified =
                crate::classify::classify_one_file(&path, tmp.path(), &v1_normalization_table());
            assert_eq!(classified.frame_type, case.expected_type, "{}", case.name);
            assert_eq!(classified.is_master, case.expected_master, "{}", case.name);
            if let Some(detector) = case.expected_detector {
                assert_eq!(classified.master_detector, Some(detector), "{}", case.name);
            }
        }
    }

    #[test]
    fn single_light_folder() {
        let tmp = tmpdir();
        write_file(tmp.path(), "light_001.fits", b"dummy fits content");
        write_file(tmp.path(), "light_002.fits", b"dummy fits content 2");

        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fits_files.len(), 2);
        assert_eq!(items[0].lane, Lane::Fits);
    }

    #[test]
    fn recursive_subfolders_produce_leaf_items() {
        let tmp = tmpdir();
        let lights = tmp.path().join("lights");
        let darks = tmp.path().join("darks");
        fs::create_dir_all(&lights).unwrap();
        fs::create_dir_all(&darks).unwrap();

        write_file(&lights, "light_001.fits", b"l1");
        write_file(&darks, "dark_001.fits", b"d1");

        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items.len(), 2, "each leaf folder is one item");
    }

    #[test]
    fn intermediate_folder_without_files_is_not_an_item() {
        let tmp = tmpdir();
        let sub = tmp.path().join("date").join("target").join("lights");
        fs::create_dir_all(&sub).unwrap();
        write_file(&sub, "frame.fits", b"f");

        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items.len(), 1, "only the leaf with fits file is an item");
        assert!(items[0].folder_path.ends_with("lights"));
    }

    #[test]
    fn video_files_routed_to_video_lane() {
        let tmp = tmpdir();
        let planetary = tmp.path().join("planetary");
        fs::create_dir_all(&planetary).unwrap();
        write_file(&planetary, "jupiter.ser", b"SER data");

        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].lane, Lane::Video);
        assert_eq!(items[0].video_files.len(), 1);
    }

    #[test]
    fn empty_root_returns_no_items() {
        let tmp = tmpdir();
        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert!(items.is_empty());
    }

    #[test]
    fn xisf_files_included_in_fits_lane() {
        let tmp = tmpdir();
        write_file(tmp.path(), "frame.xisf", b"XISF0100 data");

        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].lane, Lane::Fits);
        // xisf_files list populated, fits_files empty
        assert_eq!(items[0].xisf_files.len(), 1);
        assert_eq!(items[0].fits_files.len(), 0);
    }

    #[test]
    fn non_directory_root_returns_error() {
        let tmp = tmpdir();
        let file_path = tmp.path().join("not_a_dir.fits");
        write_file(tmp.path(), "not_a_dir.fits", b"x");
        let err = scan_root(&file_path, &ScanOptions::default());
        assert!(err.is_err());
    }

    #[test]
    fn format_fits_for_fits_only_folder() {
        let tmp = tmpdir();
        write_file(tmp.path(), "dark_001.fits", b"f1");
        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items[0].format, FileFormat::Fits);
    }

    #[test]
    fn format_xisf_for_xisf_only_folder() {
        let tmp = tmpdir();
        write_file(tmp.path(), "dark_001.xisf", b"f1");
        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items[0].format, FileFormat::Xisf);
    }

    #[test]
    fn format_mixed_for_folder_with_fits_and_xisf() {
        let tmp = tmpdir();
        write_file(tmp.path(), "dark_001.fits", b"f1");
        write_file(tmp.path(), "dark_002.xisf", b"f2");
        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items[0].format, FileFormat::Mixed);
    }

    #[test]
    fn format_video_for_video_only_folder() {
        let tmp = tmpdir();
        let planetary = tmp.path().join("p");
        fs::create_dir_all(&planetary).unwrap();
        write_file(&planetary, "jupiter.ser", b"SER");
        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items[0].format, FileFormat::Video);
    }

    /// Non-FITS dummy files yield no masters (metadata unreadable → None).
    #[test]
    fn no_masters_for_dummy_fits_content() {
        let tmp = tmpdir();
        write_file(tmp.path(), "dark_001.fits", b"not a real fits file");
        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items.len(), 1);
        assert!(items[0].masters.is_empty(), "dummy file cannot be a master");
    }

    /// A master-shaped file name never outranks an unreadable header: the
    /// extractor rejects a header with no recognisable keyword, so no
    /// path-only inference runs (astro-plan-z7997).
    #[test]
    fn corrupt_master_named_fits_is_not_detected_as_a_master() {
        let tmp = tmpdir();
        let folder = tmp.path().join("calibration");
        fs::create_dir_all(&folder).unwrap();
        write_file(&folder, "masterDark.fits", &[0xffu8; 2880]);

        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items.len(), 1);
        assert!(
            items[0].masters.is_empty(),
            "a corrupt header carries no evidence for master detection"
        );
    }

    /// The neighbouring case the z7997 rejection must not swallow: a valid
    /// header without `IMAGETYP` still reaches path inference.
    #[test]
    fn keywordless_imagetyp_master_still_infers_frame_type_from_path() {
        let tmp = tmpdir();
        let folder = tmp.path().join("calibration");
        fs::create_dir_all(&folder).unwrap();
        write_realistic_fits(&folder, "masterDark.fits", None, None);

        let items = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items.len(), 1);
        let master = items[0].masters.first().expect("path inference must still detect a master");
        assert_eq!(master.detection.frame_type, FrameType::Dark);
        assert!(!master.detection.stack_count_evidence, "no header integration count present");
    }

    /// Constitution §I regression: a symlinked subdirectory reachable from the
    /// scan root must not be traversed unless `follow_symlinks` is enabled.
    #[cfg(unix)]
    #[test]
    fn symlinked_subdir_not_traversed_by_default() {
        use std::os::unix::fs::symlink;

        let tmp = tmpdir();
        let real_target = tmp.path().join("real_target");
        fs::create_dir_all(&real_target).unwrap();
        write_file(&real_target, "hidden.fits", b"hidden");

        let scan_root_dir = tmp.path().join("scan_root");
        fs::create_dir_all(&scan_root_dir).unwrap();
        symlink(&real_target, scan_root_dir.join("linked")).unwrap();

        let items = scan_root(&scan_root_dir, &ScanOptions::default()).unwrap().items;
        assert!(items.is_empty(), "must not see files behind an un-enabled symlink");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_subdir_traversed_when_follow_symlinks_enabled() {
        use std::os::unix::fs::symlink;

        let tmp = tmpdir();
        let real_target = tmp.path().join("real_target");
        fs::create_dir_all(&real_target).unwrap();
        write_file(&real_target, "visible.fits", b"visible");

        let scan_root_dir = tmp.path().join("scan_root");
        fs::create_dir_all(&scan_root_dir).unwrap();
        symlink(&real_target, scan_root_dir.join("linked")).unwrap();

        let options = ScanOptions { follow_symlinks: true, workers: None };
        let items = scan_root(&scan_root_dir, &options).unwrap().items;
        assert_eq!(items.len(), 1, "symlinked subdir is traversed when explicitly enabled");
    }

    /// Create a directory junction, the reparse-point kind `is_symlink()` does
    /// **not** report. Uses the `mklink /J` shell builtin (junctions need no
    /// admin privilege, unlike symlinks) rather than adding a dependency for
    /// two tests — the same approach as `fs_pathsafe`'s junction test.
    #[cfg(windows)]
    fn make_junction(link: &std::path::Path, target: &std::path::Path) {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J", link.to_str().unwrap(), target.to_str().unwrap()])
            .status()
            .expect("mklink invocation failed");
        assert!(status.success(), "mklink /J failed to create the test junction");
    }

    /// Windows-only counterpart to the two `cfg(unix)` symlink tests above.
    ///
    /// The Unix tests give no evidence about Windows: a followed junction can
    /// walk the scan into a loop or onto an unrelated drive and produce inbox
    /// items the user never pointed at (constitution product constraints).
    #[cfg(windows)]
    #[test]
    fn junction_subdir_not_traversed_by_default() {
        let tmp = tmpdir();
        let real_target = tmp.path().join("real_target");
        fs::create_dir_all(&real_target).unwrap();
        write_file(&real_target, "hidden.fits", b"hidden");

        let scan_root_dir = tmp.path().join("scan_root");
        fs::create_dir_all(&scan_root_dir).unwrap();
        make_junction(&scan_root_dir.join("junction_to_target"), &real_target);

        let items = scan_root(&scan_root_dir, &ScanOptions::default()).unwrap().items;
        assert!(items.is_empty(), "must not see files behind an un-enabled junction");
    }

    /// The opt-in direction, mirroring
    /// `symlinked_subdir_traversed_when_follow_symlinks_enabled`.
    #[cfg(windows)]
    #[test]
    fn junction_subdir_traversed_when_follow_symlinks_enabled() {
        let tmp = tmpdir();
        let real_target = tmp.path().join("real_target");
        fs::create_dir_all(&real_target).unwrap();
        write_file(&real_target, "visible.fits", b"visible");

        let scan_root_dir = tmp.path().join("scan_root");
        fs::create_dir_all(&scan_root_dir).unwrap();
        make_junction(&scan_root_dir.join("junction_to_target"), &real_target);

        let options = ScanOptions { follow_symlinks: true, workers: None };
        let items = scan_root(&scan_root_dir, &options).unwrap().items;
        assert_eq!(items.len(), 1, "junction is traversed when explicitly enabled");
    }

    /// Directory link usable on both lanes: a symlink on Unix, a junction on
    /// Windows, where `is_symlink()` does not report the reparse point.
    fn link_dir(link: &Path, target: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        make_junction(link, target);
    }

    /// A link back to an ancestor is re-entered without a visited set, so the
    /// one real directory is inventoried once per re-entry — 16 items where one
    /// is expected. The walk still ends, because each re-entry adds a link
    /// component and the OS caps symlink resolution (`ELOOP`), so the bound
    /// comes from the platform rather than from the walker.
    #[test]
    fn opt_in_traversal_terminates_on_a_directory_cycle() {
        let tmp = tmpdir();
        let root = tmp.path().join("library");
        let frames = root.join("frames");
        fs::create_dir_all(&frames).unwrap();
        write_file(&frames, "light_001.fits", b"light");
        link_dir(&frames.join("back_to_root"), &root);

        let options = ScanOptions { follow_symlinks: true, workers: None };
        let items = scan_root(&root, &options).unwrap().items;
        assert_eq!(items.len(), 1, "the one real frame is inventoried exactly once");
    }

    /// Constitution §I: the root itself is a link, and `is_dir()` follows it, so
    /// the default scan walked into it. The gate applies to the root on the same
    /// terms as to any entry below it.
    #[test]
    fn symlinked_scan_root_rejected_by_default() {
        let tmp = tmpdir();
        let real_root = tmp.path().join("real_root");
        fs::create_dir_all(&real_root).unwrap();
        write_file(&real_root, "hidden.fits", b"hidden");

        let linked_root = tmp.path().join("linked_root");
        link_dir(&linked_root, &real_root);

        let error = scan_root(&linked_root, &ScanOptions::default())
            .expect_err("a symlinked root must be refused while following is disabled");
        assert!(
            error.contains("scan root is unusable"),
            "error must name the root as unusable, got: {error}"
        );
    }

    #[test]
    fn symlinked_scan_root_accepted_when_follow_symlinks_enabled() {
        let tmp = tmpdir();
        let real_root = tmp.path().join("real_root");
        fs::create_dir_all(&real_root).unwrap();
        write_file(&real_root, "visible.fits", b"visible");

        let linked_root = tmp.path().join("linked_root");
        link_dir(&linked_root, &real_root);

        let options = ScanOptions { follow_symlinks: true, workers: None };
        let items = scan_root(&linked_root, &options).unwrap().items;
        assert_eq!(items.len(), 1, "a symlinked root is walkable when explicitly enabled");
    }

    /// Multi-leaf sort-order is deterministic regardless of OS readdir ordering
    /// and regardless of whether the sequential or parallel code path is used.
    ///
    /// Three sibling leaf directories are compared: the expected order is
    /// ascending by relative path, which is what the sort at the end of
    /// `scan_root` guarantees.
    #[test]
    fn multi_leaf_relative_path_sort_order() {
        let tmp = tmpdir();
        for name in &["zz_folder", "aa_folder", "mm_folder"] {
            let dir = tmp.path().join(name);
            fs::create_dir_all(&dir).unwrap();
            write_file(&dir, "frame.fits", b"dummy");
        }

        let expected = ["aa_folder", "mm_folder", "zz_folder"];

        // Sequential path (production default).
        let items_seq = scan_root(tmp.path(), &ScanOptions::default()).unwrap().items;
        assert_eq!(items_seq.len(), 3);
        for (item, exp) in items_seq.iter().zip(expected.iter()) {
            assert!(
                item.folder_path.ends_with(exp),
                "sequential: expected suffix {exp}, got {:?}",
                item.folder_path
            );
        }

        // Parallel path (workers=2 exercises the thread-scope branch).
        let opts_par = ScanOptions { follow_symlinks: false, workers: Some(2) };
        let items_par = scan_root(tmp.path(), &opts_par).unwrap().items;
        assert_eq!(items_par.len(), 3);
        for (item, exp) in items_par.iter().zip(expected.iter()) {
            assert!(
                item.folder_path.ends_with(exp),
                "parallel: expected suffix {exp}, got {:?}",
                item.folder_path
            );
        }
    }

    /// An unreadable / corrupt file in one leaf must not abort the scan;
    /// the other leaves must still be returned. Exercises the PARALLEL path
    /// (sequential coverage lives in `no_masters_for_dummy_fits_content`).
    #[test]
    fn bad_file_does_not_abort_parallel_scan() {
        let tmp = tmpdir();

        // Leaf A: good files.
        let good = tmp.path().join("good");
        fs::create_dir_all(&good).unwrap();
        write_file(&good, "frame_001.fits", b"dummy fits");
        write_file(&good, "frame_002.fits", b"dummy fits");

        // Leaf B: a file whose content is not valid FITS/XISF (unreadable
        // header) — must not propagate an error that kills the scan.
        let bad = tmp.path().join("bad");
        fs::create_dir_all(&bad).unwrap();
        write_file(&bad, "corrupt.fits", b"this is not a fits header at all");

        let opts = ScanOptions { follow_symlinks: false, workers: Some(2) };
        let items = scan_root(tmp.path(), &opts).unwrap().items;

        // Both leaves must appear (bad content is tolerated, not fatal).
        assert_eq!(items.len(), 2, "corrupt file in one leaf must not drop the other leaf");

        // The corrupt leaf still produces an item (the file is listed), but
        // master detection silently returns nothing for unreadable headers.
        let bad_item = items.iter().find(|i| i.folder_path.ends_with("bad")).unwrap();
        assert_eq!(bad_item.fits_files.len(), 1);
        assert!(bad_item.masters.is_empty(), "corrupt file must not be reported as a master");
    }

    /// An unreadable subdir (chmod 000) must not abort the scan.
    ///
    /// The readable sibling must still be returned as an item, and the
    /// unreadable dir must appear exactly once in `warnings`. The root itself
    /// remains readable, so `scan_root` returns `Ok`.
    ///
    /// Permission changes are Unix-only; the test is gated on `cfg(unix)` and
    /// skipped automatically when chmod had no effect (e.g. running as root).
    #[cfg(unix)]
    #[test]
    fn unreadable_subdir_produces_warning_not_error() {
        use std::os::unix::fs::PermissionsExt;

        // Defined before any statements so `clippy::items_after_statements` is satisfied.
        struct RestorePerms(std::path::PathBuf);
        impl Drop for RestorePerms {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
        }

        let tmp = tmpdir();

        // Readable leaf with real files.
        let good = tmp.path().join("good");
        fs::create_dir_all(&good).unwrap();
        write_file(&good, "frame.fits", b"dummy fits");

        // Subdir made unreadable.
        let locked = tmp.path().join("locked");
        fs::create_dir_all(&locked).unwrap();
        write_file(&locked, "hidden.fits", b"hidden");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        // Restore permissions on drop so tempdir cleanup succeeds.
        let _restore = RestorePerms(locked.clone());

        // Skip when chmod had no effect (running as root bypasses permission bits).
        if fs::read_dir(&locked).is_ok() {
            return;
        }

        let out = scan_root(tmp.path(), &ScanOptions::default()).unwrap();

        assert_eq!(out.items.len(), 1, "readable leaf must still be found");
        assert!(out.items[0].folder_path.ends_with("good"), "only the good folder is an item");

        assert_eq!(out.warnings.len(), 1, "exactly one warning for the locked dir");
        assert_eq!(out.warnings[0].path, locked, "warning path must match the locked dir");
        assert!(!out.warnings[0].reason.is_empty(), "warning must carry a non-empty reason");
    }
}
