// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Stable identifier and timestamp primitives.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;
use time::OffsetDateTime;
use uuid::Uuid;

/// Stable UUIDv4 identifier for any entity.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    Type,
)]
#[serde(transparent)]
pub struct EntityId(Uuid);

impl EntityId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for EntityId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Opaque UUID identifier for an `AuditLogEntry`.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    Type,
)]
#[serde(transparent)]
pub struct AuditId(Uuid);

impl AuditId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for AuditId {
    fn default() -> Self {
        Self::new()
    }
}

/// 32-byte SHA-256 content hash. Lazy — populated only when a workflow demands it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    /// Construct from a lowercase hex string. Panics in debug if length != 64.
    #[must_use]
    pub fn from_hex(hex: impl Into<String>) -> Self {
        let s = hex.into();
        debug_assert_eq!(s.len(), 64, "SHA-256 hex must be 64 chars");
        Self(s)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// RFC 3339 with the subsecond fraction pinned to exactly six digits.
///
/// The fixed width is load-bearing for ordering, not cosmetic.
/// `well_known::Rfc3339` trims trailing zeros from the fraction, so it emits
/// both `.1Z` and `.15Z`; comparing those as text puts `.1Z` AFTER `.15Z`
/// (`Z` > `5`) even though 0.1 s precedes 0.15 s. These strings land in the
/// `*_at` columns that SQLite `ORDER BY` compares as TEXT, so a trimmed
/// fraction makes those orderings silently non-chronological. Padding to a
/// constant width restores `text order == chronological order`.
const ISO_FIXED_MICROS: &[time::format_description::FormatItem<'static>] = time::macros::format_description!(
    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]Z"
);

/// Format `value` as UTC RFC 3339 with a constant-width subsecond fraction.
///
/// # Panics
///
/// Panics if the formatter fails, which cannot happen for a well-formed
/// `OffsetDateTime` and this format description.
fn iso_fixed_micros(value: OffsetDateTime) -> String {
    value
        .to_offset(time::UtcOffset::UTC)
        .format(&ISO_FIXED_MICROS)
        .expect("fixed-micros RFC 3339 format is always valid for OffsetDateTime")
}

/// RFC 3339 UTC timestamp wrapper.
///
/// The explicit `time::serde::rfc3339` is load-bearing: `OffsetDateTime`'s
/// default serde impl is a 9-element integer component array, which contradicts
/// this type's `JsonSchema` (`string` / `date-time`), its `now_iso()` helper,
/// and the RFC 3339 string written to the durable `events.emitted_at` column
/// (issue #1093).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(transparent)]
#[specta(transparent)]
pub struct Timestamp(#[serde(with = "time::serde::rfc3339")] OffsetDateTime);

impl Timestamp {
    #[must_use]
    pub fn now_utc() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    /// Return the current UTC time as an RFC 3339 / ISO-8601 string.
    ///
    /// This is the canonical single home for "give me a timestamp string right
    /// now" — later dedup work (US11) will redirect callers here.
    #[must_use]
    pub fn now_iso() -> String {
        iso_fixed_micros(OffsetDateTime::now_utc())
    }

    /// Render this instant in the same form [`Timestamp::now_iso`] produces.
    ///
    /// Use this for any value bound to a column an `ORDER BY` touches:
    /// formatting with `well_known::Rfc3339` instead yields a variable-width
    /// fraction, which does not sort chronologically as TEXT.
    #[must_use]
    pub fn to_iso(self) -> String {
        iso_fixed_micros(self.0)
    }

    #[must_use]
    pub const fn from_offset_date_time(value: OffsetDateTime) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_offset_date_time(self) -> OffsetDateTime {
        self.0
    }
}

/// Return a fresh UUIDv4 as a hyphenated lowercase string (36 chars).
///
/// This is the canonical single home for "give me a new ID string" —
/// later dedup work (US11) will redirect callers here.
#[must_use]
pub fn new_id() -> String {
    EntityId::new().to_string()
}

impl From<OffsetDateTime> for Timestamp {
    fn from(value: OffsetDateTime) -> Self {
        Self::from_offset_date_time(value)
    }
}

// Manual JsonSchema impl: represent as RFC 3339 date-time string.
//
// schemars 1.x: `schema_name` returns `Cow<'static, str>`, `json_schema`
// takes `&mut SchemaGenerator` and returns `schemars::Schema` (a thin wrapper
// over `serde_json::Value`). The `json_schema!` macro builds it from JSON.
impl schemars::JsonSchema for Timestamp {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Timestamp")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "format": "date-time",
            "description": "RFC 3339 UTC timestamp."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{new_id, Timestamp};

    /// Every `*_at` column holds these strings and SQLite `ORDER BY` compares
    /// them as TEXT, so text order MUST equal chronological order even when the
    /// two instants have different numbers of significant subsecond digits.
    /// Under a trailing-zero-trimming formatter this pair renders `.1Z` and
    /// `.15Z`, and `Z` > `5` reverses them (astro-plan-lxnwv).
    #[test]
    fn iso_text_order_matches_chronological_order_across_subsecond_widths() {
        let base = time::OffsetDateTime::from_unix_timestamp(1_752_000_000)
            .expect("fixed unix timestamp is in range");
        let earlier = super::iso_fixed_micros(base + time::Duration::milliseconds(100));
        let later = super::iso_fixed_micros(base + time::Duration::milliseconds(150));

        assert!(earlier < later, "{earlier} must sort before {later}");
    }

    #[test]
    fn now_iso_parses_as_rfc3339() {
        let s = Timestamp::now_iso();
        // Must parse back without error (time Rfc3339 round-trip).
        time::OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|e| panic!("now_iso() produced non-RFC3339 output {s:?}: {e}"));
    }

    /// Pins the wire form against `OffsetDateTime`'s default 9-int component
    /// array (issue #1093) and against the type's own `JsonSchema`.
    #[test]
    fn timestamp_serializes_as_an_rfc3339_string_and_round_trips() {
        let ts = Timestamp::from_offset_date_time(
            time::OffsetDateTime::from_unix_timestamp(1_752_000_000)
                .expect("fixed unix timestamp is in range"),
        );

        let json = serde_json::to_string(&ts).expect("Timestamp should serialize");
        assert_eq!(json, r#""2025-07-08T18:40:00Z""#);

        let back: Timestamp = serde_json::from_str(&json).expect("Timestamp should deserialize");
        assert_eq!(back, ts, "serialize/deserialize must round-trip");
    }

    #[test]
    fn new_id_is_36_char_uuid() {
        let id = new_id();
        assert_eq!(id.len(), 36, "UUID string must be 36 chars, got {id:?}");
        // Standard hyphenated UUID: 8-4-4-4-12
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5, "UUID must have 5 hyphen-separated groups: {id:?}");
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }
}
