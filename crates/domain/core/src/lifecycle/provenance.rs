// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `ProvenancedValue<T>` — append-only value wrapper carrying observed/inferred/reviewed history.
//!
//! Priority rule: reviewed > inferred > observed > generated > planned > applied
//! (spec 002 data-model.md §ProvenancedValue).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ids::{EntityId, Timestamp};

/// Origin tag that classifies how a value was derived or confirmed.
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
#[serde(rename_all = "snake_case")]
pub enum ProvenanceTag {
    /// Extracted directly from a FITS/XISF keyword or filesystem attribute.
    Observed,
    /// Derived algorithmically (e.g. session key from frame metadata).
    Inferred,
    /// Confirmed or corrected by the user.
    Reviewed,
    /// Recomputed from source data (e.g. manifest regeneration).
    Generated,
    /// Set by a pending `FilesystemPlan` — cleared when plan resolves.
    Planned,
    /// Written after a `FilesystemPlan` reaches `applied`.
    Applied,
}

/// Priority score — lower number = higher priority.
fn priority(tag: ProvenanceTag) -> u8 {
    match tag {
        ProvenanceTag::Reviewed => 0,
        ProvenanceTag::Inferred => 1,
        ProvenanceTag::Observed => 2,
        ProvenanceTag::Generated => 3,
        ProvenanceTag::Planned => 4,
        ProvenanceTag::Applied => 5,
    }
}

/// One entry in the append-only provenance history.
///
/// `T` must satisfy both `JsonSchema` (for schemars) and `Type` (for specta).
/// The bounds are expressed via attribute overrides rather than inline generic
/// constraints to avoid conflicts when derive macros add independent `where`
/// clauses for each trait.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
#[schemars(bound = "T: JsonSchema")]
#[specta(bound = "T: Type")]
pub struct ProvenanceEntry<T> {
    /// The value that was in effect when this entry was captured.
    pub value: T,
    pub origin: ProvenanceTag,
    pub captured_at: Timestamp,
    /// Opaque reference to the originating source (file_record id, plan id, reviewer id, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<EntityId>,
    /// Optional pointer to the entry that superseded this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
}

/// Wrapper that carries a value with its full provenance trail.
///
/// `history` is append-only; mutation produces a new entry without erasing prior ones.
/// Inline history is bounded per origin tag; older entries spill to `provenance_history_archive`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
#[schemars(bound = "T: Clone + JsonSchema")]
#[specta(bound = "T: Clone + Type")]
pub struct ProvenancedValue<T> {
    /// Effective current value (priority: reviewed > inferred > observed > generated > planned > applied).
    pub current: T,
    /// The origin tag of the winning entry.
    pub origin: ProvenanceTag,
    /// Append-only history. Most recent N entries per origin, newest first.
    pub history: Vec<ProvenanceEntry<T>>,
    /// True when older entries exist in the `provenance_history_archive` table.
    #[serde(default)]
    pub history_truncated: bool,
}

impl<T: Clone + JsonSchema + Type> ProvenancedValue<T> {
    /// Construct with a single initial entry.
    #[must_use]
    pub fn new(value: T, origin: ProvenanceTag, captured_at: Timestamp) -> Self {
        let entry = ProvenanceEntry {
            value: value.clone(),
            origin,
            captured_at,
            source_id: None,
            replaced_by: None,
        };
        Self { current: value, origin, history: vec![entry], history_truncated: false }
    }

    /// Append a new entry and recompute the winning current value.
    pub fn push(&mut self, entry: ProvenanceEntry<T>) {
        // If the new entry has higher priority (lower score), promote it to current.
        if priority(entry.origin) <= priority(self.origin) {
            self.current = entry.value.clone();
            self.origin = entry.origin;
        }
        self.history.insert(0, entry); // newest first
    }
}
