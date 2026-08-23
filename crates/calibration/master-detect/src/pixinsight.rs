// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! PixInsight / WBPP-specific calibration master detector (spec 040, FR-002).
//!
//! # PixInsight / WBPP master heuristics
//!
//! WBPP writes the word "master" **into** the `IMAGETYP` keyword value:
//!   - `IMAGETYP = "Master Dark"`
//!   - `IMAGETYP = "Master Bias"`
//!   - `IMAGETYP = "Master Flat"`
//!
//! It also stores masters in paths like `masterDarks/masterDark.xisf`.
//!
//! A master signature is `IMAGETYP` (lowercased) containing `"master"`, or a
//! file name or relative path carrying a `master`-prefixed token or
//! `"_stacked"`. Detection priority for `is_master`:
//! 1. Where the signature is in `IMAGETYP` — or `IMAGETYP` is absent and the
//!    path carries it — a present `STACKCNT`/`NCOMBINE` count decides (`> 1` is
//!    a master) and is reported as `stack_count_evidence`.
//! 2. Otherwise the signature alone decides and no header evidence is reported.
//!    A plain base `IMAGETYP` next to a count is Siril's convention, which
//!    `SirilDetector` owns however the file is named.
//!
//! The base frame type is determined by stripping "master" from `IMAGETYP`
//! before parsing, so `"Master Dark"` → `"Dark"` → `FrameType::Dark`.
//!
//! If `IMAGETYP` is absent but the path carries a `master`-prefixed token or
//! `"_stacked"`, this detector still returns a result — but only if the path
//! itself reveals the frame type (e.g. `"masterDarks/"`). In practice a
//! PixInsight XISF will always carry `IMAGETYP`; the path fallback is a safety
//! net for edge cases.

use crate::{
    parse_frame_type, path_looks_like_master, DetectInput, MasterDetection, MasterDetector,
};

/// Detector for PixInsight / WBPP-created calibration masters.
pub struct PixInsightDetector;

impl MasterDetector for PixInsightDetector {
    fn id(&self) -> &'static str {
        "pixinsight"
    }

    fn detect(&self, input: &DetectInput<'_>) -> Option<MasterDetection> {
        let imagetyp_lc = input.imagetyp.map(str::to_ascii_lowercase);

        // Determine whether the IMAGETYP itself signals a master.
        let imagetyp_has_master = imagetyp_lc.as_deref().is_some_and(|s| s.contains("master"));

        // Determine whether the path signals a master (fallback).
        let path_has_master = path_looks_like_master(input.file_name, input.rel_path);

        let has_master_naming = imagetyp_has_master || path_has_master;

        // A plain base IMAGETYP alongside STACKCNT/NCOMBINE is Siril's own
        // signature, and a header pair outranks a file name: claiming the count
        // there would outrank `SirilDetector` in `detect_master` and relabel a
        // Siril master as PixInsight's. The count is this detector's evidence
        // only where the master signature is in IMAGETYP, or IMAGETYP is absent
        // and the path carries it.
        let owns_header_count =
            if input.imagetyp.is_some() { imagetyp_has_master } else { path_has_master };
        let (is_master, stack_count_evidence) = match input.stack_count {
            Some(n) if owns_header_count => (n > 1, true),
            _ => (has_master_naming, false),
        };

        // Determine base frame type.
        // Prefer IMAGETYP (strip "master" before parsing). Fall back to path
        // heuristics only when IMAGETYP is absent.
        let frame_type = if let Some(raw) = input.imagetyp {
            parse_frame_type(raw)?
        } else {
            // No IMAGETYP: try to infer frame type from the path.
            // Only do this when the path also signals a master; otherwise there
            // is not enough evidence for PixInsight-specific detection.
            if !path_has_master {
                return None;
            }
            infer_frame_type_from_path(input.file_name, input.rel_path)?
        };

        Some(MasterDetection { frame_type, is_master, detector: self.id(), stack_count_evidence })
    }
}

/// Try to infer a frame type from the file name or path when IMAGETYP is absent.
///
/// Looks for type keywords in the lowercased path/name.
fn infer_frame_type_from_path(file_name: &str, rel_path: &str) -> Option<metadata_core::FrameType> {
    let combined = format!("{} {}", file_name.to_ascii_lowercase(), rel_path.to_ascii_lowercase());
    // Reuse parse_frame_type; strip "master" first.
    let without_master = combined.replace("master", "");
    parse_frame_type(&without_master)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect_master;
    use metadata_core::FrameType;

    fn input<'a>(
        imagetyp: Option<&'a str>,
        stack_count: Option<u32>,
        file_name: &'a str,
        rel_path: &'a str,
    ) -> DetectInput<'a> {
        DetectInput { imagetyp, stack_count, file_name, rel_path }
    }

    // ── Positive cases ────────────────────────────────────────────────────────

    /// PixInsight XISF master: IMAGETYP="Master Dark" → Dark + is_master=true (SC-001)
    #[test]
    fn pixinsight_xisf_master_dark() {
        let inp = input(Some("Master Dark"), None, "masterDark.xisf", "masterDarks/");
        let result = PixInsightDetector.detect(&inp).unwrap();
        assert_eq!(result.frame_type, FrameType::Dark);
        assert!(result.is_master);
        assert_eq!(result.detector, "pixinsight");
    }

    /// PixInsight XISF master: IMAGETYP="Master Bias" → Bias + is_master=true
    #[test]
    fn pixinsight_xisf_master_bias() {
        let inp = input(Some("Master Bias"), None, "masterBias.xisf", "masterBias/");
        let result = PixInsightDetector.detect(&inp).unwrap();
        assert_eq!(result.frame_type, FrameType::Bias);
        assert!(result.is_master);
    }

    /// PixInsight XISF master: IMAGETYP="Master Flat" → Flat + is_master=true
    #[test]
    fn pixinsight_xisf_master_flat() {
        let inp = input(Some("Master Flat"), None, "masterFlat_Ha.xisf", "masterFlats/");
        let result = PixInsightDetector.detect(&inp).unwrap();
        assert_eq!(result.frame_type, FrameType::Flat);
        assert!(result.is_master);
    }

    /// Name fallback: no "master" IMAGETYP but name contains "master" → is_master=true (SC-001 case 3)
    #[test]
    fn pixinsight_name_fallback_flat_ha() {
        // IMAGETYP is a plain "FLAT" (no "master" in it); name has "master"
        let inp = input(Some("FLAT"), None, "masterFlat_Ha.xisf", "calibration/");
        let result = PixInsightDetector.detect(&inp).unwrap();
        assert_eq!(result.frame_type, FrameType::Flat);
        assert!(result.is_master, "file name with 'master' prefix must set is_master");
        assert_eq!(result.detector, "pixinsight");
    }

    /// _stacked in name triggers is_master even with plain IMAGETYP.
    #[test]
    fn pixinsight_stacked_suffix_in_name() {
        let inp = input(Some("DARK"), None, "dark_Ha_stacked.xisf", "calibration/");
        let result = PixInsightDetector.detect(&inp).unwrap();
        assert_eq!(result.frame_type, FrameType::Dark);
        assert!(result.is_master);
    }

    // ── Negative cases ────────────────────────────────────────────────────────

    /// Plain IMAGETYP="DARK", no master signals → is_master=false (SC-001 case 4).
    #[test]
    fn pixinsight_plain_dark_sub_is_not_master() {
        let inp = input(Some("DARK"), None, "dark_001.fits", "calibration/darks/");
        let result = PixInsightDetector.detect(&inp).unwrap();
        assert_eq!(result.frame_type, FrameType::Dark);
        assert!(!result.is_master, "a plain sub must not be flagged as master");
        assert_eq!(result.detector, "pixinsight");
    }

    /// No IMAGETYP and no master in path → None.
    #[test]
    fn pixinsight_no_imagetyp_no_master_path_returns_none() {
        let inp = input(None, None, "dark_001.fits", "calibration/darks/");
        assert!(PixInsightDetector.detect(&inp).is_none());
    }

    /// Asserted against the detector rather than `detect_master`, because
    /// `SirilDetector` also reports a present count and answers first whenever
    /// `IMAGETYP` is parseable, which would leave this arm unexercised.
    #[test]
    fn a_present_stack_count_decides_a_pixinsight_master_on_its_own() {
        let stacked = input(Some("Master Dark"), Some(30), "masterDark.xisf", "masters/");
        let result = PixInsightDetector.detect(&stacked).unwrap();
        assert!(result.stack_count_evidence, "STACKCNT=30 is header evidence");
        assert!(result.is_master);

        let single = input(Some("Master Dark"), Some(1), "masterDark.xisf", "masters/");
        let result = PixInsightDetector.detect(&single).unwrap();
        assert!(result.stack_count_evidence);
        assert!(!result.is_master, "STACKCNT=1 outranks every master naming signal");
    }

    /// A plain base `IMAGETYP` plus a count is Siril's signature, not
    /// PixInsight's, so this detector must not claim header evidence for it and
    /// must not outrank `SirilDetector` in `detect_master`. A `master` token in
    /// the file name does not transfer the count: a header pair outranks a name.
    #[test]
    fn a_plain_imagetyp_beside_a_count_is_not_this_detector_s_evidence() {
        let unnamed = input(Some("DARK"), Some(30), "dark_030.fits", "calibration/darks/");
        let result = PixInsightDetector.detect(&unnamed).unwrap();
        assert!(!result.stack_count_evidence);
        assert!(!result.is_master);
        assert_eq!(detect_master(&unnamed).unwrap().detector, "siril");

        let named = input(Some("LIGHT"), Some(1), "master_light_stackcnt_one.fits", "");
        let result = PixInsightDetector.detect(&named).unwrap();
        assert!(!result.stack_count_evidence, "a name must not transfer the count");
        let arbitrated = detect_master(&named).unwrap();
        assert_eq!(arbitrated.detector, "siril");
        assert!(!arbitrated.is_master, "STACKCNT=1 is not a master whoever reports it");
    }
}
