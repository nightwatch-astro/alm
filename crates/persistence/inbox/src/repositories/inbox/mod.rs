// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for Inbox items, classifications, evidence, and plan links
//! (spec 005, migration 0020).
//!
//! All state-machine enforcement lives in `crates/app/core/src/inbox/`.
//!
//! Split by owning table (#968): [`items`] is `inbox_items`; [`source_groups`]
//! is `inbox_source_groups`; [`classification`] is `inbox_classifications`,
//! `inbox_classification_evidence`, and `inbox_file_overrides`; [`metadata`] is
//! `inbox_file_metadata` and `inbox_classification_breakdown`; [`plan_links`]
//! is `inbox_plan_links`; [`projections`] is the read-only cross-table list,
//! stats, and grouping-key queries.

pub mod classification;
pub mod items;
pub mod metadata;
pub mod plan_links;
pub mod projections;
pub mod source_groups;

#[cfg(test)]
mod tests;

pub use classification::{
    delete_evidence_for_item_conn, delete_evidence_for_items, get_classification, insert_evidence,
    insert_evidence_batch, insert_evidence_conn, list_evidence, list_file_overrides_for_group,
    mark_file_override_stale, mark_override_stale, set_file_override, set_overrides,
    upsert_classification, upsert_classification_batch, upsert_classification_conn,
    FileOverrideRow, InboxClassificationRow, InboxEvidenceRow, InsertEvidence,
    UpsertClassification, UpsertClassificationRow,
};
pub use items::{
    delete_sub_item_if_unlinked_conn, get_inbox_item, get_source_group_id_for_item,
    insert_inbox_item, list_inbox_sub_items, list_inbox_sub_items_conn,
    list_item_ids_for_source_group, reset_inbox_item_to_unconfirmed, update_inbox_item_scan_conn,
    update_inbox_item_state, update_inbox_item_state_conn, upsert_inbox_sub_item,
    upsert_inbox_sub_item_conn, InboxItemRow, InsertInboxItem, UpsertInboxSubItem,
};
pub use metadata::{
    delete_breakdown_for_item_conn, delete_breakdown_for_items, delete_file_metadata_for_item_conn,
    delete_file_metadata_for_items, get_file_metadata, list_breakdown,
    list_inbox_attribution_geometry, list_inbox_file_metadata, list_inbox_pointing,
    upsert_breakdown_row, upsert_breakdown_row_conn, upsert_inbox_file_metadata,
    upsert_inbox_file_metadata_batch, upsert_inbox_file_metadata_conn, InboxAttributionGeometryRow,
    InboxBreakdownRow, InboxFileMetadataRow, InboxPointingRow, UpsertFileMetadata,
};
pub use plan_links::{
    delete_plan_link, find_orphaned_plan_links, get_plan_link, get_plan_link_by_plan_id,
    insert_plan_link, InboxPlanLinkRow,
};
pub use projections::{
    count_distinct_inbox_folders, format_exposure_label, grouping_keys_for_items, inbox_stats,
    list_unacknowledged_across_roots, InboxItemGroupingKeys, InboxListRow, InboxStatsRow,
};
pub use source_groups::{
    last_scanned_by_root, list_unclassified_source_groups, update_source_group_child_count_conn,
    upsert_inbox_source_group, InboxSourceGroupListRow, InboxSourceGroupRow, UpsertSourceGroup,
};
