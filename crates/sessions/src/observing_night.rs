// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical civil observing-night derivation.
//!
//! IANA timezone resolution belongs at the application boundary. This module
//! receives the date-effective local timestamp after that resolution, or a
//! local timestamp whose fallback use has already been reviewed.

use thiserror::Error;
use time::{Date, OffsetDateTime, PrimitiveDateTime};

/// Evidence path used to derive an observing night.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ObservingNightDerivation {
    /// A canonical exposure instant resolved through the named acquisition
    /// site's IANA timezone rules.
    AcquisitionTimezone { timezone_name: String },
    /// A local timestamp accepted through an explicit review when no usable
    /// canonical instant and timezone were available.
    ReviewedLocalFallback,
}

/// Immutable local calendar date and the evidence path that produced it.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ObservingNight {
    date: Date,
    derivation: ObservingNightDerivation,
}

impl ObservingNight {
    /// Derive from a canonical instant already resolved into the acquisition
    /// site's date-effective UTC offset.
    ///
    /// # Errors
    ///
    /// Returns [`ObservingNightError::MissingTimezoneName`] for an empty IANA
    /// timezone snapshot, or [`ObservingNightError::DateUnderflow`] when a
    /// pre-noon timestamp falls before [`Date::MIN`].
    pub fn from_acquisition_timezone(
        resolved_local_capture_at: OffsetDateTime,
        timezone_name: impl Into<String>,
    ) -> Result<Self, ObservingNightError> {
        let timezone_name = timezone_name.into();
        if timezone_name.trim().is_empty() {
            return Err(ObservingNightError::MissingTimezoneName);
        }

        Ok(Self {
            date: noon_bounded_date(
                resolved_local_capture_at.date(),
                resolved_local_capture_at.hour(),
            )?,
            derivation: ObservingNightDerivation::AcquisitionTimezone { timezone_name },
        })
    }

    /// Derive from a reviewed local timestamp without inventing a timezone.
    ///
    /// # Errors
    ///
    /// Returns [`ObservingNightError::DateUnderflow`] when a pre-noon
    /// timestamp falls before [`Date::MIN`].
    pub fn from_reviewed_local_fallback(
        local_capture_at: PrimitiveDateTime,
    ) -> Result<Self, ObservingNightError> {
        Ok(Self {
            date: noon_bounded_date(local_capture_at.date(), local_capture_at.hour())?,
            derivation: ObservingNightDerivation::ReviewedLocalFallback,
        })
    }

    #[must_use]
    pub const fn date(&self) -> Date {
        self.date
    }

    #[must_use]
    pub const fn derivation(&self) -> &ObservingNightDerivation {
        &self.derivation
    }

    #[must_use]
    pub fn acquisition_timezone(&self) -> Option<&str> {
        match &self.derivation {
            ObservingNightDerivation::AcquisitionTimezone { timezone_name } => Some(timezone_name),
            ObservingNightDerivation::ReviewedLocalFallback => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ObservingNightError {
    #[error("acquisition-timezone derivation requires an IANA timezone name")]
    MissingTimezoneName,
    #[error("pre-noon observing-night derivation falls before the supported date range")]
    DateUnderflow,
}

/// The single noon-bounded observing-night date derivation in this crate.
///
/// A pre-noon local timestamp belongs to the previous night; when that day does
/// not exist the derivation is refused rather than substituting `local_date`.
pub(crate) fn noon_bounded_date(
    local_date: Date,
    local_hour: u8,
) -> Result<Date, ObservingNightError> {
    if local_hour < 12 {
        local_date.previous_day().ok_or(ObservingNightError::DateUnderflow)
    } else {
        Ok(local_date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Duration, Month, Time, UtcOffset};

    fn local(
        year: i32,
        month: Month,
        day: u8,
        hour: u8,
        minute: u8,
        offset_hours: i8,
    ) -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(year, month, day).unwrap(),
            Time::from_hms(hour, minute, 0).unwrap(),
        )
        .assume_offset(UtcOffset::from_hms(offset_hours, 0, 0).unwrap())
    }

    fn utc(year: i32, month: Month, day: u8, hour: u8, minute: u8) -> OffsetDateTime {
        local(year, month, day, hour, minute, 0)
    }

    #[test]
    fn exact_noon_starts_the_new_observing_night() {
        let exact_noon = local(2026, Month::March, 16, 12, 0, 1);
        let just_before = exact_noon - Duration::microseconds(1);

        let at_boundary =
            ObservingNight::from_acquisition_timezone(exact_noon, "Europe/Amsterdam").unwrap();
        let before_boundary =
            ObservingNight::from_acquisition_timezone(just_before, "Europe/Amsterdam").unwrap();

        assert_eq!(at_boundary.date(), Date::from_calendar_date(2026, Month::March, 16).unwrap());
        assert_eq!(
            before_boundary.date(),
            Date::from_calendar_date(2026, Month::March, 15).unwrap()
        );
    }

    #[test]
    fn spring_forward_uses_the_resolved_date_effective_offset() {
        let before_jump =
            utc(2026, Month::March, 8, 6, 59).to_offset(UtcOffset::from_hms(-5, 0, 0).unwrap());
        let after_jump =
            utc(2026, Month::March, 8, 7, 0).to_offset(UtcOffset::from_hms(-4, 0, 0).unwrap());

        let before =
            ObservingNight::from_acquisition_timezone(before_jump, "America/New_York").unwrap();
        let after =
            ObservingNight::from_acquisition_timezone(after_jump, "America/New_York").unwrap();

        let expected = Date::from_calendar_date(2026, Month::March, 7).unwrap();
        assert_eq!(before.date(), expected);
        assert_eq!(after.date(), expected);
    }

    #[test]
    fn fall_back_accepts_both_resolved_offsets_for_the_repeated_hour() {
        let first =
            utc(2026, Month::November, 1, 5, 30).to_offset(UtcOffset::from_hms(-4, 0, 0).unwrap());
        let second =
            utc(2026, Month::November, 1, 6, 30).to_offset(UtcOffset::from_hms(-5, 0, 0).unwrap());

        let first_night =
            ObservingNight::from_acquisition_timezone(first, "America/New_York").unwrap();
        let second_night =
            ObservingNight::from_acquisition_timezone(second, "America/New_York").unwrap();

        let expected = Date::from_calendar_date(2026, Month::October, 31).unwrap();
        assert_eq!(first_night.date(), expected);
        assert_eq!(second_night.date(), expected);
    }

    #[test]
    fn remote_site_timezone_wins_over_the_machine_timezone() {
        let canonical = utc(2026, Month::March, 15, 10, 0);
        let acquisition_site = canonical.to_offset(UtcOffset::from_hms(-7, 0, 0).unwrap());

        let night =
            ObservingNight::from_acquisition_timezone(acquisition_site, "America/Los_Angeles")
                .unwrap();

        assert_eq!(night.date(), Date::from_calendar_date(2026, Month::March, 14).unwrap());
        assert_eq!(night.acquisition_timezone(), Some("America/Los_Angeles"));
    }

    #[test]
    fn reviewed_fallback_keeps_timezone_absent() {
        let timestamp = PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::March, 16).unwrap(),
            Time::from_hms(11, 30, 0).unwrap(),
        );

        let night = ObservingNight::from_reviewed_local_fallback(timestamp).unwrap();

        assert_eq!(night.date(), Date::from_calendar_date(2026, Month::March, 15).unwrap());
        assert_eq!(night.derivation(), &ObservingNightDerivation::ReviewedLocalFallback);
        assert_eq!(night.acquisition_timezone(), None);
    }

    #[test]
    fn acquisition_timezone_requires_a_named_timezone_snapshot() {
        assert_eq!(
            ObservingNight::from_acquisition_timezone(
                local(2026, Month::March, 16, 12, 0, 0),
                "  ",
            ),
            Err(ObservingNightError::MissingTimezoneName)
        );
    }
}
