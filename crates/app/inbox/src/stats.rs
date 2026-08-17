// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `inbox.stats` use case (spec 041 US6/FR-021, T038).
//!
//! Returns aggregate per-frame-type counts across all unacknowledged inbox
//! items. Thin orchestration: all SQL lives in
//! `persistence_inbox::repositories::inbox::inbox_stats`.
#![allow(clippy::doc_markdown)]

use persistence_inbox::repositories::inbox as repo;
use sqlx::SqlitePool;

use app_core_errors::db_internal_ctx;
use contracts_core::inbox::{InboxStatsPerType, InboxStatsResponse, InboxStatsTotals};
use contracts_core::ContractError;

/// Return aggregate stats across all unacknowledged inbox items.
///
/// # Errors
/// Returns `internal.database` on query failure.
pub async fn inbox_stats(pool: &SqlitePool) -> Result<InboxStatsResponse, ContractError> {
    let rows = repo::inbox_stats(pool).await.map_err(|e| db_internal_ctx(e, "inbox stats"))?;

    // Masters and images partition cleanly by frame type, so summing the
    // per-type buckets is correct. Folders do NOT: a mixed light+dark folder
    // appears in both per-type rows, so summing folder_count would double-count
    // it. Use a distinct count for the folder total instead.
    let total_folders = u32::try_from(
        repo::count_distinct_inbox_folders(pool)
            .await
            .map_err(|e| db_internal_ctx(e, "count distinct inbox folders"))?,
    )
    .unwrap_or(u32::MAX);
    let mut total_masters: u32 = 0;
    let mut total_images: u32 = 0;

    let per_type: Vec<InboxStatsPerType> = rows
        .into_iter()
        .map(|r| {
            let folder_count = u32::try_from(r.folder_count).unwrap_or(u32::MAX);
            let master_count = u32::try_from(r.master_count).unwrap_or(u32::MAX);
            let image_count = u32::try_from(r.image_count).unwrap_or(u32::MAX);
            total_masters = total_masters.saturating_add(master_count);
            total_images = total_images.saturating_add(image_count);
            InboxStatsPerType { frame_type: r.frame_type, folder_count, master_count, image_count }
        })
        .collect();

    Ok(InboxStatsResponse {
        per_type,
        totals: InboxStatsTotals {
            folders: total_folders,
            masters: total_masters,
            images: total_images,
        },
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_inbox::repositories::inbox::{InsertEvidence, InsertInboxItem};

    async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    /// Insert one inbox item with two evidence rows and check stats maps them.
    #[tokio::test]
    async fn stats_returns_per_type_counts() {
        let db = test_db().await;
        let pool = db.pool();

        // Item: folder with two light files.
        repo::insert_inbox_item(
            pool,
            &InsertInboxItem {
                id: "item-stats-1",
                root_id: "root-1",
                relative_path: "stats/lights",
                file_count: 2,
                content_signature: Some("sig-s1"),
                lane: "fits",
            },
        )
        .await
        .unwrap();

        // Update state to 'classified' so it is unacknowledged.
        repo::update_inbox_item_state(pool, "item-stats-1", "classified").await.unwrap();

        repo::insert_evidence(
            pool,
            &InsertEvidence {
                id: "ev-stats-1a",
                inbox_item_id: "item-stats-1",
                relative_file_path: "stats/lights/light_001.fits",
                frame_type: Some("light"),
                evidence_source: "imagetyp_header",
                raw_value: Some("Light Frame"),
                unclassified: false,
                manual_override: None,
                is_master: false,
                master_detector: None,
            },
        )
        .await
        .unwrap();

        repo::insert_evidence(
            pool,
            &InsertEvidence {
                id: "ev-stats-1b",
                inbox_item_id: "item-stats-1",
                relative_file_path: "stats/lights/light_002.fits",
                frame_type: Some("light"),
                evidence_source: "imagetyp_header",
                raw_value: Some("Light Frame"),
                unclassified: false,
                manual_override: None,
                is_master: false,
                master_detector: None,
            },
        )
        .await
        .unwrap();

        let resp = inbox_stats(pool).await.unwrap();

        assert_eq!(resp.per_type.len(), 1, "one frame type (light)");
        let light = &resp.per_type[0];
        assert_eq!(light.frame_type, "light");
        assert_eq!(light.folder_count, 1, "one folder item");
        assert_eq!(light.master_count, 0, "no masters");
        assert_eq!(light.image_count, 2, "two image files");

        assert_eq!(resp.totals.folders, 1);
        assert_eq!(resp.totals.masters, 0);
        assert_eq!(resp.totals.images, 2);
    }

    /// A folder containing more than one frame type appears in each per-type
    /// row, but `totals.folders` must count it ONCE (distinct), not sum the
    /// per-type buckets. Regression guard for double-counting.
    #[tokio::test]
    async fn stats_totals_folders_distinct_for_mixed_type_folder() {
        let db = test_db().await;
        let pool = db.pool();

        repo::insert_inbox_item(
            pool,
            &InsertInboxItem {
                id: "item-mixed",
                root_id: "root-1",
                relative_path: "stats/mixed",
                file_count: 2,
                content_signature: Some("sig-mixed"),
                lane: "fits",
            },
        )
        .await
        .unwrap();
        repo::update_inbox_item_state(pool, "item-mixed", "classified").await.unwrap();

        for (id, fname, ft, raw) in [
            ("ev-mx-l", "stats/mixed/light_001.fits", "light", "Light Frame"),
            ("ev-mx-d", "stats/mixed/dark_001.fits", "dark", "Dark Frame"),
        ] {
            repo::insert_evidence(
                pool,
                &InsertEvidence {
                    id,
                    inbox_item_id: "item-mixed",
                    relative_file_path: fname,
                    frame_type: Some(ft),
                    evidence_source: "imagetyp_header",
                    raw_value: Some(raw),
                    unclassified: false,
                    manual_override: None,
                    is_master: false,
                    master_detector: None,
                },
            )
            .await
            .unwrap();
        }

        let resp = inbox_stats(pool).await.unwrap();

        assert_eq!(resp.per_type.len(), 2, "light + dark rows");
        for row in &resp.per_type {
            assert_eq!(row.folder_count, 1, "the single folder appears in each type row");
        }
        // The folder is ONE distinct folder despite two type rows.
        assert_eq!(resp.totals.folders, 1, "mixed-type folder must not be double-counted");
        assert_eq!(resp.totals.images, 2);
    }
}
