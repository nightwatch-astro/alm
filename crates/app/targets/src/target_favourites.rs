// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `targets.favourites.list` / `targets.favourites.add` / `targets.favourites.remove`
//! use cases (spec 051 US2).
//!
//! Thin orchestration over the `target_favourite` repository functions
//! (`persistence_targets::repositories::target_favourites`): promotes the target
//! from the shared redb resolve cache into `canonical_target` if it is not
//! already durable (spec 052 P1 FR-004 — favouriting is an in-use commit),
//! then maps repository results to contract DTOs.
//!
//! ## Constitution
//!
//! - §I No filesystem mutations: read/write SQLite metadata only.
//! - §V SQLite (`target_favourite`) is the durable record.

use sqlx::SqlitePool;
use uuid::Uuid;

use contracts_core::targets::{
    TargetFavouriteAddResult, TargetFavouriteRemoveResult, TargetFavouriteRequest,
    TargetFavouritesListResult,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::Timestamp;
use persistence_targets::repositories::target_favourites;

use app_core_errors::db_err;

fn not_found(id: &str) -> ContractError {
    ContractError::new(
        ErrorCode::TargetNotFound,
        format!("Target '{id}' not found."),
        ErrorSeverity::Blocking,
        false,
    )
}

/// `targets.favourites.list` — return the ids of every currently-favourited
/// canonical target.
///
/// # Errors
///
/// Returns [`ContractError`] with code `internal.database`.
pub async fn list(pool: &SqlitePool) -> Result<TargetFavouritesListResult, ContractError> {
    let target_ids = target_favourites::list_favourites(pool).await.map_err(db_err)?;
    Ok(TargetFavouritesListResult { target_ids })
}

/// `targets.favourites.add` — favourite a canonical target. Idempotent: adding
/// an already-favourited target succeeds with no error and does not reset
/// `favouritedAt`.
///
/// Promotes the target from the shared redb resolve cache into
/// `canonical_target` first (spec 052 P1 FR-004: favouriting is an in-use
/// commit) — `target_id` is a UUID a prior `target.search`/`target.resolve`
/// response returned, which may not be durable yet.
///
/// # Errors
///
/// Returns [`ContractError`] with code `target.not_found` (the id is not a
/// valid UUID, or is unknown to both `canonical_target` and the redb cache)
/// or `internal.database`.
pub async fn add(
    pool: &SqlitePool,
    redb_cache: &dyn simbad_resolver::Cache,
    req: &TargetFavouriteRequest,
) -> Result<TargetFavouriteAddResult, ContractError> {
    let uuid = Uuid::parse_str(&req.target_id).map_err(|_| not_found(&req.target_id))?;
    let promoted =
        crate::target_resolve::promote_by_id(pool, redb_cache, uuid, "targets.favourites.add")
            .await?;
    if !promoted {
        return Err(not_found(&req.target_id));
    }

    let favourited_at = Timestamp::now_iso();
    target_favourites::add_favourite(pool, &req.target_id, &favourited_at).await.map_err(db_err)?;

    // Re-read the stored favourited_at: a repeat add is a no-op and must
    // reflect the *original* timestamp, not the one just computed above.
    let stored_at = target_favourites::get_favourited_at(pool, &req.target_id)
        .await
        .map_err(db_err)?
        .unwrap_or(favourited_at);

    Ok(TargetFavouriteAddResult { target_id: req.target_id.clone(), favourited_at: stored_at })
}

/// `targets.favourites.remove` — unfavourite a canonical target. Idempotent:
/// removing a target that was never favourited succeeds with no error.
///
/// # Errors
///
/// Returns [`ContractError`] with code `internal.database`.
pub async fn remove(
    pool: &SqlitePool,
    req: &TargetFavouriteRequest,
) -> Result<TargetFavouriteRemoveResult, ContractError> {
    target_favourites::remove_favourite(pool, &req.target_id).await.map_err(db_err)?;
    Ok(TargetFavouriteRemoveResult { target_id: req.target_id.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;
    use simbad_resolver::{Cache as _, Store};

    async fn setup() -> Database {
        let db = Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db
    }

    /// A durable `canonical_target` row already promoted (mirrors a target
    /// that was already added-to-project/session-linked before favouriting)
    /// — `promote_by_id`'s fast path finds it without touching the cache.
    async fn insert_target(pool: &SqlitePool, id: Uuid) {
        sqlx::query(
            "INSERT INTO canonical_target
             (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at)
             VALUES (?, NULL, 'Test Target', 'galaxy', 10.0, 20.0, 'seed', '2026-01-01T00:00:00Z')",
        )
        .bind(id.to_string())
        .execute(pool)
        .await
        .expect("insert_target failed");
    }

    /// An empty redb cache — `add`'s promotion has nothing to fall back to
    /// beyond an already-durable row (see [`insert_target`]).
    fn empty_cache() -> simbad_resolver::RedbCache {
        Store::in_memory().unwrap().cache()
    }

    #[tokio::test]
    async fn list_is_empty_when_nothing_favourited() {
        let db = setup().await;
        let result = list(db.pool()).await.unwrap();
        assert!(result.target_ids.is_empty());
    }

    #[tokio::test]
    async fn add_then_list_includes_target() {
        let db = setup().await;
        let id = Uuid::new_v4();
        insert_target(db.pool(), id).await;
        let cache = empty_cache();

        let added = add(db.pool(), &cache, &TargetFavouriteRequest { target_id: id.to_string() })
            .await
            .unwrap();
        assert_eq!(added.target_id, id.to_string());
        assert!(!added.favourited_at.is_empty());

        let listed = list(db.pool()).await.unwrap();
        assert_eq!(listed.target_ids, vec![id.to_string()]);
    }

    #[tokio::test]
    async fn add_unknown_target_returns_not_found() {
        let db = setup().await;
        let cache = empty_cache();
        let err = add(
            db.pool(),
            &cache,
            &TargetFavouriteRequest { target_id: domain_core::ids::new_id() },
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::TargetNotFound);
    }

    #[tokio::test]
    async fn add_invalid_uuid_returns_not_found() {
        let db = setup().await;
        let cache = empty_cache();
        let err =
            add(db.pool(), &cache, &TargetFavouriteRequest { target_id: "missing".to_owned() })
                .await
                .unwrap_err();
        assert_eq!(err.code, ErrorCode::TargetNotFound);
    }

    #[tokio::test]
    async fn add_promotes_from_redb_cache_when_not_yet_durable() {
        let db = setup().await;
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let ns = simbad_resolver::identity::namespace("astro-plan.targets");
        let identity = simbad_resolver::ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: None,
            object_type: simbad_resolver::ObjectType::Galaxy,
            otype_raw: "G".to_owned(),
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: None,
            aliases: vec![simbad_resolver::ResolvedAlias::new(
                "M 31",
                simbad_resolver::AliasKind::Designation,
            )],
            source: simbad_resolver::TargetSource::Resolved,
        };
        cache.upsert(&identity, &ns).await.unwrap();
        let id = targeting::identity::target_id_from_designation("M 31");

        let added = add(db.pool(), &cache, &TargetFavouriteRequest { target_id: id.to_string() })
            .await
            .unwrap();
        assert_eq!(added.target_id, id.to_string());

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM canonical_target")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1, "favouriting must promote the redb-cached target");
    }

    #[tokio::test]
    async fn add_twice_is_idempotent_and_preserves_original_timestamp() {
        let db = setup().await;
        let id = Uuid::new_v4();
        insert_target(db.pool(), id).await;
        let cache = empty_cache();

        let first = add(db.pool(), &cache, &TargetFavouriteRequest { target_id: id.to_string() })
            .await
            .unwrap();
        let second = add(db.pool(), &cache, &TargetFavouriteRequest { target_id: id.to_string() })
            .await
            .unwrap();

        assert_eq!(first.favourited_at, second.favourited_at);

        let listed = list(db.pool()).await.unwrap();
        assert_eq!(listed.target_ids.len(), 1);
    }

    #[tokio::test]
    async fn remove_deletes_and_list_no_longer_includes_it() {
        let db = setup().await;
        let id = Uuid::new_v4();
        insert_target(db.pool(), id).await;
        let cache = empty_cache();
        add(db.pool(), &cache, &TargetFavouriteRequest { target_id: id.to_string() })
            .await
            .unwrap();

        let removed =
            remove(db.pool(), &TargetFavouriteRequest { target_id: id.to_string() }).await.unwrap();
        assert_eq!(removed.target_id, id.to_string());

        let listed = list(db.pool()).await.unwrap();
        assert!(listed.target_ids.is_empty());
    }

    #[tokio::test]
    async fn remove_of_never_favourited_id_is_a_noop() {
        let db = setup().await;
        let id = Uuid::new_v4();
        insert_target(db.pool(), id).await;
        let removed =
            remove(db.pool(), &TargetFavouriteRequest { target_id: id.to_string() }).await.unwrap();
        assert_eq!(removed.target_id, id.to_string());
    }
}
