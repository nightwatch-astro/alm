// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Optic-train composite identity (Q12/Q17 grouping key).
//!
//! The single canonical implementation of the `telescop|instrume|focal_length`
//! composite, shared by every consumer that must agree on the same
//! `optic_train_key` string for the same physical equipment: the F-Framing-5
//! Inbox-confirm attribution pass (matching against *staged*, non-durable
//! metadata) and `app_core_targets::ingest_sessions` (writing the *durable*
//! `acquisition_session.optic_train_key` column at apply completion). A
//! divergent implementation in either place would let attribution suggest a
//! framing match that the durable session then fails to actually key under —
//! silently breaking FR-019's optic-train prefilter.
//!
//! `crates/app/inbox::grouping::engine::optic_train` computes a structurally
//! identical key for the persisted `inbox_item.group_key` column (spec 041
//! T064 / FR-039). That key is a DIFFERENT artifact: it uses the grouping
//! engine's `SENTINEL_MISSING` (`"∅"`) for absent parts so its output is
//! consistent with every other grouping dimension stored in the same column.
//! This module uses `"-"`. The two are NOT interchangeable: changing either
//! sentinel would re-group existing `inbox_item` rows, which are keyed by
//! the `(root_id, relative_path, group_key)` UNIQUE constraint. The grouping
//! key must stay independent of the framing-identity key.

/// Sentinel for a present-but-normalized-empty part (mirrors
/// `app_core_inbox::grouping::SENTINEL_MISSING`'s role, kept private here
/// since this module has no other consumer of the sentinel string itself).
const SENTINEL_MISSING: &str = "-";

/// Optic-train composite = `telescop|instrume|focal_length_mm`. Built only
/// from present parts; entirely absent parts render the sentinel. Each text
/// part is normalized (trimmed, whitespace-collapsed, case-folded); focal
/// length is bucketed to whole mm so float noise doesn't fork the key.
/// Returns `None` only when all three inputs are absent.
#[must_use]
pub fn optic_train_key(
    telescop: Option<&str>,
    instrume: Option<&str>,
    focal_length_mm: Option<f64>,
) -> Option<String> {
    let tel = normalize_text(telescop);
    let inst = normalize_text(instrume);
    let fl = focal_length_mm.map(|f| format_focal_length(f.round()));
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

pub(crate) fn normalize_text(value: Option<&str>) -> Option<String> {
    let v = value?.trim();
    if v.is_empty() {
        return None;
    }
    Some(v.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase())
}

/// Whole-mm focal length rendered without a decimal point (bucketing already
/// rounded it — this only avoids a trailing `.0`).
#[allow(clippy::cast_possible_truncation)]
fn format_focal_length(value: f64) -> String {
    (value as i64).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_of_all_three_parts() {
        assert_eq!(
            optic_train_key(Some("RASA 8"), Some("ASI2600MM"), Some(400.0)).as_deref(),
            Some("rasa 8|asi2600mm|400")
        );
    }

    #[test]
    fn entirely_absent_is_none() {
        assert_eq!(optic_train_key(None, None, None), None);
    }

    #[test]
    fn partial_data_uses_sentinel_for_missing_parts() {
        assert_eq!(optic_train_key(Some("RASA 8"), None, None).as_deref(), Some("rasa 8|-|-"));
    }

    #[test]
    fn focal_length_bucketed_to_whole_mm() {
        assert_eq!(
            optic_train_key(None, None, Some(400.4)).as_deref(),
            optic_train_key(None, None, Some(399.6)).as_deref(),
        );
    }

    #[test]
    fn whitespace_and_case_are_normalized() {
        assert_eq!(
            optic_train_key(Some("  RASA   8 "), Some("asi2600mm"), None).as_deref(),
            Some("rasa 8|asi2600mm|-")
        );
    }
}
