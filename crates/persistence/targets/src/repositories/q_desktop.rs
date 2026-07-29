// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Desktop Tauri command repository (db-boundary-zero drain).
//!
//! Backs `apps/desktop/src-tauri/src/commands/inbox.rs`,
//! `commands/status.rs`, `commands/target_lookup.rs`, and the spec 035 US4
//! ingest-resolution drain in `lib.rs`. Free functions only, mirroring the
//! `inventory`/`targets` repository idiom (no repo struct/trait).

use sqlx::{Executor, Sqlite, SqlitePool};

use persistence_core::DbResult;

// ── inbox.scan.folder support (spec 041 T065) ──────────────────────────────────

/// Insert (or reuse, via `INSERT OR IGNORE`) the individual `inbox_items` row
/// for a single detected calibration master.
///
/// Accepts any [`sqlx::Executor`] so callers can pass either `&SqlitePool`
/// (auto-commit) or `&mut Transaction` (batched commit).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
#[allow(clippy::too_many_arguments)]
pub async fn insert_inbox_master_item<'e, E>(
    executor: E,
    id: &str,
    root_id: &str,
    relative_path: &str,
    lane: &str,
    format: &str,
    frame_type: &str,
    filter: Option<&str>,
    exposure_s: Option<f64>,
) -> DbResult<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        "INSERT OR IGNORE INTO inbox_items
            (id, root_id, relative_path, file_count, discovered_at, last_scanned_at,
             content_signature, state, lane, format, is_master_item,
             master_frame_type, master_filter, master_exposure_s)
         VALUES (?, ?, ?, 1, datetime('now'), datetime('now'), '', 'pending_classification',
                 ?, ?, 1, ?, ?, ?)",
    )
    .bind(id)
    .bind(root_id)
    .bind(relative_path)
    .bind(lane)
    .bind(format)
    .bind(frame_type)
    .bind(filter)
    .bind(exposure_s)
    .execute(executor)
    .await?;
    Ok(())
}

/// Individual master-item `inbox_items` row shape, scoped by `(root_id,
/// relative_path)`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InboxMasterItemRow {
    pub id: String,
    pub state: String,
    pub file_count: i64,
    pub lane: String,
    pub content_signature: Option<String>,
    pub is_master_item: i64,
    pub master_frame_type: Option<String>,
    pub master_filter: Option<String>,
    pub master_exposure_s: Option<f64>,
}

/// Fetch the authoritative `inbox_items` row for an individual master file
/// (may have existed from a prior scan).
///
/// Accepts any [`sqlx::Executor`] so callers can read within the same
/// transaction that wrote the row.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn get_inbox_master_item_row<'e, E>(
    executor: E,
    root_id: &str,
    relative_path: &str,
) -> DbResult<Option<InboxMasterItemRow>>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query_as::<_, InboxMasterItemRow>(
        "SELECT id, state, file_count, lane, content_signature,
                is_master_item, master_frame_type, master_filter, master_exposure_s
         FROM inbox_items WHERE root_id = ? AND relative_path = ?",
    )
    .bind(root_id)
    .bind(relative_path)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

// ── status.summary (spec 030 T023) ──────────────────────────────────────────────

/// Count unacknowledged inbox items (`pending_classification` or
/// `classified`) across all registered roots.
///
/// Journey J02 requires this badge to match the queue's real contents. It used
/// to need a superseded-placeholder exclusion to manage that: a split folder
/// was counted once per sub-item *plus* once for its placeholder, so the badge
/// read higher than the visible queue.
///
/// Spec 058 T012/T024 removed the need rather than the symptom. Scan no longer
/// writes a placeholder, so there is no row for this count and the queue list
/// to disagree about, and both now use the same plain state predicate with no
/// suppression on either side (FR-026, SC-007).
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn count_unacknowledged_inbox_items(pool: &SqlitePool) -> DbResult<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM inbox_items i
         JOIN registered_sources r ON r.id = i.root_id
         WHERE i.state IN ('pending_classification', 'classified')",
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Count all `acquisition_session` rows.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn count_acquisition_sessions(pool: &SqlitePool) -> DbResult<i64> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM acquisition_session").fetch_one(pool).await?;
    Ok(count)
}

/// Count all rows in the `calibration_master_view` projection.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn count_calibration_masters(pool: &SqlitePool) -> DbResult<i64> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM calibration_master_view").fetch_one(pool).await?;
    Ok(count)
}

/// Count all `canonical_target` rows.
pub use super::q_resolver::count_canonical_targets;

/// Count all `projects` rows.
///
/// # Errors
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn count_projects(pool: &SqlitePool) -> DbResult<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects").fetch_one(pool).await?;
    Ok(count)
}

// ── resolver_settings read (spec 035 target.resolve + US4 ingest drain) ───────

/// Singleton `resolver_settings` row (id = 1): online toggle, SIMBAD
/// endpoint, and request timeout, read by both `target.resolve` and the
/// background ingest-resolution drain.
///
/// Same query as [`super::q_targets_mgmt::ResolverSettingsOnlineRow`] /
/// `get_resolver_settings_online`; re-exported here under this module's
/// established name so `target_lookup.rs`/`lib.rs` are unaffected.
pub use super::q_targets_mgmt::ResolverSettingsOnlineRow as ResolverSettingsRow;

/// Fetch the singleton `resolver_settings` row (id = 1).
pub use super::q_targets_mgmt::get_resolver_settings_online as get_resolver_settings;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::test_support::setup_db;

    #[tokio::test]
    async fn status_counts_are_zero_on_fresh_db() {
        let db = setup_db().await;
        assert_eq!(count_unacknowledged_inbox_items(db.pool()).await.unwrap(), 0);
        assert_eq!(count_acquisition_sessions(db.pool()).await.unwrap(), 0);
        assert_eq!(count_calibration_masters(db.pool()).await.unwrap(), 0);
        assert_eq!(count_canonical_targets(db.pool()).await.unwrap(), 0);
        assert_eq!(count_projects(db.pool()).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn resolver_settings_seeded_by_migration_0031() {
        let db = setup_db().await;
        let row = get_resolver_settings(db.pool()).await.unwrap().expect("seeded singleton row");
        assert_eq!(row.online_enabled, 1, "default online_enabled per migration 0031");
        assert_eq!(row.request_timeout_secs, 10, "default request_timeout_secs");
    }

    /// Registers a real `registered_sources` row via `first_run::register_source_batch`
    /// (same path the desktop scan/inbox code uses) — `inbox_items.root_id` is a
    /// real FK, and `INSERT OR IGNORE` silently drops rows on FK violation.
    async fn register_test_root(pool: &sqlx::SqlitePool) -> String {
        use domain_core::first_run::{
            OrganizationState, RegisterSourceBatchRequest, RegisterSourceRequest, ScanDepth,
            SourceKind,
        };
        let batch_req = RegisterSourceBatchRequest {
            sources: vec![RegisterSourceRequest {
                kind: SourceKind::Inbox,
                path: "/astro/inbox".to_owned(),
                kind_subtype: None,
                scan_depth: ScanDepth::Recursive,
                organization_state: OrganizationState::Unorganized,
            }],
        };
        let batch_resp =
            persistence_lifecycle::repositories::first_run::register_source_batch(pool, &batch_req)
                .await
                .unwrap();
        batch_resp.items[0].source_id.as_deref().unwrap().to_owned()
    }

    #[tokio::test]
    async fn inbox_master_item_round_trips() {
        let db = setup_db().await;
        let root_id = register_test_root(db.pool()).await;
        insert_inbox_master_item(
            db.pool(),
            "master-1",
            &root_id,
            "2026-01-01/Flats/flat_L_001.fits",
            "fits",
            "fits",
            "flat",
            Some("L"),
            Some(2.5),
        )
        .await
        .unwrap();

        let row =
            get_inbox_master_item_row(db.pool(), &root_id, "2026-01-01/Flats/flat_L_001.fits")
                .await
                .unwrap()
                .expect("row inserted");
        assert_eq!(row.id, "master-1");
        assert_eq!(row.file_count, 1);
        assert_eq!(row.is_master_item, 1);
        assert_eq!(row.master_frame_type.as_deref(), Some("flat"));
        assert_eq!(row.master_filter.as_deref(), Some("L"));
        assert_eq!(row.master_exposure_s, Some(2.5));
    }
}
