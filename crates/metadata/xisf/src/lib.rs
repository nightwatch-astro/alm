// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! XISF metadata extraction adapter (spec 005 T006), built on the
//! `xisf-header` crate.
//!
//! # XISF Header Format
//!
//! An XISF file (PixInsight's native format) begins with a 16-byte signature
//! followed by a 4-byte little-endian XML header length, 4-byte reserved, and
//! then the XML header itself.
//!
//! The XML `<xisf>` element contains an `<Image>` element with optional
//! `<FITSKeyword>` children. Each `<FITSKeyword>` carries `name`, `value`, and
//! `comment` attributes — exactly the FITS header keyword model.
//!
//! This extractor reads only the XML header (no pixel data) and extracts the
//! same keywords as the FITS extractor via the `<FITSKeyword>` elements.
//!
//! No heavy C dependencies — `xisf-header` is pure Rust.
#![allow(clippy::doc_markdown)]

use std::path::Path;

use metadata_core::{
    interpret_wcs_pointing, sexagesimal_dec_to_deg, sexagesimal_ra_to_deg, MetadataExtractError,
    MetadataExtractor, RawFileMetadata,
};
use xisf_header::{Error as XisfError, FromField, Header};

// ── XisfExtractor ─────────────────────────────────────────────────────────────

/// Adapter that reads FITS-compatible metadata from XISF files via their XML
/// header.
///
/// Supports `.xisf` extension.
pub struct XisfExtractor;

impl MetadataExtractor for XisfExtractor {
    fn extract(&self, path: &Path) -> Result<Option<RawFileMetadata>, MetadataExtractError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        if !self.supports_extension(&ext) {
            return Ok(None);
        }

        // Reads only the 16-byte preamble + declared XML header (capped at
        // 8 MiB internally); never touches pixel/attachment data.
        //
        // `xisf-header` 0.4.3 length-checks before every slice, so no panic is
        // known here; the guard keeps a future parser panic from reaching the
        // inbox scan, matching the FITS adapter. It depends on panics
        // unwinding: the release profile in the workspace `Cargo.toml`
        // deliberately keeps the default `panic = "unwind"`.
        let header =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Header::read_from_file(path)))
                .map_err(|_| MetadataExtractError::Parse {
                    path: path.display().to_string(),
                    msg: "the XISF header parser panicked on malformed header bytes".to_owned(),
                })?
                .map_err(|e| map_err(&e, path))?;

        Ok(Some(parse_header(&header)))
    }

    fn supports_extension(&self, ext: &str) -> bool {
        ext == "xisf"
    }
}

/// Map an `xisf_header::Error` onto this crate's error contract: signature,
/// size, encoding, and I/O failures are `Io`; XML/attribute syntax errors are
/// `Parse`.
fn map_err(e: &XisfError, path: &Path) -> MetadataExtractError {
    let msg = e.to_string();
    let path = path.display().to_string();
    match e {
        XisfError::Xml(_) | XisfError::Attr(_) => MetadataExtractError::Parse { path, msg },
        _ => MetadataExtractError::Io { path, msg },
    }
}

// ── Header field extraction ──────────────────────────────────────────────────

/// Typed keyword read that never hard-errors: a duplicated keyword
/// (`XisfError::Ambiguous`) falls back to its first occurrence instead of
/// surfacing an error, preserving this extractor's "always `None`, never
/// `Err`" field-level contract.
fn get<T: FromField>(header: &Header, key: &str) -> Option<T> {
    match header.get::<T>(key) {
        Ok(v) => v,
        Err(XisfError::Ambiguous { .. }) => header.get_all::<T>(key).into_iter().next(),
        Err(_) => None,
    }
}

/// Like [`get`] but for raw keyword text (`Header::get_str`).
fn get_str(header: &Header, key: &str) -> Option<String> {
    match header.get_str(key) {
        Ok(v) => v.map(str::to_owned),
        Err(XisfError::Ambiguous { .. }) => header.get_all::<String>(key).into_iter().next(),
        Err(_) => None,
    }
}

/// Trim and treat an empty value as absent. `xisf_header`'s `FromField<String>`
/// never returns `None` for an empty-but-present keyword, unlike the numeric
/// `FromField` impls (which already reject empty text); string-typed fields
/// need this explicit check to match the FITS extractor's contract.
fn non_empty(s: Option<String>) -> Option<String> {
    s.and_then(|s| {
        let t = s.trim();
        (!t.is_empty()).then(|| t.to_owned())
    })
}

/// Map header keywords (and, as a fallback, XISF `<Property>` elements) into
/// [`RawFileMetadata`].
///
/// The observer-location fallback chains and RA/DEC decimal-over-sexagesimal
/// precedence are shared value shapes but are intentionally kept as separate
/// keyword lookups (fallback precedence); the field list is necessarily long.
// A struct-literal initializer would be far less readable than these named
// per-field assignments given the multi-keyword fallback chains.
#[allow(clippy::field_reassign_with_default)]
fn parse_header(header: &Header) -> RawFileMetadata {
    let mut meta = RawFileMetadata::default();

    meta.image_typ = non_empty(get_str(header, "IMAGETYP"));
    meta.filter = non_empty(get_str(header, "FILTER"));
    meta.object = non_empty(get_str(header, "OBJECT"));
    // EXPTIME takes priority; EXPOSURE is the fallback.
    meta.exposure =
        non_empty(get_str(header, "EXPTIME")).or_else(|| non_empty(get_str(header, "EXPOSURE")));
    meta.gain = non_empty(get_str(header, "GAIN"));
    meta.x_binning = non_empty(get_str(header, "XBINNING"));
    meta.y_binning = non_empty(get_str(header, "YBINNING"));
    meta.naxis1 = non_empty(get_str(header, "NAXIS1"));
    meta.naxis2 = non_empty(get_str(header, "NAXIS2"));
    meta.instrume = non_empty(get_str(header, "INSTRUME"));
    meta.telescop = non_empty(get_str(header, "TELESCOP"));
    meta.date_obs = non_empty(get_str(header, "DATE-OBS"));

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
    meta.rotator_name = non_empty(get_str(header, "ROTNAME"));
    meta.sky_rotation_deg = get(header, "OBJCTROT");
    // Readout mode (optional grouping dim).
    meta.readout_mode = non_empty(get_str(header, "READOUTM"));
    // Optic train inputs. FOCALLEN (FITSKeyword, mm) takes precedence over the
    // XISF native Property (metres → mm). XPIXSZ/PIXSIZE (FITSKeyword, µm)
    // take precedence over the Property (already µm).
    meta.focal_length_mm = get(header, "FOCALLEN").or_else(|| {
        header.property_get::<f64>("Instrument:Telescope:FocalLength").map(|m| m * 1000.0)
    });
    meta.pixel_size_um = get::<f64>(header, "XPIXSZ")
        .or_else(|| get::<f64>(header, "PIXSIZE"))
        .or_else(|| header.property_get::<f64>("Image:PixelSize"));
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
    meta.date_loc = non_empty(get_str(header, "DATE-LOC"));
    meta.date_end = non_empty(get_str(header, "DATE-END"));
    meta.mjd_avg = get(header, "MJD-AVG");
    meta.mjd_obs = get(header, "MJD-OBS");

    // Plate-solved WCS pointing (spec 052 P3, FR-012): passthrough keyword
    // reads only — interpretation lives once in
    // `metadata_core::interpret_wcs_pointing`. XISF carries the same
    // FITS-compatible WCS keywords via `<FITSKeyword>`.
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

// ── Test helpers ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// XISF file signature bytes (8-byte `XISF0100` marker), for building test
    /// containers only — `xisf_header::Header` validates this internally.
    const XISF_SIGNATURE: &[u8; 8] = b"XISF0100";

    /// Build a minimal XISF byte stream with the given FITSKeyword entries.
    ///
    /// `keywords` is a list of `(name, value, comment)` triples.
    fn build_xisf(keywords: &[(&str, &str, &str)]) -> Vec<u8> {
        let mut kw_xml = String::new();
        for (name, value, comment) in keywords {
            kw_xml.push_str("   <FITSKeyword name=\"");
            kw_xml.push_str(name);
            kw_xml.push_str("\" value=\"");
            kw_xml.push_str(value);
            kw_xml.push_str("\" comment=\"");
            kw_xml.push_str(comment);
            kw_xml.push_str("\" />\n");
        }

        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xisf version="1.0" xmlns="http://www.pixinsight.com/xisf">
 <Image geometry="4144:2822:1" sampleFormat="Float32" >
{kw_xml}
 </Image>
</xisf>"#
        );

        let xml_bytes = xml.as_bytes();
        let xml_len = u32::try_from(xml_bytes.len()).expect("test XML too large");

        let mut out = Vec::new();
        out.extend_from_slice(XISF_SIGNATURE);
        out.extend_from_slice(&xml_len.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]); // reserved
        out.extend_from_slice(xml_bytes);
        out
    }

    /// Build an XISF byte stream with explicit `<Property>` and `<FITSKeyword>`
    /// blocks. `properties` is `(id, type, value)`; `keywords` is
    /// `(name, value, comment)`.
    fn build_xisf_full(
        properties: &[(&str, &str, &str)],
        keywords: &[(&str, &str, &str)],
    ) -> Vec<u8> {
        use std::fmt::Write as _;
        let mut body = String::new();
        for (id, ty, value) in properties {
            let _ = writeln!(body, "  <Property id=\"{id}\" type=\"{ty}\" value=\"{value}\"/>");
        }
        for (name, value, comment) in keywords {
            let _ = writeln!(
                body,
                "  <FITSKeyword name=\"{name}\" value=\"{value}\" comment=\"{comment}\" />"
            );
        }
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xisf version="1.0" xmlns="http://www.pixinsight.com/xisf">
 <Image geometry="4144:2822:1" sampleFormat="Float32" >
{body} </Image>
</xisf>"#
        );
        let xml_bytes = xml.as_bytes();
        let xml_len = u32::try_from(xml_bytes.len()).expect("test XML too large");
        let mut out = Vec::new();
        out.extend_from_slice(XISF_SIGNATURE);
        out.extend_from_slice(&xml_len.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(xml_bytes);
        out
    }

    /// Parse a built XISF byte stream via the real `Header::parse` +
    /// [`parse_header`] path.
    fn parse(data: &[u8]) -> RawFileMetadata {
        let header = Header::parse(data).unwrap();
        parse_header(&header)
    }

    #[test]
    fn parses_imagetyp_light() {
        let data = build_xisf(&[("IMAGETYP", "'Light Frame'", "frame type")]);
        let meta = parse(&data);
        assert_eq!(meta.image_typ, Some("Light Frame".to_owned()));
    }

    #[test]
    fn parses_imagetyp_dark() {
        let data = build_xisf(&[("IMAGETYP", "'Dark Frame'", "")]);
        let meta = parse(&data);
        assert_eq!(meta.image_typ, Some("Dark Frame".to_owned()));
    }

    #[test]
    fn parses_filter_and_object() {
        let data = build_xisf(&[
            ("FILTER", "'Ha      '", "filter name"),
            ("OBJECT", "'NGC 7000'", "target"),
        ]);
        let meta = parse(&data);
        assert_eq!(meta.filter, Some("Ha".to_owned()));
        assert_eq!(meta.object, Some("NGC 7000".to_owned()));
    }

    #[test]
    fn parses_exptime() {
        let data = build_xisf(&[("EXPTIME", "300.0", "exposure in seconds")]);
        let meta = parse(&data);
        assert_eq!(meta.exposure, Some("300.0".to_owned()));
    }

    #[test]
    fn parses_gain_and_binning() {
        let data = build_xisf(&[
            ("GAIN", "100", "camera gain"),
            ("XBINNING", "1", ""),
            ("YBINNING", "1", ""),
        ]);
        let meta = parse(&data);
        assert_eq!(meta.gain, Some("100".to_owned()));
        assert_eq!(meta.x_binning, Some("1".to_owned()));
        assert_eq!(meta.y_binning, Some("1".to_owned()));
    }

    #[test]
    fn parses_instrume_telescop() {
        let data = build_xisf(&[
            ("INSTRUME", "'ZWO ASI2600MM Pro'", "camera"),
            ("TELESCOP", "'AT130-EDT'", "scope"),
        ]);
        let meta = parse(&data);
        assert_eq!(meta.instrume, Some("ZWO ASI2600MM Pro".to_owned()));
        assert_eq!(meta.telescop, Some("AT130-EDT".to_owned()));
    }

    #[test]
    fn parses_date_obs() {
        let data = build_xisf(&[("DATE-OBS", "'2025-10-10T22:15:00'", "")]);
        let meta = parse(&data);
        assert_eq!(meta.date_obs, Some("2025-10-10T22:15:00".to_owned()));
    }

    #[test]
    fn missing_keywords_return_none() {
        let data = build_xisf(&[]);
        let meta = parse(&data);
        assert!(meta.image_typ.is_none());
        assert!(meta.filter.is_none());
        assert!(meta.object.is_none());
    }

    #[test]
    fn bad_signature_returns_error() {
        // Exercises the local extract() -> map_err() path (not
        // Header::parse() in isolation) so a regression in this crate's
        // error mapping — e.g. `map_err` dropped or misrouting the
        // signature-failure variant — is actually caught.
        let mut data = build_xisf(&[]);
        data[0..8].copy_from_slice(b"NOTXISF!");

        let path = std::env::temp_dir()
            .join(format!("xisf_bad_signature_test_{}.xisf", std::process::id()));
        std::fs::write(&path, &data).expect("write test fixture");

        let result = XisfExtractor.extract(&path);
        let _ = std::fs::remove_file(&path);

        match result {
            Err(MetadataExtractError::Io { .. }) => {}
            other => panic!("expected Io error for bad signature, got {other:?}"),
        }
    }

    #[test]
    fn extractor_rejects_fits_extension() {
        let extractor = XisfExtractor;
        assert!(!extractor.supports_extension("fits"));
        assert!(extractor.supports_extension("xisf"));
    }

    // ── Extended extracted metadata (spec 041 T062) ───────────────────────────

    fn approx(a: Option<f64>, b: f64) {
        let v = a.expect("expected Some");
        assert!((v - b).abs() < 1e-4, "expected {b}, got {v}");
    }

    #[test]
    fn parses_extended_fits_keywords() {
        let data = build_xisf(&[
            ("OFFSET", "20", ""),
            ("SET-TEMP", "0.0", ""),
            ("CCD-TEMP", "0.2", ""),
            ("READOUTM", "'Low Noise'", ""),
            ("ROTATANG", "12.4320640563965", ""),
            ("OBJCTROT", "12.43", ""),
            ("ROTNAME", "'Manual Rotator + OAG'", ""),
            ("FOCALLEN", "525.0", ""),
            ("XPIXSZ", "3.76", ""),
        ]);
        let meta = parse(&data);
        assert_eq!(meta.offset, Some(20));
        approx(meta.set_temp_c, 0.0);
        approx(meta.ccd_temp_c, 0.2);
        assert_eq!(meta.readout_mode.as_deref(), Some("Low Noise"));
        approx(meta.rotator_angle_deg, 12.432_064);
        approx(meta.sky_rotation_deg, 12.43);
        assert_eq!(meta.rotator_name.as_deref(), Some("Manual Rotator + OAG"));
        approx(meta.focal_length_mm, 525.0);
        approx(meta.pixel_size_um, 3.76);
    }

    #[test]
    fn prefers_decimal_ra_dec_over_sexagesimal() {
        // Real NINA XISF: both decimal RA/DEC and quoted OBJCTRA/OBJCTDEC present.
        let data = build_xisf(&[
            ("RA", "83.74159794045094", ""),
            ("DEC", "-5.34226120604197", ""),
            ("OBJCTRA", "'5 34 57.984'", ""),
            ("OBJCTDEC", "'-5 20 32.14'", ""),
        ]);
        let meta = parse(&data);
        approx(meta.ra_deg, 83.741_598);
        approx(meta.dec_deg, -5.342_261);
    }

    #[test]
    fn falls_back_to_sexagesimal_ra_dec() {
        let data =
            build_xisf(&[("OBJCTRA", "'5 34 57.984'", ""), ("OBJCTDEC", "'-5 20 32.14'", "")]);
        let meta = parse(&data);
        approx(meta.ra_deg, 83.7416);
        approx(meta.dec_deg, -5.342_261);
    }

    #[test]
    fn focal_length_property_converts_metres_to_mm() {
        // No FOCALLEN FITSKeyword → Property fallback (metres → mm).
        let data =
            build_xisf_full(&[("Instrument:Telescope:FocalLength", "Float64", "0.835784")], &[]);
        let meta = parse(&data);
        approx(meta.focal_length_mm, 835.784);
    }

    #[test]
    fn fits_keyword_focallen_wins_over_property() {
        // Both present: FITSKeyword (mm) takes precedence over Property (metres).
        let data = build_xisf_full(
            &[("Instrument:Telescope:FocalLength", "Float64", "0.835784")],
            &[("FOCALLEN", "835.78444", "mm")],
        );
        let meta = parse(&data);
        approx(meta.focal_length_mm, 835.784_44);
    }

    #[test]
    fn pixel_size_property_image_pixelsize() {
        let data = build_xisf_full(&[("Image:PixelSize", "Float64", "3.76")], &[]);
        let meta = parse(&data);
        approx(meta.pixel_size_um, 3.76);
    }

    #[test]
    fn observer_location_and_time_keywords() {
        let data = build_xisf(&[
            ("SITELAT", "24.839", ""),
            ("SITELONG", "55.383", ""),
            ("SITEELEV", "101.0", ""),
            ("DATE-LOC", "'2025-10-17T19:23:39.413'", ""),
            ("DATE-END", "'2025-10-17T19:24:09.413'", ""),
            ("MJD-AVG", "60965.0", ""),
            ("MJD-OBS", "60965.5", ""),
        ]);
        let meta = parse(&data);
        approx(meta.observer_lat, 24.839);
        approx(meta.observer_long, 55.383);
        approx(meta.observer_elev, 101.0);
        assert_eq!(meta.date_loc.as_deref(), Some("2025-10-17T19:23:39.413"));
        assert_eq!(meta.date_end.as_deref(), Some("2025-10-17T19:24:09.413"));
        approx(meta.mjd_avg, 60965.0);
        approx(meta.mjd_obs, 60965.5);
    }

    #[test]
    fn extended_fields_absent_are_none() {
        let data = build_xisf(&[("IMAGETYP", "'Light Frame'", "")]);
        let meta = parse(&data);
        assert!(meta.offset.is_none());
        assert!(meta.ra_deg.is_none());
        assert!(meta.rotator_angle_deg.is_none());
        assert!(meta.focal_length_mm.is_none());
        assert!(meta.pixel_size_um.is_none());
        assert!(meta.observer_lat.is_none());
        assert!(meta.mjd_avg.is_none());
    }

    // ── WCS plate-solved pointing (spec 052 P3) ───────────────────────────────

    #[test]
    fn parses_wcs_pointing_with_cd_matrix() {
        let data = build_xisf(&[
            ("CTYPE1", "'RA---TAN'", ""),
            ("CTYPE2", "'DEC--TAN'", ""),
            ("CRVAL1", "10.684708", ""),
            ("CRVAL2", "41.26875", ""),
            ("CD1_1", "-0.0001935", ""),
            ("CD2_1", "0.0000501", ""),
        ]);
        let meta = parse(&data);
        approx(meta.wcs_ra_deg, 10.684_708);
        approx(meta.wcs_dec_deg, 41.268_75);
        assert!(meta.wcs_rotation_deg.is_some());
    }

    #[test]
    fn wcs_pointing_absent_without_equatorial_ctype() {
        let data = build_xisf(&[("CRVAL1", "10.0", ""), ("CRVAL2", "20.0", "")]);
        let meta = parse(&data);
        assert!(meta.wcs_ra_deg.is_none());
    }
}
