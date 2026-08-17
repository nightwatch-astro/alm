// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Video file detection for the Inbox `video` lane (spec 005 T-VideoDetect).
//!
//! This crate performs extension-based detection only. No pixel or container
//! parsing is done here. Files with video extensions are routed to
//! `lane = "video"` in the inbox scan and are NOT subject to FITS/XISF
//! classification.
//!
//! Out-of-scope for spec 005: planetary/lunar metadata extraction, SER header
//! parsing, frame-count/duration extraction. Those belong to a future spec.
//! (Ref: R-Video-1, T-VideoLaneDocs)
#![allow(clippy::doc_markdown)]

/// A video file record discovered during an inbox scan.
///
/// Contains path metadata only; no frame content is inspected here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFileRecord {
    /// Absolute or root-relative path to the file.
    pub path: std::path::PathBuf,
    /// File name (last component of `path`).
    pub file_name: String,
    /// Lower-case extension without the leading dot.
    pub extension: String,
}

/// Video extensions recognized for inbox `lane = "video"` routing.
///
/// `.ser` — SharpCap, FireCapture, ZWO, ASIAIR planetary capture
/// `.avi` — legacy Windows video container (various)
/// `.mp4` — modern compressed video
/// `.mov` — QuickTime container (macOS capture tools)
const VIDEO_EXTENSIONS: &[&str] = &["ser", "avi", "mp4", "mov"];

/// Returns `true` if the given file extension (without dot, case-insensitive)
/// is recognized as a video format.
#[must_use]
pub fn is_video_extension(ext: &str) -> bool {
    let lower = ext.trim().to_ascii_lowercase();
    VIDEO_EXTENSIONS.contains(&lower.as_str())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_ser_extension() {
        assert!(is_video_extension("ser"));
        assert!(is_video_extension("SER"));
        assert!(is_video_extension("Ser"));
    }

    #[test]
    fn recognizes_avi_mp4_mov() {
        assert!(is_video_extension("avi"));
        assert!(is_video_extension("AVI"));
        assert!(is_video_extension("mp4"));
        assert!(is_video_extension("MP4"));
        assert!(is_video_extension("mov"));
        assert!(is_video_extension("MOV"));
    }

    #[test]
    fn rejects_fits_and_xisf() {
        assert!(!is_video_extension("fits"));
        assert!(!is_video_extension("fit"));
        assert!(!is_video_extension("xisf"));
    }
}
