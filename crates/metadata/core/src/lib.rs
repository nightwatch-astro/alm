// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared metadata model used by format-specific extractors (spec 005).
//!
//! # Contents
//!
//! - [`FrameType`] — canonical classification enum.
//! - [`EvidenceSource`] — how the frame type was determined.
//! - [`ImageTypNormalizationTable`] — maps raw IMAGETYP strings to
//!   [`FrameType`] via case-insensitive lookup.
//! - [`MetadataExtractor`] — trait implemented by FITS/XISF adapters.
//! - [`RawFileMetadata`] — minimal header values returned by extractors.
//! - [`CaptureProfileRegistry`] — versioned normalization of raw FITS/XISF fields.
#![allow(clippy::doc_markdown)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

mod evidence;
mod profile;

pub use evidence::{
    CalculatedFocalLength, CanonicalField, CaptureProfileVersion, EvidenceConfidence,
    EvidenceError, EvidenceState, FieldEvidence, MetadataEvidence, MetadataValue, RawMetadata,
    MAX_EVIDENCE_PAYLOAD_BYTES, MAX_EVIDENCE_VALUE_BYTES,
};
pub use profile::{CaptureProfileError, CaptureProfileRegistry, MAX_CAPTURE_PROFILE_TOML_BYTES};

// ── FrameType ─────────────────────────────────────────────────────────────────

/// Canonical astronomical frame types used throughout the app.
///
/// `DarkFlat` is reserved in v1; the normalization table does NOT map any
/// IMAGETYP value to it — files land as `unclassified` and can be manually
/// promoted via `inbox.reclassify`. (Ref: R-DarkFlat-Reserved)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameType {
    Light,
    Dark,
    Bias,
    Flat,
    /// Reserved — not produced by the v1 normalization table.
    DarkFlat,
}

impl FrameType {
    /// Return the canonical lowercase string used in contracts.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Bias => "bias",
            Self::Flat => "flat",
            Self::DarkFlat => "dark_flat",
        }
    }

    /// Parse from a contract/DB string (case-insensitive).
    #[must_use]
    pub fn from_str_ci(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "bias" => Some(Self::Bias),
            "flat" => Some(Self::Flat),
            "dark_flat" => Some(Self::DarkFlat),
            _ => None,
        }
    }
}

impl std::fmt::Display for FrameType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── EvidenceSource ────────────────────────────────────────────────────────────

/// How a [`FrameType`] assignment was determined for a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// Derived from the FITS `IMAGETYP` header keyword.
    ImagetypHeader,
    /// Derived from an XISF `FITSKeyword` property.
    XisfProperty,
    /// Manually set by the user via `inbox.reclassify`.
    ManualOverride,
    /// No determinable source (IMAGETYP absent or unreadable).
    None,
}

impl EvidenceSource {
    /// Return the canonical lowercase string used in the DB.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImagetypHeader => "imagetyp_header",

            Self::XisfProperty => "xisf_property",
            Self::ManualOverride => "manual_override",
            Self::None => "none",
        }
    }

    /// Parse from DB/contract string.
    #[must_use]
    pub fn from_str_ci(s: &str) -> Option<Self> {
        match s {
            "imagetyp_header" => Some(Self::ImagetypHeader),
            "xisf_property" => Some(Self::XisfProperty),
            "manual_override" => Some(Self::ManualOverride),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

// ── ImageTypNormalizationTable ────────────────────────────────────────────────

/// Case-insensitive mapping from raw IMAGETYP string values → [`FrameType`].
///
/// The canonical mapping is loaded from the embedded TOML data file
/// `data/imagetyp_normalization.toml` (R-IMAGETYP-Norm). Unknown values
/// return `None` (file is marked unclassified).
///
/// `DarkFlat` is intentionally absent from the v1 mapping (R-DarkFlat-Reserved).
#[derive(Clone, Debug)]
pub struct ImageTypNormalizationTable {
    /// Lowercase-normalised key → FrameType.
    entries: HashMap<String, FrameType>,
}

impl ImageTypNormalizationTable {
    /// Build from an iterator of `(raw_value, FrameType)` pairs.
    pub fn from_entries(iter: impl IntoIterator<Item = (String, FrameType)>) -> Self {
        let entries = iter.into_iter().map(|(k, v)| (k.trim().to_ascii_lowercase(), v)).collect();
        Self { entries }
    }

    /// Normalise a raw `IMAGETYP` string to a [`FrameType`].
    ///
    /// Comparison is case-insensitive and trims surrounding whitespace.
    /// Returns `None` for unknown or empty values.
    #[must_use]
    pub fn normalize(&self, raw: &str) -> Option<FrameType> {
        let key = raw.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        self.entries.get(&key).copied()
    }

    /// Return the number of entries in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the table has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Build the V1 normalization table from the canonical embedded mapping.
///
/// Source: `docs/research/imagetyp-normalization.md` (T0-IMAGETYP-Research).
#[must_use]
pub fn v1_normalization_table() -> ImageTypNormalizationTable {
    // Comprehensive mapping compiled from NINA, SGP, APT, Voyager, Ekos/KStars,
    // MaximDL, ACP, ASIAIR, SharpCap, ZWO ASI, FireCapture documentation and
    // user-reported real FITS files. (Ref: R-IMAGETYP-Norm, T0-IMAGETYP-Research)
    let entries: &[(&str, FrameType)] = &[
        // ── LIGHT / SCIENCE frames ────────────────────────────────────────────
        ("light", FrameType::Light),
        ("light frame", FrameType::Light),
        ("light frames", FrameType::Light),
        ("science", FrameType::Light),
        ("science frame", FrameType::Light),
        ("science frames", FrameType::Light),
        ("object", FrameType::Light), // ACP, MaximDL
        // ── DARK frames ───────────────────────────────────────────────────────
        ("dark", FrameType::Dark),
        ("dark frame", FrameType::Dark),
        ("dark frames", FrameType::Dark),
        // ── BIAS frames ───────────────────────────────────────────────────────
        ("bias", FrameType::Bias),
        ("bias frame", FrameType::Bias),
        ("bias frames", FrameType::Bias),
        ("offset", FrameType::Bias), // SGP, some European capture software
        ("offset frame", FrameType::Bias),
        ("zero", FrameType::Bias), // MaximDL
        // ── FLAT frames ───────────────────────────────────────────────────────
        ("flat", FrameType::Flat),
        ("flat frame", FrameType::Flat),
        ("flat frames", FrameType::Flat),
        ("sky flat", FrameType::Flat),
        ("dawn flat", FrameType::Flat),
        ("dusk flat", FrameType::Flat),
        ("twilight flat", FrameType::Flat),
        // NOTE: dark_flat is intentionally absent (R-DarkFlat-Reserved).
        // Files with "darkflat", "dark flat", "flat dark" etc. are unclassified
        // until manually reclassified via inbox.reclassify.
    ];

    ImageTypNormalizationTable::from_entries(entries.iter().map(|(k, v)| ((*k).to_owned(), *v)))
}

// ── RawFileMetadata ───────────────────────────────────────────────────────────

/// Minimal extracted header values returned by format-specific extractors.
///
/// Fields are `Option<String>` because any header keyword may be absent or
/// unreadable. Normalization and classification happen in `crates/domain/core`
/// or `crates/app/core`.
#[derive(Clone, Debug, Default)]
pub struct RawFileMetadata {
    /// Raw `IMAGETYP` string (before normalization).
    pub image_typ: Option<String>,
    /// `FILTER` keyword value.
    pub filter: Option<String>,
    /// `OBJECT` keyword value.
    pub object: Option<String>,
    /// `EXPTIME` or `EXPOSURE` (seconds as a string).
    pub exposure: Option<String>,
    /// `GAIN` keyword value.
    pub gain: Option<String>,
    /// `XBINNING` value.
    pub x_binning: Option<String>,
    /// `YBINNING` value.
    pub y_binning: Option<String>,
    /// `NAXIS1` (image width in pixels).
    pub naxis1: Option<String>,
    /// `NAXIS2` (image height in pixels).
    pub naxis2: Option<String>,
    /// `INSTRUME` (camera/instrument identifier).
    pub instrume: Option<String>,
    /// `CAMERAID` (`vendor_model_serial` camera body identifier). Distinguishes
    /// two bodies of one model only for vendors that populate the serial
    /// segment; see `sessions::camera_body_id` for the derivation.
    pub cameraid: Option<String>,
    /// `TELESCOP` (telescope identifier).
    pub telescop: Option<String>,
    /// `DATE-OBS` (ISO 8601 string or FITS-style `YYYY-MM-DDTHH:MM:SS`).
    pub date_obs: Option<String>,
    /// Integration count from `STACKCNT` (preferred) or `NCOMBINE`.
    ///
    /// Present only in stacked/master files that carry this keyword.
    /// Used by the master-detect crate to identify stacked calibration frames.
    pub stack_count: Option<u32>,

    // ── Extended extracted metadata (spec 041 T062, R-9/R-18) ───────────────
    // All fields are best-effort: absent header ⇒ `None`, never an error.
    /// Camera read-out offset / pedestal from `OFFSET` (fallback `BLKLEVEL`).
    /// Grouping dimension for all four frame types. ADU.
    pub offset: Option<i64>,
    /// Sensor set/target temperature from `SET-TEMP`. Default dark-grouping
    /// temperature source (R-18 temperature policy). Degrees Celsius.
    pub set_temp_c: Option<f64>,
    /// Sensor actual temperature from `CCD-TEMP` (fallback `DET-TEMP`,
    /// DWARF III). Deviation-warning source. Degrees Celsius.
    pub ccd_temp_c: Option<f64>,
    /// Right ascension in decimal degrees.
    ///
    /// Decimal `RA` is preferred; sexagesimal `OBJCTRA` (`H M S`) is converted
    /// to decimal degrees (×15) as a fallback. Light pointing + R-17 target
    /// resolution.
    pub ra_deg: Option<f64>,
    /// Declination in decimal degrees.
    ///
    /// Decimal `DEC` is preferred; sexagesimal `OBJCTDEC` (`±D M S`) is
    /// converted to decimal degrees as a fallback. Light pointing + R-17.
    pub dec_deg: Option<f64>,
    /// Mechanical rotator angle in degrees from `ROTATANG` (= `ROTATOR`).
    ///
    /// This is the flat↔light match key and tolerant light grouping
    /// dimension (R-18). NOT the sky position angle.
    pub rotator_angle_deg: Option<f64>,
    /// Rotator device identifier from `ROTNAME`. Informational only.
    pub rotator_name: Option<String>,
    /// Sky position angle in degrees from `OBJCTROT`.
    ///
    /// Informational only — explicitly NOT a flat-match key (R-18). Kept
    /// separate from [`Self::rotator_angle_deg`] so the two are never swapped.
    pub sky_rotation_deg: Option<f64>,
    /// Sensor readout mode from `READOUTM`. Optional grouping dim, default OFF.
    pub readout_mode: Option<String>,
    /// Focal length in millimetres from `FOCALLEN`
    /// (XISF `Instrument:Telescope:FocalLength` is in metres → ×1000).
    /// Optic-train composite input (light + flat).
    pub focal_length_mm: Option<f64>,
    /// Pixel size in micrometres from `XPIXSZ` (fallback `PIXSIZE`;
    /// XISF `Image:PixelSize`). Feeds the FOV-aware target radius (R-17).
    pub pixel_size_um: Option<f64>,
    /// Observer latitude in degrees from `SITELAT` → `OBSGEO-B` → `LAT-OBS`.
    /// Future grouping only.
    pub observer_lat: Option<f64>,
    /// Observer longitude in degrees from `SITELONG` → `OBSGEO-L` →
    /// `LONG-OBS`. Prerequisite for UTC-fallback night binning.
    pub observer_long: Option<f64>,
    /// Observer elevation in metres from `SITEELEV` → `OBSGEO-H` → `ALT-OBS`.
    /// Future grouping only.
    pub observer_elev: Option<f64>,
    /// Local civil time of observation from `DATE-LOC`. Observing-night =
    /// local calendar date under a noon boundary (R-18).
    pub date_loc: Option<String>,
    /// Observation end time from `DATE-END`. Dark-run span heuristic.
    pub date_end: Option<String>,
    /// Modified Julian Date of exposure midpoint from `MJD-AVG`
    /// (NINA 3.2+). Ordering / dark-run span / UTC math (preferred).
    pub mjd_avg: Option<f64>,
    /// Modified Julian Date of exposure start from `MJD-OBS`.
    /// Ordering / dark-run span fallback.
    pub mjd_obs: Option<f64>,

    // ── Plate-solved WCS pointing (spec 052 P3, FR-012) ─────────────────────
    // Populated only when `CTYPE1`/`CTYPE2` are genuine equatorial WCS
    // projections (see [`interpret_wcs_pointing`]) — a bare `CRVAL1/2` pair
    // with no matching `CTYPE` is not trusted as a solve. Distinct from
    // [`Self::ra_deg`]/[`Self::dec_deg`] (mount `RA`/`DEC`/`OBJCTRA`/
    // `OBJCTDEC`), never conflated: WCS is the high-confidence source,
    // mount is medium (FR-012 precedence).
    /// Plate-solved right ascension from `CRVAL1`, decimal degrees.
    pub wcs_ra_deg: Option<f64>,
    /// Plate-solved declination from `CRVAL2`, decimal degrees.
    pub wcs_dec_deg: Option<f64>,
    /// Plate-solved sky position angle (east of north), decimal degrees, from
    /// the WCS CD matrix (preferred) or `CROTA2` fallback. `None` when no
    /// rotation term is present even though the pointing itself solved.
    pub wcs_rotation_deg: Option<f64>,
}

// ── Coordinate / value parsing helpers ──────────────────────────────────────

/// Parse a sexagesimal right-ascension string in `H M S` form (e.g.
/// `"18 10 38"` or `"5 34 57.984"`) into decimal **degrees**.
///
/// RA is expressed in hours; the result is multiplied by 15 (360° / 24h).
/// Separators may be spaces or colons. Returns `None` for unparseable input,
/// or when the parsed value is outside the RA domain (`[0, 360)`).
///
/// Delegates the sexagesimal→decimal conversion to
/// `skymath::Equatorial::parse_at_epoch` in [`skymath::ParseMode::Lenient`]
/// (a paired call with a `"0"` Dec sentinel, since that constructor validates
/// RA and Dec together; Lenient tolerates the missing-minutes/seconds and
/// bare-decimal forms this parser has always accepted). FITS/XISF header
/// values are frequently single-quoted; that quoting is stripped here before
/// parsing, since `skymath` treats quote characters as part of the token (a
/// FITS-specific tolerance, not a general coordinate concern).
#[must_use]
pub fn sexagesimal_ra_to_deg(raw: &str) -> Option<f64> {
    let cleaned = strip_fits_quotes(raw)?;
    skymath::Equatorial::parse_at_epoch(
        cleaned,
        "0",
        skymath::Epoch::J2000,
        skymath::ParseMode::Lenient,
    )
    .ok()
    .map(|e| e.ra().degrees())
}

/// Parse a sexagesimal declination string in `±D M S` form (e.g.
/// `"-15 01 11"` or `"+0 00 00.00"`) into decimal **degrees**.
///
/// A leading `-` on the degrees field applies to the whole value.
/// Separators may be spaces or colons. Returns `None` for unparseable input,
/// or when the parsed value is outside the Dec domain (`[-90, 90]`).
///
/// See [`sexagesimal_ra_to_deg`] for the shared parsing/quoting approach.
#[must_use]
pub fn sexagesimal_dec_to_deg(raw: &str) -> Option<f64> {
    let cleaned = strip_fits_quotes(raw)?;
    skymath::Equatorial::parse_at_epoch(
        "0",
        cleaned,
        skymath::Epoch::J2000,
        skymath::ParseMode::Lenient,
    )
    .ok()
    .map(|e| e.dec().degrees())
}

/// Strip surrounding whitespace and a single layer of FITS single-quoting
/// (`'...'`) from a header value string. Returns `None` for empty input.
fn strip_fits_quotes(raw: &str) -> Option<&str> {
    let trimmed = raw.trim().trim_matches('\'').trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

// ── WCS pointing interpretation (spec 052 P3) ────────────────────────────────

/// A plate-solved WCS pointing derived from standard FITS WCS keywords.
///
/// Shared by every format adapter (`crates/metadata/fits`,
/// `crates/metadata/xisf`) via [`interpret_wcs_pointing`] so the WCS
/// interpretation itself lives in exactly one place — the adapters only read
/// the raw passthrough keywords (`CRVAL1/2`, `CTYPE1/2`, the `CD*` matrix,
/// `CROTA2`) via their own header API and hand them here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WcsPointing {
    /// Plate-solved right ascension, decimal degrees.
    pub ra_deg: f64,
    /// Plate-solved declination, decimal degrees.
    pub dec_deg: f64,
    /// Sky position angle (east of north), decimal degrees, when a rotation
    /// term is present.
    pub rotation_deg: Option<f64>,
}

/// Interpret raw WCS header keywords into a [`WcsPointing`], or `None` when
/// the header does not carry a real equatorial WCS solution.
///
/// Gated on `CTYPE1`/`CTYPE2` being genuine RA/Dec projections (e.g.
/// `RA---TAN`/`DEC--TAN`) — a bare `CRVAL1/2` pair with no matching `CTYPE`
/// is not trusted as a solve (could be a leftover/garbage keyword from an
/// unrelated tool). Rotation prefers the CD matrix
/// (`atan2(CD2_1, CD1_1)`, the position angle of the WCS Y axis under a
/// simple rotation+scale — this does not attempt to fully decompose a
/// skewed/flipped CD matrix, which is adequate at the planning-grade
/// precision this app needs) over `CROTA2` when both are present; `CROTA2`
/// alone is used verbatim when there is no CD matrix.
#[must_use]
#[allow(clippy::too_many_arguments)] // one keyword per WCS term; a struct would just move the list
pub fn interpret_wcs_pointing(
    ctype1: Option<&str>,
    ctype2: Option<&str>,
    crval1: Option<f64>,
    crval2: Option<f64>,
    cd1_1: Option<f64>,
    cd2_1: Option<f64>,
    crota2: Option<f64>,
) -> Option<WcsPointing> {
    if !is_ra_axis(ctype1) || !is_dec_axis(ctype2) {
        return None;
    }
    let ra_deg = crval1.filter(|v| v.is_finite())?;
    let dec_deg = crval2.filter(|v| v.is_finite())?;

    let rotation_deg = match (cd1_1, cd2_1) {
        (Some(a), Some(b)) if a.is_finite() && b.is_finite() && (a != 0.0 || b != 0.0) => {
            Some(b.atan2(a).to_degrees())
        }
        _ => crota2.filter(|v| v.is_finite()),
    };

    Some(WcsPointing { ra_deg, dec_deg, rotation_deg })
}

/// Whether a `CTYPE` value names the WCS RA axis (`RA---<projection>`, e.g.
/// `RA---TAN`, `RA---TAN-SIP`).
fn is_ra_axis(ctype: Option<&str>) -> bool {
    ctype.is_some_and(|c| c.trim().starts_with("RA--"))
}

/// Whether a `CTYPE` value names the WCS Dec axis (`DEC--<projection>`).
fn is_dec_axis(ctype: Option<&str>) -> bool {
    ctype.is_some_and(|c| c.trim().starts_with("DEC-"))
}

#[cfg(test)]
mod wcs_tests {
    use super::*;

    #[test]
    fn solved_tan_projection_with_cd_matrix_rotation() {
        let w = interpret_wcs_pointing(
            Some("RA---TAN"),
            Some("DEC--TAN"),
            Some(10.684_708),
            Some(41.268_75),
            Some(-0.000_193_5),
            Some(0.000_050_1),
            None,
        )
        .expect("real WCS keywords must solve");
        assert!((w.ra_deg - 10.684_708).abs() < 1e-9);
        assert!((w.dec_deg - 41.268_75).abs() < 1e-9);
        assert!(w.rotation_deg.is_some());
    }

    #[test]
    fn crota2_fallback_when_no_cd_matrix() {
        let w = interpret_wcs_pointing(
            Some("RA---TAN"),
            Some("DEC--TAN"),
            Some(83.822_08),
            Some(-5.391_11),
            None,
            None,
            Some(12.5),
        )
        .unwrap();
        assert_eq!(w.rotation_deg, Some(12.5));
    }

    #[test]
    fn no_rotation_term_is_none_but_pointing_still_solves() {
        let w = interpret_wcs_pointing(
            Some("RA---TAN"),
            Some("DEC--TAN"),
            Some(83.822_08),
            Some(-5.391_11),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(w.rotation_deg, None);
    }

    #[test]
    fn missing_or_non_equatorial_ctype_is_not_solved() {
        // No CTYPE at all.
        assert!(
            interpret_wcs_pointing(None, None, Some(10.0), Some(20.0), None, None, None).is_none()
        );
        // Galactic projection, not equatorial — CRVAL1/2 must not be trusted.
        assert!(interpret_wcs_pointing(
            Some("GLON-TAN"),
            Some("GLAT-TAN"),
            Some(10.0),
            Some(20.0),
            None,
            None,
            None
        )
        .is_none());
    }

    #[test]
    fn missing_crval_is_not_solved() {
        assert!(interpret_wcs_pointing(
            Some("RA---TAN"),
            Some("DEC--TAN"),
            None,
            Some(20.0),
            None,
            None,
            None
        )
        .is_none());
    }
}

/// Parse a decimal value from a FITS/XISF value string, tolerating a trailing
/// FITS comment is the caller's responsibility (pass the already-extracted
/// value). Returns `None` for empty or non-numeric input.
#[must_use]
pub fn parse_f64(raw: &str) -> Option<f64> {
    let t = raw.trim();
    if t.is_empty() {
        None
    } else {
        t.parse::<f64>().ok()
    }
}

/// Parse an integer value (e.g. `OFFSET`) from a value string. Accepts values
/// written as floats (`"20.0"` → `20`) by truncating toward zero.
#[must_use]
pub fn parse_i64(raw: &str) -> Option<i64> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<i64>() {
        return Some(v);
    }
    // Fall back to float parse (some writers emit `20.0`); round to nearest
    // representable integer (offsets/pedestals are small, well within range).
    #[allow(clippy::cast_possible_truncation)]
    t.parse::<f64>().ok().map(|f| f.trunc() as i64)
}

// ── MetadataExtractor ─────────────────────────────────────────────────────────

/// Trait implemented by format-specific adapters (`crates/metadata/fits`,
/// `crates/metadata/xisf`).
///
/// Extractors read header-level metadata only; they MUST NOT modify files.
pub trait MetadataExtractor: Send + Sync {
    /// Read raw metadata from the file at `path`.
    ///
    /// Returns `Ok(None)` when the file exists but the format is not supported
    /// by this extractor (e.g. a FITS extractor encountering an XISF file).
    ///
    /// # Errors
    /// Returns an error string describing any I/O or parse failure.
    fn extract(
        &self,
        path: &std::path::Path,
    ) -> Result<Option<RawFileMetadata>, MetadataExtractError>;

    /// Returns `true` if this extractor can handle the given file extension
    /// (case-insensitive, without leading dot).
    fn supports_extension(&self, ext: &str) -> bool;
}

/// Error type for [`MetadataExtractor::extract`].
#[derive(Clone, Debug, thiserror::Error)]
pub enum MetadataExtractError {
    #[error("I/O error reading {path}: {msg}")]
    Io { path: String, msg: String },
    #[error("Parse error in {path}: {msg}")]
    Parse { path: String, msg: String },
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_type_round_trips() {
        for ft in [
            FrameType::Light,
            FrameType::Dark,
            FrameType::Bias,
            FrameType::Flat,
            FrameType::DarkFlat,
        ] {
            assert_eq!(FrameType::from_str_ci(ft.as_str()), Some(ft));
        }
    }

    #[test]
    fn evidence_source_round_trips() {
        for es in [
            EvidenceSource::ImagetypHeader,
            EvidenceSource::XisfProperty,
            EvidenceSource::ManualOverride,
            EvidenceSource::None,
        ] {
            assert_eq!(EvidenceSource::from_str_ci(es.as_str()), Some(es));
        }
    }

    #[test]
    fn normalization_table_light_variants() {
        let table = v1_normalization_table();
        assert_eq!(table.normalize("Light"), Some(FrameType::Light));
        assert_eq!(table.normalize("LIGHT"), Some(FrameType::Light));
        assert_eq!(table.normalize("light frame"), Some(FrameType::Light));
        assert_eq!(table.normalize("Science"), Some(FrameType::Light));
        assert_eq!(table.normalize("object"), Some(FrameType::Light));
    }

    #[test]
    fn normalization_table_dark_variants() {
        let table = v1_normalization_table();
        assert_eq!(table.normalize("Dark"), Some(FrameType::Dark));
        assert_eq!(table.normalize("DARK FRAME"), Some(FrameType::Dark));
    }

    #[test]
    fn normalization_table_bias_variants() {
        let table = v1_normalization_table();
        assert_eq!(table.normalize("Bias"), Some(FrameType::Bias));
        assert_eq!(table.normalize("Offset"), Some(FrameType::Bias));
        assert_eq!(table.normalize("zero"), Some(FrameType::Bias));
    }

    #[test]
    fn normalization_table_flat_variants() {
        let table = v1_normalization_table();
        assert_eq!(table.normalize("Flat"), Some(FrameType::Flat));
        assert_eq!(table.normalize("flat frame"), Some(FrameType::Flat));
        assert_eq!(table.normalize("Sky Flat"), Some(FrameType::Flat));
    }

    #[test]
    fn normalization_table_unknown_returns_none() {
        let table = v1_normalization_table();
        assert_eq!(table.normalize("UNKNOWN_TYPE"), None);
        assert_eq!(table.normalize(""), None);
        assert_eq!(table.normalize("dark flat"), None);
        assert_eq!(table.normalize("darkflat"), None);
    }

    #[test]
    fn normalization_table_trims_whitespace() {
        let table = v1_normalization_table();
        assert_eq!(table.normalize("  Light  "), Some(FrameType::Light));
        assert_eq!(table.normalize("\tDark\t"), Some(FrameType::Dark));
    }

    #[test]
    fn normalization_table_not_empty() {
        let table = v1_normalization_table();
        assert!(!table.is_empty());
        assert!(table.len() > 10);
    }

    // ── Sexagesimal coordinate conversion (spec 041 T062) ─────────────────────

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "expected {b}, got {a}");
    }

    #[test]
    fn ra_hms_to_deg_real_poseidon() {
        // Real Poseidon-C header: OBJCTRA = '18 10 38' (H M S)
        // 18h10m38s = (18 + 10/60 + 38/3600) * 15 = 272.6583333…°
        approx(sexagesimal_ra_to_deg("18 10 38").unwrap(), 272.658_333_333);
    }

    #[test]
    fn ra_hms_to_deg_with_fractional_seconds() {
        // Real XISF header: OBJCTRA = '5 34 57.984'
        approx(sexagesimal_ra_to_deg("5 34 57.984").unwrap(), 83.7416);
    }

    #[test]
    fn ra_hms_strips_surrounding_quotes() {
        approx(sexagesimal_ra_to_deg("'5 34 57.984'").unwrap(), 83.7416);
    }

    #[test]
    fn ra_hms_colon_separator() {
        approx(sexagesimal_ra_to_deg("18:10:38").unwrap(), 272.658_333_333);
    }

    #[test]
    fn dec_dms_negative_real_poseidon() {
        // Real Poseidon-C header: OBJCTDEC = '-15 01 11' (D M S)
        // -(15 + 1/60 + 11/3600) = -15.019722…
        approx(sexagesimal_dec_to_deg("-15 01 11").unwrap(), -15.019_722_222);
    }

    #[test]
    fn dec_dms_positive_zero() {
        // Real DWARF/synthetic: OBJCTDEC = '+0 00 00.00'
        approx(sexagesimal_dec_to_deg("+0 00 00.00").unwrap(), 0.0);
    }

    #[test]
    fn dec_dms_negative_sub_degree_keeps_sign() {
        // -0d 20m → must stay negative even though degree field is 0
        approx(sexagesimal_dec_to_deg("-0 20 00").unwrap(), -0.333_333_333);
    }

    #[test]
    fn sexagesimal_degrees_only() {
        approx(sexagesimal_dec_to_deg("45").unwrap(), 45.0);
    }

    #[test]
    fn sexagesimal_invalid_returns_none() {
        assert!(sexagesimal_ra_to_deg("").is_none());
        assert!(sexagesimal_ra_to_deg("abc").is_none());
        assert!(sexagesimal_dec_to_deg("'   '").is_none());
    }

    #[test]
    fn parse_i64_handles_int_and_float() {
        assert_eq!(parse_i64("20"), Some(20));
        assert_eq!(parse_i64(" 50 "), Some(50));
        assert_eq!(parse_i64("20.0"), Some(20));
        assert_eq!(parse_i64(""), None);
        assert_eq!(parse_i64("x"), None);
    }

    #[test]
    fn parse_f64_helper() {
        approx(parse_f64("3.76").unwrap(), 3.76);
        approx(parse_f64(" -15.0055 ").unwrap(), -15.0055);
        assert!(parse_f64("").is_none());
    }
}
