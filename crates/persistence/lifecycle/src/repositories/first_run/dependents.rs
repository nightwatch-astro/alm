// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Root dependency queries: `roots.remap` preview sampling and the
//! `roots.delete` dependents guard.

use sqlx::SqlitePool;

use persistence_core::DbResult;

/// Fetch every relative path previously recorded for a root, for exhaustive
/// `roots.remap` preview verification (issue #560).
///
/// Reads BOTH `file_record` (light frames already ingested through inbox
/// plan-apply — see `app_targets::ingest_sessions`) AND `inbox_items` (items
/// scanned into the Inbox but not yet ingested — these live under the
/// *inbox* root's own id, which never gets `file_record` rows, since ingest
/// writes `file_record` against the *destination* root instead). A root that
/// was only ever scanned into the Inbox previously sampled as empty here,
/// which made a remap preview vacuously report "all verified" with nothing
/// actually checked.
///
/// `inbox_items` rows in the terminal `resolved` state are excluded: those
/// items were already moved out of this root by a prior plan-apply, so their
/// `relative_path` legitimately no longer exists at the root's original
/// location regardless of any remap — checking them would produce false
/// "not found" results unrelated to the remap itself.
///
/// No `LIMIT` — a prior 5-path sample let files outside the sample go
/// unverified; correctness (this gates a destructive-adjacent path swap) is
/// prioritised over the sample being lighter to render.
///
/// Deduplicated (`UNION`) and ordered for determinism. Roots with no rows in
/// either table (calibration/project roots, or raw roots registered directly
/// without ever receiving an inbox scan) yield an empty result — there is
/// nothing recorded to verify.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn relative_paths_for_root(pool: &SqlitePool, root_id: &str) -> DbResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT relative_path FROM file_record WHERE root_id = ? \
         UNION \
         SELECT relative_path FROM inbox_items WHERE root_id = ? AND state != 'resolved' \
         ORDER BY relative_path ASC",
    )
    .bind(root_id)
    .bind(root_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(p,)| p).collect())
}

/// Count dependent records referencing a root, for the `roots.delete`
/// dependents-guard (P6b, decision D8: block rather than cascade-nullify).
///
/// `registered_sources` has no FK cascade, so every table that stores a root
/// id must be checked explicitly: `inbox_items.root_id`, `plan_items.source_id`,
/// `file_record.root_id`, `acquisition_session.root_id`, and
/// `calibration_session.root_id`.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn count_root_dependents(
    pool: &SqlitePool,
    root_id: &str,
) -> DbResult<domain_core::first_run::RootDependencyCounts> {
    fn to_u32(count: i64) -> u32 {
        u32::try_from(count.max(0)).unwrap_or(u32::MAX)
    }

    let inbox_items: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM inbox_items WHERE root_id = ?")
        .bind(root_id)
        .fetch_one(pool)
        .await?;
    let plan_items: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM plan_items WHERE source_id = ?")
        .bind(root_id)
        .fetch_one(pool)
        .await?;
    let file_records: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM file_record WHERE root_id = ?")
        .bind(root_id)
        .fetch_one(pool)
        .await?;
    let acquisition_sessions: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM acquisition_session WHERE root_id = ?")
            .bind(root_id)
            .fetch_one(pool)
            .await?;
    let calibration_sessions: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM calibration_session WHERE root_id = ?")
            .bind(root_id)
            .fetch_one(pool)
            .await?;

    Ok(domain_core::first_run::RootDependencyCounts {
        inbox_items: to_u32(inbox_items.0),
        plan_items: to_u32(plan_items.0),
        file_records: to_u32(file_records.0),
        acquisition_sessions: to_u32(acquisition_sessions.0),
        calibration_sessions: to_u32(calibration_sessions.0),
    })
}
