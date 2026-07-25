// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Provenance hydration for spec 002 Phase 4.
//!
//! Reads `provenance_history_archive` rows for a given (entity_type, entity_id)
//! and folds them into per-field `ProvenancedValue<Value>` records.
//!
//! Priority rule for selecting `current` (matches
//! `domain_core::lifecycle::provenance::priority`):
//!   `reviewed > inferred > observed > generated > planned > applied`
//!
//! Per spec 002 amendment B-provenance-retention, inline `history` is capped at
//! `INLINE_HISTORY_LIMIT = 10` newest entries per field. When the table holds
//! more entries for a field, `history_truncated` is set to true on the
//! returned `ProvenancedValue`.

use std::collections::HashMap;

use domain_core::ids::{EntityId, Timestamp};

/// Tuple shape returned by the raw `provenance_history_archive` SELECT.
///
/// Ordered to match the column projection in `load_provenance`:
/// `id, asset_id, asset_type, field_path, origin, value, captured_at, source_id, replaced_by`.
/// `value` decodes via `sqlx::types::Json` at fetch time — a malformed cell
/// fails the whole query (propagates), matching the pre-migration `?` on
/// `serde_json::from_str` in `row_to_entry`.
type RawProvenanceTuple =
    (String, String, String, String, String, Json<Value>, String, Option<String>, Option<String>);
use domain_core::lifecycle::provenance::{ProvenanceEntry, ProvenanceTag, ProvenancedValue};
use serde_json::Value;
use sqlx::types::Json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use persistence_core::{DbError, DbResult};

/// Inline retention window. Older entries remain queryable from the archive
/// table but are not embedded in the returned `ProvenancedValue.history`.
pub const INLINE_HISTORY_LIMIT: usize = 10;

/// Raw row from `provenance_history_archive`. Decoded by hand because the
/// `sqlx::query!` macros require a compile-time DATABASE_URL.
#[derive(Clone, Debug)]
pub struct ProvenanceRow {
    pub id: String,
    pub entity_id: String,
    pub entity_type: String,
    pub field_path: String,
    /// Matches `ProvenanceTag` snake_case discriminant.
    pub origin: String,
    /// Already decoded via `sqlx::types::Json` in the SELECT tuple.
    pub value: Value,
    /// RFC 3339 timestamp.
    pub captured_at: String,
    pub source_id: Option<String>,
    pub superseded_by: Option<String>,
}

fn parse_tag(s: &str) -> Result<ProvenanceTag, DbError> {
    match s {
        "observed" => Ok(ProvenanceTag::Observed),
        "inferred" => Ok(ProvenanceTag::Inferred),
        "reviewed" => Ok(ProvenanceTag::Reviewed),
        "generated" => Ok(ProvenanceTag::Generated),
        "planned" => Ok(ProvenanceTag::Planned),
        "applied" => Ok(ProvenanceTag::Applied),
        other => Err(DbError::NotFound(format!("unknown provenance origin '{other}'"))),
    }
}

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

fn parse_timestamp(s: &str) -> Result<Timestamp, DbError> {
    let odt = OffsetDateTime::parse(s, &Rfc3339)
        .map_err(|e| DbError::NotFound(format!("bad rfc3339 '{s}': {e}")))?;
    Ok(Timestamp::from_offset_date_time(odt))
}

fn parse_entity_id(s: &str) -> Result<EntityId, DbError> {
    let uuid = Uuid::parse_str(s).map_err(|e| DbError::NotFound(format!("bad uuid '{s}': {e}")))?;
    Ok(EntityId::from_uuid(uuid))
}

/// Convert one raw row into a typed `ProvenanceEntry<Value>`.
fn row_to_entry(row: &ProvenanceRow) -> DbResult<ProvenanceEntry<Value>> {
    let origin = parse_tag(&row.origin)?;
    let captured_at = parse_timestamp(&row.captured_at)?;
    let source_id = row.source_id.as_deref().map(parse_entity_id).transpose()?;
    Ok(ProvenanceEntry {
        value: row.value.clone(),
        origin,
        captured_at,
        source_id,
        replaced_by: row.superseded_by.clone(),
    })
}

/// Load provenance entries for the given asset and fold them into per-field
/// `ProvenancedValue<Value>` records.
///
/// Returns a map keyed by `field_path`. Each value carries the winning
/// `current` value selected by priority (with `superseded_by IS NULL`
/// preferred when present) and at most `INLINE_HISTORY_LIMIT` newest entries
/// in `history`.
///
/// The boolean second return slot is `true` when at least one field had more
/// than `INLINE_HISTORY_LIMIT` entries in the archive.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` for SQL failures (including a stored `value`
/// that cannot be parsed as JSON — decoded via `sqlx::types::Json` at fetch
/// time) and [`DbError::NotFound`] when stored discriminants/UUIDs/timestamps
/// cannot be decoded.
pub async fn load_provenance(
    pool: &sqlx::SqlitePool,
    entity_id: EntityId,
    entity_type_str: &str,
) -> DbResult<(HashMap<String, ProvenancedValue<Value>>, bool)> {
    let id_str = entity_id.as_uuid().to_string();
    // Ordered oldest -> newest so that grouping below preserves chronology.
    let raw: Vec<RawProvenanceTuple> = sqlx::query_as(
        "SELECT id, asset_id, asset_type, field_path, origin, value, captured_at, source_id, replaced_by \
         FROM provenance_history_archive \
         WHERE asset_id = ? AND asset_type = ? \
         ORDER BY field_path ASC, captured_at ASC",
    )
    .bind(&id_str)
    .bind(entity_type_str)
    .fetch_all(pool)
    .await?;

    let rows: Vec<ProvenanceRow> = raw
        .into_iter()
        .map(
            |(
                id,
                entity_id,
                entity_type,
                field_path,
                origin,
                Json(value),
                captured_at,
                source_id,
                superseded_by,
            )| {
                ProvenanceRow {
                    id,
                    entity_id,
                    entity_type,
                    field_path,
                    origin,
                    value,
                    captured_at,
                    source_id,
                    superseded_by,
                }
            },
        )
        .collect();

    // Group by field_path. Preserves the SQL ordering (oldest first).
    let mut grouped: HashMap<String, Vec<ProvenanceRow>> = HashMap::new();
    for row in rows {
        grouped.entry(row.field_path.clone()).or_default().push(row);
    }

    let mut any_truncated = false;
    let mut out: HashMap<String, ProvenancedValue<Value>> = HashMap::new();

    for (field_path, rows) in grouped {
        let total = rows.len();
        let truncated = total > INLINE_HISTORY_LIMIT;
        if truncated {
            any_truncated = true;
        }

        // Decode every row into a ProvenanceEntry once.
        let mut entries: Vec<ProvenanceEntry<Value>> =
            rows.iter().map(row_to_entry).collect::<DbResult<Vec<_>>>()?;

        // Resolution rule:
        //   1. Prefer entries whose `replaced_by` is NULL (i.e. not superseded
        //      by another archive row). This honours the explicit pointer when
        //      writers maintain it.
        //   2. Among the candidate set, pick by priority (lower = higher).
        //   3. Tie-break on captured_at (newest wins).
        //
        // If every entry is superseded (defensive fallback), reuse the full
        // set under the same priority rule.
        let unsuperseded: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.superseded_by.is_none())
            .map(|(i, _)| i)
            .collect();
        let candidate_indices: Vec<usize> =
            if unsuperseded.is_empty() { (0..entries.len()).collect() } else { unsuperseded };

        let pick_index = candidate_indices
            .into_iter()
            .min_by(|&a, &b| {
                let ea = &entries[a];
                let eb = &entries[b];
                priority(ea.origin).cmp(&priority(eb.origin)).then_with(|| {
                    // newest wins on priority tie
                    eb.captured_at.as_offset_date_time().cmp(&ea.captured_at.as_offset_date_time())
                })
            })
            .ok_or_else(|| {
                DbError::NotFound(format!("no provenance entries for field {field_path}"))
            })?;

        let current_value = entries[pick_index].value.clone();
        let current_origin = entries[pick_index].origin;

        // Build inline history: newest first, capped at INLINE_HISTORY_LIMIT.
        entries.reverse(); // now newest -> oldest
        if entries.len() > INLINE_HISTORY_LIMIT {
            entries.truncate(INLINE_HISTORY_LIMIT);
        }

        out.insert(
            field_path,
            ProvenancedValue {
                current: current_value,
                origin: current_origin,
                history: entries,
                history_truncated: truncated,
            },
        );
    }

    Ok((out, any_truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_order_matches_domain() {
        // reviewed beats every other tag.
        assert!(priority(ProvenanceTag::Reviewed) < priority(ProvenanceTag::Inferred));
        assert!(priority(ProvenanceTag::Inferred) < priority(ProvenanceTag::Observed));
        assert!(priority(ProvenanceTag::Observed) < priority(ProvenanceTag::Generated));
        assert!(priority(ProvenanceTag::Generated) < priority(ProvenanceTag::Planned));
        assert!(priority(ProvenanceTag::Planned) < priority(ProvenanceTag::Applied));
    }

    #[test]
    fn parse_tag_round_trip() {
        for (s, t) in [
            ("observed", ProvenanceTag::Observed),
            ("inferred", ProvenanceTag::Inferred),
            ("reviewed", ProvenanceTag::Reviewed),
            ("generated", ProvenanceTag::Generated),
            ("planned", ProvenanceTag::Planned),
            ("applied", ProvenanceTag::Applied),
        ] {
            assert_eq!(parse_tag(s).unwrap(), t);
        }
        assert!(parse_tag("bogus").is_err());
    }
}
