// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(
    clippy::missing_errors_doc,
    clippy::explicit_auto_deref,
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    dead_code
)]

//! Installer trait and item type for the Update View apply loop (spec 062 FR-100).

use contracts_core::ContractError;
use sqlx::SqlitePool;

/// One plan item to be installed.
#[derive(Debug, Clone)]
pub struct InstallItem {
    pub item_row_id: i64,
    pub item_public_id: String,
    pub session_row_id: i64,
    pub frame_row_id: i64,
    pub destination_root_row_id: i64,
    pub destination_relative_path: String,
    pub approved_fingerprint: String,
    pub ordinal: i64,
}

/// Callbacks from the install loop to the persistence layer.
pub trait InstallerCallbacks: Send + Sync {
    fn on_intent_prepared<'a>(
        &'a self,
        pool: &'a SqlitePool,
        item: &'a InstallItem,
        ownership_token: &'a str,
        command_id: &'a str,
        lease_owner: &'a str,
        lease_generation: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ContractError>> + Send + 'a>>;

    fn on_installed<'a>(
        &'a self,
        pool: &'a SqlitePool,
        item_row_id: i64,
        lease_owner: &'a str,
        lease_generation: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ContractError>> + Send + 'a>>;

    fn on_journaled<'a>(
        &'a self,
        pool: &'a SqlitePool,
        item: &'a InstallItem,
        content_fingerprint: &'a str,
        operation_command_id: &'a str,
        lease_owner: &'a str,
        lease_generation: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i64, ContractError>> + Send + 'a>>;
}

/// Run a single install step: prepare intent → install → journal.
pub async fn run_install(
    pool: &SqlitePool,
    _plan_row_id: i64,
    item: &InstallItem,
    operation_command_id: &str,
    lease_owner: &str,
    lease_generation: i64,
    callbacks: &impl InstallerCallbacks,
) -> Result<(i64, String), ContractError> {
    let ownership_token = uuid::Uuid::new_v4().to_string();
    callbacks
        .on_intent_prepared(
            pool,
            item,
            &ownership_token,
            operation_command_id,
            lease_owner,
            lease_generation,
        )
        .await?;
    let content_fingerprint = item.approved_fingerprint.clone();
    callbacks.on_installed(pool, item.item_row_id, lease_owner, lease_generation).await?;
    let entry_row_id = callbacks
        .on_journaled(
            pool,
            item,
            &content_fingerprint,
            operation_command_id,
            lease_owner,
            lease_generation,
        )
        .await?;
    Ok((entry_row_id, content_fingerprint))
}
