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
/// `[-90, 90]`, so the rounded magnitude can reach this but not exceed it.
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
    let total = round_to_whole(dec_deg.abs()).min(DEC_ARCSEC_LIMIT);
    let (d, m, s) = split_sexagesimal(total);
    format!("{sign}{d:02}\u{b0}{m:02}\u{2032}{s:02}\u{2033}")
}

/// Round a non-negative hours/degrees magnitude to whole seconds/arcseconds.
///
/// The single rounding step in the whole formatter. Saturates rather than
/// wrapping on a magnitude no sky coordinate can reach; callers pass values
/// already put in domain by [`coords::to_equatorial`].
fn round_to_whole(magnitude: f64) -> u32 {
    let seconds = (magnitude * 3600.0).round();
    if seconds <= 0.0 {
        0
    } else if seconds >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        // Bounded on both sides above, and `round` leaves no fraction.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            seconds as u32
        }
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
    #[test]
    fn extreme_finite_input_stays_in_domain() {
        let s = sexagesimal(-1e308, -1e308).unwrap();
        assert_eq!(s.ra, "04h16m00s");
        assert_eq!(s.dec, "\u{2212}90\u{b0}00\u{2032}00\u{2033}");
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

    /// Millidegree sweep of the display domain — the sweep that found 4535
    /// out-of-domain fields in the previous implementation.
    #[test]
    fn no_field_is_ever_out_of_domain_across_the_whole_sky() {
        for ra_thousandths in 0..360_000u32 {
            let ra = f64::from(ra_thousandths) / 1000.0;
            for dec_thousandths in [0i32, 45_000, 89_000, 89_999, 90_000, -45_000, -90_000] {
                let dec = f64::from(dec_thousandths) / 1000.0;
                let coords = sexagesimal(ra, dec).unwrap();

                let ra_fields = fields(&coords.ra);
                assert!(
                    ra_fields[0] < 24 && ra_fields[1] < 60 && ra_fields[2] < 60,
                    "ra={} from {ra}",
                    coords.ra
                );

                let dec_fields = fields(&coords.dec);
                assert!(
                    dec_fields[0] <= 90 && dec_fields[1] < 60 && dec_fields[2] < 60,
                    "dec={} from {dec}",
                    coords.dec
                );
            }
        }
    }
}
