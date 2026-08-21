// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Sexagesimal RA/Dec display formatting.
//!
//! `skymath::Angle::format_ra`/`format_dec` (0.7.1) document themselves as
//! carry-safe but are not: they round in *value* units
//! (`(value.abs() * 3600.0).round() / 3600.0`), which reintroduces float error,
//! then truncate each field and let `{:02.0}` round the leftover seconds
//! remainder a second time. RA 15.25° reaches `01:00:60` that way, and an
//! exhaustive millidegree sweep finds 3115 such RA values and 1420 such Dec
//! values. A `60` seconds field is an out-of-domain sky position, not a cosmetic
//! defect (`astro-plan-3v3r.8.40`).
//!
//! So the decomposition happens here instead: round once to whole seconds in
//! integer units, then split the integer. Carry is structural — a value that
//! rounds up to 60 seconds becomes the next minute before any field exists, so
//! no field can hold 60.
//!
//! `skymath` still owns the coordinate itself: [`coords::to_equatorial`] is what
//! puts an arbitrary finite input into domain, wrapping RA and clamping Dec.

use crate::coords::{self, Pointing};

/// Seconds of time in 24h of right ascension. RA that carries up to exactly
/// `24h00m00s` wraps to `00h00m00s`; RA has no sign and no 24th hour.
const RA_SECONDS_PER_DAY: u32 = 24 * 3600;

/// Arcseconds in the 90° Dec limit. [`coords::to_equatorial`] clamps Dec into
/// `[-90, 90]` and `90 * 3600` is exact in binary floating point, so the rounded
/// magnitude reaches this value but never exceeds it.
const DEC_ARCSEC_LIMIT: u32 = 90 * 3600;

/// A target's RA/Dec formatted in astronomy notation: `HHhMMmSSs` for RA,
/// `±DD°MM′SS″` for Dec (negative sign is U+2212, not the ASCII hyphen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SexagesimalCoords {
    pub ra: String,
    pub dec: String,
}

/// Format decimal-degree RA/Dec as astronomy-notation sexagesimal strings.
///
/// Returns `None` when either coordinate is non-finite (NaN/±inf) — never a
/// fabricated string. Out-of-domain-but-finite input is wrapped/clamped into
/// domain first (see [`coords::to_equatorial`]).
///
/// Every field is in domain: RA hours `00..=23`, Dec degrees `00..=90`, minutes
/// and seconds `00..=59`.
#[must_use]
pub fn sexagesimal(ra_deg: f64, dec_deg: f64) -> Option<SexagesimalCoords> {
    if !ra_deg.is_finite() || !dec_deg.is_finite() {
        return None;
    }
    let (ra_deg, dec_deg) = coords::to_equatorial(Pointing::new(ra_deg, dec_deg)).to_degrees();
    Some(SexagesimalCoords { ra: ra_glyphs(ra_deg), dec: dec_glyphs(dec_deg) })
}

/// In-domain RA degrees → `HHhMMmSSs`.
fn ra_glyphs(ra_deg: f64) -> String {
    // `to_equatorial` already wrapped into [0, 360), so the modulo only catches
    // a carry off the top: 359.99999° rounds to 24h00m00s, which is 00h00m00s.
    let total = round_to_whole(ra_deg / 15.0) % RA_SECONDS_PER_DAY;
    let (h, m, s) = split_sexagesimal(total);
    format!("{h:02}h{m:02}m{s:02}s")
}

/// In-domain Dec degrees → `±DD°MM′SS″`, with U+2212 (minus sign) for negative
/// rather than the ASCII hyphen.
fn dec_glyphs(dec_deg: f64) -> String {
    let sign = if dec_deg < 0.0 { "\u{2212}" } else { "+" };
    let total = round_to_whole(dec_deg.abs());
    debug_assert!(total <= DEC_ARCSEC_LIMIT, "to_equatorial should have clamped Dec: {dec_deg}");
    let (d, m, s) = split_sexagesimal(total);
    format!("{sign}{d:02}\u{b0}{m:02}\u{2032}{s:02}\u{2033}")
}

/// Round a non-negative hours/degrees magnitude to whole seconds/arcseconds.
///
/// The single rounding step in the whole formatter. Callers pass magnitudes
/// already put in domain by [`coords::to_equatorial`], so the clamp only keeps
/// the cast total, never reshapes a real coordinate.
fn round_to_whole(magnitude: f64) -> u32 {
    let seconds = (magnitude * 3600.0).round().clamp(0.0, f64::from(u32::MAX));
    // Clamped on both sides above, and `round` leaves no fraction.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        seconds as u32
    }
}

/// Split whole seconds into `(units, minutes, seconds)`, each field in domain by
/// construction.
fn split_sexagesimal(total: u32) -> (u32, u32, u32) {
    (total / 3600, (total % 3600) / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The digit runs of a formatted coordinate as integers, so a test can
    /// assert on the field domain rather than on a substring.
    fn fields(text: &str) -> Vec<u32> {
        text.split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().expect("digit run should parse"))
            .collect()
    }

    #[test]
    fn formats_m31_exact_glyphs() {
        // M31: RA 10.6847deg -> 00h42m44s, Dec 41.2688deg -> +41°16′08″.
        let s = sexagesimal(10.6847, 41.2688).unwrap();
        assert_eq!(s.ra, "00h42m44s");
        assert_eq!(s.dec, "+41\u{b0}16\u{2032}08\u{2033}");
    }

    #[test]
    fn negative_dec_uses_u2212_not_ascii_hyphen() {
        let s = sexagesimal(10.0, -5.5).unwrap();
        assert_eq!(s.dec, "\u{2212}05\u{b0}30\u{2032}00\u{2033}");
        assert!(!s.dec.starts_with('-'), "must be U+2212, not ASCII hyphen: {}", s.dec);
    }

    /// `-0.0 < 0.0` is false, so a Dec that rounds to zero from below still reads
    /// `+00°00′00″`. A signed-zero test here would flip that to a minus pole.
    #[test]
    fn dec_of_negative_zero_is_not_signed() {
        assert_eq!(sexagesimal(0.0, -0.0).unwrap().dec, "+00\u{b0}00\u{2032}00\u{2033}");
    }

    #[test]
    fn seconds_carry_never_shows_60_and_keeps_glyphs() {
        // 44.999_999_999° Dec rounds its seconds field up into the next minute;
        // must land on a valid glyph string, never "...′60″".
        let s = sexagesimal(0.0, 44.999_999_999).unwrap();
        assert_eq!(s.dec, "+45\u{b0}00\u{2032}00\u{2033}");
        assert!(!s.dec.contains("60\u{2033}"), "dec={}", s.dec);
    }

    /// The values `skymath`'s own formatter gets wrong. RA 15.25° is not an
    /// adversarial input — it is a coordinate a user can point a mount at.
    #[test]
    fn upstream_carry_faults_land_on_the_next_minute() {
        assert_eq!(sexagesimal(15.25, 0.0).unwrap().ra, "01h01m00s");
        assert_eq!(sexagesimal(16.0, 0.0).unwrap().ra, "01h04m00s");
        assert_eq!(sexagesimal(0.0, -89.85).unwrap().dec, "\u{2212}89\u{b0}51\u{2032}00\u{2033}");
    }

    /// A finite but absurd input still produces an in-domain position, because
    /// `to_equatorial` wraps before this module rounds. `-1e308` is the fuzz
    /// vector that first surfaced the defect; it reached `04h15m60s`.
    ///
    /// Asserts the field domain rather than the exact glyphs: which RA `-1e308`
    /// wraps to is `skymath`'s float normalization, and pinning it would make an
    /// upstream normalization change read as a defect here.
    #[test]
    fn extreme_finite_input_stays_in_domain() {
        let s = sexagesimal(-1e308, -1e308).unwrap();
        assert_ra_in_domain(&s.ra, -1e308);
        assert_dec_in_domain(&s.dec, -1e308);
    }

    #[test]
    fn ra_that_carries_off_the_top_wraps_to_zero() {
        // 359.99999° rounds to 24h00m00s, which does not exist.
        let s = sexagesimal(359.999_99, 0.0).unwrap();
        assert_eq!(s.ra, "00h00m00s");
    }

    #[test]
    fn non_finite_is_none() {
        assert!(sexagesimal(f64::NAN, 0.0).is_none());
        assert!(sexagesimal(0.0, f64::INFINITY).is_none());
    }

    /// Assert an RA string holds hours `00..=23` and minutes/seconds `00..=59`.
    fn assert_ra_in_domain(ra: &str, from: f64) {
        let f = fields(ra);
        assert!(f[0] < 24 && f[1] < 60 && f[2] < 60, "ra={ra} from {from}");
    }

    /// Assert a Dec string holds degrees `00..=90`, minutes/seconds `00..=59`,
    /// and that the 90th degree is the pole exactly — `90°59′59″` is off the sky.
    fn assert_dec_in_domain(dec: &str, from: f64) {
        let f = fields(dec);
        assert!(f[1] < 60 && f[2] < 60, "dec={dec} from {from}");
        assert!(
            f[0] < 90 || (f[0] == 90 && f[1] == 0 && f[2] == 0),
            "dec={dec} from {from}"
        );
    }

    /// Every RA the display can show, at millidegree resolution. Fails on the
    /// previous implementation at 3115 of these values.
    #[test]
    fn no_ra_field_is_ever_out_of_domain() {
        for thousandths in 0..360_000u32 {
            let ra = f64::from(thousandths) / 1000.0;
            assert_ra_in_domain(&sexagesimal(ra, 0.0).unwrap().ra, ra);
        }
    }

    /// Every Dec the display can show, at millidegree resolution, both signs and
    /// both poles. Fails on the previous implementation at 1420 of these values.
    #[test]
    fn no_dec_field_is_ever_out_of_domain() {
        for thousandths in -90_000..=90_000i32 {
            let dec = f64::from(thousandths) / 1000.0;
            assert_dec_in_domain(&sexagesimal(0.0, dec).unwrap().dec, dec);
        }
    }
}
