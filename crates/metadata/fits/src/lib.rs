// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure-Rust FITS header reader (spec 005 T005), built on the `fits-header` crate.
//!
//! # FITS Header Format
//!
//! A FITS file begins with one or more 2880-byte blocks. Each block contains
//! 36 header cards; each card is exactly 80 ASCII bytes. The header ends at the
//! first card whose first 8 bytes are `END     `.
//!
//! This extractor reads only the header keywords required for inbox
//! classification: `IMAGETYP`, `FILTER`, `OBJECT`, `EXPTIME`, `EXPOSURE`,
//! `GAIN`, `XBINNING`, `YBINNING`, `NAXIS1`, `NAXIS2`, `INSTRUME`, `TELESCOP`,
//! `DATE-OBS`.
//!
//! No cfitsio or heavy C dependencies — `fits-header` is pure Rust. A missing
//! keyword yields `None` rather than an error. A header the parser cannot
//! handle at all — including one that panics inside `fits-header` — yields
//! [`MetadataExtractError::Parse`]; [`FitsExtractor::extract`] never panics.
//!
//! A silent mis-parse passes through this adapter undetected, because it does
//! not validate that the header block is ASCII: `fits-header` lossy-decodes
//! each card before slicing it at fixed offsets, so one invalid UTF-8 byte
//! shifts every later field and can yield a wrong keyword and value with no
//! panic and no error.
#![allow(clippy::doc_markdown)]

use std::io::{self, Read};
use std::path::Path;

use fits_header::{FitsError, FromCard, Header};
use metadata_core::{
    interpret_wcs_pointing, sexagesimal_dec_to_deg, sexagesimal_ra_to_deg, MetadataExtractError,
    MetadataExtractor, RawFileMetadata,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const BLOCK_SIZE: usize = 2880;
const CARD_SIZE: usize = 80;

/// Maximum number of blocks to read before giving up.
/// A typical FITS primary header is 1-4 blocks; 32 is extremely generous.
const MAX_HEADER_BLOCKS: usize = 32;

// ── FitsExtractor ─────────────────────────────────────────────────────────────

/// Adapter that reads standard FITS header keywords using a pure-Rust parser.
///
/// Supports `.fits`, `.fit`, `.fts` extensions.
pub struct FitsExtractor;

impl MetadataExtractor for FitsExtractor {
    fn extract(&self, path: &Path) -> Result<Option<RawFileMetadata>, MetadataExtractError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        if !self.supports_extension(&ext) {
            return Ok(None);
        }

        let file = std::fs::File::open(path).map_err(|e| MetadataExtractError::Io {
            path: path.display().to_string(),
            msg: e.to_string(),
        })?;

        let bytes = read_header_bytes(file).map_err(|e| MetadataExtractError::Io {
            path: path.display().to_string(),
            msg: e.to_string(),
        })?;

        // `fits-header` 0.4.2 lossy-decodes the block and then slices the
        // resulting `String` at fixed byte offsets (`parse.rs:38`, `:47`,
        // `:61`, `:92`, `:98`). A non-ASCII byte widens to a 3-byte U+FFFD, so
        // those
        // offsets can land inside a char and panic. Containing that here
        // depends on panics unwinding: the release profile in the workspace
        // `Cargo.toml` deliberately keeps the default `panic = "unwind"`.
        let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fits_header::Header::parse(&bytes)
        }))
        .map_err(|_| MetadataExtractError::Parse {
            path: path.display().to_string(),
            msg: "the FITS header parser panicked on malformed header bytes".to_owned(),
        })?;

        // Header::parse never actually errs (parsing is lenient by design);
        // propagate defensively rather than unwrap.
        let header = parsed.map_err(|e| MetadataExtractError::Parse {
            path: path.display().to_string(),
            msg: e.to_string(),
        })?;

        Ok(Some(parse_header(&header)))
    }

    fn supports_extension(&self, ext: &str) -> bool {
        matches!(ext, "fits" | "fit" | "fts")
    }
}

// ── Header reading ────────────────────────────────────────────────────────────

/// Read header blocks from `reader`, stopping once the `END` card is seen or
/// after `MAX_HEADER_BLOCKS` (whichever first).
///
/// FITS files can carry gigabytes of pixel data after the header; bounding the
/// read here keeps header extraction cheap regardless of file size instead of
/// pulling the whole file into memory for `fits_header::parse`.
fn read_header_bytes(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(BLOCK_SIZE);
    let mut block = [0u8; BLOCK_SIZE];

    for _ in 0..MAX_HEADER_BLOCKS {
        match reader.read_exact(&mut block) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }

        let has_end = block
            .as_chunks::<CARD_SIZE>()
            .0
            .iter()
            .any(|c| c.starts_with(b"END     ") || c.starts_with(b"END\x00"));
        bytes.extend_from_slice(&block);
        if has_end {
            break;
        }
    }

    Ok(bytes)
}

// ── Header field extraction ──────────────────────────────────────────────────

/// Typed keyword read that never hard-errors: a duplicated keyword
/// (`FitsError::AmbiguousKeyword`) falls back to its first occurrence instead
/// of surfacing an error, preserving this extractor's "always `None`, never
/// `Err`" contract for corrupt/odd files.
fn get<T: FromCard>(header: &Header, key: &str) -> Option<T> {
    match header.get::<T>(key) {
        Ok(v) => v,
        Err(FitsError::AmbiguousKeyword { .. }) => header.get_all::<T>(key).into_iter().next(),
        Err(_) => None,
    }
}

/// Like [`get`] but for quoted string values (`Header::get_str`).
fn get_str(header: &Header, key: &str) -> Option<String> {
    match header.get_str(key) {
        Ok(v) => v.map(str::to_owned),
        Err(FitsError::AmbiguousKeyword { .. }) => header.get_all::<String>(key).into_iter().next(),
        Err(_) => None,
    }
}

/// Map header keywords into [`RawFileMetadata`].
///
/// The observer-location fallback chains and RA/DEC decimal-over-sexagesimal
/// precedence are shared value shapes but are intentionally kept as separate
/// keyword lookups (fallback precedence); the field list is necessarily long.
// A struct-literal initializer would be far less readable than these named
// per-field assignments given the multi-keyword fallback chains.
#[allow(clippy::field_reassign_with_default)]
fn parse_header(header: &Header) -> RawFileMetadata {
    let mut meta = RawFileMetadata::default();

    meta.image_typ = get_str(header, "IMAGETYP");
    meta.filter = get_str(header, "FILTER");
    meta.object = get_str(header, "OBJECT");
    // EXPTIME takes priority; EXPOSURE is the fallback.
    meta.exposure = get::<String>(header, "EXPTIME").or_else(|| get::<String>(header, "EXPOSURE"));
    meta.gain = get(header, "GAIN");
    meta.x_binning = get(header, "XBINNING");
    meta.y_binning = get(header, "YBINNING");
    meta.naxis1 = get(header, "NAXIS1");
    meta.naxis2 = get(header, "NAXIS2");
    meta.instrume = get_str(header, "INSTRUME");
    meta.telescop = get_str(header, "TELESCOP");
    meta.date_obs = get_str(header, "DATE-OBS");

    // Stack/integration count: STACKCNT (preferred) or NCOMBINE fallback.
    meta.stack_count = get::<u32>(header, "STACKCNT").or_else(|| get::<u32>(header, "NCOMBINE"));

    // ── Extended extracted metadata (spec 041 T062, R-9/R-18) ───────────────
    // Offset / pedestal: OFFSET preferred, BLKLEVEL fallback.
    meta.offset = get::<i64>(header, "OFFSET").or_else(|| get::<i64>(header, "BLKLEVEL"));
    // Temperatures.
    meta.set_temp_c = get(header, "SET-TEMP");
    // CCD-TEMP preferred; DET-TEMP is the DWARF III non-standard fallback.
    meta.ccd_temp_c = get::<f64>(header, "CCD-TEMP").or_else(|| get::<f64>(header, "DET-TEMP"));
    // Pointing: decimal RA/DEC preferred over sexagesimal OBJCTRA/OBJCTDEC.
    meta.ra_deg = get::<f64>(header, "RA")
        .or_else(|| get_str(header, "OBJCTRA").and_then(|s| sexagesimal_ra_to_deg(&s)));
    meta.dec_deg = get::<f64>(header, "DEC")
        .or_else(|| get_str(header, "OBJCTDEC").and_then(|s| sexagesimal_dec_to_deg(&s)));
    // Rotation: ROTATANG (= ROTATOR, mechanical) is the flat-match key;
    // OBJCTROT is the informational sky position angle. Never swap them.
    meta.rotator_angle_deg =
        get::<f64>(header, "ROTATANG").or_else(|| get::<f64>(header, "ROTATOR"));
    meta.rotator_name = get_str(header, "ROTNAME");
    meta.sky_rotation_deg = get(header, "OBJCTROT");
    // Readout mode (optional grouping dim).
    meta.readout_mode = get_str(header, "READOUTM");
    // Optic train inputs.
    meta.focal_length_mm = get(header, "FOCALLEN");
    meta.pixel_size_um = get::<f64>(header, "XPIXSZ").or_else(|| get::<f64>(header, "PIXSIZE"));
    // Observer location: SITE* preferred, OBSGEO-* then *-OBS fallbacks.
    meta.observer_lat = get::<f64>(header, "SITELAT")
        .or_else(|| get::<f64>(header, "OBSGEO-B"))
        .or_else(|| get::<f64>(header, "LAT-OBS"));
    meta.observer_long = get::<f64>(header, "SITELONG")
        .or_else(|| get::<f64>(header, "OBSGEO-L"))
        .or_else(|| get::<f64>(header, "LONG-OBS"));
    meta.observer_elev = get::<f64>(header, "SITEELEV")
        .or_else(|| get::<f64>(header, "OBSGEO-H"))
        .or_else(|| get::<f64>(header, "ALT-OBS"));
    // Time keywords.
    meta.date_loc = get_str(header, "DATE-LOC");
    meta.date_end = get_str(header, "DATE-END");
    meta.mjd_avg = get(header, "MJD-AVG");
    meta.mjd_obs = get(header, "MJD-OBS");

    // Plate-solved WCS pointing (spec 052 P3, FR-012): passthrough keyword
    // reads only — interpretation (the CTYPE solve-gate, CD/CROTA2 rotation)
    // lives once in `metadata_core::interpret_wcs_pointing`.
    if let Some(wcs) = interpret_wcs_pointing(
        get_str(header, "CTYPE1").as_deref(),
        get_str(header, "CTYPE2").as_deref(),
        get(header, "CRVAL1"),
        get(header, "CRVAL2"),
        get(header, "CD1_1"),
        get(header, "CD2_1"),
        get(header, "CROTA2"),
    ) {
        meta.wcs_ra_deg = Some(wcs.ra_deg);
        meta.wcs_dec_deg = Some(wcs.dec_deg);
        meta.wcs_rotation_deg = wcs.rotation_deg;
    }

    meta
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────────────

    /// Build a single FITS block (2880 bytes) from a list of 80-char card strings.
    /// Missing cards are filled with spaces. Appends an END card.
    fn build_fits_header(cards: &[&str]) -> Vec<u8> {
        const CARDS_PER_BLOCK: usize = BLOCK_SIZE / CARD_SIZE;
        let mut block = vec![b' '; BLOCK_SIZE];
        let mut idx = 0usize;

        for card_str in cards {
            if idx >= CARDS_PER_BLOCK {
                break;
            }
            let bytes = card_str.as_bytes();
            let len = bytes.len().min(CARD_SIZE);
            block[idx * CARD_SIZE..idx * CARD_SIZE + len].copy_from_slice(&bytes[..len]);
            idx += 1;
        }

        // Write END card
        if idx < CARDS_PER_BLOCK {
            let end_pos = idx * CARD_SIZE;
            block[end_pos..end_pos + 3].copy_from_slice(b"END");
        }

        block
    }

    /// Pad a card string to exactly 80 chars.
    fn pad80(s: &str) -> String {
        format!("{s:<80}")
    }

    /// Parse a set of 80-char card strings into [`RawFileMetadata`] via the
    /// real [`fits_header::Header::parse`] + [`parse_header`] path.
    fn parse(cards: &[String]) -> RawFileMetadata {
        let refs: Vec<&str> = cards.iter().map(String::as_str).collect();
        let block = build_fits_header(&refs);
        let header = fits_header::Header::parse(&block).unwrap();
        parse_header(&header)
    }

    // ── IMAGETYP extraction ───────────────────────────────────────────────────

    #[test]
    fn parses_imagetyp_light_frame() {
        let meta = parse(&[pad80("IMAGETYP= 'Light Frame'")]);
        assert_eq!(meta.image_typ, Some("Light Frame".to_owned()));
    }

    #[test]
    fn parses_imagetyp_dark() {
        let meta = parse(&[pad80("IMAGETYP= 'Dark Frame'           / frame type")]);
        assert_eq!(meta.image_typ, Some("Dark Frame".to_owned()));
    }

    #[test]
    fn parses_imagetyp_bias() {
        let meta = parse(&[pad80("IMAGETYP= 'Bias Frame'")]);
        assert_eq!(meta.image_typ, Some("Bias Frame".to_owned()));
    }

    #[test]
    fn parses_filter() {
        let meta = parse(&[pad80("FILTER  = 'Ha      '")]);
        assert_eq!(meta.filter, Some("Ha".to_owned()));
    }

    #[test]
    fn parses_object() {
        let meta = parse(&[pad80("OBJECT  = 'NGC 7000'           / object name")]);
        assert_eq!(meta.object, Some("NGC 7000".to_owned()));
    }

    #[test]
    fn parses_exptime() {
        let meta = parse(&[pad80("EXPTIME =                 300.0 / exposure time in seconds")]);
        assert_eq!(meta.exposure, Some("300.0".to_owned()));
    }

    #[test]
    fn parses_gain() {
        let meta = parse(&[pad80("GAIN    =                   100 / camera gain")]);
        assert_eq!(meta.gain, Some("100".to_owned()));
    }

    #[test]
    fn parses_naxis() {
        let meta = parse(&[
            pad80("NAXIS1  =                  4144"),
            pad80("NAXIS2  =                  2822"),
        ]);
        assert_eq!(meta.naxis1, Some("4144".to_owned()));
        assert_eq!(meta.naxis2, Some("2822".to_owned()));
    }

    #[test]
    fn parses_instrume_and_telescop() {
        let meta = parse(&[pad80("INSTRUME= 'ZWO ASI2600MM Pro'"), pad80("TELESCOP= 'AT130-EDT'")]);
        assert_eq!(meta.instrume, Some("ZWO ASI2600MM Pro".to_owned()));
        assert_eq!(meta.telescop, Some("AT130-EDT".to_owned()));
    }

    #[test]
    fn parses_date_obs() {
        let meta = parse(&[pad80("DATE-OBS= '2025-10-10T22:15:00'")]);
        assert_eq!(meta.date_obs, Some("2025-10-10T22:15:00".to_owned()));
    }

    #[test]
    fn missing_keywords_return_none() {
        let meta =
            parse(&[pad80("SIMPLE  =                    T / file conforms to FITS standard")]);
        assert!(meta.image_typ.is_none());
        assert!(meta.filter.is_none());
        assert!(meta.object.is_none());
    }

    #[test]
    fn empty_block_returns_empty_metadata() {
        let meta = parse(&[]);
        assert!(meta.image_typ.is_none());
    }

    /// Regression for astro-plan-3v3r.20.33 / 20.16: a `.fits` file whose first
    /// block is non-ASCII drives `fits-header` 0.4.2 into a non-char-boundary
    /// slice, which in production unwinds out of the inbox scan.
    #[test]
    fn malformed_header_bytes_are_an_error_not_a_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("corrupt.fits");
        std::fs::write(&path, [0xffu8; BLOCK_SIZE]).expect("write fixture");

        let err = FitsExtractor.extract(&path).expect_err("malformed header must be an error");
        assert!(
            matches!(err, MetadataExtractError::Parse { .. }),
            "expected a Parse error, got {err:?}"
        );
    }

    #[test]
    fn extractor_rejects_xisf_extension() {
        let extractor = FitsExtractor;
        assert!(!extractor.supports_extension("xisf"));
        assert!(extractor.supports_extension("fits"));
        assert!(extractor.supports_extension("fit"));
        assert!(extractor.supports_extension("fts"));
    }

    #[test]
    fn multi_keyword_card_set() {
        let meta = parse(&[
            pad80("IMAGETYP= 'Flat Frame'"),
            pad80("FILTER  = 'OIII    '"),
            pad80("EXPTIME =                   3.0"),
            pad80("XBINNING=                     1"),
            pad80("YBINNING=                     1"),
        ]);
        assert_eq!(meta.image_typ, Some("Flat Frame".to_owned()));
        assert_eq!(meta.filter, Some("OIII".to_owned()));
        assert_eq!(meta.exposure, Some("3.0".to_owned()));
        assert_eq!(meta.x_binning, Some("1".to_owned()));
        assert_eq!(meta.y_binning, Some("1".to_owned()));
    }

    #[test]
    fn exposure_keyword_fallback_to_exposure() {
        // EXPTIME takes priority; if absent, EXPOSURE is used
        let meta = parse(&[pad80("EXPOSURE=                 120.0 / exposure in seconds")]);
        assert_eq!(meta.exposure, Some("120.0".to_owned()));
    }

    // ── Extended extracted metadata (spec 041 T062) ───────────────────────────

    fn approx(a: Option<f64>, b: f64) {
        let v = a.expect("expected Some");
        assert!((v - b).abs() < 1e-4, "expected {b}, got {v}");
    }

    #[test]
    fn parses_offset_with_blklevel_fallback() {
        // OFFSET preferred.
        let meta = parse(&[pad80("OFFSET  =                   20 / sensor gain offset")]);
        assert_eq!(meta.offset, Some(20));
        // BLKLEVEL fallback when OFFSET absent.
        let meta = parse(&[pad80("BLKLEVEL=                  512 / pedestal")]);
        assert_eq!(meta.offset, Some(512));
        // OFFSET wins over BLKLEVEL regardless of card order.
        let meta = parse(&[
            pad80("BLKLEVEL=                  512"),
            pad80("OFFSET  =                   20"),
        ]);
        assert_eq!(meta.offset, Some(20));
    }

    #[test]
    fn parses_set_and_ccd_temp_with_det_temp_fallback() {
        let meta = parse(&[
            pad80("SET-TEMP=                  0.0 / [degC] CCD temperature setpoint"),
            pad80("CCD-TEMP=                  0.2 / [degC] CCD temperature"),
        ]);
        approx(meta.set_temp_c, 0.0);
        approx(meta.ccd_temp_c, 0.2);
        // DWARF III: no CCD-TEMP, DET-TEMP fallback feeds ccd_temp_c.
        let meta = parse(&[pad80("DET-TEMP=                   44 / Detector temperature in C")]);
        approx(meta.ccd_temp_c, 44.0);
    }

    #[test]
    fn prefers_decimal_ra_dec() {
        // Real Poseidon header values; decimal RA/DEC win over sexagesimal.
        let meta = parse(&[
            pad80("RA      =     272.682006826377 / [deg] RA of telescope"),
            pad80("DEC     =    -15.0055460224596 / [deg] Declination of telescope"),
            pad80("OBJCTRA = '18 10 38'           / [H M S] RA of imaged object"),
            pad80("OBJCTDEC= '-15 01 11'          / [D M S] Dec of imaged object"),
        ]);
        approx(meta.ra_deg, 272.682_006_826);
        approx(meta.dec_deg, -15.005_546_022);
    }

    #[test]
    fn falls_back_to_sexagesimal_ra_dec() {
        // No decimal RA/DEC → convert OBJCTRA/OBJCTDEC.
        let meta = parse(&[
            pad80("OBJCTRA = '18 10 38'           / [H M S] RA of imaged object"),
            pad80("OBJCTDEC= '-15 01 11'          / [D M S] Dec of imaged object"),
        ]);
        approx(meta.ra_deg, 272.658_333_333);
        approx(meta.dec_deg, -15.019_722_222);
    }

    #[test]
    fn rotatang_is_match_key_objctrot_is_informational() {
        // ROTATANG (mechanical) ≠ OBJCTROT (sky PA): must not be swapped.
        let meta = parse(&[
            pad80("ROTATANG=     12.4320640563965 / [deg] Mechanical rotator angle"),
            pad80("OBJCTROT=                12.43 / [deg] planned rotation"),
            pad80("ROTNAME = 'Manual Rotator + OAG' / Rotator equipment name"),
        ]);
        approx(meta.rotator_angle_deg, 12.432_064);
        approx(meta.sky_rotation_deg, 12.43);
        assert_eq!(meta.rotator_name.as_deref(), Some("Manual Rotator + OAG"));
    }

    #[test]
    fn rotator_keyword_is_fallback_for_rotatang() {
        let meta = parse(&[pad80("ROTATOR =     90.5 / [deg] Mechanical rotator angle")]);
        approx(meta.rotator_angle_deg, 90.5);
        // And ROTATANG wins if both present.
        let meta = parse(&[pad80("ROTATOR =     90.5"), pad80("ROTATANG=     12.4")]);
        approx(meta.rotator_angle_deg, 12.4);
    }

    #[test]
    fn parses_readout_focallen_pixsize() {
        let meta = parse(&[
            pad80("READOUTM= 'Low Noise'          / Sensor readout mode"),
            pad80("FOCALLEN=                525.0 / [mm] Focal length"),
            pad80("XPIXSZ  =                 3.76 / [um] Pixel X axis size"),
        ]);
        assert_eq!(meta.readout_mode.as_deref(), Some("Low Noise"));
        approx(meta.focal_length_mm, 525.0);
        approx(meta.pixel_size_um, 3.76);
    }

    #[test]
    fn pixsize_fallback_when_xpixsz_absent() {
        let meta = parse(&[pad80("PIXSIZE =                 9.00 / pixel size")]);
        approx(meta.pixel_size_um, 9.0);
    }

    #[test]
    fn parses_observer_location_with_fallback_chain() {
        // SITE* preferred.
        let meta = parse(&[
            pad80("SITELAT =               24.839 / [deg] site latitude"),
            pad80("SITELONG=               55.383 / [deg] site longitude"),
            pad80("SITEELEV=                101.0 / [m] site elevation"),
        ]);
        approx(meta.observer_lat, 24.839);
        approx(meta.observer_long, 55.383);
        approx(meta.observer_elev, 101.0);
        // *-OBS fallback (real Jellyfish master uses LAT-OBS/LONG-OBS/ALT-OBS).
        let meta = parse(&[
            pad80("LAT-OBS =               24.839"),
            pad80("LONG-OBS=               55.383"),
            pad80("ALT-OBS =                101.0"),
        ]);
        approx(meta.observer_lat, 24.839);
        approx(meta.observer_long, 55.383);
        approx(meta.observer_elev, 101.0);
    }

    #[test]
    fn parses_time_keywords() {
        let meta = parse(&[
            pad80("DATE-LOC= '2025-10-17T19:23:39.413' / Time of observation (local)"),
            pad80("DATE-END= '2025-10-17T19:24:09.413' / end of observation"),
            pad80("MJD-AVG =       60965.0 / exposure midpoint"),
            pad80("MJD-OBS =       60965.5 / exposure start"),
        ]);
        assert_eq!(meta.date_loc.as_deref(), Some("2025-10-17T19:23:39.413"));
        assert_eq!(meta.date_end.as_deref(), Some("2025-10-17T19:24:09.413"));
        approx(meta.mjd_avg, 60965.0);
        approx(meta.mjd_obs, 60965.5);
    }

    #[test]
    fn extended_fields_absent_are_none() {
        let meta = parse(&[pad80("IMAGETYP= 'Light Frame'")]);
        assert!(meta.offset.is_none());
        assert!(meta.set_temp_c.is_none());
        assert!(meta.ccd_temp_c.is_none());
        assert!(meta.ra_deg.is_none());
        assert!(meta.dec_deg.is_none());
        assert!(meta.rotator_angle_deg.is_none());
        assert!(meta.sky_rotation_deg.is_none());
        assert!(meta.readout_mode.is_none());
        assert!(meta.focal_length_mm.is_none());
        assert!(meta.pixel_size_um.is_none());
        assert!(meta.observer_lat.is_none());
        assert!(meta.date_loc.is_none());
        assert!(meta.mjd_avg.is_none());
    }

    // ── WCS plate-solved pointing (spec 052 P3) ───────────────────────────────

    #[test]
    fn parses_wcs_pointing_with_cd_matrix() {
        let meta = parse(&[
            pad80("CTYPE1  = 'RA---TAN'"),
            pad80("CTYPE2  = 'DEC--TAN'"),
            pad80("CRVAL1  =            10.684708 / [deg] solved RA"),
            pad80("CRVAL2  =             41.26875 / [deg] solved Dec"),
            pad80("CD1_1   =        -0.0001935"),
            pad80("CD2_1   =         0.0000501"),
        ]);
        approx(meta.wcs_ra_deg, 10.684_708);
        approx(meta.wcs_dec_deg, 41.268_75);
        assert!(meta.wcs_rotation_deg.is_some());
    }

    #[test]
    fn wcs_pointing_absent_without_equatorial_ctype() {
        // A bare CRVAL1/2 pair with no matching CTYPE is not trusted as a solve.
        let meta = parse(&[
            pad80("CRVAL1  =                 10.0"),
            pad80("CRVAL2  =                 20.0"),
        ]);
        assert!(meta.wcs_ra_deg.is_none());
        assert!(meta.wcs_dec_deg.is_none());
    }

    #[test]
    fn wcs_rotation_falls_back_to_crota2() {
        let meta = parse(&[
            pad80("CTYPE1  = 'RA---TAN'"),
            pad80("CTYPE2  = 'DEC--TAN'"),
            pad80("CRVAL1  =                 83.822"),
            pad80("CRVAL2  =                 -5.391"),
            pad80("CROTA2  =                 12.5"),
        ]);
        assert_eq!(meta.wcs_rotation_deg, Some(12.5));
    }
}
