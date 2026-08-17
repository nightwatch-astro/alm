// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure grouping engine: computes a canonical key/label/warnings from metadata.

use std::fmt::Write as _;

use metadata_core::FrameType;

use super::config::{GroupingConfig, TempSource, SENTINEL_MISSING};
use super::dimension::Dimension;
use super::metadata::FrameMetadata;
use super::result::{GroupKey, GroupLabel, GroupResult, GroupWarning};

/// Compute the deterministic group key + label + warnings for one file.
///
/// The key is a canonical serialization of `frame_type` followed by each
/// **enabled** dimension's normalized/bucketed value, rendered in the fixed
/// `Dimension::order` order. A missing value for an enabled dimension renders
/// [`SENTINEL_MISSING`]. Identical `(metadata, config)` always yields an
/// identical key (FR-042) — there is no clock, hashing of floats, or map
/// iteration involved.
///
/// `config.frame_type` is ignored in favour of `meta.frame_type` for the type
/// token (the caller is expected to pass the matching config, but the key
/// always reflects the file's authoritative type).
#[must_use]
pub fn group_file(meta: &FrameMetadata, config: &GroupingConfig) -> GroupResult {
    let ft = meta.frame_type;

    // The key always opens with the authoritative type token.
    let mut key = format!("type={}", ft.as_str());
    // Label discriminators: only the dims that carry a present, meaningful value.
    let mut label_parts: Vec<String> = Vec::new();
    let mut warnings: Vec<GroupWarning> = Vec::new();

    for &dim in config.dimensions() {
        let value = render_dimension(dim, meta, config, &mut warnings);
        // Canonical key: every enabled dim contributes, sentinel when missing.
        let key_value = value.as_deref().unwrap_or(SENTINEL_MISSING);
        let _ = write!(key, "·{}={}", dim.key_name(), key_value);

        // Label: only present values become discriminators (sentinels are noise).
        if let Some(v) = value {
            if let Some(pretty) = label_value(dim, &v) {
                label_parts.push(pretty);
            }
        }
    }

    let mut label = format!("(root) · {}", ft.as_str());
    for part in &label_parts {
        let _ = write!(label, " · {part}");
    }

    GroupResult { key: GroupKey(key), label: GroupLabel(label), warnings }
}

/// Render one dimension to its canonical, normalized/bucketed string value.
/// Returns `None` when the underlying metadata is absent (→ sentinel in the
/// key). May push warnings (temperature deviation, missing rotation).
fn render_dimension(
    dim: Dimension,
    meta: &FrameMetadata,
    config: &GroupingConfig,
    warnings: &mut Vec<GroupWarning>,
) -> Option<String> {
    match dim {
        Dimension::Camera => normalize_text(meta.instrume.as_deref()),
        Dimension::OpticTrain => optic_train(meta),
        Dimension::Filter => normalize_text(meta.filter.as_deref()),
        Dimension::Exposure => {
            meta.exposure_s.map(|e| format_num(bucket(e, config.exposure_bucket_s)))
        }
        Dimension::Gain => normalize_text(meta.gain.as_deref()),
        Dimension::Offset => meta.offset.map(|o| o.to_string()),
        Dimension::SetTemp => temperature(meta, config, warnings),
        Dimension::Binning => binning(meta),
        Dimension::Pointing => pointing(meta, config.pointing_tolerance_deg),
        Dimension::Rotation => rotation(meta, config, warnings),
        Dimension::Readout => normalize_text(meta.readout_mode.as_deref()),
        Dimension::ObservingNight => observing_night(meta),
    }
}

/// Optic-train composite = `telescop|instrume|focallen` (FR-039). Built only
/// from present parts; entirely absent ⇒ `None` (sentinel). Each part is
/// normalized; focal length is bucketed to whole mm so float noise doesn't fork
/// the group.
///
/// Intentionally separate from `sessions::optic_train_key` (spec 041 T064):
/// that function uses `"-"` for the framing-attribution identity key; this one
/// uses [`SENTINEL_MISSING`] (`"∅"`) so its output is consistent with every
/// other dimension in the persisted `group_key` column. The two keys are NOT
/// interchangeable — changing either sentinel would re-group existing inbox
/// items stored in the `inbox_item.group_key` UNIQUE column.
fn optic_train(meta: &FrameMetadata) -> Option<String> {
    let tel = normalize_text(meta.telescop.as_deref());
    let inst = normalize_text(meta.instrume.as_deref());
    let fl = meta.focal_length_mm.map(|f| format_num(f.round()));
    if tel.is_none() && inst.is_none() && fl.is_none() {
        return None;
    }
    Some(format!(
        "{}|{}|{}",
        tel.as_deref().unwrap_or(SENTINEL_MISSING),
        inst.as_deref().unwrap_or(SENTINEL_MISSING),
        fl.as_deref().unwrap_or(SENTINEL_MISSING),
    ))
}

/// Temperature dimension (R-9 / FR-037). Bucketed `SET-TEMP` (or `CCD-TEMP`
/// per the toggle). When both temps are present and deviate beyond the warn
/// threshold, push a [`GroupWarning::TempDeviation`] but keep the setpoint
/// governing the group (no split).
fn temperature(
    meta: &FrameMetadata,
    config: &GroupingConfig,
    warnings: &mut Vec<GroupWarning>,
) -> Option<String> {
    // Deviation warning is independent of which source governs the group.
    if let (Some(set), Some(ccd)) = (meta.set_temp_c, meta.ccd_temp_c) {
        let deviation = (ccd - set).abs();
        if deviation > config.temp_deviation_warn_c {
            warnings.push(GroupWarning::TempDeviation {
                deviation_c: deviation,
                threshold_c: config.temp_deviation_warn_c,
            });
        }
    }

    let source = match config.temp_source {
        TempSource::SetTemp => meta.set_temp_c,
        TempSource::CcdTemp => meta.ccd_temp_c,
    };
    source.map(|t| format_num(bucket(t, config.set_temp_tolerance_c)))
}

/// Pointing dimension (FR-038). RA/Dec snapped to the tolerance grid so frames
/// within `pointing_tolerance_deg` share a bucket. Both must be present.
fn pointing(meta: &FrameMetadata, tolerance_deg: f64) -> Option<String> {
    let (ra, dec) = (meta.ra_deg?, meta.dec_deg?);
    let ra_b = format_num(bucket(ra, tolerance_deg));
    let dec_b = format_num(bucket(dec, tolerance_deg));
    Some(format!("{ra_b},{dec_b}"))
}

/// Rotation dimension for grouping (FR-040, R-18). Uses the mechanical
/// `ROTATANG`, NEVER `OBJCTROT`. Snapped to the rotation tolerance grid. When
/// absent on a flat with rotation enabled and `flat_rotation_required` OFF,
/// pushes [`GroupWarning::RotationUnavailable`] and returns `None` (sentinel).
fn rotation(
    meta: &FrameMetadata,
    config: &GroupingConfig,
    warnings: &mut Vec<GroupWarning>,
) -> Option<String> {
    if let Some(angle) = meta.rotator_angle_deg {
        return Some(format_num(bucket(angle, config.rotation_tolerance_deg)));
    }
    // Absent ROTATANG: on a flat (rotation enabled, not required) this is the
    // "matched without rotation" warn path (R-18); otherwise a plain sentinel.
    if meta.frame_type == FrameType::Flat && !config.flat_rotation_required {
        warnings.push(GroupWarning::RotationUnavailable);
    }
    None
}

/// Binning dimension, normalized to `"NxN"`. Requires both factors.
fn binning(meta: &FrameMetadata) -> Option<String> {
    match (meta.binning_x, meta.binning_y) {
        (Some(x), Some(y)) => Some(format!("{x}x{y}")),
        _ => None,
    }
}

/// Observing-night = local calendar date under a noon boundary (R-18). Source
/// priority `DATE-LOC → DATE-OBS`. We take the local date directly: NINA writes
/// `DATE-LOC` already in local civil time, so the calendar date *is* the
/// observing night for evening sessions. (A future enhancement may shift the
/// pre-noon hours back a day; the current rule keys on the rendered date so it
/// is deterministic.)
fn observing_night(meta: &FrameMetadata) -> Option<String> {
    let raw = meta
        .date_loc
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(meta.date_obs.as_deref())
        .filter(|s| !s.trim().is_empty())?;
    // Take the date portion before any 'T' (ISO-8601) or space separator.
    let date = raw.trim().split(['T', ' ']).next().unwrap_or(raw).trim();
    if date.is_empty() {
        None
    } else {
        Some(date.to_owned())
    }
}

// ── Value helpers ────────────────────────────────────────────────────────────────

/// Normalize free text (camera/filter/telescope/gain/readout): trim, collapse
/// internal whitespace, and case-fold so trivial header variations don't fork a
/// group. Empty ⇒ `None`.
fn normalize_text(value: Option<&str>) -> Option<String> {
    let v = value?.trim();
    if v.is_empty() {
        return None;
    }
    // Single-pass fold: collapse whitespace + ASCII lowercase without a Vec
    // intermediate. ASCII lowercase is intentional — group_key values are
    // persisted to the DB and must not change when non-ASCII casing rules
    // change (e.g. Turkish İ). Matches the original to_ascii_lowercase() call.
    let mut out = String::with_capacity(v.len());
    let mut prev_ws = false;
    for c in v.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c.to_ascii_lowercase());
            prev_ws = false;
        }
    }
    Some(out)
}

/// Snap a continuous value to the nearest multiple of `size` (FR-036). A
/// non-positive `size` disables bucketing (returns the value unchanged). The
/// result is canonicalized through [`format_num`] by the caller.
#[must_use]
fn bucket(value: f64, size: f64) -> f64 {
    if size <= 0.0 || !size.is_finite() {
        return value;
    }
    (value / size).round() * size
}

/// Canonical numeric formatting: integers render without a decimal point;
/// fractional values render with up to 6 significant decimals, trailing zeros
/// trimmed. `-0.0` normalizes to `0`. This makes the key stable regardless of
/// the float's binary representation (FR-042).
#[must_use]
fn format_num(value: f64) -> String {
    // Normalize negative zero.
    let v = if value == 0.0 { 0.0 } else { value };
    if v.fract() == 0.0 && v.abs() < 1e15 {
        // Whole number — render as an integer with no decimal point.
        #[allow(clippy::cast_possible_truncation)]
        return (v as i64).to_string();
    }
    // Fractional — fixed 6-dp then trim trailing zeros (deterministic).
    let mut s = format!("{v:.6}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// Render a dimension's canonical value into a friendly label fragment, or
/// `None` to omit it from the label entirely (e.g. optic-train, which is too
/// verbose for the discriminator line).
fn label_value(dim: Dimension, value: &str) -> Option<String> {
    match dim {
        // Optic-train is part of identity but too verbose for the label line.
        Dimension::OpticTrain => None,
        // Dimensions whose raw normalized value reads cleanly on its own.
        Dimension::Camera
        | Dimension::Filter
        | Dimension::Binning
        | Dimension::Readout
        | Dimension::ObservingNight => Some(value.to_owned()),
        Dimension::Exposure => Some(format!("{value}s")),
        Dimension::Gain => Some(format!("gain {value}")),
        Dimension::Offset => Some(format!("offset {value}")),
        Dimension::SetTemp => Some(format!("{value}°C")),
        Dimension::Pointing => Some(format!("@{value}")),
        Dimension::Rotation => Some(format!("rot {value}°")),
    }
}

#[cfg(test)]
mod tests;
