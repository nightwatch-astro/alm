// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical `(entity_type, from, to) → action_critical_fields[]` table
//! (spec 002 FR-009/FR-010, T050).
//!
//! Authored from data-model.md §Action-Bound Review. The transition use case
//! looks up this table on every edge attempt; for each listed field path it
//! fetches the field's provenance origin and refuses the transition with
//! `provenance.unreviewed` when any required field is not yet `reviewed`.
//!
//! Field-level review state is derived from each field's
//! `ProvenancedValue::origin` — it is NOT a per-entity column. The
//! repository surface for that read is
//! `LifecycleRepository::field_origins`.
//!
//! Spec 041 FR-051 (T076, Phase 13): the only cells ever populated here gated
//! the `AcquisitionSession`/`InventorySession` `candidate → confirmed` and
//! `needs_review → confirmed` edges (confirming a session required
//! `observer_location` to be reviewed). Those edges — and the entity types
//! that carried them — no longer exist: sessions are derived,
//! already-confirmed inventory with no review transition. [`TABLE`] is
//! therefore empty; the mechanism remains in place for a future entity
//! family that needs action-bound review.

use crate::lifecycle::data_asset::EntityType;

/// A dotted field path on a Data Asset (matches `provenance.field_path`).
pub type FieldPath = &'static str;

/// One row in the action-bound review table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionReviewEdge {
    pub entity_type: EntityType,
    pub from: &'static str,
    pub to: &'static str,
    /// Field paths whose `ProvenancedValue.origin` MUST be `reviewed`
    /// before the edge may be applied.
    pub critical_fields: &'static [FieldPath],
}

/// Canonical rule table.
///
/// See data-model.md §Action-Bound Review for the authoritative table.
/// Empty since spec 041 FR-051 removed the session review-state edges that
/// were previously the table's only rows.
pub const TABLE: &[ActionReviewEdge] = &[];

/// Return the field paths whose provenance origin MUST be `reviewed` before
/// the `(entity_type, from, to)` edge may be applied.
///
/// Returns an empty slice for every edge not present in [`TABLE`].
#[must_use]
pub fn action_critical_fields(
    entity_type: EntityType,
    from: &str,
    to: &str,
) -> &'static [FieldPath] {
    TABLE
        .iter()
        .find(|edge| edge.entity_type == entity_type && edge.from == from && edge.to == to)
        .map_or(&[][..], |edge| edge.critical_fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_edge_returns_empty() {
        let fields = action_critical_fields(EntityType::Project, "ready", "processing");
        assert!(fields.is_empty());
    }
}
