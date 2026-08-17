// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-root reconcile/detection configuration (spec 048 T005).
//!
//! Per `research.md` R1, this reuses the existing generic `(scope, key,
//! value)` KV table (`protection_defaults`, migration 0035 — the same
//! precedent spec 016/033 established for global protection defaults)
//! rather than adding new columns to `library_root` or minting a dedicated
//! table. Each root gets its own scope (`"inventory_root:<root_id>"`); an
//! absent key resolves to its documented default (data-model.md):
//!
//! | Key                          | Default        |
//! |-------------------------------|----------------|
//! | `reconcile.mode`              | `flag_missing` |
//! | `detection.live`              | `true`         |
//! | `detection.scheduled`         | `false`        |
//! | `detection.on_open`           | `false`        |
//! | `detection.follow_symlinks`   | `false`        |
//!
//! No migration is required for this feature (research.md "Migration note").

use contracts_core::inventory_frame::{
    DetectionConfig, DetectionConfigUpdate, ReconcileMode, RootConfigSetRequest,
    RootInventoryConfig,
};
use contracts_core::ContractError;
use persistence_plans::repositories::source_protection::{
    get_protection_default, set_protection_default,
};
use sqlx::SqlitePool;

use app_core_errors::db_err;

fn scope_for(root_id: &str) -> String {
    format!("inventory_root:{root_id}")
}

const KEY_RECONCILE_MODE: &str = "reconcile.mode";
const KEY_DETECTION_LIVE: &str = "detection.live";
const KEY_DETECTION_SCHEDULED: &str = "detection.scheduled";
const KEY_DETECTION_ON_OPEN: &str = "detection.on_open";
const KEY_DETECTION_FOLLOW_SYMLINKS: &str = "detection.follow_symlinks";

fn parse_reconcile_mode(value: &serde_json::Value) -> ReconcileMode {
    match value.as_str() {
        Some("auto_reconcile") => ReconcileMode::AutoReconcile,
        _ => ReconcileMode::FlagMissing,
    }
}

fn encode_reconcile_mode(mode: ReconcileMode) -> serde_json::Value {
    match mode {
        ReconcileMode::FlagMissing => serde_json::json!("flag_missing"),
        ReconcileMode::AutoReconcile => serde_json::json!("auto_reconcile"),
    }
}

fn bool_or(value: Option<serde_json::Value>, default: bool) -> bool {
    value.and_then(|v| v.as_bool()).unwrap_or(default)
}

/// Read a root's effective reconcile/detection configuration, filling in the
/// documented default for any key that has never been written (spec 048 T034
/// / `inventory.root_config.get`).
///
/// # Errors
///
/// Returns `ContractError` (`internal.database`) on a query failure.
pub async fn get_root_config(
    pool: &SqlitePool,
    root_id: &str,
) -> Result<RootInventoryConfig, ContractError> {
    let scope = scope_for(root_id);

    let mode = get_protection_default(pool, &scope, KEY_RECONCILE_MODE)
        .await
        .map_err(db_err)?
        .map_or(ReconcileMode::FlagMissing, |v| parse_reconcile_mode(&v));

    let defaults = DetectionConfig::default();
    let live = bool_or(
        get_protection_default(pool, &scope, KEY_DETECTION_LIVE).await.map_err(db_err)?,
        defaults.live,
    );
    let scheduled = bool_or(
        get_protection_default(pool, &scope, KEY_DETECTION_SCHEDULED).await.map_err(db_err)?,
        defaults.scheduled,
    );
    let on_open = bool_or(
        get_protection_default(pool, &scope, KEY_DETECTION_ON_OPEN).await.map_err(db_err)?,
        defaults.on_open,
    );
    let follow_symlinks = bool_or(
        get_protection_default(pool, &scope, KEY_DETECTION_FOLLOW_SYMLINKS)
            .await
            .map_err(db_err)?,
        defaults.follow_symlinks,
    );

    Ok(RootInventoryConfig {
        reconcile_mode: mode,
        detection: DetectionConfig { live, scheduled, on_open, follow_symlinks },
    })
}

/// Write a (possibly partial) update to a root's reconcile/detection
/// configuration and return the resulting effective configuration (spec 048
/// T034 / `inventory.root_config.set`). Unset fields in `req` leave the
/// stored value unchanged.
///
/// # Errors
///
/// Returns `ContractError` (`internal.database`) on a query failure.
pub async fn set_root_config(
    pool: &SqlitePool,
    req: &RootConfigSetRequest,
) -> Result<RootInventoryConfig, ContractError> {
    let scope = scope_for(&req.root_id);

    if let Some(mode) = req.reconcile_mode {
        set_protection_default(pool, &scope, KEY_RECONCILE_MODE, &encode_reconcile_mode(mode))
            .await
            .map_err(db_err)?;
    }

    if let Some(DetectionConfigUpdate { live, scheduled, on_open, follow_symlinks }) = req.detection
    {
        if let Some(v) = live {
            set_protection_default(pool, &scope, KEY_DETECTION_LIVE, &serde_json::json!(v))
                .await
                .map_err(db_err)?;
        }
        if let Some(v) = scheduled {
            set_protection_default(pool, &scope, KEY_DETECTION_SCHEDULED, &serde_json::json!(v))
                .await
                .map_err(db_err)?;
        }
        if let Some(v) = on_open {
            set_protection_default(pool, &scope, KEY_DETECTION_ON_OPEN, &serde_json::json!(v))
                .await
                .map_err(db_err)?;
        }
        if let Some(v) = follow_symlinks {
            set_protection_default(
                pool,
                &scope,
                KEY_DETECTION_FOLLOW_SYMLINKS,
                &serde_json::json!(v),
            )
            .await
            .map_err(db_err)?;
        }
    }

    get_root_config(pool, &req.root_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts_core::inventory_frame::DetectionConfigUpdate;

    async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    #[tokio::test]
    async fn get_returns_documented_defaults_when_unset() {
        let db = test_db().await;
        let cfg = get_root_config(db.pool(), "root-1").await.unwrap();

        assert_eq!(cfg.reconcile_mode, ReconcileMode::FlagMissing);
        assert!(cfg.detection.live);
        assert!(!cfg.detection.scheduled);
        assert!(!cfg.detection.on_open);
        assert!(!cfg.detection.follow_symlinks);
    }

    #[tokio::test]
    async fn set_reconcile_mode_round_trips() {
        let db = test_db().await;
        let req = RootConfigSetRequest {
            root_id: "root-1".to_owned(),
            reconcile_mode: Some(ReconcileMode::AutoReconcile),
            detection: None,
        };
        let cfg = set_root_config(db.pool(), &req).await.unwrap();
        assert_eq!(cfg.reconcile_mode, ReconcileMode::AutoReconcile);

        // Round-trips via a fresh get.
        let reread = get_root_config(db.pool(), "root-1").await.unwrap();
        assert_eq!(reread.reconcile_mode, ReconcileMode::AutoReconcile);
    }

    #[tokio::test]
    async fn set_is_a_partial_update() {
        let db = test_db().await;

        // First set follow_symlinks only.
        let req1 = RootConfigSetRequest {
            root_id: "root-1".to_owned(),
            reconcile_mode: None,
            detection: Some(DetectionConfigUpdate {
                follow_symlinks: Some(true),
                ..Default::default()
            }),
        };
        set_root_config(db.pool(), &req1).await.unwrap();

        // Then set scheduled only — follow_symlinks must remain true.
        let req2 = RootConfigSetRequest {
            root_id: "root-1".to_owned(),
            reconcile_mode: None,
            detection: Some(DetectionConfigUpdate { scheduled: Some(true), ..Default::default() }),
        };
        let cfg = set_root_config(db.pool(), &req2).await.unwrap();

        assert!(cfg.detection.follow_symlinks, "earlier partial update must be preserved");
        assert!(cfg.detection.scheduled);
        assert!(cfg.detection.live, "untouched key stays at its default");
    }

    #[tokio::test]
    async fn different_roots_do_not_share_config() {
        let db = test_db().await;
        let req = RootConfigSetRequest {
            root_id: "root-a".to_owned(),
            reconcile_mode: Some(ReconcileMode::AutoReconcile),
            detection: None,
        };
        set_root_config(db.pool(), &req).await.unwrap();

        let other = get_root_config(db.pool(), "root-b").await.unwrap();
        assert_eq!(
            other.reconcile_mode,
            ReconcileMode::FlagMissing,
            "root-b must not inherit root-a's override"
        );
    }
}
