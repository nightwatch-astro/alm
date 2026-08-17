// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-frame inventory Tauri commands (spec 048 T006).
//!
//! `inventory_frame_list`, `inventory_reconcile_run`, and
//! `inventory_frame_relink` are wired through `app_core::frame_inventory`.
//! `inventory_root_config_get`/`_set` are wired through
//! `app_core_settings::root_config`.
//!
//! `inventory_watcher_attach`/`_detach` bind a root's live and scheduled
//! detection triggers to the lifetime of the surface showing its frame
//! inventory (spec 048 T023/T024/T026, `crate::frame_watcher`).
//!
//! Command fn names below are the literal Tauri invoke targets (no specta
//! rename) — e.g. `inventory_frame_list` is invoked as `"inventory_frame_list"`.

use app_core::frame_inventory::{list_frames, relink_frame, run_reconcile};
use app_core::settings::root_config::{get_root_config, set_root_config};
use contracts_core::inventory_frame::{
    InventoryFrameListRequest, InventoryFrameListResponse, InventoryFrameRelinkRequest,
    InventoryFrameRelinkResponse, InventoryReconcileRunRequest, InventoryReconcileRunResponse,
    RootConfigGetRequest, RootConfigSetRequest, RootInventoryConfig, RootWatcherRequest,
};
use contracts_core::ContractError;
use sqlx::SqlitePool;
use tauri::State;

use crate::commands::lifecycle::AppState;
use crate::frame_watcher::FrameWatcherRegistry;

/// `inventory.frame.list` — list per-frame inventory entries for a session
/// or root.
///
/// # Errors
/// Returns `ContractError` on database failure or an invalid scope.
#[tauri::command]
#[specta::specta]
pub async fn inventory_frame_list(
    req: InventoryFrameListRequest,
    pool: State<'_, SqlitePool>,
) -> Result<InventoryFrameListResponse, ContractError> {
    list_frames(&pool, &req).await
}

/// `inventory.reconcile.run` — run a reconciliation pass over a root.
///
/// # Errors
/// Returns `ContractError` (`root.unavailable`) when the root is not
/// registered, or a database error otherwise. Never mutates a file.
#[tauri::command]
#[specta::specta]
pub async fn inventory_reconcile_run(
    req: InventoryReconcileRunRequest,
    pool: State<'_, SqlitePool>,
    app_state: State<'_, AppState>,
) -> Result<InventoryReconcileRunResponse, ContractError> {
    run_reconcile(&pool, &app_state.bus, &req).await
}

/// `inventory.frame.relink` — relink a surfaced missing frame to a candidate
/// file under the same root, confirmed by sha256 content hash.
///
/// # Errors
/// Returns `ContractError` (`frame.not_found`, `root.unavailable`,
/// `file.not_found`, `hash.mismatch`) per `app_core::frame_inventory::relink_frame`.
#[tauri::command]
#[specta::specta]
pub async fn inventory_frame_relink(
    req: InventoryFrameRelinkRequest,
    pool: State<'_, SqlitePool>,
    app_state: State<'_, AppState>,
) -> Result<InventoryFrameRelinkResponse, ContractError> {
    relink_frame(&pool, &app_state.bus, &req).await
}

/// `inventory.root_config.get` — read a root's reconcile/detection
/// configuration, with documented defaults filled in for unset keys.
///
/// # Errors
/// Returns `ContractError` on database failure.
#[tauri::command]
#[specta::specta]
pub async fn inventory_root_config_get(
    req: RootConfigGetRequest,
    pool: State<'_, SqlitePool>,
) -> Result<RootInventoryConfig, ContractError> {
    get_root_config(&pool, &req.root_id).await
}

/// `inventory.root_config.set` — write a (possibly partial) update to a
/// root's reconcile/detection configuration.
///
/// A currently-attached root is re-attached so the new detection triggers take
/// effect immediately; without that, toggling `live` off would leave the OS
/// watcher running until the surface closed.
///
/// # Errors
/// Returns `ContractError` on database failure.
#[tauri::command]
#[specta::specta]
pub async fn inventory_root_config_set(
    req: RootConfigSetRequest,
    pool: State<'_, SqlitePool>,
    app_state: State<'_, AppState>,
    registry: State<'_, FrameWatcherRegistry>,
) -> Result<RootInventoryConfig, ContractError> {
    let config = set_root_config(&pool, &req).await?;
    crate::frame_watcher::reattach_if_attached(&pool, &app_state.bus, &registry, &req.root_id)
        .await;
    Ok(config)
}

/// `inventory.watcher.attach` — start the root's configured live and scheduled
/// detection triggers, and run its `on_open` reconcile if enabled.
///
/// Idempotent. An unavailable or unregistered root is not an error: nothing is
/// attached and a later attach retries.
///
/// # Errors
/// Returns `ContractError` when the root's config cannot be read or its OS
/// watcher cannot be started.
#[tauri::command]
#[specta::specta]
pub async fn inventory_watcher_attach(
    req: RootWatcherRequest,
    pool: State<'_, SqlitePool>,
    app_state: State<'_, AppState>,
    registry: State<'_, FrameWatcherRegistry>,
) -> Result<(), ContractError> {
    crate::frame_watcher::attach_root_watcher(&pool, &app_state.bus, &registry, &req.root_id)
        .await
        .map_err(ContractError::internal)
}

/// `inventory.watcher.detach` — stop the root's live watch and scheduled
/// trigger so no watch is held on an idle root (research R2).
///
/// Idempotent: detaching an unattached root is a silent no-op.
///
/// # Errors
/// Never fails; the `Result` matches the shared command shape.
#[tauri::command]
#[specta::specta]
pub async fn inventory_watcher_detach(
    req: RootWatcherRequest,
    registry: State<'_, FrameWatcherRegistry>,
) -> Result<(), ContractError> {
    crate::frame_watcher::detach_root_watcher(&registry, &req.root_id).await;
    Ok(())
}
